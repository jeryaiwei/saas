//! Auth service — login / logout / get-info / tenant-list / refresh-token business logic.

use super::dto::{
    CaptchaCodeResponseDto, CurrentUserInfoResponseDto, LoginDto, LoginTokenResponseDto,
    RefreshTokenDto, TenantListForLoginDto, TenantVo,
};
use crate::domain::UserRepo;
use crate::state::AppState;
use framework::auth::{jwt, session, JwtClaims, UserSession};
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::infra::{captcha, crypto};
use framework::response::ResponseCode;

#[tracing::instrument(skip_all, fields(username = %dto.username))]
pub async fn login(state: &AppState, dto: LoginDto) -> Result<LoginTokenResponseDto, AppError> {
    // 1. Captcha — if the client sent a captcha id, verify it.
    if let (Some(cid), Some(code)) = (dto.captcha_id.as_deref(), dto.captcha_code.as_deref()) {
        if !cid.is_empty() {
            let ok = captcha::verify_and_consume(&state.redis, &state.config.redis_keys, cid, code)
                .await
                .into_internal()?;
            if !ok {
                return Err(AppError::business(ResponseCode::CAPTCHA_INVALID));
            }
        }
    }

    // 2. Look up user
    let user = UserRepo::find_by_username(&state.pg, &dto.username)
        .await
        .into_internal()?
        .or_business(ResponseCode::INVALID_CREDENTIALS)?;

    if !user.is_active() {
        return Err(AppError::business(ResponseCode::ACCOUNT_LOCKED));
    }

    // 3. Verify password (bcrypt, must accept NestJS-written hashes)
    if !crypto::verify_password(&dto.password, &user.password) {
        return Err(AppError::business(ResponseCode::INVALID_CREDENTIALS));
    }

    // 4. Resolve tenant binding — default > first active
    let user_tenants = UserRepo::find_user_tenants(&state.pg, &user.user_id)
        .await
        .into_internal()?;

    let (chosen_tenant_id, is_admin) = match user_tenants.iter().find(|t| t.is_default_flag()) {
        Some(t) => (Some(t.tenant_id.clone()), t.is_admin_flag()),
        None => match user_tenants.first() {
            Some(t) => (Some(t.tenant_id.clone()), t.is_admin_flag()),
            None => (None, false),
        },
    };

    // 5. Permissions.
    //    - Admins on any tenant → every menu permission (NestJS behavior;
    //      Phase 2 will apply the tenant-package menu filter).
    //    - Non-admins → user → role → role-menu → menu join.
    //    - No tenant binding → empty list.
    let permissions = match chosen_tenant_id.as_deref() {
        Some(tid) if is_admin => UserRepo::resolve_all_menu_perms(&state.pg, tid)
            .await
            .into_internal()
            .inspect(|p| {
                tracing::debug!(
                    tenant_id = %tid,
                    perm_count = p.len(),
                    "admin user granted package-scoped menu permissions"
                );
            })?,
        Some(tid) => UserRepo::resolve_role_permissions(&state.pg, &user.user_id, tid)
            .await
            .into_internal()?,
        None => Vec::new(),
    };

    // 6. Build session and persist under a fresh uuid.
    //
    // `user_id`, `user_type`, and `chosen_tenant_id` are still cloned here
    // because they're moved into `JwtClaims` below. The other fields
    // (`user_name`, `platform_id`, `lang`) are moved directly — `user` is
    // never read again after this struct literal, so partial-move is safe.
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let sess = UserSession {
        user_id: user.user_id.clone(),
        user_name: user.user_name,
        user_type: user.user_type.clone(),
        tenant_id: chosen_tenant_id.clone(),
        platform_id: Some(user.platform_id),
        sys_code: None,
        lang: user.lang,
        is_admin,
        permissions,
        roles: Vec::new(),
    };
    session::store(
        &state.redis,
        &state.config.redis_keys,
        &session_uuid,
        &sess,
        state.config.jwt.expires_in_sec as u64,
    )
    .await
    .into_internal()?;

    // 7. Sign the (thin) JWT
    let claims = JwtClaims::new(
        session_uuid,
        user.user_id,
        chosen_tenant_id,
        user.user_type,
        None,
        state.config.jwt.expires_in_sec,
    );
    let token = jwt::encode_token(&claims, &state.config.jwt)?;

    tracing::info!(
        username = %sess.user_name,
        user_id = %sess.user_id,
        "login success"
    );

    Ok(LoginTokenResponseDto {
        access_token: token.clone(),
        refresh_token: Some(token),
        expire_in: state.config.jwt.expires_in_sec,
        refresh_expire_in: Some(state.config.jwt.refresh_expires_in_sec),
        client_id: None,
        scope: None,
        openid: None,
    })
}

#[tracing::instrument(skip_all)]
pub async fn get_captcha(state: &AppState) -> Result<CaptchaCodeResponseDto, AppError> {
    let code = captcha::generate_and_store(
        &state.redis,
        &state.config.redis_keys,
        &state.config.redis_ttl,
    )
    .await
    .into_internal()?;
    Ok(CaptchaCodeResponseDto {
        uuid: code.uuid,
        img: code.image,
    })
}

#[tracing::instrument(skip_all, fields(uuid = %claims.uuid))]
pub async fn logout(state: &AppState, claims: &JwtClaims) -> Result<(), AppError> {
    // Delete the session so any concurrent request returns TOKEN_EXPIRED.
    session::delete(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?;
    // Add to single-token blacklist (belt + braces for in-flight tokens).
    session::blacklist(
        &state.redis,
        &state.config.redis_keys,
        &claims.uuid,
        state.config.redis_ttl.token_blacklist,
    )
    .await
    .into_internal()?;
    Ok(())
}

#[tracing::instrument(skip_all, fields(user_id = %session.user_id))]
pub async fn get_info(
    state: &AppState,
    session: &UserSession,
) -> Result<CurrentUserInfoResponseDto, AppError> {
    // Refresh the user row from DB so profile fields stay current.
    let user = UserRepo::find_by_id(&state.pg, &session.user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::USER_NOT_FOUND)?;

    Ok(CurrentUserInfoResponseDto {
        user_id: user.user_id,
        user_name: user.user_name,
        nick_name: user.nick_name,
        avatar: user.avatar,
        email: user.email,
        phonenumber: user.phonenumber,
        user_type: user.user_type,
        tenant_id: session.tenant_id.clone(),
        platform_id: session.platform_id.clone(),
        is_admin: session.is_admin,
        roles: session.roles.clone(),
        permissions: session.permissions.clone(),
    })
}

/// Return the list of active tenants for the login page dropdown.
/// Public — no auth needed.
#[tracing::instrument(skip_all)]
pub async fn tenant_list(state: &AppState) -> Result<TenantListForLoginDto, AppError> {
    if !state.config.tenant.enabled {
        return Ok(TenantListForLoginDto {
            tenant_enabled: false,
            vo_list: Vec::new(),
        });
    }

    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT tenant_id, company_name, domain FROM sys_tenant \
         WHERE status = '0' AND del_flag = '0' \
         ORDER BY create_at ASC LIMIT 500",
    )
    .fetch_all(&state.pg)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("tenant_list: {e}")))?;

    let vo_list = rows
        .into_iter()
        .map(|(tenant_id, company_name, domain)| TenantVo {
            tenant_id,
            company_name,
            domain,
        })
        .collect();

    Ok(TenantListForLoginDto {
        tenant_enabled: true,
        vo_list,
    })
}

/// Refresh an access token. Validates the old token, invalidates it, and
/// issues a new one with recalculated permissions.
#[tracing::instrument(skip_all)]
pub async fn refresh_token(
    state: &AppState,
    dto: RefreshTokenDto,
) -> Result<LoginTokenResponseDto, AppError> {
    // 1. Decode the refresh token JWT
    let claims = jwt::decode_token(&dto.refresh_token, &state.config.jwt)?;

    // 2. Check single-token blacklist
    let blacklisted = session::is_blacklisted(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?;
    if blacklisted {
        return Err(AppError::auth(ResponseCode::TOKEN_INVALID));
    }

    // 3. Check user-level token version
    let current_version =
        session::get_user_token_version(&state.redis, &state.config.redis_keys, &claims.user_id)
            .await
            .into_internal()?;
    if let (Some(current), Some(token_ver)) = (current_version, claims.token_version) {
        if token_ver < current {
            return Err(AppError::auth(ResponseCode::TOKEN_INVALID));
        }
    }

    // 4. Fetch old session from Redis
    let old_session = session::fetch(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?
        .ok_or_else(|| AppError::auth(ResponseCode::TOKEN_EXPIRED))?;

    // 5. Delete old session + blacklist old token
    session::delete(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?;
    session::blacklist(
        &state.redis,
        &state.config.redis_keys,
        &claims.uuid,
        state.config.redis_ttl.token_blacklist,
    )
    .await
    .into_internal()?;

    // 6. Recalculate permissions
    let (permissions, is_admin) = match old_session.tenant_id.as_deref() {
        Some(tid) if old_session.is_admin => {
            let perms = UserRepo::resolve_all_menu_perms(&state.pg, tid)
                .await
                .into_internal()?;
            (perms, true)
        }
        Some(tid) => {
            let perms = UserRepo::resolve_role_permissions(&state.pg, &old_session.user_id, tid)
                .await
                .into_internal()?;
            (perms, false)
        }
        None => (Vec::new(), false),
    };

    // 7. Build new session + new JWT
    let new_uuid = uuid::Uuid::new_v4().to_string();
    let new_session = UserSession {
        user_id: old_session.user_id.clone(),
        user_name: old_session.user_name,
        user_type: old_session.user_type.clone(),
        tenant_id: old_session.tenant_id.clone(),
        platform_id: old_session.platform_id,
        sys_code: old_session.sys_code,
        lang: old_session.lang,
        is_admin,
        permissions,
        roles: old_session.roles,
    };

    // 8. Store new session in Redis
    session::store(
        &state.redis,
        &state.config.redis_keys,
        &new_uuid,
        &new_session,
        state.config.jwt.expires_in_sec as u64,
    )
    .await
    .into_internal()?;

    // 9. Sign the new JWT
    let new_claims = JwtClaims::new(
        new_uuid,
        old_session.user_id,
        old_session.tenant_id,
        old_session.user_type,
        current_version,
        state.config.jwt.expires_in_sec,
    );
    let token = jwt::encode_token(&new_claims, &state.config.jwt)?;

    Ok(LoginTokenResponseDto {
        access_token: token.clone(),
        refresh_token: Some(token),
        expire_in: state.config.jwt.expires_in_sec,
        refresh_expire_in: Some(state.config.jwt.refresh_expires_in_sec),
        client_id: None,
        scope: None,
        openid: None,
    })
}

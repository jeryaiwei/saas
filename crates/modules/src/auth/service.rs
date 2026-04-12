//! Auth service — login / logout / get-info business logic.

use super::dto::{
    CaptchaCodeResponseDto, CurrentUserInfoResponseDto, LoginDto, LoginTokenResponseDto,
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
        Some(tid) if is_admin => UserRepo::resolve_all_menu_perms(&state.pg)
            .await
            .into_internal()
            .inspect(|p| {
                tracing::debug!(
                    tenant = %tid,
                    count = p.len(),
                    "admin user granted all menu permissions"
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

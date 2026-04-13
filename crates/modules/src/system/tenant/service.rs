//! Tenant service — business orchestration.

use super::dto::{
    CreateTenantDto, ListTenantDto, TenantDetailResponseDto, TenantListItemResponseDto,
    TenantSelectOptionDto, TenantSwitchStatusDto, UpdateTenantDto,
};
use crate::domain::constants::SUPER_TENANT_ID;
use crate::domain::{
    AdminUserInfo, TenantInsertParams, TenantListFilter, TenantPackageRepo, TenantRepo,
    TenantUpdateParams, UserInsertParams, UserRepo,
};
use crate::state::AppState;
use anyhow::Context;
use framework::auth::{jwt, session, JwtClaims, UserSession};
use framework::constants::USER_TYPE_CLIENT;
use framework::context::RequestContext;
use framework::error::{AppError, BusinessCheckBool, BusinessCheckOption, IntoAppError};
use framework::infra::crypto::hash_password;
use framework::response::{ApiResponse, Page, ResponseCode};

/// Returns `true` when `tenant_id` is the system super-tenant — operations
/// on this tenant are restricted.
fn is_protected_tenant(tenant_id: &str) -> bool {
    tenant_id == SUPER_TENANT_ID
}

/// Create one or more tenants + one shared admin user + bindings in a single
/// transaction.
///
/// When `package_ids` contains N elements, N tenant rows are created:
/// - tenant_id is `format!("{:06}", base + i)` for i in 0..N
/// - company_name is `dto.company_name` when N==1, else `"<name>-<pkg_name>"`
/// - a single admin user is created and bound to ALL N tenants
/// - the first tenant binding is marked `is_default='1'`, others `'0'`
#[tracing::instrument(skip_all, fields(
    company_name = %dto.company_name,
    username = %dto.username,
    package_count = dto.package_ids.len(),
))]
pub async fn create(state: &AppState, dto: CreateTenantDto) -> Result<ApiResponse<()>, AppError> {
    // 1. Validate all package_ids exist + active
    let active_packages = TenantPackageRepo::find_active_by_ids(&state.pg, &dto.package_ids)
        .await
        .into_internal()?;
    (active_packages.len() != dto.package_ids.len())
        .business_err_if(ResponseCode::TENANT_PACKAGE_NOT_FOUND)?;

    // 2. Validate parent_id + layer constraints
    if let Some(ref parent_id) = dto.parent_id {
        let parent = TenantRepo::find_by_tenant_id(&state.pg, parent_id)
            .await
            .into_internal()?
            .or_business(ResponseCode::TENANT_PARENT_NOT_FOUND)?;

        // Layer constraint: only super tenant and platform tenants can have children.
        // Super tenant (tenant_id = SUPER_TENANT_ID) → creates platform.
        // Platform (parent_id = SUPER_TENANT_ID) → creates regular tenant.
        // Regular tenant (anything else) → cannot create children.
        let is_parent_super = parent.tenant_id == SUPER_TENANT_ID;
        let is_parent_platform = parent.parent_id.as_deref() == Some(SUPER_TENANT_ID);
        if !is_parent_super && !is_parent_platform {
            return Err(AppError::business(ResponseCode::TENANT_HAS_CHILDREN));
        }
    }

    // 2b. Non-super tenants must have a package
    // (super tenant = parent_id is None or SUPER_TENANT_ID itself)
    if dto.parent_id.is_some() && dto.package_ids.is_empty() {
        return Err(AppError::business(ResponseCode::TENANT_PACKAGE_NOT_FOUND));
    }

    // 3. Validate company_name prefix unique
    let name_exists = TenantRepo::exists_by_company_name_prefix(&state.pg, &dto.company_name, None)
        .await
        .into_internal()?;
    name_exists.business_err_if(ResponseCode::TENANT_COMPANY_EXISTS)?;

    // 4. Validate username unique
    let username_unique = UserRepo::verify_user_name_unique(&state.pg, &dto.username)
        .await
        .into_internal()?;
    (!username_unique).business_err_if(ResponseCode::DUPLICATE_KEY)?;

    // 5. Hash password
    let password_hash = hash_password(&dto.password)
        .context("hash_password: create tenant")
        .into_internal()?;

    // 6. Pre-allocate one sequence value per package to avoid concurrent collision.
    //    Each call to nextval is atomic; acquiring all IDs before the transaction
    //    ensures no two parallel calls share the same tenant_id.
    let mut allocated_ids: Vec<i64> = Vec::with_capacity(dto.package_ids.len());
    for _ in 0..dto.package_ids.len() {
        let id = TenantRepo::generate_next_tenant_id(&state.pg)
            .await
            .into_internal()?;
        allocated_ids.push(id);
    }

    // 7. If multiple packages, collect package names for company name suffixes
    let pkg_names: Vec<String> = if dto.package_ids.len() > 1 {
        // Preserve input order: map package_id -> package_name
        let name_map: std::collections::HashMap<String, String> = active_packages
            .iter()
            .map(|p| (p.package_id.clone(), p.package_name.clone()))
            .collect();
        dto.package_ids
            .iter()
            .map(|id| name_map.get(id).cloned().unwrap_or_default())
            .collect()
    } else {
        vec![]
    };

    // 8. Begin transaction
    let mut tx = state
        .pg
        .begin()
        .await
        .context("create tenant: begin tx")
        .into_internal()?;

    // 9. Insert N tenant rows
    let mut created_tenant_ids: Vec<String> = Vec::with_capacity(dto.package_ids.len());
    for (i, package_id) in dto.package_ids.iter().enumerate() {
        let tenant_id = format!("{:06}", allocated_ids[i]);
        let company_name = if dto.package_ids.len() == 1 {
            dto.company_name.clone()
        } else {
            format!("{}-{}", dto.company_name, pkg_names[i])
        };

        TenantRepo::insert(
            &mut *tx,
            TenantInsertParams {
                tenant_id: tenant_id.clone(),
                parent_id: dto.parent_id.clone(),
                contact_user_name: dto.contact_user_name.clone(),
                contact_phone: dto.contact_phone.clone(),
                company_name,
                license_number: dto.license_number.clone(),
                address: dto.address.clone(),
                intro: dto.intro.clone(),
                domain: dto.domain.clone(),
                package_id: Some(package_id.clone()),
                expire_time: dto.expire_time.clone(),
                account_count: dto.account_count,
                status: dto.status.clone(),
                language: dto.language.clone(),
                remark: dto.remark.clone(),
            },
        )
        .await
        .into_internal()?;

        created_tenant_ids.push(tenant_id);
    }

    // 10. Create admin user — platform_id 取决于父租户类型
    //     创建平台 (parent=超级) → platform_id = SUPER_TENANT_ID
    //     创建一般 (parent=平台) → platform_id = 平台的 tenant_id
    let admin_platform_id = dto
        .parent_id
        .clone()
        .unwrap_or_else(|| SUPER_TENANT_ID.to_string());

    let user = UserRepo::insert(
        &mut *tx,
        UserInsertParams {
            user_name: dto.username,
            nick_name: "租户管理员".into(),
            password_hash,
            platform_id: admin_platform_id,
            dept_id: None,
            email: String::new(),
            phonenumber: String::new(),
            sex: "2".into(),
            avatar: String::new(),
            status: "0".into(),
            remark: None,
        },
    )
    .await
    .into_internal()?;

    // 11. Bind admin user to each tenant
    for (i, created_tenant_id) in created_tenant_ids.iter().enumerate() {
        let is_default = if i == 0 { "1" } else { "0" };
        TenantRepo::insert_user_tenant_binding(
            &mut *tx,
            &user.user_id,
            created_tenant_id,
            is_default,
            "1", // is_admin
        )
        .await
        .into_internal()?;
    }

    // 12. Commit
    tx.commit()
        .await
        .context("create tenant: commit tx")
        .into_internal()?;

    Ok(ApiResponse::success())
}

/// Fetch a single tenant by surrogate `id` (UUID PK).
#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: &str) -> Result<TenantDetailResponseDto, AppError> {
    let tenant = TenantRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    let admin_info: Option<AdminUserInfo> =
        TenantRepo::find_admin_user_info(&state.pg, &tenant.tenant_id)
            .await
            .into_internal()?;

    let admin_name_map =
        TenantRepo::find_admin_user_names(&state.pg, std::slice::from_ref(&tenant.tenant_id))
            .await
            .into_internal()?;
    let admin_user_name = admin_name_map.get(&tenant.tenant_id).cloned();

    Ok(TenantDetailResponseDto::from_entity_with_admin(
        tenant,
        admin_info,
        admin_user_name,
    ))
}

/// Paginated tenant list.
#[tracing::instrument(skip_all, fields(
    has_company_name = query.company_name.is_some(),
    page_num = query.page.page_num,
    page_size = query.page.page_size,
))]
pub async fn list(
    state: &AppState,
    query: ListTenantDto,
) -> Result<Page<TenantListItemResponseDto>, AppError> {
    let page = TenantRepo::find_page(
        &state.pg,
        TenantListFilter {
            tenant_id: query.tenant_id,
            contact_user_name: query.contact_user_name,
            contact_phone: query.contact_phone,
            company_name: query.company_name,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    // Batch-fetch admin user names for all returned tenants
    let tenant_ids: Vec<String> = page.rows.iter().map(|t| t.tenant_id.clone()).collect();
    let admin_name_map = TenantRepo::find_admin_user_names(&state.pg, &tenant_ids)
        .await
        .into_internal()?;

    let mapped = page.map_rows(|tenant| {
        let admin_user_name = admin_name_map.get(&tenant.tenant_id).cloned();
        TenantListItemResponseDto::from_entity(tenant, admin_user_name)
    });

    Ok(mapped)
}

/// Update tenant scalar fields.
#[tracing::instrument(skip_all, fields(id = %dto.id, tenant_id = %dto.tenant_id))]
pub async fn update(state: &AppState, dto: UpdateTenantDto) -> Result<(), AppError> {
    // 1. Fetch existing record
    TenantRepo::find_by_id(&state.pg, &dto.id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    // 2. Protected tenant check — cannot mutate critical fields on the super tenant
    if is_protected_tenant(&dto.tenant_id)
        && (dto.status.is_some()
            || dto.company_name.is_some()
            || dto.package_id.is_some()
            || dto.expire_time.is_some()
            || dto.account_count.is_some())
    {
        return Err(AppError::business(ResponseCode::TENANT_PROTECTED));
    }

    // 3. Uniqueness check if company_name is changing
    if let Some(ref name) = dto.company_name {
        let exists =
            TenantRepo::exists_by_company_name_prefix(&state.pg, name, Some(&dto.tenant_id))
                .await
                .into_internal()?;
        exists.business_err_if(ResponseCode::TENANT_COMPANY_EXISTS)?;
    }

    // 4. Apply update
    let affected = TenantRepo::update_by_id(
        &state.pg,
        TenantUpdateParams {
            id: dto.id,
            contact_user_name: dto.contact_user_name,
            contact_phone: dto.contact_phone,
            company_name: dto.company_name,
            license_number: dto.license_number,
            address: dto.address,
            intro: dto.intro,
            domain: dto.domain,
            package_id: dto.package_id,
            expire_time: dto.expire_time,
            account_count: dto.account_count,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    Ok(())
}

/// Soft-delete one or more tenants. Accepts a comma-separated id list.
/// Pre-validates all targets, then applies the batch delete inside a
/// transaction so partial success is impossible.
///
/// Guards:
/// - cannot delete the protected super-tenant
/// - cannot delete a tenant that still has active child tenants
#[tracing::instrument(skip_all, fields(path_ids = %path_ids))]
pub async fn remove(state: &AppState, path_ids: &str) -> Result<(), AppError> {
    let ids: Vec<String> = path_ids
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    if ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }

    // Fetch each tenant and check the protected guard
    let mut tenant_ids: Vec<String> = Vec::with_capacity(ids.len());
    for id in &ids {
        let tenant = TenantRepo::find_by_id(&state.pg, id)
            .await
            .into_internal()?
            .or_business(ResponseCode::DATA_NOT_FOUND)?;

        if is_protected_tenant(&tenant.tenant_id) {
            return Err(AppError::business(ResponseCode::TENANT_PROTECTED));
        }
        tenant_ids.push(tenant.tenant_id);
    }

    // Check none of the targets have child tenants
    let parents_with_children = TenantRepo::find_tenant_ids_with_children(&state.pg, &tenant_ids)
        .await
        .into_internal()?;
    if !parents_with_children.is_empty() {
        return Err(AppError::business(ResponseCode::TENANT_HAS_CHILDREN));
    }

    // Batch soft-delete inside a transaction
    let mut tx = state
        .pg
        .begin()
        .await
        .context("remove: begin tx")
        .into_internal()?;

    TenantRepo::soft_delete_by_ids(&mut *tx, &ids)
        .await
        .into_internal()?;

    tx.commit()
        .await
        .context("remove: commit tx")
        .into_internal()?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tenant select-list / switch / clear / switch-status
// ---------------------------------------------------------------------------

/// Return list of tenants the current user can switch to.
/// Switchable tenant list — different query per admin level.
/// Super admin: all active tenants.
/// Platform admin: platform children + own bindings.
/// Others: only bound tenants.
#[tracing::instrument(skip_all)]
pub async fn select_list(state: &AppState) -> Result<Vec<TenantSelectOptionDto>, AppError> {
    let ctx = RequestContext::current_cloned();
    let user_id = ctx
        .user_id
        .as_deref()
        .ok_or_else(|| AppError::auth(ResponseCode::TOKEN_EXPIRED))?;
    let current_tid = ctx.tenant_id.as_deref().unwrap_or("");

    // Determine admin level from DB bindings (not session — session may reflect a switched state)
    let user_tenants = UserRepo::find_user_tenants(&state.pg, user_id)
        .await
        .into_internal()?;
    let is_super = user_tenants
        .iter()
        .any(|t| t.tenant_id == SUPER_TENANT_ID && t.is_admin_flag());
    let is_platform = !is_super
        && ctx.platform_id.is_some()
        && user_tenants
            .iter()
            .any(|t| Some(t.tenant_id.as_str()) == ctx.platform_id.as_deref() && t.is_admin_flag());

    let rows: Vec<(String, String)> = if is_super {
        // Super admin: all active tenants
        sqlx::query_as(
            "SELECT tenant_id, company_name FROM sys_tenant \
             WHERE status = '0' AND del_flag = '0' AND tenant_id <> $1 \
             ORDER BY create_at ASC LIMIT 500",
        )
        .bind(current_tid)
        .fetch_all(&state.pg)
        .await
    } else if is_platform {
        // Platform admin: children of this platform + own bindings
        sqlx::query_as(
            "SELECT DISTINCT t.tenant_id, t.company_name FROM sys_tenant t \
             WHERE t.status = '0' AND t.del_flag = '0' AND t.tenant_id <> $1 \
               AND (t.parent_id = $2 \
                    OR t.tenant_id IN (SELECT tenant_id FROM sys_user_tenant WHERE user_id = $3 AND status = '0')) \
             ORDER BY t.create_at ASC",
        )
        .bind(current_tid)
        .bind(ctx.platform_id.as_deref().unwrap_or(""))
        .bind(user_id)
        .fetch_all(&state.pg)
        .await
    } else {
        // Regular user: only bound tenants
        sqlx::query_as(
            "SELECT t.tenant_id, t.company_name \
             FROM sys_user_tenant ut \
             JOIN sys_tenant t ON t.tenant_id = ut.tenant_id \
             WHERE ut.user_id = $1 AND ut.status = '0' \
               AND t.status = '0' AND t.del_flag = '0' \
               AND t.tenant_id <> $2 \
             ORDER BY t.create_at ASC",
        )
        .bind(user_id)
        .bind(current_tid)
        .fetch_all(&state.pg)
        .await
    }
    .map_err(|e| AppError::Internal(anyhow::anyhow!("select_list: {e}")))?;

    Ok(rows
        .into_iter()
        .map(|(tenant_id, company_name)| TenantSelectOptionDto {
            tenant_id,
            company_name,
        })
        .collect())
}

/// Switch to a target tenant with full validation + switchedFrom snapshot.
async fn switch_to_tenant(
    state: &AppState,
    user_id: &str,
    target_tenant_id: &str,
    claims: &JwtClaims,
) -> Result<crate::auth::dto::LoginTokenResponseDto, AppError> {
    // 1. Fetch old session
    let old_session = session::fetch(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?
        .or_business(ResponseCode::TOKEN_EXPIRED)?;

    // 2. Cannot switch to current tenant
    if old_session.tenant_id.as_deref() == Some(target_tenant_id) {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }

    // 3. CLIENT users cannot switch
    if old_session.user_type == USER_TYPE_CLIENT {
        return Err(AppError::forbidden(ResponseCode::FORBIDDEN));
    }

    // 4. Target tenant must be active + not expired
    let target: (
        String,
        Option<String>,
        Option<chrono::DateTime<chrono::Utc>>,
    ) = sqlx::query_as(
        "SELECT status, parent_id, expire_time FROM sys_tenant \
         WHERE tenant_id = $1 AND del_flag = '0'",
    )
    .bind(target_tenant_id)
    .fetch_optional(&state.pg)
    .await
    .context("switch: fetch target tenant")
    .into_internal()?
    .ok_or_else(|| AppError::business(ResponseCode::TENANT_NOT_FOUND))?;

    if target.0 != "0" {
        return Err(AppError::business(ResponseCode::TENANT_NOT_FOUND));
    }
    if let Some(exp) = target.2 {
        if exp < chrono::Utc::now() {
            return Err(AppError::business(ResponseCode::TENANT_EXPIRED));
        }
    }

    // 5. Permission check — determine admin level from DB bindings (not session,
    //    which may reflect a switched state where super admin looks like a regular user).
    let user_tenants = UserRepo::find_user_tenants(&state.pg, user_id)
        .await
        .into_internal()?;
    let is_super = user_tenants
        .iter()
        .any(|t| t.tenant_id == SUPER_TENANT_ID && t.is_admin_flag());
    let is_platform = !is_super
        && old_session.platform_id.is_some()
        && user_tenants.iter().any(|t| {
            Some(t.tenant_id.as_str()) == old_session.platform_id.as_deref() && t.is_admin_flag()
        });

    if !is_super {
        if is_platform {
            // Platform admin: target must be child of platform
            let platform_id = old_session.platform_id.as_deref().unwrap_or("");
            if target.1.as_deref() != Some(platform_id) {
                // Also allow if user has explicit binding
                let has_binding: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM sys_user_tenant \
                     WHERE user_id = $1 AND tenant_id = $2 AND status = '0')",
                )
                .bind(user_id)
                .bind(target_tenant_id)
                .fetch_one(&state.pg)
                .await
                .context("switch: check binding")
                .into_internal()?;
                if !has_binding {
                    return Err(AppError::business(ResponseCode::TENANT_BINDING_NOT_FOUND));
                }
            }
        } else {
            // Regular user: must have binding
            let has_binding: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM sys_user_tenant \
                 WHERE user_id = $1 AND tenant_id = $2 AND status = '0')",
            )
            .bind(user_id)
            .bind(target_tenant_id)
            .fetch_one(&state.pg)
            .await
            .context("switch: check binding")
            .into_internal()?;
            if !has_binding {
                return Err(AppError::business(ResponseCode::TENANT_BINDING_NOT_FOUND));
            }
        }
    }

    // 6. Save switchedFrom snapshot (only if not already switched)
    let existing_original = session::fetch_switch_original(&state.redis, &claims.uuid)
        .await
        .into_internal()?;
    if existing_original.is_none() {
        let original = session::SwitchOriginal {
            tenant_id: old_session.tenant_id.clone().unwrap_or_default(),
            is_admin: old_session.is_admin,
            permissions: old_session.permissions.clone(),
            sys_code: old_session.sys_code.clone(),
            switched_at: chrono::Utc::now().to_rfc3339(),
        };
        session::store_switch_original(
            &state.redis,
            &claims.uuid,
            &original,
            state.config.jwt.expires_in_sec as u64,
        )
        .await
        .into_internal()?;
    }

    // 7. Determine isAdmin in target tenant (reuse user_tenants from step 5)
    let target_is_admin = if is_super {
        true
    } else {
        user_tenants
            .iter()
            .find(|t| t.tenant_id == target_tenant_id)
            .map(|t| t.is_admin_flag())
            .unwrap_or(false)
    };

    // 8. Recalculate permissions
    let permissions = if target_is_admin {
        UserRepo::resolve_all_menu_perms(&state.pg, target_tenant_id)
            .await
            .into_internal()?
    } else {
        UserRepo::resolve_role_permissions(&state.pg, user_id, target_tenant_id)
            .await
            .into_internal()?
    };

    // 9. Resolve sys_code
    let sys_code: Option<String> = sqlx::query_scalar(
        "SELECT p.code FROM sys_tenant t \
         LEFT JOIN sys_tenant_package p ON t.package_id = p.package_id \
           AND p.del_flag = '0' AND p.status = '0' \
         WHERE t.tenant_id = $1 AND t.del_flag = '0'",
    )
    .bind(target_tenant_id)
    .fetch_optional(&state.pg)
    .await
    .context("switch: resolve sys_code")
    .into_internal()?
    .flatten();

    // 10. Update session
    let new_session = UserSession {
        tenant_id: Some(target_tenant_id.to_string()),
        is_admin: target_is_admin,
        permissions,
        sys_code,
        ..old_session.clone()
    };

    session::store(
        &state.redis,
        &state.config.redis_keys,
        &claims.uuid,
        &new_session,
        state.config.jwt.expires_in_sec as u64,
    )
    .await
    .into_internal()?;

    // 11. Return new token
    let new_claims = JwtClaims::new(
        &claims.uuid,
        &old_session.user_id,
        Some(target_tenant_id.to_string()),
        old_session.user_type,
        claims.token_version,
        state.config.jwt.expires_in_sec,
    );
    let token = jwt::encode_token(&new_claims, &state.config.jwt)?;

    Ok(crate::auth::dto::LoginTokenResponseDto {
        access_token: token.clone(),
        refresh_token: Some(token),
        expire_in: state.config.jwt.expires_in_sec,
        refresh_expire_in: Some(state.config.jwt.refresh_expires_in_sec),
        client_id: None,
        scope: None,
        openid: None,
    })
}

/// Switch to a specific tenant.
#[tracing::instrument(skip_all, fields(target_tenant_id = %target_tenant_id))]
pub async fn dynamic_switch(
    state: &AppState,
    target_tenant_id: &str,
    claims: &JwtClaims,
) -> Result<crate::auth::dto::LoginTokenResponseDto, AppError> {
    switch_to_tenant(state, &claims.user_id, target_tenant_id, claims).await
}

/// Restore to original tenant using switchedFrom snapshot.
#[tracing::instrument(skip_all)]
pub async fn dynamic_clear(
    state: &AppState,
    claims: &JwtClaims,
) -> Result<crate::auth::dto::LoginTokenResponseDto, AppError> {
    // Try to restore from snapshot
    let original = session::fetch_switch_original(&state.redis, &claims.uuid)
        .await
        .into_internal()?;

    if let Some(orig) = original {
        // Restore from snapshot — no permission recalculation
        let old_session = session::fetch(&state.redis, &state.config.redis_keys, &claims.uuid)
            .await
            .into_internal()?
            .or_business(ResponseCode::TOKEN_EXPIRED)?;

        let restored = UserSession {
            tenant_id: Some(orig.tenant_id.clone()),
            is_admin: orig.is_admin,
            permissions: orig.permissions,
            sys_code: orig.sys_code,
            ..old_session.clone()
        };

        session::store(
            &state.redis,
            &state.config.redis_keys,
            &claims.uuid,
            &restored,
            state.config.jwt.expires_in_sec as u64,
        )
        .await
        .into_internal()?;

        session::delete_switch_original(&state.redis, &claims.uuid)
            .await
            .into_internal()?;

        let new_claims = JwtClaims::new(
            &claims.uuid,
            &old_session.user_id,
            Some(orig.tenant_id),
            old_session.user_type,
            claims.token_version,
            state.config.jwt.expires_in_sec,
        );
        let token = jwt::encode_token(&new_claims, &state.config.jwt)?;

        Ok(crate::auth::dto::LoginTokenResponseDto {
            access_token: token.clone(),
            refresh_token: Some(token),
            expire_in: state.config.jwt.expires_in_sec,
            refresh_expire_in: Some(state.config.jwt.refresh_expires_in_sec),
            client_id: None,
            scope: None,
            openid: None,
        })
    } else {
        // No snapshot — fallback to default binding
        let user_tenants = UserRepo::find_user_tenants(&state.pg, &claims.user_id)
            .await
            .into_internal()?;
        let default_binding = user_tenants
            .iter()
            .find(|t| t.is_default_flag())
            .or_else(|| user_tenants.first())
            .ok_or_else(|| AppError::business(ResponseCode::TENANT_BINDING_NOT_FOUND))?;
        switch_to_tenant(state, &claims.user_id, &default_binding.tenant_id, claims).await
    }
}

/// Return current tenant switch status.
#[tracing::instrument(skip_all)]
pub async fn switch_status(state: &AppState) -> Result<TenantSwitchStatusDto, AppError> {
    let user_id = RequestContext::with_current(|ctx| ctx.user_id.clone())
        .flatten()
        .or_business(ResponseCode::TOKEN_EXPIRED)?;
    let current_tenant_id = RequestContext::with_current(|ctx| ctx.tenant_id.clone()).flatten();

    let user_tenants = UserRepo::find_user_tenants(&state.pg, &user_id)
        .await
        .into_internal()?;

    let default_tenant_id = user_tenants
        .iter()
        .find(|t| t.is_default_flag())
        .map(|t| t.tenant_id.clone());

    let is_switched = match (&current_tenant_id, &default_tenant_id) {
        (Some(current), Some(default)) => current != default,
        _ => false,
    };

    Ok(TenantSwitchStatusDto {
        current_tenant_id,
        default_tenant_id,
        is_switched,
    })
}

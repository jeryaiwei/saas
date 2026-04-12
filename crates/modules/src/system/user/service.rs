//! User service — business orchestration.

use super::dto::{
    AuthRoleResponseDto, AuthRoleUpdateDto, ChangeUserStatusDto, CreateUserDto, ListUserDto,
    ResetPwdDto, UpdateUserDto, UserDetailResponseDto, UserInfoResponseDto,
    UserListItemResponseDto, UserOptionQueryDto, UserOptionResponseDto, UserProfileResponseDto,
};
use crate::domain::{
    RoleRepo, TenantRepo, UserInsertParams, UserListFilter, UserRepo, UserUpdateParams,
};
use crate::state::AppState;
use anyhow::Context;
use framework::context::RequestContext;
use framework::error::{AppError, BusinessCheckBool, BusinessCheckOption, IntoAppError};
use framework::infra::crypto::hash_password;
use framework::response::{Page, ResponseCode};

/// Returns true if `user_id` corresponds to the system super-admin row.
/// Delegates to `UserRepo::is_super_admin` — kept as a service-level
/// wrapper so guard call sites stay short (`is_super_admin_user(state, id).await?`)
/// and the error type is already `AppError` rather than `anyhow::Error`.
pub(super) async fn is_super_admin_user(state: &AppState, user_id: &str) -> Result<bool, AppError> {
    UserRepo::is_super_admin(&state.pg, user_id)
        .await
        .into_internal()
}

/// Returns true if `target_user_id` matches the caller's user_id from
/// the current RequestContext. Used by self-op guards (can't delete
/// yourself, can't change your own status, can't change your own roles).
///
/// In the integration test harness `as_super_admin` scope, the caller
/// user_id is the literal string `"it-admin"` — tests check self-op by
/// targeting that exact id. In production, middleware sets the real
/// user_id from the JWT.
pub(super) fn is_self_op(target_user_id: &str) -> bool {
    framework::context::RequestContext::with_current(|ctx| ctx.user_id.clone())
        .flatten()
        .is_some_and(|uid| uid == target_user_id)
}

/// Fetch a user by id, tenant-scoped. Returns `DATA_NOT_FOUND` when the
/// user doesn't exist in the current tenant (also covers soft-deleted
/// and cross-tenant attempts — information hiding).
#[tracing::instrument(skip_all, fields(user_id = %user_id))]
pub async fn find_by_id(
    state: &AppState,
    user_id: &str,
) -> Result<UserDetailResponseDto, AppError> {
    let user = UserRepo::find_by_id_tenant_scoped(&state.pg, user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    let role_ids = RoleRepo::find_role_ids_by_user(&state.pg, &user.user_id)
        .await
        .into_internal()?;

    Ok(UserDetailResponseDto::from_entity(user, role_ids))
}

/// Paginated user list. Validation runs in the extractor before reaching
/// this function.
#[tracing::instrument(skip_all, fields(
    has_user_name = query.user_name.is_some(),
    page_num = query.page.page_num,
    page_size = query.page.page_size,
))]
pub async fn list(
    state: &AppState,
    query: ListUserDto,
) -> Result<Page<UserListItemResponseDto>, AppError> {
    let page = UserRepo::find_page(
        &state.pg,
        UserListFilter {
            user_name: query.user_name,
            nick_name: query.nick_name,
            email: query.email,
            phonenumber: query.phonenumber,
            status: query.status,
            dept_id: query.dept_id,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(UserListItemResponseDto::from_entity))
}

/// Return active users in the current tenant as flat dropdown options.
#[tracing::instrument(skip_all, fields(has_user_name = query.user_name.is_some()))]
pub async fn option_select(
    state: &AppState,
    query: UserOptionQueryDto,
) -> Result<Vec<UserOptionResponseDto>, AppError> {
    let rows = UserRepo::find_option_list(&state.pg, query.user_name.as_deref())
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(UserOptionResponseDto::from_entity)
        .collect())
}

/// Create a user + tenant binding + role bindings in a single transaction.
///
/// Validation order:
/// 1. user_name unique (platform-wide)
/// 2. role_ids (if any) exist in current tenant
/// 3. password hash via bcrypt
/// 4. tx: INSERT sys_user → INSERT sys_user_tenant → REPLACE sys_user_role
/// 5. commit
/// 6. return detail DTO with the role_ids that were submitted
#[tracing::instrument(skip_all, fields(user_name = %dto.user_name, role_count = dto.role_ids.len()))]
pub async fn create(
    state: &AppState,
    dto: CreateUserDto,
) -> Result<UserDetailResponseDto, AppError> {
    // 1. user_name uniqueness
    let unique = UserRepo::verify_user_name_unique(&state.pg, &dto.user_name)
        .await
        .into_internal()?;
    (!unique).business_err_if(ResponseCode::DUPLICATE_KEY)?;

    // 2. role_ids validation
    let roles_ok = RoleRepo::verify_role_ids_in_tenant(&state.pg, &dto.role_ids)
        .await
        .into_internal()?;
    (!roles_ok).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    // 3. hash password
    let password_hash = hash_password(&dto.password)
        .context("hash_password: create")
        .into_internal()?;

    // 4. transaction
    let mut tx = state
        .pg
        .begin()
        .await
        .context("create: begin tx")
        .into_internal()?;

    let user = UserRepo::insert_tx(
        &mut tx,
        UserInsertParams {
            user_name: dto.user_name,
            nick_name: dto.nick_name,
            password_hash,
            dept_id: dto.dept_id,
            email: dto.email,
            phonenumber: dto.phonenumber,
            sex: dto.sex,
            avatar: dto.avatar,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    // Tenant id is now an explicit parameter — extract from request context
    // (previously called inside the method itself).
    let current_tenant = framework::context::current_tenant_scope()
        .context("create user: tenant_id required")
        .into_internal()?;
    TenantRepo::insert_user_tenant_binding_tx(&mut tx, &user.user_id, &current_tenant, "1", "0")
        .await
        .into_internal()?;

    RoleRepo::replace_user_roles_tx(&mut tx, &user.user_id, &dto.role_ids)
        .await
        .into_internal()?;

    tx.commit()
        .await
        .context("create: commit tx")
        .into_internal()?;

    // Return the detail DTO with the submitted role_ids (no extra SELECT).
    Ok(UserDetailResponseDto::from_entity(user, dto.role_ids))
}

/// Update a user's scalar fields + replace role bindings. Returns
/// `DATA_NOT_FOUND` when the user doesn't exist in the current tenant
/// (information hiding — includes cross-tenant edits and soft-deleted
/// users). Validates role_ids pre-transaction.
///
/// NOTE: does NOT modify `user_name` (immutable) or `password` (separate
/// reset-pwd endpoint). Admin edit guard: because `user_name` isn't in
/// the DTO at all, there's no way this endpoint can rename the super
/// admin — the guard is enforced structurally by the DTO shape.
#[tracing::instrument(skip_all, fields(user_id = %dto.user_id, role_count = dto.role_ids.len()))]
pub async fn update(state: &AppState, dto: UpdateUserDto) -> Result<(), AppError> {
    // Validate role_ids pre-transaction
    let roles_ok = RoleRepo::verify_role_ids_in_tenant(&state.pg, &dto.role_ids)
        .await
        .into_internal()?;
    (!roles_ok).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    let mut tx = state
        .pg
        .begin()
        .await
        .context("update: begin tx")
        .into_internal()?;

    // Clone user_id before moving the rest of dto into the params struct —
    // RoleRepo::replace_user_roles_tx needs it after the scalar UPDATE.
    let user_id = dto.user_id.clone();

    let affected = UserRepo::update_tx(
        &mut tx,
        UserUpdateParams {
            user_id: dto.user_id,
            nick_name: dto.nick_name,
            email: dto.email,
            phonenumber: dto.phonenumber,
            sex: dto.sex,
            avatar: dto.avatar,
            status: dto.status,
            dept_id: dto.dept_id,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    // Replace role bindings only after the scalar update succeeded.
    RoleRepo::replace_user_roles_tx(&mut tx, &user_id, &dto.role_ids)
        .await
        .into_internal()?;

    tx.commit()
        .await
        .context("update: commit tx")
        .into_internal()?;

    Ok(())
}

/// Flip a user's status. Guards:
/// - cannot disable self (self-lockout protection)
/// - cannot disable super admin
#[tracing::instrument(skip_all, fields(user_id = %dto.user_id, status = %dto.status))]
pub async fn change_status(state: &AppState, dto: ChangeUserStatusDto) -> Result<(), AppError> {
    // Self-guard
    if is_self_op(&dto.user_id) {
        return Err(AppError::business(ResponseCode::OPERATION_NOT_ALLOWED));
    }

    // Admin-guard
    if is_super_admin_user(state, &dto.user_id).await? {
        return Err(AppError::business(ResponseCode::OPERATION_NOT_ALLOWED));
    }

    let affected = UserRepo::change_status(&state.pg, &dto.user_id, &dto.status)
        .await
        .into_internal()?;
    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)
}

/// Soft-delete one or more users. NestJS accepts a comma-separated list
/// in the path segment (`DELETE /system/user/id1,id2,id3`). We split,
/// pre-validate ALL targets (guards + existence), then process the
/// writes. Validation loops run first so partial success is impossible
/// — either every id in the batch is valid and all are deleted, or the
/// batch aborts before any write happens.
///
/// Guards apply per-target:
/// - cannot delete self
/// - cannot delete super admin
/// - must exist in the current tenant (returns `DATA_NOT_FOUND` otherwise)
///
/// Tiny TOCTOU window: a concurrent delete between the existence check
/// and the write could cause a write to affect 0 rows. In that case we
/// still surface `DATA_NOT_FOUND`, but any earlier id in the batch is
/// already committed — the race is narrow and admin-only, acceptable
/// for Phase 1. A proper fix (wrap the write loop in a transaction)
/// is tracked for Phase 2.
#[tracing::instrument(skip_all, fields(path_ids = %path_ids))]
pub async fn remove(state: &AppState, path_ids: &str) -> Result<(), AppError> {
    let ids: Vec<&str> = path_ids.split(',').filter(|s| !s.is_empty()).collect();
    if ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }

    // Pre-validate guards + existence for ALL ids before any write.
    // Two loops so the whole batch fails fast on the first violation.
    for id in &ids {
        if is_self_op(id) {
            return Err(AppError::business(ResponseCode::OPERATION_NOT_ALLOWED));
        }
        if is_super_admin_user(state, id).await? {
            return Err(AppError::business(ResponseCode::OPERATION_NOT_ALLOWED));
        }
        UserRepo::find_by_id_tenant_scoped(&state.pg, id)
            .await
            .into_internal()?
            .or_business(ResponseCode::DATA_NOT_FOUND)?;
    }

    // Apply deletes. A post-check race (concurrent delete after the
    // existence check but before the write) would surface as 0 affected
    // rows → DATA_NOT_FOUND — documented above.
    for id in &ids {
        let affected = UserRepo::soft_delete_by_id(&state.pg, id)
            .await
            .into_internal()?;
        (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)?;
    }

    Ok(())
}

/// Admin password reset. Validates:
/// - target is not the super-admin row (OPERATION_NOT_ALLOWED)
/// - target exists in the current tenant (DATA_NOT_FOUND)
///
/// After successful UPDATE, bumps the target user's Redis token version
/// so all existing JWTs for that user immediately become invalid on
/// their next request. The bump is best-effort — if Redis is down, the
/// password is still reset (hard-fail on the primary intent) but a
/// warning is logged.
///
/// NO self-guard: an admin may reset their own password. They're already
/// authenticated as themselves, and resetting is just changing.
#[tracing::instrument(skip_all, fields(user_id = %dto.user_id))]
pub async fn reset_password(state: &AppState, dto: ResetPwdDto) -> Result<(), AppError> {
    // Admin guard
    if is_super_admin_user(state, &dto.user_id).await? {
        return Err(AppError::business(ResponseCode::OPERATION_NOT_ALLOWED));
    }

    // Hash the new password
    let password_hash = hash_password(&dto.password)
        .context("hash_password: reset")
        .into_internal()?;

    // Apply the UPDATE
    let affected = UserRepo::reset_password(&state.pg, &dto.user_id, &password_hash)
        .await
        .into_internal()?;
    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    // Best-effort session invalidation via token version bump. A failure
    // here is logged but does NOT roll back the DB change — the password
    // is already reset and the primary auth goal (old password no longer
    // works) is met. The target user's existing JWTs remain valid only
    // until they naturally expire.
    if let Err(e) = framework::auth::session::bump_user_token_version(
        &state.redis,
        &state.config.redis_keys,
        &dto.user_id,
    )
    .await
    {
        tracing::warn!(
            error = %e,
            user_id = %dto.user_id,
            "token version bump failed after password reset"
        );
    }

    Ok(())
}

/// Return the target user's profile + their current role bindings.
/// Tenant-scoped via `find_by_id_tenant_scoped`. No guards — reading
/// any user's role list is just a privileged query.
///
/// The user sub-object uses `UserProfileResponseDto` (not the full
/// detail DTO) specifically because the role list lives at the top
/// level of `AuthRoleResponseDto` — duplicating it inside the nested
/// user would serialize twice AND force a `Vec<String>` clone here.
#[tracing::instrument(skip_all, fields(user_id = %user_id))]
pub async fn find_auth_role(
    state: &AppState,
    user_id: &str,
) -> Result<AuthRoleResponseDto, AppError> {
    let user = UserRepo::find_by_id_tenant_scoped(&state.pg, user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    let role_ids = RoleRepo::find_role_ids_by_user(&state.pg, &user.user_id)
        .await
        .into_internal()?;

    Ok(AuthRoleResponseDto {
        user: UserProfileResponseDto::from_entity(user),
        role_ids,
    })
}

/// Replace a user's role bindings entirely. Guards:
/// - cannot modify own roles (self-guard, prevents privilege escalation)
/// - cannot modify super-admin's roles
/// - target must exist in current tenant
/// - all role_ids must exist in current tenant
///
/// Empty role_ids is valid ("unassign all"). The underlying
/// `RoleRepo::replace_user_roles_tx` handles the DELETE-all + optional
/// bulk INSERT inside the caller's transaction.
#[tracing::instrument(skip_all, fields(user_id = %dto.user_id, role_count = dto.role_ids.len()))]
pub async fn update_auth_role(state: &AppState, dto: AuthRoleUpdateDto) -> Result<(), AppError> {
    // Self-guard
    if is_self_op(&dto.user_id) {
        return Err(AppError::business(ResponseCode::OPERATION_NOT_ALLOWED));
    }

    // Admin-guard
    if is_super_admin_user(state, &dto.user_id).await? {
        return Err(AppError::business(ResponseCode::OPERATION_NOT_ALLOWED));
    }

    // Target must exist in current tenant
    UserRepo::find_by_id_tenant_scoped(&state.pg, &dto.user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    // All role_ids must exist in current tenant
    let roles_ok = RoleRepo::verify_role_ids_in_tenant(&state.pg, &dto.role_ids)
        .await
        .into_internal()?;
    (!roles_ok).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    // Transaction: replace the bindings
    let mut tx = state
        .pg
        .begin()
        .await
        .context("update_auth_role: begin tx")
        .into_internal()?;

    RoleRepo::replace_user_roles_tx(&mut tx, &dto.user_id, &dto.role_ids)
        .await
        .into_internal()?;

    tx.commit()
        .await
        .context("update_auth_role: commit tx")
        .into_internal()?;

    Ok(())
}

/// Return the current logged-in user's profile. Reads user_id from
/// RequestContext and fetches the full row via Phase 0's non-tenant
/// scoped `find_by_id`.
#[tracing::instrument(skip_all)]
pub async fn info(state: &AppState) -> Result<UserInfoResponseDto, AppError> {
    let user_id = RequestContext::with_current(|ctx| ctx.user_id.clone())
        .flatten()
        .or_business(ResponseCode::TOKEN_EXPIRED)?;

    let user = UserRepo::find_by_id(&state.pg, &user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    Ok(UserInfoResponseDto::from_entity(user))
}

//! Role service — business orchestration.

use super::dto::{
    AllocatedUserResponseDto, AuthUserAssignDto, AuthUserCancelDto, AuthUserListQueryDto,
    ChangeRoleStatusDto, CreateRoleDto, ListRoleDto, RoleDetailResponseDto,
    RoleListItemResponseDto, RoleOptionResponseDto, UpdateRoleDto,
};
use crate::domain::{
    AllocatedUserFilter, RoleInsertParams, RoleListFilter, RoleRepo, RoleUpdateParams,
};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckBool, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

/// Fetch a single role by id. Returns `DATA_NOT_FOUND` when the role
/// doesn't exist in the current tenant (tenant scoping is enforced in
/// `RoleRepo::find_by_id`).
#[tracing::instrument(skip_all, fields(role_id = %role_id))]
pub async fn find_by_id(
    state: &AppState,
    role_id: &str,
) -> Result<RoleDetailResponseDto, AppError> {
    let role = RoleRepo::find_by_id(&state.pg, role_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    let menu_ids = RoleRepo::find_menu_ids_by_role(&state.pg, &role.role_id)
        .await
        .into_internal()?;

    Ok(RoleDetailResponseDto::from_entity(role, menu_ids))
}

/// Paginated list. The `ListRoleDto` query params are validated by the
/// `ValidatedQuery` extractor before reaching this function, so we only
/// delegate to `RoleRepo::find_page` and map each row into the lightweight
/// list DTO.
#[tracing::instrument(skip_all, fields(
    page_num = query.page.page_num,
    page_size = query.page.page_size,
))]
pub async fn list(
    state: &AppState,
    query: ListRoleDto,
) -> Result<Page<RoleListItemResponseDto>, AppError> {
    let page = RoleRepo::find_page(
        &state.pg,
        RoleListFilter {
            name: query.role_name,
            role_key: query.role_key,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(RoleListItemResponseDto::from_entity))
}

/// Create a role with its menu bindings. Returns the full detail DTO
/// including the bound menu_ids (which are the same list the caller
/// passed in, so no second SELECT is needed).
///
/// Phase 1 sub-phase 1 does NOT enforce:
///   - `role_key` uniqueness per tenant (NestJS does via a separate query)
///   - `menu_ids` referencing real active menus in the tenant's package
///
/// Both will land in Phase 2 or when the user module unlocks the
/// dependency graph for cross-module validation.
#[tracing::instrument(skip_all, fields(role_name = %dto.role_name, menu_count = dto.menu_ids.len()))]
pub async fn create(
    state: &AppState,
    dto: CreateRoleDto,
) -> Result<RoleDetailResponseDto, AppError> {
    // Clone menu_ids before moving the rest into the params struct —
    // the detail DTO echoes the submitted list back without a second SELECT.
    let menu_ids = dto.menu_ids.clone();

    let mut tx = state
        .pg
        .begin()
        .await
        .context("create: begin tx")
        .into_internal()?;

    let role = RoleRepo::insert_with_menus(
        &mut tx,
        RoleInsertParams {
            role_name: dto.role_name,
            role_key: dto.role_key,
            role_sort: dto.role_sort,
            status: dto.status,
            remark: dto.remark,
            menu_ids: dto.menu_ids,
        },
    )
    .await
    .into_internal()?;

    tx.commit()
        .await
        .context("create: commit tx")
        .into_internal()?;

    Ok(RoleDetailResponseDto::from_entity(role, menu_ids))
}

/// Update a role's scalar fields + menu bindings. Returns `DATA_NOT_FOUND`
/// when the role doesn't exist in the current tenant (including
/// cross-tenant edit attempts, which surface as "not found" for
/// information-hiding per the spec).
///
/// Phase 1 sub-phase 1 does NOT enforce role_key uniqueness per tenant
/// (consistent with `create`). Phase 2 adds it.
#[tracing::instrument(skip_all, fields(role_id = %dto.role_id, menu_count = dto.menu_ids.len()))]
pub async fn update(state: &AppState, dto: UpdateRoleDto) -> Result<(), AppError> {
    let mut tx = state
        .pg
        .begin()
        .await
        .context("update: begin tx")
        .into_internal()?;

    let affected = RoleRepo::update_with_menus(
        &mut tx,
        RoleUpdateParams {
            role_id: dto.role_id,
            role_name: dto.role_name,
            role_key: dto.role_key,
            role_sort: dto.role_sort,
            status: dto.status,
            remark: dto.remark,
            menu_ids: dto.menu_ids,
        },
    )
    .await
    .into_internal()?;

    tx.commit()
        .await
        .context("update: commit tx")
        .into_internal()?;

    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)
}

/// Toggle a role's `status`. Returns `DATA_NOT_FOUND` when the role
/// doesn't exist in the current tenant.
#[tracing::instrument(skip_all, fields(role_id = %dto.role_id, status = %dto.status))]
pub async fn change_status(state: &AppState, dto: ChangeRoleStatusDto) -> Result<(), AppError> {
    let affected = RoleRepo::change_status(&state.pg, &dto.role_id, &dto.status)
        .await
        .into_internal()?;
    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)
}

/// Soft-delete a role. Returns `DATA_NOT_FOUND` when the role doesn't
/// exist in the current tenant.
#[tracing::instrument(skip_all, fields(role_id = %role_id))]
pub async fn remove(state: &AppState, role_id: &str) -> Result<(), AppError> {
    let affected = RoleRepo::soft_delete_by_id(&state.pg, role_id)
        .await
        .into_internal()?;
    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)
}

/// Return all active roles in the current tenant as flat dropdown options.
#[tracing::instrument(skip_all)]
pub async fn option_select(state: &AppState) -> Result<Vec<RoleOptionResponseDto>, AppError> {
    let rows = RoleRepo::find_option_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(RoleOptionResponseDto::from_entity)
        .collect())
}

/// Paginated list of users currently bound to `role_id` in the current
/// tenant. Validation has already been performed by the `ValidatedQuery`
/// extractor before this function is called.
#[tracing::instrument(skip_all, fields(role_id = %query.role_id))]
pub async fn allocated_users(
    state: &AppState,
    query: AuthUserListQueryDto,
) -> Result<Page<AllocatedUserResponseDto>, AppError> {
    let page = RoleRepo::find_allocated_users_page(
        &state.pg,
        AllocatedUserFilter {
            role_id: query.role_id,
            user_name: query.user_name,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(AllocatedUserResponseDto::from_row))
}

/// Paginated list of users in the current tenant who are NOT bound to
/// `role_id`. Validation is extractor-level.
#[tracing::instrument(skip_all, fields(role_id = %query.role_id))]
pub async fn unallocated_users(
    state: &AppState,
    query: AuthUserListQueryDto,
) -> Result<Page<AllocatedUserResponseDto>, AppError> {
    let page = RoleRepo::find_unallocated_users_page(
        &state.pg,
        AllocatedUserFilter {
            role_id: query.role_id,
            user_name: query.user_name,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(AllocatedUserResponseDto::from_row))
}

/// Bulk-assign `user_ids` to `role_id`. Validates the role exists in
/// the current tenant before inserting — returns `DATA_NOT_FOUND`
/// otherwise. Individual `user_ids` are NOT tenant-verified (consistent
/// with how create/update handle menu_ids — Phase 1 defers cross-module
/// validation to Phase 2). The underlying repo call is idempotent.
#[tracing::instrument(skip_all, fields(role_id = %dto.role_id, user_count = dto.user_ids.len()))]
pub async fn assign_users(state: &AppState, dto: AuthUserAssignDto) -> Result<(), AppError> {
    RoleRepo::find_by_id(&state.pg, &dto.role_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    RoleRepo::insert_user_roles(&state.pg, &dto.role_id, &dto.user_ids)
        .await
        .into_internal()?;
    Ok(())
}

/// Bulk-unassign `user_ids` from `role_id`. Validates the role
/// belongs to the current tenant first — prevents a cross-tenant
/// write path where a caller who knows a foreign `role_id` could
/// otherwise delete `sys_user_role` rows that don't belong to their
/// tenant. Once past the guard, the DELETE is idempotent: unassigning
/// users that aren't currently bound affects 0 rows and returns
/// success.
#[tracing::instrument(skip_all, fields(role_id = %dto.role_id, user_count = dto.user_ids.len()))]
pub async fn unassign_users(state: &AppState, dto: AuthUserCancelDto) -> Result<(), AppError> {
    RoleRepo::find_by_id(&state.pg, &dto.role_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    RoleRepo::delete_user_roles(&state.pg, &dto.role_id, &dto.user_ids)
        .await
        .into_internal()?;
    Ok(())
}

//! Role HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{operlog, require_authenticated, require_permission};
use std::convert::Infallible;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/system/role/{id}", tag = "角色管理",
    summary = "查询角色详情",
    params(("id" = String, Path, description = "role id")),
    responses((status = 200, body = ApiResponse<dto::RoleDetailResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<ApiResponse<dto::RoleDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &role_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/role/list", tag = "角色管理",
    summary = "角色列表",
    params(dto::ListRoleDto),
    responses((status = 200, body = ApiResponse<Page<dto::RoleListItemResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListRoleDto>,
) -> Result<ApiResponse<Page<dto::RoleListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/system/role/", tag = "角色管理",
    summary = "新增角色",
    request_body = dto::CreateRoleDto,
    responses((status = 200, body = ApiResponse<dto::RoleDetailResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateRoleDto>,
) -> Result<ApiResponse<dto::RoleDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/role/", tag = "角色管理",
    summary = "修改角色",
    request_body = dto::UpdateRoleDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateRoleDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(put, path = "/system/role/change-status", tag = "角色管理",
    summary = "修改角色状态",
    request_body = dto::ChangeRoleStatusDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn change_status(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ChangeRoleStatusDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::change_status(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/system/role/{id}", tag = "角色管理",
    summary = "删除角色",
    params(("id" = String, Path, description = "role ids, comma separated")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &role_id).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/role/option-select", tag = "角色管理",
    summary = "角色下拉选项",
    responses((status = 200, body = ApiResponse<Vec<dto::RoleOptionResponseDto>>))
)]
pub(crate) async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::RoleOptionResponseDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/role/auth-user/allocated-list", tag = "角色管理",
    summary = "已分配用户列表",
    params(dto::AuthUserListQueryDto),
    responses((status = 200, body = ApiResponse<Page<dto::AllocatedUserResponseDto>>))
)]
pub(crate) async fn allocated_users(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::AuthUserListQueryDto>,
) -> Result<ApiResponse<Page<dto::AllocatedUserResponseDto>>, AppError> {
    let resp = service::allocated_users(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/role/auth-user/unallocated-list", tag = "角色管理",
    summary = "未分配用户列表",
    params(dto::AuthUserListQueryDto),
    responses((status = 200, body = ApiResponse<Page<dto::AllocatedUserResponseDto>>))
)]
pub(crate) async fn unallocated_users(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::AuthUserListQueryDto>,
) -> Result<ApiResponse<Page<dto::AllocatedUserResponseDto>>, AppError> {
    let resp = service::unallocated_users(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/role/auth-user/select-all", tag = "角色管理",
    summary = "批量授权用户",
    request_body = dto::AuthUserAssignDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn assign_users(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthUserAssignDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::assign_users(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(put, path = "/system/role/auth-user/cancel", tag = "角色管理",
    summary = "批量取消授权",
    request_body = dto::AuthUserCancelDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn unassign_users(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthUserCancelDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::unassign_users(&state, dto).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:role:add"))
                .layer(operlog!("角色管理", Insert))
        }))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:role:edit"))
                .layer(operlog!("角色管理", Update))
        }))
        .routes(routes!(list).layer(require_permission!("system:role:list")))
        .routes(routes!(option_select).layer(require_authenticated!()))
        .routes(routes!(change_status).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:role:change-status"))
                .layer(operlog!("角色管理", Update))
        }))
        .routes(routes!(allocated_users).layer(require_permission!("system:role:allocated-list")))
        .routes(
            routes!(unallocated_users).layer(require_permission!("system:role:unallocated-list")),
        )
        .routes(routes!(assign_users).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:role:select-auth-all"))
                .layer(operlog!("角色管理", Grant))
        }))
        .routes(routes!(unassign_users).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:role:cancel-auth"))
                .layer(operlog!("角色管理", Grant))
        }))
        .routes(routes!(find_by_id).layer(require_permission!("system:role:query")))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:role:remove"))
                .layer(operlog!("角色管理", Delete))
        }))
}

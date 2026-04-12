//! Menu HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::operlog;
use framework::response::ApiResponse;
use framework::{require_authenticated, require_permission};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/system/menu/", tag = "菜单管理",
    summary = "新增菜单",
    request_body = dto::CreateMenuDto,
    responses((status = 200, body = ApiResponse<dto::MenuDetailResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateMenuDto>,
) -> Result<ApiResponse<dto::MenuDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/menu/", tag = "菜单管理",
    summary = "修改菜单",
    request_body = dto::UpdateMenuDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateMenuDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/menu/list", tag = "菜单管理",
    summary = "菜单列表",
    params(dto::ListMenuDto),
    responses((status = 200, body = ApiResponse<Vec<dto::MenuDetailResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListMenuDto>,
) -> Result<ApiResponse<Vec<dto::MenuDetailResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/menu/tree-select", tag = "菜单管理",
    summary = "菜单树形选择",
    responses((status = 200, body = ApiResponse<Vec<dto::TreeNode>>))
)]
pub(crate) async fn tree_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::TreeNode>>, AppError> {
    let resp = service::tree_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/menu/role-menu-tree-select/{roleId}", tag = "菜单管理",
    summary = "角色菜单树形选择",
    params(("roleId" = String, Path, description = "role id")),
    responses((status = 200, body = ApiResponse<dto::MenuTreeSelectResponseDto>))
)]
pub(crate) async fn role_menu_tree_select(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<ApiResponse<dto::MenuTreeSelectResponseDto>, AppError> {
    let resp = service::role_menu_tree_select(&state, &role_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/menu/tenant-package-menu-tree-select/{packageId}", tag = "菜单管理",
    summary = "套餐菜单树形选择",
    params(("packageId" = String, Path, description = "package id")),
    responses((status = 200, body = ApiResponse<dto::MenuTreeSelectResponseDto>))
)]
pub(crate) async fn package_menu_tree_select(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
) -> Result<ApiResponse<dto::MenuTreeSelectResponseDto>, AppError> {
    let resp = service::package_menu_tree_select(&state, &package_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/menu/cascade/{menuIds}", tag = "菜单管理",
    summary = "级联删除菜单",
    params(("menuIds" = String, Path, description = "comma-separated menu ids")),
    responses((status = 200, description = "affected row count", body = String))
)]
pub(crate) async fn cascade_remove(
    State(state): State<AppState>,
    Path(menu_ids): Path<String>,
) -> Result<ApiResponse<u64>, AppError> {
    let affected = service::cascade_remove(&state, &menu_ids).await?;
    Ok(ApiResponse::ok(affected))
}

#[utoipa::path(get, path = "/system/menu/{menuId}", tag = "菜单管理",
    summary = "查询菜单详情",
    params(("menuId" = String, Path, description = "menu id")),
    responses((status = 200, body = ApiResponse<dto::MenuDetailResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(menu_id): Path<String>,
) -> Result<ApiResponse<dto::MenuDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &menu_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/menu/{menuId}", tag = "菜单管理",
    summary = "删除菜单",
    params(("menuId" = String, Path, description = "menu id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(menu_id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &menu_id).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:menu:add"))
                .layer(operlog!("菜单管理", Insert))
        }))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:menu:edit"))
                .layer(operlog!("菜单管理", Update))
        }))
        .routes(routes!(list).layer(require_permission!("system:menu:list")))
        .routes(routes!(tree_select).layer(require_permission!("system:menu:tree-select")))
        .routes(
            routes!(role_menu_tree_select).layer(require_permission!("system:menu:role-menu-tree")),
        )
        .routes(routes!(package_menu_tree_select).layer(require_authenticated!()))
        .routes(routes!(cascade_remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:menu:cascade-remove"))
                .layer(operlog!("菜单管理", Delete))
        }))
        .routes(routes!(find_by_id).layer(require_permission!("system:menu:query")))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:menu:remove"))
                .layer(operlog!("菜单管理", Delete))
        }))
}

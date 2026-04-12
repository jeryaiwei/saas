//! Menu HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Router,
};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::ApiResponse;
use framework::{require_authenticated, require_permission};

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

pub fn router() -> Router<AppState> {
    Router::new()
        // CRITICAL: literal-prefix routes MUST be registered BEFORE wildcard `/{menuId}` routes.
        .route(
            "/system/menu/",
            post(create).route_layer(require_permission!("system:menu:add")),
        )
        .route(
            "/system/menu/",
            put(update).route_layer(require_permission!("system:menu:edit")),
        )
        .route(
            "/system/menu/list",
            get(list).route_layer(require_permission!("system:menu:list")),
        )
        .route(
            "/system/menu/tree-select",
            get(tree_select).route_layer(require_permission!("system:menu:tree-select")),
        )
        .route(
            "/system/menu/role-menu-tree-select/{roleId}",
            get(role_menu_tree_select)
                .route_layer(require_permission!("system:menu:role-menu-tree")),
        )
        .route(
            "/system/menu/tenant-package-menu-tree-select/{packageId}",
            get(package_menu_tree_select).route_layer(require_authenticated!()),
        )
        .route(
            "/system/menu/cascade/{menuIds}",
            delete(cascade_remove).route_layer(require_permission!("system:menu:cascade-remove")),
        )
        .route(
            "/system/menu/{menuId}",
            get(find_by_id).route_layer(require_permission!("system:menu:query")),
        )
        .route(
            "/system/menu/{menuId}",
            delete(remove).route_layer(require_permission!("system:menu:remove")),
        )
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    create,
    update,
    list,
    tree_select,
    role_menu_tree_select,
    package_menu_tree_select,
    cascade_remove,
    find_by_id,
    remove
))]
pub struct MenuApi;

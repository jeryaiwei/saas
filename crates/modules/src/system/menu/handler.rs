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

async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateMenuDto>,
) -> Result<ApiResponse<dto::MenuDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateMenuDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListMenuDto>,
) -> Result<ApiResponse<Vec<dto::MenuDetailResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

async fn tree_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::TreeNode>>, AppError> {
    let resp = service::tree_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

async fn role_menu_tree_select(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<ApiResponse<dto::MenuTreeSelectResponseDto>, AppError> {
    let resp = service::role_menu_tree_select(&state, &role_id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn package_menu_tree_select(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
) -> Result<ApiResponse<dto::MenuTreeSelectResponseDto>, AppError> {
    let resp = service::package_menu_tree_select(&state, &package_id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn cascade_remove(
    State(state): State<AppState>,
    Path(menu_ids): Path<String>,
) -> Result<ApiResponse<u64>, AppError> {
    let affected = service::cascade_remove(&state, &menu_ids).await?;
    Ok(ApiResponse::ok(affected))
}

async fn find_by_id(
    State(state): State<AppState>,
    Path(menu_id): Path<String>,
) -> Result<ApiResponse<dto::MenuDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &menu_id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn remove(
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

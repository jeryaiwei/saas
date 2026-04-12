//! Role HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Router,
};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{require_authenticated, require_permission};

async fn find_by_id(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<ApiResponse<dto::RoleDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &role_id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListRoleDto>,
) -> Result<ApiResponse<Page<dto::RoleListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateRoleDto>,
) -> Result<ApiResponse<dto::RoleDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateRoleDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn change_status(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ChangeRoleStatusDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::change_status(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn remove(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &role_id).await?;
    Ok(ApiResponse::success())
}

async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::RoleOptionResponseDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

async fn allocated_users(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::AuthUserListQueryDto>,
) -> Result<ApiResponse<Page<dto::AllocatedUserResponseDto>>, AppError> {
    let resp = service::allocated_users(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

async fn unallocated_users(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::AuthUserListQueryDto>,
) -> Result<ApiResponse<Page<dto::AllocatedUserResponseDto>>, AppError> {
    let resp = service::unallocated_users(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

async fn assign_users(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthUserAssignDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::assign_users(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn unassign_users(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthUserCancelDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::unassign_users(&state, dto).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/role/",
            post(create).route_layer(require_permission!("system:role:add")),
        )
        .route(
            "/system/role/",
            put(update).route_layer(require_permission!("system:role:edit")),
        )
        .route(
            "/system/role/list",
            get(list).route_layer(require_permission!("system:role:list")),
        )
        .route(
            "/system/role/option-select",
            get(option_select).route_layer(require_authenticated!()),
        )
        .route(
            "/system/role/change-status",
            put(change_status).route_layer(require_permission!("system:role:change-status")),
        )
        .route(
            "/system/role/auth-user/allocated-list",
            get(allocated_users).route_layer(require_permission!("system:role:allocated-list")),
        )
        .route(
            "/system/role/auth-user/unallocated-list",
            get(unallocated_users).route_layer(require_permission!("system:role:unallocated-list")),
        )
        .route(
            "/system/role/auth-user/select-all",
            put(assign_users).route_layer(require_permission!("system:role:select-auth-all")),
        )
        .route(
            "/system/role/auth-user/cancel",
            put(unassign_users).route_layer(require_permission!("system:role:cancel-auth")),
        )
        .route(
            "/system/role/{id}",
            get(find_by_id).route_layer(require_permission!("system:role:query")),
        )
        .route(
            "/system/role/{id}",
            delete(remove).route_layer(require_permission!("system:role:remove")),
        )
}

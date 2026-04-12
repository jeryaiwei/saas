//! User HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Router,
};
use framework::auth::Role;
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{require_authenticated, require_permission, require_role};

async fn find_by_id(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<ApiResponse<dto::UserDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &user_id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListUserDto>,
) -> Result<ApiResponse<Page<dto::UserListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

async fn option_select(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::UserOptionQueryDto>,
) -> Result<ApiResponse<Vec<dto::UserOptionResponseDto>>, AppError> {
    let resp = service::option_select(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

async fn info(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::UserInfoResponseDto>, AppError> {
    let resp = service::info(&state).await?;
    Ok(ApiResponse::ok(resp))
}

async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateUserDto>,
) -> Result<ApiResponse<dto::UserDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateUserDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn change_status(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ChangeUserStatusDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::change_status(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}

async fn reset_password(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ResetPwdDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::reset_password(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn auth_role(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<ApiResponse<dto::AuthRoleResponseDto>, AppError> {
    let resp = service::find_auth_role(&state, &user_id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn update_auth_role(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthRoleUpdateDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_auth_role(&state, dto).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/user/",
            post(create).route_layer(require_permission!("system:user:add")),
        )
        .route(
            "/system/user/",
            put(update).route_layer(require_permission!("system:user:edit")),
        )
        .route(
            "/system/user/list",
            get(list).route_layer(require_permission!("system:user:list")),
        )
        .route(
            "/system/user/option-select",
            get(option_select).route_layer(require_authenticated!()),
        )
        .route(
            "/system/user/info",
            get(info).route_layer(require_authenticated!()),
        )
        .route(
            "/system/user/change-status",
            put(change_status).route_layer(require_role!(Role::TenantAdmin)),
        )
        .route(
            "/system/user/reset-pwd",
            put(reset_password).route_layer(require_role!(Role::TenantAdmin)),
        )
        .route(
            "/system/user/auth-role",
            put(update_auth_role).route_layer(require_role!(Role::TenantAdmin)),
        )
        .route(
            "/system/user/auth-role/{id}",
            get(auth_role).route_layer(require_role!(Role::TenantAdmin)),
        )
        .route(
            "/system/user/{id}",
            get(find_by_id).route_layer(require_permission!("system:user:query")),
        )
        .route(
            "/system/user/{id}",
            delete(remove).route_layer(require_role!(Role::TenantAdmin)),
        )
}

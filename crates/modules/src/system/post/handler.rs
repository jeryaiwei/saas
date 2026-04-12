//! Post HTTP handlers + router wiring.

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

async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreatePostDto>,
) -> Result<ApiResponse<dto::PostResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdatePostDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListPostDto>,
) -> Result<ApiResponse<Page<dto::PostResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::PostResponseDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::PostResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/post/",
            post(create).route_layer(require_permission!("system:post:add")),
        )
        .route(
            "/system/post/",
            put(update).route_layer(require_permission!("system:post:edit")),
        )
        .route(
            "/system/post/list",
            get(list).route_layer(require_permission!("system:post:list")),
        )
        .route(
            "/system/post/option-select",
            get(option_select).route_layer(require_authenticated!()),
        )
        .route(
            "/system/post/{id}",
            get(find_by_id).route_layer(require_permission!("system:post:query")),
        )
        .route(
            "/system/post/{id}",
            delete(remove).route_layer(require_permission!("system:post:remove")),
        )
}

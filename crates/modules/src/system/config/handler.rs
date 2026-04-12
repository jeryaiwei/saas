//! Config HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Router,
};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::require_permission;
use framework::response::{ApiResponse, Page};

async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateConfigDto>,
) -> Result<ApiResponse<dto::ConfigResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateConfigDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn update_by_key(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateConfigByKeyDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_by_key(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListConfigDto>,
) -> Result<ApiResponse<Page<dto::ConfigResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::ConfigResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn find_by_key(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<ApiResponse<dto::ConfigResponseDto>, AppError> {
    let resp = service::find_by_key(&state, &key).await?;
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
            "/system/config/",
            post(create).route_layer(require_permission!("system:config:add")),
        )
        .route(
            "/system/config/",
            put(update).route_layer(require_permission!("system:config:edit")),
        )
        .route(
            "/system/config/key",
            put(update_by_key).route_layer(require_permission!("system:config:edit-by-key")),
        )
        .route(
            "/system/config/list",
            get(list).route_layer(require_permission!("system:config:list")),
        )
        // literal-prefix routes BEFORE wildcard `/{id}`
        .route(
            "/system/config/key/{config_key}",
            get(find_by_key).route_layer(require_permission!("system:config:query-by-key")),
        )
        .route(
            "/system/config/{id}",
            get(find_by_id).route_layer(require_permission!("system:config:query")),
        )
        .route(
            "/system/config/{id}",
            delete(remove).route_layer(require_permission!("system:config:remove")),
        )
}

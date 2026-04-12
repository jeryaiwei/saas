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

#[utoipa::path(post, path = "/system/config/", tag = "配置管理",
    summary = "新增配置",
    request_body = dto::CreateConfigDto,
    responses((status = 200, body = ApiResponse<dto::ConfigResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateConfigDto>,
) -> Result<ApiResponse<dto::ConfigResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/config/", tag = "配置管理",
    summary = "修改配置",
    request_body = dto::UpdateConfigDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateConfigDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(put, path = "/system/config/key", tag = "配置管理",
    summary = "按键名修改配置值",
    request_body = dto::UpdateConfigByKeyDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update_by_key(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateConfigByKeyDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_by_key(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/config/list", tag = "配置管理",
    summary = "配置列表",
    params(dto::ListConfigDto),
    responses((status = 200, body = ApiResponse<Page<dto::ConfigResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListConfigDto>,
) -> Result<ApiResponse<Page<dto::ConfigResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/system/config/{id}", tag = "配置管理",
    summary = "查询配置详情",
    params(("id" = String, Path, description = "config id")),
    responses((status = 200, body = ApiResponse<dto::ConfigResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::ConfigResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/config/key/{config_key}", tag = "配置管理",
    summary = "按键名查询配置",
    params(("config_key" = String, Path, description = "config key")),
    responses((status = 200, body = ApiResponse<dto::ConfigResponseDto>))
)]
pub(crate) async fn find_by_key(
    State(state): State<AppState>,
    Path(key): Path<String>,
) -> Result<ApiResponse<dto::ConfigResponseDto>, AppError> {
    let resp = service::find_by_key(&state, &key).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/config/{id}", tag = "配置管理",
    summary = "删除配置",
    params(("id" = String, Path, description = "ids, comma separated")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
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

#[derive(utoipa::OpenApi)]
#[openapi(paths(create, update, update_by_key, list, find_by_id, find_by_key, remove))]
pub struct ConfigApi;

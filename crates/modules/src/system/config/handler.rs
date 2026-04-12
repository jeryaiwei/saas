//! Config HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::operlog;
use framework::require_permission;
use framework::response::{ApiResponse, Page};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

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

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:config:add"))
                .layer(operlog!("配置管理", Insert))
        }))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:config:edit"))
                .layer(operlog!("配置管理", Update))
        }))
        .routes(routes!(update_by_key).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:config:edit-by-key"))
                .layer(operlog!("配置管理", Update))
        }))
        .routes(routes!(list).layer(require_permission!("system:config:list")))
        // literal-prefix routes BEFORE wildcard `/{id}`
        .routes(routes!(find_by_key).layer(require_permission!("system:config:query-by-key")))
        .routes(routes!(find_by_id).layer(require_permission!("system:config:query")))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:config:remove"))
                .layer(operlog!("配置管理", Delete))
        }))
}

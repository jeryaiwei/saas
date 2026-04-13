//! Cache monitor HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::response::ApiResponse;
use framework::{operlog, require_permission};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/monitor/cache", tag = "缓存监控",
    summary = "缓存监控信息",
    responses((status = 200, body = ApiResponse<dto::CacheInfoDto>))
)]
pub(crate) async fn get_info(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::CacheInfoDto>, AppError> {
    let resp = service::get_info(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/monitor/cache/names", tag = "缓存监控",
    summary = "缓存分类列表",
    responses((status = 200, body = ApiResponse<Vec<dto::CacheNameDto>>))
)]
pub(crate) async fn get_names() -> Result<ApiResponse<Vec<dto::CacheNameDto>>, AppError> {
    Ok(ApiResponse::ok(service::get_names()))
}

#[utoipa::path(get, path = "/monitor/cache/{cacheName}/keys", tag = "缓存监控",
    summary = "缓存键列表",
    params(("cacheName" = String, Path, description = "cache name prefix")),
    responses((status = 200, body = ApiResponse<Vec<dto::CacheKeyDto>>))
)]
pub(crate) async fn get_keys(
    State(state): State<AppState>,
    Path(cache_name): Path<String>,
) -> Result<ApiResponse<Vec<dto::CacheKeyDto>>, AppError> {
    let resp = service::get_keys(&state, &cache_name).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/monitor/cache/{cacheName}/keys/{cacheKey}", tag = "缓存监控",
    summary = "查询缓存值",
    params(
        ("cacheName" = String, Path, description = "cache name prefix"),
        ("cacheKey" = String, Path, description = "full cache key"),
    ),
    responses((status = 200, body = ApiResponse<dto::CacheValueDto>))
)]
pub(crate) async fn get_value(
    State(state): State<AppState>,
    Path((_cache_name, cache_key)): Path<(String, String)>,
) -> Result<ApiResponse<dto::CacheValueDto>, AppError> {
    let resp = service::get_value(&state, &cache_key).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/monitor/cache/keys/{cacheKey}", tag = "缓存监控",
    summary = "清除指定缓存",
    params(("cacheKey" = String, Path, description = "full cache key")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn clear_cache_key(
    State(state): State<AppState>,
    Path(cache_key): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::clear_cache_key(&state, &cache_key).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/monitor/cache/all", tag = "缓存监控",
    summary = "清空所有缓存",
    responses((status = 200, description = "success"))
)]
pub(crate) async fn clear_all(State(state): State<AppState>) -> Result<ApiResponse<()>, AppError> {
    service::clear_all(&state).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/monitor/cache/{cacheName}", tag = "缓存监控",
    summary = "清除分类缓存",
    params(("cacheName" = String, Path, description = "cache name prefix")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn clear_cache_name(
    State(state): State<AppState>,
    Path(cache_name): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::clear_cache_name(&state, &cache_name).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // --- GET info (no path param) ---
        .routes(routes!(get_info).layer(require_permission!("monitor:cache:query")))
        // --- Literal paths BEFORE wildcard `/{cacheName}` ---
        .routes(routes!(get_names).layer(require_permission!("monitor:cache:list")))
        .routes(routes!(clear_cache_key).map(|r| {
            r.layer::<_, Infallible>(require_permission!("monitor:cache:remove"))
                .layer(operlog!("缓存监控", Delete))
        }))
        .routes(routes!(clear_all).map(|r| {
            r.layer::<_, Infallible>(require_permission!("monitor:cache:remove"))
                .layer(operlog!("缓存监控", Clean))
        }))
        // --- Wildcard `/{cacheName}` paths ---
        .routes(routes!(get_keys).layer(require_permission!("monitor:cache:list")))
        .routes(routes!(get_value).layer(require_permission!("monitor:cache:query")))
        .routes(routes!(clear_cache_name).map(|r| {
            r.layer::<_, Infallible>(require_permission!("monitor:cache:remove"))
                .layer(operlog!("缓存监控", Clean))
        }))
}

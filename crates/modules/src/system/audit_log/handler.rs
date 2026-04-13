//! Audit log HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::ValidatedQuery;
use framework::require_permission;
use framework::response::{ApiResponse, Page};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/system/tenant-audit/list", tag = "审计日志",
    summary = "审计日志列表",
    params(dto::ListAuditLogDto),
    responses((status = 200, body = ApiResponse<Page<dto::AuditLogResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListAuditLogDto>,
) -> Result<ApiResponse<Page<dto::AuditLogResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/system/tenant-audit/{id}", tag = "审计日志",
    summary = "查询审计日志详情",
    params(("id" = String, Path, description = "audit log id")),
    responses((status = 200, body = ApiResponse<dto::AuditLogResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::AuditLogResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant-audit/stats/summary", tag = "审计日志",
    summary = "审计日志统计",
    responses((status = 200, body = ApiResponse<dto::AuditLogStatsDto>))
)]
pub(crate) async fn stats_summary(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::AuditLogStatsDto>, AppError> {
    let resp = service::stats_summary(&state).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list).layer(require_permission!("system:tenant-audit:list")))
        // literal-prefix routes BEFORE wildcard `/{id}`
        .routes(routes!(stats_summary).layer(require_permission!("system:tenant-audit:stats")))
        .routes(routes!(find_by_id).layer(require_permission!("system:tenant-audit:query")))
}

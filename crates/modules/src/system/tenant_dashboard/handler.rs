//! Tenant dashboard HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::State;
use framework::auth::Role;
use framework::error::AppError;
use framework::require_role;
use framework::response::ApiResponse;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/system/tenant-dashboard/stats", tag = "租户仪表盘",
    summary = "租户统计概览",
    responses((status = 200, body = ApiResponse<dto::TenantStatsDto>))
)]
pub(crate) async fn get_stats(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::TenantStatsDto>, AppError> {
    let resp = service::get_stats(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant-dashboard/trend", tag = "租户仪表盘",
    summary = "租户增长趋势",
    responses((status = 200, body = ApiResponse<Vec<dto::TenantTrendDto>>))
)]
pub(crate) async fn get_trend(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::TenantTrendDto>>, AppError> {
    let resp = service::get_trend(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant-dashboard/package-distribution", tag = "租户仪表盘",
    summary = "套餐分布",
    responses((status = 200, body = ApiResponse<Vec<dto::PackageDistributionDto>>))
)]
pub(crate) async fn get_package_distribution(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::PackageDistributionDto>>, AppError> {
    let resp = service::get_package_distribution(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant-dashboard/expiring-tenants", tag = "租户仪表盘",
    summary = "即将过期租户",
    responses((status = 200, body = ApiResponse<Vec<dto::ExpiringTenantDto>>))
)]
pub(crate) async fn get_expiring(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::ExpiringTenantDto>>, AppError> {
    let resp = service::get_expiring(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant-dashboard/quota-top", tag = "租户仪表盘",
    summary = "配额使用 Top10",
    responses((status = 200, body = ApiResponse<Vec<dto::QuotaTopTenantDto>>))
)]
pub(crate) async fn get_quota_top(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::QuotaTopTenantDto>>, AppError> {
    let resp = service::get_quota_top(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant-dashboard/", tag = "租户仪表盘",
    summary = "租户仪表盘全部数据",
    responses((status = 200, body = ApiResponse<dto::TenantDashboardDto>))
)]
pub(crate) async fn get_dashboard(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::TenantDashboardDto>, AppError> {
    let resp = service::get_dashboard(&state).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(get_stats).layer(require_role!(Role::SuperAdmin)))
        .routes(routes!(get_trend).layer(require_role!(Role::SuperAdmin)))
        .routes(routes!(get_package_distribution).layer(require_role!(Role::SuperAdmin)))
        .routes(routes!(get_expiring).layer(require_role!(Role::SuperAdmin)))
        .routes(routes!(get_quota_top).layer(require_role!(Role::SuperAdmin)))
        .routes(routes!(get_dashboard).layer(require_role!(Role::SuperAdmin)))
}

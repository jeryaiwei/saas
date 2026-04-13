//! Tenant dashboard DTOs.

use serde::Serialize;

/// High-level tenant statistics.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantStatsDto {
    pub total_tenants: i64,
    pub active_tenants: i64,
    pub disabled_tenants: i64,
    /// Tenants expiring within 30 days.
    pub expiring_soon: i64,
}

/// Daily tenant creation count for trend charts.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantTrendDto {
    pub date: String,
    pub count: i64,
}

/// Package distribution — how many tenants per package.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageDistributionDto {
    pub package_name: String,
    pub count: i64,
}

/// A tenant that is about to expire.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpiringTenantDto {
    pub tenant_id: String,
    pub company_name: String,
    pub expire_time: Option<String>,
    pub days_remaining: i64,
}

/// Tenant ranked by quota usage.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct QuotaTopTenantDto {
    pub tenant_id: String,
    pub company_name: String,
    pub account_count: i32,
    pub storage_used: i32,
    pub storage_quota: i32,
}

/// Aggregated dashboard data combining all sub-queries.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantDashboardDto {
    pub stats: TenantStatsDto,
    pub trend: Vec<TenantTrendDto>,
    pub package_distribution: Vec<PackageDistributionDto>,
    pub expiring_tenants: Vec<ExpiringTenantDto>,
    pub quota_top: Vec<QuotaTopTenantDto>,
}

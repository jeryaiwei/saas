//! Tenant dashboard service — raw SQL queries on sys_tenant / sys_tenant_package.

use super::dto::{
    ExpiringTenantDto, PackageDistributionDto, QuotaTopTenantDto, TenantDashboardDto,
    TenantStatsDto, TenantTrendDto,
};
use crate::state::AppState;
use framework::error::{AppError, IntoAppError};

/// Tenant statistics overview.
#[tracing::instrument(skip_all)]
pub async fn get_stats(state: &AppState) -> Result<TenantStatsDto, AppError> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        "SELECT \
           COUNT(*) as total, \
           COUNT(*) FILTER (WHERE status = '0') as active, \
           COUNT(*) FILTER (WHERE status = '1') as disabled, \
           COUNT(*) FILTER (WHERE expire_time IS NOT NULL \
             AND expire_time < NOW() + INTERVAL '30 days' \
             AND expire_time > NOW()) as expiring \
         FROM sys_tenant WHERE del_flag = '0'",
    )
    .fetch_one(&state.pg)
    .await
    .map_err(|e| anyhow::anyhow!("tenant stats query: {e}"))
    .into_internal()?;

    Ok(TenantStatsDto {
        total_tenants: row.0,
        active_tenants: row.1,
        disabled_tenants: row.2,
        expiring_soon: row.3,
    })
}

/// Tenant creation trend over the last 30 days.
#[tracing::instrument(skip_all)]
pub async fn get_trend(state: &AppState) -> Result<Vec<TenantTrendDto>, AppError> {
    let rows = sqlx::query_as::<_, (chrono::NaiveDate, i64)>(
        "SELECT DATE(create_at) as date, COUNT(*) as count \
         FROM sys_tenant \
         WHERE del_flag = '0' AND create_at >= NOW() - INTERVAL '30 days' \
         GROUP BY DATE(create_at) ORDER BY date",
    )
    .fetch_all(&state.pg)
    .await
    .map_err(|e| anyhow::anyhow!("tenant trend query: {e}"))
    .into_internal()?;

    Ok(rows
        .into_iter()
        .map(|(date, count)| TenantTrendDto {
            date: date.to_string(),
            count,
        })
        .collect())
}

/// Package distribution — tenant count per package.
#[tracing::instrument(skip_all)]
pub async fn get_package_distribution(
    state: &AppState,
) -> Result<Vec<PackageDistributionDto>, AppError> {
    let rows = sqlx::query_as::<_, (Option<String>, i64)>(
        "SELECT p.package_name, COUNT(t.id) as count \
         FROM sys_tenant t \
         LEFT JOIN sys_tenant_package p ON t.package_id = p.package_id \
         WHERE t.del_flag = '0' \
         GROUP BY p.package_name",
    )
    .fetch_all(&state.pg)
    .await
    .map_err(|e| anyhow::anyhow!("package distribution query: {e}"))
    .into_internal()?;

    Ok(rows
        .into_iter()
        .map(|(name, count)| PackageDistributionDto {
            package_name: name.unwrap_or_else(|| "未分配".to_string()),
            count,
        })
        .collect())
}

/// Tenants expiring within 30 days, ordered by expire_time (limit 20).
#[tracing::instrument(skip_all)]
pub async fn get_expiring(state: &AppState) -> Result<Vec<ExpiringTenantDto>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, Option<chrono::NaiveDateTime>, i64)>(
        "SELECT tenant_id, company_name, expire_time, \
           EXTRACT(DAY FROM expire_time - NOW())::bigint as days_remaining \
         FROM sys_tenant \
         WHERE del_flag = '0' AND expire_time IS NOT NULL \
           AND expire_time > NOW() AND expire_time < NOW() + INTERVAL '30 days' \
         ORDER BY expire_time LIMIT 20",
    )
    .fetch_all(&state.pg)
    .await
    .map_err(|e| anyhow::anyhow!("expiring tenants query: {e}"))
    .into_internal()?;

    Ok(rows
        .into_iter()
        .map(
            |(tenant_id, company_name, expire_time, days_remaining)| ExpiringTenantDto {
                tenant_id,
                company_name,
                expire_time: expire_time.map(|t| t.to_string()),
                days_remaining,
            },
        )
        .collect())
}

/// Top 10 tenants by storage usage.
#[tracing::instrument(skip_all)]
pub async fn get_quota_top(state: &AppState) -> Result<Vec<QuotaTopTenantDto>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, i32, i32, i32)>(
        "SELECT tenant_id, company_name, account_count, storage_used, storage_quota \
         FROM sys_tenant \
         WHERE del_flag = '0' AND status = '0' \
         ORDER BY storage_used DESC LIMIT 10",
    )
    .fetch_all(&state.pg)
    .await
    .map_err(|e| anyhow::anyhow!("quota top query: {e}"))
    .into_internal()?;

    Ok(rows
        .into_iter()
        .map(
            |(tenant_id, company_name, account_count, storage_used, storage_quota)| {
                QuotaTopTenantDto {
                    tenant_id,
                    company_name,
                    account_count,
                    storage_used,
                    storage_quota,
                }
            },
        )
        .collect())
}

/// Aggregate all dashboard data in parallel.
#[tracing::instrument(skip_all)]
pub async fn get_dashboard(state: &AppState) -> Result<TenantDashboardDto, AppError> {
    let (stats, trend, package_distribution, expiring_tenants, quota_top) = tokio::try_join!(
        get_stats(state),
        get_trend(state),
        get_package_distribution(state),
        get_expiring(state),
        get_quota_top(state),
    )?;

    Ok(TenantDashboardDto {
        stats,
        trend,
        package_distribution,
        expiring_tenants,
        quota_top,
    })
}

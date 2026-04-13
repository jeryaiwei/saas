//! AuditLogRepo — read-only queries for `sys_audit_log`.
//!
//! STRICT tenant-scoped. No write operations.

use super::entities::SysAuditLog;
use anyhow::Context;
use framework::context::current_tenant_scope;
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};

const AUDIT_LOG_COLUMNS: &str = "\
    id, tenant_id, user_id, user_name, action, module, \
    target_type, target_id, old_value, new_value, ip, \
    user_agent, request_id, status, error_msg, duration, create_at";

const AUDIT_LOG_PAGE_WHERE: &str = "\
    WHERE tenant_id = $1 \
      AND ($2::varchar IS NULL OR action LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR module LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR user_name LIKE '%' || $4 || '%') \
      AND ($5::varchar IS NULL OR status = $5)";

#[derive(Debug)]
pub struct AuditLogListFilter {
    pub action: Option<String>,
    pub module: Option<String>,
    pub user_name: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

pub struct AuditLogRepo;

impl AuditLogRepo {
    #[tracing::instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: &str,
    ) -> anyhow::Result<Option<SysAuditLog>> {
        let tenant = current_tenant_scope().context("audit_log.find_by_id: tenant_id required")?;
        let sql = format!(
            "SELECT {AUDIT_LOG_COLUMNS} FROM sys_audit_log \
              WHERE id = $1 AND tenant_id = $2 LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysAuditLog>(&sql)
            .bind(id)
            .bind(&tenant)
            .fetch_optional(executor)
            .await
            .context("find_by_id: select sys_audit_log")?;
        Ok(row)
    }

    #[tracing::instrument(skip_all, fields(
        has_action = filter.action.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: AuditLogListFilter,
    ) -> anyhow::Result<framework::response::Page<SysAuditLog>> {
        let mut conn = conn
            .acquire()
            .await
            .context("audit_log.find_page: acquire")?;
        let tenant = current_tenant_scope().context("audit_log.find_page: tenant_id required")?;
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {AUDIT_LOG_COLUMNS} FROM sys_audit_log \
             {AUDIT_LOG_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $6 OFFSET $7"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysAuditLog>(&rows_sql)
                .bind(&tenant)
                .bind(filter.action.as_deref())
                .bind(filter.module.as_deref())
                .bind(filter.user_name.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "audit_log.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_audit_log {AUDIT_LOG_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(&tenant)
                .bind(filter.action.as_deref())
                .bind(filter.module.as_deref())
                .bind(filter.user_name.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "audit_log.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            tracing::warn!(
                got = rows.len(),
                limit = p.limit,
                "audit_log.find_page: rows exceeded LIMIT; truncating"
            );
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(
                rows_ms,
                count_ms,
                total_ms,
                "audit_log.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Stats summary using an acquirable connection for multiple queries.
    #[tracing::instrument(skip_all)]
    pub async fn stats_summary_full(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
    ) -> anyhow::Result<(i64, i64, Vec<(String, i64)>)> {
        let mut conn = conn.acquire().await.context("audit_log.stats: acquire")?;
        let tenant = current_tenant_scope().context("audit_log.stats: tenant_id required")?;

        let counts: (i64, i64) = sqlx::query_as(
            "SELECT \
                COUNT(*), \
                COUNT(*) FILTER (WHERE create_at >= CURRENT_DATE) \
             FROM sys_audit_log \
             WHERE tenant_id = $1",
        )
        .bind(&tenant)
        .fetch_one(&mut *conn)
        .await
        .context("stats: counts")?;

        let action_rows: Vec<(String, i64)> = sqlx::query_as(
            "SELECT action, COUNT(*) \
             FROM sys_audit_log \
             WHERE tenant_id = $1 \
             GROUP BY action \
             ORDER BY COUNT(*) DESC \
             LIMIT 50",
        )
        .bind(&tenant)
        .fetch_all(&mut *conn)
        .await
        .context("stats: action_counts")?;

        Ok((counts.0, counts.1, action_rows))
    }
}

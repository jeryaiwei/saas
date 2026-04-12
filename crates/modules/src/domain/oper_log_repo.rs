//! OperLogRepo — hand-written SQL for sys_oper_log.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. STRICT tenant model — filtered by tenant_id.
//! 4. Read-only + delete (no insert/update from API).

use super::entities::SysOperLog;
use anyhow::Context;
use framework::context::current_tenant_scope;
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use sqlx::PgPool;
use tracing::instrument;

const COLUMNS: &str = "\
    oper_id, tenant_id, title, business_type, request_method, \
    operator_type, oper_name, dept_name, oper_url, oper_location, \
    oper_param, json_result, error_msg, method, oper_ip, \
    oper_time, status, cost_time";

const OPER_LOG_PAGE_WHERE: &str = "\
    WHERE ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR title LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR oper_name LIKE '%' || $3 || '%') \
      AND ($4::int4 IS NULL OR business_type = $4) \
      AND ($5::varchar IS NULL OR status = $5)";

#[derive(Debug)]
pub struct OperLogListFilter {
    pub title: Option<String>,
    pub oper_name: Option<String>,
    pub business_type: Option<i32>,
    pub status: Option<String>,
    pub page: PageQuery,
}

pub struct OperLogRepo;

impl OperLogRepo {
    #[instrument(skip_all, fields(oper_id = %oper_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        oper_id: &str,
    ) -> anyhow::Result<Option<SysOperLog>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_oper_log \
             WHERE oper_id = $1 \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysOperLog>(&sql)
            .bind(oper_id)
            .bind(tenant.as_deref())
            .fetch_optional(executor)
            .await
            .context("oper_log.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_title = filter.title.is_some(),
        has_oper_name = filter.oper_name.is_some(),
        has_business_type = filter.business_type.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        pool: &PgPool,
        filter: OperLogListFilter,
    ) -> anyhow::Result<framework::response::Page<SysOperLog>> {
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_oper_log {OPER_LOG_PAGE_WHERE} \
             ORDER BY oper_time DESC \
             LIMIT $6 OFFSET $7"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysOperLog>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.title.as_deref())
                .bind(filter.oper_name.as_deref())
                .bind(filter.business_type)
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "oper_log.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_oper_log {OPER_LOG_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.title.as_deref())
                .bind(filter.oper_name.as_deref())
                .bind(filter.business_type)
                .bind(filter.status.as_deref())
                .fetch_one(pool),
            "oper_log.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(
                rows_ms,
                count_ms,
                total_ms,
                "oper_log.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Hard delete a single oper log entry.
    #[instrument(skip_all, fields(oper_id = %oper_id))]
    pub async fn delete_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        oper_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let rows = sqlx::query(
            "DELETE FROM sys_oper_log \
             WHERE oper_id = $1 \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(oper_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("oper_log.delete_by_id")?
        .rows_affected();
        Ok(rows)
    }

    /// Hard delete all oper log entries for the current tenant.
    #[instrument(skip_all)]
    pub async fn delete_all(executor: impl sqlx::PgExecutor<'_>) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let rows = sqlx::query(
            "DELETE FROM sys_oper_log \
             WHERE ($1::varchar IS NULL OR tenant_id = $1)",
        )
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("oper_log.delete_all")?
        .rows_affected();
        Ok(rows)
    }
}

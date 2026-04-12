//! LoginLogRepo — hand-written SQL for sys_logininfor.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. STRICT tenant model — filtered by tenant_id.
//! 4. Read-only + soft-delete (no insert/update from API).

use super::entities::SysLogininfor;
use anyhow::Context;
use framework::context::{audit_update_by, current_tenant_scope};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    info_id, tenant_id, user_name, ipaddr, login_location, \
    browser, os, device_type, status, msg, del_flag, login_time";

const LOGIN_LOG_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR user_name LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR ipaddr LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR status = $4)";

#[derive(Debug)]
pub struct LoginLogListFilter {
    pub user_name: Option<String>,
    pub ipaddr: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

pub struct LoginLogRepo;

impl LoginLogRepo {
    #[instrument(skip_all, fields(
        has_user_name = filter.user_name.is_some(),
        has_ipaddr = filter.ipaddr.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: LoginLogListFilter,
    ) -> anyhow::Result<framework::response::Page<SysLogininfor>> {
        let mut conn = conn
            .acquire()
            .await
            .context("login_log.find_page: acquire")?;
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_logininfor {LOGIN_LOG_PAGE_WHERE} \
             ORDER BY login_time DESC \
             LIMIT $5 OFFSET $6"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysLogininfor>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.user_name.as_deref())
                .bind(filter.ipaddr.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "login_log.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_logininfor {LOGIN_LOG_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.user_name.as_deref())
                .bind(filter.ipaddr.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "login_log.find_page count",
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
                "login_log.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Soft delete a single login log entry.
    #[instrument(skip_all, fields(info_id = %info_id))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        info_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let _update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_logininfor SET del_flag = '1' \
             WHERE info_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(info_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("login_log.soft_delete")?
        .rows_affected();
        Ok(rows)
    }

    /// Soft delete all login log entries for the current tenant.
    #[instrument(skip_all)]
    pub async fn soft_delete_all(executor: impl sqlx::PgExecutor<'_>) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let _update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_logininfor SET del_flag = '1' \
             WHERE del_flag = '0' \
               AND ($1::varchar IS NULL OR tenant_id = $1)",
        )
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("login_log.soft_delete_all")?
        .rows_affected();
        Ok(rows)
    }
}

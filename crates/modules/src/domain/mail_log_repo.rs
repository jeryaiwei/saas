//! MailLogRepo — hand-written SQL for sys_mail_log.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. READ-ONLY — no insert/update/delete from API.
//! 4. NOT tenant-scoped — no current_tenant_scope.

use super::entities::SysMailLog;
use anyhow::Context;
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    id, user_id, user_type, to_mail, account_id, from_mail, \
    template_id, template_code, template_nickname, template_title, \
    template_content, template_params, send_status, send_time, error_msg";

const PAGE_WHERE: &str = "\
    WHERE ($1::varchar IS NULL OR to_mail LIKE '%' || $1 || '%') \
      AND ($2::varchar IS NULL OR template_code = $2) \
      AND ($3::int IS NULL OR send_status = $3)";

#[derive(Debug)]
pub struct MailLogListFilter {
    pub to_mail: Option<String>,
    pub template_code: Option<String>,
    pub send_status: Option<i32>,
    pub page: PageQuery,
}

pub struct MailLogRepo;

impl MailLogRepo {
    #[instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: i64,
    ) -> anyhow::Result<Option<SysMailLog>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_mail_log \
             WHERE id = $1 \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysMailLog>(&sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .context("mail_log.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_to_mail = filter.to_mail.is_some(),
        has_template_code = filter.template_code.is_some(),
        has_send_status = filter.send_status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: MailLogListFilter,
    ) -> anyhow::Result<framework::response::Page<SysMailLog>> {
        let mut conn = conn
            .acquire()
            .await
            .context("mail_log.find_page: acquire")?;
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_mail_log {PAGE_WHERE} \
             ORDER BY send_time DESC \
             LIMIT $4 OFFSET $5"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysMailLog>(&rows_sql)
                .bind(filter.to_mail.as_deref())
                .bind(filter.template_code.as_deref())
                .bind(filter.send_status)
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "mail_log.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_mail_log {PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(filter.to_mail.as_deref())
                .bind(filter.template_code.as_deref())
                .bind(filter.send_status)
                .fetch_one(&mut *conn),
            "mail_log.find_page count",
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
                "mail_log.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }
}

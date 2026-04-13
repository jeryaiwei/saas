//! MailLogRepo — hand-written SQL for sys_mail_log.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. Has insert/update_status for the mail send service.
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
pub struct MailLogInsertParams {
    pub user_id: Option<String>,
    pub user_type: Option<i32>,
    pub to_mail: String,
    pub account_id: i32,
    pub from_mail: String,
    pub template_id: i32,
    pub template_code: String,
    pub template_nickname: String,
    pub template_title: String,
    pub template_content: String,
    pub template_params: Option<String>,
}

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

    #[instrument(skip_all, fields(to_mail = %params.to_mail, template_code = %params.template_code))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: MailLogInsertParams,
    ) -> anyhow::Result<SysMailLog> {
        let sql = format!(
            "INSERT INTO sys_mail_log (\
                user_id, user_type, to_mail, account_id, from_mail, \
                template_id, template_code, template_nickname, template_title, \
                template_content, template_params, send_status, send_time\
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, 0, CURRENT_TIMESTAMP) \
            RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysMailLog>(&sql)
            .bind(params.user_id.as_deref())
            .bind(params.user_type)
            .bind(&params.to_mail)
            .bind(params.account_id)
            .bind(&params.from_mail)
            .bind(params.template_id)
            .bind(&params.template_code)
            .bind(&params.template_nickname)
            .bind(&params.template_title)
            .bind(&params.template_content)
            .bind(params.template_params.as_deref())
            .fetch_one(executor)
            .await
            .context("mail_log.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(id = %id, send_status = %send_status))]
    pub async fn update_status(
        executor: impl sqlx::PgExecutor<'_>,
        id: i64,
        send_status: i32,
        error_msg: Option<&str>,
    ) -> anyhow::Result<u64> {
        let sql = "\
            UPDATE sys_mail_log \
            SET send_status = $2, \
                send_time = CASE WHEN $2 = 1 THEN CURRENT_TIMESTAMP ELSE send_time END, \
                error_msg = $3 \
            WHERE id = $1";
        let affected = sqlx::query(sql)
            .bind(id)
            .bind(send_status)
            .bind(error_msg)
            .execute(executor)
            .await
            .context("mail_log.update_status")?
            .rows_affected();
        Ok(affected)
    }
}

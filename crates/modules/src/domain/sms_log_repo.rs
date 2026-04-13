//! SmsLogRepo — hand-written SQL for sys_sms_log.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. Has insert/update_status for the SMS send service.
//! 4. NOT tenant-scoped — no current_tenant_scope.

use super::entities::SysSmsLog;
use anyhow::Context;
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    id, channel_id, channel_code, template_id, template_code, \
    mobile, content, params, send_status, send_time, \
    receive_status, receive_time, api_send_code, api_receive_code, error_msg";

const LOG_PAGE_WHERE: &str = "\
    WHERE ($1::varchar IS NULL OR mobile LIKE '%' || $1 || '%') \
      AND ($2::varchar IS NULL OR template_code = $2) \
      AND ($3::int IS NULL OR send_status = $3)";

#[derive(Debug)]
pub struct SmsLogInsertParams {
    pub channel_id: i32,
    pub channel_code: String,
    pub template_id: i32,
    pub template_code: String,
    pub mobile: String,
    pub content: String,
    pub params: Option<String>,
}

#[derive(Debug)]
pub struct SmsLogListFilter {
    pub mobile: Option<String>,
    pub template_code: Option<String>,
    pub send_status: Option<i32>,
    pub page: PageQuery,
}

pub struct SmsLogRepo;

impl SmsLogRepo {
    #[instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: i64,
    ) -> anyhow::Result<Option<SysSmsLog>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_sms_log \
             WHERE id = $1 \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysSmsLog>(&sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .context("sms_log.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_mobile = filter.mobile.is_some(),
        has_template_code = filter.template_code.is_some(),
        has_send_status = filter.send_status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: SmsLogListFilter,
    ) -> anyhow::Result<framework::response::Page<SysSmsLog>> {
        let mut conn = conn.acquire().await.context("sms_log.find_page: acquire")?;
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_sms_log {LOG_PAGE_WHERE} \
             ORDER BY send_time DESC \
             LIMIT $4 OFFSET $5"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysSmsLog>(&rows_sql)
                .bind(filter.mobile.as_deref())
                .bind(filter.template_code.as_deref())
                .bind(filter.send_status)
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "sms_log.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_sms_log {LOG_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(filter.mobile.as_deref())
                .bind(filter.template_code.as_deref())
                .bind(filter.send_status)
                .fetch_one(&mut *conn),
            "sms_log.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(rows_ms, count_ms, total_ms, "sms_log.find_page: slow query");
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    #[instrument(skip_all, fields(mobile = %params.mobile, template_code = %params.template_code))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: SmsLogInsertParams,
    ) -> anyhow::Result<SysSmsLog> {
        let sql = format!(
            "INSERT INTO sys_sms_log (\
                channel_id, channel_code, template_id, template_code, \
                mobile, content, params, send_status, send_time\
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, 0, CURRENT_TIMESTAMP) \
            RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysSmsLog>(&sql)
            .bind(params.channel_id)
            .bind(&params.channel_code)
            .bind(params.template_id)
            .bind(&params.template_code)
            .bind(&params.mobile)
            .bind(&params.content)
            .bind(params.params.as_deref())
            .fetch_one(executor)
            .await
            .context("sms_log.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(id = %id, send_status = %send_status))]
    pub async fn update_status(
        executor: impl sqlx::PgExecutor<'_>,
        id: i64,
        send_status: i32,
        api_send_code: Option<&str>,
        error_msg: Option<&str>,
    ) -> anyhow::Result<u64> {
        let sql = "\
            UPDATE sys_sms_log \
            SET send_status = $2, \
                api_send_code = $3, \
                error_msg = $4 \
            WHERE id = $1";
        let affected = sqlx::query(sql)
            .bind(id)
            .bind(send_status)
            .bind(api_send_code)
            .bind(error_msg)
            .execute(executor)
            .await
            .context("sms_log.update_status")?
            .rows_affected();
        Ok(affected)
    }
}

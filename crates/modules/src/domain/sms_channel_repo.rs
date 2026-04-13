//! SmsChannelRepo — hand-written SQL for sys_sms_channel.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on sys_sms_channel are single-owned here.
//! 4. NOT tenant-scoped — no current_tenant_scope.

use super::entities::SysSmsChannel;
use anyhow::Context;
use framework::context::{audit_update_by, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    id, code, name, signature, api_key, api_secret, callback_url, \
    status, remark, create_by, create_at, update_by, update_at, del_flag";

const CHANNEL_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR name LIKE '%' || $1 || '%') \
      AND ($2::varchar IS NULL OR code LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR status = $3)";

#[derive(Debug)]
pub struct SmsChannelListFilter {
    pub name: Option<String>,
    pub code: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct SmsChannelInsertParams {
    pub code: String,
    pub name: String,
    pub signature: String,
    pub api_key: String,
    pub api_secret: String,
    pub callback_url: Option<String>,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct SmsChannelUpdateParams {
    pub id: i32,
    pub code: Option<String>,
    pub name: Option<String>,
    pub signature: Option<String>,
    pub api_key: Option<String>,
    pub api_secret: Option<String>,
    pub callback_url: Option<Option<String>>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct SmsChannelRepo;

impl SmsChannelRepo {
    #[instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: i32,
    ) -> anyhow::Result<Option<SysSmsChannel>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_sms_channel \
             WHERE id = $1 AND del_flag = '0' \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysSmsChannel>(&sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .context("sms_channel.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_name = filter.name.is_some(),
        has_code = filter.code.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: SmsChannelListFilter,
    ) -> anyhow::Result<framework::response::Page<SysSmsChannel>> {
        let mut conn = conn
            .acquire()
            .await
            .context("sms_channel.find_page: acquire")?;
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_sms_channel {CHANNEL_PAGE_WHERE} \
             ORDER BY id DESC \
             LIMIT $4 OFFSET $5"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysSmsChannel>(&rows_sql)
                .bind(filter.name.as_deref())
                .bind(filter.code.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "sms_channel.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_sms_channel {CHANNEL_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(filter.name.as_deref())
                .bind(filter.code.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "sms_channel.find_page count",
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
                "sms_channel.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Active channels for dropdown — capped at 500.
    #[instrument(skip_all)]
    pub async fn find_enabled_list(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<Vec<SysSmsChannel>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_sms_channel \
             WHERE del_flag = '0' AND status = '0' \
             ORDER BY id DESC \
             LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysSmsChannel>(&sql)
            .fetch_all(executor)
            .await
            .context("sms_channel.find_enabled_list")?;
        Ok(rows)
    }

    /// Check if a code already exists (excluding a given id for update scenarios).
    #[instrument(skip_all, fields(code = %code))]
    pub async fn exists_by_code(
        executor: impl sqlx::PgExecutor<'_>,
        code: &str,
        exclude_id: Option<i32>,
    ) -> anyhow::Result<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                SELECT 1 FROM sys_sms_channel \
                WHERE code = $1 AND del_flag = '0' \
                  AND ($2::int IS NULL OR id <> $2)\
            )",
        )
        .bind(code)
        .bind(exclude_id)
        .fetch_one(executor)
        .await
        .context("sms_channel.exists_by_code")?;
        Ok(exists)
    }

    #[instrument(skip_all, fields(name = %params.name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: SmsChannelInsertParams,
    ) -> anyhow::Result<SysSmsChannel> {
        let audit = AuditInsert::now();
        let sql = format!(
            "INSERT INTO sys_sms_channel (\
                code, name, signature, api_key, api_secret, callback_url, \
                status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                $1, $2, $3, $4, $5, $6, $7, '0', $8, $9, \
                CURRENT_TIMESTAMP, $10\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysSmsChannel>(&sql)
            .bind(&params.code)
            .bind(&params.name)
            .bind(&params.signature)
            .bind(&params.api_key)
            .bind(&params.api_secret)
            .bind(params.callback_url.as_deref())
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("sms_channel.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(id = %params.id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: SmsChannelUpdateParams,
    ) -> anyhow::Result<u64> {
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_sms_channel SET \
                code         = COALESCE($2, code), \
                name         = COALESCE($3, name), \
                signature    = COALESCE($4, signature), \
                api_key      = COALESCE($5, api_key), \
                api_secret   = COALESCE($6, api_secret), \
                callback_url = CASE WHEN $7::boolean THEN $8 ELSE callback_url END, \
                status       = COALESCE($9, status), \
                remark       = CASE WHEN $10::boolean THEN $11 ELSE remark END, \
                update_by    = $12, \
                update_at    = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0'",
        )
        .bind(params.id)
        .bind(params.code.as_deref())
        .bind(params.name.as_deref())
        .bind(params.signature.as_deref())
        .bind(params.api_key.as_deref())
        .bind(params.api_secret.as_deref())
        // callback_url — nullable update via flag pattern
        .bind(params.callback_url.is_some())
        .bind(params.callback_url.as_ref().and_then(|o| o.as_deref()))
        .bind(params.status.as_deref())
        // remark — nullable update via flag pattern
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("sms_channel.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(id = %id))]
    pub async fn soft_delete(executor: impl sqlx::PgExecutor<'_>, id: i32) -> anyhow::Result<u64> {
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_sms_channel SET del_flag = '1', update_by = $2, update_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0'",
        )
        .bind(id)
        .bind(&update_by)
        .execute(executor)
        .await
        .context("sms_channel.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}

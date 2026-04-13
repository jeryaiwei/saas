//! SmsTemplateRepo — hand-written SQL for sys_sms_template.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on sys_sms_template are single-owned here.
//! 4. NOT tenant-scoped — no current_tenant_scope.

use super::entities::SysSmsTemplate;
use anyhow::Context;
use framework::context::{audit_update_by, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    id, channel_id, code, name, content, params, api_template_id, \
    type, status, remark, create_by, create_at, update_by, update_at, del_flag";

const TEMPLATE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR name LIKE '%' || $1 || '%') \
      AND ($2::varchar IS NULL OR code LIKE '%' || $2 || '%') \
      AND ($3::int IS NULL OR channel_id = $3) \
      AND ($4::int IS NULL OR type = $4) \
      AND ($5::varchar IS NULL OR status = $5)";

#[derive(Debug)]
pub struct SmsTemplateListFilter {
    pub name: Option<String>,
    pub code: Option<String>,
    pub channel_id: Option<i32>,
    pub r_type: Option<i32>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct SmsTemplateInsertParams {
    pub channel_id: i32,
    pub code: String,
    pub name: String,
    pub content: String,
    pub params: Option<String>,
    pub api_template_id: String,
    pub r_type: i32,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct SmsTemplateUpdateParams {
    pub id: i32,
    pub channel_id: Option<i32>,
    pub code: Option<String>,
    pub name: Option<String>,
    pub content: Option<String>,
    pub params: Option<Option<String>>,
    pub api_template_id: Option<String>,
    pub r_type: Option<i32>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct SmsTemplateRepo;

impl SmsTemplateRepo {
    /// Find an enabled template by its unique code. Used by sms-send service.
    #[instrument(skip_all, fields(code = %code))]
    pub async fn find_enabled_by_code(
        executor: impl sqlx::PgExecutor<'_>,
        code: &str,
    ) -> anyhow::Result<Option<SysSmsTemplate>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_sms_template \
             WHERE code = $1 AND status = '0' AND del_flag = '0' \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysSmsTemplate>(&sql)
            .bind(code)
            .fetch_optional(executor)
            .await
            .context("sms_template.find_enabled_by_code")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: i32,
    ) -> anyhow::Result<Option<SysSmsTemplate>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_sms_template \
             WHERE id = $1 AND del_flag = '0' \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysSmsTemplate>(&sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .context("sms_template.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_name = filter.name.is_some(),
        has_code = filter.code.is_some(),
        has_channel_id = filter.channel_id.is_some(),
        has_type = filter.r_type.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: SmsTemplateListFilter,
    ) -> anyhow::Result<framework::response::Page<SysSmsTemplate>> {
        let mut conn = conn
            .acquire()
            .await
            .context("sms_template.find_page: acquire")?;
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_sms_template {TEMPLATE_PAGE_WHERE} \
             ORDER BY id DESC \
             LIMIT $6 OFFSET $7"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysSmsTemplate>(&rows_sql)
                .bind(filter.name.as_deref())
                .bind(filter.code.as_deref())
                .bind(filter.channel_id)
                .bind(filter.r_type)
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "sms_template.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_sms_template {TEMPLATE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(filter.name.as_deref())
                .bind(filter.code.as_deref())
                .bind(filter.channel_id)
                .bind(filter.r_type)
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "sms_template.find_page count",
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
                "sms_template.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
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
                SELECT 1 FROM sys_sms_template \
                WHERE code = $1 AND del_flag = '0' \
                  AND ($2::int IS NULL OR id <> $2)\
            )",
        )
        .bind(code)
        .bind(exclude_id)
        .fetch_one(executor)
        .await
        .context("sms_template.exists_by_code")?;
        Ok(exists)
    }

    #[instrument(skip_all, fields(name = %params.name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: SmsTemplateInsertParams,
    ) -> anyhow::Result<SysSmsTemplate> {
        let audit = AuditInsert::now();
        let sql = format!(
            "INSERT INTO sys_sms_template (\
                channel_id, code, name, content, params, api_template_id, \
                type, status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                $1, $2, $3, $4, $5, $6, $7, $8, '0', $9, $10, \
                CURRENT_TIMESTAMP, $11\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysSmsTemplate>(&sql)
            .bind(params.channel_id)
            .bind(&params.code)
            .bind(&params.name)
            .bind(&params.content)
            .bind(params.params.as_deref())
            .bind(&params.api_template_id)
            .bind(params.r_type)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("sms_template.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(id = %params.id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: SmsTemplateUpdateParams,
    ) -> anyhow::Result<u64> {
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_sms_template SET \
                channel_id      = COALESCE($2, channel_id), \
                code            = COALESCE($3, code), \
                name            = COALESCE($4, name), \
                content         = COALESCE($5, content), \
                params          = CASE WHEN $6::boolean THEN $7 ELSE params END, \
                api_template_id = COALESCE($8, api_template_id), \
                type            = COALESCE($9, type), \
                status          = COALESCE($10, status), \
                remark          = CASE WHEN $11::boolean THEN $12 ELSE remark END, \
                update_by       = $13, \
                update_at       = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0'",
        )
        .bind(params.id)
        .bind(params.channel_id)
        .bind(params.code.as_deref())
        .bind(params.name.as_deref())
        .bind(params.content.as_deref())
        // params — nullable update via flag pattern
        .bind(params.params.is_some())
        .bind(params.params.as_ref().and_then(|o| o.as_deref()))
        .bind(params.api_template_id.as_deref())
        .bind(params.r_type)
        .bind(params.status.as_deref())
        // remark — nullable update via flag pattern
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("sms_template.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(id = %id))]
    pub async fn soft_delete(executor: impl sqlx::PgExecutor<'_>, id: i32) -> anyhow::Result<u64> {
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_sms_template SET del_flag = '1', update_by = $2, update_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0'",
        )
        .bind(id)
        .bind(&update_by)
        .execute(executor)
        .await
        .context("sms_template.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}

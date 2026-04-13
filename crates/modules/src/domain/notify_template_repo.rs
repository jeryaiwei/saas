//! NotifyTemplateRepo — hand-written SQL for sys_notify_template.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on sys_notify_template are single-owned here.
//! 4. NOT tenant-scoped — no current_tenant_scope.

use super::entities::SysNotifyTemplate;
use anyhow::Context;
use framework::context::{audit_update_by, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    id, name, code, nickname, content, params, type, status, \
    remark, create_by, create_at, update_by, update_at, del_flag, i18n";

const TEMPLATE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR name LIKE '%' || $1 || '%') \
      AND ($2::int IS NULL OR type = $2) \
      AND ($3::varchar IS NULL OR status = $3)";

#[derive(Debug)]
pub struct NotifyTemplateListFilter {
    pub name: Option<String>,
    pub r_type: Option<i32>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct NotifyTemplateInsertParams {
    pub name: String,
    pub code: String,
    pub nickname: String,
    pub content: String,
    pub params: Option<String>,
    pub r_type: i32,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct NotifyTemplateUpdateParams {
    pub id: i32,
    pub name: Option<String>,
    pub code: Option<String>,
    pub nickname: Option<String>,
    pub content: Option<String>,
    pub params: Option<Option<String>>,
    pub r_type: Option<i32>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct NotifyTemplateRepo;

impl NotifyTemplateRepo {
    #[instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: i32,
    ) -> anyhow::Result<Option<SysNotifyTemplate>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_notify_template \
             WHERE id = $1 AND del_flag = '0' \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysNotifyTemplate>(&sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .context("notify_template.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_name = filter.name.is_some(),
        has_type = filter.r_type.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: NotifyTemplateListFilter,
    ) -> anyhow::Result<framework::response::Page<SysNotifyTemplate>> {
        let mut conn = conn
            .acquire()
            .await
            .context("notify_template.find_page: acquire")?;
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_notify_template {TEMPLATE_PAGE_WHERE} \
             ORDER BY id DESC \
             LIMIT $4 OFFSET $5"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysNotifyTemplate>(&rows_sql)
                .bind(filter.name.as_deref())
                .bind(filter.r_type)
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "notify_template.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_notify_template {TEMPLATE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(filter.name.as_deref())
                .bind(filter.r_type)
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "notify_template.find_page count",
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
                "notify_template.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Active templates for dropdown — capped at 500.
    #[instrument(skip_all)]
    pub async fn find_option_list(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<Vec<SysNotifyTemplate>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_notify_template \
             WHERE del_flag = '0' AND status = '0' \
             ORDER BY id DESC \
             LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysNotifyTemplate>(&sql)
            .fetch_all(executor)
            .await
            .context("notify_template.find_option_list")?;
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
                SELECT 1 FROM sys_notify_template \
                WHERE code = $1 AND del_flag = '0' \
                  AND ($2::int IS NULL OR id <> $2)\
            )",
        )
        .bind(code)
        .bind(exclude_id)
        .fetch_one(executor)
        .await
        .context("notify_template.exists_by_code")?;
        Ok(exists)
    }

    #[instrument(skip_all, fields(name = %params.name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: NotifyTemplateInsertParams,
    ) -> anyhow::Result<SysNotifyTemplate> {
        let audit = AuditInsert::now();
        let sql = format!(
            "INSERT INTO sys_notify_template (\
                name, code, nickname, content, params, type, \
                status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                $1, $2, $3, $4, $5, $6, $7, '0', $8, $9, \
                CURRENT_TIMESTAMP, $10\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysNotifyTemplate>(&sql)
            .bind(&params.name)
            .bind(&params.code)
            .bind(&params.nickname)
            .bind(&params.content)
            .bind(params.params.as_deref())
            .bind(params.r_type)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("notify_template.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(id = %params.id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: NotifyTemplateUpdateParams,
    ) -> anyhow::Result<u64> {
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_notify_template SET \
                name     = COALESCE($2, name), \
                code     = COALESCE($3, code), \
                nickname = COALESCE($4, nickname), \
                content  = COALESCE($5, content), \
                params   = CASE WHEN $6::boolean THEN $7 ELSE params END, \
                type     = COALESCE($8, type), \
                status   = COALESCE($9, status), \
                remark   = CASE WHEN $10::boolean THEN $11 ELSE remark END, \
                update_by = $12, \
                update_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0'",
        )
        .bind(params.id)
        .bind(params.name.as_deref())
        .bind(params.code.as_deref())
        .bind(params.nickname.as_deref())
        .bind(params.content.as_deref())
        // params — nullable update via flag pattern
        .bind(params.params.is_some())
        .bind(params.params.as_ref().and_then(|o| o.as_deref()))
        .bind(params.r_type)
        .bind(params.status.as_deref())
        // remark — nullable update via flag pattern
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("notify_template.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(id = %id))]
    pub async fn soft_delete(executor: impl sqlx::PgExecutor<'_>, id: i32) -> anyhow::Result<u64> {
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_notify_template SET del_flag = '1', update_by = $2, update_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0'",
        )
        .bind(id)
        .bind(&update_by)
        .execute(executor)
        .await
        .context("notify_template.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}

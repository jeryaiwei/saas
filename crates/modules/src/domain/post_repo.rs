//! PostRepo — hand-written SQL for sys_post.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on sys_post are single-owned here.
//! 4. STRICT tenant model — filtered by tenant_id.

use super::entities::SysPost;
use anyhow::Context;
use framework::context::{audit_update_by, current_tenant_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use sqlx::PgPool;
use tracing::instrument;

const COLUMNS: &str = "\
    post_id, tenant_id, dept_id, post_code, post_category, post_name, \
    post_sort, status, create_by, create_at, update_by, update_at, \
    remark, del_flag, i18n";

const POST_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR post_name LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR post_code LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR status = $4)";

#[derive(Debug)]
pub struct PostListFilter {
    pub post_name: Option<String>,
    pub post_code: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct PostInsertParams {
    pub dept_id: Option<String>,
    pub post_code: String,
    pub post_category: Option<String>,
    pub post_name: String,
    pub post_sort: i32,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct PostUpdateParams {
    pub post_id: String,
    pub dept_id: Option<Option<String>>,
    pub post_code: Option<String>,
    pub post_category: Option<Option<String>>,
    pub post_name: Option<String>,
    pub post_sort: Option<i32>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct PostRepo;

impl PostRepo {
    #[instrument(skip_all, fields(post_id = %post_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        post_id: &str,
    ) -> anyhow::Result<Option<SysPost>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_post \
             WHERE post_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysPost>(&sql)
            .bind(post_id)
            .bind(tenant.as_deref())
            .fetch_optional(executor)
            .await
            .context("post.find_by_id")?;
        Ok(row)
    }

    /// Check if a post_code already exists within the tenant (excluding a
    /// given post_id for update scenarios).
    #[instrument(skip_all, fields(post_code = %post_code))]
    pub async fn exists_by_code(
        executor: impl sqlx::PgExecutor<'_>,
        post_code: &str,
        exclude_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let tenant = current_tenant_scope();
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                SELECT 1 FROM sys_post \
                WHERE post_code = $1 AND del_flag = '0' \
                  AND ($2::varchar IS NULL OR tenant_id = $2) \
                  AND ($3::varchar IS NULL OR post_id <> $3)\
            )",
        )
        .bind(post_code)
        .bind(tenant.as_deref())
        .bind(exclude_id)
        .fetch_one(executor)
        .await
        .context("post.exists_by_code")?;
        Ok(exists)
    }

    /// Check if a post_name already exists within the tenant.
    #[instrument(skip_all, fields(post_name = %post_name))]
    pub async fn exists_by_name(
        executor: impl sqlx::PgExecutor<'_>,
        post_name: &str,
        exclude_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let tenant = current_tenant_scope();
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                SELECT 1 FROM sys_post \
                WHERE post_name = $1 AND del_flag = '0' \
                  AND ($2::varchar IS NULL OR tenant_id = $2) \
                  AND ($3::varchar IS NULL OR post_id <> $3)\
            )",
        )
        .bind(post_name)
        .bind(tenant.as_deref())
        .bind(exclude_id)
        .fetch_one(executor)
        .await
        .context("post.exists_by_name")?;
        Ok(exists)
    }

    #[instrument(skip_all, fields(
        has_name = filter.post_name.is_some(),
        has_code = filter.post_code.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        pool: &PgPool,
        filter: PostListFilter,
    ) -> anyhow::Result<framework::response::Page<SysPost>> {
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_post {POST_PAGE_WHERE} \
             ORDER BY post_sort ASC, create_at DESC \
             LIMIT $5 OFFSET $6"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysPost>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.post_name.as_deref())
                .bind(filter.post_code.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "post.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_post {POST_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.post_name.as_deref())
                .bind(filter.post_code.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(pool),
            "post.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(rows_ms, count_ms, total_ms, "post.find_page: slow query");
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Active posts for dropdown — tenant-scoped, capped at 500.
    #[instrument(skip_all)]
    pub async fn find_option_list(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<Vec<SysPost>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_post \
             WHERE del_flag = '0' AND status = '0' \
               AND ($1::varchar IS NULL OR tenant_id = $1) \
             ORDER BY post_sort ASC \
             LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysPost>(&sql)
            .bind(tenant.as_deref())
            .fetch_all(executor)
            .await
            .context("post.find_option_list")?;
        Ok(rows)
    }

    #[instrument(skip_all, fields(post_name = %params.post_name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: PostInsertParams,
    ) -> anyhow::Result<SysPost> {
        let audit = AuditInsert::now();
        let tenant = current_tenant_scope().context("post.insert: tenant_id required")?;
        let sql = format!(
            "INSERT INTO sys_post (\
                post_id, tenant_id, dept_id, post_code, post_category, post_name, \
                post_sort, status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, '0', $8, $9, \
                CURRENT_TIMESTAMP, $10\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysPost>(&sql)
            .bind(&tenant)
            .bind(params.dept_id.as_deref())
            .bind(&params.post_code)
            .bind(params.post_category.as_deref())
            .bind(&params.post_name)
            .bind(params.post_sort)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("post.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(post_id = %params.post_id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: PostUpdateParams,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_post SET \
                dept_id      = COALESCE($3, dept_id), \
                post_code    = COALESCE($4, post_code), \
                post_category = CASE WHEN $5::boolean THEN $6 ELSE post_category END, \
                post_name    = COALESCE($7, post_name), \
                post_sort    = COALESCE($8, post_sort), \
                status       = COALESCE($9, status), \
                remark       = CASE WHEN $10::boolean THEN $11 ELSE remark END, \
                update_by    = $12, \
                update_at    = CURRENT_TIMESTAMP \
             WHERE post_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(&params.post_id)
        .bind(tenant.as_deref())
        // dept_id — simple COALESCE (flatten Option<Option<>> for bind)
        .bind(params.dept_id.as_ref().map(|o| o.as_deref()))
        .bind(params.post_code.as_deref())
        // post_category — nullable update via flag pattern
        .bind(params.post_category.is_some())
        .bind(params.post_category.as_ref().and_then(|o| o.as_deref()))
        .bind(params.post_name.as_deref())
        .bind(params.post_sort)
        .bind(params.status.as_deref())
        // remark — nullable update via flag pattern
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("post.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(post_id = %post_id))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        post_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_post SET del_flag = '1', update_by = $3, update_at = CURRENT_TIMESTAMP \
             WHERE post_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(post_id)
        .bind(tenant.as_deref())
        .bind(&update_by)
        .execute(executor)
        .await
        .context("post.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}

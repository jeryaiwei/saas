//! FileShareRepo — hand-written SQL for `sys_file_share`.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/DELETE on `sys_file_share` are single-owned here.
//! 4. STRICT tenant model — filtered by tenant_id (except public access).

use super::entities::SysFileShare;
use anyhow::Context;
use framework::context::{current_tenant_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    share_id, tenant_id, upload_id, share_code, expire_time, \
    max_download, download_count, status, create_by, create_at";

const MY_PAGE_WHERE: &str = "\
    WHERE ($1::varchar IS NULL OR tenant_id = $1) \
      AND create_by = $2";

#[derive(Debug)]
pub struct ShareInsertParams {
    pub tenant_id: String,
    pub upload_id: String,
    pub share_code: Option<String>,
    pub expire_time: Option<chrono::DateTime<chrono::Utc>>,
    pub max_download: i32,
}

pub struct FileShareRepo;

impl FileShareRepo {
    /// Find a share by id — NO tenant scope (public access).
    #[instrument(skip_all, fields(share_id = %share_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        share_id: &str,
    ) -> anyhow::Result<Option<SysFileShare>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_file_share \
             WHERE share_id = $1 AND status = '0' \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysFileShare>(&sql)
            .bind(share_id)
            .fetch_optional(executor)
            .await
            .context("file_share.find_by_id")?;
        Ok(row)
    }

    /// Paginated list of shares created by the current user. Tenant-scoped.
    #[instrument(skip_all, fields(
        create_by = %create_by,
        page_num = page.page_num,
        page_size = page.page_size,
    ))]
    pub async fn find_my_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        create_by: &str,
        page: PageQuery,
    ) -> anyhow::Result<framework::response::Page<SysFileShare>> {
        let mut conn = conn
            .acquire()
            .await
            .context("file_share.find_my_page: acquire")?;
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(page.page_num, page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_file_share {MY_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $3 OFFSET $4"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysFileShare>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(create_by)
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "file_share.find_my_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_file_share {MY_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(create_by)
                .fetch_one(&mut *conn),
            "file_share.find_my_page count",
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
                "file_share.find_my_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Insert a new share. Returns the newly-inserted row.
    #[instrument(skip_all, fields(upload_id = %params.upload_id))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: ShareInsertParams,
    ) -> anyhow::Result<SysFileShare> {
        let audit = AuditInsert::now();
        let sql = format!(
            "INSERT INTO sys_file_share (\
                share_id, tenant_id, upload_id, share_code, expire_time, \
                max_download, download_count, status, create_by\
            ) VALUES (\
                gen_random_uuid(), $1, $2, $3, $4, $5, 0, '0', $6\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysFileShare>(&sql)
            .bind(&params.tenant_id)
            .bind(&params.upload_id)
            .bind(params.share_code.as_deref())
            .bind(params.expire_time)
            .bind(params.max_download)
            .bind(&audit.create_by)
            .fetch_one(executor)
            .await
            .context("file_share.insert")?;
        Ok(row)
    }

    /// Delete a share by id. Tenant-scoped.
    #[instrument(skip_all, fields(share_id = %share_id))]
    pub async fn delete_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        share_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let affected = sqlx::query(
            "DELETE FROM sys_file_share \
             WHERE share_id = $1 \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(share_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("file_share.delete_by_id")?
        .rows_affected();
        Ok(affected)
    }

    /// Increment download count for a share. No tenant scope (public access).
    #[instrument(skip_all, fields(share_id = %share_id))]
    pub async fn increment_download_count(
        executor: impl sqlx::PgExecutor<'_>,
        share_id: &str,
    ) -> anyhow::Result<u64> {
        let affected = sqlx::query(
            "UPDATE sys_file_share SET download_count = download_count + 1 \
             WHERE share_id = $1 AND status = '0'",
        )
        .bind(share_id)
        .execute(executor)
        .await
        .context("file_share.increment_download_count")?
        .rows_affected();
        Ok(affected)
    }
}

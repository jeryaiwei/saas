//! UploadRepo — hand-written SQL for `sys_upload`.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on `sys_upload` are single-owned here.
//! 4. STRICT tenant model — filtered by tenant_id.

use super::entities::SysUpload;
use anyhow::Context;
use framework::context::{audit_update_by, current_tenant_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    upload_id, tenant_id, folder_id, size, file_name, new_file_name, url, \
    ext, mime_type, storage_type, file_md5, thumbnail, parent_file_id, \
    version, is_latest, download_count, status, del_flag, create_by, \
    create_at, update_by, update_at, remark";

const FILE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR folder_id = $2) \
      AND ($3::varchar IS NULL OR file_name LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR ext = $4) \
      AND ($5::varchar IS NULL OR status = $5)";

const RECYCLE_PAGE_WHERE: &str = "\
    WHERE del_flag = '1' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR file_name LIKE '%' || $2 || '%')";

#[derive(Debug)]
pub struct UploadListFilter {
    pub folder_id: Option<String>,
    pub file_name: Option<String>,
    pub ext: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct RecycleListFilter {
    pub file_name: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct UploadInsertParams {
    pub upload_id: String,
    pub tenant_id: String,
    pub folder_id: String,
    pub size: i32,
    pub file_name: String,
    pub new_file_name: String,
    pub url: String,
    pub ext: Option<String>,
    pub mime_type: Option<String>,
    pub storage_type: String,
    pub file_md5: Option<String>,
    pub thumbnail: Option<String>,
    pub parent_file_id: Option<String>,
    pub version: i32,
    pub is_latest: bool,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct UploadUpdateParams {
    pub upload_id: String,
    pub file_name: Option<String>,
    pub folder_id: Option<String>,
    pub remark: Option<String>,
}

pub struct UploadRepo;

impl UploadRepo {
    /// Find a single upload by id, tenant-scoped, soft-delete filtered.
    #[instrument(skip_all, fields(upload_id = %upload_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        upload_id: &str,
    ) -> anyhow::Result<Option<SysUpload>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_upload \
             WHERE upload_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysUpload>(&sql)
            .bind(upload_id)
            .bind(tenant.as_deref())
            .fetch_optional(executor)
            .await
            .context("upload.find_by_id")?;
        Ok(row)
    }

    /// Paginated file list with filters.
    #[instrument(skip_all, fields(
        has_folder = filter.folder_id.is_some(),
        has_name = filter.file_name.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: UploadListFilter,
    ) -> anyhow::Result<framework::response::Page<SysUpload>> {
        let mut conn = conn.acquire().await.context("upload.find_page: acquire")?;
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_upload {FILE_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $6 OFFSET $7"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysUpload>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.folder_id.as_deref())
                .bind(filter.file_name.as_deref())
                .bind(filter.ext.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "upload.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_upload {FILE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.folder_id.as_deref())
                .bind(filter.file_name.as_deref())
                .bind(filter.ext.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "upload.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(rows_ms, count_ms, total_ms, "upload.find_page: slow query");
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Paginated recycle bin (del_flag='1').
    #[instrument(skip_all, fields(
        has_name = filter.file_name.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_recycle_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: RecycleListFilter,
    ) -> anyhow::Result<framework::response::Page<SysUpload>> {
        let mut conn = conn
            .acquire()
            .await
            .context("upload.find_recycle_page: acquire")?;
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_upload {RECYCLE_PAGE_WHERE} \
             ORDER BY update_at DESC \
             LIMIT $3 OFFSET $4"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysUpload>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.file_name.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "upload.find_recycle_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_upload {RECYCLE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.file_name.as_deref())
                .fetch_one(&mut *conn),
            "upload.find_recycle_page count",
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
                "upload.find_recycle_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Insert a new upload record. Returns the newly-inserted row.
    #[instrument(skip_all, fields(file_name = %params.file_name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: UploadInsertParams,
    ) -> anyhow::Result<SysUpload> {
        let audit = AuditInsert::now();
        let sql = format!(
            "INSERT INTO sys_upload (\
                upload_id, tenant_id, folder_id, size, file_name, new_file_name, url, \
                ext, mime_type, storage_type, file_md5, thumbnail, parent_file_id, \
                version, is_latest, download_count, status, del_flag, \
                create_by, update_by, update_at, remark\
            ) VALUES (\
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, \
                $14, $15, 0, '0', '0', $16, $17, CURRENT_TIMESTAMP, $18\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysUpload>(&sql)
            .bind(&params.upload_id)
            .bind(&params.tenant_id)
            .bind(&params.folder_id)
            .bind(params.size)
            .bind(&params.file_name)
            .bind(&params.new_file_name)
            .bind(&params.url)
            .bind(params.ext.as_deref())
            .bind(params.mime_type.as_deref())
            .bind(&params.storage_type)
            .bind(params.file_md5.as_deref())
            .bind(params.thumbnail.as_deref())
            .bind(params.parent_file_id.as_deref())
            .bind(params.version)
            .bind(params.is_latest)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("upload.insert")?;
        Ok(row)
    }

    /// Update upload fields with COALESCE for optional fields.
    #[instrument(skip_all, fields(upload_id = %params.upload_id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: UploadUpdateParams,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_upload SET \
                file_name = COALESCE($1, file_name), \
                folder_id = COALESCE($2, folder_id), \
                remark    = COALESCE($3, remark), \
                update_by = $4, \
                update_at = CURRENT_TIMESTAMP \
             WHERE upload_id = $5 AND del_flag = '0' \
               AND ($6::varchar IS NULL OR tenant_id = $6)",
        )
        .bind(params.file_name.as_deref())
        .bind(params.folder_id.as_deref())
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.upload_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("upload.update_by_id")?
        .rows_affected();
        Ok(affected)
    }

    /// Soft-delete uploads (del_flag = '1'). Tenant-scoped.
    #[instrument(skip_all)]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        ids: &[String],
    ) -> anyhow::Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_upload SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
             WHERE upload_id = ANY($2) AND del_flag = '0' \
               AND ($3::varchar IS NULL OR tenant_id = $3)",
        )
        .bind(&updater)
        .bind(ids)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("upload.soft_delete")?
        .rows_affected();
        Ok(affected)
    }

    /// Restore uploads from recycle bin (del_flag = '0'). Tenant-scoped.
    #[instrument(skip_all)]
    pub async fn restore(
        executor: impl sqlx::PgExecutor<'_>,
        ids: &[String],
    ) -> anyhow::Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_upload SET del_flag = '0', update_by = $1, update_at = CURRENT_TIMESTAMP \
             WHERE upload_id = ANY($2) AND del_flag = '1' \
               AND ($3::varchar IS NULL OR tenant_id = $3)",
        )
        .bind(&updater)
        .bind(ids)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("upload.restore")?
        .rows_affected();
        Ok(affected)
    }

    /// Hard-delete uploads from recycle bin. Tenant-scoped.
    #[instrument(skip_all)]
    pub async fn hard_delete(
        executor: impl sqlx::PgExecutor<'_>,
        tenant_id: &str,
    ) -> anyhow::Result<u64> {
        let affected =
            sqlx::query("DELETE FROM sys_upload WHERE del_flag = '1' AND tenant_id = $1")
                .bind(tenant_id)
                .execute(executor)
                .await
                .context("upload.hard_delete")?
                .rows_affected();
        Ok(affected)
    }

    /// Find an active file by MD5 hash within the same tenant (for instant upload).
    #[instrument(skip_all, fields(tenant_id = %tenant_id, file_md5 = %file_md5))]
    pub async fn find_by_md5(
        executor: impl sqlx::PgExecutor<'_>,
        tenant_id: &str,
        file_md5: &str,
    ) -> anyhow::Result<Option<SysUpload>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_upload \
             WHERE tenant_id = $1 AND file_md5 = $2 AND del_flag = '0' \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysUpload>(&sql)
            .bind(tenant_id)
            .bind(file_md5)
            .fetch_optional(executor)
            .await
            .context("upload.find_by_md5")?;
        Ok(row)
    }

    /// Increment download count by 1.
    #[instrument(skip_all, fields(upload_id = %upload_id))]
    pub async fn increment_download_count(
        executor: impl sqlx::PgExecutor<'_>,
        upload_id: &str,
    ) -> anyhow::Result<u64> {
        let affected = sqlx::query(
            "UPDATE sys_upload SET download_count = download_count + 1 \
             WHERE upload_id = $1 AND del_flag = '0'",
        )
        .bind(upload_id)
        .execute(executor)
        .await
        .context("upload.increment_download_count")?
        .rows_affected();
        Ok(affected)
    }

    /// Find all versions of a file by parent_file_id. Tenant-scoped.
    #[instrument(skip_all, fields(parent_file_id = %parent_file_id))]
    pub async fn find_versions(
        executor: impl sqlx::PgExecutor<'_>,
        parent_file_id: &str,
    ) -> anyhow::Result<Vec<SysUpload>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_upload \
             WHERE del_flag = '0' \
               AND ($1::varchar IS NULL OR tenant_id = $1) \
               AND (parent_file_id = $2 OR upload_id = $2) \
             ORDER BY version DESC"
        );
        let rows = sqlx::query_as::<_, SysUpload>(&sql)
            .bind(tenant.as_deref())
            .bind(parent_file_id)
            .fetch_all(executor)
            .await
            .context("upload.find_versions")?;
        Ok(rows)
    }

    /// Move uploads to a different folder. Tenant-scoped.
    #[instrument(skip_all, fields(target_folder_id = %target_folder_id))]
    pub async fn move_to_folder(
        executor: impl sqlx::PgExecutor<'_>,
        ids: &[String],
        target_folder_id: &str,
    ) -> anyhow::Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_upload SET folder_id = $1, update_by = $2, update_at = CURRENT_TIMESTAMP \
             WHERE upload_id = ANY($3) AND del_flag = '0' \
               AND ($4::varchar IS NULL OR tenant_id = $4)",
        )
        .bind(target_folder_id)
        .bind(&updater)
        .bind(ids)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("upload.move_to_folder")?
        .rows_affected();
        Ok(affected)
    }
}

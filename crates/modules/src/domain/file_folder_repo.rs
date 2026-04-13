//! FileFolderRepo — hand-written SQL for `sys_file_folder`.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on `sys_file_folder` are single-owned here.
//! 4. STRICT tenant model — filtered by tenant_id.

use super::entities::SysFileFolder;
use anyhow::Context;
use framework::context::{audit_update_by, current_tenant_scope, AuditInsert};
use tracing::instrument;

const COLUMNS: &str = "\
    folder_id, tenant_id, parent_id, folder_name, folder_path, order_num, \
    status, del_flag, create_by, create_at, update_by, update_at, remark";

#[derive(Debug)]
pub struct FolderInsertParams {
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub folder_name: String,
    pub folder_path: String,
    pub order_num: i32,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct FolderUpdateParams {
    pub folder_id: String,
    pub folder_name: Option<String>,
    pub order_num: Option<i32>,
    pub remark: Option<String>,
}

pub struct FileFolderRepo;

impl FileFolderRepo {
    /// Find a single folder by id, tenant-scoped, soft-delete filtered.
    #[instrument(skip_all, fields(folder_id = %folder_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        folder_id: &str,
    ) -> anyhow::Result<Option<SysFileFolder>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_file_folder \
             WHERE folder_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysFileFolder>(&sql)
            .bind(folder_id)
            .bind(tenant.as_deref())
            .fetch_optional(executor)
            .await
            .context("file_folder.find_by_id")?;
        Ok(row)
    }

    /// Non-paginated list filtered by parent_id, tenant-scoped, ordered by order_num.
    #[instrument(skip_all, fields(has_parent = parent_id.is_some()))]
    pub async fn find_list(
        executor: impl sqlx::PgExecutor<'_>,
        parent_id: Option<&str>,
    ) -> anyhow::Result<Vec<SysFileFolder>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_file_folder \
             WHERE del_flag = '0' \
               AND ($1::varchar IS NULL OR tenant_id = $1) \
               AND ($2::varchar IS NULL OR parent_id = $2) \
             ORDER BY order_num ASC"
        );
        let rows = sqlx::query_as::<_, SysFileFolder>(&sql)
            .bind(tenant.as_deref())
            .bind(parent_id)
            .fetch_all(executor)
            .await
            .context("file_folder.find_list")?;
        Ok(rows)
    }

    /// Fetch all folders for tree building. Tenant-scoped, ordered by order_num.
    #[instrument(skip_all)]
    pub async fn find_tree(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<Vec<SysFileFolder>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_file_folder \
             WHERE del_flag = '0' \
               AND ($1::varchar IS NULL OR tenant_id = $1) \
             ORDER BY order_num ASC"
        );
        let rows = sqlx::query_as::<_, SysFileFolder>(&sql)
            .bind(tenant.as_deref())
            .fetch_all(executor)
            .await
            .context("file_folder.find_tree")?;
        Ok(rows)
    }

    /// Check if a folder has sub-folders.
    #[instrument(skip_all, fields(folder_id = %folder_id))]
    pub async fn has_children(
        executor: impl sqlx::PgExecutor<'_>,
        folder_id: &str,
    ) -> anyhow::Result<bool> {
        let tenant = current_tenant_scope();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_file_folder \
             WHERE parent_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(folder_id)
        .bind(tenant.as_deref())
        .fetch_one(executor)
        .await
        .context("file_folder.has_children")?;
        Ok(count > 0)
    }

    /// Insert a new folder. Returns the newly-inserted row.
    #[instrument(skip_all, fields(folder_name = %params.folder_name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: FolderInsertParams,
    ) -> anyhow::Result<SysFileFolder> {
        let audit = AuditInsert::now();
        let sql = format!(
            "INSERT INTO sys_file_folder (\
                folder_id, tenant_id, parent_id, folder_name, folder_path, order_num, \
                status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                gen_random_uuid(), $1, $2, $3, $4, $5, '0', '0', $6, $7, \
                CURRENT_TIMESTAMP, $8\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysFileFolder>(&sql)
            .bind(&params.tenant_id)
            .bind(params.parent_id.as_deref())
            .bind(&params.folder_name)
            .bind(&params.folder_path)
            .bind(params.order_num)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("file_folder.insert")?;
        Ok(row)
    }

    /// Update folder fields with COALESCE for optional fields.
    #[instrument(skip_all, fields(folder_id = %params.folder_id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: FolderUpdateParams,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_file_folder SET \
                folder_name = COALESCE($1, folder_name), \
                order_num   = COALESCE($2, order_num), \
                remark      = COALESCE($3, remark), \
                update_by   = $4, \
                update_at   = CURRENT_TIMESTAMP \
             WHERE folder_id = $5 AND del_flag = '0' \
               AND ($6::varchar IS NULL OR tenant_id = $6)",
        )
        .bind(params.folder_name.as_deref())
        .bind(params.order_num)
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.folder_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("file_folder.update_by_id")?
        .rows_affected();
        Ok(affected)
    }

    /// Soft-delete a folder (del_flag = '1'). Tenant-scoped.
    #[instrument(skip_all, fields(folder_id = %folder_id))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        folder_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_file_folder SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
             WHERE folder_id = $2 AND del_flag = '0' \
               AND ($3::varchar IS NULL OR tenant_id = $3)",
        )
        .bind(&updater)
        .bind(folder_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("file_folder.soft_delete")?
        .rows_affected();
        Ok(affected)
    }
}

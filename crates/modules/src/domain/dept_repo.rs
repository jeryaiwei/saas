//! DeptRepo — hand-written SQL for `sys_dept`.
//!
//! Conventions (DAO):
//! 1. Each method is one SQL statement or one tightly-coupled transaction.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on `sys_dept` are single-owned here.

use super::entities::SysDept;
use anyhow::Context;
use framework::context::{audit_update_by, current_tenant_scope, AuditInsert};
use tracing::instrument;

/// Single source of truth for `SELECT` column lists.
const DEPT_COLUMNS: &str = "\
    dept_id, tenant_id, parent_id, ancestors, dept_name, order_num, \
    leader, phone, email, status, del_flag, create_by, create_at, \
    update_by, update_at, remark, i18n";

/// Query filter for `DeptRepo::find_list`.
#[derive(Debug)]
pub struct DeptListFilter {
    pub dept_name: Option<String>,
    pub status: Option<String>,
}

/// Write parameters for `DeptRepo::insert`.
#[derive(Debug)]
pub struct DeptInsertParams {
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub ancestors: Vec<String>,
    pub dept_name: String,
    pub order_num: i32,
    pub leader: String,
    pub phone: String,
    pub email: String,
    pub status: String,
    pub remark: Option<String>,
}

/// Write parameters for `DeptRepo::update_by_id`.
#[derive(Debug)]
pub struct DeptUpdateParams {
    pub dept_id: String,
    pub parent_id: Option<String>,
    pub ancestors: Option<Vec<String>>,
    pub dept_name: Option<String>,
    pub order_num: Option<i32>,
    pub leader: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub status: Option<String>,
    pub remark: Option<String>,
}

/// Repository for `sys_dept`. See module docs for the DAO conventions.
pub struct DeptRepo;

impl DeptRepo {
    /// Find a single dept by `dept_id`, tenant-scoped, soft-delete filtered.
    #[instrument(skip_all, fields(dept_id = %dept_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        dept_id: &str,
    ) -> anyhow::Result<Option<SysDept>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {DEPT_COLUMNS} \
               FROM sys_dept \
              WHERE dept_id = $1 \
                AND del_flag = '0' \
                AND ($2::varchar IS NULL OR tenant_id = $2) \
              LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysDept>(&sql)
            .bind(dept_id)
            .bind(tenant.as_deref())
            .fetch_optional(executor)
            .await
            .context("find_by_id: select sys_dept")?;
        Ok(row)
    }

    /// Non-paginated full list with optional dept_name / status filters.
    /// Tenant-scoped, ordered by `order_num ASC`.
    #[instrument(skip_all, fields(
        has_name = filter.dept_name.is_some(),
        has_status = filter.status.is_some(),
    ))]
    pub async fn find_list(
        executor: impl sqlx::PgExecutor<'_>,
        filter: DeptListFilter,
    ) -> anyhow::Result<Vec<SysDept>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {DEPT_COLUMNS} FROM sys_dept \
              WHERE del_flag = '0' \
                AND ($1::varchar IS NULL OR tenant_id = $1) \
                AND ($2::varchar IS NULL OR dept_name LIKE '%' || $2 || '%') \
                AND ($3::varchar IS NULL OR status = $3) \
              ORDER BY order_num ASC"
        );
        let rows = sqlx::query_as::<_, SysDept>(&sql)
            .bind(tenant.as_deref())
            .bind(filter.dept_name.as_deref())
            .bind(filter.status.as_deref())
            .fetch_all(executor)
            .await
            .context("find_list: select sys_dept")?;
        Ok(rows)
    }

    /// Active-only option list for dropdowns. Tenant-scoped, hard cap 500.
    #[instrument(skip_all)]
    pub async fn find_option_list(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<Vec<SysDept>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {DEPT_COLUMNS} FROM sys_dept \
              WHERE del_flag = '0' AND status = '0' \
                AND ($1::varchar IS NULL OR tenant_id = $1) \
              ORDER BY order_num ASC LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysDept>(&sql)
            .bind(tenant.as_deref())
            .fetch_all(executor)
            .await
            .context("find_option_list: select sys_dept")?;
        Ok(rows)
    }

    /// List excluding `exclude_dept_id` and all its descendants.
    /// Excludes rows where `exclude_dept_id` appears in the `ancestors` array.
    #[instrument(skip_all, fields(exclude_dept_id = %exclude_dept_id))]
    pub async fn find_excluding(
        executor: impl sqlx::PgExecutor<'_>,
        exclude_dept_id: &str,
    ) -> anyhow::Result<Vec<SysDept>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {DEPT_COLUMNS} FROM sys_dept \
              WHERE del_flag = '0' \
                AND ($1::varchar IS NULL OR tenant_id = $1) \
                AND dept_id != $2 \
                AND NOT ($2 = ANY(ancestors)) \
              ORDER BY order_num ASC"
        );
        let rows = sqlx::query_as::<_, SysDept>(&sql)
            .bind(tenant.as_deref())
            .bind(exclude_dept_id)
            .fetch_all(executor)
            .await
            .context("find_excluding: select sys_dept")?;
        Ok(rows)
    }

    /// Return the `ancestors` array of the given parent dept.
    /// Returns `None` if the parent does not exist (caller maps to 7014).
    #[instrument(skip_all, fields(parent_id = %parent_id))]
    pub async fn find_parent_ancestors(
        executor: impl sqlx::PgExecutor<'_>,
        parent_id: &str,
    ) -> anyhow::Result<Option<Vec<String>>> {
        let tenant = current_tenant_scope();
        let row: Option<(Vec<String>,)> = sqlx::query_as::<_, (Vec<String>,)>(
            "SELECT ancestors FROM sys_dept \
              WHERE dept_id = $1 \
                AND del_flag = '0' \
                AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(parent_id)
        .bind(tenant.as_deref())
        .fetch_optional(executor)
        .await
        .context("find_parent_ancestors: select sys_dept")?;
        Ok(row.map(|(a,)| a))
    }

    /// Insert a new dept. Audit fields are stamped from `AuditInsert::now()`.
    /// Returns the newly-inserted row.
    #[instrument(skip_all, fields(dept_name = %params.dept_name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: DeptInsertParams,
    ) -> anyhow::Result<SysDept> {
        let audit = AuditInsert::now();
        let dept_id = uuid::Uuid::new_v4().to_string();

        let sql = format!(
            "INSERT INTO sys_dept (\
                dept_id, tenant_id, parent_id, ancestors, dept_name, order_num, \
                leader, phone, email, status, del_flag, create_by, update_by, \
                update_at, remark \
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, '0', $11, $12, \
                      CURRENT_TIMESTAMP, $13) \
            RETURNING {DEPT_COLUMNS}"
        );

        let row = sqlx::query_as::<_, SysDept>(&sql)
            .bind(&dept_id)
            .bind(&params.tenant_id)
            .bind(params.parent_id.as_deref())
            .bind(&params.ancestors)
            .bind(&params.dept_name)
            .bind(params.order_num)
            .bind(&params.leader)
            .bind(&params.phone)
            .bind(&params.email)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("insert: insert sys_dept")?;
        Ok(row)
    }

    /// Update scalar fields with COALESCE for optional fields.
    /// `ancestors` when `Some` replaces the existing value; when `None` keeps it.
    /// Audit `update_by` / `update_at` are always stamped.
    /// Returns `rows_affected` — 0 means "not found".
    #[instrument(skip_all, fields(dept_id = %params.dept_id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: DeptUpdateParams,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_dept \
                SET parent_id  = COALESCE($1, parent_id), \
                    ancestors  = COALESCE($2, ancestors), \
                    dept_name  = COALESCE($3, dept_name), \
                    order_num  = COALESCE($4, order_num), \
                    leader     = COALESCE($5, leader), \
                    phone      = COALESCE($6, phone), \
                    email      = COALESCE($7, email), \
                    status     = COALESCE($8, status), \
                    remark     = COALESCE($9, remark), \
                    update_by  = $10, \
                    update_at  = CURRENT_TIMESTAMP \
              WHERE dept_id = $11 \
                AND del_flag = '0' \
                AND ($12::varchar IS NULL OR tenant_id = $12)",
        )
        .bind(params.parent_id.as_deref())
        .bind(params.ancestors.as_deref())
        .bind(params.dept_name.as_deref())
        .bind(params.order_num)
        .bind(params.leader.as_deref())
        .bind(params.phone.as_deref())
        .bind(params.email.as_deref())
        .bind(params.status.as_deref())
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.dept_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("update_by_id: update sys_dept")?
        .rows_affected();

        Ok(affected)
    }

    /// Soft-delete a dept (`del_flag = '1'`). Tenant-scoped.
    #[instrument(skip_all, fields(dept_id = %dept_id))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        dept_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_dept \
                SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
              WHERE dept_id = $2 \
                AND del_flag = '0' \
                AND ($3::varchar IS NULL OR tenant_id = $3)",
        )
        .bind(&updater)
        .bind(dept_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("soft_delete: update sys_dept")?
        .rows_affected();

        Ok(affected)
    }
}

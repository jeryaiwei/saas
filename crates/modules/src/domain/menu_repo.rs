//! MenuRepo — hand-written SQL for `sys_menu`.
//!
//! Conventions (DAO):
//! 1. Each method is one SQL statement or one tightly-coupled transaction.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on `sys_menu` are single-owned here.

use super::entities::SysMenu;
use anyhow::Context;
use framework::context::{audit_update_by, AuditInsert};
use tracing::instrument;

/// Single source of truth for `SELECT` column lists.
const MENU_COLUMNS: &str = "\
    menu_id, menu_name, parent_id, order_num, path, component, query, \
    is_frame, is_cache, menu_type, visible, status, perms, icon, \
    create_by, create_at, update_by, update_at, remark, del_flag, i18n";

/// Lightweight projection for building tree structures — only what the
/// frontend needs to render the menu tree.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MenuTreeRow {
    pub menu_id: String,
    pub menu_name: String,
    pub parent_id: Option<String>,
}

/// Projection for role-menu tree with checked state.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoleMenuTreeRow {
    pub menu_id: String,
    pub menu_name: String,
    pub parent_id: Option<String>,
    pub is_checked: bool,
}

/// Query filter for `MenuRepo::find_list`.
#[derive(Debug)]
pub struct MenuListFilter {
    pub menu_name: Option<String>,
    pub status: Option<String>,
    pub parent_id: Option<String>,
    pub menu_type: Option<String>,
}

/// Write parameters for `MenuRepo::insert`.
#[derive(Debug)]
pub struct MenuInsertParams {
    pub menu_name: String,
    pub parent_id: Option<String>,
    pub order_num: i32,
    pub path: String,
    pub component: Option<String>,
    pub query: String,
    pub is_frame: String,
    pub is_cache: String,
    pub menu_type: String,
    pub visible: String,
    pub status: String,
    pub perms: String,
    pub icon: String,
    pub remark: Option<String>,
}

/// Write parameters for `MenuRepo::update_by_id`.
#[derive(Debug)]
pub struct MenuUpdateParams {
    pub menu_id: String,
    pub menu_name: Option<String>,
    pub parent_id: Option<String>,
    pub order_num: Option<i32>,
    pub path: Option<String>,
    pub component: Option<String>,
    pub query: Option<String>,
    pub is_frame: Option<String>,
    pub is_cache: Option<String>,
    pub menu_type: Option<String>,
    pub visible: Option<String>,
    pub status: Option<String>,
    pub perms: Option<String>,
    pub icon: Option<String>,
    pub remark: Option<String>,
}

/// Repository for `sys_menu`. See module docs for the DAO conventions.
pub struct MenuRepo;

impl MenuRepo {
    /// Find a single menu by `menu_id`, soft-delete filtered.
    #[instrument(skip_all, fields(menu_id = %menu_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        menu_id: &str,
    ) -> anyhow::Result<Option<SysMenu>> {
        let sql = format!(
            "SELECT {MENU_COLUMNS} \
               FROM sys_menu \
              WHERE menu_id = $1 \
                AND del_flag = '0' \
              LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysMenu>(&sql)
            .bind(menu_id)
            .fetch_optional(executor)
            .await
            .context("find_by_id: select sys_menu")?;
        Ok(row)
    }

    /// Non-paginated full list with optional filters.
    #[instrument(skip_all, fields(
        has_name = filter.menu_name.is_some(),
        has_status = filter.status.is_some(),
        has_parent = filter.parent_id.is_some(),
        has_type = filter.menu_type.is_some(),
    ))]
    pub async fn find_list(
        executor: impl sqlx::PgExecutor<'_>,
        filter: MenuListFilter,
    ) -> anyhow::Result<Vec<SysMenu>> {
        let sql = format!(
            "SELECT {MENU_COLUMNS} FROM sys_menu \
              WHERE del_flag = '0' \
                AND ($1::varchar IS NULL OR menu_name LIKE '%' || $1 || '%') \
                AND ($2::varchar IS NULL OR status = $2) \
                AND ($3::varchar IS NULL OR parent_id = $3) \
                AND ($4::varchar IS NULL OR menu_type = $4) \
              ORDER BY parent_id ASC, order_num ASC, menu_id ASC"
        );
        let rows = sqlx::query_as::<_, SysMenu>(&sql)
            .bind(filter.menu_name.as_deref())
            .bind(filter.status.as_deref())
            .bind(filter.parent_id.as_deref())
            .bind(filter.menu_type.as_deref())
            .fetch_all(executor)
            .await
            .context("find_list: select sys_menu")?;
        Ok(rows)
    }

    /// Return minimal tree node rows for building menu tree structures.
    #[instrument(skip_all)]
    pub async fn find_tree_nodes(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<Vec<MenuTreeRow>> {
        let rows = sqlx::query_as::<_, MenuTreeRow>(
            "SELECT menu_id, menu_name, parent_id \
               FROM sys_menu \
              WHERE del_flag = '0' \
              ORDER BY parent_id ASC, order_num ASC",
        )
        .fetch_all(executor)
        .await
        .context("find_tree_nodes: select sys_menu")?;
        Ok(rows)
    }

    /// Role-menu tree with checked state for super-admin context.
    /// Returns all menus, marking those bound to `role_id` in `tenant_id`.
    #[instrument(skip_all, fields(role_id = %role_id, tenant_id = %tenant_id))]
    pub async fn find_role_menu_tree_for_admin(
        executor: impl sqlx::PgExecutor<'_>,
        role_id: &str,
        tenant_id: &str,
    ) -> anyhow::Result<Vec<RoleMenuTreeRow>> {
        let rows = sqlx::query_as::<_, RoleMenuTreeRow>(
            "SELECT m.menu_id, m.menu_name, m.parent_id, \
                    (rm.menu_id IS NOT NULL) AS is_checked \
               FROM sys_menu m \
               LEFT JOIN ( \
                 SELECT rm.menu_id FROM sys_role_menu rm \
                  INNER JOIN sys_role r ON r.role_id = rm.role_id \
                    AND r.tenant_id = $2 AND r.del_flag = '0' \
                  WHERE rm.role_id = $1 \
               ) rm ON m.menu_id = rm.menu_id \
              WHERE m.del_flag = '0' \
              ORDER BY m.parent_id ASC, m.order_num ASC",
        )
        .bind(role_id)
        .bind(tenant_id)
        .fetch_all(executor)
        .await
        .context("find_role_menu_tree_for_admin: select sys_menu")?;
        Ok(rows)
    }

    /// Role-menu tree with checked state for tenant context.
    /// Filters menus to those allowed by the tenant's package.
    #[instrument(skip_all, fields(role_id = %role_id, tenant_id = %tenant_id))]
    pub async fn find_role_menu_tree_for_tenant(
        executor: impl sqlx::PgExecutor<'_>,
        role_id: &str,
        tenant_id: &str,
    ) -> anyhow::Result<Vec<RoleMenuTreeRow>> {
        let rows = sqlx::query_as::<_, RoleMenuTreeRow>(
            "SELECT m.menu_id, m.menu_name, m.parent_id, \
                    (rm.menu_id IS NOT NULL) AS is_checked \
               FROM sys_menu m \
               LEFT JOIN ( \
                 SELECT rm.menu_id FROM sys_role_menu rm \
                  INNER JOIN sys_role r ON r.role_id = rm.role_id \
                    AND r.tenant_id = $2 AND r.del_flag = '0' \
                  WHERE rm.role_id = $1 \
               ) rm ON m.menu_id = rm.menu_id \
               LEFT JOIN sys_tenant t ON t.tenant_id = $2 AND t.del_flag = '0' \
               LEFT JOIN sys_tenant_package p ON t.package_id = p.package_id \
                 AND p.del_flag = '0' AND p.status = '0' \
              WHERE m.del_flag = '0' \
                AND (p.menu_ids IS NULL OR m.menu_id = ANY(p.menu_ids)) \
              ORDER BY m.parent_id ASC, m.order_num ASC",
        )
        .bind(role_id)
        .bind(tenant_id)
        .fetch_all(executor)
        .await
        .context("find_role_menu_tree_for_tenant: select sys_menu")?;
        Ok(rows)
    }

    /// Return the `menu_ids` array for a tenant package.
    #[instrument(skip_all, fields(package_id = %package_id))]
    pub async fn find_package_menu_ids(
        executor: impl sqlx::PgExecutor<'_>,
        package_id: &str,
    ) -> anyhow::Result<Option<Vec<String>>> {
        let row: Option<Vec<String>> = sqlx::query_scalar(
            "SELECT menu_ids FROM sys_tenant_package \
              WHERE package_id = $1 \
                AND del_flag = '0' \
                AND status = '0'",
        )
        .bind(package_id)
        .fetch_optional(executor)
        .await
        .context("find_package_menu_ids: select sys_tenant_package")?;
        Ok(row)
    }

    /// Insert a new menu. Audit fields are stamped from `AuditInsert::now()`.
    /// Returns the newly-inserted row.
    #[instrument(skip_all, fields(menu_name = %params.menu_name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: MenuInsertParams,
    ) -> anyhow::Result<SysMenu> {
        let audit = AuditInsert::now();
        let menu_id = uuid::Uuid::new_v4().to_string();

        let sql = format!(
            "INSERT INTO sys_menu (\
                menu_id, menu_name, parent_id, order_num, path, component, query, \
                is_frame, is_cache, menu_type, visible, status, perms, icon, \
                del_flag, create_by, update_by, update_at, remark \
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, \
                      '0', $15, $16, CURRENT_TIMESTAMP, $17) \
            RETURNING {MENU_COLUMNS}"
        );

        let row = sqlx::query_as::<_, SysMenu>(&sql)
            .bind(&menu_id)
            .bind(&params.menu_name)
            .bind(params.parent_id.as_deref())
            .bind(params.order_num)
            .bind(&params.path)
            .bind(params.component.as_deref())
            .bind(&params.query)
            .bind(&params.is_frame)
            .bind(&params.is_cache)
            .bind(&params.menu_type)
            .bind(&params.visible)
            .bind(&params.status)
            .bind(&params.perms)
            .bind(&params.icon)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("insert: insert sys_menu")?;
        Ok(row)
    }

    /// Update scalar fields with COALESCE for optional fields. Returns
    /// `rows_affected` — 0 means "not found". Audit `update_by` / `update_at`
    /// are always stamped.
    #[instrument(skip_all, fields(menu_id = %params.menu_id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: MenuUpdateParams,
    ) -> anyhow::Result<u64> {
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_menu \
                SET menu_name  = COALESCE($1, menu_name), \
                    parent_id  = COALESCE($2, parent_id), \
                    order_num  = COALESCE($3, order_num), \
                    path       = COALESCE($4, path), \
                    component  = COALESCE($5, component), \
                    query      = COALESCE($6, query), \
                    is_frame   = COALESCE($7, is_frame), \
                    is_cache   = COALESCE($8, is_cache), \
                    menu_type  = COALESCE($9, menu_type), \
                    visible    = COALESCE($10, visible), \
                    status     = COALESCE($11, status), \
                    perms      = COALESCE($12, perms), \
                    icon       = COALESCE($13, icon), \
                    remark     = COALESCE($14, remark), \
                    update_by  = $15, \
                    update_at  = CURRENT_TIMESTAMP \
              WHERE menu_id = $16 \
                AND del_flag = '0'",
        )
        .bind(params.menu_name.as_deref())
        .bind(params.parent_id.as_deref())
        .bind(params.order_num)
        .bind(params.path.as_deref())
        .bind(params.component.as_deref())
        .bind(params.query.as_deref())
        .bind(params.is_frame.as_deref())
        .bind(params.is_cache.as_deref())
        .bind(params.menu_type.as_deref())
        .bind(params.visible.as_deref())
        .bind(params.status.as_deref())
        .bind(params.perms.as_deref())
        .bind(params.icon.as_deref())
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.menu_id)
        .execute(executor)
        .await
        .context("update_by_id: update sys_menu")?
        .rows_affected();

        Ok(affected)
    }

    /// Soft-delete a single menu (`del_flag = '1'`). Idempotent.
    #[instrument(skip_all, fields(menu_id = %menu_id))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        menu_id: &str,
    ) -> anyhow::Result<u64> {
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_menu \
                SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
              WHERE menu_id = $2 \
                AND del_flag = '0'",
        )
        .bind(&updater)
        .bind(menu_id)
        .execute(executor)
        .await
        .context("soft_delete: update sys_menu")?
        .rows_affected();

        Ok(affected)
    }

    /// Soft-delete a menu and all its descendants using a recursive CTE.
    /// Returns the total number of rows affected.
    #[instrument(skip_all, fields(id_count = menu_ids.len()))]
    pub async fn cascade_soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        menu_ids: &[String],
    ) -> anyhow::Result<u64> {
        if menu_ids.is_empty() {
            return Ok(0);
        }
        let updater = audit_update_by();

        let affected = sqlx::query(
            "WITH RECURSIVE menu_tree AS ( \
                 SELECT menu_id FROM sys_menu \
                  WHERE menu_id = ANY($1::varchar[]) AND del_flag = '0' \
                 UNION ALL \
                 SELECT m.menu_id FROM sys_menu m \
                  INNER JOIN menu_tree mt ON m.parent_id = mt.menu_id \
                  WHERE m.del_flag = '0' \
             ) \
             UPDATE sys_menu \
                SET del_flag = '1', update_by = $2, update_at = CURRENT_TIMESTAMP \
              WHERE menu_id IN (SELECT menu_id FROM menu_tree) \
                AND del_flag = '0'",
        )
        .bind(menu_ids)
        .bind(&updater)
        .execute(executor)
        .await
        .context("cascade_soft_delete: update sys_menu")?
        .rows_affected();

        Ok(affected)
    }
}

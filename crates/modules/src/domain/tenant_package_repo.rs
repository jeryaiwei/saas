//! TenantPackageRepo — hand-written SQL for `sys_tenant_package`.
//!
//! Conventions (DAO):
//! 1. Each method is one SQL statement or one tightly-coupled transaction.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on `sys_tenant_package` are single-owned here.

use super::entities::SysTenantPackage;
use anyhow::Context;
use framework::context::{audit_update_by, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use sqlx::PgPool;
use tracing::instrument;

/// Single source of truth for `SELECT` column lists.
const COLUMNS: &str = "\
    package_id, code, package_name, menu_ids, menu_check_strictly, \
    status, del_flag, create_by, create_at, update_by, update_at, remark";

/// Shared WHERE clause for `find_page` row + count queries.
const PACKAGE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR package_name LIKE '%' || $1 || '%') \
      AND ($2::varchar IS NULL OR status = $2)";

/// Query filter + pagination for `TenantPackageRepo::find_page`.
#[derive(Debug)]
pub struct PackageListFilter {
    pub package_name: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

/// Write parameters for `TenantPackageRepo::insert`.
#[derive(Debug)]
pub struct PackageInsertParams {
    pub code: String,
    pub package_name: String,
    pub menu_ids: Vec<String>,
    pub menu_check_strictly: bool,
    pub status: String,
    pub remark: Option<String>,
}

/// Write parameters for `TenantPackageRepo::update_by_id`.
#[derive(Debug)]
pub struct PackageUpdateParams {
    pub package_id: String,
    pub code: Option<String>,
    pub package_name: Option<String>,
    pub menu_ids: Option<Vec<String>>,
    pub menu_check_strictly: Option<bool>,
    pub status: Option<String>,
    pub remark: Option<String>,
}

/// Repository for `sys_tenant_package`. See module docs for the DAO conventions.
pub struct TenantPackageRepo;

impl TenantPackageRepo {
    /// Find a single package by `package_id`, soft-delete filtered.
    #[instrument(skip_all, fields(package_id = %package_id))]
    pub async fn find_by_id(
        pool: &PgPool,
        package_id: &str,
    ) -> anyhow::Result<Option<SysTenantPackage>> {
        let sql = format!(
            "SELECT {COLUMNS} \
               FROM sys_tenant_package \
              WHERE package_id = $1 \
                AND del_flag = '0' \
              LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysTenantPackage>(&sql)
            .bind(package_id)
            .fetch_optional(pool)
            .await
            .context("find_by_id: select sys_tenant_package")?;
        Ok(row)
    }

    /// Paginated list with optional `package_name` (LIKE) and `status` (exact) filters.
    #[instrument(skip_all, fields(
        has_name = filter.package_name.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
        rows_len = tracing::field::Empty,
        total = tracing::field::Empty,
    ))]
    pub async fn find_page(
        pool: &PgPool,
        filter: PackageListFilter,
    ) -> anyhow::Result<framework::response::Page<SysTenantPackage>> {
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_tenant_package {PACKAGE_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $3 OFFSET $4"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysTenantPackage>(&rows_sql)
                .bind(filter.package_name.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "tenant_package.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_tenant_package {PACKAGE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(filter.package_name.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(pool),
            "tenant_package.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            tracing::warn!(
                got = rows.len(),
                limit = p.limit,
                "tenant_package.find_page: rows exceeded LIMIT; truncating"
            );
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(
                rows_ms,
                count_ms,
                total_ms,
                budget_ms = SLOW_QUERY_WARN_MS,
                "tenant_package.find_page: slow paginated query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);

        let span = tracing::Span::current();
        span.record("rows_len", rows.len() as u64);
        span.record("total", total);

        Ok(p.into_page(rows, total))
    }

    /// Return active (`status='0'`) packages for dropdown UI — flat list, no
    /// pagination. Hard cap of 500 rows as a safety bound.
    #[instrument(skip_all)]
    pub async fn find_option_list(pool: &PgPool) -> anyhow::Result<Vec<SysTenantPackage>> {
        let sql = format!(
            "SELECT {COLUMNS} \
               FROM sys_tenant_package \
              WHERE status = '0' \
                AND del_flag = '0' \
              ORDER BY create_at DESC \
              LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysTenantPackage>(&sql)
            .fetch_all(pool)
            .await
            .context("find_option_list: select sys_tenant_package")?;
        Ok(rows)
    }

    /// Find active packages by a slice of ids. Used to validate bulk
    /// references (e.g. tenant creation). Returns only active rows.
    #[instrument(skip_all, fields(id_count = ids.len()))]
    pub async fn find_active_by_ids(
        pool: &PgPool,
        ids: &[String],
    ) -> anyhow::Result<Vec<SysTenantPackage>> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let sql = format!(
            "SELECT {COLUMNS} \
               FROM sys_tenant_package \
              WHERE package_id = ANY($1) \
                AND status = '0' \
                AND del_flag = '0'"
        );
        let rows = sqlx::query_as::<_, SysTenantPackage>(&sql)
            .bind(ids)
            .fetch_all(pool)
            .await
            .context("find_active_by_ids: select sys_tenant_package")?;
        Ok(rows)
    }

    /// Return `true` when `code` is not yet taken. Pass `exclude_id` to
    /// ignore the current package during an update check.
    #[instrument(skip_all, fields(code = %code))]
    pub async fn verify_code_unique(
        pool: &PgPool,
        code: &str,
        exclude_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_tenant_package \
              WHERE code = $1 \
                AND del_flag = '0' \
                AND ($2::varchar IS NULL OR package_id != $2)",
        )
        .bind(code)
        .bind(exclude_id)
        .fetch_one(pool)
        .await
        .context("verify_code_unique: count sys_tenant_package")?;
        Ok(count == 0)
    }

    /// Return `true` when `package_name` is not yet taken. Pass `exclude_id`
    /// to ignore the current package during an update check.
    #[instrument(skip_all, fields(package_name = %name))]
    pub async fn verify_name_unique(
        pool: &PgPool,
        name: &str,
        exclude_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_tenant_package \
              WHERE package_name = $1 \
                AND del_flag = '0' \
                AND ($2::varchar IS NULL OR package_id != $2)",
        )
        .bind(name)
        .bind(exclude_id)
        .fetch_one(pool)
        .await
        .context("verify_name_unique: count sys_tenant_package")?;
        Ok(count == 0)
    }

    /// Return `true` when any of the `ids` is referenced by an active tenant.
    /// Used as a guard before bulk delete.
    #[instrument(skip_all, fields(id_count = ids.len()))]
    pub async fn is_any_in_use(pool: &PgPool, ids: &[String]) -> anyhow::Result<bool> {
        if ids.is_empty() {
            return Ok(false);
        }
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_tenant \
              WHERE package_id = ANY($1) \
                AND del_flag = '0'",
        )
        .bind(ids)
        .fetch_one(pool)
        .await
        .context("is_any_in_use: count sys_tenant")?;
        Ok(count > 0)
    }

    /// Insert a new package. Audit fields are stamped from `AuditInsert::now()`.
    /// Returns the newly-inserted row.
    #[instrument(skip_all, fields(code = %params.code, package_name = %params.package_name))]
    pub async fn insert(
        pool: &PgPool,
        params: PackageInsertParams,
    ) -> anyhow::Result<SysTenantPackage> {
        let audit = AuditInsert::now();
        let package_id = uuid::Uuid::new_v4().to_string();

        let sql = format!(
            "INSERT INTO sys_tenant_package (\
                package_id, code, package_name, menu_ids, menu_check_strictly, \
                status, del_flag, create_by, update_by, update_at, remark \
            ) VALUES ($1, $2, $3, $4, $5, $6, '0', $7, $8, CURRENT_TIMESTAMP, $9) \
            RETURNING {COLUMNS}"
        );

        let row = sqlx::query_as::<_, SysTenantPackage>(&sql)
            .bind(&package_id)
            .bind(&params.code)
            .bind(&params.package_name)
            .bind(&params.menu_ids)
            .bind(params.menu_check_strictly)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(pool)
            .await
            .context("insert: insert sys_tenant_package")?;
        Ok(row)
    }

    /// Update scalar fields with COALESCE for optional fields. Returns
    /// `rows_affected` — 0 means "not found". Audit `update_by` / `update_at`
    /// are always stamped.
    #[instrument(skip_all, fields(package_id = %params.package_id))]
    pub async fn update_by_id(pool: &PgPool, params: PackageUpdateParams) -> anyhow::Result<u64> {
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_tenant_package \
                SET code                = COALESCE($1, code), \
                    package_name        = COALESCE($2, package_name), \
                    menu_ids            = COALESCE($3, menu_ids), \
                    menu_check_strictly = COALESCE($4, menu_check_strictly), \
                    status              = COALESCE($5, status), \
                    remark              = COALESCE($6, remark), \
                    update_by           = $7, \
                    update_at           = CURRENT_TIMESTAMP \
              WHERE package_id = $8 \
                AND del_flag = '0'",
        )
        .bind(params.code.as_deref())
        .bind(params.package_name.as_deref())
        .bind(params.menu_ids.as_deref())
        .bind(params.menu_check_strictly)
        .bind(params.status.as_deref())
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.package_id)
        .execute(pool)
        .await
        .context("update_by_id: update sys_tenant_package")?
        .rows_affected();

        Ok(affected)
    }

    /// Soft-delete a batch of packages (`del_flag = '1'`). Idempotent.
    #[instrument(skip_all, fields(id_count = ids.len()))]
    pub async fn soft_delete_by_ids(pool: &PgPool, ids: &[String]) -> anyhow::Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_tenant_package \
                SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
              WHERE package_id = ANY($2) \
                AND del_flag = '0'",
        )
        .bind(&updater)
        .bind(ids)
        .execute(pool)
        .await
        .context("soft_delete_by_ids: update sys_tenant_package")?
        .rows_affected();

        Ok(affected)
    }
}

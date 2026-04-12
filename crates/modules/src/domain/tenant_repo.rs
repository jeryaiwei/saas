//! TenantRepo — hand-written SQL for `sys_tenant` and `sys_user_tenant` write ownership.
//!
//! Conventions (DAO):
//! 1. Each method is one SQL statement or one tightly-coupled transaction.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on `sys_tenant` are single-owned here.
//! 4. `insert_user_tenant_binding` migrated from `user_repo` — this is
//!    the single owner of `sys_user_tenant` writes.
//! 5. NOT tenant-scoped (no `current_tenant_scope()`) — tenant management is
//!    platform-scoped and operates across all tenants.

use super::entities::SysTenant;
use anyhow::Context;
use framework::context::{audit_update_by, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::instrument;

/// Projection struct for `sys_tenant` + LEFT JOIN `sys_tenant_package`.
/// Used by `find_by_id` and `find_page` which need the package name for display.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantWithPackageName {
    // --- all SysTenant fields (same order as entity, with `t.` alias in SQL) ---
    pub id: String,
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub contact_user_name: Option<String>,
    pub contact_phone: Option<String>,
    pub company_name: String,
    pub license_number: Option<String>,
    pub address: Option<String>,
    pub intro: Option<String>,
    pub domain: Option<String>,
    pub package_id: Option<String>,
    pub expire_time: Option<chrono::DateTime<chrono::Utc>>,
    pub account_count: i32,
    pub storage_quota: i32,
    pub storage_used: i32,
    pub api_quota: i32,
    pub language: String,
    pub verify_status: Option<String>,
    pub license_image_url: Option<String>,
    pub reject_reason: Option<String>,
    pub verified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: chrono::DateTime<chrono::Utc>,
    pub update_by: String,
    pub update_at: chrono::DateTime<chrono::Utc>,
    pub remark: Option<String>,
    // --- JOIN projection ---
    pub package_name: Option<String>,
}

/// Small projection for admin user info queries.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AdminUserInfo {
    pub nick_name: String,
    pub phonenumber: String,
    pub whatsapp: String,
}

/// Internal projection row for `find_admin_user_names`.
#[derive(sqlx::FromRow)]
struct AdminNameRow {
    tenant_id: String,
    user_name: String,
}

/// Single source of truth for `sys_tenant` SELECT column list with `t.` alias.
/// Used in all JOIN queries.
const TENANT_COLUMNS: &str = "\
    t.id, t.tenant_id, t.parent_id, t.contact_user_name, t.contact_phone, \
    t.company_name, t.license_number, t.address, t.intro, t.domain, \
    t.package_id, t.expire_time, t.account_count, t.storage_quota, \
    t.storage_used, t.api_quota, t.language, t.verify_status, \
    t.license_image_url, t.reject_reason, t.verified_at, t.status, \
    t.del_flag, t.create_by, t.create_at, t.update_by, t.update_at, t.remark";

/// Shared WHERE clause for `find_page` rows + count queries.
/// $1=tenant_id, $2=contact_user_name, $3=contact_phone, $4=company_name, $5=status
const TENANT_PAGE_WHERE: &str = "\
    WHERE t.del_flag = '0' \
      AND ($1::varchar IS NULL OR t.tenant_id LIKE '%' || $1 || '%') \
      AND ($2::varchar IS NULL OR t.contact_user_name LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR t.contact_phone LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR t.company_name LIKE '%' || $4 || '%') \
      AND ($5::varchar IS NULL OR t.status = $5)";

/// Query filter + pagination for `TenantRepo::find_page`.
#[derive(Debug)]
pub struct TenantListFilter {
    pub tenant_id: Option<String>,
    pub contact_user_name: Option<String>,
    pub contact_phone: Option<String>,
    pub company_name: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

/// Write parameters for `TenantRepo::insert`.
/// `id` (uuid), audit fields, and storage/api quota defaults are stamped
/// inside the method.
#[derive(Debug)]
pub struct TenantInsertParams {
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub contact_user_name: Option<String>,
    pub contact_phone: Option<String>,
    pub company_name: String,
    pub license_number: Option<String>,
    pub address: Option<String>,
    pub intro: Option<String>,
    pub domain: Option<String>,
    pub package_id: Option<String>,
    pub expire_time: Option<String>,
    pub account_count: i32,
    pub status: String,
    pub language: String,
    pub remark: Option<String>,
}

/// Write parameters for `TenantRepo::update_by_id`.
/// All optional fields use COALESCE — `None` means "no change".
#[derive(Debug)]
pub struct TenantUpdateParams {
    pub id: String,
    pub contact_user_name: Option<String>,
    pub contact_phone: Option<String>,
    pub company_name: Option<String>,
    pub license_number: Option<String>,
    pub address: Option<String>,
    pub intro: Option<String>,
    pub domain: Option<String>,
    pub package_id: Option<String>,
    pub expire_time: Option<String>,
    pub account_count: Option<i32>,
    pub status: Option<String>,
    pub remark: Option<String>,
}

/// Repository for `sys_tenant` and `sys_user_tenant` write operations.
/// See module docs for the five DAO conventions.
pub struct TenantRepo;

impl TenantRepo {
    /// Find a single tenant by its surrogate `id` (UUID PK). LEFT JOINs
    /// `sys_tenant_package` so callers can display the package name without
    /// a second query.
    ///
    /// Returns `None` if not found or soft-deleted.
    #[instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: &str,
    ) -> anyhow::Result<Option<TenantWithPackageName>> {
        let sql = format!(
            "SELECT {TENANT_COLUMNS}, p.package_name \
               FROM sys_tenant t \
               LEFT JOIN sys_tenant_package p ON t.package_id = p.package_id \
              WHERE t.id = $1 \
                AND t.del_flag = '0' \
              LIMIT 1"
        );
        let row = sqlx::query_as::<_, TenantWithPackageName>(&sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .context("find_by_id: select sys_tenant")?;
        Ok(row)
    }

    /// Find a tenant by its business `tenant_id` (e.g. `"000001"`).
    /// No JOIN — for existence/uniqueness checks only. Returns `None` if
    /// not found or soft-deleted.
    #[instrument(skip_all, fields(tenant_id = %tenant_id))]
    pub async fn find_by_tenant_id(
        executor: impl sqlx::PgExecutor<'_>,
        tenant_id: &str,
    ) -> anyhow::Result<Option<SysTenant>> {
        let sql = format!(
            "SELECT {TENANT_COLUMNS} \
               FROM sys_tenant t \
              WHERE t.tenant_id = $1 \
                AND t.del_flag = '0' \
              LIMIT 1"
        );
        // Strip the `t.` prefix: this SELECT has no JOIN, bare column names are fine
        // for the RETURNING-less fetch. But TENANT_COLUMNS uses `t.` alias so we
        // need an aliased FROM — the `t` alias is already present in the FROM clause.
        let row = sqlx::query_as::<_, SysTenant>(&sql)
            .bind(tenant_id)
            .fetch_optional(executor)
            .await
            .context("find_by_tenant_id: select sys_tenant")?;
        Ok(row)
    }

    /// Paginated list of tenants with optional filters. LEFT JOINs
    /// `sys_tenant_package` for the package name column.
    ///
    /// ## Expected indexes
    /// - `sys_tenant(del_flag, create_at DESC)` — sort + soft-delete filter
    /// - `sys_tenant(status)` — status filter
    ///
    /// ## Consistency caveats
    /// Offset pagination is not snapshot-consistent. See
    /// `docs/framework/framework-pagination-spec.md` §8.1.
    #[instrument(skip_all, fields(
        has_tenant_id = filter.tenant_id.is_some(),
        has_company_name = filter.company_name.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
        rows_len = tracing::field::Empty,
        total = tracing::field::Empty,
    ))]
    pub async fn find_page(
        pool: &PgPool,
        filter: TenantListFilter,
    ) -> anyhow::Result<framework::response::Page<TenantWithPackageName>> {
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {TENANT_COLUMNS}, p.package_name \
               FROM sys_tenant t \
               LEFT JOIN sys_tenant_package p ON t.package_id = p.package_id \
             {TENANT_PAGE_WHERE} \
             ORDER BY t.create_at DESC \
             LIMIT $6 OFFSET $7"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, TenantWithPackageName>(&rows_sql)
                .bind(filter.tenant_id.as_deref())
                .bind(filter.contact_user_name.as_deref())
                .bind(filter.contact_phone.as_deref())
                .bind(filter.company_name.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "tenant.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!(
            "SELECT COUNT(*) \
               FROM sys_tenant t \
               LEFT JOIN sys_tenant_package p ON t.package_id = p.package_id \
             {TENANT_PAGE_WHERE}"
        );
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(filter.tenant_id.as_deref())
                .bind(filter.contact_user_name.as_deref())
                .bind(filter.contact_phone.as_deref())
                .bind(filter.company_name.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(pool),
            "tenant.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        // Runtime post-condition: if DB returned more rows than LIMIT, truncate
        // defensively and warn (spec §4.3 post-condition).
        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            tracing::warn!(
                got = rows.len(),
                limit = p.limit,
                "tenant.find_page: rows exceeded LIMIT; truncating"
            );
            rows.truncate(p.limit as usize);
        }

        // Slow-query signal (spec §6.2): warn above 300ms budget.
        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(
                rows_ms,
                count_ms,
                total_ms,
                budget_ms = SLOW_QUERY_WARN_MS,
                "tenant.find_page: slow paginated query"
            );
        }

        // Reconcile total under Race B (spec §8.2).
        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);

        let span = tracing::Span::current();
        span.record("rows_len", rows.len() as u64);
        span.record("total", total);

        Ok(p.into_page(rows, total))
    }

    /// For each `tenant_id` in the slice, find the earliest admin user name
    /// (`is_admin='1'`, `status='0'`). Returns a map of tenant_id → user_name.
    ///
    /// Uses `DISTINCT ON (ut.tenant_id)` + `ORDER BY ut.tenant_id, ut.create_at ASC`
    /// to reliably pick the first admin registered per tenant.
    #[instrument(skip_all, fields(tenant_count = tenant_ids.len()))]
    pub async fn find_admin_user_names(
        executor: impl sqlx::PgExecutor<'_>,
        tenant_ids: &[String],
    ) -> anyhow::Result<HashMap<String, String>> {
        if tenant_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows: Vec<AdminNameRow> = sqlx::query_as(
            "SELECT DISTINCT ON (ut.tenant_id) ut.tenant_id, u.user_name \
               FROM sys_user_tenant ut \
               INNER JOIN sys_user u ON ut.user_id = u.user_id \
              WHERE ut.tenant_id = ANY($1) \
                AND ut.is_admin = '1' \
                AND ut.status = '0' \
                AND u.del_flag = '0' \
              ORDER BY ut.tenant_id, ut.create_at ASC",
        )
        .bind(tenant_ids)
        .fetch_all(executor)
        .await
        .context("find_admin_user_names: select sys_user_tenant")?;

        Ok(rows
            .into_iter()
            .map(|r| (r.tenant_id, r.user_name))
            .collect())
    }

    /// Find the primary admin user's contact info for a single tenant.
    /// Returns the earliest-registered admin (`is_admin='1'`, `status='0'`).
    #[instrument(skip_all, fields(tenant_id = %tenant_id))]
    pub async fn find_admin_user_info(
        executor: impl sqlx::PgExecutor<'_>,
        tenant_id: &str,
    ) -> anyhow::Result<Option<AdminUserInfo>> {
        let row = sqlx::query_as::<_, AdminUserInfo>(
            "SELECT u.nick_name, \
                    COALESCE(u.phonenumber, '') AS phonenumber, \
                    COALESCE(u.whatsapp, '') AS whatsapp \
               FROM sys_user_tenant ut \
               INNER JOIN sys_user u ON ut.user_id = u.user_id \
              WHERE ut.tenant_id = $1 \
                AND ut.is_admin = '1' \
                AND ut.status = '0' \
                AND u.del_flag = '0' \
              ORDER BY ut.create_at ASC \
              LIMIT 1",
        )
        .bind(tenant_id)
        .fetch_optional(executor)
        .await
        .context("find_admin_user_info: select sys_user_tenant")?;
        Ok(row)
    }

    /// Return the union of `tenant_ids` that have at least one child tenant.
    /// Used as a guard before delete — callers must refuse to delete parent
    /// tenants that still have children.
    #[instrument(skip_all, fields(tenant_count = tenant_ids.len()))]
    pub async fn find_tenant_ids_with_children(
        executor: impl sqlx::PgExecutor<'_>,
        tenant_ids: &[String],
    ) -> anyhow::Result<Vec<String>> {
        if tenant_ids.is_empty() {
            return Ok(vec![]);
        }
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT parent_id \
               FROM sys_tenant \
              WHERE parent_id = ANY($1) \
                AND del_flag = '0'",
        )
        .bind(tenant_ids)
        .fetch_all(executor)
        .await
        .context("find_tenant_ids_with_children: select sys_tenant")?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    /// Return `true` when a tenant whose `company_name` starts with `name`
    /// already exists. Pass `exclude_tenant_id` to ignore the current tenant
    /// during update uniqueness checks.
    #[instrument(skip_all, fields(name = %name))]
    pub async fn exists_by_company_name_prefix(
        executor: impl sqlx::PgExecutor<'_>,
        name: &str,
        exclude_tenant_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_tenant \
              WHERE company_name LIKE $1 || '%' \
                AND del_flag = '0' \
                AND ($2::varchar IS NULL OR tenant_id != $2)",
        )
        .bind(name)
        .bind(exclude_tenant_id)
        .fetch_one(executor)
        .await
        .context("exists_by_company_name_prefix: count sys_tenant")?;
        Ok(count > 0)
    }

    /// Fetch the next value from the `tenant_id_seq` PostgreSQL sequence.
    /// The caller uses this to construct a zero-padded `tenant_id` string
    /// (e.g. `format!("{:06}", next_val)` → `"000042"`).
    #[instrument(skip_all)]
    pub async fn generate_next_tenant_id(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<i64> {
        let next_val: i64 = sqlx::query_scalar("SELECT nextval('tenant_id_seq') AS next_val")
            .fetch_one(executor)
            .await
            .context("generate_next_tenant_id: nextval")?;
        Ok(next_val)
    }

    /// Insert a new `sys_tenant` row using the provided executor (pool or
    /// transaction). Returns the inserted `SysTenant` via RETURNING.
    ///
    /// - `id` is generated with `uuid::Uuid::new_v4()`
    /// - Audit fields come from `AuditInsert::now()`
    /// - Storage/api quotas default to 0 (callers configure them separately)
    /// - `del_flag = '0'` is hardcoded
    #[instrument(skip_all, fields(tenant_id = %params.tenant_id, company_name = %params.company_name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: TenantInsertParams,
    ) -> anyhow::Result<SysTenant> {
        let audit = AuditInsert::now();
        let id = uuid::Uuid::new_v4().to_string();

        // For the RETURNING clause we need bare column names (no `t.` alias).
        // Strip the `t.` prefix from TENANT_COLUMNS for this single use.
        let plain_columns = TENANT_COLUMNS.replace("t.", "");

        let sql = format!(
            "INSERT INTO sys_tenant (\
                id, tenant_id, parent_id, contact_user_name, contact_phone, \
                company_name, license_number, address, intro, domain, \
                package_id, expire_time, account_count, storage_quota, \
                storage_used, api_quota, language, status, del_flag, \
                create_by, update_by, update_at, remark\
            ) VALUES (\
                $1, $2, $3, $4, $5, \
                $6, $7, $8, $9, $10, \
                $11, $12::timestamptz, $13, 0, \
                0, 0, $14, $15, '0', \
                $16, $17, CURRENT_TIMESTAMP, $18\
            ) RETURNING {plain_columns}"
        );

        let tenant = sqlx::query_as::<_, SysTenant>(&sql)
            .bind(&id)
            .bind(&params.tenant_id)
            .bind(params.parent_id.as_deref())
            .bind(params.contact_user_name.as_deref())
            .bind(params.contact_phone.as_deref())
            .bind(&params.company_name)
            .bind(params.license_number.as_deref())
            .bind(params.address.as_deref())
            .bind(params.intro.as_deref())
            .bind(params.domain.as_deref())
            .bind(params.package_id.as_deref())
            .bind(params.expire_time.as_deref())
            .bind(params.account_count)
            .bind(&params.language)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("insert: insert sys_tenant")?;

        Ok(tenant)
    }

    /// Update scalar fields of a tenant by surrogate `id`. Optional fields
    /// use COALESCE so callers can pass `None` to leave them unchanged.
    /// Returns `rows_affected` — 0 means "not found or already deleted".
    #[instrument(skip_all, fields(id = %params.id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: TenantUpdateParams,
    ) -> anyhow::Result<u64> {
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_tenant \
                SET contact_user_name = COALESCE($1, contact_user_name), \
                    contact_phone     = COALESCE($2, contact_phone), \
                    company_name      = COALESCE($3, company_name), \
                    license_number    = COALESCE($4, license_number), \
                    address           = COALESCE($5, address), \
                    intro             = COALESCE($6, intro), \
                    domain            = COALESCE($7, domain), \
                    package_id        = COALESCE($8, package_id), \
                    expire_time       = COALESCE($9::timestamptz, expire_time), \
                    account_count     = COALESCE($10, account_count), \
                    status            = COALESCE($11, status), \
                    remark            = COALESCE($12, remark), \
                    update_by         = $13, \
                    update_at         = CURRENT_TIMESTAMP \
              WHERE id = $14 \
                AND del_flag = '0'",
        )
        .bind(params.contact_user_name.as_deref())
        .bind(params.contact_phone.as_deref())
        .bind(params.company_name.as_deref())
        .bind(params.license_number.as_deref())
        .bind(params.address.as_deref())
        .bind(params.intro.as_deref())
        .bind(params.domain.as_deref())
        .bind(params.package_id.as_deref())
        .bind(params.expire_time.as_deref())
        .bind(params.account_count)
        .bind(params.status.as_deref())
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.id)
        .execute(executor)
        .await
        .context("update_by_id: update sys_tenant")?
        .rows_affected();

        Ok(affected)
    }

    /// Soft-delete a batch of tenants by surrogate `id` (`del_flag = '1'`).
    /// Idempotent — already-deleted rows are silently skipped.
    /// Returns `rows_affected` (number of rows actually updated).
    #[instrument(skip_all, fields(id_count = ids.len()))]
    pub async fn soft_delete_by_ids(
        executor: impl sqlx::PgExecutor<'_>,
        ids: &[String],
    ) -> anyhow::Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_tenant \
                SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
              WHERE id = ANY($2) \
                AND del_flag = '0'",
        )
        .bind(&updater)
        .bind(ids)
        .execute(executor)
        .await
        .context("soft_delete_by_ids: update sys_tenant")?
        .rows_affected();

        Ok(affected)
    }

    /// Insert a `sys_user_tenant` row binding `user_id` to `tenant_id` using
    /// the provided executor (pool or transaction). Migrated from `UserRepo` —
    /// this is now the single write owner of `sys_user_tenant`.
    ///
    /// Parameters:
    /// - `is_admin`:   `"1"` = admin, `"0"` = regular member
    /// - `is_default`: `"1"` = default tenant for the user, `"0"` = non-default
    ///
    /// `ON CONFLICT (user_id, tenant_id) DO NOTHING` makes this idempotent —
    /// re-binding an already-bound user is a no-op rather than an error.
    ///
    /// Audit fields come from `AuditInsert::now()`.
    #[instrument(skip_all, fields(user_id = %user_id, tenant_id = %tenant_id, is_admin = %is_admin))]
    pub async fn insert_user_tenant_binding(
        executor: impl sqlx::PgExecutor<'_>,
        user_id: &str,
        tenant_id: &str,
        is_default: &str,
        is_admin: &str,
    ) -> anyhow::Result<()> {
        let audit = AuditInsert::now();
        // `update_at` has no DB default but is NOT NULL — supply CURRENT_TIMESTAMP.
        // `create_by` / `update_by` stamped from AuditInsert to preserve audit trail
        // parity with the sys_user insert in the same transaction.
        sqlx::query(
            "INSERT INTO sys_user_tenant \
                (user_id, tenant_id, is_default, is_admin, status, \
                 create_by, update_by, update_at) \
             VALUES ($1, $2, $3, $4, '0', $5, $6, CURRENT_TIMESTAMP) \
             ON CONFLICT (user_id, tenant_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(tenant_id)
        .bind(is_default)
        .bind(is_admin)
        .bind(&audit.create_by)
        .bind(&audit.update_by)
        .execute(executor)
        .await
        .context("insert_user_tenant_binding: insert sys_user_tenant")?;
        Ok(())
    }
}

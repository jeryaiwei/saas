//! RoleRepo — hand-written SQL for sys_role and its join tables.
//!
//! Conventions (from the spec's DAO conventions section):
//! 1. Each method is one SQL statement OR one tightly-coupled transaction.
//! 2. No cross-repo calls from inside this file — only service.rs orchestrates.
//! 3. Cross-table JOINs are allowed (the allocated-users query reads
//!    sys_user + sys_user_tenant).
//! 4. INSERT/UPDATE/DELETE on sys_role and its join tables are single-owner
//!    to this file.

use super::entities::SysRole;
use anyhow::Context;
use chrono::{DateTime, Utc};
use framework::context::{audit_update_by, current_tenant_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use sqlx::{PgPool, Postgres, Transaction};
use tracing::instrument;

/// Projection row for allocated/unallocated user list queries. Local to
/// this module but `pub` so DTOs can convert from it. Contains the columns
/// the list pages need, NOT the full `SysUser`.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AllocatedUserRow {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub phonenumber: String,
    pub status: String,
    pub create_at: DateTime<Utc>,
}

/// Single source of truth for `SELECT` column lists. Keeps `find_by_id`,
/// `find_page`, and friends in sync as the schema evolves.
const COLUMNS: &str = "\
    role_id, tenant_id, role_name, role_key, role_sort, data_scope, \
    menu_check_strictly, dept_check_strictly, status, del_flag, \
    create_by, create_at, update_by, update_at, remark";

/// Shared WHERE clause for `find_page` row + count queries. Single
/// source of truth — add a new filter here and both queries pick it up.
const ROLE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR role_name LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR role_key LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR status = $4)";

/// Shared WHERE for `find_allocated_users_page` rows + count.
const ALLOCATED_USER_PAGE_WHERE: &str = "\
    WHERE ur.role_id = $1 \
      AND u.del_flag = '0' \
      AND ut.status = '0' \
      AND ($2::varchar IS NULL OR ut.tenant_id = $2) \
      AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%')";

/// Shared WHERE for `find_unallocated_users_page` rows + count. Used
/// with a `LEFT JOIN sys_user_role ur ON ur.user_id = u.user_id AND
/// ur.role_id = $1` in FROM; the WHERE filters by `ur.role_id IS NULL`
/// to keep only users *not* bound to the role. Bind order matches
/// allocated: `$1=role_id, $2=tenant, $3=user_name`.
const UNALLOCATED_USER_PAGE_WHERE: &str = "\
    WHERE ur.role_id IS NULL \
      AND u.del_flag = '0' \
      AND ut.status = '0' \
      AND ($2::varchar IS NULL OR ut.tenant_id = $2) \
      AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%')";

/// Query filter + pagination for `RoleRepo::find_page`. Mirrors
/// `system::role::dto::ListRoleDto` but owned at the repo layer
/// (DAO isolation — no upstream DTO dependency in domain).
///
/// `page: PageQuery` carries validator attrs from framework; those only
/// fire at HTTP extraction time. This struct doesn't derive `Validate`.
#[derive(Debug)]
pub struct RoleListFilter {
    pub name: Option<String>,
    pub role_key: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

/// Query filter + pagination shared by `find_allocated_users_page` and
/// `find_unallocated_users_page`. Both endpoints accept a role id + an
/// optional user_name substring + standard paging.
#[derive(Debug)]
pub struct AllocatedUserFilter {
    pub role_id: String,
    pub user_name: Option<String>,
    pub page: PageQuery,
}

/// Write parameters for `RoleRepo::insert_with_menus`. `role_id`,
/// `tenant_id`, and audit fields are stamped inside the method.
/// The caller must provide a `&mut Transaction` — the method performs
/// multiple queries (INSERT role + bulk INSERT role_menus).
#[derive(Debug)]
pub struct RoleInsertParams {
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i32,
    pub status: String,
    pub remark: Option<String>,
    pub menu_ids: Vec<String>,
}

/// Write parameters for `RoleRepo::update_with_menus`. The caller must
/// provide a `&mut Transaction` — the method performs multiple queries.
/// The UPDATE stamps audit fields inside the method.
#[derive(Debug)]
pub struct RoleUpdateParams {
    pub role_id: String,
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i32,
    pub status: String,
    pub remark: Option<String>,
    pub menu_ids: Vec<String>,
}

/// Repository for `sys_role` and its join tables (`sys_role_menu`,
/// `sys_user_role`). See module docs for the four DAO conventions.
pub struct RoleRepo;

impl RoleRepo {
    /// Private: bulk-insert role↔menu bindings inside an existing
    /// transaction. Dedupes caller input via `SELECT DISTINCT`. No-op
    /// when `menu_ids` is empty. Used by both `insert_with_menus` and
    /// `update_with_menus`.
    async fn bulk_insert_role_menus(
        tx: &mut Transaction<'_, Postgres>,
        role_id: &str,
        menu_ids: &[String],
    ) -> anyhow::Result<()> {
        if menu_ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO sys_role_menu (role_id, menu_id) \
             SELECT DISTINCT $1, unnest($2::varchar[])",
        )
        .bind(role_id)
        .bind(menu_ids)
        .execute(&mut **tx)
        .await
        .context("bulk_insert_role_menus")?;
        Ok(())
    }

    /// Find by role_id, tenant-scoped, soft-delete filtered.
    #[instrument(skip_all, fields(role_id = %role_id))]
    pub async fn find_by_id(pool: &PgPool, role_id: &str) -> anyhow::Result<Option<SysRole>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} \
               FROM sys_role \
              WHERE role_id = $1 \
                AND del_flag = '0' \
                AND ($2::varchar IS NULL OR tenant_id = $2) \
              LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysRole>(&sql)
            .bind(role_id)
            .bind(tenant.as_deref())
            .fetch_optional(pool)
            .await
            .context("find_by_id")?;
        Ok(row)
    }

    /// List menu_ids bound to a role. Not tenant-scoped because `sys_role_menu`
    /// has no tenant column (the `sys_role` row itself is tenant-scoped).
    /// Results are sorted by `menu_id` for deterministic test assertions.
    #[instrument(skip_all, fields(role_id = %role_id))]
    pub async fn find_menu_ids_by_role(
        pool: &PgPool,
        role_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT menu_id FROM sys_role_menu WHERE role_id = $1 ORDER BY menu_id")
                .bind(role_id)
                .fetch_all(pool)
                .await
                .context("find_menu_ids_by_role")?;
        Ok(rows.into_iter().map(|(m,)| m).collect())
    }

    /// Paginated list with optional name / role_key / status filters.
    /// Tenant-scoped via `current_tenant_scope`.
    ///
    /// ## Expected indexes
    /// - `sys_role(tenant_id, status, role_sort)` WHERE `del_flag = '0'` —
    ///   combined filter + sort (partial index, v1 TBD)
    ///
    /// See `docs/framework-pagination-indexes.md` §2 for the global registry.
    ///
    /// ## Consistency caveats
    /// Offset pagination is not snapshot-consistent. See
    /// `docs/framework-pagination-spec.md` §8.1.
    ///
    /// ## Performance expectation
    /// Role tables are small (typically < 1k rows per tenant); seq scan
    /// is acceptable until the global index is added.
    #[instrument(skip_all, fields(
        tenant_id = tracing::field::Empty,
        has_name = filter.name.is_some(),
        has_role_key = filter.role_key.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
        rows_len = tracing::field::Empty,
        total = tracing::field::Empty,
    ))]
    pub async fn find_page(
        pool: &PgPool,
        filter: RoleListFilter,
    ) -> anyhow::Result<framework::response::Page<SysRole>> {
        let tenant = current_tenant_scope();
        if let Some(t) = tenant.as_deref() {
            tracing::Span::current().record("tenant_id", t);
        }
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_role {ROLE_PAGE_WHERE} \
             ORDER BY role_sort ASC, create_at DESC \
             LIMIT $5 OFFSET $6"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysRole>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.name.as_deref())
                .bind(filter.role_key.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "role.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_role {ROLE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.name.as_deref())
                .bind(filter.role_key.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(pool),
            "role.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            tracing::warn!(
                got = rows.len(),
                limit = p.limit,
                "role.find_page: rows exceeded LIMIT; truncating"
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
                "role.find_page: slow paginated query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);

        let span = tracing::Span::current();
        span.record("rows_len", rows.len() as u64);
        span.record("total", total);

        Ok(p.into_page(rows, total))
    }

    /// Create a role and bind its menus inside a caller-provided transaction.
    ///
    /// Returns the newly-inserted `SysRole` row. The caller (service layer)
    /// is expected to have already validated that `menu_ids` reference
    /// active menus that belong to the tenant's package range — Phase 1
    /// doesn't enforce that yet (it's Phase 2 work), so any valid
    /// `menu_id` string will insert.
    ///
    /// Audit fields (`create_by` / `update_by`) are stamped from
    /// `AuditInsert::now()`. Tenant is pulled from `current_tenant_scope()`;
    /// callers MUST be in a tenant-scoped context (this is enforced by
    /// the route-level access layer — a super-admin `run_ignoring_tenant`
    /// context would return an error here because tenant is required).
    ///
    /// Takes `&mut Transaction` because it performs multiple queries (INSERT
    /// role + bulk INSERT role_menus). The caller manages begin/commit.
    #[instrument(skip_all, fields(role_name = %params.role_name, menu_count = params.menu_ids.len()))]
    pub async fn insert_with_menus(
        tx: &mut Transaction<'_, Postgres>,
        params: RoleInsertParams,
    ) -> anyhow::Result<SysRole> {
        let audit = AuditInsert::now();
        let tenant = current_tenant_scope()
            .context("insert_with_menus: tenant_id required from RequestContext")?;
        let role_id = uuid::Uuid::new_v4().to_string();

        // `update_at` is NOT NULL with no DB default (unlike `create_at`
        // which defaults to `CURRENT_TIMESTAMP`), so we stamp it
        // explicitly here to match the `sys_role` schema.
        let insert_sql = format!(
            "INSERT INTO sys_role (\
                role_id, tenant_id, role_name, role_key, role_sort, \
                data_scope, menu_check_strictly, dept_check_strictly, \
                status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES ($1, $2, $3, $4, $5, '1', false, false, $6, '0', $7, $8, CURRENT_TIMESTAMP, $9) \
            RETURNING {COLUMNS}"
        );
        let role = sqlx::query_as::<_, SysRole>(&insert_sql)
            .bind(&role_id)
            .bind(&tenant)
            .bind(&params.role_name)
            .bind(&params.role_key)
            .bind(params.role_sort)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(&mut **tx)
            .await
            .context("insert_with_menus: insert sys_role")?;

        Self::bulk_insert_role_menus(tx, &role_id, &params.menu_ids).await?;

        Ok(role)
    }

    /// Update a role's scalar fields AND replace its menu bindings inside
    /// a caller-provided transaction. Strategy: UPDATE sys_role, then
    /// `DELETE FROM sys_role_menu WHERE role_id = ?` + bulk re-insert.
    /// Simpler than computing a diff, and safe because `sys_role_menu`
    /// has no audit fields or FK cascades beyond the composite PK.
    ///
    /// Returns the number of `sys_role` rows affected by the UPDATE
    /// (not including role_menu rows). Zero means "no such role in the
    /// current tenant" — caller should map to `DATA_NOT_FOUND`.
    ///
    /// Tenant scoping: the UPDATE is gated by
    /// `($8::varchar IS NULL OR tenant_id = $8)` so cross-tenant edits
    /// return affected=0 (same 1001 response as "not found"), which is
    /// the information-hiding strategy documented in the spec.
    ///
    /// Takes `&mut Transaction` because it performs multiple queries
    /// (UPDATE + DELETE + INSERT). The caller manages begin/commit.
    #[instrument(skip_all, fields(role_id = %params.role_id, menu_count = params.menu_ids.len()))]
    pub async fn update_with_menus(
        tx: &mut Transaction<'_, Postgres>,
        params: RoleUpdateParams,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_role \
                SET role_name = $1, role_key = $2, role_sort = $3, \
                    status = $4, remark = $5, update_by = $6, \
                    update_at = CURRENT_TIMESTAMP \
              WHERE role_id = $7 \
                AND del_flag = '0' \
                AND ($8::varchar IS NULL OR tenant_id = $8)",
        )
        .bind(&params.role_name)
        .bind(&params.role_key)
        .bind(params.role_sort)
        .bind(&params.status)
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.role_id)
        .bind(tenant.as_deref())
        .execute(&mut **tx)
        .await
        .context("update_with_menus: update sys_role")?
        .rows_affected();

        if affected > 0 {
            // Replace-all: delete existing bindings, insert new ones.
            // Defense-in-depth: SELECT DISTINCT dedupes caller typos so
            // duplicate menu_ids don't hit the composite PK violation.
            sqlx::query("DELETE FROM sys_role_menu WHERE role_id = $1")
                .bind(&params.role_id)
                .execute(&mut **tx)
                .await
                .context("update_with_menus: delete sys_role_menu")?;

            Self::bulk_insert_role_menus(tx, &params.role_id, &params.menu_ids).await?;
        }

        Ok(affected)
    }

    /// Toggle a role's `status` column. Tenant-scoped, soft-delete filtered.
    /// Returns rows_affected — 0 means "not found in current tenant".
    #[instrument(skip_all, fields(role_id = %role_id, status = %status))]
    pub async fn change_status(pool: &PgPool, role_id: &str, status: &str) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_role \
                SET status = $1, update_by = $2, update_at = CURRENT_TIMESTAMP \
              WHERE role_id = $3 \
                AND del_flag = '0' \
                AND ($4::varchar IS NULL OR tenant_id = $4)",
        )
        .bind(status)
        .bind(&updater)
        .bind(role_id)
        .bind(tenant.as_deref())
        .execute(pool)
        .await
        .context("change_status: update sys_role")?
        .rows_affected();

        Ok(affected)
    }

    /// Soft-delete a role by id (sets `del_flag = '1'`). Tenant-scoped.
    /// Returns rows_affected.
    #[instrument(skip_all, fields(role_id = %role_id))]
    pub async fn soft_delete_by_id(pool: &PgPool, role_id: &str) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_role \
                SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
              WHERE role_id = $2 \
                AND del_flag = '0' \
                AND ($3::varchar IS NULL OR tenant_id = $3)",
        )
        .bind(&updater)
        .bind(role_id)
        .bind(tenant.as_deref())
        .execute(pool)
        .await
        .context("soft_delete_by_id: update sys_role")?
        .rows_affected();

        Ok(affected)
    }

    /// Return active (`status='0'`) roles for dropdown UI — flat list, no
    /// pagination. Tenant-scoped. Hard cap of 500 rows as a safety bound.
    #[instrument(skip_all)]
    pub async fn find_option_list(pool: &PgPool) -> anyhow::Result<Vec<SysRole>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} \
               FROM sys_role \
              WHERE del_flag = '0' \
                AND status = '0' \
                AND ($1::varchar IS NULL OR tenant_id = $1) \
              ORDER BY role_sort ASC \
              LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysRole>(&sql)
            .bind(tenant.as_deref())
            .fetch_all(pool)
            .await
            .context("find_option_list: select sys_role")?;
        Ok(rows)
    }

    /// Users currently bound to `role_id` in the current tenant.
    /// Joins sys_user + sys_user_role + sys_user_tenant. Reads `sys_user`
    /// from the user module's table — allowed by DAO rule 3 (cross-table
    /// JOINs where the caller's mental model lives) because callers think
    /// "this role's users", not "users with this role". The JOIN is
    /// read-only; writes to `sys_user` stay in `user_repo.rs`.
    ///
    /// ## Expected indexes
    /// - `sys_user_role(role_id)` — `WHERE ur.role_id = $1`
    /// - `sys_user_tenant(user_id)` + `(tenant_id, status)` — JOIN + filter
    /// - `sys_user(create_at DESC) WHERE del_flag = '0'` — sort + soft delete
    ///
    /// See `docs/framework-pagination-indexes.md` §3 for the registry.
    ///
    /// ## Consistency caveats
    /// Same Race A/B/C as generic offset pagination. See spec §8.1.
    ///
    /// ## Performance expectation
    /// - Small roles (< 100 users): < 20ms
    /// - Large roles (> 10k users): up to 300ms; deep pages unsupported
    #[instrument(skip_all, fields(
        tenant_id = tracing::field::Empty,
        role_id = %filter.role_id,
        has_name_filter = filter.user_name.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
        rows_len = tracing::field::Empty,
        total = tracing::field::Empty,
    ))]
    pub async fn find_allocated_users_page(
        pool: &PgPool,
        filter: AllocatedUserFilter,
    ) -> anyhow::Result<framework::response::Page<AllocatedUserRow>> {
        let tenant = current_tenant_scope();
        if let Some(t) = tenant.as_deref() {
            tracing::Span::current().record("tenant_id", t);
        }
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT u.user_id, u.user_name, u.nick_name, u.email, \
                    u.phonenumber, u.status, u.create_at \
               FROM sys_user u \
               JOIN sys_user_role ur ON ur.user_id = u.user_id \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
             {ALLOCATED_USER_PAGE_WHERE} \
             ORDER BY u.create_at DESC \
             LIMIT $4 OFFSET $5"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, AllocatedUserRow>(&rows_sql)
                .bind(&filter.role_id)
                .bind(tenant.as_deref())
                .bind(filter.user_name.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "role.find_allocated_users_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!(
            "SELECT COUNT(*) FROM sys_user u \
               JOIN sys_user_role ur ON ur.user_id = u.user_id \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
             {ALLOCATED_USER_PAGE_WHERE}"
        );
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(&filter.role_id)
                .bind(tenant.as_deref())
                .bind(filter.user_name.as_deref())
                .fetch_one(pool),
            "role.find_allocated_users_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            tracing::warn!(
                got = rows.len(),
                limit = p.limit,
                "role.find_allocated_users_page: rows exceeded LIMIT; truncating"
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
                "role.find_allocated_users_page: slow paginated query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);

        let span = tracing::Span::current();
        span.record("rows_len", rows.len() as u64);
        span.record("total", total);

        Ok(p.into_page(rows, total))
    }

    /// Users in the current tenant who are NOT bound to `role_id`.
    /// LEFT JOIN anti-pattern: start from every active user in the tenant,
    /// LEFT JOIN their `sys_user_role` row for this specific role_id, and
    /// keep only the rows where the join missed (`ur.role_id IS NULL`).
    ///
    /// Returns the same `AllocatedUserRow` projection — the shape is
    /// identical; only the set-membership semantics differ.
    ///
    /// ## Expected indexes
    /// Same as `find_allocated_users_page`; the LEFT JOIN anti-join still
    /// uses `sys_user_role(user_id, role_id)` (composite PK) effectively.
    ///
    /// See `docs/framework-pagination-indexes.md` §4 for the registry.
    ///
    /// ## Consistency caveats
    /// Same Race A/B/C as generic offset pagination. See spec §8.1.
    ///
    /// ## Performance expectation
    /// Dominated by `sys_user` table size (the universe); anti-join cost
    /// grows with total tenant user count, not role size.
    #[instrument(skip_all, fields(
        tenant_id = tracing::field::Empty,
        role_id = %filter.role_id,
        has_name_filter = filter.user_name.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
        rows_len = tracing::field::Empty,
        total = tracing::field::Empty,
    ))]
    pub async fn find_unallocated_users_page(
        pool: &PgPool,
        filter: AllocatedUserFilter,
    ) -> anyhow::Result<framework::response::Page<AllocatedUserRow>> {
        let tenant = current_tenant_scope();
        if let Some(t) = tenant.as_deref() {
            tracing::Span::current().record("tenant_id", t);
        }
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT u.user_id, u.user_name, u.nick_name, u.email, \
                    u.phonenumber, u.status, u.create_at \
               FROM sys_user u \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
               LEFT JOIN sys_user_role ur \
                      ON ur.user_id = u.user_id AND ur.role_id = $1 \
             {UNALLOCATED_USER_PAGE_WHERE} \
             ORDER BY u.create_at DESC \
             LIMIT $4 OFFSET $5"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, AllocatedUserRow>(&rows_sql)
                .bind(&filter.role_id)
                .bind(tenant.as_deref())
                .bind(filter.user_name.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "role.find_unallocated_users_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!(
            "SELECT COUNT(*) \
               FROM sys_user u \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
               LEFT JOIN sys_user_role ur \
                      ON ur.user_id = u.user_id AND ur.role_id = $1 \
             {UNALLOCATED_USER_PAGE_WHERE}"
        );
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(&filter.role_id)
                .bind(tenant.as_deref())
                .bind(filter.user_name.as_deref())
                .fetch_one(pool),
            "role.find_unallocated_users_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            tracing::warn!(
                got = rows.len(),
                limit = p.limit,
                "role.find_unallocated_users_page: rows exceeded LIMIT; truncating"
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
                "role.find_unallocated_users_page: slow paginated query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);

        let span = tracing::Span::current();
        span.record("rows_len", rows.len() as u64);
        span.record("total", total);

        Ok(p.into_page(rows, total))
    }

    /// Bulk-assign users to a role via UNNEST. Idempotent: re-submitting
    /// the same user_ids is a no-op thanks to `ON CONFLICT DO NOTHING`
    /// on the `(user_id, role_id)` composite primary key.
    ///
    /// Returns rows actually inserted (excludes conflict-skipped rows).
    /// Empty input is a fast no-op returning `Ok(0)`.
    ///
    /// Note: this method does NOT verify that `user_ids` belong to the
    /// current tenant. Tenant verification lives in the service layer
    /// (or is deferred to Phase 2 — consistent with how role_menu
    /// binding handles menu_id validation).
    #[instrument(skip_all, fields(role_id = %role_id, user_count = user_ids.len()))]
    pub async fn insert_user_roles(
        pool: &PgPool,
        role_id: &str,
        user_ids: &[String],
    ) -> anyhow::Result<u64> {
        if user_ids.is_empty() {
            return Ok(0);
        }
        // Bind order: user_ids=$1 (array, unnested), role_id=$2 (scalar).
        // Intentionally reversed relative to `bulk_insert_role_menus` where
        // the scalar comes first — each helper's SELECT clause picks a
        // different column order, so follow the SQL literally.
        let affected = sqlx::query(
            "INSERT INTO sys_user_role (user_id, role_id) \
             SELECT unnest($1::varchar[]), $2 \
             ON CONFLICT (user_id, role_id) DO NOTHING",
        )
        .bind(user_ids)
        .bind(role_id)
        .execute(pool)
        .await
        .context("insert_user_roles: bulk insert")?
        .rows_affected();
        Ok(affected)
    }

    /// List role_ids bound to a user. Used by the user module's
    /// `GET /system/user/{id}` detail projection and `GET /system/user/auth-role/{id}`.
    ///
    /// Not tenant-scoped at the SQL level because `sys_user_role` has no
    /// tenant column — the caller is expected to have already validated
    /// the user belongs to the current tenant via `user_repo::find_by_id_tenant_scoped`.
    /// Results are sorted by `role_id` for deterministic assertions.
    #[instrument(skip_all, fields(user_id = %user_id))]
    pub async fn find_role_ids_by_user(
        pool: &PgPool,
        user_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT role_id FROM sys_user_role WHERE user_id = $1 ORDER BY role_id")
                .bind(user_id)
                .fetch_all(pool)
                .await
                .context("find_role_ids_by_user")?;
        Ok(rows.into_iter().map(|(r,)| r).collect())
    }

    /// Bulk-unassign users from a role. Returns rows deleted. Empty
    /// input is a fast no-op returning `Ok(0)`. Not tenant-scoped at the
    /// SQL level — service layer validates the role before calling.
    #[instrument(skip_all, fields(role_id = %role_id, user_count = user_ids.len()))]
    pub async fn delete_user_roles(
        pool: &PgPool,
        role_id: &str,
        user_ids: &[String],
    ) -> anyhow::Result<u64> {
        if user_ids.is_empty() {
            return Ok(0);
        }
        let affected = sqlx::query(
            "DELETE FROM sys_user_role \
              WHERE role_id = $1 \
                AND user_id = ANY($2::varchar[])",
        )
        .bind(role_id)
        .bind(user_ids)
        .execute(pool)
        .await
        .context("delete_user_roles: bulk delete")?
        .rows_affected();
        Ok(affected)
    }

    /// Verify all `role_ids` exist in `sys_role` and belong to the current
    /// tenant (or are tenant-less when super-admin context is active).
    /// Returns `Ok(true)` when all ids are valid, `Ok(false)` otherwise.
    /// Used by user service to pre-validate role bindings before INSERT.
    ///
    /// Empty input is trivially `Ok(true)` — no bindings to validate.
    #[instrument(skip_all, fields(role_count = role_ids.len()))]
    pub async fn verify_role_ids_in_tenant(
        pool: &PgPool,
        role_ids: &[String],
    ) -> anyhow::Result<bool> {
        if role_ids.is_empty() {
            return Ok(true);
        }
        let tenant = current_tenant_scope();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_role \
              WHERE role_id = ANY($1::varchar[]) \
                AND del_flag = '0' \
                AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(role_ids)
        .bind(tenant.as_deref())
        .fetch_one(pool)
        .await
        .context("verify_role_ids_in_tenant")?;
        Ok(count as usize == role_ids.len())
    }

    /// Replace a user's role bindings entirely — delete existing rows, then
    /// bulk insert the new list. Caller-provided transaction. Used by both
    /// user create/update and the `PUT /system/user/auth-role` endpoint.
    ///
    /// Empty `role_ids` is a valid "unassign all" operation: the delete
    /// runs, no insert follows. Duplicates in input are deduped via
    /// `SELECT DISTINCT` defense-in-depth against composite PK violations.
    ///
    /// Takes `&mut Transaction` because it performs multiple queries
    /// (DELETE + conditional INSERT). The caller manages begin/commit.
    #[instrument(skip_all, fields(user_id = %user_id, role_count = role_ids.len()))]
    pub async fn replace_user_roles(
        tx: &mut Transaction<'_, Postgres>,
        user_id: &str,
        role_ids: &[String],
    ) -> anyhow::Result<()> {
        sqlx::query("DELETE FROM sys_user_role WHERE user_id = $1")
            .bind(user_id)
            .execute(&mut **tx)
            .await
            .context("replace_user_roles: delete old")?;

        if !role_ids.is_empty() {
            sqlx::query(
                "INSERT INTO sys_user_role (user_id, role_id) \
                 SELECT DISTINCT $1, unnest($2::varchar[])",
            )
            .bind(user_id)
            .bind(role_ids)
            .execute(&mut **tx)
            .await
            .context("replace_user_roles: bulk insert")?;
        }

        Ok(())
    }
}

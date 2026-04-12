//! User / tenant / permission queries.
//!
//! All queries use `sqlx::query_as` (runtime) rather than the compile-time
//! `query_as!` macro so the crate compiles without a live database or a
//! committed `.sqlx/` metadata snapshot. Gate 6 smoke tests cover the real
//! execution path.

use super::entities::{SysUser, SysUserTenant};
use anyhow::Context;
use framework::constants::{PLATFORM_ID_DEFAULT, USER_TYPE_CUSTOM};
use framework::context::{audit_update_by, current_tenant_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use sqlx::{PgPool, Postgres, Transaction};

pub struct UserRepo;

/// Single source of truth for `sys_user` SELECT column lists with the
/// `u.` alias prefix required when JOINing `sys_user_tenant`. Keep in
/// sync with `SysUser` FromRow field order for readability.
const USER_COLUMNS: &str = "\
    u.user_id, u.platform_id, u.dept_id, u.user_name, u.nick_name, \
    u.user_type, u.client_type, u.lang, u.email, u.phonenumber, \
    u.whatsapp, u.sex, u.avatar, u.password, u.status, u.del_flag, \
    u.login_ip, u.login_date, u.create_by, u.create_at, u.update_by, \
    u.update_at, u.remark";

/// Shared WHERE clause for `find_page` rows + count queries.
/// Single source of truth — add a new filter here and both queries
/// pick it up (spec §4.4).
const USER_PAGE_WHERE: &str = "\
    WHERE u.del_flag = '0' \
      AND ut.status = '0' \
      AND ($1::varchar IS NULL OR ut.tenant_id = $1) \
      AND ($2::varchar IS NULL OR u.user_name LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR u.nick_name LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR u.email LIKE '%' || $4 || '%') \
      AND ($5::varchar IS NULL OR u.phonenumber LIKE '%' || $5 || '%') \
      AND ($6::varchar IS NULL OR u.status = $6) \
      AND ($7::varchar IS NULL OR u.dept_id = $7)";

/// Write parameters for `UserRepo::insert_tx`. All fields are owned so
/// the service layer can move DTO fields in without lifetime ceremony.
/// `user_id`, `platform_id`, `user_type`, and audit fields are stamped
/// inside `insert_tx` — not included here.
#[derive(Debug)]
pub struct UserInsertParams {
    pub user_name: String,
    pub nick_name: String,
    pub password_hash: String,
    pub dept_id: Option<String>,
    pub email: String,
    pub phonenumber: String,
    pub sex: String,
    pub avatar: String,
    pub status: String,
    pub remark: Option<String>,
}

/// Write parameters for `UserRepo::update_tx`. Mirrors `UpdateUserDto`
/// minus `role_ids` (roles are written via `RoleRepo::replace_user_roles_tx`,
/// not part of sys_user UPDATE) and minus `user_name` (immutable per
/// NestJS contract).
#[derive(Debug)]
pub struct UserUpdateParams {
    pub user_id: String,
    pub nick_name: String,
    pub email: String,
    pub phonenumber: String,
    pub sex: String,
    pub avatar: String,
    pub status: String,
    pub dept_id: Option<String>,
    pub remark: Option<String>,
}

/// Query filter + pagination for `UserRepo::find_page`. Mirrors
/// `system::user::dto::ListUserDto` but owned at the repo layer
/// (DAO isolation — no upstream DTO dependency in domain).
///
/// `page: PageQuery` carries validator attrs from framework; those only
/// fire at HTTP extraction time. This struct itself doesn't derive
/// `Validate` — the DAO trusts values already vetted by the extractor
/// and re-clamps defensively via `PaginationParams::from`.
#[derive(Debug)]
pub struct UserListFilter {
    pub user_name: Option<String>,
    pub nick_name: Option<String>,
    pub email: Option<String>,
    pub phonenumber: Option<String>,
    pub status: Option<String>,
    pub dept_id: Option<String>,
    pub page: PageQuery,
}

impl UserRepo {
    /// Look up a user by username. Returns only non-deleted rows.
    /// NOT tenant-scoped — used by the Phase 0 auth flow which needs
    /// global user lookup during login.
    #[tracing::instrument(skip_all, fields(username = %username))]
    pub async fn find_by_username(
        pool: &PgPool,
        username: &str,
    ) -> anyhow::Result<Option<SysUser>> {
        let sql = format!(
            "SELECT {USER_COLUMNS} FROM sys_user u \
              WHERE u.user_name = $1 AND u.del_flag = '0' LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysUser>(&sql)
            .bind(username)
            .fetch_optional(pool)
            .await
            .context("find_by_username")?;
        Ok(row)
    }

    /// Fetch a user by primary key. NOT tenant-scoped — used by the Phase 0
    /// auth flow which needs global user lookup during login. Admin CRUD
    /// paths use `find_by_id_tenant_scoped` instead.
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn find_by_id(pool: &PgPool, user_id: &str) -> anyhow::Result<Option<SysUser>> {
        let sql = format!(
            "SELECT {USER_COLUMNS} FROM sys_user u \
              WHERE u.user_id = $1 AND u.del_flag = '0' LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysUser>(&sql)
            .bind(user_id)
            .fetch_optional(pool)
            .await
            .context("find_by_id")?;
        Ok(row)
    }

    /// Tenant-scoped find-by-id for admin CRUD. Joins `sys_user_tenant`
    /// to enforce the current tenant's membership. Returns `None` if
    /// the user doesn't exist, is soft-deleted, or isn't bound to the
    /// current tenant — all treated as "not found" by the service layer
    /// (information hiding).
    ///
    /// Use this for admin endpoints. The existing `find_by_id` (without
    /// tenant scoping) is reserved for the Phase 0 auth flow which needs
    /// global user lookup during login.
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn find_by_id_tenant_scoped(
        pool: &sqlx::PgPool,
        user_id: &str,
    ) -> anyhow::Result<Option<SysUser>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {USER_COLUMNS} \
               FROM sys_user u \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
              WHERE u.user_id = $1 \
                AND u.del_flag = '0' \
                AND ut.status = '0' \
                AND ($2::varchar IS NULL OR ut.tenant_id = $2) \
              LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysUser>(&sql)
            .bind(user_id)
            .bind(tenant.as_deref())
            .fetch_optional(pool)
            .await
            .context("find_by_id_tenant_scoped: select sys_user")?;
        Ok(row)
    }

    /// Paginated list of users in the current tenant. Joins `sys_user_tenant`
    /// for tenant membership + status filter.
    ///
    /// ## Expected indexes
    /// - `sys_user_tenant(user_id)` — JOIN ON
    /// - `sys_user_tenant(tenant_id, status)` — tenant filter
    /// - `sys_user(create_at DESC) WHERE del_flag = '0'` — sort + soft delete
    ///
    /// See `docs/framework-pagination-indexes.md` §1 for the global registry.
    ///
    /// ## Consistency caveats
    /// Offset pagination is not snapshot-consistent. Concurrent insert/delete
    /// between page-N and page-(N+1) fetches may cause duplicate or missing
    /// rows. See `docs/framework-pagination-spec.md` §8.1 (Race A/B/C).
    ///
    /// ## Performance expectation
    /// - Shallow (offset < 1000): < 10ms on 10k-user tenant
    /// - Deep (offset > 10000): up to 500ms on 1M-user tenant — use
    ///   cursor pagination for deep-page use cases (spec §11 Phase 3).
    ///
    /// **Invariant**: callers must ensure `current_tenant_scope()` returns
    /// `Some(...)`. With `tenant=None`, a user bound to multiple active
    /// `sys_user_tenant` rows would produce duplicate result rows because
    /// the JOIN predicate has no tenant_id filter. Phase 1 auth flow always
    /// sets a tenant; Phase 2 admin tooling must not call this via
    /// `run_ignoring_tenant()` without also deduping on `u.user_id`.
    #[tracing::instrument(skip_all, fields(
        tenant_id = tracing::field::Empty,
        has_user_name = filter.user_name.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
        rows_len = tracing::field::Empty,
        total = tracing::field::Empty,
    ))]
    pub async fn find_page(
        pool: &PgPool,
        filter: UserListFilter,
    ) -> anyhow::Result<framework::response::Page<SysUser>> {
        let tenant = current_tenant_scope();
        if let Some(t) = tenant.as_deref() {
            tracing::Span::current().record("tenant_id", t);
        }
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {USER_COLUMNS} FROM sys_user u \
             JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
             {USER_PAGE_WHERE} \
             ORDER BY u.create_at DESC \
             LIMIT $8 OFFSET $9"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysUser>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.user_name.as_deref())
                .bind(filter.nick_name.as_deref())
                .bind(filter.email.as_deref())
                .bind(filter.phonenumber.as_deref())
                .bind(filter.status.as_deref())
                .bind(filter.dept_id.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "user.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!(
            "SELECT COUNT(*) FROM sys_user u \
             JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
             {USER_PAGE_WHERE}"
        );
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.user_name.as_deref())
                .bind(filter.nick_name.as_deref())
                .bind(filter.email.as_deref())
                .bind(filter.phonenumber.as_deref())
                .bind(filter.status.as_deref())
                .bind(filter.dept_id.as_deref())
                .fetch_one(pool),
            "user.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        // Runtime post-condition: if the DB returned more rows than
        // LIMIT, something is wrong. Don't panic — truncate defensively
        // and warn so ops can investigate. (spec §4.3 post-condition)
        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            tracing::warn!(
                got = rows.len(),
                limit = p.limit,
                "user.find_page: rows exceeded LIMIT; truncating"
            );
            rows.truncate(p.limit as usize);
        }

        // Slow-query signal (spec §6.2): 300ms budget, warn above.
        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(
                rows_ms,
                count_ms,
                total_ms,
                budget_ms = SLOW_QUERY_WARN_MS,
                "user.find_page: slow paginated query"
            );
        }

        // Reconcile total under Race B (spec §8.2).
        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);

        let span = tracing::Span::current();
        span.record("rows_len", rows.len() as u64);
        span.record("total", total);

        Ok(p.into_page(rows, total))
    }

    /// Users in the current tenant, active only, for dropdown UI.
    /// Hard cap 500 rows. Optional `user_name` substring search.
    #[tracing::instrument(skip_all, fields(has_name_filter = user_name.is_some()))]
    pub async fn find_option_list(
        pool: &PgPool,
        user_name: Option<&str>,
    ) -> anyhow::Result<Vec<SysUser>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {USER_COLUMNS} \
               FROM sys_user u \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
              WHERE u.del_flag = '0' \
                AND u.status = '0' \
                AND ut.status = '0' \
                AND ($1::varchar IS NULL OR ut.tenant_id = $1) \
                AND ($2::varchar IS NULL OR u.user_name LIKE '%' || $2 || '%') \
              ORDER BY u.user_name ASC \
              LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysUser>(&sql)
            .bind(tenant.as_deref())
            .bind(user_name)
            .fetch_all(pool)
            .await
            .context("find_option_list: select sys_user")?;
        Ok(rows)
    }

    /// Return all `sys_user_tenant` rows for a user (active only).
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn find_user_tenants(
        pool: &PgPool,
        user_id: &str,
    ) -> anyhow::Result<Vec<SysUserTenant>> {
        let sql = r#"
            SELECT id, user_id, tenant_id, is_default, is_admin, status
              FROM sys_user_tenant
             WHERE user_id = $1 AND status = '0'
             ORDER BY is_default DESC
        "#;
        let rows = sqlx::query_as::<_, SysUserTenant>(sql)
            .bind(user_id)
            .fetch_all(pool)
            .await
            .context("find_user_tenants")?;
        Ok(rows)
    }

    /// Resolve permissions for a **non-admin** user by joining
    /// user → role → role-menu → menu.
    ///
    /// NestJS additionally intersects with `SysTenantPackage.menuIds` for the
    /// current tenant — Phase 2 of the Rust port will implement that filter.
    #[tracing::instrument(skip_all, fields(user_id = %user_id, tenant_id = %tenant_id))]
    pub async fn resolve_role_permissions(
        pool: &PgPool,
        user_id: &str,
        tenant_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let sql = r#"
            SELECT DISTINCT m.perms
              FROM sys_menu m
              JOIN sys_role_menu rm ON rm.menu_id = m.menu_id
              JOIN sys_user_role ur ON ur.role_id = rm.role_id
              JOIN sys_role r       ON r.role_id = ur.role_id
             WHERE ur.user_id = $1
               AND r.tenant_id = $2
               AND r.status = '0' AND r.del_flag = '0'
               AND m.status = '0' AND m.del_flag = '0'
               AND m.perms <> ''
        "#;
        let rows: Vec<(String,)> = sqlx::query_as(sql)
            .bind(user_id)
            .bind(tenant_id)
            .fetch_all(pool)
            .await
            .context("resolve_role_permissions")?;
        Ok(rows.into_iter().map(|(p,)| p).collect())
    }

    /// Return every non-empty menu permission in the system. Used for admin
    /// users (NestJS short-circuits role checks when `SysUserTenant.isAdmin
    /// = '1'` and grants the full tenant-package menu range).
    ///
    /// **Phase 0 simplification**: this does NOT intersect with
    /// `SysTenantPackage.menuIds`. Admin users in a restricted-package tenant
    /// would therefore be over-granted. Phase 2 will add the package filter.
    #[tracing::instrument(skip_all)]
    pub async fn resolve_all_menu_perms(pool: &PgPool) -> anyhow::Result<Vec<String>> {
        let sql = r#"
            SELECT DISTINCT perms
              FROM sys_menu
             WHERE status = '0' AND del_flag = '0'
               AND perms <> ''
        "#;
        let rows: Vec<(String,)> = sqlx::query_as(sql)
            .fetch_all(pool)
            .await
            .context("resolve_all_menu_perms")?;
        Ok(rows.into_iter().map(|(p,)| p).collect())
    }

    /// Insert a new sys_user row inside a caller-provided transaction.
    /// The `password` must ALREADY be bcrypt-hashed — this method does
    /// NOT hash. Returns the inserted SysUser.
    ///
    /// Audit fields (`create_by` / `update_by`) come from `AuditInsert::now()`.
    /// `platform_id` is hardcoded `'000000'` (multi-platform deferred).
    /// `user_type` is `'10'` (CUSTOM admin user).
    #[tracing::instrument(skip_all, fields(user_name = %params.user_name))]
    pub async fn insert_tx(
        tx: &mut Transaction<'_, Postgres>,
        params: UserInsertParams,
    ) -> anyhow::Result<SysUser> {
        let audit = AuditInsert::now();
        let user_id = uuid::Uuid::new_v4().to_string();

        // The RETURNING clause uses bare column names (no `u.` alias)
        // because there's no JOIN in this statement. Strip the `u.` prefix
        // from USER_COLUMNS for this single use.
        let plain_columns = USER_COLUMNS.replace("u.", "");

        // platform_id + user_type inlined from domain constants (Phase 1
        // single-platform assumption). When multi-platform lands in Phase 2
        // these become bind parameters pulled from RequestContext.
        let sql = format!(
            "INSERT INTO sys_user (\
                user_id, platform_id, dept_id, user_name, nick_name, user_type, \
                email, phonenumber, whatsapp, sex, avatar, password, status, del_flag, \
                login_ip, create_by, update_by, update_at, remark\
            ) VALUES (\
                $1, '{PLATFORM_ID_DEFAULT}', $2, $3, $4, '{USER_TYPE_CUSTOM}', \
                $5, $6, '', $7, $8, $9, $10, '0', \
                '', $11, $12, CURRENT_TIMESTAMP, $13\
            ) RETURNING {plain_columns}"
        );

        let user = sqlx::query_as::<_, SysUser>(&sql)
            .bind(&user_id)
            .bind(params.dept_id.as_deref())
            .bind(&params.user_name)
            .bind(&params.nick_name)
            .bind(&params.email)
            .bind(&params.phonenumber)
            .bind(&params.sex)
            .bind(&params.avatar)
            .bind(&params.password_hash)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(&mut **tx)
            .await
            .context("insert_tx: insert sys_user")?;

        Ok(user)
    }

    /// Insert a `sys_user_tenant` row binding the user to the current tenant
    /// as a default member (not admin). The `id` column has a gen_random_uuid()
    /// default, so it's not in the INSERT column list.
    ///
    /// Temporary ownership — migrates to tenant_repo when that module lands
    /// (Phase 1 Sub-Phase 5+).
    ///
    /// Requires `current_tenant_scope()` to return `Some` — callers must be
    /// inside a tenant context, not super-tenant bypass.
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn insert_user_tenant_binding_tx(
        tx: &mut Transaction<'_, Postgres>,
        user_id: &str,
    ) -> anyhow::Result<()> {
        let tenant =
            current_tenant_scope().context("insert_user_tenant_binding_tx: tenant_id required")?;
        let audit = AuditInsert::now();
        // is_default='1', is_admin='0', status='0' — all char(1) string literals.
        // update_at has no DB default but is NOT NULL — supply CURRENT_TIMESTAMP.
        // create_by / update_by stamped from AuditInsert to preserve audit trail
        // parity with the sys_user insert in the same transaction.
        sqlx::query(
            "INSERT INTO sys_user_tenant \
                (user_id, tenant_id, is_default, is_admin, status, \
                 create_by, update_by, update_at) \
             VALUES ($1, $2, '1', '0', '0', $3, $4, CURRENT_TIMESTAMP) \
             ON CONFLICT (user_id, tenant_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(&tenant)
        .bind(&audit.create_by)
        .bind(&audit.update_by)
        .execute(&mut **tx)
        .await
        .context("insert_user_tenant_binding_tx")?;
        Ok(())
    }

    /// Update user scalar fields. Tenant guard via EXISTS subquery.
    /// Returns rows_affected — 0 means not found in current tenant.
    /// Does NOT touch `user_name` (immutable per NestJS contract) or
    /// `password` (use `reset_password` for that).
    #[tracing::instrument(skip_all, fields(user_id = %params.user_id))]
    pub async fn update_tx(
        tx: &mut Transaction<'_, Postgres>,
        params: UserUpdateParams,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_user \
                SET nick_name = $1, email = $2, phonenumber = $3, sex = $4, \
                    avatar = $5, status = $6, dept_id = $7, remark = $8, \
                    update_by = $9, update_at = CURRENT_TIMESTAMP \
              WHERE user_id = $10 \
                AND del_flag = '0' \
                AND ($11::varchar IS NULL OR EXISTS (\
                      SELECT 1 FROM sys_user_tenant \
                       WHERE user_id = sys_user.user_id \
                         AND tenant_id = $11 \
                         AND status = '0'\
                    ))",
        )
        .bind(&params.nick_name)
        .bind(&params.email)
        .bind(&params.phonenumber)
        .bind(&params.sex)
        .bind(&params.avatar)
        .bind(&params.status)
        .bind(params.dept_id.as_deref())
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.user_id)
        .bind(tenant.as_deref())
        .execute(&mut **tx)
        .await
        .context("update_tx: update sys_user")?
        .rows_affected();

        Ok(affected)
    }

    /// Flip user status with tenant + soft-delete guards. Returns
    /// rows_affected — 0 means not found in current tenant.
    #[tracing::instrument(skip_all, fields(user_id = %user_id, status = %status))]
    pub async fn change_status(pool: &PgPool, user_id: &str, status: &str) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_user \
                SET status = $1, update_by = $2, update_at = CURRENT_TIMESTAMP \
              WHERE user_id = $3 \
                AND del_flag = '0' \
                AND ($4::varchar IS NULL OR EXISTS (\
                      SELECT 1 FROM sys_user_tenant \
                       WHERE user_id = sys_user.user_id \
                         AND tenant_id = $4 \
                         AND status = '0'\
                    ))",
        )
        .bind(status)
        .bind(&updater)
        .bind(user_id)
        .bind(tenant.as_deref())
        .execute(pool)
        .await
        .context("change_status: update sys_user")?
        .rows_affected();
        Ok(affected)
    }

    /// Soft-delete a user (sets `del_flag = '1'`). Tenant guard via EXISTS.
    /// Returns rows_affected — 0 means not found in current tenant.
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn soft_delete_by_id(pool: &PgPool, user_id: &str) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_user \
                SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
              WHERE user_id = $2 \
                AND del_flag = '0' \
                AND ($3::varchar IS NULL OR EXISTS (\
                      SELECT 1 FROM sys_user_tenant \
                       WHERE user_id = sys_user.user_id \
                         AND tenant_id = $3 \
                         AND status = '0'\
                    ))",
        )
        .bind(&updater)
        .bind(user_id)
        .bind(tenant.as_deref())
        .execute(pool)
        .await
        .context("soft_delete_by_id: update sys_user")?
        .rows_affected();
        Ok(affected)
    }

    /// Update a user's password hash. Tenant guard via EXISTS subquery.
    /// Returns rows_affected — 0 means not found in current tenant.
    /// The `password_hash` must ALREADY be bcrypt-hashed by the caller.
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn reset_password(
        pool: &PgPool,
        user_id: &str,
        password_hash: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_user \
                SET password = $1, update_by = $2, update_at = CURRENT_TIMESTAMP \
              WHERE user_id = $3 \
                AND del_flag = '0' \
                AND ($4::varchar IS NULL OR EXISTS (\
                      SELECT 1 FROM sys_user_tenant \
                       WHERE user_id = sys_user.user_id \
                         AND tenant_id = $4 \
                         AND status = '0'\
                    ))",
        )
        .bind(password_hash)
        .bind(&updater)
        .bind(user_id)
        .bind(tenant.as_deref())
        .execute(pool)
        .await
        .context("reset_password: update sys_user")?
        .rows_affected();
        Ok(affected)
    }

    /// Returns true if `user_name` is unused in `sys_user`. Platform-wide
    /// (not tenant-scoped) because the unique index `sys_user_user_name_key`
    /// is unconditional.
    ///
    /// **Important**: does NOT filter `del_flag='0'`. The DB unique index
    /// considers soft-deleted rows too, so filtering here would produce a
    /// false "available" answer that later hits a unique-violation → 500
    /// on INSERT. Reusing a soft-deleted username is blocked — consistent
    /// with NestJS semantics and DB reality.
    #[tracing::instrument(skip_all, fields(user_name = %user_name))]
    pub async fn verify_user_name_unique(pool: &PgPool, user_name: &str) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sys_user WHERE user_name = $1")
            .bind(user_name)
            .fetch_one(pool)
            .await
            .context("verify_user_name_unique")?;
        Ok(count == 0)
    }

    /// Returns true if `user_id` corresponds to the system super-admin row
    /// (`user_name = SUPER_ADMIN_USERNAME AND platform_id = PLATFORM_ID_DEFAULT`).
    /// Used by guard checks across write endpoints to block operations on
    /// the superuser.
    ///
    /// NOT tenant-scoped — the super admin is platform-scoped identity,
    /// the check must work regardless of which tenant the caller is
    /// currently scoped to.
    #[tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn is_super_admin(pool: &PgPool, user_id: &str) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_user \
              WHERE user_id = $1 \
                AND user_name = $2 \
                AND platform_id = $3 \
                AND del_flag = '0'",
        )
        .bind(user_id)
        .bind(crate::domain::constants::SUPER_ADMIN_USERNAME)
        .bind(PLATFORM_ID_DEFAULT)
        .fetch_one(pool)
        .await
        .context("is_super_admin")?;
        Ok(count > 0)
    }
}

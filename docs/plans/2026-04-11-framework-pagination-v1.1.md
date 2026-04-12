# Framework Pagination v1.1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the pagination framework from v1.0 (structural compliance) to v1.1 (runtime safety + observability) by adding query timeout, slow-query warnings, runtime post-condition recovery, total-consistency repair, i18n parameterized validation errors, and index/plan assertion test helpers — all without breaking the wire contract.

**Architecture:** v1.0 established the normative types and structure. v1.1 layers runtime guarantees on top: bounded query duration (hard timeout), user-visible slow-query signal (tracing warn), defensive rows-truncation when post-conditions fail (fail-safe instead of debug-panic), `total = max(total, offset + rows.len())` to repair Race B self-contradiction, validator error messages that include the actual bound (instead of generic "out of range"), and two test helpers (`assert_indexes_exist`, `check_no_seq_scan`) that future tests can use to catch DDL regressions.

**Tech Stack:** Rust, axum 0.8, sqlx 0.8 (runtime queries), tokio 1.x, tracing 0.1, validator 0.20, anyhow, serde_json (for EXPLAIN plan parsing).

**Baseline assumption:** All v1.0 changes are already merged. Current test count is 153 passing. Run `cd server-rs && cargo test --workspace 2>&1 | grep "test result"` to confirm before starting. If baseline is different, stop and investigate.

**Spec reference:** `docs/framework/framework-pagination-spec.md` — read §6.2, §7.1, §8.2 for the v1.1 deferral notes.

**Git policy:** per standing user preference, no automatic `git commit` steps. The implementer (subagent or inline executor) **must not** run any git commands. The user will stage and commit manually at the end of each task or batch, using their own discretion about message and scope. Any step that previously said "commit" is now "verify the change compiles + tests pass, then hand back to the user for manual commit".

---

### Task 1: Framework constants + timeout helper

**Files:**
- Modify: `server-rs/crates/framework/src/response/pagination.rs`
- Test: same file (`#[cfg(test)] mod tests`)

Adds two new `pub const` values plus a generic `with_timeout` helper. The helper takes any future returning `Result<T, E: Into<anyhow::Error>>` and wraps it in `tokio::time::timeout`, converting timeout elapsed to a descriptive `anyhow::Error`. It's placed in `pagination.rs` because pagination is its sole v1.1 user; if adoption grows, move to `framework/src/infra/`.

- [ ] **Step 1.1: Write failing test for `with_timeout` happy path and timeout path**

Add to `framework/src/response/pagination.rs` `mod tests`:

```rust
    #[tokio::test]
    async fn with_timeout_returns_ok_when_future_completes_in_time() {
        let fut = async { Ok::<_, anyhow::Error>(42_i64) };
        let result = with_timeout(fut, "test.happy").await.unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn with_timeout_returns_err_when_future_exceeds_budget() {
        let fut = async {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            Ok::<_, anyhow::Error>(0_i64)
        };
        // Use a tiny override so the test finishes quickly. The helper
        // itself reads QUERY_TIMEOUT_SECS, so for testability we expose
        // a `with_timeout_for` variant that accepts an explicit Duration.
        let result = with_timeout_for(
            fut,
            std::time::Duration::from_millis(50),
            "test.slow",
        )
        .await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("timeout"),
            "expected timeout error, got: {}",
            err
        );
        assert!(err.to_string().contains("test.slow"));
    }

    #[tokio::test]
    async fn with_timeout_propagates_inner_error() {
        let fut = async {
            Err::<i64, _>(anyhow::anyhow!("inner failure"))
        };
        let err = with_timeout(fut, "test.inner").await.unwrap_err();
        assert!(err.to_string().contains("inner failure"));
    }
```

- [ ] **Step 1.2: Run tests to verify they fail**

```bash
cd server-rs && cargo test -p framework with_timeout 2>&1 | tail -20
```

Expected: fails with "cannot find function `with_timeout`" and "`with_timeout_for`".

- [ ] **Step 1.3: Add constants + implementation**

Append to `framework/src/response/pagination.rs` after the existing `PAGE_*` constants:

```rust
// ──────────────────────────────────────────────────────────────────────
// Query duration policy (v1.1).
// ──────────────────────────────────────────────────────────────────────

/// Hard timeout for each individual DB query in a paginated flow.
/// Applied via `with_timeout` — queries exceeding this bound return an
/// `anyhow::Error` without waiting for the DB to finish.
pub const QUERY_TIMEOUT_SECS: u64 = 5;

/// Threshold above which a paginated query (rows_ms + count_ms) emits a
/// `tracing::warn!` — signals to operators that the query is drifting
/// toward timeout territory even though it still returns in time.
pub const SLOW_QUERY_WARN_MS: u128 = 300;
```

Add the helper functions at the end of `impl PaginationParams` or at module scope (module scope is cleaner, put after the impl block):

```rust
/// Wrap a fallible async operation with the framework's default query
/// timeout (`QUERY_TIMEOUT_SECS`). On timeout, returns a descriptive
/// `anyhow::Error` carrying the `ctx` label; on inner error, propagates
/// the error up with the `ctx` as anyhow context.
///
/// ```ignore
/// let rows = with_timeout(
///     sqlx::query_as::<_, SysUser>(sql).bind(...).fetch_all(pool),
///     "user.find_page rows",
/// ).await?;
/// ```
pub async fn with_timeout<T, E, F>(fut: F, ctx: &'static str) -> anyhow::Result<T>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: Into<anyhow::Error>,
{
    with_timeout_for(
        fut,
        std::time::Duration::from_secs(QUERY_TIMEOUT_SECS),
        ctx,
    )
    .await
}

/// Variant of `with_timeout` accepting an explicit duration — used by
/// tests that need a tiny budget to exercise the timeout path. Production
/// code should always use `with_timeout` so the policy stays centralized.
pub async fn with_timeout_for<T, E, F>(
    fut: F,
    budget: std::time::Duration,
    ctx: &'static str,
) -> anyhow::Result<T>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: Into<anyhow::Error>,
{
    match tokio::time::timeout(budget, fut).await {
        Ok(Ok(val)) => Ok(val),
        Ok(Err(e)) => Err(e.into()).map_err(|inner: anyhow::Error| inner.context(ctx)),
        Err(_elapsed) => Err(anyhow::anyhow!(
            "{}: query timeout after {:?}",
            ctx,
            budget
        )),
    }
}
```

- [ ] **Step 1.4: Run tests to verify they pass**

```bash
cd server-rs && cargo test -p framework with_timeout 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 1.5: Verify the full framework test suite still passes + clippy clean**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
cd server-rs && cargo clippy -p framework --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: framework tests go from 69 to 72 (69 + 3 new timeout tests), clippy clean.

- [ ] **Step 1.6: Report Task 1 complete**

Report back: constants defined, helper compiles, 3 new tests pass, framework test count 69 → 72, clippy clean. Await user's commit decision before proceeding to Task 2.

---

### Task 2: Total-consistency helper

**Files:**
- Modify: `server-rs/crates/framework/src/response/pagination.rs`
- Test: same file

Spec §8.2: under Race B (row deleted between `SELECT rows` and `SELECT COUNT`), we can observe `total < rows.len() + offset`, which the client sees as "Page 3 of 2" nonsense. Add a helper that clamps `total` upward to at least `offset + rows.len()`.

- [ ] **Step 2.1: Write failing test**

Add to `framework/src/response/pagination.rs` `mod tests`:

```rust
    #[test]
    fn reconcile_total_keeps_larger_value() {
        // Happy path: actual total > observed rows + offset
        assert_eq!(PaginationParams::reconcile_total(100, 20, 10), 100);
    }

    #[test]
    fn reconcile_total_bumps_up_when_race_shrunk_it() {
        // Race B: COUNT ran AFTER a delete, but rows query saw the deleted row
        // total=5 but we actually returned rows 40..=49 at offset=40
        // Client should see total=50, not 5 (which would imply empty page)
        assert_eq!(PaginationParams::reconcile_total(5, 10, 40), 50);
    }

    #[test]
    fn reconcile_total_handles_negative_input() {
        // Defensive: if DB returns a negative count (should never happen),
        // reconcile should not propagate the negative.
        assert_eq!(PaginationParams::reconcile_total(-5, 3, 10), 13);
    }
```

- [ ] **Step 2.2: Run tests to verify failure**

```bash
cd server-rs && cargo test -p framework reconcile_total 2>&1 | tail -15
```

Expected: fails with "no associated function `reconcile_total`".

- [ ] **Step 2.3: Implement `reconcile_total`**

Add to `impl PaginationParams` block in `framework/src/response/pagination.rs`:

```rust
    /// Repair `total` when a Race B (row deleted between rows and count
    /// queries) caused `total < offset + rows.len()` — a self-contradictory
    /// state that makes the client see "Page N of fewer-than-N". Clamps
    /// `total` upward so the arithmetic is consistent.
    ///
    /// Static helper: takes raw values so it can be unit-tested without
    /// materializing a `PaginationParams` instance (which would require
    /// the request context).
    pub fn reconcile_total(observed_total: i64, rows_len: usize, offset: i64) -> i64 {
        let lower_bound = offset.saturating_add(rows_len as i64);
        observed_total.max(lower_bound)
    }
```

- [ ] **Step 2.4: Run tests to verify pass**

```bash
cd server-rs && cargo test -p framework reconcile_total 2>&1 | tail -10
```

Expected: 3 passed.

- [ ] **Step 2.5: Verify framework suite + clippy**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
cd server-rs && cargo clippy -p framework --all-targets -- -D warnings 2>&1 | tail -3
```

Expected: framework tests at 75 (72 + 3 new reconcile tests), clippy clean.

- [ ] **Step 2.6: Report Task 2 complete**

Report back: reconcile_total implemented, 3 new tests pass, framework test count 72 → 75, clippy clean.

---

### Task 3: user_repo::find_page v1.1 compliance

**Files:**
- Modify: `server-rs/crates/modules/src/domain/user_repo.rs`
- Test: existing integration tests in `crates/modules/tests/user_module_tests.rs` cover correctness

Apply four v1.1 upgrades in one pass:
1. Wrap both DB queries with `with_timeout`
2. Time each query separately; emit `tracing::warn!` if `rows_ms + count_ms > SLOW_QUERY_WARN_MS`
3. Upgrade `debug_assert!` post-condition to runtime check that truncates + warns (never panics in production)
4. Apply `reconcile_total` before constructing `Page<T>`

- [ ] **Step 3.1: Update imports**

Edit `crates/modules/src/domain/user_repo.rs`:

```rust
use framework::response::{
    with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS,
};
```

(Replaces the existing `use framework::response::{PageQuery, PaginationParams};` line.)

- [ ] **Step 3.2: Replace `find_page` body**

In `crates/modules/src/domain/user_repo.rs`, replace the entire body of `find_page` (the method signature and doc comment stay unchanged; only what's between `let tenant = current_tenant_scope();` and the final `Ok(p.into_page(...))` changes).

New body:

```rust
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

        // Runtime post-condition: if the DB returned more rows than the
        // LIMIT, something is wrong (bad SQL, planner bug, connection reuse).
        // Don't panic — truncate defensively and warn so ops can investigate.
        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            tracing::warn!(
                got = rows.len(),
                limit = p.limit,
                "user.find_page: rows exceeded LIMIT; truncating"
            );
            rows.truncate(p.limit as usize);
        }

        // Slow-query signal (spec §6.2): total budget 300ms, warn above.
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
```

- [ ] **Step 3.3: Compile-check**

```bash
cd server-rs && cargo check -p modules 2>&1 | tail -10
```

Expected: clean compile.

- [ ] **Step 3.4: Run existing user integration tests**

```bash
cd server-rs && cargo test -p modules --test user_module_tests 2>&1 | grep "test result"
```

Expected: 25 passed (unchanged).

- [ ] **Step 3.5: Report Task 3 complete**

Report back: user_repo::find_page v1.1-compliant (timeout + slow warn + truncate + reconcile), 25 user integration tests still pass, clippy clean.

---

### Task 4: role_repo::find_page v1.1 compliance

**Files:**
- Modify: `server-rs/crates/modules/src/domain/role_repo.rs`
- Test: existing `crates/modules/tests/role_module_tests.rs` covers correctness

Identical transformation to Task 3, applied to `role_repo::find_page`. Repo-local context labels change from `"user.find_page rows"` to `"role.find_page rows"`.

- [ ] **Step 4.1: Update imports**

Edit `crates/modules/src/domain/role_repo.rs` — change:

```rust
use framework::response::{PageQuery, PaginationParams};
```

to:

```rust
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
```

- [ ] **Step 4.2: Replace `find_page` body**

In `crates/modules/src/domain/role_repo.rs`, replace the body of `find_page` (doc comment + `#[instrument(...)]` stay unchanged):

```rust
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
```

- [ ] **Step 4.3: Compile-check + run role integration tests**

```bash
cd server-rs && cargo check -p modules 2>&1 | tail -5
cd server-rs && cargo test -p modules --test role_module_tests 2>&1 | grep "test result"
```

Expected: clean compile, 23 passed (unchanged).

- [ ] **Step 4.4: Report Task 4 complete**

Report back: role_repo::find_page v1.1-compliant, 23 role integration tests still pass, clippy clean.

---

### Task 5: role_repo::find_allocated_users_page v1.1 compliance

**Files:**
- Modify: `server-rs/crates/modules/src/domain/role_repo.rs` (same file as Task 4)
- Test: existing role integration tests cover correctness

Same pattern as Task 3/4 applied to `find_allocated_users_page`.

- [ ] **Step 5.1: Replace `find_allocated_users_page` body**

Replace the body between the `#[instrument(...)]` attribute and the closing brace of `find_allocated_users_page`:

```rust
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
```

- [ ] **Step 5.2: Compile-check**

```bash
cd server-rs && cargo check -p modules 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 5.3: Report Task 5 complete**

Report back: find_allocated_users_page v1.1-compliant, clippy clean.

---

### Task 6: role_repo::find_unallocated_users_page v1.1 compliance

**Files:**
- Modify: `server-rs/crates/modules/src/domain/role_repo.rs`

Same pattern applied to the anti-join variant.

- [ ] **Step 6.1: Replace `find_unallocated_users_page` body**

Replace the body between `#[instrument(...)]` and the closing brace:

```rust
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
```

- [ ] **Step 6.2: Verify full workspace compiles + all tests pass**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cd server-rs && cargo fmt --check && echo "fmt ok"
```

Expected: 
- all 4 test binaries pass (3 + 75 + 33 + 23 + 25 = 159 total after Tasks 1-2 added 6 framework tests)
- clippy clean
- fmt ok

- [ ] **Step 6.3: Report Tasks 3-6 batch complete**

Report back: all 4 find_page methods v1.1-compliant, full workspace 159 passed (153 + 6 from Tasks 1+2), clippy clean, fmt ok.

---

### Task 7: i18n parameterized validation error messages

**Files:**
- Modify: `server-rs/crates/framework/src/error/app_error.rs`
- Modify: `server-rs/crates/framework/src/i18n/mod.rs`
- Modify: `server-rs/i18n/zh-CN.json`
- Modify: `server-rs/i18n/en-US.json`
- Test: new test in `crates/framework/src/error/app_error.rs`

Spec §7.1 v1 limitation: `AppError::Validation` ignores validator params like `{min}/{max}`. v1.1 fixes this by extracting `ValidationError.params` (a `HashMap<Cow<'static, str>, serde_json::Value>`) and folding them into the i18n substitution map.

- [ ] **Step 7.1: Read current app_error Validation path**

```bash
cd server-rs && cat crates/framework/src/error/app_error.rs | head -200
```

Find the `impl IntoResponse for AppError` match arm for `Validation`. Note the current FieldError.message→i18n-key lookup path.

- [ ] **Step 7.2: Write failing test**

Add to `crates/framework/src/error/app_error.rs` test module:

```rust
    #[test]
    fn validation_error_substitutes_min_max_params_from_validator() {
        use validator::ValidationError;
        use std::borrow::Cow;

        // Construct a FieldError carrying validator params (as if from a
        // #[validate(range(min = 1, max = 200))] violation).
        let mut ve = ValidationError::new("range");
        ve.add_param(Cow::Borrowed("min"), &1_i64);
        ve.add_param(Cow::Borrowed("max"), &200_i64);
        ve.add_param(Cow::Borrowed("value"), &500_i64);

        let mut errors = validator::ValidationErrors::new();
        errors.add("pageSize", ve);

        let app_err = AppError::Validation(errors);

        // Render the error — we care that the body JSON has a message
        // with the actual min/max substituted, not a literal "{min}".
        let response = app_err.into_response();
        let body = response_body_to_string_blocking(response);
        assert!(
            body.contains("1") && body.contains("200"),
            "expected min=1 and max=200 in message body, got: {}",
            body
        );
        assert!(
            !body.contains("{min}") && !body.contains("{max}"),
            "placeholders should have been substituted, body: {}",
            body
        );
    }

    // Helper for the test: drain the response body into a String.
    // This is a test-only helper, placed inline so it doesn't leak
    // into the crate's public surface.
    fn response_body_to_string_blocking(
        resp: axum::response::Response,
    ) -> String {
        use http_body_util::BodyExt;
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (_, body) = resp.into_parts();
            let bytes = body.collect().await.unwrap().to_bytes();
            String::from_utf8(bytes.to_vec()).unwrap()
        })
    }
```

- [ ] **Step 7.3: Run test to verify failure**

```bash
cd server-rs && cargo test -p framework validation_error_substitutes 2>&1 | tail -30
```

Expected: fails with either "placeholders should have been substituted" OR "expected min=1" (depending on current message content).

- [ ] **Step 7.4: Update i18n JSON files**

Edit `server-rs/i18n/zh-CN.json` — change the `valid.range` entry:

```json
  "valid.range": "字段值超出允许范围（应在 {min} 到 {max} 之间）",
```

Edit `server-rs/i18n/en-US.json`:

```json
  "valid.range": "Value out of allowed range (expected between {min} and {max})",
```

- [ ] **Step 7.5: Add `get_by_key_with_json_params` i18n helper**

Currently `get_by_key` takes a raw i18n key and returns the pre-substituted message. Validator gives us `HashMap<Cow<'static, str>, serde_json::Value>` — need to stringify and substitute. Add a variant that accepts the JSON params.

Add to `framework/src/i18n/mod.rs`:

```rust
/// Variant of `get_by_key` accepting validator-style params
/// (`HashMap<Cow<'static, str>, serde_json::Value>`). Unwraps JSON
/// numbers and strings into their raw string form before substitution,
/// so `{min}` becomes `1` rather than `1.0` or `"1"`. Used by the
/// `AppError::Validation → IntoResponse` path.
pub fn get_by_key_with_json_params(
    key: &str,
    lang: &str,
    params: &std::collections::HashMap<
        std::borrow::Cow<'static, str>,
        serde_json::Value,
    >,
) -> Option<String> {
    let raw = get_by_key(key, lang)?;
    if params.is_empty() {
        return Some(raw);
    }
    let mut out = raw;
    for (k, v) in params {
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            other => other.to_string(),
        };
        out = out.replace(&format!("{{{}}}", k.as_ref()), &s);
    }
    Some(out)
}
```

**Do not** add a `get_message_with_json_params` variant (ResponseCode-based). It would have zero callers today — validation errors use string keys, not ResponseCode numeric keys. Add it if/when a caller materializes.

- [ ] **Step 7.6: Wire into `AppError::Validation → IntoResponse`**

Edit `crates/framework/src/error/app_error.rs`. Find the Validation branch in the match. Change the per-FieldError message resolution to use `get_by_key_with_json_params` with the FieldError's `params` map.

Exact change: locate the loop that builds `errors` (the response field-errors list). For each `FieldError`, instead of calling `get_by_key(&field_error.message, lang)`, call:

```rust
let msg = framework::i18n::get_by_key_with_json_params(
    &field_error.message,  // this is the validator's "code" like "range"
    lang,
    &field_error.params,
)
.unwrap_or_else(|| {
    tracing::warn!(
        key = %field_error.message,
        "missing i18n entry for validation error; falling back to raw key"
    );
    field_error.message.to_string()
});
```

**Important**: the existing code may look up the i18n key as `"valid.{code}"` not just `"{code}"`. Follow the existing convention — just add the params map alongside.

Run to confirm the actual shape:

```bash
cd server-rs && grep -n "valid\." crates/framework/src/error/app_error.rs
```

If the code constructs `format!("valid.{}", code)`, keep that prefix:

```rust
let i18n_key = format!("valid.{}", field_error.code);
let msg = framework::i18n::get_by_key_with_json_params(&i18n_key, lang, &field_error.params)
    .unwrap_or_else(|| format!("[{}]", i18n_key));
```

- [ ] **Step 7.7: Run the new test to verify pass**

```bash
cd server-rs && cargo test -p framework validation_error_substitutes 2>&1 | tail -10
```

Expected: passes.

- [ ] **Step 7.8: Run full framework test suite + existing i18n tests**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
cd server-rs && cargo test -p framework i18n 2>&1 | tail -10
```

Expected: framework tests at 76 (75 after Tasks 1+2 + 1 new validation_error_substitutes test from Step 7.2). All existing i18n tests still pass.

- [ ] **Step 7.9: Add targeted i18n test for the new helper**

Add to `crates/framework/src/i18n/mod.rs` test module:

```rust
    #[test]
    fn get_by_key_with_json_params_substitutes_json_numbers() {
        use std::borrow::Cow;
        let mut params = std::collections::HashMap::new();
        params.insert(Cow::Borrowed("min"), serde_json::json!(1));
        params.insert(Cow::Borrowed("max"), serde_json::json!(200));

        let got = get_by_key_with_json_params("valid.range", "zh-CN", &params).unwrap();
        assert!(got.contains("1"));
        assert!(got.contains("200"));
        assert!(!got.contains("{min}"));
        assert!(!got.contains("{max}"));
    }

    #[test]
    fn get_by_key_with_json_params_no_params_returns_raw() {
        let params = std::collections::HashMap::new();
        let got = get_by_key_with_json_params("200", "zh-CN", &params).unwrap();
        assert_eq!(got, "操作成功");
    }
```

- [ ] **Step 7.10: Run i18n tests to verify pass**

```bash
cd server-rs && cargo test -p framework i18n 2>&1 | tail -10
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
```

Expected: framework tests at 78 (76 + 2 new i18n helper tests from Step 7.9).

- [ ] **Step 7.11: Report Task 7 complete**

Report back: i18n validation errors parameterized, framework test count 75 → 78, clippy clean.

---

### Task 8: `check_no_seq_scan` pure helper

**Files:**
- Create: `server-rs/crates/framework/src/testing/mod.rs`
- Create: `server-rs/crates/framework/src/testing/explain_plan.rs`
- Modify: `server-rs/crates/framework/src/lib.rs` (add `pub mod testing;`)
- Test: inline in `explain_plan.rs`

A pure function that walks a Postgres `EXPLAIN (FORMAT JSON)` output tree and asserts no `Seq Scan` nodes reference non-exempt tables. Takes `&serde_json::Value` so it's fully testable with fixture JSON and doesn't require a live DB. Future integration tests will use it once real seed data + indexes are in place.

The module is placed under `framework::testing::*` as public (not feature-gated) because test helpers shipping in the prod binary are zero-cost — they're only linked when referenced.

- [ ] **Step 8.1: Create the testing module file**

Create `server-rs/crates/framework/src/testing/mod.rs`:

```rust
//! Test helpers for downstream crates. This module is `pub` because
//! cargo's test/integration layout makes feature-gated test helpers
//! painful to share across crates — the helpers here are pure functions
//! with zero runtime cost if unused, so they ship in the prod binary
//! without concern.
//!
//! See `docs/framework/framework-pagination-spec.md` §5 and `v1.1 Phase` for the
//! intended usage of these helpers.

pub mod explain_plan;
```

- [ ] **Step 8.2: Create the explain_plan helper with failing test**

Create `server-rs/crates/framework/src/testing/explain_plan.rs`:

```rust
//! Postgres `EXPLAIN (FORMAT JSON)` plan assertion helpers.
//!
//! These functions take `&serde_json::Value` (the decoded plan JSON) and
//! walk the node tree to check specific properties. They are intentionally
//! pure — no sqlx dependency, no async — so they can be unit-tested with
//! fixture JSON and reused from any crate.

use serde_json::Value;

/// Walk a Postgres `EXPLAIN (FORMAT JSON)` plan tree and return `Err`
/// if any node's `"Node Type"` is `"Seq Scan"` on a table not in the
/// `exempt_tables` allowlist. Small tables (like dictionaries, enum
/// lookups, or single-tenant config rows) typically want to be exempt
/// because seq scan is actually the optimal plan for <1k-row tables.
///
/// The plan_root argument is the raw JSON returned by
/// `SELECT json_agg(plan_array) FROM ... EXPLAIN`, which Postgres wraps
/// in an outer array with a single `{"Plan": {...}}` element.
///
/// # Example
///
/// ```ignore
/// let plan_str: String = sqlx::query_scalar(
///     "EXPLAIN (FORMAT JSON) SELECT ... FROM sys_user u WHERE ...",
/// )
/// .bind(...)
/// .fetch_one(&pool)
/// .await?;
/// let plan: serde_json::Value = serde_json::from_str(&plan_str)?;
/// check_no_seq_scan(&plan, &["sys_dict", "sys_config"])?;
/// ```
pub fn check_no_seq_scan(plan_root: &Value, exempt_tables: &[&str]) -> Result<(), String> {
    let root_node = plan_root
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|obj| obj.get("Plan"))
        .ok_or_else(|| {
            "unexpected EXPLAIN (FORMAT JSON) output shape — \
             expected outer array with .Plan element"
                .to_string()
        })?;
    walk_plan_node(root_node, exempt_tables)
}

fn walk_plan_node(node: &Value, exempt: &[&str]) -> Result<(), String> {
    let node_type = node
        .get("Node Type")
        .and_then(|v| v.as_str())
        .unwrap_or("<unknown>");

    if node_type == "Seq Scan" {
        let relation = node
            .get("Relation Name")
            .and_then(|v| v.as_str())
            .unwrap_or("<unknown>");
        if !exempt.contains(&relation) {
            return Err(format!(
                "seq scan detected on non-exempt table: {} \
                 (exempt list: {:?})",
                relation, exempt
            ));
        }
    }

    if let Some(subplans) = node.get("Plans").and_then(|v| v.as_array()) {
        for sub in subplans {
            walk_plan_node(sub, exempt)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn flat_index_scan_passes() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Index Scan",
                "Relation Name": "sys_user",
                "Plans": []
            }
        }]);
        assert!(check_no_seq_scan(&plan, &[]).is_ok());
    }

    #[test]
    fn flat_seq_scan_on_non_exempt_fails() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "sys_user"
            }
        }]);
        let err = check_no_seq_scan(&plan, &[]).unwrap_err();
        assert!(err.contains("sys_user"));
        assert!(err.contains("seq scan"));
    }

    #[test]
    fn seq_scan_on_exempt_table_passes() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Seq Scan",
                "Relation Name": "sys_dict"
            }
        }]);
        assert!(check_no_seq_scan(&plan, &["sys_dict"]).is_ok());
    }

    #[test]
    fn nested_seq_scan_inside_hash_join_fails() {
        // Typical JOIN plan: outer Hash Join with inner seq scan on a
        // non-exempt table. The recursive walk must catch this.
        let plan = json!([{
            "Plan": {
                "Node Type": "Hash Join",
                "Plans": [
                    {
                        "Node Type": "Index Scan",
                        "Relation Name": "sys_user_tenant"
                    },
                    {
                        "Node Type": "Hash",
                        "Plans": [
                            {
                                "Node Type": "Seq Scan",
                                "Relation Name": "sys_user"
                            }
                        ]
                    }
                ]
            }
        }]);
        let err = check_no_seq_scan(&plan, &[]).unwrap_err();
        assert!(err.contains("sys_user"));
    }

    #[test]
    fn nested_seq_scan_on_exempt_table_passes() {
        let plan = json!([{
            "Plan": {
                "Node Type": "Nested Loop",
                "Plans": [
                    {
                        "Node Type": "Seq Scan",
                        "Relation Name": "sys_dict"
                    },
                    {
                        "Node Type": "Index Scan",
                        "Relation Name": "sys_user"
                    }
                ]
            }
        }]);
        assert!(check_no_seq_scan(&plan, &["sys_dict"]).is_ok());
    }

    #[test]
    fn unexpected_shape_returns_descriptive_error() {
        let plan = json!({"not": "an array"});
        let err = check_no_seq_scan(&plan, &[]).unwrap_err();
        assert!(err.contains("unexpected EXPLAIN"));
    }
}
```

- [ ] **Step 8.3: Wire into framework lib**

Edit `server-rs/crates/framework/src/lib.rs` — add at the top-level module list:

```rust
pub mod testing;
```

Place it alphabetically near `pub mod response;`.

- [ ] **Step 8.4: Run tests to verify pass**

```bash
cd server-rs && cargo test -p framework explain_plan 2>&1 | tail -15
```

Expected: 6 passed.

- [ ] **Step 8.5: Verify full framework suite + clippy**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
cd server-rs && cargo clippy -p framework --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: framework tests at 84 (78 after Task 7 + 6 new explain_plan tests), clippy clean.

- [ ] **Step 8.6: Report Task 8 complete**

Report back: `check_no_seq_scan` helper implemented, 6 unit tests pass with fixture JSON, framework test count 78 → 84, clippy clean.

---

### Task 9: `assert_indexes_exist` helper

**Files:**
- Create: `server-rs/crates/framework/src/testing/pg_catalog.rs`
- Modify: `server-rs/crates/framework/src/testing/mod.rs` (add submodule)
- Test: deferred — needs live DB, tests belong in integration tests, not unit tests

An async helper that queries `pg_catalog.pg_indexes` for the existence of named indexes on named tables. Integration tests in the modules crate will use it to assert that migration files actually created the indexes the `find_page` doc comments promise.

For v1.1, this helper ships but **is not wired into any test** — the indexes themselves don't yet exist, so asserting them would fail. Once migrations land (v1.2), integration tests will reference this helper.

- [ ] **Step 9.1: Create pg_catalog helper**

Create `server-rs/crates/framework/src/testing/pg_catalog.rs`:

```rust
//! Postgres system catalog queries for integration tests.
//!
//! Primary use: assert that expected indexes listed in
//! `docs/framework/framework-pagination-indexes.md` actually exist in the DB
//! after migrations have run. Called from integration tests — not a
//! runtime check — so the async signature consumes an existing
//! `sqlx::PgPool`.

use sqlx::PgPool;

/// Row returned by `pg_indexes` queries. Public so callers can inspect
/// detailed columns if needed (e.g. for generating diagnostic reports).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IndexRow {
    pub schemaname: String,
    pub tablename: String,
    pub indexname: String,
}

/// Query all indexes defined on `table_name` in the current schema.
/// Useful for diagnostic test output and for the `assert_index_exists`
/// helper to walk the list without making N round-trips.
pub async fn list_indexes_on_table(
    pool: &PgPool,
    table_name: &str,
) -> anyhow::Result<Vec<IndexRow>> {
    let rows = sqlx::query_as::<_, IndexRow>(
        "SELECT schemaname, tablename, indexname \
           FROM pg_indexes \
          WHERE tablename = $1 \
          ORDER BY indexname",
    )
    .bind(table_name)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Assert that the named index exists on the named table. Returns a
/// descriptive error string on failure listing what indexes DO exist
/// on the table, so the test reporter can immediately tell what's missing.
///
/// # Example
///
/// ```ignore
/// assert_index_exists(
///     &pool,
///     "sys_user_tenant",
///     "idx_sys_user_tenant_tenant_status",
/// ).await.unwrap();
/// ```
pub async fn assert_index_exists(
    pool: &PgPool,
    table_name: &str,
    index_name: &str,
) -> Result<(), String> {
    let rows = list_indexes_on_table(pool, table_name)
        .await
        .map_err(|e| format!("failed to query pg_indexes: {}", e))?;

    if rows.iter().any(|r| r.indexname == index_name) {
        Ok(())
    } else {
        let existing: Vec<&str> = rows.iter().map(|r| r.indexname.as_str()).collect();
        Err(format!(
            "missing index `{}` on table `{}`; existing indexes: {:?}",
            index_name, table_name, existing
        ))
    }
}
```

- [ ] **Step 9.2: Wire into testing module**

Edit `crates/framework/src/testing/mod.rs`:

```rust
//! Test helpers for downstream crates. ...

pub mod explain_plan;
pub mod pg_catalog;
```

- [ ] **Step 9.3: Compile-check**

```bash
cd server-rs && cargo check -p framework 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 9.4: Verify framework test suite**

```bash
cd server-rs && cargo test -p framework 2>&1 | grep "test result"
cd server-rs && cargo clippy -p framework --all-targets -- -D warnings 2>&1 | tail -3
```

Expected: framework tests still at 84 (no new unit tests — this helper is DB-dependent and belongs in integration tests once indexes exist), clippy clean.

- [ ] **Step 9.5: Report Task 9 complete**

Report back: `assert_index_exists` async helper implemented, framework compiles clean, no new tests (DB-dependent helper).

---

### Task 10: Documentation updates + spec v1.1 completion

**Files:**
- Modify: `server-rs/docs/framework/framework-pagination-spec.md`
- Modify: `server-rs/docs/framework/framework-pagination-indexes.md`

Reflect v1.1 completion in the normative docs and indexes registry.

- [ ] **Step 10.1: Update spec §11 table**

Edit `server-rs/docs/framework/framework-pagination-spec.md`. Find the "| **v1.1** |" row in §11 and update the "触发条件" column from "本周内立项" to "✅ 2026-04-11 已实施"; update the "预估成本" column footer with actual LOC delta (filled in after Task 10.3 below).

- [ ] **Step 10.2: Update spec §6.2 and §7.1 "v1.1 引入" notes**

Find the v1.1 deferral notes in §6.2 (slow query warn) and §7.1 (i18n parameterization). Replace "v1.1 引入" with "✅ v1.1 已实施（2026-04-11）" and strike-through or delete the old limitation note.

- [ ] **Step 10.3: Update spec §8.2 Race B fix note**

Find §8.2 "v1.1 应当" note about reconcile_total. Replace with "✅ v1.1 已通过 `PaginationParams::reconcile_total` 实施".

- [ ] **Step 10.4: Update spec §13 PR checklist**

Add new v1.1 checklist items at the end of §13:

```markdown
- [ ] `find_page` 的 rows/count 查询都用 `with_timeout` 包裹
- [ ] `find_page` 收尾有 `reconcile_total` 修复 Race B
- [ ] `find_page` 的 post-condition 违反时走 truncate + warn，不 panic
- [ ] 如涉及新索引假设，已在 `docs/framework/framework-pagination-indexes.md` 登记并有对应 migration
```

- [ ] **Step 10.5: Update indexes doc todo section**

Edit `server-rs/docs/framework/framework-pagination-indexes.md`. Find the "待办事项" section and mark the seq-scan helper item complete:

```markdown
## 待办事项

- [ ] 对 `user_repo::find_page` 跑一次 `EXPLAIN (ANALYZE, BUFFERS)` with 100k seed
- [ ] 确认 `sys_user_tenant` 上是否已有 `(tenant_id, status)` 复合索引；如无，写 migration 创建
- [ ] 确认 `sys_user_role` 上是否已有反向 `role_id` 索引；如无，写 migration 创建
- [ ] 为 `sys_user` 创建 `create_at DESC WHERE del_flag='0'` partial index
- [x] ~~引入 `framework::testing::assert_no_seq_scan` helper~~ ✅ v1.1 — see `framework::testing::explain_plan::check_no_seq_scan`
- [x] ~~引入 `framework::testing::assert_index_exists` helper~~ ✅ v1.1 — see `framework::testing::pg_catalog::assert_index_exists`
- [ ] 为每个 `find_page` 挂一个 seq-scan regression integration test (v1.2 — depends on indexes existing first)
```

- [ ] **Step 10.6: Run full workspace verify**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cd server-rs && cargo fmt --check && echo "fmt ok"
```

Expected:

- framework: 84 passed (69 baseline + 3 timeout + 3 reconcile + 1 validation_error + 2 i18n helper + 6 explain_plan)
- modules lib: 33 passed
- role integration: 23 passed
- user integration: 25 passed
- app: 3 passed
- total: 168 passed (up from 153 baseline, +15)
- clippy clean
- fmt ok

- [ ] **Step 10.7: Run smoke tests**

```bash
cd server-rs
pkill -f target/debug/app 2>/dev/null
sleep 1
cargo build -p app 2>&1 | tail -3
./target/debug/app > /tmp/tea-rs-v11-role.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-role-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null
wait 2>/dev/null

./target/debug/app > /tmp/tea-rs-v11-user.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-user-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null
wait 2>/dev/null
```

Expected: `ALL 14 STEPS PASSED` (role) + `ALL 16 STEPS PASSED` (user).

- [ ] **Step 10.8: Report plan complete**

Report back with final summary:

- All 10 tasks complete
- Workspace tests: 153 → 168 (+15)
- Framework tests: 69 → 84 (+15)
- 4 `find_page` methods upgraded to v1.1 (timeout + slow warn + truncate + reconcile)
- i18n validation errors now parameterized
- Two new test helpers available (`check_no_seq_scan`, `assert_index_exists`)
- Spec + indexes doc updated
- Smoke tests 14/14 + 16/16 green
- Zero wire contract changes
- Zero new crate dependencies
- Hand back to user for manual commit and merge.

---

## Post-plan status snapshot

After all 10 tasks:

| Metric | v1.0 baseline | v1.1 target |
|---|---|---|
| Total tests passing | 153 | 168 (+15) |
| Framework tests | 69 | 84 (+15) |
| `find_page` methods with timeout | 0 / 4 | 4 / 4 |
| `find_page` methods with slow-query warn | 0 / 4 | 4 / 4 |
| `find_page` methods with truncate+warn post-condition | 0 / 4 | 4 / 4 |
| `find_page` methods with reconcile_total | 0 / 4 | 4 / 4 |
| i18n-parameterized validator errors | ❌ | ✅ |
| `check_no_seq_scan` helper available | ❌ | ✅ (pure fn) |
| `assert_index_exists` helper available | ❌ | ✅ (async) |
| Wire contract changes | — | 0 |
| New crate dependencies | — | 0 |
| Smoke tests green | 14/14 + 16/16 | 14/14 + 16/16 |

---

## What this plan explicitly doesn't do (deferred to v1.2+)

Per spec §11 触发器表 + v1.1 scope decisions:

1. **Write the actual index migrations** — the helper (`assert_index_exists`) exists, but `docs/framework/framework-pagination-indexes.md` "待办事项" still lists TBD indexes. Creating them requires real `EXPLAIN ANALYZE` on seeded data — defer to v1.2 when seed data arrives.
2. **Wire seq-scan regression tests to specific `find_page` queries** — the helper exists, but on the current 100-row dev DB Postgres picks seq scan over index scan regardless (index scan is more expensive for tiny tables). Enabling these tests requires `SET enable_seqscan = off` + a non-trivial seed; defer to v1.2.
3. **`total: Option<u64>` + `has_more` mode** — v2.0 per spec §11.
4. **Cursor pagination** — v3.0.
5. **Sort framework** — v3.1.
6. **Export/streaming endpoint** — v3.2.
7. **Tenant-aware PagePolicy** — rejected indefinitely per spec design principle 1 (explicit > smart).
8. **`paginate_with_tracing` proc-macro** — rejected per spec §10 禁止模式 and v1.1 design review (<6 call sites, over-abstraction).

---

## Execution handoff

Plan complete. Two execution options:

**1. Subagent-Driven (recommended)** — Controller dispatches fresh implementer + 2 reviewer subagents per task. Each task gets isolated context, two-stage review (spec compliance + code quality), fast iteration.

**2. Inline Execution** — Execute tasks in the current session with checkpoints for human review between tasks.

Which approach?

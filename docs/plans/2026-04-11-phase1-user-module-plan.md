# Phase 1 Sub-Phase 2a — User Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task.

**Goal:** Deliver the 11 admin CRUD + management endpoints for the user module in the Rust rewrite, preserving the NestJS wire contract so Vue web's "系统管理 → 用户管理" page can switch `VITE_API_URL` to the Rust backend with zero frontend changes.

**Architecture:** Continues Route C (explicit SQL + small helpers + hand-written `sqlx::query_as`) established in Sub-Phase 1. Tenant scoping for `sys_user` uses a `sys_user_tenant` JOIN (read path) or an `EXISTS` subquery (write path). `sys_user_role` writes stay owned by `role_repo.rs` via new tx-accepting helpers. `sys_user_tenant` writes are temporarily owned by `user_repo.rs`. All service functions use the Batch 5.5 ergonomic error traits and handlers use `require_permission!` / `require_role!` / new `require_authenticated!` macros.

**Tech Stack:** Rust, axum 0.8, sqlx (runtime query, no macro), validator 0.20, anyhow, tracing, bcrypt via `framework::infra::crypto`, PostgreSQL 15 (real dev DB at 127.0.0.1:5432/saas_tea).

---

## File Structure Overview

### New files

```text
server-rs/crates/modules/src/system/user/
├── mod.rs            (new, ~8 LOC)    — pub use + router merge
├── dto.rs            (new, ~280 LOC)  — 11 DTOs + validation + fixture macros + unit tests
├── service.rs        (new, ~380 LOC)  — business logic, guards, transactions
└── handler.rs        (new, ~260 LOC)  — axum handlers + router()

server-rs/crates/modules/tests/
└── user_module_tests.rs  (new, ~500 LOC)  — ~22 real-DB integration tests

server-rs/scripts/
└── smoke-user-module.sh  (new, ~200 LOC)  — 16-step end-to-end script
```

### Modified files

```text
server-rs/crates/framework/src/response/codes.rs       — add OPERATION_NOT_ALLOWED = 1004
server-rs/crates/framework/src/response/mod.rs         — re-export fmt_ts (after promotion)
server-rs/crates/framework/src/response/time.rs        — new module, hosts fmt_ts (promoted from role dto)
server-rs/crates/framework/src/middleware/access_macros.rs — add require_authenticated! macro
server-rs/crates/modules/src/domain/entities.rs        — add SysUser struct
server-rs/crates/modules/src/domain/user_repo.rs       — add find_by_id, find_page, find_option_list, find_info, insert_tx, insert_user_tenant_binding_tx, update_tx, change_status, soft_delete_by_id, reset_password, verify_user_name_unique
server-rs/crates/modules/src/domain/role_repo.rs       — add find_role_ids_by_user, verify_role_ids_in_tenant, replace_user_roles_tx
server-rs/crates/modules/src/system/role/handler.rs    — retrofit option-select to use new require_authenticated! macro
server-rs/crates/modules/src/system/role/dto.rs        — remove fmt_ts (now imported from framework)
server-rs/crates/modules/src/system/mod.rs             — re-export system::user
server-rs/crates/modules/src/lib.rs                    — wire user router into modules::router()
server-rs/crates/modules/tests/common/mod.rs           — add cleanup_test_users(pool, prefix) helper
```

---

## Conventions Recap (read before starting any task)

These are the Batch 5.5 ergonomic patterns established in Sub-Phase 1. DO NOT regress to older verbose forms.

| Concern | ✅ Use | ❌ Never use |
|---|---|---|
| Repo error plumbing | `.context("method: step")?` (requires `use anyhow::Context;`) | `.map_err(|e| anyhow::anyhow!("..."))` |
| Service Result<anyhow, _> → AppError | `.into_internal()?` | `.map_err(AppError::Internal)?` |
| Service Option<T> → AppError::Business | `.or_business(code)?` | `BusinessError::throw_if_null(...)` |
| Service bool → AppError::Business | `.business_err_if(code)` | `if cond { return BusinessError::throw(...) }` |
| Handler route permission layer | `require_permission!("x:y:z")` | `route_layer(from_fn_with_state(access::require(AccessSpec::permission(...)), access::enforce))` |
| Handler route role layer | `require_role!("TENANT_ADMIN")` | raw from_fn_with_state |
| Handler route authenticated-only | `require_authenticated!()` (new this sub-phase) | raw from_fn_with_state |
| List DTO pagination | `#[serde(flatten)] #[validate(nested)] pub page: PageQuery` | local page_num/page_size fields |
| Repo method observability | `#[instrument(skip_all, fields(...))]` — EVERY public method | (none) |
| Tenant-scoped reads | `JOIN sys_user_tenant ut ON ... WHERE ($N::varchar IS NULL OR ut.tenant_id = $N)` | `WHERE platform_id = $N` (platform_id ≠ tenant_id!) |
| Tenant-scoped writes | `WHERE ... AND ($N::varchar IS NULL OR EXISTS (SELECT 1 FROM sys_user_tenant WHERE user_id = sys_user.user_id AND tenant_id = $N AND status = '0'))` | direct column filter (sys_user has no tenant_id column) |
| Map repo rows to DTOs | `page.map_rows(UserListItemResponseDto::from_entity)` | manual `Page::new(page.rows.into_iter().map(...).collect(), ...)` |
| Transactions crossing repos | service opens `tx`, passes `&mut tx` to tx-accepting repo helpers, service commits | each repo opening its own tx |

---

## Reference tables

Admin user identity (for guards):

```text
user_name = 'admin'
platform_id = '000000'
user_id = 'cf827fc0-e7cc-4b9f-913c-e20628ade20a'  (dev DB seed)
```

Test DB credentials (for smoke + integration):

```text
host:     127.0.0.1:5432
database: saas_tea
user:     saas_tea
password: 123456
```

Current baseline test count: **110 passing** (Sub-Phase 1 complete). Target at end of Sub-Phase 2a: **~148 passing** (+ ~16 unit + ~22 integration).

---

# WEEK 1 — Read-only foundation (Tasks 1-10)

Goal: `GET /list`, `GET /{id}`, `GET /option-select`, `GET /info` end-to-end against real DB. No writes yet.

---

### Task 1: Framework additions — 3 small primitives

**Files:**
- Modify: `crates/framework/src/response/codes.rs`
- Create: `crates/framework/src/response/time.rs`
- Modify: `crates/framework/src/response/mod.rs`
- Modify: `crates/framework/src/middleware/access_macros.rs`
- Modify: `crates/modules/src/system/role/dto.rs` (remove local fmt_ts, import from framework)
- Modify: `crates/modules/src/system/role/handler.rs` (retrofit option-select to require_authenticated!)

- [ ] **Step 1: Add OPERATION_NOT_ALLOWED constant**

Edit `crates/framework/src/response/codes.rs`, in the "通用业务错误" segment after `OPTIMISTIC_LOCK_CONFLICT`:

```rust
    pub const OPTIMISTIC_LOCK_CONFLICT: Self = Self(1003);
    pub const OPERATION_NOT_ALLOWED: Self = Self(1004);
```

- [ ] **Step 2: Create `framework::response::time` module with fmt_ts**

Create `crates/framework/src/response/time.rs`:

```rust
//! Time formatting helpers for wire responses.
//!
//! NestJS `BaseResponseDto` uses dayjs `YYYY-MM-DD HH:mm:ss` in
//! `Asia/Shanghai` (UTC+8, no DST). Mirror that exactly so the Vue
//! web frontend parses both backends' strings identically.

use chrono::{DateTime, FixedOffset, Utc};

/// Format a UTC timestamp as `YYYY-MM-DD HH:mm:ss` in Asia/Shanghai.
/// UTC+8 has no DST, so `FixedOffset` matches `Asia/Shanghai` exactly
/// without pulling in the `chrono-tz` IANA database.
pub fn fmt_ts(ts: &DateTime<Utc>) -> String {
    let offset = FixedOffset::east_opt(8 * 3600).expect("valid UTC+8 offset");
    ts.with_timezone(&offset)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn fmt_ts_formats_in_asia_shanghai() {
        let utc = Utc.with_ymd_and_hms(2026, 4, 11, 14, 0, 0).unwrap();
        assert_eq!(fmt_ts(&utc), "2026-04-11 22:00:00");
    }

    #[test]
    fn fmt_ts_handles_midnight_boundary() {
        let utc = Utc.with_ymd_and_hms(2026, 4, 11, 16, 0, 0).unwrap();
        assert_eq!(fmt_ts(&utc), "2026-04-12 00:00:00");
    }

    #[test]
    fn fmt_ts_handles_year_boundary() {
        let utc = Utc.with_ymd_and_hms(2026, 12, 31, 16, 0, 0).unwrap();
        assert_eq!(fmt_ts(&utc), "2027-01-01 00:00:00");
    }
}
```

- [ ] **Step 3: Re-export `fmt_ts` from `framework::response`**

Edit `crates/framework/src/response/mod.rs` — add `pub mod time;` and `pub use time::fmt_ts;` with the other re-exports.

- [ ] **Step 4: Add `require_authenticated!` macro**

Edit `crates/framework/src/middleware/access_macros.rs`, append after `require_role!`:

```rust
/// Route-layer macro for authenticated-only routes (no specific permission
/// or role required). Equivalent to the raw
/// `from_fn_with_state(access::require(AccessSpec::authenticated()), access::enforce)`
/// form.
///
/// Usage: `.route("/path", get(handler).route_layer(require_authenticated!()))`
#[macro_export]
macro_rules! require_authenticated {
    () => {
        axum::middleware::from_fn_with_state(
            $crate::middleware::access::require($crate::auth::AccessSpec::authenticated()),
            $crate::middleware::access::enforce,
        )
    };
}
```

- [ ] **Step 5: Remove local `fmt_ts` from role DTO and import from framework**

Edit `crates/modules/src/system/role/dto.rs`:
- Delete the `pub(super) fn fmt_ts(...)` function and its doc comment
- Delete the 3 `fmt_ts_*` unit tests in the `tests` module (they live in `framework::response::time::tests` now)
- Change the import line:

```rust
use framework::response::{fmt_ts, PageQuery};
```

Remove `use chrono::{DateTime, FixedOffset, Utc};` from dto.rs (still need `chrono` for other types; check and keep only what's used).

- [ ] **Step 6: Retrofit role option-select to use `require_authenticated!`**

Edit `crates/modules/src/system/role/handler.rs`. Find the option-select route — currently uses raw `from_fn_with_state(...)`. Replace with:

```rust
.route(
    "/system/role/option-select",
    get(option_select).route_layer(require_authenticated!()),
)
```

Remove the now-unused imports from handler.rs: `axum::middleware::from_fn_with_state`, `framework::auth::AccessSpec`, `framework::middleware::access` — ONLY if they're not used anywhere else in that file.

- [ ] **Step 7: Verification**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
cargo test --workspace 2>&1 | grep "test result"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --check && echo "fmt ok"
```

Expected: previous **110 passing → still 110** (3 fmt_ts tests moved from modules → framework; net zero). Clippy clean. Fmt clean.

- [ ] **Step 8: Role module smoke regression**

```bash
pkill -f target/debug/app 2>/dev/null; sleep 1
./target/debug/app > /tmp/tea-rs-app-task1.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-role-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected: `ALL 14 STEPS PASSED`. Role's option-select must still work via the new macro.

---

### Task 2: `SysUser` entity struct

**Files:**
- Modify: `crates/modules/src/domain/entities.rs`

- [ ] **Step 1: Add SysUser struct**

Append to `entities.rs`:

```rust
use chrono::{DateTime, Utc};

/// Full `sys_user` row. Mirrors the DB schema including the bcrypt
/// password hash. Intentionally does NOT derive `Serialize` — the
/// wire response uses explicit DTOs that omit the password field, so
/// we cannot accidentally leak it through a `ApiResponse<SysUser>` path.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SysUser {
    pub user_id: String,
    pub platform_id: String,
    pub dept_id: Option<String>,
    pub user_name: String,
    pub nick_name: String,
    pub user_type: String,
    pub client_type: Option<String>,
    pub lang: Option<String>,
    pub email: String,
    pub phonenumber: String,
    pub whatsapp: String,
    pub sex: String,
    pub avatar: String,
    pub password: String,
    pub status: String,
    pub del_flag: String,
    pub login_ip: String,
    pub login_date: Option<DateTime<Utc>>,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
}
```

Check whether `chrono::{DateTime, Utc}` is already imported at the top of `entities.rs` — if yes, skip the extra `use`. Check whether a simpler `SysUser` struct already exists from Phase 0 — if so, extend it in-place rather than duplicate (read Phase 0's user_repo.rs for any existing struct first).

- [ ] **Step 2: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
```

Expected: no errors.

---

### Task 3: `user_repo.rs` — add COLUMNS constant + `find_by_id` with tenant JOIN

**Files:**
- Modify: `crates/modules/src/domain/user_repo.rs`

- [ ] **Step 1: Add `USER_COLUMNS` constant**

Add near the top of `user_repo.rs`:

```rust
/// Single source of truth for `sys_user` SELECT column lists. Keep in
/// sync with `SysUser` FromRow field order for readability.
const USER_COLUMNS: &str = "\
    u.user_id, u.platform_id, u.dept_id, u.user_name, u.nick_name, \
    u.user_type, u.client_type, u.lang, u.email, u.phonenumber, \
    u.whatsapp, u.sex, u.avatar, u.password, u.status, u.del_flag, \
    u.login_ip, u.login_date, u.create_by, u.create_at, u.update_by, \
    u.update_at, u.remark";
```

The `u.` alias prefix matters because reads JOIN `sys_user_tenant ut` — ambiguous column names (`user_id`) would fail otherwise.

- [ ] **Step 2: Add `find_by_id_tenant_scoped` method**

**Important**: Phase 0 already has a `find_by_id(pool, user_id)` method used by the auth flow. It's NOT tenant-scoped (the auth flow needs global user lookup). DO NOT modify it. Add a NEW method with a distinct name for the admin CRUD path:

```rust
use anyhow::Context;
use tracing::instrument;

impl UserRepo {
    /// Tenant-scoped find-by-id for admin CRUD. Joins `sys_user_tenant`
    /// to enforce the current tenant's membership. Returns `None` if
    /// the user doesn't exist, is soft-deleted, or isn't bound to the
    /// current tenant — all treated as "not found" by the service layer
    /// (information hiding).
    ///
    /// Use this for admin endpoints. The existing `find_by_id` (without
    /// tenant scoping) is reserved for the Phase 0 auth flow which
    /// needs global user lookup during login.
    #[instrument(skip_all, fields(user_id = %user_id))]
    pub async fn find_by_id_tenant_scoped(
        pool: &PgPool,
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
}
```

Imports at top of `user_repo.rs` — verify `current_tenant_scope` is already imported (from Phase 0 usage); if not, add `use super::common::current_tenant_scope;`. Same for `SysUser` — add `use super::entities::SysUser;` if absent.

- [ ] **Step 3: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo test --workspace 2>&1 | grep "test result"
```

Expected: 110 passing, no new failures.

---

### Task 4: Extend integration harness with user cleanup helper

**Files:**
- Modify: `crates/modules/tests/common/mod.rs`

- [ ] **Step 1: Add `cleanup_test_users` helper**

Append to `common/mod.rs`:

```rust
/// Cleanup helper — delete all `sys_user` rows + their `sys_user_role`
/// and `sys_user_tenant` bindings created by a given test prefix.
/// Matches `user_name LIKE '{prefix}%'`.
pub async fn cleanup_test_users(pool: &PgPool, prefix: &str) {
    let pattern = format!("{prefix}%");
    // Order matters: delete join rows first (no audit fields), then the
    // user row itself. Wrap errors with `.expect(...)` so test panics
    // surface cleanup failures clearly.
    sqlx::query(
        "DELETE FROM sys_user_role WHERE user_id IN \
         (SELECT user_id FROM sys_user WHERE user_name LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_user_role for test users");
    sqlx::query(
        "DELETE FROM sys_user_tenant WHERE user_id IN \
         (SELECT user_id FROM sys_user WHERE user_name LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_user_tenant for test users");
    sqlx::query("DELETE FROM sys_user WHERE user_name LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_user for test users");
}
```

- [ ] **Step 2: Verification**

```bash
cargo check -p modules --tests 2>&1 | tail -5
```

Expected: compiles. `#![allow(dead_code)]` at top of common/mod.rs suppresses the "unused" warning until later tasks use it.

---

### Task 5: User DTOs — detail + list item + list query

**Files:**
- Create: `crates/modules/src/system/user/mod.rs`
- Create: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/system/mod.rs`

- [ ] **Step 1: Create `system/user/mod.rs`**

```rust
//! User management endpoints — admin CRUD + management.
//! Personal profile + batch endpoints deferred to Sub-Phase 2b.

pub mod dto;
pub mod handler;
pub mod service;

pub use handler::router;
```

- [ ] **Step 2: Register user module in `system/mod.rs`**

Add `pub mod user;` alongside `pub mod role;`.

- [ ] **Step 3: Create `system/user/dto.rs` with the Week 1 DTOs**

```rust
//! User DTOs — wire shapes matching NestJS for cross-backend compat.

use crate::domain::SysUser;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::{Validate, ValidationError};

fn default_status() -> String {
    "0".into()
}

/// Accept only `"0"` (active) or `"1"` (disabled) for user status.
/// Mirrors the role module's `validate_status_flag`.
fn validate_status_flag(value: &str) -> Result<(), ValidationError> {
    match value {
        "0" | "1" => Ok(()),
        _ => Err(ValidationError::new("status_flag")),
    }
}

/// Accept only `"0"` (male), `"1"` (female), or `"2"` (unknown).
fn validate_sex_flag(value: &str) -> Result<(), ValidationError> {
    match value {
        "0" | "1" | "2" => Ok(()),
        _ => Err(ValidationError::new("sex_flag")),
    }
}

/// Full user detail returned by `GET /system/user/:id` and `POST /system/user/`.
/// Excludes the `password` field — NEVER include it in any wire response.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDetailResponseDto {
    pub user_id: String,
    pub platform_id: String,
    pub dept_id: Option<String>,
    pub user_name: String,
    pub nick_name: String,
    pub user_type: String,
    pub email: String,
    pub phonenumber: String,
    pub sex: String,
    pub avatar: String,
    pub status: String,
    pub remark: Option<String>,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub role_ids: Vec<String>,
}

impl UserDetailResponseDto {
    pub fn from_entity(user: SysUser, role_ids: Vec<String>) -> Self {
        Self {
            user_id: user.user_id,
            platform_id: user.platform_id,
            dept_id: user.dept_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
            user_type: user.user_type,
            email: user.email,
            phonenumber: user.phonenumber,
            sex: user.sex,
            avatar: user.avatar,
            status: user.status,
            remark: user.remark,
            create_by: user.create_by,
            create_at: fmt_ts(&user.create_at),
            update_by: user.update_by,
            update_at: fmt_ts(&user.update_at),
            role_ids,
        }
    }
}

/// Lightweight row for `GET /system/user/list`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserListItemResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub phonenumber: String,
    pub sex: String,
    pub status: String,
    pub dept_id: Option<String>,
    pub create_at: String,
}

impl UserListItemResponseDto {
    pub fn from_entity(user: SysUser) -> Self {
        Self {
            user_id: user.user_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
            email: user.email,
            phonenumber: user.phonenumber,
            sex: user.sex,
            status: user.status,
            dept_id: user.dept_id,
            create_at: fmt_ts(&user.create_at),
        }
    }
}

/// Query string for `GET /system/user/list`.
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListUserDto {
    #[validate(length(max = 50))]
    pub user_name: Option<String>,
    #[validate(length(max = 30))]
    pub nick_name: Option<String>,
    #[validate(length(max = 50))]
    pub email: Option<String>,
    #[validate(length(max = 11))]
    pub phonenumber: Option<String>,
    pub status: Option<String>,
    pub dept_id: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

// Later tasks append more DTOs to this file:
// - CreateUserDto (Task 12)
// - UpdateUserDto (Task 16)
// - ChangeUserStatusDto (Task 17)
// - ResetPwdDto (Task 21)
// - AuthRoleQueryResponseDto (Task 22)
// - AuthRoleUpdateDto (Task 23)
// - UserOptionResponseDto (Task 8)
// - UserInfoResponseDto (Task 9)

#[cfg(test)]
mod tests {
    use super::{ListUserDto, UserDetailResponseDto, UserListItemResponseDto};
    use framework::response::PageQuery;
    use validator::Validate;

    #[test]
    fn list_user_dto_accepts_valid_defaults() {
        let dto = ListUserDto {
            user_name: None,
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: PageQuery::default(),
        };
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn list_user_dto_rejects_oversize_user_name_filter() {
        let dto = ListUserDto {
            user_name: Some("a".repeat(51)),
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: PageQuery::default(),
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn list_user_dto_rejects_page_num_zero() {
        let dto = ListUserDto {
            user_name: None,
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: PageQuery {
                page_num: 0,
                page_size: 10,
            },
        };
        assert!(dto.validate().is_err());
    }
}
```

- [ ] **Step 4: Verification**

```bash
cargo test -p modules --lib 'system::user::dto' 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: 3 new DTO tests pass. Workspace total: **113 passing** (110 + 3).

---

### Task 6: `RoleRepo::find_role_ids_by_user` bridge method

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`

- [ ] **Step 1: Add method**

Append to `impl RoleRepo`:

```rust
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
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT role_id FROM sys_user_role WHERE user_id = $1 ORDER BY role_id",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .context("find_role_ids_by_user")?;
    Ok(rows.into_iter().map(|(r,)| r).collect())
}
```

- [ ] **Step 2: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
```

---

### Task 7: User service + handler — `GET /list` and `GET /{id}`, wire into app

**Files:**
- Create: `crates/modules/src/system/user/service.rs`
- Create: `crates/modules/src/system/user/handler.rs`
- Modify: `crates/modules/src/lib.rs` (wire user router)
- Modify: `crates/modules/src/domain/user_repo.rs` (add `find_page`)

- [ ] **Step 1: Add `UserRepo::find_page`**

Append to `impl UserRepo`:

```rust
/// Paginated list of users in the current tenant. Joins `sys_user_tenant`
/// for tenant membership + status filter. Optional filters: user_name,
/// nick_name, email, phonenumber, status, dept_id — all substring or
/// exact-match via `IS NULL OR ...` predicates.
#[instrument(skip_all, fields(
    has_user_name = user_name.is_some(),
    has_status = status.is_some(),
    page_num, page_size
))]
#[allow(clippy::too_many_arguments)]
pub async fn find_page(
    pool: &PgPool,
    user_name: Option<&str>,
    nick_name: Option<&str>,
    email: Option<&str>,
    phonenumber: Option<&str>,
    status: Option<&str>,
    dept_id: Option<&str>,
    page_num: u32,
    page_size: u32,
) -> anyhow::Result<framework::response::Page<SysUser>> {
    let tenant = current_tenant_scope();
    let safe_page_num = page_num.max(1);
    let safe_page_size = page_size.clamp(1, 200);
    let offset = ((safe_page_num - 1) * safe_page_size) as i64;
    let limit = safe_page_size as i64;

    let where_sql = "\
        WHERE u.del_flag = '0' \
          AND ut.status = '0' \
          AND ($1::varchar IS NULL OR ut.tenant_id = $1) \
          AND ($2::varchar IS NULL OR u.user_name LIKE '%' || $2 || '%') \
          AND ($3::varchar IS NULL OR u.nick_name LIKE '%' || $3 || '%') \
          AND ($4::varchar IS NULL OR u.email LIKE '%' || $4 || '%') \
          AND ($5::varchar IS NULL OR u.phonenumber LIKE '%' || $5 || '%') \
          AND ($6::varchar IS NULL OR u.status = $6) \
          AND ($7::varchar IS NULL OR u.dept_id = $7)";

    let rows_sql = format!(
        "SELECT {USER_COLUMNS} FROM sys_user u \
         JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
         {where_sql} \
         ORDER BY u.create_at DESC \
         LIMIT $8 OFFSET $9"
    );
    let rows = sqlx::query_as::<_, SysUser>(&rows_sql)
        .bind(tenant.as_deref())
        .bind(user_name)
        .bind(nick_name)
        .bind(email)
        .bind(phonenumber)
        .bind(status)
        .bind(dept_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .context("find_page rows")?;

    let count_sql = format!(
        "SELECT COUNT(*) FROM sys_user u \
         JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
         {where_sql}"
    );
    let total: i64 = sqlx::query_scalar(&count_sql)
        .bind(tenant.as_deref())
        .bind(user_name)
        .bind(nick_name)
        .bind(email)
        .bind(phonenumber)
        .bind(status)
        .bind(dept_id)
        .fetch_one(pool)
        .await
        .context("find_page count")?;

    Ok(framework::response::Page::new(
        rows,
        total as u64,
        safe_page_num,
        safe_page_size,
    ))
}
```

- [ ] **Step 2: Create `system/user/service.rs`**

```rust
//! User service — business orchestration.

use super::dto::{ListUserDto, UserDetailResponseDto, UserListItemResponseDto};
use crate::domain::{RoleRepo, UserRepo};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

/// Fetch a user by id, tenant-scoped. Returns `DATA_NOT_FOUND` when the
/// user doesn't exist in the current tenant (also covers soft-deleted
/// and cross-tenant attempts — information hiding).
pub async fn find_by_id(
    state: &AppState,
    user_id: &str,
) -> Result<UserDetailResponseDto, AppError> {
    let user = UserRepo::find_by_id_tenant_scoped(&state.pg, user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    let role_ids = RoleRepo::find_role_ids_by_user(&state.pg, &user.user_id)
        .await
        .into_internal()?;

    Ok(UserDetailResponseDto::from_entity(user, role_ids))
}

/// Paginated user list. Validation runs in the extractor before reaching
/// this function.
pub async fn list(
    state: &AppState,
    query: ListUserDto,
) -> Result<Page<UserListItemResponseDto>, AppError> {
    let page = UserRepo::find_page(
        &state.pg,
        query.user_name.as_deref(),
        query.nick_name.as_deref(),
        query.email.as_deref(),
        query.phonenumber.as_deref(),
        query.status.as_deref(),
        query.dept_id.as_deref(),
        query.page.page_num,
        query.page.page_size,
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(UserListItemResponseDto::from_entity))
}
```

- [ ] **Step 3: Create `system/user/handler.rs`**

```rust
//! User HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::get,
    Router,
};
use framework::error::AppError;
use framework::extractors::ValidatedQuery;
use framework::require_permission;
use framework::response::{ApiResponse, Page};

async fn find_by_id(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<ApiResponse<dto::UserDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &user_id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListUserDto>,
) -> Result<ApiResponse<Page<dto::UserListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/user/list",
            get(list).route_layer(require_permission!("system:user:list")),
        )
        .route(
            "/system/user/{id}",
            get(find_by_id).route_layer(require_permission!("system:user:query")),
        )
}
```

- [ ] **Step 4: Wire user router into `modules::router()`**

Edit `crates/modules/src/lib.rs`. Find the `router()` function that already merges `system::role::router()`. Merge user's router alongside it:

```rust
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(system::role::router())
        .merge(system::user::router())  // NEW
        .with_state(state)
}
```

(Adjust to match the existing code exactly — the snippet above is illustrative.)

- [ ] **Step 5: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo build -p app 2>&1 | tail -3
cargo test --workspace 2>&1 | grep "test result"
```

Expected: 113 passing (3 new DTO tests, no functional tests yet). Clippy clean.

---

### Task 8: `GET /system/user/option-select`

**Files:**
- Modify: `crates/modules/src/domain/user_repo.rs`
- Modify: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: Repo method**

Append to `impl UserRepo`:

```rust
/// Users in the current tenant, active only, for dropdown UI.
/// Hard cap 500 rows. Supports optional `user_name` substring search.
#[instrument(skip_all, fields(has_name_filter = user_name.is_some()))]
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
```

- [ ] **Step 2: DTO — add `UserOptionQueryDto` and `UserOptionResponseDto`**

Append to `dto.rs`:

```rust
/// Optional search query for `GET /system/user/option-select`.
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UserOptionQueryDto {
    #[validate(length(max = 50))]
    pub user_name: Option<String>,
}

/// Dropdown-optimized flat user projection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOptionResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
}

impl UserOptionResponseDto {
    pub fn from_entity(user: SysUser) -> Self {
        Self {
            user_id: user.user_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
        }
    }
}
```

- [ ] **Step 3: Service**

Append to `service.rs`:

```rust
use super::dto::{UserOptionQueryDto, UserOptionResponseDto};

/// Return active users in the current tenant as flat dropdown options.
/// Supports optional name substring filter.
pub async fn option_select(
    state: &AppState,
    query: UserOptionQueryDto,
) -> Result<Vec<UserOptionResponseDto>, AppError> {
    let rows = UserRepo::find_option_list(&state.pg, query.user_name.as_deref())
        .await
        .into_internal()?;
    Ok(rows.into_iter().map(UserOptionResponseDto::from_entity).collect())
}
```

- [ ] **Step 4: Handler + route**

Append handler:

```rust
async fn option_select(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::UserOptionQueryDto>,
) -> Result<ApiResponse<Vec<dto::UserOptionResponseDto>>, AppError> {
    let resp = service::option_select(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}
```

Add to `router()`, BEFORE the `{id}` route (literal-before-param):

```rust
.route(
    "/system/user/option-select",
    get(option_select).route_layer(framework::require_authenticated!()),
)
```

Also import the macro at the top of handler.rs if not already:

```rust
use framework::{require_authenticated, require_permission};
```

- [ ] **Step 5: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

---

### Task 9: `GET /system/user/info`

**Files:**
- Modify: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: DTO**

Append to `dto.rs`:

```rust
/// Response for `GET /system/user/info`. Leaner than Phase 0's
/// `/api/v1/info` — returns just the current user's fields, no
/// permissions or tenant list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfoResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub phonenumber: String,
    pub avatar: String,
    pub sex: String,
    pub status: String,
    pub remark: Option<String>,
}

impl UserInfoResponseDto {
    pub fn from_entity(user: SysUser) -> Self {
        Self {
            user_id: user.user_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
            email: user.email,
            phonenumber: user.phonenumber,
            avatar: user.avatar,
            sex: user.sex,
            status: user.status,
            remark: user.remark,
        }
    }
}
```

- [ ] **Step 2: Service**

Append to `service.rs`:

```rust
use framework::context::current_request_context;

/// Return the current logged-in user's profile. Reads the user_id from
/// `RequestContext` and fetches the full row via the global (non-tenant
/// scoped) `find_by_id` — since the user IS the caller, tenant scoping
/// would be redundant.
pub async fn info(state: &AppState) -> Result<super::dto::UserInfoResponseDto, AppError> {
    let ctx = current_request_context().ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!("info: RequestContext missing"))
    })?;
    let user_id = ctx
        .user_id
        .as_ref()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("info: user_id absent")))?;

    let user = UserRepo::find_by_id(&state.pg, user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    Ok(super::dto::UserInfoResponseDto::from_entity(user))
}
```

**Important**: this uses Phase 0's existing non-tenant-scoped `UserRepo::find_by_id`. If the exact function name or signature differs from what's shown, inspect Phase 0's `user_repo.rs` and adjust the call. The `framework::context::current_request_context()` name may also vary — check the framework's context module for the actual accessor.

- [ ] **Step 3: Handler + route**

Append handler:

```rust
async fn info(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::UserInfoResponseDto>, AppError> {
    let resp = service::info(&state).await?;
    Ok(ApiResponse::ok(resp))
}
```

Add route (literal, before `{id}`):

```rust
.route(
    "/system/user/info",
    get(info).route_layer(framework::require_authenticated!()),
)
```

- [ ] **Step 4: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo build -p app 2>&1 | tail -3
cargo test --workspace 2>&1 | grep "test result"
```

Expected: 113 passing. Build green.

---

### Task 10: Week 1 manual smoke

**Files:** none (live curl)

- [ ] **Step 1: Start app**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
pkill -f target/debug/app 2>/dev/null; sleep 1
./target/debug/app > /tmp/tea-rs-app-week1-user.log 2>&1 &
APP_PID=$!
sleep 2
```

- [ ] **Step 2: Login + verify each Week 1 endpoint**

```bash
TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")

echo "==== GET /system/user/list ===="
curl -sS "http://127.0.0.1:18080/api/v1/system/user/list?pageNum=1&pageSize=10" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

echo "==== GET /system/user/info ===="
curl -sS http://127.0.0.1:18080/api/v1/system/user/info \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

echo "==== GET /system/user/option-select ===="
curl -sS http://127.0.0.1:18080/api/v1/system/user/option-select \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

ADMIN_ID="cf827fc0-e7cc-4b9f-913c-e20628ade20a"
echo "==== GET /system/user/{id} (admin) ===="
curl -sS "http://127.0.0.1:18080/api/v1/system/user/$ADMIN_ID" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected:
- list: `{code:200, data:{rows:[...], total:N}}`, at least 1 admin row
- info: returns admin's user profile (no password field visible)
- option-select: returns active tenant users
- by-id: full detail including `roleIds` array (may be empty)

- [ ] **Step 3: Week 1 gate checks**

```bash
cargo test --workspace 2>&1 | grep "test result"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
cargo fmt --check && echo "fmt ok"
bash scripts/smoke-role-module.sh 2>&1 | tail -3  # role regression
```

Expected: 113 tests, clippy clean, fmt clean, role smoke 14/14.

---

# WEEK 2 — Writes (Tasks 11-20)

Goal: POST create, PUT update, change-status, DELETE soft delete — all with guards.

---

### Task 11: `RoleRepo` — add `verify_role_ids_in_tenant` + `replace_user_roles_tx`

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`

- [ ] **Step 1: `verify_role_ids_in_tenant`**

Append to `impl RoleRepo`:

```rust
/// Verify all `role_ids` exist in `sys_role` and belong to the current
/// tenant (or are tenant-less when super-admin context is active).
/// Returns `Ok(())` on success, `Err(DATA_NOT_FOUND)` if any id is
/// missing. Used by user service to pre-validate role bindings before
/// INSERT.
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
```

- [ ] **Step 2: `replace_user_roles_tx` (tx-accepting)**

Append to `impl RoleRepo`:

```rust
/// Replace a user's role bindings entirely — delete existing rows,
/// bulk insert the new list. Caller-provided transaction. Used by both
/// user create/update and the `PUT /system/user/auth-role` endpoint.
///
/// Empty `role_ids` is a valid "unassign all" operation: the delete
/// runs, no insert follows. Duplicates in input are deduped via
/// `SELECT DISTINCT` defense-in-depth against composite PK violations.
#[instrument(skip_all, fields(user_id = %user_id, role_count = role_ids.len()))]
pub async fn replace_user_roles_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    role_ids: &[String],
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sys_user_role WHERE user_id = $1")
        .bind(user_id)
        .execute(&mut **tx)
        .await
        .context("replace_user_roles_tx: delete old")?;

    if !role_ids.is_empty() {
        sqlx::query(
            "INSERT INTO sys_user_role (user_id, role_id) \
             SELECT DISTINCT $1, unnest($2::varchar[])",
        )
        .bind(user_id)
        .bind(role_ids)
        .execute(&mut **tx)
        .await
        .context("replace_user_roles_tx: bulk insert")?;
    }

    Ok(())
}
```

- [ ] **Step 3: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
```

---

### Task 12: `CreateUserDto` + `UserRepo::insert_tx` + tenant binding helper

**Files:**
- Modify: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/domain/user_repo.rs`

- [ ] **Step 1: CreateUserDto + tests**

Append to `dto.rs`:

```rust
/// Request body for `POST /system/user/`. Wire-compatible with
/// NestJS `CreateUserRequestDto`.
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserDto {
    pub dept_id: Option<String>,
    #[validate(length(min = 1, max = 30))]
    pub nick_name: String,
    #[validate(length(min = 2, max = 50))]
    pub user_name: String,
    /// Plaintext password. Will be bcrypt-hashed before insert.
    /// Sub-Phase 2a enforces a relaxed rule: length 6-20. NestJS uses
    /// a stricter "upper+lower+digit+symbol" rule — deferred until a
    /// documented policy lands.
    #[validate(length(min = 6, max = 20))]
    pub password: String,
    #[validate(length(max = 50))]
    #[serde(default)]
    pub email: String,
    #[validate(length(max = 11))]
    #[serde(default)]
    pub phonenumber: String,
    #[serde(default = "default_sex")]
    #[validate(custom(function = "validate_sex_flag"))]
    pub sex: String,
    #[validate(length(max = 255))]
    #[serde(default)]
    pub avatar: String,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
    #[serde(default)]
    pub role_ids: Vec<String>,
}

fn default_sex() -> String {
    "2".into()
}
```

Add 3 new unit tests to the `tests` module (update the `use super::{...}` line to include `CreateUserDto`):

```rust
#[test]
fn create_user_dto_rejects_short_password() {
    let dto = CreateUserDto {
        dept_id: None,
        nick_name: "it-user".into(),
        user_name: "it-user".into(),
        password: "short".into(),
        email: "".into(),
        phonenumber: "".into(),
        sex: "2".into(),
        avatar: "".into(),
        status: "0".into(),
        remark: None,
        role_ids: vec![],
    };
    assert!(dto.validate().is_err());
}

#[test]
fn create_user_dto_rejects_empty_nick_name() {
    let dto = CreateUserDto {
        dept_id: None,
        nick_name: "".into(),
        user_name: "it-user".into(),
        password: "abc123".into(),
        email: "".into(),
        phonenumber: "".into(),
        sex: "2".into(),
        avatar: "".into(),
        status: "0".into(),
        remark: None,
        role_ids: vec![],
    };
    assert!(dto.validate().is_err());
}

#[test]
fn create_user_dto_accepts_valid_minimum() {
    let dto = CreateUserDto {
        dept_id: None,
        nick_name: "it".into(),
        user_name: "it-user".into(),
        password: "abc123".into(),
        email: "".into(),
        phonenumber: "".into(),
        sex: "2".into(),
        avatar: "".into(),
        status: "0".into(),
        remark: None,
        role_ids: vec![],
    };
    assert!(dto.validate().is_ok());
}
```

- [ ] **Step 2: `UserRepo::insert_tx`**

Append to `impl UserRepo`:

```rust
use super::common::AuditInsert;

/// Insert a new sys_user row inside a caller-provided transaction.
/// The `password` must ALREADY be bcrypt-hashed — this method does
/// NOT hash. Returns the inserted SysUser.
///
/// Audit fields (`create_by` / `update_by`) come from `AuditInsert::now()`.
/// `platform_id` is hardcoded `'000000'` — multi-platform deployments
/// don't exist yet. `user_type` is `'10'` (CUSTOM).
#[instrument(skip_all, fields(user_name = %user_name))]
#[allow(clippy::too_many_arguments)]
pub async fn insert_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_name: &str,
    nick_name: &str,
    password_hash: &str,
    dept_id: Option<&str>,
    email: &str,
    phonenumber: &str,
    sex: &str,
    avatar: &str,
    status: &str,
    remark: Option<&str>,
) -> anyhow::Result<SysUser> {
    let audit = AuditInsert::now();
    let user_id = uuid::Uuid::new_v4().to_string();

    // Build SELECT columns with NO u. alias for RETURNING since this
    // is a bare INSERT not a JOIN query — strip the alias.
    let plain_columns = USER_COLUMNS.replace("u.", "");

    let sql = format!(
        "INSERT INTO sys_user (\
            user_id, platform_id, dept_id, user_name, nick_name, user_type, \
            email, phonenumber, whatsapp, sex, avatar, password, status, del_flag, \
            login_ip, create_by, update_by, update_at, remark\
        ) VALUES (\
            $1, '000000', $2, $3, $4, '10', \
            $5, $6, '', $7, $8, $9, $10, '0', \
            '', $11, $12, CURRENT_TIMESTAMP, $13\
        ) RETURNING {plain_columns}"
    );

    let user = sqlx::query_as::<_, SysUser>(&sql)
        .bind(&user_id)
        .bind(dept_id)
        .bind(user_name)
        .bind(nick_name)
        .bind(email)
        .bind(phonenumber)
        .bind(sex)
        .bind(avatar)
        .bind(password_hash)
        .bind(status)
        .bind(&audit.create_by)
        .bind(&audit.update_by)
        .bind(remark)
        .fetch_one(&mut **tx)
        .await
        .context("insert_tx: insert sys_user")?;

    Ok(user)
}
```

- [ ] **Step 3: `UserRepo::insert_user_tenant_binding_tx`**

Append to `impl UserRepo`:

```rust
/// Insert a `sys_user_tenant` row binding the user to the current
/// tenant as a default member (not admin). Temporary ownership —
/// migrates to tenant_repo when that module lands.
///
/// Requires `current_tenant_scope()` to return `Some` — callers must
/// be inside a tenant context, not super-tenant bypass.
#[instrument(skip_all, fields(user_id = %user_id))]
pub async fn insert_user_tenant_binding_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
) -> anyhow::Result<()> {
    let tenant = current_tenant_scope()
        .context("insert_user_tenant_binding_tx: tenant_id required")?;
    sqlx::query(
        "INSERT INTO sys_user_tenant (\
            id, user_id, tenant_id, is_default, is_admin, status\
         ) VALUES (gen_random_uuid()::varchar, $1, $2, true, false, '0') \
         ON CONFLICT (user_id, tenant_id) DO NOTHING",
    )
    .bind(user_id)
    .bind(&tenant)
    .execute(&mut **tx)
    .await
    .context("insert_user_tenant_binding_tx")?;
    Ok(())
}
```

**Note**: the `gen_random_uuid()::varchar` cast may need adjustment depending on the column type of `sys_user_tenant.id`. If it's `uuid`, use `gen_random_uuid()`; if it's `varchar(36)`, use `gen_random_uuid()::varchar`. Check the schema via:

```bash
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c "\d sys_user_tenant"
```

Adjust the SQL if needed.

- [ ] **Step 4: `UserRepo::verify_user_name_unique`**

Append:

```rust
/// Returns true if `user_name` is unused in the current platform.
/// `sys_user.user_name` has a unique index across all tenants at the
/// platform level, so we check platform-wide, not tenant-scoped.
#[instrument(skip_all, fields(user_name = %user_name))]
pub async fn verify_user_name_unique(
    pool: &PgPool,
    user_name: &str,
) -> anyhow::Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sys_user \
          WHERE user_name = $1 AND del_flag = '0'",
    )
    .bind(user_name)
    .fetch_one(pool)
    .await
    .context("verify_user_name_unique")?;
    Ok(count == 0)
}
```

- [ ] **Step 5: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo test -p modules --lib 'system::user::dto' 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
```

Expected: 3 new DTO tests pass → 116 total.

---

### Task 13: Service `create` + handler `POST /system/user/`

**Files:**
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: Service `create`**

Append to `service.rs`:

```rust
use super::dto::CreateUserDto;
use framework::error::BusinessCheckBool;
use framework::infra::crypto::hash_password;

/// Create a user + tenant binding + role bindings in a single transaction.
///
/// Validation order:
/// 1. user_name unique (platform-wide)
/// 2. role_ids (if any) exist in current tenant
/// 3. password hash via bcrypt
/// 4. tx: INSERT sys_user → INSERT sys_user_tenant → REPLACE sys_user_role
/// 5. commit
/// 6. fetch role_ids for response shape
pub async fn create(
    state: &AppState,
    dto: CreateUserDto,
) -> Result<UserDetailResponseDto, AppError> {
    // 1. user_name uniqueness
    let unique = UserRepo::verify_user_name_unique(&state.pg, &dto.user_name)
        .await
        .into_internal()?;
    (!unique).business_err_if(ResponseCode::DUPLICATE_KEY)?;

    // 2. role_ids validation
    let roles_ok = RoleRepo::verify_role_ids_in_tenant(&state.pg, &dto.role_ids)
        .await
        .into_internal()?;
    (!roles_ok).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    // 3. hash password
    let password_hash = hash_password(&dto.password)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    // 4. transaction
    let mut tx = state
        .pg
        .begin()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("begin tx: {e}")))?;

    let user = UserRepo::insert_tx(
        &mut tx,
        &dto.user_name,
        &dto.nick_name,
        &password_hash,
        dto.dept_id.as_deref(),
        &dto.email,
        &dto.phonenumber,
        &dto.sex,
        &dto.avatar,
        &dto.status,
        dto.remark.as_deref(),
    )
    .await
    .into_internal()?;

    UserRepo::insert_user_tenant_binding_tx(&mut tx, &user.user_id)
        .await
        .into_internal()?;

    RoleRepo::replace_user_roles_tx(&mut tx, &user.user_id, &dto.role_ids)
        .await
        .into_internal()?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("commit tx: {e}")))?;

    Ok(UserDetailResponseDto::from_entity(user, dto.role_ids))
}
```

- [ ] **Step 2: Handler + route**

Append handler:

```rust
use axum::routing::post;
use framework::extractors::ValidatedJson;

async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateUserDto>,
) -> Result<ApiResponse<dto::UserDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}
```

Add to `router()`:

```rust
.route(
    "/system/user/",
    post(create).route_layer(require_permission!("system:user:add")),
)
```

- [ ] **Step 3: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo build -p app 2>&1 | tail -3
cargo test --workspace 2>&1 | grep "test result"
```

Expected: 116 passing. Build green.

---

### Task 14: Manual smoke — create user + verify in DB

- [ ] **Step 1: Create a user, verify 3 rows**

```bash
pkill -f target/debug/app 2>/dev/null; sleep 1
./target/debug/app > /tmp/tea-rs-app-task14.log 2>&1 &
APP_PID=$!
sleep 2

TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")

# Pick a real role id from option-select
ROLE_ID=$(curl -sS http://127.0.0.1:18080/api/v1/system/role/option-select \
  -H "Authorization: Bearer $TOKEN" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data'][0]['roleId'])")
echo "using role: $ROLE_ID"

echo "==== POST /system/user/ ===="
curl -sS -X POST http://127.0.0.1:18080/api/v1/system/user/ \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"userName\":\"it-task14-user\",\"nickName\":\"it-task14\",\"password\":\"abc123\",\"roleIds\":[\"$ROLE_ID\"]}" \
  | python3 -m json.tool

echo "==== DB checks ===="
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "SELECT user_id, user_name, status FROM sys_user WHERE user_name='it-task14-user';"
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "SELECT ur.user_id, ur.role_id FROM sys_user_role ur \
   JOIN sys_user u ON u.user_id = ur.user_id WHERE u.user_name='it-task14-user';"
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "SELECT ut.tenant_id, ut.is_default, ut.status FROM sys_user_tenant ut \
   JOIN sys_user u ON u.user_id = ut.user_id WHERE u.user_name='it-task14-user';"

# Cleanup
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "DELETE FROM sys_user_role WHERE user_id IN (SELECT user_id FROM sys_user WHERE user_name='it-task14-user'); \
   DELETE FROM sys_user_tenant WHERE user_id IN (SELECT user_id FROM sys_user WHERE user_name='it-task14-user'); \
   DELETE FROM sys_user WHERE user_name='it-task14-user';"

kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected:
- POST returns `{code:200, data:{userId: "...", roleIds:[one id]}}`
- `sys_user` has 1 row
- `sys_user_role` has 1 row with the role id
- `sys_user_tenant` has 1 row with `is_default=t, status='0'`, tenant_id='000000'

---

### Task 15: `UpdateUserDto` + `UserRepo::update_tx` + service update + handler `PUT /`

**Files:**
- Modify: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/domain/user_repo.rs`
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: DTO**

Append to `dto.rs`:

```rust
/// Request body for `PUT /system/user/`. Wire-compatible with
/// NestJS `UpdateUserRequestDto`. `user_id` is in the body.
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserDto {
    #[validate(length(min = 1, max = 36))]
    pub user_id: String,
    pub dept_id: Option<String>,
    #[validate(length(min = 1, max = 30))]
    pub nick_name: String,
    #[validate(length(max = 50))]
    #[serde(default)]
    pub email: String,
    #[validate(length(max = 11))]
    #[serde(default)]
    pub phonenumber: String,
    #[validate(custom(function = "validate_sex_flag"))]
    pub sex: String,
    #[validate(length(max = 255))]
    #[serde(default)]
    pub avatar: String,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
    #[serde(default)]
    pub role_ids: Vec<String>,
}
```

Add a validation unit test:

```rust
#[test]
fn update_user_dto_rejects_empty_user_id() {
    let dto = UpdateUserDto {
        user_id: "".into(),
        dept_id: None,
        nick_name: "x".into(),
        email: "".into(),
        phonenumber: "".into(),
        sex: "0".into(),
        avatar: "".into(),
        status: "0".into(),
        remark: None,
        role_ids: vec![],
    };
    assert!(dto.validate().is_err());
}
```

Update the `use super::{...}` import line to include `UpdateUserDto`.

- [ ] **Step 2: `UserRepo::update_tx`**

Append to `impl UserRepo`:

```rust
/// Update user scalar fields. Tenant guard via EXISTS subquery.
/// Returns rows_affected — 0 means not found in current tenant.
/// Does NOT touch `user_name` (immutable per NestJS contract) or
/// `password` (use `reset_password` for that).
#[instrument(skip_all, fields(user_id = %user_id))]
#[allow(clippy::too_many_arguments)]
pub async fn update_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    nick_name: &str,
    email: &str,
    phonenumber: &str,
    sex: &str,
    avatar: &str,
    status: &str,
    dept_id: Option<&str>,
    remark: Option<&str>,
) -> anyhow::Result<u64> {
    let tenant = current_tenant_scope();
    let updater = super::common::audit_update_by();

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
    .bind(nick_name)
    .bind(email)
    .bind(phonenumber)
    .bind(sex)
    .bind(avatar)
    .bind(status)
    .bind(dept_id)
    .bind(remark)
    .bind(&updater)
    .bind(user_id)
    .bind(tenant.as_deref())
    .execute(&mut **tx)
    .await
    .context("update_tx: update sys_user")?
    .rows_affected();

    Ok(affected)
}
```

- [ ] **Step 3: Service `update`**

Append to `service.rs`:

```rust
use super::dto::UpdateUserDto;

/// Update a user + replace role bindings. Returns `DATA_NOT_FOUND` if
/// the user doesn't exist in the current tenant. Admin edit rules:
/// `user_name` is immutable on admin users — enforced implicitly by
/// this function NOT writing user_name at all (it's not in UpdateUserDto).
pub async fn update(state: &AppState, dto: UpdateUserDto) -> Result<(), AppError> {
    // Validate new role_ids before opening the transaction.
    let roles_ok = RoleRepo::verify_role_ids_in_tenant(&state.pg, &dto.role_ids)
        .await
        .into_internal()?;
    (!roles_ok).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    let mut tx = state
        .pg
        .begin()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("begin tx: {e}")))?;

    let affected = UserRepo::update_tx(
        &mut tx,
        &dto.user_id,
        &dto.nick_name,
        &dto.email,
        &dto.phonenumber,
        &dto.sex,
        &dto.avatar,
        &dto.status,
        dto.dept_id.as_deref(),
        dto.remark.as_deref(),
    )
    .await
    .into_internal()?;

    if affected == 0 {
        return Err(AppError::Business {
            code: ResponseCode::DATA_NOT_FOUND,
            msg: None,
        });
    }

    // Replace role bindings only if the scalar update succeeded.
    RoleRepo::replace_user_roles_tx(&mut tx, &dto.user_id, &dto.role_ids)
        .await
        .into_internal()?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("commit tx: {e}")))?;

    Ok(())
}
```

**Note on `AppError::Business` shape**: the exact construction form may differ from the snippet above. If `AppError::Business { code, msg }` doesn't compile, check `framework::error::AppError` for the actual variant shape. The existing role service uses `.business_err_if(code)` for this — prefer that form if possible:

```rust
(affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)?;
```

- [ ] **Step 4: Handler + route**

Append handler:

```rust
use axum::routing::put;

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateUserDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}
```

Add to `router()`:

```rust
.route(
    "/system/user/",
    put(update).route_layer(require_permission!("system:user:edit")),
)
```

- [ ] **Step 5: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo test -p modules --lib 'system::user::dto' 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo build -p app 2>&1 | tail -3
```

Expected: 117 passing (116 + 1 new DTO test).

---

### Task 16: Guards helper — `is_super_admin_user` + self-check

**Files:**
- Modify: `crates/modules/src/system/user/service.rs`

- [ ] **Step 1: Helper functions**

Append to `service.rs`, near the top (after imports, before public functions):

```rust
/// Returns true if `user_id` corresponds to the system super-admin row
/// — `user_name = 'admin' AND platform_id = '000000'`. Used by guard
/// checks to block operations on the superuser.
async fn is_super_admin_user(
    state: &AppState,
    user_id: &str,
) -> Result<bool, AppError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sys_user \
          WHERE user_id = $1 \
            AND user_name = 'admin' \
            AND platform_id = '000000' \
            AND del_flag = '0'",
    )
    .bind(user_id)
    .fetch_one(&state.pg)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("is_super_admin_user: {e}")))?;
    Ok(count > 0)
}

/// Returns true if `target_user_id` matches the caller's user_id from
/// the current RequestContext. Used by self-op guards (can't delete
/// yourself, can't change your own status, can't change your own roles).
fn is_self_op(target_user_id: &str) -> bool {
    framework::context::current_request_context()
        .and_then(|ctx| ctx.user_id.clone())
        .is_some_and(|uid| uid == target_user_id)
}
```

**Note**: `framework::context::current_request_context` and `ctx.user_id` field names need to match what the framework actually exposes. Inspect `crates/framework/src/context/mod.rs` and adjust. The existing `auth::service` module almost certainly uses the same accessor — grep for the pattern.

- [ ] **Step 2: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
```

---

### Task 17: `ChangeUserStatusDto` + `UserRepo::change_status` + service + handler

**Files:**
- Modify: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/domain/user_repo.rs`
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: DTO**

Append to `dto.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ChangeUserStatusDto {
    #[validate(length(min = 1, max = 36))]
    pub user_id: String,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
}
```

Add one validation test. Update the tests `use` line to include `ChangeUserStatusDto`.

- [ ] **Step 2: Repo method**

Append to `impl UserRepo`:

```rust
/// Flip user status with tenant + soft-delete guards. Returns
/// rows_affected — 0 means not found in current tenant.
#[instrument(skip_all, fields(user_id = %user_id, status = %status))]
pub async fn change_status(
    pool: &PgPool,
    user_id: &str,
    status: &str,
) -> anyhow::Result<u64> {
    let tenant = current_tenant_scope();
    let updater = super::common::audit_update_by();
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
```

- [ ] **Step 3: Service with self + admin guards**

Append to `service.rs`:

```rust
use super::dto::ChangeUserStatusDto;

/// Flip a user's status. Guards:
/// - target cannot be self (self-lockout protection)
/// - target cannot be super admin
pub async fn change_status(
    state: &AppState,
    dto: ChangeUserStatusDto,
) -> Result<(), AppError> {
    // Self-guard
    if is_self_op(&dto.user_id) {
        return Err(AppError::Business {
            code: ResponseCode::OPERATION_NOT_ALLOWED,
            msg: Some("不能对自己执行该操作".into()),
        });
    }

    // Admin-guard
    if is_super_admin_user(state, &dto.user_id).await? {
        return Err(AppError::Business {
            code: ResponseCode::OPERATION_NOT_ALLOWED,
            msg: Some("不能对超级管理员执行该操作".into()),
        });
    }

    let affected = UserRepo::change_status(&state.pg, &dto.user_id, &dto.status)
        .await
        .into_internal()?;
    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)
}
```

**Note**: `AppError::Business { code, msg: Some("...") }` shape — check the actual variant. If the `msg` field is different or the variant takes different params, use `.business_err_if` for the common case and raw construction only where a custom msg is needed. Inspect `framework::error::AppError` first.

- [ ] **Step 4: Handler + route**

Append handler:

```rust
use framework::require_role;

async fn change_status(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ChangeUserStatusDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::change_status(&state, dto).await?;
    Ok(ApiResponse::success())
}
```

Add route:

```rust
.route(
    "/system/user/change-status",
    put(change_status).route_layer(require_role!("TENANT_ADMIN")),
)
```

- [ ] **Step 5: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo test -p modules --lib 'system::user::dto' 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

---

### Task 18: `DELETE /system/user/:id` (soft delete with guards)

**Files:**
- Modify: `crates/modules/src/domain/user_repo.rs`
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: Repo soft delete**

Append to `impl UserRepo`:

```rust
/// Soft-delete a user (sets `del_flag = '1'`). Tenant guard via EXISTS.
#[instrument(skip_all, fields(user_id = %user_id))]
pub async fn soft_delete_by_id(pool: &PgPool, user_id: &str) -> anyhow::Result<u64> {
    let tenant = current_tenant_scope();
    let updater = super::common::audit_update_by();
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
```

- [ ] **Step 2: Service `remove` — supports CSV multi-id from NestJS contract**

Append to `service.rs`:

```rust
/// Soft-delete one or more users. NestJS accepts a comma-separated list
/// in the path segment (`DELETE /system/user/id1,id2,id3`). We split,
/// apply guards to each, and process them in sequence. Any guard
/// violation aborts the whole batch (no partial success).
///
/// Guards apply per-target:
/// - cannot delete self
/// - cannot delete super admin
pub async fn remove(state: &AppState, path_ids: &str) -> Result<(), AppError> {
    let ids: Vec<&str> = path_ids.split(',').filter(|s| !s.is_empty()).collect();
    if ids.is_empty() {
        return Err(AppError::Business {
            code: ResponseCode::PARAM_INVALID,
            msg: Some("userIds cannot be empty".into()),
        });
    }

    // Validate all guards first — abort before any DB writes if any
    // target is invalid.
    for id in &ids {
        if is_self_op(id) {
            return Err(AppError::Business {
                code: ResponseCode::OPERATION_NOT_ALLOWED,
                msg: Some("不能删除自己".into()),
            });
        }
        if is_super_admin_user(state, id).await? {
            return Err(AppError::Business {
                code: ResponseCode::OPERATION_NOT_ALLOWED,
                msg: Some("不能删除超级管理员".into()),
            });
        }
    }

    // Apply deletes. Any affected=0 surfaces as DATA_NOT_FOUND for the
    // specific id that missed.
    for id in &ids {
        let affected = UserRepo::soft_delete_by_id(&state.pg, id)
            .await
            .into_internal()?;
        (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)?;
    }

    Ok(())
}
```

- [ ] **Step 3: Handler + route**

Append handler:

```rust
use axum::routing::delete;

async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}
```

Add route:

```rust
.route(
    "/system/user/{id}",
    delete(remove).route_layer(require_role!("TENANT_ADMIN")),
)
```

- [ ] **Step 4: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo build -p app 2>&1 | tail -3
```

---

### Task 19: Week 2 manual smoke gate

**Files:** none (live curl)

- [ ] **Step 1: Full Week 2 sequence**

```bash
pkill -f target/debug/app 2>/dev/null; sleep 1
./target/debug/app > /tmp/tea-rs-app-week2-user.log 2>&1 &
APP_PID=$!
sleep 2

TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
ROLE_ID=$(curl -sS http://127.0.0.1:18080/api/v1/system/role/option-select \
  -H "Authorization: Bearer $TOKEN" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data'][0]['roleId'])")

echo "==== 1. POST create ===="
CREATED=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/system/user/ \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"userName\":\"it-week2-user\",\"nickName\":\"it-week2\",\"password\":\"abc123\",\"roleIds\":[\"$ROLE_ID\"]}")
echo "$CREATED" | python3 -m json.tool
NEW_ID=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['userId'])")

echo "==== 2. GET by id ===="
curl -sS "http://127.0.0.1:18080/api/v1/system/user/$NEW_ID" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

echo "==== 3. PUT update ===="
curl -sS -X PUT http://127.0.0.1:18080/api/v1/system/user/ \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"userId\":\"$NEW_ID\",\"nickName\":\"it-week2-updated\",\"email\":\"x@y.com\",\"phonenumber\":\"\",\"sex\":\"0\",\"avatar\":\"\",\"status\":\"0\",\"roleIds\":[]}" \
  | python3 -m json.tool

echo "==== 4. PUT change-status to 1 ===="
curl -sS -X PUT http://127.0.0.1:18080/api/v1/system/user/change-status \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"userId\":\"$NEW_ID\",\"status\":\"1\"}" | python3 -m json.tool

echo "==== 5. change-status on admin (expect 1004) ===="
ADMIN_ID="cf827fc0-e7cc-4b9f-913c-e20628ade20a"
curl -sS -X PUT http://127.0.0.1:18080/api/v1/system/user/change-status \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"userId\":\"$ADMIN_ID\",\"status\":\"1\"}" | python3 -m json.tool

echo "==== 6. change-status on self (expect 1004) ===="
curl -sS -X PUT http://127.0.0.1:18080/api/v1/system/user/change-status \
  -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"userId\":\"$ADMIN_ID\",\"status\":\"1\"}" | python3 -m json.tool

echo "==== 7. DELETE ===="
curl -sS -X DELETE "http://127.0.0.1:18080/api/v1/system/user/$NEW_ID" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

echo "==== 8. DELETE admin (expect 1004) ===="
curl -sS -X DELETE "http://127.0.0.1:18080/api/v1/system/user/$ADMIN_ID" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

echo "==== 9. GET after delete (expect 1001) ===="
curl -sS "http://127.0.0.1:18080/api/v1/system/user/$NEW_ID" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

# Cleanup
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "DELETE FROM sys_user_role WHERE user_id='$NEW_ID'; \
   DELETE FROM sys_user_tenant WHERE user_id='$NEW_ID'; \
   DELETE FROM sys_user WHERE user_id='$NEW_ID';"
kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected: 1/2/3/4/7 return 200; 5/6/8 return 1004; 9 returns 1001.

- [ ] **Step 2: Workspace checks**

```bash
cargo test --workspace 2>&1 | grep "test result"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -3
bash scripts/smoke-role-module.sh 2>&1 | tail -3
```

Expected: 118 passing (113 + CreateUserDto tests × 3 + UpdateUserDto test × 1 + ChangeUserStatusDto test × 1 = 118). Clippy clean. Role smoke still 14/14.

---

### Task 20: (placeholder for any mid-week-2 rework) — skip if Task 19 green

If Task 19's Week 2 smoke gate passes without issues, this task is a no-op and can be marked complete. If issues surface, capture them here as sub-steps and fix before proceeding.

---

# WEEK 3 — Reset-pwd + auth-role + tests + exit (Tasks 21-27)

---

### Task 21: `PUT /system/user/reset-pwd`

**Files:**
- Modify: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/domain/user_repo.rs`
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: DTO**

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ResetPwdDto {
    #[validate(length(min = 1, max = 36))]
    pub user_id: String,
    #[validate(length(min = 6, max = 20))]
    pub password: String,
}
```

Add 1 validation test. Import `ResetPwdDto` in tests.

- [ ] **Step 2: Repo `reset_password`**

```rust
#[instrument(skip_all, fields(user_id = %user_id))]
pub async fn reset_password(
    pool: &PgPool,
    user_id: &str,
    password_hash: &str,
) -> anyhow::Result<u64> {
    let tenant = current_tenant_scope();
    let updater = super::common::audit_update_by();
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
```

- [ ] **Step 3: Service**

```rust
use super::dto::ResetPwdDto;

/// Admin reset of another user's password. Blocks reset of super admin.
/// Invalidates the target user's Redis sessions after successful update
/// so their existing JWTs stop working.
pub async fn reset_password(
    state: &AppState,
    dto: ResetPwdDto,
) -> Result<(), AppError> {
    if is_super_admin_user(state, &dto.user_id).await? {
        return Err(AppError::Business {
            code: ResponseCode::OPERATION_NOT_ALLOWED,
            msg: Some("不能重置超级管理员的密码".into()),
        });
    }

    let password_hash = hash_password(&dto.password)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;

    let affected = UserRepo::reset_password(&state.pg, &dto.user_id, &password_hash)
        .await
        .into_internal()?;
    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    // Best-effort session invalidation. A failure here is logged but
    // does NOT roll back the password change — the user will still be
    // unable to log in with the old password, which is the critical
    // invariant.
    if let Err(e) = framework::infra::session::invalidate_user_sessions(
        &state.redis,
        &dto.user_id,
    )
    .await
    {
        tracing::warn!(error = %e, user_id = %dto.user_id, "session invalidation failed");
    }

    Ok(())
}
```

**Note**: `framework::infra::session::invalidate_user_sessions` is the expected Phase 0 helper — inspect the actual framework session module and adjust the call. If no helper exists, either:
- (a) skip the invalidation step with a `// TODO: wire session invalidation once framework exposes helper`; OR
- (b) call a direct Redis DEL pattern on the session key

Prefer (a) — don't invent new framework APIs inline.

- [ ] **Step 4: Handler + route**

```rust
async fn reset_password(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ResetPwdDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::reset_password(&state, dto).await?;
    Ok(ApiResponse::success())
}
```

Route:

```rust
.route(
    "/system/user/reset-pwd",
    put(reset_password).route_layer(require_role!("TENANT_ADMIN")),
)
```

- [ ] **Step 5: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo test -p modules --lib 'system::user::dto' 2>&1 | tail -10
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

---

### Task 22: `GET /system/user/auth-role/:id`

**Files:**
- Modify: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: DTO**

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRoleResponseDto {
    pub user: UserDetailResponseDto,
    pub role_ids: Vec<String>,
}
```

- [ ] **Step 2: Service**

```rust
/// Return the target user's profile + current role bindings.
/// Tenant-scoped via find_by_id.
pub async fn find_auth_role(
    state: &AppState,
    user_id: &str,
) -> Result<super::dto::AuthRoleResponseDto, AppError> {
    let user = UserRepo::find_by_id_tenant_scoped(&state.pg, user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    let role_ids = RoleRepo::find_role_ids_by_user(&state.pg, &user.user_id)
        .await
        .into_internal()?;

    Ok(super::dto::AuthRoleResponseDto {
        user: UserDetailResponseDto::from_entity(user, role_ids.clone()),
        role_ids,
    })
}
```

- [ ] **Step 3: Handler + route**

```rust
async fn auth_role(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<ApiResponse<dto::AuthRoleResponseDto>, AppError> {
    let resp = service::find_auth_role(&state, &user_id).await?;
    Ok(ApiResponse::ok(resp))
}
```

Route:

```rust
.route(
    "/system/user/auth-role/{id}",
    get(auth_role).route_layer(require_role!("TENANT_ADMIN")),
)
```

- [ ] **Step 4: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

---

### Task 23: `PUT /system/user/auth-role`

**Files:**
- Modify: `crates/modules/src/system/user/dto.rs`
- Modify: `crates/modules/src/system/user/service.rs`
- Modify: `crates/modules/src/system/user/handler.rs`

- [ ] **Step 1: DTO**

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AuthRoleUpdateDto {
    #[validate(length(min = 1, max = 36))]
    pub user_id: String,
    #[serde(default)]
    pub role_ids: Vec<String>,
}
```

- [ ] **Step 2: Service**

```rust
use super::dto::AuthRoleUpdateDto;

/// Replace a user's role bindings entirely. Guards:
/// - cannot modify own roles
/// - cannot modify super admin's roles
/// Validates all role_ids exist in current tenant before opening tx.
pub async fn update_auth_role(
    state: &AppState,
    dto: AuthRoleUpdateDto,
) -> Result<(), AppError> {
    if is_self_op(&dto.user_id) {
        return Err(AppError::Business {
            code: ResponseCode::OPERATION_NOT_ALLOWED,
            msg: Some("不能修改自己的角色".into()),
        });
    }
    if is_super_admin_user(state, &dto.user_id).await? {
        return Err(AppError::Business {
            code: ResponseCode::OPERATION_NOT_ALLOWED,
            msg: Some("不能修改超级管理员的角色".into()),
        });
    }

    // Verify target user exists in tenant before touching role bindings.
    UserRepo::find_by_id_tenant_scoped(&state.pg, &dto.user_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;

    let roles_ok = RoleRepo::verify_role_ids_in_tenant(&state.pg, &dto.role_ids)
        .await
        .into_internal()?;
    (!roles_ok).business_err_if(ResponseCode::DATA_NOT_FOUND)?;

    let mut tx = state
        .pg
        .begin()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("begin tx: {e}")))?;

    RoleRepo::replace_user_roles_tx(&mut tx, &dto.user_id, &dto.role_ids)
        .await
        .into_internal()?;

    tx.commit()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("commit tx: {e}")))?;

    Ok(())
}
```

- [ ] **Step 3: Handler + route**

```rust
async fn update_auth_role(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthRoleUpdateDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_auth_role(&state, dto).await?;
    Ok(ApiResponse::success())
}
```

Route:

```rust
.route(
    "/system/user/auth-role",
    put(update_auth_role).route_layer(require_role!("TENANT_ADMIN")),
)
```

- [ ] **Step 4: Verification**

```bash
cargo check -p modules 2>&1 | tail -5
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo build -p app 2>&1 | tail -3
cargo test --workspace 2>&1 | grep "test result"
```

Expected: 119 passing.

---

### Task 24: Integration test suite (~22 tests)

**Files:**
- Create: `crates/modules/tests/user_module_tests.rs`

- [ ] **Step 1: Scaffold**

Create `user_module_tests.rs`:

```rust
//! Integration tests for the user module. Real DB at 127.0.0.1:5432/saas_tea.
//! Run with `--test-threads=1` to avoid concurrent cleanup conflicts.

#[path = "common/mod.rs"]
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use framework::error::AppError;
use framework::response::ResponseCode;
use modules::domain::{RoleRepo, UserRepo};
use modules::system::user::{dto, service};
use tower::ServiceExt;

const ADMIN_USER_ID: &str = "cf827fc0-e7cc-4b9f-913c-e20628ade20a";

fn make_create_dto(prefix: &str, user_name: &str, role_ids: Vec<String>) -> dto::CreateUserDto {
    dto::CreateUserDto {
        dept_id: None,
        nick_name: format!("{prefix}-nick"),
        user_name: user_name.into(),
        password: "abc123".into(),
        email: "".into(),
        phonenumber: "".into(),
        sex: "2".into(),
        avatar: "".into(),
        status: "0".into(),
        remark: None,
        role_ids,
    }
}
```

- [ ] **Step 2: Basic 401 wiring test**

```rust
#[tokio::test]
async fn user_find_by_id_rejects_unauthenticated_with_401() {
    let (_state, router) = common::build_state_and_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/system/user/does-not-exist-xyz")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 3: CRUD + retrieval tests (8)**

```rust
#[tokio::test]
async fn create_and_find_by_id_happy_path() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-create-find-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let created = service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect("create");

        let fetched = service::find_by_id(&state, &created.user_id).await.expect("find");
        assert_eq!(fetched.user_name, user_name);
        assert_eq!(fetched.role_ids.len(), 0);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn create_with_roles_persists_bindings() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-create-roles-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        // Pick 2 real role ids from the DB
        let option_roles = RoleRepo::find_option_list(&state.pg).await.expect("roles");
        assert!(option_roles.len() >= 2, "need at least 2 seed roles");
        let role_ids: Vec<String> = option_roles.iter().take(2).map(|r| r.role_id.clone()).collect();

        let created = service::create(
            &state,
            make_create_dto(prefix, &user_name, role_ids.clone()),
        )
        .await
        .expect("create");

        assert_eq!(created.role_ids.len(), 2);
        for rid in &role_ids {
            assert!(created.role_ids.contains(rid));
        }

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn create_fails_on_duplicate_user_name() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-dup-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect("first create");

        let err = service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect_err("second create should fail");

        match err {
            AppError::Business { code, .. } => {
                assert_eq!(code, ResponseCode::DUPLICATE_KEY);
            }
            other => panic!("expected DUPLICATE_KEY, got {other:?}"),
        }

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn find_by_id_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;
    common::as_super_admin(async {
        let err = service::find_by_id(&state, "00000000-0000-0000-0000-ffffffffffff")
            .await
            .expect_err("should 1001");
        match err {
            AppError::Business { code, .. } => assert_eq!(code, ResponseCode::DATA_NOT_FOUND),
            other => panic!("expected DATA_NOT_FOUND, got {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn find_by_id_after_soft_delete_returns_not_found() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-softdel-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let created = service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect("create");

        // Note: remove() takes a comma-separated string of ids
        service::remove(&state, &created.user_id).await.expect("remove");

        let err = service::find_by_id(&state, &created.user_id)
            .await
            .expect_err("should be 1001 after soft delete");
        match err {
            AppError::Business { code, .. } => assert_eq!(code, ResponseCode::DATA_NOT_FOUND),
            other => panic!("expected DATA_NOT_FOUND, got {other:?}"),
        }
        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn list_finds_seeded_user_by_user_name_filter() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-listfilter-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect("create");

        let query = dto::ListUserDto {
            user_name: Some(user_name.clone()),
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: framework::response::PageQuery::default(),
        };
        let page = service::list(&state, query).await.expect("list");
        assert_eq!(page.total, 1);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn list_pagination_metadata_correct() {
    let (state, _router) = common::build_state_and_router().await;
    common::as_super_admin(async {
        let query = dto::ListUserDto {
            user_name: None,
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: framework::response::PageQuery {
                page_num: 1,
                page_size: 2,
            },
        };
        let page = service::list(&state, query).await.expect("list");
        assert_eq!(page.page_num, 1);
        assert_eq!(page.page_size, 2);
    })
    .await;
}

#[tokio::test]
async fn option_select_excludes_disabled_users() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-optdisable-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let created = service::create(
            &state,
            make_create_dto(prefix, &user_name, vec![]),
        )
        .await
        .expect("create");

        // Disable the user
        service::change_status(
            &state,
            dto::ChangeUserStatusDto {
                user_id: created.user_id.clone(),
                status: "1".into(),
            },
        )
        .await
        .expect("change_status");

        let options = service::option_select(
            &state,
            dto::UserOptionQueryDto {
                user_name: Some(user_name.clone()),
            },
        )
        .await
        .expect("option_select");
        assert!(options.iter().all(|u| u.user_name != user_name));

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}
```

- [ ] **Step 4: Update / change-status / guards tests (8)**

```rust
#[tokio::test]
async fn update_replaces_role_bindings() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-update-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let option_roles = RoleRepo::find_option_list(&state.pg).await.expect("roles");
        assert!(option_roles.len() >= 2);
        let role_a = option_roles[0].role_id.clone();
        let role_b = option_roles[1].role_id.clone();

        let created = service::create(
            &state,
            make_create_dto(prefix, &user_name, vec![role_a.clone()]),
        )
        .await
        .expect("create");

        service::update(
            &state,
            dto::UpdateUserDto {
                user_id: created.user_id.clone(),
                dept_id: None,
                nick_name: "updated".into(),
                email: "".into(),
                phonenumber: "".into(),
                sex: "0".into(),
                avatar: "".into(),
                status: "0".into(),
                remark: None,
                role_ids: vec![role_b.clone()],
            },
        )
        .await
        .expect("update");

        let fetched = service::find_by_id(&state, &created.user_id).await.expect("find");
        assert_eq!(fetched.nick_name, "updated");
        assert_eq!(fetched.role_ids.len(), 1);
        assert_eq!(fetched.role_ids[0], role_b);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn change_status_flips_and_persists() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-chst-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let created = service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect("create");

        service::change_status(
            &state,
            dto::ChangeUserStatusDto {
                user_id: created.user_id.clone(),
                status: "1".into(),
            },
        )
        .await
        .expect("change_status");

        let fetched = service::find_by_id(&state, &created.user_id).await.expect("find");
        assert_eq!(fetched.status, "1");

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn change_status_on_admin_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;
    common::as_super_admin(async {
        let err = service::change_status(
            &state,
            dto::ChangeUserStatusDto {
                user_id: ADMIN_USER_ID.into(),
                status: "1".into(),
            },
        )
        .await
        .expect_err("should block");
        match err {
            AppError::Business { code, .. } => {
                assert_eq!(code, ResponseCode::OPERATION_NOT_ALLOWED);
            }
            other => panic!("expected OPERATION_NOT_ALLOWED, got {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn change_status_on_self_is_blocked() {
    // as_super_admin sets user_id = "it-admin" per the harness — self-op
    // is detected by that id, not the real admin row. Use it-admin as
    // both target and self to trigger the guard.
    let (state, _router) = common::build_state_and_router().await;
    common::as_super_admin(async {
        let err = service::change_status(
            &state,
            dto::ChangeUserStatusDto {
                user_id: "it-admin".into(),
                status: "1".into(),
            },
        )
        .await
        .expect_err("should block");
        match err {
            AppError::Business { code, .. } => {
                assert_eq!(code, ResponseCode::OPERATION_NOT_ALLOWED);
            }
            other => panic!("expected OPERATION_NOT_ALLOWED, got {other:?}"),
        }
    })
    .await;
}

#[tokio::test]
async fn remove_soft_deletes_and_blocks_admin() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-remove-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let created = service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect("create");

        service::remove(&state, &created.user_id).await.expect("remove");

        // Verify find returns 1001
        let err = service::find_by_id(&state, &created.user_id)
            .await
            .expect_err("should be gone");
        match err {
            AppError::Business { code, .. } => assert_eq!(code, ResponseCode::DATA_NOT_FOUND),
            _ => panic!("wrong error"),
        }

        // Admin delete is blocked
        let err = service::remove(&state, ADMIN_USER_ID).await.expect_err("should block");
        match err {
            AppError::Business { code, .. } => {
                assert_eq!(code, ResponseCode::OPERATION_NOT_ALLOWED);
            }
            _ => panic!("wrong error"),
        }

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn reset_password_updates_hash() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-resetpw-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let created = service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect("create");

        service::reset_password(
            &state,
            dto::ResetPwdDto {
                user_id: created.user_id.clone(),
                password: "newpass123".into(),
            },
        )
        .await
        .expect("reset");

        // Verify via raw query — password column changed and is a valid bcrypt hash
        let row: (String,) = sqlx::query_as("SELECT password FROM sys_user WHERE user_id = $1")
            .bind(&created.user_id)
            .fetch_one(&state.pg)
            .await
            .unwrap();
        assert!(row.0.starts_with("$2"), "not a bcrypt hash: {}", row.0);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn reset_password_on_admin_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;
    common::as_super_admin(async {
        let err = service::reset_password(
            &state,
            dto::ResetPwdDto {
                user_id: ADMIN_USER_ID.into(),
                password: "anything123".into(),
            },
        )
        .await
        .expect_err("should block");
        match err {
            AppError::Business { code, .. } => {
                assert_eq!(code, ResponseCode::OPERATION_NOT_ALLOWED);
            }
            _ => panic!("wrong error"),
        }
    })
    .await;
}

#[tokio::test]
async fn update_auth_role_replaces_bindings() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-authrole-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let option_roles = RoleRepo::find_option_list(&state.pg).await.expect("roles");
        assert!(option_roles.len() >= 2);
        let role_a = option_roles[0].role_id.clone();
        let role_b = option_roles[1].role_id.clone();

        let created = service::create(
            &state,
            make_create_dto(prefix, &user_name, vec![role_a]),
        )
        .await
        .expect("create");

        service::update_auth_role(
            &state,
            dto::AuthRoleUpdateDto {
                user_id: created.user_id.clone(),
                role_ids: vec![role_b.clone()],
            },
        )
        .await
        .expect("update_auth_role");

        let fetched = service::find_auth_role(&state, &created.user_id).await.expect("find");
        assert_eq!(fetched.role_ids, vec![role_b]);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn update_auth_role_on_admin_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;
    common::as_super_admin(async {
        let err = service::update_auth_role(
            &state,
            dto::AuthRoleUpdateDto {
                user_id: ADMIN_USER_ID.into(),
                role_ids: vec![],
            },
        )
        .await
        .expect_err("should block");
        match err {
            AppError::Business { code, .. } => {
                assert_eq!(code, ResponseCode::OPERATION_NOT_ALLOWED);
            }
            _ => panic!("wrong error"),
        }
    })
    .await;
}

#[tokio::test]
async fn update_auth_role_with_invalid_role_id_rejects() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-badrole-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let created = service::create(&state, make_create_dto(prefix, &user_name, vec![]))
            .await
            .expect("create");

        let err = service::update_auth_role(
            &state,
            dto::AuthRoleUpdateDto {
                user_id: created.user_id.clone(),
                role_ids: vec!["00000000-0000-0000-0000-ffffffffffff".into()],
            },
        )
        .await
        .expect_err("should reject");
        match err {
            AppError::Business { code, .. } => {
                assert_eq!(code, ResponseCode::DATA_NOT_FOUND);
            }
            _ => panic!("wrong error"),
        }

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn find_auth_role_returns_current_roles() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-findauth-";
    let user_name = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;
        let option_roles = RoleRepo::find_option_list(&state.pg).await.expect("roles");
        let role_id = option_roles[0].role_id.clone();

        let created = service::create(
            &state,
            make_create_dto(prefix, &user_name, vec![role_id.clone()]),
        )
        .await
        .expect("create");

        let auth = service::find_auth_role(&state, &created.user_id).await.expect("find");
        assert_eq!(auth.role_ids, vec![role_id]);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

#[tokio::test]
async fn find_auth_role_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;
    common::as_super_admin(async {
        let err = service::find_auth_role(&state, "00000000-0000-0000-0000-ffffffffffff")
            .await
            .expect_err("should 1001");
        match err {
            AppError::Business { code, .. } => assert_eq!(code, ResponseCode::DATA_NOT_FOUND),
            _ => panic!("wrong error"),
        }
    })
    .await;
}
```

- [ ] **Step 5: Run integration tests**

```bash
cargo test --test user_module_tests -- --test-threads=1 2>&1 | tail -40
```

Expected: 22/22 passing (21 new + 1 401-wiring).

- [ ] **Step 6: Workspace green**

```bash
cargo test --workspace 2>&1 | grep "test result"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
```

Expected: ~**148 passing** (110 baseline + ~16 unit + 22 integration).

---

### Task 25: `scripts/smoke-user-module.sh`

**Files:**
- Create: `server-rs/scripts/smoke-user-module.sh`

- [ ] **Step 1: Write script**

Create `scripts/smoke-user-module.sh`:

```bash
#!/usr/bin/env bash
# scripts/smoke-user-module.sh
#
# End-to-end verification of all 11 user endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
PREFIX="it-smoke-user-$(date +%s)"
USER_NAME="${PREFIX}-u"
TOKEN=""
NEW_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  if [[ -n "${NEW_ID:-}" ]]; then
    PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
      "DELETE FROM sys_user_role WHERE user_id='$NEW_ID'; \
       DELETE FROM sys_user_tenant WHERE user_id='$NEW_ID'; \
       DELETE FROM sys_user WHERE user_id='$NEW_ID';" || true
  fi
  CLEANUP_DONE=true
}

assert_eq() {
  local expected="$1" actual="$2" msg="$3"
  if [[ "$expected" != "$actual" ]]; then
    echo "FAIL: $msg (expected '$expected', got '$actual')"
    exit 1
  fi
  echo "  OK: $msg"
}

step() { echo ""; echo "=== $1 ==="; }

step "1. login"
TOKEN=$(curl -sS -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
assert_eq true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

step "2. pick a real role id"
ROLE_ID=$(curl -sS "$BASE/system/role/option-select" "${H[@]}" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data'][0]['roleId'])")
echo "role_id: $ROLE_ID"

step "3. POST create user"
CREATED=$(curl -sS -X POST "$BASE/system/user/" "${H[@]}" \
  -d "{\"userName\":\"$USER_NAME\",\"nickName\":\"$USER_NAME-nick\",\"password\":\"abc123\",\"roleIds\":[\"$ROLE_ID\"]}")
echo "$CREATED" | python3 -m json.tool
NEW_ID=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['userId'])")
assert_eq 36 "${#NEW_ID}" "user_id is a uuid"

step "4. GET list (filter by userName)"
LIST=$(curl -sS "$BASE/system/user/list?userName=$USER_NAME&pageNum=1&pageSize=10" "${H[@]}")
COUNT=$(echo "$LIST" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 1 "$COUNT" "list returns the new user"

step "5. GET /:id"
DETAIL=$(curl -sS "$BASE/system/user/$NEW_ID" "${H[@]}")
ROLE_COUNT=$(echo "$DETAIL" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['roleIds']))")
assert_eq 1 "$ROLE_COUNT" "user has 1 bound role"

step "6. PUT update (change nick + clear roles)"
curl -sS -X PUT "$BASE/system/user/" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"nickName\":\"$USER_NAME-updated\",\"email\":\"\",\"phonenumber\":\"\",\"sex\":\"0\",\"avatar\":\"\",\"status\":\"0\",\"roleIds\":[]}" > /dev/null
DETAIL=$(curl -sS "$BASE/system/user/$NEW_ID" "${H[@]}")
NICK=$(echo "$DETAIL" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['nickName'])")
assert_eq "${USER_NAME}-updated" "$NICK" "nick_name updated"

step "7. PUT change-status disable"
curl -sS -X PUT "$BASE/system/user/change-status" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"status\":\"1\"}" > /dev/null
STATUS=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT status FROM sys_user WHERE user_id='$NEW_ID';" | tr -d ' \n')
assert_eq 1 "$STATUS" "disabled in DB"

step "8. option-select excludes disabled user"
OPTS=$(curl -sS "$BASE/system/user/option-select?userName=$USER_NAME" "${H[@]}")
VISIBLE=$(echo "$OPTS" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(any(u['userName']=='$USER_NAME' for u in d))")
assert_eq False "$VISIBLE" "disabled user hidden from option-select"

step "9. change-status on admin (expect 1004)"
CODE=$(curl -sS -X PUT "$BASE/system/user/change-status" "${H[@]}" \
  -d '{"userId":"cf827fc0-e7cc-4b9f-913c-e20628ade20a","status":"1"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1004 "$CODE" "admin status change blocked"

step "10. Re-enable test user"
curl -sS -X PUT "$BASE/system/user/change-status" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"status\":\"0\"}" > /dev/null

step "11. PUT reset-pwd"
CODE=$(curl -sS -X PUT "$BASE/system/user/reset-pwd" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"password\":\"newpwd1\"}" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$CODE" "reset-pwd succeeded"

step "12. reset-pwd on admin (expect 1004)"
CODE=$(curl -sS -X PUT "$BASE/system/user/reset-pwd" "${H[@]}" \
  -d '{"userId":"cf827fc0-e7cc-4b9f-913c-e20628ade20a","password":"anything1"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1004 "$CODE" "admin reset blocked"

step "13. GET auth-role/:id"
AUTH=$(curl -sS "$BASE/system/user/auth-role/$NEW_ID" "${H[@]}")
AUTH_ROLES=$(echo "$AUTH" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['roleIds']))")
assert_eq 0 "$AUTH_ROLES" "auth-role returns current (empty) role list"

step "14. PUT auth-role with 1 role"
curl -sS -X PUT "$BASE/system/user/auth-role" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"roleIds\":[\"$ROLE_ID\"]}" > /dev/null
AUTH=$(curl -sS "$BASE/system/user/auth-role/$NEW_ID" "${H[@]}")
AUTH_ROLES=$(echo "$AUTH" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['roleIds']))")
assert_eq 1 "$AUTH_ROLES" "auth-role now has 1 role"

step "15. DELETE /:id"
curl -sS -X DELETE "$BASE/system/user/$NEW_ID" "${H[@]}" > /dev/null
CODE=$(curl -sS "$BASE/system/user/$NEW_ID" "${H[@]}" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1001 "$CODE" "find returns 1001 after delete"
DEL_FLAG=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT del_flag FROM sys_user WHERE user_id='$NEW_ID';" | tr -d ' \n')
assert_eq 1 "$DEL_FLAG" "del_flag = 1"

step "16. DELETE admin (expect 1004)"
CODE=$(curl -sS -X DELETE "$BASE/system/user/cf827fc0-e7cc-4b9f-913c-e20628ade20a" "${H[@]}" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1004 "$CODE" "admin delete blocked"

echo ""
echo "ALL 16 STEPS PASSED"
```

- [ ] **Step 2: Make executable + run**

```bash
mkdir -p scripts
chmod +x scripts/smoke-user-module.sh

pkill -f target/debug/app 2>/dev/null; sleep 1
./target/debug/app > /tmp/tea-rs-app-smoke-user.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-user-module.sh
kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected: `ALL 16 STEPS PASSED`.

---

### Task 26: Regression + Week 3 exit gate

**Files:** none (verification)

- [ ] **Step 1: Phase 0 smoke regression**

```bash
pkill -f target/debug/app 2>/dev/null; sleep 1
./target/debug/app > /tmp/tea-rs-app-final.log 2>&1 &
APP_PID=$!
sleep 2

echo "==== /health/live ===="; curl -sS http://127.0.0.1:18080/health/live; echo
echo "==== /health/ready ===="; curl -sS http://127.0.0.1:18080/health/ready; echo
TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
echo "==== /api/v1/info ===="
curl -sS http://127.0.0.1:18080/api/v1/info -H "Authorization: Bearer $TOKEN" \
  | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(f\"userName: {d['userName']}, perms count: {len(d['permissions'])}\")"
echo "==== /auth/logout ===="
curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/logout -H "Authorization: Bearer $TOKEN"; echo

kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected: health up, login/info/logout all green, perms count > 0.

- [ ] **Step 2: Role module regression**

```bash
pkill -f target/debug/app 2>/dev/null; sleep 1
./target/debug/app > /tmp/tea-rs-app-final-role.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-role-module.sh
kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected: `ALL 14 STEPS PASSED`.

- [ ] **Step 3: User module smoke**

```bash
pkill -f target/debug/app 2>/dev/null; sleep 1
./target/debug/app > /tmp/tea-rs-app-final-user.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-user-module.sh
kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected: `ALL 16 STEPS PASSED`.

- [ ] **Step 4: Full workspace + clippy + fmt**

```bash
cargo test --workspace 2>&1 | grep "test result"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --check && echo "fmt ok"
```

Expected: **~148 passing** (actual count: 110 + DTO tests + integration tests), clippy clean, fmt clean.

---

### Task 27: Web frontend cut-over validation

**Files:** none (user-driven manual test)

- [ ] **Step 1: Point Vue web at Rust**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/web
grep -r VITE_API_URL .env* 2>/dev/null

# Either temporarily export OR edit .env.development
VITE_API_URL=http://localhost:18080 pnpm dev
```

- [ ] **Step 2: Manual browser walkthrough**

In one terminal keep Rust app running:

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
./target/debug/app
```

In the browser:
1. Log in as `admin / admin123`
2. Navigate to 系统管理 → 用户管理
3. List page loads — verify users visible, no 404/500 in console
4. Click "新增" — create a user with name, nickname, select 1 role, password
5. Verify it appears in the list
6. Click "编辑" — change nickname, swap roles, save
7. Click status toggle — disable user, verify greyed out
8. Re-enable
9. Click "重置密码" — enter new password, verify 200
10. Click "分配角色" — modify role list, save
11. Click "删除" — soft delete, verify removed from list
12. Attempt to delete / disable / reset-pwd the admin user — verify error toast shows 1004-based message

- [ ] **Step 3: Revert web config**

Restore `web/.env.development` if you edited it.

- [ ] **Step 4: Stop Rust app**

```bash
pkill -f target/debug/app
```

- [ ] **Step 5: Final sub-phase gate**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
cargo test --workspace 2>&1 | grep "test result"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --check && echo "fmt ok"
```

All four of the following must hold:
- Workspace tests all pass (~148)
- Clippy clean
- Fmt clean
- Both smoke scripts pass (`smoke-role-module.sh` + `smoke-user-module.sh`)
- Vue web admin user flow works end-to-end against Rust

**Phase 1 Sub-Phase 2a done.** User module is end-to-end substitutable for NestJS on the admin CRUD surface.

---

## Self-Review Checklist

### Spec coverage

- [x] 11 endpoints: Tasks 7 (list + find_by_id), 8 (option-select), 9 (info), 13 (create), 15 (update), 17 (change-status), 18 (delete), 21 (reset-pwd), 22 (auth-role GET), 23 (auth-role PUT)
- [x] `sys_user_tenant` JOIN pattern: Task 3 (find_by_id) + Task 7 (find_page) + Task 8 (find_option_list)
- [x] `sys_user_tenant` write temporary ownership: Task 12 (insert_user_tenant_binding_tx)
- [x] `sys_user_role` single-owner via role_repo: Tasks 6, 11 (new role_repo methods) + service composes them in tasks 13/15/23
- [x] Password hashing via framework::infra::crypto: Tasks 13, 21
- [x] Self-guard + admin protection: Task 16 (helpers) + Tasks 17, 18, 21, 23 (applied)
- [x] OPERATION_NOT_ALLOWED new code: Task 1
- [x] require_authenticated! macro new + retroactive: Task 1
- [x] fmt_ts promotion: Task 1
- [x] Integration test suite ~22 tests: Task 24
- [x] Smoke script: Task 25
- [x] Regressions + exit gate: Task 26, 27

### Placeholder scan

- [x] No "TBD", "TODO", "implement later" in step contents
- [x] Every step with code has the actual code
- [x] A few "Note: inspect X and adjust if actual names differ" callouts exist — these are explicit instructions to verify against the codebase, not placeholders

### Type consistency

- [x] `UserRepo::find_by_id_tenant_scoped` named consistently across Task 3 and callers (Task 7, 24)
- [x] `RoleRepo::find_role_ids_by_user` named consistently (Task 6 → Task 7 → Task 24)
- [x] `RoleRepo::replace_user_roles_tx` tx-accepting (Task 11) consistently called via `&mut tx` from service layer
- [x] `RoleRepo::verify_role_ids_in_tenant` returns `bool`, service negates + `business_err_if` (Tasks 13, 15, 23)
- [x] `is_super_admin_user` returns `Result<bool, AppError>`, service early-returns on true (Tasks 16, 17, 18, 21, 23)
- [x] `is_self_op(target_user_id)` returns `bool` sync (Task 16), callers branch on truth
- [x] `CreateUserDto.sex` default is `"2"` (unknown), `ChangeUserStatusDto.status` uses `validate_status_flag` custom validator
- [x] DTO imports use `use framework::response::{fmt_ts, PageQuery};` consistently after Task 1 promotes fmt_ts

### Expected test count progression

| After Task | Workspace tests | New this task |
|---|---|---|
| 1 (framework additions) | 110 | +0 (3 fmt_ts tests moved, net zero) |
| 5 (DTOs v1) | 113 | +3 (ListUserDto tests) |
| 10 (Week 1 gate) | 113 | +0 |
| 12 (Create DTO + repo helpers) | 116 | +3 |
| 15 (Update DTO) | 117 | +1 |
| 17 (ChangeUserStatus DTO) | 118 | +1 |
| 19 (Week 2 gate) | 118 | +0 |
| 21 (Reset pw DTO) | 119 | +1 |
| 24 (Integration suite) | **~141** | +22 (includes 1 401 test + 21 functional) |

Final target: **~141 passing** (110 baseline + 9 unit + 22 integration). Week 3 gate threshold is "cargo test --workspace green; actual count ≥ 140".

---

**Plan complete. Saved to `server-rs/docs/plans/2026-04-11-phase1-user-module-plan.md`.**

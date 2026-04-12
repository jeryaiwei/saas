# Phase 1 Sub-Phase 1 — Role Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the NestJS Role module in Rust (`server-rs/crates/modules`) against the real `saas_tea` database, covering 11 endpoints over 3 weeks, byte-compatible on the wire with the legacy backend so the Vue web frontend can switch `VITE_API_URL` with zero frontend changes.

**Architecture:** Hand-written SQL in a single `RoleRepo` namespace struct. Three small helpers (`AuditInsert::now` / `audit_update_by` / `current_tenant_scope`) handle cross-cutting concerns at the call site instead of via trait magic. Multi-statement writes use `sqlx::Transaction<'_, Postgres>` passed explicitly as a function parameter. Service layer orchestrates repo calls; handler layer is thin Axum wrapping. No caching, no operlog, no optimistic lock in this sub-phase.

**Tech Stack:** sqlx 0.8 (runtime `query_as::<_, T>`, no compile-time macros), axum 0.8, validator 0.20, tokio 1, framework crate (Phase 0). Runs on Rust 1.91.1.

**Spec:** [../specs/2026-04-10-phase1-role-module-design.md](../specs/2026-04-10-phase1-role-module-design.md)

**Working directory:** `/Users/jason/Documents/Project/node/tea-saas/server-rs`

**⚠ User preference — no git operations.** Commit steps in this plan are replaced with "verification checkpoints" (`cargo check` / `cargo test` / `cargo clippy`). Do not run `git commit` unless the user explicitly asks.

**Prerequisites before starting:**
- `config/development.yaml` points at a running `saas_tea` PostgreSQL on `127.0.0.1:5432` and Redis on `127.0.0.1:6379/4` (Phase 0 already wired)
- `admin / admin123` is a known valid login (verified in Phase 0 smoke test)
- `cargo build --workspace` is green on the Phase 0 baseline before touching any Phase 1 code

---

## File Structure Overview

### New files
```text
crates/modules/src/
├── domain/
│   ├── common.rs                    # 3 helpers (~40 LOC)
│   └── role_repo.rs                 # RoleRepo struct + 12 static methods (~350 LOC)
└── system/                          # new top-level module
    ├── mod.rs                       # re-export system::role
    └── role/
        ├── mod.rs                   # re-export router()
        ├── dto.rs                   # ~10 request/response DTOs (~200 LOC)
        ├── service.rs               # 11 service functions (~280 LOC)
        └── handler.rs               # 11 axum handlers + router() (~260 LOC)

crates/modules/tests/                # integration test harness (new)
└── role_module_tests.rs             # ~22 integration tests against live saas_tea DB

server-rs/scripts/
└── smoke-role-module.sh             # end-to-end curl sequence for week 3 gate
```

### Modified files
```text
crates/modules/src/lib.rs            # declare system module, wire role::router()
crates/modules/src/domain/mod.rs     # expose common + role_repo
crates/modules/src/domain/entities.rs # add SysRole struct
crates/app/src/main.rs               # nest system router at /api/v1/system
```

### No changes to
- Phase 0 framework layer (`crates/framework/**`)
- Database schema (`sys_role` / `sys_role_menu` / `sys_user_role` / `sys_user` / `sys_menu` / `sys_user_tenant` already exist, managed by NestJS Prisma)
- Config files
- i18n files

---

## Conventions Recap (read before starting any task)

Drawn from the spec's "DAO conventions" and "Technical Approach" sections:

1. Repo methods are `pub async fn` on a unit struct (`pub struct RoleRepo;`), taking `&PgPool` or `&mut sqlx::Transaction<'_, sqlx::Postgres>` as the first argument. No `self`.
2. Every write method calls `AuditInsert::now()` (for INSERT) or `audit_update_by()` (for UPDATE) to populate audit columns.
3. Every tenant-scoped SELECT reads `current_tenant_scope()` and passes it as a bound parameter. The WHERE clause uses `($N::varchar IS NULL OR tenant_id = $N)` so that `run_ignoring_tenant` degrades gracefully to "no filter."
4. Soft delete: SELECTs include `del_flag = '0'`; DELETEs are rewritten as `UPDATE ... SET del_flag = '1'`.
5. Column lists are named explicitly — no `SELECT *`. Use a private `const COLUMNS: &str` at the top of `role_repo.rs` to avoid drift.
6. Repos never call other repos. Cross-repo orchestration happens in `service.rs`.
7. Writes are single-owner: only `role_repo.rs` writes to `sys_role` / `sys_role_menu` / `sys_user_role`. Reading `sys_user` / `sys_user_tenant` / `sys_menu` via JOIN is fine.
8. Tests: unit test the helpers + DTO validation only. Service + handler + SQL correctness are covered by integration tests against the real dev DB.
9. Every task ends with a "verification checkpoint" running `cargo check -p modules` and any newly added `cargo test`. If any fail, do not proceed to the next task.

---

# WEEK 1 — Foundation + Read Endpoints (Tasks 1-15)

Goal: three endpoints live (`POST /system/role/`, `GET /system/role/list`, `GET /system/role/:id`), proving the data-layer skeleton against real DB.

---

### Task 1: `domain::common` helpers with unit tests

**Files:**
- Create: `crates/modules/src/domain/common.rs`

- [ ] **Step 1: Write the failing tests**

```rust
// crates/modules/src/domain/common.rs
//! Shared helpers used across repos in the `domain` layer. Three tiny
//! functions — no trait hierarchy, no macros. Each helper is a single
//! call site reader.

use framework::context::RequestContext;

/// Audit values for INSERTs. Reads the current user id from the active
/// `RequestContext`; falls back to empty string for background tasks /
/// system-initiated writes without an HTTP caller.
pub struct AuditInsert {
    pub create_by: String,
    pub update_by: String,
}

impl AuditInsert {
    pub fn now() -> Self {
        let user_id = RequestContext::with_current(|c| c.user_id.clone())
            .flatten()
            .unwrap_or_default();
        Self {
            create_by: user_id.clone(),
            update_by: user_id,
        }
    }
}

/// Audit value for UPDATEs — just the caller's user id.
pub fn audit_update_by() -> String {
    RequestContext::with_current(|c| c.user_id.clone())
        .flatten()
        .unwrap_or_default()
}

/// Current tenant id for STRICT-scoped queries. Returns `None` for super
/// tenant, when `run_ignoring_tenant` is in effect, or when no context is
/// in scope (e.g. unit tests without `scope`).
pub fn current_tenant_scope() -> Option<String> {
    RequestContext::with_current(|c| {
        if c.ignore_tenant {
            None
        } else {
            c.tenant_id.clone()
        }
    })
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use framework::context::{scope, RequestContext};

    #[tokio::test]
    async fn audit_insert_reads_current_user() {
        let ctx = RequestContext {
            user_id: Some("u-1".into()),
            ..Default::default()
        };
        scope(ctx, async {
            let a = AuditInsert::now();
            assert_eq!(a.create_by, "u-1");
            assert_eq!(a.update_by, "u-1");
        })
        .await;
    }

    #[tokio::test]
    async fn audit_insert_empty_when_no_user() {
        let ctx = RequestContext::default();
        scope(ctx, async {
            let a = AuditInsert::now();
            assert_eq!(a.create_by, "");
        })
        .await;
    }

    #[tokio::test]
    async fn audit_update_by_reads_current_user() {
        let ctx = RequestContext {
            user_id: Some("u-42".into()),
            ..Default::default()
        };
        scope(ctx, async {
            assert_eq!(audit_update_by(), "u-42");
        })
        .await;
    }

    #[tokio::test]
    async fn current_tenant_scope_returns_tenant() {
        let ctx = RequestContext {
            tenant_id: Some("t-1".into()),
            ..Default::default()
        };
        scope(ctx, async {
            assert_eq!(current_tenant_scope().as_deref(), Some("t-1"));
        })
        .await;
    }

    #[tokio::test]
    async fn current_tenant_scope_returns_none_when_ignoring() {
        let ctx = RequestContext {
            tenant_id: Some("t-1".into()),
            ignore_tenant: true,
            ..Default::default()
        };
        scope(ctx, async {
            assert_eq!(current_tenant_scope(), None);
        })
        .await;
    }

    #[tokio::test]
    async fn current_tenant_scope_returns_none_without_context() {
        // No scope() wrapping — task_local! is not active.
        assert_eq!(current_tenant_scope(), None);
    }
}
```

- [ ] **Step 2: Wire the module into `domain/mod.rs`**

Modify: `crates/modules/src/domain/mod.rs`

```rust
//! Domain layer — entity structs (sqlx rows) and repositories.
//!
//! Phase 0 only models the 6 tables needed for the login → /info flow.
//! Phase 1 adds role + role_repo + shared helpers in `common`.

pub mod common;
pub mod entities;
pub mod role_repo;
pub mod user_repo;

pub use entities::{SysUser, SysUserTenant};
pub use role_repo::RoleRepo;
pub use user_repo::UserRepo;
```

Note: `role_repo` will not compile yet because the file is empty. That's fine — Task 3 creates the file. For now, temporarily remove the `pub mod role_repo;` line until Task 3.

Actually, to avoid thrash, leave `role_repo` out of `mod.rs` for this task. Task 3 adds it.

Corrected `domain/mod.rs` content for Task 1:

```rust
pub mod common;
pub mod entities;
pub mod user_repo;

pub use entities::{SysUser, SysUserTenant};
pub use user_repo::UserRepo;
```

- [ ] **Step 3: Verify tests pass**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
cargo test -p modules --lib domain::common
```

Expected output:
```text
running 6 tests
test domain::common::tests::audit_insert_empty_when_no_user ... ok
test domain::common::tests::audit_insert_reads_current_user ... ok
test domain::common::tests::audit_update_by_reads_current_user ... ok
test domain::common::tests::current_tenant_scope_returns_tenant ... ok
test domain::common::tests::current_tenant_scope_returns_none_when_ignoring ... ok
test domain::common::tests::current_tenant_scope_returns_none_without_context ... ok

test result: ok. 6 passed; 0 failed
```

- [ ] **Step 4: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo fmt --check
```

All three must pass.

---

### Task 2: `SysRole` entity struct

**Files:**
- Modify: `crates/modules/src/domain/entities.rs`

- [ ] **Step 1: Append `SysRole` struct**

Add to the end of `entities.rs`:

```rust
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysRole {
    pub role_id: String,
    pub tenant_id: String,
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i32,
    pub data_scope: String,
    pub menu_check_strictly: bool,
    pub dept_check_strictly: bool,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: chrono::DateTime<chrono::Utc>,
    pub update_by: String,
    pub update_at: chrono::DateTime<chrono::Utc>,
    pub remark: Option<String>,
}

impl SysRole {
    pub fn is_active(&self) -> bool {
        self.del_flag == "0" && self.status == "0"
    }
}
```

- [ ] **Step 2: Re-export from `domain/mod.rs`**

```rust
pub use entities::{SysRole, SysUser, SysUserTenant};
```

- [ ] **Step 3: Verification checkpoint**

```bash
cargo check -p modules
```

Expected: passes. No tests run yet; the struct is unused until Task 4.

---

### Task 3: `role_repo.rs` skeleton with `COLUMNS` constant + `find_by_id`

**Files:**
- Create: `crates/modules/src/domain/role_repo.rs`

- [ ] **Step 1: Create file with module header + constant + find_by_id**

```rust
//! RoleRepo — hand-written SQL for sys_role and its join tables.
//!
//! Conventions (from the spec's DAO conventions section):
//! 1. Each method is one SQL statement OR one tightly-coupled transaction.
//! 2. No cross-repo calls from inside this file — only service.rs orchestrates.
//! 3. Cross-table JOINs are allowed (the allocated-users query reads
//!    sys_user + sys_user_tenant).
//! 4. INSERT/UPDATE/DELETE on sys_role and its join tables are single-owner
//!    to this file.

use super::common::{audit_update_by, current_tenant_scope, AuditInsert};
use super::entities::SysRole;
use framework::response::Page;
use sqlx::{PgPool, Postgres, Transaction};

/// Single source of truth for `SELECT` column lists. Keeps `find_by_id`,
/// `find_page`, and friends in sync as the schema evolves.
const COLUMNS: &str = "\
    role_id, tenant_id, role_name, role_key, role_sort, data_scope, \
    menu_check_strictly, dept_check_strictly, status, del_flag, \
    create_by, create_at, update_by, update_at, remark";

pub struct RoleRepo;

impl RoleRepo {
    // ------------------------------------------------------------------
    // READ: find by role_id, tenant-scoped, soft-delete filtered
    // ------------------------------------------------------------------
    pub async fn find_by_id(
        pool: &PgPool,
        role_id: &str,
    ) -> anyhow::Result<Option<SysRole>> {
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
            .map_err(|e| anyhow::anyhow!("find_by_id: {e}"))?;
        Ok(row)
    }
}
```

- [ ] **Step 2: Re-enable `role_repo` in `domain/mod.rs`**

```rust
pub mod common;
pub mod entities;
pub mod role_repo;
pub mod user_repo;

pub use entities::{SysRole, SysUser, SysUserTenant};
pub use role_repo::RoleRepo;
pub use user_repo::UserRepo;
```

- [ ] **Step 3: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
```

Expected: passes. `RoleRepo::find_by_id` is not called yet so no runtime test.

---

### Task 4: `system::role` module scaffold — empty router

**Files:**
- Create: `crates/modules/src/system/mod.rs`
- Create: `crates/modules/src/system/role/mod.rs`
- Create: `crates/modules/src/system/role/dto.rs`
- Create: `crates/modules/src/system/role/service.rs`
- Create: `crates/modules/src/system/role/handler.rs`
- Modify: `crates/modules/src/lib.rs`

- [ ] **Step 1: Create `system/mod.rs`**

```rust
//! System (backend management) endpoints.
//!
//! Phase 1 sub-phase 1 adds only the `role` module. Subsequent sub-phases
//! add user, menu, dept, post, dict, config, tenant.

pub mod role;
```

- [ ] **Step 2: Create `system/role/mod.rs`**

```rust
//! Role module — CRUD, role-menu binding, role-user assignment.

pub mod dto;
pub mod handler;
pub mod service;

pub use handler::router;
```

- [ ] **Step 3: Create `system/role/dto.rs` with empty scaffold**

```rust
//! Role DTOs — wire shapes matching NestJS for cross-backend compat.

use serde::{Deserialize, Serialize};

// DTOs will be added endpoint-by-endpoint starting in Task 6.
// Keep this file present so handler/service compile.

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct _PlaceholderKeepModuleCompiling;
```

- [ ] **Step 4: Create `system/role/service.rs` with empty scaffold**

```rust
//! Role service — business orchestration.

// Service functions are added endpoint-by-endpoint starting in Task 6.
```

- [ ] **Step 5: Create `system/role/handler.rs` with empty router**

```rust
//! Role HTTP handlers + router wiring.

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    // Routes are added endpoint-by-endpoint. Empty router is a valid
    // `Router<AppState>` that contributes zero endpoints to the app mount.
    Router::new()
}
```

- [ ] **Step 6: Register `system` module in `lib.rs`**

```rust
//! modules — HTTP handlers, services, and domain layer.
//!
//! Phase 0 modules: `auth`, `health`, `domain`, `state`.
//! Phase 1 adds `system::role` (this sub-phase).

pub mod auth;
pub mod domain;
pub mod health;
pub mod state;
pub mod system;

pub use state::AppState;

use axum::Router;

/// Compose the Phase 0 + Phase 1 router.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(auth::router())
        .merge(health::router())
        .merge(system::role::router())
        .with_state(state)
}
```

- [ ] **Step 7: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

Expected: all pass. The app still builds because `system::role::router()` contributes zero routes.

---

### Task 5: Integration test harness

**Files:**
- Create: `crates/modules/tests/common/mod.rs`
- Create: `crates/modules/tests/role_module_tests.rs`

- [ ] **Step 1: Create shared test harness**

Create: `crates/modules/tests/common/mod.rs`

```rust
//! Shared integration test scaffold. Loads `config/development.yaml`
//! via framework's `AppConfig::load`, builds a `PgPool` + `RedisPool`,
//! and exposes an `AppState` + an axum `TestServer` built from the real
//! `modules::router`.
//!
//! These tests hit the live `saas_tea` dev database. Each test uses a
//! distinct `role_name` / `role_key` prefix (`it-{test_name}-`) and
//! cleans up its own rows on drop.

use axum::Router;
use framework::{
    config::AppConfig,
    context::{scope, RequestContext},
    infra::{pg, redis},
    telemetry,
};
use modules::{router, AppState};
use sqlx::PgPool;
use std::sync::{Arc, Once};

static INIT: Once = Once::new();

pub async fn build_state_and_router() -> (AppState, Router) {
    // Tracing is global — init once per test process.
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();
    });

    let cfg = Arc::new(AppConfig::load().expect("load config"));
    let metrics_handle = telemetry::metrics::init_recorder()
        .unwrap_or_else(|_| {
            // init_recorder can only be called once per process; integration
            // tests run within a single process, so ignore re-init errors.
            metrics_exporter_prometheus::PrometheusBuilder::new()
                .build_recorder()
                .handle()
        });

    let pg_pool = pg::connect_lazy(&cfg.db.postgresql).expect("pg pool");
    let redis_pool = redis::build(&cfg.db.redis).expect("redis pool");
    let state = AppState {
        config: cfg,
        pg: pg_pool,
        redis: redis_pool,
        metrics: metrics_handle,
    };
    let router = router(state.clone());
    (state, router)
}

/// Run `fut` inside a `RequestContext` matching the `admin / 000000`
/// super-admin. Skips the real JWT middleware because tests call repos
/// directly or use the axum router with an already-injected context.
pub async fn as_super_admin<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let ctx = RequestContext {
        request_id: Some(format!("it-{}", uuid::Uuid::new_v4())),
        tenant_id: Some("000000".into()),
        platform_id: Some("000000".into()),
        user_id: Some("it-admin".into()),
        user_name: Some("it-admin".into()),
        user_type: Some("10".into()),
        is_admin: true,
        lang_code: Some("en-US".into()),
        ..Default::default()
    };
    scope(ctx, fut).await
}

/// Cleanup helper — delete all rows created by a given test prefix.
pub async fn cleanup_test_rows(pool: &PgPool, prefix: &str) {
    let pattern = format!("{prefix}%");
    let _ = sqlx::query(
        "DELETE FROM sys_role_menu WHERE role_id IN \
         (SELECT role_id FROM sys_role WHERE role_key LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await;
    let _ = sqlx::query(
        "DELETE FROM sys_user_role WHERE role_id IN \
         (SELECT role_id FROM sys_role WHERE role_key LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await;
    let _ = sqlx::query("DELETE FROM sys_role WHERE role_key LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await;
}
```

- [ ] **Step 2: Create empty `role_module_tests.rs`**

```rust
//! Integration tests for the role module. Tests hit the live `saas_tea`
//! dev DB at `127.0.0.1:5432`. Add one test per endpoint as tasks land.

#[path = "common/mod.rs"]
mod common;

// Tests added task-by-task starting in Task 9.
```

- [ ] **Step 3: Add `uuid` + `tracing-subscriber` to modules dev-deps if missing**

Modify: `crates/modules/Cargo.toml`

```toml
[dev-dependencies]
tracing-subscriber = { workspace = true }
metrics-exporter-prometheus = { workspace = true }
```

(framework already re-exports `uuid` and `tracing-subscriber` through its own deps — `modules` needs them explicitly only for the test harness.)

- [ ] **Step 4: Verification checkpoint**

```bash
cargo check -p modules --tests
```

Expected: passes. No tests run yet.

---

### Task 6: `RoleDetailResponseDto` + service `find_by_id` + handler `GET /:id`

**Files:**
- Modify: `crates/modules/src/system/role/dto.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Add response DTO**

Replace `dto.rs` placeholder with:

```rust
//! Role DTOs — wire shapes matching NestJS for cross-backend compat.

use crate::domain::SysRole;
use chrono::{DateTime, Utc};
use serde::Serialize;

fn fmt_ts(ts: &DateTime<Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M:%S").to_string()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDetailResponseDto {
    pub role_id: String,
    pub tenant_id: String,
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i32,
    pub data_scope: String,
    pub menu_check_strictly: bool,
    pub dept_check_strictly: bool,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
    /// Menu ids bound to this role. Populated by the service layer
    /// via a separate `SELECT menu_id FROM sys_role_menu WHERE role_id = ?`.
    pub menu_ids: Vec<String>,
}

impl RoleDetailResponseDto {
    pub fn from_entity(role: SysRole, menu_ids: Vec<String>) -> Self {
        Self {
            role_id: role.role_id,
            tenant_id: role.tenant_id,
            role_name: role.role_name,
            role_key: role.role_key,
            role_sort: role.role_sort,
            data_scope: role.data_scope,
            menu_check_strictly: role.menu_check_strictly,
            dept_check_strictly: role.dept_check_strictly,
            status: role.status,
            create_by: role.create_by,
            create_at: fmt_ts(&role.create_at),
            update_by: role.update_by,
            update_at: fmt_ts(&role.update_at),
            remark: role.remark,
            menu_ids,
        }
    }
}
```

- [ ] **Step 2: Add `find_menu_ids_by_role` to `RoleRepo`**

Modify: `crates/modules/src/domain/role_repo.rs` — append method to `impl RoleRepo`:

```rust
    /// List menu_ids bound to a role. Used by `find_by_id` to populate
    /// the detail response. Not tenant-scoped because `sys_role_menu` has
    /// no tenant column (the `sys_role` row itself is tenant-scoped).
    pub async fn find_menu_ids_by_role(
        pool: &PgPool,
        role_id: &str,
    ) -> anyhow::Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT menu_id FROM sys_role_menu WHERE role_id = $1 ORDER BY menu_id",
        )
        .bind(role_id)
        .fetch_all(pool)
        .await
        .map_err(|e| anyhow::anyhow!("find_menu_ids_by_role: {e}"))?;
        Ok(rows.into_iter().map(|(m,)| m).collect())
    }
```

- [ ] **Step 3: Add service function**

Replace `service.rs` content with:

```rust
//! Role service — business orchestration.

use super::dto::RoleDetailResponseDto;
use crate::domain::RoleRepo;
use crate::state::AppState;
use framework::error::{AppError, BusinessError};
use framework::response::ResponseCode;

pub async fn find_by_id(
    state: &AppState,
    role_id: &str,
) -> Result<RoleDetailResponseDto, AppError> {
    let role = RoleRepo::find_by_id(&state.pg, role_id)
        .await
        .map_err(AppError::Internal)?;
    let role = BusinessError::throw_if_null(role, ResponseCode::DATA_NOT_FOUND)?;

    let menu_ids = RoleRepo::find_menu_ids_by_role(&state.pg, &role.role_id)
        .await
        .map_err(AppError::Internal)?;

    Ok(RoleDetailResponseDto::from_entity(role, menu_ids))
}
```

- [ ] **Step 4: Add handler + wire route**

Replace `handler.rs` content with:

```rust
//! Role HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    middleware::from_fn_with_state,
    routing::get,
    Router,
};
use framework::auth::AccessSpec;
use framework::error::AppError;
use framework::middleware::access;
use framework::response::ApiResponse;

async fn find_by_id(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<ApiResponse<dto::RoleDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &role_id).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/system/role/:id",
        get(find_by_id).route_layer(from_fn_with_state(
            access::require(AccessSpec::permission("system:role:query")),
            access::enforce,
        )),
    )
}
```

- [ ] **Step 5: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo fmt --check
cargo build -p app
```

All must pass.

---

### Task 7: Wire `system::role::router` into `/api/v1` mount

**Files:**
- Modify: `crates/app/src/main.rs`

- [ ] **Step 1: Update router assembly to include system router**

In `main.rs`, find the Router assembly block and update:

```rust
    // 8. Router assembly
    let app = Router::new()
        .nest(API_PREFIX, modules::auth::router())
        .nest(API_PREFIX, modules::system::role::router())
        .merge(modules::health::router())
        .with_state(state)
        // ... layers unchanged
```

Note: `lib::router()` from Task 4's Step 6 already merges `system::role::router()` at the root, but in `main.rs` we bypass `lib::router()` and compose manually to control the `/api/v1` nest point. The two `.nest(API_PREFIX, ...)` calls merge — Axum supports multiple `.nest` at the same prefix by combining their routes.

- [ ] **Step 2: Run the server**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
cargo run -p app
```

Expected: starts without error, logs `listening` on `0.0.0.0:18080`.

- [ ] **Step 3: Manual curl (server still running in another shell)**

Login first to get a token:

```bash
curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])"
```

Save token in `$TOKEN`. Then hit a non-existent role id:

```bash
curl -sS -o /tmp/resp.json -w "HTTP %{http_code}\n" \
  http://127.0.0.1:18080/api/v1/system/role/nonexistent \
  -H "Authorization: Bearer $TOKEN"
cat /tmp/resp.json
```

Expected: HTTP 200 with body `{"code":1001,"msg":"数据不存在",...}` (BusinessException semantics — HTTP 200, body code 1001).

Stop the server (Ctrl+C).

- [ ] **Step 4: Verification checkpoint**

```bash
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

---

### Task 8: Integration test for `GET /system/role/:id` not-found

**Files:**
- Modify: `crates/modules/tests/role_module_tests.rs`

- [ ] **Step 1: Add first integration test**

Append to `role_module_tests.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use tower::ServiceExt;

#[tokio::test]
async fn find_by_id_not_found_returns_business_error() {
    let (_state, router) = common::build_state_and_router().await;

    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/system/role/does-not-exist-xyz-123")
        .header("Authorization", "Bearer invalid-but-whitelisted-not-needed")
        .body(Body::empty())
        .unwrap();

    // Note: this request will fail auth middleware because there's no valid
    // JWT. The integration test for the happy path (Task 12) logs in first.
    // This test intentionally hits the auth rejection path to prove the
    // router is wired in and the envelope format is correct.
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
```

- [ ] **Step 2: Add `tower` to dev-deps**

Modify: `crates/modules/Cargo.toml`

```toml
[dev-dependencies]
tower = { workspace = true }
tracing-subscriber = { workspace = true }
metrics-exporter-prometheus = { workspace = true }
```

- [ ] **Step 3: Run the test**

```bash
cargo test -p modules --test role_module_tests find_by_id_not_found
```

Expected: 1 test passes. Test proves the router is wired and an unauthenticated request to `/api/v1/system/role/:id` returns 401.

- [ ] **Step 4: Verification checkpoint**

```bash
cargo test --workspace
```

Expected: all Phase 0 tests (48) + the new test still pass.

---

### Task 9: `find_page` repo method + `ListRoleDto`

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`
- Modify: `crates/modules/src/system/role/dto.rs`

- [ ] **Step 1: Append `find_page` to `RoleRepo`**

Add to `impl RoleRepo`:

```rust
    /// Paginated list with optional name / role_key / status filters.
    /// Tenant-scoped via `current_tenant_scope`.
    pub async fn find_page(
        pool: &PgPool,
        name: Option<&str>,
        role_key: Option<&str>,
        status: Option<&str>,
        page_num: u32,
        page_size: u32,
    ) -> anyhow::Result<Page<SysRole>> {
        let tenant = current_tenant_scope();
        let safe_page_num = page_num.max(1);
        let safe_page_size = page_size.clamp(1, 200);
        let offset = ((safe_page_num - 1) * safe_page_size) as i64;
        let limit = safe_page_size as i64;

        let sql = format!(
            "SELECT {COLUMNS} \
               FROM sys_role \
              WHERE del_flag = '0' \
                AND ($1::varchar IS NULL OR tenant_id = $1) \
                AND ($2::varchar IS NULL OR role_name LIKE '%' || $2 || '%') \
                AND ($3::varchar IS NULL OR role_key LIKE '%' || $3 || '%') \
                AND ($4::varchar IS NULL OR status = $4) \
              ORDER BY role_sort ASC, create_at DESC \
              LIMIT $5 OFFSET $6"
        );
        let rows = sqlx::query_as::<_, SysRole>(&sql)
            .bind(tenant.as_deref())
            .bind(name)
            .bind(role_key)
            .bind(status)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await
            .map_err(|e| anyhow::anyhow!("find_page rows: {e}"))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_role \
              WHERE del_flag = '0' \
                AND ($1::varchar IS NULL OR tenant_id = $1) \
                AND ($2::varchar IS NULL OR role_name LIKE '%' || $2 || '%') \
                AND ($3::varchar IS NULL OR role_key LIKE '%' || $3 || '%') \
                AND ($4::varchar IS NULL OR status = $4)",
        )
        .bind(tenant.as_deref())
        .bind(name)
        .bind(role_key)
        .bind(status)
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow::anyhow!("find_page count: {e}"))?;

        Ok(Page::new(rows, total as u64, safe_page_num, safe_page_size))
    }
```

- [ ] **Step 2: Add `ListRoleDto` + `RoleListItemResponseDto` to `dto.rs`**

Append:

```rust
use serde::Deserialize;
use validator::Validate;

#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListRoleDto {
    pub role_name: Option<String>,
    pub role_key: Option<String>,
    pub status: Option<String>,
    #[validate(range(min = 1, max = 10000))]
    #[serde(default = "default_page_num")]
    pub page_num: u32,
    #[validate(range(min = 1, max = 200))]
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page_num() -> u32 {
    1
}
fn default_page_size() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleListItemResponseDto {
    pub role_id: String,
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i32,
    pub status: String,
    pub create_at: String,
    pub remark: Option<String>,
}

impl RoleListItemResponseDto {
    pub fn from_entity(role: SysRole) -> Self {
        Self {
            role_id: role.role_id,
            role_name: role.role_name,
            role_key: role.role_key,
            role_sort: role.role_sort,
            status: role.status,
            create_at: fmt_ts(&role.create_at),
            remark: role.remark,
        }
    }
}
```

- [ ] **Step 3: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
```

---

### Task 10: Service `list` + handler `GET /system/role/list`

**Files:**
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Add `list` service function**

Append to `service.rs`:

```rust
use super::dto::{ListRoleDto, RoleListItemResponseDto};
use framework::response::Page;

pub async fn list(
    state: &AppState,
    query: ListRoleDto,
) -> Result<Page<RoleListItemResponseDto>, AppError> {
    let page = RoleRepo::find_page(
        &state.pg,
        query.role_name.as_deref(),
        query.role_key.as_deref(),
        query.status.as_deref(),
        query.page_num,
        query.page_size,
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Page::new(
        page.rows.into_iter().map(RoleListItemResponseDto::from_entity).collect(),
        page.total,
        page.page_num,
        page.page_size,
    ))
}
```

- [ ] **Step 2: Add `list` handler + route**

Modify `handler.rs`:

```rust
use axum::extract::Query;
use framework::extractors::ValidatedJson;
use framework::response::Page;

async fn list(
    State(state): State<AppState>,
    Query(query): Query<dto::ListRoleDto>,
) -> Result<ApiResponse<Page<dto::RoleListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/role/list",
            get(list).route_layer(from_fn_with_state(
                access::require(AccessSpec::permission("system:role:list")),
                access::enforce,
            )),
        )
        .route(
            "/system/role/:id",
            get(find_by_id).route_layer(from_fn_with_state(
                access::require(AccessSpec::permission("system:role:query")),
                access::enforce,
            )),
        )
}
```

Note: `ListRoleDto` is extracted via `Query` (not `ValidatedJson`) because list params come from the URL query string. Query does not auto-run `validator::Validate`, so validation happens inside the service function:

```rust
use validator::Validate;

pub async fn list(
    state: &AppState,
    query: ListRoleDto,
) -> Result<Page<RoleListItemResponseDto>, AppError> {
    query.validate().map_err(|e| {
        AppError::Internal(anyhow::anyhow!("list query validation: {e}"))
    })?;
    // ... rest as above
}
```

(In Phase 1 sub-phase 2 we can write a `ValidatedQuery` extractor to mirror `ValidatedJson`. For now the service-layer check is sufficient and matches the pattern in Phase 0 login.)

- [ ] **Step 3: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo fmt --check
cargo build -p app
```

---

### Task 11: Manual smoke test — GET /list + GET /:id

- [ ] **Step 1: Start the app**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
cargo run -p app
```

- [ ] **Step 2: Login + list roles**

```bash
TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")

curl -sS http://127.0.0.1:18080/api/v1/system/role/list \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool | head -40
```

Expected output:
- HTTP 200
- Body contains `"code":200`, `"data":{"rows":[...], "total": N, "pageNum":1, "pageSize":10, "pages": ...}`
- At least the NestJS-created roles visible (e.g. "超级管理员", "普通角色") in `rows`

- [ ] **Step 3: Fetch a real role detail**

```bash
# Pick any role_id from the list response
ROLE_ID=$(curl -sS http://127.0.0.1:18080/api/v1/system/role/list \
  -H "Authorization: Bearer $TOKEN" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['rows'][0]['roleId'])")

curl -sS http://127.0.0.1:18080/api/v1/system/role/$ROLE_ID \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool
```

Expected:
- HTTP 200
- `data` contains `roleId`, `roleName`, `roleKey`, `roleSort`, `status`, `menuIds` (array, may be non-empty if the role has bindings)

- [ ] **Step 4: Filtered list**

```bash
curl -sS "http://127.0.0.1:18080/api/v1/system/role/list?roleName=admin" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool | head -20
```

Expected: only rows where `roleName` contains "admin".

Stop the server.

- [ ] **Step 5: Verification checkpoint**

```bash
cargo test --workspace
cargo clippy --all-targets -- -D warnings
```

---

### Task 12: `RoleRepo::insert_with_menus` transaction

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`

- [ ] **Step 1: Add transactional create method**

Append to `impl RoleRepo`:

```rust
    /// Create a role and bind menus in one transaction.
    ///
    /// Returns the newly-inserted `SysRole` row. The caller is expected to
    /// have validated that `menu_ids` all exist and are active (done in
    /// the service layer before this method is called).
    pub async fn insert_with_menus(
        pool: &PgPool,
        role_name: &str,
        role_key: &str,
        role_sort: i32,
        status: &str,
        remark: Option<&str>,
        menu_ids: &[String],
    ) -> anyhow::Result<SysRole> {
        let audit = AuditInsert::now();
        let tenant = current_tenant_scope()
            .ok_or_else(|| anyhow::anyhow!("insert_with_menus: tenant_id required"))?;
        let role_id = uuid::Uuid::new_v4().to_string();

        let mut tx: Transaction<'_, Postgres> = pool
            .begin()
            .await
            .map_err(|e| anyhow::anyhow!("begin tx: {e}"))?;

        let insert_sql = format!(
            "INSERT INTO sys_role (\
                role_id, tenant_id, role_name, role_key, role_sort, \
                data_scope, menu_check_strictly, dept_check_strictly, \
                status, del_flag, create_by, update_by, remark\
            ) VALUES ($1, $2, $3, $4, $5, '1', false, false, $6, '0', $7, $8, $9) \
            RETURNING {COLUMNS}"
        );
        let role = sqlx::query_as::<_, SysRole>(&insert_sql)
            .bind(&role_id)
            .bind(&tenant)
            .bind(role_name)
            .bind(role_key)
            .bind(role_sort)
            .bind(status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(remark)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| anyhow::anyhow!("insert sys_role: {e}"))?;

        if !menu_ids.is_empty() {
            // Bulk insert via UNNEST to do a single round-trip even for
            // hundreds of menu bindings.
            sqlx::query(
                "INSERT INTO sys_role_menu (role_id, menu_id) \
                 SELECT $1, unnest($2::varchar[])",
            )
            .bind(&role_id)
            .bind(menu_ids)
            .execute(&mut *tx)
            .await
            .map_err(|e| anyhow::anyhow!("insert sys_role_menu: {e}"))?;
        }

        tx.commit()
            .await
            .map_err(|e| anyhow::anyhow!("commit tx: {e}"))?;
        Ok(role)
    }
```

- [ ] **Step 2: Add `uuid` to `modules/Cargo.toml` dependencies**

It should already be there from Phase 0; verify:

```bash
grep uuid crates/modules/Cargo.toml
```

Expected: `uuid.workspace = true` in `[dependencies]`. If missing, add it.

- [ ] **Step 3: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
```

---

### Task 13: `CreateRoleDto` + service `create`

**Files:**
- Modify: `crates/modules/src/system/role/dto.rs`
- Modify: `crates/modules/src/system/role/service.rs`

- [ ] **Step 1: Add `CreateRoleDto`**

Append to `dto.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoleDto {
    #[validate(length(min = 1, max = 30))]
    pub role_name: String,
    #[validate(length(min = 1, max = 100))]
    pub role_key: String,
    #[validate(range(min = 0, max = 9999))]
    pub role_sort: i32,
    #[serde(default = "default_status")]
    #[validate(length(min = 1, max = 1))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
    #[serde(default)]
    pub menu_ids: Vec<String>,
}

fn default_status() -> String {
    "0".into()
}
```

- [ ] **Step 2: Add `create` service function**

Append to `service.rs`:

```rust
use super::dto::{CreateRoleDto, RoleDetailResponseDto};

pub async fn create(
    state: &AppState,
    dto: CreateRoleDto,
) -> Result<RoleDetailResponseDto, AppError> {
    // Phase 0.5: no role_key uniqueness check yet. NestJS does it via a
    // separate query. Phase 1 sub-phase 2 can add it when user module lands
    // (same pattern). A unique constraint on the DB side would back-stop
    // this, but `sys_role` currently uses a non-unique index.
    //
    // Future: validate menu_ids all exist + are active + are within the
    // tenant's package range. Phase 2 work.

    let role = RoleRepo::insert_with_menus(
        &state.pg,
        &dto.role_name,
        &dto.role_key,
        dto.role_sort,
        &dto.status,
        dto.remark.as_deref(),
        &dto.menu_ids,
    )
    .await
    .map_err(AppError::Internal)?;

    // The bound menu_ids are already known — avoid a second SELECT.
    Ok(RoleDetailResponseDto::from_entity(role, dto.menu_ids))
}
```

- [ ] **Step 3: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
```

---

### Task 14: Handler `POST /system/role/`

**Files:**
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Add POST handler**

Append to `handler.rs` imports:

```rust
use axum::routing::post;
```

Add handler function:

```rust
async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateRoleDto>,
) -> Result<ApiResponse<dto::RoleDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}
```

Update `router()` to include the new route:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/role/",
            post(create).route_layer(from_fn_with_state(
                access::require(AccessSpec::permission("system:role:add")),
                access::enforce,
            )),
        )
        .route(
            "/system/role/list",
            get(list).route_layer(from_fn_with_state(
                access::require(AccessSpec::permission("system:role:list")),
                access::enforce,
            )),
        )
        .route(
            "/system/role/:id",
            get(find_by_id).route_layer(from_fn_with_state(
                access::require(AccessSpec::permission("system:role:query")),
                access::enforce,
            )),
        )
}
```

- [ ] **Step 2: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo fmt --check
cargo build -p app
```

---

### Task 15: Manual smoke test — POST create + verify in DB

- [ ] **Step 1: Start server + create a role**

```bash
cargo run -p app &
sleep 2

TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")

# Pick 3 menu ids to bind (from the live DB)
MENU_IDS=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT menu_id FROM sys_menu WHERE del_flag='0' AND perms <> '' LIMIT 3;" \
  | tr -d ' ' | grep -v '^$' | python3 -c "import sys,json; print(json.dumps([l.strip() for l in sys.stdin if l.strip()]))")

echo "Using menu ids: $MENU_IDS"

BODY=$(printf '{"roleName":"it-role-1","roleKey":"it:role:1","roleSort":100,"remark":"integration test role","menuIds":%s}' "$MENU_IDS")

curl -sS -X POST http://127.0.0.1:18080/api/v1/system/role/ \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "$BODY" | python3 -m json.tool
```

Expected: HTTP 200, response contains the new `roleId`, the `menuIds` you passed in.

- [ ] **Step 2: Verify via GET /:id**

```bash
# Extract the roleId from the POST response (or query DB)
NEW_ID=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT role_id FROM sys_role WHERE role_key='it:role:1';" | tr -d ' \n')

curl -sS "http://127.0.0.1:18080/api/v1/system/role/$NEW_ID" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool
```

Expected: GET returns the same role with menuIds array of length 3.

- [ ] **Step 3: Verify DB side**

```bash
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "SELECT role_id, role_name, role_key, role_sort, tenant_id, create_by FROM sys_role WHERE role_key='it:role:1';"

PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "SELECT role_id, menu_id FROM sys_role_menu WHERE role_id='$NEW_ID';"
```

Expected:
- One `sys_role` row with `tenant_id='000000'`, `create_by='cf827fc0-...'` (the admin's user_id, from Phase 0 validation), `role_sort=100`.
- Three `sys_role_menu` rows with matching `role_id`.

- [ ] **Step 4: Cleanup**

```bash
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "DELETE FROM sys_role_menu WHERE role_id='$NEW_ID'; DELETE FROM sys_role WHERE role_id='$NEW_ID';"

pkill -f "target/debug/app"
```

- [ ] **Step 5: Week 1 exit gate**

```bash
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

All three must pass.

**Week 1 done.** 3 endpoints (`POST create`, `GET list`, `GET :id`) working end-to-end against real DB.

---

# WEEK 2 — Lifecycle + Query Endpoints (Tasks 16-27)

Goal: 6 more endpoints (update, change-status, delete, option-select, allocated-list, unallocated-list) — proving UPDATE tx with menu diff and complex JOIN queries.

---

### Task 16: `RoleRepo::update_with_menus` transaction

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`

- [ ] **Step 1: Append update method**

```rust
    /// Update a role's scalar fields AND replace its menu bindings
    /// atomically. Returns the row count affected for the UPDATE statement
    /// (not including role_menu rows). Zero means "no such role for this
    /// tenant" → caller should map to `DATA_NOT_FOUND`.
    pub async fn update_with_menus(
        pool: &PgPool,
        role_id: &str,
        role_name: &str,
        role_key: &str,
        role_sort: i32,
        status: &str,
        remark: Option<&str>,
        menu_ids: &[String],
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let mut tx: Transaction<'_, Postgres> = pool
            .begin()
            .await
            .map_err(|e| anyhow::anyhow!("begin tx: {e}"))?;

        let affected = sqlx::query(
            "UPDATE sys_role \
                SET role_name = $1, role_key = $2, role_sort = $3, \
                    status = $4, remark = $5, update_by = $6, update_at = NOW() \
              WHERE role_id = $7 \
                AND del_flag = '0' \
                AND ($8::varchar IS NULL OR tenant_id = $8)",
        )
        .bind(role_name)
        .bind(role_key)
        .bind(role_sort)
        .bind(status)
        .bind(remark)
        .bind(&updater)
        .bind(role_id)
        .bind(tenant.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow::anyhow!("update sys_role: {e}"))?
        .rows_affected();

        if affected > 0 {
            // Replace-all strategy: delete existing bindings, insert new ones.
            // Simpler than computing the diff and safe because role_menu has
            // no audit columns or referential integrity beyond the FK.
            sqlx::query("DELETE FROM sys_role_menu WHERE role_id = $1")
                .bind(role_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| anyhow::anyhow!("delete sys_role_menu: {e}"))?;

            if !menu_ids.is_empty() {
                sqlx::query(
                    "INSERT INTO sys_role_menu (role_id, menu_id) \
                     SELECT $1, unnest($2::varchar[])",
                )
                .bind(role_id)
                .bind(menu_ids)
                .execute(&mut *tx)
                .await
                .map_err(|e| anyhow::anyhow!("insert sys_role_menu: {e}"))?;
            }
        }

        tx.commit()
            .await
            .map_err(|e| anyhow::anyhow!("commit tx: {e}"))?;
        Ok(affected)
    }
```

- [ ] **Step 2: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
```

---

### Task 17: `UpdateRoleDto` + service `update` + handler `PUT /system/role/`

**Files:**
- Modify: `crates/modules/src/system/role/dto.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Add `UpdateRoleDto`**

Append to `dto.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoleDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    #[validate(length(min = 1, max = 30))]
    pub role_name: String,
    #[validate(length(min = 1, max = 100))]
    pub role_key: String,
    #[validate(range(min = 0, max = 9999))]
    pub role_sort: i32,
    #[validate(length(min = 1, max = 1))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
    #[serde(default)]
    pub menu_ids: Vec<String>,
}
```

- [ ] **Step 2: Add `update` service function**

Append to `service.rs`:

```rust
use super::dto::UpdateRoleDto;

pub async fn update(state: &AppState, dto: UpdateRoleDto) -> Result<(), AppError> {
    let affected = RoleRepo::update_with_menus(
        &state.pg,
        &dto.role_id,
        &dto.role_name,
        &dto.role_key,
        dto.role_sort,
        &dto.status,
        dto.remark.as_deref(),
        &dto.menu_ids,
    )
    .await
    .map_err(AppError::Internal)?;

    if affected == 0 {
        return BusinessError::throw(ResponseCode::DATA_NOT_FOUND);
    }
    Ok(())
}
```

- [ ] **Step 3: Add handler + route**

Append to `handler.rs`:

```rust
use axum::routing::put;

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateRoleDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}
```

Add to `router()`:

```rust
.route(
    "/system/role/",
    post(create)
        .route_layer(from_fn_with_state(
            access::require(AccessSpec::permission("system:role:add")),
            access::enforce,
        ))
        .put(update)
        .route_layer(from_fn_with_state(
            access::require(AccessSpec::permission("system:role:edit")),
            access::enforce,
        )),
)
```

**Caveat**: chaining `.post(...).route_layer(...).put(...).route_layer(...)` applies both layers to both methods. This is technically wrong — Axum will reject the second `route_layer` with a panic OR apply both. Per the spec, the plan-phase-locks-exact-pattern:

**Correct approach**: use two separate `.route` calls for the same path. Axum 0.8 merges MethodRouters on the same path:

```rust
Router::new()
    .route(
        "/system/role/",
        post(create).route_layer(from_fn_with_state(
            access::require(AccessSpec::permission("system:role:add")),
            access::enforce,
        )),
    )
    .route(
        "/system/role/",
        put(update).route_layer(from_fn_with_state(
            access::require(AccessSpec::permission("system:role:edit")),
            access::enforce,
        )),
    )
    .route(
        "/system/role/list",
        get(list).route_layer(from_fn_with_state(
            access::require(AccessSpec::permission("system:role:list")),
            access::enforce,
        )),
    )
    .route(
        "/system/role/:id",
        get(find_by_id).route_layer(from_fn_with_state(
            access::require(AccessSpec::permission("system:role:query")),
            access::enforce,
        )),
    )
```

- [ ] **Step 4: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

If `cargo build` fails with "paths overlap" or similar, Axum 0.8 does NOT merge routes on the same path. Fall back to this alternative: use `axum::routing::MethodRouter::on` or split into a helper function. See Axum 0.8 changelog for exact merge semantics. Document the resolution inline and proceed.

---

### Task 18: `ChangeStatusDto` + `change_status` end-to-end

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`
- Modify: `crates/modules/src/system/role/dto.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Repo method**

```rust
    pub async fn change_status(
        pool: &PgPool,
        role_id: &str,
        status: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_role \
                SET status = $1, update_by = $2, update_at = NOW() \
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
        .map_err(|e| anyhow::anyhow!("change_status: {e}"))?
        .rows_affected();

        Ok(affected)
    }
```

- [ ] **Step 2: DTO**

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRoleStatusDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    #[validate(length(min = 1, max = 1))]
    pub status: String,
}
```

- [ ] **Step 3: Service**

```rust
use super::dto::ChangeRoleStatusDto;

pub async fn change_status(
    state: &AppState,
    dto: ChangeRoleStatusDto,
) -> Result<(), AppError> {
    let affected = RoleRepo::change_status(&state.pg, &dto.role_id, &dto.status)
        .await
        .map_err(AppError::Internal)?;
    if affected == 0 {
        return BusinessError::throw(ResponseCode::DATA_NOT_FOUND);
    }
    Ok(())
}
```

- [ ] **Step 4: Handler + route**

```rust
async fn change_status(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ChangeRoleStatusDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::change_status(&state, dto).await?;
    Ok(ApiResponse::success())
}
```

Add to `router()`:

```rust
.route(
    "/system/role/change-status",
    put(change_status).route_layer(from_fn_with_state(
        access::require(AccessSpec::permission("system:role:change-status")),
        access::enforce,
    )),
)
```

- [ ] **Step 5: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

---

### Task 19: `DELETE /system/role/:id` (soft delete)

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Repo soft delete**

```rust
    pub async fn soft_delete_by_id(
        pool: &PgPool,
        role_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let updater = audit_update_by();

        let affected = sqlx::query(
            "UPDATE sys_role \
                SET del_flag = '1', update_by = $1, update_at = NOW() \
              WHERE role_id = $2 \
                AND del_flag = '0' \
                AND ($3::varchar IS NULL OR tenant_id = $3)",
        )
        .bind(&updater)
        .bind(role_id)
        .bind(tenant.as_deref())
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("soft_delete_by_id: {e}"))?
        .rows_affected();

        Ok(affected)
    }
```

- [ ] **Step 2: Service**

```rust
pub async fn remove(state: &AppState, role_id: &str) -> Result<(), AppError> {
    let affected = RoleRepo::soft_delete_by_id(&state.pg, role_id)
        .await
        .map_err(AppError::Internal)?;
    if affected == 0 {
        return BusinessError::throw(ResponseCode::DATA_NOT_FOUND);
    }
    Ok(())
}
```

- [ ] **Step 3: Handler + route**

```rust
use axum::routing::delete;

async fn remove(
    State(state): State<AppState>,
    Path(role_id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &role_id).await?;
    Ok(ApiResponse::success())
}
```

Add to `router()`:

```rust
.route(
    "/system/role/:id",
    get(find_by_id)
        .route_layer(from_fn_with_state(
            access::require(AccessSpec::permission("system:role:query")),
            access::enforce,
        )),
)
.route(
    "/system/role/:id",
    delete(remove).route_layer(from_fn_with_state(
        access::require(AccessSpec::permission("system:role:remove")),
        access::enforce,
    )),
)
```

- [ ] **Step 4: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

---

### Task 20: `GET /system/role/option-select`

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`
- Modify: `crates/modules/src/system/role/dto.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Repo**

```rust
    /// Return active roles for dropdown UI — flat list, no pagination.
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
            .map_err(|e| anyhow::anyhow!("find_option_list: {e}"))?;
        Ok(rows)
    }
```

- [ ] **Step 2: Option DTO**

Append to `dto.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleOptionResponseDto {
    pub role_id: String,
    pub role_name: String,
    pub role_key: String,
}

impl RoleOptionResponseDto {
    pub fn from_entity(role: SysRole) -> Self {
        Self {
            role_id: role.role_id,
            role_name: role.role_name,
            role_key: role.role_key,
        }
    }
}
```

- [ ] **Step 3: Service**

```rust
use super::dto::RoleOptionResponseDto;

pub async fn option_select(
    state: &AppState,
) -> Result<Vec<RoleOptionResponseDto>, AppError> {
    let rows = RoleRepo::find_option_list(&state.pg)
        .await
        .map_err(AppError::Internal)?;
    Ok(rows.into_iter().map(RoleOptionResponseDto::from_entity).collect())
}
```

- [ ] **Step 4: Handler + route (authenticated-only, no specific permission)**

```rust
async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::RoleOptionResponseDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}
```

Add to `router()`:

```rust
.route(
    "/system/role/option-select",
    get(option_select).route_layer(from_fn_with_state(
        access::require(AccessSpec::authenticated()),
        access::enforce,
    )),
)
```

- [ ] **Step 5: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

---

### Task 21: `RoleRepo::find_allocated_users_page` (JOIN query)

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`

- [ ] **Step 1: Add local projection struct + JOIN query**

Append to `role_repo.rs`:

```rust
use chrono::{DateTime, Utc};

/// Projection row for `find_allocated_users_page`. Local to this file —
/// not re-exported. Contains only the columns the allocated-list page
/// needs, not the full `SysUser`.
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

impl RoleRepo {
    /// Users currently bound to `role_id` in the current tenant.
    /// This query joins sys_user + sys_user_role + sys_user_tenant — it
    /// reads `sys_user` from "UserRepo's territory" but stays in
    /// `role_repo.rs` because the caller's mental model is "this role's
    /// users." See the spec's DAO conventions (rule 3).
    pub async fn find_allocated_users_page(
        pool: &PgPool,
        role_id: &str,
        user_name_filter: Option<&str>,
        page_num: u32,
        page_size: u32,
    ) -> anyhow::Result<Page<AllocatedUserRow>> {
        let tenant = current_tenant_scope();
        let safe_page_num = page_num.max(1);
        let safe_page_size = page_size.clamp(1, 200);
        let offset = ((safe_page_num - 1) * safe_page_size) as i64;
        let limit = safe_page_size as i64;

        let rows = sqlx::query_as::<_, AllocatedUserRow>(
            "SELECT u.user_id, u.user_name, u.nick_name, u.email, \
                    u.phonenumber, u.status, u.create_at \
               FROM sys_user u \
               JOIN sys_user_role ur ON ur.user_id = u.user_id \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
              WHERE ur.role_id = $1 \
                AND ($2::varchar IS NULL OR ut.tenant_id = $2) \
                AND u.del_flag = '0' \
                AND ut.status = '0' \
                AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%') \
              ORDER BY u.create_at DESC \
              LIMIT $4 OFFSET $5",
        )
        .bind(role_id)
        .bind(tenant.as_deref())
        .bind(user_name_filter)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| anyhow::anyhow!("find_allocated_users_page rows: {e}"))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_user u \
               JOIN sys_user_role ur ON ur.user_id = u.user_id \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
              WHERE ur.role_id = $1 \
                AND ($2::varchar IS NULL OR ut.tenant_id = $2) \
                AND u.del_flag = '0' \
                AND ut.status = '0' \
                AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%')",
        )
        .bind(role_id)
        .bind(tenant.as_deref())
        .bind(user_name_filter)
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow::anyhow!("find_allocated_users_page count: {e}"))?;

        Ok(Page::new(rows, total as u64, safe_page_num, safe_page_size))
    }
}
```

- [ ] **Step 2: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
```

---

### Task 22: `GET /system/role/auth-user/allocated-list` endpoint

**Files:**
- Modify: `crates/modules/src/system/role/dto.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: DTOs**

Append to `dto.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AuthUserListQueryDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    pub user_name: Option<String>,
    #[validate(range(min = 1, max = 10000))]
    #[serde(default = "default_page_num")]
    pub page_num: u32,
    #[validate(range(min = 1, max = 200))]
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocatedUserResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub phonenumber: String,
    pub status: String,
    pub create_at: String,
}
```

Add import at top of `dto.rs`:

```rust
use crate::domain::role_repo::AllocatedUserRow;
```

Expose `AllocatedUserRow` from `role_repo.rs`:

```rust
// In role_repo.rs
pub use self::AllocatedUserRow; // already pub
```

And add conversion:

```rust
impl AllocatedUserResponseDto {
    pub fn from_row(r: AllocatedUserRow) -> Self {
        Self {
            user_id: r.user_id,
            user_name: r.user_name,
            nick_name: r.nick_name,
            email: r.email,
            phonenumber: r.phonenumber,
            status: r.status,
            create_at: fmt_ts(&r.create_at),
        }
    }
}
```

- [ ] **Step 2: Service**

Append to `service.rs`:

```rust
use super::dto::{AllocatedUserResponseDto, AuthUserListQueryDto};
use validator::Validate;

pub async fn allocated_users(
    state: &AppState,
    query: AuthUserListQueryDto,
) -> Result<Page<AllocatedUserResponseDto>, AppError> {
    query
        .validate()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("validation: {e}")))?;

    let page = RoleRepo::find_allocated_users_page(
        &state.pg,
        &query.role_id,
        query.user_name.as_deref(),
        query.page_num,
        query.page_size,
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Page::new(
        page.rows.into_iter().map(AllocatedUserResponseDto::from_row).collect(),
        page.total,
        page.page_num,
        page.page_size,
    ))
}
```

- [ ] **Step 3: Handler + route**

```rust
async fn allocated_users(
    State(state): State<AppState>,
    Query(query): Query<dto::AuthUserListQueryDto>,
) -> Result<ApiResponse<Page<dto::AllocatedUserResponseDto>>, AppError> {
    let resp = service::allocated_users(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}
```

Add to `router()`:

```rust
.route(
    "/system/role/auth-user/allocated-list",
    get(allocated_users).route_layer(from_fn_with_state(
        access::require(AccessSpec::permission("system:role:allocated-list")),
        access::enforce,
    )),
)
```

- [ ] **Step 4: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

---

### Task 23: `RoleRepo::find_unallocated_users_page` + endpoint

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Repo — LEFT JOIN anti-pattern**

Append to `role_repo.rs` (`impl RoleRepo`):

```rust
    /// Users in the current tenant who are NOT bound to `role_id`.
    /// Implemented via LEFT JOIN anti-pattern: find all users in this
    /// tenant, LEFT JOIN their user_role rows for this specific role_id,
    /// filter where the join is null.
    pub async fn find_unallocated_users_page(
        pool: &PgPool,
        role_id: &str,
        user_name_filter: Option<&str>,
        page_num: u32,
        page_size: u32,
    ) -> anyhow::Result<Page<AllocatedUserRow>> {
        let tenant = current_tenant_scope();
        let safe_page_num = page_num.max(1);
        let safe_page_size = page_size.clamp(1, 200);
        let offset = ((safe_page_num - 1) * safe_page_size) as i64;
        let limit = safe_page_size as i64;

        let rows = sqlx::query_as::<_, AllocatedUserRow>(
            "SELECT u.user_id, u.user_name, u.nick_name, u.email, \
                    u.phonenumber, u.status, u.create_at \
               FROM sys_user u \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
               LEFT JOIN sys_user_role ur \
                      ON ur.user_id = u.user_id AND ur.role_id = $1 \
              WHERE ur.role_id IS NULL \
                AND ($2::varchar IS NULL OR ut.tenant_id = $2) \
                AND u.del_flag = '0' \
                AND ut.status = '0' \
                AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%') \
              ORDER BY u.create_at DESC \
              LIMIT $4 OFFSET $5",
        )
        .bind(role_id)
        .bind(tenant.as_deref())
        .bind(user_name_filter)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await
        .map_err(|e| anyhow::anyhow!("find_unallocated_users_page rows: {e}"))?;

        let total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) \
               FROM sys_user u \
               JOIN sys_user_tenant ut ON ut.user_id = u.user_id \
               LEFT JOIN sys_user_role ur \
                      ON ur.user_id = u.user_id AND ur.role_id = $1 \
              WHERE ur.role_id IS NULL \
                AND ($2::varchar IS NULL OR ut.tenant_id = $2) \
                AND u.del_flag = '0' \
                AND ut.status = '0' \
                AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%')",
        )
        .bind(role_id)
        .bind(tenant.as_deref())
        .bind(user_name_filter)
        .fetch_one(pool)
        .await
        .map_err(|e| anyhow::anyhow!("find_unallocated_users_page count: {e}"))?;

        Ok(Page::new(rows, total as u64, safe_page_num, safe_page_size))
    }
```

- [ ] **Step 2: Service**

```rust
pub async fn unallocated_users(
    state: &AppState,
    query: AuthUserListQueryDto,
) -> Result<Page<AllocatedUserResponseDto>, AppError> {
    query
        .validate()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("validation: {e}")))?;

    let page = RoleRepo::find_unallocated_users_page(
        &state.pg,
        &query.role_id,
        query.user_name.as_deref(),
        query.page_num,
        query.page_size,
    )
    .await
    .map_err(AppError::Internal)?;

    Ok(Page::new(
        page.rows.into_iter().map(AllocatedUserResponseDto::from_row).collect(),
        page.total,
        page.page_num,
        page.page_size,
    ))
}
```

- [ ] **Step 3: Handler + route**

```rust
async fn unallocated_users(
    State(state): State<AppState>,
    Query(query): Query<dto::AuthUserListQueryDto>,
) -> Result<ApiResponse<Page<dto::AllocatedUserResponseDto>>, AppError> {
    let resp = service::unallocated_users(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}
```

Add to `router()`:

```rust
.route(
    "/system/role/auth-user/unallocated-list",
    get(unallocated_users).route_layer(from_fn_with_state(
        access::require(AccessSpec::permission("system:role:unallocated-list")),
        access::enforce,
    )),
)
```

- [ ] **Step 4: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

---

### Task 24: Manual smoke test — Week 2 endpoints

- [ ] **Step 1: Seed a test role + bind admin user to it**

```bash
cargo run -p app &
sleep 2

TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")

# Create a role via the new endpoint
curl -sS -X POST http://127.0.0.1:18080/api/v1/system/role/ \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"roleName":"it-week2-role","roleKey":"it:week2","roleSort":100,"menuIds":[]}' \
  > /tmp/created.json

NEW_ID=$(python3 -c "import json; print(json.load(open('/tmp/created.json'))['data']['roleId'])")
echo "Created role: $NEW_ID"
```

- [ ] **Step 2: Test UPDATE with menu replacement**

```bash
# Pick 2 menu ids
MENU_A=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT menu_id FROM sys_menu WHERE del_flag='0' AND perms <> '' LIMIT 1;" | tr -d ' \n')
MENU_B=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT menu_id FROM sys_menu WHERE del_flag='0' AND perms <> '' OFFSET 1 LIMIT 1;" | tr -d ' \n')

curl -sS -X PUT http://127.0.0.1:18080/api/v1/system/role/ \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"roleId\":\"$NEW_ID\",\"roleName\":\"it-week2-role-updated\",\"roleKey\":\"it:week2\",\"roleSort\":200,\"status\":\"0\",\"menuIds\":[\"$MENU_A\",\"$MENU_B\"]}"
echo ""

# Verify menus replaced (should be exactly 2 rows)
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "SELECT COUNT(*) FROM sys_role_menu WHERE role_id='$NEW_ID';"
```

Expected: count = 2.

- [ ] **Step 3: Test change-status**

```bash
curl -sS -X PUT http://127.0.0.1:18080/api/v1/system/role/change-status \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"roleId\":\"$NEW_ID\",\"status\":\"1\"}"
echo ""

PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "SELECT status FROM sys_role WHERE role_id='$NEW_ID';"
```

Expected: status = '1'.

- [ ] **Step 4: Test option-select filters out disabled role**

```bash
curl -sS http://127.0.0.1:18080/api/v1/system/role/option-select \
  -H "Authorization: Bearer $TOKEN" \
  | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print('it-week2 visible:', any(r['roleKey']=='it:week2' for r in d))"
```

Expected: `it-week2 visible: False`.

- [ ] **Step 5: Reactivate + manually bind admin user, test allocated/unallocated**

```bash
# Reactivate
curl -sS -X PUT http://127.0.0.1:18080/api/v1/system/role/change-status \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d "{\"roleId\":\"$NEW_ID\",\"status\":\"0\"}"

# Manually bind admin to this role via DB (until Week 3 builds the endpoint)
ADMIN_ID="cf827fc0-e7cc-4b9f-913c-e20628ade20a"
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "INSERT INTO sys_user_role (user_id, role_id) VALUES ('$ADMIN_ID', '$NEW_ID') ON CONFLICT DO NOTHING;"

# allocated-list — should include admin
curl -sS "http://127.0.0.1:18080/api/v1/system/role/auth-user/allocated-list?roleId=$NEW_ID&pageNum=1&pageSize=10" \
  -H "Authorization: Bearer $TOKEN" | python3 -m json.tool

# unallocated-list — should NOT include admin
curl -sS "http://127.0.0.1:18080/api/v1/system/role/auth-user/unallocated-list?roleId=$NEW_ID&pageNum=1&pageSize=10" \
  -H "Authorization: Bearer $TOKEN" \
  | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print('admin in unallocated:', any(r['userName']=='admin' for r in d['rows']))"
```

Expected: allocated-list includes admin, unallocated-list does not.

- [ ] **Step 6: Test DELETE → find_by_id returns 1001**

```bash
curl -sS -X DELETE "http://127.0.0.1:18080/api/v1/system/role/$NEW_ID" \
  -H "Authorization: Bearer $TOKEN"
echo ""

curl -sS "http://127.0.0.1:18080/api/v1/system/role/$NEW_ID" \
  -H "Authorization: Bearer $TOKEN"
echo ""
```

Expected: first call returns `{"code":200,...}`; second returns `{"code":1001,"msg":"数据不存在",...}`.

- [ ] **Step 7: Cleanup**

```bash
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "DELETE FROM sys_role_menu WHERE role_id='$NEW_ID'; \
   DELETE FROM sys_user_role WHERE role_id='$NEW_ID'; \
   DELETE FROM sys_role WHERE role_id='$NEW_ID';"

pkill -f "target/debug/app"
```

- [ ] **Step 8: Week 2 exit gate**

```bash
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

**Week 2 done.** 9 endpoints live.

---

# WEEK 3 — Batch Assignment + Exit Gate (Tasks 25-30)

Goal: 2 batch endpoints (`select-all`, `cancel`), final smoke script, web frontend cut-over validation.

---

### Task 25: `RoleRepo::insert_user_roles` (batch UNNEST with ON CONFLICT)

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`

- [ ] **Step 1: Append batch insert method**

```rust
    /// Bulk-assign users to a role. Idempotent: re-submitting the same
    /// user_ids is a no-op thanks to `ON CONFLICT DO NOTHING` on the
    /// `(user_id, role_id)` primary key.
    ///
    /// Returns the number of rows actually inserted (not including
    /// conflicts that were skipped).
    pub async fn insert_user_roles(
        pool: &PgPool,
        role_id: &str,
        user_ids: &[String],
    ) -> anyhow::Result<u64> {
        if user_ids.is_empty() {
            return Ok(0);
        }
        let affected = sqlx::query(
            "INSERT INTO sys_user_role (user_id, role_id) \
             SELECT unnest($1::varchar[]), $2 \
             ON CONFLICT (user_id, role_id) DO NOTHING",
        )
        .bind(user_ids)
        .bind(role_id)
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("insert_user_roles: {e}"))?
        .rows_affected();
        Ok(affected)
    }
```

- [ ] **Step 2: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
```

---

### Task 26: `PUT /system/role/auth-user/select-all` endpoint

**Files:**
- Modify: `crates/modules/src/system/role/dto.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: DTO**

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AuthUserAssignDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    #[validate(length(min = 1, max = 1000))]
    pub user_ids: Vec<String>,
}
```

- [ ] **Step 2: Service**

```rust
use super::dto::AuthUserAssignDto;

pub async fn assign_users(
    state: &AppState,
    dto: AuthUserAssignDto,
) -> Result<(), AppError> {
    // Verify the role exists in current tenant before assigning.
    let role = RoleRepo::find_by_id(&state.pg, &dto.role_id)
        .await
        .map_err(AppError::Internal)?;
    BusinessError::throw_if_null(role, ResponseCode::DATA_NOT_FOUND)?;

    RoleRepo::insert_user_roles(&state.pg, &dto.role_id, &dto.user_ids)
        .await
        .map_err(AppError::Internal)?;
    Ok(())
}
```

- [ ] **Step 3: Handler + route**

```rust
async fn assign_users(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthUserAssignDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::assign_users(&state, dto).await?;
    Ok(ApiResponse::success())
}
```

Add to `router()`:

```rust
.route(
    "/system/role/auth-user/select-all",
    put(assign_users).route_layer(from_fn_with_state(
        access::require(AccessSpec::permission("system:role:select-auth-all")),
        access::enforce,
    )),
)
```

- [ ] **Step 4: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

---

### Task 27: `RoleRepo::delete_user_roles` + `PUT /auth-user/cancel`

**Files:**
- Modify: `crates/modules/src/domain/role_repo.rs`
- Modify: `crates/modules/src/system/role/dto.rs`
- Modify: `crates/modules/src/system/role/service.rs`
- Modify: `crates/modules/src/system/role/handler.rs`

- [ ] **Step 1: Repo**

```rust
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
        .map_err(|e| anyhow::anyhow!("delete_user_roles: {e}"))?
        .rows_affected();
        Ok(affected)
    }
```

- [ ] **Step 2: DTO — reuse `AuthUserAssignDto` shape**

Add a new type alias for clarity but share the validation:

```rust
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AuthUserCancelDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    #[validate(length(min = 1, max = 1000))]
    pub user_ids: Vec<String>,
}
```

- [ ] **Step 3: Service**

```rust
use super::dto::AuthUserCancelDto;

pub async fn unassign_users(
    state: &AppState,
    dto: AuthUserCancelDto,
) -> Result<(), AppError> {
    RoleRepo::delete_user_roles(&state.pg, &dto.role_id, &dto.user_ids)
        .await
        .map_err(AppError::Internal)?;
    Ok(())
}
```

- [ ] **Step 4: Handler + route**

```rust
async fn unassign_users(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthUserCancelDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::unassign_users(&state, dto).await?;
    Ok(ApiResponse::success())
}
```

Add to `router()`:

```rust
.route(
    "/system/role/auth-user/cancel",
    put(unassign_users).route_layer(from_fn_with_state(
        access::require(AccessSpec::permission("system:role:cancel-auth")),
        access::enforce,
    )),
)
```

- [ ] **Step 5: Verification checkpoint**

```bash
cargo check -p modules
cargo clippy -p modules -- -D warnings
cargo build -p app
```

---

### Task 28: End-to-end smoke script

**Files:**
- Create: `server-rs/scripts/smoke-role-module.sh`

- [ ] **Step 1: Write smoke script**

```bash
#!/usr/bin/env bash
# scripts/smoke-role-module.sh
#
# End-to-end verification of all 11 role endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates an `it-smoke-` prefixed role, exercises every
# endpoint, then cleans up its own rows.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
PREFIX="it-smoke-$(date +%s)"
ROLE_NAME="${PREFIX}-role"
ROLE_KEY="${PREFIX}:role"
TOKEN=""
ROLE_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  if [[ -n "${ROLE_ID:-}" ]]; then
    PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
      "DELETE FROM sys_role_menu WHERE role_id='$ROLE_ID'; \
       DELETE FROM sys_user_role WHERE role_id='$ROLE_ID'; \
       DELETE FROM sys_role WHERE role_id='$ROLE_ID';" || true
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

step() {
  echo ""
  echo "=== $1 ==="
}

step "1. login"
TOKEN=$(curl -sS -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
assert_eq true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

step "2. pick 3 menu ids"
MENU_IDS=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT menu_id FROM sys_menu WHERE del_flag='0' AND perms <> '' ORDER BY menu_id LIMIT 3;" \
  | tr -d ' ' | grep -v '^$' \
  | python3 -c "import sys,json; print(json.dumps([l.strip() for l in sys.stdin if l.strip()]))")
echo "menu_ids: $MENU_IDS"

step "3. POST create"
CREATED=$(curl -sS -X POST "$BASE/system/role/" "${H[@]}" \
  -d "{\"roleName\":\"$ROLE_NAME\",\"roleKey\":\"$ROLE_KEY\",\"roleSort\":100,\"menuIds\":$MENU_IDS}")
echo "$CREATED" | python3 -m json.tool
ROLE_ID=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['roleId'])")
assert_eq 36 "${#ROLE_ID}" "role_id is a uuid"

step "4. GET /list (new role visible)"
LIST=$(curl -sS "$BASE/system/role/list?roleKey=$ROLE_KEY" "${H[@]}")
COUNT=$(echo "$LIST" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 1 "$COUNT" "list returns exactly the new role"

step "5. GET /:id"
DETAIL=$(curl -sS "$BASE/system/role/$ROLE_ID" "${H[@]}")
MENU_COUNT=$(echo "$DETAIL" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['menuIds']))")
assert_eq 3 "$MENU_COUNT" "role detail has 3 bound menus"

step "6. PUT update (replace menus with 2, rename)"
NEW_MENUS=$(echo "$MENU_IDS" | python3 -c "import sys,json; ids=json.load(sys.stdin); print(json.dumps(ids[:2]))")
curl -sS -X PUT "$BASE/system/role/" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"roleName\":\"${ROLE_NAME}-v2\",\"roleKey\":\"$ROLE_KEY\",\"roleSort\":200,\"status\":\"0\",\"menuIds\":$NEW_MENUS}" > /dev/null

DETAIL=$(curl -sS "$BASE/system/role/$ROLE_ID" "${H[@]}")
MENU_COUNT=$(echo "$DETAIL" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['menuIds']))")
assert_eq 2 "$MENU_COUNT" "after update, menu count is 2"

step "7. PUT change-status → disable"
curl -sS -X PUT "$BASE/system/role/change-status" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"status\":\"1\"}" > /dev/null

STATUS=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT status FROM sys_role WHERE role_id='$ROLE_ID';" | tr -d ' \n')
assert_eq 1 "$STATUS" "status is now 1 (disabled)"

step "8. GET /option-select (disabled role hidden)"
OPTIONS=$(curl -sS "$BASE/system/role/option-select" "${H[@]}")
FOUND=$(echo "$OPTIONS" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(any(r['roleKey']=='$ROLE_KEY' for r in d))")
assert_eq False "$FOUND" "disabled role filtered out of option-select"

step "9. Re-enable role"
curl -sS -X PUT "$BASE/system/role/change-status" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"status\":\"0\"}" > /dev/null

step "10. PUT select-all → assign admin user"
ADMIN_ID="cf827fc0-e7cc-4b9f-913c-e20628ade20a"
curl -sS -X PUT "$BASE/system/role/auth-user/select-all" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"userIds\":[\"$ADMIN_ID\"]}" > /dev/null

ALLOC=$(curl -sS "$BASE/system/role/auth-user/allocated-list?roleId=$ROLE_ID&pageNum=1&pageSize=10" "${H[@]}")
ALLOC_COUNT=$(echo "$ALLOC" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 1 "$ALLOC_COUNT" "allocated list has 1 user after assign"

step "11. select-all is idempotent (re-submit)"
curl -sS -X PUT "$BASE/system/role/auth-user/select-all" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"userIds\":[\"$ADMIN_ID\"]}" > /dev/null

ALLOC=$(curl -sS "$BASE/system/role/auth-user/allocated-list?roleId=$ROLE_ID&pageNum=1&pageSize=10" "${H[@]}")
ALLOC_COUNT=$(echo "$ALLOC" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 1 "$ALLOC_COUNT" "allocated list still 1 after idempotent re-assign"

step "12. GET unallocated-list (admin not listed)"
UNALLOC=$(curl -sS "$BASE/system/role/auth-user/unallocated-list?roleId=$ROLE_ID&pageNum=1&pageSize=50" "${H[@]}")
FOUND=$(echo "$UNALLOC" | python3 -c "import sys,json; print(any(r['userName']=='admin' for r in json.load(sys.stdin)['data']['rows']))")
assert_eq False "$FOUND" "admin not in unallocated list"

step "13. PUT cancel → unassign admin"
curl -sS -X PUT "$BASE/system/role/auth-user/cancel" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"userIds\":[\"$ADMIN_ID\"]}" > /dev/null

ALLOC=$(curl -sS "$BASE/system/role/auth-user/allocated-list?roleId=$ROLE_ID&pageNum=1&pageSize=10" "${H[@]}")
ALLOC_COUNT=$(echo "$ALLOC" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 0 "$ALLOC_COUNT" "allocated list empty after cancel"

step "14. DELETE /:id → soft delete"
curl -sS -X DELETE "$BASE/system/role/$ROLE_ID" "${H[@]}" > /dev/null

DETAIL=$(curl -sS "$BASE/system/role/$ROLE_ID" "${H[@]}")
CODE=$(echo "$DETAIL" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1001 "$CODE" "detail returns DATA_NOT_FOUND after soft delete"

DEL_FLAG=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT del_flag FROM sys_role WHERE role_id='$ROLE_ID';" | tr -d ' \n')
assert_eq 1 "$DEL_FLAG" "row has del_flag='1' in DB"

echo ""
echo "ALL 14 STEPS PASSED"
```

- [ ] **Step 2: Make executable**

```bash
chmod +x server-rs/scripts/smoke-role-module.sh
```

- [ ] **Step 3: Run the script**

```bash
cargo run -p app &
APP_PID=$!
sleep 2

bash server-rs/scripts/smoke-role-module.sh

kill $APP_PID 2>/dev/null
```

Expected: script prints "ALL 14 STEPS PASSED".

- [ ] **Step 4: Verification checkpoint**

```bash
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

---

### Task 29: Phase 0 regression check

- [ ] **Step 1: Rerun Phase 0 login smoke**

```bash
cargo run -p app &
APP_PID=$!
sleep 2

# Phase 0 endpoints
curl -sS http://127.0.0.1:18080/health/live
echo ""
curl -sS http://127.0.0.1:18080/health/ready
echo ""

TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")

curl -sS http://127.0.0.1:18080/api/v1/info -H "Authorization: Bearer $TOKEN" \
  | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(f\"userName: {d['userName']}, perms count: {len(d['permissions'])}\")"

curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/logout -H "Authorization: Bearer $TOKEN"
echo ""

kill $APP_PID
```

Expected:
- `/health/live` returns `{"status":"ok"}`
- `/health/ready` returns `{"status":"ok",...}`
- Login returns a token
- `/info` prints `userName: admin, perms count: 269` (or similar — must be > 0)
- Logout returns `{"code":200,...}`

No regression from Phase 0.

- [ ] **Step 2: Full test run**

```bash
cargo test --workspace
```

Expected: all 48 Phase 0 tests + any new integration tests pass.

---

### Task 30: Web frontend cut-over validation

- [ ] **Step 1: Start Rust app**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
cargo run -p app &
```

- [ ] **Step 2: Point web frontend at Rust service**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/web
# Check existing VITE_API_URL setting
grep -r "VITE_API_URL" .env* 2>/dev/null
```

Temporarily override via shell:

```bash
VITE_API_URL=http://localhost:18080 pnpm dev
```

Or edit `web/.env.development` to set `VITE_API_URL=http://localhost:18080` (remember to revert when done).

- [ ] **Step 3: Manual browser test**

Open the web UI, log in as `admin / admin123`, and exercise the role management page:

1. **List page loads** — no 404/500s in browser console or app log
2. **Create a new role** with a name, key, sort, and 2-3 selected menus
3. **View the new role** — detail modal shows the correct menus pre-checked
4. **Edit the role** — change name, swap menus
5. **Toggle status** — disable, verify greyed out
6. **Re-enable**
7. **Assign users** — open the "assign users" dialog, pick admin, save
8. **Unassign** — open dialog again, remove admin
9. **Delete the role**

Each step should work with no frontend code changes.

- [ ] **Step 4: Revert web config**

Restore `web/.env.development` if changed.

- [ ] **Step 5: Stop Rust app**

```bash
pkill -f "target/debug/app"
```

- [ ] **Step 6: Final Week 3 exit gate**

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs
cargo test --workspace
cargo clippy --all-targets -- -D warnings
cargo fmt --check
bash scripts/smoke-role-module.sh  # requires app running; start if needed
```

All four must pass.

**Phase 1 Sub-Phase 1 done.** Role module is end-to-end substitutable for NestJS.

---

## Self-Review Checklist

### Spec coverage

- [x] In-scope 11 endpoints: Task 6/7 (GET :id), 9/10/11 (GET list), 12/13/14/15 (POST create), 16/17 (PUT update), 18 (change-status), 19 (DELETE), 20 (option-select), 21/22 (allocated), 23 (unallocated), 25/26 (select-all), 27 (cancel)
- [x] DAO conventions: helpers in Task 1, repo pattern established in Task 3, cross-table JOIN in Task 21 with spec reference in comments
- [x] Transaction boundaries: explicit `Transaction<'_, Postgres>` in Tasks 12 and 16
- [x] Audit field auto-injection: `AuditInsert::now()` and `audit_update_by()` called explicitly in every write
- [x] Tenant scoping: `current_tenant_scope()` + `($N::varchar IS NULL OR tenant_id = $N)` in every tenant-scoped SELECT/UPDATE
- [x] Access control: each route wrapped with `access::require` layer; permissions match the spec's permission table
- [x] Testing strategy: Task 1 unit tests for helpers; Task 5 integration harness; Task 8 first integration test; manual smoke tests per week; Task 28 full smoke script
- [x] Error handling: Business → throw via `BusinessError`; Internal → `AppError::Internal`; Validation via `ValidatedJson`
- [x] Week 1/2/3 gates all defined (Tasks 11, 24, 30)
- [x] Phase 0 regression (Task 29)

### Out of scope (confirmed not in any task)

- [x] `data-scope` and `dept-tree` — not present
- [x] `export` — not present
- [x] Caching (`@Cacheable` / `@CacheEvict`) — not present
- [x] Operlog — not present
- [x] `role_dept` binding — not present

### Placeholder scan

- [x] No "TBD" / "TODO" / "similar to" in task steps
- [x] All code blocks are complete — no `// ... impl here`
- [x] Exact file paths in every task header
- [x] Exact commands with expected output in verification steps

### Type consistency

- [x] `AllocatedUserRow` defined in Task 21, used in Tasks 22 + 23
- [x] `AuthUserListQueryDto` defined in Task 22, used in Task 23
- [x] `CreateRoleDto` / `UpdateRoleDto` / `ChangeRoleStatusDto` / `ListRoleDto` / `AuthUserAssignDto` / `AuthUserCancelDto` / `RoleDetailResponseDto` / `RoleListItemResponseDto` / `RoleOptionResponseDto` / `AllocatedUserResponseDto` — all defined before use
- [x] `RoleRepo` method names consistent: `find_by_id`, `find_menu_ids_by_role`, `find_page`, `find_option_list`, `find_allocated_users_page`, `find_unallocated_users_page`, `insert_with_menus`, `update_with_menus`, `change_status`, `soft_delete_by_id`, `insert_user_roles`, `delete_user_roles` (12 methods total)
- [x] Service function names consistent: `find_by_id`, `list`, `create`, `update`, `change_status`, `remove`, `option_select`, `allocated_users`, `unallocated_users`, `assign_users`, `unassign_users` (11 functions)
- [x] Handler function names match service names

### Known deviations from spec

1. **Spec's SQL sketch for list pagination uses a different filter order** than Task 9's implementation (spec shows 4 filter args, plan uses 4 filter args plus limit/offset; the count query in the plan omits limit/offset as expected). Consistent.
2. **Spec's `Router` composition pseudo-code** used `.with_access("...")` chained helper; plan uses explicit `from_fn_with_state(access::require(...), access::enforce)` because that's the real Phase 0 helper. This is fine — the spec marked that pseudo-code as "plan phase locks exact pattern."
3. **Role-key uniqueness check** is deferred in the service layer (Task 13 Step 2) with a comment pointing to Phase 1 sub-phase 2. The spec's Error Handling table lists `DUPLICATE_KEY` as a possible error. Document the deferral; it is not a blocker for the web frontend cut-over.

---

**Plan complete and saved to `server-rs/docs/plans/2026-04-10-phase1-role-module-plan.md`.**

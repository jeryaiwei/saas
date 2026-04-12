# Phase 1 Sub-Phase 2a — User Module (CRUD + admin 管控)

**Status:** Draft (brainstorm 2026-04-11)
**Supersedes:** n/a — first spec in Phase 1 Sub-Phase 2
**Predecessor:** [2026-04-10-phase1-role-module-design.md](./2026-04-10-phase1-role-module-design.md) (Phase 1 Sub-Phase 1, role module — complete)
**Predicted duration:** 3 weeks
**Predicted LOC delta:** ~1400 new + ~60 modified

---

## Context

Phase 1 Sub-Phase 1 delivered the role module end-to-end (11 endpoints, 110 tests passing, real-DB integration suite, smoke script). It validated the Route C data-layer choice and established the Batch 5.5 ergonomic primitives (`IntoAppError`, `require_permission!`, `PageQuery`, `ValidatedQuery`, `Page::map_rows`, `AuditInsert`, `current_tenant_scope`). The role slice was the proof; user slice is the first module to cash in on the template.

Phase 1 Sub-Phase 2a narrows the NestJS user module (21 endpoints total) to the **CRUD + admin management** subset — the endpoints Vue web's "系统管理 → 用户管理" page exercises. Out of scope: personal profile / avatar upload / password self-change (Sub-Phase 2b), batch create/delete (Sub-Phase 2b), xlsx export (observability sub-phase), dept-tree and role-post helper queries (depend on dept/post modules that don't exist yet).

Success for this sub-phase means Vue web's user management page can switch `VITE_API_URL=http://localhost:18080` and complete every admin flow — list, create with roles, edit, change status, reset password, assign roles — without changing any frontend code.

---

## Scope

### In — 11 endpoints (target for 3 weeks)

| # | Method + Path | Permission | Complexity | Week |
| --- | --- | --- | --- | --- |
| 1 | `GET /api/v1/system/user/list` | `system:user:list` | Paginated list with tenant JOIN + filters | 1 |
| 2 | `GET /api/v1/system/user/:userId` | `system:user:query` | Single read + role_ids + post_ids projection | 1 |
| 3 | `GET /api/v1/system/user/option-select` | _authenticated_ | Flat search-capable list for dropdowns | 1 |
| 4 | `GET /api/v1/system/user/info` | _authenticated_ | Current user profile for Vue frontend | 1 |
| 5 | `POST /api/v1/system/user/` | `system:user:add` | Multi-table tx (sys_user + sys_user_role + sys_user_tenant) | 2 |
| 6 | `PUT /api/v1/system/user/` | `system:user:edit` | UPDATE + role rebind (delete-all + insert-all) | 2 |
| 7 | `PUT /api/v1/system/user/change-status` | _TENANT_ADMIN role_ | Single UPDATE, admin+self guards | 2 |
| 8 | `DELETE /api/v1/system/user/:id` | _TENANT_ADMIN role_ | Soft delete, admin+self guards, CSV multi-id | 2 |
| 9 | `PUT /api/v1/system/user/reset-pwd` | _TENANT_ADMIN role_ | Hash new password, admin guard, invalidate sessions | 3 |
| 10 | `GET /api/v1/system/user/auth-role/:id` | _TENANT_ADMIN role_ | Delegates to `role_repo::find_role_ids_by_user` | 3 |
| 11 | `PUT /api/v1/system/user/auth-role` | _TENANT_ADMIN role_ | Delegates to `role_repo::replace_user_roles` (tx) | 3 |

Permission strings with hyphens match NestJS exactly. Four endpoints use the `TENANT_ADMIN` role-based check rather than a permission string — the existing `require_role!` framework macro (extracted in Batch 5.5) wires these identically to `require_permission!`.

### Out — deferred to later sub-phases

- `GET /profile`, `PUT /profile`, `POST /profile/avatar`, `PUT /profile/update-pwd` — personal center endpoints. Different access semantics (caller is the target). Grouped into Sub-Phase 2b once the admin CRUD template is proven.
- `POST /batch`, `DELETE /batch` — batch create/delete. Mechanically similar to the single-item versions but introduce partial-success reporting that's worth solving once instead of twice. Sub-Phase 2b.
- `POST /export` — xlsx streaming. Independent infrastructure work (requires an xlsx writer crate, streaming response type, long-running request handling). Observability sub-phase.
- `GET /dept-tree` — depends on dept module. Defer until dept slice.
- `GET /` (role-post helper) — depends on post module. Defer until post slice. The role half of this payload is already available via `GET /system/role/option-select`; the frontend can stitch until post ships.
- `GET /list/dept/:deptId` — depends on dept validation. Defer.
- Login, logout, get-current-user-permissions — already delivered in Phase 0 (`/auth/login`, `/auth/logout`, `/api/v1/info`). Sub-Phase 2a's `/system/user/info` endpoint is a separate NestJS endpoint aimed at user-management pages, not the post-login profile fetch — see the data-flow note below.

**Clarification on `/system/user/info` vs Phase 0 `/api/v1/info`**: NestJS has two GET-info endpoints with similar names but different shapes. Phase 0's `/api/v1/info` returns `{ user, permissions, roles, tenants }` for the post-login Soybean frontend bootstrap — already done. `/system/user/info` returns a leaner `{ user }` shape used by specific admin pages. Sub-Phase 2a implements the second one; it does NOT touch Phase 0's implementation.

---

## Technical Approach

### Continues Route C from Phase 1 Sub-Phase 1

All conventions from the role module spec apply unchanged:

- Hand-written SQL via `sqlx::query_as::<_, T>(sql)` + runtime binding
- `const COLUMNS: &str` in each repo to keep SELECT lists in sync
- Static `RoleRepo`-style struct namespaces with async methods
- Transactions as `&mut Transaction<'_, Postgres>` passed as function parameters
- `AuditInsert::now()` + `audit_update_by()` called explicitly by each write
- `#[instrument(skip_all, fields(...))]` on every public repo method
- Service layer uses ergonomic error traits: `.into_internal()?`, `.or_business(code)?`, `.business_err_if(code)`
- Handler layer uses `require_permission!(...)` / `require_role!(...)` macro route layers, thin extract-delegate-wrap shape
- DTOs use `#[serde(rename_all = "camelCase")]` + `#[serde(flatten)] pub page: PageQuery` for list queries

### Tenant-scoping via `sys_user_tenant` JOIN (new pattern)

**This is the one structural change from the role module.** Where `sys_role` had a direct `tenant_id` column and filter `WHERE tenant_id = $n`, `sys_user` does not. A user's tenant membership lives in `sys_user_tenant` as a many-to-many join. Every read/write in this module that should be tenant-scoped must enforce membership through that join table.

The `platform_id` column on `sys_user` is NOT the tenant_id — it is the platform identifier (always `'000000'` in the current deployment) and must not be used for tenant scoping.

**Read pattern** (used by list, find_by_id, option-select):

```sql
SELECT {COLUMNS}
  FROM sys_user u
  JOIN sys_user_tenant ut ON ut.user_id = u.user_id
 WHERE u.del_flag = '0'
   AND ut.status = '0'
   AND ($1::varchar IS NULL OR ut.tenant_id = $1)
```

**Write pattern** (used by update, change-status, soft-delete, reset-pwd): the UPDATE/DELETE statement cannot JOIN directly, so use an `EXISTS` subquery to enforce membership:

```sql
UPDATE sys_user
   SET nick_name = $1, ...
 WHERE user_id = $2
   AND del_flag = '0'
   AND ($3::varchar IS NULL OR EXISTS (
         SELECT 1 FROM sys_user_tenant
          WHERE user_id = sys_user.user_id
            AND tenant_id = $3
            AND status = '0'
       ))
```

Cross-tenant edit attempts surface as `affected_rows = 0` → `DATA_NOT_FOUND` (information hiding, same strategy as role module).

**Create pattern**: opens a transaction; inserts `sys_user`, then bulk-inserts `sys_user_role` via `RoleRepo::bulk_insert_role_menus`-style UNNEST, then inserts the single `sys_user_tenant` row for the current tenant. One commit.

### `sys_user_tenant` writes — temporarily owned by `user_repo`

Per DAO rule 4, `sys_user_tenant` should have a single writer. The natural owner is a future tenant module. Until that module exists, `user_repo.rs` temporarily owns writes to `sys_user_tenant` — specifically, the single `insert_user_tenant_binding` helper called during user creation. The spec documents this as a temporary ownership; when the tenant module starts, migration is a 15-minute mechanical move.

Soft-deleting a user does **not** cascade-update `sys_user_tenant.status`. The allocated-list / user-list JOIN patterns already filter on `u.del_flag = '0'`, so soft-deleted users are invisible to reads without needing to touch the join table. Keeping the write surface minimal.

### Password handling — framework crypto, not new helpers

`framework::infra::crypto::{hash_password, verify_password}` already exists (Phase 0). User module's create and reset-pwd endpoints call `hash_password` directly. No new helper; no new crate.

Password complexity validation mirrors NestJS's `IsStrongPassword` rule — **but** that requirement lives on the request DTO, not the hashing function. Sub-Phase 2a enforces a relaxed rule via `validator` crate: length 6-20, at least one letter and one digit. The NestJS "upper + lower + digit + symbol" rule is stricter but pulling in a `regex`-based custom validator for it is low-value until we have a documented policy. Noted as a risk.

### Self-guard and admin protection

NestJS enforces these invariants at the service layer on five endpoints. Sub-Phase 2a matches them exactly:

| Endpoint | Self-op blocked? | Admin target blocked? |
|---|---|---|
| `DELETE /system/user/:id` | ✓ | ✓ |
| `PUT /change-status` | ✓ | ✓ |
| `PUT /reset-pwd` | – | ✓ |
| `PUT /` (edit) | – | user_name immutable on admin |
| `PUT /auth-role` | ✓ | ✓ |

**"Admin user" is defined strictly as** `user_name = 'admin' AND platform_id = '000000'` — the superuser row, not any user whose `sys_user_tenant.is_admin = true`. Service-layer helper:

```rust
async fn is_super_admin_user(pool: &PgPool, user_id: &str) -> anyhow::Result<bool>;
```

**"Self"** is `user_id == current RequestContext user_id`. No DB query required; RequestContext is always populated inside an authenticated request.

On violation, service returns `AppError::Business(ResponseCode::OPERATION_NOT_ALLOWED)` — **a new business code added in this sub-phase** (see Error Handling).

---

## Data Model (6 tables touched, 0 new)

| Table | Access | Why |
|---|---|---|
| `sys_user` | RW | user entity |
| `sys_user_role` | RW via `role_repo` | role bindings (created on user create, replaced on auth-role update) |
| `sys_user_tenant` | RW | tenant membership (created on user create; read on every tenant-scoped query) |
| `sys_role` | R | validation — role_ids on create/update must exist in current tenant |
| `sys_user_post` | **not touched** | post assignments deferred with post module |
| `sys_dept` | **not touched** | dept_id is stored but not validated (dept module not yet built) |

All writes to `sys_user_role` go through `RoleRepo::bulk_insert_role_menus`-style helpers that live in `role_repo.rs` — see DAO conventions below.

---

## Component Breakdown

### New files

```text
server-rs/crates/modules/src/
├── domain/
│   ├── entities.rs           (modified, +35 LOC)  — add SysUser struct
│   └── user_repo.rs          (modified, +280 LOC) — extend existing Phase 0 file: add insert/update/find_page/soft_delete/change_status/reset_pwd + insert_user_tenant_binding
└── system/
    └── user/                 (new package)
        ├── mod.rs            (new, ~5 LOC)      — re-exports + router()
        ├── dto.rs            (new, ~260 LOC)    — 10+ DTOs + validation + fixture macros + tests
        ├── service.rs        (new, ~350 LOC)    — orchestrates user_repo + role_repo calls + transactions + guards
        └── handler.rs        (new, ~250 LOC)    — axum handlers + route-level access_spec layers
```

### Modified files

```text
server-rs/crates/modules/src/lib.rs             — re-export system::user, extend router()
server-rs/crates/modules/src/domain/role_repo.rs — add find_role_ids_by_user + replace_user_roles + verify_role_ids_in_tenant (3 new user-facing methods)
server-rs/crates/framework/src/response/codes.rs — add OPERATION_NOT_ALLOWED = 1004
server-rs/crates/modules/src/system/role/dto.rs  — promote fmt_ts to framework::response (see Follow-Ups)
server-rs/crates/framework/src/response/mod.rs   — re-export fmt_ts after promotion
```

### Responsibility boundaries

- **`domain::entities::SysUser`** — full-row `FromRow` struct, zero logic. Mirrors `sys_user` columns, including `password` hash (sensitive — never serialized to wire, see SecurityNote below).
- **`domain::user_repo::UserRepo`** — extends existing Phase 0 code. Read methods use the `sys_user JOIN sys_user_tenant` pattern. Writes use EXISTS subquery for tenant guard.
- **`domain::role_repo::RoleRepo`** — adds `find_role_ids_by_user(pool, user_id)` and `replace_user_roles(tx, user_id, role_ids)`. These are the two methods user module's `auth-role` endpoints delegate to, preserving DAO rule 4 (sys_user_role single-owner).
- **`system::user::dto`** — NestJS wire compat. Each DTO has its own `#[validate]` rules + a fixture macro for tests.
- **`system::user::service`** — business rules + self-guard + admin protection + transaction boundaries. Composes `UserRepo` and `RoleRepo` calls.
- **`system::user::handler`** — thin axum handlers. Applies `require_permission!` or `require_role!` per route.

### DAO conventions — unchanged from Sub-Phase 1

Same four rules. Highlights of what this means in Sub-Phase 2a:

1. Each repo method = one SQL statement or one tightly-coupled transaction ✓
2. Repo methods never call other repos. User service calls both `UserRepo` and `RoleRepo` — that's service-layer orchestration, allowed.
3. Cross-table JOINs allowed in any repo. `find_page` joins `sys_user + sys_user_tenant`; the JOIN is read-only and lives in `user_repo.rs` because the caller's mental model is "give me this tenant's users."
4. Writes are single-owner. `user_repo` owns `sys_user` writes. `role_repo` owns `sys_user_role` writes. `user_repo` **temporarily** owns `sys_user_tenant` writes (see Technical Approach).

### SysUser entity — sensitive fields

```rust
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
    pub password: String,  // bcrypt hash — NEVER serialized
    pub status: String,
    pub del_flag: String,
    pub login_ip: String,
    pub login_date: Option<chrono::DateTime<chrono::Utc>>,
    pub create_by: String,
    pub create_at: chrono::DateTime<chrono::Utc>,
    pub update_by: String,
    pub update_at: chrono::DateTime<chrono::Utc>,
    pub remark: Option<String>,
}
```

**Security note on `password` field**: `SysUser` intentionally does NOT derive `Serialize`. Wire responses use DTOs (`UserDetailResponseDto`, `UserListItemResponseDto`, etc.) that explicitly omit the password. This prevents accidental leakage through a new endpoint that happens to return the raw entity. Every DTO must construct via `from_entity` and explicitly skip `password`.

---

## Key SQL Sketches

### Tenant-scoped find_by_id

```sql
SELECT {COLUMNS}
  FROM sys_user u
  JOIN sys_user_tenant ut ON ut.user_id = u.user_id
 WHERE u.user_id = $1
   AND u.del_flag = '0'
   AND ut.status = '0'
   AND ($2::varchar IS NULL OR ut.tenant_id = $2)
 LIMIT 1
```

The LIMIT 1 matters: if a user has multiple tenant bindings and super-tenant scope is active, the JOIN can produce multiple rows. We only want the user entity once.

### Paginated list with filters

```sql
SELECT {COLUMNS}
  FROM sys_user u
  JOIN sys_user_tenant ut ON ut.user_id = u.user_id
 WHERE u.del_flag = '0'
   AND ut.status = '0'
   AND ($1::varchar IS NULL OR ut.tenant_id = $1)
   AND ($2::varchar IS NULL OR u.user_name LIKE '%' || $2 || '%')
   AND ($3::varchar IS NULL OR u.nick_name LIKE '%' || $3 || '%')
   AND ($4::varchar IS NULL OR u.phonenumber LIKE '%' || $4 || '%')
   AND ($5::varchar IS NULL OR u.email LIKE '%' || $5 || '%')
   AND ($6::varchar IS NULL OR u.status = $6)
   AND ($7::varchar IS NULL OR u.dept_id = $7)
 ORDER BY u.create_at DESC
 LIMIT $8 OFFSET $9
```

Companion `SELECT COUNT(*)` with matching WHERE.

### Create user + role bindings + tenant binding (transaction)

```sql
-- Step 1: INSERT sys_user with bcrypt'd password
INSERT INTO sys_user (
  user_id, platform_id, dept_id, user_name, nick_name, user_type,
  email, phonenumber, sex, avatar, password, status, del_flag,
  create_by, update_by, update_at, remark
) VALUES ($1, '000000', $2, $3, $4, '10',
          $5, $6, $7, $8, $9, $10, '0',
          $11, $12, CURRENT_TIMESTAMP, $13)
RETURNING {COLUMNS};

-- Step 2: INSERT sys_user_role rows via UNNEST (delegated to role_repo)
INSERT INTO sys_user_role (user_id, role_id)
SELECT $1, unnest($2::varchar[])
ON CONFLICT DO NOTHING;

-- Step 3: INSERT sys_user_tenant default binding
INSERT INTO sys_user_tenant (
  id, user_id, tenant_id, is_default, is_admin, status
) VALUES (gen_random_uuid(), $1, $2, true, false, '0')
ON CONFLICT (user_id, tenant_id) DO NOTHING;
```

All three in one transaction. Rollback on any failure.

### Update user + replace role bindings (transaction)

```sql
-- Step 1: UPDATE sys_user with EXISTS tenant guard
UPDATE sys_user
   SET nick_name = $1, email = $2, phonenumber = $3, sex = $4,
       dept_id = $5, remark = $6, update_by = $7, update_at = CURRENT_TIMESTAMP
 WHERE user_id = $8
   AND del_flag = '0'
   AND ($9::varchar IS NULL OR EXISTS (
         SELECT 1 FROM sys_user_tenant
          WHERE user_id = sys_user.user_id
            AND tenant_id = $9
            AND status = '0'
       ));

-- Step 2: DELETE old sys_user_role rows (delegated to role_repo)
DELETE FROM sys_user_role WHERE user_id = $1;

-- Step 3: Bulk-insert new role_ids via UNNEST
INSERT INTO sys_user_role (user_id, role_id)
SELECT $1, unnest($2::varchar[])
ON CONFLICT DO NOTHING;
```

### Reset password (single UPDATE, tenant + admin guards)

```sql
-- Service layer checks admin guard first (not in SQL)
UPDATE sys_user
   SET password = $1, update_by = $2, update_at = CURRENT_TIMESTAMP
 WHERE user_id = $3
   AND del_flag = '0'
   AND ($4::varchar IS NULL OR EXISTS (
         SELECT 1 FROM sys_user_tenant
          WHERE user_id = sys_user.user_id
            AND tenant_id = $4
            AND status = '0'
       ))
```

`affected_rows = 0` → `DATA_NOT_FOUND` (not OPERATION_NOT_ALLOWED — that's only for the admin guard, which is checked before the UPDATE).

**Session invalidation on reset-pwd**: NestJS calls `tokenBlacklistService.invalidateAllUserTokens`. Rust equivalent: clear Redis session keys for the target user. Phase 0's session layer exposes `session::invalidate_user_sessions(redis, user_id)` — call it after successful UPDATE, outside the transaction.

---

## Error Handling

All service errors propagate through `framework::error::AppError`. Specific mappings:

| Condition | Error variant | Response code | Wire shape |
|---|---|---|---|
| user not found | `Business(DATA_NOT_FOUND)` | 1001 | HTTP 200 + `{code:1001}` |
| user_name already exists (tenant-scoped) | `Business(DUPLICATE_KEY)` | 1002 | 200 + `{code:1002}` |
| trying to edit a user from another tenant | `Business(DATA_NOT_FOUND)` | 1001 | info hiding — same as role module |
| role_id doesn't exist or isn't in tenant | `Business(DATA_NOT_FOUND)` | 1001 | validated pre-transaction |
| **self-op blocked** (delete self, disable self, change own roles) | `Business(OPERATION_NOT_ALLOWED)` | **1004 (new)** | 200 + `{code:1004, msg:'不能对自己执行该操作'}` |
| **admin target blocked** (delete admin, reset admin pwd, etc.) | `Business(OPERATION_NOT_ALLOWED)` | 1004 | 200 + `{code:1004, msg:'不能对超级管理员执行该操作'}` |
| password hashing failure | `Internal(anyhow::Error)` | 500 | logged |
| DB error | `Internal` | 500 | logged |
| DTO validation failure | `Validation{errors}` | 400 | per-field via `ValidatedJson` |
| missing permission | `Forbidden(FORBIDDEN)` | 403 | route-layer |

### New code: `OPERATION_NOT_ALLOWED = 1004`

Added to `framework/src/response/codes.rs` in the "通用业务错误" segment right after `DUPLICATE_KEY` and `OPTIMISTIC_LOCK_CONFLICT`:

```rust
pub const OPERATION_NOT_ALLOWED: Self = Self(1004);
```

One-line i18n entry (Chinese default): `"1004": "操作不被允许"`. Callers provide context-specific messages via the `Business` variant's `msg` override.

---

## Transaction Boundaries

| Operation | Tx scope | Rollback trigger |
|---|---|---|
| `create` | INSERT sys_user + bulk INSERT sys_user_role + INSERT sys_user_tenant | any sqlx error inside the closure |
| `update` | UPDATE sys_user + DELETE sys_user_role + bulk INSERT sys_user_role | same |
| `change_status` | single UPDATE sys_user.status | n/a |
| `reset_pwd` | single UPDATE sys_user.password + post-tx session invalidation (best-effort) | session invalidation failure is logged but does NOT roll back the DB update |
| `remove` (soft delete) | single UPDATE sys_user.del_flag | n/a |
| `update_auth_role` | DELETE sys_user_role + bulk INSERT sys_user_role (delegated to `role_repo::replace_user_roles`) | same |

Reads (`list`, `find_by_id`, `option_select`, `info`, `find_auth_role`) use the pool directly.

---

## Access Control

Route-level layers in `system/user/handler.rs` `router()`:

| Endpoint | Layer |
|---|---|
| `GET /system/user/list` | `require_permission!("system:user:list")` |
| `GET /system/user/:id` | `require_permission!("system:user:query")` |
| `GET /system/user/option-select` | `require_authenticated!()` — **new macro** (see below) |
| `GET /system/user/info` | `require_authenticated!()` |
| `POST /system/user/` | `require_permission!("system:user:add")` |
| `PUT /system/user/` | `require_permission!("system:user:edit")` |
| `PUT /system/user/change-status` | `require_role!("TENANT_ADMIN")` |
| `DELETE /system/user/:id` | `require_role!("TENANT_ADMIN")` |
| `PUT /system/user/reset-pwd` | `require_role!("TENANT_ADMIN")` |
| `GET /system/user/auth-role/:id` | `require_role!("TENANT_ADMIN")` |
| `PUT /system/user/auth-role` | `require_role!("TENANT_ADMIN")` |

### New framework addition: `require_authenticated!` macro

Role module left one route (`GET /system/role/option-select`) using the raw `from_fn_with_state(access::require(AccessSpec::authenticated()), access::enforce)` form as a one-off. Sub-Phase 2a has two such routes (`option-select` and `info`) — the second occurrence triggers the promote-to-macro rule. Add a fourth macro to `access_macros.rs`:

```rust
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

And retroactively apply it to role's `option-select` route for consistency (single-line edit).

---

## Testing Strategy

### Unit tests (colocated, `#[cfg(test)] mod tests` blocks)

- **DTO validation** (10-12 tests): each DTO's `reject_empty_*`, `accept_valid`, + targeted rules (email format, phone length, password strength floor, status enum `{"0","1"}`, sex enum `{"0","1","2"}`)
- **Service guards** (6-8 tests): helper functions for self-guard and admin detection, tested via fixture `is_super_admin_user` returning true/false
- **fmt_ts promotion** (3 tests, moved from role): timezone correctness + midnight + year boundary

### Integration tests (`crates/modules/tests/user_module_tests.rs`)

Mirror the role module pattern exactly:
- Real `saas_tea` dev DB connection via the existing `common::build_state_and_router` + `common::as_super_admin` harness
- Each test uses a `it-user-{test_name}-` prefix for its `user_name` + cleanup helper that DELETEs `sys_user_tenant`, `sys_user_role`, `sys_user` rows by prefix
- Target: ~22 tests covering happy path + error path for each of the 11 endpoints
- Run with `--test-threads=1` to avoid DB contention (same reason as role module)

### Manual smoke tests

`scripts/smoke-user-module.sh` — `bash` script with `set -euo pipefail` and cleanup trap, following `smoke-role-module.sh`'s shape:

1. Login as admin
2. POST create user `it-smoke-<ts>` with 1 role
3. GET list (filter by userName), verify new user visible
4. GET by id, verify role_ids correct
5. PUT update user (change nick_name + replace roles)
6. GET by id, verify update applied
7. PUT change-status disable
8. GET by id, verify status='1'
9. PUT change-status enable
10. PUT reset-pwd
11. GET auth-role, verify role list
12. PUT auth-role to empty list, re-GET, verify empty
13. Try DELETE admin user → expect 1004
14. DELETE the smoke user → verify soft-deleted
15. GET after delete → expect 1001
16. Cleanup

Plus a Week 3 manual browser test against Vue web (pointing `VITE_API_URL=http://localhost:18080`).

---

## Verification — Week-by-Week Gates

### Week 1 gate

- [ ] `cargo build -p modules` succeeds
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] DTO unit tests passing (10-12)
- [ ] Manual curl: login → `GET /system/user/list?pageNum=1&pageSize=10` returns paginated users
- [ ] Manual curl: `GET /system/user/:id` returns a user with role_ids
- [ ] Manual curl: `GET /system/user/option-select?userName=admin` returns filtered list
- [ ] `/system/user/info` returns the current admin user's profile

### Week 2 gate

- [ ] All Week 1 checks still pass
- [ ] Manual curl: `POST /system/user/` creates a user with 1 role binding; DB check shows `sys_user` + `sys_user_role` + `sys_user_tenant` rows
- [ ] Manual curl: `PUT /system/user/` updates and replaces role bindings (0 → 2 roles)
- [ ] Manual curl: `PUT /system/user/change-status` flips `sys_user.status`; DB check confirms
- [ ] Manual curl: `PUT /system/user/change-status` on admin user returns 1004
- [ ] Manual curl: `DELETE /system/user/:id` sets `del_flag='1'`; subsequent find returns 1001
- [ ] Manual curl: `DELETE /system/user/:id` on admin user returns 1004
- [ ] Manual curl: `DELETE /system/user/:id` on self returns 1004

### Week 3 gate (sub-phase exit)

- [ ] All Week 1+2 checks still pass
- [ ] Manual curl: `PUT /system/user/reset-pwd` succeeds and the target user can log in with the new password
- [ ] Manual curl: `PUT /system/user/reset-pwd` on admin user returns 1004
- [ ] Manual curl: `GET /system/user/auth-role/:id` returns current role list
- [ ] Manual curl: `PUT /system/user/auth-role` replaces role list; re-GET confirms
- [ ] Manual curl: `PUT /system/user/auth-role` on admin user returns 1004
- [ ] Integration tests: ~22 passing (real DB, `--test-threads=1`)
- [ ] `scripts/smoke-user-module.sh` — 16/16 steps pass
- [ ] `cargo test --workspace` — all previous tests + new unit + new integration pass (target: **~148 passing** = 110 baseline + ~16 unit + ~22 integration)
- [ ] Vue web user management page works zero-change against Rust backend
- [ ] Phase 0 regression smoke: login/info/logout still green
- [ ] Role module regression: `scripts/smoke-role-module.sh` still 14/14

---

## Risks & Known Limitations

1. **Password strength rule is relaxed vs NestJS.** NestJS requires upper+lower+digit+symbol. Sub-Phase 2a uses a weaker "length 6-20, 1 letter + 1 digit" until a documented policy lands. Documented as a Phase 2 security-hardening item.

2. **`dept_id` is stored but not validated.** Users can be created with a dept_id pointing to a non-existent dept. Dept module doesn't exist yet. Same pattern as role module's unvalidated menu_ids. Phase 2 tightens.

3. **Role_id validation cross-tenant.** When creating/updating a user with role_ids, the service must verify each role_id exists in `sys_role` and belongs to the current tenant (or is tenant-less). Sub-Phase 2a will implement this as a pre-transaction check via `RoleRepo::verify_role_ids_in_tenant`. **This method doesn't exist yet and needs to be added to role_repo** — minor scope creep that the spec accepts because role_repo is the natural home.

4. **`sys_user_tenant` write ownership is temporary.** Until the tenant module starts, `user_repo` is the writer. The migration is mechanical but must happen before tenant module's first write endpoint ships.

5. **Self-guard based on RequestContext user_id.** If RequestContext is somehow unpopulated (bug in middleware, test path that skips JWT enforcement), self-guard is bypassed. The integration test harness DOES populate RequestContext via `as_super_admin`, so tests will exercise the guard. Flag as a test-hygiene issue, not a prod risk.

6. **No optimistic concurrency.** Two admins editing the same user simultaneously → last-write-wins. Consistent with role module and NestJS.

7. **`platform_id` is hardcoded `'000000'` on create.** Multi-platform deployments don't exist yet. Defer to whenever Phase 2 introduces the second platform.

8. **Password plaintext in transit.** No request signing / payload encryption. HTTPS is relied on for transport security. Same as Phase 0 login.

9. **`login_ip` / `login_date` not updated by user module.** Those fields are written by the auth login flow (Phase 0). This sub-phase doesn't touch them on create (they'll be set on first login) or update.

10. **Integration tests depend on live DB.** Same CI concern as role module; spec doesn't solve CI, plan phase addresses test isolation via `--test-threads=1` and per-test prefixes.

---

## Out of Scope (explicit non-goals)

- Personal profile endpoints (Sub-Phase 2b)
- Avatar upload (multipart infrastructure — file-upload sub-phase)
- Password self-change (Sub-Phase 2b)
- Batch create/delete (Sub-Phase 2b)
- CSV/xlsx export (observability sub-phase)
- `dept-tree` and `role-post` helper queries
- `list/dept/:deptId` per-department user list
- Login failure counter / account lockout (already in Phase 0's auth module)
- Operation log (`sys_oper_log`) — observability sub-phase
- User activity audit trail — same
- Email / phone verification flows
- 2FA / WebAuthn
- Session enumeration / revoke-by-device
- NestJS co-existence beyond what Phase 0 already provides

---

## Follow-Ups (after this sub-phase)

In order:

1. **Sub-Phase 2b (user self-service)** — profile GET/PUT, avatar upload (blocks on multipart framework), self-change password, batch create/delete
2. **Menu module** — unlocks `dept-tree` frontend routes and real permission trees
3. **Dept module** — unlocks `dept_id` validation, `dept-tree`, and user-by-dept queries
4. **Post module** — unlocks post assignments + the deferred `role-post` helper endpoint
5. **Tenant module** — takes ownership of `sys_user_tenant` from user_repo; enables multi-tenant user flows
6. **Observability sub-phase** — operlog writer, xlsx export streaming, metrics-heavy instrumentation
7. **Password policy hardening** — enforce NestJS's full strength rule once policy is documented

---

## Primitives this sub-phase adds to framework

Small additions only (Batch 5.5-style promotion on evidence of second caller):

1. `ResponseCode::OPERATION_NOT_ALLOWED = 1004` (new constant, ~1 line)
2. `require_authenticated!` macro (~8 lines) + retroactive use in role's `option-select` route
3. `fmt_ts` promotion from `role/dto.rs` to `framework::response` module (~30 LOC move + export)

No new crate dependencies. No new abstractions beyond these three small items.

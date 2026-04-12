# Phase 1 Sub-Phase 1 — Role Module

**Status:** Approved (brainstorm 2026-04-10)
**Supersedes:** n/a — first spec in Phase 1
**Predecessor:** [/Users/jason/.claude/plans/spicy-doodling-bentley.md](../../../.claude/plans/spicy-doodling-bentley.md) (Phase 0)
**Predicted duration:** 3 weeks
**Predicted LOC delta:** ~1200 new + ~40 modified

---

## Context

Phase 0 delivered the Rust framework scaffold (config, context, middleware, JWT, Redis session, response envelope, error mapping, i18n, telemetry) and a minimum viable `/auth/login → /info → /auth/logout` link against the real `saas_tea` database. End-to-end smoke tests prove the Rust service is functionally substitutable for NestJS on that narrow slice: admin user logs in with `admin / admin123`, receives JWT, fetches profile, logs out. bcrypt hashes written by NestJS verify correctly in Rust. Redis session keys coexist under the shared `saas_tea:` prefix.

Phase 1 is the core RBAC system modules (user / role / menu / dept / post / dict / config / tenant / tenant-package). The full surface is ~260 NestJS endpoints and 8-11 weeks of work; too large for a single spec. We decompose Phase 1 into **vertical slices**, one module at a time, and use **this spec (role module)** as the first slice because:

1. **Role exercises every cross-cutting concern** — multi-table transaction (role + role_menu), complex joins (allocated/unallocated user lookup), pagination with tenant scope, soft delete, audit field auto-injection.
2. **Role has few upstream dependencies** — only needs `sys_menu` and `sys_user_role` rows that already exist in the NestJS-managed schema, no other Rust module.
3. **Role validates the data-layer abstraction choice before it commits the whole project** — if the hand-written SQL pattern is wrong, we find out at 3 weeks in, not 3 months.

Success for this sub-phase means the Vue web frontend's role management page can switch from NestJS to Rust (`VITE_API_URL=http://localhost:18080`) and complete every flow — list, create, edit, assign users, delete — without changing any frontend code.

---

## Scope

### In — 11 endpoints (target for 3 weeks)

| # | Method + Path | Permission | Complexity | Week |
| --- | --- | --- | --- | --- |
| 1 | `POST /api/v1/system/role/` | `system:role:add` | Multi-table tx (role + role_menu) | 1 |
| 2 | `GET /api/v1/system/role/list` | `system:role:list` | Pagination + filters + tenant scope | 1 |
| 3 | `GET /api/v1/system/role/:id` | `system:role:query` | Simple read | 1 |
| 4 | `PUT /api/v1/system/role/` | `system:role:edit` | UPDATE + role_menu diff (delete-all + insert-all) | 2 |
| 5 | `PUT /api/v1/system/role/change-status` | `system:role:change-status` | Single UPDATE | 2 |
| 6 | `DELETE /api/v1/system/role/:id` | `system:role:remove` | Soft delete | 2 |
| 7 | `GET /api/v1/system/role/option-select` | _public_ | Non-paginated list | 2 |
| 8 | `GET /api/v1/system/role/auth-user/allocated-list` | `system:role:allocated-list` | Multi-table JOIN pagination | 2 |
| 9 | `GET /api/v1/system/role/auth-user/unallocated-list` | `system:role:unallocated-list` | Reverse JOIN pagination | 2 |
| 10 | `PUT /api/v1/system/role/auth-user/select-all` | `system:role:select-auth-all` | Batch INSERT sys_user_role | 3 |
| 11 | `PUT /api/v1/system/role/auth-user/cancel` | `system:role:cancel-auth` | Batch DELETE (params-based) | 3 |

### Out — deferred to later sub-phases

- `PUT /system/role/data-scope` and `GET /system/role/dept-tree/:id` — depend on the dept module, defer until dept slice.
- `POST /system/role/export` — CSV streaming is an independent work item, not part of core RBAC.
- `@Cacheable` / `@CacheEvict` analogs — no caching layer in this sub-phase. Role list is cheap enough; caching introduces eviction bugs that are worth avoiding until we have a second module to validate the cache helper shape against.
- `@Operlog` operation log decorator — the operlog table and its background writer are part of the observability sub-phase, not this one.
- `role_dept` binding — only used for `data-scope`, deferred with it.

---

## Technical Approach

### Route C: Explicit SQL + small helpers

Decided during brainstorming (2026-04-10). Summary of the decision:

- **No ORM trait layer, no proc macros.** Each repo is a plain `struct RoleRepo;` with static methods taking `&PgPool` or `&mut Transaction<'_, Postgres>` as the first argument.
- **SQL is written out literally** in each method, including `WHERE tenant_id = $n AND del_flag = '0'`. Tenant filter is a bound parameter via `($n::varchar IS NULL OR tenant_id = $n)` so `run_ignoring_tenant()` and super-tenant cases degrade gracefully.
- **Audit fields are set by calling `AuditInsert::now()` or `audit_update_by()` explicitly** in each write method. No hidden injection.
- **Transactions are values passed as function parameters** (`&mut Transaction<'_, Postgres>`). Callers construct the transaction via `pool.begin().await?` and pass it through the call chain. No CLS-based propagation.
- **Helpers are functions, not traits.** Three to start (`AuditInsert::now`, `audit_update_by`, `current_tenant_scope`); grow only when a duplicate pattern appears ≥3 times.

The full rationale for choosing C over a deeper abstraction is preserved in the brainstorming transcript; the short version is: Rust does not have a Prisma `$extends` analog on top of sqlx, and building one would be a 2-3 week R&D project that costs compile time, debuggability, and flexibility for savings that only kick in at module 8+.

### SQL execution style

- **`sqlx::query_as::<_, T>(sql).bind(...)` runtime mode** — no `query_as!` compile-time macro, no `.sqlx/` metadata. Consistent with Phase 0. Keeps `cargo check` fast and lets us iterate on SQL without regenerating offline metadata. Upgrade to the macro happens later when query counts plateau.
- **Consistent column lists** — each repo defines a private `const COLUMNS: &str = "role_id, tenant_id, ..."` to avoid drift between `find_by_id` and `find_page` SELECT clauses. One place to add a new column.
- **One `SysRole` `FromRow` entity**. No read model / projection variants in this sub-phase.

### Transaction pattern

```rust
// Pattern: callers open the tx, pass it down, commit once at the end.
let mut tx = state.pg.begin().await?;
let role = RoleRepo::create(&mut tx, dto).await?;
RoleRepo::replace_menus(&mut tx, &role.role_id, &dto.menu_ids).await?;
tx.commit().await?;
```

Rationale over `@Transactional`-style magic:
- Transaction lifetime is **in the type system** (`&mut Transaction<'_, Postgres>`), so forgetting `commit()` is a borrow-checker visible oversight on a value held across an `.await`.
- No CLS / task_local required — the Phase 0 framework still doesn't need to stash pg pools in context.
- The service layer is where `begin` lives; the handler hands the pool to the service, the service opens and commits.

---

## Data Model (6 tables touched, 0 new)

All tables already exist under NestJS-Prisma ownership. Rust only reads/writes; schema changes remain forbidden until Phase 2.

| Table | Access | Why |
| --- | --- | --- |
| `sys_role` | RW | role entity |
| `sys_role_menu` | RW | role ↔ menu binding (join table, no audit fields) |
| `sys_user_role` | RW | role ↔ user binding (join table, no audit fields) |
| `sys_menu` | R | validated when binding — the menu_id must exist + be active |
| `sys_user` | R | JOIN target for `auth-user/allocated-list` |
| `sys_user_tenant` | R | JOIN target for `auth-user/allocated-list` (filter users by current tenant binding) |

See [../../docs/phase0-schema-reference.sql](../phase0-schema-reference.sql) for the full DDL reference.

---

## Component Breakdown

### New files

```text
server-rs/crates/modules/src/
├── domain/
│   ├── common.rs             (new, ~40 LOC)    — AuditInsert / audit_update_by / current_tenant_scope
│   ├── entities.rs           (modified, +25 LOC) — add SysRole struct
│   └── role_repo.rs          (new, ~300 LOC)   — RoleRepo with 11 methods
└── system/
    └── role/                 (new package)
        ├── mod.rs            (new, ~5 LOC)     — re-exports + router()
        ├── dto.rs            (new, ~150 LOC)   — request + response DTOs with validator rules
        ├── service.rs        (new, ~250 LOC)   — orchestrates repo calls + transactions
        └── handler.rs        (new, ~250 LOC)   — axum handlers + route-level access_spec layers
```

### Modified files

```text
server-rs/crates/modules/src/lib.rs             — re-export system::role, extend router()
server-rs/crates/modules/src/domain/mod.rs      — expose common + role_repo
server-rs/crates/app/src/main.rs                — wire system::role::router() into /api/v1 mount point
```

### Responsibility boundaries

- **`domain::common`** — pure helpers, no state. Pulls from `RequestContext` only.
- **`domain::entities::SysRole`** — `FromRow` struct. Zero logic. Mirrors `sys_role` columns.
- **`domain::role_repo::RoleRepo`** — static methods. Each method is one SQL statement OR one transaction (for the few multi-statement operations). Returns domain structs or `anyhow::Error`.
- **`system::role::dto`** — wire-level request and response shapes. `validator::Validate` for input validation. `serde` camelCase to match NestJS wire.
- **`system::role::service`** — business rules. Input validation beyond what `validator` can express (e.g. "role_key must be unique per tenant"). Opens transactions. Maps domain → DTO. Returns `Result<T, AppError>`.
- **`system::role::handler`** — thin axum handler wrappers around service calls. Wires `ValidatedJson`, `State<AppState>`, `Extension<JwtClaims>`. Applies `access::require(AccessSpec::permission(...))` per route. Returns `ApiResponse<T>`.

### DAO conventions — no type-level isolation, four disciplines

Repos are plain struct namespaces with static methods. There is **no type-level
isolation** between them: `RoleRepo` can `SELECT` from `sys_user`, entity structs
are shared via `domain::entities`, and any repo can import any helper. The
discipline that keeps this from turning into spaghetti is enforced by
convention and code review, not the type system. Four rules:

1. **Each repo method = one SQL statement, or one tightly-coupled
   transaction of related statements.** No branching business logic, no
   orchestration across multiple logical operations. If a method needs to
   "validate X, then insert Y, then audit Z," that's a service method.
2. **Repo methods never call other repos.** Cross-repo orchestration lives
   in the service layer. A repo's only dependency is `PgPool` /
   `Transaction` / shared helpers. This keeps the repo-layer dependency
   graph flat (no cycles possible) and mock-free unit testing trivial.
3. **Cross-table `JOIN`s are allowed in any repo** — put them where the
   caller's mental model lives. `find_allocated_users_page` joins
   `sys_role_menu` / `sys_user_role` / `sys_user` / `sys_user_tenant`, and
   belongs in `RoleRepo` because callers think "give me this role's
   users," not "give me users with this role." The JOIN reads foreign
   tables but does not write them.
4. **Writes (`INSERT`/`UPDATE`/`DELETE`) are single-owner.** Only
   `role_repo.rs` writes to `sys_role`; only `user_repo.rs` writes to
   `sys_user`. This concentrates the risk surface for audit-field
   injection, soft delete, tenant scoping, and future optimistic-lock
   columns. `sys_role_menu` and `sys_user_role` are role-owned join
   tables whose writes live in `role_repo.rs`.

### Entity sharing vs. projection structs

- **Full-row entity structs (`SysUser`, `SysRole`, `SysMenu`)** live in
  `domain::entities` and are imported by any repo that needs them. These
  track the real table shape; they change rarely.
- **Projection structs for specific queries** (e.g. `AllocatedUserRow`
  with only the 7 columns the allocated-list page needs) are defined
  **inside the repo file that issues the query** (`role_repo.rs`). They
  are local types, not re-exported outside the module. If two repos need
  the same projection, that's a signal to either share via
  `domain::entities` or accept a small duplication — prefer the former
  when the projection is truly identical.

### Naming conventions

- DTO classes: `CreateRoleDto` / `UpdateRoleDto` / `ListRoleDto` / `RoleResponseDto` / `RoleDetailResponseDto` / `AllocatedUserResponseDto` — mirrors NestJS DTO class names byte-compatible where possible to simplify future generated client SDKs.
- Service functions: `create` / `list` / `find_by_id` / `update` / `remove` / `change_status` / `option_select` / `allocated_users` / `unallocated_users` / `assign_users` / `unassign_users`.
- Repo functions: `insert` / `find_by_id` / `find_page` / `update` / `replace_menus` / `soft_delete_by_id` / `find_option_list` / `find_allocated_users_page` / `find_unallocated_users_page` / `insert_user_roles` / `delete_user_roles`.

---

## Key SQL Sketches

These are representative SQL shapes, not final code. The plan phase will lock exact column lists and parameter orders.

### Tenant-scoped find_by_id

```sql
SELECT {{COLUMNS}}
  FROM sys_role
 WHERE role_id = $1
   AND del_flag = '0'
   AND ($2::varchar IS NULL OR tenant_id = $2)
 LIMIT 1
```

### Paginated find_page with filters

```sql
SELECT {{COLUMNS}}
  FROM sys_role
 WHERE del_flag = '0'
   AND ($1::varchar IS NULL OR tenant_id = $1)
   AND ($2::varchar IS NULL OR role_name LIKE '%' || $2 || '%')
   AND ($3::varchar IS NULL OR role_key LIKE '%' || $3 || '%')
   AND ($4::varchar IS NULL OR status = $4)
 ORDER BY role_sort ASC, create_at DESC
 LIMIT $5 OFFSET $6
```

Companion `SELECT COUNT(*)` with the same filter clause for pagination total.

### Create role + bind menus (transaction)

```sql
-- Step 1: INSERT role
INSERT INTO sys_role (
  role_id, tenant_id, role_name, role_key, role_sort,
  data_scope, menu_check_strictly, dept_check_strictly,
  status, del_flag, create_by, update_by
) VALUES ($1, $2, $3, $4, $5, '1', false, false, '0', '0', $6, $7)
RETURNING {{COLUMNS}};

-- Step 2: INSERT role_menu rows in a loop (or UNNEST-based bulk insert)
INSERT INTO sys_role_menu (role_id, menu_id)
SELECT $1, unnest($2::varchar[])
```

### Update role + replace menu bindings (transaction)

```sql
-- Step 1: UPDATE role
UPDATE sys_role
   SET role_name = $1, role_key = $2, role_sort = $3, status = $4,
       remark = $5, update_by = $6, update_at = NOW()
 WHERE role_id = $7
   AND del_flag = '0'
   AND ($8::varchar IS NULL OR tenant_id = $8);

-- Step 2: DELETE old bindings
DELETE FROM sys_role_menu WHERE role_id = $1;

-- Step 3: INSERT new bindings (same UNNEST bulk pattern)
INSERT INTO sys_role_menu (role_id, menu_id)
SELECT $1, unnest($2::varchar[])
```

### Allocated users list (role ← user JOIN, tenant-scoped)

```sql
SELECT u.user_id, u.user_name, u.nick_name, u.email, u.phonenumber, u.status,
       u.create_at
  FROM sys_user u
  JOIN sys_user_role ur ON ur.user_id = u.user_id
  JOIN sys_user_tenant ut ON ut.user_id = u.user_id
 WHERE ur.role_id = $1
   AND ut.tenant_id = $2  -- current tenant from RequestContext
   AND u.del_flag = '0'
   AND ut.status = '0'
   AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%')
 ORDER BY u.create_at DESC
 LIMIT $4 OFFSET $5
```

This query lives in `role_repo.rs` even though it SELECTs from `sys_user`
— see the DAO conventions section above (rule 3: "cross-table JOINs are
allowed in any repo; put them where the caller's mental model lives").
The return type is a local projection struct `AllocatedUserRow` defined
in the same file, not the full `SysUser` entity.

### Batch insert user-role bindings (using UNNEST)

```sql
INSERT INTO sys_user_role (user_id, role_id)
SELECT unnest($1::varchar[]), $2
ON CONFLICT (user_id, role_id) DO NOTHING
```

### Batch delete user-role bindings

```sql
DELETE FROM sys_user_role
 WHERE role_id = $1
   AND user_id = ANY($2::varchar[])
```

---

## Error Handling

All service errors propagate through `framework::error::AppError` (Phase 0). Specific mappings for this sub-phase:

| Condition | Error variant | Response code | Wire shape |
| --- | --- | --- | --- |
| role not found | `Business(DATA_NOT_FOUND)` | 1001 | HTTP 200 + `{code:1001}` |
| `role_key` already exists in tenant | `Business(DUPLICATE_KEY)` | 1002 | HTTP 200 + `{code:1002}` |
| trying to edit a role from another tenant | `Business(DATA_NOT_FOUND)` | 1001 | Not `FORBIDDEN` — behave as if the row doesn't exist to avoid information disclosure |
| menu_id doesn't exist or is deleted | `Business(DATA_NOT_FOUND)` | 1001 | validated pre-transaction |
| user in `select-all` doesn't exist in current tenant | `Business(DATA_NOT_FOUND)` | 1001 | validated pre-transaction |
| DB connection / driver error | `Internal(anyhow::Error)` | 500 | HTTP 500, stack logged |
| DTO validation failure | `Validation{errors}` | 400 | HTTP 400 with per-field messages (via `ValidatedJson`) |
| missing permission | `Forbidden(FORBIDDEN)` | 403 | handled by `access::enforce` layer before service runs |

No new `ResponseCode` constants are introduced — reuses Phase 0 codes. New i18n keys (if any for role-specific errors) are deferred; Phase 0 i18n defaults suffice.

---

## Transaction Boundaries

| Operation | Tx scope | Rollback trigger |
| --- | --- | --- |
| `create` | `INSERT sys_role` + `INSERT sys_role_menu` bulk | any sqlx error inside the closure; auto via `Transaction` drop without `commit` |
| `update` | `UPDATE sys_role` + `DELETE sys_role_menu` + `INSERT sys_role_menu` bulk | same |
| `remove` | single `UPDATE sys_role SET del_flag='1'` | n/a — single statement, no explicit tx |
| `change_status` | single `UPDATE sys_role SET status=$` | n/a |
| `assign_users` | single batch `INSERT sys_user_role` with ON CONFLICT | n/a |
| `unassign_users` | single batch `DELETE sys_user_role` | n/a |

Read operations (`list`, `find_by_id`, `allocated_users`, `unallocated_users`, `option_select`) do not open transactions; they use the pool directly via `&PgPool`.

---

## Access Control

Route-level `access::require(AccessSpec::permission("..."))` layers, applied in `system/role/handler.rs` `router()` function. Permissions match the NestJS strings exactly (per Gate 0 verification):

| Endpoint | Permission |
| --- | --- |
| `POST /system/role/` | `system:role:add` |
| `GET /system/role/list` | `system:role:list` |
| `GET /system/role/:id` | `system:role:query` |
| `PUT /system/role/` | `system:role:edit` |
| `PUT /system/role/change-status` | `system:role:change-status` |
| `DELETE /system/role/:id` | `system:role:remove` |
| `GET /system/role/option-select` | _authenticated-only_ (no permission) |
| `GET /system/role/auth-user/allocated-list` | `system:role:allocated-list` |
| `GET /system/role/auth-user/unallocated-list` | `system:role:unallocated-list` |
| `PUT /system/role/auth-user/select-all` | `system:role:select-auth-all` |
| `PUT /system/role/auth-user/cancel` | `system:role:cancel-auth` |

**Composition pattern** (illustrative, plan phase locks exact Axum 0.8 MethodRouter composition for the same-path-different-method case `POST /system/role/` + `PUT /system/role/`):

```rust
// Pseudo-code. Concrete Axum 0.8 pattern TBD in plan phase — likely one
// of: per-method route! macro, separate Router::new().route(...).merge(),
// or explicit MethodRouter::on(MethodFilter::POST/PUT, ...).
fn router() -> Router<AppState> {
    Router::new()
        .route("/system/role/", post(create).with_access("system:role:add"))
        .route("/system/role/", put(update).with_access("system:role:edit"))
        .route("/system/role/list", get(list).with_access("system:role:list"))
        // ... etc
}
```

`GET /system/role/option-select` uses `AccessSpec::authenticated()` (logged-in users only, no specific permission) to match NestJS behavior.

Admin super-tenant bypass (Phase 0's `resolve_all_menu_perms` for `is_admin=true`) already grants all these permissions, so `admin / admin123` sessions succeed on all endpoints without further work.

---

## Testing Strategy

### Unit tests (in `modules` crate, colocated with source)

- `domain::common` — 3 tests: `AuditInsert::now` returns current user id / empty when unset / behaves under `run_ignoring_tenant`.
- `dto` parsing + validation — 4-6 tests per DTO covering edge cases (empty role_name, role_sort out of range, role_key max length, menu_ids array bounds).
- `service` business rules with mocked repo — 8-12 tests (role not found, duplicate key, cross-tenant edit blocked, transaction rollback on menu binding failure).

### Integration tests (new `tests/` dir in `modules` crate)

- Real `saas_tea` dev DB connection via `sqlx::test` or a shared `#[tokio::test]` harness.
- Seed a fixture role + user in a known tenant (`0e2e-test`), run CRUD sequence end-to-end, assert final DB state.
- Cover all 11 endpoints with one happy path + one failure path each. Total ~22 integration tests.
- Cleanup via `TRUNCATE sys_role, sys_role_menu, sys_user_role WHERE tenant_id = '0e2e-test'` in `before_each`.

### Manual smoke tests (Week 3 milestone)

- `curl` script in `scripts/smoke-role-module.sh`:
  1. Login as admin → get token
  2. POST create role with 3 menu_ids → capture role_id
  3. GET list → verify role appears
  4. GET :id → verify full detail
  5. PUT update role + change menu bindings
  6. PUT change-status → disable
  7. GET option-select → verify disabled role filtered out
  8. PUT select-all → assign 2 users
  9. GET allocated-list → verify 2 users present
  10. GET unallocated-list → verify the 2 users absent
  11. PUT cancel → unassign 1 user
  12. DELETE role → soft delete
  13. GET :id → expect 1001 DATA_NOT_FOUND
  14. Cleanup
- Web frontend manual test — Week 3 final gate. Point Vue web's `VITE_API_URL` at `http://localhost:18080`, complete role management page flow end-to-end.

---

## Verification — How We Know Each Week is Done

### Week 1 gate

- [ ] `cargo build -p modules` succeeds
- [ ] `cargo clippy --all-targets -- -D warnings` zero warnings in role module
- [ ] `cargo fmt --check` passes
- [ ] `cargo test -p modules domain::common` — 3 passing
- [ ] `cargo test -p modules dto::role` — 4-6 passing
- [ ] Manual curl: login → `POST /api/v1/system/role/ {roleName, roleKey, roleSort, menuIds:[...3 ids]}` returns 200 with new role_id
- [ ] Manual curl: `GET /api/v1/system/role/list?pageNum=1&pageSize=10` returns 200 with the new role in rows
- [ ] Manual curl: `GET /api/v1/system/role/{role_id}` returns 200 with full detail
- [ ] DB inspection: `SELECT * FROM sys_role_menu WHERE role_id = ?` shows exactly 3 rows with correct menu_ids

### Week 2 gate

- [ ] All week 1 checks still pass
- [ ] `cargo test -p modules service::role` — 8-12 passing
- [ ] Manual curl: update role name + change menu bindings from 3 → 4 menus; `sys_role_menu` contains exactly 4 rows for that role after commit
- [ ] Manual curl: `change-status` flips role status, subsequent `option-select` filters it out
- [ ] Manual curl: `DELETE /api/v1/system/role/{id}`; subsequent `find_by_id` returns 1001 DATA_NOT_FOUND; DB row has `del_flag='1'`
- [ ] Manual curl: `GET /api/v1/system/role/auth-user/allocated-list?roleId={rid}` returns paginated users
- [ ] Integration tests: 22 passing

### Week 3 gate (sub-phase exit)

- [ ] All week 1+2 checks still pass
- [ ] Manual curl: `PUT select-all` with 2 userIds inserts 2 `sys_user_role` rows; idempotent on re-submission (no duplicates due to `ON CONFLICT`)
- [ ] Manual curl: `PUT cancel` with 1 userId removes exactly that row
- [ ] `scripts/smoke-role-module.sh` full sequence passes end-to-end
- [ ] Vue web role management page works against Rust backend (`VITE_API_URL=http://localhost:18080`): list, create, edit, status toggle, user assignment, delete
- [ ] `cargo test --workspace` — all Phase 0 tests (48) + Phase 1 role tests still pass
- [ ] Phase 0 Gate 6 smoke script still passes (no regression)

---

## Risks & Known Limitations

1. **`@Operlog` not captured.** Phase 0 has no operation log writer; role CRUD operations will not be audited to `sys_oper_log` during this sub-phase. Mitigation: document the gap; frontend audit page continues to show NestJS-era operations. Phase 1 observability sub-phase closes this.

2. **No caching.** Every role list / option-select request hits PG. For `sys_role` in dev this is <5ms. In prod with larger tenant counts this could matter; revisit when the cache helper layer is added in a later sub-phase.

3. **Menu package intersection skipped.** Phase 0 `resolve_all_menu_perms` already over-grants admin users. This sub-phase does not tighten it — super-admin admin login still sees all 269 perms including menus their tenant package shouldn't expose. Phase 2 adds `SysTenantPackage.menuIds` filter.

4. **Role validation does not cross-check tenant package.** A user creating a role can bind any `menu_id` whose row exists, even menus outside their tenant package range. NestJS has the same gap in some code paths; mirroring it is acceptable for Phase 1.

5. **No optimistic concurrency.** Phase 0 / Phase 1 do not implement `@OptimisticLock`. Two admins editing the same role simultaneously → last-write-wins. NestJS does have the decorator but it's not applied to role updates.

6. **`role_dept` (data scope) skipped.** Trade-off with deferring the `data-scope` endpoint. Web frontend's role detail page will show no dept scope for roles created via Rust until the dept slice lands.

7. **Integration tests depend on live DB.** The 22 integration tests require `127.0.0.1:5432/saas_tea` to be reachable. CI needs to either skip them (`#[ignore]` + `--ignored` flag) or spin up a throwaway Postgres via docker-compose. This spec does not solve CI — it's out of scope; plan phase will address.

---

## Out of Scope (explicit non-goals)

- Role history / audit trail
- Role import from CSV/Excel
- Role templates / presets
- Role inheritance / hierarchy
- Caching layer
- Dept module integration (`data-scope`, `dept-tree`)
- Menu module (the menus themselves)
- User module (users are read-only in this sub-phase)
- Tenant module (tenant data is read-only)
- NestJS co-existence beyond what Phase 0 already provides
- Migration from NestJS schema (schema is read-only, still owned by Prisma)

---

## Follow-Ups (next sub-phases after role)

In rough order of priority and dependency:

1. **User module** (Phase 1-user) — highest dependency target, 40+ methods. Can copy the role template for common patterns (find_by_id / list / create / update / soft_delete).
2. **Menu module** (Phase 1-menu) — tree assembly + frontend routing. Depends on nothing, but unlocks `data-scope` in role and real frontend routes.
3. **Dept module** (Phase 1-dept) — completes `data-scope`, `dept-tree`, and user assignment with dept context. Depends on menu.
4. **Post / Dict / Config** (Phase 1-lite) — small CRUD modules, batch sub-phase.
5. **Tenant CRUD + package + switch** (Phase 1-tenant) — largest remaining, touches session state.

After all Phase 1 sub-phases: Phase 2 adds the observability layer (operlog, audit log, metrics-heavy instrumentation), schema ownership flip (Rust starts owning migrations), tenant package filtering, caching framework.

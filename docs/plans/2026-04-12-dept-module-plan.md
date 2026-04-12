# Dept Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Dept CRUD (7 endpoints) — tenant-scoped department management with `ancestors` path array for fast hierarchy queries and exclude-self-and-descendants support.

**Architecture:** Single sub-module (`system/dept`) following the role/user/tenant/menu pattern. Key differences from menu: (1) tenant-scoped via `current_tenant_scope()` in every query, (2) `ancestors TEXT[]` path management on create/update, (3) exclude endpoint uses `NOT ($1 = ANY(ancestors))` for O(1) descendant exclusion, (4) no pagination (full flat list like menu), (5) root dept convention `parent_id = "0"`.

**Tech Stack:** Rust, axum 0.8, sqlx 0.8, validator 0.20, serde, anyhow, chrono, uuid.

**Spec:** `docs/specs/2026-04-12-dept-module-design.md`

**Baseline:** 252 tests passing.

**Git policy:** No automatic git commands.

---

## File Structure

### New files (7)

| File | Responsibility |
|------|---------------|
| `crates/modules/src/domain/dept_repo.rs` | `sys_dept` CRUD + ancestors management + exclude query |
| `crates/modules/src/system/dept/mod.rs` | Module re-export |
| `crates/modules/src/system/dept/dto.rs` | Dept request/response DTOs |
| `crates/modules/src/system/dept/service.rs` | Dept business logic (7 functions) |
| `crates/modules/src/system/dept/handler.rs` | Dept HTTP handlers + router (7 routes) |
| `crates/modules/tests/dept_module_tests.rs` | Integration tests |
| `scripts/smoke-dept-module.sh` | E2E smoke script |

### Modified files (6)

| File | Change |
|------|--------|
| `crates/modules/src/domain/entities.rs` | Add `SysDept` struct (17 columns) |
| `crates/modules/src/domain/mod.rs` | Register `dept_repo` + re-exports |
| `crates/modules/src/system/mod.rs` | Add `pub mod dept;` |
| `crates/modules/src/lib.rs` | Add `system::dept::router()` to `api_router()` |
| `crates/framework/src/response/codes.rs` | Add 7010, 7014, 7015 |
| `i18n/*.json` + `crates/framework/src/i18n/mod.rs` | Add 3 i18n entries + update coverage test |
| `crates/modules/tests/common/mod.rs` | Add `cleanup_test_depts` helper |

---

### Task 1: ResponseCode + i18n + SysDept entity

**Files:**
- Modify: `crates/framework/src/response/codes.rs`
- Modify: `i18n/zh-CN.json`, `i18n/en-US.json`
- Modify: `crates/framework/src/i18n/mod.rs`
- Modify: `crates/modules/src/domain/entities.rs`
- Modify: `crates/modules/src/domain/mod.rs`

- [ ] **Step 1: Add 3 ResponseCode constants**

In `crates/framework/src/response/codes.rs`, add after MENU_NOT_FOUND:

```rust
    pub const DEPT_NOT_FOUND: Self = Self(7010);
    pub const DEPT_PARENT_NOT_FOUND: Self = Self(7014);
    pub const DEPT_NESTING_TOO_DEEP: Self = Self(7015);
```

- [ ] **Step 2: Add i18n entries**

zh-CN.json:
```json
  "7010": "部门不存在",
  "7014": "父部门不存在",
  "7015": "部门嵌套层级超过限制",
```

en-US.json:
```json
  "7010": "Department not found",
  "7014": "Parent department not found",
  "7015": "Department nesting level exceeded",
```

- [ ] **Step 3: Update i18n coverage test**

Add to the codes array:
```rust
            ResponseCode::DEPT_NOT_FOUND,
            ResponseCode::DEPT_PARENT_NOT_FOUND,
            ResponseCode::DEPT_NESTING_TOO_DEEP,
```

- [ ] **Step 4: Add SysDept entity**

In `crates/modules/src/domain/entities.rs`:

```rust
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysDept {
    pub dept_id: String,
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub ancestors: Vec<String>,
    pub dept_name: String,
    pub order_num: i32,
    pub leader: String,
    pub phone: String,
    pub email: String,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
    pub i18n: Option<serde_json::Value>,
}
```

Update `domain/mod.rs` re-exports to include `SysDept`.

- [ ] **Step 5: Verify**: `cargo test -p framework every_response_code && cargo check --workspace`
- [ ] **Step 6: Report Task 1 complete**

---

### Task 2: DeptRepo

**Files:**
- Create: `crates/modules/src/domain/dept_repo.rs`
- Modify: `crates/modules/src/domain/mod.rs`

### Methods (8):

1. **`find_by_id(pool, dept_id)`** — tenant-scoped single fetch
```sql
SELECT {COLUMNS} FROM sys_dept
WHERE dept_id = $1 AND del_flag = '0'
  AND ($2::varchar IS NULL OR tenant_id = $2)
LIMIT 1
```

2. **`find_list(pool, filter: DeptListFilter)`** — non-paginated, tenant-scoped
```sql
SELECT {COLUMNS} FROM sys_dept
WHERE del_flag = '0'
  AND ($1::varchar IS NULL OR tenant_id = $1)
  AND ($2::varchar IS NULL OR dept_name LIKE '%' || $2 || '%')
  AND ($3::varchar IS NULL OR status = $3)
ORDER BY order_num ASC
```

3. **`find_option_list(pool)`** — active only, tenant-scoped, cap 500
```sql
SELECT {COLUMNS} FROM sys_dept
WHERE del_flag = '0' AND status = '0'
  AND ($1::varchar IS NULL OR tenant_id = $1)
ORDER BY order_num ASC LIMIT 500
```

4. **`find_excluding(pool, exclude_dept_id)`** — tenant-scoped, excludes self + descendants
```sql
SELECT {COLUMNS} FROM sys_dept
WHERE del_flag = '0'
  AND ($1::varchar IS NULL OR tenant_id = $1)
  AND dept_id != $2
  AND NOT ($2 = ANY(ancestors))
ORDER BY order_num ASC
```

5. **`find_parent_ancestors(pool, parent_id)`** — returns `Option<Vec<String>>`
```sql
SELECT ancestors FROM sys_dept
WHERE dept_id = $1 AND del_flag = '0'
  AND ($2::varchar IS NULL OR tenant_id = $2)
```
Returns `None` if parent doesn't exist (→ 7014 error in service).

6. **`insert(pool, params: DeptInsertParams)`** — INSERT RETURNING, uuid PK
7. **`update_by_id(pool, params: DeptUpdateParams)`** — UPDATE with COALESCE
8. **`soft_delete(pool, dept_id)`** — tenant-scoped soft delete

### Column constant:
```rust
const DEPT_COLUMNS: &str = "\
    dept_id, tenant_id, parent_id, ancestors, dept_name, order_num, \
    leader, phone, email, status, del_flag, create_by, create_at, \
    update_by, update_at, remark, i18n";
```

### Params structs:

```rust
#[derive(Debug)]
pub struct DeptListFilter {
    pub dept_name: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug)]
pub struct DeptInsertParams {
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub ancestors: Vec<String>,
    pub dept_name: String,
    pub order_num: i32,
    pub leader: String,
    pub phone: String,
    pub email: String,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct DeptUpdateParams {
    pub dept_id: String,
    pub parent_id: Option<String>,
    pub ancestors: Option<Vec<String>>,
    pub dept_name: Option<String>,
    pub order_num: Option<i32>,
    pub leader: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub status: Option<String>,
    pub remark: Option<String>,
}
```

Register in `domain/mod.rs`:
```rust
pub mod dept_repo;
pub use dept_repo::{DeptInsertParams, DeptListFilter, DeptRepo, DeptUpdateParams};
```

- [ ] **Step 1: Create dept_repo.rs with all 8 methods**
- [ ] **Step 2: Register in domain/mod.rs**
- [ ] **Step 3: Compile-check**: `cargo check --workspace`
- [ ] **Step 4: Report Task 2 complete**

---

### Task 3: Dept DTO + Service + Handler + Wiring

**Files:**
- Create: `crates/modules/src/system/dept/mod.rs`, `dto.rs`, `service.rs`, `handler.rs`
- Modify: `crates/modules/src/system/mod.rs`, `crates/modules/src/lib.rs`

### dto.rs

**Response**: `DeptResponseDto` — all SysDept fields except `tenant_id`, `del_flag`, `i18n`. Timestamps via `fmt_ts()`. `ancestors: Vec<String>` serialized as JSON array on wire.

**Request DTOs**:

`CreateDeptDto`:
- `parent_id: String` (REQUIRED — "0" for root)
- `dept_name: String` (length 1-30)
- `order_num: i32` (range min=0)
- `leader: Option<String>` (length max=20)
- `phone: Option<String>` (length max=11)
- `email: Option<String>` (email validator)
- `status: String` (default "0", validate_status_flag)

`UpdateDeptDto`:
- `dept_id: String` (required)
- All fields from Create (parent_id still required per NestJS inheritance)

`ListDeptDto`:
- `dept_name: Option<String>`
- `status: Option<String>`
- **No PageQuery** (non-paginated)

### service.rs — 7 functions:

| Function | Key logic |
|----------|-----------|
| `create(state, dto)` | Build ancestors (root: `["0"]`, else: `[...parent_ancestors, parent_id]`), depth check ≤ 2000, INSERT |
| `update(state, dto)` | Fetch existing → 7010, if parent_id changed recalculate ancestors, depth check |
| `find_by_id(state, id)` | → 7010 if None |
| `list(state, query)` | `find_list(filter)` → map DTOs |
| `remove(state, id)` | `soft_delete` (no guards) |
| `option_select(state)` | `find_option_list` → map DTOs |
| `exclude_list(state, id)` | `find_excluding(id)` → map DTOs |

**Ancestors building** (in create + update):
```rust
let ancestors = if dto.parent_id == "0" {
    vec!["0".to_string()]
} else {
    let parent_ancestors = DeptRepo::find_parent_ancestors(&state.pg, &dto.parent_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DEPT_PARENT_NOT_FOUND)?;
    let mut a = parent_ancestors;
    a.push(dto.parent_id.clone());
    a
};
if ancestors.len() > 2000 {
    return Err(AppError::business(ResponseCode::DEPT_NESTING_TOO_DEEP));
}
```

### handler.rs — 7 routes:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/system/dept/", post(create).route_layer(require_permission!("system:dept:add")))
        .route("/system/dept/", put(update).route_layer(require_permission!("system:dept:edit")))
        .route("/system/dept/list", get(list).route_layer(require_permission!("system:dept:list")))
        .route("/system/dept/option-select", get(option_select).route_layer(require_authenticated!()))
        .route("/system/dept/list/exclude/{id}", get(exclude_list).route_layer(require_permission!("system:dept:exclude-list")))
        .route("/system/dept/{id}", get(find_by_id).route_layer(require_permission!("system:dept:query")))
        .route("/system/dept/{id}", delete(remove).route_layer(require_permission!("system:dept:remove")))
}
```

Route ordering: literal paths (`/list`, `/option-select`, `/list/exclude/{id}`) BEFORE wildcard `/{id}`.

### Wiring:
- `system/mod.rs`: add `pub mod dept;`
- `lib.rs` `api_router()`: add `.merge(system::dept::router())`

- [ ] **Step 1: Create all 4 module files**
- [ ] **Step 2: Wire in system/mod.rs + lib.rs**
- [ ] **Step 3: Compile-check + clippy**
- [ ] **Step 4: Run existing tests (regression)**
- [ ] **Step 5: Report Task 3 complete**

---

### Task 4: Integration tests + Smoke

**Files:**
- Create: `crates/modules/tests/dept_module_tests.rs`
- Create: `scripts/smoke-dept-module.sh`
- Modify: `crates/modules/tests/common/mod.rs`

### Cleanup helper:
```rust
pub async fn cleanup_test_depts(pool: &PgPool, prefix: &str) {
    assert!(!prefix.is_empty());
    let pattern = format!("{prefix}%");
    sqlx::query("DELETE FROM sys_dept WHERE dept_name LIKE $1")
        .bind(&pattern).execute(pool).await.expect("cleanup sys_dept");
}
```

### 14 integration tests:

Each test uses unique `suffix = uuid[..8]` + suffix-scoped cleanup.

1. `create_dept_root` — parent_id="0", verify ancestors=["0"]
2. `create_dept_with_parent` — create root, then child, verify ancestors includes root's dept_id
3. `create_dept_parent_not_found` — non-existent parent_id → 7014
4. `create_dept_nesting_too_deep` — mock a dept with 2001-element ancestors, try to create child → 7015 (this test needs a direct DB INSERT to set up the deep ancestor chain)
5. `list_depts_returns_flat` — create 2, list, assert count >= 2
6. `list_depts_filters_by_name` — filter by unique name
7. `list_depts_filters_by_status` — create disabled, filter active, excluded
8. `get_dept_detail` — create, fetch, verify fields + ancestors
9. `get_dept_nonexistent` — 7010
10. `update_dept_changes_fields` — create, update dept_name, verify
11. `update_dept_reparent_recalculates_ancestors` — create root→child, reparent child to "0", verify ancestors = ["0"]
12. `delete_dept_soft_deletes` — verify del_flag='1' in DB
13. `option_select_returns_active_only` — create active + disabled, option_select excludes disabled
14. `exclude_list_excludes_self_and_descendants` — create root→child→grandchild, exclude(root), verify child + grandchild NOT in result

### Smoke script (~8 steps):

Login → create root dept → create child dept → list → detail → update → exclude list (verify child excluded when excluding root) → delete child → delete root

- [ ] **Step 1: Add cleanup helper**
- [ ] **Step 2: Create dept_module_tests.rs with 14 tests**
- [ ] **Step 3: Create smoke script**
- [ ] **Step 4: Run integration tests**
- [ ] **Step 5: Run all 5 smoke suites**
- [ ] **Step 6: Final verify**: `cargo test --workspace && cargo clippy && cargo fmt --check`
- [ ] **Step 7: Report plan complete**

Report:
- All 4 tasks complete
- Tests: 252 → ~266 (+14)
- 7 new endpoints
- 5 smoke suites green (role + user + tenant + menu + dept)
- Zero wire contract changes, zero new crate deps

---

## Post-plan status snapshot

| Metric | Baseline | Target |
|--------|----------|--------|
| Total tests | 252 | ~266 (+14) |
| Dept endpoints | 0 | 7 |
| New domain files | 0 | 1 (dept_repo) |
| New system modules | 0 | 1 (dept) |
| Smoke scripts | 4 | 5 |

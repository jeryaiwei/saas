# Menu Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Menu CRUD (9 endpoints) — backend management of `sys_menu` with tree queries, cascade delete via PostgreSQL CTE, and admin/tenant-scoped role-menu-tree views.

**Architecture:** Single sub-module (`system/menu`) following the established role/user/tenant pattern — handler → service → repo. Key difference from other modules: no pagination (full list), tree building in service layer via `list_to_tree()` helper, and CTE recursion for cascade delete. The `list_to_tree` utility is placed in service.rs since menu is the only consumer (YAGNI — extract to framework if a second module needs it).

**Tech Stack:** Rust, axum 0.8, sqlx 0.8 (runtime queries), validator 0.20, serde, serde_json (for `i18n` JSONB column), anyhow, chrono, uuid.

**Spec:** `docs/specs/2026-04-12-menu-module-design.md`

**Baseline:** 234 tests passing. Run `cd server-rs && cargo test --workspace 2>&1 | grep "test result:"` to confirm.

**Git policy:** Per standing user preference, no automatic git commands.

---

## File Structure

### New files (7)

| File | Responsibility |
|------|---------------|
| `crates/modules/src/domain/menu_repo.rs` | `sys_menu` CRUD + tree queries + CTE cascade delete |
| `crates/modules/src/system/menu/mod.rs` | Module re-export |
| `crates/modules/src/system/menu/dto.rs` | Menu request/response DTOs + `TreeNode` + `list_to_tree()` helper |
| `crates/modules/src/system/menu/service.rs` | Menu business logic (9 service functions) |
| `crates/modules/src/system/menu/handler.rs` | Menu HTTP handlers + router (9 routes) |
| `crates/modules/tests/menu_module_tests.rs` | Integration tests |
| `scripts/smoke-menu-module.sh` | End-to-end smoke script |

### Modified files (7)

| File | Change |
|------|--------|
| `crates/modules/src/domain/entities.rs` | Add `SysMenu` struct (21 columns) |
| `crates/modules/src/domain/mod.rs` | Register `menu_repo` + re-exports |
| `crates/modules/src/system/mod.rs` | Add `pub mod menu;` |
| `crates/modules/src/lib.rs` | Add `system::menu::router()` to `api_router()` |
| `crates/framework/src/response/codes.rs` | Add MENU_NOT_FOUND (7020) |
| `i18n/zh-CN.json` + `i18n/en-US.json` | Add 1 new entry |
| `crates/framework/src/i18n/mod.rs` | Update coverage test list |

---

### Task 1: ResponseCode + i18n + SysMenu entity

**Files:**
- Modify: `crates/framework/src/response/codes.rs`
- Modify: `i18n/zh-CN.json`, `i18n/en-US.json`
- Modify: `crates/framework/src/i18n/mod.rs`
- Modify: `crates/modules/src/domain/entities.rs`
- Modify: `crates/modules/src/domain/mod.rs`

- [ ] **Step 1: Add MENU_NOT_FOUND constant**

In `crates/framework/src/response/codes.rs`, add after the tenant section:

```rust
    // --- 7000-7099 system module ---
    pub const MENU_NOT_FOUND: Self = Self(7020);
```

- [ ] **Step 2: Add i18n entries**

In `i18n/zh-CN.json`, add:
```json
  "7020": "菜单不存在",
```

In `i18n/en-US.json`, add:
```json
  "7020": "Menu not found",
```

- [ ] **Step 3: Update i18n coverage test**

In `crates/framework/src/i18n/mod.rs`, find the `every_response_code_has_i18n_entries_in_all_langs` test. Add after the tenant section:

```rust
            // 7000-7099 system module
            ResponseCode::MENU_NOT_FOUND,
```

- [ ] **Step 4: Add SysMenu entity**

In `crates/modules/src/domain/entities.rs`, add at the end:

```rust
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysMenu {
    pub menu_id: String,
    pub menu_name: String,
    pub parent_id: Option<String>,
    pub order_num: i32,
    pub path: String,
    pub component: Option<String>,
    pub query: String,
    pub is_frame: String,
    pub is_cache: String,
    pub menu_type: String,
    pub visible: String,
    pub status: String,
    pub perms: String,
    pub icon: String,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
    pub del_flag: String,
    pub i18n: Option<serde_json::Value>,
}
```

Update `domain/mod.rs` re-exports:
```rust
pub use entities::{SysMenu, SysRole, SysTenant, SysTenantPackage, SysUser, SysUserTenant};
```

- [ ] **Step 5: Verify**

```bash
cd server-rs && cargo test -p framework every_response_code_has_i18n 2>&1 | tail -5
cd server-rs && cargo check --workspace 2>&1 | tail -5
```

- [ ] **Step 6: Report Task 1 complete**

---

### Task 2: MenuRepo

**Files:**
- Create: `crates/modules/src/domain/menu_repo.rs`
- Modify: `crates/modules/src/domain/mod.rs`

This repo is simpler than tenant_repo — single table, no JOINs for basic CRUD, but has two special features: CTE cascade delete and admin/tenant-scoped role-menu-tree queries.

Methods to implement:

| Method | Returns | Notes |
|--------|---------|-------|
| `find_by_id(pool, menu_id)` | `Option<SysMenu>` | WHERE menu_id=$1 AND del_flag='0' |
| `find_list(pool, filter: MenuListFilter)` | `Vec<SysMenu>` | Non-paginated, full list, 4 optional filters |
| `find_tree_nodes(pool)` | `Vec<MenuTreeRow>` | SELECT menu_id, menu_name, parent_id only |
| `find_role_menu_tree_for_admin(pool, role_id, tenant_id)` | `Vec<RoleMenuTreeRow>` | All menus + LEFT JOIN role_menu for is_checked |
| `find_role_menu_tree_for_tenant(pool, role_id, tenant_id)` | `Vec<RoleMenuTreeRow>` | Same + package filter |
| `find_package_menu_ids(pool, package_id)` | `Option<Vec<String>>` | SELECT menu_ids from sys_tenant_package |
| `insert(pool, params: MenuInsertParams)` | `SysMenu` | INSERT RETURNING |
| `update_by_id(pool, params: MenuUpdateParams)` | `u64` | UPDATE with COALESCE |
| `soft_delete(pool, menu_id)` | `u64` | Single soft delete, no guards |
| `cascade_soft_delete(pool, menu_ids: &[String])` | `u64` | Recursive CTE |

Projection structs (placed in repo file):

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MenuTreeRow {
    pub menu_id: String,
    pub menu_name: String,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoleMenuTreeRow {
    pub menu_id: String,
    pub menu_name: String,
    pub parent_id: Option<String>,
    pub is_checked: bool,
}
```

Filter struct:
```rust
#[derive(Debug)]
pub struct MenuListFilter {
    pub menu_name: Option<String>,
    pub status: Option<String>,
    pub parent_id: Option<String>,
    pub menu_type: Option<String>,
}
```

Params structs: `MenuInsertParams` (14 fields matching CreateMenuDto), `MenuUpdateParams` (menu_id + 14 Optional fields).

**Key SQL for cascade delete** (the CTE):
```sql
WITH RECURSIVE menu_tree AS (
    SELECT menu_id FROM sys_menu
     WHERE menu_id = ANY($1::varchar[]) AND del_flag = '0'
    UNION ALL
    SELECT m.menu_id FROM sys_menu m
     INNER JOIN menu_tree mt ON m.parent_id = mt.menu_id
     WHERE m.del_flag = '0'
)
UPDATE sys_menu SET del_flag = '1', update_by = $2, update_at = CURRENT_TIMESTAMP
 WHERE menu_id IN (SELECT menu_id FROM menu_tree) AND del_flag = '0'
```

**Key SQL for role-menu-tree (admin)**:
```sql
SELECT m.menu_id, m.menu_name, m.parent_id,
       (rm.menu_id IS NOT NULL) AS is_checked
  FROM sys_menu m
  LEFT JOIN (
    SELECT rm.menu_id FROM sys_role_menu rm
     INNER JOIN sys_role r ON r.role_id = rm.role_id
       AND r.tenant_id = $2 AND r.del_flag = '0'
     WHERE rm.role_id = $1
  ) rm ON m.menu_id = rm.menu_id
 WHERE m.del_flag = '0'
 ORDER BY m.parent_id ASC, m.order_num ASC
```

**Key SQL for role-menu-tree (tenant)**: Same as admin + add:
```sql
  LEFT JOIN sys_tenant t ON t.tenant_id = $2 AND t.del_flag = '0'
  LEFT JOIN sys_tenant_package p ON t.package_id = p.package_id
    AND p.del_flag = '0' AND p.status = '0'
  -- add to WHERE:
  AND (p.menu_ids IS NULL OR m.menu_id = ANY(p.menu_ids))
```

**`find_list` WHERE clause**: Build dynamically since no filter struct embeds `PageQuery`:
```sql
WHERE del_flag = '0'
  AND ($1::varchar IS NULL OR menu_name LIKE '%' || $1 || '%')
  AND ($2::varchar IS NULL OR status = $2)
  AND ($3::varchar IS NULL OR parent_id = $3)
  AND ($4::varchar IS NULL OR menu_type = $4)
ORDER BY parent_id ASC, order_num ASC, menu_id ASC
```

Register in `domain/mod.rs`:
```rust
pub mod menu_repo;
pub use menu_repo::{MenuInsertParams, MenuListFilter, MenuRepo, MenuTreeRow, MenuUpdateParams, RoleMenuTreeRow};
```

- [ ] **Step 1: Create menu_repo.rs with all methods**
- [ ] **Step 2: Register in domain/mod.rs**
- [ ] **Step 3: Compile-check**: `cargo check --workspace`
- [ ] **Step 4: Report Task 2 complete**

---

### Task 3: Menu DTO + Service + Handler + Wiring

**Files:**
- Create: `crates/modules/src/system/menu/mod.rs`
- Create: `crates/modules/src/system/menu/dto.rs`
- Create: `crates/modules/src/system/menu/service.rs`
- Create: `crates/modules/src/system/menu/handler.rs`
- Modify: `crates/modules/src/system/mod.rs`
- Modify: `crates/modules/src/lib.rs`

### dto.rs

**Response DTOs** (all `#[serde(rename_all = "camelCase")]`):

`MenuDetailResponseDto` — all SysMenu fields formatted via `fmt_ts` for timestamps, with `from_entity(SysMenu)` method. Also used for list items (NestJS returns full detail in list).

`TreeNode` — for tree-select endpoints:
```rust
#[derive(Debug, Serialize)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeNode>,
}
```

`MenuTreeSelectResponseDto` — for role-menu-tree and package-menu-tree:
```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuTreeSelectResponseDto {
    pub menus: Vec<TreeNode>,
    pub checked_keys: Vec<String>,
}
```

**Request DTOs**:

`CreateMenuDto` — 14 fields from spec §2.1 with validators.

`UpdateMenuDto` — menu_id (required) + 14 Optional fields.

`ListMenuDto` — 4 optional filter fields (menu_name, status, parent_id, menu_type). **No PageQuery** — this is a non-paginated list.

**`list_to_tree` helper** — placed in dto.rs (it's a pure data transformation, dto.rs is its natural home):

```rust
/// Convert a flat list of `{id, label, parent_id}` rows into a nested
/// tree of `TreeNode`. O(n) single-pass using HashMap for parent lookup.
pub fn list_to_tree(rows: Vec<MenuTreeRow>) -> Vec<TreeNode> {
    use std::collections::HashMap;

    let mut nodes: Vec<TreeNode> = rows
        .iter()
        .map(|r| TreeNode {
            id: r.menu_id.clone(),
            label: r.menu_name.clone(),
            children: Vec::new(),
        })
        .collect();

    // Build index: menu_id → position in nodes vec
    let index: HashMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (r.menu_id.as_str(), i))
        .collect();

    // Collect children indices grouped by parent
    let mut children_map: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut root_indices: Vec<usize> = Vec::new();

    for (i, row) in rows.iter().enumerate() {
        match row.parent_id.as_deref() {
            Some(pid) if index.contains_key(pid) => {
                children_map.entry(index[pid]).or_default().push(i);
            }
            _ => root_indices.push(i),
        }
    }

    // Build tree bottom-up (reverse to avoid borrow issues)
    // Process in reverse index order so children are attached before parents
    fn build(idx: usize, nodes: &mut Vec<TreeNode>, children_map: &HashMap<usize, Vec<usize>>) -> TreeNode {
        let mut node = TreeNode {
            id: std::mem::take(&mut nodes[idx].id),
            label: std::mem::take(&mut nodes[idx].label),
            children: Vec::new(),
        };
        if let Some(child_indices) = children_map.get(&idx) {
            for &ci in child_indices {
                node.children.push(build(ci, nodes, children_map));
            }
        }
        node
    }

    root_indices.iter().map(|&ri| build(ri, &mut nodes, &children_map)).collect()
}
```

### service.rs

9 public functions:

| Function | Key logic |
|----------|-----------|
| `create(state, dto)` | `MenuRepo::insert`, return detail DTO |
| `update(state, dto)` | Fetch existing → 7020, then `MenuRepo::update_by_id` |
| `find_by_id(state, menu_id)` | `MenuRepo::find_by_id` → 7020 if None |
| `list(state, query)` | `MenuRepo::find_list(filter)` → map to DTOs |
| `remove(state, menu_id)` | `MenuRepo::soft_delete` (no guards) |
| `cascade_remove(state, path_ids)` | Split comma-sep → `MenuRepo::cascade_soft_delete` |
| `tree_select(state)` | `MenuRepo::find_tree_nodes` → `list_to_tree()` |
| `role_menu_tree_select(state, role_id)` | Admin/tenant branch → repo → split checked_keys → `list_to_tree()` |
| `package_menu_tree_select(state, package_id)` | Parallel: `find_tree_nodes` + `find_package_menu_ids` → assemble |

For `role_menu_tree_select`, read admin flag from `RequestContext`:
```rust
let is_admin = framework::context::RequestContext::with_current(|ctx| ctx.is_admin)
    .unwrap_or(false);
let tenant_id = framework::context::current_tenant_scope()
    .unwrap_or_default();
```

### handler.rs

9 routes:
```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/system/menu/", post(create).route_layer(require_permission!("system:menu:add")))
        .route("/system/menu/", put(update).route_layer(require_permission!("system:menu:edit")))
        .route("/system/menu/list", get(list).route_layer(require_permission!("system:menu:list")))
        .route("/system/menu/tree-select", get(tree_select).route_layer(require_permission!("system:menu:tree-select")))
        .route("/system/menu/role-menu-tree-select/{roleId}", get(role_menu_tree_select).route_layer(require_permission!("system:menu:role-menu-tree")))
        .route("/system/menu/tenant-package-menu-tree-select/{packageId}", get(package_menu_tree_select).route_layer(require_authenticated!()))
        .route("/system/menu/cascade/{menuIds}", delete(cascade_remove).route_layer(require_permission!("system:menu:cascade-remove")))
        .route("/system/menu/{menuId}", get(find_by_id).route_layer(require_permission!("system:menu:query")))
        .route("/system/menu/{menuId}", delete(remove).route_layer(require_permission!("system:menu:remove")))
}
```

**IMPORTANT route ordering**: `/system/menu/list`, `/system/menu/tree-select`, `/system/menu/cascade/{menuIds}`, and other literal-prefix routes MUST be registered BEFORE the wildcard `/{menuId}` routes — otherwise axum matches the literal path segment as a `menuId` parameter.

### Wiring

`system/mod.rs`: add `pub mod menu;`
`lib.rs` `api_router()`: add `.merge(system::menu::router())`

- [ ] **Step 1: Create mod.rs**
- [ ] **Step 2: Create dto.rs with all DTOs + list_to_tree helper**
- [ ] **Step 3: Create service.rs with all 9 functions**
- [ ] **Step 4: Create handler.rs with 9 handlers + router**
- [ ] **Step 5: Wire in system/mod.rs + lib.rs**
- [ ] **Step 6: Compile-check + clippy**: `cargo check --workspace && cargo clippy --all-targets -- -D warnings`
- [ ] **Step 7: Run existing tests (regression)**: `cargo test --workspace 2>&1 | grep "test result:"`
- [ ] **Step 8: Report Task 3 complete**

---

### Task 4: Integration tests

**Files:**
- Create: `crates/modules/tests/menu_module_tests.rs`
- Modify: `crates/modules/tests/common/mod.rs` (add cleanup helper)

**Cleanup helper** in `common/mod.rs`:
```rust
pub async fn cleanup_test_menus(pool: &PgPool, prefix: &str) {
    assert!(!prefix.is_empty());
    let pattern = format!("{prefix}%");
    // Delete role-menu bindings for test menus
    sqlx::query(
        "DELETE FROM sys_role_menu WHERE menu_id IN \
         (SELECT menu_id FROM sys_menu WHERE menu_name LIKE $1)",
    )
    .bind(&pattern).execute(pool).await.expect("cleanup sys_role_menu for test menus");
    // Delete test menus
    sqlx::query("DELETE FROM sys_menu WHERE menu_name LIKE $1")
        .bind(&pattern).execute(pool).await.expect("cleanup sys_menu");
}
```

**15 tests** — each test uses a unique `suffix = uuid[..8]` and cleans up with suffix-scoped prefix (learned from tenant parallel fix):

1. `create_menu_directory` — type M, verify returned detail
2. `create_menu_page` — type C with component path
3. `create_menu_button` — type F with perms string
4. `create_child_menu` — create parent M, then child C under it
5. `list_menus_returns_flat` — create 2 menus, list, assert count >= 2
6. `list_menus_filters_by_name` — create with unique name, filter by it
7. `list_menus_filters_by_status` — create active + disabled, filter status='0'
8. `list_menus_filters_by_parent_id` — create parent + child, filter by parent_id
9. `get_menu_detail` — create, fetch by id, verify fields
10. `get_menu_nonexistent` — random UUID → 7020
11. `update_menu_changes_fields` — create, update menu_name, verify
12. `delete_menu_soft_deletes` — create, delete, verify del_flag in DB
13. `cascade_delete_removes_descendants` — create parent → child → grandchild, cascade delete parent, verify all 3 have del_flag='1'
14. `tree_select_returns_nested_tree` — create parent + child, call tree_select, find parent node, assert it has children
15. `role_menu_tree_select_returns_checked_keys` — create menu, bind to a role via sys_role_menu INSERT, call role_menu_tree_select, verify checked_keys contains the menu_id

- [ ] **Step 1: Add cleanup helper to common/mod.rs**
- [ ] **Step 2: Create menu_module_tests.rs with all 15 tests**
- [ ] **Step 3: Run tests**: `cargo test -p modules --test menu_module_tests 2>&1 | tail -30`
- [ ] **Step 4: Run full workspace**: `cargo test --workspace 2>&1 | grep "test result:"`
- [ ] **Step 5: Report Task 4 complete**

---

### Task 5: Smoke script + final verify

**Files:**
- Create: `scripts/smoke-menu-module.sh`

**10 steps** — follows same pattern as existing smoke scripts:

1. Login as admin
2. Create directory menu (type M)
3. Create child page menu (type C) under it
4. Create button menu (type F) under the page
5. List menus — assert code=200
6. Get detail of directory — assert menuName matches
7. Update directory's icon
8. Tree-select — assert code=200
9. Delete button
10. Cascade delete directory — should remove directory + child page

Cleanup via psql: `DELETE FROM sys_menu WHERE menu_name LIKE '${PREFIX}%'`

Script uses short prefix to fit varchar(50) menu_name constraint.

- [ ] **Step 1: Create smoke script**
- [ ] **Step 2: Run all 4 smoke suites**:
```bash
cd server-rs
pkill -f target/debug/app 2>/dev/null; sleep 1
cargo build -p app 2>&1 | tail -3
./target/debug/app > /tmp/tea-rs-menu-smoke.log 2>&1 &
APP_PID=$!; sleep 2
bash scripts/smoke-role-module.sh 2>&1 | tail -3
bash scripts/smoke-user-module.sh 2>&1 | tail -3
bash scripts/smoke-tenant-module.sh 2>&1 | tail -3
bash scripts/smoke-menu-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null; wait 2>/dev/null
```
Expected: role 14/14, user 16/16, tenant 13/13, menu 10/10

- [ ] **Step 3: Final verify**:
```bash
cargo test --workspace 2>&1 | grep "test result:"
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cargo fmt --check && echo fmt ok
```

- [ ] **Step 4: Report plan complete**

---

## Post-plan status snapshot

| Metric | Baseline | Target |
|--------|----------|--------|
| Total tests | 234 | ~249 (+15) |
| Menu endpoints | 0 | 9 |
| New domain files | 0 | 1 (menu_repo) |
| New system modules | 0 | 1 (menu) |
| Smoke scripts | 3 | 4 |
| Wire contract changes | — | 0 |
| New crate deps | — | 0 |

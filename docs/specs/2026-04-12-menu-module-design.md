# Menu Module Design Spec (Phase 1 Sub-Phase 4)

**Scope**: Menu CRUD (9 endpoints) — backend management of `sys_menu`
**Not in scope**: `GET /system/menu/routers`（前端路由构建，scope B，Vue web cut-over 时做）

---

## 1. Entity

### SysMenu (21 columns)

```sql
CREATE TABLE sys_menu (
  menu_id    VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  menu_name  VARCHAR(50)  NOT NULL,
  parent_id  VARCHAR(36),                       -- NULL = root menu
  order_num  INT          NOT NULL,
  path       VARCHAR(200) NOT NULL DEFAULT '',
  component  VARCHAR(255),
  query      VARCHAR(255) NOT NULL DEFAULT '',
  is_frame   CHAR(1)      NOT NULL,             -- '0'=yes '1'=no
  is_cache   CHAR(1)      NOT NULL,             -- '0'=cache '1'=no cache
  menu_type  CHAR(1)      NOT NULL,             -- 'M'=目录 'C'=菜单 'F'=按钮
  visible    CHAR(1)      NOT NULL,             -- '0'=show '1'=hide
  status     CHAR(1)      NOT NULL,             -- '0'=normal '1'=disabled
  perms      VARCHAR(100) NOT NULL DEFAULT '',   -- e.g. 'system:user:add'
  icon       VARCHAR(100) NOT NULL DEFAULT '',
  create_by  VARCHAR(64)  NOT NULL DEFAULT '',
  create_at  TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by  VARCHAR(64)  NOT NULL DEFAULT '',
  update_at  TIMESTAMPTZ(6) NOT NULL,
  remark     VARCHAR(500),
  del_flag   CHAR(1)      NOT NULL DEFAULT '0',
  i18n       JSONB                               -- Phase 1: read-only, transparent pass-through
);
```

Rust `SysMenu` struct maps all 21 columns via `sqlx::FromRow`. The `i18n` column maps to `Option<serde_json::Value>`.

**Menu types**:
- `M` = 目录 (directory) — container, no component
- `C` = 菜单 (menu) — has component path, renders a page
- `F` = 按钮 (button) — permission-only, not visible in nav

---

## 2. Endpoints (9)

### 2.1 CRUD (5)

#### POST `/system/menu/`
**Permission**: `system:menu:add`
**Request DTO** (`CreateMenuDto`):

| Field | Type | Required | Validator | Default |
|-------|------|----------|-----------|---------|
| menu_name | String | yes | length(1, 50) | - |
| parent_id | Option\<String\> | no | - | None (root) |
| order_num | i32 | yes | - | - |
| path | Option\<String\> | no | length(max=200) | "" |
| component | Option\<String\> | no | length(max=255) | None |
| query | Option\<String\> | no | length(max=255) | "" |
| is_frame | String | yes | length(1,1) | - |
| is_cache | String | yes | length(1,1) | - |
| menu_type | String | yes | length(1,1) | - |
| visible | String | yes | length(1,1) | - |
| status | String | no | validate_status_flag | "0" |
| perms | Option\<String\> | no | length(max=100) | "" |
| icon | Option\<String\> | no | length(max=100) | "" |
| remark | Option\<String\> | no | length(max=500) | None |

**Business logic**: INSERT `sys_menu`, return `MenuDetailResponseDto`.

#### PUT `/system/menu/`
**Permission**: `system:menu:edit`
**Request DTO** (`UpdateMenuDto`): `menu_id` (required) + all fields from Create as Optional.

**Business logic**:
1. Fetch existing by `menu_id` → 7020 MENU_NOT_FOUND if missing
2. UPDATE with COALESCE for optional fields
3. Return success

#### GET `/system/menu/list`
**Permission**: `system:menu:list`
**Query DTO** (`ListMenuDto`):

| Field | Type | Filter mode |
|-------|------|-------------|
| menu_name | Option\<String\> | substring LIKE |
| status | Option\<String\> | exact match |
| parent_id | Option\<String\> | exact match |
| menu_type | Option\<String\> | exact match |

**Returns**: `Vec<MenuResponseDto>` — **全量扁平列表，非分页**。前端用 `parentId` 自行构建树。ORDER BY `parent_id ASC, order_num ASC, menu_id ASC`。

**注意**：NestJS 的 list 端点对 admin 用户返回全量菜单，对 tenant 用户按 `sys_tenant_package.menu_ids` 过滤。Phase 1 的 Rust 端**仅面向 admin 管理后台**，返回全量（和 NestJS admin 路径一致）。

#### GET `/system/menu/{menuId}`
**Permission**: `system:menu:query`
**Returns**: `MenuDetailResponseDto`

#### DELETE `/system/menu/{menuId}`
**Permission**: `system:menu:remove`
**No guards** — NestJS 的 `remove()` 直接软删除，不检查子菜单和角色绑定。Rust 端对齐此行为。如果要阻止删除有子菜单/角色绑定的菜单，使用前端确认弹窗，不在后端 guard。

Soft delete: `UPDATE SET del_flag='1'`

### 2.2 Cascade Delete (1)

#### DELETE `/system/menu/cascade/{menuIds}`
**Permission**: `system:menu:cascade-remove`
**Path**: comma-separated menu_id UUIDs

**Business logic**: PostgreSQL recursive CTE finds all descendants, then batch soft-delete:

```sql
WITH RECURSIVE descendants AS (
    SELECT menu_id FROM sys_menu
     WHERE menu_id = ANY($1) AND del_flag = '0'
    UNION ALL
    SELECT m.menu_id FROM sys_menu m
      JOIN descendants d ON m.parent_id = d.menu_id
     WHERE m.del_flag = '0'
)
UPDATE sys_menu SET del_flag = '1', update_by = $2, update_at = CURRENT_TIMESTAMP
 WHERE menu_id IN (SELECT menu_id FROM descendants)
   AND del_flag = '0'
```

Returns affected count. No guards — cascade means "delete everything below".

### 2.3 Tree Queries (3)

#### GET `/system/menu/tree-select`
**Permission**: `system:menu:tree-select`
**Returns**: `Vec<TreeNode>` — **嵌套树结构**（NestJS 在 service 层用 `ListToTree()` 构建后返回）:

```rust
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
}
```

**Data flow**:
1. Repo: `SELECT menu_id, menu_name, parent_id FROM sys_menu WHERE del_flag = '0' ORDER BY parent_id ASC, order_num ASC` → flat Vec
2. Service: `list_to_tree(flat_list)` → nested `Vec<TreeNode>`

**`list_to_tree` 算法**（O(n) 单遍扫描）：和 NestJS 的 `ListToTree()` 对齐——遍历扁平列表，用 HashMap\<menu_id, &mut TreeNode\> 做父子关联，`parent_id IS NULL` 的节点为根。放在 service 层作为一个 helper 函数。

#### GET `/system/menu/role-menu-tree-select/{roleId}`
**Permission**: `system:menu:role-menu-tree`
**Returns**: `MenuTreeSelectResponseDto`:

```rust
pub struct MenuTreeSelectResponseDto {
    pub menus: Vec<TreeNode>,              // all menus as nested tree
    pub checked_keys: Vec<String>,         // menu_ids assigned to this role/package
}
```

**Admin vs Tenant 分支**（和 NestJS 对齐）：
- **Admin** (`RequestContext.is_admin`): repo 返回全量菜单 + LEFT JOIN `sys_role_menu` 标记 `is_checked`
- **Tenant user**: 同上但加 `sys_tenant_package.menu_ids` 过滤（只看套餐范围内的菜单）

Service 从 repo 的 flat `Vec<{menu_id, menu_name, parent_id, is_checked}>` 中分离出 `checked_keys` 数组，然后对菜单列表调 `list_to_tree()` 构建树。

**Admin query**:
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

**Tenant query**: 同上 + `LEFT JOIN sys_tenant → sys_tenant_package` + `AND m.menu_id = ANY(p.menu_ids)`

#### GET `/system/menu/tenant-package-menu-tree-select/{packageId}`
**Permission**: `require_authenticated`
**Returns**: same `MenuTreeSelectResponseDto` structure

**Two parallel queries**:
1. All menus → tree (same as tree-select)
2. Package's menu_ids: `SELECT menu_ids FROM sys_tenant_package WHERE package_id = $1 AND del_flag = '0' AND status = '0'` → `Vec<String>` (TEXT[] column)

Service assembles: `menus = list_to_tree(all_menus)`, `checked_keys = package_menu_ids`.

---

## 3. Response DTOs

### MenuDetailResponseDto

```rust
pub struct MenuDetailResponseDto {
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
    pub create_at: String,         // formatted
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
    pub i18n: Option<serde_json::Value>,
}
```

### MenuResponseDto (list item)

Same fields as detail — menu list is NOT paginated and returns full details per row (NestJS behavior). Can reuse `MenuDetailResponseDto` for both list and detail.

### MenuTreeNodeDto + RoleMenuTreeResponseDto

See §2.3 above.

---

## 4. Data Layer

### New files

| File | Content |
|------|---------|
| `domain/menu_repo.rs` | `SysMenu` CRUD + tree queries + CTE cascade delete |
| `system/menu/mod.rs` | Module re-export |
| `system/menu/dto.rs` | Request/Response DTOs |
| `system/menu/service.rs` | Business logic |
| `system/menu/handler.rs` | HTTP handlers + router |
| `tests/menu_module_tests.rs` | Integration tests |
| `scripts/smoke-menu-module.sh` | E2E smoke |

### Modified files

| File | Change |
|------|--------|
| `domain/entities.rs` | Add `SysMenu` struct |
| `domain/mod.rs` | Register `menu_repo` + re-exports |
| `system/mod.rs` | Add `pub mod menu;` |
| `lib.rs` | Add `system::menu::router()` to `api_router()` |
| `response/codes.rs` | Add 7020-7022 |
| `i18n/*.json` | Add 3 new entries |
| `i18n/mod.rs` | Update coverage test list |

### MenuRepo methods

| Method | SQL pattern | Notes |
|--------|-------------|-------|
| `find_by_id(pool, menu_id)` | SELECT WHERE menu_id=$1 AND del_flag='0' | Single fetch |
| `find_list(pool, filter: MenuListFilter)` | SELECT WHERE del_flag='0' + optional name/status/parent_id/menu_type filters, ORDER BY parent_id, order_num, menu_id | **非分页**，全量返回 |
| `find_tree_nodes(pool)` | SELECT menu_id, menu_name, parent_id WHERE del_flag='0' ORDER BY parent_id, order_num | Minimal fields for tree-select + tree building base |
| `find_role_menu_tree_for_admin(pool, role_id, tenant_id)` | SELECT m.menu_id, m.menu_name, m.parent_id, (rm IS NOT NULL) as is_checked — LEFT JOIN role_menu subquery | Admin: all menus + checked flag |
| `find_role_menu_tree_for_tenant(pool, role_id, tenant_id)` | Same as admin + LEFT JOIN tenant_package + `AND m.menu_id = ANY(p.menu_ids)` | Tenant: package-scoped menus + checked flag |
| `find_package_menu_ids(pool, package_id)` | SELECT menu_ids FROM sys_tenant_package WHERE package_id=$1 AND del_flag='0' AND status='0' | TEXT[] column → Vec\<String\> |
| `insert(pool, params: MenuInsertParams)` | INSERT RETURNING | uuid PK |
| `update_by_id(pool, params: MenuUpdateParams)` | UPDATE with COALESCE | |
| `soft_delete(pool, menu_id)` | UPDATE SET del_flag='1' | Single menu, no guards |
| `cascade_soft_delete(pool, menu_ids: &[String])` | Recursive CTE + batch UPDATE | Returns affected count |

### Params structs

```rust
#[derive(Debug)]
pub struct MenuListFilter {
    pub menu_name: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug)]
pub struct MenuInsertParams {
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
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct MenuUpdateParams {
    pub menu_id: String,
    pub menu_name: Option<String>,
    pub parent_id: Option<String>,
    pub order_num: Option<i32>,
    pub path: Option<String>,
    pub component: Option<String>,
    pub query: Option<String>,
    pub is_frame: Option<String>,
    pub is_cache: Option<String>,
    pub menu_type: Option<String>,
    pub visible: Option<String>,
    pub status: Option<String>,
    pub perms: Option<String>,
    pub icon: Option<String>,
    pub remark: Option<String>,
}
```

---

## 5. Error Codes

New `ResponseCode` constants in `framework/src/response/codes.rs` (7000-7099 segment):

| Code | Constant | zh-CN | en-US |
|------|----------|-------|-------|
| 7020 | MENU_NOT_FOUND | 菜单不存在 | Menu not found |

Note: NestJS 定义了 7021 (MENU_HAS_CHILDREN) 和 7022 (MENU_HAS_ROLES) 但 `remove()` 实际不使用。Rust 端暂不添加——如果未来要加 guard，再加对应 code。

---

## 6. Testing Strategy

### Integration tests (~15 tests)

1. Create menu (type M directory) returns detail
2. Create menu (type C menu) with component path
3. Create menu (type F button) with perms
4. Create child menu under existing parent
5. List menus returns flat list (no pagination)
6. List menus filters by menu_name (substring)
7. List menus filters by status
8. List menus filters by parent_id
9. Get menu detail by id
10. Get menu nonexistent → 7020
11. Update menu changes fields
12. Delete menu soft-deletes
13. Cascade delete removes parent + all descendants
14. Tree-select returns nested tree with children
15. Role-menu-tree-select returns tree + checked_keys

### Smoke script (~10 steps)

Login → create directory → create child menu → create button → list → detail → update → tree-select → delete button → cascade delete directory + children → verify DB cleanup.

---

## 7. Wire Contract

All response DTOs use `#[serde(rename_all = "camelCase")]`. Key field mappings:

- `menuId` (not `menu_id`)
- `menuName` (not `menu_name`)
- `parentId` (not `parent_id`)
- `orderNum` (not `order_num`)
- `menuType` (not `menu_type`)
- `isFrame` (not `is_frame`)
- `isCache` (not `is_cache`)
- `checkedKeys` (not `checked_keys`)

---

## 8. Not In Scope (YAGNI)

1. `GET /system/menu/routers` — 前端路由构建（Vue web cut-over 时做）
2. Tenant package menu filtering in list endpoint（NestJS 的 findAllMenus 有按套餐过滤的分支，Phase 1 的 menu list 返回全量）
3. Cache mechanism（NestJS 有 Redis 缓存菜单数据）
4. `i18n` JSONB 列的写入（只读透传）
5. `queryParam` 字段（NestJS DTO 有但和 `query` 重复）
6. Menu import/export

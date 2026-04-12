# Dept Module Design Spec (Phase 1 Sub-Phase 5)

**Scope**: Dept CRUD (7 endpoints) — backend management of `sys_dept` with tenant scoping + ancestors path management
**Not in scope**: Data scope permission (findDeptIdsByDataScope), cache, attachDeptInfoToUsers, postIds filter

---

## 1. Entity

### SysDept (17 columns)

```sql
CREATE TABLE sys_dept (
  dept_id    VARCHAR(36)    PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id  VARCHAR(20)    NOT NULL DEFAULT '000000',
  parent_id  VARCHAR(36),                           -- NULL = root dept
  ancestors  TEXT[]         NOT NULL DEFAULT '{}',   -- path from root to parent
  dept_name  VARCHAR(30)    NOT NULL,
  order_num  INT            NOT NULL,
  leader     VARCHAR(20)    NOT NULL DEFAULT '',
  phone      VARCHAR(11)    NOT NULL DEFAULT '',
  email      VARCHAR(50)    NOT NULL DEFAULT '',
  status     CHAR(1)        NOT NULL DEFAULT '0',
  del_flag   CHAR(1)        NOT NULL DEFAULT '0',
  create_by  VARCHAR(64)    NOT NULL DEFAULT '',
  create_at  TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by  VARCHAR(64)    NOT NULL DEFAULT '',
  update_at  TIMESTAMPTZ(6) NOT NULL,
  remark     VARCHAR(500),
  i18n       JSONB
);
```

**Key difference from menu**: `tenant_id` column — dept is **tenant-scoped**, all queries filter by `current_tenant_scope()`.

**`ancestors` field**: PostgreSQL `TEXT[]` array. Stores the path from root to the **parent** of this dept (not including self). Examples:
- Root dept (parent_id = "0"): ancestors = `["0"]`
- Dept A under root: ancestors = `["0", "root_dept_id"]`
- Dept B under A: ancestors = `["0", "root_dept_id", "A_dept_id"]`

Used for:
- Fast child lookup: `WHERE $1 = ANY(ancestors)` finds all descendants without recursion
- Exclude endpoint: exclude a dept + all its descendants

**Root department convention**: `parent_id = "0"` (string literal, not NULL) represents a root department. NestJS seed data uses this convention. The string `"0"` acts as a sentinel — `ancestors = ["0"]` for root depts, `["0", parent_id, ...]` for non-root.

---

## 2. Endpoints (7)

#### POST `/system/dept/`
**Permission**: `system:dept:add`
**Request DTO** (`CreateDeptDto`):

| Field | Type | Required | Validator | Default |
|-------|------|----------|-----------|---------|
| parent_id | String | yes | - | - |
| dept_name | String | yes | length(1, 30) | - |
| order_num | i32 | yes | range(min=0) | - |
| leader | Option\<String\> | no | length(max=20) | "" |
| phone | Option\<String\> | no | length(max=11) | "" |
| email | Option\<String\> | no | email format | "" |
| status | String | no | validate_status_flag | "0" |

**Business logic**:

1. If `parent_id == "0"` (root dept): `ancestors = vec!["0".to_string()]`
2. Else: fetch parent's ancestors via `DeptRepo::find_parent_ancestors(parent_id)` → if None → 7014 DEPT_PARENT_NOT_FOUND. Build `ancestors = [...parent_ancestors, parent_id]`
3. Check `ancestors.len() <= 2000` → else 7015 DEPT_NESTING_TOO_DEEP
4. INSERT with `current_tenant_scope()` as `tenant_id`
5. Return success

#### PUT `/system/dept/`
**Permission**: `system:dept:edit`
**Request DTO** (`UpdateDeptDto`): `dept_id` (required) + all fields from Create.

**Business logic**:

1. Fetch existing by `dept_id` → 7010 if not found
2. If `parent_id` changed and `parent_id != "0"`: recalculate ancestors (same logic as create step 2)
3. If `parent_id` changed and `parent_id == "0"`: set `ancestors = vec!["0"]` (become root)
4. Check `ancestors.len() <= 2000` → else 7015 (NestJS 遗漏了 update 的深度检查，Rust 端补上)
5. UPDATE (does NOT cascade to children's ancestors — known NestJS behavior, accepted as Phase 1 debt)

#### GET `/system/dept/list`
**Permission**: `system:dept:list`
**Query DTO** (`ListDeptDto`):

| Field | Type | Filter mode |
|-------|------|-------------|
| dept_name | Option\<String\> | substring LIKE |
| status | Option\<String\> | exact match |

**Returns**: `Vec<DeptResponseDto>` — non-paginated, full list. **Tenant-scoped** (`WHERE tenant_id = $current`). ORDER BY `order_num ASC`.

#### GET `/system/dept/{id}`
**Permission**: `system:dept:query`

#### DELETE `/system/dept/{id}`
**Permission**: `system:dept:remove`
**No guards** — direct soft delete (NestJS behavior).

#### GET `/system/dept/option-select`
**Permission**: `require_authenticated`
**Returns**: Active depts only (status='0', del_flag='0'), tenant-scoped, cap 500. ORDER BY `order_num ASC`.

#### GET `/system/dept/list/exclude/{id}`
**Permission**: `system:dept:exclude-list`
**Returns**: All depts EXCEPT the given dept and its descendants.

SQL: `SELECT ... FROM sys_dept WHERE del_flag='0' AND tenant_id=$1 AND dept_id != $2 AND NOT ($2 = ANY(ancestors)) ORDER BY order_num ASC`

This uses the `ancestors` array — if `$2` (the excluded dept's ID) appears in a dept's `ancestors`, that dept is a descendant and gets excluded.

---

## 3. Response DTOs

### DeptResponseDto (used for detail, list, option-select, exclude)

```rust
pub struct DeptResponseDto {
    pub dept_id: String,
    pub parent_id: Option<String>,
    pub ancestors: Vec<String>,       // wire: JSON array ["0", "parent_id", ...]
    pub dept_name: String,
    pub order_num: i32,
    pub leader: String,
    pub phone: String,
    pub email: String,
    pub status: String,
    pub create_by: String,
    pub create_at: String,            // formatted via fmt_ts
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}
```

**Excluded from response**: `tenant_id`, `del_flag`, `i18n` — NestJS BaseResponseDto doesn't expose these via `@Expose()`.

---

## 4. Data Layer

### New files

| File | Content |
|------|---------|
| `domain/dept_repo.rs` | `SysDept` CRUD + ancestors management + exclude query |
| `system/dept/mod.rs` | Module re-export |
| `system/dept/dto.rs` | Request/Response DTOs |
| `system/dept/service.rs` | Business logic (7 functions) |
| `system/dept/handler.rs` | HTTP handlers + router |
| `tests/dept_module_tests.rs` | Integration tests |
| `scripts/smoke-dept-module.sh` | E2E smoke |

### DeptRepo methods

| Method | SQL pattern | Notes |
|--------|-------------|-------|
| `find_by_id(pool, dept_id)` | SELECT WHERE dept_id=$1 AND del_flag='0' AND tenant_id=$current | |
| `find_list(pool, filter: DeptListFilter)` | SELECT WHERE del_flag='0' AND tenant_id=$current + optional name/status, ORDER BY order_num | Non-paginated |
| `find_option_list(pool)` | SELECT WHERE status='0' AND del_flag='0' AND tenant_id=$current, ORDER BY order_num, LIMIT 500 | |
| `find_excluding(pool, dept_id)` | SELECT WHERE del_flag='0' AND tenant_id=$current AND dept_id!=$1 AND NOT ($1 = ANY(ancestors)) | Uses ancestors array |
| `find_parent_ancestors(pool, parent_id)` | SELECT ancestors FROM sys_dept WHERE dept_id=$1 AND del_flag='0' AND tenant_id=$current | Returns `Option<Vec<String>>` |
| `insert(pool, params: DeptInsertParams)` | INSERT with tenant_id from context | |
| `update_by_id(pool, params: DeptUpdateParams)` | UPDATE with COALESCE | |
| `soft_delete(pool, dept_id)` | UPDATE SET del_flag='1' WHERE dept_id=$1 AND tenant_id=$current | Tenant-scoped |

### Params structs

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
    pub ancestors: Option<Vec<String>>,  // None = don't change, Some = recalculated
    pub dept_name: Option<String>,
    pub order_num: Option<i32>,
    pub leader: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub status: Option<String>,
    pub remark: Option<String>,
}
```

---

## 5. Error Codes

| Code | Constant | zh-CN | en-US |
|------|----------|-------|-------|
| 7010 | DEPT_NOT_FOUND | 部门不存在 | Department not found |
| 7014 | DEPT_PARENT_NOT_FOUND | 父部门不存在 | Parent department not found |
| 7015 | DEPT_NESTING_TOO_DEEP | 部门嵌套层级超过限制 | Department nesting level exceeded |

---

## 6. Testing Strategy

### Integration tests (~14 tests)

1. Create dept with parent → verify ancestors built correctly
2. Create dept with non-existent parent → 7014
3. Create dept with deep nesting → 7015 (mock ancestors > 2000)
4. List depts returns flat list (non-paginated)
5. List depts filters by dept_name
6. List depts filters by status
7. Get dept detail
8. Get dept nonexistent → 7010
9. Update dept changes fields
10. Update dept parent_id → ancestors recalculated
11. Delete dept soft-deletes
12. Option-select returns active only
13. Exclude list excludes self + descendants
14. Dept is tenant-scoped (verify other tenant's dept not visible)

### Smoke script (~8 steps)

Login → create root dept → create child dept → list → detail → update → exclude list (verify child excluded when excluding root) → delete child → delete root

---

## 7. Wire Contract

`#[serde(rename_all = "camelCase")]`. Key mappings:
- `deptId`, `parentId`, `deptName`, `orderNum`

---

## 8. Not In Scope

1. Data scope permission (findDeptIdsByDataScope) — Phase 2
2. Cache mechanism
3. `attachDeptInfoToUsers` — Phase 2 user-dept join
4. `postIds` filter in list (post module doesn't exist)
5. Cascade ancestors update to children on parent_id change (known NestJS debt, accepted)
6. `i18n` JSONB column write (read-only pass-through)
7. Dept tree building endpoint (NestJS `deptTree()` — frontend builds tree from flat list)

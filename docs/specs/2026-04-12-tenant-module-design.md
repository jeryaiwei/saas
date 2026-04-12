# Tenant Module Design Spec (Phase 1 Sub-Phase 3)

**Scope**: Tenant CRUD (6 endpoints) + Tenant Package CRUD (6 endpoints)
**Not in scope**: Tenant switching, enterprise certification, data sync, quota/dashboard/audit, Excel export

---

## 1. Entities

### 1.1 SysTenant (27 columns)

```sql
CREATE TABLE sys_tenant (
  id                 VARCHAR(36) PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id          VARCHAR(20) NOT NULL UNIQUE,        -- 6-digit business ID from sequence
  parent_id          VARCHAR(20),                         -- hierarchical parent
  contact_user_name  VARCHAR(50),
  contact_phone      VARCHAR(20),
  company_name       VARCHAR(100) NOT NULL,
  license_number     VARCHAR(50),
  address            VARCHAR(200),
  intro              TEXT,
  domain             VARCHAR(100),
  package_id         VARCHAR(36),                         -- FK: sys_tenant_package.package_id
  expire_time        TIMESTAMPTZ(6),
  account_count      INT NOT NULL DEFAULT -1,             -- -1 = unlimited
  storage_quota      INT NOT NULL DEFAULT 10240,          -- MB (Phase 2+, read-only in Phase 1)
  storage_used       INT NOT NULL DEFAULT 0,              -- Phase 2+, read-only
  api_quota          INT NOT NULL DEFAULT 10000,          -- Phase 2+, read-only
  language           VARCHAR(10) NOT NULL DEFAULT 'zh-CN',
  verify_status      VARCHAR(20),                         -- Phase 2+ (enterprise cert)
  license_image_url  VARCHAR(500),                        -- Phase 2+
  reject_reason      VARCHAR(500),                        -- Phase 2+
  verified_at        TIMESTAMPTZ(6),                      -- Phase 2+
  status             CHAR(1) NOT NULL DEFAULT '0',
  del_flag           CHAR(1) NOT NULL DEFAULT '0',
  create_by          VARCHAR(64) NOT NULL DEFAULT '',
  create_at          TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by          VARCHAR(64) NOT NULL DEFAULT '',
  update_at          TIMESTAMPTZ(6) NOT NULL,
  remark             VARCHAR(500)
);
```

Rust `SysTenant` struct maps all 27 columns via `sqlx::FromRow`. Phase 2+ columns (`storage_*`, `api_quota`, `verify_status`, `license_image_url`, `reject_reason`, `verified_at`) are present in the struct but never written by Phase 1 endpoints.

**Custom `Debug` impl**: Omits `intro` field (potentially large) from debug output. All other fields printed normally.

### 1.2 SysTenantPackage (12 columns)

```sql
CREATE TABLE sys_tenant_package (
  package_id          VARCHAR(36) PRIMARY KEY DEFAULT gen_random_uuid(),
  code                VARCHAR(20) NOT NULL UNIQUE,
  package_name        VARCHAR(50) NOT NULL,
  menu_ids            TEXT[] NOT NULL DEFAULT '{}',        -- PostgreSQL text array
  menu_check_strictly BOOLEAN NOT NULL DEFAULT false,
  status              CHAR(1) NOT NULL DEFAULT '0',
  del_flag            CHAR(1) NOT NULL DEFAULT '0',
  create_by           VARCHAR(64) NOT NULL DEFAULT '',
  create_at           TIMESTAMPTZ(6) NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by           VARCHAR(64) NOT NULL DEFAULT '',
  update_at           TIMESTAMPTZ(6) NOT NULL,
  remark              VARCHAR(500)
);
```

**`menu_ids` column**: PostgreSQL `TEXT[]` array. In sqlx, bind/fetch as `Vec<String>`. No JOIN needed to resolve menu names — the package stores raw IDs, consumers (permission resolution in Phase 2) look up menu rows by ID at query time.

### 1.3 SysUserTenant (existing, no schema change)

Already defined in Phase 0. Ownership of **write operations** on this table migrates from `user_repo` to `tenant_repo` in this phase (see section 5).

### 1.4 Tenant ID Generation

PostgreSQL sequence `tenant_id_seq`, starting at `MAX(tenant_id) + 1` or `100001` if no tenants exist. The sequence must already exist in the database (created by NestJS seed). Rust queries it via:

```sql
SELECT nextval('tenant_id_seq') AS next_val
```

Returns `i64`, formatted to 6-digit zero-padded string: `format!("{:06}", next_val)`.

**Protected tenant**: `tenant_id = "000000"` is the super tenant. The constant `SUPER_TENANT_ID` lives in `domain/constants.rs` alongside the existing `SUPER_ADMIN_USERNAME`.

---

## 2. Endpoints

### 2.1 Tenant (6 endpoints)

#### POST `/system/tenant/`
**Permission**: `system:tenant:add` + `Role::PlatformAdmin`
**Request DTO** (`CreateTenantDto`):

| Field | Type | Required | Validator | Default |
|-------|------|----------|-----------|---------|
| company_name | String | yes | length(1, 100) | - |
| username | String | yes | length(1, 30) | - |
| password | String | yes | length(6, 128) | - |
| package_ids | Vec\<String\> | yes | length(min = 1) | - |
| parent_id | Option\<String\> | no | length(1, 20) | None |
| contact_user_name | Option\<String\> | no | length(0, 50) | None |
| contact_phone | Option\<String\> | no | length(0, 20) | None |
| license_number | Option\<String\> | no | length(0, 50) | None |
| address | Option\<String\> | no | length(0, 200) | None |
| intro | Option\<String\> | no | - | None |
| domain | Option\<String\> | no | length(0, 100) | None |
| expire_time | Option\<String\> | no | - | None |
| account_count | Option\<i32\> | no | - | -1 |
| status | String | no | validate_status_flag | "0" |
| language | Option\<String\> | no | length(2, 10) | "zh-CN" |
| remark | Option\<String\> | no | length(0, 500) | None |

**Multi-package creation model**: `package_ids` contains 1..N package IDs. Each package ID produces one independent tenant. All tenants share one admin user bound to all of them via `sys_user_tenant`. This is how a single customer gets access to multiple scopes (one tenant per package, switching tenant = switching scope).

**Company name rules**:
- Single package (`package_ids.len() == 1`): company_name used as-is
- Multiple packages (`package_ids.len() > 1`): each tenant named `"{company_name}-{package_name}"`

**Business logic** (service layer, single transaction):
1. Validate all `package_ids` exist and are active — batch query `TenantPackageRepo::find_active_by_ids(package_ids)`, compare counts; if any missing → 4020 TENANT_PACKAGE_NOT_FOUND
2. Validate `parent_id` tenant exists (if provided)
3. Validate `company_name` not already used (prefix match: `WHERE company_name LIKE $1 || '%' AND del_flag = '0'`)
4. Validate `username` globally unique (`UserRepo::verify_user_name_unique`)
5. Hash `password` via `crypto::hash_password`
6. Generate base `tenant_id` via `SELECT nextval('tenant_id_seq')` — for N packages, tenant IDs are `base_id + 0`, `base_id + 1`, ..., `base_id + (N-1)`, each formatted as 6-digit zero-padded string
7. If N > 1: fetch package names for company name construction (`TenantPackageRepo::find_names_by_ids`)
8. Begin tx:
   - For i in 0..N:
     - `tenant_id = format!("{:06}", base_id + i)`
     - `company = if N == 1 { company_name } else { "{company_name}-{package_name}" }`
     - `TenantRepo::insert_tx(tx, TenantInsertParams { tenant_id, company_name: company, package_id: package_ids[i], ... })`
   - `UserRepo::insert_tx(tx, UserInsertParams { ... })` → create admin user (once)
   - For each created tenant_id:
     - `TenantRepo::insert_user_tenant_binding_tx(tx, user_id, tenant_id, is_admin="1")`
     - First tenant's binding: `is_default = "1"`; others: `is_default = "0"`
9. Commit tx
10. Return success (no body data — consistent with NestJS which returns `Result.ok()`)

**Sequence allocation**: When N > 1, only one `nextval` call is made to get the base ID. Subsequent IDs are `base + i` (simple arithmetic, no extra sequence calls). This matches NestJS behavior and avoids N round-trips to the sequence.

**Response**: `ApiResponse::success()` (no body data, consistent with NestJS).

#### GET `/system/tenant/list`
**Permission**: `system:tenant:list` + `Role::SuperAdmin`
**Query DTO** (`ListTenantDto`):

| Field | Type | Filter mode |
|-------|------|-------------|
| tenant_id | Option\<String\> | substring LIKE |
| contact_user_name | Option\<String\> | substring LIKE |
| contact_phone | Option\<String\> | substring LIKE |
| company_name | Option\<String\> | substring LIKE |
| status | Option\<String\> | exact match |
| page | PageQuery | flatten |

**Data assembly** (2 queries, not JOIN):
1. `TenantRepo::find_page(filter)` → paginated `Vec<SysTenant>` + `total`
2. Batch: `TenantRepo::find_admin_user_names(tenant_ids)` → `HashMap<String, String>` (tenant_id → admin username)
3. Service maps both into `TenantListItemResponseDto` (with `admin_user_name` + `package_name` from JOIN in query 1)

**Note**: `find_page` LEFT JOINs `sys_tenant_package` to get `package_name`. The admin username is a separate batch query because it crosses the `sys_user_tenant + sys_user` tables via `DISTINCT ON`.

#### GET `/system/tenant/{id}`
**Permission**: `system:tenant:query`
**Path**: `id` = the UUID primary key (NOT `tenant_id` business number)

**Data assembly** (2 queries):
1. `TenantRepo::find_by_id(id)` → `SysTenant` (with package_name via LEFT JOIN)
2. `TenantRepo::find_admin_user_info(tenant_id)` → `{nick_name, phonenumber, whatsapp}`
3. Service assembles into `TenantDetailResponseDto`

#### PUT `/system/tenant/`
**Permission**: `system:tenant:edit`
**Request DTO** (`UpdateTenantDto`):

| Field | Type | Required |
|-------|------|----------|
| id | String | yes (UUID PK) |
| tenant_id | String | yes (for protected-tenant check, NOT updated) |
| contact_user_name | Option\<String\> | no |
| contact_phone | Option\<String\> | no |
| company_name | Option\<String\> | no |
| license_number | Option\<String\> | no |
| address | Option\<String\> | no |
| intro | Option\<String\> | no |
| domain | Option\<String\> | no |
| package_id | Option\<String\> | no |
| expire_time | Option\<String\> | no |
| account_count | Option\<i32\> | no |
| status | Option\<String\> | no |
| remark | Option\<String\> | no |

**Immutable fields** (not in DTO): `tenant_id`, `parent_id`, `language`, `username`, `password`

**Business logic**:
1. Fetch existing tenant by `id`, 404 if not found
2. Protected tenant check: if `is_protected_tenant(tenant_id)`, reject changes to `status` / `company_name` / `package_id` / `expire_time` / `account_count`
3. If `company_name` changed: check uniqueness (exclude self via `tenant_id`)
4. `TenantRepo::update_by_id(id, params)` → UPDATE sys_tenant
5. Return success (no body data)

#### DELETE `/system/tenant/{ids}`
**Permission**: `system:tenant:remove` + `Role::PlatformAdmin`
**Path**: comma-separated UUIDs (same pattern as user delete)

**Guards** (validate ALL before any write):
1. For each tenant: `is_protected_tenant(tenant_id)` → 4010 TENANT_PROTECTED
2. Batch: `TenantRepo::find_tenant_ids_with_children(tenant_ids)` → if any has children → 4016 TENANT_HAS_CHILDREN
3. Soft delete: `TenantRepo::soft_delete_by_ids(ids)`

**Does NOT** cascade-delete admin users or `sys_user_tenant` bindings. The admin user and bindings survive tenant deletion (consistent with NestJS).

### 2.2 Tenant Package (6 endpoints)

#### POST `/system/tenant-package/`
**Permission**: `system:tenant-package:add`
**Request DTO** (`CreateTenantPackageDto`):

| Field | Type | Required | Validator |
|-------|------|----------|-----------|
| code | String | yes | length(1, 20) |
| package_name | String | yes | length(1, 50) |
| menu_ids | Vec\<String\> | no | default empty |
| menu_check_strictly | Option\<bool\> | no | default false |
| status | String | no | validate_status_flag, default "0" |
| remark | Option\<String\> | no | length(0, 500) |

**Business logic**:
1. Validate `code` unique (`TenantPackageRepo::verify_code_unique`)
2. Validate `package_name` unique (`TenantPackageRepo::verify_name_unique`)
3. `TenantPackageRepo::insert` → INSERT `sys_tenant_package`
4. Return `TenantPackageDetailResponseDto`

#### GET `/system/tenant-package/list`
**Permission**: `system:tenant-package:list`
**Query DTO** (`ListTenantPackageDto`):

| Field | Type | Filter mode |
|-------|------|-------------|
| package_name | Option\<String\> | substring LIKE |
| status | Option\<String\> | exact match |
| page | PageQuery | flatten |

#### GET `/system/tenant-package/{id}`
**Permission**: `system:tenant-package:query`
**Returns**: Full package detail including `menu_ids` array.

#### PUT `/system/tenant-package/`
**Permission**: `system:tenant-package:edit`
**Request DTO** (`UpdateTenantPackageDto`):

| Field | Type | Required |
|-------|------|----------|
| package_id | String | yes |
| code | Option\<String\> | no |
| package_name | Option\<String\> | no |
| menu_ids | Option\<Vec\<String\>\> | no |
| menu_check_strictly | Option\<bool\> | no |
| status | Option\<String\> | no |
| remark | Option\<String\> | no |

**Business logic**:
1. Fetch existing by `package_id`, 404 if not found
2. If `code` changed: check uniqueness (exclude self)
3. If `package_name` changed: check uniqueness (exclude self)
4. `TenantPackageRepo::update_by_id(package_id, params)` → UPDATE
5. Return success

#### DELETE `/system/tenant-package/{ids}`
**Permission**: `system:tenant-package:remove`
**Path**: comma-separated package_id UUIDs

**Guard**: `TenantPackageRepo::is_any_in_use(ids)` → count active tenants referencing these packages. If > 0 → 4023 TENANT_PACKAGE_IN_USE.

Soft delete: `TenantPackageRepo::soft_delete_by_ids(ids)`

#### GET `/system/tenant-package/option-select`
**Permission**: `require_authenticated`
**Returns**: Active packages only (status='0', del_flag='0'), fields: `package_id`, `code`, `package_name`. ORDER BY `create_at DESC`, cap 500.

---

## 3. Response DTOs

### TenantDetailResponseDto

```rust
pub struct TenantDetailResponseDto {
    pub id: String,
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub contact_user_name: Option<String>,
    pub contact_phone: Option<String>,
    pub company_name: String,
    pub license_number: Option<String>,
    pub address: Option<String>,
    pub intro: Option<String>,
    pub domain: Option<String>,
    pub package_id: Option<String>,
    pub package_name: Option<String>,       // from LEFT JOIN
    pub expire_time: Option<String>,        // formatted YYYY-MM-DD HH:mm:ss
    pub account_count: i32,
    pub language: String,
    pub admin_user_name: Option<String>,    // from batch/separate query
    pub nick_name: Option<String>,          // admin user's nick_name
    pub phonenumber: Option<String>,        // admin user's phone
    pub whatsapp: Option<String>,           // admin user's whatsapp
    pub status: String,
    pub create_by: String,
    pub create_at: String,                  // formatted
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}
```

### TenantListItemResponseDto

Same as detail but without `nick_name` / `phonenumber` / `whatsapp` (admin user info is just `admin_user_name` from batch query), and without `intro` / `license_number` / `address` / `domain` (lightweight for list).

### TenantPackageDetailResponseDto

```rust
pub struct TenantPackageDetailResponseDto {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
    pub menu_ids: Vec<String>,
    pub menu_check_strictly: bool,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}
```

### TenantPackageOptionResponseDto

```rust
pub struct TenantPackageOptionResponseDto {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
}
```

---

## 4. Data Layer

### 4.1 New files

| File | Content |
|------|---------|
| `domain/tenant_repo.rs` | `SysTenant` CRUD + `sys_user_tenant` writes + tenant_id sequence + admin user info queries |
| `domain/tenant_package_repo.rs` | `SysTenantPackage` CRUD + in-use check |
| `domain/entities.rs` | Add `SysTenant` + `SysTenantPackage` structs |
| `system/tenant/mod.rs` | Module re-export |
| `system/tenant/dto.rs` | Request/Response DTOs |
| `system/tenant/service.rs` | Business logic |
| `system/tenant/handler.rs` | HTTP handlers + router |
| `system/tenant_package/mod.rs` | Module re-export |
| `system/tenant_package/dto.rs` | Request/Response DTOs |
| `system/tenant_package/service.rs` | Business logic |
| `system/tenant_package/handler.rs` | HTTP handlers + router |

### 4.2 TenantRepo methods

| Method | SQL pattern | Notes |
|--------|-------------|-------|
| `find_by_id(id)` | SELECT + LEFT JOIN package | Returns SysTenant + package_name |
| `find_by_tenant_id(tenant_id)` | SELECT WHERE tenant_id = $1 | For existence checks |
| `find_page(filter)` | SELECT + LEFT JOIN package + WHERE + LIMIT/OFFSET | Shared WHERE const, pagination v1.1 pattern |
| `find_admin_user_names(tenant_ids)` | DISTINCT ON + JOIN sys_user_tenant + sys_user | Batch, returns HashMap |
| `find_admin_user_info(tenant_id)` | JOIN sys_user_tenant + sys_user WHERE is_admin='1' LIMIT 1 | Single tenant detail |
| `find_tenant_ids_with_children(tenant_ids)` | SELECT DISTINCT parent_id WHERE parent_id = ANY($1) | For cascade delete check |
| `exists_by_company_name_prefix(name, exclude_tenant_id?)` | COUNT WHERE company_name LIKE $1\|\|'%' | Uniqueness check |
| `generate_next_tenant_id()` | SELECT nextval('tenant_id_seq') | Returns i64 |
| `insert_tx(tx, params)` | INSERT RETURNING all columns | Owned params struct: `TenantInsertParams` |
| `update_by_id(id, params)` | UPDATE WHERE id = $1 | Owned params struct: `TenantUpdateParams` |
| `soft_delete_by_ids(ids)` | UPDATE SET del_flag='1' WHERE id = ANY($1) | Batch soft delete |
| `insert_user_tenant_binding_tx(tx, user_id, tenant_id, is_admin)` | INSERT sys_user_tenant | **Migrated from user_repo** |

### 4.3 TenantPackageRepo methods

| Method | SQL pattern | Notes |
|--------|-------------|-------|
| `find_by_id(id)` | SELECT WHERE package_id = $1 | Single fetch |
| `find_page(filter)` | SELECT + WHERE + LIMIT/OFFSET | Standard pagination |
| `find_option_list()` | SELECT WHERE status='0' AND del_flag='0' ORDER BY create_at DESC LIMIT 500 | Dropdown |
| `find_active_by_ids(ids)` | SELECT WHERE package_id = ANY($1) AND status='0' AND del_flag='0' | For create-tenant batch validation |
| `find_names_by_ids(ids)` | SELECT package_id, package_name WHERE package_id = ANY($1) | For multi-package company name construction |
| `verify_code_unique(code, exclude_id?)` | COUNT WHERE code=$1 AND del_flag='0' | Uniqueness |
| `verify_name_unique(name, exclude_id?)` | COUNT WHERE package_name=$1 AND del_flag='0' | Uniqueness |
| `is_any_in_use(ids)` | COUNT FROM sys_tenant WHERE package_id = ANY($1) AND del_flag='0' | Delete guard |
| `insert(params)` | INSERT RETURNING all columns | Owned params struct |
| `update_by_id(id, params)` | UPDATE WHERE package_id = $1 | Owned params struct |
| `soft_delete_by_ids(ids)` | UPDATE SET del_flag='1' WHERE package_id = ANY($1) | Batch soft delete |

---

## 5. Cross-Module Migration

### 5.1 `insert_user_tenant_binding_tx` ownership transfer

**From**: `user_repo.rs` → **To**: `tenant_repo.rs`

The method signature gains a new `is_admin: &str` parameter (was hardcoded `'0'`; tenant create needs `'1'`):

```rust
pub async fn insert_user_tenant_binding_tx(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    tenant_id: &str,
    is_admin: &str,
) -> anyhow::Result<()>
```

**Call site updates**:
- `user/service.rs::create()` → changes import from `UserRepo::insert_user_tenant_binding_tx` to `TenantRepo::insert_user_tenant_binding_tx`, passes `is_admin = "0"`
- `tenant/service.rs::create()` → calls `TenantRepo::insert_user_tenant_binding_tx` with `is_admin = "1"`

### 5.2 `UserRepo::insert_tx` parameter extension

Current `UserInsertParams` assumes `platform_id = PLATFORM_ID_DEFAULT` and `user_type = USER_TYPE_CUSTOM` (hardcoded in the SQL). This is correct for tenant admin creation too — no change needed. The tenant module's admin user is always a CUSTOM backend user on the default platform.

---

## 6. Error Codes

New `ResponseCode` constants in `framework/src/response/codes.rs` (4000-4029 segment):

| Code | Constant | zh-CN | en-US |
|------|----------|-------|-------|
| 4001 | TENANT_NOT_FOUND | 租户不存在 | Tenant not found |
| 4010 | TENANT_PROTECTED | 不可操作受保护的系统租户 | Cannot modify protected system tenant |
| 4013 | TENANT_COMPANY_EXISTS | 公司名称已存在 | Company name already exists |
| 4014 | TENANT_PARENT_NOT_FOUND | 父租户不存在 | Parent tenant not found |
| 4016 | TENANT_HAS_CHILDREN | 不可删除含有子租户的租户 | Cannot delete tenant with child tenants |
| 4020 | TENANT_PACKAGE_NOT_FOUND | 套餐不存在 | Tenant package not found |
| 4021 | TENANT_PACKAGE_CODE_EXISTS | 套餐编码已存在 | Package code already exists |
| 4022 | TENANT_PACKAGE_NAME_EXISTS | 套餐名称已存在 | Package name already exists |
| 4023 | TENANT_PACKAGE_IN_USE | 不可删除正在使用的套餐 | Cannot delete package in use by tenants |

**Note**: 4001 (TENANT_NOT_FOUND) and 4002 (TENANT_EXPIRED) already exist in `codes.rs` but with wrong i18n text — 4001 currently says "租户已禁用" (should be "租户不存在"), 4002 says "租户已过期" (correct). Fix 4001's i18n text.

Also update `every_response_code_has_i18n_entries_in_all_langs` test list with the new constants.

---

## 7. Constants

Add to `domain/constants.rs`:

```rust
pub const SUPER_TENANT_ID: &str = "000000";
```

Add helper in `tenant_repo.rs`:

```rust
pub fn is_protected_tenant(tenant_id: &str) -> bool {
    tenant_id == SUPER_TENANT_ID
}
```

Protected tenant guard: cannot modify `status`, `company_name`, `package_id`, `expire_time`, `account_count` fields on tenants where `is_protected_tenant(tenant_id)` returns true.

---

## 8. Testing Strategy

### 8.1 Unit tests (framework layer)

- `ResponseCode` i18n coverage test updated with new 4000-segment constants (9 new codes)
- `is_protected_tenant` helper: true for "000000", false for anything else

### 8.2 Integration tests

New file: `crates/modules/tests/tenant_module_tests.rs`

**Tenant tests** (~22 tests):

1. Create single-package tenant returns success with generated tenant_id
2. Create tenant auto-creates admin user (verify via DB query)
3. Create tenant with package_id binds package
4. Create multi-package tenant creates N tenants + 1 user bound to all N
5. Create multi-package tenant names each tenant `{companyName}-{packageName}`
6. Create multi-package tenant sets first binding as is_default='1', others '0'
7. Create tenant with invalid package_id → 4020
8. Create tenant with duplicate company_name → 4013
9. Create tenant with duplicate username → 1002 (DUPLICATE_KEY)
10. Create tenant with parent_id that doesn't exist → 4014
11. List tenants returns paginated results with admin_user_name
12. List tenants filters by company_name / status / tenant_id
13. Get tenant detail includes admin user info
14. Get tenant with non-existent id → 1001 (DATA_NOT_FOUND)
15. Update tenant changes mutable fields
16. Update protected tenant's status → 4010
17. Update tenant company_name to duplicate → 4013
18. Delete tenant soft-deletes
19. Delete protected tenant → 4010
20. Delete tenant with children → 4016

**Tenant Package tests** (~14 tests):
1. Create package returns detail with menu_ids
2. Create package with duplicate code → 4021
3. Create package with duplicate name → 4022
4. List packages with filter
5. Get package detail with menu_ids array
6. Update package changes menu_ids
7. Update package name to duplicate → 4022
8. Delete package succeeds when not in use
9. Delete package in use → 4023
10. Option-select returns active only

### 8.3 Smoke script

New file: `scripts/smoke-tenant-module.sh`

Covers the full lifecycle: create package → create tenant (with package + admin user) → list → detail → update → delete package guard (in-use) → delete tenant → delete package (now ok).

---

## 9. Wire Contract Alignment

All response DTOs use `#[serde(rename_all = "camelCase")]` to match NestJS wire format. Field names:

- `tenantId` (not `tenant_id`)
- `companyName` (not `company_name`)
- `packageId` (not `package_id`)
- `packageName` (not `package_name`)
- `adminUserName` (not `admin_user_name`)
- `expireTime` (not `expire_time`)
- `accountCount` (not `account_count`)
- `menuIds` (not `menu_ids`)
- `menuCheckStrictly` (not `menu_check_strictly`)

Timestamps formatted via `fmt_ts` (existing helper, `YYYY-MM-DDTHH:mm:ss.sssZ`).

---

## 10. Not In Scope (explicit YAGNI)

1. Enterprise certification flow (verify_status/license_image_url/reject_reason)
2. Tenant switching (select-list/switch/clear/switch-status)
3. Data sync (sync-dict/sync-package/sync-config)
4. Quota enforcement (storage_quota/api_quota checks)
5. Dashboard / audit endpoints
6. Excel export
7. Cache invalidation (no Redis cache layer in Rust Phase 1)
8. Tenant hierarchy depth check (only parent existence validated)
9. Package menu sync to tenants on package update (Phase 2 — tenant-package-menu filter)

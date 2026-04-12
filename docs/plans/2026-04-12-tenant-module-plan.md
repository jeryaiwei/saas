# Tenant Module Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement Tenant CRUD (6 endpoints) + Tenant Package CRUD (6 endpoints) with multi-package creation support, matching NestJS wire contract.

**Architecture:** Two sub-modules (`system/tenant` + `system/tenant_package`) following the established role/user module pattern — handler → service → repo with owned params structs. Tenant creation auto-creates an admin user + N tenant bindings in a single transaction. `sys_user_tenant` write ownership migrates from `user_repo` to `tenant_repo`.

**Tech Stack:** Rust, axum 0.8, sqlx 0.8 (runtime queries), validator 0.20, serde, anyhow, chrono, uuid.

**Spec:** `docs/specs/2026-04-12-tenant-module-design.md`

**Baseline:** 198 tests passing. Run `cd server-rs && cargo test --workspace 2>&1 | grep "test result:"` to confirm.

**Git policy:** Per standing user preference, no automatic git commands. Implementer must not run git. User commits manually.

---

## File Structure

### New files (14)

| File | Responsibility |
|------|---------------|
| `crates/modules/src/domain/tenant_repo.rs` | `sys_tenant` CRUD + `sys_user_tenant` writes + tenant_id sequence |
| `crates/modules/src/domain/tenant_package_repo.rs` | `sys_tenant_package` CRUD + in-use check |
| `crates/modules/src/system/tenant/mod.rs` | Module re-export |
| `crates/modules/src/system/tenant/dto.rs` | Tenant request/response DTOs |
| `crates/modules/src/system/tenant/service.rs` | Tenant business logic |
| `crates/modules/src/system/tenant/handler.rs` | Tenant HTTP handlers + router |
| `crates/modules/src/system/tenant_package/mod.rs` | Module re-export |
| `crates/modules/src/system/tenant_package/dto.rs` | Package request/response DTOs |
| `crates/modules/src/system/tenant_package/service.rs` | Package business logic |
| `crates/modules/src/system/tenant_package/handler.rs` | Package HTTP handlers + router |
| `crates/modules/tests/tenant_module_tests.rs` | Integration tests |
| `scripts/smoke-tenant-module.sh` | End-to-end smoke script |

### Modified files (8)

| File | Change |
|------|--------|
| `crates/modules/src/domain/entities.rs` | Add `SysTenant` + `SysTenantPackage` structs |
| `crates/modules/src/domain/mod.rs` | Add `pub mod tenant_repo; pub mod tenant_package_repo;` + re-exports |
| `crates/modules/src/domain/constants.rs` | Add `SUPER_TENANT_ID` |
| `crates/modules/src/domain/user_repo.rs` | Remove `insert_user_tenant_binding_tx` (migrated to tenant_repo) |
| `crates/modules/src/system/user/service.rs` | Update import: `TenantRepo::insert_user_tenant_binding_tx` |
| `crates/modules/src/system/mod.rs` | Add `pub mod tenant; pub mod tenant_package;` |
| `crates/modules/src/lib.rs` | Add tenant + tenant_package routers to `api_router()` |
| `crates/framework/src/response/codes.rs` | Add 9 new ResponseCode constants (4001-4023 segment) |
| `i18n/zh-CN.json` + `i18n/en-US.json` | Add 9 new i18n entries |
| `crates/framework/src/i18n/mod.rs` | Update `every_response_code_has_i18n_entries` test list |

---

### Task 1: ResponseCode + i18n entries + entity structs

**Files:**
- Modify: `crates/framework/src/response/codes.rs`
- Modify: `i18n/zh-CN.json`
- Modify: `i18n/en-US.json`
- Modify: `crates/framework/src/i18n/mod.rs`
- Modify: `crates/modules/src/domain/entities.rs`
- Modify: `crates/modules/src/domain/constants.rs`

This task adds the foundation types that everything else depends on.

- [ ] **Step 1: Add 9 new ResponseCode constants**

Edit `crates/framework/src/response/codes.rs`. Add after the existing `// --- 4000-4029 tenant ---` section:

```rust
    // --- 4000-4029 tenant ---
    pub const TENANT_NOT_FOUND: Self = Self(4001);
    pub const TENANT_DISABLED: Self = Self(4002);
    pub const TENANT_PROTECTED: Self = Self(4010);
    pub const TENANT_COMPANY_EXISTS: Self = Self(4013);
    pub const TENANT_PARENT_NOT_FOUND: Self = Self(4014);
    pub const TENANT_HAS_CHILDREN: Self = Self(4016);
    pub const TENANT_PACKAGE_NOT_FOUND: Self = Self(4020);
    pub const TENANT_PACKAGE_CODE_EXISTS: Self = Self(4021);
    pub const TENANT_PACKAGE_NAME_EXISTS: Self = Self(4022);
    pub const TENANT_PACKAGE_IN_USE: Self = Self(4023);
```

Note: `TENANT_DISABLED(4001)` and `TENANT_EXPIRED(4002)` already exist in the file. Keep those, add the new ones starting from 4010.

- [ ] **Step 2: Fix existing i18n for 4001 + add 8 new entries**

Edit `i18n/zh-CN.json`. The existing `"4001"` says "租户已禁用" — change it to "租户不存在" (matching NestJS TENANT_NOT_FOUND). Add new entries:

```json
  "4001": "租户不存在",
  "4002": "租户已过期",
  "4010": "不可操作受保护的系统租户",
  "4013": "公司名称已存在",
  "4014": "父租户不存在",
  "4016": "不可删除含有子租户的租户",
  "4020": "套餐不存在",
  "4021": "套餐编码已存在",
  "4022": "套餐名称已存在",
  "4023": "不可删除正在使用的套餐",
```

Edit `i18n/en-US.json`:

```json
  "4001": "Tenant not found",
  "4002": "Tenant expired",
  "4010": "Cannot modify protected system tenant",
  "4013": "Company name already exists",
  "4014": "Parent tenant not found",
  "4016": "Cannot delete tenant with child tenants",
  "4020": "Tenant package not found",
  "4021": "Package code already exists",
  "4022": "Package name already exists",
  "4023": "Cannot delete package in use by tenants",
```

- [ ] **Step 3: Update i18n coverage test list**

Edit `crates/framework/src/i18n/mod.rs`. Find the `every_response_code_has_i18n_entries_in_all_langs` test. Add the new constants to the `codes` array in the `// 4000-4029 tenant` section:

```rust
            // 4000-4029 tenant
            ResponseCode::TENANT_NOT_FOUND,
            ResponseCode::TENANT_DISABLED,
            ResponseCode::TENANT_PROTECTED,
            ResponseCode::TENANT_COMPANY_EXISTS,
            ResponseCode::TENANT_PARENT_NOT_FOUND,
            ResponseCode::TENANT_HAS_CHILDREN,
            ResponseCode::TENANT_PACKAGE_NOT_FOUND,
            ResponseCode::TENANT_PACKAGE_CODE_EXISTS,
            ResponseCode::TENANT_PACKAGE_NAME_EXISTS,
            ResponseCode::TENANT_PACKAGE_IN_USE,
```

- [ ] **Step 4: Add `SUPER_TENANT_ID` constant**

Edit `crates/modules/src/domain/constants.rs`. Add:

```rust
pub const SUPER_TENANT_ID: &str = "000000";
```

- [ ] **Step 5: Add `SysTenant` entity struct**

Edit `crates/modules/src/domain/entities.rs`. Add at the end of the file:

```rust
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysTenant {
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
    pub expire_time: Option<DateTime<Utc>>,
    pub account_count: i32,
    pub storage_quota: i32,
    pub storage_used: i32,
    pub api_quota: i32,
    pub language: String,
    pub verify_status: Option<String>,
    pub license_image_url: Option<String>,
    pub reject_reason: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysTenantPackage {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
    pub menu_ids: Vec<String>,
    pub menu_check_strictly: bool,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
}
```

- [ ] **Step 6: Verify compile + i18n test**

```bash
cd server-rs && cargo test -p framework every_response_code_has_i18n 2>&1 | tail -10
cd server-rs && cargo check --workspace 2>&1 | tail -5
```

Expected: i18n test passes with all new codes covered, workspace compiles clean.

- [ ] **Step 7: Report Task 1 complete**

---

### Task 2: TenantPackageRepo (simpler, no cross-module deps)

**Files:**
- Create: `crates/modules/src/domain/tenant_package_repo.rs`
- Modify: `crates/modules/src/domain/mod.rs`

Starting with package repo because it has zero cross-module dependencies — tenant repo depends on it (for `find_active_by_ids`) but not the reverse.

- [ ] **Step 1: Create tenant_package_repo.rs**

Create `crates/modules/src/domain/tenant_package_repo.rs`:

```rust
//! TenantPackageRepo — hand-written SQL for sys_tenant_package.

use super::entities::SysTenantPackage;
use anyhow::Context;
use framework::context::{audit_update_by, AuditInsert};
use framework::response::{PageQuery, PaginationParams};
use sqlx::PgPool;

const COLUMNS: &str = "\
    package_id, code, package_name, menu_ids, menu_check_strictly, \
    status, del_flag, create_by, create_at, update_by, update_at, remark";

const PACKAGE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR package_name LIKE '%' || $1 || '%') \
      AND ($2::varchar IS NULL OR status = $2)";

#[derive(Debug)]
pub struct PackageListFilter {
    pub package_name: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct PackageInsertParams {
    pub code: String,
    pub package_name: String,
    pub menu_ids: Vec<String>,
    pub menu_check_strictly: bool,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct PackageUpdateParams {
    pub package_id: String,
    pub code: Option<String>,
    pub package_name: Option<String>,
    pub menu_ids: Option<Vec<String>>,
    pub menu_check_strictly: Option<bool>,
    pub status: Option<String>,
    pub remark: Option<String>,
}

pub struct TenantPackageRepo;

impl TenantPackageRepo {
    #[tracing::instrument(skip_all, fields(package_id = %package_id))]
    pub async fn find_by_id(
        pool: &PgPool,
        package_id: &str,
    ) -> anyhow::Result<Option<SysTenantPackage>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_tenant_package \
              WHERE package_id = $1 AND del_flag = '0' LIMIT 1"
        );
        sqlx::query_as::<_, SysTenantPackage>(&sql)
            .bind(package_id)
            .fetch_optional(pool)
            .await
            .context("find_by_id")
    }

    #[tracing::instrument(skip_all, fields(
        has_name = filter.package_name.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        pool: &PgPool,
        filter: PackageListFilter,
    ) -> anyhow::Result<framework::response::Page<SysTenantPackage>> {
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_tenant_package {PACKAGE_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $3 OFFSET $4"
        );
        let rows = sqlx::query_as::<_, SysTenantPackage>(&rows_sql)
            .bind(filter.package_name.as_deref())
            .bind(filter.status.as_deref())
            .bind(p.limit)
            .bind(p.offset)
            .fetch_all(pool)
            .await
            .context("find_page rows")?;

        let count_sql = format!(
            "SELECT COUNT(*) FROM sys_tenant_package {PACKAGE_PAGE_WHERE}"
        );
        let total: i64 = sqlx::query_scalar(&count_sql)
            .bind(filter.package_name.as_deref())
            .bind(filter.status.as_deref())
            .fetch_one(pool)
            .await
            .context("find_page count")?;

        Ok(p.into_page(rows, total))
    }

    #[tracing::instrument(skip_all)]
    pub async fn find_option_list(pool: &PgPool) -> anyhow::Result<Vec<SysTenantPackage>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_tenant_package \
              WHERE del_flag = '0' AND status = '0' \
              ORDER BY create_at DESC LIMIT 500"
        );
        sqlx::query_as::<_, SysTenantPackage>(&sql)
            .fetch_all(pool)
            .await
            .context("find_option_list")
    }

    #[tracing::instrument(skip_all, fields(count = ids.len()))]
    pub async fn find_active_by_ids(
        pool: &PgPool,
        ids: &[String],
    ) -> anyhow::Result<Vec<SysTenantPackage>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_tenant_package \
              WHERE package_id = ANY($1) AND status = '0' AND del_flag = '0'"
        );
        sqlx::query_as::<_, SysTenantPackage>(&sql)
            .bind(ids)
            .fetch_all(pool)
            .await
            .context("find_active_by_ids")
    }

    #[tracing::instrument(skip_all, fields(code = %code))]
    pub async fn verify_code_unique(
        pool: &PgPool,
        code: &str,
        exclude_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_tenant_package \
              WHERE code = $1 AND del_flag = '0' \
              AND ($2::varchar IS NULL OR package_id != $2)",
        )
        .bind(code)
        .bind(exclude_id)
        .fetch_one(pool)
        .await
        .context("verify_code_unique")?;
        Ok(count == 0)
    }

    #[tracing::instrument(skip_all, fields(name = %name))]
    pub async fn verify_name_unique(
        pool: &PgPool,
        name: &str,
        exclude_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_tenant_package \
              WHERE package_name = $1 AND del_flag = '0' \
              AND ($2::varchar IS NULL OR package_id != $2)",
        )
        .bind(name)
        .bind(exclude_id)
        .fetch_one(pool)
        .await
        .context("verify_name_unique")?;
        Ok(count == 0)
    }

    #[tracing::instrument(skip_all, fields(count = ids.len()))]
    pub async fn is_any_in_use(pool: &PgPool, ids: &[String]) -> anyhow::Result<bool> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_tenant \
              WHERE package_id = ANY($1) AND del_flag = '0'",
        )
        .bind(ids)
        .fetch_one(pool)
        .await
        .context("is_any_in_use")?;
        Ok(count > 0)
    }

    #[tracing::instrument(skip_all, fields(code = %params.code))]
    pub async fn insert(
        pool: &PgPool,
        params: PackageInsertParams,
    ) -> anyhow::Result<SysTenantPackage> {
        let audit = AuditInsert::now();
        let package_id = uuid::Uuid::new_v4().to_string();
        let sql = format!(
            "INSERT INTO sys_tenant_package (\
                package_id, code, package_name, menu_ids, menu_check_strictly, \
                status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                $1, $2, $3, $4, $5, $6, '0', $7, $8, CURRENT_TIMESTAMP, $9\
            ) RETURNING {COLUMNS}"
        );
        sqlx::query_as::<_, SysTenantPackage>(&sql)
            .bind(&package_id)
            .bind(&params.code)
            .bind(&params.package_name)
            .bind(&params.menu_ids)
            .bind(params.menu_check_strictly)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(pool)
            .await
            .context("insert")
    }

    #[tracing::instrument(skip_all, fields(package_id = %params.package_id))]
    pub async fn update_by_id(
        pool: &PgPool,
        params: PackageUpdateParams,
    ) -> anyhow::Result<u64> {
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_tenant_package \
                SET code = COALESCE($1, code), \
                    package_name = COALESCE($2, package_name), \
                    menu_ids = COALESCE($3, menu_ids), \
                    menu_check_strictly = COALESCE($4, menu_check_strictly), \
                    status = COALESCE($5, status), \
                    remark = COALESCE($6, remark), \
                    update_by = $7, update_at = CURRENT_TIMESTAMP \
              WHERE package_id = $8 AND del_flag = '0'",
        )
        .bind(params.code.as_deref())
        .bind(params.package_name.as_deref())
        .bind(params.menu_ids.as_deref())
        .bind(params.menu_check_strictly)
        .bind(params.status.as_deref())
        .bind(params.remark.as_deref())
        .bind(&updater)
        .bind(&params.package_id)
        .execute(pool)
        .await
        .context("update_by_id")?
        .rows_affected();
        Ok(affected)
    }

    #[tracing::instrument(skip_all, fields(count = ids.len()))]
    pub async fn soft_delete_by_ids(pool: &PgPool, ids: &[String]) -> anyhow::Result<u64> {
        let updater = audit_update_by();
        let affected = sqlx::query(
            "UPDATE sys_tenant_package \
                SET del_flag = '1', update_by = $1, update_at = CURRENT_TIMESTAMP \
              WHERE package_id = ANY($2) AND del_flag = '0'",
        )
        .bind(&updater)
        .bind(ids)
        .execute(pool)
        .await
        .context("soft_delete_by_ids")?
        .rows_affected();
        Ok(affected)
    }
}
```

- [ ] **Step 2: Register in domain/mod.rs**

Edit `crates/modules/src/domain/mod.rs`. Add:

```rust
pub mod tenant_package_repo;
```

And re-exports:

```rust
pub use tenant_package_repo::{
    PackageInsertParams, PackageListFilter, PackageUpdateParams, TenantPackageRepo,
};
```

- [ ] **Step 3: Add `SysTenantPackage` to entity re-exports**

In the same `domain/mod.rs`, update the entities use line:

```rust
pub use entities::{SysRole, SysTenant, SysTenantPackage, SysUser, SysUserTenant};
```

- [ ] **Step 4: Compile-check**

```bash
cd server-rs && cargo check -p modules 2>&1 | tail -10
```

Expected: clean.

- [ ] **Step 5: Report Task 2 complete**

---

### Task 3: Tenant Package DTO + Service + Handler

**Files:**
- Create: `crates/modules/src/system/tenant_package/mod.rs`
- Create: `crates/modules/src/system/tenant_package/dto.rs`
- Create: `crates/modules/src/system/tenant_package/service.rs`
- Create: `crates/modules/src/system/tenant_package/handler.rs`
- Modify: `crates/modules/src/system/mod.rs`
- Modify: `crates/modules/src/lib.rs`

Wires up the 6 tenant-package endpoints. This is self-contained — no cross-module deps.

- [ ] **Step 1: Create mod.rs**

Create `crates/modules/src/system/tenant_package/mod.rs`:

```rust
//! Tenant package module — CRUD for sys_tenant_package.

pub mod dto;
pub mod handler;
pub mod service;

pub use handler::router;
```

- [ ] **Step 2: Create dto.rs**

Create `crates/modules/src/system/tenant_package/dto.rs`:

```rust
//! Tenant package DTOs.

use crate::domain::SysTenantPackage;
use crate::domain::validators::{default_status, validate_status_flag};
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageDetailResponseDto {
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

impl PackageDetailResponseDto {
    pub fn from_entity(p: SysTenantPackage) -> Self {
        Self {
            package_id: p.package_id,
            code: p.code,
            package_name: p.package_name,
            menu_ids: p.menu_ids,
            menu_check_strictly: p.menu_check_strictly,
            status: p.status,
            create_by: p.create_by,
            create_at: fmt_ts(&p.create_at),
            update_by: p.update_by,
            update_at: fmt_ts(&p.update_at),
            remark: p.remark,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageListItemResponseDto {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
    pub status: String,
    pub create_at: String,
}

impl PackageListItemResponseDto {
    pub fn from_entity(p: SysTenantPackage) -> Self {
        Self {
            package_id: p.package_id,
            code: p.code,
            package_name: p.package_name,
            status: p.status,
            create_at: fmt_ts(&p.create_at),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageOptionResponseDto {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
}

impl PackageOptionResponseDto {
    pub fn from_entity(p: SysTenantPackage) -> Self {
        Self {
            package_id: p.package_id,
            code: p.code,
            package_name: p.package_name,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreatePackageDto {
    #[validate(length(min = 1, max = 20))]
    pub code: String,
    #[validate(length(min = 1, max = 50))]
    pub package_name: String,
    #[serde(default)]
    pub menu_ids: Vec<String>,
    #[serde(default)]
    pub menu_check_strictly: bool,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePackageDto {
    pub package_id: String,
    #[validate(length(min = 1, max = 20))]
    pub code: Option<String>,
    #[validate(length(min = 1, max = 50))]
    pub package_name: Option<String>,
    pub menu_ids: Option<Vec<String>>,
    pub menu_check_strictly: Option<bool>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListPackageDto {
    pub package_name: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
```

- [ ] **Step 3: Create service.rs**

Create `crates/modules/src/system/tenant_package/service.rs`:

```rust
//! Tenant package service — business orchestration.

use super::dto::{
    CreatePackageDto, ListPackageDto, PackageDetailResponseDto,
    PackageListItemResponseDto, PackageOptionResponseDto, UpdatePackageDto,
};
use crate::domain::{
    PackageInsertParams, PackageListFilter, PackageUpdateParams, TenantPackageRepo,
};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckBool, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(package_id = %package_id))]
pub async fn find_by_id(
    state: &AppState,
    package_id: &str,
) -> Result<PackageDetailResponseDto, AppError> {
    let pkg = TenantPackageRepo::find_by_id(&state.pg, package_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::TENANT_PACKAGE_NOT_FOUND)?;
    Ok(PackageDetailResponseDto::from_entity(pkg))
}

#[tracing::instrument(skip_all, fields(
    has_name = query.package_name.is_some(),
    page_num = query.page.page_num,
    page_size = query.page.page_size,
))]
pub async fn list(
    state: &AppState,
    query: ListPackageDto,
) -> Result<Page<PackageListItemResponseDto>, AppError> {
    let page = TenantPackageRepo::find_page(
        &state.pg,
        PackageListFilter {
            package_name: query.package_name,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(PackageListItemResponseDto::from_entity))
}

#[tracing::instrument(skip_all)]
pub async fn option_select(
    state: &AppState,
) -> Result<Vec<PackageOptionResponseDto>, AppError> {
    let rows = TenantPackageRepo::find_option_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows.into_iter().map(PackageOptionResponseDto::from_entity).collect())
}

#[tracing::instrument(skip_all, fields(code = %dto.code))]
pub async fn create(
    state: &AppState,
    dto: CreatePackageDto,
) -> Result<PackageDetailResponseDto, AppError> {
    let code_ok = TenantPackageRepo::verify_code_unique(&state.pg, &dto.code, None)
        .await
        .into_internal()?;
    (!code_ok).business_err_if(ResponseCode::TENANT_PACKAGE_CODE_EXISTS)?;

    let name_ok = TenantPackageRepo::verify_name_unique(&state.pg, &dto.package_name, None)
        .await
        .into_internal()?;
    (!name_ok).business_err_if(ResponseCode::TENANT_PACKAGE_NAME_EXISTS)?;

    let pkg = TenantPackageRepo::insert(
        &state.pg,
        PackageInsertParams {
            code: dto.code,
            package_name: dto.package_name,
            menu_ids: dto.menu_ids,
            menu_check_strictly: dto.menu_check_strictly,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(PackageDetailResponseDto::from_entity(pkg))
}

#[tracing::instrument(skip_all, fields(package_id = %dto.package_id))]
pub async fn update(state: &AppState, dto: UpdatePackageDto) -> Result<(), AppError> {
    let existing = TenantPackageRepo::find_by_id(&state.pg, &dto.package_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::TENANT_PACKAGE_NOT_FOUND)?;

    if let Some(ref code) = dto.code {
        if code != &existing.code {
            let ok = TenantPackageRepo::verify_code_unique(
                &state.pg,
                code,
                Some(&dto.package_id),
            )
            .await
            .into_internal()?;
            (!ok).business_err_if(ResponseCode::TENANT_PACKAGE_CODE_EXISTS)?;
        }
    }

    if let Some(ref name) = dto.package_name {
        if name != &existing.package_name {
            let ok = TenantPackageRepo::verify_name_unique(
                &state.pg,
                name,
                Some(&dto.package_id),
            )
            .await
            .into_internal()?;
            (!ok).business_err_if(ResponseCode::TENANT_PACKAGE_NAME_EXISTS)?;
        }
    }

    let affected = TenantPackageRepo::update_by_id(
        &state.pg,
        PackageUpdateParams {
            package_id: dto.package_id,
            code: dto.code,
            package_name: dto.package_name,
            menu_ids: dto.menu_ids,
            menu_check_strictly: dto.menu_check_strictly,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;
    (affected == 0).business_err_if(ResponseCode::TENANT_PACKAGE_NOT_FOUND)
}

#[tracing::instrument(skip_all, fields(path_ids = %path_ids))]
pub async fn remove(state: &AppState, path_ids: &str) -> Result<(), AppError> {
    let ids: Vec<String> = path_ids
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();
    if ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }

    let in_use = TenantPackageRepo::is_any_in_use(&state.pg, &ids)
        .await
        .into_internal()?;
    in_use.business_err_if(ResponseCode::TENANT_PACKAGE_IN_USE)?;

    TenantPackageRepo::soft_delete_by_ids(&state.pg, &ids)
        .await
        .into_internal()?;

    Ok(())
}
```

- [ ] **Step 4: Create handler.rs**

Create `crates/modules/src/system/tenant_package/handler.rs`:

```rust
//! Tenant package HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Router,
};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{require_authenticated, require_permission};

async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::PackageDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListPackageDto>,
) -> Result<ApiResponse<Page<dto::PackageListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::PackageOptionResponseDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreatePackageDto>,
) -> Result<ApiResponse<dto::PackageDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdatePackageDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/tenant-package/",
            post(create).route_layer(require_permission!("system:tenant-package:add")),
        )
        .route(
            "/system/tenant-package/",
            put(update).route_layer(require_permission!("system:tenant-package:edit")),
        )
        .route(
            "/system/tenant-package/list",
            get(list).route_layer(require_permission!("system:tenant-package:list")),
        )
        .route(
            "/system/tenant-package/option-select",
            get(option_select).route_layer(require_authenticated!()),
        )
        .route(
            "/system/tenant-package/{id}",
            get(find_by_id).route_layer(require_permission!("system:tenant-package:query")),
        )
        .route(
            "/system/tenant-package/{ids}",
            delete(remove).route_layer(require_permission!("system:tenant-package:remove")),
        )
}
```

- [ ] **Step 5: Register in system/mod.rs + lib.rs**

Edit `crates/modules/src/system/mod.rs`:

```rust
pub mod role;
pub mod tenant;
pub mod tenant_package;
pub mod user;
```

Edit `crates/modules/src/lib.rs` — add to `api_router()`:

```rust
pub fn api_router() -> Router<AppState> {
    Router::new()
        .merge(auth::router())
        .merge(system::role::router())
        .merge(system::user::router())
        .merge(system::tenant::router())
        .merge(system::tenant_package::router())
}
```

Note: `system::tenant::router()` doesn't exist yet — create a stub mod.rs for now (Task 4 fills it in). Create `crates/modules/src/system/tenant/mod.rs`:

```rust
//! Tenant module — CRUD for sys_tenant.

pub mod dto;
pub mod handler;
pub mod service;

pub use handler::router;
```

And create empty placeholder files that compile:

`crates/modules/src/system/tenant/dto.rs`:
```rust
//! Tenant DTOs — placeholder, filled in Task 4.
```

`crates/modules/src/system/tenant/service.rs`:
```rust
//! Tenant service — placeholder, filled in Task 4.
```

`crates/modules/src/system/tenant/handler.rs`:
```rust
//! Tenant HTTP handlers — placeholder, filled in Task 4.

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
}
```

- [ ] **Step 6: Compile-check**

```bash
cd server-rs && cargo check --workspace 2>&1 | tail -10
```

Expected: clean compile. All 6 tenant-package endpoints wired.

- [ ] **Step 7: Report Task 3 complete**

---

### Task 4: TenantRepo + migration of `insert_user_tenant_binding_tx`

**Files:**
- Create: `crates/modules/src/domain/tenant_repo.rs`
- Modify: `crates/modules/src/domain/user_repo.rs` (remove `insert_user_tenant_binding_tx`)
- Modify: `crates/modules/src/domain/mod.rs` (add re-exports)
- Modify: `crates/modules/src/system/user/service.rs` (update import)

This is the most critical task — it creates the tenant repo AND migrates `sys_user_tenant` write ownership.

- [ ] **Step 1: Create tenant_repo.rs**

Create `crates/modules/src/domain/tenant_repo.rs`. This is a large file (~350 LOC). The implementer must write the full file with all methods from the spec §4.2 table:

Methods to implement:
- `find_by_id(pool, id)` → SELECT + LEFT JOIN sys_tenant_package for package_name
- `find_by_tenant_id(pool, tenant_id)` → SELECT WHERE tenant_id = $1 (existence check)
- `find_page(pool, filter: TenantListFilter)` → paginated, LEFT JOIN package
- `find_admin_user_names(pool, tenant_ids: &[String])` → DISTINCT ON + JOIN sys_user_tenant + sys_user → HashMap
- `find_admin_user_info(pool, tenant_id)` → JOIN sys_user_tenant + sys_user, LIMIT 1
- `find_tenant_ids_with_children(pool, tenant_ids: &[String])` → SELECT DISTINCT parent_id
- `exists_by_company_name_prefix(pool, name, exclude_tenant_id?)` → COUNT WHERE LIKE
- `generate_next_tenant_id(pool)` → SELECT nextval('tenant_id_seq')
- `insert_tx(tx, params: TenantInsertParams)` → INSERT RETURNING
- `update_by_id(pool, params: TenantUpdateParams)` → UPDATE with COALESCE
- `soft_delete_by_ids(pool, ids: &[String])` → batch soft delete
- `insert_user_tenant_binding_tx(tx, user_id, tenant_id, is_admin, is_default)` → **migrated from user_repo**

Params structs: `TenantInsertParams`, `TenantUpdateParams`, `TenantListFilter`.

The `find_page` query must LEFT JOIN `sys_tenant_package p ON t.package_id = p.package_id` to get `package_name`. The entity struct for this JOIN result needs an extra field. Use a local projection struct:

```rust
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TenantWithPackageName {
    // all SysTenant fields...
    pub package_name: Option<String>,
}
```

Key SQL patterns (consistent with existing role_repo/user_repo):
- `($N::varchar IS NULL OR ...)` for optional filters
- `AuditInsert::now()` for create audit fields
- `audit_update_by()` for update audit fields
- `current_tenant_scope()` is NOT used here — tenant management is platform-scoped, not tenant-scoped

- [ ] **Step 2: Remove `insert_user_tenant_binding_tx` from user_repo.rs**

Delete the `insert_user_tenant_binding_tx` method from `crates/modules/src/domain/user_repo.rs`. This includes:
- The method signature and body (~25 lines)
- The doc comment above it

- [ ] **Step 3: Update user/service.rs import**

Edit `crates/modules/src/system/user/service.rs`. Change the call from `UserRepo::insert_user_tenant_binding_tx` to `TenantRepo::insert_user_tenant_binding_tx`, and add `is_default = "1"`, `is_admin = "0"` parameters:

Update import:
```rust
use crate::domain::{RoleRepo, TenantRepo, UserInsertParams, UserListFilter, UserRepo, UserUpdateParams};
```

Update the call site in `create()`:
```rust
    TenantRepo::insert_user_tenant_binding_tx(&mut tx, &user.user_id, "1", "0")
        .await
        .into_internal()?;
```

Note: `insert_user_tenant_binding_tx` now takes `is_default: &str` and `is_admin: &str` as explicit parameters (no longer hardcoded).

- [ ] **Step 4: Register in domain/mod.rs**

```rust
pub mod tenant_repo;
```

And re-exports:
```rust
pub use tenant_repo::{TenantInsertParams, TenantListFilter, TenantRepo, TenantUpdateParams, TenantWithPackageName};
```

- [ ] **Step 5: Compile-check + run existing user tests**

```bash
cd server-rs && cargo check --workspace 2>&1 | tail -10
cd server-rs && cargo test -p modules --test user_module_tests 2>&1 | grep "test result:"
```

Expected: clean compile, 25 user tests still pass (the migration must not break the existing user create flow).

- [ ] **Step 6: Report Task 4 complete**

---

### Task 5: Tenant DTO + Service + Handler

**Files:**
- Modify: `crates/modules/src/system/tenant/dto.rs` (replace placeholder)
- Modify: `crates/modules/src/system/tenant/service.rs` (replace placeholder)
- Modify: `crates/modules/src/system/tenant/handler.rs` (replace placeholder)

The tenant create service is the most complex method — it creates N tenants + 1 user + N bindings in one transaction. The implementer should reference `user/service.rs::create()` for the tx pattern and spec §2.1 for the exact flow.

Key design points for the implementer:
- `create()` accepts `package_ids: Vec<String>` (min 1 element)
- For N > 1 packages: fetch package names, construct company names as `"{base}-{pkg_name}"`
- Generate base tenant_id via `TenantRepo::generate_next_tenant_id()`, then `base + i` for each
- Admin user created ONCE via `UserRepo::insert_tx` with `nick_name = "租户管理员"`
- First tenant binding is `is_default = "1"`, rest are `"0"`; all are `is_admin = "1"`
- Protected tenant check in update: `SUPER_TENANT_ID = "000000"` blocks changes to status/company_name/package_id/expire_time/account_count
- Delete guard: check protected + children before soft delete
- Response DTOs need `from_entity` that takes `TenantWithPackageName` + optional admin info

Handler router wiring — 6 routes matching spec §2.1:
```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/tenant/",
            post(create).route_layer(require_access!{ permission: "system:tenant:add", role: Role::PlatformAdmin }),
        )
        .route(
            "/system/tenant/",
            put(update).route_layer(require_permission!("system:tenant:edit")),
        )
        .route(
            "/system/tenant/list",
            get(list).route_layer(require_access!{ permission: "system:tenant:list", role: Role::SuperAdmin }),
        )
        .route(
            "/system/tenant/{id}",
            get(find_by_id).route_layer(require_permission!("system:tenant:query")),
        )
        .route(
            "/system/tenant/{ids}",
            delete(remove).route_layer(require_access!{ permission: "system:tenant:remove", role: Role::PlatformAdmin }),
        )
}
```

The implementer should write the complete code for all three files. Total expected LOC: dto.rs ~200, service.rs ~250, handler.rs ~80.

- [ ] **Step 1: Write dto.rs with all request/response DTOs**

DTOs needed: `CreateTenantDto`, `UpdateTenantDto`, `ListTenantDto`, `TenantDetailResponseDto`, `TenantListItemResponseDto`.

- [ ] **Step 2: Write service.rs with all 5 service methods**

Methods: `create`, `find_by_id`, `list`, `update`, `remove`.

- [ ] **Step 3: Write handler.rs with 5 handlers + router**

- [ ] **Step 4: Compile-check**

```bash
cd server-rs && cargo check --workspace 2>&1 | tail -10
```

Expected: clean.

- [ ] **Step 5: Run all existing tests (regression)**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result:"
```

Expected: 198+ passing (all existing tests still pass).

- [ ] **Step 6: Report Task 5 complete**

---

### Task 6: Integration tests — Tenant Package

**Files:**
- Create: `crates/modules/tests/tenant_module_tests.rs`
- Modify: `crates/modules/tests/common/mod.rs` (add cleanup helper)

Start with tenant-package tests because they have no dependency on tenant create (simpler).

- [ ] **Step 1: Add cleanup helper for tenant test data**

Edit `crates/modules/tests/common/mod.rs`. Add:

```rust
pub async fn cleanup_test_tenants(pool: &PgPool, prefix: &str) {
    assert!(!prefix.is_empty(), "cleanup_test_tenants: prefix must not be empty");
    let pattern = format!("{prefix}%");
    // Delete user-tenant bindings for test tenants
    sqlx::query(
        "DELETE FROM sys_user_tenant WHERE tenant_id IN \
         (SELECT tenant_id FROM sys_tenant WHERE company_name LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_user_tenant for test tenants");
    // Delete test tenants
    sqlx::query("DELETE FROM sys_tenant WHERE company_name LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_tenant");
}

pub async fn cleanup_test_packages(pool: &PgPool, prefix: &str) {
    assert!(!prefix.is_empty(), "cleanup_test_packages: prefix must not be empty");
    let pattern = format!("{prefix}%");
    sqlx::query("DELETE FROM sys_tenant_package WHERE code LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_tenant_package");
}
```

- [ ] **Step 2: Create test file with ~14 tenant-package tests**

Create `crates/modules/tests/tenant_module_tests.rs` with tests covering:
1. Create package returns detail with menu_ids
2. Create package with duplicate code → 4021
3. Create package with duplicate name → 4022
4. List packages with filter
5. Get package detail with menu_ids array
6. Update package changes menu_ids
7. Update package name to duplicate → 4022
8. Delete package succeeds when not in use
9. Delete package in use → 4023 (create a tenant first referencing the package)
10. Option-select returns active only

All tests should use `as_super_admin(async { ... }).await` wrapper and have cleanup on `Drop` or in a cleanup block.

- [ ] **Step 3: Run tenant package tests**

```bash
cd server-rs && cargo test -p modules --test tenant_module_tests 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 4: Report Task 6 complete**

---

### Task 7: Integration tests — Tenant CRUD

**Files:**
- Modify: `crates/modules/tests/tenant_module_tests.rs`

Add ~20 tenant-specific tests to the existing test file.

- [ ] **Step 1: Add tenant CRUD tests**

Tests covering:
1. Create single-package tenant — verify tenant_id generated, admin user created
2. Create multi-package tenant — verify N tenants created, 1 user, N bindings
3. Create multi-package — verify company names `{base}-{pkgName}`
4. Create multi-package — verify first binding is_default='1', others '0'
5. Create tenant with invalid package_id → 4020
6. Create tenant with duplicate company_name → 4013
7. Create tenant with duplicate username → 1002
8. Create tenant with non-existent parent_id → 4014
9. List tenants returns admin_user_name
10. List tenants filters by company_name / status
11. Get tenant detail includes admin user info
12. Get tenant with non-existent id → 1001
13. Update tenant changes mutable fields
14. Update protected tenant's status → 4010
15. Update tenant company_name to duplicate → 4013
16. Delete tenant soft-deletes
17. Delete protected tenant → 4010
18. Delete tenant with children → 4016

- [ ] **Step 2: Run all tenant tests**

```bash
cd server-rs && cargo test -p modules --test tenant_module_tests 2>&1 | tail -30
```

Expected: all tests pass.

- [ ] **Step 3: Run full workspace tests (regression)**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result:"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cd server-rs && cargo fmt --check && echo "fmt ok"
```

Expected: 198 + ~34 new = ~232 passing, clippy clean, fmt ok.

- [ ] **Step 4: Report Task 7 complete**

---

### Task 8: Smoke script + final verify

**Files:**
- Create: `scripts/smoke-tenant-module.sh`

- [ ] **Step 1: Write smoke script**

Full lifecycle smoke: login → create package → create tenant (single-package) → list → detail → update → create child tenant → delete parent (blocked) → delete child → delete parent → delete package (blocked while tenant exists) → verify cleanup.

The smoke script should follow the same pattern as `scripts/smoke-role-module.sh` and `scripts/smoke-user-module.sh`:
- `BASE="http://127.0.0.1:18080/api/v1"`
- Login as admin, capture token
- Create a test package with prefix `it-smoke-tenant-`
- Create a tenant referencing that package
- Exercise all endpoints
- Cleanup via psql

- [ ] **Step 2: Run smoke**

```bash
cd server-rs
pkill -f target/debug/app 2>/dev/null; sleep 1
cargo build -p app 2>&1 | tail -3
./target/debug/app > /tmp/tea-rs-tenant-smoke.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-tenant-module.sh 2>&1 | tail -5
kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected: ALL STEPS PASSED.

- [ ] **Step 3: Run ALL smoke scripts (regression)**

```bash
./target/debug/app > /tmp/tea-rs-all-smoke.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-role-module.sh 2>&1 | tail -3
bash scripts/smoke-user-module.sh 2>&1 | tail -3
bash scripts/smoke-tenant-module.sh 2>&1 | tail -3
kill $APP_PID 2>/dev/null; wait 2>/dev/null
```

Expected: role 14/14, user 16/16, tenant ALL PASSED.

- [ ] **Step 4: Final workspace verify**

```bash
cd server-rs && cargo test --workspace 2>&1 | grep "test result:"
cd server-rs && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
cd server-rs && cargo fmt --check && echo "fmt ok"
```

- [ ] **Step 5: Report plan complete**

Report:
- All 8 tasks complete
- Workspace tests: 198 → ~232 (+34)
- 12 new endpoints wired (6 tenant + 6 package)
- `sys_user_tenant` ownership migrated to tenant_repo
- Multi-package creation supported
- Smoke: role 14/14, user 16/16, tenant ALL PASSED
- Zero wire contract changes
- Zero new crate dependencies

---

## Post-plan status snapshot

| Metric | Baseline | Target |
|--------|----------|--------|
| Total tests | 198 | ~232 (+34) |
| Tenant endpoints | 0 | 6 |
| Package endpoints | 0 | 6 |
| New domain files | 0 | 2 (tenant_repo + tenant_package_repo) |
| New system modules | 0 | 2 (tenant + tenant_package) |
| Smoke scripts | 2 | 3 |
| Wire contract changes | — | 0 |
| New crate deps | — | 0 |

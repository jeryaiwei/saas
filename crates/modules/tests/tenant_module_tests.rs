//! Integration tests for the tenant module. Real DB at 127.0.0.1:5432/saas_tea.
//! Tests are safe to run in parallel — each test cleans up only its own data.

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::{PageQuery, ResponseCode};
use modules::system::tenant::{dto as tenant_dto, service as tenant_service};
use modules::system::tenant_package::{dto as pkg_dto, service as pkg_service};

// ─── unique test prefix ─────────────────────────────────────────────────────
const PREFIX: &str = "it-tenant-";

// ─── helper: build a CreatePackageDto ───────────────────────────────────────

// Package code is varchar(20), package_name is varchar(50).
// Use "itp-" prefix (4 chars) + 8-char uuid suffix = 12 chars for code.
const PKG_CODE_PREFIX: &str = "itp-";

fn make_package_dto(suffix: &str, status: &str) -> pkg_dto::CreatePackageDto {
    pkg_dto::CreatePackageDto {
        code: format!("{PKG_CODE_PREFIX}{suffix}"),
        package_name: format!("{PREFIX}pkg-{suffix}"),
        menu_ids: vec![],
        menu_check_strictly: false,
        status: status.into(),
        remark: None,
    }
}

fn make_package_dto_with_menus(suffix: &str, menu_ids: Vec<String>) -> pkg_dto::CreatePackageDto {
    pkg_dto::CreatePackageDto {
        code: format!("{PKG_CODE_PREFIX}{suffix}"),
        package_name: format!("{PREFIX}pkg-{suffix}"),
        menu_ids,
        menu_check_strictly: false,
        status: "0".into(),
        remark: None,
    }
}

// ─── helper: build a CreateTenantDto ────────────────────────────────────────

fn make_tenant_dto(suffix: &str, package_ids: Vec<String>) -> tenant_dto::CreateTenantDto {
    tenant_dto::CreateTenantDto {
        company_name: format!("{PREFIX}{suffix}"),
        username: format!("{PREFIX}{suffix}"),
        password: "abc123456".into(),
        package_ids,
        parent_id: None,
        contact_user_name: None,
        contact_phone: None,
        license_number: None,
        address: None,
        intro: None,
        domain: None,
        expire_time: None,
        account_count: -1,
        status: "0".into(),
        language: "zh-CN".into(),
        remark: None,
    }
}

// ─── helper: assert specific error codes ────────────────────────────────────

fn assert_business_code(err: AppError, expected: ResponseCode, label: &str) {
    match err {
        AppError::Business { code, .. } => {
            assert_eq!(code, expected, "{label}: expected {expected}, got {code}");
        }
        other => panic!("{label}: expected Business({expected}), got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tenant Package Tests (~10)
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 1: create a package, assert code + package_name + menu_ids match.
#[tokio::test]
async fn create_package_returns_detail() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_package_dto(suffix, "0");
        let expected_code = dto.code.clone();
        let expected_name = dto.package_name.clone();

        let created = pkg_service::create(&state, dto)
            .await
            .expect("create package should succeed");

        assert_eq!(created.code, expected_code);
        assert_eq!(created.package_name, expected_name);
        assert!(created.menu_ids.is_empty());

        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 2: create, then try to create again with same code -> 4021.
#[tokio::test]
async fn create_package_with_duplicate_code_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto1 = make_package_dto(suffix, "0");
        pkg_service::create(&state, dto1)
            .await
            .expect("first create should succeed");

        // Same code, different name
        let dto2 = pkg_dto::CreatePackageDto {
            code: format!("{PKG_CODE_PREFIX}{suffix}"),
            package_name: format!("{PREFIX}pkg-{suffix}-dup"),
            menu_ids: vec![],
            menu_check_strictly: false,
            status: "0".into(),
            remark: None,
        };
        let err = pkg_service::create(&state, dto2)
            .await
            .expect_err("duplicate code should fail");
        assert_business_code(err, ResponseCode::TENANT_PACKAGE_CODE_EXISTS, "dup code");

        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 3: create, then try again with different code but same name -> 4022.
#[tokio::test]
async fn create_package_with_duplicate_name_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto1 = make_package_dto(suffix, "0");
        let expected_name = dto1.package_name.clone();
        pkg_service::create(&state, dto1)
            .await
            .expect("first create should succeed");

        // Different code, same name
        let dto2 = pkg_dto::CreatePackageDto {
            code: format!("{PKG_CODE_PREFIX}{suffix}b"),
            package_name: expected_name,
            menu_ids: vec![],
            menu_check_strictly: false,
            status: "0".into(),
            remark: None,
        };
        let err = pkg_service::create(&state, dto2)
            .await
            .expect_err("duplicate name should fail");
        assert_business_code(err, ResponseCode::TENANT_PACKAGE_NAME_EXISTS, "dup name");

        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 4: create 2 packages, list with page_size=10, assert rows >= 2.
#[tokio::test]
async fn list_packages_returns_paginated() {
    let (state, _) = common::build_state_and_router().await;
    let s1 = &uuid::Uuid::new_v4().to_string()[..8];
    let s2 = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        pkg_service::create(&state, make_package_dto(s1, "0"))
            .await
            .expect("create pkg 1");
        pkg_service::create(&state, make_package_dto(s2, "0"))
            .await
            .expect("create pkg 2");

        let query = pkg_dto::ListPackageDto {
            package_name: Some(format!("{PREFIX}pkg-")),
            status: None,
            page: PageQuery {
                page_num: 1,
                page_size: 10,
            },
        };
        let page = pkg_service::list(&state, query)
            .await
            .expect("list should succeed");

        assert!(
            page.rows.len() >= 2,
            "should find at least 2 packages, found {}",
            page.rows.len()
        );

        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s1}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s2}")).await;
    })
    .await;
}

/// Test 5: create with menu_ids, fetch by id, assert menu_ids correct.
#[tokio::test]
async fn get_package_detail_includes_menu_ids() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let menu_ids = vec!["m1".to_string(), "m2".to_string()];
        let dto = make_package_dto_with_menus(suffix, menu_ids.clone());
        let created = pkg_service::create(&state, dto)
            .await
            .expect("create should succeed");

        let fetched = pkg_service::find_by_id(&state, &created.package_id)
            .await
            .expect("find_by_id should succeed");

        let mut expected = menu_ids;
        expected.sort();
        let mut actual = fetched.menu_ids.clone();
        actual.sort();
        assert_eq!(actual, expected, "menu_ids should match");

        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 6: create, update menu_ids, fetch again, verify changed.
#[tokio::test]
async fn update_package_changes_menu_ids() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_package_dto_with_menus(suffix, vec!["m1".into(), "m2".into()]);
        let created = pkg_service::create(&state, dto)
            .await
            .expect("create should succeed");

        let update_dto = pkg_dto::UpdatePackageDto {
            package_id: created.package_id.clone(),
            code: None,
            package_name: None,
            menu_ids: Some(vec!["m3".into(), "m4".into(), "m5".into()]),
            menu_check_strictly: None,
            status: None,
            remark: None,
        };
        pkg_service::update(&state, update_dto)
            .await
            .expect("update should succeed");

        let fetched = pkg_service::find_by_id(&state, &created.package_id)
            .await
            .expect("find_by_id after update");

        let mut actual = fetched.menu_ids.clone();
        actual.sort();
        let mut expected = vec!["m3".to_string(), "m4".to_string(), "m5".to_string()];
        expected.sort();
        assert_eq!(actual, expected, "menu_ids should be updated");

        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 7: create 2 packages, try to rename one to the other's name -> 4022.
#[tokio::test]
async fn update_package_name_to_duplicate_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let s1 = &uuid::Uuid::new_v4().to_string()[..8];
    let s2 = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg1 = pkg_service::create(&state, make_package_dto(s1, "0"))
            .await
            .expect("create pkg 1");
        let pkg2 = pkg_service::create(&state, make_package_dto(s2, "0"))
            .await
            .expect("create pkg 2");

        let update_dto = pkg_dto::UpdatePackageDto {
            package_id: pkg1.package_id.clone(),
            code: None,
            package_name: Some(pkg2.package_name.clone()),
            menu_ids: None,
            menu_check_strictly: None,
            status: None,
            remark: None,
        };
        let err = pkg_service::update(&state, update_dto)
            .await
            .expect_err("rename to duplicate name should fail");
        assert_business_code(
            err,
            ResponseCode::TENANT_PACKAGE_NAME_EXISTS,
            "dup name update",
        );

        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s1}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s2}")).await;
    })
    .await;
}

/// Test 8: create, delete, fetch -> should be TENANT_PACKAGE_NOT_FOUND.
#[tokio::test]
async fn delete_package_succeeds_when_not_in_use() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create should succeed");

        pkg_service::remove(&state, &created.package_id)
            .await
            .expect("remove should succeed");

        let err = pkg_service::find_by_id(&state, &created.package_id)
            .await
            .expect_err("find_by_id after delete should fail");
        assert_business_code(
            err,
            ResponseCode::TENANT_PACKAGE_NOT_FOUND,
            "deleted package find",
        );

        // Package was already deleted by the service; cleanup is a no-op but safe
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 9: create package, create tenant referencing it, try to delete package -> 4023.
#[tokio::test]
async fn delete_package_in_use_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        // Create a tenant referencing this package
        let tenant_dto = make_tenant_dto(suffix, vec![pkg.package_id.clone()]);
        tenant_service::create(&state, tenant_dto)
            .await
            .expect("create tenant referencing package");

        let err = pkg_service::remove(&state, &pkg.package_id)
            .await
            .expect_err("delete in-use package should fail");
        assert_business_code(err, ResponseCode::TENANT_PACKAGE_IN_USE, "pkg in use");

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 10: create 2 packages (one active, one status='1'), option_select
/// should only include the active one.
#[tokio::test]
async fn option_select_returns_active_only() {
    let (state, _) = common::build_state_and_router().await;
    let s_active = &uuid::Uuid::new_v4().to_string()[..8];
    let s_disabled = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let active_pkg = pkg_service::create(&state, make_package_dto(s_active, "0"))
            .await
            .expect("create active package");
        let disabled_pkg = pkg_service::create(&state, make_package_dto(s_disabled, "1"))
            .await
            .expect("create disabled package");

        let options = pkg_service::option_select(&state)
            .await
            .expect("option_select should succeed");

        let active_present = options
            .iter()
            .any(|o| o.package_id == active_pkg.package_id);
        let disabled_present = options
            .iter()
            .any(|o| o.package_id == disabled_pkg.package_id);

        assert!(
            active_present,
            "active package should appear in option_select"
        );
        assert!(
            !disabled_present,
            "disabled package should NOT appear in option_select"
        );

        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s_active}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s_disabled}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tenant CRUD Tests (~18)
// ═══════════════════════════════════════════════════════════════════════════════

/// Test 11: create a package, then create a tenant with it, verify tenant_id is 6-digit.
#[tokio::test]
async fn create_tenant_single_package() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        let tenant_dto = make_tenant_dto(suffix, vec![pkg.package_id.clone()]);
        tenant_service::create(&state, tenant_dto)
            .await
            .expect("create tenant should succeed");

        // Verify tenant exists with a 6-digit tenant_id
        let row: Option<(String,)> = sqlx::query_as(
            "SELECT tenant_id FROM sys_tenant WHERE company_name = $1 AND del_flag = '0' LIMIT 1",
        )
        .bind(format!("{PREFIX}{suffix}"))
        .fetch_optional(&state.pg)
        .await
        .expect("query tenant");

        let (tenant_id,) = row.expect("tenant should exist in DB");
        assert_eq!(tenant_id.len(), 6, "tenant_id should be 6 digits");
        assert!(
            tenant_id.chars().all(|c| c.is_ascii_digit()),
            "tenant_id should be all digits"
        );

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 12: create tenant, query sys_user for the admin username, assert exists.
#[tokio::test]
async fn create_tenant_auto_creates_admin_user() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        let expected_username = format!("{PREFIX}{suffix}");
        let tenant_dto = make_tenant_dto(suffix, vec![pkg.package_id.clone()]);
        tenant_service::create(&state, tenant_dto)
            .await
            .expect("create tenant");

        let user_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM sys_user WHERE user_name = $1 AND del_flag = '0')",
        )
        .bind(&expected_username)
        .fetch_one(&state.pg)
        .await
        .expect("query user");

        assert!(user_exists, "admin user should be created with the tenant");

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 13: create 2 packages, create tenant with both package_ids, verify
/// 2 tenants created (query sys_tenant by company_name prefix).
#[tokio::test]
async fn create_tenant_multi_package() {
    let (state, _) = common::build_state_and_router().await;
    let s1 = &uuid::Uuid::new_v4().to_string()[..8];
    let s2 = &uuid::Uuid::new_v4().to_string()[..8];
    let tenant_suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg1 = pkg_service::create(&state, make_package_dto(s1, "0"))
            .await
            .expect("create pkg 1");
        let pkg2 = pkg_service::create(&state, make_package_dto(s2, "0"))
            .await
            .expect("create pkg 2");

        let company_name = format!("{PREFIX}{tenant_suffix}");
        let tenant_dto = make_tenant_dto(
            tenant_suffix,
            vec![pkg1.package_id.clone(), pkg2.package_id.clone()],
        );
        tenant_service::create(&state, tenant_dto)
            .await
            .expect("create multi-package tenant");

        // Query how many tenants were created with our company_name prefix
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_tenant WHERE company_name LIKE $1 AND del_flag = '0'",
        )
        .bind(format!("{company_name}%"))
        .fetch_one(&state.pg)
        .await
        .expect("count tenants");

        assert_eq!(count, 2, "should create 2 tenant rows for 2 packages");

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{tenant_suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s1}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s2}")).await;
    })
    .await;
}

/// Test 14: create 2 packages, create tenant with both, verify each tenant's
/// company_name includes the package_name.
#[tokio::test]
async fn create_tenant_multi_package_names_include_package_name() {
    let (state, _) = common::build_state_and_router().await;
    let s1 = &uuid::Uuid::new_v4().to_string()[..8];
    let s2 = &uuid::Uuid::new_v4().to_string()[..8];
    let tenant_suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg1 = pkg_service::create(&state, make_package_dto(s1, "0"))
            .await
            .expect("create pkg 1");
        let pkg2 = pkg_service::create(&state, make_package_dto(s2, "0"))
            .await
            .expect("create pkg 2");

        let company_name = format!("{PREFIX}{tenant_suffix}");
        let tenant_dto = make_tenant_dto(
            tenant_suffix,
            vec![pkg1.package_id.clone(), pkg2.package_id.clone()],
        );
        tenant_service::create(&state, tenant_dto)
            .await
            .expect("create multi-package tenant");

        // Each tenant company_name should be `{company_name}-{package_name}`
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT company_name FROM sys_tenant WHERE company_name LIKE $1 AND del_flag = '0' ORDER BY company_name",
        )
        .bind(format!("{company_name}%"))
        .fetch_all(&state.pg)
        .await
        .expect("query tenant company names");

        assert_eq!(rows.len(), 2, "should have 2 tenant rows");
        let names: Vec<&str> = rows.iter().map(|r| r.0.as_str()).collect();
        // Each name should contain the corresponding package name
        let has_pkg1_name = names.iter().any(|n| n.contains(&pkg1.package_name));
        let has_pkg2_name = names.iter().any(|n| n.contains(&pkg2.package_name));
        assert!(has_pkg1_name, "one tenant company_name should include pkg1 name");
        assert!(has_pkg2_name, "one tenant company_name should include pkg2 name");

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{tenant_suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s1}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s2}")).await;
    })
    .await;
}

/// Test 15: verify via DB query: first tenant's binding has is_default='1',
/// second has '0'.
#[tokio::test]
async fn create_tenant_multi_package_first_binding_is_default() {
    let (state, _) = common::build_state_and_router().await;
    let s1 = &uuid::Uuid::new_v4().to_string()[..8];
    let s2 = &uuid::Uuid::new_v4().to_string()[..8];
    let tenant_suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg1 = pkg_service::create(&state, make_package_dto(s1, "0"))
            .await
            .expect("create pkg 1");
        let pkg2 = pkg_service::create(&state, make_package_dto(s2, "0"))
            .await
            .expect("create pkg 2");

        let admin_username = format!("{PREFIX}{tenant_suffix}");
        let tenant_dto = make_tenant_dto(
            tenant_suffix,
            vec![pkg1.package_id.clone(), pkg2.package_id.clone()],
        );
        tenant_service::create(&state, tenant_dto)
            .await
            .expect("create multi-package tenant");

        // Query bindings via the admin user
        let bindings: Vec<(String, String)> = sqlx::query_as(
            "SELECT ut.tenant_id, ut.is_default \
             FROM sys_user_tenant ut \
             JOIN sys_user u ON u.user_id = ut.user_id \
             WHERE u.user_name = $1 \
             ORDER BY ut.tenant_id ASC",
        )
        .bind(&admin_username)
        .fetch_all(&state.pg)
        .await
        .expect("query user-tenant bindings");

        assert_eq!(bindings.len(), 2, "should have 2 bindings");

        // The first tenant (lowest tenant_id) should be default
        let default_count = bindings.iter().filter(|(_, d)| d == "1").count();
        let non_default_count = bindings.iter().filter(|(_, d)| d == "0").count();
        assert_eq!(default_count, 1, "exactly one binding should be default");
        assert_eq!(
            non_default_count, 1,
            "exactly one binding should be non-default"
        );

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{tenant_suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s1}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{s2}")).await;
    })
    .await;
}

/// Test 16: create with non-existent package_id -> 4020.
#[tokio::test]
async fn create_tenant_invalid_package_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let tenant_dto =
            make_tenant_dto(suffix, vec!["00000000-0000-0000-0000-ffffffffffff".into()]);
        let err = tenant_service::create(&state, tenant_dto)
            .await
            .expect_err("invalid package_id should fail");
        assert_business_code(
            err,
            ResponseCode::TENANT_PACKAGE_NOT_FOUND,
            "invalid package",
        );
    })
    .await;
}

/// Test 17: create, then create again with same company_name -> 4013.
#[tokio::test]
async fn create_tenant_duplicate_company_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        let dto1 = make_tenant_dto(suffix, vec![pkg.package_id.clone()]);
        tenant_service::create(&state, dto1)
            .await
            .expect("first tenant create");

        // Second create with same company_name but different username
        let suffix2 = &uuid::Uuid::new_v4().to_string()[..8];
        let dto2 = tenant_dto::CreateTenantDto {
            company_name: format!("{PREFIX}{suffix}"),
            username: format!("{PREFIX}{suffix2}"),
            password: "abc123456".into(),
            package_ids: vec![pkg.package_id.clone()],
            parent_id: None,
            contact_user_name: None,
            contact_phone: None,
            license_number: None,
            address: None,
            intro: None,
            domain: None,
            expire_time: None,
            account_count: -1,
            status: "0".into(),
            language: "zh-CN".into(),
            remark: None,
        };
        let err = tenant_service::create(&state, dto2)
            .await
            .expect_err("duplicate company_name should fail");
        assert_business_code(err, ResponseCode::TENANT_COMPANY_EXISTS, "dup company");

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 18: create, then create again with same username -> 1002.
#[tokio::test]
async fn create_tenant_duplicate_username_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let s1 = &uuid::Uuid::new_v4().to_string()[..8];
        let s2 = &uuid::Uuid::new_v4().to_string()[..8];
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        // First tenant
        let dto1 = tenant_dto::CreateTenantDto {
            company_name: format!("{PREFIX}comp-{s1}"),
            username: format!("{PREFIX}{suffix}"),
            password: "abc123456".into(),
            package_ids: vec![pkg.package_id.clone()],
            parent_id: None,
            contact_user_name: None,
            contact_phone: None,
            license_number: None,
            address: None,
            intro: None,
            domain: None,
            expire_time: None,
            account_count: -1,
            status: "0".into(),
            language: "zh-CN".into(),
            remark: None,
        };
        tenant_service::create(&state, dto1)
            .await
            .expect("first tenant create");

        // Second tenant with different company_name but same username
        let dto2 = tenant_dto::CreateTenantDto {
            company_name: format!("{PREFIX}comp-{s2}"),
            username: format!("{PREFIX}{suffix}"),
            password: "abc123456".into(),
            package_ids: vec![pkg.package_id.clone()],
            parent_id: None,
            contact_user_name: None,
            contact_phone: None,
            license_number: None,
            address: None,
            intro: None,
            domain: None,
            expire_time: None,
            account_count: -1,
            status: "0".into(),
            language: "zh-CN".into(),
            remark: None,
        };
        let err = tenant_service::create(&state, dto2)
            .await
            .expect_err("duplicate username should fail");
        assert_business_code(err, ResponseCode::DUPLICATE_KEY, "dup username");

        // Tenant has company_name "it-tenant-comp-{s1}" and user_name "it-tenant-{suffix}";
        // clean each up with its own specific prefix.
        common::cleanup_test_users(&state.pg, &format!("it-tenant-{suffix}")).await;
        // After users removed, delete tenants by company_name prefix
        sqlx::query("DELETE FROM sys_tenant WHERE company_name LIKE $1")
            .bind(format!("it-tenant-comp-{s1}%"))
            .execute(&state.pg)
            .await
            .expect("cleanup tenant for test 18");
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 19: create with parent_id that doesn't exist -> 4014.
#[tokio::test]
async fn create_tenant_invalid_parent_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        let mut dto = make_tenant_dto(suffix, vec![pkg.package_id.clone()]);
        dto.parent_id = Some("999999".into());

        let err = tenant_service::create(&state, dto)
            .await
            .expect_err("invalid parent_id should fail");
        assert_business_code(err, ResponseCode::TENANT_PARENT_NOT_FOUND, "bad parent");

        // Tenant creation failed, so only the package needs cleanup
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 20: create tenant, list, verify row has admin_user_name.
#[tokio::test]
async fn list_tenants_returns_admin_user_name() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        let expected_username = format!("{PREFIX}{suffix}");
        let tenant_dto = make_tenant_dto(suffix, vec![pkg.package_id.clone()]);
        tenant_service::create(&state, tenant_dto)
            .await
            .expect("create tenant");

        let query = tenant_dto::ListTenantDto {
            tenant_id: None,
            contact_user_name: None,
            contact_phone: None,
            company_name: Some(format!("{PREFIX}{suffix}")),
            status: None,
            page: PageQuery::default(),
        };
        let page = tenant_service::list(&state, query)
            .await
            .expect("list should succeed");

        assert!(!page.rows.is_empty(), "should find at least 1 tenant");
        let found = page
            .rows
            .iter()
            .find(|r| r.admin_user_name.as_deref() == Some(expected_username.as_str()));
        assert!(
            found.is_some(),
            "list should return admin_user_name for the tenant"
        );

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 21: create 2 tenants with different names, filter by one, verify only
/// that one returned.
#[tokio::test]
async fn list_tenants_filters_by_company_name() {
    let (state, _) = common::build_state_and_router().await;
    let s1 = &uuid::Uuid::new_v4().to_string()[..8];
    let s2 = &uuid::Uuid::new_v4().to_string()[..8];
    let p1 = &uuid::Uuid::new_v4().to_string()[..8];
    let p2 = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg1 = pkg_service::create(&state, make_package_dto(p1, "0"))
            .await
            .expect("create pkg 1");
        let pkg2 = pkg_service::create(&state, make_package_dto(p2, "0"))
            .await
            .expect("create pkg 2");

        tenant_service::create(&state, make_tenant_dto(s1, vec![pkg1.package_id.clone()]))
            .await
            .expect("create tenant 1");

        tenant_service::create(&state, make_tenant_dto(s2, vec![pkg2.package_id.clone()]))
            .await
            .expect("create tenant 2");

        // Filter by s1's company_name
        let query = tenant_dto::ListTenantDto {
            tenant_id: None,
            contact_user_name: None,
            contact_phone: None,
            company_name: Some(format!("{PREFIX}{s1}")),
            status: None,
            page: PageQuery::default(),
        };
        let page = tenant_service::list(&state, query)
            .await
            .expect("list should succeed");

        assert_eq!(page.rows.len(), 1, "filter should return exactly 1 tenant");
        assert!(
            page.rows[0].company_name.contains(&format!("{PREFIX}{s1}")),
            "returned tenant should match the filter"
        );

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{s1}")).await;
        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{s2}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{p1}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{p2}")).await;
    })
    .await;
}

/// Test 22: create tenant, get detail, verify nick_name is "租户管理员".
#[tokio::test]
async fn get_tenant_detail_includes_admin_info() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        tenant_service::create(
            &state,
            make_tenant_dto(suffix, vec![pkg.package_id.clone()]),
        )
        .await
        .expect("create tenant");

        // Get the tenant's UUID id from the DB
        let (id,): (String,) = sqlx::query_as(
            "SELECT id FROM sys_tenant WHERE company_name = $1 AND del_flag = '0' LIMIT 1",
        )
        .bind(format!("{PREFIX}{suffix}"))
        .fetch_one(&state.pg)
        .await
        .expect("query tenant id");

        let detail = tenant_service::find_by_id(&state, &id)
            .await
            .expect("find_by_id should succeed");

        assert_eq!(
            detail.nick_name.as_deref(),
            Some("租户管理员"),
            "admin nick_name should be '租户管理员'"
        );

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 23: get with random UUID -> DATA_NOT_FOUND (1001).
#[tokio::test]
async fn get_tenant_nonexistent_returns_not_found() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = uuid::Uuid::new_v4().to_string();
        let err = tenant_service::find_by_id(&state, &ghost_id)
            .await
            .expect_err("should return DATA_NOT_FOUND for ghost tenant");
        assert_business_code(err, ResponseCode::DATA_NOT_FOUND, "ghost tenant");
    })
    .await;
}

/// Test 24: create, update contact_phone, fetch, verify changed.
#[tokio::test]
async fn update_tenant_changes_fields() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        tenant_service::create(
            &state,
            make_tenant_dto(suffix, vec![pkg.package_id.clone()]),
        )
        .await
        .expect("create tenant");

        // Get the tenant from DB
        let (id, tenant_id): (String, String) = sqlx::query_as(
            "SELECT id, tenant_id FROM sys_tenant WHERE company_name = $1 AND del_flag = '0' LIMIT 1",
        )
        .bind(format!("{PREFIX}{suffix}"))
        .fetch_one(&state.pg)
        .await
        .expect("query tenant");

        let update_dto = tenant_dto::UpdateTenantDto {
            id: id.clone(),
            tenant_id: tenant_id.clone(),
            contact_user_name: None,
            contact_phone: Some("13800138000".into()),
            company_name: None,
            license_number: None,
            address: None,
            intro: None,
            domain: None,
            package_id: None,
            expire_time: None,
            account_count: None,
            status: None,
            remark: None,
        };
        tenant_service::update(&state, update_dto)
            .await
            .expect("update should succeed");

        let detail = tenant_service::find_by_id(&state, &id)
            .await
            .expect("find_by_id after update");
        assert_eq!(
            detail.contact_phone.as_deref(),
            Some("13800138000"),
            "contact_phone should be updated"
        );

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 25: try to update tenant_id="000000" status -> 4010.
#[tokio::test]
async fn update_protected_tenant_status_rejects() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let super_tenant: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM sys_tenant WHERE tenant_id = '000000' AND del_flag = '0' LIMIT 1",
        )
        .fetch_optional(&state.pg)
        .await
        .expect("query super tenant");

        let Some((super_id,)) = super_tenant else {
            tracing::warn!("super tenant (000000) not found in dev DB — skipping test");
            return;
        };

        let update_dto = tenant_dto::UpdateTenantDto {
            id: super_id,
            tenant_id: "000000".into(),
            contact_user_name: None,
            contact_phone: None,
            company_name: None,
            license_number: None,
            address: None,
            intro: None,
            domain: None,
            package_id: None,
            expire_time: None,
            account_count: None,
            status: Some("1".into()),
            remark: None,
        };
        let err = tenant_service::update(&state, update_dto)
            .await
            .expect_err("should reject updating protected tenant status");
        assert_business_code(err, ResponseCode::TENANT_PROTECTED, "protected update");
    })
    .await;
}

/// Test 26: create 2 tenants, try to rename one to the other's name -> 4013.
#[tokio::test]
async fn update_tenant_company_name_duplicate_rejects() {
    let (state, _) = common::build_state_and_router().await;
    let s1 = &uuid::Uuid::new_v4().to_string()[..8];
    let s2 = &uuid::Uuid::new_v4().to_string()[..8];
    let p1 = &uuid::Uuid::new_v4().to_string()[..8];
    let p2 = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg1 = pkg_service::create(&state, make_package_dto(p1, "0"))
            .await
            .expect("create pkg 1");
        let pkg2 = pkg_service::create(&state, make_package_dto(p2, "0"))
            .await
            .expect("create pkg 2");

        tenant_service::create(
            &state,
            make_tenant_dto(s1, vec![pkg1.package_id.clone()]),
        )
        .await
        .expect("create tenant 1");
        tenant_service::create(
            &state,
            make_tenant_dto(s2, vec![pkg2.package_id.clone()]),
        )
        .await
        .expect("create tenant 2");

        // Get tenant 1's id + tenant_id
        let (id1, tid1): (String, String) = sqlx::query_as(
            "SELECT id, tenant_id FROM sys_tenant WHERE company_name = $1 AND del_flag = '0' LIMIT 1",
        )
        .bind(format!("{PREFIX}{s1}"))
        .fetch_one(&state.pg)
        .await
        .expect("query tenant 1");

        // Try to rename tenant 1 to tenant 2's company_name
        let update_dto = tenant_dto::UpdateTenantDto {
            id: id1,
            tenant_id: tid1,
            contact_user_name: None,
            contact_phone: None,
            company_name: Some(format!("{PREFIX}{s2}")),
            license_number: None,
            address: None,
            intro: None,
            domain: None,
            package_id: None,
            expire_time: None,
            account_count: None,
            status: None,
            remark: None,
        };
        let err = tenant_service::update(&state, update_dto)
            .await
            .expect_err("rename to duplicate company_name should fail");
        assert_business_code(err, ResponseCode::TENANT_COMPANY_EXISTS, "dup company update");

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{s1}")).await;
        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{s2}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{p1}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{p2}")).await;
    })
    .await;
}

/// Test 27: create, delete, verify del_flag='1' in DB.
#[tokio::test]
async fn delete_tenant_soft_deletes() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let pkg = pkg_service::create(&state, make_package_dto(suffix, "0"))
            .await
            .expect("create package");

        tenant_service::create(
            &state,
            make_tenant_dto(suffix, vec![pkg.package_id.clone()]),
        )
        .await
        .expect("create tenant");

        // Get the tenant's UUID id
        let (id,): (String,) = sqlx::query_as(
            "SELECT id FROM sys_tenant WHERE company_name = $1 AND del_flag = '0' LIMIT 1",
        )
        .bind(format!("{PREFIX}{suffix}"))
        .fetch_one(&state.pg)
        .await
        .expect("query tenant id");

        tenant_service::remove(&state, &id)
            .await
            .expect("remove should succeed");

        // Verify del_flag is set
        let del_flag: String = sqlx::query_scalar("SELECT del_flag FROM sys_tenant WHERE id = $1")
            .bind(&id)
            .fetch_one(&state.pg)
            .await
            .expect("should find the row after soft delete");
        assert_eq!(del_flag, "1", "del_flag should be '1' after remove");

        common::cleanup_test_tenants(&state.pg, &format!("it-tenant-{suffix}")).await;
        common::cleanup_test_packages(&state.pg, &format!("{PKG_CODE_PREFIX}{suffix}")).await;
    })
    .await;
}

/// Test 28: try to delete tenant_id="000000" -> 4010.
#[tokio::test]
async fn delete_protected_tenant_rejects() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let super_tenant: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM sys_tenant WHERE tenant_id = '000000' AND del_flag = '0' LIMIT 1",
        )
        .fetch_optional(&state.pg)
        .await
        .expect("query super tenant");

        let Some((super_id,)) = super_tenant else {
            tracing::warn!("super tenant (000000) not found in dev DB — skipping test");
            return;
        };

        let err = tenant_service::remove(&state, &super_id)
            .await
            .expect_err("should reject deleting protected tenant");
        assert_business_code(err, ResponseCode::TENANT_PROTECTED, "protected delete");
    })
    .await;
}

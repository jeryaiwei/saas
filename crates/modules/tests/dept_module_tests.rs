//! Integration tests for the dept module. Real DB at 127.0.0.1:5432/saas_tea.
//! Tests are safe to run in parallel — each test uses a unique suffix and
//! cleans up only its own data (suffix-scoped prefix).

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::ResponseCode;
use modules::system::dept::{dto as dept_dto, service as dept_service};

// ─── unique test prefix ─────────────────────────────────────────────────────
const PREFIX: &str = "it-dept-";

// ─── helper: build a CreateDeptDto ──────────────────────────────────────────

fn make_create_dto(suffix: &str, parent_id: &str) -> dept_dto::CreateDeptDto {
    dept_dto::CreateDeptDto {
        parent_id: parent_id.into(),
        dept_name: format!("{PREFIX}{suffix}"),
        order_num: 1,
        leader: None,
        phone: None,
        email: None,
        status: "0".into(),
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
// 1. create_dept_root — parent_id="0", verify ancestors contains "0"
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_dept_root() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "0");
        let resp = dept_service::create(&state, dto)
            .await
            .expect("create root dept");

        assert!(
            resp.ancestors.contains(&"0".to_string()),
            "root dept ancestors should contain '0'"
        );

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. create_dept_with_parent — root then child, verify child ancestors
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_dept_with_parent() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create root
        let root_dto = make_create_dto(suffix, "0");
        let root = dept_service::create(&state, root_dto)
            .await
            .expect("create root");

        // Create child
        let child_suffix = format!("{suffix}-ch");
        let child_dto = make_create_dto(&child_suffix, &root.dept_id);
        let child = dept_service::create(&state, child_dto)
            .await
            .expect("create child");

        assert!(
            child.ancestors.contains(&root.dept_id),
            "child ancestors should include root dept_id"
        );
        assert!(
            child.ancestors.contains(&"0".to_string()),
            "child ancestors should also include '0' (root of root)"
        );

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. create_dept_parent_not_found — non-existent parent → 7014
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_dept_parent_not_found() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let fake_parent = uuid::Uuid::new_v4().to_string();
        let dto = make_create_dto(suffix, &fake_parent);
        let err = dept_service::create(&state, dto)
            .await
            .expect_err("should fail for non-existent parent");

        assert_business_code(
            err,
            ResponseCode::DEPT_PARENT_NOT_FOUND,
            "create_dept_parent_not_found",
        );

        // Nothing was created, but clean up just in case
        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. create_dept_nesting_too_deep — ancestors > 2000 → 7015
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_dept_nesting_too_deep() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Insert a dept via raw SQL with 2001-element ancestors array
        let deep_dept_id = uuid::Uuid::new_v4().to_string();
        let deep_ancestors: Vec<String> = (0..2001).map(|i| format!("a{i}")).collect();

        sqlx::query(
            "INSERT INTO sys_dept (\
                dept_id, tenant_id, parent_id, ancestors, dept_name, order_num, \
                leader, phone, email, status, del_flag, create_by, update_by, update_at\
            ) VALUES ($1, '000000', '0', $2, $3, 0, '', '', '', '0', '0', '', '', CURRENT_TIMESTAMP)",
        )
        .bind(&deep_dept_id)
        .bind(&deep_ancestors)
        .bind(format!("{PREFIX}{suffix}-deep"))
        .execute(&state.pg)
        .await
        .expect("insert deep dept");

        // Try to create child under the deep dept
        let child_dto = make_create_dto(&format!("{suffix}-nested"), &deep_dept_id);
        let err = dept_service::create(&state, child_dto)
            .await
            .expect_err("should fail for too-deep nesting");

        assert_business_code(
            err,
            ResponseCode::DEPT_NESTING_TOO_DEEP,
            "create_dept_nesting_too_deep",
        );

        // Cleanup: remove the raw-inserted dept and any test rows
        sqlx::query("DELETE FROM sys_dept WHERE dept_id = $1")
            .bind(&deep_dept_id)
            .execute(&state.pg)
            .await
            .expect("cleanup deep dept");
        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. list_depts_returns_flat — create 2 depts, list, assert count >= 2
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_depts_returns_flat() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto1 = make_create_dto(&format!("{suffix}-a"), "0");
        let dto2 = make_create_dto(&format!("{suffix}-b"), "0");
        dept_service::create(&state, dto1)
            .await
            .expect("create dept1");
        dept_service::create(&state, dto2)
            .await
            .expect("create dept2");

        let list = dept_service::list(
            &state,
            dept_dto::ListDeptDto {
                dept_name: None,
                status: None,
            },
        )
        .await
        .expect("list depts");
        assert!(list.len() >= 2, "list should return at least 2 depts");

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. list_depts_filters_by_name — create with unique name, filter, assert match
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_depts_filters_by_name() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "0");
        let expected_name = dto.dept_name.clone();
        dept_service::create(&state, dto)
            .await
            .expect("create dept");

        let list = dept_service::list(
            &state,
            dept_dto::ListDeptDto {
                dept_name: Some(expected_name.clone()),
                status: None,
            },
        )
        .await
        .expect("list filtered");

        assert!(
            list.iter().any(|d| d.dept_name == expected_name),
            "filtered list should contain dept with the unique name"
        );

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. list_depts_filters_by_status — create disabled, filter active, excluded
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_depts_filters_by_status() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create a dept with status='1' (disabled)
        let mut dto = make_create_dto(suffix, "0");
        dto.status = "1".into();
        let created = dept_service::create(&state, dto)
            .await
            .expect("create disabled dept");

        // List with filter status='0' — the created dept should not appear
        let list = dept_service::list(
            &state,
            dept_dto::ListDeptDto {
                dept_name: Some(created.dept_name.clone()),
                status: Some("0".into()),
            },
        )
        .await
        .expect("list filtered");

        assert!(
            !list.iter().any(|d| d.dept_id == created.dept_id),
            "dept with status=1 should be excluded when filtering by status=0"
        );

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. get_dept_detail — create, fetch by id, verify fields + ancestors
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn get_dept_detail() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "0");
        let expected_name = dto.dept_name.clone();
        let created = dept_service::create(&state, dto)
            .await
            .expect("create dept");

        let detail = dept_service::find_by_id(&state, &created.dept_id)
            .await
            .expect("find by id");

        assert_eq!(detail.dept_id, created.dept_id, "dept_id should match");
        assert_eq!(detail.dept_name, expected_name, "dept_name should match");
        assert!(
            detail.ancestors.contains(&"0".to_string()),
            "ancestors should contain '0'"
        );

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. get_dept_nonexistent — random UUID → 7010
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn get_dept_nonexistent() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let fake_id = uuid::Uuid::new_v4().to_string();
        let err = dept_service::find_by_id(&state, &fake_id)
            .await
            .expect_err("should fail for nonexistent dept");

        assert_business_code(err, ResponseCode::DEPT_NOT_FOUND, "get_dept_nonexistent");
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. update_dept_changes_fields — create, update dept_name, verify changed
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn update_dept_changes_fields() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "0");
        let created = dept_service::create(&state, dto)
            .await
            .expect("create dept");

        let new_name = format!("{PREFIX}{suffix}-updated");
        dept_service::update(
            &state,
            dept_dto::UpdateDeptDto {
                dept_id: created.dept_id.clone(),
                parent_id: "0".into(),
                dept_name: Some(new_name.clone()),
                order_num: None,
                leader: None,
                phone: None,
                email: None,
                status: None,
                remark: None,
            },
        )
        .await
        .expect("update dept");

        let updated = dept_service::find_by_id(&state, &created.dept_id)
            .await
            .expect("find updated dept");
        assert_eq!(updated.dept_name, new_name, "name should be updated");

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. update_dept_reparent_recalculates_ancestors
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn update_dept_reparent_recalculates_ancestors() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create root R1 and child C1
        let r1_dto = make_create_dto(&format!("{suffix}-r1"), "0");
        let r1 = dept_service::create(&state, r1_dto)
            .await
            .expect("create R1");

        let c1_dto = make_create_dto(&format!("{suffix}-c1"), &r1.dept_id);
        let c1 = dept_service::create(&state, c1_dto)
            .await
            .expect("create C1");

        // Verify C1 ancestors includes R1
        assert!(
            c1.ancestors.contains(&r1.dept_id),
            "C1 initial ancestors should include R1"
        );

        // Create another root R2
        let r2_dto = make_create_dto(&format!("{suffix}-r2"), "0");
        let r2 = dept_service::create(&state, r2_dto)
            .await
            .expect("create R2");

        // Update C1's parent to R2
        dept_service::update(
            &state,
            dept_dto::UpdateDeptDto {
                dept_id: c1.dept_id.clone(),
                parent_id: r2.dept_id.clone(),
                dept_name: None,
                order_num: None,
                leader: None,
                phone: None,
                email: None,
                status: None,
                remark: None,
            },
        )
        .await
        .expect("reparent C1 to R2");

        // Fetch C1 and verify ancestors now includes R2
        let reparented = dept_service::find_by_id(&state, &c1.dept_id)
            .await
            .expect("find reparented C1");

        assert!(
            reparented.ancestors.contains(&r2.dept_id),
            "reparented C1 ancestors should include R2's dept_id"
        );
        assert!(
            !reparented.ancestors.contains(&r1.dept_id),
            "reparented C1 ancestors should NOT include R1's dept_id"
        );

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. delete_dept_soft_deletes — create, delete, verify del_flag='1'
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn delete_dept_soft_deletes() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "0");
        let created = dept_service::create(&state, dto)
            .await
            .expect("create dept");

        dept_service::remove(&state, &created.dept_id)
            .await
            .expect("remove dept");

        // Verify del_flag='1' via raw SQL (find_by_id filters del_flag='0')
        let row: Option<(String,)> =
            sqlx::query_as("SELECT del_flag FROM sys_dept WHERE dept_id = $1")
                .bind(&created.dept_id)
                .fetch_optional(&state.pg)
                .await
                .expect("raw select del_flag");

        let del_flag = row.expect("dept row should still exist").0;
        assert_eq!(del_flag, "1", "del_flag should be '1' after soft delete");

        // Hard-delete for cleanup (soft-deleted rows won't match name filter)
        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
        sqlx::query("DELETE FROM sys_dept WHERE dept_id = $1")
            .bind(&created.dept_id)
            .execute(&state.pg)
            .await
            .expect("hard cleanup");
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. option_select_returns_active_only — create active + disabled, verify
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn option_select_returns_active_only() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create active dept
        let active_dto = make_create_dto(&format!("{suffix}-act"), "0");
        let active = dept_service::create(&state, active_dto)
            .await
            .expect("create active dept");

        // Create disabled dept
        let mut disabled_dto = make_create_dto(&format!("{suffix}-dis"), "0");
        disabled_dto.status = "1".into();
        let disabled = dept_service::create(&state, disabled_dto)
            .await
            .expect("create disabled dept");

        let options = dept_service::option_select(&state)
            .await
            .expect("option_select");

        assert!(
            options.iter().any(|d| d.dept_id == active.dept_id),
            "active dept should appear in option_select"
        );
        assert!(
            !options.iter().any(|d| d.dept_id == disabled.dept_id),
            "disabled dept should NOT appear in option_select"
        );

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. exclude_list_excludes_self_and_descendants
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn exclude_list_excludes_self_and_descendants() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create root -> child -> grandchild
        let root_dto = make_create_dto(&format!("{suffix}-rt"), "0");
        let root = dept_service::create(&state, root_dto)
            .await
            .expect("create root");

        let child_dto = make_create_dto(&format!("{suffix}-ch"), &root.dept_id);
        let child = dept_service::create(&state, child_dto)
            .await
            .expect("create child");

        let gc_dto = make_create_dto(&format!("{suffix}-gc"), &child.dept_id);
        let grandchild = dept_service::create(&state, gc_dto)
            .await
            .expect("create grandchild");

        // Call exclude_list(root.dept_id)
        let excluded = dept_service::exclude_list(&state, &root.dept_id)
            .await
            .expect("exclude_list");

        // Verify root, child, grandchild are all excluded
        assert!(
            !excluded.iter().any(|d| d.dept_id == root.dept_id),
            "root should be excluded"
        );
        assert!(
            !excluded.iter().any(|d| d.dept_id == child.dept_id),
            "child should be excluded (descendant of root)"
        );
        assert!(
            !excluded.iter().any(|d| d.dept_id == grandchild.dept_id),
            "grandchild should be excluded (descendant of root)"
        );

        common::cleanup_test_depts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

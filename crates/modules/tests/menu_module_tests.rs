//! Integration tests for the menu module. Real DB at 127.0.0.1:5432/saas_tea.
//! Tests are safe to run in parallel — each test uses a unique suffix and
//! cleans up only its own data (suffix-scoped prefix).

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::ResponseCode;
// sqlx is used directly for raw SQL queries in cleanup/verification steps.
use modules::system::menu::{dto as menu_dto, service as menu_service};

// ─── unique test prefix ─────────────────────────────────────────────────────
const PREFIX: &str = "it-menu-";

// ─── helper: build a CreateMenuDto ──────────────────────────────────────────

fn make_create_dto(
    suffix: &str,
    menu_type: &str,
    parent_id: Option<String>,
) -> menu_dto::CreateMenuDto {
    menu_dto::CreateMenuDto {
        menu_name: format!("{PREFIX}{suffix}"),
        parent_id,
        order_num: 1,
        path: format!("/{suffix}"),
        component: if menu_type == "C" {
            Some(format!("system/{suffix}/index"))
        } else {
            None
        },
        query: None,
        is_frame: "1".into(),
        is_cache: "0".into(),
        menu_type: menu_type.into(),
        visible: "0".into(),
        status: "0".into(),
        perms: if menu_type == "F" {
            Some(format!("system:{suffix}:btn"))
        } else {
            None
        },
        icon: Some("menu".into()),
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
// 1. create_menu_directory — type M, verify menuType/menuName
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_menu_directory() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "M", None);
        let expected_name = dto.menu_name.clone();
        let resp = menu_service::create(&state, dto).await.expect("create dir");
        assert_eq!(resp.menu_type, "M", "menu_type should be M");
        assert_eq!(resp.menu_name, expected_name, "menu_name should match");

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. create_menu_page — type C, verify component is set
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_menu_page() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "C", None);
        let resp = menu_service::create(&state, dto)
            .await
            .expect("create page");
        assert_eq!(resp.menu_type, "C", "menu_type should be C");
        assert!(
            resp.component.is_some(),
            "component should be set for type C"
        );

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. create_menu_button — type F, verify perms is set
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_menu_button() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "F", None);
        let resp = menu_service::create(&state, dto)
            .await
            .expect("create button");
        assert_eq!(resp.menu_type, "F", "menu_type should be F");
        assert!(!resp.perms.is_empty(), "perms should be set for type F");

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. create_child_menu — parent M then child C, verify parent_id
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_child_menu() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create parent
        let parent_dto = make_create_dto(suffix, "M", None);
        let parent = menu_service::create(&state, parent_dto)
            .await
            .expect("create parent");

        // Create child
        let child_suffix = format!("{suffix}-ch");
        let child_dto = make_create_dto(&child_suffix, "C", Some(parent.menu_id.clone()));
        let child = menu_service::create(&state, child_dto)
            .await
            .expect("create child");

        assert_eq!(
            child.parent_id.as_deref(),
            Some(parent.menu_id.as_str()),
            "child parent_id should match parent menu_id"
        );

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. list_menus_returns_flat — create 2 menus, list all, assert >= 2
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_menus_returns_flat() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto1 = make_create_dto(&format!("{suffix}-a"), "M", None);
        let dto2 = make_create_dto(&format!("{suffix}-b"), "M", None);
        menu_service::create(&state, dto1)
            .await
            .expect("create menu1");
        menu_service::create(&state, dto2)
            .await
            .expect("create menu2");

        let list = menu_service::list(
            &state,
            menu_dto::ListMenuDto {
                menu_name: None,
                status: None,
                parent_id: None,
                menu_type: None,
            },
        )
        .await
        .expect("list menus");
        assert!(list.len() >= 2, "list should return at least 2 menus");

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. list_menus_filters_by_name — filter by unique name
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_menus_filters_by_name() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "M", None);
        let expected_name = dto.menu_name.clone();
        menu_service::create(&state, dto)
            .await
            .expect("create menu");

        let list = menu_service::list(
            &state,
            menu_dto::ListMenuDto {
                menu_name: Some(expected_name.clone()),
                status: None,
                parent_id: None,
                menu_type: None,
            },
        )
        .await
        .expect("list filtered");

        assert!(
            list.iter().any(|m| m.menu_name == expected_name),
            "filtered list should contain menu with the unique name"
        );

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. list_menus_filters_by_status — create status='1', filter status='0'
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_menus_filters_by_status() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create a menu with status='1' (disabled)
        let mut dto = make_create_dto(suffix, "M", None);
        dto.status = "1".into();
        let created = menu_service::create(&state, dto)
            .await
            .expect("create menu");

        // List with filter status='0' — the created menu should not appear
        let list = menu_service::list(
            &state,
            menu_dto::ListMenuDto {
                menu_name: Some(created.menu_name.clone()),
                status: Some("0".into()),
                parent_id: None,
                menu_type: None,
            },
        )
        .await
        .expect("list filtered");

        assert!(
            !list.iter().any(|m| m.menu_id == created.menu_id),
            "menu with status=1 should be excluded when filtering by status=0"
        );

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. list_menus_filters_by_parent_id — parent + child, filter by parent_id
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_menus_filters_by_parent_id() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let parent_dto = make_create_dto(suffix, "M", None);
        let parent = menu_service::create(&state, parent_dto)
            .await
            .expect("create parent");

        let child_suffix = format!("{suffix}-ch");
        let child_dto = make_create_dto(&child_suffix, "C", Some(parent.menu_id.clone()));
        let child = menu_service::create(&state, child_dto)
            .await
            .expect("create child");

        let list = menu_service::list(
            &state,
            menu_dto::ListMenuDto {
                menu_name: None,
                status: None,
                parent_id: Some(parent.menu_id.clone()),
                menu_type: None,
            },
        )
        .await
        .expect("list by parent_id");

        assert!(
            list.iter().any(|m| m.menu_id == child.menu_id),
            "child should be returned when filtering by parent_id"
        );

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 9. get_menu_detail — create, fetch by menu_id, verify fields
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn get_menu_detail() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "M", None);
        let expected_name = dto.menu_name.clone();
        let created = menu_service::create(&state, dto)
            .await
            .expect("create menu");

        let detail = menu_service::find_by_id(&state, &created.menu_id)
            .await
            .expect("find by id");

        assert_eq!(detail.menu_id, created.menu_id, "menu_id should match");
        assert_eq!(detail.menu_name, expected_name, "menu_name should match");
        assert_eq!(detail.menu_type, "M", "menu_type should match");

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 10. get_menu_nonexistent — random UUID -> MENU_NOT_FOUND (7020)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn get_menu_nonexistent() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let fake_id = uuid::Uuid::new_v4().to_string();
        let err = menu_service::find_by_id(&state, &fake_id)
            .await
            .expect_err("should fail for nonexistent menu");

        assert_business_code(err, ResponseCode::MENU_NOT_FOUND, "get_menu_nonexistent");
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 11. update_menu_changes_fields — create, update name, verify changed
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn update_menu_changes_fields() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "M", None);
        let created = menu_service::create(&state, dto)
            .await
            .expect("create menu");

        let new_name = format!("{PREFIX}{suffix}-updated");
        menu_service::update(
            &state,
            menu_dto::UpdateMenuDto {
                menu_id: created.menu_id.clone(),
                menu_name: Some(new_name.clone()),
                parent_id: None,
                order_num: None,
                path: None,
                component: None,
                query: None,
                is_frame: None,
                is_cache: None,
                menu_type: None,
                visible: None,
                status: None,
                perms: None,
                icon: None,
                remark: None,
            },
        )
        .await
        .expect("update menu");

        let updated = menu_service::find_by_id(&state, &created.menu_id)
            .await
            .expect("find updated menu");
        assert_eq!(updated.menu_name, new_name, "name should be updated");

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 12. delete_menu_soft_deletes — create, delete, verify del_flag='1' in DB
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn delete_menu_soft_deletes() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix, "M", None);
        let created = menu_service::create(&state, dto)
            .await
            .expect("create menu");

        menu_service::remove(&state, &created.menu_id)
            .await
            .expect("remove menu");

        // Verify del_flag='1' via raw SQL (find_by_id filters del_flag='0')
        let row: Option<(String,)> =
            sqlx::query_as("SELECT del_flag FROM sys_menu WHERE menu_id = $1")
                .bind(&created.menu_id)
                .fetch_optional(&state.pg)
                .await
                .expect("raw select del_flag");

        let del_flag = row.expect("menu row should still exist").0;
        assert_eq!(del_flag, "1", "del_flag should be '1' after soft delete");

        // Hard-delete for cleanup (already soft-deleted, so cleanup_test_menus
        // would miss it with LIKE match; just do a direct DELETE)
        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
        sqlx::query("DELETE FROM sys_menu WHERE menu_id = $1")
            .bind(&created.menu_id)
            .execute(&state.pg)
            .await
            .expect("hard cleanup");
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 13. cascade_delete_removes_descendants — parent M -> child C -> grandchild F
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cascade_delete_removes_descendants() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create parent M
        let parent_dto = make_create_dto(suffix, "M", None);
        let parent = menu_service::create(&state, parent_dto)
            .await
            .expect("create parent");

        // Create child C
        let child_suffix = format!("{suffix}-ch");
        let child_dto = make_create_dto(&child_suffix, "C", Some(parent.menu_id.clone()));
        let child = menu_service::create(&state, child_dto)
            .await
            .expect("create child");

        // Create grandchild F
        let gc_suffix = format!("{suffix}-gc");
        let gc_dto = make_create_dto(&gc_suffix, "F", Some(child.menu_id.clone()));
        let grandchild = menu_service::create(&state, gc_dto)
            .await
            .expect("create grandchild");

        // Cascade delete parent
        let affected = menu_service::cascade_remove(&state, &parent.menu_id)
            .await
            .expect("cascade remove");
        assert_eq!(
            affected, 3,
            "should soft-delete parent + child + grandchild"
        );

        // Verify all 3 have del_flag='1'
        for (label, id) in [
            ("parent", &parent.menu_id),
            ("child", &child.menu_id),
            ("grandchild", &grandchild.menu_id),
        ] {
            let row: Option<(String,)> =
                sqlx::query_as("SELECT del_flag FROM sys_menu WHERE menu_id = $1")
                    .bind(id)
                    .fetch_optional(&state.pg)
                    .await
                    .expect("raw select del_flag");
            let del_flag = row.unwrap_or_else(|| panic!("{label} should exist")).0;
            assert_eq!(del_flag, "1", "{label} del_flag should be '1'");
        }

        // Hard-delete all 3 for cleanup (they are soft-deleted so LIKE won't match on name)
        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
        for id in [&parent.menu_id, &child.menu_id, &grandchild.menu_id] {
            sqlx::query("DELETE FROM sys_menu WHERE menu_id = $1")
                .bind(id)
                .execute(&state.pg)
                .await
                .expect("hard cleanup");
        }
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 14. tree_select_returns_nested_tree — parent + child, verify nesting
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tree_select_returns_nested_tree() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let parent_dto = make_create_dto(suffix, "M", None);
        let parent = menu_service::create(&state, parent_dto)
            .await
            .expect("create parent");

        let child_suffix = format!("{suffix}-ch");
        let child_dto = make_create_dto(&child_suffix, "C", Some(parent.menu_id.clone()));
        menu_service::create(&state, child_dto)
            .await
            .expect("create child");

        let tree = menu_service::tree_select(&state)
            .await
            .expect("tree_select");

        // Find the parent node in the tree
        fn find_node<'a>(
            nodes: &'a [menu_dto::TreeNode],
            id: &str,
        ) -> Option<&'a menu_dto::TreeNode> {
            for n in nodes {
                if n.id == id {
                    return Some(n);
                }
                if let Some(found) = find_node(&n.children, id) {
                    return Some(found);
                }
            }
            None
        }

        let parent_node = find_node(&tree, &parent.menu_id).expect("parent should be in the tree");
        assert!(
            !parent_node.children.is_empty(),
            "parent node should have children"
        );

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// 15. role_menu_tree_select_returns_checked_keys
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn role_menu_tree_select_returns_checked_keys() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Find an existing role in tenant 000000
        let role_id: Option<(String,)> = sqlx::query_as(
            "SELECT role_id FROM sys_role WHERE tenant_id = '000000' AND del_flag = '0' LIMIT 1",
        )
        .fetch_optional(&state.pg)
        .await
        .expect("find test role");

        let role_id = match role_id {
            Some((id,)) => id,
            None => {
                eprintln!(
                    "SKIP: role_menu_tree_select_returns_checked_keys — no role in tenant 000000"
                );
                return;
            }
        };

        // Create a test menu
        let dto = make_create_dto(suffix, "M", None);
        let created = menu_service::create(&state, dto)
            .await
            .expect("create menu");

        // Directly insert into sys_role_menu
        sqlx::query("INSERT INTO sys_role_menu (role_id, menu_id) VALUES ($1, $2)")
            .bind(&role_id)
            .bind(&created.menu_id)
            .execute(&state.pg)
            .await
            .expect("insert sys_role_menu");

        // Call role_menu_tree_select
        let resp = menu_service::role_menu_tree_select(&state, &role_id)
            .await
            .expect("role_menu_tree_select");

        assert!(
            resp.checked_keys.contains(&created.menu_id),
            "checked_keys should contain the menu_id we bound"
        );

        // Cleanup: remove the sys_role_menu row first, then the menu
        sqlx::query("DELETE FROM sys_role_menu WHERE role_id = $1 AND menu_id = $2")
            .bind(&role_id)
            .bind(&created.menu_id)
            .execute(&state.pg)
            .await
            .expect("cleanup sys_role_menu");

        common::cleanup_test_menus(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

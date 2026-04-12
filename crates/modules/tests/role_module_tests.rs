//! Integration tests for the role module. Tests hit the live `saas_tea`
//! dev DB at `127.0.0.1:5432`. Tests are added batch-by-batch.

#[path = "common/mod.rs"]
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use framework::error::AppError;
use framework::response::{PageQuery, ResponseCode};
use modules::system::role::{dto, service};
use tower::ServiceExt;

// ─── known seed data ────────────────────────────────────────────────────────

/// Two menu_ids confirmed present in `sys_menu` (del_flag='0', perms<>'').
const MENU_A: &str = "a0080000-0000-0000-0000-000000000001";
const MENU_B: &str = "a0080001-0000-0000-0000-000000000001";
const MENU_C: &str = "a0080001-0000-0000-0000-000000000002";

/// The single active admin user in tenant 000000.
const ADMIN_USER_ID: &str = "cf827fc0-e7cc-4b9f-913c-e20628ade20a";

// ─── HTTP middleware test (batch 0) ─────────────────────────────────────────

/// Proves the `/system/role/:id` route is wired in and rejects
/// unauthenticated callers with 401 (not 404). The happy-path integration
/// test lands in a later batch once `POST /role/` can seed data.
///
/// Note: `common::build_state_and_router` returns the router via
/// `modules::router(state)`, which merges role routes flat at the root
/// (no `/api/v1` prefix). The `/api/v1` nesting happens in `app/main.rs`.
/// For this reason the test URI here does not include `/api/v1` — what
/// matters is that the route resolves (not 404) and that `access::enforce`
/// rejects the missing session with 401.
#[tokio::test]
async fn find_by_id_rejects_unauthenticated_with_401() {
    let (_state, router) = common::build_state_and_router().await;

    let req = Request::builder()
        .method("GET")
        .uri("/system/role/does-not-exist-xyz-123")
        .body(Body::empty())
        .unwrap();

    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── helper: assert DATA_NOT_FOUND ──────────────────────────────────────────

fn assert_data_not_found(err: AppError) {
    match err {
        AppError::Business { code, .. } => {
            assert_eq!(
                code,
                ResponseCode::DATA_NOT_FOUND,
                "expected DATA_NOT_FOUND (1001), got code {code}"
            );
        }
        other => panic!("expected Business(DATA_NOT_FOUND), got {other:?}"),
    }
}

// ─── CRUD: create / find_by_id / update / remove ────────────────────────────

/// Test 1: create a role with 0 menus, find it by id.
#[tokio::test]
async fn create_and_find_by_id_happy_path() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-create-find-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-create-find".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        let fetched = service::find_by_id(&state, &created.role_id)
            .await
            .expect("find_by_id should succeed");

        assert_eq!(fetched.role_key, role_key);
        assert_eq!(fetched.menu_ids.len(), 0);

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 2: create with 2 real menu_ids, find it, assert both appear in detail.
#[tokio::test]
async fn create_with_menus_persists_bindings() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-create-menus-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-create-menus".into(),
            role_key: role_key.clone(),
            role_sort: 2,
            status: "0".into(),
            remark: None,
            menu_ids: vec![MENU_A.into(), MENU_B.into()],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        let fetched = service::find_by_id(&state, &created.role_id)
            .await
            .expect("find_by_id should succeed");

        assert_eq!(fetched.role_key, role_key);
        // menu_ids are sorted by menu_id in the repo
        let mut expected = vec![MENU_A.to_string(), MENU_B.to_string()];
        expected.sort();
        let mut actual = fetched.menu_ids.clone();
        actual.sort();
        assert_eq!(actual, expected, "bound menu_ids should match");

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 3: find_by_id with a ghost UUID returns DATA_NOT_FOUND.
#[tokio::test]
async fn find_by_id_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = uuid::Uuid::new_v4().to_string();
        let err = service::find_by_id(&state, &ghost_id)
            .await
            .expect_err("should return DATA_NOT_FOUND");
        assert_data_not_found(err);
    })
    .await;
}

/// Test 4: create a role, soft-delete it, then find_by_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn find_by_id_returns_not_found_after_soft_delete() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-find-after-del-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-find-after-del".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        service::remove(&state, &created.role_id)
            .await
            .expect("remove should succeed");

        let err = service::find_by_id(&state, &created.role_id)
            .await
            .expect_err("should return DATA_NOT_FOUND after delete");
        assert_data_not_found(err);

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 5: create with 3 menus, update with 2 different menus, assert exactly
/// the 2 new menu_ids appear and the old ones are gone.
#[tokio::test]
async fn update_replaces_menu_bindings() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-update-menus-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-update-menus".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![MENU_A.into(), MENU_B.into(), MENU_C.into()],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        // Update: replace menus with just MENU_B + MENU_C (dropping MENU_A,
        // but keeping 2 to ensure the replace-all logic runs).
        let update_dto = dto::UpdateRoleDto {
            role_id: created.role_id.clone(),
            role_name: "it-update-menus-v2".into(),
            role_key: role_key.clone(),
            role_sort: 2,
            status: "0".into(),
            remark: None,
            menu_ids: vec![MENU_B.into(), MENU_C.into()],
        };
        service::update(&state, update_dto)
            .await
            .expect("update should succeed");

        let fetched = service::find_by_id(&state, &created.role_id)
            .await
            .expect("find_by_id should succeed");

        let mut expected = vec![MENU_B.to_string(), MENU_C.to_string()];
        expected.sort();
        let mut actual = fetched.menu_ids.clone();
        actual.sort();
        assert_eq!(actual, expected, "menus should be replaced");
        assert_eq!(fetched.role_name, "it-update-menus-v2");

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 6: update with a ghost role_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn update_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = uuid::Uuid::new_v4().to_string();
        let update_dto = dto::UpdateRoleDto {
            role_id: ghost_id,
            role_name: "ghost-role".into(),
            role_key: "ghost:key".into(),
            role_sort: 0,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let err = service::update(&state, update_dto)
            .await
            .expect_err("should return DATA_NOT_FOUND for ghost role");
        assert_data_not_found(err);
    })
    .await;
}

// ─── list / option-select ───────────────────────────────────────────────────

/// Test 7: list with role_key filter finds the seeded role.
#[tokio::test]
async fn list_finds_seeded_role_by_role_key_filter() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-list-filter-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-list-filter".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        let query = dto::ListRoleDto {
            role_name: None,
            role_key: Some(role_key.clone()),
            status: None,
            page: PageQuery::default(),
        };
        let page = service::list(&state, query)
            .await
            .expect("list should succeed");

        assert_eq!(page.rows.len(), 1, "should find exactly 1 row");
        assert_eq!(page.rows[0].role_key, role_key);

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 8: pagination metadata is correct.
#[tokio::test]
async fn list_pagination_metadata_correct() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let query = dto::ListRoleDto {
            role_name: None,
            role_key: None,
            status: None,
            page: PageQuery {
                page_num: 1,
                page_size: 2,
            },
        };
        let page = service::list(&state, query)
            .await
            .expect("list should succeed");

        assert_eq!(page.page_num, 1);
        assert_eq!(page.page_size, 2);
        // Rows returned is at most page_size
        assert!(page.rows.len() <= 2);
    })
    .await;
}

/// Test 9: option_select excludes disabled roles (status='1').
#[tokio::test]
async fn option_select_excludes_disabled_role() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-option-sel-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-option-sel".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(), // active
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        // Should appear in option-select while active
        let options = service::option_select(&state)
            .await
            .expect("option_select should succeed");
        let present = options.iter().any(|r| r.role_id == created.role_id);
        assert!(present, "active role should appear in option-select");

        // Disable the role
        service::change_status(
            &state,
            dto::ChangeRoleStatusDto {
                role_id: created.role_id.clone(),
                status: "1".into(),
            },
        )
        .await
        .expect("change_status should succeed");

        // Should NOT appear in option-select after disabling
        let options = service::option_select(&state)
            .await
            .expect("option_select should succeed");
        let present = options.iter().any(|r| r.role_id == created.role_id);
        assert!(
            !present,
            "disabled role should be excluded from option-select"
        );

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

// ─── change-status ──────────────────────────────────────────────────────────

/// Test 10: change_status flips the value and persists it.
#[tokio::test]
async fn change_status_flips_and_persists() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-chg-status-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-chg-status".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        service::change_status(
            &state,
            dto::ChangeRoleStatusDto {
                role_id: created.role_id.clone(),
                status: "1".into(),
            },
        )
        .await
        .expect("change_status should succeed");

        let fetched = service::find_by_id(&state, &created.role_id)
            .await
            .expect("find_by_id should succeed");
        assert_eq!(fetched.status, "1", "status should be flipped to 1");

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 11: change_status with a ghost role_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn change_status_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = uuid::Uuid::new_v4().to_string();
        let err = service::change_status(
            &state,
            dto::ChangeRoleStatusDto {
                role_id: ghost_id,
                status: "1".into(),
            },
        )
        .await
        .expect_err("should return DATA_NOT_FOUND for ghost role");
        assert_data_not_found(err);
    })
    .await;
}

// ─── remove (soft delete) ───────────────────────────────────────────────────

/// Test 12: remove sets del_flag='1' in the DB.
#[tokio::test]
async fn remove_sets_del_flag_in_db() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-remove-flag-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-remove-flag".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        service::remove(&state, &created.role_id)
            .await
            .expect("remove should succeed");

        // Directly query del_flag — the service won't return soft-deleted rows
        let del_flag: String =
            sqlx::query_scalar("SELECT del_flag FROM sys_role WHERE role_id = $1")
                .bind(&created.role_id)
                .fetch_one(&state.pg)
                .await
                .expect("should find the row (even after soft delete)");
        assert_eq!(del_flag, "1", "del_flag should be set to '1' after remove");

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 13: remove with a ghost role_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn remove_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = uuid::Uuid::new_v4().to_string();
        let err = service::remove(&state, &ghost_id)
            .await
            .expect_err("should return DATA_NOT_FOUND for ghost role");
        assert_data_not_found(err);
    })
    .await;
}

// ─── allocated / unallocated users ──────────────────────────────────────────

/// Test 14: allocated_users returns the seeded binding.
#[tokio::test]
async fn allocated_users_returns_seeded_binding() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-alloc-seed-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-alloc-seed".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        // Directly insert a user-role binding via sqlx (avoids going through
        // assign_users to keep this test independent of test 17).
        sqlx::query(
            "INSERT INTO sys_user_role (user_id, role_id) VALUES ($1, $2) \
             ON CONFLICT (user_id, role_id) DO NOTHING",
        )
        .bind(ADMIN_USER_ID)
        .bind(&created.role_id)
        .execute(&state.pg)
        .await
        .expect("seed sys_user_role");

        let query = dto::AuthUserListQueryDto {
            role_id: created.role_id.clone(),
            user_name: None,
            page: PageQuery::default(),
        };
        let page = service::allocated_users(&state, query)
            .await
            .expect("allocated_users should succeed");

        assert_eq!(page.total, 1, "should find 1 allocated user");
        assert_eq!(
            page.rows[0].user_id, ADMIN_USER_ID,
            "the allocated user should be the admin"
        );

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 15: allocated_users for a ghost role_id returns total=0 (not an error).
#[tokio::test]
async fn allocated_users_empty_for_bogus_role() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = uuid::Uuid::new_v4().to_string();
        let query = dto::AuthUserListQueryDto {
            role_id: ghost_id,
            user_name: None,
            page: PageQuery::default(),
        };
        let page = service::allocated_users(&state, query)
            .await
            .expect("allocated_users with ghost role_id should return empty page");
        assert_eq!(page.total, 0);
    })
    .await;
}

/// Test 16: unallocated_users excludes a user that is bound to the role.
#[tokio::test]
async fn unallocated_users_excludes_bound_user() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-unalloc-excl-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-unalloc-excl".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        // Bind the admin user to this role
        sqlx::query(
            "INSERT INTO sys_user_role (user_id, role_id) VALUES ($1, $2) \
             ON CONFLICT (user_id, role_id) DO NOTHING",
        )
        .bind(ADMIN_USER_ID)
        .bind(&created.role_id)
        .execute(&state.pg)
        .await
        .expect("seed sys_user_role");

        let query = dto::AuthUserListQueryDto {
            role_id: created.role_id.clone(),
            user_name: None,
            page: PageQuery::default(),
        };
        let page = service::unallocated_users(&state, query)
            .await
            .expect("unallocated_users should succeed");

        // The admin user is bound, so it must NOT appear in unallocated list
        let admin_in_list = page.rows.iter().any(|r| r.user_id == ADMIN_USER_ID);
        assert!(
            !admin_in_list,
            "bound admin user must not appear in unallocated list"
        );

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

// ─── assign / unassign users ─────────────────────────────────────────────────

/// Test 17: assign_users persists a binding in sys_user_role.
#[tokio::test]
async fn assign_users_persists_binding() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-assign-persist-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-assign-persist".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        service::assign_users(
            &state,
            dto::AuthUserAssignDto {
                role_id: created.role_id.clone(),
                user_ids: vec![ADMIN_USER_ID.into()],
            },
        )
        .await
        .expect("assign_users should succeed");

        // Verify via raw sqlx
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_user_role WHERE user_id = $1 AND role_id = $2",
        )
        .bind(ADMIN_USER_ID)
        .bind(&created.role_id)
        .fetch_one(&state.pg)
        .await
        .expect("count query should succeed");

        assert_eq!(count, 1, "should have exactly 1 binding");

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 18: assign_users is idempotent (ON CONFLICT DO NOTHING).
#[tokio::test]
async fn assign_users_is_idempotent() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-assign-idem-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-assign-idem".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        let assign_dto = || dto::AuthUserAssignDto {
            role_id: created.role_id.clone(),
            user_ids: vec![ADMIN_USER_ID.into()],
        };

        service::assign_users(&state, assign_dto())
            .await
            .expect("first assign_users should succeed");
        service::assign_users(&state, assign_dto())
            .await
            .expect("second assign_users should succeed (idempotent)");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_user_role WHERE user_id = $1 AND role_id = $2",
        )
        .bind(ADMIN_USER_ID)
        .bind(&created.role_id)
        .fetch_one(&state.pg)
        .await
        .expect("count query should succeed");

        assert_eq!(
            count, 1,
            "duplicate assign should still leave exactly 1 row"
        );

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 19: assign_users with a ghost role_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn assign_users_on_ghost_role_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = uuid::Uuid::new_v4().to_string();
        let err = service::assign_users(
            &state,
            dto::AuthUserAssignDto {
                role_id: ghost_id,
                user_ids: vec![ADMIN_USER_ID.into()],
            },
        )
        .await
        .expect_err("should return DATA_NOT_FOUND for ghost role");
        assert_data_not_found(err);
    })
    .await;
}

/// Test 20: unassign_users removes an existing binding.
#[tokio::test]
async fn unassign_users_removes_binding() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-unassign-rm-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-unassign-rm".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        // First assign
        service::assign_users(
            &state,
            dto::AuthUserAssignDto {
                role_id: created.role_id.clone(),
                user_ids: vec![ADMIN_USER_ID.into()],
            },
        )
        .await
        .expect("assign_users should succeed");

        // Then unassign
        service::unassign_users(
            &state,
            dto::AuthUserCancelDto {
                role_id: created.role_id.clone(),
                user_ids: vec![ADMIN_USER_ID.into()],
            },
        )
        .await
        .expect("unassign_users should succeed");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_user_role WHERE user_id = $1 AND role_id = $2",
        )
        .bind(ADMIN_USER_ID)
        .bind(&created.role_id)
        .fetch_one(&state.pg)
        .await
        .expect("count query should succeed");

        assert_eq!(count, 0, "binding should be removed");

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

/// Test 21: unassign_users with a ghost role_id returns DATA_NOT_FOUND.
/// This specifically exercises the tenant-guard fix: the service validates
/// that the role exists in the current tenant before attempting the DELETE.
#[tokio::test]
async fn unassign_users_on_ghost_role_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = uuid::Uuid::new_v4().to_string();
        let err = service::unassign_users(
            &state,
            dto::AuthUserCancelDto {
                role_id: ghost_id,
                user_ids: vec![ADMIN_USER_ID.into()],
            },
        )
        .await
        .expect_err("should return DATA_NOT_FOUND for ghost role");
        assert_data_not_found(err);
    })
    .await;
}

/// Test 22: unassign_users with no existing binding is a no-op (idempotent).
#[tokio::test]
async fn unassign_users_with_nothing_bound_is_noop_success() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-unassign-noop-";
    let role_key = format!("{prefix}{}", uuid::Uuid::new_v4());

    common::as_super_admin(async {
        common::cleanup_test_rows(&state.pg, prefix).await;

        let create_dto = dto::CreateRoleDto {
            role_name: "it-unassign-noop".into(),
            role_key: role_key.clone(),
            role_sort: 1,
            status: "0".into(),
            remark: None,
            menu_ids: vec![],
        };
        let created = service::create(&state, create_dto)
            .await
            .expect("create should succeed");

        // Unassign a user that was never assigned — should succeed silently
        service::unassign_users(
            &state,
            dto::AuthUserCancelDto {
                role_id: created.role_id.clone(),
                user_ids: vec![ADMIN_USER_ID.into()],
            },
        )
        .await
        .expect("unassign_users with no binding should be Ok(())");

        common::cleanup_test_rows(&state.pg, prefix).await;
    })
    .await;
}

//! Integration tests for the user module. Real DB at 127.0.0.1:5432/saas_tea.
//! Run with `--test-threads=1` to avoid concurrent cleanup conflicts between
//! tests that share the same `sys_user` table.

#[path = "common/mod.rs"]
mod common;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use framework::error::AppError;
use framework::response::{PageQuery, ResponseCode};
use modules::domain::RoleRepo;
use modules::system::user::{dto, service};
use tower::ServiceExt;

// ─── known seed data ────────────────────────────────────────────────────────

/// The single active super-admin user in platform 000000.
const ADMIN_USER_ID: &str = "cf827fc0-e7cc-4b9f-913c-e20628ade20a";

// ─── helper: build a CreateUserDto ──────────────────────────────────────────

/// Fixture: build a valid CreateUserDto with the given prefix + suffix + role_ids.
/// The suffix should be at most 8 chars so that `prefix + suffix` fits in
/// `sys_user.user_name varchar(50)`.
fn make_create_dto(prefix: &str, suffix: &str, role_ids: Vec<String>) -> dto::CreateUserDto {
    dto::CreateUserDto {
        dept_id: None,
        nick_name: format!("{prefix}-nick"),
        user_name: format!("{prefix}{suffix}"),
        password: "abc123".into(),
        email: "".into(),
        phonenumber: "".into(),
        sex: "2".into(),
        avatar: "".into(),
        status: "0".into(),
        remark: None,
        role_ids,
    }
}

// ─── helper: assert specific error codes ────────────────────────────────────

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

fn assert_operation_not_allowed(err: AppError) {
    match err {
        AppError::Business { code, .. } => {
            assert_eq!(
                code,
                ResponseCode::OPERATION_NOT_ALLOWED,
                "expected OPERATION_NOT_ALLOWED, got code {code}"
            );
        }
        other => panic!("expected Business(OPERATION_NOT_ALLOWED), got {other:?}"),
    }
}

// ─── HTTP middleware test (0) ────────────────────────────────────────────────

/// Proves the `/system/user/:id` route is wired in and rejects
/// unauthenticated callers with 401 (not 404).
#[tokio::test]
async fn user_find_by_id_rejects_unauthenticated_with_401() {
    let (_state, router) = common::build_state_and_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/system/user/does-not-exist-xyz")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ─── CRUD: create / find_by_id ───────────────────────────────────────────────

/// Test 1: create a user with 0 roles, find by id, assert user_name + role_ids.
#[tokio::test]
async fn create_and_find_by_id_happy_path() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-create-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];
    let expected_user_name = format!("{prefix}{suffix}");

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let created = service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("create should succeed");

        let fetched = service::find_by_id(&state, &created.user_id)
            .await
            .expect("find_by_id should succeed");

        assert_eq!(fetched.user_name, expected_user_name);
        assert_eq!(fetched.role_ids.len(), 0);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 2: create with 2 real role_ids, assert both appear in the returned DTO.
#[tokio::test]
async fn create_with_roles_persists_bindings() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-create-roles-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        // Fetch 2 real role_ids from the live tenant
        let roles = RoleRepo::find_option_list(&state.pg)
            .await
            .expect("find_option_list should succeed");
        assert!(
            roles.len() >= 2,
            "need at least 2 active roles in the dev DB; found {}",
            roles.len()
        );
        let role_a = roles[0].role_id.clone();
        let role_b = roles[1].role_id.clone();

        let created = service::create(
            &state,
            make_create_dto(prefix, suffix, vec![role_a.clone(), role_b.clone()]),
        )
        .await
        .expect("create with roles should succeed");

        // The service returns submitted role_ids directly; also verify via find_by_id
        let fetched = service::find_by_id(&state, &created.user_id)
            .await
            .expect("find_by_id should succeed");

        let mut expected = vec![role_a.clone(), role_b.clone()];
        expected.sort();
        let mut actual = fetched.role_ids.clone();
        actual.sort();
        assert_eq!(actual, expected, "bound role_ids should match");

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 3: create twice with the same user_name returns DUPLICATE_KEY.
#[tokio::test]
async fn create_fails_on_duplicate_user_name() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-dup-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("first create should succeed");

        let err = service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect_err("second create with same user_name should fail");

        match err {
            AppError::Business { code, .. } => {
                assert_eq!(
                    code,
                    ResponseCode::DUPLICATE_KEY,
                    "expected DUPLICATE_KEY, got code {code}"
                );
            }
            other => panic!("expected Business(DUPLICATE_KEY), got {other:?}"),
        }

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 4: find_by_id with a ghost UUID returns DATA_NOT_FOUND.
#[tokio::test]
async fn find_by_id_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = "00000000-0000-0000-0000-ffffffffffff";
        let err = service::find_by_id(&state, ghost_id)
            .await
            .expect_err("should return DATA_NOT_FOUND for ghost user");
        assert_data_not_found(err);
    })
    .await;
}

/// Test 5: create, remove, then find_by_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn find_by_id_after_soft_delete_returns_not_found() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-softdel-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let created = service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("create should succeed");

        service::remove(&state, &created.user_id)
            .await
            .expect("remove should succeed");

        let err = service::find_by_id(&state, &created.user_id)
            .await
            .expect_err("should return DATA_NOT_FOUND after soft delete");
        assert_data_not_found(err);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 6: create with known user_name, list with exact filter, assert total == 1.
#[tokio::test]
async fn list_finds_seeded_user_by_filter() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-listfilter-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];
    let user_name = format!("{prefix}{suffix}");

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("create should succeed");

        let query = dto::ListUserDto {
            user_name: Some(user_name.clone()),
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: PageQuery::default(),
        };
        let page = service::list(&state, query)
            .await
            .expect("list should succeed");

        assert_eq!(page.rows.len(), 1, "should find exactly 1 row");
        assert_eq!(page.rows[0].user_name, user_name);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

// ─── List / option-select ────────────────────────────────────────────────────

/// Test 7: list with page_size=2 returns correct pagination metadata.
#[tokio::test]
async fn list_pagination_metadata_correct() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let query = dto::ListUserDto {
            user_name: None,
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
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
        assert!(page.rows.len() <= 2);
    })
    .await;
}

/// Test 8: create user, disable it, call option_select — user must NOT appear.
#[tokio::test]
async fn option_select_excludes_disabled_users() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-optdisable-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];
    let user_name = format!("{prefix}{suffix}");

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let created = service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("create should succeed");

        // Disable the user
        service::change_status(
            &state,
            dto::ChangeUserStatusDto {
                user_id: created.user_id.clone(),
                status: "1".into(),
            },
        )
        .await
        .expect("change_status to disabled should succeed");

        // option_select filtered by exact user_name should not include disabled user
        let options = service::option_select(
            &state,
            dto::UserOptionQueryDto {
                user_name: Some(user_name.clone()),
            },
        )
        .await
        .expect("option_select should succeed");

        let present = options.iter().any(|u| u.user_id == created.user_id);
        assert!(
            !present,
            "disabled user should be excluded from option-select"
        );

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 9: info() with harness user_id "it-admin" (not a real DB row)
/// returns DATA_NOT_FOUND (exercises the not-found error path in info()).
#[tokio::test]
async fn info_returns_data_not_found_for_harness_user() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        // The harness context has user_id = "it-admin" which doesn't exist
        // in sys_user. The info() service calls UserRepo::find_by_id (not
        // tenant-scoped) and will return None → DATA_NOT_FOUND.
        let err = service::info(&state)
            .await
            .expect_err("should return DATA_NOT_FOUND for non-existent harness user");
        match err {
            AppError::Business { code, .. } => {
                assert_eq!(
                    code,
                    ResponseCode::DATA_NOT_FOUND,
                    "expected DATA_NOT_FOUND, got {code}"
                );
            }
            other => panic!("expected Business(DATA_NOT_FOUND), got {other:?}"),
        }
    })
    .await;
}

// ─── Update / change-status / remove / guards ────────────────────────────────

/// Test 10: create with role_a, update with role_b, find → only role_b.
#[tokio::test]
async fn update_replaces_role_bindings() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-update-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let roles = RoleRepo::find_option_list(&state.pg)
            .await
            .expect("find_option_list should succeed");
        assert!(
            roles.len() >= 2,
            "need at least 2 active roles; found {}",
            roles.len()
        );
        let role_a = roles[0].role_id.clone();
        let role_b = roles[1].role_id.clone();

        let created = service::create(
            &state,
            make_create_dto(prefix, suffix, vec![role_a.clone()]),
        )
        .await
        .expect("create should succeed");

        let update_dto = dto::UpdateUserDto {
            user_id: created.user_id.clone(),
            dept_id: None,
            nick_name: format!("{prefix}-nick-v2"),
            email: "".into(),
            phonenumber: "".into(),
            sex: "2".into(),
            avatar: "".into(),
            status: "0".into(),
            remark: None,
            role_ids: vec![role_b.clone()],
        };
        service::update(&state, update_dto)
            .await
            .expect("update should succeed");

        let fetched = service::find_by_id(&state, &created.user_id)
            .await
            .expect("find_by_id should succeed");

        assert_eq!(
            fetched.role_ids,
            vec![role_b.clone()],
            "should have only role_b after update"
        );

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 11: update with a ghost user_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn update_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = "00000000-0000-0000-0000-ffffffffffff";
        let update_dto = dto::UpdateUserDto {
            user_id: ghost_id.into(),
            dept_id: None,
            nick_name: "ghost-nick".into(),
            email: "".into(),
            phonenumber: "".into(),
            sex: "2".into(),
            avatar: "".into(),
            status: "0".into(),
            remark: None,
            role_ids: vec![],
        };
        let err = service::update(&state, update_dto)
            .await
            .expect_err("should return DATA_NOT_FOUND for ghost user");
        assert_data_not_found(err);
    })
    .await;
}

/// Test 12: create, change_status to '1', find, assert status == '1'.
#[tokio::test]
async fn change_status_flips_and_persists() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-chst-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let created = service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("create should succeed");

        service::change_status(
            &state,
            dto::ChangeUserStatusDto {
                user_id: created.user_id.clone(),
                status: "1".into(),
            },
        )
        .await
        .expect("change_status should succeed");

        let fetched = service::find_by_id(&state, &created.user_id)
            .await
            .expect("find_by_id should succeed");
        assert_eq!(fetched.status, "1", "status should be flipped to '1'");

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 13: change_status targeting ADMIN_USER_ID returns OPERATION_NOT_ALLOWED.
#[tokio::test]
async fn change_status_on_admin_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = service::change_status(
            &state,
            dto::ChangeUserStatusDto {
                user_id: ADMIN_USER_ID.into(),
                status: "1".into(),
            },
        )
        .await
        .expect_err("should block change_status on super admin");
        assert_operation_not_allowed(err);
    })
    .await;
}

/// Test 14: change_status targeting "it-admin" (the harness user_id) returns
/// OPERATION_NOT_ALLOWED — exercises the self-op guard.
#[tokio::test]
async fn change_status_on_self_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = service::change_status(
            &state,
            dto::ChangeUserStatusDto {
                user_id: "it-admin".into(), // matches RequestContext.user_id in harness
                status: "1".into(),
            },
        )
        .await
        .expect_err("should block change_status on self");
        assert_operation_not_allowed(err);
    })
    .await;
}

/// Test 15: create → remove → find returns DATA_NOT_FOUND.
/// Also: remove(ADMIN_USER_ID) returns OPERATION_NOT_ALLOWED.
#[tokio::test]
async fn remove_soft_deletes_and_blocks_admin() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-rm-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let created = service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("create should succeed");

        service::remove(&state, &created.user_id)
            .await
            .expect("remove should succeed");

        let err = service::find_by_id(&state, &created.user_id)
            .await
            .expect_err("should return DATA_NOT_FOUND after soft delete");
        assert_data_not_found(err);

        // Admin block
        let err = service::remove(&state, ADMIN_USER_ID)
            .await
            .expect_err("should block remove on super admin");
        assert_operation_not_allowed(err);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 16: remove("") returns PARAM_INVALID.
#[tokio::test]
async fn remove_rejects_empty_ids() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = service::remove(&state, "")
            .await
            .expect_err("should reject empty path_ids");
        match err {
            AppError::Business { code, .. } => {
                assert_eq!(
                    code,
                    ResponseCode::PARAM_INVALID,
                    "expected PARAM_INVALID, got {code}"
                );
            }
            other => panic!("expected Business(PARAM_INVALID), got {other:?}"),
        }
    })
    .await;
}

// ─── Reset-pwd ───────────────────────────────────────────────────────────────

/// Test 17: create, reset_password, query DB directly — hash starts with "$2".
#[tokio::test]
async fn reset_password_updates_hash() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-resetpw-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let created = service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("create should succeed");

        service::reset_password(
            &state,
            dto::ResetPwdDto {
                user_id: created.user_id.clone(),
                password: "newpass123".into(),
            },
        )
        .await
        .expect("reset_password should succeed");

        let row: (String,) = sqlx::query_as("SELECT password FROM sys_user WHERE user_id = $1")
            .bind(&created.user_id)
            .fetch_one(&state.pg)
            .await
            .expect("fetch password");

        assert!(
            row.0.starts_with("$2"),
            "password should be a bcrypt hash, got: {}",
            row.0
        );

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 18: reset_password targeting ADMIN_USER_ID returns OPERATION_NOT_ALLOWED.
#[tokio::test]
async fn reset_password_on_admin_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = service::reset_password(
            &state,
            dto::ResetPwdDto {
                user_id: ADMIN_USER_ID.into(),
                password: "newpass123".into(),
            },
        )
        .await
        .expect_err("should block reset_password on super admin");
        assert_operation_not_allowed(err);
    })
    .await;
}

// ─── Auth-role ───────────────────────────────────────────────────────────────

/// Test 19: create with role_a, update_auth_role with [role_b], find_auth_role
/// asserts role_ids == [role_b].
#[tokio::test]
async fn update_auth_role_replaces_bindings() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-authrole-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let roles = RoleRepo::find_option_list(&state.pg)
            .await
            .expect("find_option_list should succeed");
        assert!(
            roles.len() >= 2,
            "need at least 2 active roles; found {}",
            roles.len()
        );
        let role_a = roles[0].role_id.clone();
        let role_b = roles[1].role_id.clone();

        let created = service::create(
            &state,
            make_create_dto(prefix, suffix, vec![role_a.clone()]),
        )
        .await
        .expect("create should succeed");

        service::update_auth_role(
            &state,
            dto::AuthRoleUpdateDto {
                user_id: created.user_id.clone(),
                role_ids: vec![role_b.clone()],
            },
        )
        .await
        .expect("update_auth_role should succeed");

        let auth = service::find_auth_role(&state, &created.user_id)
            .await
            .expect("find_auth_role should succeed");

        assert_eq!(
            auth.role_ids,
            vec![role_b.clone()],
            "should have only role_b after update_auth_role"
        );

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 20: update_auth_role targeting ADMIN_USER_ID returns OPERATION_NOT_ALLOWED.
#[tokio::test]
async fn update_auth_role_on_admin_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = service::update_auth_role(
            &state,
            dto::AuthRoleUpdateDto {
                user_id: ADMIN_USER_ID.into(),
                role_ids: vec![],
            },
        )
        .await
        .expect_err("should block update_auth_role on super admin");
        assert_operation_not_allowed(err);
    })
    .await;
}

/// Test 21: create user, update_auth_role with a fake role_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn update_auth_role_with_invalid_role_id_rejects() {
    let (state, _router) = common::build_state_and_router().await;
    let prefix = "it-user-authrole-badrole-";
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        common::cleanup_test_users(&state.pg, prefix).await;

        let created = service::create(&state, make_create_dto(prefix, suffix, vec![]))
            .await
            .expect("create should succeed");

        let err = service::update_auth_role(
            &state,
            dto::AuthRoleUpdateDto {
                user_id: created.user_id.clone(),
                role_ids: vec!["00000000-0000-0000-0000-ffffffffffff".into()],
            },
        )
        .await
        .expect_err("should reject invalid role_id");
        assert_data_not_found(err);

        common::cleanup_test_users(&state.pg, prefix).await;
    })
    .await;
}

/// Test 22: find_auth_role with a ghost user_id returns DATA_NOT_FOUND.
#[tokio::test]
async fn find_auth_role_not_found_returns_data_not_found() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let ghost_id = "00000000-0000-0000-0000-ffffffffffff";
        let err = service::find_auth_role(&state, ghost_id)
            .await
            .expect_err("should return DATA_NOT_FOUND for ghost user");
        assert_data_not_found(err);
    })
    .await;
}

/// Test 23: remove targeting self (harness user_id = "it-admin") returns
/// OPERATION_NOT_ALLOWED. Proves `is_self_op` guard fires before any DB write
/// for DELETE — the symmetric case of `change_status_on_self_is_blocked`.
#[tokio::test]
async fn remove_on_self_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = service::remove(&state, "it-admin")
            .await
            .expect_err("should block remove on self");
        assert_operation_not_allowed(err);
    })
    .await;
}

/// Test 24: update_auth_role targeting self returns OPERATION_NOT_ALLOWED.
/// Prevents privilege escalation by editing own roles — the symmetric case
/// of `change_status_on_self_is_blocked` for the auth-role endpoint.
#[tokio::test]
async fn update_auth_role_on_self_is_blocked() {
    let (state, _router) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = service::update_auth_role(
            &state,
            dto::AuthRoleUpdateDto {
                user_id: "it-admin".into(),
                role_ids: vec![],
            },
        )
        .await
        .expect_err("should block update_auth_role on self");
        assert_operation_not_allowed(err);
    })
    .await;
}

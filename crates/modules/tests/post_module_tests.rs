//! Integration tests for the post module.

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::ResponseCode;
use modules::system::post::{dto as post_dto, service as post_service};

const PREFIX: &str = "it-post-";

fn make_create_dto(suffix: &str) -> post_dto::CreatePostDto {
    post_dto::CreatePostDto {
        post_code: format!("{PREFIX}{suffix}"),
        post_name: format!("{PREFIX}name-{suffix}"),
        post_category: None,
        dept_id: None,
        post_sort: 1,
        status: "0".into(),
        remark: None,
    }
}

fn assert_business_code(err: AppError, expected: ResponseCode, label: &str) {
    match err {
        AppError::Business { code, .. } => {
            assert_eq!(code, expected, "{label}: expected {expected}, got {code}");
        }
        other => panic!("{label}: expected Business({expected}), got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. create post
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_post() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix);
        let resp = post_service::create(&state, dto)
            .await
            .expect("create post");

        assert_eq!(resp.post_code, format!("{PREFIX}{suffix}"));
        assert_eq!(resp.status, "0");

        common::cleanup_test_posts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. find_by_id
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_post_by_id() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = post_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create");

        let found = post_service::find_by_id(&state, &created.post_id)
            .await
            .expect("find_by_id");
        assert_eq!(found.post_id, created.post_id);
        assert_eq!(found.post_code, created.post_code);

        common::cleanup_test_posts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. find_by_id not found
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_post_not_found() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = post_service::find_by_id(&state, "nonexistent-id")
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::POST_NOT_FOUND, "find_by_id");
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. duplicate code
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_post_duplicate_code() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        post_service::create(&state, make_create_dto(suffix))
            .await
            .expect("first create");

        let dup = make_create_dto(suffix);
        let err = post_service::create(&state, dup).await.unwrap_err();
        assert_business_code(err, ResponseCode::POST_CODE_EXISTS, "dup code");

        common::cleanup_test_posts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. update post
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn update_post() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = post_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create");

        let update_dto = post_dto::UpdatePostDto {
            post_id: created.post_id.clone(),
            post_code: None,
            post_name: Some(format!("{PREFIX}updated-{suffix}")),
            post_category: None,
            dept_id: None,
            post_sort: Some(99),
            status: None,
            remark: None,
        };
        post_service::update(&state, update_dto)
            .await
            .expect("update");

        let found = post_service::find_by_id(&state, &created.post_id)
            .await
            .expect("find after update");
        assert_eq!(found.post_sort, 99);

        common::cleanup_test_posts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. list paginated
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_posts_paginated() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        // Create 3 posts
        for i in 0..3 {
            let mut dto = make_create_dto(&format!("{suffix}-{i}"));
            dto.post_name = format!("{PREFIX}list-{suffix}-{i}");
            post_service::create(&state, dto).await.expect("create");
        }

        let query = post_dto::ListPostDto {
            post_code: Some(format!("{PREFIX}{suffix}")),
            post_name: None,
            status: None,
            page: framework::response::PageQuery {
                page_num: 1,
                page_size: 2,
            },
        };
        let page = post_service::list(&state, query).await.expect("list");
        assert_eq!(page.rows.len(), 2);
        assert!(page.total >= 3);
        assert_eq!(page.page_size, 2);

        common::cleanup_test_posts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. batch remove
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn remove_posts_batch() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let a = post_service::create(&state, make_create_dto(&format!("{suffix}-a")))
            .await
            .expect("create a");
        let mut dto_b = make_create_dto(&format!("{suffix}-b"));
        dto_b.post_name = format!("{PREFIX}name-{suffix}-b");
        let b = post_service::create(&state, dto_b).await.expect("create b");

        let ids = format!("{},{}", a.post_id, b.post_id);
        post_service::remove(&state, &ids)
            .await
            .expect("batch remove");

        // Both should be soft-deleted (not found)
        let err = post_service::find_by_id(&state, &a.post_id)
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::POST_NOT_FOUND, "a gone");

        common::cleanup_test_posts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 8. option_select
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn option_select() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        post_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create");

        let options = post_service::option_select(&state)
            .await
            .expect("option_select");
        assert!(
            options
                .iter()
                .any(|p| p.post_code == format!("{PREFIX}{suffix}")),
            "option_select should include the test post"
        );

        common::cleanup_test_posts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

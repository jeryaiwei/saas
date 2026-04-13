//! Integration tests for the notice module.

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::ResponseCode;
use modules::message::notice::{dto as notice_dto, service as notice_service};

const PREFIX: &str = "it-notice-";

fn make_create_dto(suffix: &str) -> notice_dto::CreateNoticeDto {
    notice_dto::CreateNoticeDto {
        notice_title: format!("{PREFIX}{suffix}"),
        notice_type: "1".into(),
        notice_content: Some("test content".into()),
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

#[tokio::test]
async fn create_notice() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let resp = notice_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create notice");
        assert_eq!(resp.notice_title, format!("{PREFIX}{suffix}"));
        assert_eq!(resp.notice_type, "1");

        common::cleanup_test_notices(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn find_notice_by_id() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = notice_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create");
        let found = notice_service::find_by_id(&state, &created.notice_id)
            .await
            .expect("find");
        assert_eq!(found.notice_id, created.notice_id);

        common::cleanup_test_notices(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn find_notice_not_found() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = notice_service::find_by_id(&state, "nonexistent")
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::NOTICE_NOT_FOUND, "not found");
    })
    .await;
}

#[tokio::test]
async fn update_notice() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = notice_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create");

        notice_service::update(
            &state,
            notice_dto::UpdateNoticeDto {
                notice_id: created.notice_id.clone(),
                notice_title: Some(format!("{PREFIX}updated-{suffix}")),
                notice_type: None,
                notice_content: None,
                status: None,
                remark: None,
            },
        )
        .await
        .expect("update");

        let found = notice_service::find_by_id(&state, &created.notice_id)
            .await
            .expect("find");
        assert!(found.notice_title.contains("updated"));

        common::cleanup_test_notices(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn list_notices_paginated() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        for i in 0..3 {
            let mut dto = make_create_dto(&format!("{suffix}-{i}"));
            dto.notice_title = format!("{PREFIX}{suffix}-{i}");
            notice_service::create(&state, dto).await.expect("create");
        }

        let query = notice_dto::ListNoticeDto {
            notice_title: Some(format!("{PREFIX}{suffix}")),
            notice_type: None,
            status: None,
            page: framework::response::PageQuery {
                page_num: 1,
                page_size: 2,
            },
        };
        let page = notice_service::list(&state, query).await.expect("list");
        assert_eq!(page.rows.len(), 2);
        assert!(page.total >= 3);

        common::cleanup_test_notices(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn remove_notices_batch() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let a = notice_service::create(&state, make_create_dto(&format!("{suffix}-a")))
            .await
            .expect("a");
        let mut dto_b = make_create_dto(&format!("{suffix}-b"));
        dto_b.notice_title = format!("{PREFIX}{suffix}-b");
        let b = notice_service::create(&state, dto_b).await.expect("b");

        notice_service::remove(&state, &format!("{},{}", a.notice_id, b.notice_id))
            .await
            .expect("remove");

        let err = notice_service::find_by_id(&state, &a.notice_id)
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::NOTICE_NOT_FOUND, "a gone");

        common::cleanup_test_notices(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

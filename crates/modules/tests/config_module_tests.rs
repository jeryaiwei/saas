//! Integration tests for the config module.

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::ResponseCode;
use modules::system::config::{dto as config_dto, service as config_service};

const PREFIX: &str = "it-cfg-";

fn make_create_dto(suffix: &str) -> config_dto::CreateConfigDto {
    config_dto::CreateConfigDto {
        config_name: format!("{PREFIX}name-{suffix}"),
        config_key: format!("{PREFIX}{suffix}"),
        config_value: format!("value-{suffix}"),
        config_type: "N".into(),
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
// 1. create config
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_config() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let dto = make_create_dto(suffix);
        let resp = config_service::create(&state, dto)
            .await
            .expect("create config");

        assert_eq!(resp.config_key, format!("{PREFIX}{suffix}"));
        assert_eq!(resp.config_type, "N");

        common::cleanup_test_configs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. find_by_id + find_by_key
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn find_config_by_id_and_key() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = config_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create");

        let by_id = config_service::find_by_id(&state, &created.config_id)
            .await
            .expect("find_by_id");
        assert_eq!(by_id.config_key, created.config_key);

        let by_key = config_service::find_by_key(&state, &created.config_key)
            .await
            .expect("find_by_key");
        assert_eq!(by_key.config_id, created.config_id);

        common::cleanup_test_configs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. duplicate key
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn create_config_duplicate_key() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        config_service::create(&state, make_create_dto(suffix))
            .await
            .expect("first");

        let err = config_service::create(&state, make_create_dto(suffix))
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::CONFIG_KEY_EXISTS, "dup key");

        common::cleanup_test_configs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. update config
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn update_config() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = config_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create");

        config_service::update(
            &state,
            config_dto::UpdateConfigDto {
                config_id: created.config_id.clone(),
                config_name: None,
                config_key: None,
                config_value: Some("updated-value".into()),
                config_type: None,
                status: None,
                remark: None,
            },
        )
        .await
        .expect("update");

        let found = config_service::find_by_id(&state, &created.config_id)
            .await
            .expect("find");
        assert_eq!(found.config_value, "updated-value");

        common::cleanup_test_configs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. update_by_key
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn update_config_by_key() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = config_service::create(&state, make_create_dto(suffix))
            .await
            .expect("create");

        config_service::update_by_key(
            &state,
            config_dto::UpdateConfigByKeyDto {
                config_key: created.config_key.clone(),
                config_value: "key-updated".into(),
            },
        )
        .await
        .expect("update_by_key");

        let found = config_service::find_by_key(&state, &created.config_key)
            .await
            .expect("find");
        assert_eq!(found.config_value, "key-updated");

        common::cleanup_test_configs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. list paginated
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn list_configs_paginated() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        for i in 0..3 {
            let mut dto = make_create_dto(&format!("{suffix}-{i}"));
            dto.config_name = format!("{PREFIX}list-{suffix}-{i}");
            config_service::create(&state, dto).await.expect("create");
        }

        let query = config_dto::ListConfigDto {
            config_key: Some(format!("{PREFIX}{suffix}")),
            config_name: None,
            config_type: None,
            page: framework::response::PageQuery {
                page_num: 1,
                page_size: 2,
            },
        };
        let page = config_service::list(&state, query).await.expect("list");
        assert_eq!(page.rows.len(), 2);
        assert!(page.total >= 3);

        common::cleanup_test_configs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 7. batch remove
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn remove_configs_batch() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let a = config_service::create(&state, make_create_dto(&format!("{suffix}-a")))
            .await
            .expect("create a");
        let mut dto_b = make_create_dto(&format!("{suffix}-b"));
        dto_b.config_name = format!("{PREFIX}name-{suffix}-b");
        let b = config_service::create(&state, dto_b)
            .await
            .expect("create b");

        let ids = format!("{},{}", a.config_id, b.config_id);
        config_service::remove(&state, &ids)
            .await
            .expect("batch remove");

        let err = config_service::find_by_id(&state, &a.config_id)
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::CONFIG_NOT_FOUND, "a gone");

        common::cleanup_test_configs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

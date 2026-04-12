//! Integration tests for the dict module (DictType + DictData).

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::ResponseCode;
use modules::system::dict::{dto as dict_dto, service as dict_service};

const PREFIX: &str = "it-dict-";

fn make_create_type_dto(suffix: &str) -> dict_dto::CreateDictTypeDto {
    dict_dto::CreateDictTypeDto {
        dict_name: format!("{PREFIX}name-{suffix}"),
        dict_type: format!("{PREFIX}{suffix}"),
        status: "0".into(),
        remark: None,
    }
}

fn make_create_data_dto(dict_type: &str, value: &str) -> dict_dto::CreateDictDataDto {
    dict_dto::CreateDictDataDto {
        dict_type: dict_type.into(),
        dict_label: format!("label-{value}"),
        dict_value: value.into(),
        dict_sort: 0,
        css_class: String::new(),
        list_class: String::new(),
        is_default: "N".into(),
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

// ===========================================================================
// DictType tests
// ===========================================================================

#[tokio::test]
async fn create_dict_type() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let resp = dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("create type");
        assert_eq!(resp.dict_type, format!("{PREFIX}{suffix}"));

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn create_dict_type_duplicate() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("first");

        let err = dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::DICT_TYPE_EXISTS, "dup type");

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn find_dict_type_by_id() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("create");
        let found = dict_service::find_type_by_id(&state, &created.dict_id)
            .await
            .expect("find");
        assert_eq!(found.dict_type, created.dict_type);

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn update_dict_type() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let created = dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("create");

        dict_service::update_type(
            &state,
            dict_dto::UpdateDictTypeDto {
                dict_id: created.dict_id.clone(),
                dict_name: Some(format!("{PREFIX}updated-{suffix}")),
                dict_type: None,
                status: None,
                remark: None,
            },
        )
        .await
        .expect("update");

        let found = dict_service::find_type_by_id(&state, &created.dict_id)
            .await
            .expect("find");
        assert_eq!(found.dict_name, format!("{PREFIX}updated-{suffix}"));

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn list_dict_types_paginated() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        for i in 0..3 {
            let mut dto = make_create_type_dto(&format!("{suffix}-{i}"));
            dto.dict_name = format!("{PREFIX}list-{suffix}-{i}");
            dict_service::create_type(&state, dto).await.expect("create");
        }

        let query = dict_dto::ListDictTypeDto {
            dict_type: Some(format!("{PREFIX}{suffix}")),
            dict_name: None,
            status: None,
            page: framework::response::PageQuery {
                page_num: 1,
                page_size: 2,
            },
        };
        let page = dict_service::list_types(&state, query)
            .await
            .expect("list");
        assert_eq!(page.rows.len(), 2);
        assert!(page.total >= 3);

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn remove_dict_types_batch() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let a = dict_service::create_type(&state, make_create_type_dto(&format!("{suffix}-a")))
            .await
            .expect("create a");
        let mut dto_b = make_create_type_dto(&format!("{suffix}-b"));
        dto_b.dict_name = format!("{PREFIX}name-{suffix}-b");
        let b = dict_service::create_type(&state, dto_b)
            .await
            .expect("create b");

        dict_service::remove_types(&state, &format!("{},{}", a.dict_id, b.dict_id))
            .await
            .expect("remove");

        let err = dict_service::find_type_by_id(&state, &a.dict_id)
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::DICT_TYPE_NOT_FOUND, "a gone");

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

// ===========================================================================
// DictData tests
// ===========================================================================

#[tokio::test]
async fn create_dict_data() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];
    let dict_type = format!("{PREFIX}{suffix}");

    common::as_super_admin(async {
        dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("create type");

        let data = dict_service::create_data(&state, make_create_data_dto(&dict_type, "val1"))
            .await
            .expect("create data");
        assert_eq!(data.dict_type, dict_type);
        assert_eq!(data.dict_value, "val1");

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn create_dict_data_duplicate() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];
    let dict_type = format!("{PREFIX}{suffix}");

    common::as_super_admin(async {
        dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("create type");

        dict_service::create_data(&state, make_create_data_dto(&dict_type, "dup"))
            .await
            .expect("first");

        let err = dict_service::create_data(&state, make_create_data_dto(&dict_type, "dup"))
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::DICT_DATA_EXISTS, "dup data");

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn find_dict_data_by_type() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];
    let dict_type = format!("{PREFIX}{suffix}");

    common::as_super_admin(async {
        dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("create type");

        for i in 0..3 {
            dict_service::create_data(
                &state,
                make_create_data_dto(&dict_type, &format!("v{i}")),
            )
            .await
            .expect("create data");
        }

        let by_type = dict_service::find_data_by_type(&state, &dict_type)
            .await
            .expect("find_by_type");
        assert_eq!(by_type.len(), 3);

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn remove_dict_data_batch() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];
    let dict_type = format!("{PREFIX}{suffix}");

    common::as_super_admin(async {
        dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("create type");

        let a = dict_service::create_data(&state, make_create_data_dto(&dict_type, "del-a"))
            .await
            .expect("a");
        let b = dict_service::create_data(&state, make_create_data_dto(&dict_type, "del-b"))
            .await
            .expect("b");

        dict_service::remove_data(&state, &format!("{},{}", a.dict_code, b.dict_code))
            .await
            .expect("remove");

        let err = dict_service::find_data_by_id(&state, &a.dict_code)
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::DICT_DATA_NOT_FOUND, "a gone");

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn type_option_select() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        dict_service::create_type(&state, make_create_type_dto(suffix))
            .await
            .expect("create");

        let options = dict_service::type_option_select(&state)
            .await
            .expect("option_select");
        assert!(
            options
                .iter()
                .any(|t| t.dict_type == format!("{PREFIX}{suffix}")),
            "option_select should include the test dict type"
        );

        common::cleanup_test_dicts(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

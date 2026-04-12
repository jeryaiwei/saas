//! Integration tests for the oper_log module.
//! Since oper logs are write-only from the app side (no create API), we
//! insert test rows via raw SQL and test list/find/delete.

#[path = "common/mod.rs"]
mod common;

use modules::system::oper_log::{dto as oper_log_dto, service as oper_log_service};

const PREFIX: &str = "it-operlog-";

async fn insert_test_oper_log(pool: &sqlx::PgPool, suffix: &str, tenant_id: &str) -> String {
    let row: (String,) = sqlx::query_as(
        "INSERT INTO sys_oper_log (\
            oper_id, tenant_id, title, business_type, request_method, operator_type, \
            oper_name, dept_name, oper_url, oper_location, oper_param, json_result, \
            error_msg, method, oper_ip, status, cost_time\
        ) VALUES (\
            gen_random_uuid(), $1, $2, 1, 'GET', 1, 'admin', '', '/test', '', \
            '', '', '', 'test.method', '127.0.0.1', '0', 10\
        ) RETURNING oper_id",
    )
    .bind(tenant_id)
    .bind(format!("{PREFIX}{suffix}"))
    .fetch_one(pool)
    .await
    .expect("insert test oper log");
    row.0
}

#[tokio::test]
async fn list_oper_logs() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        for i in 0..3 {
            insert_test_oper_log(&state.pg, &format!("{suffix}-{i}"), "000000").await;
        }

        let query = oper_log_dto::ListOperLogDto {
            title: Some(format!("{PREFIX}{suffix}")),
            oper_name: None,
            business_type: None,
            status: None,
            page: framework::response::PageQuery {
                page_num: 1,
                page_size: 2,
            },
        };
        let page = oper_log_service::list(&state, query).await.expect("list");
        assert_eq!(page.rows.len(), 2);
        assert!(page.total >= 3);

        common::cleanup_test_oper_logs(&state.pg, PREFIX).await;
    })
    .await;
}

#[tokio::test]
async fn find_oper_log_by_id() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let id = insert_test_oper_log(&state.pg, suffix, "000000").await;
        let found = oper_log_service::find_by_id(&state, &id)
            .await
            .expect("find");
        assert_eq!(found.oper_id, id);

        common::cleanup_test_oper_logs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn remove_oper_log() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let id = insert_test_oper_log(&state.pg, suffix, "000000").await;
        oper_log_service::remove(&state, &id).await.expect("remove");

        let err = oper_log_service::find_by_id(&state, &id).await.unwrap_err();
        match err {
            framework::error::AppError::Business { code, .. } => {
                assert_eq!(code, framework::response::ResponseCode::OPER_LOG_NOT_FOUND);
            }
            other => panic!("expected Business, got {other:?}"),
        }

        common::cleanup_test_oper_logs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

//! Integration tests for the login_log module.
//! Insert test rows via raw SQL since login logs are system-generated.

#[path = "common/mod.rs"]
mod common;

use modules::system::login_log::{dto as login_log_dto, service as login_log_service};

const PREFIX: &str = "it-loginlog-";

async fn insert_test_login_log(pool: &sqlx::PgPool, suffix: &str, tenant_id: &str) -> String {
    let row: (String,) = sqlx::query_as(
        "INSERT INTO sys_logininfor (\
            info_id, tenant_id, user_name, ipaddr, login_location, browser, \
            os, device_type, status, msg, del_flag\
        ) VALUES (\
            gen_random_uuid(), $1, $2, '127.0.0.1', 'local', 'Chrome', \
            'macOS', '0', '0', 'login ok', '0'\
        ) RETURNING info_id",
    )
    .bind(tenant_id)
    .bind(format!("{PREFIX}{suffix}"))
    .fetch_one(pool)
    .await
    .expect("insert test login log");
    row.0
}

#[tokio::test]
async fn list_login_logs() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        for i in 0..3 {
            insert_test_login_log(&state.pg, &format!("{suffix}-{i}"), "000000").await;
        }

        let query = login_log_dto::ListLoginLogDto {
            user_name: Some(format!("{PREFIX}{suffix}")),
            ipaddr: None,
            status: None,
            page: framework::response::PageQuery {
                page_num: 1,
                page_size: 2,
            },
        };
        let page = login_log_service::list(&state, query).await.expect("list");
        assert_eq!(page.rows.len(), 2);
        assert!(page.total >= 3);

        common::cleanup_test_login_logs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

#[tokio::test]
async fn remove_login_log_batch() {
    let (state, _) = common::build_state_and_router().await;
    let suffix = &uuid::Uuid::new_v4().to_string()[..8];

    common::as_super_admin(async {
        let a = insert_test_login_log(&state.pg, &format!("{suffix}-a"), "000000").await;
        let b = insert_test_login_log(&state.pg, &format!("{suffix}-b"), "000000").await;

        login_log_service::remove(&state, &format!("{a},{b}"))
            .await
            .expect("remove");

        // Soft-deleted — won't appear in list
        let query = login_log_dto::ListLoginLogDto {
            user_name: Some(format!("{PREFIX}{suffix}")),
            ipaddr: None,
            status: None,
            page: framework::response::PageQuery {
                page_num: 1,
                page_size: 10,
            },
        };
        let page = login_log_service::list(&state, query).await.expect("list");
        assert_eq!(page.total, 0, "soft-deleted rows should not appear");

        common::cleanup_test_login_logs(&state.pg, &format!("{PREFIX}{suffix}")).await;
    })
    .await;
}

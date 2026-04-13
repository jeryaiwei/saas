//! Integration tests for the SMS send module.
//!
//! These tests exercise the service layer against a real database.
//! SMS API calls are NOT tested (uses mock clients);
//! we only verify log creation, validation, and resend logic.

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::ResponseCode;
use modules::message::sms_send::{
    dto::{BatchSendSmsDto, SendSmsDto},
    service as sms_send_service,
};

fn assert_business_code(err: AppError, expected: ResponseCode, label: &str) {
    match err {
        AppError::Business { code, .. } => {
            assert_eq!(code, expected, "{label}: expected {expected}, got {code}");
        }
        AppError::BusinessWithMsg { code, .. } => {
            assert_eq!(code, expected, "{label}: expected {expected}, got {code}");
        }
        other => panic!("{label}: expected Business({expected}), got {other:?}"),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. send with nonexistent template → SMS_TEMPLATE_NOT_FOUND
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn send_template_not_found() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = sms_send_service::send(
            &state,
            SendSmsDto {
                mobile: "13800138000".into(),
                template_code: "nonexistent-sms-template".into(),
                params: None,
            },
        )
        .await
        .unwrap_err();
        assert_business_code(err, ResponseCode::SMS_TEMPLATE_NOT_FOUND, "template not found");
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. batch send exceeds limit → BATCH_SIZE_EXCEEDED
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn batch_send_exceeds_limit() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = sms_send_service::batch_send(
            &state,
            BatchSendSmsDto {
                mobiles: (0..101).map(|i| format!("1380013{i:04}")).collect(),
                template_code: "any".into(),
                params: None,
            },
        )
        .await
        .unwrap_err();
        assert_business_code(err, ResponseCode::BATCH_SIZE_EXCEEDED, "batch > 100");
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. resend nonexistent log → SEND_LOG_NOT_FOUND
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn resend_log_not_found() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = sms_send_service::resend(&state, 999999999)
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::SEND_LOG_NOT_FOUND, "log not found");
    })
    .await;
}

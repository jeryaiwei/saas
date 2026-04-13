//! Integration tests for the mail send module.
//!
//! These tests exercise the service layer against a real database.
//! SMTP sending is NOT tested (would need a real SMTP server or mock);
//! we only verify log creation, validation, and resend logic.

#[path = "common/mod.rs"]
mod common;

use framework::error::AppError;
use framework::response::ResponseCode;
use modules::message::mail_send::{
    dto::{BatchSendMailDto, SendMailDto},
    service as mail_send_service,
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
// 1. send with nonexistent template → MAIL_TEMPLATE_NOT_FOUND
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn send_template_not_found() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = mail_send_service::send(
            &state,
            SendMailDto {
                to_mail: "test@example.com".into(),
                template_code: "nonexistent-template-code".into(),
                params: None,
            },
        )
        .await
        .unwrap_err();
        assert_business_code(err, ResponseCode::MAIL_TEMPLATE_NOT_FOUND, "template not found");
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
        let err = mail_send_service::batch_send(
            &state,
            BatchSendMailDto {
                to_mails: (0..101).map(|i| format!("user{i}@example.com")).collect(),
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
        let err = mail_send_service::resend(&state, 999999999)
            .await
            .unwrap_err();
        assert_business_code(err, ResponseCode::SEND_LOG_NOT_FOUND, "log not found");
    })
    .await;
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. batch send with invalid email → PARAM_INVALID
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn batch_send_invalid_email() {
    let (state, _) = common::build_state_and_router().await;

    common::as_super_admin(async {
        let err = mail_send_service::batch_send(
            &state,
            BatchSendMailDto {
                to_mails: vec!["not-an-email".into()],
                template_code: "any".into(),
                params: None,
            },
        )
        .await
        .unwrap_err();
        assert_business_code(err, ResponseCode::PARAM_INVALID, "invalid email");
    })
    .await;
}

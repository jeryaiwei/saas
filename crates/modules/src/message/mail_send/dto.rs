//! Mail send DTOs — wire shapes for mail-send endpoints.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendMailDto {
    #[validate(email)]
    pub to_mail: String,
    #[validate(length(min = 1))]
    pub template_code: String,
    pub params: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchSendMailDto {
    #[validate(length(min = 1, max = 100))]
    pub to_mails: Vec<String>,
    #[validate(length(min = 1))]
    pub template_code: String,
    pub params: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TestMailDto {
    #[validate(email)]
    pub to_mail: String,
    pub account_id: i32,
    pub title: Option<String>,
    pub content: Option<String>,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendMailResponseDto {
    pub log_id: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchSendMailResponseDto {
    pub count: i32,
}

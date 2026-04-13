//! SMS send DTOs — wire shapes for sms-send endpoints.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendSmsDto {
    #[validate(length(min = 1, max = 20))]
    pub mobile: String,
    #[validate(length(min = 1, max = 100))]
    pub template_code: String,
    pub params: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchSendSmsDto {
    #[validate(length(min = 1, max = 100))]
    pub mobiles: Vec<String>,
    #[validate(length(min = 1, max = 100))]
    pub template_code: String,
    pub params: Option<HashMap<String, String>>,
}

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendSmsResponseDto {
    pub log_id: i64,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchSendSmsResponseDto {
    pub count: i32,
}

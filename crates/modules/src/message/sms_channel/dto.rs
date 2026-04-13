//! SMS channel DTOs — wire shapes for `sys_sms_channel` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysSmsChannel;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsChannelResponseDto {
    pub id: i32,
    pub code: String,
    pub name: String,
    pub signature: String,
    pub api_key: String,
    #[serde(skip_serializing)]
    pub api_secret: String,
    pub callback_url: Option<String>,
    pub status: String,
    pub remark: Option<String>,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
}

impl SmsChannelResponseDto {
    pub fn from_entity(c: SysSmsChannel) -> Self {
        Self {
            id: c.id,
            code: c.code,
            name: c.name,
            signature: c.signature,
            api_key: c.api_key,
            api_secret: c.api_secret,
            callback_url: c.callback_url,
            status: c.status,
            remark: c.remark,
            create_by: c.create_by,
            create_at: fmt_ts(&c.create_at),
            update_by: c.update_by,
            update_at: fmt_ts(&c.update_at),
        }
    }
}

/// Lightweight DTO for enabled dropdown select.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsChannelOptionDto {
    pub id: i32,
    pub code: String,
    pub name: String,
    pub signature: String,
}

impl SmsChannelOptionDto {
    pub fn from_entity(c: SysSmsChannel) -> Self {
        Self {
            id: c.id,
            code: c.code,
            name: c.name,
            signature: c.signature,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateSmsChannelDto {
    #[validate(length(min = 1, max = 50))]
    pub code: String,
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(length(min = 1, max = 100))]
    pub signature: String,
    #[validate(length(min = 1, max = 255))]
    pub api_key: String,
    #[validate(length(min = 1, max = 255))]
    pub api_secret: String,
    #[validate(length(max = 500))]
    pub callback_url: Option<String>,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSmsChannelDto {
    pub id: i32,
    #[validate(length(min = 1, max = 50))]
    pub code: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub signature: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub api_key: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub api_secret: Option<String>,
    pub callback_url: Option<Option<String>>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListSmsChannelDto {
    pub name: Option<String>,
    pub code: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

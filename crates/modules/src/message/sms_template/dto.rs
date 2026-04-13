//! SMS template DTOs — wire shapes for `sys_sms_template` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysSmsTemplate;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsTemplateResponseDto {
    pub id: i32,
    pub channel_id: i32,
    pub code: String,
    pub name: String,
    pub content: String,
    pub params: Option<String>,
    pub api_template_id: String,
    pub r#type: i32,
    pub status: String,
    pub remark: Option<String>,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
}

impl SmsTemplateResponseDto {
    pub fn from_entity(t: SysSmsTemplate) -> Self {
        Self {
            id: t.id,
            channel_id: t.channel_id,
            code: t.code,
            name: t.name,
            content: t.content,
            params: t.params,
            api_template_id: t.api_template_id,
            r#type: t.r#type,
            status: t.status,
            remark: t.remark,
            create_by: t.create_by,
            create_at: fmt_ts(&t.create_at),
            update_by: t.update_by,
            update_at: fmt_ts(&t.update_at),
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateSmsTemplateDto {
    pub channel_id: i32,
    #[validate(length(min = 1, max = 100))]
    pub code: String,
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    pub content: String,
    pub params: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub api_template_id: String,
    #[serde(default = "default_type")]
    pub r#type: i32,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSmsTemplateDto {
    pub id: i32,
    pub channel_id: Option<i32>,
    #[validate(length(min = 1, max = 100))]
    pub code: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    pub content: Option<String>,
    pub params: Option<Option<String>>,
    #[validate(length(min = 1, max = 100))]
    pub api_template_id: Option<String>,
    pub r#type: Option<i32>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListSmsTemplateDto {
    pub name: Option<String>,
    pub code: Option<String>,
    pub channel_id: Option<i32>,
    pub r#type: Option<i32>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

fn default_type() -> i32 {
    1
}

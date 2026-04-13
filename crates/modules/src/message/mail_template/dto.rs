//! Mail template DTOs — wire shapes for `sys_mail_template` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysMailTemplate;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MailTemplateResponseDto {
    pub id: i32,
    pub name: String,
    pub code: String,
    pub account_id: i32,
    pub nickname: String,
    pub title: String,
    pub content: String,
    pub params: Option<String>,
    pub status: String,
    pub remark: Option<String>,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
}

impl MailTemplateResponseDto {
    pub fn from_entity(t: SysMailTemplate) -> Self {
        Self {
            id: t.id,
            name: t.name,
            code: t.code,
            account_id: t.account_id,
            nickname: t.nickname,
            title: t.title,
            content: t.content,
            params: t.params,
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
pub struct CreateMailTemplateDto {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(length(min = 1, max = 100))]
    pub code: String,
    pub account_id: i32,
    #[validate(length(min = 1, max = 100))]
    pub nickname: String,
    #[validate(length(min = 1, max = 255))]
    pub title: String,
    pub content: String,
    pub params: Option<String>,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMailTemplateDto {
    pub id: i32,
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub code: Option<String>,
    pub account_id: Option<i32>,
    #[validate(length(min = 1, max = 100))]
    pub nickname: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub title: Option<String>,
    pub content: Option<String>,
    pub params: Option<Option<String>>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListMailTemplateDto {
    pub name: Option<String>,
    pub code: Option<String>,
    pub account_id: Option<i32>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

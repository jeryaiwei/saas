//! Mail account DTOs — wire shapes for `sys_mail_account` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysMailAccount;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MailAccountResponseDto {
    pub id: i32,
    pub mail: String,
    pub username: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub host: String,
    pub port: i32,
    pub ssl_enable: bool,
    pub status: String,
    pub remark: Option<String>,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
}

impl MailAccountResponseDto {
    pub fn from_entity(a: SysMailAccount) -> Self {
        Self {
            id: a.id,
            mail: a.mail,
            username: a.username,
            password: a.password,
            host: a.host,
            port: a.port,
            ssl_enable: a.ssl_enable,
            status: a.status,
            remark: a.remark,
            create_by: a.create_by,
            create_at: fmt_ts(&a.create_at),
            update_by: a.update_by,
            update_at: fmt_ts(&a.update_at),
        }
    }
}

/// Lightweight DTO for enabled accounts dropdown.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MailAccountOptionDto {
    pub id: i32,
    pub mail: String,
    pub username: String,
}

impl MailAccountOptionDto {
    pub fn from_entity(a: SysMailAccount) -> Self {
        Self {
            id: a.id,
            mail: a.mail,
            username: a.username,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateMailAccountDto {
    #[validate(length(min = 1, max = 255), email)]
    pub mail: String,
    #[validate(length(min = 1, max = 255))]
    pub username: String,
    #[validate(length(min = 1, max = 255))]
    pub password: String,
    #[validate(length(min = 1, max = 255))]
    pub host: String,
    pub port: i32,
    #[serde(default)]
    pub ssl_enable: bool,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMailAccountDto {
    pub id: i32,
    #[validate(length(min = 1, max = 255), email)]
    pub mail: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub username: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub password: Option<String>,
    #[validate(length(min = 1, max = 255))]
    pub host: Option<String>,
    pub port: Option<i32>,
    pub ssl_enable: Option<bool>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListMailAccountDto {
    pub mail: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

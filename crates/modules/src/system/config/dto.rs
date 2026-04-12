//! Config DTOs — wire shapes for `sys_config` endpoints.

use crate::domain::validators::{default_status, validate_config_type, validate_status_flag};
use crate::domain::SysConfig;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponseDto {
    pub config_id: String,
    pub config_name: String,
    pub config_key: String,
    pub config_value: String,
    pub config_type: String,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl ConfigResponseDto {
    pub fn from_entity(c: SysConfig) -> Self {
        Self {
            config_id: c.config_id,
            config_name: c.config_name,
            config_key: c.config_key,
            config_value: c.config_value,
            config_type: c.config_type,
            status: c.status,
            create_by: c.create_by,
            create_at: fmt_ts(&c.create_at),
            update_by: c.update_by,
            update_at: fmt_ts(&c.update_at),
            remark: c.remark,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateConfigDto {
    #[validate(length(min = 1, max = 100))]
    pub config_name: String,
    #[validate(length(min = 1, max = 100))]
    pub config_key: String,
    pub config_value: String,
    /// "Y" = system built-in, "N" = user-defined
    #[serde(default = "default_config_type")]
    #[validate(custom(function = "validate_config_type"))]
    pub config_type: String,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

fn default_config_type() -> String {
    "N".into()
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigDto {
    pub config_id: String,
    #[validate(length(min = 1, max = 100))]
    pub config_name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub config_key: Option<String>,
    pub config_value: Option<String>,
    #[validate(custom(function = "validate_config_type"))]
    pub config_type: Option<String>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

/// Update config value by key.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateConfigByKeyDto {
    #[validate(length(min = 1, max = 100))]
    pub config_key: String,
    pub config_value: String,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListConfigDto {
    pub config_name: Option<String>,
    pub config_key: Option<String>,
    pub config_type: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

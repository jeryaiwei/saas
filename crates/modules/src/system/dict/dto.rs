//! Dict DTOs — wire shapes for `sys_dict_type` and `sys_dict_data` endpoints.

use crate::domain::validators::{default_status, validate_status_flag, validate_yes_no_flag};
use crate::domain::{SysDictData, SysDictType};
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ===========================================================================
// DictType Response / Request DTOs
// ===========================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictTypeResponseDto {
    pub dict_id: String,
    pub dict_name: String,
    pub dict_type: String,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl DictTypeResponseDto {
    pub fn from_entity(d: SysDictType) -> Self {
        Self {
            dict_id: d.dict_id,
            dict_name: d.dict_name,
            dict_type: d.dict_type,
            status: d.status,
            create_by: d.create_by,
            create_at: fmt_ts(&d.create_at),
            update_by: d.update_by,
            update_at: fmt_ts(&d.update_at),
            remark: d.remark,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateDictTypeDto {
    #[validate(length(min = 1, max = 100))]
    pub dict_name: String,
    #[validate(length(min = 1, max = 100))]
    pub dict_type: String,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDictTypeDto {
    pub dict_id: String,
    #[validate(length(min = 1, max = 100))]
    pub dict_name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub dict_type: Option<String>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListDictTypeDto {
    pub dict_name: Option<String>,
    pub dict_type: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

// ===========================================================================
// DictData Response / Request DTOs
// ===========================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictDataResponseDto {
    pub dict_code: String,
    pub dict_sort: i32,
    pub dict_label: String,
    pub dict_value: String,
    pub dict_type: String,
    pub css_class: String,
    pub list_class: String,
    pub is_default: String,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl DictDataResponseDto {
    pub fn from_entity(d: SysDictData) -> Self {
        Self {
            dict_code: d.dict_code,
            dict_sort: d.dict_sort,
            dict_label: d.dict_label,
            dict_value: d.dict_value,
            dict_type: d.dict_type,
            css_class: d.css_class,
            list_class: d.list_class,
            is_default: d.is_default,
            status: d.status,
            create_by: d.create_by,
            create_at: fmt_ts(&d.create_at),
            update_by: d.update_by,
            update_at: fmt_ts(&d.update_at),
            remark: d.remark,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateDictDataDto {
    #[validate(length(min = 1, max = 100))]
    pub dict_type: String,
    #[validate(length(min = 1, max = 100))]
    pub dict_label: String,
    #[validate(length(min = 1, max = 100))]
    pub dict_value: String,
    #[serde(default)]
    pub dict_sort: i32,
    #[serde(default)]
    pub css_class: String,
    #[serde(default)]
    pub list_class: String,
    #[serde(default = "default_is_default")]
    #[validate(custom(function = "validate_yes_no_flag"))]
    pub is_default: String,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

fn default_is_default() -> String {
    "N".into()
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDictDataDto {
    pub dict_code: String,
    #[validate(length(min = 1, max = 100))]
    pub dict_label: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub dict_value: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub dict_type: Option<String>,
    pub dict_sort: Option<i32>,
    pub css_class: Option<String>,
    pub list_class: Option<String>,
    #[validate(custom(function = "validate_yes_no_flag"))]
    pub is_default: Option<String>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListDictDataDto {
    pub dict_type: Option<String>,
    pub dict_label: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

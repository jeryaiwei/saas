//! OperLog DTOs — wire shapes for `sys_oper_log` endpoints.

use crate::domain::entities::SysOperLog;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperLogResponseDto {
    pub oper_id: String,
    pub tenant_id: String,
    pub title: String,
    pub business_type: i32,
    pub request_method: String,
    pub operator_type: i32,
    pub oper_name: String,
    pub dept_name: String,
    pub oper_url: String,
    pub oper_location: String,
    pub oper_param: String,
    pub json_result: String,
    pub error_msg: String,
    pub method: String,
    pub oper_ip: String,
    pub oper_time: String,
    pub status: String,
    pub cost_time: i32,
}

impl OperLogResponseDto {
    pub fn from_entity(e: SysOperLog) -> Self {
        Self {
            oper_id: e.oper_id,
            tenant_id: e.tenant_id,
            title: e.title,
            business_type: e.business_type,
            request_method: e.request_method,
            operator_type: e.operator_type,
            oper_name: e.oper_name,
            dept_name: e.dept_name,
            oper_url: e.oper_url,
            oper_location: e.oper_location,
            oper_param: e.oper_param,
            json_result: e.json_result,
            error_msg: e.error_msg,
            method: e.method,
            oper_ip: e.oper_ip,
            oper_time: fmt_ts(&e.oper_time),
            status: e.status,
            cost_time: e.cost_time,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListOperLogDto {
    pub title: Option<String>,
    pub oper_name: Option<String>,
    pub business_type: Option<i32>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

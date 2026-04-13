//! Audit log DTOs — wire shapes for tenant audit log endpoints.

use crate::domain::SysAuditLog;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// Full audit log detail.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogResponseDto {
    pub id: String,
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    pub action: String,
    pub module: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub ip: String,
    pub user_agent: Option<String>,
    pub request_id: Option<String>,
    pub status: String,
    pub error_msg: Option<String>,
    pub duration: i32,
    pub create_at: String,
}

impl AuditLogResponseDto {
    pub fn from_entity(e: SysAuditLog) -> Self {
        Self {
            id: e.id,
            tenant_id: e.tenant_id,
            user_id: e.user_id,
            user_name: e.user_name,
            action: e.action,
            module: e.module,
            target_type: e.target_type,
            target_id: e.target_id,
            old_value: e.old_value,
            new_value: e.new_value,
            ip: e.ip,
            user_agent: e.user_agent,
            request_id: e.request_id,
            status: e.status,
            error_msg: e.error_msg,
            duration: e.duration,
            create_at: fmt_ts(&e.create_at),
        }
    }
}

/// Stats summary response.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogStatsDto {
    pub total: i64,
    pub today_count: i64,
    pub action_counts: HashMap<String, i64>,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Query string for `GET /system/tenant-audit/list`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListAuditLogDto {
    #[validate(length(max = 50))]
    pub action: Option<String>,
    #[validate(length(max = 50))]
    pub module: Option<String>,
    #[validate(length(max = 50))]
    pub user_name: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

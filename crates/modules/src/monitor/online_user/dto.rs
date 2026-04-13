//! Online user DTOs.

use serde::Serialize;

/// Online user item from Redis session.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OnlineUserResponseDto {
    pub token_id: String,
    pub user_id: String,
    pub user_name: String,
    pub user_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_id: Option<String>,
    pub is_admin: bool,
}

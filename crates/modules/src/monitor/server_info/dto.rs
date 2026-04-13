//! Server info DTOs.

use serde::Serialize;

/// Server monitoring information.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfoDto {
    pub cpu_cores: usize,
    pub memory_total_mb: u64,
    pub memory_used_mb: u64,
    pub memory_usage_percent: f64,
    pub uptime_secs: u64,
    pub rust_version: String,
    pub os_info: String,
    pub arch: String,
}

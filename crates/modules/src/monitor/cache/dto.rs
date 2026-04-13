//! Cache monitor DTOs.

use serde::Serialize;

/// Redis server info snapshot.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfoDto {
    pub redis_version: String,
    pub used_memory: String,
    pub used_memory_human: String,
    pub connected_clients: i64,
    pub maxmemory_human: String,
    pub uptime_in_days: i64,
    pub db_size: i64,
}

/// Cache name category.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheNameDto {
    pub cache_name: String,
    pub remark: String,
}

/// A single cache key.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheKeyDto {
    pub cache_key: String,
}

/// Cache key with its value and TTL.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CacheValueDto {
    pub cache_key: String,
    pub cache_value: String,
    pub ttl: i64,
}

//! Redis-backed user session (the "fat" half of the thin-JWT/fat-session model).
//!
//! Keys (all prefixes configurable via [`crate::config::RedisKeyConfig`]):
//!
//! | Purpose                      | Key format                       | TTL           |
//! | ---------------------------- | -------------------------------- | ------------- |
//! | Session payload              | `login_token_session:{uuid}`     | JWT lifetime  |
//! | Single-token blacklist       | `token_blacklist:{uuid}`         | 24 h          |
//! | Per-user token version       | `user_token_version:{user_id}`   | 7 d           |
//!
//! These match NestJS exactly so the Rust service can share Redis with the
//! legacy backend during the progressive-migration window.

use crate::config::RedisKeyConfig;
use crate::infra::redis::{RedisExt, RedisPool};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSession {
    pub user_id: String,
    pub user_name: String,
    /// `"10"` = CUSTOM (backend) / `"20"` = CLIENT (C-end).
    pub user_type: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub platform_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sys_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub is_admin: bool,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

/// Fetch the session payload for a given JWT `uuid`. Returns `Ok(None)` if
/// the session key has expired or never existed.
#[tracing::instrument(skip_all, fields(uuid = %uuid))]
pub async fn fetch(
    pool: &RedisPool,
    keys: &RedisKeyConfig,
    uuid: &str,
) -> anyhow::Result<Option<UserSession>> {
    let key = format!("{}{}", keys.login_session, uuid);
    pool.get_json(&key).await
}

/// Write (or replace) the session payload under the JWT `uuid`.
#[tracing::instrument(skip_all, fields(uuid = %uuid, user_id = %session.user_id, ttl_sec))]
pub async fn store(
    pool: &RedisPool,
    keys: &RedisKeyConfig,
    uuid: &str,
    session: &UserSession,
    ttl_sec: u64,
) -> anyhow::Result<()> {
    let key = format!("{}{}", keys.login_session, uuid);
    pool.set_ex(&key, session, ttl_sec).await
}

/// Delete the session (logout).
#[tracing::instrument(skip_all, fields(uuid = %uuid))]
pub async fn delete(pool: &RedisPool, keys: &RedisKeyConfig, uuid: &str) -> anyhow::Result<()> {
    let key = format!("{}{}", keys.login_session, uuid);
    pool.del(&key).await
}

/// Add a token `uuid` to the single-token blacklist (logout of one device).
#[tracing::instrument(skip_all, fields(uuid = %uuid, ttl_sec))]
pub async fn blacklist(
    pool: &RedisPool,
    keys: &RedisKeyConfig,
    uuid: &str,
    ttl_sec: u64,
) -> anyhow::Result<()> {
    let key = format!("{}{}", keys.token_blacklist, uuid);
    let now_ms = chrono::Utc::now().timestamp_millis().to_string();
    pool.set_ex_raw(&key, &now_ms, ttl_sec).await
}

/// Check whether a token `uuid` has been blacklisted.
#[tracing::instrument(skip_all, fields(uuid = %uuid))]
pub async fn is_blacklisted(
    pool: &RedisPool,
    keys: &RedisKeyConfig,
    uuid: &str,
) -> anyhow::Result<bool> {
    let key = format!("{}{}", keys.token_blacklist, uuid);
    pool.exists(&key).await
}

/// Read the current per-user token version (used to invalidate all tokens
/// of a user at once, e.g. on password change).
#[tracing::instrument(skip_all, fields(user_id = %user_id))]
pub async fn get_user_token_version(
    pool: &RedisPool,
    keys: &RedisKeyConfig,
    user_id: &str,
) -> anyhow::Result<Option<i64>> {
    let key = format!("{}{}", keys.user_token_version, user_id);
    let v = pool.get_raw(&key).await?;
    Ok(v.and_then(|s| s.parse().ok()))
}

/// Atomically increment the per-user token version counter, invalidating
/// all existing JWTs that carry an older version. Used by password reset
/// and similar "force logout all devices" flows.
///
/// Sets a 7-day TTL on the key to match the max JWT lifetime — after
/// which the counter can safely reset since no outstanding tokens exist.
/// Returns the new version value.
#[tracing::instrument(skip_all, fields(user_id = %user_id))]
pub async fn bump_user_token_version(
    pool: &RedisPool,
    keys: &RedisKeyConfig,
    user_id: &str,
) -> anyhow::Result<i64> {
    let key = format!("{}{}", keys.user_token_version, user_id);
    // INCR + set 7-day TTL so stale counters get garbage collected
    // after all tokens they could possibly apply to have expired.
    pool.incr_ex(&key, 604800).await
}

// ─── Tenant switch original state ────────────────────────────────────────

/// Snapshot of session state before a tenant switch. Stored in Redis
/// under `tenant:switch:original:{uuid}` so `dynamic/clear` can restore
/// without recalculating permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwitchOriginal {
    pub tenant_id: String,
    pub is_admin: bool,
    pub permissions: Vec<String>,
    pub sys_code: Option<String>,
    pub switched_at: String,
}

const SWITCH_ORIGINAL_PREFIX: &str = "tenant:switch:original:";

/// Save the pre-switch state so `dynamic/clear` can restore it.
#[tracing::instrument(skip_all, fields(uuid = %uuid))]
pub async fn store_switch_original(
    pool: &RedisPool,
    uuid: &str,
    original: &SwitchOriginal,
    ttl_sec: u64,
) -> anyhow::Result<()> {
    let key = format!("{SWITCH_ORIGINAL_PREFIX}{uuid}");
    pool.set_ex(&key, original, ttl_sec).await
}

/// Fetch the pre-switch state. Returns `None` if user hasn't switched.
#[tracing::instrument(skip_all, fields(uuid = %uuid))]
pub async fn fetch_switch_original(
    pool: &RedisPool,
    uuid: &str,
) -> anyhow::Result<Option<SwitchOriginal>> {
    let key = format!("{SWITCH_ORIGINAL_PREFIX}{uuid}");
    pool.get_json(&key).await
}

/// Delete the pre-switch state after restore.
#[tracing::instrument(skip_all, fields(uuid = %uuid))]
pub async fn delete_switch_original(pool: &RedisPool, uuid: &str) -> anyhow::Result<()> {
    let key = format!("{SWITCH_ORIGINAL_PREFIX}{uuid}");
    pool.del(&key).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_serializes_camel_case() {
        let s = UserSession {
            user_id: "u1".into(),
            user_name: "alice".into(),
            user_type: "10".into(),
            tenant_id: Some("t0".into()),
            platform_id: Some("p0".into()),
            sys_code: None,
            lang: Some("zh-CN".into()),
            is_admin: true,
            permissions: vec!["system:user:list".into()],
            roles: vec!["admin".into()],
        };
        let json = serde_json::to_value(&s).unwrap();
        assert!(json.get("userId").is_some());
        assert!(json.get("userName").is_some());
        assert!(json.get("userType").is_some());
        assert!(json.get("tenantId").is_some());
        assert!(json.get("platformId").is_some());
        assert!(json.get("isAdmin").is_some());
        assert!(json.get("permissions").is_some());
    }

    #[test]
    fn session_deserializes_from_nestjs_shape() {
        let nest_json = r#"{
            "userId": "u-1",
            "userName": "bob",
            "userType": "20",
            "tenantId": "t-1",
            "platformId": "t-1",
            "isAdmin": false,
            "permissions": ["a", "b"]
        }"#;
        let s: UserSession = serde_json::from_str(nest_json).unwrap();
        assert_eq!(s.user_id, "u-1");
        assert_eq!(s.user_type, "20");
        assert_eq!(s.permissions.len(), 2);
        assert_eq!(s.roles.len(), 0); // default
    }
}

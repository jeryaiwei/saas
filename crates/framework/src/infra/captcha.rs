//! Captcha — Phase 0 stub.
//!
//! Generates a random 4-digit numeric code, stores it in Redis under the key
//! `{redis_keys.captcha}{uuid}` with TTL `{redis_ttl.captcha}` seconds,
//! returning `{uuid, text}`. The key prefix and TTL match NestJS
//! (`captcha_code:` / 300 seconds) so the Rust service can share Redis with
//! the NestJS server during cross-service testing.
//!
//! Phase 0 does NOT render an SVG image — the stub returns an empty string for
//! the image field so `/auth/code` can still satisfy its schema. A real
//! renderer (Phase 3 or wherever image support becomes necessary) should
//! replace [`CaptchaCode::image`] with an actual SVG/PNG payload.

use crate::config::{RedisKeyConfig, RedisTtlConfig};
use crate::infra::redis::{RedisExt, RedisPool};

#[derive(Debug, Clone)]
pub struct CaptchaCode {
    pub uuid: String,
    pub text: String,
    /// Base64 SVG image. Phase 0 stub returns empty string; see module docs.
    pub image: String,
}

/// Generate a new captcha and persist it to Redis.
pub async fn generate_and_store(
    pool: &RedisPool,
    keys: &RedisKeyConfig,
    ttl: &RedisTtlConfig,
) -> anyhow::Result<CaptchaCode> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let text: String = (0..4)
        .map(|_| rand::random_range(0..10).to_string())
        .collect();
    let key = format!("{}{}", keys.captcha, uuid);
    pool.set_ex_raw(&key, &text, ttl.captcha).await?;

    Ok(CaptchaCode {
        uuid,
        text,
        image: String::new(),
    })
}

/// Verify and consume a captcha code. Returns `true` on match, and always
/// deletes the key afterwards so it is single-use.
pub async fn verify_and_consume(
    pool: &RedisPool,
    keys: &RedisKeyConfig,
    uuid: &str,
    input: &str,
) -> anyhow::Result<bool> {
    let key = format!("{}{}", keys.captcha, uuid);
    let stored = pool.get_raw(&key).await?;
    // Single-use — delete regardless of match outcome.
    pool.del(&key).await?;

    Ok(match stored {
        Some(expected) => expected.eq_ignore_ascii_case(input),
        None => false,
    })
}

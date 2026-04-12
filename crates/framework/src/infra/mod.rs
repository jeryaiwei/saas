//! Infrastructure adapters: PostgreSQL pool, Redis pool, crypto, captcha.
//!
//! These are thin wrappers around external crates (sqlx / deadpool-redis /
//! bcrypt) that:
//! - construct pools from [`crate::config`] types,
//! - expose `ping` for health checks,
//! - avoid leaking external types into higher layers where possible.

pub mod captcha;
pub mod crypto;
pub mod pg;
pub mod redis;

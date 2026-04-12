//! Password hashing + verification via bcrypt.
//!
//! NestJS (per Gate 0 verification) uses standard bcrypt with the `$2b$`
//! variant stored in `SysUser.password`. The Rust `bcrypt` crate accepts both
//! `$2a$` and `$2b$`, so hashes written by NestJS must verify here and vice
//! versa — this is the prerequisite for cross-service login compatibility
//! during the progressive migration.
//!
//! ⚠️ Cross-compat smoke test: Gate 6 must feed a **real** hash from the NestJS
//! `sys_user.password` column through [`verify_password`] with the known
//! plaintext to prove end-to-end compatibility. The unit tests in this file
//! only cover round-trip correctness of this crate.

use bcrypt::{hash, verify, DEFAULT_COST};

#[tracing::instrument(skip_all, name = "infra.crypto.hash_password")]
pub fn hash_password(plain: &str) -> anyhow::Result<String> {
    hash(plain, DEFAULT_COST).map_err(|e| anyhow::anyhow!("bcrypt hash: {e}"))
}

pub fn hash_password_with_cost(plain: &str, cost: u32) -> anyhow::Result<String> {
    hash(plain, cost).map_err(|e| anyhow::anyhow!("bcrypt hash: {e}"))
}

/// Returns `true` iff `plain` matches the given bcrypt hash.
///
/// Errors from the underlying library (e.g. malformed hash) are logged at
/// WARN and reported as `Ok(false)` so that callers can treat them as a
/// generic "invalid credentials" response without leaking details.
pub fn verify_password(plain: &str, hash_str: &str) -> bool {
    match verify(plain, hash_str) {
        Ok(ok) => ok,
        Err(e) => {
            tracing::warn!(error = %e, "bcrypt verify error (treating as false)");
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_verify_ok() {
        let h = hash_password_with_cost("Admin@123", 4).unwrap();
        assert!(verify_password("Admin@123", &h));
    }

    #[test]
    fn wrong_password_returns_false() {
        let h = hash_password_with_cost("Admin@123", 4).unwrap();
        assert!(!verify_password("Admin@456", &h));
    }

    #[test]
    fn malformed_hash_returns_false_not_panic() {
        assert!(!verify_password("anything", "not-a-bcrypt-hash"));
    }

    #[test]
    fn hash_produces_bcrypt_2b_prefix() {
        // NestJS writes `$2b$...` — the Rust bcrypt crate should too.
        let h = hash_password_with_cost("x", 4).unwrap();
        assert!(h.starts_with("$2b$"), "hash does not start with $2b$: {h}");
    }
}

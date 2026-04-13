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

use aes::Aes256;
use bcrypt::{hash, verify, DEFAULT_COST};
use cbc::cipher::{block_padding::Pkcs7, BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use scrypt::{scrypt, Params as ScryptParams};

type Aes256CbcEnc = cbc::Encryptor<Aes256>;
type Aes256CbcDec = cbc::Decryptor<Aes256>;

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

// ─── AES-256-CBC (NestJS CryptoHelper compat) ──────────────────────────────

/// Derive a 32-byte key from `secret_key` using scrypt with the same
/// parameters as Node.js `crypto.scryptSync(key, 'salt', 32)`.
fn derive_aes_key(secret_key: &str) -> anyhow::Result<[u8; 32]> {
    // Node.js scryptSync defaults: N = 2^14 = 16384, r = 8, p = 1
    let params = ScryptParams::new(14, 8, 1, 32)
        .map_err(|e| anyhow::anyhow!("scrypt params: {e}"))?;
    let mut key = [0u8; 32];
    scrypt(secret_key.as_bytes(), b"salt", &params, &mut key)
        .map_err(|e| anyhow::anyhow!("scrypt derive: {e}"))?;
    Ok(key)
}

/// Check if a string looks like it was encrypted by NestJS CryptoHelper.
/// Format: `"{32 hex chars}:{hex ciphertext}"`
pub fn is_encrypted(text: &str) -> bool {
    let Some((iv_part, ct_part)) = text.split_once(':') else {
        return false;
    };
    iv_part.len() == 32
        && !ct_part.is_empty()
        && iv_part.bytes().all(|b| b.is_ascii_hexdigit())
        && ct_part.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Decrypt a value encrypted by NestJS CryptoHelper (AES-256-CBC).
///
/// Key derivation: `scrypt(secret_key, b"salt", 32 bytes)`.
/// Ciphertext format: `"iv_hex:ciphertext_hex"`.
///
/// Returns the original text unchanged if it doesn't look encrypted.
pub fn decrypt_aes256cbc(encrypted_text: &str, secret_key: &str) -> anyhow::Result<String> {
    if !is_encrypted(encrypted_text) {
        return Ok(encrypted_text.to_string());
    }

    let (iv_hex, ct_hex) = encrypted_text
        .split_once(':')
        .expect("is_encrypted guarantees a colon");

    let iv_bytes = hex::decode(iv_hex).map_err(|e| anyhow::anyhow!("IV hex decode: {e}"))?;
    let iv: [u8; 16] = iv_bytes
        .try_into()
        .map_err(|_| anyhow::anyhow!("IV must be 16 bytes"))?;
    let ciphertext =
        hex::decode(ct_hex).map_err(|e| anyhow::anyhow!("ciphertext hex decode: {e}"))?;

    let key = derive_aes_key(secret_key)?;

    let plaintext = Aes256CbcDec::new(&key.into(), &iv.into())
        .decrypt_padded_vec::<Pkcs7>(&ciphertext)
        .map_err(|e| anyhow::anyhow!("AES-256-CBC decrypt: {e}"))?;

    String::from_utf8(plaintext).map_err(|e| anyhow::anyhow!("UTF-8 decode: {e}"))
}

/// Encrypt a value using the same algorithm as NestJS CryptoHelper.
///
/// Output format: `"iv_hex:ciphertext_hex"`.
pub fn encrypt_aes256cbc(plain_text: &str, secret_key: &str) -> anyhow::Result<String> {
    let mut iv = [0u8; 16];
    rand::fill(&mut iv);

    let key = derive_aes_key(secret_key)?;

    let ciphertext = Aes256CbcEnc::new(&key.into(), &iv.into())
        .encrypt_padded_vec::<Pkcs7>(plain_text.as_bytes());

    Ok(format!("{}:{}", hex::encode(iv), hex::encode(ciphertext)))
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

    // ─── AES-256-CBC tests ──────────────────────────────────────────────────

    #[test]
    fn is_encrypted_valid() {
        // 32 hex chars : some hex
        assert!(is_encrypted(
            "0123456789abcdef0123456789abcdef:deadbeef"
        ));
    }

    #[test]
    fn is_encrypted_plain_text() {
        assert!(!is_encrypted("plain-password"));
        assert!(!is_encrypted(""));
        assert!(!is_encrypted("short:abc"));
    }

    #[test]
    fn decrypt_plain_text_returns_as_is() {
        let result = decrypt_aes256cbc("not-encrypted", "key").unwrap();
        assert_eq!(result, "not-encrypted");
    }

    #[test]
    fn aes256cbc_round_trip() {
        let key = "mail-password-encryption-key-32b";
        let plain = "smtp-password-123";
        let encrypted = encrypt_aes256cbc(plain, key).unwrap();
        assert!(is_encrypted(&encrypted));
        let decrypted = decrypt_aes256cbc(&encrypted, key).unwrap();
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn aes256cbc_round_trip_unicode() {
        let key = "mail-password-encryption-key-32b";
        let plain = "密码测试!@#$%^&*()";
        let encrypted = encrypt_aes256cbc(plain, key).unwrap();
        let decrypted = decrypt_aes256cbc(&encrypted, key).unwrap();
        assert_eq!(decrypted, plain);
    }

    #[test]
    fn aes256cbc_wrong_key_fails() {
        let encrypted =
            encrypt_aes256cbc("secret", "mail-password-encryption-key-32b").unwrap();
        let result = decrypt_aes256cbc(&encrypted, "wrong-key-that-is-also-32-bytes!");
        assert!(result.is_err());
    }

    #[test]
    fn aes256cbc_decrypt_nestjs_ciphertext() {
        // Ciphertext generated by NestJS CryptoHelper.encrypt():
        //   key  = "mail-password-encryption-key-32b"
        //   plain = "test-smtp-password"
        //   Node.js: crypto.scryptSync(key, 'salt', 32) + AES-256-CBC
        let nestjs_encrypted =
            "a3309202da1da3410b9c381345db7ff1:9de6ec72fec22ca181fcb8baedfc260c24f87dd877958881844ecb9c674a8f1d";
        let key = "mail-password-encryption-key-32b";
        let decrypted = decrypt_aes256cbc(nestjs_encrypted, key).unwrap();
        assert_eq!(decrypted, "test-smtp-password");
    }
}

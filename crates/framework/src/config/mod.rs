//! Strongly-typed application config.
//!
//! Load order: `config/default.yaml` -> `config/{APP_ENV}.yaml` (optional)
//! -> environment variables (prefix `APP__`, separator `__`).
//!
//! Example: `APP__DB__POSTGRESQL__URL=...` maps to `db.postgresql.url`.
//!
//! # Security guarantees (v1.0 config validation)
//!
//! `AppConfig::load()` runs fail-fast validation on security-sensitive
//! fields and refuses to return a config that would leave the server
//! vulnerable. Specifically:
//!
//! - **JWT secret**: must be at least 32 bytes (256 bits for HS256) AND
//!   not match a blacklist of known default values. A service started
//!   with `jwt.secret = "change_me_in_production"` **will not boot**.
//! - **JWT expiry**: bounded to ≤ 30 days — long-lived access tokens
//!   are an anti-pattern (use refresh tokens for session extension).
//! - **Redis key prefixes**: all `redis_keys.*` fields must end with `:`
//!   so composed keys like `{prefix}{uuid}` don't silently partition
//!   the keyspace away from the NestJS side.
//!
//! `PostgresConfig`, `RedisConfig`, and `JwtConfig` also override `Debug`
//! to redact credentials. A `tracing::debug!("{:?}", cfg)` call writing
//! to a log aggregator will produce `postgresql://***:***@host:port/db`
//! rather than leaking the real password.

use anyhow::Context;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub db: DbConfig,
    pub jwt: JwtConfig,
    pub tenant: TenantConfig,
    pub cors: CorsConfig,
    pub logger: LoggerConfig,
    pub redis_keys: RedisKeyConfig,
    pub redis_ttl: RedisTtlConfig,
    #[serde(default)]
    pub mail: MailConfig,
    #[serde(default)]
    pub upload: UploadConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    pub postgresql: PostgresConfig,
    pub redis: RedisConfig,
}

// `Debug` is hand-written below to redact URL credentials. Derive
// `Clone + Deserialize` only.
#[derive(Clone, Deserialize)]
pub struct PostgresConfig {
    pub url: String,
    pub max_connections: u32,
    #[serde(default = "default_acquire_timeout")]
    pub acquire_timeout_sec: u64,
    #[serde(default = "default_idle_timeout")]
    pub idle_timeout_sec: u64,
}

impl std::fmt::Debug for PostgresConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PostgresConfig")
            .field("url", &redact_url_creds(&self.url))
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout_sec", &self.acquire_timeout_sec)
            .field("idle_timeout_sec", &self.idle_timeout_sec)
            .finish()
    }
}

fn default_acquire_timeout() -> u64 {
    30
}
fn default_idle_timeout() -> u64 {
    300
}

// `Debug` is hand-written below to redact URL credentials.
#[derive(Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

impl std::fmt::Debug for RedisConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RedisConfig")
            .field("url", &redact_url_creds(&self.url))
            .field("pool_size", &self.pool_size)
            .finish()
    }
}

// `Debug` is hand-written below to redact the JWT signing secret.
#[derive(Clone, Deserialize)]
pub struct JwtConfig {
    pub secret: String,
    pub expires_in_sec: i64,
    pub refresh_expires_in_sec: i64,
}

impl std::fmt::Debug for JwtConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtConfig")
            .field("secret", &"***")
            .field("expires_in_sec", &self.expires_in_sec)
            .field("refresh_expires_in_sec", &self.refresh_expires_in_sec)
            .finish()
    }
}

/// Redact credentials from a URL like `postgresql://user:pw@host:port/db`
/// → `postgresql://***:***@host:port/db`. Preserves the host portion so
/// ops can still identify **which** server is configured when reading a
/// Debug dump. Also handles the `redis://:pw@host/...` form (no user,
/// only password) and the no-auth form (pass-through).
fn redact_url_creds(url: &str) -> String {
    let Some(scheme_end) = url.find("://") else {
        return url.to_string();
    };
    let after_scheme = scheme_end + 3;
    let Some(at_idx_rel) = url[after_scheme..].find('@') else {
        return url.to_string(); // no credentials to redact
    };
    let at_idx = after_scheme + at_idx_rel;

    format!("{}***:***{}", &url[..after_scheme], &url[at_idx..])
}

#[derive(Debug, Clone, Deserialize)]
pub struct TenantConfig {
    pub enabled: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CorsConfig {
    #[serde(default, deserialize_with = "deserialize_csv")]
    pub origins: Vec<String>,
    #[serde(default)]
    pub app_domain: String,
}

/// Accept both a comma-separated string (env var) and an array (yaml).
fn deserialize_csv<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Either {
        S(String),
        V(Vec<String>),
    }
    match Either::deserialize(d)? {
        Either::S(s) => Ok(s
            .split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect()),
        Either::V(v) => Ok(v),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggerConfig {
    pub level: String,
    pub dir: String,
    pub json: bool,
    #[serde(default = "default_rotation")]
    pub file_rotation: String,
}

fn default_rotation() -> String {
    "daily".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisKeyConfig {
    pub captcha: String,
    pub token_blacklist: String,
    pub user_token_version: String,
    pub login_session: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisTtlConfig {
    pub captcha: u64,
    pub token_blacklist: u64,
    pub user_token_version: u64,
}

// `Debug` is hand-written below to redact the encryption key.
#[derive(Clone, Deserialize)]
pub struct MailConfig {
    #[serde(default = "default_mail_password_key")]
    pub mail_password_key: String,
}

impl Default for MailConfig {
    fn default() -> Self {
        Self {
            mail_password_key: default_mail_password_key(),
        }
    }
}

impl std::fmt::Debug for MailConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MailConfig")
            .field("mail_password_key", &"***")
            .finish()
    }
}

fn default_mail_password_key() -> String {
    "mail-password-encryption-key-32b".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadConfig {
    #[serde(default = "default_storage_type")]
    pub storage_type: String,
    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: u64,
    /// Allowed MIME types (whitelist). Empty = allow all.
    #[serde(default)]
    pub allowed_types: Vec<String>,
    /// Blocked file extensions (blacklist). e.g. ["exe","bat","sh"]
    #[serde(default)]
    pub blocked_extensions: Vec<String>,
    /// Local storage config (only needed when storage_type = "local")
    #[serde(default)]
    pub local: Option<LocalStorageConfig>,
    /// OSS config (only needed when storage_type = "oss")
    #[serde(default)]
    pub oss: Option<OssConfig>,
    /// COS config (only needed when storage_type = "cos")
    #[serde(default)]
    pub cos: Option<CosConfig>,
}

impl Default for UploadConfig {
    fn default() -> Self {
        Self {
            storage_type: default_storage_type(),
            max_file_size_mb: default_max_file_size_mb(),
            allowed_types: vec![],
            blocked_extensions: vec![],
            local: None,
            oss: None,
            cos: None,
        }
    }
}

 

#[derive(Debug, Clone, Deserialize)]
pub struct LocalStorageConfig {
    #[serde(default = "default_upload_dir")]
    pub upload_dir: String,
    #[serde(default="default_upload_domain")]
    pub domain:String,
}
 
#[derive(Clone, Deserialize)]
pub struct OssConfig {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub bucket: String,
    pub region: String,
    /// Custom endpoint (overrides default `https://{bucket}.{region}.aliyuncs.com`)
    pub endpoint: Option<String>,
    /// Custom access domain for URL generation
    pub domain: Option<String>,
    /// Storage path prefix (e.g. "uploads")
    #[serde(default)]
    pub location: String,
}

impl std::fmt::Debug for OssConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OssConfig")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("access_key_id", &"******")
            .field("access_key_secret", &"******")
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Deserialize)]
pub struct CosConfig {
    pub secret_id: String,
    pub secret_key: String,
    pub bucket: String,
    pub region: String,
    /// Custom access domain
    pub domain: Option<String>,
    /// Storage path prefix
    #[serde(default)]
    pub location: String,
}

impl std::fmt::Debug for CosConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CosConfig")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("secret_id", &"******")
            .field("secret_key", &"******")
            .finish_non_exhaustive()
    }
}

fn default_storage_type() -> String {
    "local".into()
}

fn default_upload_domain() ->String {
    "http://localhost".into()
}

fn default_upload_dir() -> String {
    "./uploads".into()
}
fn default_max_file_size_mb() -> u64 {
    100
}

// ──────────────────────────────────────────────────────────────────────
// Validation constants (security P0 — see module-level docs).
// ──────────────────────────────────────────────────────────────────────

/// Minimum JWT secret length in bytes. HS256 is keyed on SHA-256, which
/// is a 256-bit (32-byte) primitive — shorter secrets reduce the search
/// space for brute-force attacks below the algorithm's security margin.
pub const JWT_SECRET_MIN_LEN: usize = 32;

/// Known-insecure JWT secret values that must never appear in a loaded
/// config. If one of these shows up, the service refuses to boot — it
/// means someone pushed `config/default.yaml` fixture data into prod or
/// forgot to set `APP__JWT__SECRET`. The list is intentionally short:
/// reviewers should treat any new entry here as a forensic artifact.
const JWT_SECRET_BLACKLIST: &[&str] = &[
    "change_me_in_production",
    "changeme",
    "secret",
    "jwt_secret",
    "jwt-secret",
    "dev",
    "development",
    "test",
    "testing",
    "password",
];

/// Maximum JWT expiry (30 days). Long-lived access tokens are a
/// security anti-pattern: they widen the window for stolen-token reuse
/// and defeat the purpose of token versioning. Refresh tokens should
/// be used for session extension.
pub const JWT_EXPIRES_MAX_SEC: i64 = 30 * 24 * 60 * 60;

impl AppConfig {
    /// Load config from the yaml + env var hierarchy, then run
    /// fail-fast validation on security-sensitive fields. Returns
    /// `anyhow::Error` on either a parse failure or a validation
    /// failure — both are fatal, the service must not start.
    pub fn load() -> anyhow::Result<Self> {
        let env = std::env::var("APP_ENV").unwrap_or_else(|_| "development".into());

        let settings = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
            .add_source(config::File::with_name(&format!("config/{env}")).required(false))
            .add_source(
                config::Environment::with_prefix("APP")
                    .separator("__")
                    .try_parsing(true)
                    // Only scope CSV parsing to known list fields, otherwise
                    // every scalar string (e.g. a postgres URL) would be
                    // interpreted as a sequence and fail to deserialize.
                    .list_separator(",")
                    .with_list_parse_key("cors.origins"),
            )
            .build()
            .context("loading config file hierarchy")?;

        let cfg: Self = settings
            .try_deserialize()
            .context("deserializing merged config into AppConfig")?;

        cfg.validate()?;

        Ok(cfg)
    }

    /// Fail-fast validation of security-sensitive fields. Called
    /// automatically by `load()` but also exposed publicly so tests
    /// can exercise each branch in isolation without touching the
    /// real yaml loader.
    ///
    /// Error ordering is intentional: blacklist checks fire **before**
    /// length checks so that `"change_me_in_production"` produces the
    /// more specific "known-insecure default" error message rather
    /// than the generic "too short" one.
    pub fn validate(&self) -> anyhow::Result<()> {
        // --- JWT secret blacklist ---
        if JWT_SECRET_BLACKLIST.contains(&self.jwt.secret.as_str()) {
            anyhow::bail!(
                "jwt.secret is a known-insecure default value \
                 (blacklist match); set APP__JWT__SECRET to a real \
                 secret of at least {} bytes",
                JWT_SECRET_MIN_LEN
            );
        }

        // --- JWT secret length ---
        if self.jwt.secret.len() < JWT_SECRET_MIN_LEN {
            anyhow::bail!(
                "jwt.secret must be at least {} bytes (got {}); \
                 HS256 security margin requires ≥ 256 bits of entropy",
                JWT_SECRET_MIN_LEN,
                self.jwt.secret.len()
            );
        }

        // --- JWT expiry bounds ---
        if self.jwt.expires_in_sec <= 0 {
            anyhow::bail!(
                "jwt.expires_in_sec must be > 0 (got {})",
                self.jwt.expires_in_sec
            );
        }
        if self.jwt.expires_in_sec > JWT_EXPIRES_MAX_SEC {
            anyhow::bail!(
                "jwt.expires_in_sec must be ≤ {} (30 days); got {}. \
                 Long-lived access tokens are a security anti-pattern; \
                 use refresh tokens for session extension",
                JWT_EXPIRES_MAX_SEC,
                self.jwt.expires_in_sec
            );
        }
        if self.jwt.refresh_expires_in_sec <= 0 {
            anyhow::bail!(
                "jwt.refresh_expires_in_sec must be > 0 (got {})",
                self.jwt.refresh_expires_in_sec
            );
        }

        // --- Redis key prefix format ---
        // All prefixes are used with `format!("{}{}", prefix, uuid)`.
        // A missing trailing `:` would silently produce keys like
        // `saas_tea_captcha<uuid>` instead of `saas_tea:captcha:<uuid>`,
        // partitioning the keyspace away from the NestJS side without
        // any runtime error.
        for (field, value) in [
            ("captcha", &self.redis_keys.captcha),
            ("token_blacklist", &self.redis_keys.token_blacklist),
            ("user_token_version", &self.redis_keys.user_token_version),
            ("login_session", &self.redis_keys.login_session),
        ] {
            if !value.ends_with(':') {
                anyhow::bail!(
                    "redis_keys.{} must end with ':' (got {:?}); \
                     missing colon would silently partition the keyspace",
                    field,
                    value
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> AppConfig {
        AppConfig {
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 8080,
            },
            db: DbConfig {
                postgresql: PostgresConfig {
                    url: "postgresql://user:pass@localhost:5432/db".into(),
                    max_connections: 10,
                    acquire_timeout_sec: 30,
                    idle_timeout_sec: 300,
                },
                redis: RedisConfig {
                    url: "redis://localhost:6379/0".into(),
                    pool_size: 16,
                },
            },
            jwt: JwtConfig {
                secret: "a-very-long-and-random-test-secret-32chars+".into(),
                expires_in_sec: 3600,
                refresh_expires_in_sec: 86400,
            },
            tenant: TenantConfig { enabled: true },
            cors: CorsConfig {
                origins: vec![],
                app_domain: String::new(),
            },
            logger: LoggerConfig {
                level: "info".into(),
                dir: "./logs".into(),
                json: true,
                file_rotation: "daily".into(),
            },
            redis_keys: RedisKeyConfig {
                captcha: "test:captcha:".into(),
                token_blacklist: "test:blacklist:".into(),
                user_token_version: "test:tokver:".into(),
                login_session: "test:session:".into(),
            },
            redis_ttl: RedisTtlConfig {
                captcha: 300,
                token_blacklist: 86400,
                user_token_version: 604800,
            },
            mail: MailConfig::default(),
            upload: UploadConfig::default(),
        }
    }

    // ── Happy path ─────────────────────────────────────────────────

    #[test]
    fn validate_happy_path_ok() {
        assert!(valid_config().validate().is_ok());
    }

    // ── JWT secret blacklist ───────────────────────────────────────

    #[test]
    fn validate_rejects_blacklisted_change_me_in_production() {
        let mut cfg = valid_config();
        cfg.jwt.secret = "change_me_in_production".into();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("known-insecure default"),
            "blacklist check must fire first, got: {err}"
        );
    }

    #[test]
    fn validate_rejects_blacklisted_secret_as_value() {
        let mut cfg = valid_config();
        cfg.jwt.secret = "secret".into();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("known-insecure default"), "{err}");
    }

    // ── JWT secret length ──────────────────────────────────────────

    #[test]
    fn validate_rejects_short_jwt_secret_below_32_bytes() {
        let mut cfg = valid_config();
        // Not in blacklist, but only 23 chars.
        cfg.jwt.secret = "short-unique-not-listed".into();
        assert_eq!(cfg.jwt.secret.len(), 23);
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("at least 32 bytes"),
            "length check must fire after blacklist, got: {err}"
        );
    }

    #[test]
    fn validate_accepts_exact_32_byte_secret() {
        let mut cfg = valid_config();
        cfg.jwt.secret = "a".repeat(32);
        assert!(cfg.validate().is_ok());
    }

    // ── JWT expiry bounds ──────────────────────────────────────────

    #[test]
    fn validate_rejects_zero_expiry() {
        let mut cfg = valid_config();
        cfg.jwt.expires_in_sec = 0;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("must be > 0"));
    }

    #[test]
    fn validate_rejects_negative_expiry() {
        let mut cfg = valid_config();
        cfg.jwt.expires_in_sec = -1;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn validate_rejects_expiry_over_30_days() {
        let mut cfg = valid_config();
        cfg.jwt.expires_in_sec = JWT_EXPIRES_MAX_SEC + 1;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("30 days"));
    }

    #[test]
    fn validate_accepts_expiry_exactly_30_days() {
        let mut cfg = valid_config();
        cfg.jwt.expires_in_sec = JWT_EXPIRES_MAX_SEC;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn validate_rejects_zero_refresh_expiry() {
        let mut cfg = valid_config();
        cfg.jwt.refresh_expires_in_sec = 0;
        assert!(cfg.validate().is_err());
    }

    // ── Redis key prefix format ────────────────────────────────────

    #[test]
    fn validate_rejects_captcha_prefix_missing_colon() {
        let mut cfg = valid_config();
        cfg.redis_keys.captcha = "captcha_code".into(); // no trailing ':'
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("redis_keys.captcha"));
        assert!(err.contains("must end with ':'"));
    }

    #[test]
    fn validate_rejects_login_session_prefix_missing_colon() {
        let mut cfg = valid_config();
        cfg.redis_keys.login_session = "session_".into();
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("redis_keys.login_session"));
    }

    // ── URL redaction helper ───────────────────────────────────────

    #[test]
    fn redact_url_postgres_user_and_password() {
        assert_eq!(
            redact_url_creds("postgresql://saas_tea:123456@127.0.0.1:5432/saas_tea"),
            "postgresql://***:***@127.0.0.1:5432/saas_tea"
        );
    }

    #[test]
    fn redact_url_redis_password_only() {
        assert_eq!(
            redact_url_creds("redis://:redis_FKxk3i@127.0.0.1:6379/4"),
            "redis://***:***@127.0.0.1:6379/4"
        );
    }

    #[test]
    fn redact_url_no_credentials_passthrough() {
        assert_eq!(
            redact_url_creds("redis://localhost:6379/0"),
            "redis://localhost:6379/0"
        );
    }

    #[test]
    fn redact_url_no_scheme_passthrough() {
        // Not a URL — nothing to do
        assert_eq!(redact_url_creds("hello"), "hello");
    }

    // ── Debug impls redact credentials ─────────────────────────────

    #[test]
    fn debug_postgres_config_redacts_password() {
        let cfg = PostgresConfig {
            url: "postgresql://user:supersecret@host:5432/db".into(),
            max_connections: 10,
            acquire_timeout_sec: 30,
            idle_timeout_sec: 300,
        };
        let debug = format!("{:?}", cfg);
        assert!(
            !debug.contains("supersecret"),
            "password must not appear in Debug: {}",
            debug
        );
        assert!(debug.contains("***"));
        assert!(
            debug.contains("host:5432"),
            "host should still be visible: {}",
            debug
        );
    }

    #[test]
    fn debug_redis_config_redacts_password() {
        let cfg = RedisConfig {
            url: "redis://:supersecret@host:6379/0".into(),
            pool_size: 16,
        };
        let debug = format!("{:?}", cfg);
        assert!(!debug.contains("supersecret"), "{}", debug);
        assert!(debug.contains("***"));
    }

    #[test]
    fn debug_jwt_config_redacts_secret() {
        let cfg = JwtConfig {
            secret: "super-secret-value-should-never-appear".into(),
            expires_in_sec: 3600,
            refresh_expires_in_sec: 86400,
        };
        let debug = format!("{:?}", cfg);
        assert!(
            !debug.contains("super-secret"),
            "secret must not appear in Debug: {}",
            debug
        );
        assert!(debug.contains("***"));
        assert!(
            debug.contains("3600"),
            "non-sensitive fields should still be visible"
        );
    }
}

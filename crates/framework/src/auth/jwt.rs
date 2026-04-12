//! JWT encode/decode.
//!
//! The payload is intentionally minimal — it matches the NestJS server, which
//! keeps `{uuid, userId, tenantId, userType, tokenVersion?}` in the JWT and
//! stores the full session (permissions/platformId/isAdmin/…) in Redis under
//! `login_token_session:{uuid}`.
//!
//! See [`crate::auth::session::UserSession`] for the Redis-stored half.

use crate::config::JwtConfig;
use crate::error::AppError;
use crate::response::ResponseCode;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// Session key in Redis (`login_token_session:{uuid}`).
    pub uuid: String,
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "tenantId", skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    /// `"10"` = CUSTOM (backend) / `"20"` = CLIENT (C-end).
    #[serde(rename = "userType")]
    pub user_type: String,
    /// Optional — incremented to invalidate all tokens of a user
    /// (e.g. on password change).
    #[serde(
        rename = "tokenVersion",
        skip_serializing_if = "Option::is_none",
        default
    )]
    pub token_version: Option<i64>,
    pub iat: i64,
    pub exp: i64,
}

impl JwtClaims {
    pub fn new(
        uuid: impl Into<String>,
        user_id: impl Into<String>,
        tenant_id: Option<String>,
        user_type: impl Into<String>,
        token_version: Option<i64>,
        expires_in_sec: i64,
    ) -> Self {
        let now = chrono::Utc::now().timestamp();
        Self {
            uuid: uuid.into(),
            user_id: user_id.into(),
            tenant_id,
            user_type: user_type.into(),
            token_version,
            iat: now,
            exp: now + expires_in_sec,
        }
    }
}

pub fn encode_token(claims: &JwtClaims, cfg: &JwtConfig) -> Result<String, AppError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(cfg.secret.as_bytes()),
    )
    .map_err(|e| {
        tracing::warn!(error = %e, "jwt encode failed");
        AppError::Internal(anyhow::anyhow!("jwt encode: {e}"))
    })
}

pub fn decode_token(token: &str, cfg: &JwtConfig) -> Result<JwtClaims, AppError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 30;
    decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(cfg.secret.as_bytes()),
        &validation,
    )
    .map(|d| d.claims)
    .map_err(|e| {
        tracing::debug!(error = %e, "jwt decode failed");
        match e.kind() {
            jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                AppError::auth(ResponseCode::TOKEN_EXPIRED)
            }
            _ => AppError::auth(ResponseCode::TOKEN_INVALID),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> JwtConfig {
        JwtConfig {
            secret: "test-secret-1234567890".into(),
            expires_in_sec: 3600,
            refresh_expires_in_sec: 86400,
        }
    }

    #[test]
    fn round_trip_encode_decode() {
        let claims = JwtClaims::new("uuid-1", "user-1", Some("t0".into()), "10", Some(1), 3600);
        let token = encode_token(&claims, &cfg()).unwrap();
        let decoded = decode_token(&token, &cfg()).unwrap();
        assert_eq!(decoded.uuid, "uuid-1");
        assert_eq!(decoded.user_id, "user-1");
        assert_eq!(decoded.tenant_id.as_deref(), Some("t0"));
        assert_eq!(decoded.user_type, "10");
        assert_eq!(decoded.token_version, Some(1));
    }

    #[test]
    fn decode_with_wrong_secret_fails_with_token_invalid() {
        let claims = JwtClaims::new("u", "uid", None, "10", None, 3600);
        let token = encode_token(&claims, &cfg()).unwrap();
        let bad = JwtConfig {
            secret: "other".into(),
            expires_in_sec: 3600,
            refresh_expires_in_sec: 86400,
        };
        let err = decode_token(&token, &bad).unwrap_err();
        match err {
            AppError::Auth { code } => assert_eq!(code, ResponseCode::TOKEN_INVALID),
            _ => panic!("expected Auth error"),
        }
    }

    #[test]
    fn expired_token_maps_to_token_expired() {
        // iat/exp in the past
        let mut claims = JwtClaims::new("u", "uid", None, "10", None, 0);
        claims.iat -= 120;
        claims.exp -= 60;
        let token = encode_token(&claims, &cfg()).unwrap();
        let err = decode_token(&token, &cfg()).unwrap_err();
        match err {
            AppError::Auth { code } => assert_eq!(code, ResponseCode::TOKEN_EXPIRED),
            _ => panic!("expected Auth error"),
        }
    }

    #[test]
    fn camel_case_serialization() {
        let claims = JwtClaims::new("u", "uid", Some("t".into()), "20", Some(5), 3600);
        let json = serde_json::to_value(&claims).unwrap();
        assert!(json.get("userId").is_some(), "missing userId");
        assert!(json.get("tenantId").is_some(), "missing tenantId");
        assert!(json.get("userType").is_some(), "missing userType");
        assert!(json.get("tokenVersion").is_some(), "missing tokenVersion");
    }
}

//! `auth` — JWT guard middleware.
//!
//! Sequence for non-whitelisted routes:
//!
//! 1. Parse `Authorization: Bearer <token>` → 401 on missing/malformed.
//! 2. Decode the JWT → `TOKEN_INVALID` / `TOKEN_EXPIRED`.
//! 3. Check single-token blacklist → `TOKEN_INVALID`.
//! 4. Compare user-level token version → `TOKEN_INVALID` if stale.
//! 5. Fetch Redis session (`login_token_session:{uuid}`) → `TOKEN_EXPIRED`
//!    if missing.
//! 6. Populate `RequestContext` with session fields and insert both the
//!    `UserSession` and `JwtClaims` into the request extensions so downstream
//!    layers (tenant, access, handlers) can consume them.
//!
//! The whitelist lets us bypass auth for public routes (`/auth/login`,
//! `/auth/code`, `/health/*`, `/metrics`).

use crate::auth::{jwt, session, JwtClaims, UserSession};
use crate::config::{JwtConfig, RedisKeyConfig};
use crate::context::RequestContext;
use crate::error::AppError;
use crate::infra::redis::RedisPool;
use crate::response::ResponseCode;
use axum::{
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct AuthState {
    pub jwt: Arc<JwtConfig>,
    pub redis: RedisPool,
    pub redis_keys: Arc<RedisKeyConfig>,
    /// Entries may be either `"/path"` (any method) or `"METHOD:/path"`
    /// (case-insensitive). Prefix match — `"/auth/"` covers every subpath.
    pub whitelist: Arc<Vec<String>>,
}

#[tracing::instrument(skip_all, name = "middleware.auth")]
pub async fn auth(
    State(state): State<AuthState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if is_whitelisted(&state.whitelist, req.method(), req.uri().path()) {
        return Ok(next.run(req).await);
    }

    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::auth(ResponseCode::TOKEN_INVALID))?;

    let token = auth_header
        .strip_prefix("Bearer ")
        .or_else(|| auth_header.strip_prefix("bearer "))
        .ok_or_else(|| AppError::auth(ResponseCode::TOKEN_INVALID))?;

    let claims = jwt::decode_token(token, &state.jwt)?;

    // 1. Single-token blacklist
    let blacklisted = session::is_blacklisted(&state.redis, &state.redis_keys, &claims.uuid)
        .await
        .map_err(AppError::Internal)?;
    if blacklisted {
        return Err(AppError::auth(ResponseCode::TOKEN_INVALID));
    }

    // 2. User-level token version (invalidate all tokens on password change)
    if let Some(claim_ver) = claims.token_version {
        let stored_ver =
            session::get_user_token_version(&state.redis, &state.redis_keys, &claims.user_id)
                .await
                .map_err(AppError::Internal)?;
        if let Some(stored) = stored_ver {
            if claim_ver < stored {
                return Err(AppError::auth(ResponseCode::TOKEN_INVALID));
            }
        }
    }

    // 3. Fetch full Redis session
    let user_session = session::fetch(&state.redis, &state.redis_keys, &claims.uuid)
        .await
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::auth(ResponseCode::TOKEN_EXPIRED))?;

    // 4. Populate RequestContext from the session
    RequestContext::mutate(|ctx: &mut RequestContext| {
        ctx.user_id = Some(user_session.user_id.clone());
        ctx.user_name = Some(user_session.user_name.clone());
        ctx.user_type = Some(user_session.user_type.clone());
        if user_session.tenant_id.is_some() {
            ctx.tenant_id = user_session.tenant_id.clone();
        }
        ctx.platform_id = user_session.platform_id.clone();
        ctx.sys_code = user_session.sys_code.clone();
        ctx.is_admin = user_session.is_admin;
        if let Some(lang) = user_session.lang.clone() {
            // Session lang takes precedence over Accept-Language.
            ctx.lang_code = Some(lang);
        }
    });

    // 5. Propagate session identity to the root span so downstream
    //    log events carry user_id / user_name / tenant_id without
    //    every service function having to re-declare them.
    let span = tracing::Span::current();
    span.record("user_id", user_session.user_id.as_str());
    span.record("user_name", user_session.user_name.as_str());
    if let Some(tid) = user_session.tenant_id.as_deref() {
        span.record("tenant_id", tid);
    }

    // 6. Stash session + claims in request extensions so that
    //    `middleware::access::enforce` and handlers can read them.
    req.extensions_mut().insert::<UserSession>(user_session);
    req.extensions_mut().insert::<JwtClaims>(claims);

    Ok(next.run(req).await)
}

fn is_whitelisted(whitelist: &[String], method: &Method, path: &str) -> bool {
    whitelist.iter().any(|rule| {
        if let Some((m, p)) = rule.split_once(':') {
            method.as_str().eq_ignore_ascii_case(m) && path.starts_with(p)
        } else {
            path.starts_with(rule)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_matches_prefix() {
        let wl = vec!["/auth/".to_string(), "/health".to_string()];
        assert!(is_whitelisted(&wl, &Method::GET, "/auth/login"));
        assert!(is_whitelisted(&wl, &Method::POST, "/auth/code"));
        assert!(is_whitelisted(&wl, &Method::GET, "/health/live"));
        assert!(!is_whitelisted(&wl, &Method::GET, "/system/user/list"));
    }

    #[test]
    fn whitelist_matches_method_prefix() {
        let wl = vec!["POST:/auth/login".to_string()];
        assert!(is_whitelisted(&wl, &Method::POST, "/auth/login"));
        assert!(!is_whitelisted(&wl, &Method::GET, "/auth/login"));
    }
}

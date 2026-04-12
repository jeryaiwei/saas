//! `tenant` — tenant guard middleware.
//!
//! Runs **after** [`crate::middleware::auth`]. By this point the session
//! fields are already in [`crate::context::RequestContext`]. This layer only
//! enforces the NestJS invariant:
//!
//! > Backend users (`userType = "10"`) must have a non-empty `tenantId`.
//!
//! If `tenant.enabled = false` in config (mirroring NestJS `TENANT_ENABLED`),
//! this middleware is a pass-through.

use crate::config::TenantConfig;
use crate::context::RequestContext;
use crate::error::AppError;
use crate::response::ResponseCode;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

#[derive(Clone)]
pub struct TenantState {
    pub tenant: Arc<TenantConfig>,
}

pub async fn tenant_guard(
    State(state): State<TenantState>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    if !state.tenant.enabled {
        return Ok(next.run(req).await);
    }

    // Unauthenticated paths (those passed through by `auth` whitelist) have
    // an empty context — skip tenant enforcement.
    let snapshot = RequestContext::with_current(|ctx| {
        (
            ctx.user_type.clone(),
            ctx.tenant_id.clone(),
            ctx.user_id.clone(),
        )
    });

    let Some((user_type, tenant_id, user_id)) = snapshot else {
        return Ok(next.run(req).await);
    };

    // Not authenticated yet (public route).
    if user_id.is_none() {
        return Ok(next.run(req).await);
    }

    let is_backend = user_type.as_deref() == Some("10");
    if is_backend && tenant_id.is_none() {
        return Err(AppError::forbidden(ResponseCode::FORBIDDEN));
    }

    Ok(next.run(req).await)
}

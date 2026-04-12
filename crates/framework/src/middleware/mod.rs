//! Axum middleware stack.
//!
//! Execution order (matching NestJS guards):
//!
//! 1. [`tenant_http`] — initialize `RequestContext` (request id, lang, optional
//!    tenant-id header).
//! 2. [`auth`] — decode JWT, verify blacklist + token version, fetch Redis
//!    session, populate `RequestContext`.
//! 3. [`tenant`] — finalize tenant context from the session; reject backend
//!    users without a tenant.
//! 4. [`access`] — **route-level** RBAC checks via
//!    `axum::middleware::from_fn_with_state` (see module docs).
//! 5. [`telemetry`] — HTTP metrics (requests, latency).

pub mod access;
pub mod access_macros;
pub mod auth;
pub mod telemetry;
pub mod tenant;
pub mod tenant_http;

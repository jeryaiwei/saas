//! framework — standalone reusable SaaS backend foundation.
//!
//! No project-specific coupling. Contains the cross-cutting layers that every
//! multi-tenant HTTP service needs:
//!
//! - [`config`]    — strongly-typed config loader (yaml + env var merge)
//! - [`context`]   — per-request context via `tokio::task_local!` + scope helpers
//! - [`response`]  — unified envelope, pagination, and response codes
//! - [`error`]     — `AppError` + `IntoResponse` + `BusinessError` factory
//! - [`i18n`]      — runtime message lookup with `{placeholder}` substitution
//! - [`telemetry`] — tracing init + Prometheus metrics recorder
//!

pub mod auth;
pub mod config;
pub mod constants;
pub mod context;
pub mod error;
pub mod extractors;
pub mod i18n;
pub mod infra;
pub mod middleware;
pub mod response;
pub mod telemetry;
pub mod testing;

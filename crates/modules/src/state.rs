//! Application state shared across Axum handlers.
//!
//! Constructed once at bootstrap (`app::main`) and passed to `Router::with_state`.
//! Sub-states for middleware (e.g. `framework::middleware::auth::AuthState`)
//! are derived from this in Gate 6.

use framework::config::AppConfig;
use framework::infra::redis::RedisPool;
use framework::telemetry::metrics::PrometheusHandle;
use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub pg: PgPool,
    pub redis: RedisPool,
    pub metrics: PrometheusHandle,
}

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
use tokio::sync::Semaphore;

/// Max concurrent background mail send tasks.
const MAIL_SEND_PERMITS: usize = 10;
/// Max concurrent background SMS send tasks.
const SMS_SEND_PERMITS: usize = 20;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub pg: PgPool,
    pub redis: RedisPool,
    pub metrics: PrometheusHandle,
    /// Semaphore for mail send backpressure.
    pub mail_semaphore: Arc<Semaphore>,
    /// Semaphore for SMS send backpressure.
    pub sms_semaphore: Arc<Semaphore>,
}

impl AppState {
    /// Create default semaphores for mail/SMS send.
    pub fn new_semaphores() -> (Arc<Semaphore>, Arc<Semaphore>) {
        (
            Arc::new(Semaphore::new(MAIL_SEND_PERMITS)),
            Arc::new(Semaphore::new(SMS_SEND_PERMITS)),
        )
    }
}

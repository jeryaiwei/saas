//! modules — HTTP handlers, services, and domain layer.

pub mod auth;
pub mod domain;
pub mod health;
pub mod state;
pub mod system;

pub use state::AppState;

use axum::Router;

/// API-prefixed subset of the module routers — everything that should live
/// under `/api/v1` in production. Single source of truth for "which modules
/// contribute routes under the API prefix". Both `app::main` (nested under
/// `API_PREFIX`) and the integration test harness (`tests/common/mod.rs`,
/// flat-mounted via `router()`) compose this same function so that adding a
/// new module in one place can't accidentally leave the other out of sync.
///
/// Health routes intentionally stay OUTSIDE this function — they live at
/// the root (`/health/live`, `/health/ready`) for Kubernetes probes.
pub fn api_router() -> Router<AppState> {
    Router::new()
        .merge(auth::router())
        .merge(system::role::router())
        .merge(system::tenant::router())
        .merge(system::tenant_package::router())
        .merge(system::user::router())
}

/// Flat router used by the integration test harness — combines
/// `api_router()` with the health routes and `with_state(state)` so that
/// `router.oneshot(request)` in tests can hit any endpoint without
/// needing to know about the production API prefix.
///
/// Production (`app::main`) does NOT use this function — it composes
/// `api_router().nest(API_PREFIX, ...)` + `health::router()` + layers
/// directly so middleware ordering stays explicit in main.rs.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(api_router())
        .merge(health::router())
        .with_state(state)
}

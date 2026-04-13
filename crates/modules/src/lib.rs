//! modules — HTTP handlers, services, and domain layer.

pub mod auth;
pub mod domain;
pub mod health;
pub mod message;
pub mod monitor;
pub mod openapi;
pub mod state;
pub mod system;

pub use openapi::ApiDoc;
pub use state::AppState;

use axum::Router;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

/// Build the API router and OpenAPI spec together. Each module's `router()`
/// returns an `OpenApiRouter` that carries both axum routes and OpenAPI paths.
/// Adding a new module = adding one `.merge(...)` line here.
fn api_openapi_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // auth
        .merge(auth::router())
        // system
        .merge(system::audit_log::router())
        .merge(system::config::router())
        .merge(system::dept::router())
        .merge(system::dict::router())
        .merge(system::menu::router())
        .merge(system::post::router())
        .merge(system::role::router())
        .merge(system::tenant::router())
        .merge(system::tenant_package::router())
        .merge(system::tenant_dashboard::router())
        .merge(system::user::router())
        .merge(system::file_manager::router())
        // message
        .merge(message::notice::router())
        .merge(message::notify_template::router())
        .merge(message::notify_message::router())
        .merge(message::mail_account::router())
        .merge(message::mail_template::router())
        .merge(message::mail_log::router())
        .merge(message::mail_send::router())
        .merge(message::sms_channel::router())
        .merge(message::sms_template::router())
        .merge(message::sms_log::router())
        .merge(message::sms_send::router())
        // monitor
        .merge(monitor::oper_log::router())
        .merge(monitor::login_log::router())
        .merge(monitor::online_user::router())
        .merge(monitor::server_info::router())
        .merge(monitor::cache::router())
}

/// Split into axum Router + OpenAPI spec. Called by `app::main`.
pub fn api_router_and_openapi() -> (Router<AppState>, utoipa::openapi::OpenApi) {
    let (router, mut api) = api_openapi_router().split_for_parts();
    // Merge global info / tags / security from ApiDoc
    let global = ApiDoc::openapi();
    api.info = global.info;
    api.tags = global.tags;
    api.security = global.security;
    if let Some(gc) = global.components {
        let components = api.components.get_or_insert_with(Default::default);
        components.security_schemes.extend(gc.security_schemes);
    }
    api.servers = global.servers;
    (router, api)
}

/// Flat router used by the integration test harness.
pub fn router(state: AppState) -> Router {
    let (api_router, _) = api_router_and_openapi();
    Router::new()
        .merge(api_router)
        .merge(health::router())
        .with_state(state)
}

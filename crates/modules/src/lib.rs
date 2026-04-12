//! modules — HTTP handlers, services, and domain layer.

pub mod auth;
pub mod domain;
pub mod health;
pub mod openapi;
pub mod state;
pub mod system;

pub use openapi::ApiDoc;
pub use state::AppState;

use axum::Router;
use utoipa::OpenApi;

/// Register all API modules. Adding a new module = adding one line here.
/// Both the axum Router and the OpenAPI spec are derived from this single list.
macro_rules! register_modules {
    ($( $router:expr, $api:ty );+ $(;)?) => {
        /// API-prefixed subset of the module routers — everything under `/api/v1`.
        pub fn api_router() -> Router<AppState> {
            let mut r = Router::new();
            $( r = r.merge($router); )+
            r
        }

        /// Merge per-module OpenAPI definitions into the global spec.
        pub fn api_openapi() -> utoipa::openapi::OpenApi {
            let mut doc = ApiDoc::openapi();
            $( doc.merge(<$api>::openapi()); )+
            doc
        }
    };
}

register_modules! {
    auth::router(),                        auth::handler::AuthApi;
    system::config::router(),              system::config::handler::ConfigApi;
    system::dept::router(),                system::dept::handler::DeptApi;
    system::dict::router(),                system::dict::handler::DictApi;
    system::menu::router(),                system::menu::handler::MenuApi;
    system::post::router(),                system::post::handler::PostApi;
    system::role::router(),                system::role::handler::RoleApi;
    system::tenant::router(),              system::tenant::handler::TenantApi;
    system::tenant_package::router(),      system::tenant_package::handler::TenantPackageApi;
    system::user::router(),                system::user::handler::UserApi;
}

/// Flat router used by the integration test harness — combines
/// `api_router()` with the health routes and `with_state(state)` so that
/// `router.oneshot(request)` in tests can hit any endpoint without
/// needing to know about the production API prefix.
pub fn router(state: AppState) -> Router {
    Router::new()
        .merge(api_router())
        .merge(health::router())
        .with_state(state)
}

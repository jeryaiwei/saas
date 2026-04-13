//! app — binary entry point.
//!
//! Assembles the Phase 0 HTTP server end-to-end:
//!
//! ```text
//! load_config
//!   ├─ init_tracing (file + stdout)
//!   ├─ init_metrics (Prometheus recorder)
//!   ├─ PgPool::connect_lazy
//!   ├─ RedisPool::build
//!   ├─ build_router (modules::router + global layers)
//!   └─ axum::serve(listener, app).with_graceful_shutdown(SIGINT | SIGTERM)
//! ```

use axum::{
    http::{request::Parts, HeaderValue, Method},
    middleware::{from_fn, from_fn_with_state},
    Extension, Router,
};
use framework::{
    config::AppConfig,
    infra::{pg, redis},
    middleware::{
        auth::{self as auth_mw, AuthState},
        telemetry as telemetry_mw, tenant as tenant_mw, tenant_http,
    },
    telemetry,
};
use modules::AppState;
use regex::Regex;
use std::{sync::Arc, time::Duration};
use tokio::{net::TcpListener, signal};
use tower_http::{
    compression::CompressionLayer,
    cors::{AllowOrigin, Any, CorsLayer},
    trace::TraceLayer,
};
use tracing::info;
use utoipa_swagger_ui::SwaggerUi;

/// NestJS global prefix `/api` + URI versioning `/v1` — mounted under every
/// API route so the Vue web frontend and Flutter app hit identical URLs
/// regardless of which backend is serving them.
const API_PREFIX: &str = "/api/v1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Config — single source of truth is config/{APP_ENV}.yaml.
    //    Dev values (incl. DB/Redis URLs and JWT secret) live in
    //    config/development.yaml; prod is expected to do the same.
    let cfg = Arc::new(AppConfig::load()?);

    // 2. Tracing (hold the guard for the lifetime of main)
    let _tracing_guard = telemetry::tracing::init(&cfg.logger);

    // 3. Metrics recorder → handle for /metrics
    let metrics_handle = telemetry::metrics::init_recorder()?;

    info!(
        host = %cfg.server.host,
        port = cfg.server.port,
        env = %std::env::var("APP_ENV").unwrap_or_else(|_| "development".into()),
        "bootstrap begin"
    );

    // 4. Infra (lazy — server starts even if DB is not yet reachable)
    let pg_pool = pg::connect_lazy(&cfg.db.postgresql)?;
    let redis_pool = redis::build(&cfg.db.redis)?;

    // 5. Compose shared AppState
    let state = AppState {
        config: cfg.clone(),
        pg: pg_pool,
        redis: redis_pool.clone(),
        metrics: metrics_handle,
    };

    // 6. Middleware sub-states
    let auth_state = AuthState {
        jwt: Arc::new(cfg.jwt.clone()),
        redis: redis_pool.clone(),
        redis_keys: Arc::new(cfg.redis_keys.clone()),
        whitelist: Arc::new(default_whitelist()),
    };
    let tenant_state = tenant_mw::TenantState {
        tenant: Arc::new(cfg.tenant.clone()),
    };

    // 7. CORS (Phase 0: allow-any in dev; explicit list in prod)
    let cors = build_cors(&cfg);

    // 8. Router assembly
    //
    // - API routes (auth/*, info) are nested under `/api/v1` to match NestJS
    //   `app.setGlobalPrefix('/api')` + `enableVersioning(default '1')`.
    // - Health + metrics stay at the root so K8s probes and Prometheus don't
    //   need to know about the API prefix.
    //
    // `.layer(X)` wraps everything already in the router, so the LAST layer
    // applied becomes the OUTERMOST. To make `tenant_http` run first on
    // every request, it must be applied LAST in this chain.
    // `api_router()` is the single source of truth for which modules
    // contribute API-prefixed routes. Adding a new module only requires
    // updating `modules::api_router()`; both this binary and the test
    // harness pick up the change automatically.
    let (api_router, openapi) = modules::api_router_and_openapi();
    let operlog_pool = state.pg.clone();
    let app = Router::new()
        .nest(API_PREFIX, api_router)
        .merge(modules::health::router())
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi))
        .with_state(state)
        // innermost custom layers
        .layer(from_fn_with_state(tenant_state, tenant_mw::tenant_guard))
        .layer(Extension(operlog_pool)) // PgPool for operlog route-level middleware
        .layer(from_fn_with_state(auth_state, auth_mw::auth))
        // telemetry
        .layer(from_fn(telemetry_mw::metrics_middleware))
        .layer(TraceLayer::new_for_http())
        // transport
        .layer(CompressionLayer::new())
        .layer(cors)
        // OUTERMOST: establishes RequestContext before anything else runs
        .layer(from_fn(tenant_http::tenant_http));

    // 9. Serve
    let addr = format!("{}:{}", cfg.server.host, cfg.server.port);
    let listener = TcpListener::bind(&addr).await?;
    info!(%addr, "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("shutdown complete");
    Ok(())
}

fn default_whitelist() -> Vec<String> {
    vec![
        // API routes under /api/v1 prefix
        format!("POST:{API_PREFIX}/auth/login"),
        format!("GET:{API_PREFIX}/auth/code"),
        // Infra / docs routes (no prefix)
        "/health".into(),
        "GET:/metrics".into(),
        "/swagger-ui".into(),
        "/api-docs".into(),
    ]
}

fn build_cors(cfg: &AppConfig) -> CorsLayer {
    let methods = [
        Method::GET,
        Method::POST,
        Method::PUT,
        Method::DELETE,
        Method::OPTIONS,
    ];

    let origins: Vec<String> = cfg.cors.origins.clone();
    let subdomain_regex = build_subdomain_regex(&cfg.cors.app_domain);

    // No origins AND no APP_DOMAIN → fully permissive (dev-only). Cannot mix
    // `AllowOrigin::any` with `allow_credentials(true)`, so this branch does
    // NOT set credentials.
    if origins.is_empty() && subdomain_regex.is_none() {
        return CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(methods)
            .allow_headers(Any)
            .max_age(Duration::from_secs(3600));
    }

    // Explicit origin list or subdomain regex → can safely enable credentials,
    // but must enumerate allowed headers (CORS spec forbids `*` with
    // credentials). Match the header set NestJS sends.
    let predicate = move |origin: &HeaderValue, _parts: &Parts| -> bool {
        let Ok(origin_str) = origin.to_str() else {
            return false;
        };
        if origins.iter().any(|o| o == origin_str) {
            return true;
        }
        if let Some(re) = &subdomain_regex {
            return re.is_match(origin_str);
        }
        false
    };

    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(predicate))
        .allow_methods(methods)
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            axum::http::header::ACCEPT,
            axum::http::header::ACCEPT_LANGUAGE,
            HeaderValue::from_static("tenant-id")
                .as_bytes()
                .try_into()
                .unwrap(),
            HeaderValue::from_static("x-request-id")
                .as_bytes()
                .try_into()
                .unwrap(),
        ])
        .allow_credentials(true)
        .max_age(Duration::from_secs(3600))
}

/// Build the NestJS-compatible subdomain matcher if `APP_DOMAIN` is set.
///
/// Pattern: `^https://(([a-z0-9-]+\.)?<APP_DOMAIN>)$`
///
/// Example: `app_domain = "example.com"` allows both `https://example.com`
/// and `https://api.example.com` but not `https://evil-example.com`.
fn build_subdomain_regex(app_domain: &str) -> Option<Regex> {
    let trimmed = app_domain.trim();
    if trimmed.is_empty() {
        return None;
    }
    let pattern = format!(r"^https://(([a-z0-9-]+\.)?{})$", regex::escape(trimmed));
    match Regex::new(&pattern) {
        Ok(re) => Some(re),
        Err(e) => {
            tracing::warn!(error = %e, "invalid APP_DOMAIN; subdomain CORS regex disabled");
            None
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => { info!("received SIGINT"); }
        _ = terminate => { info!("received SIGTERM"); }
    }
}

#[cfg(test)]
mod tests {
    use super::build_subdomain_regex;

    #[test]
    fn subdomain_regex_matches_apex_and_sub() {
        let re = build_subdomain_regex("example.com").unwrap();
        assert!(re.is_match("https://example.com"));
        assert!(re.is_match("https://api.example.com"));
        assert!(re.is_match("https://a-b-c.example.com"));
    }

    #[test]
    fn subdomain_regex_rejects_lookalikes() {
        let re = build_subdomain_regex("example.com").unwrap();
        assert!(!re.is_match("https://evil-example.com"));
        assert!(!re.is_match("https://example.com.evil.com"));
        assert!(!re.is_match("http://example.com")); // wrong scheme
        assert!(!re.is_match("https://a.b.example.com")); // too deep
    }

    #[test]
    fn empty_domain_yields_none() {
        assert!(build_subdomain_regex("").is_none());
        assert!(build_subdomain_regex("   ").is_none());
    }
}

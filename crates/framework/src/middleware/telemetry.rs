//! HTTP metrics collection.
//!
//! Records two Prometheus series per request:
//! - `http_requests_total{method, path, status}` counter
//! - `http_request_duration_seconds{method, path}` histogram
//!
//! **Path label uses the matched route template**, not the raw URI path.
//! This is a P0 production-safety requirement: emitting the raw URI would
//! create one Prometheus time series per distinct path parameter value
//! (e.g. `/api/v1/system/user/u-001`, `/api/v1/system/user/u-002`, ...),
//! trivially exploding cardinality into the millions and OOM-ing the
//! scraper. `MatchedPath` returns `/api/v1/system/user/{id}` instead.
//!
//! Unmatched requests (404s, static assets, pre-routing errors) fall
//! back to the literal label `"<unmatched>"` so they still get counted
//! but share a single bounded series.
//!
//! Structured access logging is provided by `tower_http::trace::TraceLayer`,
//! which the application wires directly via `.layer(TraceLayer::new_for_http())`.

use axum::{extract::MatchedPath, extract::Request, middleware::Next, response::Response};
use std::time::Instant;

/// Fallback label for requests whose route didn't match any registered
/// handler (e.g. 404s, OPTIONS preflight for unknown paths). Kept as a
/// single bounded label so the cardinality stays low.
const UNMATCHED_PATH_LABEL: &str = "<unmatched>";

/// Resolve the metric `path` label from a request: prefer axum's
/// `MatchedPath` (the route template like `/api/v1/system/user/{id}`)
/// and fall back to `UNMATCHED_PATH_LABEL` when routing hasn't found
/// a match. Extracted as a pure helper so it can be unit-tested with
/// synthetic `Request` fixtures.
fn route_label(req: &Request) -> String {
    req.extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| UNMATCHED_PATH_LABEL.to_string())
}

pub async fn metrics_middleware(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    // Use the route template (e.g. `/api/v1/system/user/{id}`) rather
    // than the raw URI. See module-level docs for why.
    let path = route_label(&req);
    let start = Instant::now();

    let response = next.run(req).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    metrics::counter!(
        "http_requests_total",
        "method" => method.clone(),
        "path" => path.clone(),
        "status" => status,
    )
    .increment(1);

    metrics::histogram!(
        "http_request_duration_seconds",
        "method" => method,
        "path" => path,
    )
    .record(duration);

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request as HttpRequest;

    /// Fallback branch: a synthetic request with no `MatchedPath`
    /// extension must resolve to the bounded `<unmatched>` label,
    /// NOT to `req.uri().path()` (which would leak dynamic path
    /// segments into metric labels).
    #[test]
    fn route_label_falls_back_to_unmatched_when_no_matched_path() {
        // Build a bare axum Request with a URI that contains a
        // dynamic-looking segment. Without a MatchedPath extension,
        // the fallback branch must produce `<unmatched>` — crucially,
        // it must NOT be `/api/v1/system/user/u-aaa` (which is what
        // the buggy pre-fix code emitted).
        let req = HttpRequest::builder()
            .method("GET")
            .uri("/api/v1/system/user/u-aaa")
            .body(Body::empty())
            .unwrap();

        assert_eq!(route_label(&req), UNMATCHED_PATH_LABEL);
    }

    /// Happy path: when the router has populated `MatchedPath` in
    /// request extensions, `route_label` returns the route template.
    /// MatchedPath's constructor is non-public in axum, so we build
    /// a fixture via `axum::Router` — the one reliable way to
    /// simulate what the real routing stack produces.
    #[tokio::test]
    async fn route_label_returns_matched_path_when_set_by_router() {
        use axum::extract::Request as AxumRequest;
        use axum::routing::get;
        use axum::Router;
        use std::sync::Arc;
        use tokio::sync::Mutex;

        // Capture the route_label the middleware observes for each
        // request. Using a Mutex<Option<String>> so we can read it
        // back after the request completes.
        let observed: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let observed_clone = observed.clone();

        let app = Router::new()
            .route("/users/{id}", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(
                move |req: AxumRequest, next: axum::middleware::Next| {
                    let observed = observed_clone.clone();
                    async move {
                        let label = route_label(&req);
                        *observed.lock().await = Some(label);
                        next.run(req).await
                    }
                },
            ));

        // Send a request with a dynamic id segment.
        let req = HttpRequest::builder()
            .method("GET")
            .uri("/users/u-zzz")
            .body(Body::empty())
            .unwrap();
        let _ = <Router as tower::ServiceExt<AxumRequest>>::oneshot(app, req)
            .await
            .unwrap();

        let got = observed.lock().await.clone();
        // The KEY assertion: the dynamic `u-zzz` must NOT appear in
        // the label. The label MUST be the route template.
        assert_eq!(
            got.as_deref(),
            Some("/users/{id}"),
            "route_label must return the matched route template, not the raw URI"
        );
    }
}

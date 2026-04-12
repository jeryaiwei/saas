//! Health check + metrics endpoints.

use crate::state::AppState;
use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::get, Json, Router};
use framework::infra::{pg, redis};
use serde::Serialize;

#[derive(Debug, Serialize)]
struct LiveResponse {
    status: &'static str,
}

#[derive(Debug, Serialize)]
struct ReadyResponse {
    status: &'static str,
    details: ReadyDetails,
}

#[derive(Debug, Serialize)]
struct ReadyDetails {
    pg: CheckResult,
    redis: CheckResult,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
enum CheckResult {
    Up,
    Down,
}

async fn live() -> impl IntoResponse {
    (StatusCode::OK, Json(LiveResponse { status: "ok" }))
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    let (pg_ok, redis_ok) = tokio::join!(pg::ping(&state.pg), redis::ping(&state.redis));

    let pg_result = if pg_ok.is_ok() {
        CheckResult::Up
    } else {
        CheckResult::Down
    };
    let redis_result = if redis_ok.is_ok() {
        CheckResult::Up
    } else {
        CheckResult::Down
    };

    let status_code = if pg_ok.is_ok() && redis_ok.is_ok() {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadyResponse {
            status: if matches!(status_code, StatusCode::OK) {
                "ok"
            } else {
                "degraded"
            },
            details: ReadyDetails {
                pg: pg_result,
                redis: redis_result,
            },
        }),
    )
}

async fn metrics(State(state): State<AppState>) -> impl IntoResponse {
    let body = state.metrics.render();
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        body,
    )
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/metrics", get(metrics))
}

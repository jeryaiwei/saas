//! HTTP-layer integration tests for the axum web server.
//!
//! These tests exercise the full middleware stack (tenant_http → auth → tenant
//! guard → route layers) through `tower::ServiceExt::oneshot`, hitting the
//! live dev database.

#[path = "common/mod.rs"]
mod common;

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::middleware::{from_fn, from_fn_with_state};
use axum::Router;
use framework::middleware::{
    auth as auth_mw, tenant as tenant_mw, tenant_http,
};
use http_body_util::BodyExt;
use std::sync::Arc;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// Router builder — adds the same global middleware as app/main.rs
// ---------------------------------------------------------------------------

/// Build the test router with the full middleware stack. The bare
/// `modules::router(state)` only carries route-level layers (permission,
/// operlog). We need the same global layers that `main.rs` adds so that
/// JWT auth, tenant context, and request-id propagation work end-to-end.
async fn build_test_router() -> Router {
    let (state, bare_router) = common::build_state_and_router().await;

    let auth_state = auth_mw::AuthState {
        jwt: Arc::new(state.config.jwt.clone()),
        redis: state.redis.clone(),
        redis_keys: Arc::new(state.config.redis_keys.clone()),
        whitelist: Arc::new(test_whitelist()),
    };
    let tenant_state = tenant_mw::TenantState {
        tenant: Arc::new(state.config.tenant.clone()),
    };

    // Mirror the layer order from main.rs (last applied = outermost):
    // tenant_http → auth → tenant_guard → (route layers)
    bare_router
        .layer(from_fn_with_state(tenant_state, tenant_mw::tenant_guard))
        .layer(axum::Extension(state.pg.clone()))
        .layer(from_fn_with_state(auth_state, auth_mw::auth))
        .layer(from_fn(tenant_http::tenant_http))
}

/// Whitelist for the test router. Paths here are WITHOUT the `/api/v1`
/// prefix because `modules::router()` merges routes flat.
fn test_whitelist() -> Vec<String> {
    vec![
        "POST:/auth/login".into(),
        "GET:/auth/code".into(),
        "GET:/auth/tenant/list".into(),
        "POST:/auth/refresh-token".into(),
        "/health".into(),
        "GET:/metrics".into(),
    ]
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Parse response body as JSON.
async fn body_json(resp: axum::response::Response) -> serde_json::Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

/// Login as admin and return the access token.
async fn login(router: &Router) -> String {
    let req = Request::builder()
        .method("POST")
        .uri("/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("tenant-id", "000000")
        .body(Body::from(
            r#"{"username":"admin","password":"admin123"}"#,
        ))
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    json["data"]["access_token"]
        .as_str()
        .expect("login must return access_token")
        .to_string()
}

/// Build a GET request with auth headers.
fn get(uri: &str, token: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .header("Authorization", format!("Bearer {token}"))
        .header("tenant-id", "000000")
        .body(Body::empty())
        .unwrap()
}

/// Build a POST request with JSON body and auth headers.
fn post_json(uri: &str, token: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("Authorization", format!("Bearer {token}"))
        .header("tenant-id", "000000")
        .body(Body::from(body.to_string()))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Test cases
// ---------------------------------------------------------------------------

/// 1. GET /health/live returns 200 with no auth required.
#[tokio::test]
async fn health_live() {
    let router = build_test_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/health/live")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

/// 2. POST /auth/login with valid credentials returns 200 with a non-empty
///    access_token.
#[tokio::test]
async fn auth_login_success() {
    let router = build_test_router().await;
    let req = Request::builder()
        .method("POST")
        .uri("/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("tenant-id", "000000")
        .body(Body::from(
            r#"{"username":"admin","password":"admin123"}"#,
        ))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["code"], 200);
    let token = json["data"]["access_token"].as_str().unwrap();
    assert!(!token.is_empty(), "access_token must not be empty");
}

/// 3. POST /auth/login with wrong password returns a non-200 business code.
#[tokio::test]
async fn auth_login_wrong_password() {
    let router = build_test_router().await;
    let req = Request::builder()
        .method("POST")
        .uri("/auth/login")
        .header(header::CONTENT_TYPE, "application/json")
        .header("tenant-id", "000000")
        .body(Body::from(
            r#"{"username":"admin","password":"wrong_password_xyz"}"#,
        ))
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    let code = json["code"].as_i64().unwrap();
    assert_ne!(code, 200, "wrong password must not return success code");
}

/// 4. GET a protected endpoint without Authorization header returns 401.
#[tokio::test]
async fn no_token_returns_401() {
    let router = build_test_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/system/role/list?pageNum=1&pageSize=10")
        .header("tenant-id", "000000")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// 5. GET a protected endpoint with a valid token returns 200.
#[tokio::test]
async fn valid_token_returns_200() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let req = get("/system/role/list?pageNum=1&pageSize=10", &token);
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["code"], 200);
}

/// 6. Every API response has the standard envelope: code, msg, data,
///    requestId, timestamp.
#[tokio::test]
async fn response_envelope_format() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let req = get("/system/role/list?pageNum=1&pageSize=10", &token);
    let resp = router.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;

    assert!(json.get("code").is_some(), "missing 'code' field");
    assert!(json.get("msg").is_some(), "missing 'msg' field");
    assert!(json.get("data").is_some(), "missing 'data' field");
    assert!(
        json.get("requestId").is_some(),
        "missing 'requestId' field"
    );
    assert!(
        json.get("timestamp").is_some(),
        "missing 'timestamp' field"
    );
}

/// 7. Pagination responses include rows (array), total (number), pageNum,
///    pageSize.
#[tokio::test]
async fn pagination_format() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let req = get("/system/role/list?pageNum=1&pageSize=10", &token);
    let resp = router.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    let data = &json["data"];

    assert!(data["rows"].is_array(), "data.rows must be an array");
    assert!(data["total"].is_number(), "data.total must be a number");
    assert!(data["pageNum"].is_number(), "data.pageNum must be a number");
    assert!(
        data["pageSize"].is_number(),
        "data.pageSize must be a number"
    );
}

/// 8. Role list response uses camelCase field names (roleId, roleName).
#[tokio::test]
async fn camel_case_serialization() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let req = get("/system/role/list?pageNum=1&pageSize=10", &token);
    let resp = router.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    let rows = json["data"]["rows"].as_array().unwrap();

    // There should be at least one role in the dev database.
    assert!(!rows.is_empty(), "expected at least one role in dev DB");
    let first = &rows[0];
    assert!(
        first.get("roleId").is_some(),
        "expected camelCase field 'roleId'"
    );
    assert!(
        first.get("roleName").is_some(),
        "expected camelCase field 'roleName'"
    );
    // Verify snake_case fields are absent.
    assert!(
        first.get("role_id").is_none(),
        "snake_case 'role_id' should not appear"
    );
    assert!(
        first.get("role_name").is_none(),
        "snake_case 'role_name' should not appear"
    );
}

/// 9. POST with an empty body to a validated endpoint returns 400.
#[tokio::test]
async fn dto_validation_missing_field() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let req = post_json("/system/post/", &token, "{}");
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "empty body should trigger validation error (400)"
    );
}

/// 10. POST with a field exceeding max length returns 400.
#[tokio::test]
async fn dto_validation_field_too_long() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let long_name = "x".repeat(300);
    let body = serde_json::json!({
        "postCode": "http-test-long",
        "postName": long_name,
    })
    .to_string();
    let req = post_json("/system/post/", &token, &body);
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "field exceeding max length should trigger validation error (400)"
    );
}

/// 11. Business errors are wrapped in the standard envelope with the
///     correct business code. Sending a mail with a nonexistent template
///     code should return code 7140 (MAIL_TEMPLATE_NOT_FOUND).
#[tokio::test]
async fn business_error_envelope() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let body = serde_json::json!({
        "toMail": "test@example.com",
        "templateCode": "nonexistent_template_code_xyz_999",
    })
    .to_string();
    let req = post_json("/message/mail-send", &token, &body);
    let resp = router.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;

    assert_eq!(
        json["code"].as_i64().unwrap(),
        7140,
        "expected MAIL_TEMPLATE_NOT_FOUND (7140)"
    );
    let msg = json["msg"].as_str().unwrap();
    assert!(!msg.is_empty(), "error msg must not be empty");
}

/// 12. POST /common/upload with a non-multipart body should not return 200.
#[tokio::test]
async fn upload_missing_file() {
    let router = build_test_router().await;
    let token = login(&router).await;
    // Send a JSON body to a multipart handler — axum rejects this.
    let req = post_json("/common/upload", &token, r#"{"file":"nope"}"#);
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::OK,
        "upload with wrong content type must not succeed"
    );
}

/// 13. GET /common/upload/<nonexistent-id> returns business code 5020
///     (FILE_NOT_FOUND).
#[tokio::test]
async fn download_not_found() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let req = get("/common/upload/nonexistent-id-xyz-999", &token);
    let resp = router.clone().oneshot(req).await.unwrap();
    let json = body_json(resp).await;
    assert_eq!(
        json["code"].as_i64().unwrap(),
        5020,
        "expected FILE_NOT_FOUND (5020)"
    );
}

/// 14. GET a completely nonexistent path returns 404 (with a valid token
///     so the auth middleware doesn't short-circuit to 401).
#[tokio::test]
async fn not_found_path() {
    let router = build_test_router().await;
    let token = login(&router).await;
    let req = get("/nonexistent/path", &token);
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

/// 15. GET /health/ready returns 200 (verifies DB + Redis connectivity).
#[tokio::test]
async fn health_ready() {
    let router = build_test_router().await;
    let req = Request::builder()
        .method("GET")
        .uri("/health/ready")
        .body(Body::empty())
        .unwrap();
    let resp = router.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let json = body_json(resp).await;
    assert_eq!(json["status"], "ok");
}

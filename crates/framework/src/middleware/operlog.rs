//! `operlog` — operation log middleware.
//!
//! Two-part design:
//! 1. **Route-level**: `OperlogMarkLayer` — lightweight layer that inserts
//!    an `OperlogMark` into request extensions (title + business_type).
//! 2. **Global-level**: `global_operlog` — reads the mark, buffers
//!    request/response body, writes `sys_oper_log` asynchronously.
//!
//! ## Usage
//!
//! handler.rs — mark routes that need logging:
//! ```ignore
//! use framework::operlog;
//!
//! pub fn router() -> OpenApiRouter<AppState> {
//!     OpenApiRouter::new()
//!         .routes(routes!(create)
//!             .layer(require_permission!("system:role:add"))
//!             .layer(operlog!("角色管理", Insert)))
//! }
//! ```
//!
//! main.rs — add the global layer (runs for all routes, no-op without mark):
//! ```ignore
//! use framework::middleware::operlog;
//! .layer(from_fn(move |req, next| operlog::global_operlog(pg_pool.clone(), req, next)))
//! ```

use crate::context::{self, RequestContext};
use axum::{
    body::{self, Body},
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use sqlx::PgPool;
use std::time::Instant;
use tower::Layer;
use tower::Service;

// ─── Business type constants ────────────────────────────────────────────────

/// Mirrors NestJS `BusinessType` enum.
pub struct BusinessType;

#[allow(non_upper_case_globals)]
impl BusinessType {
    pub const Other: i32 = 0;
    pub const Insert: i32 = 1;
    pub const Update: i32 = 2;
    pub const Delete: i32 = 3;
    pub const Grant: i32 = 4;
    pub const Export: i32 = 5;
    pub const Import: i32 = 6;
    pub const Clean: i32 = 9;
}

// ─── Route-level mark ───────────────────────────────────────────────────────

/// Marker set by the route-level `operlog!` layer. The global middleware
/// reads this to decide whether and how to log.
#[derive(Clone)]
pub struct OperlogMark {
    pub title: &'static str,
    pub business_type: i32,
}

/// Tower Layer that inserts `OperlogMark` into request extensions.
/// Built by the `operlog!` macro.
#[derive(Clone)]
pub struct OperlogMarkLayer {
    mark: OperlogMark,
}

impl OperlogMarkLayer {
    pub fn new(title: &'static str, business_type: i32) -> Self {
        Self {
            mark: OperlogMark {
                title,
                business_type,
            },
        }
    }
}

impl<S> Layer<S> for OperlogMarkLayer {
    type Service = OperlogMarkService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OperlogMarkService {
            inner,
            mark: self.mark.clone(),
        }
    }
}

#[derive(Clone)]
pub struct OperlogMarkService<S> {
    inner: S,
    mark: OperlogMark,
}

impl<S> Service<Request> for OperlogMarkService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, mut req: Request) -> Self::Future {
        req.extensions_mut().insert(self.mark.clone());
        self.inner.call(req)
    }
}

// ─── Global middleware ──────────────────────────────────────────────────────

/// Global operlog middleware. Add in main.rs as a layer.
/// No-op when the request lacks an `OperlogMark` extension.
pub async fn global_operlog(pool: PgPool, req: Request, next: Next) -> Response {
    let mark = req.extensions().get::<OperlogMark>().cloned();

    if mark.is_none() {
        return next.run(req).await;
    }
    let mark = mark.unwrap();
    let start = Instant::now();
    let method = req.method().as_str().to_owned();
    let uri = req.uri().to_string();

    // Buffer request body, reconstruct for handler.
    let (parts, body) = req.into_parts();
    let body_bytes = body::to_bytes(body, usize::MAX).await.unwrap_or_default();
    let oper_param = truncate(&String::from_utf8_lossy(&body_bytes), 2000);
    let req = Request::from_parts(parts, Body::from(body_bytes));

    let response = next.run(req).await;
    let cost_time = start.elapsed().as_millis() as i32;
    let status_code = response.status();

    // Buffer response body, reconstruct for caller.
    let (resp_parts, body) = response.into_parts();
    let resp_bytes = body::to_bytes(body, usize::MAX).await.unwrap_or_default();
    let json_result = truncate(&String::from_utf8_lossy(&resp_bytes), 2000);
    let (log_status, error_msg) = parse_status(status_code, &resp_bytes);
    let response = Response::from_parts(resp_parts, Body::from(resp_bytes));

    // Async write — does not block response.
    let ctx = RequestContext::current_cloned();
    let title = mark.title;
    let business_type = mark.business_type;

    context::scope_spawn(async move {
        let oper_name = ctx.user_name.as_deref().unwrap_or("");
        let tenant_id = ctx.tenant_id.as_deref().unwrap_or("000000");

        if let Err(e) = sqlx::query(
            "INSERT INTO sys_oper_log (\
                oper_id, tenant_id, title, business_type, request_method, \
                operator_type, oper_name, dept_name, oper_url, oper_location, \
                oper_param, json_result, error_msg, method, oper_ip, status, cost_time\
            ) VALUES (\
                gen_random_uuid(), $1, $2, $3, $4, 1, $5, '', $6, '', \
                $7, $8, $9, $10, '', $11, $12\
            )",
        )
        .bind(tenant_id)
        .bind(title)
        .bind(business_type)
        .bind(&method)
        .bind(oper_name)
        .bind(&uri)
        .bind(&oper_param)
        .bind(&json_result)
        .bind(&error_msg)
        .bind(&method)
        .bind(&log_status)
        .bind(cost_time)
        .execute(&pool)
        .await
        {
            tracing::warn!(error = %e, title, "operlog: failed to write");
        }
    });

    response
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn parse_status(status_code: StatusCode, body: &[u8]) -> (String, String) {
    if status_code != StatusCode::OK {
        return ("1".into(), format!("HTTP {}", status_code.as_u16()));
    }
    match serde_json::from_slice::<serde_json::Value>(body) {
        Ok(v) => {
            let code = v.get("code").and_then(|c| c.as_i64()).unwrap_or(200);
            if code == 200 {
                ("0".into(), String::new())
            } else {
                let msg = v
                    .get("msg")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string();
                ("1".into(), truncate(&msg, 2000))
            }
        }
        Err(_) => ("0".into(), String::new()),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        // Find a valid UTF-8 boundary at or before `max`.
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_string()
    }
}

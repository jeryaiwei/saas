//! `operlog` — operation log route-level middleware.
//!
//! `OperlogLayer` wraps write handlers, buffers request/response body,
//! and writes `sys_oper_log` asynchronously. `PgPool` is read from
//! request extensions (injected by main.rs via `Extension<PgPool>`).
//!
//! ## Usage
//!
//! handler.rs — mark write routes:
//! ```ignore
//! use framework::operlog;
//!
//! .routes(routes!(create).map(|r| {
//!     r.layer::<_, Infallible>(require_permission!("system:role:add"))
//!         .layer(operlog!("角色管理", Insert))
//! }))
//! ```
//!
//! main.rs — inject PgPool into request extensions:
//! ```ignore
//! use axum::Extension;
//! .layer(Extension(state.pg.clone()))
//! ```

use crate::context::{self, RequestContext};
use axum::{
    body::{self, Body},
    extract::Request,
    http::StatusCode,
    response::Response,
};
use sqlx::PgPool;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;
use tower::{Layer, Service};

// ─── BusinessType ───────────────────────────────────────────────────────

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

// ─── Layer ──────────────────────────────────────────────────────────────

/// Route-level Tower Layer. Buffers request/response body and writes
/// `sys_oper_log` asynchronously after handler execution.
#[derive(Clone)]
pub struct OperlogLayer {
    title: &'static str,
    business_type: i32,
}

impl OperlogLayer {
    pub fn new(title: &'static str, business_type: i32) -> Self {
        Self {
            title,
            business_type,
        }
    }
}

impl<S> Layer<S> for OperlogLayer {
    type Service = OperlogService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        OperlogService {
            inner,
            title: self.title,
            business_type: self.business_type,
        }
    }
}

// ─── Service ────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct OperlogService<S> {
    inner: S,
    title: &'static str,
    business_type: i32,
}

impl<S> Service<Request> for OperlogService<S>
where
    S: Service<Request, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let pool = req.extensions().get::<PgPool>().cloned();
        let mut inner = self.inner.clone();
        let title = self.title;
        let business_type = self.business_type;

        Box::pin(async move {
            let start = Instant::now();
            let method = req.method().as_str().to_owned();
            let uri = req.uri().to_string();

            // Buffer request body, reconstruct for handler.
            let (parts, req_body) = req.into_parts();
            let body_bytes = body::to_bytes(req_body, usize::MAX)
                .await
                .unwrap_or_default();
            let oper_param = truncate(&String::from_utf8_lossy(&body_bytes), 2000);
            let req = Request::from_parts(parts, Body::from(body_bytes));

            // Execute handler.
            let response = inner.call(req).await?;
            let cost_time = start.elapsed().as_millis() as i32;
            let status_code = response.status();

            // Buffer response body, reconstruct for caller.
            let (resp_parts, resp_body) = response.into_parts();
            let resp_bytes = body::to_bytes(resp_body, usize::MAX)
                .await
                .unwrap_or_default();
            let json_result = truncate(&String::from_utf8_lossy(&resp_bytes), 2000);
            let (log_status, error_msg) = parse_status(status_code, &resp_bytes);
            let response = Response::from_parts(resp_parts, Body::from(resp_bytes));

            // Async write — does not block response.
            if let Some(pool) = pool {
                let ctx = RequestContext::current_cloned();
                context::scope_spawn(async move {
                    let oper_name = ctx.user_name.as_deref().unwrap_or("");
                    let tenant_id = ctx.tenant_id.as_deref().unwrap_or("000000");

                    if let Err(e) = sqlx::query(
                        "INSERT INTO sys_oper_log (\
                            oper_id, tenant_id, title, business_type, request_method, \
                            operator_type, oper_name, dept_name, oper_url, oper_location, \
                            oper_param, json_result, error_msg, method, oper_ip, \
                            status, cost_time\
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
            } else {
                tracing::warn!(title, "operlog: PgPool extension missing, log skipped");
            }

            Ok(response)
        })
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────

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
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        s[..end].to_string()
    }
}

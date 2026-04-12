//! `tenant_http` — outermost request context middleware.
//!
//! Runs before any guards. Responsibilities:
//!
//! 1. Parse or generate `x-request-id` for distributed tracing.
//! 2. Parse `Accept-Language` → `RequestContext.lang_code`.
//! 3. Read the optional `tenant-id` header (rarely used since Gate 0 verified
//!    that tenant routing is session-based, not header-based — kept as a
//!    bootstrap hint for public endpoints).
//! 4. Wrap `next.run(req)` inside [`crate::context::scope`] so every downstream
//!    layer and handler has access to the context.

use crate::context::{scope, RequestContext};
use crate::i18n;
use axum::{
    extract::{MatchedPath, Request},
    http::HeaderMap,
    middleware::Next,
    response::Response,
};
use tracing::{field, info_span, Instrument};

pub async fn tenant_http(req: Request, next: Next) -> Response {
    let headers = req.headers();
    let request_id = extract_request_id(headers);
    let lang_code = extract_lang(headers);
    let tenant_id = headers
        .get("tenant-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let method = req.method().as_str().to_owned();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "<unmatched>".to_string());

    // Root span — all downstream #[instrument] spans nest inside this one,
    // so request_id / tenant_id / user_id appear automatically in every log
    // line. user_id / user_name are filled by auth middleware after session load.
    let span = info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %path,
        tenant_id = field::Empty,
        user_id = field::Empty,
        user_name = field::Empty,
        status = field::Empty,
    );

    if let Some(ref t) = tenant_id {
        span.record("tenant_id", t.as_str());
    }

    let ctx = RequestContext {
        request_id: Some(request_id),
        tenant_id,
        lang_code: Some(lang_code),
        ..Default::default()
    };

    async move {
        let response = scope(ctx, next.run(req)).await;
        tracing::Span::current().record("status", response.status().as_u16());
        response
    }
    .instrument(span)
    .await
}

fn extract_request_id(h: &HeaderMap) -> String {
    h.get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

/// Parse the first language tag from `Accept-Language`.
/// Example: `"zh-CN,en-US;q=0.9,en;q=0.8"` → `"zh-CN"`.
fn extract_lang(h: &HeaderMap) -> String {
    h.get("accept-language")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| i18n::DEFAULT_LANG.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn lang_picks_first_tag() {
        let mut h = HeaderMap::new();
        h.insert(
            "accept-language",
            HeaderValue::from_static("en-US,zh-CN;q=0.9,en;q=0.8"),
        );
        assert_eq!(extract_lang(&h), "en-US");
    }

    #[test]
    fn lang_strips_quality() {
        let mut h = HeaderMap::new();
        h.insert("accept-language", HeaderValue::from_static("zh-CN;q=0.9"));
        assert_eq!(extract_lang(&h), "zh-CN");
    }

    #[test]
    fn lang_falls_back_to_default() {
        let h = HeaderMap::new();
        assert_eq!(extract_lang(&h), i18n::DEFAULT_LANG);
    }

    #[test]
    fn request_id_is_taken_from_header() {
        let mut h = HeaderMap::new();
        h.insert("x-request-id", HeaderValue::from_static("abc-123"));
        assert_eq!(extract_request_id(&h), "abc-123");
    }

    #[test]
    fn request_id_generates_uuid_when_missing() {
        let h = HeaderMap::new();
        let id = extract_request_id(&h);
        // UUID v4 is 36 chars with 4 hyphens.
        assert_eq!(id.len(), 36);
        assert_eq!(id.matches('-').count(), 4);
    }
}

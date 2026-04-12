//! `ApiResponse<T>` — the unified response envelope for every HTTP handler.
//!
//! Mirrors the NestJS `Result<T>` shape exactly:
//! ```json
//! { "code": 200, "msg": "...", "data": ..., "requestId": "...", "timestamp": "..." }
//! ```
//!
//! The `request_id` and `timestamp` are populated from the active
//! [`RequestContext`] at serialization time.

use crate::context::RequestContext;
use crate::i18n;
use crate::response::ResponseCode;
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{SecondsFormat, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    pub code: i32,
    pub msg: String,
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub timestamp: String,
}

impl<T> ApiResponse<T> {
    /// Wrap `data` with a 200 SUCCESS envelope. The only way to
    /// construct a successful response — error responses must go
    /// through `AppError::IntoResponse`.
    pub fn ok(data: T) -> Self {
        let lang = RequestContext::with_current(|c| c.lang_code.clone())
            .flatten()
            .unwrap_or_else(|| i18n::DEFAULT_LANG.to_string());
        let request_id = RequestContext::with_current(|c| c.request_id.clone()).flatten();
        Self {
            code: ResponseCode::SUCCESS.as_i32(),
            msg: i18n::get_message(ResponseCode::SUCCESS, &lang),
            data: Some(data),
            request_id,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }
}

impl ApiResponse<()> {
    /// Success envelope with no payload (create/update/delete endpoints).
    pub fn success() -> Self {
        let lang = RequestContext::with_current(|c| c.lang_code.clone())
            .flatten()
            .unwrap_or_else(|| i18n::DEFAULT_LANG.to_string());
        let request_id = RequestContext::with_current(|c| c.request_id.clone()).flatten();
        Self {
            code: ResponseCode::SUCCESS.as_i32(),
            msg: i18n::get_message(ResponseCode::SUCCESS, &lang),
            data: None,
            request_id,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }
}

impl ApiResponse<serde_json::Value> {
    /// Error envelope used exclusively by `AppError::IntoResponse`.
    /// Not for handler/service use — those paths return
    /// `Result<ApiResponse<T>, AppError>` and the error branch is
    /// serialized through this helper automatically. See
    /// `docs/framework-error-envelope-spec.md` §2.2 for why this is
    /// `pub(crate)` rather than public.
    ///
    /// `data` is `serde_json::Value` so the `Validation` variant can
    /// carry a list of `FieldError`s while the other variants carry
    /// `Value::Null`.
    pub(crate) fn error(code: ResponseCode, msg: String, data: serde_json::Value) -> Self {
        let request_id = RequestContext::with_current(|c| c.request_id.clone()).flatten();
        Self {
            code: code.as_i32(),
            msg,
            data: Some(data),
            request_id,
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }
}

impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response {
        (StatusCode::OK, Json(self)).into_response()
    }
}

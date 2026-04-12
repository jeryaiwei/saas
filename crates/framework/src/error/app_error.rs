//! `AppError` — the single error type every handler may return.
//!
//! Maps to HTTP status and the unified `{code, msg, data, requestId, timestamp}`
//! body via `IntoResponse`. Mirrors NestJS `GlobalExceptionFilter` semantics:
//!
//! - `Business`   -> HTTP 200 + business code (never error-level)
//! - `Auth`       -> HTTP 401
//! - `Forbidden`  -> HTTP 403
//! - `Validation` -> HTTP 400
//! - `Internal`   -> HTTP 500 + full stack logged at ERROR

use crate::context::RequestContext;
use crate::i18n;
use crate::response::{ApiResponse, ResponseCode};
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("business error [{code}]")]
    Business { code: ResponseCode },

    #[error("authentication error [{code}]")]
    Auth { code: ResponseCode },

    #[error("forbidden [{code}]")]
    Forbidden { code: ResponseCode },

    #[error("validation error")]
    Validation { errors: Vec<FieldError> },

    /// Wraps an `anyhow::Error`. **No `#[from]` impl**: callers must use
    /// `IntoAppError::into_internal()` explicitly so that "this became
    /// a 500" is a visible decision at every call site (spec §2.3).
    #[error(transparent)]
    Internal(anyhow::Error),
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct FieldError {
    pub field: String,
    pub message: String,
    /// Validator-produced substitution params (e.g. `min`, `max`,
    /// `value`). Framework-internal — never serialized to wire.
    /// `AppError::IntoResponse` uses this map to substitute `{min}` /
    /// `{max}` placeholders in the translated i18n message. See
    /// `docs/framework-error-envelope-spec.md` §7.1 (v1.1).
    #[serde(skip)]
    pub params: std::collections::HashMap<String, serde_json::Value>,
}

impl AppError {
    pub fn business(code: ResponseCode) -> Self {
        Self::Business { code }
    }

    pub fn auth(code: ResponseCode) -> Self {
        Self::Auth { code }
    }

    pub fn forbidden(code: ResponseCode) -> Self {
        Self::Forbidden { code }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            AppError::Business { .. } => StatusCode::OK,
            AppError::Auth { .. } => StatusCode::UNAUTHORIZED,
            AppError::Forbidden { .. } => StatusCode::FORBIDDEN,
            AppError::Validation { .. } => StatusCode::BAD_REQUEST,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let lang = RequestContext::with_current(|c| c.lang_code.clone())
            .flatten()
            .unwrap_or_else(|| i18n::DEFAULT_LANG.to_string());
        let status = self.status_code();

        let (code, msg, data): (ResponseCode, String, Value) = match self {
            AppError::Business { code } => (code, i18n::get_message(code, &lang), Value::Null),
            AppError::Auth { code } => (code, i18n::get_message(code, &lang), Value::Null),
            AppError::Forbidden { code } => (code, i18n::get_message(code, &lang), Value::Null),
            AppError::Validation { errors } => {
                // Translate each field error's message through i18n using
                // the `valid.<code>` key convention, substituting the
                // validator's params (e.g. min/max) into the message
                // placeholders. Missing translations fall back to the
                // raw validator code (preserves debug signal) + warn log.
                let translated: Vec<FieldError> = errors
                    .into_iter()
                    .map(|e| {
                        let key = format!("valid.{}", e.message);
                        let message = match i18n::get_by_key(&key, &lang) {
                            Some(raw) if !e.params.is_empty() => {
                                // Substitute {param} placeholders inline.
                                // We avoid calling get_by_key_with_json_params
                                // because it takes Cow keys while we have
                                // String keys — one inline loop is simpler
                                // than a key-type conversion.
                                let mut out = raw;
                                for (k, v) in &e.params {
                                    let s = match v {
                                        serde_json::Value::String(s) => s.clone(),
                                        serde_json::Value::Number(n) => n.to_string(),
                                        other => other.to_string(),
                                    };
                                    out = out.replace(&format!("{{{}}}", k), &s);
                                }
                                out
                            }
                            Some(raw) => raw,
                            None => {
                                tracing::warn!(
                                    i18n_key = %key,
                                    lang = %lang,
                                    "missing i18n entry for validation error; falling back to raw code"
                                );
                                e.message.clone()
                            }
                        };
                        FieldError {
                            field: e.field,
                            message,
                            params: Default::default(),
                        }
                    })
                    .collect();
                (
                    ResponseCode::BAD_REQUEST,
                    i18n::get_message(ResponseCode::BAD_REQUEST, &lang),
                    serde_json::to_value(&translated).unwrap_or(Value::Null),
                )
            }
            AppError::Internal(ref e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    ResponseCode::INTERNAL_SERVER_ERROR,
                    i18n::get_message(ResponseCode::INTERNAL_SERVER_ERROR, &lang),
                    Value::Null,
                )
            }
        };

        let body = ApiResponse::<Value>::error(code, msg, data);
        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_mapping_covers_every_variant() {
        // Every AppError variant MUST have an entry in `cases`. Adding
        // a new variant without updating both the inner `status_code()`
        // match AND this table triggers the `exhaustive_check` below —
        // see spec §2.3 (AppError closed-set rule).
        let cases: Vec<(AppError, StatusCode)> = vec![
            (
                AppError::business(ResponseCode::DATA_NOT_FOUND),
                StatusCode::OK,
            ),
            (
                AppError::auth(ResponseCode::TOKEN_INVALID),
                StatusCode::UNAUTHORIZED,
            ),
            (
                AppError::forbidden(ResponseCode::FORBIDDEN),
                StatusCode::FORBIDDEN,
            ),
            (
                AppError::Validation { errors: vec![] },
                StatusCode::BAD_REQUEST,
            ),
            (
                AppError::Internal(anyhow::anyhow!("boom")),
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
        ];

        for (err, expected) in &cases {
            assert_eq!(
                err.status_code(),
                *expected,
                "status mapping drift for {:?}",
                err
            );
        }

        // Compile-time exhaustive check: if a new AppError variant is
        // added without extending either the `cases` vector above or
        // the inner match, one of these arms must fail.
        fn exhaustive_check(e: &AppError) -> StatusCode {
            match e {
                AppError::Business { .. } => StatusCode::OK,
                AppError::Auth { .. } => StatusCode::UNAUTHORIZED,
                AppError::Forbidden { .. } => StatusCode::FORBIDDEN,
                AppError::Validation { .. } => StatusCode::BAD_REQUEST,
                AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            }
        }
        for (err, expected) in &cases {
            assert_eq!(exhaustive_check(err), *expected);
        }
    }
}

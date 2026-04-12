//! `ValidatedQuery<T>` — an axum query-string extractor that runs
//! `Validate::validate` on the deserialized struct and routes both
//! deserialize failures and validation failures through
//! [`AppError::Validation`] so the wire response is the unified
//! `{code: 400, msg, data: [...], requestId, timestamp}` envelope
//! instead of axum's default `text/plain` 400.
//!
//! Mirrors [`ValidatedJson`] for JSON-body handlers.

use crate::error::{AppError, FieldError};
use axum::extract::rejection::QueryRejection;
use axum::extract::{FromRequestParts, Query};
use axum::http::request::Parts;
use serde::de::DeserializeOwned;
use validator::Validate;

use super::validated_json::validation_errors_to_app_error;

#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ValidatedQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate + Send + 'static,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(query_rejection_to_app_error)?;
        value.validate().map_err(validation_errors_to_app_error)?;
        Ok(Self(value))
    }
}

fn query_rejection_to_app_error(rej: QueryRejection) -> AppError {
    let message = rej.body_text();
    tracing::debug!(error = %message, "query deserialize rejection");
    AppError::Validation {
        errors: vec![FieldError {
            field: "query".into(),
            message,
            params: Default::default(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, Validate)]
    struct Q {
        #[validate(range(min = 1, max = 100))]
        page_size: u32,
    }

    #[test]
    fn validation_error_flat_fields() {
        let q = Q { page_size: 500 };
        let errs = q.validate().unwrap_err();
        let app = validation_errors_to_app_error(errs);
        match app {
            AppError::Validation { errors } => {
                assert_eq!(errors.len(), 1);
                assert_eq!(errors[0].field, "page_size");
            }
            _ => panic!("expected Validation"),
        }
    }

    #[test]
    fn validation_success_is_ok() {
        let q = Q { page_size: 10 };
        assert!(q.validate().is_ok());
    }
}

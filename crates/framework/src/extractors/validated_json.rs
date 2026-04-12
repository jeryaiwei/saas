//! `ValidatedJson<T>` — a `Json` extractor that also runs `Validate::validate`
//! and maps every failure (malformed JSON, missing field, failed validation
//! rule) to [`AppError::Validation`] so the wire response stays
//! `{code: 400, msg, data: [...], requestId, timestamp}`.

use crate::error::{AppError, FieldError};
use axum::{
    extract::{rejection::JsonRejection, FromRequest, Request},
    Json,
};
use serde::de::DeserializeOwned;
use validator::{Validate, ValidationErrors, ValidationErrorsKind};

#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate + Send + 'static,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(json_rejection_to_app_error)?;
        value.validate().map_err(validation_errors_to_app_error)?;
        Ok(Self(value))
    }
}

fn json_rejection_to_app_error(rej: JsonRejection) -> AppError {
    let message = rej.body_text();
    tracing::debug!(error = %message, "json deserialize rejection");
    AppError::Validation {
        errors: vec![FieldError {
            field: "body".into(),
            message,
            params: Default::default(),
        }],
    }
}

pub fn validation_errors_to_app_error(errors: ValidationErrors) -> AppError {
    let mut out = Vec::new();
    collect_errors("", &errors, &mut out);
    AppError::Validation { errors: out }
}

/// Walk a `ValidationErrors` tree, flattening every leaf `Field` error
/// into `FieldError { field: "outer.inner", message }`. Handles the
/// nested `#[validate(nested)]` case used by `PageQuery` flattened into
/// list-query DTOs — without this the outer DTO's `data:[]` would be
/// empty when a nested bound fails.
fn collect_errors(prefix: &str, errors: &ValidationErrors, out: &mut Vec<FieldError>) {
    for (field, kind) in errors.errors() {
        let path = if prefix.is_empty() {
            field.to_string()
        } else {
            format!("{prefix}.{field}")
        };
        match kind {
            ValidationErrorsKind::Field(errs) => {
                for err in errs {
                    // Use the validator `code` (e.g. "range", "length")
                    // as the i18n key stem — the IntoResponse path will
                    // look up `valid.<code>` and substitute `err.params`
                    // (min/max/etc) into the message placeholders.
                    let message = err.code.to_string();
                    // Stringify Cow<'static, str> keys into owned String
                    // for framework-internal storage (avoids lifetime
                    // gymnastics in FieldError; params are tiny so the
                    // extra allocation is negligible).
                    let params = err
                        .params
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.clone()))
                        .collect();
                    out.push(FieldError {
                        field: path.clone(),
                        message,
                        params,
                    });
                }
            }
            ValidationErrorsKind::Struct(inner) => {
                collect_errors(&path, inner, out);
            }
            ValidationErrorsKind::List(map) => {
                for (idx, inner) in map {
                    let indexed = format!("{path}[{idx}]");
                    collect_errors(&indexed, inner, out);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use validator::Validate;

    #[derive(Debug, Deserialize, Validate)]
    struct Foo {
        #[validate(length(min = 3, max = 10))]
        name: String,
    }

    #[test]
    fn validation_error_flat_fields() {
        let foo = Foo { name: "x".into() };
        let errs = foo.validate().unwrap_err();
        let app = validation_errors_to_app_error(errs);
        match app {
            AppError::Validation { errors } => {
                assert_eq!(errors.len(), 1);
                assert_eq!(errors[0].field, "name");
            }
            _ => panic!("expected Validation"),
        }
    }

    #[test]
    fn validation_success_produces_no_errors() {
        let foo = Foo {
            name: "alice".into(),
        };
        assert!(foo.validate().is_ok());
    }
}

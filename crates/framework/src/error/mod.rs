//! Application error type + ergonomic extension traits.
//!
//! The old `BusinessError::throw_if_null(...)` / `throw_if(...)` factory
//! was deleted in Phase 2 hygiene — it was fully superseded by the
//! `ext` traits (`Option::or_business`, `bool::business_err_if`,
//! `anyhow::Result::into_internal`) since Sub-Phase 1 Batch 5.5, and
//! zero callers remained in the codebase.

pub mod app_error;
pub mod ext;

pub use app_error::{AppError, FieldError};
pub use ext::{BusinessCheckBool, BusinessCheckOption, IntoAppError};

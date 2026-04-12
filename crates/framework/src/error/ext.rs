//! Extension traits that shortcut common error-mapping patterns in the
//! service layer.

use super::AppError;
use crate::response::ResponseCode;

/// Shortcut `.map_err(AppError::Internal)?` at repo-call boundaries.
///
/// ```ignore
/// let role = RoleRepo::find_by_id(&state.pg, role_id).await.into_internal()?;
/// ```
pub trait IntoAppError<T> {
    fn into_internal(self) -> Result<T, AppError>;
}

impl<T> IntoAppError<T> for anyhow::Result<T> {
    fn into_internal(self) -> Result<T, AppError> {
        self.map_err(AppError::Internal)
    }
}

/// Convert `Option::None` into a `Business(code)` error.
///
/// ```ignore
/// let role = role.or_business(ResponseCode::DATA_NOT_FOUND)?;
/// ```
pub trait BusinessCheckOption<T> {
    fn or_business(self, code: ResponseCode) -> Result<T, AppError>;
}

impl<T> BusinessCheckOption<T> for Option<T> {
    fn or_business(self, code: ResponseCode) -> Result<T, AppError> {
        self.ok_or_else(|| AppError::business(code))
    }
}

/// Return a `Business(code)` error when `self` is `true`.
///
/// ```ignore
/// (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)?;
/// ```
pub trait BusinessCheckBool {
    fn business_err_if(self, code: ResponseCode) -> Result<(), AppError>;
}

impl BusinessCheckBool for bool {
    fn business_err_if(self, code: ResponseCode) -> Result<(), AppError> {
        if self {
            Err(AppError::business(code))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn into_internal_maps_anyhow_to_app_error() {
        let r: anyhow::Result<i32> = Err(anyhow::anyhow!("boom"));
        let mapped = r.into_internal();
        assert!(matches!(mapped, Err(AppError::Internal(_))));
    }

    #[test]
    fn into_internal_passes_ok_through() {
        let r: anyhow::Result<i32> = Ok(42);
        assert_eq!(r.into_internal().unwrap(), 42);
    }

    #[test]
    fn or_business_converts_none() {
        let v: Option<i32> = None;
        let r = v.or_business(ResponseCode::DATA_NOT_FOUND);
        assert!(matches!(
            r,
            Err(AppError::Business { code, .. }) if code == ResponseCode::DATA_NOT_FOUND
        ));
    }

    #[test]
    fn or_business_passes_some_through() {
        let v = Some(7);
        assert_eq!(v.or_business(ResponseCode::DATA_NOT_FOUND).unwrap(), 7);
    }

    #[test]
    fn business_err_if_true_returns_err() {
        let r = true.business_err_if(ResponseCode::DATA_NOT_FOUND);
        assert!(matches!(r, Err(AppError::Business { .. })));
    }

    #[test]
    fn business_err_if_false_returns_ok() {
        assert!(false.business_err_if(ResponseCode::DATA_NOT_FOUND).is_ok());
    }
}

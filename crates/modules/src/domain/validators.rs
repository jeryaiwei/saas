//! Shared `validator` crate custom validators and DTO default helpers.
//!
//! These live in `domain/` rather than in any single `system/*/dto.rs`
//! because the same rules apply across multiple modules — for example
//! `"0"/"1"` status validation is used by both `sys_role` and `sys_user`,
//! and sex validation will apply to any DTO that mirrors the `sys_user.sex`
//! char(1) enum.

use validator::ValidationError;

/// Default value for the `status` field in DTOs — `"0"` (active).
pub fn default_status() -> String {
    "0".into()
}

/// Accept only `"0"` (active) or `"1"` (disabled). Used by DTOs whose
/// `status` field maps to a `char(1)` DB column with no CHECK constraint.
pub fn validate_status_flag(value: &str) -> Result<(), ValidationError> {
    match value {
        "0" | "1" => Ok(()),
        _ => Err(ValidationError::new("status_flag")),
    }
}

/// Accept only `"0"` (male), `"1"` (female), or `"2"` (unknown). Used by
/// user DTOs whose `sex` field maps to a `char(1)` DB column.
pub fn validate_sex_flag(value: &str) -> Result<(), ValidationError> {
    match value {
        "0" | "1" | "2" => Ok(()),
        _ => Err(ValidationError::new("sex_flag")),
    }
}

/// Default value for the `sex` field in DTOs — `"2"` (unknown).
pub fn default_sex() -> String {
    "2".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_status_flag_accepts_zero_and_one() {
        assert!(validate_status_flag("0").is_ok());
        assert!(validate_status_flag("1").is_ok());
    }

    #[test]
    fn validate_status_flag_rejects_others() {
        assert!(validate_status_flag("").is_err());
        assert!(validate_status_flag("2").is_err());
        assert!(validate_status_flag("x").is_err());
    }

    #[test]
    fn validate_sex_flag_accepts_zero_one_two() {
        assert!(validate_sex_flag("0").is_ok());
        assert!(validate_sex_flag("1").is_ok());
        assert!(validate_sex_flag("2").is_ok());
    }

    #[test]
    fn validate_sex_flag_rejects_others() {
        assert!(validate_sex_flag("").is_err());
        assert!(validate_sex_flag("3").is_err());
    }
}

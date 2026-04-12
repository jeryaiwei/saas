//! User DTOs — wire shapes matching NestJS for cross-backend compat.

use crate::domain::validators::{
    default_sex, default_status, validate_sex_flag, validate_status_flag,
};
use crate::domain::SysUser;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Full user detail returned by `GET /system/user/:id` and `POST /system/user/`.
/// Excludes the `password` field — NEVER include it in any wire response.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDetailResponseDto {
    pub user_id: String,
    pub platform_id: String,
    pub dept_id: Option<String>,
    pub user_name: String,
    pub nick_name: String,
    pub user_type: String,
    pub email: String,
    pub phonenumber: String,
    pub sex: String,
    pub avatar: String,
    pub status: String,
    pub remark: Option<String>,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub role_ids: Vec<String>,
}

impl UserDetailResponseDto {
    pub fn from_entity(user: SysUser, role_ids: Vec<String>) -> Self {
        Self {
            user_id: user.user_id,
            platform_id: user.platform_id,
            dept_id: user.dept_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
            user_type: user.user_type,
            email: user.email,
            phonenumber: user.phonenumber,
            sex: user.sex,
            avatar: user.avatar,
            status: user.status,
            remark: user.remark,
            create_by: user.create_by,
            create_at: fmt_ts(&user.create_at),
            update_by: user.update_by,
            update_at: fmt_ts(&user.update_at),
            role_ids,
        }
    }
}

/// Lightweight row for `GET /system/user/list`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserListItemResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub phonenumber: String,
    pub sex: String,
    pub status: String,
    pub dept_id: Option<String>,
    pub create_at: String,
}

impl UserListItemResponseDto {
    pub fn from_entity(user: SysUser) -> Self {
        Self {
            user_id: user.user_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
            email: user.email,
            phonenumber: user.phonenumber,
            sex: user.sex,
            status: user.status,
            dept_id: user.dept_id,
            create_at: fmt_ts(&user.create_at),
        }
    }
}

/// Query string for `GET /system/user/list`.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListUserDto {
    #[validate(length(max = 50))]
    pub user_name: Option<String>,
    #[validate(length(max = 30))]
    pub nick_name: Option<String>,
    #[validate(length(max = 50))]
    pub email: Option<String>,
    #[validate(length(max = 11))]
    pub phonenumber: Option<String>,
    pub status: Option<String>,
    pub dept_id: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

/// Optional search query for `GET /system/user/option-select`.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UserOptionQueryDto {
    #[validate(length(max = 50))]
    pub user_name: Option<String>,
}

/// Dropdown-optimized flat user projection.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOptionResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
}

impl UserOptionResponseDto {
    pub fn from_entity(user: SysUser) -> Self {
        Self {
            user_id: user.user_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
        }
    }
}

/// Response for `GET /system/user/info`. Leaner than Phase 0's
/// `/api/v1/info` — returns just the current user's basic fields.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserInfoResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub phonenumber: String,
    pub avatar: String,
    pub sex: String,
    pub status: String,
    pub remark: Option<String>,
}

impl UserInfoResponseDto {
    pub fn from_entity(user: SysUser) -> Self {
        Self {
            user_id: user.user_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
            email: user.email,
            phonenumber: user.phonenumber,
            avatar: user.avatar,
            sex: user.sex,
            status: user.status,
            remark: user.remark,
        }
    }
}

/// Request body for `POST /system/user/`. Wire-compatible with
/// NestJS `CreateUserRequestDto`.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateUserDto {
    pub dept_id: Option<String>,
    #[validate(length(min = 1, max = 30))]
    pub nick_name: String,
    #[validate(length(min = 2, max = 50))]
    pub user_name: String,
    /// Plaintext password. Will be bcrypt-hashed before insert.
    /// Sub-Phase 2a enforces a relaxed rule: length 6-20. NestJS uses
    /// a stricter upper+lower+digit+symbol rule — deferred until a
    /// documented policy lands.
    #[validate(length(min = 6, max = 20))]
    pub password: String,
    #[validate(length(max = 50))]
    #[serde(default)]
    pub email: String,
    #[validate(length(max = 11))]
    #[serde(default)]
    pub phonenumber: String,
    #[serde(default = "default_sex")]
    #[validate(custom(function = "validate_sex_flag"))]
    pub sex: String,
    #[validate(length(max = 255))]
    #[serde(default)]
    pub avatar: String,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
    #[serde(default)]
    pub role_ids: Vec<String>,
}

/// Request body for `PUT /system/user/change-status`.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ChangeUserStatusDto {
    #[validate(length(min = 1, max = 36))]
    pub user_id: String,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
}

/// Request body for `PUT /system/user/`. Wire-compatible with NestJS
/// `UpdateUserRequestDto`. `user_id` is in the body (NestJS convention
/// for PUT).
///
/// Excludes `user_name` (immutable per NestJS contract) and `password`
/// (changed via `PUT /system/user/reset-pwd` only).
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateUserDto {
    #[validate(length(min = 1, max = 36))]
    pub user_id: String,
    pub dept_id: Option<String>,
    #[validate(length(min = 1, max = 30))]
    pub nick_name: String,
    #[validate(length(max = 50))]
    #[serde(default)]
    pub email: String,
    #[validate(length(max = 11))]
    #[serde(default)]
    pub phonenumber: String,
    #[validate(custom(function = "validate_sex_flag"))]
    pub sex: String,
    #[validate(length(max = 255))]
    #[serde(default)]
    pub avatar: String,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
    #[serde(default)]
    pub role_ids: Vec<String>,
}

/// Request body for `PUT /system/user/reset-pwd`. Admin password reset
/// — no old-password verification required. The super-admin row cannot
/// be reset by this endpoint (blocked by the service guard).
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ResetPwdDto {
    #[validate(length(min = 1, max = 36))]
    pub user_id: String,
    /// New plaintext password. Sub-Phase 2a uses relaxed validation
    /// (length 6-20). Will be bcrypt-hashed by the service.
    #[validate(length(min = 6, max = 20))]
    pub password: String,
}

/// Response for `GET /system/user/auth-role/{id}`. Bundles the target
/// user's detail shape with their current role_ids list. Duplicates
/// `role_ids` at the top level for convenience on the frontend (the
/// Vue role-assignment dialog reads the top-level field).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthRoleResponseDto {
    /// Target user profile — **without** `role_ids`. The role list
    /// lives at the top level of this response only. Using
    /// `UserProfileResponseDto` here (instead of `UserDetailResponseDto`)
    /// avoids both a wire-contract quirk (the same array appearing twice
    /// in the JSON) and a runtime `Vec<String>` clone in the service
    /// layer when building this response.
    pub user: UserProfileResponseDto,
    pub role_ids: Vec<String>,
}

/// Leaner variant of `UserDetailResponseDto` — identical shape minus
/// the `role_ids` field. Used inside `AuthRoleResponseDto` where the
/// role list lives at the enclosing struct's top level and duplicating
/// it inside the user sub-object would serialize twice and force a
/// `Vec<String>` clone in the service layer.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileResponseDto {
    pub user_id: String,
    pub platform_id: String,
    pub dept_id: Option<String>,
    pub user_name: String,
    pub nick_name: String,
    pub user_type: String,
    pub email: String,
    pub phonenumber: String,
    pub sex: String,
    pub avatar: String,
    pub status: String,
    pub remark: Option<String>,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
}

impl UserProfileResponseDto {
    pub fn from_entity(user: SysUser) -> Self {
        Self {
            user_id: user.user_id,
            platform_id: user.platform_id,
            dept_id: user.dept_id,
            user_name: user.user_name,
            nick_name: user.nick_name,
            user_type: user.user_type,
            email: user.email,
            phonenumber: user.phonenumber,
            sex: user.sex,
            avatar: user.avatar,
            status: user.status,
            remark: user.remark,
            create_by: user.create_by,
            create_at: fmt_ts(&user.create_at),
            update_by: user.update_by,
            update_at: fmt_ts(&user.update_at),
        }
    }
}

/// Request body for `PUT /system/user/auth-role`. Replaces the target
/// user's role bindings entirely (delete-all + bulk insert). Empty
/// `role_ids` is the "unassign all" operation.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AuthRoleUpdateDto {
    #[validate(length(min = 1, max = 36))]
    pub user_id: String,
    #[serde(default)]
    pub role_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        AuthRoleUpdateDto, ChangeUserStatusDto, CreateUserDto, ListUserDto, ResetPwdDto,
        UpdateUserDto,
    };
    use framework::response::PageQuery;
    use validator::Validate;

    /// All-zeros UUID used as a filler in DTO validation tests where
    /// the `user_id` field needs to be syntactically valid but is not
    /// the subject of the assertion.
    const TEST_ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

    /// Fixture macro for `CreateUserDto` — starts with a valid default,
    /// override only the fields the test cares about.
    ///
    /// ```ignore
    /// let dto = create_user_dto_fixture!(nick_name: "".into());
    /// ```
    macro_rules! create_user_dto_fixture {
        ($($field:ident : $value:expr),* $(,)?) => {{
            #[allow(unused_mut)]
            let mut dto = CreateUserDto {
                dept_id: None,
                nick_name: "it".into(),
                user_name: "it-user".into(),
                password: "abc123".into(),
                email: "".into(),
                phonenumber: "".into(),
                sex: "2".into(),
                avatar: "".into(),
                status: "0".into(),
                remark: None,
                role_ids: vec![],
            };
            $( dto.$field = $value; )*
            dto
        }};
    }

    /// Fixture macro for `UpdateUserDto` — same pattern as the create
    /// fixture, starts with the all-zeros UUID for `user_id`.
    macro_rules! update_user_dto_fixture {
        ($($field:ident : $value:expr),* $(,)?) => {{
            #[allow(unused_mut)]
            let mut dto = UpdateUserDto {
                user_id: TEST_ZERO_UUID.into(),
                dept_id: None,
                nick_name: "ok".into(),
                email: "".into(),
                phonenumber: "".into(),
                sex: "0".into(),
                avatar: "".into(),
                status: "0".into(),
                remark: None,
                role_ids: vec![],
            };
            $( dto.$field = $value; )*
            dto
        }};
    }

    #[test]
    fn list_user_dto_accepts_valid_defaults() {
        let dto = ListUserDto {
            user_name: None,
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: PageQuery::default(),
        };
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn list_user_dto_rejects_oversize_user_name_filter() {
        let dto = ListUserDto {
            user_name: Some("a".repeat(51)),
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: PageQuery::default(),
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn list_user_dto_rejects_page_num_zero() {
        let dto = ListUserDto {
            user_name: None,
            nick_name: None,
            email: None,
            phonenumber: None,
            status: None,
            dept_id: None,
            page: PageQuery {
                page_num: 0,
                page_size: 10,
            },
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_user_dto_rejects_short_password() {
        let dto = create_user_dto_fixture!(password: "short".into());
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_user_dto_rejects_empty_nick_name() {
        let dto = create_user_dto_fixture!(nick_name: "".into());
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_user_dto_accepts_valid_minimum() {
        let dto = create_user_dto_fixture!();
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn change_user_status_dto_rejects_invalid_status() {
        let dto = ChangeUserStatusDto {
            user_id: TEST_ZERO_UUID.into(),
            status: "x".into(),
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn update_user_dto_rejects_empty_user_id() {
        let dto = update_user_dto_fixture!(user_id: "".into());
        assert!(dto.validate().is_err());
    }

    #[test]
    fn reset_pwd_dto_rejects_short_password() {
        let dto = ResetPwdDto {
            user_id: TEST_ZERO_UUID.into(),
            password: "short".into(),
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn auth_role_update_dto_rejects_empty_user_id() {
        let dto = AuthRoleUpdateDto {
            user_id: "".into(),
            role_ids: vec![],
        };
        assert!(dto.validate().is_err());
    }
}

//! Role DTOs — wire shapes matching NestJS for cross-backend compat.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysRole;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

/// Full role detail, including bound menu ids. Returned by
/// `GET /system/role/:id` and `POST /system/role/`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleDetailResponseDto {
    pub role_id: String,
    pub tenant_id: String,
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i32,
    pub data_scope: String,
    pub menu_check_strictly: bool,
    pub dept_check_strictly: bool,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
    /// Menu ids bound to this role, sorted by `menu_id` for deterministic
    /// serialization.
    pub menu_ids: Vec<String>,
}

impl RoleDetailResponseDto {
    /// Convert a `SysRole` row + its bound menu ids into the wire response
    /// shape. Timestamps are formatted via `fmt_ts`.
    pub fn from_entity(role: SysRole, menu_ids: Vec<String>) -> Self {
        Self {
            role_id: role.role_id,
            tenant_id: role.tenant_id,
            role_name: role.role_name,
            role_key: role.role_key,
            role_sort: role.role_sort,
            data_scope: role.data_scope,
            menu_check_strictly: role.menu_check_strictly,
            dept_check_strictly: role.dept_check_strictly,
            status: role.status,
            create_by: role.create_by,
            create_at: fmt_ts(&role.create_at),
            update_by: role.update_by,
            update_at: fmt_ts(&role.update_at),
            remark: role.remark,
            menu_ids,
        }
    }
}

/// Query string for `GET /system/role/list`. Extracted via
/// `ValidatedQuery` which runs validation before the handler sees it.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListRoleDto {
    pub role_name: Option<String>,
    pub role_key: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

/// Lightweight row shape for the role list page. Excludes menu bindings
/// and audit-only fields to keep payload size predictable.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleListItemResponseDto {
    pub role_id: String,
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i32,
    pub status: String,
    pub create_at: String,
    pub remark: Option<String>,
}

impl RoleListItemResponseDto {
    pub fn from_entity(role: SysRole) -> Self {
        Self {
            role_id: role.role_id,
            role_name: role.role_name,
            role_key: role.role_key,
            role_sort: role.role_sort,
            status: role.status,
            create_at: fmt_ts(&role.create_at),
            remark: role.remark,
        }
    }
}

/// Request body for `POST /system/role/`. Wire-compatible with the
/// NestJS `CreateRoleRequestDto`.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateRoleDto {
    #[validate(length(min = 1, max = 30))]
    pub role_name: String,
    #[validate(length(min = 1, max = 100))]
    pub role_key: String,
    #[validate(range(min = 0, max = 9999))]
    pub role_sort: i32,
    /// `"0"` = active, `"1"` = disabled. Defaults to active when the
    /// client omits the field.
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
    /// Menu ids to bind to the new role. Empty list means "no bindings"
    /// — the role will exist but have no menu permissions. Duplicate
    /// ids are NOT filtered (relies on caller correctness).
    #[serde(default)]
    pub menu_ids: Vec<String>,
}

/// Request body for `PUT /system/role/`. Wire-compatible with NestJS
/// `UpdateRoleRequestDto`. Note `role_id` is in the body (NestJS
/// convention for PUT) not the URL path.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRoleDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    #[validate(length(min = 1, max = 30))]
    pub role_name: String,
    #[validate(length(min = 1, max = 100))]
    pub role_key: String,
    #[validate(range(min = 0, max = 9999))]
    pub role_sort: i32,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
    #[serde(default)]
    pub menu_ids: Vec<String>,
}

/// Request body for `PUT /system/role/change-status`.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRoleStatusDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
}

/// Dropdown-optimized flat projection of `SysRole`. Returned by
/// `GET /system/role/option-select`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoleOptionResponseDto {
    pub role_id: String,
    pub role_name: String,
    pub role_key: String,
}

impl RoleOptionResponseDto {
    pub fn from_entity(role: SysRole) -> Self {
        Self {
            role_id: role.role_id,
            role_name: role.role_name,
            role_key: role.role_key,
        }
    }
}

/// Query string for `GET /system/role/auth-user/allocated-list` and
/// `GET /system/role/auth-user/unallocated-list`. Reuses `PageQuery` via
/// flatten so pagination validation matches `ListRoleDto`.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AuthUserListQueryDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    /// Optional user_name substring filter. Capped at 50 chars to match
    /// the `sys_user.user_name` column width — prevents pathological
    /// LIKE patterns dragging the DB through a huge sequential scan.
    #[validate(length(max = 50))]
    pub user_name: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

/// Wire response row for allocated/unallocated user list endpoints.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocatedUserResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
    pub email: String,
    pub phonenumber: String,
    pub status: String,
    pub create_at: String,
}

impl AllocatedUserResponseDto {
    pub fn from_row(r: crate::domain::role_repo::AllocatedUserRow) -> Self {
        Self {
            user_id: r.user_id,
            user_name: r.user_name,
            nick_name: r.nick_name,
            email: r.email,
            phonenumber: r.phonenumber,
            status: r.status,
            create_at: fmt_ts(&r.create_at),
        }
    }
}

/// Request body for `PUT /system/role/auth-user/select-all`.
/// Bulk-assigns `user_ids` to `role_id`. Idempotent — the backend
/// uses `ON CONFLICT DO NOTHING` so re-submissions are safe.
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AuthUserAssignDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    /// User ids to bind. Validated as 1..=1000 elements to prevent
    /// pathological batch sizes. Duplicates are tolerated (ON CONFLICT).
    #[validate(length(min = 1, max = 1000))]
    pub user_ids: Vec<String>,
}

/// Request body for `PUT /system/role/auth-user/cancel`.
/// Bulk-removes `user_ids` from `role_id`. Shape is identical to
/// `AuthUserAssignDto` but kept as a distinct type to match the
/// NestJS wire contract (separate DTOs per endpoint).
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct AuthUserCancelDto {
    #[validate(length(min = 1, max = 36))]
    pub role_id: String,
    #[validate(length(min = 1, max = 1000))]
    pub user_ids: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        AuthUserAssignDto, AuthUserCancelDto, AuthUserListQueryDto, ChangeRoleStatusDto,
        CreateRoleDto, ListRoleDto, UpdateRoleDto,
    };
    use framework::response::PageQuery;
    use validator::Validate;

    /// All-zeros UUID used as a filler in DTO validation tests where
    /// the `role_id` field needs to be syntactically valid but is not
    /// the subject of the assertion.
    const TEST_ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

    /// Fixture macro for `CreateRoleDto` — start with a valid default,
    /// override only the fields the test cares about.
    ///
    /// ```ignore
    /// let dto = create_dto_fixture!(role_name: "".into());
    /// ```
    macro_rules! create_dto_fixture {
        ($($field:ident : $value:expr),* $(,)?) => {{
            #[allow(unused_mut)]
            let mut dto = CreateRoleDto {
                role_name: "ok".into(),
                role_key: "some:key".into(),
                role_sort: 0,
                status: "0".into(),
                remark: None,
                menu_ids: vec![],
            };
            $( dto.$field = $value; )*
            dto
        }};
    }

    /// Fixture macro for `UpdateRoleDto` — same pattern as
    /// `create_dto_fixture!`, starts with a valid default using the
    /// all-zeros UUID for `role_id`.
    macro_rules! update_dto_fixture {
        ($($field:ident : $value:expr),* $(,)?) => {{
            #[allow(unused_mut)]
            let mut dto = UpdateRoleDto {
                role_id: TEST_ZERO_UUID.into(),
                role_name: "ok".into(),
                role_key: "k".into(),
                role_sort: 0,
                status: "0".into(),
                remark: None,
                menu_ids: vec![],
            };
            $( dto.$field = $value; )*
            dto
        }};
    }

    #[test]
    fn list_role_dto_rejects_page_size_over_limit() {
        let dto = ListRoleDto {
            role_name: None,
            role_key: None,
            status: None,
            page: PageQuery {
                page_num: 1,
                page_size: 500,
            },
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn list_role_dto_rejects_page_num_zero() {
        let dto = ListRoleDto {
            role_name: None,
            role_key: None,
            status: None,
            page: PageQuery {
                page_num: 0,
                page_size: 10,
            },
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn list_role_dto_accepts_valid_bounds() {
        let dto = ListRoleDto {
            role_name: Some("admin".into()),
            role_key: None,
            status: Some("0".into()),
            page: PageQuery::default(),
        };
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn create_role_dto_rejects_empty_role_name() {
        let dto = create_dto_fixture!(role_name: "".into());
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_role_dto_rejects_invalid_status_flag() {
        // Anything outside {"0", "1"} must be rejected — the DB column is
        // CHAR(1) with no CHECK constraint, so validation is the only gate.
        let dto = create_dto_fixture!(status: "2".into());
        assert!(dto.validate().is_err());
    }

    #[test]
    fn change_role_status_dto_rejects_invalid_status_flag() {
        let dto = ChangeRoleStatusDto {
            role_id: TEST_ZERO_UUID.into(),
            status: "x".into(),
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_role_dto_rejects_role_sort_negative() {
        let dto = create_dto_fixture!(role_sort: -1);
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_role_dto_accepts_valid_minimum() {
        let dto = create_dto_fixture!();
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn update_role_dto_rejects_empty_role_id() {
        let dto = update_dto_fixture!(role_id: "".into());
        assert!(dto.validate().is_err());
    }

    #[test]
    fn update_role_dto_accepts_valid_minimum() {
        let dto = update_dto_fixture!();
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn change_role_status_dto_rejects_empty_role_id() {
        let dto = ChangeRoleStatusDto {
            role_id: "".into(),
            status: "0".into(),
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn change_role_status_dto_rejects_empty_status() {
        let dto = ChangeRoleStatusDto {
            role_id: TEST_ZERO_UUID.into(),
            status: "".into(),
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn change_role_status_dto_accepts_valid() {
        let dto = ChangeRoleStatusDto {
            role_id: TEST_ZERO_UUID.into(),
            status: "0".into(),
        };
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn auth_user_list_query_rejects_empty_role_id() {
        let dto = AuthUserListQueryDto {
            role_id: "".into(),
            user_name: None,
            page: PageQuery::default(),
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn auth_user_list_query_accepts_valid() {
        let dto = AuthUserListQueryDto {
            role_id: TEST_ZERO_UUID.into(),
            user_name: Some("admin".into()),
            page: PageQuery::default(),
        };
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn auth_user_assign_dto_rejects_empty_user_ids() {
        let dto = AuthUserAssignDto {
            role_id: TEST_ZERO_UUID.into(),
            user_ids: vec![],
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn auth_user_assign_dto_accepts_valid() {
        let dto = AuthUserAssignDto {
            role_id: TEST_ZERO_UUID.into(),
            user_ids: vec!["user-1".into(), "user-2".into()],
        };
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn auth_user_cancel_dto_rejects_empty_user_ids() {
        let dto = AuthUserCancelDto {
            role_id: TEST_ZERO_UUID.into(),
            user_ids: vec![],
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn auth_user_cancel_dto_accepts_valid() {
        let dto = AuthUserCancelDto {
            role_id: TEST_ZERO_UUID.into(),
            user_ids: vec!["user-1".into()],
        };
        assert!(dto.validate().is_ok());
    }
}

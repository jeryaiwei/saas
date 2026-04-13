//! User HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::auth::Role;
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{operlog, require_authenticated, require_permission, require_role};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/system/user/{id}", tag = "用户管理",
    summary = "查询用户详情",
    params(("id" = String, Path, description = "user id")),
    responses((status = 200, body = ApiResponse<dto::UserDetailResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<ApiResponse<dto::UserDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &user_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/user/list", tag = "用户管理",
    summary = "用户列表",
    params(dto::ListUserDto),
    responses((status = 200, body = ApiResponse<Page<dto::UserListItemResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListUserDto>,
) -> Result<ApiResponse<Page<dto::UserListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/user/option-select", tag = "用户管理",
    summary = "用户下拉选项",
    params(dto::UserOptionQueryDto),
    responses((status = 200, body = ApiResponse<Vec<dto::UserOptionResponseDto>>))
)]
pub(crate) async fn option_select(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::UserOptionQueryDto>,
) -> Result<ApiResponse<Vec<dto::UserOptionResponseDto>>, AppError> {
    let resp = service::option_select(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/user/info", tag = "用户管理",
    summary = "获取当前用户详情",
    responses((status = 200, body = ApiResponse<dto::UserInfoResponseDto>))
)]
pub(crate) async fn info(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::UserInfoResponseDto>, AppError> {
    let resp = service::info(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/system/user/", tag = "用户管理",
    summary = "新增用户",
    request_body = dto::CreateUserDto,
    responses((status = 200, body = ApiResponse<dto::UserDetailResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateUserDto>,
) -> Result<ApiResponse<dto::UserDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/user/", tag = "用户管理",
    summary = "修改用户",
    request_body = dto::UpdateUserDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateUserDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(put, path = "/system/user/change-status", tag = "用户管理",
    summary = "修改用户状态",
    request_body = dto::ChangeUserStatusDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn change_status(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ChangeUserStatusDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::change_status(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/system/user/{id}", tag = "用户管理",
    summary = "删除用户",
    params(("id" = String, Path, description = "user ids, comma separated")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(put, path = "/system/user/reset-pwd", tag = "用户管理",
    summary = "重置密码",
    request_body = dto::ResetPwdDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn reset_password(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ResetPwdDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::reset_password(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/user/auth-role/{id}", tag = "用户管理",
    summary = "查询用户授权角色",
    params(("id" = String, Path, description = "user id")),
    responses((status = 200, body = ApiResponse<dto::AuthRoleResponseDto>))
)]
pub(crate) async fn auth_role(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<ApiResponse<dto::AuthRoleResponseDto>, AppError> {
    let resp = service::find_auth_role(&state, &user_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/user/auth-role", tag = "用户管理",
    summary = "修改用户授权角色",
    request_body = dto::AuthRoleUpdateDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update_auth_role(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::AuthRoleUpdateDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_auth_role(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/user/profile", tag = "用户管理",
    summary = "获取当前用户资料",
    responses((status = 200, body = ApiResponse<dto::UserProfileGetResponseDto>))
)]
pub(crate) async fn get_profile(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::UserProfileGetResponseDto>, AppError> {
    let resp = service::get_profile(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/user/profile", tag = "用户管理",
    summary = "修改当前用户资料",
    request_body = dto::UpdateProfileDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update_profile(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateProfileDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_profile(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(put, path = "/system/user/profile/update-pwd", tag = "用户管理",
    summary = "修改密码",
    request_body = dto::UpdatePwdDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update_pwd(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdatePwdDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_pwd(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/user/dept-tree", tag = "用户管理",
    summary = "部门树",
    responses((status = 200, body = ApiResponse<Vec<dto::DeptTreeNodeDto>>))
)]
pub(crate) async fn dept_tree(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::DeptTreeNodeDto>>, AppError> {
    let resp = service::dept_tree(&state).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:user:add"))
                .layer(operlog!("用户管理", Insert))
        }))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:user:edit"))
                .layer(operlog!("用户管理", Update))
        }))
        .routes(routes!(list).layer(require_permission!("system:user:list")))
        .routes(routes!(option_select).layer(require_authenticated!()))
        .routes(routes!(info).layer(require_authenticated!()))
        // Profile routes — must come before /{id} wildcard
        .routes(routes!(get_profile, update_profile).layer(require_authenticated!()))
        .routes(routes!(update_pwd).layer(require_authenticated!()))
        .routes(routes!(dept_tree).layer(require_authenticated!()))
        .routes(routes!(change_status).map(|r| {
            r.layer::<_, Infallible>(require_role!(Role::TenantAdmin))
                .layer(operlog!("用户管理", Update))
        }))
        .routes(routes!(reset_password).map(|r| {
            r.layer::<_, Infallible>(require_role!(Role::TenantAdmin))
                .layer(operlog!("用户管理", Update))
        }))
        .routes(routes!(update_auth_role).map(|r| {
            r.layer::<_, Infallible>(require_role!(Role::TenantAdmin))
                .layer(operlog!("用户管理", Grant))
        }))
        .routes(routes!(auth_role).layer(require_role!(Role::TenantAdmin)))
        .routes(routes!(find_by_id).layer(require_permission!("system:user:query")))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_role!(Role::TenantAdmin))
                .layer(operlog!("用户管理", Delete))
        }))
}

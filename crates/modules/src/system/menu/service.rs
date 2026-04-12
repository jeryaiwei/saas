//! Menu service — business orchestration.

use super::dto::{
    CreateMenuDto, ListMenuDto, MenuDetailResponseDto, MenuTreeSelectResponseDto, TreeNode,
    UpdateMenuDto,
};
use crate::domain::{
    MenuInsertParams, MenuListFilter, MenuRepo, MenuUpdateParams, RoleMenuTreeRow,
};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::ResponseCode;

use super::dto::list_to_tree;

/// Fetch a single menu by id. Returns `MENU_NOT_FOUND` when the menu
/// doesn't exist or has been soft-deleted.
#[tracing::instrument(skip_all, fields(menu_id = %menu_id))]
pub async fn find_by_id(
    state: &AppState,
    menu_id: &str,
) -> Result<MenuDetailResponseDto, AppError> {
    let menu = MenuRepo::find_by_id(&state.pg, menu_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MENU_NOT_FOUND)?;

    Ok(MenuDetailResponseDto::from_entity(menu))
}

/// Non-paginated list with optional filters.
#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListMenuDto,
) -> Result<Vec<MenuDetailResponseDto>, AppError> {
    let rows = MenuRepo::find_list(
        &state.pg,
        MenuListFilter {
            menu_name: query.menu_name,
            status: query.status,
            parent_id: query.parent_id,
            menu_type: query.menu_type,
        },
    )
    .await
    .into_internal()?;

    Ok(rows
        .into_iter()
        .map(MenuDetailResponseDto::from_entity)
        .collect())
}

/// Create a new menu. Returns the full detail DTO.
#[tracing::instrument(skip_all, fields(menu_name = %dto.menu_name))]
pub async fn create(
    state: &AppState,
    dto: CreateMenuDto,
) -> Result<MenuDetailResponseDto, AppError> {
    let menu = MenuRepo::insert(
        &state.pg,
        MenuInsertParams {
            menu_name: dto.menu_name,
            parent_id: dto.parent_id,
            order_num: dto.order_num,
            path: dto.path,
            component: dto.component,
            query: dto.query.unwrap_or_default(),
            is_frame: dto.is_frame,
            is_cache: dto.is_cache,
            menu_type: dto.menu_type,
            visible: dto.visible,
            status: dto.status,
            perms: dto.perms.unwrap_or_default(),
            icon: dto.icon.unwrap_or_default(),
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(MenuDetailResponseDto::from_entity(menu))
}

/// Update a menu. Verifies existence first, then updates.
/// Returns `MENU_NOT_FOUND` if not found or already deleted.
#[tracing::instrument(skip_all, fields(menu_id = %dto.menu_id))]
pub async fn update(state: &AppState, dto: UpdateMenuDto) -> Result<(), AppError> {
    // Verify menu exists.
    MenuRepo::find_by_id(&state.pg, &dto.menu_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MENU_NOT_FOUND)?;

    MenuRepo::update_by_id(
        &state.pg,
        MenuUpdateParams {
            menu_id: dto.menu_id,
            menu_name: dto.menu_name,
            parent_id: dto.parent_id,
            order_num: dto.order_num,
            path: dto.path,
            component: dto.component,
            query: dto.query,
            is_frame: dto.is_frame,
            is_cache: dto.is_cache,
            menu_type: dto.menu_type,
            visible: dto.visible,
            status: dto.status,
            perms: dto.perms,
            icon: dto.icon,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

/// Soft-delete a single menu (no cascade, no guards).
#[tracing::instrument(skip_all, fields(menu_id = %menu_id))]
pub async fn remove(state: &AppState, menu_id: &str) -> Result<(), AppError> {
    MenuRepo::soft_delete(&state.pg, menu_id)
        .await
        .into_internal()?;
    Ok(())
}

/// Cascade soft-delete a comma-separated list of menu ids and all their
/// descendants. Returns the total number of rows affected.
#[tracing::instrument(skip_all, fields(path_ids = %path_ids))]
pub async fn cascade_remove(state: &AppState, path_ids: &str) -> Result<u64, AppError> {
    let ids: Vec<String> = path_ids
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    let affected = MenuRepo::cascade_soft_delete(&state.pg, &ids)
        .await
        .into_internal()?;
    Ok(affected)
}

/// Return all menu nodes as a tree. Used for menu tree dropdowns.
#[tracing::instrument(skip_all)]
pub async fn tree_select(state: &AppState) -> Result<Vec<TreeNode>, AppError> {
    let rows = MenuRepo::find_tree_nodes(&state.pg).await.into_internal()?;
    Ok(list_to_tree(rows))
}

/// Return menu tree with checked state for a given role.
/// Branches on admin vs. tenant context.
#[tracing::instrument(skip_all, fields(role_id = %role_id))]
pub async fn role_menu_tree_select(
    state: &AppState,
    role_id: &str,
) -> Result<MenuTreeSelectResponseDto, AppError> {
    let is_admin =
        framework::context::RequestContext::with_current(|ctx| ctx.is_admin).unwrap_or(false);
    let tenant_id = framework::context::current_tenant_scope().unwrap_or_default();

    let rows: Vec<RoleMenuTreeRow> = if is_admin {
        MenuRepo::find_role_menu_tree_for_admin(&state.pg, role_id, &tenant_id)
            .await
            .into_internal()?
    } else {
        MenuRepo::find_role_menu_tree_for_tenant(&state.pg, role_id, &tenant_id)
            .await
            .into_internal()?
    };

    let checked_keys: Vec<String> = rows
        .iter()
        .filter(|r| r.is_checked)
        .map(|r| r.menu_id.clone())
        .collect();

    let tree_rows = rows
        .into_iter()
        .map(|r| crate::domain::MenuTreeRow {
            menu_id: r.menu_id,
            menu_name: r.menu_name,
            parent_id: r.parent_id,
        })
        .collect();

    let menus = list_to_tree(tree_rows);

    Ok(MenuTreeSelectResponseDto {
        menus,
        checked_keys,
    })
}

/// Return full menu tree with checked_keys for a tenant package.
#[tracing::instrument(skip_all, fields(package_id = %package_id))]
pub async fn package_menu_tree_select(
    state: &AppState,
    package_id: &str,
) -> Result<MenuTreeSelectResponseDto, AppError> {
    let tree_rows = MenuRepo::find_tree_nodes(&state.pg).await.into_internal()?;

    let checked_keys: Vec<String> = MenuRepo::find_package_menu_ids(&state.pg, package_id)
        .await
        .into_internal()?
        .unwrap_or_default();

    let menus = list_to_tree(tree_rows);

    Ok(MenuTreeSelectResponseDto {
        menus,
        checked_keys,
    })
}

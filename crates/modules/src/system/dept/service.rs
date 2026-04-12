//! Dept service — business orchestration.

use super::dto::{CreateDeptDto, DeptResponseDto, ListDeptDto, UpdateDeptDto};
use crate::domain::{DeptInsertParams, DeptListFilter, DeptRepo, DeptUpdateParams};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::ResponseCode;

/// Fetch a single dept by id. Returns `DEPT_NOT_FOUND` when missing.
#[tracing::instrument(skip_all, fields(dept_id = %dept_id))]
pub async fn find_by_id(state: &AppState, dept_id: &str) -> Result<DeptResponseDto, AppError> {
    let dept = DeptRepo::find_by_id(&state.pg, dept_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DEPT_NOT_FOUND)?;

    Ok(DeptResponseDto::from_entity(dept))
}

/// Non-paginated list with optional filters.
#[tracing::instrument(skip_all)]
pub async fn list(state: &AppState, query: ListDeptDto) -> Result<Vec<DeptResponseDto>, AppError> {
    let rows = DeptRepo::find_list(
        &state.pg,
        DeptListFilter {
            dept_name: query.dept_name,
            status: query.status,
        },
    )
    .await
    .into_internal()?;

    Ok(rows.into_iter().map(DeptResponseDto::from_entity).collect())
}

/// Create a new dept. Calculates `ancestors` from parent. Returns the full DTO.
#[tracing::instrument(skip_all, fields(dept_name = %dto.dept_name))]
pub async fn create(state: &AppState, dto: CreateDeptDto) -> Result<DeptResponseDto, AppError> {
    let ancestors = if dto.parent_id == "0" {
        vec!["0".to_string()]
    } else {
        let parent_ancestors = DeptRepo::find_parent_ancestors(&state.pg, &dto.parent_id)
            .await
            .into_internal()?
            .or_business(ResponseCode::DEPT_PARENT_NOT_FOUND)?;
        let mut a = parent_ancestors;
        a.push(dto.parent_id.clone());
        a
    };

    if ancestors.len() > 2000 {
        return Err(AppError::business(ResponseCode::DEPT_NESTING_TOO_DEEP));
    }

    let tenant_id = framework::context::current_tenant_scope()
        .context("create dept: tenant_id required")
        .into_internal()?;

    let dept = DeptRepo::insert(
        &state.pg,
        DeptInsertParams {
            tenant_id,
            parent_id: Some(dto.parent_id),
            ancestors,
            dept_name: dto.dept_name,
            order_num: dto.order_num,
            leader: dto.leader.unwrap_or_default(),
            phone: dto.phone.unwrap_or_default(),
            email: dto.email.unwrap_or_default(),
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(DeptResponseDto::from_entity(dept))
}

/// Update a dept. Recalculates ancestors when parent changes.
#[tracing::instrument(skip_all, fields(dept_id = %dto.dept_id))]
pub async fn update(state: &AppState, dto: UpdateDeptDto) -> Result<(), AppError> {
    // Verify existence first.
    let existing = DeptRepo::find_by_id(&state.pg, &dto.dept_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DEPT_NOT_FOUND)?;

    // Recalculate ancestors if parent changed.
    let new_ancestors: Option<Vec<String>> = {
        let existing_parent = existing.parent_id.unwrap_or_default();
        if dto.parent_id != existing_parent {
            let ancestors = if dto.parent_id == "0" {
                vec!["0".to_string()]
            } else {
                let parent_ancestors = DeptRepo::find_parent_ancestors(&state.pg, &dto.parent_id)
                    .await
                    .into_internal()?
                    .or_business(ResponseCode::DEPT_PARENT_NOT_FOUND)?;
                let mut a = parent_ancestors;
                a.push(dto.parent_id.clone());
                a
            };

            if ancestors.len() > 2000 {
                return Err(AppError::business(ResponseCode::DEPT_NESTING_TOO_DEEP));
            }
            Some(ancestors)
        } else {
            None
        }
    };

    let new_parent_id = Some(dto.parent_id);

    DeptRepo::update_by_id(
        &state.pg,
        DeptUpdateParams {
            dept_id: dto.dept_id,
            parent_id: new_parent_id,
            ancestors: new_ancestors,
            dept_name: dto.dept_name,
            order_num: dto.order_num,
            leader: dto.leader,
            phone: dto.phone,
            email: dto.email,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

/// Soft-delete a dept (no guards).
#[tracing::instrument(skip_all, fields(dept_id = %dept_id))]
pub async fn remove(state: &AppState, dept_id: &str) -> Result<(), AppError> {
    DeptRepo::soft_delete(&state.pg, dept_id)
        .await
        .into_internal()?;
    Ok(())
}

/// Active-only option list for dropdowns.
#[tracing::instrument(skip_all)]
pub async fn option_select(state: &AppState) -> Result<Vec<DeptResponseDto>, AppError> {
    let rows = DeptRepo::find_option_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows.into_iter().map(DeptResponseDto::from_entity).collect())
}

/// List excluding the given dept and all its descendants.
#[tracing::instrument(skip_all, fields(dept_id = %dept_id))]
pub async fn exclude_list(
    state: &AppState,
    dept_id: &str,
) -> Result<Vec<DeptResponseDto>, AppError> {
    let rows = DeptRepo::find_excluding(&state.pg, dept_id)
        .await
        .into_internal()?;
    Ok(rows.into_iter().map(DeptResponseDto::from_entity).collect())
}

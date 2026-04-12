//! Post service — business orchestration.

use super::dto::{CreatePostDto, ListPostDto, PostResponseDto, UpdatePostDto};
use crate::domain::{PostInsertParams, PostListFilter, PostRepo, PostUpdateParams};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(post_id = %post_id))]
pub async fn find_by_id(state: &AppState, post_id: &str) -> Result<PostResponseDto, AppError> {
    let post = PostRepo::find_by_id(&state.pg, post_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::POST_NOT_FOUND)?;
    Ok(PostResponseDto::from_entity(post))
}

#[tracing::instrument(skip_all)]
pub async fn list(state: &AppState, query: ListPostDto) -> Result<Page<PostResponseDto>, AppError> {
    let page = PostRepo::find_page(
        &state.pg,
        PostListFilter {
            post_name: query.post_name,
            post_code: query.post_code,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(PostResponseDto::from_entity))
}

#[tracing::instrument(skip_all, fields(post_code = %dto.post_code))]
pub async fn create(state: &AppState, dto: CreatePostDto) -> Result<PostResponseDto, AppError> {
    // Unique checks
    if PostRepo::exists_by_code(&state.pg, &dto.post_code, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::POST_CODE_EXISTS));
    }
    if PostRepo::exists_by_name(&state.pg, &dto.post_name, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::POST_NAME_EXISTS));
    }

    let post = PostRepo::insert(
        &state.pg,
        PostInsertParams {
            dept_id: dto.dept_id,
            post_code: dto.post_code,
            post_category: dto.post_category,
            post_name: dto.post_name,
            post_sort: dto.post_sort,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(PostResponseDto::from_entity(post))
}

#[tracing::instrument(skip_all, fields(post_id = %dto.post_id))]
pub async fn update(state: &AppState, dto: UpdatePostDto) -> Result<(), AppError> {
    // Verify existence
    PostRepo::find_by_id(&state.pg, &dto.post_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::POST_NOT_FOUND)?;

    // Unique checks when changing code/name
    if let Some(ref code) = dto.post_code {
        if PostRepo::exists_by_code(&state.pg, code, Some(&dto.post_id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(ResponseCode::POST_CODE_EXISTS));
        }
    }
    if let Some(ref name) = dto.post_name {
        if PostRepo::exists_by_name(&state.pg, name, Some(&dto.post_id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(ResponseCode::POST_NAME_EXISTS));
        }
    }

    PostRepo::update_by_id(
        &state.pg,
        PostUpdateParams {
            post_id: dto.post_id,
            dept_id: dto.dept_id,
            post_code: dto.post_code,
            post_category: dto.post_category,
            post_name: dto.post_name,
            post_sort: dto.post_sort,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

/// Batch soft-delete. IDs are comma-separated in the URL path.
#[tracing::instrument(skip_all, fields(ids = %ids_str))]
pub async fn remove(state: &AppState, ids_str: &str) -> Result<(), AppError> {
    let ids: Vec<&str> = ids_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }
    let mut tx = state
        .pg
        .begin()
        .await
        .context("post.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        PostRepo::soft_delete(&mut *tx, id).await.into_internal()?;
    }
    tx.commit()
        .await
        .context("post.remove: commit")
        .into_internal()?;
    Ok(())
}

#[tracing::instrument(skip_all)]
pub async fn option_select(state: &AppState) -> Result<Vec<PostResponseDto>, AppError> {
    let rows = PostRepo::find_option_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows.into_iter().map(PostResponseDto::from_entity).collect())
}

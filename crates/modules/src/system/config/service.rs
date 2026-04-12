//! Config service — business orchestration.

use super::dto::{
    ConfigResponseDto, CreateConfigDto, ListConfigDto, UpdateConfigByKeyDto, UpdateConfigDto,
};
use crate::domain::{ConfigInsertParams, ConfigListFilter, ConfigRepo, ConfigUpdateParams};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(config_id = %config_id))]
pub async fn find_by_id(state: &AppState, config_id: &str) -> Result<ConfigResponseDto, AppError> {
    let config = ConfigRepo::find_by_id(&state.pg, config_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::CONFIG_NOT_FOUND)?;
    Ok(ConfigResponseDto::from_entity(config))
}

#[tracing::instrument(skip_all, fields(config_key = %config_key))]
pub async fn find_by_key(
    state: &AppState,
    config_key: &str,
) -> Result<ConfigResponseDto, AppError> {
    let config = ConfigRepo::find_by_key(&state.pg, config_key)
        .await
        .into_internal()?
        .or_business(ResponseCode::CONFIG_NOT_FOUND)?;
    Ok(ConfigResponseDto::from_entity(config))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListConfigDto,
) -> Result<Page<ConfigResponseDto>, AppError> {
    let page = ConfigRepo::find_page(
        &state.pg,
        ConfigListFilter {
            config_name: query.config_name,
            config_key: query.config_key,
            config_type: query.config_type,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(ConfigResponseDto::from_entity))
}

#[tracing::instrument(skip_all, fields(config_key = %dto.config_key))]
pub async fn create(state: &AppState, dto: CreateConfigDto) -> Result<ConfigResponseDto, AppError> {
    if ConfigRepo::exists_by_key(&state.pg, &dto.config_key, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::CONFIG_KEY_EXISTS));
    }

    let config = ConfigRepo::insert(
        &state.pg,
        ConfigInsertParams {
            config_name: dto.config_name,
            config_key: dto.config_key,
            config_value: dto.config_value,
            config_type: dto.config_type,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(ConfigResponseDto::from_entity(config))
}

#[tracing::instrument(skip_all, fields(config_id = %dto.config_id))]
pub async fn update(state: &AppState, dto: UpdateConfigDto) -> Result<(), AppError> {
    ConfigRepo::find_by_id(&state.pg, &dto.config_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::CONFIG_NOT_FOUND)?;

    if let Some(ref key) = dto.config_key {
        if ConfigRepo::exists_by_key(&state.pg, key, Some(&dto.config_id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(ResponseCode::CONFIG_KEY_EXISTS));
        }
    }

    ConfigRepo::update_by_id(
        &state.pg,
        ConfigUpdateParams {
            config_id: dto.config_id,
            config_name: dto.config_name,
            config_key: dto.config_key,
            config_value: dto.config_value,
            config_type: dto.config_type,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

/// Update config value by key.
#[tracing::instrument(skip_all, fields(config_key = %dto.config_key))]
pub async fn update_by_key(state: &AppState, dto: UpdateConfigByKeyDto) -> Result<(), AppError> {
    ConfigRepo::find_by_key(&state.pg, &dto.config_key)
        .await
        .into_internal()?
        .or_business(ResponseCode::CONFIG_NOT_FOUND)?;

    ConfigRepo::update_value_by_key(&state.pg, &dto.config_key, &dto.config_value)
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
        .context("config.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        ConfigRepo::soft_delete(&mut *tx, id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("config.remove: commit")
        .into_internal()?;
    Ok(())
}

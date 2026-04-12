//! Dict service — business orchestration for dict type + dict data.

use super::dto::*;
use crate::domain::{
    DictDataInsertParams, DictDataListFilter, DictDataRepo, DictDataUpdateParams,
    DictTypeInsertParams, DictTypeListFilter, DictTypeRepo, DictTypeUpdateParams,
};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

// ===========================================================================
// DictType service
// ===========================================================================

#[tracing::instrument(skip_all, fields(dict_id = %dict_id))]
pub async fn find_type_by_id(
    state: &AppState,
    dict_id: &str,
) -> Result<DictTypeResponseDto, AppError> {
    let dt = DictTypeRepo::find_by_id(&state.pg, dict_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DICT_TYPE_NOT_FOUND)?;
    Ok(DictTypeResponseDto::from_entity(dt))
}

#[tracing::instrument(skip_all)]
pub async fn list_types(
    state: &AppState,
    query: ListDictTypeDto,
) -> Result<Page<DictTypeResponseDto>, AppError> {
    let page = DictTypeRepo::find_page(
        &state.pg,
        DictTypeListFilter {
            dict_name: query.dict_name,
            dict_type: query.dict_type,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(DictTypeResponseDto::from_entity))
}

#[tracing::instrument(skip_all)]
pub async fn type_option_select(state: &AppState) -> Result<Vec<DictTypeResponseDto>, AppError> {
    let rows = DictTypeRepo::find_option_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(DictTypeResponseDto::from_entity)
        .collect())
}

#[tracing::instrument(skip_all, fields(dict_type = %dto.dict_type))]
pub async fn create_type(
    state: &AppState,
    dto: CreateDictTypeDto,
) -> Result<DictTypeResponseDto, AppError> {
    if DictTypeRepo::exists_by_type(&state.pg, &dto.dict_type, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::DICT_TYPE_EXISTS));
    }

    let dt = DictTypeRepo::insert(
        &state.pg,
        DictTypeInsertParams {
            dict_name: dto.dict_name,
            dict_type: dto.dict_type,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(DictTypeResponseDto::from_entity(dt))
}

#[tracing::instrument(skip_all, fields(dict_id = %dto.dict_id))]
pub async fn update_type(state: &AppState, dto: UpdateDictTypeDto) -> Result<(), AppError> {
    DictTypeRepo::find_by_id(&state.pg, &dto.dict_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DICT_TYPE_NOT_FOUND)?;

    if let Some(ref dt) = dto.dict_type {
        if DictTypeRepo::exists_by_type(&state.pg, dt, Some(&dto.dict_id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(ResponseCode::DICT_TYPE_EXISTS));
        }
    }

    DictTypeRepo::update_by_id(
        &state.pg,
        DictTypeUpdateParams {
            dict_id: dto.dict_id,
            dict_name: dto.dict_name,
            dict_type: dto.dict_type,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(ids = %ids_str))]
pub async fn remove_types(state: &AppState, ids_str: &str) -> Result<(), AppError> {
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
        .context("dict.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        DictTypeRepo::soft_delete(&mut *tx, id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("dict.remove: commit")
        .into_internal()?;
    Ok(())
}

// ===========================================================================
// DictData service
// ===========================================================================

#[tracing::instrument(skip_all, fields(dict_code = %dict_code))]
pub async fn find_data_by_id(
    state: &AppState,
    dict_code: &str,
) -> Result<DictDataResponseDto, AppError> {
    let dd = DictDataRepo::find_by_id(&state.pg, dict_code)
        .await
        .into_internal()?
        .or_business(ResponseCode::DICT_DATA_NOT_FOUND)?;
    Ok(DictDataResponseDto::from_entity(dd))
}

/// Get all active dict data entries by dict_type name.
#[tracing::instrument(skip_all, fields(dict_type = %dict_type))]
pub async fn find_data_by_type(
    state: &AppState,
    dict_type: &str,
) -> Result<Vec<DictDataResponseDto>, AppError> {
    let rows = DictDataRepo::find_by_type(&state.pg, dict_type)
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(DictDataResponseDto::from_entity)
        .collect())
}

#[tracing::instrument(skip_all)]
pub async fn list_data(
    state: &AppState,
    query: ListDictDataDto,
) -> Result<Page<DictDataResponseDto>, AppError> {
    let page = DictDataRepo::find_page(
        &state.pg,
        DictDataListFilter {
            dict_type: query.dict_type,
            dict_label: query.dict_label,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(DictDataResponseDto::from_entity))
}

#[tracing::instrument(skip_all, fields(dict_type = %dto.dict_type, dict_value = %dto.dict_value))]
pub async fn create_data(
    state: &AppState,
    dto: CreateDictDataDto,
) -> Result<DictDataResponseDto, AppError> {
    if DictDataRepo::exists_by_type_value(&state.pg, &dto.dict_type, &dto.dict_value, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::DICT_DATA_EXISTS));
    }

    let dd = DictDataRepo::insert(
        &state.pg,
        DictDataInsertParams {
            dict_sort: dto.dict_sort,
            dict_label: dto.dict_label,
            dict_value: dto.dict_value,
            dict_type: dto.dict_type,
            css_class: dto.css_class,
            list_class: dto.list_class,
            is_default: dto.is_default,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(DictDataResponseDto::from_entity(dd))
}

#[tracing::instrument(skip_all, fields(dict_code = %dto.dict_code))]
pub async fn update_data(state: &AppState, dto: UpdateDictDataDto) -> Result<(), AppError> {
    let existing = DictDataRepo::find_by_id(&state.pg, &dto.dict_code)
        .await
        .into_internal()?
        .or_business(ResponseCode::DICT_DATA_NOT_FOUND)?;

    // Check uniqueness when dict_type or dict_value changes
    let check_type = dto.dict_type.as_deref().unwrap_or(&existing.dict_type);
    let check_value = dto.dict_value.as_deref().unwrap_or(&existing.dict_value);
    if (dto.dict_type.is_some() || dto.dict_value.is_some())
        && DictDataRepo::exists_by_type_value(
            &state.pg,
            check_type,
            check_value,
            Some(&dto.dict_code),
        )
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::DICT_DATA_EXISTS));
    }

    DictDataRepo::update_by_id(
        &state.pg,
        DictDataUpdateParams {
            dict_code: dto.dict_code,
            dict_sort: dto.dict_sort,
            dict_label: dto.dict_label,
            dict_value: dto.dict_value,
            dict_type: dto.dict_type,
            css_class: dto.css_class,
            list_class: dto.list_class,
            is_default: dto.is_default,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(ids = %ids_str))]
pub async fn remove_data(state: &AppState, ids_str: &str) -> Result<(), AppError> {
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
        .context("dict.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        DictDataRepo::soft_delete(&mut *tx, id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("dict.remove: commit")
        .into_internal()?;
    Ok(())
}

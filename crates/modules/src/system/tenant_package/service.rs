//! Tenant Package service — business orchestration.

use super::dto::{
    CreatePackageDto, ListPackageDto, PackageDetailResponseDto, PackageListItemResponseDto,
    PackageOptionResponseDto, UpdatePackageDto,
};
use crate::domain::{
    PackageInsertParams, PackageListFilter, PackageUpdateParams, TenantPackageRepo,
};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckBool, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

/// Fetch a single package by id. Returns `TENANT_PACKAGE_NOT_FOUND` when
/// the package doesn't exist or has been soft-deleted.
#[tracing::instrument(skip_all, fields(package_id = %package_id))]
pub async fn find_by_id(
    state: &AppState,
    package_id: &str,
) -> Result<PackageDetailResponseDto, AppError> {
    let pkg = TenantPackageRepo::find_by_id(&state.pg, package_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::TENANT_PACKAGE_NOT_FOUND)?;

    Ok(PackageDetailResponseDto::from_entity(pkg))
}

/// Paginated list with optional `package_name` and `status` filters.
#[tracing::instrument(skip_all, fields(
    page_num = query.page.page_num,
    page_size = query.page.page_size,
))]
pub async fn list(
    state: &AppState,
    query: ListPackageDto,
) -> Result<Page<PackageListItemResponseDto>, AppError> {
    let page = TenantPackageRepo::find_page(
        &state.pg,
        PackageListFilter {
            package_name: query.package_name,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(PackageListItemResponseDto::from_entity))
}

/// Return all active packages as flat dropdown options.
#[tracing::instrument(skip_all)]
pub async fn option_select(state: &AppState) -> Result<Vec<PackageOptionResponseDto>, AppError> {
    let rows = TenantPackageRepo::find_option_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(PackageOptionResponseDto::from_entity)
        .collect())
}

/// Create a new package. Validates that `code` and `package_name` are
/// unique before inserting. Returns the full detail DTO.
#[tracing::instrument(skip_all, fields(code = %dto.code, package_name = %dto.package_name))]
pub async fn create(
    state: &AppState,
    dto: CreatePackageDto,
) -> Result<PackageDetailResponseDto, AppError> {
    let code_ok = TenantPackageRepo::verify_code_unique(&state.pg, &dto.code, None)
        .await
        .into_internal()?;
    (!code_ok).business_err_if(ResponseCode::TENANT_PACKAGE_CODE_EXISTS)?;

    let name_ok = TenantPackageRepo::verify_name_unique(&state.pg, &dto.package_name, None)
        .await
        .into_internal()?;
    (!name_ok).business_err_if(ResponseCode::TENANT_PACKAGE_NAME_EXISTS)?;

    let pkg = TenantPackageRepo::insert(
        &state.pg,
        PackageInsertParams {
            code: dto.code,
            package_name: dto.package_name,
            menu_ids: dto.menu_ids,
            menu_check_strictly: dto.menu_check_strictly,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(PackageDetailResponseDto::from_entity(pkg))
}

/// Update a package. Validates existence; checks `code` and `package_name`
/// uniqueness only when those fields are being changed.
#[tracing::instrument(skip_all, fields(package_id = %dto.package_id))]
pub async fn update(state: &AppState, dto: UpdatePackageDto) -> Result<(), AppError> {
    // Verify package exists.
    TenantPackageRepo::find_by_id(&state.pg, &dto.package_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::TENANT_PACKAGE_NOT_FOUND)?;

    // Validate uniqueness for changed fields.
    if let Some(ref code) = dto.code {
        let code_ok = TenantPackageRepo::verify_code_unique(&state.pg, code, Some(&dto.package_id))
            .await
            .into_internal()?;
        (!code_ok).business_err_if(ResponseCode::TENANT_PACKAGE_CODE_EXISTS)?;
    }

    if let Some(ref name) = dto.package_name {
        let name_ok = TenantPackageRepo::verify_name_unique(&state.pg, name, Some(&dto.package_id))
            .await
            .into_internal()?;
        (!name_ok).business_err_if(ResponseCode::TENANT_PACKAGE_NAME_EXISTS)?;
    }

    let affected = TenantPackageRepo::update_by_id(
        &state.pg,
        PackageUpdateParams {
            package_id: dto.package_id,
            code: dto.code,
            package_name: dto.package_name,
            menu_ids: dto.menu_ids,
            menu_check_strictly: dto.menu_check_strictly,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    (affected == 0).business_err_if(ResponseCode::TENANT_PACKAGE_NOT_FOUND)
}

/// Soft-delete a comma-separated list of package ids. Guards against
/// deleting packages that are still referenced by active tenants.
#[tracing::instrument(skip_all, fields(path_ids = %path_ids))]
pub async fn remove(state: &AppState, path_ids: &str) -> Result<(), AppError> {
    let ids: Vec<String> = path_ids
        .split(',')
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .collect();

    let in_use = TenantPackageRepo::is_any_in_use(&state.pg, &ids)
        .await
        .into_internal()?;
    in_use.business_err_if(ResponseCode::TENANT_PACKAGE_IN_USE)?;

    TenantPackageRepo::soft_delete_by_ids(&state.pg, &ids)
        .await
        .into_internal()?;
    Ok(())
}

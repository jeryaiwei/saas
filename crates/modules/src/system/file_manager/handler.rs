//! File Manager HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::operlog;
use framework::response::{ApiResponse, Page};
use framework::{require_authenticated, require_permission};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

// ── Folder endpoints ────────────────────────────────────────────────────

#[utoipa::path(post, path = "/system/file-manager/folder", tag = "文件管理",
    summary = "新增文件夹",
    request_body = dto::CreateFolderDto,
    responses((status = 200, body = ApiResponse<dto::FolderResponseDto>))
)]
pub(crate) async fn create_folder(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateFolderDto>,
) -> Result<ApiResponse<dto::FolderResponseDto>, AppError> {
    let resp = service::create_folder(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/file-manager/folder", tag = "文件管理",
    summary = "修改文件夹",
    request_body = dto::UpdateFolderDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update_folder(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateFolderDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_folder(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/system/file-manager/folder/{folderId}", tag = "文件管理",
    summary = "删除文件夹",
    params(("folderId" = String, Path, description = "folder id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn delete_folder(
    State(state): State<AppState>,
    Path(folder_id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::delete_folder(&state, &folder_id).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/file-manager/folder/list", tag = "文件管理",
    summary = "文件夹列表",
    params(dto::ListFoldersDto),
    responses((status = 200, body = ApiResponse<Vec<dto::FolderResponseDto>>))
)]
pub(crate) async fn list_folders(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListFoldersDto>,
) -> Result<ApiResponse<Vec<dto::FolderResponseDto>>, AppError> {
    let resp = service::list_folders(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/file-manager/folder/tree", tag = "文件管理",
    summary = "文件夹树",
    responses((status = 200, body = ApiResponse<Vec<dto::FolderTreeNodeDto>>))
)]
pub(crate) async fn folder_tree(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::FolderTreeNodeDto>>, AppError> {
    let resp = service::folder_tree(&state).await?;
    Ok(ApiResponse::ok(resp))
}

// ── File endpoints ──────────────────────────────────────────────────────

#[utoipa::path(get, path = "/system/file-manager/file/list", tag = "文件管理",
    summary = "文件列表",
    params(dto::ListFilesDto),
    responses((status = 200, body = ApiResponse<Page<dto::FileResponseDto>>))
)]
pub(crate) async fn list_files(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListFilesDto>,
) -> Result<ApiResponse<Page<dto::FileResponseDto>>, AppError> {
    let page = service::list_files(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/system/file-manager/file/{uploadId}", tag = "文件管理",
    summary = "文件详情",
    params(("uploadId" = String, Path, description = "upload id")),
    responses((status = 200, body = ApiResponse<dto::FileResponseDto>))
)]
pub(crate) async fn file_detail(
    State(state): State<AppState>,
    Path(upload_id): Path<String>,
) -> Result<ApiResponse<dto::FileResponseDto>, AppError> {
    let resp = service::file_detail(&state, &upload_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/system/file-manager/file/move", tag = "文件管理",
    summary = "移动文件",
    request_body = dto::MoveFilesDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn move_files(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::MoveFilesDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::move_files(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(post, path = "/system/file-manager/file/rename", tag = "文件管理",
    summary = "重命名文件",
    request_body = dto::RenameFileDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn rename_file(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::RenameFileDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::rename_file(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/system/file-manager/file", tag = "文件管理",
    summary = "删除文件",
    request_body = dto::DeleteFilesDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn delete_files(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::DeleteFilesDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::delete_files(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/file-manager/file/{uploadId}/versions", tag = "文件管理",
    summary = "文件版本列表",
    params(("uploadId" = String, Path, description = "upload id")),
    responses((status = 200, body = ApiResponse<Vec<dto::FileVersionDto>>))
)]
pub(crate) async fn file_versions(
    State(state): State<AppState>,
    Path(upload_id): Path<String>,
) -> Result<ApiResponse<Vec<dto::FileVersionDto>>, AppError> {
    let resp = service::file_versions(&state, &upload_id).await?;
    Ok(ApiResponse::ok(resp))
}

// ── Share endpoints ─────────────────────────────────────────────────────

#[utoipa::path(post, path = "/system/file-manager/share", tag = "文件管理",
    summary = "创建分享",
    request_body = dto::CreateShareDto,
    responses((status = 200, body = ApiResponse<dto::ShareResponseDto>))
)]
pub(crate) async fn create_share(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateShareDto>,
) -> Result<ApiResponse<dto::ShareResponseDto>, AppError> {
    let resp = service::create_share(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/file-manager/share/{shareId}", tag = "文件管理",
    summary = "获取分享信息（公开）",
    params(("shareId" = String, Path, description = "share id")),
    responses((status = 200, body = ApiResponse<dto::SharePublicDto>))
)]
pub(crate) async fn get_share(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
) -> Result<ApiResponse<dto::SharePublicDto>, AppError> {
    let resp = service::get_share(&state, &share_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/file-manager/share/{shareId}", tag = "文件管理",
    summary = "取消分享",
    params(("shareId" = String, Path, description = "share id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn cancel_share(
    State(state): State<AppState>,
    Path(share_id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::cancel_share(&state, &share_id).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/file-manager/share/my/list", tag = "文件管理",
    summary = "我的分享列表",
    params(dto::MySharesDto),
    responses((status = 200, body = ApiResponse<Page<dto::ShareResponseDto>>))
)]
pub(crate) async fn my_shares(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::MySharesDto>,
) -> Result<ApiResponse<Page<dto::ShareResponseDto>>, AppError> {
    let page = service::my_shares(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

// ── Recycle endpoints ───────────────────────────────────────────────────

#[utoipa::path(get, path = "/system/file-manager/recycle/list", tag = "文件管理",
    summary = "回收站列表",
    params(dto::RecycleListDto),
    responses((status = 200, body = ApiResponse<Page<dto::RecycleFileDto>>))
)]
pub(crate) async fn recycle_list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::RecycleListDto>,
) -> Result<ApiResponse<Page<dto::RecycleFileDto>>, AppError> {
    let page = service::recycle_list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(put, path = "/system/file-manager/recycle/restore", tag = "文件管理",
    summary = "恢复文件",
    request_body = dto::RestoreFilesDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn restore_files(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::RestoreFilesDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::restore_files(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/system/file-manager/recycle/clear", tag = "文件管理",
    summary = "清空回收站",
    responses((status = 200, description = "success"))
)]
pub(crate) async fn clear_recycle(
    State(state): State<AppState>,
) -> Result<ApiResponse<()>, AppError> {
    service::clear_recycle(&state).await?;
    Ok(ApiResponse::success())
}

// ── Storage endpoint ────────────────────────────────────────────────────

#[utoipa::path(get, path = "/system/file-manager/storage/stats", tag = "文件管理",
    summary = "存储统计",
    responses((status = 200, body = ApiResponse<dto::StorageStatsDto>))
)]
pub(crate) async fn storage_stats(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::StorageStatsDto>, AppError> {
    let resp = service::storage_stats(&state).await?;
    Ok(ApiResponse::ok(resp))
}

// ── Router ──────────────────────────────────────────────────────────────

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // Folder
        .routes(routes!(create_folder).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file:add"))
                .layer(operlog!("文件管理", Insert))
        }))
        .routes(routes!(update_folder).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file:edit"))
                .layer(operlog!("文件管理", Update))
        }))
        .routes(routes!(delete_folder).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file:remove-folder"))
                .layer(operlog!("文件管理", Delete))
        }))
        .routes(routes!(list_folders).layer(require_permission!("system:file:list")))
        .routes(routes!(folder_tree).layer(require_permission!("system:file:folder-tree")))
        // File
        .routes(routes!(list_files).layer(require_permission!("system:file:list")))
        .routes(routes!(file_detail).layer(require_permission!("system:file:query")))
        .routes(routes!(move_files).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file:move"))
                .layer(operlog!("文件管理", Update))
        }))
        .routes(routes!(rename_file).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file:rename"))
                .layer(operlog!("文件管理", Update))
        }))
        .routes(routes!(delete_files).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file:remove"))
                .layer(operlog!("文件管理", Delete))
        }))
        .routes(routes!(file_versions).layer(require_permission!("system:file:version-list")))
        // Share
        .routes(routes!(create_share).map(|r| {
            r.layer::<_, Infallible>(require_authenticated!())
                .layer(operlog!("文件管理", Insert))
        }))
        .routes(routes!(get_share)) // NO AUTH — public endpoint
        .routes(routes!(cancel_share).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file:cancel-share"))
                .layer(operlog!("文件管理", Delete))
        }))
        .routes(routes!(my_shares).layer(require_permission!("system:file:share-list")))
        // Recycle
        .routes(routes!(recycle_list).layer(require_permission!("system:file-recycle:list")))
        .routes(routes!(restore_files).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file-recycle:restore"))
                .layer(operlog!("文件管理", Update))
        }))
        .routes(routes!(clear_recycle).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:file-recycle:remove"))
                .layer(operlog!("文件管理", Clean))
        }))
        // Storage
        .routes(routes!(storage_stats).layer(require_authenticated!()))
}

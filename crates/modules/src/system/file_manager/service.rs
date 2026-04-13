//! File Manager service — business orchestration.

use super::dto::{
    CreateFolderDto, CreateShareDto, DeleteFilesDto, FileResponseDto, FileVersionDto,
    FolderResponseDto, FolderTreeNodeDto, ListFilesDto, ListFoldersDto, MoveFilesDto, MySharesDto,
    RecycleFileDto, RecycleListDto, RenameFileDto, RestoreFilesDto, SharePublicDto,
    ShareResponseDto, StorageStatsDto, UpdateFolderDto,
};
use crate::domain::{
    FileFolderRepo, FileShareRepo, FolderInsertParams, FolderUpdateParams, RecycleListFilter,
    ShareInsertParams, UploadListFilter, UploadRepo, UploadUpdateParams,
};
use crate::state::AppState;
use anyhow::Context;
use framework::context::{current_tenant_scope, RequestContext};
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};
use std::collections::HashMap;

// ── Folder ──────────────────────────────────────────────────────────────

/// Create a new folder. Builds path from parent.
#[tracing::instrument(skip_all, fields(folder_name = %dto.folder_name))]
pub async fn create_folder(
    state: &AppState,
    dto: CreateFolderDto,
) -> Result<FolderResponseDto, AppError> {
    let tenant_id = current_tenant_scope()
        .context("create_folder: tenant_id required")
        .into_internal()?;

    // Build path from parent
    let folder_path = if let Some(ref parent_id) = dto.parent_id {
        let parent = FileFolderRepo::find_by_id(&state.pg, parent_id)
            .await
            .into_internal()?
            .or_business(ResponseCode::FOLDER_NOT_FOUND)?;
        format!("{}/{}", parent.folder_path, dto.folder_name)
    } else {
        format!("/{}", dto.folder_name)
    };

    let folder = FileFolderRepo::insert(
        &state.pg,
        FolderInsertParams {
            tenant_id,
            parent_id: dto.parent_id,
            folder_name: dto.folder_name,
            folder_path,
            order_num: 0,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(FolderResponseDto::from_entity(folder))
}

/// Update a folder.
#[tracing::instrument(skip_all, fields(folder_id = %dto.folder_id))]
pub async fn update_folder(state: &AppState, dto: UpdateFolderDto) -> Result<(), AppError> {
    FileFolderRepo::find_by_id(&state.pg, &dto.folder_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::FOLDER_NOT_FOUND)?;

    FileFolderRepo::update_by_id(
        &state.pg,
        FolderUpdateParams {
            folder_id: dto.folder_id,
            folder_name: dto.folder_name,
            order_num: dto.order_num,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

/// Delete a folder. Fails if it has sub-folders.
#[tracing::instrument(skip_all, fields(folder_id = %folder_id))]
pub async fn delete_folder(state: &AppState, folder_id: &str) -> Result<(), AppError> {
    FileFolderRepo::find_by_id(&state.pg, folder_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::FOLDER_NOT_FOUND)?;

    let has_children = FileFolderRepo::has_children(&state.pg, folder_id)
        .await
        .into_internal()?;
    if has_children {
        return Err(AppError::business(ResponseCode::FOLDER_HAS_SUBFOLDERS));
    }

    FileFolderRepo::soft_delete(&state.pg, folder_id)
        .await
        .into_internal()?;

    Ok(())
}

/// List folders by parent_id.
#[tracing::instrument(skip_all)]
pub async fn list_folders(
    state: &AppState,
    query: ListFoldersDto,
) -> Result<Vec<FolderResponseDto>, AppError> {
    let rows = FileFolderRepo::find_list(&state.pg, query.parent_id.as_deref())
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(FolderResponseDto::from_entity)
        .collect())
}

/// Build folder tree.
#[tracing::instrument(skip_all)]
pub async fn folder_tree(state: &AppState) -> Result<Vec<FolderTreeNodeDto>, AppError> {
    let rows = FileFolderRepo::find_tree(&state.pg).await.into_internal()?;

    // Build tree from flat list
    let mut nodes: Vec<FolderTreeNodeDto> = rows
        .into_iter()
        .map(|f| FolderTreeNodeDto {
            folder_id: f.folder_id,
            parent_id: f.parent_id,
            folder_name: f.folder_name,
            folder_path: f.folder_path,
            order_num: f.order_num,
            children: vec![],
        })
        .collect();

    // Index by id for children grouping
    let mut children_map: HashMap<String, Vec<FolderTreeNodeDto>> = HashMap::new();
    let mut roots: Vec<FolderTreeNodeDto> = Vec::new();

    // Collect children by parent_id
    // We need to reverse-iterate to build bottom-up
    while let Some(node) = nodes.pop() {
        match &node.parent_id {
            Some(pid) => {
                children_map.entry(pid.clone()).or_default().push(node);
            }
            None => {
                roots.push(node);
            }
        }
    }

    // Recursively attach children
    fn attach_children(
        node: &mut FolderTreeNodeDto,
        children_map: &mut HashMap<String, Vec<FolderTreeNodeDto>>,
    ) {
        if let Some(mut children) = children_map.remove(&node.folder_id) {
            for child in &mut children {
                attach_children(child, children_map);
            }
            children.sort_by_key(|c| c.order_num);
            node.children = children;
        }
    }

    for root in &mut roots {
        attach_children(root, &mut children_map);
    }

    // Also attach any remaining orphaned children to roots
    // that might be nested deeper
    roots.sort_by_key(|r| r.order_num);
    Ok(roots)
}

// ── File ────────────────────────────────────────────────────────────────

/// Paginated file list.
#[tracing::instrument(skip_all)]
pub async fn list_files(
    state: &AppState,
    query: ListFilesDto,
) -> Result<Page<FileResponseDto>, AppError> {
    let page = UploadRepo::find_page(
        &state.pg,
        UploadListFilter {
            folder_id: query.folder_id,
            file_name: query.file_name,
            ext: query.ext,
            status: None,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(FileResponseDto::from_entity))
}

/// Get file detail.
#[tracing::instrument(skip_all, fields(upload_id = %upload_id))]
pub async fn file_detail(state: &AppState, upload_id: &str) -> Result<FileResponseDto, AppError> {
    let upload = UploadRepo::find_by_id(&state.pg, upload_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::FILE_NOT_FOUND)?;

    Ok(FileResponseDto::from_entity(upload))
}

/// Move files to a target folder.
#[tracing::instrument(skip_all, fields(target = %dto.target_folder_id))]
pub async fn move_files(state: &AppState, dto: MoveFilesDto) -> Result<(), AppError> {
    // Verify target folder exists
    FileFolderRepo::find_by_id(&state.pg, &dto.target_folder_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::FOLDER_NOT_FOUND)?;

    UploadRepo::move_to_folder(&state.pg, &dto.ids, &dto.target_folder_id)
        .await
        .into_internal()?;

    Ok(())
}

/// Rename a file.
#[tracing::instrument(skip_all, fields(upload_id = %dto.upload_id))]
pub async fn rename_file(state: &AppState, dto: RenameFileDto) -> Result<(), AppError> {
    UploadRepo::find_by_id(&state.pg, &dto.upload_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::FILE_NOT_FOUND)?;

    UploadRepo::update_by_id(
        &state.pg,
        UploadUpdateParams {
            upload_id: dto.upload_id,
            file_name: Some(dto.file_name),
            folder_id: None,
            remark: None,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

/// Soft-delete files.
#[tracing::instrument(skip_all)]
pub async fn delete_files(state: &AppState, dto: DeleteFilesDto) -> Result<(), AppError> {
    UploadRepo::soft_delete(&state.pg, &dto.ids)
        .await
        .into_internal()?;
    Ok(())
}

/// Get file versions by parent_file_id.
#[tracing::instrument(skip_all, fields(upload_id = %upload_id))]
pub async fn file_versions(
    state: &AppState,
    upload_id: &str,
) -> Result<Vec<FileVersionDto>, AppError> {
    let rows = UploadRepo::find_versions(&state.pg, upload_id)
        .await
        .into_internal()?;
    Ok(rows.into_iter().map(FileVersionDto::from_entity).collect())
}

// ── Share ───────────────────────────────────────────────────────────────

/// Create a file share.
#[tracing::instrument(skip_all, fields(upload_id = %dto.upload_id))]
pub async fn create_share(
    state: &AppState,
    dto: CreateShareDto,
) -> Result<ShareResponseDto, AppError> {
    let tenant_id = current_tenant_scope()
        .context("create_share: tenant_id required")
        .into_internal()?;

    // Verify file exists
    UploadRepo::find_by_id(&state.pg, &dto.upload_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::FILE_NOT_FOUND)?;

    let share = FileShareRepo::insert(
        &state.pg,
        ShareInsertParams {
            tenant_id,
            upload_id: dto.upload_id,
            share_code: dto.share_code,
            expire_time: dto.expire_time,
            max_download: dto.max_download.unwrap_or(0),
        },
    )
    .await
    .into_internal()?;

    Ok(ShareResponseDto::from_entity(share))
}

/// Get share public info (no auth).
#[tracing::instrument(skip_all, fields(share_id = %share_id))]
pub async fn get_share(state: &AppState, share_id: &str) -> Result<SharePublicDto, AppError> {
    let share = FileShareRepo::find_by_id(&state.pg, share_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SHARE_NOT_FOUND)?;

    Ok(SharePublicDto::from_entity(share))
}

/// Cancel (delete) a share.
#[tracing::instrument(skip_all, fields(share_id = %share_id))]
pub async fn cancel_share(state: &AppState, share_id: &str) -> Result<(), AppError> {
    FileShareRepo::delete_by_id(&state.pg, share_id)
        .await
        .into_internal()?;
    Ok(())
}

/// List my shares (paginated).
#[tracing::instrument(skip_all)]
pub async fn my_shares(
    state: &AppState,
    query: MySharesDto,
) -> Result<Page<ShareResponseDto>, AppError> {
    let user_id = RequestContext::with_current(|c| c.user_id.clone())
        .flatten()
        .unwrap_or_default();

    let page = FileShareRepo::find_my_page(&state.pg, &user_id, query.page)
        .await
        .into_internal()?;

    Ok(page.map_rows(ShareResponseDto::from_entity))
}

// ── Recycle ─────────────────────────────────────────────────────────────

/// Paginated recycle bin list.
#[tracing::instrument(skip_all)]
pub async fn recycle_list(
    state: &AppState,
    query: RecycleListDto,
) -> Result<Page<RecycleFileDto>, AppError> {
    let page = UploadRepo::find_recycle_page(
        &state.pg,
        RecycleListFilter {
            file_name: query.file_name,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(RecycleFileDto::from_entity))
}

/// Restore files from recycle bin.
#[tracing::instrument(skip_all)]
pub async fn restore_files(state: &AppState, dto: RestoreFilesDto) -> Result<(), AppError> {
    UploadRepo::restore(&state.pg, &dto.ids)
        .await
        .into_internal()?;
    Ok(())
}

/// Clear recycle bin (hard delete all del_flag='1').
#[tracing::instrument(skip_all)]
pub async fn clear_recycle(state: &AppState) -> Result<(), AppError> {
    let tenant_id = current_tenant_scope()
        .context("clear_recycle: tenant_id required")
        .into_internal()?;

    UploadRepo::hard_delete(&state.pg, &tenant_id)
        .await
        .into_internal()?;
    Ok(())
}

// ── Storage ─────────────────────────────────────────────────────────────

/// Get storage usage statistics.
#[tracing::instrument(skip_all)]
pub async fn storage_stats(state: &AppState) -> Result<StorageStatsDto, AppError> {
    let tenant = current_tenant_scope();

    let file_stats: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(size::bigint), 0) \
         FROM sys_upload WHERE del_flag = '0' \
           AND ($1::varchar IS NULL OR tenant_id = $1)",
    )
    .bind(tenant.as_deref())
    .fetch_one(&state.pg)
    .await
    .context("storage_stats: file query")
    .into_internal()?;

    let folder_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sys_file_folder WHERE del_flag = '0' \
           AND ($1::varchar IS NULL OR tenant_id = $1)",
    )
    .bind(tenant.as_deref())
    .fetch_one(&state.pg)
    .await
    .context("storage_stats: folder query")
    .into_internal()?;

    Ok(StorageStatsDto {
        total_files: file_stats.0,
        total_size: file_stats.1,
        folder_count,
    })
}

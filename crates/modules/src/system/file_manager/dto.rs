//! File Manager DTOs — wire shapes for folder, file, share, recycle endpoints.

use crate::domain::{SysFileFolder, SysFileShare, SysUpload};
use chrono::{DateTime, Utc};
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// Folder detail (excludes tenant_id, del_flag).
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FolderResponseDto {
    pub folder_id: String,
    pub parent_id: Option<String>,
    pub folder_name: String,
    pub folder_path: String,
    pub order_num: i32,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl FolderResponseDto {
    pub fn from_entity(f: SysFileFolder) -> Self {
        Self {
            folder_id: f.folder_id,
            parent_id: f.parent_id,
            folder_name: f.folder_name,
            folder_path: f.folder_path,
            order_num: f.order_num,
            status: f.status,
            create_by: f.create_by,
            create_at: fmt_ts(&f.create_at),
            update_by: f.update_by,
            update_at: fmt_ts(&f.update_at),
            remark: f.remark,
        }
    }
}

/// Folder tree node with recursive children.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
#[schema(no_recursion)]
pub struct FolderTreeNodeDto {
    pub folder_id: String,
    pub parent_id: Option<String>,
    pub folder_name: String,
    pub folder_path: String,
    pub order_num: i32,
    pub children: Vec<FolderTreeNodeDto>,
}

/// File detail (excludes tenant_id, del_flag).
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileResponseDto {
    pub upload_id: String,
    pub folder_id: String,
    pub size: i32,
    pub file_name: String,
    pub new_file_name: String,
    pub url: String,
    pub ext: Option<String>,
    pub mime_type: Option<String>,
    pub storage_type: String,
    pub file_md5: Option<String>,
    pub thumbnail: Option<String>,
    pub parent_file_id: Option<String>,
    pub version: i32,
    pub is_latest: bool,
    pub download_count: i32,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl FileResponseDto {
    pub fn from_entity(u: SysUpload) -> Self {
        Self {
            upload_id: u.upload_id,
            folder_id: u.folder_id,
            size: u.size,
            file_name: u.file_name,
            new_file_name: u.new_file_name,
            url: u.url,
            ext: u.ext,
            mime_type: u.mime_type,
            storage_type: u.storage_type,
            file_md5: u.file_md5,
            thumbnail: u.thumbnail,
            parent_file_id: u.parent_file_id,
            version: u.version,
            is_latest: u.is_latest,
            download_count: u.download_count,
            status: u.status,
            create_by: u.create_by,
            create_at: fmt_ts(&u.create_at),
            update_by: u.update_by,
            update_at: fmt_ts(&u.update_at),
            remark: u.remark,
        }
    }
}

/// File version info.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct FileVersionDto {
    pub upload_id: String,
    pub file_name: String,
    pub size: i32,
    pub version: i32,
    pub is_latest: bool,
    pub create_by: String,
    pub create_at: String,
}

impl FileVersionDto {
    pub fn from_entity(u: SysUpload) -> Self {
        Self {
            upload_id: u.upload_id,
            file_name: u.file_name,
            size: u.size,
            version: u.version,
            is_latest: u.is_latest,
            create_by: u.create_by,
            create_at: fmt_ts(&u.create_at),
        }
    }
}

/// Share detail (authenticated view).
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ShareResponseDto {
    pub share_id: String,
    pub upload_id: String,
    pub share_code: Option<String>,
    pub expire_time: Option<String>,
    pub max_download: i32,
    pub download_count: i32,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
}

impl ShareResponseDto {
    pub fn from_entity(s: SysFileShare) -> Self {
        Self {
            share_id: s.share_id,
            upload_id: s.upload_id,
            share_code: s.share_code,
            expire_time: s.expire_time.as_ref().map(fmt_ts),
            max_download: s.max_download,
            download_count: s.download_count,
            status: s.status,
            create_by: s.create_by,
            create_at: fmt_ts(&s.create_at),
        }
    }
}

/// Share public view (limited fields for unauthenticated access).
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SharePublicDto {
    pub share_id: String,
    pub upload_id: String,
    pub expire_time: Option<String>,
    pub max_download: i32,
    pub download_count: i32,
    pub status: String,
}

impl SharePublicDto {
    pub fn from_entity(s: SysFileShare) -> Self {
        Self {
            share_id: s.share_id,
            upload_id: s.upload_id,
            expire_time: s.expire_time.as_ref().map(fmt_ts),
            max_download: s.max_download,
            download_count: s.download_count,
            status: s.status,
        }
    }
}

/// Recycle bin file info.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecycleFileDto {
    pub upload_id: String,
    pub file_name: String,
    pub size: i32,
    pub ext: Option<String>,
    pub url: String,
    pub create_by: String,
    pub create_at: String,
    pub update_at: String,
}

impl RecycleFileDto {
    pub fn from_entity(u: SysUpload) -> Self {
        Self {
            upload_id: u.upload_id,
            file_name: u.file_name,
            size: u.size,
            ext: u.ext,
            url: u.url,
            create_by: u.create_by,
            create_at: fmt_ts(&u.create_at),
            update_at: fmt_ts(&u.update_at),
        }
    }
}

/// Storage usage stats.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StorageStatsDto {
    pub total_files: i64,
    pub total_size: i64,
    pub folder_count: i64,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Request body for `POST /system/file-manager/folder`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateFolderDto {
    #[validate(length(min = 1, max = 100))]
    pub folder_name: String,
    pub parent_id: Option<String>,
    pub remark: Option<String>,
}

/// Request body for `PUT /system/file-manager/folder`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFolderDto {
    pub folder_id: String,
    #[validate(length(min = 1, max = 100))]
    pub folder_name: Option<String>,
    #[validate(range(min = 0))]
    pub order_num: Option<i32>,
    pub remark: Option<String>,
}

/// Query string for `GET /system/file-manager/file/list`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListFilesDto {
    pub folder_id: Option<String>,
    pub file_name: Option<String>,
    pub ext: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

/// Request body for `POST /system/file-manager/file/move`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveFilesDto {
    #[validate(length(min = 1))]
    pub ids: Vec<String>,
    pub target_folder_id: String,
}

/// Request body for `POST /system/file-manager/file/rename`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RenameFileDto {
    pub upload_id: String,
    #[validate(length(min = 1, max = 255))]
    pub file_name: String,
}

/// Request body for `DELETE /system/file-manager/file`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteFilesDto {
    #[validate(length(min = 1))]
    pub ids: Vec<String>,
}

/// Request body for `POST /system/file-manager/share`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateShareDto {
    pub upload_id: String,
    pub share_code: Option<String>,
    pub expire_time: Option<DateTime<Utc>>,
    pub max_download: Option<i32>,
}

/// Request body for `PUT /system/file-manager/recycle/restore`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestoreFilesDto {
    #[validate(length(min = 1))]
    pub ids: Vec<String>,
}

/// Query string for `GET /system/file-manager/recycle/list`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct RecycleListDto {
    pub file_name: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

/// Query string for `GET /system/file-manager/folder/list`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListFoldersDto {
    pub parent_id: Option<String>,
}

/// Query string for share list pagination.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct MySharesDto {
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

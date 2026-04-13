//! DTOs for file upload / download endpoints.

use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Upload response
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UploadResponseDto {
    pub upload_id: String,
    pub url: String,
    pub file_name: String,
    pub file_md5: String,
    pub storage_type: String,
    pub instant_upload: bool,
}

// ---------------------------------------------------------------------------
// Client direct upload — authorization request/response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientUploadAuthDto {
    #[validate(length(min = 1, max = 255))]
    pub file_name: String,
    pub size: u64,
    #[validate(length(min = 1))]
    pub mime_type: String,
    pub folder_id: Option<String>,
    /// Optional MD5 for instant-upload check before generating signed URL.
    pub file_md5: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientUploadAuthResponseDto {
    pub upload_token: String,
    pub signed_url: String,
    pub key: String,
    pub url: String,
    pub storage_type: String,
    pub expires: u64,
    /// If instant upload hit, no need to upload — file already exists.
    pub instant_upload: bool,
    /// Only set when instant_upload = true.
    pub upload_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Client direct upload — callback request/response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientUploadCallbackDto {
    #[validate(length(min = 1))]
    pub upload_token: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ClientUploadCallbackResponseDto {
    pub upload_id: String,
    pub url: String,
    pub file_name: String,
}

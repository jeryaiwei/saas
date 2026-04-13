//! DTOs for file upload / download endpoints.

use serde::Serialize;

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

//! Upload / download service.

use crate::domain::{UploadInsertParams, UploadRepo};
use crate::state::AppState;
use framework::context::current_tenant_scope;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::ResponseCode;
use md5::{Digest, Md5};

use super::dto::UploadResponseDto;

/// Format a digest result as lowercase hex string.
fn hex_digest(hash: impl AsRef<[u8]>) -> String {
    hash.as_ref()
        .iter()
        .fold(String::with_capacity(32), |mut s, b| {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", b);
            s
        })
}

/// Strip path separators, null bytes, and `..` from a user-supplied filename.
fn sanitize_filename(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or("unknown")
        .replace('\0', "")
        .chars()
        .take(200)
        .collect()
}

#[tracing::instrument(skip_all, fields(file_name = %file_name, folder_id))]
pub async fn upload(
    state: &AppState,
    file_name: String,
    content_type: String,
    data: Vec<u8>,
    folder_id: Option<String>,
) -> Result<UploadResponseDto, AppError> {
    let tenant_id = current_tenant_scope()
        .ok_or_else(|| AppError::business(ResponseCode::UNAUTHORIZED))?;
    let config = &state.config.upload;

    // Sanitize filename
    let file_name = sanitize_filename(&file_name);

    // 1. Validate file size
    let size_bytes = data.len() as u64;
    let max_bytes = config.max_file_size_mb * 1024 * 1024;
    if size_bytes > max_bytes {
        let size_mb = size_bytes / (1024 * 1024);
        return Err(AppError::business_with_msg(
            ResponseCode::FILE_SIZE_EXCEEDED,
            format!(
                "File size {}MB exceeds limit {}MB",
                size_mb, config.max_file_size_mb
            ),
        ));
    }

    // 2. Compute MD5
    let file_md5 = hex_digest(Md5::digest(&data));

    // 3. Detect MIME type (before storage, so provider can use it)
    let mime = if content_type.is_empty() || content_type == "application/octet-stream" {
        mime_guess::from_path(&file_name)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_string()
    } else {
        content_type
    };

    // 4. Check instant upload (秒传)
    if let Some(existing) = UploadRepo::find_by_md5(&state.pg, &tenant_id, &file_md5)
        .await
        .into_internal()?
    {
        let new_id = uuid::Uuid::new_v4().to_string();
        UploadRepo::insert(
            &state.pg,
            UploadInsertParams {
                upload_id: new_id.clone(),
                tenant_id: tenant_id.clone(),
                folder_id: folder_id.unwrap_or_default(),
                size: existing.size,
                file_name: file_name.clone(),
                new_file_name: existing.new_file_name.clone(),
                url: existing.url.clone(),
                ext: existing.ext.clone(),
                mime_type: existing.mime_type.clone(),
                storage_type: existing.storage_type.clone(),
                file_md5: Some(file_md5.clone()),
                thumbnail: existing.thumbnail.clone(),
                parent_file_id: None,
                version: 1,
                is_latest: true,
                remark: None,
            },
        )
        .await
        .into_internal()?;

        tracing::info!(upload_id = %new_id, %file_md5, "instant upload hit");
        return Ok(UploadResponseDto {
            upload_id: new_id,
            url: existing.url,
            file_name,
            file_md5,
            storage_type: existing.storage_type,
            instant_upload: true,
        });
    }

    // 5. Generate storage key
    let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();
    let new_file_name = format!("{}_{}", uuid::Uuid::new_v4(), file_name);
    let date_path = chrono::Utc::now().format("%Y/%m/%d").to_string();
    let key = format!("{}/{}/{}", tenant_id, date_path, new_file_name);

    // 6. Store file
    let url = state
        .storage
        .put(&key, &data, &mime)
        .await
        .map_err(|e| AppError::business_with_msg(ResponseCode::FILE_UPLOAD_FAIL, e))?;

    // 7. Insert DB record
    let upload_id = uuid::Uuid::new_v4().to_string();
    UploadRepo::insert(
        &state.pg,
        UploadInsertParams {
            upload_id: upload_id.clone(),
            tenant_id: tenant_id.clone(),
            folder_id: folder_id.unwrap_or_default(),
            size: data.len() as i32,
            file_name: file_name.clone(),
            new_file_name,
            url: url.clone(),
            ext: if ext.is_empty() { None } else { Some(ext) },
            mime_type: Some(mime),
            storage_type: config.storage_type.clone(),
            file_md5: Some(file_md5.clone()),
            thumbnail: None,
            parent_file_id: None,
            version: 1,
            is_latest: true,
            remark: None,
        },
    )
    .await
    .into_internal()?;

    tracing::info!(%upload_id, %file_md5, size = data.len(), "file uploaded");
    Ok(UploadResponseDto {
        upload_id,
        url,
        file_name,
        file_md5,
        storage_type: config.storage_type.clone(),
        instant_upload: false,
    })
}

#[tracing::instrument(skip_all, fields(upload_id = %upload_id))]
pub async fn download(
    state: &AppState,
    upload_id: &str,
) -> Result<(String, String, Vec<u8>), AppError> {
    // 1. Find file record
    let file = UploadRepo::find_by_id(&state.pg, upload_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::FILE_NOT_FOUND)?;

    // 2. Increment download count
    if let Err(e) = UploadRepo::increment_download_count(&state.pg, upload_id).await {
        tracing::warn!(error = %e, "failed to increment download count");
    }

    // 3. Read file from storage
    let data = state
        .storage
        .get(&file.url)
        .await
        .map_err(|e| AppError::business_with_msg(ResponseCode::FILE_NOT_FOUND, e))?;

    let mime = file
        .mime_type
        .unwrap_or_else(|| "application/octet-stream".to_string());
    Ok((file.file_name, mime, data))
}

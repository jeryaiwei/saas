//! Upload / download service.

use crate::domain::{UploadInsertParams, UploadRepo, SysUpload};
use crate::state::AppState;
use framework::config::UploadConfig;
use framework::context::current_tenant_scope;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::infra::redis::RedisExt;
use framework::response::ResponseCode;
use md5::{Digest, Md5};

use serde::{Deserialize, Serialize};

use super::dto::{
    ClientUploadAuthDto, ClientUploadAuthResponseDto, ClientUploadCallbackDto,
    ClientUploadCallbackResponseDto, UploadResponseDto,
};

/// Upload registration stored in Redis (strong-typed).
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UploadRegistration {
    tenant_id: String,
    key: String,
    url: String,
    file_name: String,
    size: u64,
    mime_type: String,
    folder_id: String,
    file_md5: Option<String>,
    storage_type: String,
    status: String, // "pending" | "uploaded"
}

// ─── Shared helpers ─────────────────────────────────────────────────────────

/// Parsed and sanitized file name with pre-extracted extension.
struct SafeFileName {
    /// Sanitized full filename (e.g. "photo.jpg")
    name: String,
    /// Lowercase extension without dot (e.g. "jpg"), empty if none
    ext: String,
}

impl SafeFileName {
    /// Sanitize a user-supplied filename: strip path, null bytes, limit length, extract ext.
    fn parse(raw: &str) -> Self {
        let name: String = raw
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("unknown")
            .replace('\0', "")
            .chars()
            .take(200)
            .collect();
        let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
        // Avoid treating "Makefile" (no dot) as ext="makefile"
        let ext = if name.contains('.') { ext } else { String::new() };
        Self { name, ext }
    }

    /// For DB: `Some("jpg")` or `None` if empty.
    fn ext_option(&self) -> Option<String> {
        if self.ext.is_empty() { None } else { Some(self.ext.clone()) }
    }
}

/// Validate file size + MIME whitelist + extension blacklist.
fn validate_file(
    config: &UploadConfig,
    file: &SafeFileName,
    mime: &str,
    size: u64,
) -> Result<(), AppError> {
    // Size
    let max_bytes = config.max_file_size_mb * 1024 * 1024;
    if size > max_bytes {
        return Err(AppError::business_with_msg(
            ResponseCode::FILE_SIZE_EXCEEDED,
            format!("File size {}MB exceeds limit {}MB", size / (1024 * 1024), config.max_file_size_mb),
        ));
    }
    // MIME whitelist
    if !config.allowed_types.is_empty() && !config.allowed_types.iter().any(|t| t == mime) {
        return Err(AppError::business_with_msg(
            ResponseCode::FILE_TYPE_NOT_ALLOWED,
            format!("MIME type '{}' is not allowed", mime),
        ));
    }
    // Extension blacklist
    if !file.ext.is_empty() && config.blocked_extensions.iter().any(|e| e.eq_ignore_ascii_case(&file.ext)) {
        return Err(AppError::business_with_msg(
            ResponseCode::FILE_TYPE_NOT_ALLOWED,
            format!("Extension '.{}' is blocked", file.ext),
        ));
    }
    Ok(())
}

/// Generate a storage key: `{tenant_id}/{YYYY/MM/DD}/{uuid}_{filename}`.
fn build_storage_key(tenant_id: &str, file_name: &str) -> (String, String) {
    let new_file_name = format!("{}_{}", uuid::Uuid::new_v4(), file_name);
    let date_path = chrono::Utc::now().format("%Y/%m/%d").to_string();
    let key = format!("{}/{}/{}", tenant_id, date_path, new_file_name);
    (key, new_file_name)
}

/// Create a copy DB record for instant upload (秒传).
async fn insert_instant_copy(
    state: &AppState,
    existing: &SysUpload,
    tenant_id: &str,
    file_name: &str,
    folder_id: Option<String>,
    file_md5: &str,
) -> Result<String, AppError> {
    let new_id = uuid::Uuid::new_v4().to_string();
    UploadRepo::insert(
        &state.pg,
        UploadInsertParams {
            upload_id: new_id.clone(),
            tenant_id: tenant_id.to_string(),
            folder_id: folder_id.unwrap_or_default(),
            size: existing.size,
            file_name: file_name.to_string(),
            new_file_name: existing.new_file_name.clone(),
            url: existing.url.clone(),
            ext: existing.ext.clone(),
            mime_type: existing.mime_type.clone(),
            storage_type: existing.storage_type.clone(),
            file_md5: Some(file_md5.to_string()),
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
    Ok(new_id)
}

// ─── Public API ─────────────────────────────────────────────────────────────

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
    let file = SafeFileName::parse(&file_name);

    // 1. Detect MIME
    let mime = if content_type.is_empty() || content_type == "application/octet-stream" {
        mime_guess::from_path(&file.name)
            .first_raw()
            .unwrap_or("application/octet-stream")
            .to_string()
    } else {
        content_type
    };

    // 2. Validate
    validate_file(config, &file, &mime, data.len() as u64)?;

    // 3. MD5 + instant upload
    let file_md5 = hex::encode(Md5::digest(&data));
    if let Some(existing) = UploadRepo::find_by_md5(&state.pg, &tenant_id, &file_md5)
        .await
        .into_internal()?
    {
        let new_id = insert_instant_copy(state, &existing, &tenant_id, &file.name, folder_id, &file_md5).await?;
        return Ok(UploadResponseDto {
            upload_id: new_id,
            url: existing.url,
            file_name: file.name,
            file_md5,
            storage_type: existing.storage_type,
            instant_upload: true,
        });
    }

    // 4. Store file
    let (key, _) = build_storage_key(&tenant_id, &file.name);
    let result = state
        .storage
        .put(&key, &data, &mime)
        .await
        .map_err(|e| AppError::business_with_msg(ResponseCode::FILE_UPLOAD_FAIL, e))?;

    // 5. Insert DB record
    let upload_id = uuid::Uuid::new_v4().to_string();
    UploadRepo::insert(
        &state.pg,
        UploadInsertParams {
            upload_id: upload_id.clone(),
            tenant_id: tenant_id.clone(),
            folder_id: folder_id.unwrap_or_default(),
            size: data.len() as i32,
            file_name: file.name.clone(),
            new_file_name: result.key,
            url: result.url.clone(),
            ext: file.ext_option(),
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
        url: result.url,
        file_name: file.name,
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
    let file = UploadRepo::find_by_id(&state.pg, upload_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::FILE_NOT_FOUND)?;

    if let Err(e) = UploadRepo::increment_download_count(&state.pg, upload_id).await {
        tracing::warn!(error = %e, "failed to increment download count");
    }

    let data = state
        .storage
        .get(&file.new_file_name)
        .await
        .map_err(|e| AppError::business_with_msg(ResponseCode::FILE_NOT_FOUND, e))?;

    let mime = file
        .mime_type
        .unwrap_or_else(|| "application/octet-stream".to_string());
    Ok((file.file_name, mime, data))
}

#[tracing::instrument(skip_all, fields(file_name = %dto.file_name))]
pub async fn client_authorize(
    state: &AppState,
    dto: ClientUploadAuthDto,
) -> Result<ClientUploadAuthResponseDto, AppError> {
    let tenant_id = current_tenant_scope()
        .ok_or_else(|| AppError::business(ResponseCode::UNAUTHORIZED))?;
    let config = &state.config.upload;
    let file = SafeFileName::parse(&dto.file_name);

    // 1. Validate
    validate_file(config, &file, &dto.mime_type, dto.size)?;

    // 2. Instant upload if MD5 provided
    if let Some(ref md5) = dto.file_md5 {
        if let Some(existing) = UploadRepo::find_by_md5(&state.pg, &tenant_id, md5)
            .await
            .into_internal()?
        {
            let new_id = insert_instant_copy(
                state, &existing, &tenant_id, &file.name, dto.folder_id.clone(), md5,
            )
            .await?;
            return Ok(ClientUploadAuthResponseDto {
                upload_token: String::new(),
                signed_url: String::new(),
                key: String::new(),
                url: existing.url,
                storage_type: existing.storage_type,
                expires: 0,
                instant_upload: true,
                upload_id: Some(new_id),
            });
        }
    }

    // 3. Generate storage key + signed URL
    let (key, _) = build_storage_key(&tenant_id, &file.name);
    let expires_secs = 600u64;
    let signed = state
        .storage
        .signed_put_url(&key, &dto.mime_type, expires_secs)
        .ok_or_else(|| AppError::business(ResponseCode::CLIENT_UPLOAD_NOT_SUPPORTED))?;

    // 4. Save registration to Redis
    let upload_token = uuid::Uuid::new_v4().to_string();
    let registration = UploadRegistration {
        tenant_id: tenant_id.clone(),
        key: signed.key.clone(),
        url: signed.url.clone(),
        file_name: file.name.clone(),
        size: dto.size,
        mime_type: dto.mime_type.clone(),
        folder_id: dto.folder_id.clone().unwrap_or_default(),
        file_md5: dto.file_md5.clone(),
        storage_type: config.storage_type.clone(),
        status: "pending".into(),
    };
    let redis_key = format!("upload_registration:{}", upload_token);
    state
        .redis
        .set_ex(&redis_key, &registration, expires_secs)
        .await
        .into_internal()?;

    tracing::info!(%upload_token, key = %signed.key, "client upload authorized");
    Ok(ClientUploadAuthResponseDto {
        upload_token,
        signed_url: signed.signed_url,
        key: signed.key,
        url: signed.url,
        storage_type: config.storage_type.clone(),
        expires: expires_secs,
        instant_upload: false,
        upload_id: None,
    })
}

#[tracing::instrument(skip_all, fields(upload_token = %dto.upload_token))]
pub async fn client_callback(
    state: &AppState,
    dto: ClientUploadCallbackDto,
) -> Result<ClientUploadCallbackResponseDto, AppError> {
    // 1. Reject empty token
    if dto.upload_token.is_empty() {
        return Err(AppError::business_with_msg(
            ResponseCode::UPLOAD_TOKEN_INVALID,
            "Empty token — instant upload does not require callback",
        ));
    }

    // 2. Get registration from Redis
    let redis_key = format!("upload_registration:{}", dto.upload_token);
    let reg: UploadRegistration = state
        .redis
        .get_json(&redis_key)
        .await
        .into_internal()?
        .ok_or_else(|| AppError::business(ResponseCode::UPLOAD_TOKEN_INVALID))?;

    // 3. Check not already used
    if reg.status == "uploaded" {
        return Err(AppError::business(ResponseCode::UPLOAD_TOKEN_USED));
    }

    // 4. Verify tenant consistency
    if let Some(ref tid) = current_tenant_scope() {
        if tid != &reg.tenant_id {
            return Err(AppError::business(ResponseCode::UPLOAD_TOKEN_INVALID));
        }
    }

    // 5. Verify file exists on storage
    if reg.key.is_empty() {
        return Err(AppError::business(ResponseCode::UPLOAD_TOKEN_INVALID));
    }
    let file_exists = state
        .storage
        .exists(&reg.key)
        .await
        .map_err(|e| AppError::business_with_msg(ResponseCode::FILE_NOT_FOUND, e))?;
    if !file_exists {
        return Err(AppError::business_with_msg(
            ResponseCode::FILE_NOT_FOUND,
            "File not found on storage, upload may have failed",
        ));
    }

    // 6. Create DB record
    let file = SafeFileName::parse(&reg.file_name);
    let upload_id = uuid::Uuid::new_v4().to_string();

    UploadRepo::insert(
        &state.pg,
        UploadInsertParams {
            upload_id: upload_id.clone(),
            tenant_id: reg.tenant_id,
            folder_id: reg.folder_id,
            size: reg.size as i32,
            file_name: reg.file_name.clone(),
            new_file_name: reg.key,
            url: reg.url.clone(),
            ext: file.ext_option(),
            mime_type: Some(reg.mime_type),
            storage_type: reg.storage_type,
            file_md5: reg.file_md5,
            thumbnail: None,
            parent_file_id: None,
            version: 1,
            is_latest: true,
            remark: None,
        },
    )
    .await
    .into_internal()?;

    // 7. Mark token as used (1 min TTL)
    state
        .redis
        .set_ex_raw(&redis_key, r#"{"status":"uploaded"}"#, 60)
        .await
        .into_internal()?;

    tracing::info!(%upload_id, file_name = %reg.file_name, "client upload callback completed");
    Ok(ClientUploadCallbackResponseDto {
        upload_id,
        url: reg.url,
        file_name: reg.file_name,
    })
}

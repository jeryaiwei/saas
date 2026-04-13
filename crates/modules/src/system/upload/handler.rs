//! Upload / download HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::http::header;
use axum::response::IntoResponse;
use framework::error::AppError;
use framework::extractors::ValidatedJson;
use framework::response::{ApiResponse, ResponseCode};
use framework::{operlog, require_authenticated};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/common/upload", tag = "文件上传",
    summary = "上传文件",
    responses((status = 200, body = ApiResponse<dto::UploadResponseDto>))
)]
pub(crate) async fn upload(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<ApiResponse<dto::UploadResponseDto>, AppError> {
    let mut file_data: Option<(String, String, Vec<u8>)> = None;
    let mut folder_id: Option<String> = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                let file_name = field.file_name().unwrap_or("unknown").to_string();
                let content_type = field
                    .content_type()
                    .unwrap_or("application/octet-stream")
                    .to_string();
                let bytes = field.bytes().await.map_err(|e| {
                    AppError::business_with_msg(
                        ResponseCode::PARAM_INVALID,
                        format!("read file: {e}"),
                    )
                })?;
                file_data = Some((file_name, content_type, bytes.to_vec()));
            }
            "folderId" => {
                let text = field.text().await.unwrap_or_default();
                if !text.is_empty() {
                    folder_id = Some(text);
                }
            }
            _ => {}
        }
    }

    let (file_name, content_type, data) = file_data.ok_or_else(|| {
        AppError::business_with_msg(ResponseCode::PARAM_INVALID, "Missing 'file' field")
    })?;

    let resp = service::upload(&state, file_name, content_type, data, folder_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/common/upload/{uploadId}", tag = "文件上传",
    summary = "下载文件",
    params(("uploadId" = String, Path, description = "upload id")),
    responses((status = 200, description = "file binary"))
)]
pub(crate) async fn download(
    State(state): State<AppState>,
    Path(upload_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let (file_name, mime, data) = service::download(&state, &upload_id).await?;

    // Encode filename for Content-Disposition (RFC 5987)
    let encoded_name = urlencoding::encode(&file_name);
    let disposition = format!(
        "attachment; filename=\"{}\"; filename*=UTF-8''{}",
        file_name.replace('"', ""),
        encoded_name
    );

    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        data,
    ))
}

#[utoipa::path(post, path = "/common/upload/client/authorization", tag = "文件上传",
    summary = "客户端直传授权",
    request_body = dto::ClientUploadAuthDto,
    responses((status = 200, body = ApiResponse<dto::ClientUploadAuthResponseDto>))
)]
pub(crate) async fn client_authorize(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ClientUploadAuthDto>,
) -> Result<ApiResponse<dto::ClientUploadAuthResponseDto>, AppError> {
    let resp = service::client_authorize(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/common/upload/client/callback", tag = "文件上传",
    summary = "客户端直传回调",
    request_body = dto::ClientUploadCallbackDto,
    responses((status = 200, body = ApiResponse<dto::ClientUploadCallbackResponseDto>))
)]
pub(crate) async fn client_callback(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::ClientUploadCallbackDto>,
) -> Result<ApiResponse<dto::ClientUploadCallbackResponseDto>, AppError> {
    let resp = service::client_callback(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(upload).map(|r| {
            r.layer(operlog!("文件上传", Other))
        }))
        // Body limit: 100MB + 1MB overhead for multipart framing
        .layer(DefaultBodyLimit::max(101 * 1024 * 1024))
        .routes(routes!(download).layer(require_authenticated!()))
        .routes(routes!(client_authorize).layer(require_authenticated!()))
        .routes(routes!(client_callback).layer(require_authenticated!()))
}

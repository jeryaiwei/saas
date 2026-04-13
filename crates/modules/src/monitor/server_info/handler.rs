//! Server info HTTP handler + router wiring.

use super::{dto, service};
use crate::state::AppState;
use framework::error::AppError;
use framework::require_permission;
use framework::response::ApiResponse;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/monitor/server", tag = "服务器监控",
    summary = "服务器监控信息",
    responses((status = 200, body = ApiResponse<dto::ServerInfoDto>))
)]
pub(crate) async fn server_info() -> Result<ApiResponse<dto::ServerInfoDto>, AppError> {
    let resp = service::get_server_info().await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(server_info).layer(require_permission!("monitor:server:query")))
}

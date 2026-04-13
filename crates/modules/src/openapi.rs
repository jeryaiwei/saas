//! OpenAPI global metadata — info, tags, security scheme.
//!
//! Per-module paths are collected by `OpenApiRouter::merge` in `lib.rs`.
//! This file only defines the shell struct with global metadata.

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "SaaS Rust API",
        version = "1.0.0",
        description = "SaaS Rust backend API documentation"
    ),
    servers((url = "/api/v1", description = "API v1")),
    tags(
        (name = "认证", description = "登录、验证码、用户信息"),
        (name = "配置管理", description = "系统参数配置"),
        (name = "部门管理", description = "部门树形结构"),
        (name = "字典管理", description = "字典类型与字典数据"),
        (name = "菜单管理", description = "菜单树形结构与权限标识"),
        (name = "岗位管理", description = "岗位/职位管理"),
        (name = "角色管理", description = "角色与权限分配"),
        (name = "租户管理", description = "租户 CRUD"),
        (name = "套餐管理", description = "租户套餐与菜单范围"),
        (name = "用户管理", description = "用户 CRUD 与授权"),
        (name = "审计日志", description = "租户审计日志"),
        (name = "站内信模板", description = "站内信模板管理"),
        (name = "站内信消息", description = "站内信消息管理"),
        (name = "邮箱账号", description = "邮箱账号管理"),
        (name = "邮件模板", description = "邮件模板管理"),
        (name = "邮件日志", description = "邮件日志管理"),
        (name = "短信渠道", description = "短信渠道管理"),
        (name = "短信模板", description = "短信模板管理"),
        (name = "短信日志", description = "短信日志管理"),
        (name = "在线用户", description = "在线用户监控"),
        (name = "服务器监控", description = "服务器系统信息"),
    ),
    security(("bearer_auth" = [])),
    modifiers(&SecurityAddon),
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "Enter JWT token from POST /api/v1/auth/login → data.access_token",
                    ))
                    .build(),
            ),
        );
    }
}

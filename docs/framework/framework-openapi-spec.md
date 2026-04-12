# OpenAPI / Swagger 框架规范 v1.0

**生效日期**：2026-04-12
**状态**：Normative（规范性）

---

## 1. 技术栈

| crate | 版本 | 职责 |
| --- | --- | --- |
| `utoipa` | 5.x | `#[derive(ToSchema)]` / `#[utoipa::path]` / `#[derive(OpenApi)]` |
| `utoipa-axum` | 0.2 | `OpenApiRouter` / `routes!` 宏 / `split_for_parts()` |
| `utoipa-swagger-ui` | 9.x | `/swagger-ui` 静态资源 + `/api-docs/openapi.json` |

---

## 2. 架构

```text
handler.rs
  ├── #[utoipa::path(method, path, tag, summary, params, responses)]
  │     → 生成 __path_<fn> struct (path/method/operation/tags/schemas)
  ├── pub(crate) async fn handler(...)
  └── pub fn router() -> OpenApiRouter<AppState>
        └── .routes(routes!(handler).layer(require_permission!(...)))
              → routes! 从 __path_<fn> 读取 path/method
              → .layer() 挂权限中间件

lib.rs
  └── api_openapi_router() -> OpenApiRouter
        └── .merge(auth::router()).merge(system::xxx::router())...
        └── split_for_parts() → (Router, OpenApi)

openapi.rs
  └── ApiDoc (全局 info / tags / security / servers)

main.rs
  └── SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", openapi)
```

**核心原则**：path 只在 `#[utoipa::path]` 写一次，`router()` 不重复 path 字符串。

---

## 3. DTO 规范

### 3.1 Response DTO

**必须** derive `Serialize` + `utoipa::ToSchema`：

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostResponseDto {
    pub post_id: String,
    pub post_name: String,
    // ...
}
```

### 3.2 Request DTO (JSON body)

**必须** derive `Deserialize` + `Validate` + `utoipa::ToSchema`：

```rust
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreatePostDto {
    #[validate(length(min = 1, max = 64))]
    pub post_code: String,
    // ...
}
```

### 3.3 Query DTO (URL 参数)

**必须** 额外 derive `utoipa::IntoParams`：

```rust
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListPostDto {
    pub post_name: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
```

### 3.4 递归类型

自引用 struct（如 `TreeNode`）**必须** 加 `#[schema(no_recursion)]`，否则 OpenAPI spec 生成时 stack overflow：

```rust
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(no_recursion)]
pub struct TreeNode {
    pub id: String,
    pub label: String,
    pub children: Vec<TreeNode>,
}
```

---

## 4. Handler 注解规范

### 4.1 `#[utoipa::path]` 必填字段

| 字段 | 必填 | 说明 |
| --- | --- | --- |
| method | 是 | `get` / `post` / `put` / `delete` |
| `path` | 是 | 完整路由路径（不含 `/api/v1` 前缀） |
| `tag` | 是 | 中文模块名（与 openapi.rs tags 一致） |
| `summary` | 是 | 中文端点描述 |
| `request_body` | POST/PUT 有 body 时 | `dto::CreateXxxDto` |
| `params` | GET 有 query 时 | `dto::ListXxxDto` 或 `("id" = String, Path, ...)` |
| `responses` | 是 | 见下文 |

### 4.2 responses 写法

```rust
// 有返回数据
responses((status = 200, body = ApiResponse<dto::XxxResponseDto>))
responses((status = 200, body = ApiResponse<Page<dto::XxxResponseDto>>))
responses((status = 200, body = ApiResponse<Vec<dto::XxxResponseDto>>))

// 无返回数据（create/update/delete 成功）— 不能写 ApiResponse<()>
responses((status = 200, description = "success"))
```

**禁止** `body = ApiResponse<()>` — utoipa 无法解析 `()` 作为 schema，会导致编译错误。

### 4.3 handler 可见性

**必须** 为 `pub(crate)`，否则 `routes!` 宏生成的 `__path_<fn>` 无法从 `openapi.rs` 访问：

```rust
pub(crate) async fn list(...) { ... }
```

### 4.4 完整示例

```rust
#[utoipa::path(get, path = "/system/post/list", tag = "岗位管理",
    summary = "岗位列表",
    params(dto::ListPostDto),
    responses((status = 200, body = ApiResponse<Page<dto::PostResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListPostDto>,
) -> Result<ApiResponse<Page<dto::PostResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}
```

---

## 5. Router 规范

### 5.1 返回类型

**必须** 返回 `OpenApiRouter<AppState>`（非 `axum::Router`）：

```rust
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;
use framework::{operlog, require_permission};
use std::convert::Infallible;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:post:add"))
                .layer(operlog!("岗位管理", Insert))
        }))
        .routes(routes!(list).layer(require_permission!("system:post:list")))
        .routes(routes!(option_select).layer(require_authenticated!()))
        .routes(routes!(find_by_id).layer(require_permission!("system:post:query")))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:post:remove"))
                .layer(operlog!("岗位管理", Delete))
        }))
}
```

### 5.2 路由注册规则

- 每个 handler 一行 `.routes(routes!(fn_name)...)`
- `routes!` 从 `#[utoipa::path]` 自动读取 path 和 HTTP method
- 同一 path 不同 method（如 GET `/{id}` + DELETE `/{id}`）**分开注册**
- 只读路由：`.layer(require_permission!(...))` 直接链
- 写路由：`.map(|r| r.layer::<_, Infallible>(...).layer(operlog!(...)))` 链式挂 permission + operlog
- `Infallible` turbofish 用于解决 Tower layer 类型推断歧义

### 5.3 权限 4 种模式

```rust
// 1. RBAC 权限（64 个端点）
.routes(routes!(list).layer(require_permission!("system:xxx:list")))

// 2. 仅需登录（18 个端点）
.routes(routes!(option_select).layer(require_authenticated!()))

// 3. 角色限制（6 个端点）
.routes(routes!(change_status).layer(require_role!(Role::TenantAdmin)))

// 4. 组合门控（4 个端点）
.routes(routes!(create).layer(require_access! {
    permission: "system:tenant:add",
    role: Role::PlatformAdmin,
}))
```

---

## 6. 模块注册

### 6.1 lib.rs

新增模块在 `api_openapi_router()` 加一行 `.merge()`：

```rust
fn api_openapi_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .merge(auth::router())
        .merge(system::xxx::router())   // ← 加一行
        // ...
}
```

### 6.2 openapi.rs

新增**模块组**时在 tags 列表加一行（同一模块组的端点共享 tag）：

```rust
tags(
    (name = "新模块", description = "新模块描述"),
)
```

---

## 7. 全局配置

### 7.1 Server 前缀

```rust
servers((url = "/api/v1", description = "API v1")),
```

Swagger UI 发送请求自动带 `/api/v1` 前缀。

### 7.2 安全方案

```rust
security(("bearer_auth" = [])),
```

通过 `SecurityAddon` modifier 注册 Bearer JWT scheme。Swagger UI 右上角 "Authorize" 按钮输入 token。

### 7.3 Auth 白名单

`/swagger-ui` 和 `/api-docs` 在 `main.rs` 的 `default_whitelist()` 中豁免 JWT 检查。

---

## 8. 中文标签映射

| tag | 模块 |
| --- | --- |
| 认证 | auth |
| 配置管理 | config |
| 部门管理 | dept |
| 字典管理 | dict |
| 菜单管理 | menu |
| 岗位管理 | post |
| 角色管理 | role |
| 租户管理 | tenant |
| 套餐管理 | tenant_package |
| 用户管理 | user |
| 通知公告 | notice |
| 操作日志 | oper_log |
| 登录日志 | login_log |

---

## 9. 产出指标

| 指标 | 值 |
| --- | --- |
| /swagger-ui | 200 |
| /api-docs/openapi.json | 自动生成 |
| paths | 66 (90 handlers, 部分共享 path) |
| schemas | 96 |
| securitySchemes | bearer_auth (JWT) |
| 中文 summary | 90 个端点全覆盖 |
| operlog 覆盖 | 43 个写路由 |

---

## 10. 延期项

| 项目 | 触发条件 |
| --- | --- |
| `#[api]` proc macro 统一注解 | 端点 > 150 或团队 > 2 人 |
| OpenAPI 导出文件 (CI) | 前端需要 codegen |
| Response schema 分离 (success/error) | Swagger 需要展示错误码 |

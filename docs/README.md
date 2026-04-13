# SaaS Rust Backend - 项目文档

## 项目状态

| 指标     | 值                                                                                                                                                     |
| -------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 端点数   | 188 (system 84 + message 55 + monitor 13 + auth 6 + health 9)                                                                                          |
| 模块数   | 30 (system 12 + message 11 + monitor 5 + auth 1 + health 1)                                                                                            |
| 测试数   | 326                                                                                                                                                    |
| Smoke    | 12 scripts, 119 steps (role 14 + user 16 + tenant 13 + menu 10 + dept 8 + post 8 + config 9 + dict 11 + notice 7 + notify 12 + operlog 6 + loginlog 5) |
| 框架规范 | 8 份 (pagination / error-envelope / observability / repo-executor / pagination-indexes / openapi / operlog / tenant)                                   |
| 业务设计 | 6 份 (role / user / tenant / menu / dept / mail-sms-send)                                                                                              |
| Swagger  | /swagger-ui (Bearer JWT, 中文 tag/summary)                                                                                                             |
| Operlog  | 72 写路由自动记录操作日志                                                                                                                              |

---

## 文档索引

### 框架规范 (`docs/framework/`)

定义了所有模块必须遵守的跨切面契约。新增业务模块前应读完这些规范。

| 文档                                                              | 版本        | 内容                                                                                                                          |
| ----------------------------------------------------------------- | ----------- | ----------------------------------------------------------------------------------------------------------------------------- |
| [pagination-spec](framework/framework-pagination-spec.md)         | v1.0 + v1.1 | 分页类型 (PageQuery / PaginationParams / Page), filter struct 规范, into_page, with_timeout, reconcile_total, slow-query warn |
| [pagination-indexes](framework/framework-pagination-indexes.md)   | v1.0        | 每个 find_page 的索引依赖表                                                                                                   |
| [error-envelope-spec](framework/framework-error-envelope-spec.md) | v1.0        | 统一 wire envelope (ApiResponse), AppError 5 variant, ResponseCode 段位, i18n 命名空间, FieldError 路径格式                   |
| [observability-spec](framework/framework-observability-spec.md)   | v1.0 已落地 | root span 自动注入 request_id/tenant_id/user_id, middleware instrument, login event, span 字段命名, metric 命名约定           |
| [repo-executor-spec](framework/framework-repo-executor-spec.md)   | v1.0        | impl PgExecutor vs &PgPool vs &mut Transaction 选择规则, service 层事务边界, 禁止模式                                         |
| [openapi-spec](framework/framework-openapi-spec.md)               | v1.0        | utoipa + OpenApiRouter 架构, DTO derive 规范, handler 注解, router 注册, 权限模式, 中文 tag                                   |
| [operlog-spec](framework/framework-operlog-spec.md)               | v1.1        | 操作日志路由级设计 (OperlogLayer + Extension&lt;PgPool&gt;), BusinessType, 65 写路由覆盖                                      |
| [tenant-spec](framework/framework-tenant-spec.md)                 | v1.0        | 三层租户架构, 套餐绑定, 管理员层级, 权限计算, 租户切换, Session 结构, 数据过滤模型                                            |

### 其他

| 文档                                         | 内容                                   |
| -------------------------------------------- | -------------------------------------- |
| [schema-reference.sql](schema-reference.sql) | 29 张表的 DDL (从线上 pg_indexes 导出) |
| [archive/](archive/)                         | 历史 RESUME handoff 文档               |

---

## Crate 结构

```text
server-rs/
├── crates/
│   ├── framework/     跨切面基础设施 (config, context, error, extractors,
│   │                  i18n, infra, middleware, response, telemetry, testing)
│   ├── modules/       业务模块
│   │                  system/ (config,dept,dict,menu,post,role,tenant,tenant_package,user)
│   │                  system/ + (audit_log, file_manager, tenant_dashboard)
│   │                  message/ (notice,notify_template,notify_message,mail_account,
│   │                           mail_template,mail_log,mail_send,sms_channel,
│   │                           sms_template,sms_log,sms_send)
│   │                  monitor/ (oper_log,login_log,online_user,server_info,cache)
│   │                  + auth/ + health/ + domain/ (28 repos) + openapi.rs
│   └── app/           二进制入口 (main.rs, middleware 组装, swagger-ui, CORS)
├── config/            YAML 配置 (default + development)
├── i18n/              国际化 (zh-CN.json + en-US.json)
├── scripts/           Smoke 测试脚本 (5 个)
└── docs/              本目录
```

---

## Handler 开发规范

### 新增端点标准流程

1. **dto.rs** — 加 Request/Response DTO，derive `ToSchema` + `IntoParams`(query DTO)
2. **repo.rs** — 加 domain 层 SQL 方法 (`impl PgExecutor<'_>` 单查询 / `&PgPool` 分页 / `&mut Transaction` 多写)
3. **service.rs** — 加 business 逻辑 (`#[tracing::instrument(skip_all)]`)
4. **handler.rs** — 加 handler 函数：
   ```rust
   #[utoipa::path(get, path = "/system/xxx/list", tag = "模块名",
       summary = "中文描述",
       params(dto::ListXxxDto),
       responses((status = 200, body = ApiResponse<Page<dto::XxxResponseDto>>))
   )]
   pub(crate) async fn list(...) { ... }
   ```
5. **handler.rs router()** — 加一行路由注册：
   ```rust
   .routes(routes!(list).layer(require_permission!("system:xxx:list")))
   ```
6. **lib.rs** — 在 `api_openapi_router()` 加 `.merge(system::xxx::router())`
7. **openapi.rs** — 在 tags 列表加 tag (可选，新模块组才需要)

### 路由与 OpenAPI 架构

```text
#[utoipa::path] → 定义 path + method + tag + summary + params + responses
       ↓
routes!(handler) → 从 __path_<fn> 读取 path/method，注册 axum handler
       ↓
.layer(require_permission!("xxx")) → 挂权限中间件
       ↓
OpenApiRouter::merge() → 同时收集 axum route + OpenAPI spec
       ↓
split_for_parts() → (Router, OpenApi)
```

**核心原则**：path 只在 `#[utoipa::path]` 写一次，`router()` 不再重复 path 字符串。

### 权限声明 4 种模式

| 模式      | 宏                                                 | 场景       | 数量 |
| --------- | -------------------------------------------------- | ---------- | ---- |
| RBAC 权限 | `require_permission!("system:xxx:action")`         | 标准端点   | 71   |
| 仅需登录  | `require_authenticated!()`                         | 下拉选项等 | 18   |
| 角色限制  | `require_role!(Role::TenantAdmin)`                 | 敏感操作   | 6    |
| 组合门控  | `require_access! { permission: "...", role: ... }` | 权限+角色  | 4    |

### 租户过滤模型

| 模型     | helper                     | 过滤键      | 模块                                |
| -------- | -------------------------- | ----------- | ----------------------------------- |
| STRICT   | `current_tenant_scope()`   | tenant_id   | Role, Dept, Post                    |
| PLATFORM | `current_platform_scope()` | platform_id | Config, DictType, DictData          |
| 不过滤   | —                          | —           | Menu, TenantPackage, User(via join) |

---

## Observability

### Root span 结构

```text
http_request {request_id, method, path, tenant_id, user_id, user_name, status}
  ├── middleware.auth
  ├── middleware.tenant_guard
  ├── middleware.access {has_permission, has_role, has_scope}
  └── service::list → repo::find_page → sqlx::query
```

- `request_id` / `method` / `path` 在 `tenant_http` 中间件设置
- `user_id` / `user_name` / `tenant_id` 在 `auth` 中间件设置
- `status` 在响应后设置
- 下游 `#[tracing::instrument]` 自动继承所有字段，**不得** 在 find_page 重复声明 `tenant_id`

---

## 常用命令

```bash
# 开发
cargo build -p app
./target/debug/app                     # 端口 18080

# Swagger UI
open http://127.0.0.1:18080/swagger-ui/

# 测试
cargo test --workspace                 # 326 tests
cargo clippy --all-targets
cargo fmt --check

# Smoke (需要 app 在跑 + DB 在跑, 共 12 脚本 119 steps)
for s in scripts/smoke-*.sh; do bash "$s"; done

# 或单独运行
bash scripts/smoke-role-module.sh      # 14 steps
bash scripts/smoke-user-module.sh      # 16 steps
bash scripts/smoke-tenant-module.sh    # 13 steps
bash scripts/smoke-menu-module.sh      # 10 steps
bash scripts/smoke-dept-module.sh      #  8 steps
bash scripts/smoke-post-module.sh      #  8 steps
bash scripts/smoke-config-module.sh    #  9 steps
bash scripts/smoke-dict-module.sh      # 11 steps
bash scripts/smoke-notice-module.sh    #  7 steps
bash scripts/smoke-notify-module.sh    # 12 steps
bash scripts/smoke-operlog-module.sh   #  6 steps
bash scripts/smoke-loginlog-module.sh  #  5 steps

# Git
git remote -v                          # github: jeryaiwei/saas.git
git push github main
```

---

## 下一步

1. **Vue Web 灰度切换** — 188 端点覆盖管理端，改 proxy target 即可
2. **腾讯/华为 SMS 真实接入** — 替换 mock client

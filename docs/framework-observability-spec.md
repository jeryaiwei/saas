# Tea-SaaS 可观察性框架规范 v1.0

**生效日期**：2026-04-12
**状态**：Normative（规范性）
**适用版本**：server-rs workspace

> 本规范是规范性文档，使用 RFC 风格的 `必须 / 应当 / 可以 / 不应 / 不得` 关键词。每一条规则都对应一个在设计评审中识别出的具体失败模式或架构债。
>
> **适用对象**：所有编写 handler / service / repo / middleware / business metric 的开发者、reviewer、以及框架本身的维护者。
>
> **关联规范**：
> - `docs/framework-pagination-spec.md` — pagination 观察性字段是本规范的一个专用子集
> - `docs/framework-error-envelope-spec.md` — error tracing 要求是本规范 §6 的补充

---

## 0. 适用范围

**规范覆盖**：
- `#[tracing::instrument]` 在 handler / service / repo / middleware / infra 层的使用约定
- `tracing::{info, warn, error, debug, trace}!` event 的 level 选择和结构化字段命名
- `metrics::{counter, histogram, gauge}!` 的命名、label set、cardinality 控制
- `RequestContext` 字段到 tracing span 的自动传播
- tower-http `TraceLayer` 与 axum `metrics_middleware` 的分工
- `/metrics` Prometheus endpoint 的 label cardinality 上限
- Business metric 的命名空间和注册流程
- `scope_spawn` 的强制使用场景

**规范不覆盖**：
- `tracing-opentelemetry` / OTLP / Jaeger 集成（见 §11 演化路线 v2.0）
- `/admin/log-level` runtime filter reload（v2.0）
- 业务 dashboard / alert rule（Grafana / Prometheus 配置，非 Rust 代码范畴）
- SLO / SLA 定义
- 数据留存和脱敏的合规层面

---

## 1. 设计原则（rank-ordered）

1. **Cardinality 有界 > 信号完整**：Prometheus label 的任何新增都必须通过 cardinality 预算审查——任何无边界的 label（`user_id` / `tenant_id`（1 M+ tenant 规模下）/ 原始 URI path / 完整 SQL 文本 / 任意 error message）一律禁止。这是 production-safety 的 P0。
2. **Tracing 信息跟随请求上下文而非函数**：`request_id` / `tenant_id` / `user_id` 等请求级别字段**必须**由框架层一次性注入 root span，下游 `#[instrument]` 继承；业务代码**不得**在每个 `#[instrument]` 上手写 `tenant_id` 字段（当前 pagination v1.1 的 `Span::current().record("tenant_id", ...)` 是临时妥协，v1.0 废除）。
3. **显式 event level 宪法**：`debug` / `info` / `warn` / `error` 的选择不是个人风格——每一个 level 对应一个明确的运维语义，审查时可以指着规范条款说对错。
4. **业务 metric 是 first-class**：当前全部 metric 只来自 framework HTTP middleware，业务侧零信号。v1.0 建立业务 metric 命名约定 + 起始模板，但**不**承诺立即埋点（埋点工作归属于各模块自己的 plan）。
5. **失败可见不可静默**：所有捕获但不上报的错误（例如"降级后继续执行"）必须 emit `tracing::warn!` 或 `tracing::error!`——任何 `.unwrap_or_default()` / `.ok()` 的模式在 v1.0 审查里都是 red flag。

---

## 2. 类型契约

### 2.1 tracing span 字段命名宪法

所有 `#[tracing::instrument(fields(...))]` 和 `tracing::info!(k = v, ...)` 的 key 必须遵循：

**强制命名规则**：
- **必须** 使用 `snake_case`
- **必须** 使用完整词，**不得** 使用缩写：`user_id`（不是 `uid`），`tenant_id`（不是 `tid`），`request_id`（不是 `req_id`），`role_count`（不是 `role_num`）
- **不得** 用单字段名 `id`（歧义）——必须带类型前缀（`user_id` / `role_id` / `menu_id`）
- **不得** 用 camelCase（`userId` / `roleId`）

**语义区分的保留例外**：`username` 和 `user_name` **不是**同一个字段：

| 字段名 | 来源 | 什么时候用 |
|---|---|---|
| `username` | HTTP 层登录请求体 `LoginDto.username` | auth 流程（login、captcha 校验），表示"用户提供的登录标识符" |
| `user_name` | DB 列 `sys_user.user_name` | 所有已认证的请求上下文里，表示"当前已确认存在的用户名" |

这是有意的区分。当 auth flow **未完成**时，`username` 还只是一个未经验证的输入字符串；一旦 `UserRepo::find_by_username` 成功，它变成 `user_name` 字段写进 `RequestContext`。**不得** 把两者混用。

**白名单字段**（允许出现在 span 中的标准字段）：

| 字段 | 类型 | 来源 |
|---|---|---|
| `request_id` | String | `RequestContext.request_id`（root span 自动注入） |
| `tenant_id` | String | `RequestContext.tenant_id`（root span 自动注入） |
| `user_id` | String | `RequestContext.user_id`（root span 自动注入） |
| `user_name` | String | `RequestContext.user_name`（已认证后） |
| `username` | String | 登录请求体（未认证） |
| `user_type` | String | `"10"` / `"20"` |
| `platform_id` | String | `RequestContext.platform_id` |
| `sys_code` | String | 子系统代码 |
| `role_id` / `role_ids` | String / Vec | 角色 PK |
| `menu_id` / `menu_ids` | String / Vec | 菜单 PK |
| `page_num` / `page_size` | u32 | 分页参数 |
| `rows_len` | u64 | 返回行数 |
| `total` | i64 | COUNT 结果 |
| `offset` | i64 | LIMIT offset |
| `rows_ms` / `count_ms` / `total_ms` | u128 | 查询耗时 |
| `count` | usize | 通用数量计数（上下文决定含义） |
| `code` | i32 | ResponseCode 数值 |
| `status` | String | DB 层 `status` 列值 / HTTP status |
| `method` | String | HTTP 方法 |
| `path` | String | 路由模板（**不得** 是原始 URI，见 §3.2） |
| `has_*_filter` / `has_*` | bool | 过滤条件是否启用的布尔布尔 |
| `duration_ms` | u128 | 通用耗时 |
| `error` | Display | `tracing::warn!(error = %e, ...)` 通用错误字段 |

**新增字段的流程**：新增不在上表里的字段**必须** 同步更新本规范 §2.1 + PR 描述说明语义。**不得** 未加规范先写代码。

### 2.2 tracing event level 宪法

每个 `tracing::{info,warn,error,debug,trace}!` 调用必须匹配以下决策树：

```
ERROR  — 必须触发告警，人工介入才能恢复
         例：AppError::Internal 路径、panic 捕获、unrecoverable infra 失败
         频率上限：每请求 0~1 次
         涉及 RequestContext.user/tenant 时 MUST 已经自动在 span

WARN   — 预期外但自愈/有降级；单个事件不需要报警，但聚合到 alert threshold 需要
         例：慢查询超阈值、i18n 翻译缺失、bcrypt verify 异常（当成 false）
         频率上限：每请求 0~2 次
         MUST 带结构化字段说明"怎么了"和"降级到哪里"

INFO   — 请求关键转折点 / lifecycle 事件 / 低频一次性动作
         例：服务启动完成、登录成功、租户切换、password reset 完成
         频率上限：稀疏——高频热点路径（listing, query, validation）禁止用 info
         热路径下禁止（例如 find_page 不得发 info event）

DEBUG  — 内部状态、开发辅助信息、可以在生产 `level=debug` 短暂开启
         例：JWT decode 失败细节、i18n 查表结果、span 内联参数值
         频率上限：不限；默认 filter level 下不采集

TRACE  — 极细粒度（目前 Tea-SaaS 不使用 trace level）
```

**补充规则**：
- **不得** 用 `info!` emit 高频事件（例如每个 list 查询一次）——会淹没日志管道
- **不得** 用 `warn!` / `error!` 处理预期业务错误（`Business` / `Auth` / `Forbidden` 是用户错，不是系统错）
- **必须** 对 `AppError::Internal → IntoResponse` 发出 `tracing::error!(error = ?e, "internal error")`——这是当前唯一的 500 追溯入口，已实现于 [app_error.rs:144](server-rs/crates/framework/src/error/app_error.rs#L144)
- **必须** 对 i18n 缺失、bcrypt verify 异常、慢查询用 `warn!`
- **必须** 对 JWT decode 失败用 `debug!`（不是 bug，可能是客户端过期 token）
- **不得** 在 event message 里拼接 PII：密码、明文 token、完整 session 对象
- **不得** 用 `println!` / `dbg!` 作为日志机制

### 2.3 RequestContext 字段到 root span 的自动注入

**必须** 在 `tenant_http` middleware（最外层）把 `RequestContext` 的核心字段作为 span field 注入一个 `http_request` root span。所有下游 `#[instrument]` 的 span 会自动 nest 到这个 root 下面，字段**自动继承**。

> **v1.0 实现要求**：`tenant_http` middleware 必须同时**开启 span** 和**设置 context**，两个动作在同一个作用域内。因为 `#[tracing::instrument]` 是在函数进入时创建子 span，要让它能继承 `request_id` 字段，根 span 必须在 `next.run(req).await` 期间处于 `entered` 状态。

span 结构（从外到内）：

```
http_request [request_id, tenant_id=Empty, user_id=Empty, user_name=Empty, method, path, status=Empty]
  └── <axum TraceLayer 的默认 span>
    └── <下游 middleware / handler / service / repo 的 #[instrument] spans>
```

**字段填充时机**：

| 字段 | 由谁填 | 时机 | 填充方式 |
|---|---|---|---|
| `request_id` | `tenant_http` middleware | 请求入口 | span 创建时直接填入，非 `Empty` |
| `tenant_id` | `tenant_http`（header）或 `auth`（session 覆盖） | 入口或 auth 后 | `field::Empty` + `.record(...)`，auth 覆盖时再次 `.record` |
| `user_id` | `auth` middleware | auth 成功后 | `field::Empty` + `.record(...)` |
| `user_name` | `auth` middleware | auth 成功后 | `field::Empty` + `.record(...)` |
| `method` | `tenant_http` | 入口 | 直接填入 |
| `path` | `tenant_http` | 入口 | 直接填入（**路由模板**，非 URI，见 §3.2） |
| `status` | `tenant_http` | `next.run(req).await` 完成后 | `field::Empty` + `.record(...)` |

业务代码 **不得** 在自己的 `#[instrument]` 上重复写 `tenant_id` / `user_id` / `request_id` —— 字段继承是 tracing 的语义保证。

**v1.0 迁移要求**：`user_repo::find_page` / `role_repo::find_page` / `find_allocated_users_page` / `find_unallocated_users_page` 4 个方法目前手写了 `Span::current().record("tenant_id", ...)`——v1.0 落地后这些行**必须**删除（因为 root span 已经负责了）。

### 2.4 Prometheus metric 命名规范

**必须** 遵循 Prometheus 官方命名约定 + Tea-SaaS 扩展：

```
<domain>_<subject>_<action>_<unit>{labels...}
```

**Domain 前缀（命名空间）**：
- `http_*` — HTTP 入站流量（已有 `http_requests_total`, `http_request_duration_seconds`）
- `db_*` — DB 层（SQL 查询计数、慢查询计数、连接池）
- `redis_*` — Redis 层（命令计数、命中率）
- `auth_*` — 认证和授权（登录、验证码、JWT、token blacklist）
- `tenant_*` — 租户运维事件（切换、禁用、过期）
- `biz_*` — 业务操作（列表查询、CRUD 写入、导出）
- `infra_*` — 基础设施启动、shutdown、config reload

**禁止** 前缀：
- `metric_*` / `app_*` / `server_*`（无区分度）
- 任何业务模块名作为顶层前缀（`user_*` / `role_*`）—— 走 `biz_user_*` / `biz_role_*`

**Action 后缀**（遵循 OpenMetrics）：
- `_total` — counter 必须
- `_seconds` / `_milliseconds` — histogram 时间
- `_bytes` — histogram 字节数
- 其他 gauge 用裸名

**禁止** 的名字：
- `counter1`, `metric`, `xxx_ok` / `xxx_fail`（应用 `outcome` label 区分）
- 以 i18n 文本或中文命名

### 2.5 Prometheus label cardinality 预算

**硬上限**：每个 metric 的 series 数**不得**超过以下预算：

| 类别 | 上限 | 原因 |
|---|---|---|
| `http_*` | method × path_template × status ≈ **10 × 30 × 10 = 3000** | 路由数 × 方法数 × 常见状态码 |
| `db_*` / `redis_*` | repo × operation × outcome ≈ **20 × 10 × 3 = 600** | 按 repo 方法计 |
| `auth_*` | action × outcome ≈ **10 × 5 = 50** | 极少 |
| `biz_*` | module × action × outcome ≈ **30 × 20 × 3 = 1800** | 大块预算给业务 |

**绝对禁止** 作为 label 的字段：
- `user_id` / `user_name`（> 10k 用户就爆）
- `tenant_id`（当前小，未来可能 >10k）
- `request_id`（无界，每请求不同）
- `session_uuid` / `jwt_uuid`（同上）
- 原始 URI path（见 v1.0 P0 fix：**必须**用 route template `{id}` 占位符）
- 原始 SQL 文本 / query string
- 任意 error message 文本（可能包含值）
- 文件路径、URL 绝对路径、timestamp

**允许** 作为 label：
- HTTP method（有限枚举：GET/POST/PUT/DELETE/OPTIONS ≈ 6）
- HTTP status code（3 位数字，实际 ≈ 10）
- 路由模板（有限枚举：路由表大小 ≈ 30~100）
- `outcome` = `success` / `failure` / `timeout` / `denied`（≤ 10 枚举）
- `repo_method` 字符串字面量（编译期已知的 ≈ 20）
- 业务 error code 数字（`ResponseCode`，≈ 30）
- `user_type` = `"10"` / `"20"`

**label set 变更的 PR checklist**：
- [ ] 最高 cardinality 的 label 是什么？枚举大小 ≤ 100？
- [ ] 所有 label 的乘积 ≤ 5000？
- [ ] 没有 user_id / tenant_id / session_uuid / request_id / 任意文本 error message 作为 label？
- [ ] 所有 label 值是**编译期枚举**或**来自有限配置/路由表**？

### 2.6 业务 metric 起始模板（v1.0 规范，不承诺 v1.0 埋点）

v1.0 **定义**以下 5 个业务 counter 作为命名示例和埋点起点；**实际埋点**放入各自模块的 plan，不强制 v1.0 落地：

```rust
// auth 模块
metrics::counter!("auth_login_total", "outcome" => "success").increment(1);
metrics::counter!("auth_login_total", "outcome" => "invalid_credentials").increment(1);
metrics::counter!("auth_login_total", "outcome" => "account_locked").increment(1);
metrics::counter!("auth_captcha_total", "outcome" => "verified").increment(1);
metrics::counter!("auth_captcha_total", "outcome" => "mismatch").increment(1);

// pagination 模块（现有慢查询 warn 的 metric 版本）
metrics::counter!(
    "db_pagination_slow_total",
    "repo" => "user.find_page",
).increment(1);

// DB 模块
metrics::counter!(
    "db_query_error_total",
    "repo" => "user_repo",
    "operation" => "find_page",
).increment(1);

// 租户模块
metrics::counter!(
    "tenant_access_denied_total",
    "reason" => "tenant_disabled",
).increment(1);
```

`outcome` / `reason` / `repo` / `operation` 是**闭合枚举**，每个 metric 的这些 label 值必须写进 spec 或代码注释。禁止使用运行时计算的字符串（见 §2.5）。

---

## 3. 分层责任

| 层 | 自动获得 | 必须手动写 | 不得做 |
|---|---|---|---|
| **root span (`tenant_http`)** | 创建 `http_request` span，填 `request_id` / `method` / `path` | `.record("tenant_id", ...)` / `.record("user_id", ...)` 等在下游 middleware 设置时 | 开多个 root span（违反一次请求一个 trace 的规则） |
| **auth middleware** | span 继承 `request_id` / `method` / `path` | `.record("user_id", ...)` / `.record("user_name", ...)` | 用 `tracing::info!` 记录登录成功（热路径，用 `debug!`） |
| **handler** | 字段继承 | — | 添加 `#[instrument]`（重复 span 层级，无意义） |
| **service** | 字段继承 | `#[instrument(skip_all, fields(...))]` 声明业务字段（role_count、page_num 等） | 在 instrument 上写 `tenant_id` / `user_id`（冗余） |
| **repo** | 字段继承 | 同 service，加 tenant-scope 读取标记 | 调用 `Span::current().record("tenant_id", ...)`（spec §2.3 明确禁止） |
| **metrics middleware** | `MatchedPath` 已用作 label | — | 在 middleware 里 emit 业务 metric（这是业务层职责） |
| **业务代码 metric 埋点** | — | 在 service 层发 counter/histogram，label 走 §2.6 模板 | 在 handler / repo 层埋业务 metric（service 是单一入口） |

### 3.2 `metrics_middleware` 的 path label 契约

**P0 生产安全要求**：path label **必须** 使用 axum `MatchedPath` 返回的**路由模板**（`/api/v1/system/user/{id}`），**不得** 使用 `req.uri().path()`（会泄露动态段到 label，导致 Prometheus cardinality 爆炸）。

**实现约束**：
- `metrics_middleware` **必须** 在 axum 路由解析**之后**运行（即通过 `Router::layer(...)` 挂载，而不是 `ServiceBuilder` 外层）——这样 `MatchedPath` 已经在 request extension 中
- 当 `MatchedPath` 不存在（404、pre-routing 错误）**必须** fallback 到字面量 `"<unmatched>"`——**不得** fallback 到 URI path
- 抽出 `route_label(&req) -> String` 纯函数以便单测

**v1.0 状态**：✅ 已实施（2026-04-12 P0 fix，见 [middleware/telemetry.rs](server-rs/crates/framework/src/middleware/telemetry.rs)）。

---

## 4. 端点实现规范

### 4.1 Handler 层

**不加** `#[instrument]`。Handler 只是请求分发层，root span 已经由 `tenant_http` 建立，TraceLayer 已经记录 HTTP method/path/status。在 handler 上再加 span 只是多一层 nest 没有新信号。

**不得** 在 handler 里用 `tracing::info!("got request: ...")` 打印请求——TraceLayer 已经做了。

### 4.2 Service 层

```rust
#[tracing::instrument(skip_all, fields(
    page_num = query.page.page_num,
    page_size = query.page.page_size,
    has_user_name = query.user_name.is_some(),
))]
pub async fn list(
    state: &AppState,
    query: ListUserDto,
) -> Result<Page<UserListItemResponseDto>, AppError> {
    // ... tenant_id / user_id / request_id 已由 root span 继承 ...
    let page = UserRepo::find_page(&state.pg, filter).await.into_internal()?;
    Ok(page.map_rows(UserListItemResponseDto::from_entity))
}
```

**规则**：
- **必须** 用 `#[tracing::instrument(skip_all, fields(...))]`（不是 `#[instrument]` 无前缀——模块不统一 import 时歧义），但 `use tracing::instrument;` 后的 `#[instrument]` 也允许
- **必须** 用 `skip_all` —— 默认 instrument 会把所有参数的 `Debug` 写进 span，可能包含密码/token/大对象
- **不得** 在 fields 里重复 `tenant_id` / `user_id`（已由 root span 继承）
- **不得** 把 `query` / `dto` 整体作为字段（`fields(query = ?query)`）—— span 内存膨胀
- **必须** 只选"这个方法特有的业务字段"（`page_num` / `role_count` / `has_*_filter`）

### 4.3 Repo 层

```rust
#[tracing::instrument(skip_all, fields(user_id = %user_id))]
pub async fn find_by_id(pool: &PgPool, user_id: &str) -> anyhow::Result<Option<SysUser>> {
    // ...
}
```

**规则**：
- **必须** 用 `skip_all` + `fields(...)`
- **必须** 包含方法的**主键参数**（`user_id` / `role_id`）——这是 DB 查询的最小追溯单位
- **不得** 用 `Span::current().record("tenant_id", ...)` 手动注入（v1.0 废除）
- **不得** 在 repo 层 emit metric（§3 分层责任）
- **必须** 在 rows_query / count_query 分别计时的场景继续保留 `rows_ms` / `count_ms` / `total_ms` 字段（pagination v1.1 已有）

### 4.4 Middleware 层

**必须** 给 auth / tenant / access 三个核心 middleware 加 `#[instrument(skip_all, name = "middleware.<name>")]`，以便 trace tree 里能看到每个 middleware 的耗时和分支。当前 v0 是**全裸**状态，v1.0 要补齐。

`tenant_http` **不加**普通 `#[instrument]`，因为它自己就是 root span 的创建者。

### 4.5 Infra 层（重点：`infra/crypto.rs`, `infra/redis.rs`, `infra/pg.rs`, `auth/jwt.rs`）

- `bcrypt::hash_password_with_cost`**必须**包装 `#[instrument(skip_all, name = "infra.crypto.hash_password")]`——是慢操作（~100ms），必须可见
- `jwt::encode_token` / `jwt::decode_token`**应当**包装 `#[instrument(skip_all, name = "infra.jwt.encode")]`（热路径，但时间 < 1ms，debug level 可以）
- Redis `session::*` 已有 v1.0 `#[instrument]`，**不得**回退
- `i18n::get_message` 不要加 instrument（每请求调用多次，过度仪表）

---

## 5. Root span 注入的实现契约

`tenant_http` 从 v0 的"创建 RequestContext"升级为 v1.0 的"创建 RequestContext + 开根 span"。伪代码：

```rust
use tracing::{field, info_span, Instrument};

pub async fn tenant_http(req: Request, next: Next) -> Response {
    let headers = req.headers();
    let request_id = extract_request_id(headers);
    let method = req.method().to_string();
    let path = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string())
        .unwrap_or_else(|| "<unmatched>".to_string());

    let span = info_span!(
        "http_request",
        request_id = %request_id,
        method = %method,
        path = %path,
        tenant_id = field::Empty,
        user_id = field::Empty,
        user_name = field::Empty,
        status = field::Empty,
    );

    let ctx = RequestContext {
        request_id: Some(request_id),
        tenant_id: /* header */,
        lang_code: Some(lang_code),
        ..Default::default()
    };

    async move {
        let response = scope(ctx, next.run(req)).await;
        tracing::Span::current()
            .record("status", response.status().as_u16());
        response
    }
    .instrument(span)
    .await
}
```

**关键点**：
1. `info_span!` 必须用 `field::Empty` 预留 `tenant_id` / `user_id` / `user_name` / `status` 槽位——下游 middleware / 请求结束后填充
2. `.instrument(span)` 包住 `scope(...)` + `next.run(...)` 整段，否则 span 不会 enter
3. `path` label 用 `MatchedPath`（和 `metrics_middleware` 对齐）
4. `status` 在 `next.run(req).await` 完成后 record

**auth middleware 端的协调修改**：

```rust
// 当前 auth middleware 的 RequestContext::mutate 之后，加 span record:
RequestContext::mutate(|ctx| {
    ctx.user_id = Some(user_session.user_id.clone());
    // ...
});
let span = tracing::Span::current();
span.record("user_id", user_session.user_id.as_str());
span.record("user_name", user_session.user_name.as_str());
if let Some(tid) = user_session.tenant_id.as_deref() {
    span.record("tenant_id", tid);
}
```

这几行是 framework 的职责，**不是** 业务层的职责。业务 service 不再需要管这些字段。

---

## 6. Event level 宪法（细化）

### 6.1 必须是 `error!`

- `AppError::Internal → IntoResponse` 的捕获点（目前已在 [app_error.rs:144](server-rs/crates/framework/src/error/app_error.rs#L144)）
- tokio task panic 捕获
- DB 连接池耗尽
- 启动时配置解析失败（走 `anyhow::bail!` 而非 event）

### 6.2 必须是 `warn!`

- 慢查询（pagination `SLOW_QUERY_WARN_MS` 超标）—— ✅ 已在 4 个 find_page 实装
- i18n 翻译缺失 —— ✅ 已在 [i18n/mod.rs:102](server-rs/crates/framework/src/i18n/mod.rs#L102)
- bcrypt verify 异常（catch-all 处理） —— ✅ 已在 [infra/crypto.rs:33](server-rs/crates/framework/src/infra/crypto.rs#L33)
- Post-condition 违反后的降级（如 pagination rows 超 limit 的 truncate+warn） —— ✅ 已在 pagination v1.1
- 租户切换到非常规状态（admin 动作）
- 配置 fallback（APP_DOMAIN 解析失败等，已在 [main.rs:220](server-rs/crates/app/src/main.rs#L220)）

### 6.3 必须是 `debug!`

- JWT decode 失败细节（频率高，不是 bug）—— ✅ 已在 [auth/jwt.rs:83](server-rs/crates/framework/src/auth/jwt.rs#L83)
- ValidatedJson / ValidatedQuery 反序列化失败（客户端错误，不是服务器错）—— ✅ 已在 [validated_json.rs:35](server-rs/crates/framework/src/extractors/validated_json.rs#L35) 和 [validated_query.rs:40](server-rs/crates/framework/src/extractors/validated_query.rs#L40)
- auth service 授予权限的中间过程（已在 [auth/service.rs:65](server-rs/crates/modules/src/auth/service.rs#L65) 用 debug）

### 6.4 可以是 `info!`

- 服务启动完成（`"listening"` / `"bootstrap begin"`）
- 服务优雅 shutdown 完成
- 首次 admin 登录、password reset、account unlock（低频审计事件）

**v1.0 gap**：auth service 登录成功目前**没有** event——应当补一条 `tracing::info!(username = %dto.username, "login success")`（见 §12 gap 表）。

### 6.5 禁止

- `info!` 在 find_page、list、detail 等**热路径**的成功出口
- `error!` 在业务错误路径（`Business` / `Auth` / `Forbidden`）
- 任何 level emit PII（password、token 完整字符串、completed session）
- 任何 level 在 span 上重复 `request_id` / `tenant_id`（已由 root span 覆盖）

---

## 7. 禁止模式

| 模式 | 为什么禁止 | 替代方案 |
|---|---|---|
| `req.uri().path()` 作为 metric path label | cardinality 爆炸 | `MatchedPath` + `<unmatched>` fallback |
| `user_id` / `tenant_id` / `request_id` 作为 metric label | 无界 cardinality | label 留给有限枚举，`user_id` 只进 tracing span |
| `fields(query = ?query)` 把 DTO 整体写进 span | span 内存膨胀 + PII 泄露风险 | `fields(page_num = q.page.page_num, ...)` 选业务字段 |
| `Span::current().record("tenant_id", ...)` 在 service / repo 层 | 和 root span 注入重复 | 删除手动 record，依赖继承 |
| `#[instrument]` 不加 `skip_all` | 默认把所有参数 Debug 化 | 始终 `skip_all` + 显式 `fields(...)` |
| `tracing::info!` 在 list / query 热路径 | 日志管道爆 | `debug!` or 去掉 |
| `tracing::warn!` 处理业务错误 | 语义错位，alert 误报 | Business 错误不发 event |
| `metrics::counter!("user_count_t0001", ...)` 动态名 | metric name 需要编译期常量 | `counter!("biz_user_total", "tenant_id" => TENANT_LIMITED_ENUM)` 并确保 tenant_id 在白名单 |
| handler 层加 `#[instrument]` | 和 root span 冗余 | handler 不加 span |
| 用字符串 fmt 拼消息再 `info!("...{}...")` | 结构化字段才能查询 | `info!(field = value, "message")` |
| `println!` / `dbg!` / `eprintln!` 日志 | 绕过 subscriber | 只用 `tracing` |
| `_ = some_err;` 吃掉错误 | 观察性黑洞 | 至少 `tracing::warn!(error = %e, ...)` |
| `scope_spawn` 不用，裸 `tokio::spawn` 在背景任务 | `RequestContext` 丢失 | 必须 `scope_spawn` |

---

## 8. 一致性声明（已知限制）

### 8.1 Span 字段继承的 tracing-subscriber 限制

tracing 的字段继承**不是**通过字段拷贝实现的——子 span 在创建时并不带父 span 的字段值，只有最终被 subscriber 序列化时 formatter 会遍历 span 链。这意味着：

- **json layer 的输出里**每条 log 会展开**整个 span 链**的所有字段，`request_id` 会在每条子事件里出现
- **metrics 采样器**读不到父 span 字段——如果未来有"按 request_id 聚合 histogram"的需求，需要额外 mechanism
- **开发机上 pretty formatter 的输出**可能 span 字段折叠——可读性比 json 差

v1.0 接受这些限制，不引入 `tracing-opentelemetry` 来解决。

### 8.2 `info_span!` 的成本

每个请求一个 root span 的成本：约 1 KB 分配 + subscribe 遍历。Tea-SaaS 的目标吞吐（几百 RPS）远低于 tracing 的 overhead 阈值（10K RPS）。不担心。

### 8.3 Cardinality 审查的手动纪律

v1.0 **不**实现自动 cardinality 监控（需要 Prometheus 端配置 alert）。依赖 PR review 严格执行 §2.5。当生产出现 Prometheus OOM 告警时走 v2.0 规范扩展自动化审查。

---

## 9. 安全契约

### 9.1 PII 禁止清单

tracing 和 metrics 输出均 **不得** 包含：
- 明文密码、JWT 完整字符串、session uuid
- 信用卡、身份证、手机号（除 `phonenumber` 列本身的 tracing——场景特殊且受 tenant scope 保护）
- Email 的**完整**字符串（域名部分可以，用户部分不行）——v1.0 先不强制，归属 v2.0 合规审查
- 原始 SQL with bind 值
- 请求 body / response body 完整内容

### 9.2 Error 路径的信息披露

`tracing::error!(error = ?e, ...)` 的 `e` 可能包含 DB error 的详细 SQL context（anyhow::Context 注入的）。这在日志里**可接受**——只有 ops 能看到；但 **不得** 泄露到 wire response（已由 error-envelope spec §9 确保）。

### 9.3 Metric label 的合规审查

每个新 metric 的 PR 必须检查：label 是否可能编码 PII（间接的——比如 `user_agent` 可以推断设备，`path_param_count` 可能推断 email 长度）。v1.0 没有自动化工具，依赖 reviewer 眼睛。

---

## 10. Observability 栈的分工

| 组件 | 职责 | 输出 |
|---|---|---|
| `tower-http::TraceLayer` | HTTP 请求/响应的 access log（method, path, status, duration） | `tracing::info_span!("request", ...)` |
| `tenant_http` | 创建 root span + RequestContext + 设置 lang/request_id/tenant_id | 一个 `info_span!("http_request", ...)` |
| `auth` middleware | 填充 `user_id` / `user_name` / 认证相关字段到 root span 和 context | `.record(...)` 到当前 span |
| `metrics_middleware` | 采集 HTTP counter + histogram | `metrics::counter!/histogram!` |
| 业务 `#[instrument]` | 业务字段和 timing | nested spans |
| 业务 `metrics::counter!` 调用 | 业务事件计数 | 埋点在 service 层 |
| `i18n::warn!` / `app_error::error!` | 降级/错误 event | event within span |

**冗余说明**：TraceLayer 记录 HTTP 请求维度，`tenant_http` 的 root span 记录业务上下文维度。两者**并存**——TraceLayer 更 HTTP-native，`tenant_http` root span 是业务追溯锚点。v1.0 不合并它们。

---

## 11. 演化路线

| Phase | 内容 | 触发条件 | 预估成本 |
|---|---|---|---|
| **v1.0** | `metrics` path label 用 MatchedPath（P0 fix ✅）、root span 规范、字段命名宪法、event level 宪法、metric 命名约定、业务 metric 模板 | 立即 | ~300 LOC 代码 + ~900 LOC 文档 |
| **v1.1** | auth/tenant/access/infra middleware 的 `#[instrument]` 补齐，所有热路径的慢操作可见 | 生产首次 debug 需求 | ~100 LOC |
| **v1.2** | 业务 metrics 首次埋点（按 §2.6 模板实装 5 个起始 counter） | 产品/运维要求第一个 dashboard | ~200 LOC + Grafana 配置 |
| **v2.0** | `tracing-opentelemetry` + OTLP exporter，跨服务 trace | 引入第二个 Rust 服务或 service mesh | 新 crate + config + 1 周 |
| **v2.1** | `/admin/log-level` runtime filter reload | 生产出现需要临时开 debug level 的 incident | ~150 LOC |
| **v2.2** | Automatic cardinality audit（CI 层扫描 metric label 是否来自白名单） | 出现第一次 Prometheus OOM / 高基数告警 | ~300 LOC + CI |
| **v3.0** | 结构化审计日志（`audit_*` 独立 sink，不走普通 tracing 管道） | 合规审查 | 独立一周 |

---

## 12. v1.0 与当前代码的 gap

| 规范条目 | 当前状态 | 需要改动 | 优先级 |
|---|---|---|---|
| §3.2 path label | ✅ 已用 MatchedPath（2026-04-12 P0 fix） | — | **完成** |
| §2.3 root span 自动注入 `request_id` / `tenant_id` / `user_id` | `tenant_http` 只建 RequestContext，不开 span | `tenant_http` 加 `info_span!` + `.instrument(...)` 包住 next.run | **高** |
| §2.3 废除业务层手写 `Span::current().record("tenant_id", ...)` | 4 处手写（4 个 find_page） | 删 4 行 | **高**（和上一条配对） |
| §4.4 auth/tenant/access middleware 补 `#[instrument]` | 0/3 | 加 3 个 `#[instrument]` | 中 |
| §4.5 `bcrypt::hash_password` instrument | 无 | 1 个 `#[instrument]` | 中 |
| §6.4 auth login 成功 `info!` event | 无 | 1 行 `tracing::info!(...)` | 低 |
| §2.1 `username` vs `user_name` 的语义区分 | 已是对的（认知 bug）——规范文档化即可 | 文档化到 spec | **完成**（本规范已写入） |
| §2.1 `tenant = %tid` 在 auth service | 应为 `tenant_id` | 改 1 行 | 低 |
| §2.6 业务 metric 起始模板 | 无 | 文档化到 spec，埋点延期到 v1.2 | **完成**（本规范已定义） |
| §2.5 cardinality label 审查 | 有口头约束，无自动化 | 走 v2.2 | 延期 |
| §5 root span 的 `status` 字段记录 | 无 | `tenant_http` 的 response 后 `.record("status", ...)` | **高**（和 root span 配对） |

**v1.0 必做**（gap 表的"高"优先级，3 项）：

1. `tenant_http` 升级为 root span 创建者（spec §5 的伪代码）
2. `auth` middleware 补 `.record("user_id", ...)` / `.record("user_name", ...)` / `.record("tenant_id", ...)`
3. 删除 4 个 `find_page` 里的 `Span::current().record("tenant_id", ...)` 手写行

**v1.0 次做**（可以同一批，也可以延期）：

4. `bcrypt::hash_password` / `auth` / `tenant` / `access` 的 `#[instrument]` 注解
5. auth service 登录成功的 `tracing::info!` event
6. 修复 auth service 的 `tenant = %tid` → `tenant_id = %tid`

**v1.0 不做**（明确延期）：

- 业务 metric 埋点（v1.2）
- OTEL / OTLP（v2.0）
- Runtime log level（v2.1）
- 自动 cardinality 审查（v2.2）

---

## 13. 新增 observability 的样板

### 13.1 新增一个 `#[instrument]`

**Step 1** — 检查是否在热路径：如果每秒 > 100 次调用，避免加；走 debug level 或不加
**Step 2** — 选字段：从 §2.1 白名单挑，不在白名单的先改规范
**Step 3** — 加属性：

```rust
#[tracing::instrument(skip_all, fields(
    // 只有业务字段，不要重复 request_id / tenant_id / user_id
    page_num = query.page.page_num,
    role_count = dto.role_ids.len(),
))]
pub async fn xxx(...) -> Result<...> { }
```

**Step 4** — 跑 test，确认字段出现在 test log 里（手动观察）

### 13.2 新增一个业务 metric

**Step 1** — 选命名空间（`auth_*` / `db_*` / `biz_*` / `tenant_*`，见 §2.4）
**Step 2** — 设计 label set：
- 所有 label 必须是**编译期常量**或**来自有限枚举**
- 计算总 cardinality：`len(label_1) × len(label_2) × ...` ≤ §2.5 预算
- 写进 spec §2.6 的示例表（如果是 framework-level metric）

**Step 3** — 埋点位置：**service 层**，不是 repo 或 handler

```rust
metrics::counter!(
    "biz_<module>_<action>_total",
    "outcome" => outcome,  // &'static str 枚举
    "tenant_type" => tenant_type,  // 如果是多租户敏感
)
.increment(1);
```

**Step 4** — 加一条 doc comment 说明 metric 的**基数预算**和 label 的**枚举值**（未来 reviewer 要看得到）

### 13.3 新增一个 event level 决策

- 频率 > 10/s → 不得用 `info!` 以上
- 频率 > 1/s → 不得用 `warn!` 以上
- 观察性审查问：**一周内这个 event 会触发多少条**，> 10K 建议降级

---

## 14. PR checklist

提交涉及 tracing / metrics / `#[instrument]` / middleware 的 PR 时**必须**逐项自查：

**Tracing 字段**：
- [ ] 新字段命名在 §2.1 白名单内
- [ ] 没有 camelCase
- [ ] 没有缩写（`uid` → `user_id`）
- [ ] `#[instrument]` 都用了 `skip_all`
- [ ] 没有重复 `tenant_id` / `user_id` / `request_id`（由 root span 继承）
- [ ] 没有 `fields(dto = ?dto)` 写整个对象
- [ ] 没有 `Span::current().record("tenant_id", ...)` 手写

**Event level**：
- [ ] 热路径的成功出口没有用 `info!`
- [ ] 业务错误没有用 `warn!` / `error!`
- [ ] 只有 `AppError::Internal → IntoResponse` 等 unrecoverable 点用 `error!`
- [ ] event 消息里没有密码 / token / session

**Metric label**：
- [ ] 没有 `user_id` / `tenant_id` / `request_id` / session_uuid / 原始 URI / SQL text 作为 label
- [ ] 所有 label 值是编译期常量或有限枚举
- [ ] 总 cardinality 不超 §2.5 预算
- [ ] metric name 遵循 `<domain>_<subject>_<action>_<unit>`
- [ ] counter 带 `_total` 后缀

**Middleware 顺序**：
- [ ] 新 middleware 明确了它相对 `tenant_http` / `auth` / `metrics_middleware` 的位置
- [ ] `Router::layer(...)` 应用顺序（last applied = outermost）正确

**测试**：
- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] 涉及 metric 时，手动 `curl /metrics | grep` 检查过新 metric 确实出现（并且 label 值符合预期）
- [ ] 涉及 span 字段时，手动观察过 test log 里字段出现
- [ ] 若涉及 root span 的改动，至少 smoke 过一次 role + user 套件

---

## 附录 A：v1.0 明确不做的事

1. `tracing-opentelemetry` / OTLP / Jaeger 集成
2. 跨服务 correlation id 格式（W3C Trace Context）
3. `/admin/log-level` runtime filter reload
4. 自动 cardinality 审查工具
5. 业务 metric 首次埋点（归属 v1.2）
6. SLO / SLA 配置
7. Grafana dashboard 生成
8. 审计日志独立 sink
9. `user_id` / `tenant_id` 作为 metric label 的折中方案（一律禁止）
10. 自动 PII masking（日志脱敏）

每一条都记录在 §11 触发器表，条件未满足**不得**提前做。

---

## 附录 B：与其他规范的关系

| Spec | 关系 |
|---|---|
| `framework-error-envelope-spec.md` §8.1 | error 路径的 tracing 要求——本规范 §6.1 是它的补充和细化 |
| `framework-pagination-spec.md` §6 | pagination 的 observability 字段（`rows_ms` / `count_ms` / `total_ms` / `rows_len` / `total`）必须与本规范 §2.1 白名单一致 |
| `framework-pagination-spec.md` §6.2 慢查询 | 在 v1.2 会升级为 `db_pagination_slow_total` counter，届时 pagination spec 同步更新 |
| `framework-pagination-spec.md` find_page 手写 `.record("tenant_id", ...)` | 本规范 §12 明确废除——pagination spec 后续需同步删除对应代码块描述 |

本规范是 observability 的**唯一真源**。任何其他 spec 涉及 tracing 字段 / metric 命名时都必须 reference 本规范。

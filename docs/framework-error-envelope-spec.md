# SaaS 错误与响应包络框架规范 v1.0

**生效日期**：2026-04-11
**状态**：Normative（规范性）
**适用版本**：server-rs workspace

> 本规范是规范性文档，使用 RFC 风格的 `必须 / 应当 / 可以 / 不应 / 不得` 关键词。每一条规则都对应一个在设计评审中识别出的具体失败模式或架构债。
>
> **适用对象**：所有编写 handler / service / repo / 自定义 error 路径 / 自定义 validator 的开发者、reviewer、以及框架本身的维护者。
>
> **关联规范**：`docs/framework-pagination-spec.md` — 分页包络契约是本规范的一个专用子集。

---

## 0. 适用范围

**规范覆盖**：

- 所有 HTTP 响应体（成功 + 所有错误分支）的 wire 形状和字段名
- `AppError` 的所有 variant 及其构造路径
- `ResponseCode` 的编号段和注册流程
- i18n key 命名空间和参数替换约定
- Validator 错误到 `FieldError` 的映射、字段路径格式
- Service 层错误扩展 trait（`IntoAppError`、`BusinessCheckOption`、`BusinessCheckBool`）

**规范不覆盖**：

- 分页响应的数据结构（见 `docs/framework-pagination-spec.md`）
- Cursor 分页（v3.0，独立 primitive）
- 流式响应 / SSE / WebSocket（独立 primitive）
- 文件下载、二进制响应（独立 primitive）
- OpenAPI schema 导出（触发条件见 §10）

---

## 1. 设计原则（rank-ordered）

1. **单一 wire envelope**：成功和错误共享同一个 JSON 形状，绝不出现"两个字段顺序不同但意图相同"的 struct 漂移。
2. **NestJS wire 兼容至上**：字段名（`code/msg/data/requestId/timestamp`）、字段顺序、`camelCase` 命名、`msg` 不是 `message`——在整个 web/app 前端切换前不得动。
3. **显式优于隐式**：每个 `AppError` variant 有明确 HTTP 状态码、明确构造器、明确适用场景；每个 `ResponseCode` 常量有明确编号段归属；每个 i18n key 有明确命名空间。
4. **失败面有限**：任何新增 AppError / ResponseCode / i18n key / validator 都走规定的注册流程，不得私造"只在我这个模块用一次"的变体。
5. **死代码即时清理**：无调用者的 API 必须删除；不得以"将来可能要用"为由保留。`AppError::Business.data` / `AppError::business_with_params` / `ApiResponse::with_code`（非 SUCCESS 路径）都是 v1.0 要清理的遗留。

---

## 2. 类型契约

### 2.1 `ApiResponse<T>`（wire envelope）

```rust
// framework/src/response/envelope.rs
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResponse<T> {
    pub code: i32,
    pub msg: String,
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub timestamp: String,
}
```

**契约**：

- **必须** 是整个应用**唯一**的 HTTP 响应 body 结构体；成功路径和错误路径都必须序列化为这个形状
- **不得** 另外定义 `ErrorBody` 或等价的 "错误专用" struct（v0 有这个重复，v1.0 必须合并，见 §12 gap）
- **必须** `camelCase` 序列化；字段名严格为 `code` / `msg` / `data` / `requestId` / `timestamp`
- **必须** `data: Option<T>`，允许 `None`（配合 `skip_serializing_if` 在 wire 上省略）
- **必须** `request_id: Option<String>` + `skip_serializing_if = "Option::is_none"`（无 context 时不序列化该字段，与 NestJS 行为一致）
- **必须** 只 derive `Debug, Serialize`，**不得** derive `Clone / Deserialize`（它是出站类型，无反序列化需求）
- **不得** 新增字段除非同时在 NestJS 端加同名字段并通过 web/app 回归测试

### 2.2 `ApiResponse<T>` 构造入口

只允许三个构造函数，**不得** 直接用字段 literal 构造（防止 `timestamp` / `request_id` 漏填）：

```rust
impl<T> ApiResponse<T> {
    /// 200 SUCCESS + data
    pub fn ok(data: T) -> Self;
}

impl ApiResponse<()> {
    /// 200 SUCCESS + 无数据（create/update/delete 等写操作）
    pub fn success() -> Self;
}
```

**不得** 提供 `ApiResponse::with_code(non_success_code, ...)` 入口——错误响应**必须** 通过 `AppError::IntoResponse` 产出，不允许 handler 手工拼装非 SUCCESS 的 `ApiResponse`。

v0 曾有 `with_code` 方法，v1.0 必须删除（0 调用点，见 §12）。

### 2.3 `AppError` 枚举

```rust
// framework/src/error/app_error.rs
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("business error [{code}]")]
    Business { code: ResponseCode },   // ← 注意：v1.0 删除 params/data 字段

    #[error("authentication error [{code}]")]
    Auth { code: ResponseCode },

    #[error("forbidden [{code}]")]
    Forbidden { code: ResponseCode },

    #[error("validation error")]
    Validation { errors: Vec<FieldError> },

    #[error(transparent)]
    Internal(anyhow::Error),  // ← 注意：v1.0 移除 #[from] 隐式转换
}
```

**契约**：

- 五个 variant 是**完整的 closed set**——新增 variant 必须修订本规范
- **必须** 保持 `#[derive(Debug, thiserror::Error)]`，**不得** 手写 `impl Error`
- **必须** 通过 `AppError::business(code)` / `AppError::auth(code)` / `AppError::forbidden(code)` 构造业务错误变体，**不得** 用字段 literal
- **必须** 移除 `Internal(#[from] anyhow::Error)` 的 `#[from]` 属性。`?` 操作符会通过 `From` 自动把 `anyhow::Error` 转成 `AppError::Internal`，**绕过**显式 `.into_internal()` 决策点。去掉 `#[from]` 后，每个 `anyhow::Error → AppError` 都必须走 `IntoAppError::into_internal()`，让"何时把业务错误当 500"成为显式选择
- **不得** 给 `Business` 保留 `params` 或 `data` 字段——v0 两者都是 0 调用点死代码，v1.0 必须删除
- **v1 不支持** 带参数的业务错误消息。ACCOUNT_LOCKED 的 `{minutes}` 占位符在 v1.0 保持"fmt 后仍是 `{minutes}`"的 latent 形态；启用账号锁定时走 v1.1 参数化方案（见 §10）
- **不得** 新增 `#[from]` 隐式转换（比如 `#[from] sqlx::Error`）——任何外部 error 到 `AppError` 都必须走显式 trait

### 2.4 `FieldError`（validation 明细项）

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldError {
    pub field: String,
    pub message: String,
}
```

**契约**：

- **必须** 是 `AppError::Validation { errors: Vec<FieldError> }` 的唯一明细项类型
- **必须** 只在 framework crate 内构造（extractors + error::IntoResponse），**不得** 由 modules / app 层构造
- `field` 格式见 §7.1 规范
- **必须** derive `Serialize` 以便直接作为响应 `data` 字段的元素
- **不得** derive `Deserialize`（出站类型，与 ApiResponse 一致）
- **v1.0**：保持 `Clone` derive 直到 §12 gap 清理（`validator` crate 的 FieldError 需要重构为 `owned`）

### 2.5 `ResponseCode`（业务码 newtype）

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResponseCode(pub i32);
```

**契约**：

- **必须** 是 newtype，**不得** 改成 enum（扩展性：新增常量不触发 exhaustive match）
- **必须** `#[serde(transparent)]` 使 JSON 编码为原始数字而非 `{"0": 200}` 包装
- `as_i32()` 和 `is_success()` 是唯一允许的观察函数
- **必须** 有 `Display` impl（用于 i18n key 查找），实现为 `write!(f, "{}", self.0)`
- **必须** 有 `From<i32>` 和 `From<ResponseCode> for i32` 双向转换
- **不得** 有 `Default` impl——业务码必须是显式选择

### 2.6 `ResponseCode` 常量编号段

```
200          — Success（HTTP 对齐，保留唯一）
400-499      — HTTP 客户端错误
500-599      — HTTP 服务器错误
1000-1029    — 通用业务（DATA_NOT_FOUND、DUPLICATE_KEY 等）
1030-1099    — 预留通用业务
2000-2039    — 认证 / 登录 / session
2040-2099    — 预留认证
3000-3029    — 用户模块
3030-3099    — 预留用户
4000-4029    — 租户模块
4030-4099    — 预留租户
5000-5039    — 文件模块
5040-5099    — 预留文件
6000-6009    — 第三方集成错误
6010-6099    — 预留第三方
7000-7099    — system 管理模块
7100-7199    — message 模块
7200-7299    — （保留未来模块）
8000+        — （保留）
```

**规则**：

- **必须** 严格按段位分配新常量，跨段借用一律拒绝
- **必须** 在本文档 §2.6 同步记录新段位，PR 必须同时更新本规范
- **必须** 在 `framework/src/response/codes.rs` 文件内顶部的编号注释里标注段位
- **不得** 在同一段位里留"空洞号码"（比如 1002 之后跳到 1005），除非注释说明为什么
- **必须** 为每个新 `ResponseCode` 常量同步添加两个 i18n 条目（zh-CN + en-US）
- **不得** 使用非官方文档之外的数字（例如 `ResponseCode(9999)` 字面量）

---

## 3. 分层责任

| 层                         | 持有类型                           | 职责                                                                                                                                         | 不得做                                                           |
| -------------------------- | ---------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------- |
| **Handler**                | `ApiResponse<T>`、`AppError`       | 返回 `Result<ApiResponse<T>, AppError>`，出错时 `?` 传播                                                                                     | 自己包装 `Json(...)`、自己选 status code、自己拼 wire shape      |
| **Service**                | `Result<_, AppError>`              | 用 `IntoAppError::into_internal()` / `BusinessCheckOption::or_business()` / `BusinessCheckBool::business_err_if()` 扩展 trait 做显式错误转换 | 用 `?` 把 anyhow 隐式转 AppError（v1.0 后 `#[from]` 会被移除）   |
| **Repo**                   | `anyhow::Result<_>`                | 只返 `anyhow::Error`，上下文通过 `.context(...)` 附加                                                                                        | 返回 `AppError`、构造业务错误、感知 HTTP 状态码                  |
| **Framework (AppError)**   | `AppError::IntoResponse`           | 把 AppError 转成 `ApiResponse<Value>`，查 i18n，走统一序列化                                                                                 | 暴露两份 wire struct、自己选 camelCase 规则、绕过 RequestContext |
| **Framework (extractors)** | `ValidatedJson` / `ValidatedQuery` | 把 `ValidationErrors` 映射成 `AppError::Validation { Vec<FieldError> }`                                                                      | 放行未校验的数据、调 service/repo                                |

**严格禁止**：

- Handler 直接构造 `Json(...)` 或任何 `IntoResponse` impl（除 `ApiResponse<T>` 和 `AppError` 之外）
- Repo 把 `AppError` 当返回值
- Service 用 `?` 把 repo 的 `anyhow::Error` 隐式升级为 `AppError::Internal`（v1.0 强制显式 `.into_internal()`）
- Framework 以外的 crate 构造 `FieldError`

---

## 4. 端点实现规范

### 4.1 Handler 签名

```rust
async fn create_user(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateUserDto>,
) -> Result<ApiResponse<dto::UserDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}
```

**规则**：

- **必须** 返回类型为 `Result<ApiResponse<T>, AppError>`，T 为响应 DTO
- **必须** 用 `ApiResponse::ok(data)` 或 `ApiResponse::success()` 构造成功响应
- **必须** 用 `?` 传播 `AppError`，**不得** 匹配错误后手工转换
- **必须** 提取器只使用 `State<AppState>` / `Path<_>` / `ValidatedJson<T>` / `ValidatedQuery<T>`，**不得** 直接用原生 `Json<_>` / `Query<_>`（绕过 validator）

### 4.2 Service 层错误转换

```rust
pub async fn create(state: &AppState, dto: CreateUserDto) -> Result<UserDetailResponseDto, AppError> {
    // repo 返 anyhow::Result —— 必须显式转 AppError
    let unique = UserRepo::verify_user_name_unique(&state.pg, &dto.user_name)
        .await
        .into_internal()?;

    // 业务判断 —— 用 BusinessCheckBool
    (!unique).business_err_if(ResponseCode::DUPLICATE_KEY)?;

    // Option 转业务错误 —— 用 BusinessCheckOption
    let user = UserRepo::find_by_username(&state.pg, &dto.user_name)
        .await
        .into_internal()?
        .or_business(ResponseCode::USER_NOT_FOUND)?;

    // 构造业务错误 —— 用 AppError::business
    if !user.is_active() {
        return Err(AppError::business(ResponseCode::ACCOUNT_LOCKED));
    }

    // ... 业务逻辑 ...
    Ok(UserDetailResponseDto::from_entity(user, dto.role_ids))
}
```

**规则**：

- **必须** 对每一个 `anyhow::Result` 返回的调用显式 `.into_internal()`
- **必须** 用 `BusinessCheckOption::or_business(code)` 处理 `Option::None` → 业务错误
- **必须** 用 `BusinessCheckBool::business_err_if(code)` 处理布尔值 → 业务错误
- **必须** 用 `AppError::business(code)` / `AppError::auth(code)` / `AppError::forbidden(code)` 显式构造，**不得** 用字段 literal
- **不得** 用 `?` 把 repo 的 `anyhow::Error` 自动升级为 `AppError::Internal` ——该隐式路径在 v1.0 被移除
- **不得** 在 service 层查 i18n 或拼消息——消息只在 `AppError::IntoResponse` 时产生

### 4.3 Repo 层错误约定

```rust
pub async fn find_by_id(pool: &PgPool, user_id: &str) -> anyhow::Result<Option<SysUser>> {
    sqlx::query_as::<_, SysUser>(
        "SELECT ... FROM sys_user WHERE user_id = $1 LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await
    .context("find_by_id")  // ← 必须加 context 标签
}
```

**规则**：

- **必须** 返回 `anyhow::Result<T>`，**不得** 返回 `Result<T, AppError>` 或 `Result<T, sqlx::Error>`
- **必须** 对每个 sqlx 调用链末尾加 `.context("<方法名>: <动作>")`
- **不得** 在 repo 内调用 `.into_internal()` 或感知 `AppError`
- **不得** 手动构造 `anyhow::anyhow!(...)` 来表达业务错误——业务判断是 service 层职责

---

## 5. i18n 契约

### 5.1 命名空间

i18n key 的**命名空间**严格分为两类，互不相交：

1. **数字 key**：`"200"`, `"400"`, `"1001"`, `"2003"`, ...
   - **必须** 对应一个 `ResponseCode` 常量
   - **必须** 在 `codes.rs` 有对应的 `pub const`
   - **必须** 在 zh-CN.json 和 en-US.json **都**有条目
   - 用于：`AppError::Business / Auth / Forbidden / Internal` 的消息解析

2. **点分 key**：`"valid.range"`, `"valid.length"`, `"valid.required"`, ...
   - **必须** 以 `"valid."` 前缀开头（v1.0 唯一允许的非数字命名空间）
   - **必须** 对应一个 validator crate 的错误 `code`（如 `range`、`length`、`required`）
   - **必须** 在 zh-CN.json 和 en-US.json **都**有条目
   - 用于：`AppError::Validation → FieldError.message` 的消息解析
   - `FieldError.message` 在 wire 上展示的是**查表后**的文本，不是原始 validator code

**不得** 引入其他命名空间（如 `"menu.xxx"` / `"toast.xxx"` / `"hint.xxx"`）。v1.0 只允许 `<数字>` 和 `valid.<code>` 两种。

### 5.2 占位符约定

```json
{
  "2003": "账号已锁定，请 {minutes} 分钟后重试",
  "valid.range": "字段值超出允许范围（应在 {min} 到 {max} 之间）"
}
```

**规则**：

- 占位符格式**必须** 为 `{name}`，和 NestJS `i18next` 保持一致
- **必须** 使用 `snake_case` 或 camelCase 单个标识符作为占位符名（`{minutes}`、`{minRole}`，**不得** 用 `{min-role}` 或 `{min role}`）
- ✅ **v1.1 已实施**（2026-04-11）：validator 的 `range` 等约束带 `{min}` / `{max}` / `{value}` 占位符时，在 wire 响应的 `data[n].message` 字段中**已经**被替换成实际数值。`FieldError` 新增 `params: HashMap<String, serde_json::Value>`（`#[serde(skip)]`，框架内部），`AppError::Validation → IntoResponse` 在查到 i18n 翻译后内联执行替换
- **不得** 在 `{...}` 外使用其他语法（如 `%s` / `${name}`）
- **业务错误（非 validator）参数化**仍未解决：`ACCOUNT_LOCKED` 的 `{minutes}` 至今保持字面量，因为 `AppError::business(code)` 构造器本身没有 params 通道。触发条件（账号锁定实装）未满足时不动

### 5.3 i18n 覆盖

**必须** 有一条 framework 层测试断言：对每个 `ResponseCode::*` 常量，zh-CN.json 和 en-US.json **都**有对应条目。v1.0 必须加这条测试（见 §12 gap）。

测试伪代码：

```rust
#[test]
fn every_response_code_has_i18n_entries_in_all_langs() {
    let codes: &[ResponseCode] = &[
        ResponseCode::SUCCESS,
        ResponseCode::BAD_REQUEST,
        // ... 手动列出所有 const（因为 newtype 无法迭代）
    ];
    for code in codes {
        for lang in ["zh-CN", "en-US"] {
            let msg = get_message(*code, lang);
            assert!(
                !msg.starts_with("["),  // 没有 fallback 到 [NNN] sentinel
                "missing i18n entry for {:?} in {}",
                code, lang
            );
        }
    }
}
```

**不得** 用反射 / build script 自动收集——维护显式列表是规范的一部分，新增 ResponseCode 必须同时更新测试列表，这是一个有意的"强制提醒"。

### 5.4 缺失 key 的行为

- `get_message(code, lang)` 找不到条目时：**必须** emit `tracing::warn!` + 返回 `"[{code}]"` sentinel（不得 panic，不得返回空字符串）
- `get_by_key(key, lang)` 找不到条目时：**必须** 返回 `None`，由调用方决定 fallback 策略（`AppError::Validation → IntoResponse` 中 fallback 为原始 validator code + warn log）

---

## 6. Validation 契约

### 6.1 `FieldError.field` 路径格式

v1.0 统一使用以下格式：

| 来源                          | field 值                                   | 示例              |
| ----------------------------- | ------------------------------------------ | ----------------- |
| 顶层字段 validator 失败       | 字段名（snake_case，**保留 Rust 字段名**） | `"user_name"`     |
| `#[serde(flatten)]` 嵌套失败  | `"<outer>.<inner>"` 点分路径               | `"page.page_num"` |
| `Vec<T>` 列表内嵌 struct 失败 | `"<field>[<idx>]"` 方括号索引              | `"menu_ids[3]"`   |
| JSON 反序列化失败（body）     | 字面量 `"body"`                            | `"body"`          |
| Query string 反序列化失败     | 字面量 `"query"`                           | `"query"`         |

**v1.0 已知分歧**：当前代码使用的是**snake_case Rust 字段名**（`page_num` 而非 `pageNum`），但响应 body 里的其他地方用 `camelCase`（`pageNum`）。v1.0 **必须** 统一为 camelCase：见 §12 gap。

**规则**：

- 嵌套路径分隔符**必须** 是 `.`，**不得** 是 `/` / `:` / `->`
- 列表索引**必须** 是 `[N]` 方括号，**不得** 是 `.N` 或 `(N)`
- 格式**不得** 有其他变体

### 6.2 Validator code → i18n key 映射

- Validator crate 生成的每个 `ValidationError` 都有一个 `code: Cow<'static, str>`（如 `"range"`, `"length"`）
- 映射规则：**必须** 为 `format!("valid.{}", error.code)` — 转成 i18n key 后查表
- 如果查表失败：**必须** fallback 为原始 validator code（保证前端至少看到 debug 信号），同时 emit `tracing::warn!(i18n_key = %key, lang = %lang, ...)`

### 6.3 自定义 validator 函数归属

当前（v0）：自定义 validator `validate_status_flag` / `validate_sex_flag` 住在 `modules::domain::validators`。

**v1.0 建议**（非强制）：常用业务 validator（`status_flag`, `sex_flag`, `yes_no_flag`, `positive_i32`）**应当** 提升到 `framework::validation::flags` 模块，供所有 modules 复用。

**v1.0 强制**：

- 自定义 validator **必须** 通过 `#[validate(custom = "fn_name")]` 接入
- 自定义 validator **必须** 返回 `Result<(), ValidationError>`，错误 code 用 snake_case，如 `ValidationError::new("status_flag")`
- i18n 条目**必须** 同步添加 `valid.<code>`（例如 `"valid.status_flag": "状态必须是 0（启用）或 1（禁用）"`）

### 6.4 Validator 数组 / nested 遍历

当前 `ValidatedJson::collect_errors` 已正确处理 `ValidationErrorsKind::Field / Struct / List` 三种——**不得** 新增第四种类型除非 validator crate 升级。

---

## 7. 禁止模式

| 模式                                                                           | 为什么禁止                                   | 替代方案                                        |
| ------------------------------------------------------------------------------ | -------------------------------------------- | ----------------------------------------------- |
| Handler 直接 `Json(body).into_response()`                                      | 绕过 envelope，wire 漂移                     | 返回 `Result<ApiResponse<T>, AppError>`         |
| Handler 返回 `Result<T, E>` 非 `AppError` 变体                                 | 不会被 axum 正确映射                         | 用 `AppError` 作为唯一 error 类型               |
| `Internal(#[from] anyhow::Error)`                                              | `?` 隐式升级跳过显式决策点                   | 移除 `#[from]`，强制 `.into_internal()`         |
| `AppError::Business { code, params: Some(...), data: Some(...) }` 字段 literal | 绕过规范构造器，容易漏 variant               | 只用 `AppError::business(code)` 构造器          |
| `ApiResponse::with_code(non_success, ...)`                                     | 错误响应必须走 AppError，不允许 handler 绕过 | 删除 `with_code`，只保留 `ok/success`           |
| 两份 wire struct（`ApiResponse` + `ErrorBody`）                                | drift 风险                                   | v1.0 合并为一份                                 |
| 状态码 match 散落 `status_code()` / `IntoResponse` / tests 三处                | exhaustive 漏检                              | 用单源映射表或常量方法                          |
| `AppError::Business.data` 携带业务 payload                                     | 0 调用者，纯死字段                           | 删除；成功路径返回数据走 `ApiResponse<T>`       |
| `business_with_params` 保留                                                    | 0 调用者；将来参数化走 v1.1 结构化方案       | 删除；锁定实现时恢复                            |
| 直接字段 literal 构造 `FieldError` 在 framework 外                             | 破坏 DAO/wire 隔离                           | 只在 extractors / AppError::IntoResponse 内构造 |
| 新增 `AppError` variant 不改本规范                                             | 闭合集合失效                                 | 先改本规范 §2.3 再改代码                        |
| 新增 `ResponseCode` 常量不同步 i18n JSON                                       | 覆盖测试会抓但 CI 晚于 review                | PR 同时改 codes.rs + 两份 JSON + 测试列表       |
| i18n key 不是数字或 `valid.xxx`                                                | 命名空间膨胀                                 | 只用规定的两个命名空间                          |
| 占位符用 `%s` / `${}`                                                          | 和 NestJS i18next 不兼容                     | 只用 `{name}`                                   |
| 在 repo 里 `.into_internal()`                                                  | 破坏分层                                     | repo 返 `anyhow::Result`，service 层转          |
| 自定义 handler 全局 error layer（tower middleware）                            | 重复 `AppError::IntoResponse` 职责           | 走统一入口                                      |

---

## 8. 可观察性契约

### 8.1 Error 路径的 tracing

- **必须** 对 `AppError::Internal` 在 `IntoResponse` 里 emit `tracing::error!(error = ?e, "internal error")`——这是生产调试的唯一入口
- **不应** 对 `AppError::Business / Auth / Forbidden` emit `tracing::error`——它们是预期路径，不是 bug
- **可以** 对 `AppError::Validation` emit `tracing::debug!` 记录 field 列表
- **必须** 对 i18n 缺失 emit `tracing::warn!`，带 `code` / `lang` / `key` 字段
- **不得** 在错误路径打印密码、token、session 内容、PII

### 8.2 Response 路径的 span 字段

v1.0 不强制响应层 tracing（成功响应走 axum TraceLayer，已经有 method/path/status）。v1.1 可能加 `response_size` / `marshal_ms` 指标。

---

## 9. 禁止模式（额外：常见误用陷阱）

### 9.1 `?` 跨层错误泄漏

**反面**：

```rust
// service 层
let user = repo.find_by_id(&id).await?;   // ← 这里 repo 返 anyhow::Result，?
                                           //   通过 From 隐式转 AppError::Internal
```

**v1.0 强制**：移除 `AppError::Internal` 的 `#[from]` 后上述代码编译失败，必须改为：

```rust
let user = repo.find_by_id(&id).await.into_internal()?;
```

**为什么重要**：`into_internal()` 是"我承认这是 500 不是业务错"的显式决策点；`?` + `#[from]` 让这个决策隐形。

### 9.2 Handler 返回 Option 或原始类型

**反面**：

```rust
async fn info(...) -> Option<dto::UserInfo> { ... }
```

会编译但 axum 把 `None` 变成空 body 200，客户端收到的不是 wire envelope。

**v1.0 强制**：handler 返回类型**必须** 是 `Result<ApiResponse<T>, AppError>`。

### 9.3 service 直接用字段 literal 构造 business error

**反面**：

```rust
return Err(AppError::Business {
    code: ResponseCode::DATA_NOT_FOUND,
    // 编译错误（v1.0 后），因为 v1.0 删了 params/data 字段
    params: None,
    data: None,
});
```

**v1.0 强制**：用 `AppError::business(code)` 构造器，不得用字段 literal。

---

## 10. 演化路线

| Phase                       | 内容                                                                                                                                                                                                           | 触发条件                                         | 状态                        |
| --------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------ | --------------------------- |
| **v1.0**                    | 合并 envelope、删死代码、移除 `#[from]`、i18n 覆盖测试、wire 回归测试                                                                                                                                          | 立即                                             | ✅ 2026-04-11               |
| **v1.1 (validator 侧部分)** | `get_by_key_with_json_params` + `FieldError.params` (`#[serde(skip)]`) + `collect_errors` 传参 + IntoResponse 内联替换——**仅解决 validator 路径**（`range` / `length` 等），业务错误路径的 params 入口仍然关闭 | 随 pagination v1.1 一起落地                      | ✅ 2026-04-11               |
| **v2.0 (业务错误参数化)**   | `AppError::business_with_i18n_params(code, HashMap)` 或等价接口，打通**非 validator** 路径的 `{placeholder}` 替换（`ACCOUNT_LOCKED` 的 `{minutes}` 等）                                                        | 账号锁定实装 / 首个非 validator 业务错误需要参数 | ⏳ 触发器未满足             |
| **v1.2**                    | Field path camelCase 统一（`user_name` → `userName`），完善 `FieldError.field` 规范                                                                                                                            | Vue web 客户端报字段名不一致                     | ⏳ 触发器未满足             |
| **v2.1**                    | Error envelope `data` 字段承载业务 payload（现在被 §2.3 禁止）                                                                                                                                                 | 出现 "部分成功 + 详情" 的合理需求                | ⏳ wire break，触发器未满足 |
| **v2.2**                    | 结构化错误 trace id 链路（correlation id 跨服务）                                                                                                                                                              | 引入第二个 Rust 服务或 mesh 架构                 | ⏳ 触发器未满足             |
| **v3.0**                    | OpenAPI / utoipa 导出 schema                                                                                                                                                                                   | 引入 `utoipa` 依赖                               | ⏳ 触发器未满足             |
| **v3.1**                    | 流式响应 primitive（SSE / NDJSON）                                                                                                                                                                             | 出现长连接或流式导出需求                         | ⏳ 触发器未满足             |

**触发器表是 governance 工具**：v2+ 的事项在触发条件未满足前**不得** 提前做。

---

## 11. 新增端点 / 新增错误的样板

### 11.1 新增一个业务 ResponseCode

**Step 1**：选段位。在 `framework/src/response/codes.rs` 顶部注释表里找到所属段位。

**Step 2**：追加常量。

```rust
// --- 3000-3029 user ---
pub const USER_NOT_FOUND: Self = Self(3001);
pub const INVALID_CREDENTIALS: Self = Self(3002);
pub const USER_DEPARTMENT_MISMATCH: Self = Self(3003);  // ← 新增
```

**Step 3**：同步 i18n（两份 JSON）。

```json
// server-rs/i18n/zh-CN.json
"3003": "用户不属于该部门"

// server-rs/i18n/en-US.json
"3003": "User does not belong to the specified department"
```

**Step 4**：更新 spec §2.6 段位描述（本规范）。

**Step 5**：更新 `every_response_code_has_i18n_entries_in_all_langs` 测试的显式常量列表。

**Step 6**：在 service 里使用：

```rust
return Err(AppError::business(ResponseCode::USER_DEPARTMENT_MISMATCH));
```

**Step 7**：跑 `cargo test -p framework every_response_code_has_i18n_entries` 验证覆盖。

### 11.2 新增一个 Handler

**Step 1**：定义 DTO（§4.2 pagination spec 的规则适用）。

**Step 2**：Handler 签名：

```rust
async fn create_widget(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateWidgetDto>,
) -> Result<ApiResponse<dto::WidgetDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}
```

**Step 3**：Service 用显式错误转换：

```rust
pub async fn create(state: &AppState, dto: CreateWidgetDto) -> Result<WidgetDetailResponseDto, AppError> {
    let exists = WidgetRepo::find_by_name(&state.pg, &dto.name)
        .await
        .into_internal()?;
    exists.is_some().business_err_if(ResponseCode::DUPLICATE_KEY)?;

    let widget = WidgetRepo::insert(&state.pg, dto.into())
        .await
        .into_internal()?;

    Ok(WidgetDetailResponseDto::from_entity(widget))
}
```

**Step 4**：Repo 返 `anyhow::Result` 加 `.context(...)`。

---

## 12. v1.0 与当前代码的 gap

**v1.0 实施完成时间**：2026-04-11

| 规范条目                          | 原状态                                     | 原改动方案                                                                             | 状态                                                                                            |
| --------------------------------- | ------------------------------------------ | -------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| §2.1 单一 wire envelope           | `ApiResponse<T>` + `ErrorBody` 两份 struct | 合并为 `ApiResponse<serde_json::Value>`                                                | ✅ v1.0 (Task 3)                                                                                |
| §2.2 只保留 `ok/success` 构造入口 | `ApiResponse::with_code` 存在但 0 调用者   | 删除 `with_code`                                                                       | ✅ v1.0 (Task 1)                                                                                |
| §2.3 删除 `Business.params/data`  | 两字段都是 0 调用者                        | 删除字段 + 简化 `Business { code }`                                                    | ✅ v1.0 (Task 2)                                                                                |
| §2.3 删除 `#[from] anyhow::Error` | v0 有 `#[from]` 隐式转换                   | 移除 `#[from]`                                                                         | ✅ v1.0 (Task 4) — 0 call sites affected（pre-verified by grep）                                |
| §2.3 删除 `business_with_params`  | 定义存在、0 调用者                         | 删除                                                                                   | ✅ v1.0 (Task 1)                                                                                |
| 状态码映射单源                    | 散落 3 处（fn + match + test）             | 合并为 match + exhaustive-check test                                                   | ✅ v1.0 (Task 5)                                                                                |
| §5.3 i18n 覆盖测试                | 无                                         | 新增 `every_response_code_has_i18n_entries_in_all_langs`                               | ✅ v1.0 (Task 6) — 发现并修复了 4 个缺失条目（429 / 1000 / 4001 / 4002 在 zh-CN 和 en-US 都缺） |
| Wire 回归测试                     | 无                                         | 新增 8 个 table-driven 测试（`ApiResponse::ok` / `success` + 5 个 `AppError` variant） | ✅ v1.0 (Task 7)                                                                                |
| §5.2 占位符解析                   | ACCOUNT_LOCKED `{minutes}` 字面量泄露      | 延期                                                                                   | ⏳ v1.1（与 pagination v1.1 Task 7 协调）                                                       |
| §6.1 field 路径 camelCase 统一    | 当前 snake_case                            | 延期                                                                                   | ⏳ v1.2（需要 web/app 协调）                                                                    |

**gap 总量**：10 项中 **8 项 v1.0 已实施**，2 项延期到 v1.1 / v1.2 触发时再做。

### 12.1 v1.0 实际净影响

- **删除代码**：
  - `ApiResponse::with_code` 方法
  - `AppError::business_with_params` 方法
  - `AppError::Business.params` 字段
  - `AppError::Business.data` 字段
  - `AppError::Internal` 的 `#[from]` 属性
  - `ErrorBody` 整个 struct 定义
- **新增代码**：
  - `ApiResponse::<serde_json::Value>::error(...)` `pub(crate)` helper
  - `AppError` doc comment 说明 `Internal` 无 `From` 约定
  - `status_mapping_covers_every_variant` test（替换旧 `status_mapping`）
  - `every_response_code_has_i18n_entries_in_all_langs` test（1 个 test，19 个 code × 2 lang = 38 个断言）
  - `framework/src/response/wire_test.rs` 整文件（8 个 table-driven wire regression test）
  - 4 条缺失的 i18n 条目（429 / 1000 / 4001 / 4002）
- **测试数量**：
  - Framework：69 → 78（+9）
  - Workspace：153 → 162（+9）
- **Wire 契约变更**：**0**（byte-for-byte identical output，smoke 14/14 + 16/16 green）
- **新运行时依赖**：**0**（用 `axum::body::to_bytes` 避免 `http-body-util` dev-dep）

### 12.2 v1.0 不做的

- 不动 `FieldError.field` snake_case → camelCase（需要前端协调，放 v1.2）
- 不引入 `get_by_key_with_json_params`（pagination v1.1 plan Task 7 会做）
- 不改 `ACCOUNT_LOCKED` 的 `{minutes}` 行为（v1.1 解决）
- 不删任何 `ResponseCode::*` 常量（即使 `TOO_MANY_REQUESTS` 当前 0 调用——保留给未来限流使用）
- 不改 i18n 文件结构（仍然是 `i18n/<lang>.json` + `include_str!`）
- 不做 OpenAPI schema 导出（v3.0）

---

## 13. PR checklist

提交涉及 handler / service / AppError / ApiResponse / ResponseCode / i18n 的 PR 时**必须** 逐项自查：

**Wire 契约**：

- [ ] Handler 返回类型为 `Result<ApiResponse<T>, AppError>`
- [ ] 无直接 `Json(...).into_response()` 绕过 envelope
- [ ] 成功响应用 `ApiResponse::ok(data)` 或 `ApiResponse::success()`

**错误构造**：

- [ ] Service 对 `anyhow::Result` 调用显式 `.into_internal()`，无隐式 `?` + `#[from]`
- [ ] Option → 业务错误用 `.or_business(code)`
- [ ] bool → 业务错误用 `.business_err_if(code)`
- [ ] 业务错误用 `AppError::business(code)` / `AppError::auth(code)` / `AppError::forbidden(code)` 构造，无字段 literal

**ResponseCode 新增**：

- [ ] 遵守 §2.6 段位表
- [ ] `framework/src/response/codes.rs` 常量添加
- [ ] zh-CN.json + en-US.json **都**有条目
- [ ] `every_response_code_has_i18n_entries_in_all_langs` 测试的常量列表已更新
- [ ] 本规范 §2.6 段位表同步更新

**i18n 纪律**：

- [ ] 新 i18n key 要么是数字（对应 ResponseCode），要么以 `"valid."` 开头
- [ ] 占位符用 `{name}` 格式
- [ ] 带占位符的条目在 PR 描述里标注 "v1.0 不解析 / v1.1 参数化后生效"（如有）

**测试**：

- [ ] `cargo test --workspace` 全绿
- [ ] `cargo clippy --all-targets -- -D warnings` clean
- [ ] `cargo fmt --check` clean
- [ ] 如涉及 error / envelope 逻辑，wire 回归测试仍绿

---

## 附录 A：v1.0 明确不管的事

1. Cursor / SSE / 流式响应
2. 错误响应 `data` 字段承载业务 payload
3. OpenAPI schema 导出
4. `FieldError.field` camelCase 统一（v1.2）
5. 业务错误参数化（v1.1）
6. 跨服务 correlation id（v2.1）
7. 自定义 tower error layer
8. 错误率 metrics / 熔断
9. 全局重试 / 幂等键
10. tenant-aware 错误脱敏

每一条记录在 §10 触发器表，条件未满足**不得** 提前做。

---

## 附录 B：与 pagination spec 的关系

- `Page<T>` 是 `ApiResponse<Page<T>>` 的一个特化，**必须** 通过 `ApiResponse::ok(page)` 包装返回
- `AppError::Validation` 是 `ValidatedQuery<ListDto>` 的正常失败路径，走本规范的统一 wire shape
- pagination 的 `slow query warn` / `timeout` 走 `AppError::Internal`（通过 `with_timeout` + `.into_internal()`）
- pagination spec v1.1 的 i18n 参数化改动 (`get_by_key_with_json_params`) 和本 spec 的 §5.2 占位符策略需要**协调**——本 spec v1.0 规定 "v1.0 不解析占位符"，pagination v1.1 plan 的 Task 7 必须在实施时同步升级本 spec 为 "v1.1 解析验证器占位符"

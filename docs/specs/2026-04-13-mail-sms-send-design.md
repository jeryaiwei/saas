# Mail Send + SMS Send 设计文档

> 日期: 2026-04-13
> 状态: approved
> 端点数: 7 (mail 4 + sms 3)

## 1. 概述

为 Rust 后端补齐邮件发送和短信发送能力，对齐 NestJS 端的 `mail-send` / `sms-send` 模块。
现有 CRUD 模块（mail_account, mail_template, mail_log, sms_channel, sms_template, sms_log）已就绪，
本次新增发送层 + 日志写入 + 模板解析。

### 核心决策

| 决策 | 选型 | 理由 |
|------|------|------|
| 异步模型 | tokio::spawn + Semaphore | 零外部依赖，背压可控，管理后台场景足够 |
| 邮件发送 | lettre (同步) + spawn_blocking | 同步 SmtpTransport 最稳定，spawn_blocking 不阻塞 tokio worker |
| 短信发送 | SmsClient trait + mock 实现 | 三厂商 (aliyun/tencent/huawei) mock，后续替换真实 SDK 不动 service |
| 模板解析 | 简单字符串扫描 | 不引入 regex，`${key}` 语法与 NestJS 一致 |
| 重试 | task 内 3 次指数退避 | 等价 Bull retry，无外部队列 |
| 崩溃恢复 | 不做启动扫描 | 手动补发覆盖，YAGNI |

## 2. 端点与 API 契约

### 2.1 Mail Send (4 endpoints)

| 方法 | 路径 | 权限 | 说明 |
|------|------|------|------|
| POST | `/message/mail-send` | `message:mail-send:send` | 单封发送（异步） |
| POST | `/message/mail-send/batch` | `message:mail-send:batch` | 批量发送（≤100，异步） |
| POST | `/message/mail-send/resend/{logId}` | `message:mail-send:resend` | 重发失败记录 |
| POST | `/message/mail-send/test` | `message:mail-account:test` | 测试发送（同步等待结果） |

### 2.2 SMS Send (3 endpoints)

| 方法 | 路径 | 权限 | 说明 |
|------|------|------|------|
| POST | `/message/sms-send` | `message:sms-send:send` | 单条发送（异步） |
| POST | `/message/sms-send/batch` | `message:sms-send:batch` | 批量发送（≤100，异步） |
| POST | `/message/sms-send/resend/{logId}` | `message:sms-send:resend` | 重发失败记录 |

### 2.3 请求 DTO

```rust
// --- Mail ---
SendMailDto {
    to_mail: String,           // 收件地址
    template_code: String,     // 模板编码
    params: Option<HashMap<String, String>>,  // 模板参数
}

BatchSendMailDto {
    to_mails: Vec<String>,     // 收件地址列表，max 100
    template_code: String,
    params: Option<HashMap<String, String>>,
}

TestMailDto {
    to_mail: String,           // 收件地址
    account_id: i32,           // 指定发送账户
    title: Option<String>,     // 自定义标题（不走模板）
    content: Option<String>,   // 自定义内容（不走模板）
}

// --- SMS ---
SendSmsDto {
    mobile: String,            // 手机号
    template_code: String,     // 模板编码
    params: Option<HashMap<String, String>>,
}

BatchSendSmsDto {
    mobiles: Vec<String>,      // 手机号列表，max 100
    template_code: String,
    params: Option<HashMap<String, String>>,
}
```

### 2.4 响应

- `send` → `{ code: 200, data: { log_id: i64 } }`
- `batch` → `{ code: 200, data: { count: i32 } }`
- `resend` → `{ code: 200, data: null }`
- `test` → 同步，成功 200 / 失败带 error_msg

## 3. 架构与数据流

### 3.1 发送流程

```text
POST /message/mail-send
  │
  ▼
handler (校验 DTO)
  │
  ▼
service::send()
  ├─ 1. 查 template (by code, status=enabled, del_flag='0')
  ├─ 2. 查 account (by template.account_id, status=enabled)
  ├─ 3. 解析模板: title + content 中 ${key} → 实际值
  ├─ 4. 写 mail_log (send_status=0 SENDING，快照模板字段)
  ├─ 5. tokio::spawn + semaphore
  │     └─ execute_send()
  │        ├─ spawn_blocking { lettre SMTP 发送 }
  │        ├─ 成功 → update_status(1 SUCCESS)
  │        └─ 失败 → retry 3次指数退避 → update_status(2 FAILED, error_msg)
  └─ 6. 立即返回 { log_id }
```

SMS 流程相同，区别：
- 查 template → 查 channel（而非 account）
- 发送用 SmsClient trait (mock 实现)
- 无 spawn_blocking（HTTP API 本身异步）

### 3.2 Test 发送（同步）

```text
POST /message/mail-send/test
  │
  ▼
service::test_send()
  ├─ 1. 查 account (by account_id)
  ├─ 2. 不走模板，直接用 title/content
  ├─ 3. spawn_blocking { lettre 发送 }  ← 同步等待
  ├─ 4. 不写日志
  └─ 5. 成功返回 200 / 失败返回 MAIL_SEND_FAIL + error_msg
```

### 3.3 Resend

```text
POST /message/mail-send/resend/{logId}
  │
  ▼
service::resend()
  ├─ 1. 查 log (by id)
  ├─ 2. 校验 send_status == 2 (FAILED)
  ├─ 3. update_status(0 SENDING)，清空 error_msg
  ├─ 4. 查 account (by log.account_id)
  ├─ 5. tokio::spawn + semaphore → 用 log 中快照的模板数据直接发
  └─ 6. 返回 200
```

## 4. 模块结构（新增文件）

```text
crates/modules/src/message/
├── mail_send/              ← 新增模块
│   ├── mod.rs
│   ├── handler.rs          # 4 endpoints
│   ├── service.rs          # send / batch_send / resend / test_send
│   └── dto.rs              # SendMailDto, BatchSendMailDto, TestMailDto
│
├── sms_send/               ← 新增模块
│   ├── mod.rs
│   ├── handler.rs          # 3 endpoints
│   ├── service.rs          # send / batch_send / resend
│   ├── dto.rs              # SendSmsDto, BatchSendSmsDto
│   └── client.rs           # SmsClient trait + factory + mock (aliyun/tencent/huawei)
│
├── template_parser.rs      ← 新增 (共享 ${key} 解析)
└── mod.rs                  ← 更新 (注册 mail_send, sms_send)

crates/modules/src/domain/
├── mail_log_repo.rs        ← 扩展 (+insert, +update_status)
└── sms_log_repo.rs         ← 扩展 (+insert, +update_status)

crates/framework/src/infra/
└── smtp.rs                 ← 新增 (lettre SmtpTransport 封装)

crates/modules/src/state.rs ← 更新 (AppState +mail_semaphore, +sms_semaphore)
```

## 5. 模板解析

### 5.1 语法

与 NestJS 一致：`${variableName}`

```text
模板:  "尊敬的${userName}，验证码${code}，${minutes}分钟有效"
参数:  { "userName": "张三", "code": "123456", "minutes": "5" }
结果:  "尊敬的张三，验证码123456，5分钟有效"
```

### 5.2 API

```rust
// message/template_parser.rs

/// 提取模板中所有 ${key} 的 key 名
pub fn extract_params(content: &str) -> Vec<String>

/// 校验 params 覆盖所有必需 key
pub fn validate_params(template: &str, params: &HashMap<String, String>) -> Result<(), AppError>

/// 替换 ${key} → 值
pub fn render(template: &str, params: &HashMap<String, String>) -> Result<String, AppError>
```

实现：简单字符串扫描 `${` → `}`，不引入 regex。

## 6. SMTP 封装 (lettre)

```rust
// framework/src/infra/smtp.rs

pub struct SmtpParams {
    pub host: String,
    pub port: u16,
    pub ssl_enable: bool,
    pub username: String,
    pub password: String,   // 已解密明文
}

pub struct MailMessage {
    pub from_name: String,  // template.nickname
    pub from_mail: String,  // account.mail
    pub to_mail: String,
    pub subject: String,    // render 后的 title
    pub html_body: String,  // render 后的 content
}

/// 阻塞发送，调用方需包在 spawn_blocking 中
pub fn send_mail(smtp: &SmtpParams, msg: &MailMessage) -> Result<(), String>
```

**Cargo 依赖**: `lettre = { version = "0.11", features = ["builder", "smtp-transport"] }`

## 7. SMS Client 抽象

### 7.1 Trait

```rust
// message/sms_send/client.rs

pub struct SmsSendParams {
    pub mobile: String,
    pub signature: String,          // channel.signature
    pub api_template_id: String,    // template.api_template_id
    pub params: HashMap<String, String>,
}

pub struct SmsSendResult {
    pub success: bool,
    pub api_send_code: Option<String>,
    pub error_msg: Option<String>,
}

#[async_trait]
pub trait SmsClient: Send + Sync {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult;
}
```

### 7.2 Factory

```rust
pub fn create_client(channel_code: &str, api_key: &str, api_secret: &str)
    -> Result<Box<dyn SmsClient>, AppError>
{
    match channel_code {
        "aliyun"  => Ok(Box::new(AliyunSmsClient::new(api_key, api_secret))),
        "tencent" => Ok(Box::new(TencentSmsClient::new(api_key, api_secret))),
        "huawei"  => Ok(Box::new(HuaweiSmsClient::new(api_key, api_secret))),
        _ => Err(AppError::business(ResponseCode::SMS_CHANNEL_NOT_SUPPORTED)),
    }
}
```

### 7.3 Mock 实现（MVP）

三个 client 结构体均为 mock：打 tracing::info 日志，返回 `SmsSendResult { success: true, api_send_code: Some(mock-uuid) }`。后续替换真实 SDK 只改 trait 实现体。

## 8. Repo 扩展

### 8.1 MailLogRepo (新增方法)

```rust
/// 写发送日志，快照模板字段
pub async fn insert(executor: impl PgExecutor<'_>, params: MailLogInsertParams) -> Result<SysMailLog>

/// 更新发送状态
pub async fn update_status(executor: impl PgExecutor<'_>, id: i64, status: i32, error_msg: Option<&str>) -> Result<u64>
```

`MailLogInsertParams` 字段：user_id, user_type, to_mail, account_id, from_mail, template_id,
template_code, template_nickname, template_title, template_content, template_params

### 8.2 SmsLogRepo (新增方法)

```rust
/// 写发送日志，快照模板字段
pub async fn insert(executor: impl PgExecutor<'_>, params: SmsLogInsertParams) -> Result<SysSmsLog>

/// 更新发送状态 + api_send_code
pub async fn update_status(executor: impl PgExecutor<'_>, id: i64, status: i32, api_send_code: Option<&str>, error_msg: Option<&str>) -> Result<u64>
```

`SmsLogInsertParams` 字段：channel_id, channel_code, template_id, template_code,
mobile, content (渲染后), params (JSON)

## 9. 背压控制

### AppState 扩展

```rust
pub struct AppState {
    // ... 现有字段
    pub mail_semaphore: Arc<Semaphore>,  // permit = 10
    pub sms_semaphore: Arc<Semaphore>,   // permit = 20
}
```

所有 send 的 spawn task 在执行前 `semaphore.acquire()` 等待 permit，执行完自动释放。

### 并发上限

- Mail: 10 并发 SMTP 连接 → ~50 封/秒 (取决于 SMTP 服务商)
- SMS: 20 并发 HTTP 请求 → ~200 条/秒 (取决于 API 限速)
- 超出部分排队等待 permit，不会 OOM 或连接耗尽

## 10. 重试策略

task 内重试，3 次指数退避 (2s, 4s, 8s)：

```rust
const MAX_RETRIES: u32 = 3;

async fn execute_with_retry<F, Fut>(f: F) -> Result<(), String>
where F: Fn() -> Fut, Fut: Future<Output = Result<(), String>>
{
    for attempt in 0..MAX_RETRIES {
        match f().await {
            Ok(_) => return Ok(()),
            Err(e) if attempt < MAX_RETRIES - 1 => {
                tokio::time::sleep(Duration::from_secs(2u64.pow(attempt + 1))).await;
                tracing::warn!(attempt, error = %e, "send retry");
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
```

## 11. 错误码

| 错误码 | 常量 | 场景 |
|--------|------|------|
| 9000 | `MAIL_TEMPLATE_NOT_FOUND` | 模板不存在或已禁用 |
| 9001 | `MAIL_ACCOUNT_NOT_FOUND` | 账户不存在或已禁用 |
| 9002 | `MAIL_TEMPLATE_PARAMS_MISSING` | 模板参数缺失 |
| 9003 | `MAIL_SEND_FAIL` | 发送失败 (test 同步接口用) |
| 9010 | `SMS_TEMPLATE_NOT_FOUND` | 模板不存在或已禁用 |
| 9011 | `SMS_CHANNEL_NOT_FOUND` | 渠道不存在或已禁用 |
| 9012 | `SMS_CHANNEL_NOT_SUPPORTED` | 不支持的渠道类型 |
| 9013 | `SMS_TEMPLATE_PARAMS_MISSING` | 模板参数缺失 |
| 9020 | `BATCH_SIZE_EXCEEDED` | 批量数超过 100 |

## 12. 测试策略

### 12.1 单元测试（不依赖 DB/SMTP）

| 测试 | 覆盖 |
|------|------|
| `template_parser::extract_params` | 正常 / 空模板 / 嵌套${ / 无参数 |
| `template_parser::render` | 正常替换 / 缺失参数报错 / 空 params |
| `template_parser::validate_params` | 全覆盖 / 部分缺失 / 多余参数 OK |

### 12.2 集成测试（命中 DB，mock SMTP/SMS）

| 测试 | 覆盖 |
|------|------|
| `mail_send::send` | 创建日志 → 检查 log 存在 + send_status |
| `mail_send::batch` | 100 封 → 100 条 log |
| `mail_send::resend` | 标记 FAILED → resend → status=SENDING |
| `sms_send::send` | 同 mail |
| `sms_send::batch` | 同 mail |
| `sms_send::resend` | 同 mail |
| `mail_log_repo::insert` | 字段快照正确 |
| `sms_log_repo::insert` | 字段快照正确 |

## 13. Cargo 依赖新增

| crate | 版本 | 用途 | 加到 |
|-------|------|------|------|
| `lettre` | 0.11 | SMTP 邮件发送 | framework |
| `async-trait` | 0.1 | SmsClient trait 异步方法 | modules (如未有) |

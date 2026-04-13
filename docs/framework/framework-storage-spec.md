# 文件存储规范

> 版本: v1.0
> 状态: 已落地

## 1. StorageProvider Trait

文件存储通过 `framework::infra::storage::StorageProvider` trait 抽象，
业务代码不直接依赖任何特定存储实现。

```rust
pub trait StorageProvider: Send + Sync {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<PutResult, String>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, String>;
    async fn delete(&self, key: &str) -> Result<(), String>;
    async fn exists(&self, key: &str) -> Result<bool, String>;
    fn signed_put_url(&self, key: &str, content_type: &str, expires: u64) -> Option<SignedUploadUrl>;
}
```

### PutResult

```rust
pub struct PutResult {
    pub key: String,   // 存储键 (get/delete 用)
    pub url: String,   // 完整公开 URL (DB 存储 + 前端展示)
}
```

## 2. Provider 实现

| storage_type | Provider | 签名算法 | 配置 section |
| --- | --- | --- | --- |
| `local` (默认) | LocalStorageProvider | — | `upload.local` |
| `oss` | OssStorageProvider | HMAC-SHA1 V1 | `upload.oss` |
| `cos` | CosStorageProvider | HMAC-SHA1 V5 | `upload.cos` |

### 配置示例

```yaml
upload:
  storage_type: "local"          # local / oss / cos
  max_file_size_mb: 100
  allowed_types: []              # 空 = 全部允许
  blocked_extensions: [exe, bat, cmd, sh, ps1, msi, dll, com, scr]
  local:
    upload_dir: "./uploads"
    domain: "http://localhost:18080"
  # oss:
  #   access_key_id: "xxx"
  #   access_key_secret: "xxx"
  #   bucket: "my-bucket"
  #   region: "oss-cn-hangzhou"
  #   location: "uploads"
  # cos:
  #   secret_id: "xxx"
  #   secret_key: "xxx"
  #   bucket: "my-bucket-12345"
  #   region: "ap-guangzhou"
  #   location: "uploads"
```

## 3. DB 字段约定

| 字段 | 存储内容 | 用途 |
| --- | --- | --- |
| `sys_upload.url` | 完整公开 URL | 前端展示、OSS/COS 重定向下载 |
| `sys_upload.new_file_name` | storage key | `get()` / `delete()` 操作 |
| `sys_upload.storage_type` | `local` / `oss` / `cos` | 标识存储位置 |

**对齐 NestJS**: DB url 字段存完整 URL，不存 key。

## 4. Storage Key 格式

```text
{tenant_id}/{YYYY/MM/DD}/{uuid}_{filename}
例: 000002/2026/04/13/a1b2c3d4_photo.jpg
```

- `tenant_id` 在路径中实现租户隔离
- 日期目录按天分桶
- UUID 前缀防重名
- 文件名经过 `SafeFileName::parse` 清洗

## 5. 上传端点

| 端点 | 说明 | 存储支持 |
| --- | --- | --- |
| `POST /common/upload` | 服务端中转上传 (multipart) | local + oss + cos |
| `GET /common/upload/{uploadId}` | 文件下载 | local + oss + cos |
| `POST /common/upload/client/authorization` | 客户端直传授权 | oss + cos (local 返回 5050) |
| `POST /common/upload/client/callback` | 客户端直传回调 | oss + cos |

## 6. 上传流程

### 6.1 服务端中转

```text
客户端 → POST /common/upload (multipart)
  → SafeFileName::parse (清洗文件名)
  → validate_file (大小 + MIME + 扩展名)
  → MD5 秒传检查 → 命中则复制 DB 记录返回
  → storage.put(key, bytes, mime) → PutResult{key, url}
  → DB insert (url=完整URL, new_file_name=key)
  → 返回 { uploadId, url, fileMd5, instantUpload }
```

### 6.2 客户端直传

```text
① POST /client/authorization
   → 校验 + 秒传检查
   → storage.signed_put_url(key, mime, 600s) → SignedUploadUrl
   → Redis 存 UploadRegistration (10min TTL)
   → 返回 { uploadToken, signedUrl, key }

② 客户端 PUT signedUrl → 直传 OSS/COS (不经过服务端)

③ POST /client/callback { uploadToken }
   → Redis 读 UploadRegistration (强类型)
   → 校验: 空 token / 已使用 / 租户一致 / 文件存在
   → DB insert
   → Redis 标记 uploaded (1min TTL)
   → 返回 { uploadId, url, fileName }
```

## 7. 安全规范

| 措施 | 说明 |
| --- | --- |
| SafeFileName | 剥离路径分隔符、null 字节、限制 200 字符 |
| safe_path | 过滤 `../`、starts_with 基目录校验 (local) |
| DefaultBodyLimit | 上传路由 101MB 限制 (Axum 层) |
| 扩展名黑名单 | exe/bat/cmd/sh/ps1 等可执行文件 |
| MIME 白名单 | 可选，空 = 全部允许 |
| 租户校验 | callback 验证 token 的 tenantId 与当前请求一致 |
| token 单次使用 | callback 后标记 uploaded，1min TTL 防重放 |
| OssConfig/CosConfig Debug | 脱敏 access_key/secret |

## 8. 新增 Provider 指南

添加新存储后端 (如 S3) 的步骤:

1. 创建 `crates/modules/src/system/upload/s3_storage.rs`
2. 实现 `StorageProvider` trait (5 个方法)
3. 在 `crates/framework/src/config/mod.rs` 添加 `S3Config`
4. 在 `UploadConfig` 添加 `pub s3: Option<S3Config>`
5. 在 `crates/app/src/main.rs` 的 `match storage_type` 添加 `"s3"` 分支
6. 在 `upload/mod.rs` 添加 `pub mod s3_storage;`

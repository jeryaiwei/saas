# Redis 操作规范

> 版本: v1.0
> 状态: 已落地

## 1. RedisExt Trait

所有业务模块的 Redis 操作**必须**通过 `framework::infra::redis::RedisExt` trait，
禁止直接使用 `redis::cmd()`。

```rust
use framework::infra::redis::RedisExt;

// ✅ 正确
pool.set_ex("key", &my_struct, 600).await?;
let val: Option<MyStruct> = pool.get_json("key").await?;

// ❌ 禁止
redis::cmd("SETEX").arg("key").arg(600).arg(json).query_async(&mut conn).await?;
```

## 2. API 一览

| 方法 | Redis 命令 | 签名 | 用途 |
| --- | --- | --- | --- |
| `set_ex` | SETEX | `(&self, key, &T, ttl) -> Result<()>` | JSON 序列化写入 + TTL |
| `set_ex_raw` | SETEX | `(&self, key, &str, ttl) -> Result<()>` | 原始字符串写入 + TTL |
| `get_json` | GET | `(&self, key) -> Result<Option<T>>` | JSON 反序列化读取 |
| `get_raw` | GET | `(&self, key) -> Result<Option<String>>` | 原始字符串读取 |
| `exists` | EXISTS | `(&self, key) -> Result<bool>` | 键是否存在 |
| `incr` | INCR | `(&self, key) -> Result<i64>` | 自增 |
| `expire` | EXPIRE | `(&self, key, ttl) -> Result<()>` | 设置 TTL |
| `incr_ex` | INCR+EXPIRE | `(&self, key, ttl) -> Result<i64>` | 自增并设 TTL |
| `del` | DEL | `(&self, key) -> Result<()>` | 删除 |

## 3. 使用规则

### 3.1 存储 JSON 结构

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MyData { ... }

// 写
pool.set_ex("prefix:id", &data, 600).await?;

// 读 (强类型)
let data: Option<MyData> = pool.get_json("prefix:id").await?;
```

- **必须**定义独立 struct，禁止用 `serde_json::Value` 弱类型
- struct 必须 `#[serde(rename_all = "camelCase")]` 保持 NestJS 兼容

### 3.2 存储原始值

验证码、黑名单时间戳等非 JSON 值用 `set_ex_raw` / `get_raw`：

```rust
pool.set_ex_raw("captcha:uuid", "1234", 300).await?;
let code: Option<String> = pool.get_raw("captcha:uuid").await?;
```

### 3.3 存在性检查

优先用 `exists`（比 `get_raw().is_some()` 更高效，不传输值）：

```rust
let blocked = pool.exists("blacklist:token-uuid").await?;
```

### 3.4 计数器

```rust
// 自增并设 TTL (token version、限流计数等)
let new_ver = pool.incr_ex("user_version:user_id", 604800).await?;

// 仅自增 (已有 TTL 的 key)
let count = pool.incr("rate_limit:ip").await?;

// 仅设 TTL
pool.expire("rate_limit:ip", 60).await?;
```

## 4. 何时允许原始 redis::cmd

以下场景**允许**直接使用 `redis::cmd()`：

| 命令 | 场景 | 原因 |
| --- | --- | --- |
| SCAN | 游标迭代 (monitor) | 需要持续使用同一连接 |
| INFO / DBSIZE | 服务器信息查询 | 管理命令，非业务操作 |
| FLUSHDB | 清空数据库 | 管理命令 |
| TTL | 查询剩余 TTL | 低频管理查询 |
| 批量 DEL | SCAN + 批量删除 | 需要同一连接内操作 |

## 5. 连接管理

`RedisExt` 每次调用内部获取连接并归还，调用方无需管理连接生命周期。

如果需要在同一连接上执行多个命令（如 SCAN 循环），直接使用 `pool.get().await` 获取连接。

## 6. 错误处理

所有 `RedisExt` 方法返回 `anyhow::Result`。在 service 层通过 `.into_internal()?` 转为 `AppError::Internal`：

```rust
pool.set_ex(&key, &data, ttl).await.into_internal()?;
let reg: MyStruct = pool.get_json(&key).await.into_internal()?
    .ok_or_else(|| AppError::business(ResponseCode::NOT_FOUND))?;
```

## 7. Key 命名约定

- 前缀从 `RedisKeyConfig` 读取（配置化，NestJS 兼容）
- 格式: `{prefix}{id}`，如 `login_token_session:uuid-xxx`
- 业务模块自定义 key 用 `模块:功能:id` 格式，如 `upload_registration:token-uuid`

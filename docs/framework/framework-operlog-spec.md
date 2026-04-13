# 操作日志 (Operlog) 框架规范 v1.1

**生效日期**：2026-04-13
**状态**：Normative（规范性）

---

## 1. 架构

路由级完整中间件设计（方案 C）：

```text
main.rs
  └── .layer(Extension(pool))       ← PgPool 注入到每个请求的 extensions

handler.rs (路由级)
  └── operlog!("模块名", Insert)
        → OperlogLayer / OperlogService
        → 从 req.extensions() 读取 PgPool
        → 缓冲 request/response body
        → scope_spawn 异步写 sys_oper_log
```

**无 operlog 的路由零开销** — OperlogLayer 只挂在写路由上，读路由不受影响。

---

## 2. 使用方式

### 2.1 handler.rs — 标记写路由

```rust
use framework::{operlog, require_permission};
use std::convert::Infallible;

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // 写路由：permission + operlog
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:role:add"))
                .layer(operlog!("角色管理", Insert))
        }))
        // 读路由：仅 permission，无 operlog
        .routes(routes!(list).layer(require_permission!("system:role:list")))
}
```

### 2.2 main.rs — 注入 PgPool

```rust
use axum::Extension;

let operlog_pool = state.pg.clone();
let app = Router::new()
    // ... routes ...
    .with_state(state)
    .layer(from_fn_with_state(tenant_state, tenant_mw::tenant_guard))
    .layer(Extension(operlog_pool))   // PgPool for operlog route-level middleware
    .layer(from_fn_with_state(auth_state, auth_mw::auth))
    // ...
```

`Extension<PgPool>` 注入到请求的 extensions 中，`OperlogService` 在路由级读取。

---

## 3. BusinessType 常量

| 常量 | 值 | 使用场景 |
| --- | --- | --- |
| `Other` | 0 | 其他 |
| `Insert` | 1 | 新增 (POST) |
| `Update` | 2 | 修改 (PUT)、修改状态、重置密码 |
| `Delete` | 3 | 删除 (DELETE) |
| `Grant` | 4 | 授权、取消授权 |
| `Export` | 5 | 导出 |
| `Import` | 6 | 导入 |
| `Clean` | 9 | 清空 |

---

## 4. 记录字段

写入 `sys_oper_log` 的字段来源：

| 字段 | 来源 | 说明 |
| --- | --- | --- |
| `title` | `operlog!` 宏第一个参数 | 模块中文名 |
| `business_type` | `operlog!` 宏第二个参数 | BusinessType 常量 |
| `request_method` | `req.method()` | GET/POST/PUT/DELETE |
| `oper_url` | `req.uri()` | 完整请求路径 |
| `oper_param` | request body | JSON 序列化，截断 2000 字符 |
| `json_result` | response body | JSON 序列化，截断 2000 字符 |
| `error_msg` | response body `msg` 字段 | 业务错误消息 |
| `status` | response `code` 字段 | "0"=成功 / "1"=失败 |
| `cost_time` | 计时器 | handler 耗时（毫秒） |
| `oper_name` | `RequestContext.user_name` | 操作人 |
| `tenant_id` | `RequestContext.tenant_id` | 租户 |
| `operator_type` | 固定 1 | 后台用户 |

---

## 5. 规则

### 5.1 必须加 operlog 的路由

所有 **写操作**（POST 新增、PUT 修改、DELETE 删除）**必须** 加 `operlog!`。

### 5.2 不加 operlog 的路由

- 所有 **读操作**（GET list/query/select）
- `oper_log` 模块自身（避免递归记录）
- `auth/login`、`auth/logout`（登录日志由专门的 `sys_logininfor` 记录）
- 只读模块（`mail_log`、`sms_log`）

### 5.3 layer 链式写法

写路由需要同时挂 permission + operlog 两个 layer。由于 `UtoipaMethodRouter::layer()` 的类型推断问题，**必须** 使用 `.map()` + turbofish 模式：

```rust
// 正确
.routes(routes!(create).map(|r| {
    r.layer::<_, Infallible>(require_permission!("xxx"))
        .layer(operlog!("模块", Insert))
}))

// 错误 — 编译失败（E0283 类型推断歧义）
.routes(routes!(create)
    .layer(require_permission!("xxx"))
    .layer(operlog!("模块", Insert)))
```

### 5.4 异步写入

日志通过 `context::scope_spawn` 异步写入：
- **不阻塞** HTTP 响应
- 写入失败只记 `tracing::warn`，**不影响** 业务结果
- `scope_spawn` 继承 `RequestContext`（tenant_id / user_name 可用）
- PgPool 缺失时记 `tracing::warn`，日志跳过（不 panic）

---

## 6. 当前覆盖

| 模块 | operlog 数 | 排除原因 |
| --- | --- | --- |
| 配置管理 | 4 | — |
| 部门管理 | 3 | — |
| 字典管理 | 6 | — |
| 菜单管理 | 4 | — |
| 岗位管理 | 3 | — |
| 角色管理 | 6 | — |
| 租户管理 | 3 | — |
| 套餐管理 | 3 | — |
| 用户管理 | 6 | — |
| 通知公告 | 3 | — |
| 站内信模板 | 3 | — |
| 站内信消息 | 7 | — |
| 邮箱账号 | 3 | — |
| 邮件模板 | 3 | — |
| 短信渠道 | 3 | — |
| 短信模板 | 3 | — |
| 登录日志 | 2 | — |
| 操作日志 | 0 | 避免递归 |
| 认证 | 0 | 登录日志独立记录 |
| 邮件日志 | 0 | 只读 |
| 短信日志 | 0 | 只读 |
| **合计** | **65** | |

---

## 7. 文件清单

| 文件 | 职责 |
| --- | --- |
| `framework/src/middleware/operlog.rs` | OperlogLayer + OperlogService + BusinessType |
| `framework/src/middleware/macros.rs` | `operlog!` 宏（与 require_permission! 等统一管理） |
| `app/src/main.rs` | `Extension(pool)` 注入 |
| 各 `handler.rs` router() | 路由级 `operlog!` 标记 |

---

## 8. 架构演进记录

| 版本 | 方案 | 问题 |
| --- | --- | --- |
| v1.0 | OperlogMarkLayer (route) + global_operlog (global) | route extension 对 global layer 不可见 |
| **v1.1** | **OperlogLayer (route-level 完整) + Extension&lt;PgPool&gt;** | **已解决** |

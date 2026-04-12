# 操作日志 (Operlog) 框架规范 v1.0

**生效日期**：2026-04-12
**状态**：Normative（规范性）

---

## 1. 架构

两层设计，职责分离：

```text
handler.rs (路由级)
  └── operlog!("模块名", Insert)
        → OperlogMarkLayer 往 request extension 塞 OperlogMark

main.rs (全局级)
  └── global_operlog middleware
        → 读 OperlogMark → 缓冲 request/response body
        → 异步写 sys_oper_log (scope_spawn, 不阻塞响应)
```

**无 mark 的路由零开销** — 全局 middleware 检查 extension 后直接 `next.run(req)`。

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

### 2.2 main.rs — 全局 layer

```rust
use framework::middleware::operlog;

let operlog_pool = state.pg.clone();
let app = Router::new()
    // ... routes ...
    .with_state(state)
    .layer(from_fn_with_state(tenant_state, tenant_mw::tenant_guard))
    .layer(from_fn(move |req, next| {
        operlog::global_operlog(operlog_pool.clone(), req, next)
    }))
    .layer(from_fn_with_state(auth_state, auth_mw::auth))
    // ...
```

**层序**：operlog 在 tenant_guard 之后、auth 之前。这样 operlog 能读到 `RequestContext` 中的 `user_name` / `tenant_id`（由 auth 设置）。

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

### 5.3 layer 链式写法

写路由需要同时挂 permission + operlog 两个 layer。由于 `UtoipaMethodRouter::layer()` 的类型推断问题，**必须** 使用 `.map()` + turbofish 模式：

```rust
// ✅ 正确
.routes(routes!(create).map(|r| {
    r.layer::<_, Infallible>(require_permission!("xxx"))
        .layer(operlog!("模块", Insert))
}))

// ❌ 错误 — 编译失败（E0283 类型推断歧义）
.routes(routes!(create)
    .layer(require_permission!("xxx"))
    .layer(operlog!("模块", Insert)))
```

### 5.4 异步写入

日志通过 `context::scope_spawn` 异步写入：
- **不阻塞** HTTP 响应
- 写入失败只记 `tracing::warn`，**不影响** 业务结果
- `scope_spawn` 继承 `RequestContext`（tenant_id / user_name 可用）

---

## 6. 当前覆盖

| 模块 | operlog 数 | 排除原因 |
| --- | --- | --- |
| 配置管理 | 4 | — |
| 部门管理 | 3 | — |
| 字典管理 | 6 | — |
| 登录日志 | 2 | — |
| 菜单管理 | 4 | — |
| 通知公告 | 3 | — |
| 岗位管理 | 3 | — |
| 角色管理 | 6 | — |
| 租户管理 | 3 | — |
| 套餐管理 | 3 | — |
| 用户管理 | 6 | — |
| 操作日志 | 0 | 避免递归 |
| 认证 | 0 | 登录日志独立记录 |
| **合计** | **43** | |

---

## 7. 文件清单

| 文件 | 职责 |
| --- | --- |
| `framework/src/middleware/operlog.rs` | OperlogMark + OperlogMarkLayer + global_operlog + BusinessType |
| `framework/src/middleware/macros.rs` | `operlog!` 宏定义（与 require_permission! 等统一管理） |
| `app/src/main.rs` | 全局 layer 注册 |
| 各 `handler.rs` router() | 路由级 operlog! 标记 |

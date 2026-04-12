# Phase 1 Sub-Phase 2a User 模块 — 会话交接文档（2026-04-11）

**用途**：让新的 Claude 会话无缝接手 Phase 1 Sub-Phase 2a（user 模块）的 subagent-driven 执行流程，不需要从对话历史重建上下文。新会话只要按顺序读这个文件 + spec + plan 三份，就拥有全部需要的信息。

---

## 如何在新会话里继续

在新开的 Claude Code 会话里，把下面这段作为第一条消息粘贴：

```text
按顺序阅读以下三个文件：
1. server-rs/docs/RESUME-phase1-user-module.md   （本文件 —— 当前状态）
2. server-rs/docs/specs/2026-04-11-phase1-user-module-design.md  （spec）
3. server-rs/docs/plans/2026-04-11-phase1-user-module-plan.md    （plan）

然后验证上个会话的结果没有被破坏：
  cd /Users/jason/Documents/Project/node/tea-saas/server-rs
  cargo test --workspace              （期望：113 passing）
  cargo clippy --all-targets -- -D warnings  （期望：零告警）
  cargo fmt --check                   （期望：fmt ok）
  ./target/debug/app &
  sleep 2
  bash scripts/smoke-role-module.sh   （期望：ALL 14 STEPS PASSED —— role 回归）
  pkill -f target/debug/app

验证通过后，从 Batch 5（plan 的 Tasks 11-14：role_repo 写方法 + CreateUserDto + 
POST /system/user/ + smoke）继续 subagent-driven 执行流程。

遵循 handoff 文档里"执行协议"那一节的模式：每个批次派一个 implementer subagent，
然后派 spec reviewer subagent，再派 code quality reviewer subagent，必要时跑 fix
loop，批次通过后标记完成、继续下一个。

用户偏好提醒：
- 不要跑任何 git 命令
- 不要未经询问就加新的 crate 依赖  
- 配置都在 config/development.yaml（不用 .env 文件，dotenvy 已移除）
- 报告要简洁；smoke test 要粘贴真实 curl 输出
- 用户主要说中文，回复也用中文
```

---

## 当前状态快照（2026-04-11 晚间 —— Week 1 读路径完成）

### 测试数

**113 passing**，分布：

- framework：59 个（包括 Batch 1 从 role 搬来的 3 个 `fmt_ts` 测试）
- modules（lib 单测）：28 个（包括 Batch 2 新增的 3 个 `ListUserDto` 验证测试）
- modules（集成）：23 个（不变：22 role 集成 + 1 role 401 wiring）
- app：3 个

**核心不变量**：`cargo test --workspace` 报出 **113**；`cargo clippy --all-targets -- -D warnings` 零告警；`cargo fmt --check` 干净；`scripts/smoke-role-module.sh` 14/14 绿；Phase 0 login + info + logout 正常（269 perms）。

### 已接通的端点（11 个目标中完成 4 个）

| 方法 | 路径 | 权限 | 状态 |
|---|---|---|---|
| GET | `/api/v1/system/user/list` | `system:user:list` | ✅ 分页 + 6 个过滤项 + tenant JOIN |
| GET | `/api/v1/system/user/option-select` | _authenticated_ | ✅ 500 上限 + userName 子串过滤 |
| GET | `/api/v1/system/user/info` | _authenticated_ | ✅ 从 RequestContext 取 user_id |
| GET | `/api/v1/system/user/{id}` | `system:user:query` | ✅ tenant JOIN + role_ids 投影 |

### 已完成的批次（4 / 14）

| 批次 | Plan 任务 | 交付内容 |
|---|---|---|
| 1 | Task 1 | Framework 准备：`ResponseCode::OPERATION_NOT_ALLOWED=1004` 常量 + `framework::response::time::fmt_ts` 模块（从 role/dto 提升，含 3 单测）+ `require_authenticated!` 宏 + 回填 role option-select |
| 2 | Tasks 2-5 | 地基：`SysUser` entity 扩展到 23 字段（Phase 0 scenario A：就地扩展）+ custom `Debug` impl 屏蔽 password + `user_repo` 新增 `USER_COLUMNS` 常量和 `find_by_id_tenant_scoped` + `cleanup_test_users` 测试 helper + `system/user/mod.rs` + `system/user/dto.rs`（3 个 Week 1 DTOs + 3 个单测） |
| 3 | Tasks 6-9 | 读路径：`RoleRepo::find_role_ids_by_user` 桥方法 + `UserRepo::find_page` + `find_option_list` + `system/user/service.rs`（4 个函数）+ `system/user/handler.rs`（4 个 handlers + router）+ 接入 `modules::router()` 和 `app/main.rs` + 3 个新 DTOs（`UserOptionQueryDto`、`UserOptionResponseDto`、`UserInfoResponseDto`） |
| 4 | Task 10 | Week 1 smoke gate —— 实际由 Batch 3 的 smoke 步骤覆盖，无新代码 |

### 剩余的批次（10 个 + final + user-gated）

| 批次 | Plan 任务 | 内容 |
|---|---|---|
| 5 | 11-14 | `RoleRepo::verify_role_ids_in_tenant` + `replace_user_roles_tx` + `CreateUserDto` + 3 个 `UserRepo` 写方法 (insert_tx / insert_user_tenant_binding_tx / verify_user_name_unique) + service create + handler POST + smoke |
| 6 | 15-16 | `UpdateUserDto` + `UserRepo::update_tx` + service update + handler PUT + guards helper (`is_super_admin_user` + `is_self_op`) |
| 7 | 17-18 | `ChangeUserStatusDto` + `UserRepo::change_status` + service/handler/route + `UserRepo::soft_delete_by_id` + service remove (CSV 多 ID + self/admin guards) + handler DELETE |
| 8 | 19 | Week 2 manual smoke gate（9-step 完整序列：create → update → change-status → self-block → admin-block → delete → 1001 验证） |
| 9 | 21 | `ResetPwdDto` + `UserRepo::reset_password` + service (admin guard + 会话失效) + handler PUT（Task 20 是 Task 19 的 rework slack，若 19 通过则跳过） |
| 10 | 22-23 | `AuthRoleResponseDto` + service `find_auth_role` + handler GET + `AuthRoleUpdateDto` + service `update_auth_role`（self+admin guards + 角色验证 + replace_user_roles_tx）+ handler PUT |
| 11 | 24 | 集成测试套件（~22 个真实 DB 测试，镜像 role 模块的 pattern） |
| 12 | 25 | `scripts/smoke-user-module.sh` —— 16 步自动化脚本 |
| 13 | 26 | Phase 0 + role 回归 + Week 3 exit gate 自动部分 |
| Final | — | 对整个 user 模块跑一次 code-reviewer subagent |
| USER-GATED | 27 steps 1-5 | Vue web `VITE_API_URL` 切流到 localhost:18080 浏览器手动验证（需要人肉） |

---

## 本会话内已应用的内嵌 review fixes —— 不要再被重复标记

这些是 Batches 1-3 的 code-quality reviewer 发现后立即修掉的。下个会话的 reviewer 如果又提出来，直接说"已修复"跳过。

### Batch 2 的 3 个 inline fixes

1. **`USER_COLUMNS` 常量 DRY 化** —— Phase 0 的 `find_by_username` 和 `find_by_id` 原本有自己的 23-col 硬编码 SELECT，已经改成 `format!("SELECT {USER_COLUMNS} FROM sys_user u ...")`。下次加列只改一处。

2. **`cleanup_test_users` 防空 prefix** —— `crates/modules/tests/common/mod.rs` 加了 `assert!(!prefix.is_empty(), "cleanup_test_users: prefix must not be empty")` —— 防止空 prefix 匹配所有用户整表删除。

3. **`SysUser` 的 `Debug` 手写实现屏蔽 password** —— 原先 `#[derive(Debug)]` 会把 bcrypt hash 打到日志；现在有个手写 `impl Debug for SysUser` 把 `password` 字段渲染成 `"[REDACTED]"`，其他 22 个字段正常显示。Derive 改成了只有 `Clone, FromRow, Serialize`。

### Batch 3 的 2 个 doc-comment fixes

4. **`lib.rs::router()` 的同步提醒注释** —— 注释明确说明 `main.rs` 和 `lib.rs::router()` 是两套独立的路由组装点，新增模块时必须两边都改，否则集成测试和生产 binary 会看到不同的路由集合。Phase 2 的 hygiene 任务之一是让 `main.rs` 调 `modules::router(state)` 来统一。

5. **`user_repo::find_page` 的 super-tenant invariant doc** —— 注释说明调用方必须确保 `current_tenant_scope()` 返回 `Some(...)`。因为 JOIN 里没把 tenant_id 放在 ON 子句，如果 tenant=None 且用户绑定了多个活跃 `sys_user_tenant` 行，会产生重复结果行。Phase 1 认证流程永远会设 tenant，但 Phase 2 的 admin 工具路径里必须注意。

---

## 框架/模块/spec 里比较容易踩的坑

### 坑 1：`sys_user` 没有 `tenant_id` 列 —— 隔离靠 `sys_user_tenant` JOIN

和 `sys_role` 不一样。role 模块可以直接 `WHERE tenant_id = $n`，user 模块必须：
- 读：`JOIN sys_user_tenant ut ON ut.user_id = u.user_id WHERE ($N::varchar IS NULL OR ut.tenant_id = $N) AND ut.status = '0'`
- 写：`UPDATE sys_user SET ... WHERE user_id = $M AND del_flag = '0' AND ($N::varchar IS NULL OR EXISTS (SELECT 1 FROM sys_user_tenant WHERE user_id = sys_user.user_id AND tenant_id = $N AND status = '0'))`

`sys_user.platform_id` 是平台标识（常量 `'000000'`），**不是** tenant_id。不要混用。

### 坑 2：`sys_user_tenant` 的写归属是临时的

Phase 1 Sub-Phase 2a 期间，`sys_user_tenant` 的写操作临时归 `user_repo.rs` 独占（insert_user_tenant_binding_tx + 未来可能的 status 更新）。正式的 tenant 模块启动时要把这些方法迁走。spec 里已经白纸黑字说明这一点，plan Task 12 的代码注释里也有。

### 坑 3：`sys_user_role` 的写仍然归 `role_repo.rs` 独占

POST create user、PUT update user、PUT auth-role 这三个 user 模块端点都要写 `sys_user_role`。**不要**在 `user_repo.rs` 里加 INSERT/DELETE on sys_user_role。要走 service 层组合：service 开 tx，调 `RoleRepo::replace_user_roles_tx(&mut tx, user_id, role_ids)`（Batch 5 Task 11 新增），然后 commit。DAO 规则 4 必须守住。

### 坑 4：`RequestContext` 访问器的正确 API

Batch 3 发现的正确用法（在 `framework::context::mod.rs` 里）：

```rust
framework::context::RequestContext::with_current(|ctx| ctx.user_id.clone()).flatten()
```

返回 `Option<String>`。外层 `with_current` 返回 `Option<R>`（context 不存在时 None），闭包里 `user_id.clone()` 本身也是 `Option<String>`，所以总共是 `Option<Option<String>>`，`.flatten()` 压平成 `Option<String>`。`auth/service.rs` 里有相同用法可以参考。

**不要** 用 `framework::context::current_request_context()` —— 那个名字可能不存在，plan 里写的是占位符。

### 坑 5：`OperationNotAllowed` 业务码 = 1004

Batch 1 已经添加。在 guards（self-block、admin-block）里使用时，业务码用 `ResponseCode::OPERATION_NOT_ALLOWED`，msg 用中文："不能对自己执行该操作" / "不能对超级管理员执行该操作" 等。

但是 —— **`AppError::Business` 的精确 variant shape 还没验证**。plan 里写的是 `AppError::Business { code, msg: Some("...".into()) }`，这是**推测的**结构体 variant 形态。Batch 5 的 implementer 需要先读 `framework::error::AppError` 的实际定义，如果不是那种形态，fallback 到 `.business_err_if(code)` 加 i18n key 或者给 AppError 加个 `.with_msg()` helper。

### 坑 6：`main.rs` 和 `lib.rs::router()` 两套独立路由装配

Batch 3 发现的：`app/main.rs` 不调用 `modules::router(state)`，而是自己独立地 `.nest(API_PREFIX, modules::auth::router())` + `.nest(API_PREFIX, modules::system::role::router())` + `.nest(API_PREFIX, modules::system::user::router())`。`lib.rs::router()` 只被集成测试的 `tests/common/mod.rs::build_state_and_router` 用。新加模块必须两处都注册。已在 lib.rs 加了同步提醒注释。Phase 2 的 hygiene 清理项里列了"让 main.rs 调 modules::router"。

### 坑 7：`bcrypt` 已经在 framework 层就绪

`framework::infra::crypto::{hash_password, hash_password_with_cost, verify_password}` 已存在且有 4 个单测。create 和 reset-pwd 直接 import：

```rust
use framework::infra::crypto::hash_password;
let password_hash = hash_password(&dto.password)
    .map_err(|e| AppError::Internal(anyhow::anyhow!("bcrypt: {e}")))?;
```

不需要抽取新的 helper。

### 坑 8：主键和 user_id 的 UUID 来源

`sys_user.user_id` 由应用代码生成：

```rust
let user_id = uuid::Uuid::new_v4().to_string();
```

`sys_user_tenant.id` 看 schema —— plan Task 12 建议 `gen_random_uuid()::varchar` 但没确认列类型。Batch 5 implementer 要先跑 `PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c "\d sys_user_tenant"` 确认。

---

## 用户偏好（延续到新会话）

- **绝对不跑 git 命令**。不跑 `git add` / `git commit` / `git status` / `git diff`。进度靠 `cargo test / clippy / fmt` 追踪。smoke test 要检查状态就用 `psql`，**永远不用 git**。
- **不未经询问就加新 crate 依赖**。本会话连 `regex` 都没引入（密码强度校验用宽松的 length 规则绕开了）。
- **配置都在 `config/development.yaml`**。DB url、Redis url、JWT secret 都在 yaml 里。**不要**重新引入 env 文件加载。
- **报告要粘贴真实 curl 输出**。subagent 跑 smoke test 时必须展示真实的 HTTP 响应体，不能只说"成功了"。
- **任务粒度**：dispatch 批次，不要 dispatch 单独的 plan 任务。大多数批次覆盖 2-4 个 plan 任务。见上面"剩余的批次"表。
- **用户主要用中文交流**。回复默认用中文；技术内容（代码、文件路径、命令、SQL 字段名）保持英文。

---

## 执行协议（subagent-driven development）

每个批次的流程和 role 模块完全一致：

1. **派 implementer subagent**（一般用 sonnet），prompt 要自包含、精确：
   - 完整的任务文本（直接粘贴 plan 里的代码块，不要让 subagent 自己读 plan）
   - 每个要改的文件的确切代码
   - 强制性验证步骤（`cargo check/test/clippy/fmt`）
   - 必要时的手动 smoke test（直接给 bash 命令）
   - 预期的测试总数
   - 报告格式：改了哪些文件、测试数、smoke 输出、偏离项、状态
   - 硬约束：不跑 git、不加 crate 依赖、不改无关文件

2. **派 spec reviewer subagent**（general-purpose + sonnet）：
   - 指出这个批次覆盖的 plan 任务号
   - 列出应该创建/修改的文件
   - 列出已知可接受的偏离（避免被误报）
   - 检查是否有 scope 蔓延、类型是否一致
   - 返回 ✅ COMPLIANT 或 ❌ NON-COMPLIANT

3. **如果 spec review 发现缺口**：派 fix subagent，然后重跑 spec review

4. **派 code quality reviewer subagent**（superpowers:code-reviewer + sonnet）：
   - **不**再检查 spec 合规性（已经在上一步过了）
   - 找潜在 bug、惯用法问题、测试质量、架构问题
   - 返回 ✅ / ⚠️ / ❌，带优点 + 必改项 + 观察项
   - **可以并行**和 spec reviewer 一起派出去（它们读同一份代码 snapshot，互不干扰）

5. **如果 quality review 有必改项**：派 fix subagent 重跑；**如果只有 observations**：判断哪些值得 inline 修（通常是 1-5 分钟的 cosmetic/defensive 改动），剩下的在 review 报告里标注 "accepted, tracked in backlog"

6. **在 TodoWrite 里标记批次完成**，进入下一个批次

7. **Batch 13 结束后**：派一个最终的 code-reviewer 对整个 user 模块做最后一次全面 review，然后标记 sub-phase 2a 完成

skill 在 `superpowers:subagent-driven-development`。新会话开头调一次：

```
Skill: superpowers:subagent-driven-development
```

---

## 验证命令（新会话启动后先跑这个）

在 dispatch Batch 5 之前，先确认 handoff 状态完好：

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs

# 1. 测试绿 —— 期望 113
cargo test --workspace 2>&1 | grep "test result"
# 分布：
#   app          3 passed
#   framework    59 passed
#   modules-lib  28 passed
#   modules-int  23 passed

# 2. Clippy 干净
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5

# 3. Fmt 干净
cargo fmt --check && echo "fmt ok"

# 4. App 能编译
cargo build -p app 2>&1 | tail -5

# 5. 快速 smoke —— role 模块回归（14 步），Phase 0 login 正常
./target/debug/app > /tmp/tea-rs-app-verify.log 2>&1 &
APP_PID=$!
sleep 2
bash scripts/smoke-role-module.sh 2>&1 | tail -3
curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print('login:', d['code'], '|', len(d['data']['access_token']), 'char token')"
kill $APP_PID 2>/dev/null
wait 2>/dev/null

# 6. 快速 user 模块 smoke —— 4 个 Week 1 端点都应工作
./target/debug/app > /tmp/tea-rs-app-verify2.log 2>&1 &
APP_PID=$!
sleep 2
TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
echo "--- user list ---"
curl -sS "http://127.0.0.1:18080/api/v1/system/user/list?pageNum=1&pageSize=10" \
  -H "Authorization: Bearer $TOKEN" | python3 -c "import sys,json; d=json.load(sys.stdin); print('code:', d['code'], '| rows:', len(d['data']['rows']))"
echo "--- user info ---"
curl -sS http://127.0.0.1:18080/api/v1/system/user/info \
  -H "Authorization: Bearer $TOKEN" | python3 -c "import sys,json; d=json.load(sys.stdin); print('code:', d['code'], '| userName:', d['data']['userName'])"
echo "--- user option-select ---"
curl -sS http://127.0.0.1:18080/api/v1/system/user/option-select \
  -H "Authorization: Bearer $TOKEN" | python3 -c "import sys,json; d=json.load(sys.stdin); print('code:', d['code'], '| count:', len(d['data']))"
kill $APP_PID 2>/dev/null
wait 2>/dev/null
```

**任何一步失败都不要启动 Batch 5**，先排查 regression。

---

## 快速文件索引（帮新会话快速定位）

### Spec + Plan + Handoff

| 文件 | 作用 |
|---|---|
| `docs/specs/2026-04-11-phase1-user-module-design.md` | Sub-Phase 2a spec（已批准 2026-04-11） |
| `docs/plans/2026-04-11-phase1-user-module-plan.md` | Sub-Phase 2a plan（27 个任务带代码块） |
| `docs/RESUME-phase1-user-module.md` | 本文件 |
| `docs/RESUME-phase1-role-module.md` | Sub-Phase 1 的 handoff（role 模块，已关闭） |
| `docs/specs/2026-04-10-phase1-role-module-design.md` | role 模块 spec（参考模板） |
| `docs/plans/2026-04-10-phase1-role-module-plan.md` | role 模块 plan（参考模板，30 任务） |

### Framework 层（Batch 1 接触的文件）

| 文件 | 作用 |
|---|---|
| `crates/framework/src/response/codes.rs` | Batch 1 新增 `OPERATION_NOT_ALLOWED = 1004` |
| `crates/framework/src/response/time.rs` | Batch 1 新增 —— `fmt_ts` + 3 单测（从 role/dto 提升） |
| `crates/framework/src/response/mod.rs` | Batch 1 添加 `pub mod time;` + `pub use time::fmt_ts;` |
| `crates/framework/src/middleware/access_macros.rs` | Batch 1 新增 `require_authenticated!` 宏 |
| `crates/framework/src/infra/crypto.rs` | bcrypt helper（Phase 0 就有） |
| `crates/framework/src/context/mod.rs` | RequestContext + `with_current` accessor |
| `crates/framework/src/error/ext.rs` | `IntoAppError` + `BusinessCheckOption` + `BusinessCheckBool` trait（Batch 5.5） |

### Modules 层

| 文件 | 作用 |
|---|---|
| `crates/modules/src/lib.rs` | `modules::router()` —— Batch 3 加了 user，有同步提醒注释 |
| `crates/modules/src/domain/entities.rs` | `SysUser`（23 字段，custom Debug 屏蔽 password）+ `SysUserTenant` + `SysRole` |
| `crates/modules/src/domain/user_repo.rs` | Phase 0 读方法 + Batch 2 的 `find_by_id_tenant_scoped` + `USER_COLUMNS` + Batch 3 的 `find_page` + `find_option_list` |
| `crates/modules/src/domain/role_repo.rs` | role 模块全部方法 + Batch 3 新增 `find_role_ids_by_user` |
| `crates/modules/src/domain/common.rs` | `AuditInsert` / `audit_update_by` / `current_tenant_scope` |
| `crates/modules/src/system/user/mod.rs` | `pub mod dto; pub mod handler; pub mod service; pub use handler::router;` |
| `crates/modules/src/system/user/dto.rs` | 6 个 DTO（UserDetailResponseDto / UserListItemResponseDto / ListUserDto / UserOptionQueryDto / UserOptionResponseDto / UserInfoResponseDto）+ 3 个 `#[allow(dead_code)]` helpers（default_status / validate_status_flag / validate_sex_flag）+ 3 单测 |
| `crates/modules/src/system/user/service.rs` | 4 个函数：find_by_id / list / option_select / info |
| `crates/modules/src/system/user/handler.rs` | 4 个 handlers + `router()` 有 4 条路由 |
| `crates/modules/src/system/role/handler.rs` | role 模块（Batch 1 已回填 option-select 使用 require_authenticated!） |
| `crates/modules/tests/common/mod.rs` | Phase 0 test harness + Batch 2 的 `cleanup_test_users`（带 empty-prefix 守卫） |
| `crates/app/src/main.rs` | 主 binary，三条 `.nest()` 分别挂载 auth/role/user router |

---

## 延后的 tech debt（不在 Sub-Phase 2a 范围内）

这些是明确延后到后续 sub-phase 的事情：

1. **让 `main.rs` 调 `modules::router(state)` 来统一路由组装** —— 消除 lib.rs / main.rs 双路由装配点的维护陷阱。Phase 2 hygiene 清理。
2. **Password 强度规则对齐 NestJS** —— 当前 Sub-Phase 2a 用宽松的 `length(6-20)` 规则。NestJS 要求大写+小写+数字+符号。等 policy 落文档再上 regex validator。
3. **`dept_id` 验证** —— 当前不校验 dept_id 存在（dept 模块还没建）。Phase 1 Sub-Phase 3（dept）加入。
4. **`sys_user_tenant` 写归属迁移** —— 当前 user_repo 临时托管，tenant 模块启动时迁走（Phase 1 Sub-Phase 5 之后）。
5. **个人中心端点**（profile GET/PUT、avatar 上传、change-pwd）—— Sub-Phase 2b。
6. **批量 create/delete** —— Sub-Phase 2b。
7. **xlsx export** —— observability sub-phase。
8. **`dept-tree` + `role-post` helper 端点** —— 等 dept/post 模块。
9. **`fmt_ts` 是否还要更小心的 timezone 处理** —— 目前 `FixedOffset` 够用，不引入 `chrono-tz`。
10. **集成测试的 TOCTOU / 并发安全** —— 当前用 `--test-threads=1` 避免。CI 端要不要 docker-compose 一个隔离 DB 还没决策。

---

## 最后一点

生成这个文档的对话很长。**不要尝试**读对话历史 —— 读这个文件 + spec + plan，跑验证，然后 dispatch Batch 5。所有你需要的东西都在磁盘上。

Sub-Phase 1 的 role 模块 RESUME 文档证明了这个交接模式有效；对 Sub-Phase 2a 沿用同一套。

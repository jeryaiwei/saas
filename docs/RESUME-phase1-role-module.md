# Phase 1 Role 模块 — 会话交接文档（2026-04-11）

**用途**：让新的 Claude 会话无缝接手 Phase 1 role 模块的 subagent-driven 执行流程，不需要从对话历史重建上下文。新会话只要按顺序读这个文件 + spec + plan 三份，就拥有全部需要的信息。

---

## 如何在新会话里继续

在新开的 Claude Code 会话里，把下面这段作为第一条消息粘贴：

```text
按顺序阅读以下三个文件：
1. server-rs/docs/RESUME-phase1-role-module.md   （本文件 —— 当前状态）
2. server-rs/docs/specs/2026-04-10-phase1-role-module-design.md  （spec）
3. server-rs/docs/plans/2026-04-10-phase1-role-module-plan.md    （plan）

然后验证上个会话的结果没有被破坏：
  cd /Users/jason/Documents/Project/node/tea-saas/server-rs
  cargo test --workspace   （期望：77 passing）
  cargo clippy --all-targets -- -D warnings  （期望：零告警）

验证通过后，从 Batch 6（plan 的 tasks 18-20：change-status + DELETE +
option-select）继续 subagent-driven 执行流程。遵循 handoff 文档里
"执行协议" 那一节的模式：每个批次派一个 implementer subagent，然后派
spec reviewer subagent，再派 code quality reviewer subagent，必要时跑
fix loop，批次通过后标记完成、继续下一个。

用户偏好提醒：
- 不要跑任何 git 命令
- 不要未经询问就加新的 crate 依赖
- 配置都在 config/development.yaml（不用 .env 文件，dotenvy 已移除）
- 报告要简洁；smoke test 要粘贴真实 curl 输出
- 用户主要说中文，回复也用中文
```

---

## 当前状态快照（2026-04-11 晚间 —— 自动化部分全部完成）

### 测试数

**110 passing**，分布：

- framework：56 个单测
- modules（单测）：28 个
- modules（集成）：23 个（1 个 401 wiring + **22 个新增的 role 模块真实 DB 集成测试**）
- app：3 个单测
- 核心不变量：`cargo test --workspace` 报出 **110**；`cargo clippy --all-targets -- -D warnings` 零告警；`cargo fmt --check` 干净；`scripts/smoke-role-module.sh` 14/14 绿。

### 已接通的端点（11 个目标 **全部完成**）

| 方法 | 路径 | 权限 | 状态 |
|---|---|---|---|
| POST | `/api/v1/system/role/` | `system:role:add` | ✅ |
| PUT | `/api/v1/system/role/` | `system:role:edit` | ✅ 事务 menu replace-all |
| GET | `/api/v1/system/role/list` | `system:role:list` | ✅ 分页 + 过滤 |
| GET | `/api/v1/system/role/option-select` | _authenticated_ | ✅ 唯一 raw `from_fn_with_state` 路由 |
| PUT | `/api/v1/system/role/change-status` | `system:role:change-status` | ✅ |
| GET | `/api/v1/system/role/auth-user/allocated-list` | `system:role:allocated-list` | ✅ 三表 JOIN |
| GET | `/api/v1/system/role/auth-user/unallocated-list` | `system:role:unallocated-list` | ✅ LEFT JOIN 反模式 |
| PUT | `/api/v1/system/role/auth-user/select-all` | `system:role:select-auth-all` | ✅ UNNEST + ON CONFLICT，**含租户守卫** |
| PUT | `/api/v1/system/role/auth-user/cancel` | `system:role:cancel-auth` | ✅ UNNEST + ANY，**含租户守卫（Final review 修复）** |
| GET | `/api/v1/system/role/{id}` | `system:role:query` | ✅ |
| DELETE | `/api/v1/system/role/{id}` | `system:role:remove` | ✅ 软删除 |

### 已完成的批次（全部 11 批 + 最终 review）

| 批次 | Plan 任务 | 交付内容 |
|---|---|---|
| 1 | 1-5 | 地基层：`common.rs` helpers、`SysRole` entity、`RoleRepo` 骨架、`system::role` 模块脚手架、集成测试 harness |
| 2 | 6-8 | GET `/{id}` 端到端 + `fmt_ts` Asia/Shanghai 时区修复 |
| 3 | 9-11 | GET `/list` + `ValidatedQuery<T>` 提取器 + `Page::map_rows` + validation → 400 路由修复 |
| 4 | 12-15 | POST `/` 事务性插入 + Week 1 exit gate |
| 5 | 16-17 | PUT `/` 事务性更新（menu replace-all 事务） |
| 5.5 | （hygiene） | Framework 层抽取：`IntoAppError` / `BusinessCheckOption` / `BusinessCheckBool` ext trait + `require_permission!` 宏 + `PageQuery` + validation 错误递归 |
| 6 | 18-20 | `PUT /change-status` + `DELETE /{id}` + `GET /option-select` |
| 7 | 21-22 | `GET /auth-user/allocated-list`（sys_user + sys_user_role + sys_user_tenant 三表 JOIN）+ `AllocatedUserRow` projection |
| 8 | 23-24 | `GET /auth-user/unallocated-list`（LEFT JOIN + `ur.role_id IS NULL`）+ Week 2 smoke gate 12/12 |
| 9 | 25-27 | `PUT /auth-user/select-all`（UNNEST + ON CONFLICT）+ `PUT /auth-user/cancel`（DELETE + ANY），`AuthUserAssignDto` / `AuthUserCancelDto` |
| 10 | 28 | `scripts/smoke-role-module.sh` —— 153 行、14 步自动化端到端（全部 11 端点覆盖） |
| 11 | 29, 30 步骤 6 | Phase 0 回归（login/info/logout/health 全绿）+ Week 3 exit gate 自动部分 |
| Final | — | 跨整个 role 模块的 code-reviewer subagent：READY_WITH_NON_BLOCKING_NOTES |

### Final review 之后做的 2 处修复

1. **`unassign_users` 加上租户守卫**（service.rs）—— Final reviewer 指出的跨租户写入路径：原先 `delete_user_roles` 没有租户过滤，`unassign_users` 也没有前置 `find_by_id` 守卫，理论上持有跨租户 `role_id` 的调用方能删外部租户的 `sys_user_role` 行。现在 `unassign_users` 和 `assign_users` 对称，先 `find_by_id.or_business(DATA_NOT_FOUND)`，再 DELETE。幂等性保留：真 role + 无绑定 → 200；ghost role → 1001。
2. **`status` 字段改用枚举校验**（dto.rs）—— 原先 `length(min=1,max=1)` 会让 `"x"` / `"2"` 之类的任意单字符落进 DB（`CHAR(1)` 无 CHECK 约束）。新增 `validate_status_flag` 自定义 validator，只接受 `"0"` / `"1"`。`CreateRoleDto`、`UpdateRoleDto`、`ChangeRoleStatusDto` 都切到它。2 个新单测覆盖。wire 错误格式：`{"field":"status","message":"status_flag"}`。

### 只剩一项未做（需要人肉）

**Task 30 steps 1-5 —— Vue web 前端切流手动验证**：

1. `cd web && grep -r VITE_API_URL .env*` 看当前配置
2. `VITE_API_URL=http://localhost:18080 pnpm dev`（或改 `.env.development`，用完恢复）
3. 浏览器 `admin / admin123` 登录，走一遍角色管理页的完整流程：列表加载 → 创建角色（带菜单）→ 查看详情 → 编辑（改名、换菜单）→ 禁用/启用 → 分配用户 → 取消分配 → 删除
4. 每一步都应零前端改动通过
5. 恢复 `.env.development`

做完这一步 Phase 1 Sub-Phase 1 就可以正式关闭。

---

## Batch 5.5 Framework 抽取 —— 原 plan 里没有

Batch 5.5 是 Batch 5 和 Batch 6 之间插入的「hygiene 批次」，目的是在
Batches 6-11 再重复那些模式 7 次之前，先把重复消除掉。这些改动落在
`framework/` 层，是项目级的 primitives。

**新增文件**：

- `crates/framework/src/error/ext.rs` —— `IntoAppError`、`BusinessCheckOption`、`BusinessCheckBool` 扩展 trait + 6 个单测
- `crates/framework/src/middleware/access_macros.rs` —— 三个 `#[macro_export] macro_rules!` 宏：`require_permission!`、`require_role!`、`require_scope!`

**Framework 改动**：

- `crates/framework/src/response/pagination.rs` —— 新增 `PageQuery` struct，带 `#[serde(deserialize_with = "de_u32_any")]` 辅助函数（处理 `serde_urlencoded` + flatten + `u32` 的兼容性 bug，见下面「已知框架坑点」）
- `crates/framework/src/extractors/validated_json.rs` —— 重写 `validation_errors_to_app_error`，递归处理 `Struct` / `List` 嵌套 validation 错误，错误字段路径用点号拼接（例如 `page.page_size`）
- `error/mod.rs`、`response/mod.rs`、`middleware/mod.rs` 的 `pub use` 都相应更新

**Role 模块侧的改动**：

- `RoleRepo` —— 引入 `const ROLE_PAGE_WHERE`（消除 find_page 里的重复 WHERE）、`bulk_insert_role_menus` 私有 helper（消除 insert/update 里的重复 UNNEST）、5 个方法都加上 `#[tracing::instrument]`
- `ListRoleDto` —— 改用 `#[serde(flatten)] pub page: PageQuery`，不再本地定义 `page_num` / `page_size`
- `service.rs` —— 用 `.into_internal()?`、`.or_business(code)?`、`.business_err_if(code)?`，替代原来的 `.map_err` + `BusinessError::throw_if_null`
- `handler.rs` —— 用 `require_permission!("system:role:add")` 宏，替代原来 4 行的 `route_layer(from_fn_with_state(access::require(AccessSpec::permission(...)), access::enforce))`
- `dto.rs` —— 加了 `create_dto_fixture!` + `update_dto_fixture!` 本地测试宏，测试样板大幅缩短

**为什么这对 Batches 6-11 重要**：写新端点时必须遵循 Batch 5.5 的模式，**不要回退到原来啰嗦的写法**。具体来说：

- 用 `require_permission!(...)`，不要用 `route_layer(from_fn_with_state(...))`
- 用 `.into_internal()?`，不要用 `.map_err(AppError::Internal)?`
- 用 `.or_business(code)?`，不要用 `BusinessError::throw_if_null(...)`
- 用 `.business_err_if(code)?`，不要用 `if cond { return BusinessError::throw(code); }`
- 任何批量 role_menu 插入都用 `Self::bulk_insert_role_menus(&mut tx, ...)`
- 每个新 repo 方法都加 `#[tracing::instrument(skip_all, fields(...))]`
- 新的 list DTO 都用 `#[serde(flatten)] pub page: PageQuery`
- DTO 测试用对应的 fixture 宏

---

## 已知框架坑点（Batch 5.5 发现的）

### 坑 1：`serde_urlencoded` + `#[serde(flatten)]` + `u32` 会运行时报错

给 query-string DTO 加 `#[serde(flatten)] pub page: PageQuery`（其中 `u32` 字段）之后，`GET /list?pageNum=1&pageSize=5` 会炸，报 `invalid type: string "1", expected u32`。

**根因**：`serde_urlencoded` 把所有 flattened 字段存成 `String`；没有 flatten 的时候它会在顶层做一次宽松的 string→number 转换，但 flatten 之后那层宽松转换不到子结构里。

**已做的修复**：`pagination.rs` 里有一个 `de_u32_any` serde visitor，同时接受 integer 和数字字符串。已经通过 `#[serde(deserialize_with = ...)]` 挂到 `PageQuery::page_num` 和 `page_size`。

**对未来的 list DTO**：flatten `PageQuery` 时不用额外处理，validation 会自动工作。**但如果你在一个 flattened 子结构里加新的 `u32` 字段**（例如 `#[serde(flatten)] pub cursor: CursorQuery` 带 `u32 offset`），得在那里也应用 `de_u32_any`。

### 坑 2：嵌套 validation 错误落在 `ValidationErrorsKind::Struct`

当你用 `#[validate(nested)]` 标注一个 flattened 子结构时，validation 失败会落在 `ValidationErrorsKind::Struct` 里，而不是 `Field`。旧的 `validation_errors_to_app_error` 只遍历 `field_errors()`，所以嵌套错误会被吞掉、返回空的 `data: []`。

**已做的修复**：这个函数已经重写，递归遍历所有 `ValidationErrorsKind` 变体，把路径用点号拼接。wire 输出现在是：`{"field": "page.page_size", "message": "range"}`。

---

## 用户偏好（延续到新会话）

- **绝对不跑 git 命令**。不跑 `git add` / `git commit` / `git status` / `git diff`。进度靠 `cargo test / clippy / fmt` 追踪。smoke test 要检查状态就用 `psql`，**永远不用 git**。
- **不未经询问就加新 crate 依赖**。Batches 1-5.5 刻意避开新 crate，哪怕 reviewer 提过（比如用 `FixedOffset` 而不是 `chrono-tz`，不引入任何 proc macro crate）。
- **配置都在 `config/development.yaml`**。项目里**没有** `.env.development`，**没有** `dotenvy` 加载。DB url、Redis url、JWT secret 都在 yaml 里。**不要**重新引入 env 文件加载。
- **报告要粘贴真实 curl 输出**。subagent 跑 smoke test 时必须展示真实的 HTTP 响应体，不能只说「成功了」。
- **任务粒度**：dispatch 批次，不要 dispatch 单独的 plan 任务。大多数批次覆盖 2-4 个 plan 任务。见上面「剩余的批次」表。
- **用户主要用中文交流**。回复默认用中文；技术内容（代码、文件路径、命令、SQL 字段名）保持英文。

---

## 执行协议（subagent-driven development）

每个批次的流程：

1. **派 implementer subagent**，prompt 要自包含、精确：
   - 完整的任务文本（不要说「读 plan 的第 N 个任务」，直接粘贴内容）
   - 每个要改的文件的确切代码块
   - 强制性验证步骤（`cargo check/test/clippy/fmt`）
   - 必要时的手动 smoke test（直接给 bash 命令）
   - 预期的测试总数
   - 报告格式：改了哪些文件、测试数、smoke 输出、偏离项、状态

2. **派 spec reviewer subagent**：
   - 指出这个批次覆盖的 plan 任务号
   - 列出应该创建/修改的文件
   - 列出已知可接受的偏离（避免被误报）
   - 检查是否有 scope 蔓延、类型是否一致
   - 返回 ✅ COMPLIANT 或 ❌ NON-COMPLIANT

3. **如果 spec review 发现缺口**：派 fix subagent，然后重跑 spec review

4. **派 code quality reviewer subagent**：
   - **不**再检查 spec 合规性（已经在上一步过了）
   - 找潜在 bug、惯用法问题、测试质量、架构问题
   - 返回 ✅ / ⚠️ / ❌，带优点 + 必改项 + 观察项

5. **如果 quality review 有必改项**：派 fix subagent，重跑 quality review

6. **在 TodoWrite 里标记批次完成**，进入下一个批次

7. **Batch 11 结束后**：派一个最终的 code-reviewer 对整个 role 模块做最后一次全面 review，然后标记 sub-phase 完成

skill 在 `superpowers:subagent-driven-development`。新会话开头调一次：

```
Skill: superpowers:subagent-driven-development
```

---

## 验证命令（新会话启动后先跑这个）

在 dispatch Batch 6 之前，先确认 handoff 状态完好：

```bash
cd /Users/jason/Documents/Project/node/tea-saas/server-rs

# 1. 测试绿
cargo test --workspace 2>&1 | grep "test result" | tail -5
# 期望：看到 "test result: ok. 77 passed"

# 2. Clippy 干净
cargo clippy --all-targets -- -D warnings 2>&1 | tail -5
# 期望：Finished 且无 error

# 3. Fmt 干净
cargo fmt --check && echo "fmt ok"

# 4. App 能编译
cargo build -p app 2>&1 | tail -5
# 期望：Finished

# 5. 快速 smoke —— 登录 + 列表（需要 saas_tea DB 跑在 127.0.0.1:5432）
./target/debug/app > /tmp/tea-rs-app.log 2>&1 &
APP_PID=$!
sleep 2
TOKEN=$(curl -sS -X POST http://127.0.0.1:18080/api/v1/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
curl -sS http://127.0.0.1:18080/api/v1/system/role/list \
  -H "Authorization: Bearer $TOKEN" \
  | python3 -c "import sys,json; d=json.load(sys.stdin); print('list ok, total=', d['data']['total'])"
kill $APP_PID 2>/dev/null
# 期望："list ok, total= 3"
```

**任何一步失败都不要启动 Batch 6**，先排查 regression。

---

## 延后的 tech debt（不在 Batches 6-11 范围内）

这些是 Batch 5.5 第三轮分析时识别出来的、明确延后到 Phase 1 sub-phase 2 或 Phase 2 的事情。Batches 6-11 **不要做**这些 —— 它们不在 role 模块首个切片的范围：

1. **`fmt_ts` 提升**到 `framework::response` —— 等 Phase 1 sub-phase 2 的 user 模块需要同样 helper 时再做
2. **`RoleUpsertParams` 结构体** —— 等某个批次给 insert/update API 加 `data_scope` / `menu_check_strictly` / `dept_check_strictly` 字段、参数数量越过 10+ 时再做
3. **sqlx `.sqlx/` offline metadata** —— 延后到 Phase 1 sub-phase 2，当总 query 数超过 30+ 时再引入工具链。目前 9 个 query 不值得。
4. **`find_page` 单查询优化**（`COUNT(*) OVER()`）—— 只有当 role 列表变成可测量的热路径时才做（它不会）。跳过。
5. **`find_by_id` 单查询优化**（LEFT JOIN + array_agg）—— 同上。跳过。
6. **真正的集成测试套件** —— 目前只有 1 个集成测试（401 路径）。一个正儿八经的 ~20 个测试的套件（覆盖 happy path + error path）应该在 Batch 7 和 Batch 8 之间作为 mini-dispatch 加进来。**这是延后项里最有价值的一个** —— 考虑在 Batch 11 关闭 sub-phase 之前做掉。
7. **`clippy::pedantic` 审计** —— Batch 11 final reviewer 时做一次性手动 sweep
8. **`cargo audit` / `cargo-deny` / `cargo-udeps`** —— Phase 2 CI 工具链
9. **把 `require_permission!` 宏改写成函数** —— 只有当你愿意和 axum layer 的类型命名体操搏斗才试。宏现在工作得好好的，不要为了美观重构。

---

## 快速文件索引（帮新会话快速定位）

| 文件 | 作用 |
|---|---|
| `docs/specs/2026-04-10-phase1-role-module-design.md` | spec（已批准，范围的事实源） |
| `docs/plans/2026-04-10-phase1-role-module-plan.md` | plan（30 个编号任务带代码块） |
| `docs/RESUME-phase1-role-module.md` | 本文件 |
| `crates/framework/src/error/ext.rs` | Batch 5.5 —— `IntoAppError` 等扩展 trait |
| `crates/framework/src/middleware/access_macros.rs` | Batch 5.5 —— `require_permission!` 等宏 |
| `crates/framework/src/response/pagination.rs` | `Page<T>`、`PageQuery`、`de_u32_any` |
| `crates/framework/src/extractors/validated_json.rs` | `validation_errors_to_app_error`（现在会递归） |
| `crates/modules/src/domain/common.rs` | `AuditInsert`、`audit_update_by`、`current_tenant_scope` |
| `crates/modules/src/domain/role_repo.rs` | `RoleRepo`：5 个方法 + 私有 helper + 2 个常量 |
| `crates/modules/src/system/role/dto.rs` | 6 个 DTO + `fmt_ts` + fixture 宏 + 测试 |
| `crates/modules/src/system/role/service.rs` | 4 个 service 函数，用新的 ext trait |
| `crates/modules/src/system/role/handler.rs` | 4 个 handler + router，用 `require_permission!` |
| `crates/modules/tests/common/mod.rs` | 集成测试 harness（CWD 修复、`as_super_admin`） |
| `crates/modules/tests/role_module_tests.rs` | 1 个集成测试（401 接线检查） |

---

**最后一点**：生成这个文档的对话很长。**不要尝试**读对话历史 —— 读这个文件 + spec + plan，跑验证，然后 dispatch Batch 6。所有你需要的东西都在磁盘上。

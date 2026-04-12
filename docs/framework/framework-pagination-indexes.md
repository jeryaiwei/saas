# SaaS 分页索引契约

**状态**：v1.0（2026-04-11 首次发布）
**关联规范**：`docs/framework/framework-pagination-spec.md` §5

> 本文档是**全局真源**：每个 `find_page` 方法依赖的索引必须在此登记。新增 list 端点时必须同步更新本文档，否则 PR 不得合并（见 spec §13 checklist）。

---

## 使用方式

- 每个 list 端点对应一个 section，记录：
  - **Repo 方法**（全路径）
  - **目标表**（主表 + JOIN 表）
  - **期望索引**（名称 + 列 + migration 来源）
  - **缺失时降级行为**（seq scan? warn? fail?）
  - **验证方式**（手动 EXPLAIN / CI regression test / 未验证）
- 未验证的条目**应当** 在下一次 hygiene pass 中通过 `EXPLAIN (ANALYZE)` 确认
- 索引对应的 migration **必须** 已存在；不得承诺"将来补上"

---

## 1. `user_repo::find_page`

**端点**：`GET /system/user/list`
**目标表**：`sys_user u JOIN sys_user_tenant ut`
**SQL 形态**：

```sql
SELECT u.* FROM sys_user u
JOIN sys_user_tenant ut ON ut.user_id = u.user_id
WHERE u.del_flag = '0'
  AND ut.status = '0'
  AND ($1::varchar IS NULL OR ut.tenant_id = $1)
  AND ($2::varchar IS NULL OR u.user_name LIKE '%' || $2 || '%')
  AND ($3::varchar IS NULL OR u.nick_name LIKE '%' || $3 || '%')
  AND ($4::varchar IS NULL OR u.email LIKE '%' || $4 || '%')
  AND ($5::varchar IS NULL OR u.phonenumber LIKE '%' || $5 || '%')
  AND ($6::varchar IS NULL OR u.status = $6)
  AND ($7::varchar IS NULL OR u.dept_id = $7)
ORDER BY u.create_at DESC
LIMIT $8 OFFSET $9;
```

### 期望索引

| 索引                                | 列                                              | 用途                             | Migration                      | 状态                                       |
| ----------------------------------- | ----------------------------------------------- | -------------------------------- | ------------------------------ | ------------------------------------------ |
| `pk_sys_user`                       | `user_id`                                       | JOIN 主键                        | 初始化 schema                  | ✅ 存在（主键）                            |
| `pk_sys_user_tenant`                | `id`                                            | JOIN 主键                        | 初始化 schema                  | ✅ 存在（主键）                            |
| `idx_sys_user_tenant_user_id`       | `sys_user_tenant(user_id)`                      | JOIN on `ut.user_id = u.user_id` | 初始化 schema                  | ⚠️ 假定存在，需 `\d+ sys_user_tenant` 确认 |
| `idx_sys_user_tenant_tenant_status` | `sys_user_tenant(tenant_id, status)`            | 租户过滤 + 状态过滤              | ⚠️ **待创建**                  | ❌ 未验证                                  |
| `idx_sys_user_create_at_del`        | `sys_user(create_at DESC) WHERE del_flag = '0'` | 排序 + soft delete               | ⚠️ **待创建**（partial index） | ❌ 未验证                                  |

### 缺失时降级行为

- **缺 `idx_sys_user_tenant_tenant_status`**：Postgres 退化为全表扫 `sys_user_tenant` + hash join，小租户场景仍可用（~10ms），大租户（>100k 用户）p99 恶化到数百 ms
- **缺 `idx_sys_user_create_at_del`**：`ORDER BY create_at DESC` 退化为 in-memory sort，OFFSET > 10000 时显著慢
- **LIKE 中间匹配**（`%foo%`）：**不走索引**，天然全表扫——这是规范接受的（见 spec §8.1），模糊搜索本就不该期望索引

### 验证方式

- **v1.0**：未验证。下一次 DB 可用时跑 `EXPLAIN (ANALYZE, BUFFERS) SELECT ...` 对 100k 用户种子数据
- **v1.1**：CI regression 测试 via `framework::testing::assert_no_seq_scan`（计划中）

### 性能预期（spec §4.3 声明）

- Shallow pages (offset < 1000)：< 10ms on 10k-user tenant
- Deep pages (offset > 10000)：up to 500ms on 1M-user tenant
- 深翻页需求走 cursor 分页（§11 Phase 3）

---

## 2. `role_repo::find_page`

**端点**：`GET /system/role/list`
**目标表**：`sys_role`
**SQL 形态**：

```sql
SELECT * FROM sys_role
WHERE del_flag = '0'
  AND ($1::varchar IS NULL OR tenant_id = $1)
  AND ($2::varchar IS NULL OR role_name LIKE '%' || $2 || '%')
  AND ($3::varchar IS NULL OR role_key LIKE '%' || $3 || '%')
  AND ($4::varchar IS NULL OR status = $4)
ORDER BY role_sort ASC, create_at DESC
LIMIT $5 OFFSET $6;
```

### 期望索引

| 索引                              | 列                                                              | 用途                   | Migration                      | 状态      |
| --------------------------------- | --------------------------------------------------------------- | ---------------------- | ------------------------------ | --------- |
| `pk_sys_role`                     | `role_id`                                                       | 主键                   | 初始化 schema                  | ✅ 存在   |
| `idx_sys_role_tenant_status_sort` | `sys_role(tenant_id, status, role_sort)` WHERE `del_flag = '0'` | 租户过滤 + 状态 + 排序 | ⚠️ **待创建**（partial index） | ❌ 未验证 |

### 缺失时降级行为

- **缺 `idx_sys_role_tenant_status_sort`**：全表扫 `sys_role` + in-memory sort。租户角色数通常 < 100，seq scan 实际很快，这个索引更多是"正确姿态"而非性能刚需

### 验证方式

- **v1.0**：未验证
- 小表（< 1k 行）即使 seq scan 也够快，验证优先级低于 user 表

---

## 3. `role_repo::find_allocated_users_page`

**端点**：`GET /system/role/auth-user/allocated-list`
**目标表**：`sys_user u JOIN sys_user_role ur JOIN sys_user_tenant ut`
**SQL 形态**：

```sql
SELECT u.* FROM sys_user u
JOIN sys_user_role ur ON ur.user_id = u.user_id
JOIN sys_user_tenant ut ON ut.user_id = u.user_id
WHERE ur.role_id = $1
  AND u.del_flag = '0'
  AND ut.status = '0'
  AND ($2::varchar IS NULL OR ut.tenant_id = $2)
  AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%')
ORDER BY u.create_at DESC
LIMIT $4 OFFSET $5;
```

### 期望索引

| 索引                        | 列                       | 用途                        | Migration     | 状态        |
| --------------------------- | ------------------------ | --------------------------- | ------------- | ----------- |
| `pk_sys_user_role`          | `(user_id, role_id)`     | 复合主键                    | 初始化 schema | ✅ 假定存在 |
| `idx_sys_user_role_role_id` | `sys_user_role(role_id)` | 反向 JOIN `ur.role_id = $1` | ⚠️ **待创建** | ❌ 未验证   |
| 同 §1 的 sys_user 索引      |                          |                             |               |             |

### 缺失时降级行为

- **缺 `idx_sys_user_role_role_id`**：`sys_user_role` 的复合主键 `(user_id, role_id)` 顺序不对 `role_id` 有效查询，`WHERE ur.role_id = $1` 退化为全表扫 `sys_user_role`——**关键索引，必须有**

### 验证方式

- **v1.0**：未验证
- 生产环境若 `sys_user_role` 行数 > 10k，缺这个索引会被即刻发现

---

## 4. `role_repo::find_unallocated_users_page`

**端点**：`GET /system/role/auth-user/unallocated-list`
**目标表**：`sys_user u JOIN sys_user_tenant ut LEFT ANTI JOIN sys_user_role ur`
**SQL 形态**：

```sql
SELECT u.* FROM sys_user u
JOIN sys_user_tenant ut ON ut.user_id = u.user_id
WHERE u.del_flag = '0'
  AND ut.status = '0'
  AND ($1::varchar IS NULL OR ut.tenant_id = $1)
  AND NOT EXISTS (
    SELECT 1 FROM sys_user_role ur
    WHERE ur.user_id = u.user_id AND ur.role_id = $2
  )
  AND ($3::varchar IS NULL OR u.user_name LIKE '%' || $3 || '%')
ORDER BY u.create_at DESC
LIMIT $4 OFFSET $5;
```

### 期望索引

| 索引                                     | 列                                | 用途                      | Migration           | 状态        |
| ---------------------------------------- | --------------------------------- | ------------------------- | ------------------- | ----------- |
| `idx_sys_user_role_user_role`            | `sys_user_role(user_id, role_id)` | 反半连接加速 `NOT EXISTS` | ⚠️ 复合主键应已覆盖 | ⚠️ 假定存在 |
| 同 §1 的 sys_user / sys_user_tenant 索引 |                                   |                           |                     |             |

### 缺失时降级行为

- `NOT EXISTS` 的反半连接对复合主键 `(user_id, role_id)` 天然友好，**通常** 不需要额外索引
- 唯一风险：`sys_user_role` 行数 > 1M 时，planner 可能选择 hash anti-join 而不是 nested loop anti-join，消耗内存

### 验证方式

- **v1.0**：未验证

---

## 5. `tenant_repo::find_page`

**端点**: `GET /system/tenant/list`
**目标表**: `sys_tenant t LEFT JOIN sys_tenant_package p`
**SQL 形态**: SELECT + LEFT JOIN package (for package_name) + 5 optional filters + ORDER BY t.create_at DESC + LIMIT/OFFSET

### 期望索引

| 索引 | 列 | 用途 | 状态 |
|---|---|---|---|
| `pk_sys_tenant` | `id` | 主键 | 已存在 |
| `idx_sys_tenant_create_at` | `sys_tenant(create_at DESC)` | 排序 | 待验证 |

### 性能预期

租户数通常 < 1k, seq scan 可接受。

---

## 6. `tenant_package_repo::find_page`

**端点**: `GET /system/tenant-package/list`
**目标表**: `sys_tenant_package`
**SQL 形态**: SELECT + 2 optional filters + ORDER BY create_at DESC + LIMIT/OFFSET

### 期望索引

| 索引 | 列 | 用途 | 状态 |
|---|---|---|---|
| `pk_sys_tenant_package` | `package_id` | 主键 | 已存在 |

### 性能预期

套餐数通常 < 100, 不需要额外索引。

---

## 待办事项

**v1.1 完成**（2026-04-11）：

- [x] ~~引入 `framework::testing::assert_no_seq_scan(pool, sql, exempt_tables)` helper~~ — 实际交付为 `framework::testing::explain_plan::check_no_seq_scan`（pure function，6 个单测覆盖 flat/nested/exempt/unexpected-shape 四类分支）
- [x] ~~引入 `framework::testing::assert_index_exists` helper~~ — 交付为 `framework::testing::pg_catalog::assert_index_exists` + `list_indexes_on_table`（async，DB-dependent，无单测）

**剩余待办**（延期到 v1.2 — 触发条件：生产 p99 告警 / 100k+ tenant 规模 seed 数据）：

- [ ] 对 `user_repo::find_page` 跑一次 `EXPLAIN (ANALYZE, BUFFERS)` with 100k seed
- [ ] 确认 `sys_user_tenant` 上是否已有 `(tenant_id, status)` 复合索引；如无，写 migration 创建
- [ ] 确认 `sys_user_role` 上是否已有反向 `role_id` 索引；如无，写 migration 创建
- [ ] 为 `sys_user` 创建 `create_at DESC WHERE del_flag='0'` partial index
- [ ] 为每个 `find_page` 挂一个 seq-scan regression integration test（使用 `check_no_seq_scan` + seed 数据 + `SET enable_seqscan = off`；当前 dev DB 行数 < 100 planner 仍选 seq scan，需要 seed 后才有效）

**v1.1 的基本态度未变**：当前 dev DB 行数 < 100，seq scan 足够快，spec 首要价值是**把假设显式化** + **把工具交付就位**，而不是立刻 tune。工具（`check_no_seq_scan` / `assert_index_exists`）已经 ready，等真实负载触发时就能即刻启用。

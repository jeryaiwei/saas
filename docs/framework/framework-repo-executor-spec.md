# SaaS Repository Executor 规范 v1.0

**生效日期**：2026-04-12
**状态**：Normative（规范性）

> 本规范约束所有 repo 方法的 executor 参数选择和 service 层的事务管理。

---

## 1. Executor 参数规则

### 1.1 单查询方法 → `impl PgExecutor<'_>`

方法体内只有**一次** `.fetch_*()` / `.execute()` 调用时：

```rust
pub async fn find_by_id(
    executor: impl sqlx::PgExecutor<'_>,
    id: &str,
) -> anyhow::Result<Option<SysUser>> {
    sqlx::query_as::<_, SysUser>(sql)
        .bind(id)
        .fetch_optional(executor)
        .await
        .context("find_by_id")
}
```

**必须** 使用 `impl sqlx::PgExecutor<'_>`。
**不得** 使用 `pool: &PgPool`（限制了调用方必须在事务外使用）。

### 1.2 多读查询方法 → `impl Acquire<'_>`

方法体内有**两次以上只读** `.fetch_*()` 调用时（如 find_page 的 rows + count），使用 `sqlx::Acquire`。`Acquire` 兼具灵活性（接受 `&PgPool` 和 `&mut Transaction`）和可复用性（`acquire()` 返回的连接可多次查询）。

```rust
pub async fn find_page(
    conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
    filter: UserListFilter,
) -> anyhow::Result<Page<SysUser>> {
    let mut conn = conn.acquire().await.context("user.find_page: acquire")?;
    let rows = sqlx::query_as(...).fetch_all(&mut *conn).await?;   // 第1次
    let total = sqlx::query_scalar(...).fetch_one(&mut *conn).await?; // 第2次（同一连接）
    Ok(p.into_page(rows, total))
}
```

调用方：
```rust
// 普通调用 — 从池拿连接
find_page(&state.pg, filter).await?;

// 事务内调用 — 复用事务连接
let mut tx = state.pg.begin().await?;
find_page(&mut tx, filter).await?;    // ✅ 在事务内读到一致数据
tx.commit().await?;
```

### 1.3 多写查询方法 → `&mut Transaction`

```rust
pub async fn replace_user_roles(
    tx: &mut Transaction<'_, Postgres>,
    user_id: &str,
    role_ids: &[String],
) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sys_user_role WHERE user_id = $1")
        .execute(&mut **tx).await?;  // 第1次
    // bulk INSERT ...
    sqlx::query("INSERT INTO sys_user_role ...").execute(&mut **tx).await?;  // 第2次
    Ok(())
}
```

### 1.4 决策表

| 方法内查询次数 | 查询类型 | 参数选择 | 事务内可用 |
|---|---|---|---|
| 1 | 读或写 | `impl PgExecutor<'_>` | 是 |
| 2+ | 纯读（如 rows + count） | `impl Acquire<'_>` | 是 |
| 2+ | 含写（如 DELETE + INSERT） | `&mut Transaction<'_, Postgres>` | 强制 |

---

## 2. Service 层事务规则

### 2.1 何时必须用事务

**规则**：如果一个 service 方法调用了 **2 个以上写操作**（INSERT / UPDATE / DELETE），**必须** 包在事务里。

```rust
// ✅ 正确：3 个写操作在同一个 tx 里
pub async fn create(state: &AppState, dto: CreateUserDto) -> Result<...> {
    let mut tx = state.pg.begin().await.context("begin tx").into_internal()?;
    UserRepo::insert(&mut *tx, params).await.into_internal()?;
    TenantRepo::insert_user_tenant_binding(&mut *tx, ...).await.into_internal()?;
    RoleRepo::replace_user_roles(&mut tx, ...).await.into_internal()?;
    tx.commit().await.context("commit tx").into_internal()?;
    Ok(...)
}
```

```rust
// ✅ 正确：批量删除循环在同一个 tx 里
pub async fn remove(state: &AppState, path_ids: &str) -> Result<...> {
    // ... 验证 ...
    let mut tx = state.pg.begin().await.context("begin tx").into_internal()?;
    for id in &ids {
        UserRepo::soft_delete_by_id(&mut *tx, id).await.into_internal()?;
    }
    tx.commit().await.context("commit tx").into_internal()?;
    Ok(())
}
```

### 2.2 何时不需要事务

**规则**：单个写操作**不需要**事务。

```rust
// ✅ 正确：单个 INSERT，不需要 tx
pub async fn create(state: &AppState, dto: CreateDeptDto) -> Result<...> {
    let dept = DeptRepo::insert(&state.pg, params).await.into_internal()?;
    Ok(DeptResponseDto::from_entity(dept))
}

// ✅ 正确：单个 UPDATE，不需要 tx
pub async fn change_status(state: &AppState, dto: ...) -> Result<...> {
    let affected = UserRepo::change_status(&state.pg, ...).await.into_internal()?;
    (affected == 0).business_err_if(ResponseCode::DATA_NOT_FOUND)
}
```

### 2.3 传参约定

| 传给 repo 的参数 | 语法 | 场景 |
|---|---|---|
| 非事务（pool） | `&state.pg` | 单写或纯读 |
| 事务内 + `impl PgExecutor` 方法 | `&mut *tx` | deref Transaction → PgConnection |
| 事务内 + `impl Acquire` 方法 | `&mut tx` | find_page 等多读查询 |
| 事务内 + `&mut Transaction` 方法 | `&mut tx` | 多查询写方法（如 replace_user_roles） |

**`&mut *tx` vs `&mut tx` 的区别**：
- `&mut *tx`：解引用 `Transaction` 得到 `PgConnection`，实现了 `PgExecutor`——用于单查询方法
- `&mut tx`：保持 `&mut Transaction` 引用——用于多查询写方法和 `Acquire` 方法（方法内部需要多次使用）

---

## 3. 禁止模式

| 模式 | 为什么禁止 | 替代方案 |
|---|---|---|
| `pool: &PgPool` 用于单查询方法 | 限制了在事务内的复用性 | `impl PgExecutor<'_>` |
| `pool: &PgPool` 用于多读查询方法 | 限制了在事务内的复用性 | `impl Acquire<'_>` |
| `impl PgExecutor<'_>` 用于多查询方法 | executor 第一次查询后被消费 | `impl Acquire`（纯读）或 `&mut Transaction`（含写） |
| `impl Acquire` 用于多写方法 | 调用方可传 `&pool` 绕过事务，多写不原子 | `&mut Transaction`（强制事务） |
| 批量写循环不包 tx | 部分成功——前面的已提交，后面的失败 | `begin()` → 循环 → `commit()` |
| `_tx` 后缀命名 | 已弃用——方法名应反映业务语义，不反映参数类型 | 直接用业务名（`insert` 不是 `insert_tx`） |
| repo 内部自己 `pool.begin()` | tx 边界应由 service 决定，不由 repo 决定 | service 传 `&mut tx`，repo 只管执行 |
| `?` 在 `tx.commit()` 之后做写操作 | commit 后的写不在 tx 内 | 把所有写放在 commit 之前 |

---

## 4. 当前状态

### 4.1 方法统计

| 类型 | 数量 | 参数 |
|---|---|---|
| 单查询 | 70+ | `impl PgExecutor<'_>` |
| 多读查询 | 13 | `impl Acquire<'_>`（find_page × 10 + find_allocated/unallocated_users_page × 3） |
| 多查询含写 | 4 | `&mut Transaction`（replace_user_roles, insert/update_with_menus, bulk_insert_role_menus） |

### 4.2 事务使用一览

| Service 方法 | 写操作数 | 有事务 | 状态 |
|---|---|---|---|
| user::create | 3（insert + binding + roles） | ✅ | 正确 |
| user::update | 2（update + roles） | ✅ | 正确 |
| user::update_auth_role | 1（replace_user_roles，内部 2 步） | ✅ | 正确 |
| user::remove | N（循环 soft_delete） | ✅ | 正确（v1.0 修复） |
| role::create | 2（insert_with_menus 内部） | ✅ | 正确 |
| role::update | 2（update_with_menus 内部） | ✅ | 正确 |
| tenant::create | 2N+1（N tenants + 1 user + N bindings） | ✅ | 正确 |
| tenant::remove | 1（batch soft_delete_by_ids） | ✅ | 正确（v1.0 修复） |
| 其他所有 | 0-1 | ❌ | 正确（不需要） |

---

## 5. PR checklist

提交涉及 repo 方法或 service 写操作的 PR 时**必须**自查：

- [ ] 单查询 repo 方法使用 `impl PgExecutor<'_>`（不是 `&PgPool`）
- [ ] 多读查询 repo 方法使用 `impl Acquire<'_>`（不是 `&PgPool`）
- [ ] 多写查询 repo 方法使用 `&mut Transaction`（不是 `impl Acquire`）
- [ ] Service 有 2+ 写操作时包了事务（`begin` → writes → `commit`）
- [ ] 批量写循环在事务内（不在 for 外面 commit 后又 for）
- [ ] 事务内的 `impl PgExecutor` 方法传 `&mut *tx`
- [ ] 事务内的 `impl Acquire` / `&mut Transaction` 方法传 `&mut tx`
- [ ] 没有 `_tx` 后缀命名
- [ ] repo 方法不自己 `begin()` 事务（tx 边界由 service 管）

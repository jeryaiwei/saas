# SaaS 分页框架规范 v1.0

**生效日期**：2026-04-11
**状态**：Normative（规范性）
**适用版本**：server-rs workspace

> 本规范是规范性文档，使用 RFC 风格的 `必须 / 应当 / 可以 / 不应 / 不得` 关键词。每一条规则对应一个在设计评审中识别出的具体失败模式或架构债。
>
> **适用对象**：所有在 `server-rs` 里新增或修改 list 类端点的开发者、reviewer、以及框架本身的维护者。

---

## 0. 适用范围

**规范覆盖**：

- 所有返回列表数据的 HTTP 端点
- 所有接受分页参数的 DAO 方法
- 框架层的分页 primitive：`PageQuery` / `PaginationParams` / `Page<T>`

**规范不覆盖**：

- Cursor / keyset 分页（将来的独立 primitive，见 §11 Phase 3）
- 流式 / 导出类端点（走独立 `ExportQuery` 契约，见 §11 Phase 3.2）
- 单行读（`find_by_id` 等）
- 列表型聚合（如 "每月订单数量"）

---

## 1. 设计原则（rank-ordered）

1. **显式优先于智能**：每层做什么必须能被一次 `grep` 看穿。不引入隐式转换、不埋 proc-macro、不做 tenant-aware 策略自动化。
2. **向后兼容 wire 契约**：`Page<T>` 的 JSON 形态必须与 NestJS 逐字段 byte-compatible，直到整个 web/app 前端切换完成。
3. **分层职责不耦合**：HTTP 层持 `PageQuery`，DAO 层持 `PaginationParams`，Filter 只持透传源数据。三者不越界。
4. **失败可见，不可静默**：clamp、timeout、慢查询、post-condition 违反——全部 emit tracing。
5. **文档即契约**：每个 `find_page` 的索引依赖、race 警告、性能预期，必须写在 doc comment 里。

---

## 2. 类型契约

### 2.1 `PageQuery`（wire 层）

```rust
// framework/src/response/pagination.rs
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct PageQuery {
    #[validate(range(min = 1, max = PAGE_NUM_MAX))]
    #[serde(default = "default_page_num", deserialize_with = "de_u32_any")]
    pub page_num: u32,
    #[validate(range(min = 1, max = PAGE_SIZE_MAX))]
    #[serde(default = "default_page_size", deserialize_with = "de_u32_any")]
    pub page_size: u32,
}
```

**契约**：

- **必须** 用 `#[serde(flatten)]` + `#[validate(nested)]` 嵌入到每个 ListDto，wire 格式保持平铺 `{..., pageNum, pageSize}`
- **必须** 由 `ValidatedQuery` extractor 驱动校验，**不应** 在 service 层重复校验
- **不得** 为 page_num/page_size 定义字面量上限，必须引用 `PAGE_NUM_MAX` / `PAGE_SIZE_MAX` 常量
- **不得** 被克隆（无 `Clone` derive）

**常量（单一真源）**：

```rust
pub const PAGE_NUM_MAX: u32 = 10_000;
pub const PAGE_NUM_DEFAULT: u32 = 1;
pub const PAGE_SIZE_MAX: u32 = 200;
pub const PAGE_SIZE_DEFAULT: u32 = 10;
```

常量放 `framework/src/response/pagination.rs` 顶层，**不得** 放 `framework/src/constants.rs`（那里是跨模块的业务常量，不是分页策略）。

### 2.2 `PaginationParams`（DAO 层）

```rust
#[derive(Debug, Clone, Copy)]
pub struct PaginationParams {
    pub safe_page_num: u32,
    pub safe_page_size: u32,
    pub offset: i64,
    pub limit: i64,
}

impl PaginationParams {
    pub fn from(page_num: u32, page_size: u32) -> Self { /* clamp + derive */ }
    pub fn into_page<T>(self, rows: Vec<T>, total: i64) -> Page<T>;
}
```

**契约**：

- **必须** 只在 DAO 层构造，service 层**不得** 持有或传递
- **必须** 保持 `Copy` 语义（16 字节 POD，降低调用方心智负担）
- **必须** 作为防御式 clamp 的唯一执行点（validator 的 HTTP 层 clamp 是第一道闸门，`PaginationParams::from` 是第二道，两道缺一不可）
- `into_page` **必须** 接受 `i64` 参数（消灭 `total as u64` 散落 cast），内部 `max(0) as u64`

### 2.3 `Page<T>`（响应层）

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub rows: Vec<T>,
    pub total: u64,
    pub page_num: u32,
    pub page_size: u32,
    pub pages: u64,
}
```

**契约**：

- 字段序列化顺序和名称**必须** 与 NestJS `Result.page()` 一致
- **不得** 被克隆（无 `Clone` derive）
- **必须** 通过 `Page::new` 或 `PaginationParams::into_page` 构造，**不得** 使用字段字面量直接构造（防止 `pages` 字段计算错位）
- `map_rows` 是唯一允许的类型转换 helper

**v1 不做**：`total` 字段改 `Option<u64>`、新增 `has_more` 字段——这是 wire break，属于 §11 Phase 2。

### 2.4 Filter struct（DAO 层的请求参数）

```rust
#[derive(Debug)]
pub struct UserListFilter {
    pub user_name: Option<String>,
    pub nick_name: Option<String>,
    // ... other filter fields
    /// PageQuery 的 validator attrs 仅在 HTTP extraction 时触发；
    /// 此处作为透明数据载体，不 derive Validate。
    pub page: PageQuery,
}
```

**契约**：

- **必须** 使用 owned 类型（`String`, `Option<String>`, `Vec<String>`, `PageQuery`），不得带 `'a` 生命周期
- **必须** 嵌入 `PageQuery`（而非 `page_num: u32, page_size: u32` 两字段）
- **必须** 在同一 `_repo.rs` 文件内声明（紧邻消费它的方法）
- **必须** 只 derive `Debug`，**不得** derive `Clone / Serialize / Deserialize / Validate`
- **必须** `pub`，service 层要能用字段 literal 构造
- **必须** 按 "业务过滤字段 → 分页字段" 顺序排列，`page` 字段放最后
- **不得** 承载任何 audit 字段（`create_by` 等）——由 repo 内部从 `RequestContext` 读取
- **不得** 作为 HTTP wire 层类型（`ListUserDto` 是 wire 层，`UserListFilter` 是 DAO 层，两者不通用）

---

## 3. 分层责任

| 层                   | 持有类型                                     | 职责                                                     | 不得做                                          |
| -------------------- | -------------------------------------------- | -------------------------------------------------------- | ----------------------------------------------- |
| **Wire / extractor** | `ListDto` (含 `PageQuery`)                   | serde 反序列化、validator 校验、i18n 错误翻译            | 业务判断、SQL、tenant 注入                      |
| **Handler**          | `ValidatedQuery<ListDto>`, `State<AppState>` | 调 service、包 `ApiResponse::ok`                         | 持有 repo 引用、调 DB、做 mapping               |
| **Service**          | `ListDto`, `Filter`                          | 构造 filter、跨 repo 协调、权限判定                      | 手写 SQL、验证分页边界（validator 已做）        |
| **Repo**             | `Filter`, `PaginationParams`                 | SQL 构造、bind、clamp 兜底、post-condition 断言、tracing | 调另一个 repo、越层读 DTO 类型、违反 index 契约 |

**严格禁止**：repo 导入 `system::*::dto::*Dto`（即使是 list 的只读路径），DAO isolation 是硬边界。

---

## 4. list 端点实现规范

### 4.1 DTO 定义（`system/<module>/dto.rs`）

```rust
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListUserDto {
    pub user_name: Option<String>,
    pub nick_name: Option<String>,
    // ... other filter fields
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
```

**规则**：

- **必须** derive `Debug, Deserialize, Validate`
- **不得** derive `Clone`、`Serialize`
- **必须** 用 `#[serde(rename_all = "camelCase")]`
- 字段**必须** 用 `Option<String>` 表达"可不传"，不接受空字符串作为"不过滤"
- `page: PageQuery` **必须** 是最后一个字段

### 4.2 Service 层 list 方法

```rust
#[tracing::instrument(skip_all, fields(
    has_user_name = query.user_name.is_some(),
    page_num = query.page.page_num,
    page_size = query.page.page_size,
))]
pub async fn list(
    state: &AppState,
    query: ListUserDto,
) -> Result<Page<UserListItemResponseDto>, AppError> {
    let page = UserRepo::find_page(
        &state.pg,
        UserListFilter {
            user_name: query.user_name,
            nick_name: query.nick_name,
            // ... other filter fields
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(UserListItemResponseDto::from_entity))
}
```

**规则**：

- **必须** 用显式字段 literal 构造 filter，**不得** 使用 `impl From<ListDto> for Filter`（禁止隐藏 mapping）
- **必须** 用 `into_internal()` 把 `anyhow::Error` 转 `AppError::Internal`
- **必须** 用 `page.map_rows(...)` 做实体→响应 DTO 转换
- **不得** 在这一层做 clamp、offset 计算、SQL
- **不得** 重复 `tracing::instrument` 里已经有的字段

### 4.3 Repo 层 `find_page` 方法

```rust
/// Paginated list of users in the current tenant.
///
/// ## Expected indexes
/// - `sys_user_tenant(tenant_id, status)` — tenant filter + JOIN
/// - `sys_user(create_at DESC) WHERE del_flag = '0'` — sort + soft-delete
/// - `sys_user(user_name varchar_pattern_ops)` — LIKE prefix (not used for middle-match)
///
/// See `docs/framework/framework-pagination-indexes.md` for the global registry.
///
/// ## Consistency caveats
/// Offset-based pagination is not snapshot-consistent. Concurrent
/// inserts/deletes between page-N and page-(N+1) fetches may cause
/// duplicate or missing rows. See `docs/framework/framework-pagination-spec.md` §8.
///
/// ## Performance expectation
/// - Shallow pages (offset < 1000): < 10ms on tenants up to 10k users
/// - Deep pages (offset > 10000): up to 500ms on 1M-user tenants
///   — use cursor pagination for deep-page use cases (§11 Phase 3)
#[tracing::instrument(skip_all, fields(
    tenant_id = tracing::field::Empty,
    has_user_name = filter.user_name.is_some(),
    has_status = filter.status.is_some(),
    page_num = filter.page.page_num,
    page_size = filter.page.page_size,
    rows_len = tracing::field::Empty,
    total = tracing::field::Empty,
))]
pub async fn find_page(
    pool: &PgPool,
    filter: UserListFilter,
) -> anyhow::Result<Page<SysUser>> {
    let tenant = current_tenant_scope();
    if let Some(t) = tenant.as_deref() {
        tracing::Span::current().record("tenant_id", t);
    }
    let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

    // rows query (uses USER_PAGE_WHERE const)
    let rows = sqlx::query_as::<_, SysUser>(&rows_sql).bind(/* ... */).bind(p.limit).bind(p.offset).fetch_all(pool).await.context("find_page rows")?;

    // count query (uses SAME USER_PAGE_WHERE const)
    let total: i64 = sqlx::query_scalar(&count_sql).bind(/* ... */).fetch_one(pool).await.context("find_page count")?;

    // Post-condition: rows.len() <= limit
    debug_assert!(
        rows.len() as i64 <= p.limit,
        "find_page returned more rows than limit: got={} limit={}",
        rows.len(), p.limit
    );

    tracing::Span::current()
        .record("rows_len", rows.len() as u64)
        .record("total", total);

    Ok(p.into_page(rows, total))
}
```

**规则**：

- **必须** 有一段 doc comment 包含三个 section：`## Expected indexes`、`## Consistency caveats`、`## Performance expectation`
- **必须** 用 `#[tracing::instrument]` 标注 `page_num`、`page_size`、`rows_len`、`total`（4 个标准字段，`rows_len/total` 用 `field::Empty` 延迟填充）。**不得** 在 `find_page` 的 instrument 上声明 `tenant_id`——该字段由 root span（`tenant_http` + `auth` middleware）自动继承（obs spec §2.3）
- **必须** 用 `PaginationParams::from(...)` 而不是裸 `safe_page_num.max(1)` 计算
- **必须** 用 `p.into_page(rows, total)` 作为收尾，**不得** 手写 `Page::new(rows, total as u64, ...)`
- **必须** 在 rows 查询和 count 查询**都**绑定完全一致的 WHERE 参数（共享 `const *_PAGE_WHERE: &str`）
- **必须** 有 `debug_assert!(rows.len() as i64 <= p.limit, ...)` 作为 post-condition
- **不得** 跨 repo 调用（要联查就在 SQL 里 JOIN）
- **不得** 在 find_page 里做 tenant 权限判定（那是 service 层）
- **不得** 接受 page_num / page_size 作为独立参数（必须通过 filter.page 访问）

### 4.4 WHERE 子句的单一真源

当 rows 查询和 count 查询共用 WHERE 时：

```rust
const USER_PAGE_WHERE: &str = "\
    WHERE u.del_flag = '0' \
      AND ut.status = '0' \
      AND ($1::varchar IS NULL OR ut.tenant_id = $1) \
      AND ($2::varchar IS NULL OR u.user_name LIKE '%' || $2 || '%') \
      -- etc.";

let rows_sql = format!("SELECT {USER_COLUMNS} FROM sys_user u JOIN ... {USER_PAGE_WHERE} ORDER BY u.create_at DESC LIMIT $N OFFSET $M");
let count_sql = format!("SELECT COUNT(*) FROM sys_user u JOIN ... {USER_PAGE_WHERE}");
```

**规则**：

- **必须** 把 WHERE 提取成 `const`，rows 和 count SQL 都 `format!` 进去
- 这样消灭 rows 和 count WHERE 漂移（常见 bug：给 rows 加了新过滤条件，忘了同步 count）

---

## 5. 索引契约

### 5.1 每个 `find_page` 的 doc comment 必须列索引

格式见 §4.3 的 `## Expected indexes` section。

### 5.2 数据库层索引声明汇总

**必须** 维护一份 `docs/framework/framework-pagination-indexes.md` 作为**全局真源**，列出每个 `find_page` 对应的索引名、创建它的 migration 文件、以及**索引缺失时的降级行为**。

**规则**：

- 新增 `find_page` 必须同步更新这份文档
- 每个索引必须有对应的 migration（不得依赖"生产手动建索引"）
- CI **应当** 有一个 regression 测试跑 `EXPLAIN (FORMAT JSON)` 并通过 `framework::testing::explain_plan::check_no_seq_scan` assert 计划里没有对核心表的 `Seq Scan`。helper 已在 v1.1 落地（2026-04-11），但**实际挂到具体 `find_page` 的 regression test 推迟到 v1.2**——当前 dev DB 行数太少，Postgres planner 会优先选 seq scan 而非 index，先挂测试会稳定误报。等种子数据就位再启用。

### 5.3 不得做的事

- **不得** 依赖 "Postgres 自动选择计划，通常够快"
- **不得** 在 migration 里用 `CREATE INDEX IF NOT EXISTS` 而不检查 existing 索引的列顺序（潜在的隐式 no-op）
- **不得** 对新 `find_page` 承诺 "索引将来补上"——承诺了就立刻补

---

## 6. 观察性契约

### 6.1 标准字段（每个 `find_page` 必须发出）

| 字段        | 类型           | 来源                                              | 用途                               |
| ----------- | -------------- | ------------------------------------------------- | ---------------------------------- |
| `tenant_id` | Option<String> | root span（auto-inherited from tenant_http + auth） | 按租户分组性能                     |
| `page_num`  | u32            | `filter.page.page_num`                            | 翻页深度                           |
| `page_size` | u32            | `filter.page.page_size`                           | 批量大小                           |
| `rows_len`  | u64            | 结果集                                            | 实际返回行数（≠ page_size 即尾页） |
| `total`     | i64            | COUNT query                                       | 总行数（用于检测深翻页无效请求）   |

> **注意**：`tenant_id` 由 root span 自动继承，**不得** 在 `find_page` 的 `#[instrument]` 上重复声明该字段（obs spec §2.3）。

**必须** 通过 `#[tracing::instrument(fields(...))]` 声明，**不得** 只在函数体内用 `tracing::info!`。

### 6.2 慢查询标记 ✅ v1.1 已实施（2026-04-11）

当 `rows_ms + count_ms > SLOW_QUERY_WARN_MS`（默认 300ms）时**必须** emit `WARN` 级别 tracing。v1.1 直接让每个 `find_page` 手写 4 遍这套计时+判断逻辑（4 个 find_page 调用点，未触发抽象阈值）。`with_timeout` helper 位于 `framework::response::with_timeout`，`SLOW_QUERY_WARN_MS` / `QUERY_TIMEOUT_SECS` 常量位于 `framework::response::pagination`。

### 6.3 不得做的事

- **不得** 用 `tracing::info!("listed {} users", rows.len())` 风格——必须结构化字段
- **不得** 在 `#[instrument]` 里传大对象（如 `filter = ?filter`）——会膨胀 span 内存

---

## 7. 错误与 i18n

### 7.1 HTTP 层分页校验失败

`ValidatedQuery` → `validator::Validate::validate` → `AppError::Validation` → 400 响应。

**必须** 用 `valid.range` i18n key 作为默认错误消息：

```json
"valid.range": "字段值超出允许范围（应在 {min} 到 {max} 之间）"
```

**v1.1 已实施**（2026-04-11）：`FieldError` 新增 `params: HashMap<String, serde_json::Value>` 字段（`#[serde(skip)]`，不上 wire）。`collect_errors` 把 validator 的 `error.params`（`Cow<'static, str> → Value`）转存进去；`AppError::Validation → IntoResponse` 在查到 i18n 翻译后内联执行 `{min}/{max}` 占位符替换。参数化消息直接出现在 wire 响应的 `data[n].message` 字段里。

### 7.2 DAO 层失败

- Timeout → `anyhow::Error` → service `into_internal()` → `AppError::Internal` → 500
- Unique 冲突等业务错误 → `AppError::Business` → 200 + 业务错误码
- **不得** 在 repo 里直接构造 `AppError`，repo 永远返回 `anyhow::Result`

### 7.3 部分成功禁忌

`find_page` 不应该出现"返回一部分数据 + 部分失败"。任何一个子查询（rows / count）失败都**必须** 整个方法 `Err(...)`。**不得** 用 `rows.unwrap_or_default()` 这类静默降级。

---

## 8. 一致性声明（已知限制）

### 8.1 Race conditions（当前不修复，仅文档化）

Offset 分页**不是** snapshot-consistent：

1. **Race A 插入导致重复**：并发插入导致下一页 offset 错位，旧行重复
2. **Race B 删除导致漏读**：并发删除导致下一页 skip 掉本该出现的行
3. **Race C 过滤变化导致错位**：并发修改使行跨页移动

**v1 的态度**：

- 所有 list 端点**应当** 用二级排序键 `ORDER BY <主键> DESC, <id> DESC` 消除 ties 导致的 Race A 子集
- **不应** 依赖 `REPEATABLE READ` tx 隔离（跨请求无意义，单请求性能差）
- 前端**应当** 在翻页期间显示 "数据可能已更新，建议刷新"（产品层协议，非框架职责）

需要真正 snapshot-consistent 的场景（feed / timeline / export）**必须** 等 §11 Phase 3 的 cursor 分页上线。

### 8.2 总行数与实际行数的不一致

由于 Race B，可能出现 `rows.len() + offset > total`（读完 rows 之后 count 才执行，期间有行被删）。

**v1.1 已实施**（2026-04-11）：所有 4 个 `find_page` 在收尾处调用 `PaginationParams::reconcile_total(observed_total, rows.len(), p.offset)`，把 `total` 至少提升到 `offset + rows.len()`，消除 "Page N of fewer-than-N" 这种自相矛盾。reconcile_total 是 `PaginationParams` 的静态方法，可单测（3 个 tests 覆盖 happy path / race-shrunk / negative input）。

---

## 9. 安全契约

### 9.1 总数字段的信息泄露

`Page<T>.total: u64` 暴露的语义是"当前 filter 条件下的精确行数"。在多租户系统中这是**已知的信息泄露面**。

**v1 的态度**：

- 所有 list 端点**必须** 走 RBAC / tenant-scope guard，阻止未授权调用
- **不得** 把 `total` 当作敏感数据保护——它是设计上的 wire 契约一部分
- 需要真正隐藏总数的场景**必须** 等 §11 Phase 2 的 `HasMore` 模式上线

### 9.2 SQL 注入面

分页字段（`page_num` / `page_size`）类型严格 `u32`，**不可能** 参与 SQL 注入。但过滤字段**必须** 通过 sqlx `.bind()` 绑定，**不得** 用 `format!` 拼接。

### 9.3 DoS 防护

- `PAGE_SIZE_MAX = 200` 是硬上限
- `PAGE_NUM_MAX = 10_000` 是硬上限
- validator 在 HTTP 层拒绝越界
- `PaginationParams::from` 在 DAO 层二次 clamp（防止跳过 HTTP 层的路径）

**不得** 为某个"特殊"端点把 `PAGE_SIZE_MAX` 临时调高。导出类端点必须走独立 `ExportQuery`（Phase 3.2）。

---

## 10. 禁止模式

| 模式                                                  | 为什么禁止                                   | 替代方案                                    |
| ----------------------------------------------------- | -------------------------------------------- | ------------------------------------------- |
| `impl From<ListUserDto> for UserListFilter`           | 隐藏字段 mapping                             | 显式字段 literal 构造                       |
| `find_page(pool, page_num, page_size, ...)` 位置参数  | 参数爆炸；没有 filter struct 就没有 DAO 隔离 | 定义 `XxxListFilter` struct                 |
| `Page::new(rows, total as u64, ...)` 散落             | cast 散落；绕过 `into_page` 无法统一未来演化 | 只用 `p.into_page(rows, total)`             |
| 在 repo 里调另一个 repo                               | 破坏 DAO rule 2                              | service 层协调                              |
| filter struct 带 `Clone` derive                       | 0 调用点的无用 derive                        | 只 derive `Debug`                           |
| `#[serde(rename_all = ...)]` 不一致                   | wire 形状漂移                                | 统一 `camelCase`                            |
| rows SQL 和 count SQL 的 WHERE 手写两份               | 漂移风险                                     | 提取 `const WHERE_SQL`                      |
| `paginate_with_tracing` / `ListQuery<F,S,P>` 泛型包装 | < 6 调用点时过度抽象                         | 手写，复制到阈值                            |
| `total: 0` + 非空 rows 作为"软错误"                   | 语义崩坏                                     | `debug_assert!(rows.len() as i64 <= total)` |
| 硬编码页面上限（`.clamp(1, 200)`）                    | 双源头漂移                                   | 用 `PAGE_SIZE_MAX` 常量                     |

---

## 11. 演化路线

| Phase              | 内容                                                                                                                                                                                              | 触发条件                          | 预估成本              |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- | --------------------- |
| **v1.0**（本规范） | Filter 嵌 PageQuery、into_page 接 i64、常量单源头、doc comment 标准化                                                                                                                             | 立即                              | ~250 LOC              |
| **v1.1**           | `with_timeout` helper、slow query warn、i18n 参数化（`get_by_key_with_json_params`）、post-condition runtime truncate、`reconcile_total`、`check_no_seq_scan` / `assert_index_exists` 测试 helper | ✅ 2026-04-11 已实施              | 实际 ~450 LOC         |
| **v2.0**           | `Page.total: Option<u64>` + `has_more` + `HasMore` 模式                                                                                                                                           | 出现信息泄露审计或 C 端 feed 需求 | wire break + 前端适配 |
| **v3.0**           | Cursor pagination primitive（`CursorQuery<C>` / `CursorPage<T>`）                                                                                                                                 | 深翻页 p99 > 1s 或 export 需求    | 独立一周              |
| **v3.1**           | Sort framework（`SortQuery` + `SortableList` trait + 列白名单）                                                                                                                                   | 产品提出用户可选排序列需求        | ~3 天                 |
| **v3.2**           | `ExportQuery` + 流式响应（突破 `PAGE_SIZE_MAX`）                                                                                                                                                  | 后台导出超 10k 行需求             | 独立一周              |

**触发器表是 governance 工具**：任何人看到 "我觉得该加 cursor 了" 的冲动时，先对照这张表——如果触发条件没满足，**不加**。

---

## 12. 新增 list 端点的样板

**Step 1 — DTO**（`system/<module>/dto.rs`）

```rust
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ListXxxDto {
    pub field_a: Option<String>,
    pub field_b: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
```

**Step 2 — Filter**（`domain/<module>_repo.rs` 顶部）

```rust
#[derive(Debug)]
pub struct XxxListFilter {
    pub field_a: Option<String>,
    pub field_b: Option<String>,
    /// PageQuery validator attrs only fire at HTTP extraction.
    pub page: PageQuery,
}
```

**Step 3 — WHERE 单源**

```rust
const XXX_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR field_a LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR field_b = $3)";
```

**Step 4 — `find_page`**（照搬 §4.3 的样板，替换表名/字段名）

**Step 5 — Service list**（照搬 §4.2 的样板）

**Step 6 — Handler 路由** + `require_permission!`

**Step 7 — 在 `docs/framework/framework-pagination-indexes.md` 追加索引声明**

**Step 8 — Integration test** 覆盖空结果、尾页、深翻页（offset > total）三个边界

---

## 13. PR checklist

提交 list 端点相关 PR 时**必须** 逐项自查：

- [ ] DTO 用了 `PageQuery` + `#[serde(flatten)]` + `#[validate(nested)]`
- [ ] DTO / Filter 无 `Clone` derive
- [ ] Filter struct 嵌入 `PageQuery`，不是平铺 `page_num / page_size`
- [ ] Filter 定义在 `<module>_repo.rs` 顶部，紧邻消费方法
- [ ] Service 层用显式字段 literal 构造 filter（无 `From` 隐藏 mapping）
- [ ] `find_page` doc comment 有 `## Expected indexes` section
- [ ] `find_page` doc comment 有 `## Consistency caveats` section
- [ ] `find_page` doc comment 有 `## Performance expectation` section
- [ ] `#[tracing::instrument]` 标注了 4 个标准字段（page_num, page_size, rows_len, total）——tenant_id 由 root span 自动继承，不得重复声明
- [ ] WHERE 子句提取成 `const`，rows 和 count 查询共享
- [ ] 使用 `PaginationParams::from(filter.page.page_num, filter.page.page_size)`
- [ ] 使用 `p.into_page(rows, total)` 收尾，无 `total as u64` 裸 cast
- [ ] 有 `debug_assert!(rows.len() as i64 <= p.limit)` post-condition
- [ ] 有 integration test 覆盖空结果 / 尾页 / 超范围页
- [ ] `docs/framework/framework-pagination-indexes.md` 已追加索引声明
- [ ] 对应 migration 已创建索引
- [ ] `cargo clippy --all-targets -- -D warnings` 干净
- [ ] `cargo test --workspace` 全绿
- [ ] smoke 脚本全绿

---

## 附录 A：v1.0 明确不管的事

1. cursor pagination
2. sort framework
3. `total: Option<u64>` / `HasMore` 模式
4. 查询 timeout（v1.1 引入）
5. seq-scan regression 测试（v1.1 引入）
6. slow query warn（v1.1 引入）
7. tenant-aware page policy
8. export / streaming endpoint
9. GraphQL Relay cursor 兼容
10. OpenAPI schema 导出

每一条都记录在 §11 触发器表里，对应条件未满足**不得** 提前做。

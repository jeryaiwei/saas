# SaaS Rust Backend - 项目文档

## 项目状态

| 指标     | 值                                                                                      |
| -------- | --------------------------------------------------------------------------------------- |
| 端点数   | 53 (auth 4 + role 11 + user 11 + tenant 5 + tenant_package 6 + menu 9 + dept 7)         |
| 测试数   | 267                                                                                     |
| Smoke    | role 14 + user 16 + tenant 13 + menu 10 + dept 8 = 61 steps                             |
| 框架规范 | 5 份 (pagination / error-envelope / observability / repo-executor / pagination-indexes) |
| 业务设计 | 5 份 (role / user / tenant / menu / dept)                                               |
| 实施计划 | 8 份 (6 已执行 + 2 未执行)                                                              |

---

## 文档索引

### 框架规范 (`docs/framework/`)

定义了所有模块必须遵守的跨切面契约。新增业务模块前应读完这些规范。

| 文档                                                              | 版本                     | 内容                                                                                                                          |
| ----------------------------------------------------------------- | ------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| [pagination-spec](framework/framework-pagination-spec.md)         | v1.0 + v1.1              | 分页类型 (PageQuery / PaginationParams / Page), filter struct 规范, into_page, with_timeout, reconcile_total, slow-query warn |
| [pagination-indexes](framework/framework-pagination-indexes.md)   | v1.0                     | 每个 find_page 的索引依赖注册表                                                                                               |
| [error-envelope-spec](framework/framework-error-envelope-spec.md) | v1.0                     | 统一 wire envelope (ApiResponse), AppError 5 variant, ResponseCode 段位, i18n 命名空间, FieldError 路径格式                   |
| [observability-spec](framework/framework-observability-spec.md)   | v1.0 (plan 已写, 未执行) | root span 自动注入 request_id/tenant_id/user_id, span 字段命名宪法, event level 宪法, metric 命名约定, cardinality 预算       |
| [repo-executor-spec](framework/framework-repo-executor-spec.md)   | v1.0                     | impl PgExecutor vs &PgPool vs &mut Transaction 选择规则, service 层事务边界, 禁止模式                                         |

### 业务模块设计 (`docs/specs/`)

每个模块的端点列表, DTO 字段, 数据层方法, 错误码, 测试策略。

| 文档                                                                | 模块             | 端点 | 状态   |
| ------------------------------------------------------------------- | ---------------- | ---- | ------ |
| [role-module-design](specs/2026-04-10-phase1-role-module-design.md) | Role             | 11   | 已实现 |
| [user-module-design](specs/2026-04-11-phase1-user-module-design.md) | User             | 11   | 已实现 |
| [tenant-module-design](specs/2026-04-12-tenant-module-design.md)    | Tenant + Package | 12   | 已实现 |
| [menu-module-design](specs/2026-04-12-menu-module-design.md)        | Menu             | 9    | 已实现 |
| [dept-module-design](specs/2026-04-12-dept-module-design.md)        | Dept             | 7    | 已实现 |

### 实施计划 (`docs/plans/`)

| 文档                             | 状态                                       |
| -------------------------------- | ------------------------------------------ |
| role-module-plan                 | 已执行                                     |
| user-module-plan                 | 已执行                                     |
| tenant-module-plan               | 已执行                                     |
| menu-module-plan                 | 已执行                                     |
| dept-module-plan                 | 已执行                                     |
| framework-pagination-v1.1        | 已执行                                     |
| framework-error-envelope-v1.0    | 已执行                                     |
| **framework-observability-v1.0** | **未执行** (8 tasks / 41 steps / ~200 LOC) |

### 其他

| 文档                                                       | 内容                                                                 |
| ---------------------------------------------------------- | -------------------------------------------------------------------- |
| [phase0-schema-reference.sql](phase0-schema-reference.sql) | 原始 PostgreSQL schema (Phase 0 快照)                                |
| [archive/](archive/)                                       | 历史 RESUME handoff 文档 (role / user 模块跨 session 交接用, 已完成) |

---

## Crate 结构

```
server-rs/
├── crates/
│   ├── framework/     跨切面基础设施 (config, context, error, extractors,
│   │                  i18n, infra, middleware, response, telemetry, testing)
│   ├── modules/       业务模块 (auth, system/role, system/user, system/tenant,
│   │                  system/tenant_package, system/menu, system/dept)
│   │                  + domain 层 (entities, *_repo, validators, constants)
│   └── app/           二进制入口 (main.rs, middleware 组装, CORS, shutdown)
├── config/            YAML 配置 (default + development)
├── i18n/              国际化 (zh-CN.json + en-US.json)
├── scripts/           Smoke 测试脚本 (5 个)
└── docs/              本目录
```

---

## 下一步

1. **P3**: Dict / Config / Post 模块 (批量 CRUD)
2. **P5**: Observability v1.0 执行 (root span request_id 传播)
3. **P6**: Vue Web 灰度切换

**触发器表** (不主动做, 等条件满足):

| 项目                               | 触发条件                 |
| ---------------------------------- | ------------------------ |
| Pagination v2.0 (total: Option)    | 信息泄露审计 / C 端 feed |
| Error v2.0 (业务错误参数化)        | ACCOUNT_LOCKED 实装      |
| Cursor pagination                  | 深翻页 p99 > 1s          |
| Sort framework                     | 产品要求可选排序列       |
| OpenAPI schema                     | 引入 utoipa              |
| Tenant switching (scope B)         | 后台管理员切换租户       |
| Enterprise certification (scope C) | C 端用户变企业           |

---

## 常用命令

```bash
# 开发
cargo build -p app
./target/debug/app                     # 端口 18080

# 测试
cargo test --workspace                 # 267 tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check

# Smoke (需要 app 在跑 + DB 在跑)
bash scripts/smoke-role-module.sh      # 14 steps
bash scripts/smoke-user-module.sh      # 16 steps
bash scripts/smoke-tenant-module.sh    # 13 steps
bash scripts/smoke-menu-module.sh      # 10 steps
bash scripts/smoke-dept-module.sh      #  8 steps

# Git
git remote -v                          # github: jeryaiwei/saas.git
git push github main
```

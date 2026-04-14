# Rust Server 压力测试方案设计

**日期**：2026-04-14
**作者**：jerry（与 Claude Code 协作）
**适用范围**：`server-rs/`（192 端点 / 31 模块 / NestJS → Rust 重写版）

---

## 1. 目标

同时满足四项目标，单一方案覆盖：

| 编号 | 目标 | 产出 |
|------|------|------|
| A | **容量规划** | 单实例最大可承载 RPS / 并发 VU，用于生产部署决策 |
| B | **SLO 验证** | 在目标 RPS 下验证 p95/p99 延迟、错误率达标 |
| C | **性能基线** | 建立可复现基线，后续 PR 回归对比 |
| D | **瓶颈定位** | 压到极限定位短板（DB pool / Redis / JWT / 日志 IO 等） |

不含 E（与 NestJS 对比），但基线数据可事后用于对比。

---

## 2. 工具选型

**主工具：k6**

| 候选 | 评估 | 决定 |
|------|------|------|
| **k6** | JS 场景编排 / 内置 thresholds = SLO 断言 / stages 爬坡 / Prometheus 输出 | ✅ 主工具 |
| oha | Rust 写的极简压测器，单端点吞吐高 | ⚠️ 辅助：仅用于 `/health` 纯 HTTP 上限对照 |
| wrk / vegeta | scenario 编排能力弱 | ❌ 不用 |
| goose (Rust) | 同进程高并发，但 DSL 粗糙、生态小 | ❌ 备胎 |

**附加要求**：k6 版本 ≥ 0.50（使用 `constant-arrival-rate` / `ramping-arrival-rate` 执行器），通过 `brew install k6` 或容器镜像安装。

---

## 3. 场景分组（18 个端点，分层抽样）

| 组 | 代表端点 | 权重 | 说明 |
|---|---|---|---|
| **baseline** | `GET /health` | 5% | 无 DB 无业务，纯路由+中间件基线 |
| **auth** | `POST /auth/login` | 2% | 含 bcrypt + JWT 签名开销 |
| **read-list** | `GET /system/user/list`、`/system/role/list`、`/system/menu/list`、`/system/dept/list`、`/system/dict/data/list`、`/message/notice/list` | 40% | 典型分页读；mock 前端最高频 |
| **read-detail** | `GET /system/user/{id}`、`/system/role/{id}`、`/system/menu/{id}`、`/system/dept/{id}` | 20% | 单记录读 |
| **complex-read** | `GET /monitor/operlog/list`（多 join + 10 万行分页）、`/monitor/online-user/list`、`/monitor/server-info` | 10% | 慢查询 / 内存数据 |
| **write** | `POST /system/user`、`PUT /system/user`、`POST /system/role`、`POST /message/notice` | 15% | 事务 + operlog 层自动写 |
| **file** | `POST /file/upload`（100 KB 固定 payload，local storage） | 5% | 磁盘 IO + multipart 解析 |
| **cascade** | `DELETE /system/user/{id}`（自动重建，见 §6） | 3% | 跨模块级联（触发 user_role / audit） |

**合计权重 100%**。权重依据：read-heavy 后台管理系统的经验流量分布（读写约 65:15，其余为健康检查/认证/文件/复杂查询）。

---

## 4. 测试剖面（Pattern）

四种 Pattern 共用 §3 的场景定义，仅 executor 参数不同：

| Pattern | k6 executor | 参数 | 对应目标 | 时长 |
|---|---|---|---|---|
| **Smoke** | `constant-vus` | 1 VU | 脚本自检 | 1 min |
| **Load** | `constant-arrival-rate` | 目标 RPS 恒定（见 §5） | B. SLO 验证 | 5 min |
| **Stress** | `ramping-arrival-rate` | 50 → 100 → 250 → 500 → 1000 → 2000 RPS 阶梯，每阶 90 s | A. 容量 + D. 瓶颈 | 10 min |
| **Soak**（可选） | `constant-arrival-rate` | Load 目标 RPS × 30 min | 内存泄漏、连接池耗尽、FD 泄漏 | 30 min |

---

## 5. SLO 阈值（k6 thresholds）

所有 threshold 写入 `k6/thresholds.js`，超标即 k6 exit code ≠ 0，CI 可直接用作门禁。

| 场景组 | p95 | p99 | error rate |
|---|---|---|---|
| baseline | 20 ms | 50 ms | < 0.1% |
| auth | 300 ms | 600 ms | < 0.5% |
| read-list | **200 ms** | 500 ms | < 1% |
| read-detail | **100 ms** | 300 ms | < 1% |
| complex-read | **500 ms** | 1 s | < 1% |
| write | **500 ms** | 1 s | < 1% |
| file | 1 s | 2 s | < 2% |
| cascade | 800 ms | 2 s | < 1% |
| **全局** | — | — | `http_req_failed < 1%` |

**Load Pattern 默认目标 RPS**：500（初版拍脑袋值，实际 Stress 跑完后根据 "无 SLO 违规的最高 RPS" 回填）。

**所有阈值均为 v1 初值，预期在 M4 基线跑完后按实测 p95 × 1.3 回填为 "合理上限"**。

---

## 6. 环境与拓扑

```text
┌─────────────── Mac (服务端, macOS) ─────────────┐         ┌─── Client 机 (Linux/另一台 Mac) ───┐
│  app (release build, RUST_LOG=warn)             │         │                                      │
│    ├─ PostgreSQL 16 (docker)                    │ ◄─LAN── │  k6 v0.50+                           │
│    └─ Redis 7 (docker)                          │  HTTP   │  oha (对照组)                         │
│  scripts/stress-collect.sh 后台采样             │         │  scripts/stress-run.sh 编排           │
└─────────────────────────────────────────────────┘         └──────────────────────────────────────┘
```

**服务端约束**：

- **必须** `cargo build -p app --release` 启动；debug build 性能差 10x，数据不可用。
- `RUST_LOG=warn`，关闭 info/debug；保留 slow-query warn（用于捕获慢 SQL）。
- `config/production.yaml` 或环境变量覆写关键值（压测专用 profile，在 M2 阶段落地）。
- PG `max_connections >= 200`；app `database.pool.max_connections` 与之匹配（当前默认值需在 M1 确认）。
- ulimit 提高（macOS：`ulimit -n 65536`）。

**客户端约束**：

- 与服务端同 LAN，`ping <服务端IP>` 的 p95 < 1 ms。
- 本机无其他负载。
- k6 版本 ≥ 0.50。

**Mac 本机跑的数据仅用于相对比较**（不同代码版本之间、不同 Pattern 之间）；**绝对容量数字需在生产规格 Linux VM 上重测**，这是本次方案不做但方案兼容的下一步。

---

## 7. 数据准备（seed）

压测前数据量必须充足，否则分页只返回 0-1 条，测不出索引 / join / 锁的真实开销。

| 表 | 目标数量 | 备注 |
|---|---|---|
| tenant | 10 | `stress-t-{0..9}` |
| user | 每租户 1,000（共 10,000） | `stress-u-{tenant}-{i}` |
| role | 每租户 20（共 200） | |
| dept | 每租户 3 层 × 5 叉 = 125（共 1,250） | 测递归查询 |
| menu | 共用 200 | |
| operlog | 10 万 | 测复杂分页 |

**实现**：`server-rs/stress/scripts/stress-seed.sh`

- 优先走 server 的批量 API（保持路径一致）
- operlog 10 万太慢 → 直接 SQL `INSERT ... SELECT generate_series` 批插
- 幂等：重跑前先删除 `name LIKE 'stress-%'` 的行

---

## 8. 指标采集

**客户端（k6 内置）**：每端点 p50 / p95 / p99、RPS、error rate、bytes/s → 输出 JSON + stdout 摘要。

**服务端三层**（每 5 s 采样写 CSV，由 `stress-collect.sh` 后台进程启动）：

| 层 | 工具 | 采集项 |
|---|---|---|
| 进程 | `pidstat -p $(pgrep app) 5` | CPU% / RSS / context-switch / threads |
| 系统 | `vmstat 5` + `iostat -x 5` | CPU / 内存 / IO wait / 磁盘 util |
| PG | `SELECT count(*) FROM pg_stat_activity ...` + `pg_stat_statements` 前 10 | 活动连接 / 慢 SQL |
| Redis | `redis-cli info stats` + `info memory` | ops/s / used_memory / evicted_keys |

---

## 9. 报告产物

每次运行产出 `results/<YYYYMMDD-HHMM>/`：

```text
results/20260414-1530/
├── meta.json                 # git sha, k6 version, env vars, pattern
├── k6-summary.json           # k6 --summary-export
├── k6-metrics.csv            # k6 --out csv
├── server-pidstat.csv
├── server-vmstat.csv
├── server-iostat.csv
├── pg-stats.csv
├── redis-stats.csv
└── report.md                 # stress-report.sh 生成的人类可读汇总
```

`report.md` 包含：SLO pass/fail 表、Top 5 慢端点、服务端资源高峰、PG 慢 SQL Top 10、对比上一次运行的 p95 diff。

---

## 10. 目录结构

```text
server-rs/stress/
├── README.md                 # 跑法手册（prerequisites / quickstart）
├── k6/
│   ├── lib/
│   │   ├── config.js         # BASE_URL / TENANT_ID / 目标 RPS 从 env
│   │   ├── auth.js           # 登录 + token 缓存（10 虚拟用户轮换）
│   │   └── data.js           # 随机 payload / ID 采样
│   ├── scenarios/
│   │   ├── baseline.js
│   │   ├── auth.js
│   │   ├── read-list.js
│   │   ├── read-detail.js
│   │   ├── complex-read.js
│   │   ├── write.js
│   │   ├── file.js
│   │   ├── cascade.js
│   │   └── mixed.js          # 按 §3 权重混合，用于 Load/Stress/Soak
│   └── thresholds.js         # 统一 SLO 阈值导出
├── scripts/
│   ├── stress-seed.sh
│   ├── stress-clean.sh       # 清理 stress-% 数据
│   ├── stress-collect.sh     # 后台启动服务端采样
│   ├── stress-run.sh         # 编排：build? → seed → start collector → k6 → stop collector → report
│   └── stress-report.sh      # CSV → report.md
└── results/                  # .gitignore，每次运行一个子目录
```

---

## 11. 实施里程碑（建议拆 4 个 PR）

| 里程碑 | 内容 | 验收标准 |
|---|---|---|
| **M1** | 目录骨架 + k6 smoke 脚本（health + login + user list） + README | `bash scripts/stress-run.sh smoke` 跑通 1 min，exit 0 |
| **M2** | 完整 8 个场景脚本（baseline/auth/read-list/read-detail/complex-read/write/file/cascade）+ mixed + thresholds + seed 脚本 | `bash scripts/stress-run.sh load` 跑 5 min，SLO 全 pass |
| **M3** | 服务端采样 + 报告生成（`stress-collect.sh` + `stress-report.sh`） | 一键出 `results/<ts>/report.md`，含所有指标段 |
| **M4** | Stress + Soak Pattern + 跨版本基线对比工具 | `stress-run.sh stress` 找到最大 RPS 且报告自动列出瓶颈候选 |

---

## 12. 非目标（Out of Scope）

以下内容不在本方案范围，视需另开设计：

- **分布式压测**（k6 cluster / k6-operator on k8s）
- **生产级 Linux VM 上的容量测试**（本方案兼容，但环境搭建另算）
- **与 NestJS 版本横向对比**（目标 E，未选）
- **前端（web/app）端到端性能测试**
- **安全压测**（SQL 注入 fuzzing、auth brute-force 等）

---

## 13. 风险与缓解

| 风险 | 影响 | 缓解 |
|------|------|------|
| Mac 压测客户端抢服务端 CPU | 数据失真 | **强制** 另一台机器做 Client（方案 B） |
| release build 遗漏导致数据偏差 10x | 全盘无效 | `stress-run.sh` 启动时检查 `target/release/app` 的 mtime 并提示 |
| seed 数据被业务 smoke 脚本误伤 | 数据污染 | 所有 seed name 加 `stress-` 前缀，业务 smoke 不碰 |
| PG slow log 日志膨胀 | 磁盘打满 | 压测专用 profile 关闭慢日志文件输出，仅保留 warn span |
| operlog 10 万行插完占空间 | 磁盘 | `stress-clean.sh` 提供一键清理 |
| SLO 阈值拍脑袋 | 门禁过严或过松 | M4 基线跑完后按 p95×1.3 回填 |

---

## 14. 用户确认记录（2026-04-14）

| # | 项 | 决定 |
|---|---|---|
| 1 | 目标 Load RPS | 500（默认，v1 初值；M4 实测后按 "无 SLO 违规最高 RPS" 回填） |
| 2 | SLO p95 阈值（§5 表） | 按默认值落地，M4 基线跑完后按实测 p95 × 1.3 回填 |
| 3 | Client 机器 | 进入 M1 前由用户提供（Mac / Linux VM 均可），方案兼容 |
| 4 | Soak Pattern | **v1 不做**（保留 §4 设计位，后续按需启用） |
| 5 | vs NestJS 对比 | **v1 不做**（基线可复用，后续按需） |

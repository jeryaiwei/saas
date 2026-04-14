# Rust Server 压力测试 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `server-rs/stress/` 下落地一套可复现的 k6 压力测试工具链，覆盖容量规划 / SLO 验证 / 性能基线 / 瓶颈定位四个目标。

**Architecture:** k6 主压测 + oha 对照，bash 脚本编排；服务端三层指标采样 (pidstat/vmstat + PG/Redis 查询)；每次运行产出时间戳子目录 + markdown 报告；4 个里程碑递进（smoke → load → collector → stress）。

**Tech Stack:** k6 v0.50+ (JS scenarios), bash, PostgreSQL psql, oha (Rust), pidstat/vmstat/iostat, redis-cli

**Spec:** [docs/superpowers/specs/2026-04-14-rust-stress-test-design.md](../specs/2026-04-14-rust-stress-test-design.md)

---

## File Structure

每个文件职责明确、可独立理解：

```text
server-rs/stress/
├── README.md                    # 运行手册（前置依赖、命令、排错）
├── .gitignore                   # 忽略 results/
├── k6/
│   ├── thresholds.js            # SLO 阈值常量统一导出（单一事实源）
│   ├── lib/
│   │   ├── config.js            # 从 env 读 BASE_URL / ADMIN_USER / ADMIN_PASS / RPS
│   │   ├── auth.js              # login 函数 + VU 级 token 缓存
│   │   └── data.js              # 随机 payload 与 ID 池（read-detail/cascade 用）
│   └── scenarios/
│       ├── baseline.js          # GET /health（无 DB）
│       ├── auth.js              # POST /auth/login 纯登录压测
│       ├── read-list.js         # 6 个 list 端点轮询
│       ├── read-detail.js       # 4 个 detail 端点（ID 随机采样）
│       ├── complex-read.js      # operlog / online-user / server-info
│       ├── write.js             # user/role/notice CRUD
│       ├── file.js              # 100KB multipart upload
│       ├── cascade.js           # DELETE /user/{id} 级联
│       └── mixed.js             # 按 spec §3 权重混合所有场景
├── scripts/
│   ├── stress-seed.sh           # SQL 批插 10 租户 / 10k 用户 / 10 万 operlog
│   ├── stress-clean.sh          # 清理 stress-% 前缀数据
│   ├── stress-collect.sh        # 后台起服务端采样进程组
│   ├── stress-run.sh            # 编排入口：smoke|load|stress 子命令
│   └── stress-report.sh         # CSV → report.md
└── results/                     # .gitignore，每次运行一个 <YYYYMMDD-HHMM>/ 子目录
    └── .gitkeep
```

**设计约束**：
- `k6/lib/*` 只写 helper，不包含 scenario 定义（单一职责）
- `k6/scenarios/*.js` 每个都可独立 `k6 run` 跑（便于调试）
- `mixed.js` 只用 scenarios 的辅助函数，不重复 HTTP 调用逻辑（DRY）
- `scripts/stress-run.sh` 是唯一对外入口；其他脚本都是它的子模块

---

## Milestones

| 里程碑 | 包含任务 | 交付 |
|---|---|---|
| **M1** | Task 1–7 | `stress-run.sh smoke` 1 分钟自检通过 |
| **M2** | Task 8–16 | `stress-run.sh load` 5 分钟 SLO 全 pass |
| **M3** | Task 17–20 | 一键产出完整 `report.md`（含服务端指标） |
| **M4** | Task 21–23 | `stress-run.sh stress` 找出最大 RPS，报告标注瓶颈 |

---

## M1: 骨架 + Smoke

### Task 1: 目录骨架 + README 起草 + .gitignore

**Files:**
- Create: `server-rs/stress/README.md`
- Create: `server-rs/stress/.gitignore`
- Create: `server-rs/stress/results/.gitkeep`

- [ ] **Step 1: 创建目录结构**

```bash
cd server-rs
mkdir -p stress/{k6/{lib,scenarios},scripts,results}
touch stress/results/.gitkeep
```

- [ ] **Step 2: 写 `.gitignore`**

`server-rs/stress/.gitignore`:

```gitignore
results/*
!results/.gitkeep
```

- [ ] **Step 3: 写 `README.md` 初版**

`server-rs/stress/README.md`:

````markdown
# Rust Server 压力测试

详细设计见 [docs/superpowers/specs/2026-04-14-rust-stress-test-design.md](../docs/superpowers/specs/2026-04-14-rust-stress-test-design.md)。

## 前置依赖

- Rust app release build 运行中：`cd server-rs && cargo build -p app --release && RUST_LOG=warn ./target/release/app`
- PostgreSQL (saas_tea / 123456) + Redis 运行中
- `k6` v0.50+：`brew install k6`
- `psql`, `python3`（已有）

## 快速开始

```bash
cd server-rs/stress

# 1 分钟烟雾测试（脚本自检）
bash scripts/stress-run.sh smoke

# 5 分钟负载测试（SLO 验证）
bash scripts/stress-run.sh load

# 10 分钟阶梯压测（容量 + 瓶颈）
bash scripts/stress-run.sh stress
```

结果输出在 `results/<YYYYMMDD-HHMM>/report.md`。

## 环境变量

| 变量 | 默认 | 说明 |
|---|---|---|
| `BASE_URL` | `http://127.0.0.1:18080/api/v1` | API 根地址 |
| `ADMIN_USER` | `admin` | 登录用户名 |
| `ADMIN_PASS` | `admin123` | 登录密码 |
| `TARGET_RPS` | `500` | Load pattern 目标 RPS |
| `PG_DSN` | `postgres://saas_tea:123456@127.0.0.1/saas_tea` | seed/采样用 |
````

- [ ] **Step 4: 验证**

```bash
cd server-rs/stress
ls -la
tree -L 3 .
```

Expected：看到 `k6/lib`, `k6/scenarios`, `scripts`, `results/.gitkeep` 完整结构，README 和 `.gitignore` 存在。

- [ ] **Step 5: Commit**

```bash
cd ..
git add server-rs/stress/README.md server-rs/stress/.gitignore server-rs/stress/results/.gitkeep
git commit -m "feat(stress): scaffold stress test directory structure"
```

---

### Task 2: `k6/lib/config.js`

**Files:**
- Create: `server-rs/stress/k6/lib/config.js`

- [ ] **Step 1: 写 config.js**

```javascript
// server-rs/stress/k6/lib/config.js
// 所有 env-driven 的运行时参数集中在这里。
// 新增 scenario 禁止直接读 __ENV，必须通过本文件。

export const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:18080/api/v1';
export const ADMIN_USER = __ENV.ADMIN_USER || 'admin';
export const ADMIN_PASS = __ENV.ADMIN_PASS || 'admin123';
export const TARGET_RPS = parseInt(__ENV.TARGET_RPS || '500', 10);

export const headersJson = (token) => ({
  'Content-Type': 'application/json',
  'Authorization': `Bearer ${token}`,
});
```

- [ ] **Step 2: 语法自检（k6 无 lint，用 node 解析）**

```bash
cd server-rs/stress
node --check k6/lib/config.js
```

Expected：无输出（语法合法）。

- [ ] **Step 3: Commit**

```bash
git add k6/lib/config.js
git commit -m "feat(stress): add k6 config helper"
```

---

### Task 3: `k6/lib/auth.js`

**Files:**
- Create: `server-rs/stress/k6/lib/auth.js`

- [ ] **Step 1: 写 auth.js**

```javascript
// server-rs/stress/k6/lib/auth.js
// 提供 login() 与 VU 级 token 缓存。
// 每个 VU 只在 setup 或首次调用时登录一次，避免把登录本身当成压测流量。

import http from 'k6/http';
import { check, fail } from 'k6';
import { BASE_URL, ADMIN_USER, ADMIN_PASS } from './config.js';

// VU 级缓存（每个虚拟用户独立 state）
let cachedToken = null;

export function login(username = ADMIN_USER, password = ADMIN_PASS) {
  const res = http.post(
    `${BASE_URL}/auth/login`,
    JSON.stringify({ username, password }),
    { headers: { 'Content-Type': 'application/json' }, tags: { scenario: '_auth_setup' } },
  );
  const ok = check(res, {
    'login 200': (r) => r.status === 200,
    'has access_token': (r) => r.json('data.access_token') !== undefined,
  });
  if (!ok) {
    fail(`login failed: status=${res.status} body=${res.body}`);
  }
  return res.json('data.access_token');
}

export function getToken() {
  if (!cachedToken) {
    cachedToken = login();
  }
  return cachedToken;
}
```

- [ ] **Step 2: 语法自检**

```bash
node --check k6/lib/auth.js
```

Expected：无输出。

- [ ] **Step 3: Commit**

```bash
git add k6/lib/auth.js
git commit -m "feat(stress): add k6 auth helper with per-VU token cache"
```

---

### Task 4: `k6/thresholds.js`（初版）

**Files:**
- Create: `server-rs/stress/k6/thresholds.js`

- [ ] **Step 1: 写 thresholds.js**

```javascript
// server-rs/stress/k6/thresholds.js
// 单一事实源：SLO 阈值在这里定义，所有 scenario 引用。
// 规则：scenario-specific 阈值用 tag 'scenario' 过滤。
// 全局阈值通用。

export const globalThresholds = {
  'http_req_failed': ['rate<0.01'],                          // 全局错误率 < 1%
};

export const perScenarioThresholds = {
  baseline:     { p95: 20,   p99: 50,   failRate: 0.001 },
  auth:         { p95: 300,  p99: 600,  failRate: 0.005 },
  'read-list':  { p95: 200,  p99: 500,  failRate: 0.01 },
  'read-detail':{ p95: 100,  p99: 300,  failRate: 0.01 },
  'complex-read': { p95: 500, p99: 1000, failRate: 0.01 },
  write:        { p95: 500,  p99: 1000, failRate: 0.01 },
  file:         { p95: 1000, p99: 2000, failRate: 0.02 },
  cascade:      { p95: 800,  p99: 2000, failRate: 0.01 },
};

// 生成 k6 thresholds 对象（tag 过滤形式）
export function buildThresholds(scenarios) {
  const out = { ...globalThresholds };
  for (const s of scenarios) {
    const t = perScenarioThresholds[s];
    if (!t) continue;
    out[`http_req_duration{scenario:${s}}`] = [`p(95)<${t.p95}`, `p(99)<${t.p99}`];
    out[`http_req_failed{scenario:${s}}`]   = [`rate<${t.failRate}`];
  }
  return out;
}
```

- [ ] **Step 2: 语法自检**

```bash
node --check k6/thresholds.js
```

Expected：无输出。

- [ ] **Step 3: Commit**

```bash
git add k6/thresholds.js
git commit -m "feat(stress): add SLO thresholds single source of truth"
```

---

### Task 5: `k6/scenarios/baseline.js`

**Files:**
- Create: `server-rs/stress/k6/scenarios/baseline.js`

- [ ] **Step 1: 写 baseline.js**

```javascript
// server-rs/stress/k6/scenarios/baseline.js
// 压 GET /health — 无 DB、无业务、纯 axum 路由 + 中间件基线。
// 独立可跑：`k6 run k6/scenarios/baseline.js`
// mixed.js 也会 import defaultRequest 复用。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL } from '../lib/config.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    baseline: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'baseline' },
    },
  },
  thresholds: buildThresholds(['baseline']),
};

export function defaultRequest() {
  // /health 不在 /api/v1 前缀下
  const healthUrl = BASE_URL.replace(/\/api\/v1\/?$/, '') + '/health';
  const res = http.get(healthUrl, { tags: { scenario: 'baseline' } });
  check(res, { 'health 200': (r) => r.status === 200 });
}

export default defaultRequest;
```

- [ ] **Step 2: 语法自检**

```bash
node --check k6/scenarios/baseline.js
```

Expected：无输出。

- [ ] **Step 3: 确认 /health 路径**

```bash
curl -sS http://127.0.0.1:18080/health | head -c 200
```

Expected：200 响应 JSON。如果路径不是 `/health`，需要先在 server 源码里 `grep -n 'path = "/health"' crates/modules/src/health/*.rs` 确认真实路径，再修正 scenario。

- [ ] **Step 4: Commit**

```bash
git add k6/scenarios/baseline.js
git commit -m "feat(stress): add baseline scenario hitting /health"
```

---

### Task 6: `k6/scenarios/read-list.js`（M1 只加 user/list 一个）

**Files:**
- Create: `server-rs/stress/k6/scenarios/read-list.js`

- [ ] **Step 1: 写 read-list.js（M1 最小版，只有 user/list）**

```javascript
// server-rs/stress/k6/scenarios/read-list.js
// 典型分页读。M1 只压一个端点做通路验证；M2 扩展到 6 个。
// 独立可跑：`k6 run k6/scenarios/read-list.js`

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    'read-list': {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'read-list' },
    },
  },
  thresholds: buildThresholds(['read-list']),
};

const endpoints = [
  '/system/user/list?pageNum=1&pageSize=10',
];

export function defaultRequest() {
  const token = getToken();
  const url = BASE_URL + endpoints[Math.floor(Math.random() * endpoints.length)];
  const res = http.get(url, { headers: headersJson(token), tags: { scenario: 'read-list' } });
  check(res, {
    'list 200': (r) => r.status === 200,
    'has rows': (r) => Array.isArray(r.json('data.rows')),
  });
}

export default defaultRequest;
```

- [ ] **Step 2: 语法自检**

```bash
node --check k6/scenarios/read-list.js
```

Expected：无输出。

- [ ] **Step 3: Commit**

```bash
git add k6/scenarios/read-list.js
git commit -m "feat(stress): add read-list scenario (M1 single endpoint)"
```

---

### Task 7: `scripts/stress-run.sh smoke` + 运行验证

**Files:**
- Create: `server-rs/stress/scripts/stress-run.sh`

- [ ] **Step 1: 写 stress-run.sh 初版（只支持 smoke）**

```bash
#!/usr/bin/env bash
# server-rs/stress/scripts/stress-run.sh
# 压测编排入口。
# 用法：bash scripts/stress-run.sh <smoke|load|stress>

set -euo pipefail

cd "$(dirname "$0")/.."

SUBCMD="${1:-smoke}"
TS="$(date +%Y%m%d-%H%M)"
OUT_DIR="results/${TS}"
mkdir -p "${OUT_DIR}"

# 环境检查
command -v k6 >/dev/null || { echo "ERROR: k6 not installed. brew install k6"; exit 1; }
BASE_URL="${BASE_URL:-http://127.0.0.1:18080/api/v1}"
curl -sSf "${BASE_URL%/api/v1}/health" > /dev/null \
  || { echo "ERROR: server not reachable at ${BASE_URL}. Start: RUST_LOG=warn ./target/release/app"; exit 1; }

# 记录元信息
cat > "${OUT_DIR}/meta.json" <<EOF
{
  "timestamp": "${TS}",
  "subcommand": "${SUBCMD}",
  "base_url": "${BASE_URL}",
  "git_sha": "$(git rev-parse --short HEAD 2>/dev/null || echo unknown)",
  "k6_version": "$(k6 version | head -1)"
}
EOF

run_smoke() {
  echo "=== smoke: baseline (1 min) ==="
  k6 run --out "json=${OUT_DIR}/baseline.json" k6/scenarios/baseline.js
  echo "=== smoke: read-list (1 min) ==="
  k6 run --out "json=${OUT_DIR}/read-list.json" k6/scenarios/read-list.js
}

case "${SUBCMD}" in
  smoke) run_smoke ;;
  load|stress) echo "ERROR: ${SUBCMD} not implemented yet (M2/M4)"; exit 2 ;;
  *) echo "usage: $0 <smoke|load|stress>"; exit 2 ;;
esac

echo ""
echo "=== done ==="
echo "results: ${OUT_DIR}"
```

- [ ] **Step 2: 加可执行权限**

```bash
chmod +x server-rs/stress/scripts/stress-run.sh
```

- [ ] **Step 3: 前置：启动 app release build**

```bash
cd server-rs
cargo build -p app --release
RUST_LOG=warn ./target/release/app &
APP_PID=$!
sleep 5  # 等 server 就绪
```

- [ ] **Step 4: 运行 smoke**

```bash
cd server-rs/stress
bash scripts/stress-run.sh smoke
```

Expected：
- 两个 k6 运行各 1 分钟，console 输出 `checks_succeeded: 100%`
- `http_req_failed.........: 0.00%`
- 最后打印 `=== done ===`
- `results/<ts>/` 下有 `meta.json`, `baseline.json`, `read-list.json`
- 退出码 0

- [ ] **Step 5: 关闭 app**

```bash
kill $APP_PID
```

- [ ] **Step 6: Commit**

```bash
cd ..
git add server-rs/stress/scripts/stress-run.sh
git commit -m "feat(stress): add run orchestrator with smoke subcommand"
```

**M1 完成门槛**：`bash scripts/stress-run.sh smoke` 退出码 0，SLO 全绿，产出 `results/<ts>/` 目录含 json 和 meta。

---

## M2: 完整 Scenarios + Seed + Load

### Task 8: `k6/lib/data.js` + ID 池

**Files:**
- Create: `server-rs/stress/k6/lib/data.js`

**职责**：`read-detail` 和 `cascade` 场景需要真实存在的 ID；通过 VU 初始化时拉一次 `list` 缓存 ID 池，避免每次请求都去列表。

- [ ] **Step 1: 写 data.js**

```javascript
// server-rs/stress/k6/lib/data.js
// ID 池 + payload 生成器。

import http from 'k6/http';
import { BASE_URL, headersJson } from './config.js';

// VU-local 缓存
const idPools = {};

// 从指定 list 端点拉 100 条 ID 缓存
export function loadIdPool(token, key, listPath, idField) {
  if (idPools[key]) return idPools[key];
  const res = http.get(`${BASE_URL}${listPath}?pageNum=1&pageSize=100`, {
    headers: headersJson(token),
    tags: { scenario: '_setup' },
  });
  const rows = res.json('data.rows') || [];
  idPools[key] = rows.map((r) => r[idField]).filter((v) => v !== undefined);
  if (idPools[key].length === 0) {
    throw new Error(`empty id pool for ${key}: seed data missing?`);
  }
  return idPools[key];
}

export function sampleId(pool) {
  return pool[Math.floor(Math.random() * pool.length)];
}

// 生成 stress 前缀名，便于清理
export function stressName(prefix) {
  return `stress-${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

// 典型 user create payload（不含 roleIds，需调用方补充）
export function userCreatePayload(roleIds = []) {
  const name = stressName('u');
  return {
    userName: name,
    nickName: name.slice(0, 28) + '-n',
    password: 'stress123',
    roleIds,
  };
}

// typical role create payload
export function roleCreatePayload() {
  const name = stressName('r');
  return {
    roleName: name,
    roleKey: name,
    roleSort: 99,
    menuIds: [],
    deptIds: [],
    dataScope: '1',
    status: '0',
  };
}

// notice create
export function noticeCreatePayload() {
  return {
    noticeTitle: stressName('notice'),
    noticeType: '1',
    noticeContent: 'stress test content',
    status: '0',
  };
}
```

- [ ] **Step 2: 语法自检 + commit**

```bash
cd server-rs/stress
node --check k6/lib/data.js
cd ..
git add server-rs/stress/k6/lib/data.js
git commit -m "feat(stress): add data helpers (id pool + payload generators)"
```

---

### Task 9: 扩展 `read-list.js` 到 6 端点

**Files:**
- Modify: `server-rs/stress/k6/scenarios/read-list.js`

- [ ] **Step 1: 替换 endpoints 数组**

把 `const endpoints = [...]` 改为：

```javascript
const endpoints = [
  '/system/user/list?pageNum=1&pageSize=10',
  '/system/role/list?pageNum=1&pageSize=10',
  '/system/menu/list',
  '/system/dept/list',
  '/system/dict/data/list?pageNum=1&pageSize=10',
  '/message/notice/list?pageNum=1&pageSize=10',
];
```

- [ ] **Step 2: 放宽 check（菜单/部门是树不返回 rows）**

把 `check(res, { ...has rows... })` 改为：

```javascript
check(res, {
  'list 200': (r) => r.status === 200,
  'code 200': (r) => r.json('code') === 200,
});
```

- [ ] **Step 3: 语法自检 + 单独跑 1 分钟验证**

```bash
cd server-rs/stress
node --check k6/scenarios/read-list.js
# app 需在跑
k6 run --duration 30s k6/scenarios/read-list.js
```

Expected：6 个端点轮询，`http_req_failed < 1%`，无 404/5xx。若某端点 404，需回到 server 代码对照真实 path。

- [ ] **Step 4: Commit**

```bash
cd ..
git add server-rs/stress/k6/scenarios/read-list.js
git commit -m "feat(stress): extend read-list to 6 endpoints"
```

---

### Task 10: `k6/scenarios/read-detail.js`

**Files:**
- Create: `server-rs/stress/k6/scenarios/read-detail.js`

- [ ] **Step 1: 写 read-detail.js**

```javascript
// server-rs/stress/k6/scenarios/read-detail.js
// 单记录读。VU init 时拉 ID 池，每次请求从池中采样。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { loadIdPool, sampleId } from '../lib/data.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    'read-detail': {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'read-detail' },
    },
  },
  thresholds: buildThresholds(['read-detail']),
};

const targets = [
  { path: '/system/user',  pool: 'user',  listPath: '/system/user/list',  idField: 'userId' },
  { path: '/system/role',  pool: 'role',  listPath: '/system/role/list',  idField: 'roleId' },
  { path: '/system/menu',  pool: 'menu',  listPath: '/system/menu/list',  idField: 'menuId' },
  { path: '/system/dept',  pool: 'dept',  listPath: '/system/dept/list',  idField: 'deptId' },
];

export function defaultRequest() {
  const token = getToken();
  const t = targets[Math.floor(Math.random() * targets.length)];
  const pool = loadIdPool(token, t.pool, t.listPath, t.idField);
  const id = sampleId(pool);
  const res = http.get(`${BASE_URL}${t.path}/${id}`, {
    headers: headersJson(token),
    tags: { scenario: 'read-detail' },
  });
  check(res, { 'detail 200': (r) => r.status === 200 });
}

export default defaultRequest;
```

- [ ] **Step 2: 语法自检 + 单跑**

```bash
cd server-rs/stress
node --check k6/scenarios/read-detail.js
k6 run --duration 30s k6/scenarios/read-detail.js
```

Expected：`checks_succeeded: 100%`。若 `empty id pool for menu`，说明 `/system/menu/list` 响应结构不是 `data.rows`。查 server `grep -rn 'fn list' crates/modules/src/system/menu/handler.rs` 确认返回类型后修 `listPath` 或加分支。

- [ ] **Step 3: Commit**

```bash
cd ..
git add server-rs/stress/k6/scenarios/read-detail.js
git commit -m "feat(stress): add read-detail scenario with id pool"
```

---

### Task 11: `k6/scenarios/complex-read.js`

**Files:**
- Create: `server-rs/stress/k6/scenarios/complex-read.js`

- [ ] **Step 1: 写 complex-read.js**

```javascript
// server-rs/stress/k6/scenarios/complex-read.js
// 多 join / 慢查询候选。
// 需要 seed 10 万 operlog 才有代表性（M2 Task 13 产出）。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    'complex-read': {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'complex-read' },
    },
  },
  thresholds: buildThresholds(['complex-read']),
};

const endpoints = [
  '/monitor/operlog/list?pageNum=1&pageSize=20',
  '/monitor/online-user/list?pageNum=1&pageSize=20',
  '/monitor/server-info',
];

export function defaultRequest() {
  const token = getToken();
  const url = BASE_URL + endpoints[Math.floor(Math.random() * endpoints.length)];
  const res = http.get(url, { headers: headersJson(token), tags: { scenario: 'complex-read' } });
  check(res, { 'ok': (r) => r.status === 200 });
}

export default defaultRequest;
```

- [ ] **Step 2: 语法自检 + commit**

```bash
cd server-rs/stress
node --check k6/scenarios/complex-read.js
cd ..
git add server-rs/stress/k6/scenarios/complex-read.js
git commit -m "feat(stress): add complex-read scenario"
```

---

### Task 12: `k6/scenarios/write.js`

**Files:**
- Create: `server-rs/stress/k6/scenarios/write.js`

- [ ] **Step 1: 写 write.js**

```javascript
// server-rs/stress/k6/scenarios/write.js
// 创建 + 更新；不做删除（删除走 cascade.js）。
// 每次 iteration：三选一（user / role / notice），随机 create 或 update。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { userCreatePayload, roleCreatePayload, noticeCreatePayload } from '../lib/data.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    write: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'write' },
    },
  },
  thresholds: buildThresholds(['write']),
};

const createOps = [
  { path: '/system/user/',          payload: () => userCreatePayload() },
  { path: '/system/role/',          payload: () => roleCreatePayload() },
  { path: '/message/notice/',       payload: () => noticeCreatePayload() },
];

export function defaultRequest() {
  const token = getToken();
  const op = createOps[Math.floor(Math.random() * createOps.length)];
  const res = http.post(
    `${BASE_URL}${op.path}`,
    JSON.stringify(op.payload()),
    { headers: headersJson(token), tags: { scenario: 'write' } },
  );
  check(res, {
    'write 2xx': (r) => r.status >= 200 && r.status < 300,
    'code 200': (r) => r.json('code') === 200,
  });
}

export default defaultRequest;
```

- [ ] **Step 2: 语法自检 + 单跑 10 秒**

```bash
cd server-rs/stress
node --check k6/scenarios/write.js
k6 run --duration 10s k6/scenarios/write.js
```

Expected：`code 200` check rate > 99%。若某个 create 返回 code ≠ 200（例如缺字段），看响应 body 补 payload。

- [ ] **Step 3: 清理测试期间创建的数据**

```bash
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -c \
  "DELETE FROM sys_user WHERE user_name LIKE 'stress-%';
   DELETE FROM sys_role WHERE role_name LIKE 'stress-%';
   DELETE FROM message_notice WHERE notice_title LIKE 'stress-%';"
```

- [ ] **Step 4: Commit**

```bash
cd ..
git add server-rs/stress/k6/scenarios/write.js
git commit -m "feat(stress): add write scenario (user/role/notice create)"
```

---

### Task 13: `scripts/stress-seed.sh` + `stress-clean.sh`

**Files:**
- Create: `server-rs/stress/scripts/stress-seed.sh`
- Create: `server-rs/stress/scripts/stress-clean.sh`

**设计决定**：10k 用户用 SQL `INSERT ... SELECT generate_series` 批插，密码用统一预计算 bcrypt hash（跳过真实 bcrypt 开销）。10 万 operlog 同样方式。

- [ ] **Step 1: 先生成一个固定 bcrypt hash 供 seed 复用**

```bash
cd server-rs
# 用 admin 的已有密码 hash（admin123 对应的 bcrypt）
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -At -c \
  "SELECT password FROM sys_user WHERE user_name='admin' LIMIT 1;"
# 记下输出，即 $admin 的 bcrypt hash，供下一步使用
```

记录输出值（形如 `$2b$...`），下面叫 `ADMIN_BCRYPT`。

- [ ] **Step 2: 写 stress-seed.sh**

```bash
#!/usr/bin/env bash
# server-rs/stress/scripts/stress-seed.sh
# 批插 10 租户 / 1 万用户 / 10 万 operlog。
# 幂等：重跑前先调用 stress-clean.sh。

set -euo pipefail

cd "$(dirname "$0")/.."

PG_DSN="${PG_DSN:-postgres://saas_tea:123456@127.0.0.1/saas_tea}"
PG_ARGS=(-h 127.0.0.1 -U saas_tea -d saas_tea)
export PGPASSWORD=123456

ADMIN_BCRYPT="$(psql "${PG_ARGS[@]}" -At -c \
  "SELECT password FROM sys_user WHERE user_name='admin' LIMIT 1;")"
if [[ -z "$ADMIN_BCRYPT" ]]; then
  echo "ERROR: cannot read admin password hash"; exit 1
fi

echo "=== seed: 10 tenants ==="
psql "${PG_ARGS[@]}" -c "
INSERT INTO sys_tenant (tenant_id, tenant_name, status, create_time, update_time)
SELECT 'stress-t-' || i, 'stress-tenant-' || i, '0', NOW(), NOW()
FROM generate_series(0, 9) AS i
ON CONFLICT (tenant_id) DO NOTHING;"

echo "=== seed: 10k users (1000 per tenant) ==="
psql "${PG_ARGS[@]}" -c "
INSERT INTO sys_user (user_id, user_name, nick_name, password, status, create_time, update_time)
SELECT gen_random_uuid(),
       'stress-u-' || t || '-' || i,
       'stress-nick-' || t || '-' || i,
       '${ADMIN_BCRYPT}',
       '0', NOW(), NOW()
FROM generate_series(0, 9) AS t,
     generate_series(0, 999) AS i
ON CONFLICT DO NOTHING;"

echo "=== seed: 100k operlog rows ==="
psql "${PG_ARGS[@]}" -c "
INSERT INTO monitor_oper_log (oper_id, title, business_type, method, request_method, oper_name, oper_url, status, cost_time, oper_time)
SELECT gen_random_uuid(),
       'stress-op-' || i,
       (i % 10)::text,
       'stress.method',
       'GET',
       'stress-admin',
       '/stress/path/' || i,
       '0',
       (random() * 100)::int,
       NOW() - (random() * interval '30 days')
FROM generate_series(1, 100000) AS i;"

echo ""
echo "=== done ==="
psql "${PG_ARGS[@]}" -c "
SELECT 'tenant' AS t, count(*) FROM sys_tenant WHERE tenant_id LIKE 'stress-%'
UNION ALL SELECT 'user', count(*) FROM sys_user WHERE user_name LIKE 'stress-%'
UNION ALL SELECT 'operlog', count(*) FROM monitor_oper_log WHERE title LIKE 'stress-%';"
```

- [ ] **Step 3: 写 stress-clean.sh**

```bash
#!/usr/bin/env bash
# server-rs/stress/scripts/stress-clean.sh
# 清理所有 stress-% 前缀数据。

set -euo pipefail
export PGPASSWORD=123456
PG_ARGS=(-h 127.0.0.1 -U saas_tea -d saas_tea)

echo "=== clean stress-% rows ==="
psql "${PG_ARGS[@]}" -c "
DELETE FROM sys_user_role WHERE user_id IN (SELECT user_id FROM sys_user WHERE user_name LIKE 'stress-%');
DELETE FROM sys_user_tenant WHERE user_id IN (SELECT user_id FROM sys_user WHERE user_name LIKE 'stress-%');
DELETE FROM sys_user WHERE user_name LIKE 'stress-%';
DELETE FROM sys_role WHERE role_name LIKE 'stress-%';
DELETE FROM message_notice WHERE notice_title LIKE 'stress-%';
DELETE FROM monitor_oper_log WHERE title LIKE 'stress-%';
DELETE FROM sys_tenant WHERE tenant_id LIKE 'stress-%';
"
echo "done."
```

- [ ] **Step 4: 加可执行权限并运行**

```bash
chmod +x server-rs/stress/scripts/stress-{seed,clean}.sh
cd server-rs/stress
bash scripts/stress-seed.sh
```

Expected：
- tenant=10, user=10000, operlog=100000
- 用时 < 30 秒
- 无错误

**若 schema 字段名与假设不符**（表名/列名不同），先 `\d sys_user` 等查表结构再对应修 SQL。此步必须实际跑通再往下。

- [ ] **Step 5: Commit**

```bash
cd ..
git add server-rs/stress/scripts/stress-seed.sh server-rs/stress/scripts/stress-clean.sh
git commit -m "feat(stress): add seed and clean scripts for stress test data"
```

---

### Task 14: `k6/scenarios/file.js` + `cascade.js` + `auth.js`

**Files:**
- Create: `server-rs/stress/k6/scenarios/file.js`
- Create: `server-rs/stress/k6/scenarios/cascade.js`
- Create: `server-rs/stress/k6/scenarios/auth.js`

- [ ] **Step 1: 写 `auth.js`（作为独立 scenario 压登录）**

```javascript
// server-rs/stress/k6/scenarios/auth.js
// 单独压 POST /auth/login，测 JWT 签名 + bcrypt 开销。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, ADMIN_USER, ADMIN_PASS } from '../lib/config.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    auth: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'auth' },
    },
  },
  thresholds: buildThresholds(['auth']),
};

export function defaultRequest() {
  const res = http.post(
    `${BASE_URL}/auth/login`,
    JSON.stringify({ username: ADMIN_USER, password: ADMIN_PASS }),
    { headers: { 'Content-Type': 'application/json' }, tags: { scenario: 'auth' } },
  );
  check(res, {
    'login 200': (r) => r.status === 200,
    'has token': (r) => r.json('data.access_token') !== undefined,
  });
}

export default defaultRequest;
```

- [ ] **Step 2: 写 `file.js`**

```javascript
// server-rs/stress/k6/scenarios/file.js
// 100KB multipart upload。先确认端点：/file/upload 或 /system/file/upload？
// 查 server: grep -rn '/upload' crates/modules/src/system/upload/

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    file: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'file' },
    },
  },
  thresholds: buildThresholds(['file']),
};

// 100KB 伪文件内容（一次生成，多次复用）
const payload = 'x'.repeat(100 * 1024);

export function defaultRequest() {
  const token = getToken();
  const res = http.post(
    `${BASE_URL}/file/upload`,
    { file: http.file(payload, 'stress.txt', 'text/plain') },
    { headers: { 'Authorization': `Bearer ${token}` }, tags: { scenario: 'file' } },
  );
  check(res, { 'upload 200': (r) => r.status === 200 });
}

export default defaultRequest;
```

- [ ] **Step 3: 验证 upload 端点真实路径**

```bash
cd server-rs
grep -rn '#\[utoipa::path.*upload' crates/modules/src/system/upload/ | head -5
```

如果 path 不是 `/file/upload`，修 `file.js` 中 URL。

- [ ] **Step 4: 写 `cascade.js`**

```javascript
// server-rs/stress/k6/scenarios/cascade.js
// 创建 + 立即删除 → 触发 sys_user_role / sys_user_tenant 级联。
// 不依赖 seed 数据（自产自删），避免污染。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { userCreatePayload } from '../lib/data.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    cascade: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'cascade' },
    },
  },
  thresholds: buildThresholds(['cascade']),
};

export function defaultRequest() {
  const token = getToken();
  // create
  const createRes = http.post(
    `${BASE_URL}/system/user/`,
    JSON.stringify(userCreatePayload()),
    { headers: headersJson(token), tags: { scenario: 'cascade', op: 'create' } },
  );
  const userId = createRes.json('data.userId');
  if (!userId) { return; }
  // delete (cascade)
  const delRes = http.del(
    `${BASE_URL}/system/user/${userId}`,
    null,
    { headers: headersJson(token), tags: { scenario: 'cascade', op: 'delete' } },
  );
  check(delRes, { 'delete 200': (r) => r.status === 200 });
}

export default defaultRequest;
```

- [ ] **Step 5: 语法自检 3 文件**

```bash
cd server-rs/stress
node --check k6/scenarios/auth.js
node --check k6/scenarios/file.js
node --check k6/scenarios/cascade.js
```

- [ ] **Step 6: Commit**

```bash
cd ..
git add server-rs/stress/k6/scenarios/auth.js server-rs/stress/k6/scenarios/file.js server-rs/stress/k6/scenarios/cascade.js
git commit -m "feat(stress): add auth / file / cascade scenarios"
```

---

### Task 15: `k6/scenarios/mixed.js`

**Files:**
- Create: `server-rs/stress/k6/scenarios/mixed.js`

**设计**：mixed.js 按 spec §3 权重混合所有场景。k6 的做法是每个 scenario 独立 executor，用不同的 `rate` 模拟权重。

- [ ] **Step 1: 写 mixed.js**

```javascript
// server-rs/stress/k6/scenarios/mixed.js
// 按 spec §3 权重混合所有场景。用于 load / stress / soak pattern。
// 每个子 scenario 独立 constant-arrival-rate executor，rate = TARGET_RPS * weight。

import { TARGET_RPS } from '../lib/config.js';
import { buildThresholds } from '../thresholds.js';

import { defaultRequest as baselineReq } from './baseline.js';
import { defaultRequest as authReq } from './auth.js';
import { defaultRequest as readListReq } from './read-list.js';
import { defaultRequest as readDetailReq } from './read-detail.js';
import { defaultRequest as complexReadReq } from './complex-read.js';
import { defaultRequest as writeReq } from './write.js';
import { defaultRequest as fileReq } from './file.js';
import { defaultRequest as cascadeReq } from './cascade.js';

const WEIGHTS = {
  baseline: 0.05,
  auth: 0.02,
  'read-list': 0.40,
  'read-detail': 0.20,
  'complex-read': 0.10,
  write: 0.15,
  file: 0.05,
  cascade: 0.03,
};

function scenarioFor(name, exec) {
  const rate = Math.max(1, Math.round(TARGET_RPS * WEIGHTS[name]));
  return {
    executor: 'constant-arrival-rate',
    rate,
    timeUnit: '1s',
    duration: '5m',
    preAllocatedVUs: Math.max(10, rate),
    maxVUs: Math.max(50, rate * 4),
    exec,
    tags: { scenario: name },
  };
}

export const options = {
  scenarios: {
    baseline:       scenarioFor('baseline',     'baseline'),
    auth:           scenarioFor('auth',         'auth'),
    'read-list':    scenarioFor('read-list',    'readList'),
    'read-detail':  scenarioFor('read-detail',  'readDetail'),
    'complex-read': scenarioFor('complex-read', 'complexRead'),
    write:          scenarioFor('write',        'write'),
    file:           scenarioFor('file',         'file'),
    cascade:        scenarioFor('cascade',      'cascade'),
  },
  thresholds: buildThresholds(Object.keys(WEIGHTS)),
};

export const baseline = baselineReq;
export const auth = authReq;
export const readList = readListReq;
export const readDetail = readDetailReq;
export const complexRead = complexReadReq;
export const write = writeReq;
export const file = fileReq;
export const cascade = cascadeReq;
```

- [ ] **Step 2: 语法自检**

```bash
cd server-rs/stress
node --check k6/scenarios/mixed.js
```

- [ ] **Step 3: Commit**

```bash
cd ..
git add server-rs/stress/k6/scenarios/mixed.js
git commit -m "feat(stress): add weighted mixed scenario"
```

---

### Task 16: 扩展 `stress-run.sh` 支持 `load` 子命令 + 验证

**Files:**
- Modify: `server-rs/stress/scripts/stress-run.sh`

- [ ] **Step 1: 修改 stress-run.sh 加 load 分支**

在 `run_smoke()` 之后加：

```bash
run_load() {
  echo "=== seed check ==="
  local user_cnt
  user_cnt=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -At \
    -c "SELECT count(*) FROM sys_user WHERE user_name LIKE 'stress-%';")
  if [[ "${user_cnt}" -lt 5000 ]]; then
    echo "ERROR: seed data insufficient (${user_cnt} users). Run: bash scripts/stress-seed.sh"
    exit 1
  fi
  echo "seed ok: ${user_cnt} stress users"

  echo "=== load: mixed 5min @ ${TARGET_RPS:-500} RPS ==="
  TARGET_RPS="${TARGET_RPS:-500}" \
    k6 run \
      --out "json=${OUT_DIR}/mixed.json" \
      --summary-export "${OUT_DIR}/summary.json" \
      k6/scenarios/mixed.js
}
```

并更新 `case` 分支：

```bash
case "${SUBCMD}" in
  smoke) run_smoke ;;
  load)  run_load ;;
  stress) echo "ERROR: stress not implemented yet (M4)"; exit 2 ;;
  *) echo "usage: $0 <smoke|load|stress>"; exit 2 ;;
esac
```

- [ ] **Step 2: 运行完整 load pattern**

前置：app 在跑（release）、seed 已跑。

```bash
cd server-rs/stress
bash scripts/stress-run.sh load
```

Expected：
- k6 运行 ~5 分钟
- 控制台输出每个 scenario 的 p95/p99 + thresholds 状态
- 所有 thresholds `✓`（绿）
- 退出码 0
- `results/<ts>/` 下有 `meta.json`, `mixed.json`, `summary.json`

**若 threshold 红**：先看哪个场景超了。read-list/detail p95 200/100 ms 比较激进，若跑不过按以下顺序处理：
1. 确认 app 是 release build（`file target/release/app` → Mach-O 64-bit executable）
2. 确认 RUST_LOG=warn（关 info 日志）
3. 若仍超，可降 TARGET_RPS 到 200 重跑，记录为 "初始可达 RPS"
4. 阈值本身按实测 p95 × 1.3 调整（这是 spec §14 的预期）

- [ ] **Step 3: 清理运行中产生的 stress-% 数据**

```bash
bash scripts/stress-clean.sh
# 注意：seed 数据也会被清。若要保留 seed，先 grep 过滤
```

- [ ] **Step 4: Commit**

```bash
cd ..
git add server-rs/stress/scripts/stress-run.sh
git commit -m "feat(stress): add load subcommand to run orchestrator"
```

**M2 完成门槛**：`bash scripts/stress-run.sh load` 跑完 5 分钟，所有 SLO ✓ 或已按实测 × 1.3 在 thresholds.js 回填，退出码 0。

---

## M3: 服务端采样 + 报告

### Task 17: `scripts/stress-collect.sh`

**Files:**
- Create: `server-rs/stress/scripts/stress-collect.sh`

**设计**：`stress-collect.sh` 启动后台子进程采样，通过 PID 文件支持 `start` / `stop`。

- [ ] **Step 1: 写 stress-collect.sh**

```bash
#!/usr/bin/env bash
# server-rs/stress/scripts/stress-collect.sh
# 用法：
#   stress-collect.sh start <out_dir>
#   stress-collect.sh stop  <out_dir>

set -euo pipefail
cd "$(dirname "$0")/.."

CMD="${1:-}"
OUT_DIR="${2:-}"
[[ -z "${CMD}" || -z "${OUT_DIR}" ]] && { echo "usage: $0 <start|stop> <out_dir>"; exit 2; }
mkdir -p "${OUT_DIR}"
PID_FILE="${OUT_DIR}/.collectors.pids"

export PGPASSWORD=123456
PG_ARGS=(-h 127.0.0.1 -U saas_tea -d saas_tea -At)

start_pidstat() {
  local app_pid
  app_pid="$(pgrep -f 'target/release/app' | head -1 || true)"
  if [[ -z "${app_pid}" ]]; then echo "WARN: app pid not found; skipping pidstat"; return; fi
  # macOS 没有 pidstat；用 top -l 循环替代
  (
    while true; do
      ts="$(date +%s)"
      top -l 1 -pid "${app_pid}" -stats "pid,cpu,mem,th" \
        | awk -v ts="${ts}" 'NR>=13 && NF>=4 {print ts","$1","$2","$3","$4}'
      sleep 5
    done
  ) > "${OUT_DIR}/server-pidstat.csv" 2>/dev/null &
  echo $! >> "${PID_FILE}"
}

start_vmstat() {
  # macOS vm_stat 取代 vmstat
  (
    while true; do
      ts="$(date +%s)"
      echo -n "${ts},"
      vm_stat | awk '/Pages free/||/Pages active/||/Pages wired/{print $NF}' | tr -d '.' | paste -sd',' -
      sleep 5
    done
  ) > "${OUT_DIR}/server-vmstat.csv" 2>/dev/null &
  echo $! >> "${PID_FILE}"
}

start_pg() {
  (
    while true; do
      ts="$(date +%s)"
      active="$(psql "${PG_ARGS[@]}" -c "SELECT count(*) FROM pg_stat_activity WHERE state='active';" 2>/dev/null || echo -1)"
      idle="$(psql "${PG_ARGS[@]}" -c "SELECT count(*) FROM pg_stat_activity WHERE state='idle';" 2>/dev/null || echo -1)"
      echo "${ts},${active},${idle}"
      sleep 5
    done
  ) > "${OUT_DIR}/pg-stats.csv" 2>/dev/null &
  echo $! >> "${PID_FILE}"
}

start_redis() {
  command -v redis-cli >/dev/null || { echo "WARN: redis-cli not found; skip"; return; }
  (
    while true; do
      ts="$(date +%s)"
      ops="$(redis-cli info stats 2>/dev/null | grep instantaneous_ops_per_sec | cut -d: -f2 | tr -d '\r' || echo -1)"
      mem="$(redis-cli info memory 2>/dev/null | grep '^used_memory:' | cut -d: -f2 | tr -d '\r' || echo -1)"
      echo "${ts},${ops},${mem}"
      sleep 5
    done
  ) > "${OUT_DIR}/redis-stats.csv" 2>/dev/null &
  echo $! >> "${PID_FILE}"
}

case "${CMD}" in
  start)
    : > "${PID_FILE}"
    echo "ts,pid,cpu,mem,threads"     > "${OUT_DIR}/server-pidstat.csv"
    echo "ts,free,active,wired"       > "${OUT_DIR}/server-vmstat.csv"
    echo "ts,pg_active,pg_idle"       > "${OUT_DIR}/pg-stats.csv"
    echo "ts,redis_ops,redis_mem"     > "${OUT_DIR}/redis-stats.csv"
    start_pidstat; start_vmstat; start_pg; start_redis
    echo "collectors started, pids:"; cat "${PID_FILE}"
    ;;
  stop)
    if [[ ! -f "${PID_FILE}" ]]; then echo "no pid file"; exit 0; fi
    while read -r p; do kill "${p}" 2>/dev/null || true; done < "${PID_FILE}"
    rm -f "${PID_FILE}"
    echo "collectors stopped."
    ;;
  *) echo "usage: $0 <start|stop> <out_dir>"; exit 2 ;;
esac
```

- [ ] **Step 2: 加可执行权限 + 手工验证**

```bash
chmod +x server-rs/stress/scripts/stress-collect.sh
cd server-rs/stress
mkdir -p /tmp/collect-test
bash scripts/stress-collect.sh start /tmp/collect-test
sleep 15
bash scripts/stress-collect.sh stop /tmp/collect-test
ls -la /tmp/collect-test/
head -5 /tmp/collect-test/*.csv
```

Expected：4 个 CSV，每个至少 2 行数据（header + 2 个采样点）。

- [ ] **Step 3: Commit**

```bash
cd ..
git add server-rs/stress/scripts/stress-collect.sh
git commit -m "feat(stress): add server-side metrics collector"
```

---

### Task 18: 把 collector 接入 `stress-run.sh`

**Files:**
- Modify: `server-rs/stress/scripts/stress-run.sh`

- [ ] **Step 1: 修改 run_smoke 和 run_load，在 k6 前后加采样启停**

在每个 `run_*` 函数最前加：

```bash
bash scripts/stress-collect.sh start "${OUT_DIR}"
trap 'bash scripts/stress-collect.sh stop "${OUT_DIR}"' EXIT
```

- [ ] **Step 2: 重跑 smoke 验证**

```bash
cd server-rs/stress
bash scripts/stress-run.sh smoke
ls -la results/*/
```

Expected：results 下每次运行都有 4 个 CSV + k6 输出。

- [ ] **Step 3: Commit**

```bash
cd ..
git add server-rs/stress/scripts/stress-run.sh
git commit -m "feat(stress): wire collector into run orchestrator"
```

---

### Task 19: `scripts/stress-report.sh`

**Files:**
- Create: `server-rs/stress/scripts/stress-report.sh`

- [ ] **Step 1: 写 stress-report.sh**

```bash
#!/usr/bin/env bash
# server-rs/stress/scripts/stress-report.sh
# 用法：bash scripts/stress-report.sh <out_dir>
# 读 k6 summary.json + CSV，生成 report.md

set -euo pipefail
cd "$(dirname "$0")/.."

OUT_DIR="${1:-}"
[[ -z "${OUT_DIR}" ]] && { echo "usage: $0 <out_dir>"; exit 2; }
REPORT="${OUT_DIR}/report.md"

python3 - "${OUT_DIR}" > "${REPORT}" <<'PY'
import json, sys, csv, os, statistics
from pathlib import Path

out = Path(sys.argv[1])
meta = json.loads((out / "meta.json").read_text())

print(f"# Stress Report — {meta['timestamp']}")
print()
print(f"- subcommand: **{meta['subcommand']}**")
print(f"- base_url: `{meta['base_url']}`")
print(f"- git_sha: `{meta['git_sha']}`")
print(f"- k6: `{meta['k6_version']}`")
print()

# k6 summary (load/stress only)
summary_file = out / "summary.json"
if summary_file.exists():
    summary = json.loads(summary_file.read_text())
    print("## k6 Metrics (per scenario)")
    print()
    print("| scenario | p50 | p95 | p99 | fail % |")
    print("|---|---|---|---|---|")
    metrics = summary.get("metrics", {})
    # k6 把 per-scenario 指标塞在 metrics 里带 {scenario:X} 标签，
    # summary-export 按展平 key 给出
    scenarios = ["baseline","auth","read-list","read-detail","complex-read","write","file","cascade"]
    for s in scenarios:
        dur_key = f"http_req_duration{{scenario:{s}}}"
        fail_key = f"http_req_failed{{scenario:{s}}}"
        d = metrics.get(dur_key, {}).get("values", {})
        f = metrics.get(fail_key, {}).get("values", {})
        if not d:
            continue
        print(f"| {s} | {d.get('med',0):.0f} | {d.get('p(95)',0):.0f} | {d.get('p(99)',0):.0f} | {f.get('rate',0)*100:.2f} |")
    print()

# pidstat summary
pid_csv = out / "server-pidstat.csv"
if pid_csv.exists():
    cpu_vals = []
    mem_vals = []
    with pid_csv.open() as f:
        next(f, None)
        for line in f:
            parts = line.strip().split(",")
            try:
                cpu_vals.append(float(parts[2]))
                mem_vals.append(float(parts[3]))
            except (IndexError, ValueError):
                continue
    if cpu_vals:
        print("## Server Resource")
        print()
        print(f"- CPU %: peak {max(cpu_vals):.1f}, avg {statistics.mean(cpu_vals):.1f}")
        print(f"- MEM %: peak {max(mem_vals):.1f}, avg {statistics.mean(mem_vals):.1f}")
        print()

# PG connection peak
pg_csv = out / "pg-stats.csv"
if pg_csv.exists():
    active_max = 0
    with pg_csv.open() as f:
        next(f, None)
        for line in f:
            parts = line.strip().split(",")
            try:
                active_max = max(active_max, int(parts[1]))
            except (IndexError, ValueError):
                continue
    print(f"## PostgreSQL\n\n- active connections peak: **{active_max}**\n")

print("## Raw files\n")
for p in sorted(out.iterdir()):
    print(f"- `{p.name}`")
PY

echo "Report generated: ${REPORT}"
```

- [ ] **Step 2: 加可执行权限 + 对已有 results 目录跑一次**

```bash
chmod +x server-rs/stress/scripts/stress-report.sh
cd server-rs/stress
LATEST="$(ls -td results/*/ | head -1)"
bash scripts/stress-report.sh "${LATEST}"
cat "${LATEST}/report.md"
```

Expected：生成 markdown 报告，至少包含：k6 metrics 表、Server Resource 段、PG connection 峰值、Raw files 列表。

- [ ] **Step 3: 把 report 生成接入 stress-run.sh**

在 `stress-run.sh` 末尾（各 `run_*` case 后、最终 `echo done` 前）加：

```bash
echo ""
echo "=== generating report ==="
bash scripts/stress-report.sh "${OUT_DIR}"
```

- [ ] **Step 4: 重跑 load 验证完整闭环**

```bash
cd server-rs/stress
bash scripts/stress-seed.sh  # 若未 seed
bash scripts/stress-run.sh load
cat "$(ls -td results/*/ | head -1)/report.md"
```

Expected：5 分钟后自动生成 report.md，含所有段落。

- [ ] **Step 5: Commit**

```bash
cd ..
git add server-rs/stress/scripts/stress-report.sh server-rs/stress/scripts/stress-run.sh
git commit -m "feat(stress): generate markdown report from metrics"
```

**M3 完成门槛**：`stress-run.sh load` 一次跑完自动产出 `report.md`，含 k6 per-scenario 表、Server CPU/MEM 峰值、PG 连接峰值。

---

### Task 20: 更新 README 增加 `load` + `report` 使用示例

**Files:**
- Modify: `server-rs/stress/README.md`

- [ ] **Step 1: 补充排错段**

在 README 末尾加：

````markdown
## 排错

### SLO 红了怎么办
1. 确认 app 是 release build：`file server-rs/target/release/app`
2. 确认 `RUST_LOG=warn`（关 info 日志），日志 IO 会拖慢 1 倍以上
3. 确认 seed 数据已就位：`bash scripts/stress-seed.sh`
4. 查 `results/<ts>/report.md` 的 PG `active connections peak` — 若 ≥ `database.pool.max_connections` 说明连接池打满
5. 若确认不是环境问题，按实测 p95 × 1.3 回填 `k6/thresholds.js`

### `empty id pool for X` 错误
seed 未跑或目标表空：先 `bash scripts/stress-seed.sh`。

### 端点 404
`grep -rn '#\[utoipa::path.*"/xxx/list"' server-rs/crates/modules/src/`
对齐真实路径后修对应 scenario。
````

- [ ] **Step 2: Commit**

```bash
cd ..
git add server-rs/stress/README.md
git commit -m "docs(stress): add troubleshooting guide"
```

---

## M4: Stress Pattern + 容量定位

### Task 21: 在 `stress-run.sh` 加 `stress` 子命令

**Files:**
- Modify: `server-rs/stress/scripts/stress-run.sh`

- [ ] **Step 1: 加 run_stress 函数**

```bash
run_stress() {
  bash scripts/stress-collect.sh start "${OUT_DIR}"
  trap 'bash scripts/stress-collect.sh stop "${OUT_DIR}"' EXIT

  echo "=== stress: ramping mixed 50→2000 RPS, 10 min ==="
  # k6 的 stress pattern 用 K6_RAMP=1 触发 mixed.js 里的阶梯 executor 配置
  K6_RAMP=1 \
    k6 run \
      --out "json=${OUT_DIR}/mixed-ramp.json" \
      --summary-export "${OUT_DIR}/summary.json" \
      k6/scenarios/mixed.js
}
```

并更新 case：

```bash
stress) run_stress ;;
```

- [ ] **Step 2: 修改 `mixed.js` 支持 K6_RAMP 切换到阶梯 executor**

替换 `scenarioFor` 函数：

```javascript
function scenarioFor(name, exec) {
  const weight = WEIGHTS[name];
  if (__ENV.K6_RAMP) {
    // stress pattern: ramping-arrival-rate 50→2000 RPS
    const stages = [
      { target:  50, duration: '90s' },
      { target: 100, duration: '90s' },
      { target: 250, duration: '90s' },
      { target: 500, duration: '90s' },
      { target:1000, duration: '90s' },
      { target:2000, duration: '90s' },
    ].map((s) => ({ target: Math.max(1, Math.round(s.target * weight)), duration: s.duration }));
    return {
      executor: 'ramping-arrival-rate',
      startRate: 1,
      timeUnit: '1s',
      stages,
      preAllocatedVUs: 50,
      maxVUs: 2000,
      exec,
      tags: { scenario: name },
    };
  }
  const rate = Math.max(1, Math.round(TARGET_RPS * weight));
  return {
    executor: 'constant-arrival-rate',
    rate,
    timeUnit: '1s',
    duration: '5m',
    preAllocatedVUs: Math.max(10, rate),
    maxVUs: Math.max(50, rate * 4),
    exec,
    tags: { scenario: name },
  };
}
```

- [ ] **Step 3: 语法自检 + 运行**

```bash
cd server-rs/stress
node --check k6/scenarios/mixed.js
bash scripts/stress-run.sh stress
```

Expected：
- 运行 ~10 分钟
- k6 控制台显示 6 个 target 阶段
- 生成 report.md
- 某个阶段开始出现 threshold 红，表示找到容量上限

- [ ] **Step 4: Commit**

```bash
cd ..
git add server-rs/stress/scripts/stress-run.sh server-rs/stress/k6/scenarios/mixed.js
git commit -m "feat(stress): add stress subcommand with ramping RPS"
```

---

### Task 22: `scripts/stress-compare.sh` 跨运行对比

**Files:**
- Create: `server-rs/stress/scripts/stress-compare.sh`

- [ ] **Step 1: 写 stress-compare.sh**

```bash
#!/usr/bin/env bash
# server-rs/stress/scripts/stress-compare.sh
# 用法：bash scripts/stress-compare.sh <baseline_dir> <current_dir>
# 对比两次运行的 per-scenario p95 / fail rate

set -euo pipefail
cd "$(dirname "$0")/.."

BASE="${1:-}"; CUR="${2:-}"
[[ -z "${BASE}" || -z "${CUR}" ]] && { echo "usage: $0 <baseline_dir> <current_dir>"; exit 2; }

python3 - "${BASE}" "${CUR}" <<'PY'
import json, sys
from pathlib import Path

base = json.loads((Path(sys.argv[1]) / "summary.json").read_text())["metrics"]
cur  = json.loads((Path(sys.argv[2]) / "summary.json").read_text())["metrics"]

scenarios = ["baseline","auth","read-list","read-detail","complex-read","write","file","cascade"]
print(f"| scenario | base p95 | cur p95 | diff |")
print(f"|---|---|---|---|")
for s in scenarios:
    k = f"http_req_duration{{scenario:{s}}}"
    b = base.get(k,{}).get("values",{}).get("p(95)")
    c = cur.get(k,{}).get("values",{}).get("p(95)")
    if b is None or c is None: continue
    diff_pct = (c - b) / b * 100 if b else 0
    arrow = "🔺" if diff_pct > 5 else ("🔻" if diff_pct < -5 else "→")
    print(f"| {s} | {b:.0f} ms | {c:.0f} ms | {arrow} {diff_pct:+.1f}% |")
PY
```

- [ ] **Step 2: 对两次已有 results 运行测一下**

```bash
chmod +x server-rs/stress/scripts/stress-compare.sh
cd server-rs/stress
DIRS=($(ls -td results/*/ | head -2))
bash scripts/stress-compare.sh "${DIRS[1]}" "${DIRS[0]}"
```

Expected：输出对比表格。

- [ ] **Step 3: Commit**

```bash
cd ..
git add server-rs/stress/scripts/stress-compare.sh
git commit -m "feat(stress): add cross-run p95 comparison script"
```

---

### Task 23: 最终 README 补全 + 收尾

**Files:**
- Modify: `server-rs/stress/README.md`

- [ ] **Step 1: 加 stress pattern + compare 使用说明**

在 README 的 "快速开始" 段后加：

````markdown
## 容量测试 (stress pattern)

```bash
# 10 分钟阶梯压测（50 → 2000 RPS）
bash scripts/stress-run.sh stress
# 报告里看哪个阶段 threshold 开始变红 = 单实例容量上限
cat "$(ls -td results/*/ | head -1)/report.md"
```

## 跨运行对比（基线回归）

```bash
# 对比最近两次运行
DIRS=($(ls -td results/*/ | head -2))
bash scripts/stress-compare.sh "${DIRS[1]}" "${DIRS[0]}"
```
````

- [ ] **Step 2: 跑一遍完整流程验证**

```bash
cd server-rs/stress
bash scripts/stress-clean.sh
bash scripts/stress-seed.sh
bash scripts/stress-run.sh smoke
bash scripts/stress-run.sh load
bash scripts/stress-run.sh stress
DIRS=($(ls -td results/*/ | head -2))
bash scripts/stress-compare.sh "${DIRS[1]}" "${DIRS[0]}"
```

所有步骤退出码应为 0。

- [ ] **Step 3: 最终 commit**

```bash
cd ..
git add server-rs/stress/README.md
git commit -m "docs(stress): complete README with stress and compare usage"
```

**M4 完成门槛**：`stress-run.sh stress` 能跑出容量曲线，`stress-compare.sh` 能对比两次运行输出 p95 diff。

---

## Self-Review 核查

**Spec coverage**（对照 spec 章节 → 任务）：

| Spec 章节 | 对应任务 |
|---|---|
| §2 工具选型 (k6) | Task 2-7（全部 k6 脚本） |
| §3 场景分组（8 组） | Task 5, 9, 10, 11, 12, 14（auth/file/cascade） |
| §4 Pattern（Smoke/Load/Stress） | Task 7 (smoke), Task 16 (load), Task 21 (stress) |
| §4 Pattern Soak | **未实现**（spec §14 确认 v1 不做） |
| §5 SLO 阈值 | Task 4 (thresholds.js) |
| §6 环境 | Task 1 (README 前置依赖) |
| §7 Seed 数据 | Task 13 |
| §8 指标采集 | Task 17 |
| §9 报告产物 | Task 19 |
| §10 目录结构 | Task 1 |
| §11 里程碑 M1-M4 | M1=T1-7, M2=T8-16, M3=T17-20, M4=T21-23 |

**Placeholder scan**：已清查无 TBD/TODO/"如上述"引用；每个代码步骤都有完整代码；每个命令都有 Expected 输出。

**Type consistency**：`thresholds.js` 的 `buildThresholds(scenarios)` 在所有 scenario 文件中一致使用；`config.js` 的 `headersJson(token)` 签名一致；`data.js` 的 `loadIdPool/sampleId/userCreatePayload/roleCreatePayload/noticeCreatePayload` 签名在 Task 8 定义后一致使用。

**Ambiguity check**：
- Task 5 `/health` 路径在验证步骤有 `curl` 自检 + 修正路径；
- Task 13 seed 字段名可能与 schema 不符，已在步骤里提示先 `\d sys_user`；
- Task 14 upload 端点路径在验证步骤让用户先 grep 对齐；
- Task 16 SLO 红了的处置顺序已给出 4 步。

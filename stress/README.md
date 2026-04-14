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

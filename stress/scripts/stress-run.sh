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
HEALTH_URL="${BASE_URL%/api/v1}/health/live"
curl -sSf "${HEALTH_URL}" > /dev/null \
  || { echo "ERROR: server not reachable at ${HEALTH_URL}. Start: RUST_LOG=warn ./target/release/app"; exit 1; }

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
  k6 run --summary-export "${OUT_DIR}/baseline-summary.json" k6/scenarios/baseline.js
  echo "=== smoke: read-list (1 min) ==="
  k6 run --summary-export "${OUT_DIR}/read-list-summary.json" k6/scenarios/read-list.js
}

run_load() {
  local user_cnt
  user_cnt=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -At \
    -c "SELECT count(*) FROM sys_user WHERE user_name LIKE 'stress-%';")
  if [[ "${user_cnt}" -lt 5000 ]]; then
    echo "ERROR: seed data insufficient (${user_cnt} users). Run: bash scripts/stress-seed.sh"
    exit 1
  fi
  echo "seed ok: ${user_cnt} stress users"

  echo "=== load: mixed 5 min @ ${TARGET_RPS:-500} RPS ==="
  TARGET_RPS="${TARGET_RPS:-500}" \
    k6 run \
      --summary-export "${OUT_DIR}/summary.json" \
      k6/scenarios/mixed.js
}

run_stress() {
  local user_cnt
  user_cnt=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -At \
    -c "SELECT count(*) FROM sys_user WHERE user_name LIKE 'stress-%';")
  if [[ "${user_cnt}" -lt 5000 ]]; then
    echo "ERROR: seed data insufficient (${user_cnt} users). Run: bash scripts/stress-seed.sh"
    exit 1
  fi
  echo "seed ok: ${user_cnt} stress users"

  echo "=== stress: ramping mixed 50→2000 RPS, 9 min ==="
  K6_RAMP=1 \
    k6 run \
      --summary-export "${OUT_DIR}/summary.json" \
      k6/scenarios/mixed.js
}

case "${SUBCMD}" in
  smoke)  run_smoke ;;
  load)   run_load ;;
  stress) run_stress ;;
  *) echo "usage: $0 <smoke|load|stress>"; exit 2 ;;
esac

echo ""
echo "=== done ==="
echo "results: ${OUT_DIR}"

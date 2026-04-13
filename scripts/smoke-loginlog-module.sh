#!/usr/bin/env bash
# scripts/smoke-loginlog-module.sh
#
# End-to-end verification of 5 login-log module endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script inserts a `smk-ll-` prefixed login log via psql,
# exercises list/delete/verify, then cleans up via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-ll-${TS_SHORT}"
TOKEN=""
LOG_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_logininfor WHERE user_name LIKE '${PREFIX}%';" 2>/dev/null || true
  CLEANUP_DONE=true
}

assert_eq() {
  local expected="$1" actual="$2" msg="$3"
  if [[ "$expected" != "$actual" ]]; then
    echo "FAIL: $msg (expected '$expected', got '$actual')"
    exit 1
  fi
  echo "  OK: $msg"
}

step() {
  echo ""
  echo "=== $1 ==="
}

# ---------------------------------------------------------------------------
# Step 1: Login
# ---------------------------------------------------------------------------
step "1. login"
TOKEN=$(curl -sS -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
assert_eq true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -H "tenant-id: 000000")

# ---------------------------------------------------------------------------
# Step 2: Insert test login log row via psql
# ---------------------------------------------------------------------------
step "2. insert test login log via psql"
TEST_USER="${PREFIX}-user"
PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
  "INSERT INTO sys_logininfor (info_id, tenant_id, user_name, ipaddr, login_location, browser, os, device_type, status, msg, del_flag) VALUES (gen_random_uuid(), '000000', '${TEST_USER}', '127.0.0.1', 'local', 'Chrome', 'macOS', '0', '0', 'test', '0');"
echo "  OK: inserted test login log for user=${TEST_USER}"

# ---------------------------------------------------------------------------
# Step 3: List login logs — should have 1 entry for our test user
# ---------------------------------------------------------------------------
step "3. GET /monitor/logininfor/list"
LIST_RESP=$(curl -sS "$BASE/monitor/logininfor/list?pageNum=1&pageSize=10&userName=${TEST_USER}" "${H[@]}")
echo "$LIST_RESP" | python3 -m json.tool
LIST_CODE=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_CODE" "list login logs returns code=200"

TOTAL=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['total'])")
assert_eq true "$([ "$TOTAL" -ge 1 ] && echo true || echo false)" "login log list has entries for test user (total=$TOTAL)"

# Extract the log ID
LOG_ID=$(echo "$LIST_RESP" | python3 -c "
import sys,json
rows = json.load(sys.stdin)['data']['rows']
print(rows[0].get('infoId') or rows[0].get('id'))
")
assert_eq true "$([ -n "$LOG_ID" ] && echo true || echo false)" "extracted login log ID: $LOG_ID"

# ---------------------------------------------------------------------------
# Step 4: Delete login log (soft delete)
# ---------------------------------------------------------------------------
step "4. DELETE /monitor/logininfor/${LOG_ID}"
DEL_RESP=$(curl -sS -X DELETE "$BASE/monitor/logininfor/$LOG_ID" "${H[@]}")
DEL_CODE=$(echo "$DEL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_CODE" "delete login log returns code=200"

# ---------------------------------------------------------------------------
# Step 5: Verify soft-deleted — list should be empty for test user
# ---------------------------------------------------------------------------
step "5. verify deleted (soft-deleted not in list)"
VERIFY_RESP=$(curl -sS "$BASE/monitor/logininfor/list?pageNum=1&pageSize=10&userName=${TEST_USER}" "${H[@]}")
VERIFY_TOTAL=$(echo "$VERIFY_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['total'])")
assert_eq 0 "$VERIFY_TOTAL" "login log list is empty after soft delete"

echo ""
echo "ALL 5 STEPS PASSED"

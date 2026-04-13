#!/usr/bin/env bash
# scripts/smoke-config-module.sh
#
# End-to-end verification of 9 config module endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates `smk-cfg-` prefixed configs, exercises every endpoint,
# then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-cfg-${TS_SHORT}"
TOKEN=""
CONFIG_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_config WHERE config_key LIKE '${PREFIX}%';" 2>/dev/null || true
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
  -H "tenant-id: 000000" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
assert_eq true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -H "tenant-id: 000000")

# ---------------------------------------------------------------------------
# Step 2: Create config
# ---------------------------------------------------------------------------
step "2. POST /system/config/ — create config"
CONFIG_NAME="${PREFIX}-name"
CONFIG_KEY="${PREFIX}-key"
CREATED=$(curl -sS -X POST "$BASE/system/config/" "${H[@]}" \
  -d "{\"configName\":\"${CONFIG_NAME}\",\"configKey\":\"${CONFIG_KEY}\",\"configValue\":\"test-val\",\"configType\":\"N\",\"status\":\"0\"}")
echo "$CREATED" | python3 -m json.tool
RESP_CODE=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
CONFIG_ID=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['configId'])")
assert_eq 200 "$RESP_CODE" "create config returns code=200"

# ---------------------------------------------------------------------------
# Step 3: List configs
# ---------------------------------------------------------------------------
step "3. GET /system/config/list?pageNum=1&pageSize=10"
LIST_RESP=$(curl -sS "$BASE/system/config/list?pageNum=1&pageSize=10" "${H[@]}")
LIST_CODE=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_CODE" "list configs returns code=200"

# ---------------------------------------------------------------------------
# Step 4: Get detail — assert configKey matches
# ---------------------------------------------------------------------------
step "4. GET /system/config/${CONFIG_ID}"
DETAIL_RESP=$(curl -sS "$BASE/system/config/$CONFIG_ID" "${H[@]}")
echo "$DETAIL_RESP" | python3 -m json.tool
DETAIL_KEY=$(echo "$DETAIL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['configKey'])")
assert_eq "$CONFIG_KEY" "$DETAIL_KEY" "detail configKey matches"

# ---------------------------------------------------------------------------
# Step 5: Find by key
# ---------------------------------------------------------------------------
step "5. GET /system/config/key/${CONFIG_KEY}"
KEY_RESP=$(curl -sS "$BASE/system/config/key/${CONFIG_KEY}" "${H[@]}")
KEY_CODE=$(echo "$KEY_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$KEY_CODE" "find by key returns code=200"

# ---------------------------------------------------------------------------
# Step 6: Update configValue to "updated-val"
# ---------------------------------------------------------------------------
step "6. PUT /system/config/ — update configValue"
UPD_RESP=$(curl -sS -X PUT "$BASE/system/config/" "${H[@]}" \
  -d "{\"configId\":\"${CONFIG_ID}\",\"configName\":\"${CONFIG_NAME}\",\"configKey\":\"${CONFIG_KEY}\",\"configValue\":\"updated-val\",\"configType\":\"N\",\"status\":\"0\"}")
UPD_CODE=$(echo "$UPD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UPD_CODE" "update config returns code=200"

# ---------------------------------------------------------------------------
# Step 7: Update by key
# ---------------------------------------------------------------------------
step "7. PUT /system/config/key — update by key"
UPD_KEY_RESP=$(curl -sS -X PUT "$BASE/system/config/key" "${H[@]}" \
  -d "{\"configKey\":\"${CONFIG_KEY}\",\"configValue\":\"key-updated\"}")
UPD_KEY_CODE=$(echo "$UPD_KEY_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UPD_KEY_CODE" "update by key returns code=200"

# ---------------------------------------------------------------------------
# Step 8: Delete config
# ---------------------------------------------------------------------------
step "8. DELETE /system/config/${CONFIG_ID}"
DEL_RESP=$(curl -sS -X DELETE "$BASE/system/config/$CONFIG_ID" "${H[@]}")
DEL_CODE=$(echo "$DEL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_CODE" "delete config returns code=200"

# ---------------------------------------------------------------------------
# Step 9: Verify deleted
# ---------------------------------------------------------------------------
step "9. Verify deleted (GET returns code != 200)"
VERIFY_RESP=$(curl -sS "$BASE/system/config/$CONFIG_ID" "${H[@]}")
VERIFY_CODE=$(echo "$VERIFY_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
if [[ "$VERIFY_CODE" == "200" ]]; then
  echo "FAIL: config still exists after delete"
  exit 1
fi
echo "  OK: config no longer exists (code=$VERIFY_CODE)"

echo ""
echo "ALL 9 STEPS PASSED"

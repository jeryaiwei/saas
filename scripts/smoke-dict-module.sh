#!/usr/bin/env bash
# scripts/smoke-dict-module.sh
#
# End-to-end verification of 11 dict module endpoints (type + data).
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates `smk-dt-` prefixed dict types and data, exercises every
# endpoint, then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-dt-${TS_SHORT}"
TOKEN=""
TYPE_ID=""
DATA_CODE=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_dict_data WHERE dict_type LIKE '${PREFIX}%'; DELETE FROM sys_dict_type WHERE dict_type LIKE '${PREFIX}%';" 2>/dev/null || true
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
# Step 2: Create dict type
# ---------------------------------------------------------------------------
step "2. POST /system/dict/type — create dict type"
DICT_NAME="${PREFIX}-name"
DICT_TYPE="${PREFIX}-type"
CREATED_TYPE=$(curl -sS -X POST "$BASE/system/dict/type" "${H[@]}" \
  -d "{\"dictName\":\"${DICT_NAME}\",\"dictType\":\"${DICT_TYPE}\",\"status\":\"0\"}")
echo "$CREATED_TYPE" | python3 -m json.tool
TYPE_CODE=$(echo "$CREATED_TYPE" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
TYPE_ID=$(echo "$CREATED_TYPE" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['dictId'])")
assert_eq 200 "$TYPE_CODE" "create dict type returns code=200"

# ---------------------------------------------------------------------------
# Step 3: List dict types
# ---------------------------------------------------------------------------
step "3. GET /system/dict/type/list?pageNum=1&pageSize=10"
LIST_RESP=$(curl -sS "$BASE/system/dict/type/list?pageNum=1&pageSize=10" "${H[@]}")
LIST_CODE=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_CODE" "list dict types returns code=200"

# ---------------------------------------------------------------------------
# Step 4: Get dict type detail
# ---------------------------------------------------------------------------
step "4. GET /system/dict/type/${TYPE_ID}"
DETAIL_RESP=$(curl -sS "$BASE/system/dict/type/$TYPE_ID" "${H[@]}")
echo "$DETAIL_RESP" | python3 -m json.tool
DETAIL_CODE=$(echo "$DETAIL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DETAIL_CODE" "get dict type detail returns code=200"

# ---------------------------------------------------------------------------
# Step 5: Dict type option select
# ---------------------------------------------------------------------------
step "5. GET /system/dict/type/option-select"
OPT_RESP=$(curl -sS "$BASE/system/dict/type/option-select" "${H[@]}")
OPT_CODE=$(echo "$OPT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$OPT_CODE" "option-select returns code=200"

# ---------------------------------------------------------------------------
# Step 6: Create dict data
# ---------------------------------------------------------------------------
step "6. POST /system/dict/data — create dict data"
CREATED_DATA=$(curl -sS -X POST "$BASE/system/dict/data" "${H[@]}" \
  -d "{\"dictType\":\"${DICT_TYPE}\",\"dictLabel\":\"label-1\",\"dictValue\":\"val-1\",\"status\":\"0\"}")
echo "$CREATED_DATA" | python3 -m json.tool
DATA_RESP_CODE=$(echo "$CREATED_DATA" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
DATA_CODE=$(echo "$CREATED_DATA" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['dictCode'])")
assert_eq 200 "$DATA_RESP_CODE" "create dict data returns code=200"

# ---------------------------------------------------------------------------
# Step 7: List dict data by type
# ---------------------------------------------------------------------------
step "7. GET /system/dict/data/list?pageNum=1&pageSize=10&dictType=${DICT_TYPE}"
DATA_LIST_RESP=$(curl -sS "$BASE/system/dict/data/list?pageNum=1&pageSize=10&dictType=${DICT_TYPE}" "${H[@]}")
DATA_LIST_CODE=$(echo "$DATA_LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DATA_LIST_CODE" "list dict data returns code=200"

# ---------------------------------------------------------------------------
# Step 8: Get data by type name
# ---------------------------------------------------------------------------
step "8. GET /system/dict/data/type/${DICT_TYPE}"
BY_TYPE_RESP=$(curl -sS "$BASE/system/dict/data/type/${DICT_TYPE}" "${H[@]}")
BY_TYPE_CODE=$(echo "$BY_TYPE_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$BY_TYPE_CODE" "get data by type name returns code=200"

# ---------------------------------------------------------------------------
# Step 9: Get dict data detail
# ---------------------------------------------------------------------------
step "9. GET /system/dict/data/${DATA_CODE}"
DATA_DETAIL_RESP=$(curl -sS "$BASE/system/dict/data/$DATA_CODE" "${H[@]}")
echo "$DATA_DETAIL_RESP" | python3 -m json.tool
DATA_DETAIL_CODE=$(echo "$DATA_DETAIL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DATA_DETAIL_CODE" "get dict data detail returns code=200"

# ---------------------------------------------------------------------------
# Step 10: Delete dict data
# ---------------------------------------------------------------------------
step "10. DELETE /system/dict/data/${DATA_CODE}"
DEL_DATA_RESP=$(curl -sS -X DELETE "$BASE/system/dict/data/$DATA_CODE" "${H[@]}")
DEL_DATA_CODE=$(echo "$DEL_DATA_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_DATA_CODE" "delete dict data returns code=200"

# ---------------------------------------------------------------------------
# Step 11: Delete dict type
# ---------------------------------------------------------------------------
step "11. DELETE /system/dict/type/${TYPE_ID}"
DEL_TYPE_RESP=$(curl -sS -X DELETE "$BASE/system/dict/type/$TYPE_ID" "${H[@]}")
DEL_TYPE_CODE=$(echo "$DEL_TYPE_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_TYPE_CODE" "delete dict type returns code=200"

echo ""
echo "ALL 11 STEPS PASSED"

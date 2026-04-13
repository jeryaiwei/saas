#!/usr/bin/env bash
# scripts/smoke-operlog-module.sh
#
# End-to-end verification of 6 operlog module endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script triggers operlog by creating a post with `smk-ol-` prefix,
# exercises operlog list/detail/delete, then cleans up via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-ol-${TS_SHORT}"
TOKEN=""
POST_ID=""
OPERLOG_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_oper_log WHERE title LIKE 'smk-%' OR oper_url LIKE '%smk-%'; DELETE FROM sys_post WHERE post_code LIKE '${PREFIX}%';" 2>/dev/null || true
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
# Step 2: Create a post to trigger operlog
# ---------------------------------------------------------------------------
step "2. POST create post (trigger operlog)"
POST_CODE="${PREFIX}-trigger"
POST_NAME="${PREFIX}-trigger"
CREATED_POST=$(curl -sS -X POST "$BASE/system/post/" "${H[@]}" \
  -d "{\"postCode\":\"${POST_CODE}\",\"postName\":\"${POST_NAME}\",\"postSort\":1,\"status\":\"0\"}")
echo "$CREATED_POST" | python3 -m json.tool
POST_RESP_CODE=$(echo "$CREATED_POST" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
POST_ID=$(echo "$CREATED_POST" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['postId'])")
assert_eq 200 "$POST_RESP_CODE" "create post returns code=200"

# Delete the post right away (also generates another operlog entry)
DEL_POST=$(curl -sS -X DELETE "$BASE/system/post/$POST_ID" "${H[@]}")
DEL_POST_CODE=$(echo "$DEL_POST" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_POST_CODE" "delete post returns code=200"

# ---------------------------------------------------------------------------
# Step 3: List operlogs
# ---------------------------------------------------------------------------
step "3. GET /monitor/operlog/list"
LIST_RESP=$(curl -sS "$BASE/monitor/operlog/list?pageNum=1&pageSize=10" "${H[@]}")
LIST_CODE=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_CODE" "list operlogs returns code=200"

TOTAL=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['total'])")
assert_eq true "$([ "$TOTAL" -ge 1 ] && echo true || echo false)" "operlog list has entries (total=$TOTAL)"

# ---------------------------------------------------------------------------
# Step 4: Extract first operlog ID
# ---------------------------------------------------------------------------
step "4. extract first operlog ID"
OPERLOG_ID=$(echo "$LIST_RESP" | python3 -c "
import sys,json
rows = json.load(sys.stdin)['data']['rows']
print(rows[0].get('operId') or rows[0].get('id'))
")
assert_eq true "$([ -n "$OPERLOG_ID" ] && echo true || echo false)" "extracted operlog ID: $OPERLOG_ID"

# ---------------------------------------------------------------------------
# Step 5: Get operlog detail
# ---------------------------------------------------------------------------
step "5. GET /monitor/operlog/${OPERLOG_ID}"
DETAIL_RESP=$(curl -sS "$BASE/monitor/operlog/$OPERLOG_ID" "${H[@]}")
echo "$DETAIL_RESP" | python3 -m json.tool
DETAIL_CODE=$(echo "$DETAIL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DETAIL_CODE" "operlog detail returns code=200"

# ---------------------------------------------------------------------------
# Step 6: Delete the operlog entry
# ---------------------------------------------------------------------------
step "6. DELETE /monitor/operlog/${OPERLOG_ID}"
DEL_RESP=$(curl -sS -X DELETE "$BASE/monitor/operlog/$OPERLOG_ID" "${H[@]}")
DEL_CODE=$(echo "$DEL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_CODE" "delete operlog returns code=200"

echo ""
echo "ALL 6 STEPS PASSED"

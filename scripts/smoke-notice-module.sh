#!/usr/bin/env bash
# scripts/smoke-notice-module.sh
#
# End-to-end verification of 7 notice module endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates `smk-n-` prefixed notices, exercises every endpoint,
# then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-n-${TS_SHORT}"
TOKEN=""
NOTICE_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_notice WHERE notice_title LIKE '${PREFIX}%';" 2>/dev/null || true
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
# Step 2: Create notice
# ---------------------------------------------------------------------------
step "2. POST create notice"
NOTICE_TITLE="${PREFIX}-title"
CREATED=$(curl -sS -X POST "$BASE/message/notice/" "${H[@]}" \
  -d "{\"noticeTitle\":\"${NOTICE_TITLE}\",\"noticeType\":\"1\",\"noticeContent\":\"test content\",\"status\":\"0\"}")
echo "$CREATED" | python3 -m json.tool
RESP_CODE=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
NOTICE_ID=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['noticeId'])")
assert_eq 200 "$RESP_CODE" "create notice returns code=200"
assert_eq 36 "${#NOTICE_ID}" "noticeId is a uuid"

# ---------------------------------------------------------------------------
# Step 3: List notices
# ---------------------------------------------------------------------------
step "3. GET /message/notice/list"
LIST_RESP=$(curl -sS "$BASE/message/notice/list?pageNum=1&pageSize=10" "${H[@]}")
LIST_CODE=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_CODE" "list notices returns code=200"

# ---------------------------------------------------------------------------
# Step 4: Get detail — assert noticeTitle matches
# ---------------------------------------------------------------------------
step "4. GET /message/notice/${NOTICE_ID}"
DETAIL_RESP=$(curl -sS "$BASE/message/notice/$NOTICE_ID" "${H[@]}")
echo "$DETAIL_RESP" | python3 -m json.tool
DETAIL_TITLE=$(echo "$DETAIL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['noticeTitle'])")
assert_eq "$NOTICE_TITLE" "$DETAIL_TITLE" "detail noticeTitle matches"

# ---------------------------------------------------------------------------
# Step 5: Update noticeTitle
# ---------------------------------------------------------------------------
step "5. PUT update noticeTitle"
UPDATED_TITLE="${PREFIX}-updated"
UPD_RESP=$(curl -sS -X PUT "$BASE/message/notice/" "${H[@]}" \
  -d "{\"noticeId\":\"${NOTICE_ID}\",\"noticeTitle\":\"${UPDATED_TITLE}\"}")
UPD_CODE=$(echo "$UPD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UPD_CODE" "update notice returns code=200"

# Verify title changed
DETAIL_AFTER=$(curl -sS "$BASE/message/notice/$NOTICE_ID" "${H[@]}")
TITLE_VAL=$(echo "$DETAIL_AFTER" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['noticeTitle'])")
assert_eq "$UPDATED_TITLE" "$TITLE_VAL" "noticeTitle was updated"

# ---------------------------------------------------------------------------
# Step 6: Delete notice
# ---------------------------------------------------------------------------
step "6. DELETE /message/notice/${NOTICE_ID}"
DEL_RESP=$(curl -sS -X DELETE "$BASE/message/notice/$NOTICE_ID" "${H[@]}")
DEL_CODE=$(echo "$DEL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_CODE" "delete notice returns code=200"

# ---------------------------------------------------------------------------
# Step 7: Verify deleted
# ---------------------------------------------------------------------------
step "7. verify deleted"
VERIFY_RESP=$(curl -sS "$BASE/message/notice/$NOTICE_ID" "${H[@]}")
VERIFY_CODE=$(echo "$VERIFY_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq true "$([ "$VERIFY_CODE" != "200" ] && echo true || echo false)" "deleted notice is no longer accessible"

echo ""
echo "ALL 7 STEPS PASSED"

#!/usr/bin/env bash
# scripts/smoke-post-module.sh
#
# End-to-end verification of 8 post module endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates `smk-p-` prefixed posts, exercises every endpoint,
# then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-p-${TS_SHORT}"
TOKEN=""
POST_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_post WHERE post_code LIKE '${PREFIX}%';" 2>/dev/null || true
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
# Step 2: Create post
# ---------------------------------------------------------------------------
step "2. POST /system/post/ — create post"
POST_CODE="${PREFIX}-code"
POST_NAME="${PREFIX}-name"
CREATED=$(curl -sS -X POST "$BASE/system/post/" "${H[@]}" \
  -d "{\"postCode\":\"${POST_CODE}\",\"postName\":\"${POST_NAME}\",\"postSort\":1,\"status\":\"0\"}")
echo "$CREATED" | python3 -m json.tool
RESP_CODE=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
POST_ID=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['postId'])")
assert_eq 200 "$RESP_CODE" "create post returns code=200"

# ---------------------------------------------------------------------------
# Step 3: List posts
# ---------------------------------------------------------------------------
step "3. GET /system/post/list?pageNum=1&pageSize=10"
LIST_RESP=$(curl -sS "$BASE/system/post/list?pageNum=1&pageSize=10" "${H[@]}")
LIST_CODE=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_CODE" "list posts returns code=200"

# ---------------------------------------------------------------------------
# Step 4: Get detail — assert postCode matches
# ---------------------------------------------------------------------------
step "4. GET /system/post/${POST_ID}"
DETAIL_RESP=$(curl -sS "$BASE/system/post/$POST_ID" "${H[@]}")
echo "$DETAIL_RESP" | python3 -m json.tool
DETAIL_CODE=$(echo "$DETAIL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['postCode'])")
assert_eq "$POST_CODE" "$DETAIL_CODE" "detail postCode matches"

# ---------------------------------------------------------------------------
# Step 5: Update postSort to 99
# ---------------------------------------------------------------------------
step "5. PUT /system/post/ — update postSort"
UPD_RESP=$(curl -sS -X PUT "$BASE/system/post/" "${H[@]}" \
  -d "{\"postId\":\"${POST_ID}\",\"postCode\":\"${POST_CODE}\",\"postName\":\"${POST_NAME}\",\"postSort\":99,\"status\":\"0\"}")
UPD_CODE=$(echo "$UPD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UPD_CODE" "update post returns code=200"

# ---------------------------------------------------------------------------
# Step 6: Option select — verify test post in options
# ---------------------------------------------------------------------------
step "6. GET /system/post/option-select"
OPT_RESP=$(curl -sS "$BASE/system/post/option-select" "${H[@]}")
OPT_CODE=$(echo "$OPT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$OPT_CODE" "option-select returns code=200"
OPT_FOUND=$(echo "$OPT_RESP" | python3 -c "
import sys,json
data = json.load(sys.stdin)['data']
print(any(p['postId'] == '$POST_ID' for p in data))
")
assert_eq "True" "$OPT_FOUND" "test post found in option-select"

# ---------------------------------------------------------------------------
# Step 7: Delete post
# ---------------------------------------------------------------------------
step "7. DELETE /system/post/${POST_ID}"
DEL_RESP=$(curl -sS -X DELETE "$BASE/system/post/$POST_ID" "${H[@]}")
DEL_CODE=$(echo "$DEL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_CODE" "delete post returns code=200"

# ---------------------------------------------------------------------------
# Step 8: Verify deleted
# ---------------------------------------------------------------------------
step "8. Verify deleted (GET returns code != 200)"
VERIFY_RESP=$(curl -sS "$BASE/system/post/$POST_ID" "${H[@]}")
VERIFY_CODE=$(echo "$VERIFY_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
if [[ "$VERIFY_CODE" == "200" ]]; then
  echo "FAIL: post still exists after delete"
  exit 1
fi
echo "  OK: post no longer exists (code=$VERIFY_CODE)"

echo ""
echo "ALL 8 STEPS PASSED"

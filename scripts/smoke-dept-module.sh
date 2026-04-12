#!/usr/bin/env bash
# scripts/smoke-dept-module.sh
#
# End-to-end verification of 8 dept module endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates `smk-d-` prefixed depts, exercises every endpoint,
# then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-d-${TS_SHORT}"
TOKEN=""
ROOT_ID=""
CHILD_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_dept WHERE dept_name LIKE '${PREFIX}%';" 2>/dev/null || true
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

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

# ---------------------------------------------------------------------------
# Step 2: Create root dept (parentId="0")
# ---------------------------------------------------------------------------
step "2. POST create root dept"
ROOT_NAME="${PREFIX}-root"
CREATED_ROOT=$(curl -sS -X POST "$BASE/system/dept/" "${H[@]}" \
  -d "{\"parentId\":\"0\",\"deptName\":\"${ROOT_NAME}\",\"orderNum\":1,\"status\":\"0\"}")
echo "$CREATED_ROOT" | python3 -m json.tool
RESP_CODE=$(echo "$CREATED_ROOT" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
ROOT_ID=$(echo "$CREATED_ROOT" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['deptId'])")
assert_eq 200 "$RESP_CODE" "create root returns code=200"
assert_eq 36 "${#ROOT_ID}" "root deptId is a uuid"

# ---------------------------------------------------------------------------
# Step 3: Create child dept under root
# ---------------------------------------------------------------------------
step "3. POST create child dept"
CHILD_NAME="${PREFIX}-child"
CREATED_CHILD=$(curl -sS -X POST "$BASE/system/dept/" "${H[@]}" \
  -d "{\"parentId\":\"${ROOT_ID}\",\"deptName\":\"${CHILD_NAME}\",\"orderNum\":1,\"status\":\"0\"}")
echo "$CREATED_CHILD" | python3 -m json.tool
CHILD_CODE=$(echo "$CREATED_CHILD" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
CHILD_ID=$(echo "$CREATED_CHILD" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['deptId'])")
assert_eq 200 "$CHILD_CODE" "create child returns code=200"
assert_eq 36 "${#CHILD_ID}" "child deptId is a uuid"

# ---------------------------------------------------------------------------
# Step 4: List depts
# ---------------------------------------------------------------------------
step "4. GET /system/dept/list"
LIST_RESP=$(curl -sS "$BASE/system/dept/list" "${H[@]}")
LIST_CODE=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_CODE" "list depts returns code=200"

# ---------------------------------------------------------------------------
# Step 5: Get detail of root — assert deptName matches
# ---------------------------------------------------------------------------
step "5. GET /system/dept/${ROOT_ID}"
DETAIL_RESP=$(curl -sS "$BASE/system/dept/$ROOT_ID" "${H[@]}")
echo "$DETAIL_RESP" | python3 -m json.tool
DETAIL_NAME=$(echo "$DETAIL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['deptName'])")
assert_eq "$ROOT_NAME" "$DETAIL_NAME" "detail deptName matches"

# ---------------------------------------------------------------------------
# Step 6: Update root's leader field
# ---------------------------------------------------------------------------
step "6. PUT update root leader"
UPD_RESP=$(curl -sS -X PUT "$BASE/system/dept/" "${H[@]}" \
  -d "{\"deptId\":\"${ROOT_ID}\",\"parentId\":\"0\",\"leader\":\"smoke-leader\"}")
UPD_CODE=$(echo "$UPD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UPD_CODE" "update dept returns code=200"

# Verify leader changed
DETAIL_AFTER=$(curl -sS "$BASE/system/dept/$ROOT_ID" "${H[@]}")
LEADER_VAL=$(echo "$DETAIL_AFTER" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['leader'])")
assert_eq "smoke-leader" "$LEADER_VAL" "leader was updated"

# ---------------------------------------------------------------------------
# Step 7: Exclude list (exclude root) — assert child NOT in result
# ---------------------------------------------------------------------------
step "7. GET /system/dept/list/exclude/${ROOT_ID}"
EXCL_RESP=$(curl -sS "$BASE/system/dept/list/exclude/$ROOT_ID" "${H[@]}")
EXCL_CODE=$(echo "$EXCL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$EXCL_CODE" "exclude list returns code=200"

# Verify child is NOT in the exclude list (it's a descendant of root)
CHILD_IN_EXCL=$(echo "$EXCL_RESP" | python3 -c "
import sys,json
data = json.load(sys.stdin)['data']
print(any(d['deptId'] == '$CHILD_ID' for d in data))
")
assert_eq "False" "$CHILD_IN_EXCL" "child excluded from exclude-list (descendant of root)"

# ---------------------------------------------------------------------------
# Step 8: Delete child, then delete root
# ---------------------------------------------------------------------------
step "8. DELETE child then root"
DEL_CHILD=$(curl -sS -X DELETE "$BASE/system/dept/$CHILD_ID" "${H[@]}")
DEL_CHILD_CODE=$(echo "$DEL_CHILD" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_CHILD_CODE" "delete child returns code=200"

DEL_ROOT=$(curl -sS -X DELETE "$BASE/system/dept/$ROOT_ID" "${H[@]}")
DEL_ROOT_CODE=$(echo "$DEL_ROOT" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_ROOT_CODE" "delete root returns code=200"

echo ""
echo "ALL 8 STEPS PASSED"

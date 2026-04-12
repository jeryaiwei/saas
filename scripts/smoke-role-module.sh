#!/usr/bin/env bash
# scripts/smoke-role-module.sh
#
# End-to-end verification of all 11 role endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates an `it-smoke-` prefixed role, exercises every
# endpoint, then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
PREFIX="it-smoke-$(date +%s)"
ROLE_NAME="${PREFIX}-role"
ROLE_KEY="${PREFIX}:role"
TOKEN=""
ROLE_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  if [[ -n "${ROLE_ID:-}" ]]; then
    PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
      "DELETE FROM sys_role_menu WHERE role_id='$ROLE_ID'; \
       DELETE FROM sys_user_role WHERE role_id='$ROLE_ID'; \
       DELETE FROM sys_role WHERE role_id='$ROLE_ID';" || true
  fi
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

step "1. login"
TOKEN=$(curl -sS -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
assert_eq true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

step "2. pick 3 menu ids"
MENU_IDS=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT menu_id FROM sys_menu WHERE del_flag='0' AND perms <> '' ORDER BY menu_id LIMIT 3;" \
  | tr -d ' ' | grep -v '^$' \
  | python3 -c "import sys,json; print(json.dumps([l.strip() for l in sys.stdin if l.strip()]))")
echo "menu_ids: $MENU_IDS"

step "3. POST create"
CREATED=$(curl -sS -X POST "$BASE/system/role/" "${H[@]}" \
  -d "{\"roleName\":\"$ROLE_NAME\",\"roleKey\":\"$ROLE_KEY\",\"roleSort\":100,\"menuIds\":$MENU_IDS}")
echo "$CREATED" | python3 -m json.tool
ROLE_ID=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['roleId'])")
assert_eq 36 "${#ROLE_ID}" "role_id is a uuid"

step "4. GET /list (new role visible)"
LIST=$(curl -sS "$BASE/system/role/list?roleKey=$ROLE_KEY&pageNum=1&pageSize=10" "${H[@]}")
COUNT=$(echo "$LIST" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 1 "$COUNT" "list returns exactly the new role"

step "5. GET /:id"
DETAIL=$(curl -sS "$BASE/system/role/$ROLE_ID" "${H[@]}")
MENU_COUNT=$(echo "$DETAIL" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['menuIds']))")
assert_eq 3 "$MENU_COUNT" "role detail has 3 bound menus"

step "6. PUT update (replace menus with 2, rename)"
NEW_MENUS=$(echo "$MENU_IDS" | python3 -c "import sys,json; ids=json.load(sys.stdin); print(json.dumps(ids[:2]))")
curl -sS -X PUT "$BASE/system/role/" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"roleName\":\"${ROLE_NAME}-v2\",\"roleKey\":\"$ROLE_KEY\",\"roleSort\":200,\"status\":\"0\",\"menuIds\":$NEW_MENUS}" > /dev/null

DETAIL=$(curl -sS "$BASE/system/role/$ROLE_ID" "${H[@]}")
MENU_COUNT=$(echo "$DETAIL" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['menuIds']))")
assert_eq 2 "$MENU_COUNT" "after update, menu count is 2"

step "7. PUT change-status → disable"
curl -sS -X PUT "$BASE/system/role/change-status" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"status\":\"1\"}" > /dev/null

STATUS=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT status FROM sys_role WHERE role_id='$ROLE_ID';" | tr -d ' \n')
assert_eq 1 "$STATUS" "status is now 1 (disabled)"

step "8. GET /option-select (disabled role hidden)"
OPTIONS=$(curl -sS "$BASE/system/role/option-select" "${H[@]}")
FOUND=$(echo "$OPTIONS" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(any(r['roleKey']=='$ROLE_KEY' for r in d))")
assert_eq False "$FOUND" "disabled role filtered out of option-select"

step "9. Re-enable role"
curl -sS -X PUT "$BASE/system/role/change-status" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"status\":\"0\"}" > /dev/null

step "10. PUT select-all → assign admin user"
ADMIN_ID="cf827fc0-e7cc-4b9f-913c-e20628ade20a"
curl -sS -X PUT "$BASE/system/role/auth-user/select-all" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"userIds\":[\"$ADMIN_ID\"]}" > /dev/null

ALLOC=$(curl -sS "$BASE/system/role/auth-user/allocated-list?roleId=$ROLE_ID&pageNum=1&pageSize=10" "${H[@]}")
ALLOC_COUNT=$(echo "$ALLOC" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 1 "$ALLOC_COUNT" "allocated list has 1 user after assign"

step "11. select-all is idempotent (re-submit)"
curl -sS -X PUT "$BASE/system/role/auth-user/select-all" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"userIds\":[\"$ADMIN_ID\"]}" > /dev/null

ALLOC=$(curl -sS "$BASE/system/role/auth-user/allocated-list?roleId=$ROLE_ID&pageNum=1&pageSize=10" "${H[@]}")
ALLOC_COUNT=$(echo "$ALLOC" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 1 "$ALLOC_COUNT" "allocated list still 1 after idempotent re-assign"

step "12. GET unallocated-list (admin not listed)"
UNALLOC=$(curl -sS "$BASE/system/role/auth-user/unallocated-list?roleId=$ROLE_ID&pageNum=1&pageSize=50" "${H[@]}")
FOUND=$(echo "$UNALLOC" | python3 -c "import sys,json; print(any(r['userName']=='admin' for r in json.load(sys.stdin)['data']['rows']))")
assert_eq False "$FOUND" "admin not in unallocated list"

step "13. PUT cancel → unassign admin"
curl -sS -X PUT "$BASE/system/role/auth-user/cancel" "${H[@]}" \
  -d "{\"roleId\":\"$ROLE_ID\",\"userIds\":[\"$ADMIN_ID\"]}" > /dev/null

ALLOC=$(curl -sS "$BASE/system/role/auth-user/allocated-list?roleId=$ROLE_ID&pageNum=1&pageSize=10" "${H[@]}")
ALLOC_COUNT=$(echo "$ALLOC" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 0 "$ALLOC_COUNT" "allocated list empty after cancel"

step "14. DELETE /:id → soft delete"
curl -sS -X DELETE "$BASE/system/role/$ROLE_ID" "${H[@]}" > /dev/null

DETAIL=$(curl -sS "$BASE/system/role/$ROLE_ID" "${H[@]}")
CODE=$(echo "$DETAIL" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1001 "$CODE" "detail returns DATA_NOT_FOUND after soft delete"

DEL_FLAG=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT del_flag FROM sys_role WHERE role_id='$ROLE_ID';" | tr -d ' \n')
assert_eq 1 "$DEL_FLAG" "row has del_flag='1' in DB"

echo ""
echo "ALL 14 STEPS PASSED"

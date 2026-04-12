#!/usr/bin/env bash
# scripts/smoke-menu-module.sh
#
# End-to-end verification of 10 menu module endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates `smk-m-` prefixed menus, exercises every endpoint,
# then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
# Use short timestamp to fit varchar(50) menu_name
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-m-${TS_SHORT}"
TOKEN=""
DIR_ID=""
PAGE_ID=""
BTN_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_role_menu WHERE menu_id IN (SELECT menu_id FROM sys_menu WHERE menu_name LIKE '${PREFIX}%');
     DELETE FROM sys_menu WHERE menu_name LIKE '${PREFIX}%';" 2>/dev/null || true
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
# Step 2: Create directory menu (type M)
# ---------------------------------------------------------------------------
step "2. POST create directory menu (type M)"
DIR_NAME="${PREFIX}-dir"
CREATED_DIR=$(curl -sS -X POST "$BASE/system/menu/" "${H[@]}" \
  -d "{\"menuName\":\"${DIR_NAME}\",\"orderNum\":1,\"path\":\"/${PREFIX}-dir\",\"isFrame\":\"1\",\"isCache\":\"0\",\"menuType\":\"M\",\"visible\":\"0\",\"status\":\"0\",\"icon\":\"menu\"}")
echo "$CREATED_DIR" | python3 -m json.tool
RESP_CODE=$(echo "$CREATED_DIR" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
DIR_ID=$(echo "$CREATED_DIR" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['menuId'])")
assert_eq 200 "$RESP_CODE" "create directory returns code=200"
assert_eq 36 "${#DIR_ID}" "dir menuId is a uuid"

# ---------------------------------------------------------------------------
# Step 3: Create child page menu (type C, parentId=DIR_ID)
# ---------------------------------------------------------------------------
step "3. POST create page menu (type C)"
PAGE_NAME="${PREFIX}-page"
CREATED_PAGE=$(curl -sS -X POST "$BASE/system/menu/" "${H[@]}" \
  -d "{\"menuName\":\"${PAGE_NAME}\",\"parentId\":\"${DIR_ID}\",\"orderNum\":1,\"path\":\"/${PREFIX}-page\",\"component\":\"system/test/index\",\"isFrame\":\"1\",\"isCache\":\"0\",\"menuType\":\"C\",\"visible\":\"0\",\"status\":\"0\",\"icon\":\"page\"}")
echo "$CREATED_PAGE" | python3 -m json.tool
PAGE_CODE=$(echo "$CREATED_PAGE" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
PAGE_ID=$(echo "$CREATED_PAGE" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['menuId'])")
assert_eq 200 "$PAGE_CODE" "create page returns code=200"
assert_eq 36 "${#PAGE_ID}" "page menuId is a uuid"

# ---------------------------------------------------------------------------
# Step 4: Create button (type F, parentId=PAGE_ID, perms)
# ---------------------------------------------------------------------------
step "4. POST create button menu (type F)"
BTN_NAME="${PREFIX}-btn"
CREATED_BTN=$(curl -sS -X POST "$BASE/system/menu/" "${H[@]}" \
  -d "{\"menuName\":\"${BTN_NAME}\",\"parentId\":\"${PAGE_ID}\",\"orderNum\":1,\"path\":\"\",\"isFrame\":\"1\",\"isCache\":\"0\",\"menuType\":\"F\",\"visible\":\"0\",\"status\":\"0\",\"perms\":\"system:test:btn\"}")
echo "$CREATED_BTN" | python3 -m json.tool
BTN_CODE=$(echo "$CREATED_BTN" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
BTN_ID=$(echo "$CREATED_BTN" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['menuId'])")
assert_eq 200 "$BTN_CODE" "create button returns code=200"
assert_eq 36 "${#BTN_ID}" "button menuId is a uuid"

# ---------------------------------------------------------------------------
# Step 5: List menus
# ---------------------------------------------------------------------------
step "5. GET /system/menu/list"
LIST_RESP=$(curl -sS "$BASE/system/menu/list" "${H[@]}")
LIST_CODE=$(echo "$LIST_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_CODE" "list menus returns code=200"

# ---------------------------------------------------------------------------
# Step 6: Get detail of directory
# ---------------------------------------------------------------------------
step "6. GET /system/menu/${DIR_ID}"
DETAIL_RESP=$(curl -sS "$BASE/system/menu/$DIR_ID" "${H[@]}")
echo "$DETAIL_RESP" | python3 -m json.tool
DETAIL_NAME=$(echo "$DETAIL_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['menuName'])")
assert_eq "$DIR_NAME" "$DETAIL_NAME" "detail menuName matches"

# ---------------------------------------------------------------------------
# Step 7: Update directory icon
# ---------------------------------------------------------------------------
step "7. PUT update directory icon"
UPD_RESP=$(curl -sS -X PUT "$BASE/system/menu/" "${H[@]}" \
  -d "{\"menuId\":\"${DIR_ID}\",\"icon\":\"updated-icon\"}")
UPD_CODE=$(echo "$UPD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UPD_CODE" "update menu returns code=200"

# ---------------------------------------------------------------------------
# Step 8: Tree-select
# ---------------------------------------------------------------------------
step "8. GET /system/menu/tree-select"
TREE_RESP=$(curl -sS "$BASE/system/menu/tree-select" "${H[@]}")
TREE_CODE=$(echo "$TREE_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$TREE_CODE" "tree-select returns code=200"

# ---------------------------------------------------------------------------
# Step 9: Delete button
# ---------------------------------------------------------------------------
step "9. DELETE /system/menu/${BTN_ID}"
DEL_BTN_RESP=$(curl -sS -X DELETE "$BASE/system/menu/$BTN_ID" "${H[@]}")
DEL_BTN_CODE=$(echo "$DEL_BTN_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_BTN_CODE" "delete button returns code=200"

# ---------------------------------------------------------------------------
# Step 10: Cascade delete directory (should delete DIR + PAGE)
# ---------------------------------------------------------------------------
step "10. DELETE /system/menu/cascade/${DIR_ID}"
CASCADE_RESP=$(curl -sS -X DELETE "$BASE/system/menu/cascade/$DIR_ID" "${H[@]}")
echo "$CASCADE_RESP" | python3 -m json.tool
CASCADE_CODE=$(echo "$CASCADE_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$CASCADE_CODE" "cascade delete returns code=200"

echo ""
echo "ALL 10 STEPS PASSED"

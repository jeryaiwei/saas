#!/usr/bin/env bash
# scripts/smoke-user-module.sh
#
# End-to-end verification of all 11 user endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates an `it-smoke-user-` prefixed user, exercises every
# endpoint, then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
PREFIX="it-smoke-user-$(date +%s)"
USER_NAME="${PREFIX}-u"
TOKEN=""
NEW_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  if [[ -n "${NEW_ID:-}" ]]; then
    PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
      "DELETE FROM sys_user_role WHERE user_id='$NEW_ID'; \
       DELETE FROM sys_user_tenant WHERE user_id='$NEW_ID'; \
       DELETE FROM sys_user WHERE user_id='$NEW_ID';" || true
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

step() { echo ""; echo "=== $1 ==="; }

step "1. login"
TOKEN=$(curl -sS -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
assert_eq true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

step "2. pick a real role id"
ROLE_ID=$(curl -sS "$BASE/system/role/option-select" "${H[@]}" \
  | python3 -c "
import sys,json
roles=[r for r in json.load(sys.stdin)['data'] if r['roleId'] != '00000000-0000-0000-0000-000000000000']
print(roles[0]['roleId'])")
echo "role_id: $ROLE_ID"

step "3. POST create user"
NICK="${USER_NAME:0:28}-n"
CREATED=$(curl -sS -X POST "$BASE/system/user/" "${H[@]}" \
  -d "{\"userName\":\"$USER_NAME\",\"nickName\":\"$NICK\",\"password\":\"abc123\",\"roleIds\":[\"$ROLE_ID\"]}")
echo "$CREATED" | python3 -m json.tool
NEW_ID=$(echo "$CREATED" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['userId'])")
assert_eq 36 "${#NEW_ID}" "user_id is a uuid"

step "4. GET list (filter by userName)"
LIST=$(curl -sS "$BASE/system/user/list?userName=$USER_NAME&pageNum=1&pageSize=10" "${H[@]}")
COUNT=$(echo "$LIST" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['rows']))")
assert_eq 1 "$COUNT" "list returns the new user"

step "5. GET /{id}"
DETAIL=$(curl -sS "$BASE/system/user/$NEW_ID" "${H[@]}")
ROLE_COUNT=$(echo "$DETAIL" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['roleIds']))")
assert_eq 1 "$ROLE_COUNT" "user has 1 bound role"

step "6. PUT update (change nick + clear roles)"
NICK_UPDATED="${USER_NAME:0:22}-upd"
curl -sS -X PUT "$BASE/system/user/" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"nickName\":\"$NICK_UPDATED\",\"email\":\"\",\"phonenumber\":\"\",\"sex\":\"0\",\"avatar\":\"\",\"status\":\"0\",\"roleIds\":[]}" > /dev/null
DETAIL=$(curl -sS "$BASE/system/user/$NEW_ID" "${H[@]}")
NICK=$(echo "$DETAIL" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['nickName'])")
assert_eq "$NICK_UPDATED" "$NICK" "nick_name updated"

step "7. PUT change-status disable"
curl -sS -X PUT "$BASE/system/user/change-status" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"status\":\"1\"}" > /dev/null
STATUS=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT status FROM sys_user WHERE user_id='$NEW_ID';" | tr -d ' \n')
assert_eq 1 "$STATUS" "disabled in DB"

step "8. option-select excludes disabled user"
OPTS=$(curl -sS "$BASE/system/user/option-select?userName=$USER_NAME" "${H[@]}")
VISIBLE=$(echo "$OPTS" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(any(u['userName']=='$USER_NAME' for u in d))")
assert_eq False "$VISIBLE" "disabled user hidden from option-select"

step "9. change-status on admin (expect 1004)"
CODE=$(curl -sS -X PUT "$BASE/system/user/change-status" "${H[@]}" \
  -d '{"userId":"cf827fc0-e7cc-4b9f-913c-e20628ade20a","status":"1"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1004 "$CODE" "admin status change blocked"

step "10. Re-enable test user"
curl -sS -X PUT "$BASE/system/user/change-status" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"status\":\"0\"}" > /dev/null

step "11. PUT reset-pwd"
CODE=$(curl -sS -X PUT "$BASE/system/user/reset-pwd" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"password\":\"newpwd1\"}" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$CODE" "reset-pwd succeeded"

step "12. reset-pwd on admin (expect 1004)"
CODE=$(curl -sS -X PUT "$BASE/system/user/reset-pwd" "${H[@]}" \
  -d '{"userId":"cf827fc0-e7cc-4b9f-913c-e20628ade20a","password":"anything1"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1004 "$CODE" "admin reset blocked"

step "13. GET auth-role/{id}"
AUTH=$(curl -sS "$BASE/system/user/auth-role/$NEW_ID" "${H[@]}")
AUTH_ROLES=$(echo "$AUTH" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['roleIds']))")
assert_eq 0 "$AUTH_ROLES" "auth-role returns current (empty) role list"

step "14. PUT auth-role with 1 role"
curl -sS -X PUT "$BASE/system/user/auth-role" "${H[@]}" \
  -d "{\"userId\":\"$NEW_ID\",\"roleIds\":[\"$ROLE_ID\"]}" > /dev/null
AUTH=$(curl -sS "$BASE/system/user/auth-role/$NEW_ID" "${H[@]}")
AUTH_ROLES=$(echo "$AUTH" | python3 -c "import sys,json; print(len(json.load(sys.stdin)['data']['roleIds']))")
assert_eq 1 "$AUTH_ROLES" "auth-role now has 1 role"

step "15. DELETE /{id}"
curl -sS -X DELETE "$BASE/system/user/$NEW_ID" "${H[@]}" > /dev/null
CODE=$(curl -sS "$BASE/system/user/$NEW_ID" "${H[@]}" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1001 "$CODE" "find returns 1001 after delete"
DEL_FLAG=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT del_flag FROM sys_user WHERE user_id='$NEW_ID';" | tr -d ' \n')
assert_eq 1 "$DEL_FLAG" "del_flag = 1"

step "16. DELETE admin (expect 1004)"
CODE=$(curl -sS -X DELETE "$BASE/system/user/cf827fc0-e7cc-4b9f-913c-e20628ade20a" "${H[@]}" \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 1004 "$CODE" "admin delete blocked"

echo ""
echo "ALL 16 STEPS PASSED"

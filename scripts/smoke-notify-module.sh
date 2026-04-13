#!/usr/bin/env bash
# scripts/smoke-notify-module.sh
#
# End-to-end verification of 12 notify template + message endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates `smk-nt-` prefixed templates/messages, exercises every
# endpoint, then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TS_SHORT=$(date +%s | tail -c 7)
PREFIX="smk-nt-${TS_SHORT}"
TOKEN=""
USER_ID=""
TEMPLATE_ID=""
MESSAGE_ID=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_notify_message WHERE template_code LIKE '${PREFIX}%'; DELETE FROM sys_notify_template WHERE code LIKE '${PREFIX}%';" 2>/dev/null || true
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
# Step 1: Login + get userId
# ---------------------------------------------------------------------------
step "1. login"
TOKEN=$(curl -sS -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
assert_eq true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" -H "tenant-id: 000000")

# Get current user's userId
USER_INFO=$(curl -sS "$BASE/system/user/info" "${H[@]}")
USER_ID=$(echo "$USER_INFO" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['userId'])")
assert_eq 36 "${#USER_ID}" "userId is a uuid"

# ---------------------------------------------------------------------------
# Step 2: Create notify template
# ---------------------------------------------------------------------------
step "2. POST create notify template"
TPL_NAME="${PREFIX}-tpl"
TPL_CODE="${PREFIX}-code"
CREATED_TPL=$(curl -sS -X POST "$BASE/message/notify-template/" "${H[@]}" \
  -d "{\"name\":\"${TPL_NAME}\",\"code\":\"${TPL_CODE}\",\"nickname\":\"test\",\"content\":\"hello {name}\",\"type\":1,\"status\":\"0\"}")
echo "$CREATED_TPL" | python3 -m json.tool
TPL_RESP_CODE=$(echo "$CREATED_TPL" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
TEMPLATE_ID=$(echo "$CREATED_TPL" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(d.get('id') or d.get('templateId'))")
assert_eq 200 "$TPL_RESP_CODE" "create template returns code=200"

# ---------------------------------------------------------------------------
# Step 3: List templates
# ---------------------------------------------------------------------------
step "3. GET /message/notify-template/list"
LIST_TPL=$(curl -sS "$BASE/message/notify-template/list?pageNum=1&pageSize=10" "${H[@]}")
LIST_TPL_CODE=$(echo "$LIST_TPL" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$LIST_TPL_CODE" "list templates returns code=200"

# ---------------------------------------------------------------------------
# Step 4: Get template detail
# ---------------------------------------------------------------------------
step "4. GET /message/notify-template/${TEMPLATE_ID}"
TPL_DETAIL=$(curl -sS "$BASE/message/notify-template/$TEMPLATE_ID" "${H[@]}")
echo "$TPL_DETAIL" | python3 -m json.tool
TPL_DETAIL_CODE=$(echo "$TPL_DETAIL" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$TPL_DETAIL_CODE" "template detail returns code=200"

# ---------------------------------------------------------------------------
# Step 5: Template dropdown/select
# ---------------------------------------------------------------------------
step "5. GET /message/notify-template/select"
SELECT_RESP=$(curl -sS "$BASE/message/notify-template/select" "${H[@]}")
SELECT_CODE=$(echo "$SELECT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$SELECT_CODE" "template select returns code=200"

# ---------------------------------------------------------------------------
# Step 6: Send notify message
# ---------------------------------------------------------------------------
step "6. POST send notify message"
SEND_RESP=$(curl -sS -X POST "$BASE/message/notify-message/send" "${H[@]}" \
  -d "{\"userId\":\"${USER_ID}\",\"templateId\":${TEMPLATE_ID},\"templateCode\":\"${TPL_CODE}\",\"templateNickname\":\"test\",\"templateContent\":\"hello world\"}")
echo "$SEND_RESP" | python3 -m json.tool
SEND_CODE=$(echo "$SEND_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$SEND_CODE" "send message returns code=200"
MESSAGE_ID=$(echo "$SEND_RESP" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(d.get('id') or d.get('messageId'))")

# ---------------------------------------------------------------------------
# Step 7: My messages list
# ---------------------------------------------------------------------------
step "7. GET /message/notify-message/my-list"
MY_LIST=$(curl -sS "$BASE/message/notify-message/my-list?pageNum=1&pageSize=10" "${H[@]}")
MY_LIST_CODE=$(echo "$MY_LIST" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$MY_LIST_CODE" "my-list returns code=200"

# If MESSAGE_ID was not returned from send, extract from my-list
if [[ -z "$MESSAGE_ID" || "$MESSAGE_ID" == "None" ]]; then
  MESSAGE_ID=$(echo "$MY_LIST" | python3 -c "
import sys,json
rows = json.load(sys.stdin)['data']['rows']
for r in rows:
  if r.get('templateCode','') == '${TPL_CODE}':
    print(r.get('id') or r.get('messageId'))
    break
")
fi

# ---------------------------------------------------------------------------
# Step 8: Unread count >= 1
# ---------------------------------------------------------------------------
step "8. GET /message/notify-message/unread-count"
UNREAD_RESP=$(curl -sS "$BASE/message/notify-message/unread-count" "${H[@]}")
echo "$UNREAD_RESP" | python3 -m json.tool
UNREAD_CODE=$(echo "$UNREAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UNREAD_CODE" "unread-count returns code=200"
UNREAD_COUNT=$(echo "$UNREAD_RESP" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(d if isinstance(d,int) else d.get('count',0))")
assert_eq true "$([ "$UNREAD_COUNT" -ge 1 ] && echo true || echo false)" "unread count >= 1 (got $UNREAD_COUNT)"

# ---------------------------------------------------------------------------
# Step 9: Mark message as read
# ---------------------------------------------------------------------------
step "9. PUT /message/notify-message/read/${MESSAGE_ID}"
READ_RESP=$(curl -sS -X PUT "$BASE/message/notify-message/read/$MESSAGE_ID" "${H[@]}")
READ_CODE=$(echo "$READ_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$READ_CODE" "mark read returns code=200"

# ---------------------------------------------------------------------------
# Step 10: Verify unread count decreased
# ---------------------------------------------------------------------------
step "10. verify unread count decreased"
UNREAD_AFTER=$(curl -sS "$BASE/message/notify-message/unread-count" "${H[@]}")
UNREAD_COUNT_AFTER=$(echo "$UNREAD_AFTER" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(d if isinstance(d,int) else d.get('count',0))")
assert_eq true "$([ "$UNREAD_COUNT_AFTER" -lt "$UNREAD_COUNT" ] && echo true || echo false)" "unread count decreased (was $UNREAD_COUNT, now $UNREAD_COUNT_AFTER)"

# ---------------------------------------------------------------------------
# Step 11: Delete message
# ---------------------------------------------------------------------------
step "11. DELETE /message/notify-message/${MESSAGE_ID}"
DEL_MSG=$(curl -sS -X DELETE "$BASE/message/notify-message/$MESSAGE_ID" "${H[@]}")
DEL_MSG_CODE=$(echo "$DEL_MSG" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_MSG_CODE" "delete message returns code=200"

# ---------------------------------------------------------------------------
# Step 12: Delete template
# ---------------------------------------------------------------------------
step "12. DELETE /message/notify-template/${TEMPLATE_ID}"
DEL_TPL=$(curl -sS -X DELETE "$BASE/message/notify-template/$TEMPLATE_ID" "${H[@]}")
DEL_TPL_CODE=$(echo "$DEL_TPL" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_TPL_CODE" "delete template returns code=200"

echo ""
echo "ALL 12 STEPS PASSED"

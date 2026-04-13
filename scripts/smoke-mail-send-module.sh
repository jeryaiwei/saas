#!/usr/bin/env bash
# scripts/smoke-mail-send-module.sh
#
# End-to-end verification of mail send error paths.
# Real SMTP sending NOT tested (requires mail account config).
# Tests: template not found, batch limit, resend not found, invalid email.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
TOKEN=""

assert_code() {
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
assert_code true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json")

step "2. POST /message/mail-send — template not found"
RESP=$(curl -sS -X POST "$BASE/message/mail-send" "${H[@]}" \
  -d '{"toMail":"test@example.com","templateCode":"nonexistent-template-xxx"}')
CODE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_code "7140" "$CODE" "mail template not found (7140)"

step "3. POST /message/mail-send/batch — exceeds 100 limit"
EMAILS=$(python3 -c "import json; print(json.dumps([f'u{i}@x.com' for i in range(101)]))")
RESP=$(curl -sS -X POST "$BASE/message/mail-send/batch" "${H[@]}" \
  -d "{\"toMails\":$EMAILS,\"templateCode\":\"any\"}")
CODE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_code "7190" "$CODE" "batch size exceeded (7190)"

step "4. POST /message/mail-send/resend/999999 — log not found"
RESP=$(curl -sS -X POST "$BASE/message/mail-send/resend/999999" "${H[@]}")
CODE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_code "7191" "$CODE" "send log not found (7191)"

step "5. POST /message/mail-send/batch — invalid email"
RESP=$(curl -sS -X POST "$BASE/message/mail-send/batch" "${H[@]}" \
  -d '{"toMails":["not-an-email"],"templateCode":"any"}')
CODE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_code "1000" "$CODE" "invalid email param (1000)"

step "6. POST /message/mail-send/test — account not found"
RESP=$(curl -sS -X POST "$BASE/message/mail-send/test" "${H[@]}" \
  -d '{"toMail":"test@example.com","accountId":999999}')
CODE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_code "7130" "$CODE" "mail account not found (7130)"

echo ""
echo "=== ALL 6 STEPS PASSED ==="

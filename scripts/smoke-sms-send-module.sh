#!/usr/bin/env bash
# scripts/smoke-sms-send-module.sh
#
# End-to-end verification of SMS send error paths.
# Real SMS API NOT tested (requires channel config).
# Tests: template not found, batch limit, resend not found.

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

step "2. POST /message/sms-send — template not found"
RESP=$(curl -sS -X POST "$BASE/message/sms-send" "${H[@]}" \
  -d '{"mobile":"13800138000","templateCode":"nonexistent-sms-xxx"}')
CODE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_code "7160" "$CODE" "sms template not found (7160)"

step "3. POST /message/sms-send/batch — exceeds 100 limit"
MOBILES=$(python3 -c "import json; print(json.dumps([f'1380013{i:04d}' for i in range(101)]))")
RESP=$(curl -sS -X POST "$BASE/message/sms-send/batch" "${H[@]}" \
  -d "{\"mobiles\":$MOBILES,\"templateCode\":\"any\"}")
CODE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_code "7190" "$CODE" "batch size exceeded (7190)"

step "4. POST /message/sms-send/resend/999999 — log not found"
RESP=$(curl -sS -X POST "$BASE/message/sms-send/resend/999999" "${H[@]}")
CODE=$(echo "$RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_code "7191" "$CODE" "send log not found (7191)"

echo ""
echo "=== ALL 4 STEPS PASSED ==="

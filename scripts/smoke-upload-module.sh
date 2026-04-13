#!/usr/bin/env bash
# scripts/smoke-upload-module.sh
#
# End-to-end verification of file upload endpoints.
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script uploads a test file, downloads it, verifies MD5 instant-upload,
# then cleans up via psql.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
PREFIX="it-smoke-upload-$(date +%s)"
UPLOAD_ID=""
UPLOAD_ID2=""
TOKEN=""
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_upload WHERE file_name LIKE '${PREFIX}%';" 2>/dev/null || true
  # Clean up test file from local storage
  rm -f /tmp/${PREFIX}-test.txt 2>/dev/null || true
  CLEANUP_DONE=true
  echo "cleanup done"
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

# ─── Tests ───────────────────────────────────────────────────────────────────

step "1. login"
TOKEN=$(curl -sS -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['access_token'])")
assert_eq true "$([ -n "$TOKEN" ] && echo true || echo false)" "token received"

H=(-H "Authorization: Bearer $TOKEN")

step "2. create test file"
echo "hello upload smoke test ${PREFIX}" > /tmp/${PREFIX}-test.txt
FILE_SIZE=$(wc -c < /tmp/${PREFIX}-test.txt | tr -d ' ')
echo "  file: /tmp/${PREFIX}-test.txt ($FILE_SIZE bytes)"

step "3. POST /common/upload (multipart)"
UPLOAD_RESP=$(curl -sS -X POST "$BASE/common/upload" \
  "${H[@]}" \
  -F "file=@/tmp/${PREFIX}-test.txt;filename=${PREFIX}-test.txt" \
  -F "folderId=")
echo "$UPLOAD_RESP" | python3 -m json.tool
CODE=$(echo "$UPLOAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq "200" "$CODE" "upload returns 200"
UPLOAD_ID=$(echo "$UPLOAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['uploadId'])")
assert_eq 36 "${#UPLOAD_ID}" "uploadId is a uuid"
FILE_MD5=$(echo "$UPLOAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['fileMd5'])")
assert_eq 32 "${#FILE_MD5}" "fileMd5 is 32 hex chars"
INSTANT=$(echo "$UPLOAD_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['instantUpload'])")
assert_eq "False" "$INSTANT" "first upload is not instant"

step "4. GET /common/upload/{uploadId} (download)"
HTTP_CODE=$(curl -sS -o /tmp/${PREFIX}-downloaded.txt -w "%{http_code}" \
  "$BASE/common/upload/$UPLOAD_ID" "${H[@]}")
assert_eq "200" "$HTTP_CODE" "download returns 200"
DOWNLOADED_CONTENT=$(cat /tmp/${PREFIX}-downloaded.txt)
ORIGINAL_CONTENT=$(cat /tmp/${PREFIX}-test.txt)
assert_eq "$ORIGINAL_CONTENT" "$DOWNLOADED_CONTENT" "downloaded content matches original"
rm -f /tmp/${PREFIX}-downloaded.txt

step "5. POST /common/upload again (MD5 instant upload)"
UPLOAD_RESP2=$(curl -sS -X POST "$BASE/common/upload" \
  "${H[@]}" \
  -F "file=@/tmp/${PREFIX}-test.txt;filename=${PREFIX}-test-copy.txt")
echo "$UPLOAD_RESP2" | python3 -m json.tool
CODE2=$(echo "$UPLOAD_RESP2" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq "200" "$CODE2" "second upload returns 200"
UPLOAD_ID2=$(echo "$UPLOAD_RESP2" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['uploadId'])")
INSTANT2=$(echo "$UPLOAD_RESP2" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['instantUpload'])")
assert_eq "True" "$INSTANT2" "second upload is instant (MD5 match)"
MD52=$(echo "$UPLOAD_RESP2" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['fileMd5'])")
assert_eq "$FILE_MD5" "$MD52" "MD5 matches first upload"

step "6. verify DB records"
DB_COUNT=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT COUNT(*) FROM sys_upload WHERE file_name LIKE '${PREFIX}%' AND del_flag='0';" | tr -d ' ')
assert_eq "2" "$DB_COUNT" "2 upload records in DB"

step "7. verify blocked extension"
echo "bad" > /tmp/${PREFIX}-test.exe
BLOCKED_RESP=$(curl -sS -X POST "$BASE/common/upload" \
  "${H[@]}" \
  -F "file=@/tmp/${PREFIX}-test.exe;filename=${PREFIX}-test.exe")
BLOCKED_CODE=$(echo "$BLOCKED_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq "5041" "$BLOCKED_CODE" "exe extension blocked (5041)"
rm -f /tmp/${PREFIX}-test.exe

echo ""
echo "=== ALL 7 STEPS PASSED ==="

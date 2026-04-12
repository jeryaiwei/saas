#!/usr/bin/env bash
# scripts/smoke-tenant-module.sh
#
# End-to-end verification of all 12 tenant module endpoints (6 tenant + 6 package).
# Prerequisites:
# - Rust app running on localhost:18080
# - saas_tea dev DB running with admin / admin123
# - psql + python3 available
#
# The script creates an `it-smoke-tenant-` prefixed tenant + package, exercises every
# endpoint, then cleans up its own rows via a trap on EXIT.

set -euo pipefail

BASE="http://127.0.0.1:18080/api/v1"
PREFIX="it-smoke-tenant-$(date +%s)"
TOKEN=""
PKG_ID=""
TENANT_ID=""  # UUID (not tenant_id)
CLEANUP_DONE=false

trap 'cleanup' EXIT INT TERM

cleanup() {
  if [[ "$CLEANUP_DONE" == "true" ]]; then return; fi
  echo ""
  echo "--- cleanup ---"
  PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -q -c \
    "DELETE FROM sys_user_tenant WHERE user_id IN (SELECT user_id FROM sys_user WHERE user_name LIKE '${PREFIX}%' OR user_name LIKE 'smk-%');
     DELETE FROM sys_user WHERE user_name LIKE '${PREFIX}%' OR user_name LIKE 'smk-%';
     DELETE FROM sys_tenant WHERE company_name LIKE '${PREFIX}%';
     DELETE FROM sys_tenant_package WHERE package_id = '${PKG_ID}';" 2>/dev/null || true
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
# Step 2: Create package
# ---------------------------------------------------------------------------
step "2. POST create package"
# code is varchar(20) — use a short suffix derived from the timestamp
TS_SHORT="$(date +%s | tail -c 8)"
PKG_CODE="${TS_SHORT}-pkg"
PKG_NAME="${PREFIX}-Standard"
CREATED_PKG=$(curl -sS -X POST "$BASE/system/tenant-package/" "${H[@]}" \
  -d "{\"code\":\"${PKG_CODE}\",\"packageName\":\"${PKG_NAME}\",\"menuIds\":[]}")
echo "$CREATED_PKG" | python3 -m json.tool
PKG_ID=$(echo "$CREATED_PKG" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['packageId'])")
assert_eq 36 "${#PKG_ID}" "package_id is a uuid"

# ---------------------------------------------------------------------------
# Step 3: List packages
# ---------------------------------------------------------------------------
step "3. GET /list packages"
LIST_PKG=$(curl -sS "$BASE/system/tenant-package/list?pageNum=1&pageSize=10" "${H[@]}")
CODE=$(echo "$LIST_PKG" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$CODE" "list packages returns code=200"

# ---------------------------------------------------------------------------
# Step 4: Get package detail
# ---------------------------------------------------------------------------
step "4. GET /tenant-package/:id"
DETAIL_PKG=$(curl -sS "$BASE/system/tenant-package/$PKG_ID" "${H[@]}")
DETAIL_CODE=$(echo "$DETAIL_PKG" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['code'])")
assert_eq "$PKG_CODE" "$DETAIL_CODE" "package detail code matches"

# ---------------------------------------------------------------------------
# Step 5: Update package
# ---------------------------------------------------------------------------
step "5. PUT update package"
UPD_PKG=$(curl -sS -X PUT "$BASE/system/tenant-package/" "${H[@]}" \
  -d "{\"packageId\":\"$PKG_ID\",\"remark\":\"smoke-test-updated\"}")
UPD_CODE=$(echo "$UPD_PKG" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UPD_CODE" "update package returns code=200"

# ---------------------------------------------------------------------------
# Step 6: Create tenant
# ---------------------------------------------------------------------------
step "6. POST create tenant"
TENANT_COMPANY="${PREFIX}-Corp"
# username is varchar(30) — use short timestamp suffix
TENANT_USER="smk-${TS_SHORT}-adm"
CREATED_TENANT=$(curl -sS -X POST "$BASE/system/tenant/" "${H[@]}" \
  -d "{\"companyName\":\"${TENANT_COMPANY}\",\"username\":\"${TENANT_USER}\",\"password\":\"Admin@123456\",\"packageIds\":[\"${PKG_ID}\"]}")
echo "$CREATED_TENANT" | python3 -m json.tool
RESP_CODE=$(echo "$CREATED_TENANT" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$RESP_CODE" "create tenant returns code=200"

# ---------------------------------------------------------------------------
# Step 7: List tenants — capture TENANT_ID (UUID id field)
# ---------------------------------------------------------------------------
step "7. GET /list tenants"
LIST_TENANT=$(curl -sS "$BASE/system/tenant/list?pageNum=1&pageSize=10&companyName=${PREFIX}" "${H[@]}")
echo "$LIST_TENANT" | python3 -m json.tool
TOTAL=$(echo "$LIST_TENANT" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['total'])")
TENANT_ID=$(echo "$LIST_TENANT" | python3 -c "
import sys,json
rows=json.load(sys.stdin)['data']['rows']
match=[r for r in rows if r['companyName']=='${TENANT_COMPANY}']
print(match[0]['id'])")
assert_eq true "$([ "$TOTAL" -ge 1 ] && echo true || echo false)" "tenant list total >= 1"
assert_eq 36 "${#TENANT_ID}" "tenant_id (uuid) captured"

# ---------------------------------------------------------------------------
# Step 8: Get tenant detail
# ---------------------------------------------------------------------------
step "8. GET /tenant/:id"
DETAIL_TENANT=$(curl -sS "$BASE/system/tenant/$TENANT_ID" "${H[@]}")
echo "$DETAIL_TENANT" | python3 -m json.tool
DETAIL_COMPANY=$(echo "$DETAIL_TENANT" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['companyName'])")
ADMIN_USER=$(echo "$DETAIL_TENANT" | python3 -c "import sys,json; d=json.load(sys.stdin)['data']; print(d.get('adminUserName') or '')")
assert_eq "$TENANT_COMPANY" "$DETAIL_COMPANY" "tenant detail companyName matches"
assert_eq true "$([ -n "$ADMIN_USER" ] && echo true || echo false)" "adminUserName is not empty"

# ---------------------------------------------------------------------------
# Step 9: Update tenant
# ---------------------------------------------------------------------------
step "9. PUT update tenant"
# We need tenantId (business ID) for the update DTO
BUSINESS_TENANT_ID=$(echo "$DETAIL_TENANT" | python3 -c "import sys,json; print(json.load(sys.stdin)['data']['tenantId'])")
UPD_TENANT=$(curl -sS -X PUT "$BASE/system/tenant/" "${H[@]}" \
  -d "{\"id\":\"$TENANT_ID\",\"tenantId\":\"$BUSINESS_TENANT_ID\",\"contactPhone\":\"13800138000\"}")
UPD_TENANT_CODE=$(echo "$UPD_TENANT" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$UPD_TENANT_CODE" "update tenant returns code=200"

# ---------------------------------------------------------------------------
# Step 10: Delete package (should fail — package in use by tenant)
# ---------------------------------------------------------------------------
step "10. DELETE package (expect 4023 — TENANT_PACKAGE_IN_USE)"
DEL_PKG_RESP=$(curl -sS -X DELETE "$BASE/system/tenant-package/$PKG_ID" "${H[@]}")
DEL_PKG_CODE=$(echo "$DEL_PKG_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 4023 "$DEL_PKG_CODE" "delete package in-use returns 4023"

# ---------------------------------------------------------------------------
# Step 11: Delete tenant
# ---------------------------------------------------------------------------
step "11. DELETE tenant"
DEL_TENANT_RESP=$(curl -sS -X DELETE "$BASE/system/tenant/$TENANT_ID" "${H[@]}")
DEL_TENANT_CODE=$(echo "$DEL_TENANT_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_TENANT_CODE" "delete tenant returns code=200"

# ---------------------------------------------------------------------------
# Step 12: Verify tenant soft-deleted in DB
# ---------------------------------------------------------------------------
step "12. Verify tenant soft-deleted in DB"
DEL_FLAG=$(PGPASSWORD=123456 psql -h 127.0.0.1 -U saas_tea -d saas_tea -t -c \
  "SELECT del_flag FROM sys_tenant WHERE id='$TENANT_ID';" | tr -d ' \n')
assert_eq 1 "$DEL_FLAG" "tenant del_flag='1' in DB after delete"

# ---------------------------------------------------------------------------
# Step 13: Delete package (should succeed now — no tenant references it)
# ---------------------------------------------------------------------------
step "13. DELETE package (should succeed now)"
DEL_PKG2_RESP=$(curl -sS -X DELETE "$BASE/system/tenant-package/$PKG_ID" "${H[@]}")
DEL_PKG2_CODE=$(echo "$DEL_PKG2_RESP" | python3 -c "import sys,json; print(json.load(sys.stdin)['code'])")
assert_eq 200 "$DEL_PKG2_CODE" "delete package (freed) returns code=200"

echo ""
echo "ALL 13 STEPS PASSED"

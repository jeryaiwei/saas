#!/usr/bin/env bash
# server-rs/stress/scripts/stress-clean.sh
# 清理所有 stress-% 前缀数据。

set -euo pipefail
export PGPASSWORD=123456
PG_ARGS=(-h 127.0.0.1 -U saas_tea -d saas_tea)

echo "=== clean stress-% rows ==="
psql "${PG_ARGS[@]}" -c "
DELETE FROM sys_user_role   WHERE user_id IN (SELECT user_id FROM sys_user WHERE user_name LIKE 'stress-%');
DELETE FROM sys_user_tenant WHERE user_id IN (SELECT user_id FROM sys_user WHERE user_name LIKE 'stress-%');
DELETE FROM sys_user        WHERE user_name LIKE 'stress-%';
DELETE FROM sys_role        WHERE role_name LIKE 'stress-%';
DELETE FROM sys_notice      WHERE notice_title LIKE 'stress-%';
DELETE FROM sys_oper_log    WHERE title LIKE 'stress-%';
"
echo "done."

#!/usr/bin/env bash
# server-rs/stress/scripts/stress-seed.sh
# 批插 1 万 stress 用户 + 10 万 sys_oper_log 行。
# 用户与 admin 同 platform_id '000000'，不做多租户绑定（stress 场景 admin 登录无 tenant-id header）。
# 密码复用 admin 的 bcrypt hash（admin123），stress 用户登录无意义，仅作为 list 分页数据。
# 幂等：重跑前先 stress-clean.sh。

set -euo pipefail
cd "$(dirname "$0")/.."

export PGPASSWORD=123456
PG_ARGS=(-h 127.0.0.1 -U saas_tea -d saas_tea)

ADMIN_BCRYPT="$(psql "${PG_ARGS[@]}" -At -c \
  "SELECT password FROM sys_user WHERE user_name='admin' LIMIT 1;")"
if [[ -z "$ADMIN_BCRYPT" ]]; then
  echo "ERROR: cannot read admin password hash from sys_user"; exit 1
fi

echo "=== seed: 10,000 stress users (platform 000000, user_type='10') ==="
psql "${PG_ARGS[@]}" -c "
INSERT INTO sys_user (
  user_id, platform_id, user_name, nick_name, user_type,
  email, phonenumber, whatsapp, sex, avatar, password, status, del_flag,
  login_ip, create_by, create_at, update_by, update_at
)
SELECT
  gen_random_uuid(),
  '000000',
  'stress-u-' || lpad(i::text, 5, '0'),
  'stress-nick-' || lpad(i::text, 5, '0'),
  '10',
  '', '', '', '0', '', '${ADMIN_BCRYPT}', '0', '0',
  '', 'stress-seed', NOW(), 'stress-seed', NOW()
FROM generate_series(1, 10000) AS i
ON CONFLICT (user_name) DO NOTHING;"

echo "=== seed: 100,000 sys_oper_log rows ==="
psql "${PG_ARGS[@]}" -c "
INSERT INTO sys_oper_log (
  oper_id, tenant_id, title, business_type, request_method, operator_type,
  oper_name, dept_name, oper_url, oper_location, oper_param, json_result,
  error_msg, method, oper_ip, oper_time, status, cost_time
)
SELECT
  gen_random_uuid(),
  '000000',
  'stress-op-' || lpad(i::text, 6, '0'),
  (i % 10)::int,
  'GET',
  0,
  'stress-admin', '', '/stress/path/' || i, '127.0.0.1', '', '',
  '', 'stress.method', '127.0.0.1',
  NOW() - (random() * interval '30 days'),
  '0',
  (random() * 100)::int
FROM generate_series(1, 100000) AS i;"

echo ""
echo "=== done ==="
psql "${PG_ARGS[@]}" -c "
SELECT 'stress user' AS t, count(*) FROM sys_user WHERE user_name LIKE 'stress-%'
UNION ALL SELECT 'stress operlog', count(*) FROM sys_oper_log WHERE title LIKE 'stress-%';"

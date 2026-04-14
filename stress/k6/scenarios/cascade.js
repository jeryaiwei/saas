// server-rs/stress/k6/scenarios/cascade.js
// 创建 + 立即删除 → 触发 sys_user_role / sys_user_tenant 级联。
// 自产自删，不依赖 seed。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { userCreatePayload } from '../lib/data.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    cascade: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'cascade' },
    },
  },
  thresholds: buildThresholds(['cascade']),
};

export function defaultRequest() {
  const token = getToken();
  const createRes = http.post(
    `${BASE_URL}/system/user/`,
    JSON.stringify(userCreatePayload()),
    { headers: headersJson(token), tags: { scenario: 'cascade', op: 'create' } },
  );
  const userId = createRes.json('data.userId');
  if (!userId) return;
  const delRes = http.del(
    `${BASE_URL}/system/user/${userId}`,
    null,
    { headers: headersJson(token), tags: { scenario: 'cascade', op: 'delete' } },
  );
  check(delRes, { 'delete 200': (r) => r.status === 200 });
}

export default defaultRequest;

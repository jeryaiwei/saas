// server-rs/stress/k6/scenarios/read-detail.js
// 单记录读。VU init 时拉 ID 池，每次请求从池中采样。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { loadIdPool, sampleId } from '../lib/data.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    'read-detail': {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'read-detail' },
    },
  },
  thresholds: buildThresholds(['read-detail']),
};

const targets = [
  { path: '/system/user', pool: 'user', listPath: '/system/user/list', idField: 'userId' },
  { path: '/system/role', pool: 'role', listPath: '/system/role/list', idField: 'roleId' },
  { path: '/system/menu', pool: 'menu', listPath: '/system/menu/list', idField: 'menuId' },
  { path: '/system/dept', pool: 'dept', listPath: '/system/dept/list', idField: 'deptId' },
];

export function defaultRequest() {
  const token = getToken();
  const t = targets[Math.floor(Math.random() * targets.length)];
  const pool = loadIdPool(token, t.pool, t.listPath, t.idField);
  const id = sampleId(pool);
  const res = http.get(`${BASE_URL}${t.path}/${id}`, {
    headers: headersJson(token),
    tags: { scenario: 'read-detail' },
  });
  check(res, { 'detail 200': (r) => r.status === 200 });
}

export default defaultRequest;

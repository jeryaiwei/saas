// server-rs/stress/k6/scenarios/write.js
// 创建场景（删除走 cascade.js）。每次 iteration 三选一：user / role / notice。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { userCreatePayload, roleCreatePayload, noticeCreatePayload } from '../lib/data.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    write: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'write' },
    },
  },
  thresholds: buildThresholds(['write']),
};

const createOps = [
  { path: '/system/user/',    payload: () => userCreatePayload() },
  { path: '/system/role/',    payload: () => roleCreatePayload() },
  { path: '/message/notice/', payload: () => noticeCreatePayload() },
];

export function defaultRequest() {
  const token = getToken();
  const op = createOps[Math.floor(Math.random() * createOps.length)];
  const res = http.post(
    `${BASE_URL}${op.path}`,
    JSON.stringify(op.payload()),
    { headers: headersJson(token), tags: { scenario: 'write' } },
  );
  check(res, {
    'write 2xx': (r) => r.status >= 200 && r.status < 300,
    'code 200': (r) => r.json('code') === 200,
  });
}

export default defaultRequest;

// server-rs/stress/k6/scenarios/auth.js
// 单独压 POST /auth/login，测 JWT 签名 + bcrypt 开销。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, ADMIN_USER, ADMIN_PASS } from '../lib/config.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    auth: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'auth' },
    },
  },
  thresholds: buildThresholds(['auth']),
};

export function defaultRequest() {
  const res = http.post(
    `${BASE_URL}/auth/login`,
    JSON.stringify({ username: ADMIN_USER, password: ADMIN_PASS }),
    { headers: { 'Content-Type': 'application/json' }, tags: { scenario: 'auth' } },
  );
  check(res, {
    'login 200': (r) => r.status === 200,
    'has token': (r) => r.json('data.access_token') !== undefined,
  });
}

export default defaultRequest;

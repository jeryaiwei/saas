// server-rs/stress/k6/scenarios/baseline.js
// 压 GET /health/live — 无 DB、无业务、纯 axum 路由 + 中间件基线。
// 独立可跑：`k6 run k6/scenarios/baseline.js`
// mixed.js 也会 import defaultRequest 复用。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL } from '../lib/config.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    baseline: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'baseline' },
    },
  },
  thresholds: buildThresholds(['baseline']),
};

export function defaultRequest() {
  // /health/live 不在 /api/v1 前缀下
  const healthUrl = BASE_URL.replace(/\/api\/v1\/?$/, '') + '/health/live';
  const res = http.get(healthUrl, { tags: { scenario: 'baseline' } });
  check(res, { 'health 200': (r) => r.status === 200 });
}

export default defaultRequest;

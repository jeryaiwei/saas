// server-rs/stress/k6/scenarios/file.js
// 100KB multipart upload。
// 路径：POST /common/upload，multipart field name "file"。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    file: {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'file' },
    },
  },
  thresholds: buildThresholds(['file']),
};

const payload = 'x'.repeat(100 * 1024);

export function defaultRequest() {
  const token = getToken();
  const res = http.post(
    `${BASE_URL}/common/upload`,
    { file: http.file(payload, 'stress.txt', 'text/plain') },
    { headers: { 'Authorization': `Bearer ${token}` }, tags: { scenario: 'file' } },
  );
  check(res, { 'upload 200': (r) => r.status === 200 });
}

export default defaultRequest;

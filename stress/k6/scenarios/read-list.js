// server-rs/stress/k6/scenarios/read-list.js
// 典型分页读 — 6 个最高频端点轮询。

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    'read-list': {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'read-list' },
    },
  },
  thresholds: buildThresholds(['read-list']),
};

const endpoints = [
  '/system/user/list?pageNum=1&pageSize=10',
  '/system/role/list?pageNum=1&pageSize=10',
  '/system/menu/list',
  '/system/dept/list',
  '/system/dict/data/list?pageNum=1&pageSize=10',
  '/message/notice/list?pageNum=1&pageSize=10',
];

export function defaultRequest() {
  const token = getToken();
  const url = BASE_URL + endpoints[Math.floor(Math.random() * endpoints.length)];
  const res = http.get(url, { headers: headersJson(token), tags: { scenario: 'read-list' } });
  check(res, {
    'list 200': (r) => r.status === 200,
    'code 200': (r) => r.json('code') === 200,
  });
}

export default defaultRequest;

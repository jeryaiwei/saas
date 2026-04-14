// server-rs/stress/k6/scenarios/complex-read.js
// 多 join / 慢查询候选。
// 路径说明：
//   /monitor/operlog/list — 操作日志（seed 10 万行后测复杂分页）
//   /monitor/online/list  — 在线用户（内存数据）
//   /monitor/server       — 服务器信息（CPU/mem 聚合）

import http from 'k6/http';
import { check } from 'k6';
import { BASE_URL, headersJson } from '../lib/config.js';
import { getToken } from '../lib/auth.js';
import { buildThresholds } from '../thresholds.js';

export const options = {
  scenarios: {
    'complex-read': {
      executor: 'constant-vus',
      vus: 1,
      duration: '60s',
      tags: { scenario: 'complex-read' },
    },
  },
  thresholds: buildThresholds(['complex-read']),
};

const endpoints = [
  '/monitor/operlog/list?pageNum=1&pageSize=20',
  '/monitor/online/list?pageNum=1&pageSize=20',
  '/monitor/server',
];

export function defaultRequest() {
  const token = getToken();
  const url = BASE_URL + endpoints[Math.floor(Math.random() * endpoints.length)];
  const res = http.get(url, { headers: headersJson(token), tags: { scenario: 'complex-read' } });
  check(res, { 'ok': (r) => r.status === 200 });
}

export default defaultRequest;

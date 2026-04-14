// server-rs/stress/k6/thresholds.js
// 单一事实源：SLO 阈值在这里定义，所有 scenario 引用。
// 规则：scenario-specific 阈值用 tag 'scenario' 过滤。
// 全局阈值通用。

export const globalThresholds = {
  'http_req_failed': ['rate<0.01'],                          // 全局错误率 < 1%
};

export const perScenarioThresholds = {
  baseline:     { p95: 20,   p99: 50,   failRate: 0.001 },
  auth:         { p95: 300,  p99: 600,  failRate: 0.005 },
  'read-list':  { p95: 200,  p99: 500,  failRate: 0.01 },
  'read-detail':{ p95: 100,  p99: 300,  failRate: 0.01 },
  'complex-read': { p95: 500, p99: 1000, failRate: 0.01 },
  write:        { p95: 500,  p99: 1000, failRate: 0.01 },
  file:         { p95: 1000, p99: 2000, failRate: 0.02 },
  cascade:      { p95: 800,  p99: 2000, failRate: 0.01 },
};

// 生成 k6 thresholds 对象（tag 过滤形式）
export function buildThresholds(scenarios) {
  const out = { ...globalThresholds };
  for (const s of scenarios) {
    const t = perScenarioThresholds[s];
    if (!t) continue;
    out[`http_req_duration{scenario:${s}}`] = [`p(95)<${t.p95}`, `p(99)<${t.p99}`];
    out[`http_req_failed{scenario:${s}}`]   = [`rate<${t.failRate}`];
  }
  return out;
}

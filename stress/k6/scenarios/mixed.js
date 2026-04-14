// server-rs/stress/k6/scenarios/mixed.js
// 按 spec §3 权重混合所有场景。load / stress / soak 都用这个。
// 每个子 scenario 独立 executor，rate = TARGET_RPS * weight。

import { TARGET_RPS } from '../lib/config.js';
import { buildThresholds } from '../thresholds.js';

import { defaultRequest as baselineReq } from './baseline.js';
import { defaultRequest as authReq } from './auth.js';
import { defaultRequest as readListReq } from './read-list.js';
import { defaultRequest as readDetailReq } from './read-detail.js';
import { defaultRequest as complexReadReq } from './complex-read.js';
import { defaultRequest as writeReq } from './write.js';
import { defaultRequest as fileReq } from './file.js';
import { defaultRequest as cascadeReq } from './cascade.js';

const WEIGHTS = {
  baseline: 0.05,
  auth: 0.02,
  'read-list': 0.40,
  'read-detail': 0.20,
  'complex-read': 0.10,
  write: 0.15,
  file: 0.05,
  cascade: 0.03,
};

function scenarioFor(name, exec) {
  const weight = WEIGHTS[name];
  if (__ENV.K6_RAMP) {
    // stress pattern: ramping-arrival-rate 50→2000 RPS
    const stages = [
      { target: 50, duration: '90s' },
      { target: 100, duration: '90s' },
      { target: 250, duration: '90s' },
      { target: 500, duration: '90s' },
      { target: 1000, duration: '90s' },
      { target: 2000, duration: '90s' },
    ].map((s) => ({ target: Math.max(1, Math.round(s.target * weight)), duration: s.duration }));
    return {
      executor: 'ramping-arrival-rate',
      startRate: 1,
      timeUnit: '1s',
      stages,
      preAllocatedVUs: 50,
      maxVUs: 2000,
      exec,
      tags: { scenario: name },
    };
  }
  const rate = Math.max(1, Math.round(TARGET_RPS * weight));
  return {
    executor: 'constant-arrival-rate',
    rate,
    timeUnit: '1s',
    duration: '5m',
    preAllocatedVUs: Math.max(10, rate),
    maxVUs: Math.max(50, rate * 4),
    exec,
    tags: { scenario: name },
  };
}

export const options = {
  scenarios: {
    baseline:       scenarioFor('baseline',     'baseline'),
    auth:           scenarioFor('auth',         'auth'),
    'read-list':    scenarioFor('read-list',    'readList'),
    'read-detail':  scenarioFor('read-detail',  'readDetail'),
    'complex-read': scenarioFor('complex-read', 'complexRead'),
    write:          scenarioFor('write',        'write'),
    file:           scenarioFor('file',         'file'),
    cascade:        scenarioFor('cascade',      'cascade'),
  },
  thresholds: buildThresholds(Object.keys(WEIGHTS)),
};

export const baseline = baselineReq;
export const auth = authReq;
export const readList = readListReq;
export const readDetail = readDetailReq;
export const complexRead = complexReadReq;
export const write = writeReq;
export const file = fileReq;
export const cascade = cascadeReq;

// server-rs/stress/k6/lib/auth.js
// 提供 login() 与 VU 级 token 缓存。
// 每个 VU 只在 setup 或首次调用时登录一次，避免把登录本身当成压测流量。

import http from 'k6/http';
import { check, fail } from 'k6';
import { BASE_URL, ADMIN_USER, ADMIN_PASS } from './config.js';

// VU 级缓存（每个虚拟用户独立 state）
let cachedToken = null;

export function login(username = ADMIN_USER, password = ADMIN_PASS) {
  const res = http.post(
    `${BASE_URL}/auth/login`,
    JSON.stringify({ username, password }),
    { headers: { 'Content-Type': 'application/json' }, tags: { scenario: '_auth_setup' } },
  );
  const ok = check(res, {
    'login 200': (r) => r.status === 200,
    'has access_token': (r) => r.json('data.access_token') !== undefined,
  });
  if (!ok) {
    fail(`login failed: status=${res.status} body=${res.body}`);
  }
  return res.json('data.access_token');
}

export function getToken() {
  if (!cachedToken) {
    cachedToken = login();
  }
  return cachedToken;
}

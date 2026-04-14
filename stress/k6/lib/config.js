// server-rs/stress/k6/lib/config.js
// 所有 env-driven 的运行时参数集中在这里。
// 新增 scenario 禁止直接读 __ENV，必须通过本文件。

export const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:18080/api/v1';
export const ADMIN_USER = __ENV.ADMIN_USER || 'admin';
export const ADMIN_PASS = __ENV.ADMIN_PASS || 'admin123';
export const TARGET_RPS = parseInt(__ENV.TARGET_RPS || '500', 10);

export const headersJson = (token) => ({
  'Content-Type': 'application/json',
  'Authorization': `Bearer ${token}`,
});

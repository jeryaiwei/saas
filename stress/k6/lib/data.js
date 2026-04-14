// server-rs/stress/k6/lib/data.js
// ID 池 + payload 生成器。

import http from 'k6/http';
import { BASE_URL, headersJson } from './config.js';

const idPools = {};

export function loadIdPool(token, key, listPath, idField) {
  if (idPools[key]) return idPools[key];
  const sep = listPath.includes('?') ? '&' : '?';
  const res = http.get(`${BASE_URL}${listPath}${sep}pageNum=1&pageSize=100`, {
    headers: headersJson(token),
    tags: { scenario: '_setup' },
  });
  const data = res.json('data');
  const rows = Array.isArray(data) ? data : (data && data.rows) || [];
  idPools[key] = rows.map((r) => r[idField]).filter((v) => v !== undefined);
  if (idPools[key].length === 0) {
    throw new Error(`empty id pool for ${key}: seed data missing?`);
  }
  return idPools[key];
}

export function sampleId(pool) {
  return pool[Math.floor(Math.random() * pool.length)];
}

export function stressName(prefix) {
  return `stress-${prefix}-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

// POST /system/user/ — CreateUserDto: userName, nickName, password, [email, phonenumber, sex, avatar, status, deptId, roleIds]
export function userCreatePayload(roleIds = []) {
  const name = stressName('u').slice(0, 30);
  return {
    userName: name,
    nickName: name.slice(0, 28) + '-n',
    password: 'stress123',
    roleIds,
  };
}

// POST /system/role/ — CreateRoleDto: roleName, roleKey, roleSort, [status, remark, menuIds]
export function roleCreatePayload() {
  const name = stressName('r').slice(0, 30);
  return {
    roleName: name,
    roleKey: name,
    roleSort: 99,
    status: '0',
    menuIds: [],
  };
}

// POST /message/notice/ — CreateNoticeDto: noticeTitle, noticeType, [noticeContent, status]
export function noticeCreatePayload() {
  return {
    noticeTitle: stressName('notice').slice(0, 50),
    noticeType: '1',
    noticeContent: 'stress test content',
    status: '0',
  };
}

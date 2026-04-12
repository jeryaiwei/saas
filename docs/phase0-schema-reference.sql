-- =============================================================================
-- Phase 0 Schema Reference (READ-ONLY)
-- =============================================================================
--
-- These 6 tables are everything the Rust server needs for the Phase 0 login →
-- /info flow. The authoritative schema during the progressive-migration window
-- is `server/prisma/schema.prisma`; this file exists so a reader can quickly
-- understand what the Rust code selects without bouncing between repos.
--
-- DO NOT RUN THIS FILE. It is not executed by `sqlx migrate`. The real schema
-- is owned by NestJS Prisma. If a column in these tables changes, update both
-- `server/prisma/schema.prisma` and this reference in the same PR.
--
-- Phase 2 will flip ownership: Rust's `migrations/` becomes authoritative and
-- a CI rule will enforce schema.prisma <-> migrations/ consistency.
--
-- Source: server/prisma/schema.prisma (models SysTenant, SysMenu, SysRole,
-- SysRoleMenu, SysUser, SysUserTenant). Indexes and foreign keys are omitted
-- for brevity — see the Prisma file for the complete definition.
-- =============================================================================

-- -----------------------------------------------------------------------------
-- sys_tenant — tenant definition
-- -----------------------------------------------------------------------------
CREATE TABLE sys_tenant (
  id                 VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id          VARCHAR(20)  UNIQUE NOT NULL,
  parent_id          VARCHAR(20),                          -- null for super tenant
  contact_user_name  VARCHAR(50),
  contact_phone      VARCHAR(20),
  company_name       VARCHAR(100) NOT NULL,
  license_number     VARCHAR(50),
  address            VARCHAR(200),
  intro              TEXT,
  domain             VARCHAR(100),
  package_id         VARCHAR(36),
  expire_time        TIMESTAMPTZ(6),
  account_count      INTEGER      NOT NULL DEFAULT -1,
  storage_quota      INTEGER      NOT NULL DEFAULT 10240,
  storage_used       INTEGER      NOT NULL DEFAULT 0,
  api_quota          INTEGER      NOT NULL DEFAULT 10000,
  language           VARCHAR(10)  NOT NULL DEFAULT 'zh-CN',
  verify_status      VARCHAR(20),                          -- null | PENDING_VERIFY | VERIFIED | REJECTED
  license_image_url  VARCHAR(500),
  reject_reason      VARCHAR(500),
  verified_at        TIMESTAMPTZ(6),
  status             CHAR(1)      NOT NULL DEFAULT '0',
  del_flag           CHAR(1)      NOT NULL DEFAULT '0',
  create_by          VARCHAR(64)  NOT NULL DEFAULT '',
  create_at          TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by          VARCHAR(64)  NOT NULL DEFAULT '',
  update_at          TIMESTAMPTZ(6) NOT NULL,
  remark             VARCHAR(500)
);

-- -----------------------------------------------------------------------------
-- sys_user — backend + C-end users
-- -----------------------------------------------------------------------------
CREATE TABLE sys_user (
  user_id      VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  platform_id  VARCHAR(20)  NOT NULL DEFAULT '000000',     -- owning platform tenant
  dept_id      VARCHAR(36),
  user_name    VARCHAR(50)  UNIQUE NOT NULL,
  nick_name    VARCHAR(30)  NOT NULL,
  user_type    VARCHAR(2)   NOT NULL,                      -- '10' = CUSTOM, '20' = CLIENT
  client_type  VARCHAR(20),                                -- INDIVIDUAL | ENTERPRISE (C-end only)
  lang         VARCHAR(10)  DEFAULT 'zh-CN',
  email        VARCHAR(50)  NOT NULL DEFAULT '',
  phonenumber  VARCHAR(11)  NOT NULL DEFAULT '',
  whatsapp     VARCHAR(30)  NOT NULL DEFAULT '',
  sex          CHAR(1)      NOT NULL DEFAULT '0',
  avatar       VARCHAR(255) NOT NULL DEFAULT '',
  password     VARCHAR(200) NOT NULL,                      -- bcrypt $2b$...
  status       CHAR(1)      NOT NULL DEFAULT '0',
  del_flag     CHAR(1)      NOT NULL DEFAULT '0',
  login_ip     VARCHAR(128) NOT NULL DEFAULT '',
  login_date   TIMESTAMPTZ(6),
  create_by    VARCHAR(64)  NOT NULL DEFAULT '',
  create_at    TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by    VARCHAR(64)  NOT NULL DEFAULT '',
  update_at    TIMESTAMPTZ(6) NOT NULL,
  remark       VARCHAR(500)
);

-- Partial unique index on non-empty email is created at seed time:
--   CREATE UNIQUE INDEX sys_user_email_unique ON sys_user(email) WHERE email <> '';

-- -----------------------------------------------------------------------------
-- sys_user_tenant — many-to-many user ↔ tenant with per-binding admin flag
-- -----------------------------------------------------------------------------
CREATE TABLE sys_user_tenant (
  id          VARCHAR(36) PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id     VARCHAR(36) NOT NULL,
  tenant_id   VARCHAR(20) NOT NULL,
  is_default  CHAR(1)     NOT NULL DEFAULT '0',            -- '1' = default binding
  is_admin    CHAR(1)     NOT NULL DEFAULT '0',            -- '1' = admin in this tenant
  status      CHAR(1)     NOT NULL DEFAULT '0',
  create_by   VARCHAR(64) NOT NULL DEFAULT '',
  create_at   TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by   VARCHAR(64) NOT NULL DEFAULT '',
  update_at   TIMESTAMPTZ(6) NOT NULL,
  UNIQUE (user_id, tenant_id)
);

-- -----------------------------------------------------------------------------
-- sys_role — tenant-scoped roles
-- -----------------------------------------------------------------------------
CREATE TABLE sys_role (
  role_id              VARCHAR(36) PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id            VARCHAR(20) NOT NULL DEFAULT '000000',
  role_name            VARCHAR(30) NOT NULL,
  role_key             VARCHAR(100) NOT NULL,
  role_sort            INTEGER     NOT NULL,
  data_scope           CHAR(1)     NOT NULL DEFAULT '1',
  menu_check_strictly  BOOLEAN     NOT NULL DEFAULT false,
  dept_check_strictly  BOOLEAN     NOT NULL DEFAULT false,
  status               CHAR(1)     NOT NULL DEFAULT '0',
  del_flag             CHAR(1)     NOT NULL DEFAULT '0',
  create_by            VARCHAR(64) NOT NULL DEFAULT '',
  create_at            TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by            VARCHAR(64) NOT NULL DEFAULT '',
  update_at            TIMESTAMPTZ(6) NOT NULL,
  remark               VARCHAR(500),
  i18n                 JSONB
);

-- -----------------------------------------------------------------------------
-- sys_role_menu — join table (role → menu permissions)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_role_menu (
  role_id VARCHAR(36) NOT NULL,
  menu_id VARCHAR(36) NOT NULL,
  PRIMARY KEY (role_id, menu_id)
);

-- -----------------------------------------------------------------------------
-- sys_menu — global menu tree + permission strings (shared across tenants,
-- scoped per-tenant by SysTenantPackage.menuIds)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_menu (
  menu_id   VARCHAR(36) PRIMARY KEY DEFAULT gen_random_uuid(),
  menu_name VARCHAR(50) NOT NULL,
  parent_id VARCHAR(36),
  order_num INTEGER NOT NULL,
  path      VARCHAR(200) NOT NULL DEFAULT '',
  component VARCHAR(255),
  query     VARCHAR(255) NOT NULL DEFAULT '',
  is_frame  CHAR(1) NOT NULL,
  is_cache  CHAR(1) NOT NULL,
  menu_type CHAR(1) NOT NULL,
  visible   CHAR(1) NOT NULL,
  status    CHAR(1) NOT NULL,
  perms     VARCHAR(100) NOT NULL DEFAULT '',              -- e.g. "system:user:list"
  icon      VARCHAR(100) NOT NULL DEFAULT '',
  create_by VARCHAR(64)  NOT NULL DEFAULT '',
  create_at TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by VARCHAR(64)  NOT NULL DEFAULT '',
  update_at TIMESTAMPTZ(6) NOT NULL,
  remark    VARCHAR(500),
  del_flag  CHAR(1) NOT NULL DEFAULT '0',
  i18n      JSONB
);

-- -----------------------------------------------------------------------------
-- sys_user_role — indirectly used by the Rust permission-resolve join.
-- Not written by Phase 0 code but SELECTed in `UserRepo::resolve_permissions`.
-- -----------------------------------------------------------------------------
CREATE TABLE sys_user_role (
  user_id VARCHAR(36) NOT NULL,
  role_id VARCHAR(36) NOT NULL,
  PRIMARY KEY (user_id, role_id)
);

-- =============================================================================
-- Permission resolve query (Rust UserRepo::resolve_permissions)
-- =============================================================================
--
-- SELECT DISTINCT m.perms
--   FROM sys_menu m
--   JOIN sys_role_menu rm ON rm.menu_id = m.menu_id
--   JOIN sys_user_role ur ON ur.role_id = rm.role_id
--   JOIN sys_role r       ON r.role_id = ur.role_id
--  WHERE ur.user_id  = $1
--    AND r.tenant_id = $2
--    AND r.status    = '0' AND r.del_flag = '0'
--    AND m.status    = '0' AND m.del_flag = '0'
--    AND m.perms <> '';
--
-- Note: this Phase 0 query does NOT apply tenant-package menu filtering.
-- NestJS additionally intersects with `SysTenantPackage.menuIds` for the
-- current tenant. Phase 2 of the Rust port will implement the full filter.

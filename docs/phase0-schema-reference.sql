-- =============================================================================
-- Schema Reference (READ-ONLY)
-- =============================================================================
--
-- All tables the Rust server reads or writes. The authoritative schema during
-- the progressive-migration window is `server/prisma/schema.prisma`; this
-- file exists so a reader can quickly understand what the Rust code touches
-- without bouncing between repos.
--
-- DO NOT RUN THIS FILE. It is not executed by `sqlx migrate`. The real schema
-- is owned by NestJS Prisma. If a column in these tables changes, update both
-- `server/prisma/schema.prisma` and this reference in the same PR.
--
-- Phase 2 will flip ownership: Rust's `migrations/` becomes authoritative and
-- a CI rule will enforce schema.prisma <-> migrations/ consistency.
--
-- Index definitions are taken from the live database (pg_indexes).
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
CREATE INDEX sys_tenant_tenant_id_idx     ON sys_tenant (tenant_id);
CREATE INDEX sys_tenant_parent_id_idx     ON sys_tenant (parent_id);
CREATE INDEX sys_tenant_create_at_idx     ON sys_tenant (create_at);
CREATE INDEX sys_tenant_verify_status_idx ON sys_tenant (verify_status);

-- -----------------------------------------------------------------------------
-- sys_tenant_package — tenant subscription packages (NOT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_tenant_package (
  package_id          VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  code                VARCHAR(20)  UNIQUE NOT NULL,
  package_name        VARCHAR(50)  NOT NULL,
  menu_ids            VARCHAR(36)[] NOT NULL DEFAULT '{}',
  menu_check_strictly BOOLEAN      NOT NULL DEFAULT false,
  status              CHAR(1)      NOT NULL DEFAULT '0',
  del_flag            CHAR(1)      NOT NULL DEFAULT '0',
  create_by           VARCHAR(64)  NOT NULL DEFAULT '',
  create_at           TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by           VARCHAR(64)  NOT NULL DEFAULT '',
  update_at           TIMESTAMPTZ(6) NOT NULL,
  remark              VARCHAR(500)
);
-- PK + UNIQUE(code) only

-- -----------------------------------------------------------------------------
-- sys_user — backend + C-end users
-- -----------------------------------------------------------------------------
CREATE TABLE sys_user (
  user_id      VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  platform_id  VARCHAR(20)  NOT NULL DEFAULT '000000',
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
CREATE INDEX sys_user_platform_id_idx     ON sys_user (platform_id);
CREATE INDEX sys_user_dept_id_idx         ON sys_user (dept_id);
CREATE INDEX sys_user_user_name_idx       ON sys_user (user_name);
CREATE INDEX sys_user_email_idx           ON sys_user (email);
CREATE INDEX sys_user_phonenumber_idx     ON sys_user (phonenumber);
CREATE INDEX sys_user_status_idx          ON sys_user (status);
CREATE INDEX sys_user_del_flag_status_idx ON sys_user (del_flag, status);
-- Partial unique index (created at seed time, not by Prisma):
CREATE UNIQUE INDEX sys_user_email_unique ON sys_user (email)
  WHERE email <> '' AND del_flag = '0';

-- -----------------------------------------------------------------------------
-- sys_user_tenant — many-to-many user ↔ tenant with per-binding admin flag
-- -----------------------------------------------------------------------------
CREATE TABLE sys_user_tenant (
  id          VARCHAR(36) PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id     VARCHAR(36) NOT NULL,
  tenant_id   VARCHAR(20) NOT NULL,
  is_default  CHAR(1)     NOT NULL DEFAULT '0',
  is_admin    CHAR(1)     NOT NULL DEFAULT '0',
  status      CHAR(1)     NOT NULL DEFAULT '0',
  create_by   VARCHAR(64) NOT NULL DEFAULT '',
  create_at   TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by   VARCHAR(64) NOT NULL DEFAULT '',
  update_at   TIMESTAMPTZ(6) NOT NULL,
  UNIQUE (user_id, tenant_id)
);
CREATE INDEX sys_user_tenant_user_id_idx            ON sys_user_tenant (user_id);
CREATE INDEX sys_user_tenant_tenant_id_idx          ON sys_user_tenant (tenant_id);
CREATE INDEX sys_user_tenant_user_id_is_default_idx ON sys_user_tenant (user_id, is_default);

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
CREATE INDEX sys_role_role_key_idx                 ON sys_role (role_key);
CREATE INDEX sys_role_tenant_id_status_idx         ON sys_role (tenant_id, status);
CREATE INDEX sys_role_tenant_id_role_key_idx       ON sys_role (tenant_id, role_key);
CREATE INDEX sys_role_tenant_id_del_flag_status_idx ON sys_role (tenant_id, del_flag, status);
CREATE INDEX idx_sys_role_i18n                     ON sys_role USING gin (i18n);

-- -----------------------------------------------------------------------------
-- sys_role_menu — join table (role → menu permissions)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_role_menu (
  role_id VARCHAR(36) NOT NULL,
  menu_id VARCHAR(36) NOT NULL,
  PRIMARY KEY (role_id, menu_id)
);
CREATE INDEX sys_role_menu_role_id_idx ON sys_role_menu (role_id);
CREATE INDEX sys_role_menu_menu_id_idx ON sys_role_menu (menu_id);

-- -----------------------------------------------------------------------------
-- sys_user_role — user → role binding (SELECTed in permission resolve)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_user_role (
  user_id VARCHAR(36) NOT NULL,
  role_id VARCHAR(36) NOT NULL,
  PRIMARY KEY (user_id, role_id)
);
CREATE INDEX sys_user_role_role_id_idx ON sys_user_role (role_id);

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
CREATE INDEX sys_menu_status_idx              ON sys_menu (status);
CREATE INDEX sys_menu_del_flag_status_idx     ON sys_menu (del_flag, status);
CREATE INDEX sys_menu_parent_id_order_num_idx ON sys_menu (parent_id, order_num);
CREATE INDEX idx_sys_menu_i18n               ON sys_menu USING gin (i18n);

-- -----------------------------------------------------------------------------
-- sys_dept — department tree (STRICT tenant model)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_dept (
  dept_id    VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id  VARCHAR(20)  NOT NULL DEFAULT '000000',
  parent_id  VARCHAR(36),
  ancestors  VARCHAR(36)[] NOT NULL DEFAULT '{}',
  dept_name  VARCHAR(30)  NOT NULL,
  order_num  INTEGER      NOT NULL,
  leader     VARCHAR(20)  NOT NULL DEFAULT '',
  phone      VARCHAR(11)  NOT NULL DEFAULT '',
  email      VARCHAR(50)  NOT NULL DEFAULT '',
  status     CHAR(1)      NOT NULL DEFAULT '0',
  del_flag   CHAR(1)      NOT NULL DEFAULT '0',
  create_by  VARCHAR(64)  NOT NULL DEFAULT '',
  create_at  TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by  VARCHAR(64)  NOT NULL DEFAULT '',
  update_at  TIMESTAMPTZ(6) NOT NULL,
  remark     VARCHAR(500),
  i18n       JSONB
);
CREATE INDEX sys_dept_status_idx                    ON sys_dept (status);
CREATE INDEX sys_dept_parent_id_idx                 ON sys_dept (parent_id);
CREATE INDEX sys_dept_tenant_id_status_idx          ON sys_dept (tenant_id, status);
CREATE INDEX sys_dept_tenant_id_parent_id_idx       ON sys_dept (tenant_id, parent_id);
CREATE INDEX sys_dept_tenant_id_del_flag_status_idx ON sys_dept (tenant_id, del_flag, status);
CREATE INDEX idx_sys_dept_i18n                      ON sys_dept USING gin (i18n);

-- -----------------------------------------------------------------------------
-- sys_post — job/position management (STRICT tenant model)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_post (
  post_id       VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id     VARCHAR(20)  NOT NULL DEFAULT '000000',
  dept_id       VARCHAR(36),
  post_code     VARCHAR(64)  NOT NULL,
  post_category VARCHAR(100),
  post_name     VARCHAR(50)  NOT NULL,
  post_sort     INTEGER      NOT NULL DEFAULT 0,
  status        CHAR(1)      NOT NULL DEFAULT '0',
  create_by     VARCHAR(64)  NOT NULL DEFAULT '',
  create_at     TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by     VARCHAR(64)  NOT NULL DEFAULT '',
  update_at     TIMESTAMPTZ(6) NOT NULL,
  remark        VARCHAR(500),
  del_flag      CHAR(1)      NOT NULL DEFAULT '0',
  i18n          JSONB
);
CREATE INDEX sys_post_dept_id_idx                   ON sys_post (dept_id);
CREATE INDEX sys_post_tenant_id_status_idx          ON sys_post (tenant_id, status);
CREATE INDEX sys_post_tenant_id_del_flag_status_idx ON sys_post (tenant_id, del_flag, status);
CREATE INDEX idx_sys_post_i18n                      ON sys_post USING gin (i18n);

-- -----------------------------------------------------------------------------
-- sys_config — system configuration key-value pairs (PLATFORM tenant model)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_config (
  config_id     VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id     VARCHAR(20)  NOT NULL DEFAULT '000000',
  config_name   VARCHAR(100) NOT NULL,
  config_key    VARCHAR(100) NOT NULL,
  config_value  TEXT         NOT NULL,
  config_type   CHAR(1)      NOT NULL,                     -- 'Y' = system, 'N' = user
  create_by     VARCHAR(64)  NOT NULL DEFAULT '',
  create_at     TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by     VARCHAR(64)  NOT NULL DEFAULT '',
  update_at     TIMESTAMPTZ(6) NOT NULL,
  remark        VARCHAR(500),
  status        CHAR(1)      NOT NULL DEFAULT '0',
  del_flag      CHAR(1)      NOT NULL DEFAULT '0',
  UNIQUE (tenant_id, config_key)
);
CREATE INDEX sys_config_config_key_idx              ON sys_config (config_key);
CREATE INDEX sys_config_create_at_idx               ON sys_config (create_at);
CREATE INDEX sys_config_tenant_id_status_idx        ON sys_config (tenant_id, status);
CREATE INDEX sys_config_tenant_id_config_type_idx   ON sys_config (tenant_id, config_type);
CREATE INDEX sys_config_tenant_id_del_flag_status_idx ON sys_config (tenant_id, del_flag, status);

-- -----------------------------------------------------------------------------
-- sys_dict_type — dictionary type definitions (PLATFORM tenant model)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_dict_type (
  dict_id       VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id     VARCHAR(20)  NOT NULL DEFAULT '000000',
  dict_name     VARCHAR(100) NOT NULL,
  dict_type     VARCHAR(100) NOT NULL,
  status        CHAR(1)      NOT NULL DEFAULT '0',
  create_by     VARCHAR(64)  NOT NULL DEFAULT '',
  create_at     TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by     VARCHAR(64)  NOT NULL DEFAULT '',
  update_at     TIMESTAMPTZ(6) NOT NULL,
  remark        VARCHAR(500),
  del_flag      CHAR(1)      NOT NULL DEFAULT '0',
  i18n          JSONB,
  UNIQUE (tenant_id, dict_type)
);
CREATE INDEX sys_dict_type_dict_type_idx        ON sys_dict_type (dict_type);
CREATE INDEX sys_dict_type_tenant_id_status_idx ON sys_dict_type (tenant_id, status);
CREATE INDEX idx_sys_dict_type_i18n             ON sys_dict_type USING gin (i18n);

-- -----------------------------------------------------------------------------
-- sys_dict_data — dictionary data entries (PLATFORM tenant model)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_dict_data (
  dict_code     VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id     VARCHAR(20)  NOT NULL DEFAULT '000000',
  dict_sort     INTEGER      NOT NULL DEFAULT 0,
  dict_label    VARCHAR(100) NOT NULL,
  dict_value    VARCHAR(100) NOT NULL,
  dict_type     VARCHAR(100) NOT NULL,
  css_class     VARCHAR(100) NOT NULL DEFAULT '',
  list_class    VARCHAR(100) NOT NULL DEFAULT '',
  is_default    CHAR(1)      NOT NULL DEFAULT 'N',
  status        CHAR(1)      NOT NULL DEFAULT '0',
  create_by     VARCHAR(64)  NOT NULL DEFAULT '',
  create_at     TIMESTAMPTZ(6) NOT NULL DEFAULT now(),
  update_by     VARCHAR(64)  NOT NULL DEFAULT '',
  update_at     TIMESTAMPTZ(6) NOT NULL,
  remark        VARCHAR(500),
  del_flag      CHAR(1)      NOT NULL DEFAULT '0',
  i18n          JSONB,
  UNIQUE (tenant_id, dict_type, dict_value)
);
CREATE INDEX sys_dict_data_dict_type_idx                    ON sys_dict_data (dict_type);
CREATE INDEX sys_dict_data_tenant_id_dict_type_status_idx   ON sys_dict_data (tenant_id, dict_type, status);
CREATE INDEX idx_sys_dict_data_i18n                         ON sys_dict_data USING gin (i18n);

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

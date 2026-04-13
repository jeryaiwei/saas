-- =============================================================================
-- Schema Reference (READ-ONLY)
-- =============================================================================
--
-- All 29 tables the Rust server reads or writes. The authoritative schema during
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

-- =============================================================================
-- Tables added since Phase 0 (message, monitor, file, audit)
-- =============================================================================

-- -----------------------------------------------------------------------------
-- sys_notice — system announcements (STRICT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_notice (
  notice_id       VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id       VARCHAR(20)  NOT NULL,       -- 所属租户
  notice_title    VARCHAR(50)  NOT NULL,       -- 公告标题
  notice_type     CHAR(1)      NOT NULL,       -- 类型: '1' 通知, '2' 公告
  notice_content  TEXT,                        -- 内容 (富文本)
  status          CHAR(1)      NOT NULL DEFAULT '0', -- '0' 正常, '1' 关闭
  create_by       VARCHAR(64)  NOT NULL,       -- 创建者
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by       VARCHAR(64)  NOT NULL,       -- 更新者
  update_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  del_flag        CHAR(1)      NOT NULL DEFAULT '0', -- '0' 正常, '1' 删除
  remark          VARCHAR(500)                 -- 备注
);
CREATE INDEX sys_notice_create_at_idx ON sys_notice (create_at);
CREATE INDEX sys_notice_tenant_id_create_at_idx ON sys_notice (tenant_id, create_at);
CREATE INDEX sys_notice_tenant_id_del_flag_status_idx ON sys_notice (tenant_id, del_flag, status);
CREATE INDEX sys_notice_tenant_id_notice_type_idx ON sys_notice (tenant_id, notice_type);
CREATE INDEX sys_notice_tenant_id_status_idx ON sys_notice (tenant_id, status);

-- -----------------------------------------------------------------------------
-- sys_notify_template — in-app notification templates (NOT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_notify_template (
  id              SERIAL       PRIMARY KEY,
  name            VARCHAR(100) NOT NULL,       -- 模板名称
  code            VARCHAR(100) NOT NULL UNIQUE, -- 模板编码 (唯一)
  nickname        VARCHAR(100) NOT NULL,       -- 发送人名称
  content         TEXT         NOT NULL,       -- 模板内容, 支持 ${key} 变量
  params          TEXT,                        -- 参数列表 (JSON), 如 ["userName","code"]
  type            INT          NOT NULL,       -- 类型: 1 系统通知, 2 业务通知
  status          CHAR(1)      NOT NULL DEFAULT '0', -- '0' 启用, '1' 停用
  remark          VARCHAR(500),
  create_by       VARCHAR(64)  NOT NULL,
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by       VARCHAR(64)  NOT NULL,
  update_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  del_flag        CHAR(1)      NOT NULL DEFAULT '0',
  i18n            JSONB                        -- 国际化内容 (按 lang_code 分组)
);
CREATE INDEX idx_sys_notify_template_i18n ON sys_notify_template USING gin (i18n);
CREATE INDEX sys_notify_template_status_idx ON sys_notify_template (status);
CREATE INDEX sys_notify_template_type_idx ON sys_notify_template (type);

-- -----------------------------------------------------------------------------
-- sys_notify_message — in-app notifications (STRICT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_notify_message (
  id                BIGSERIAL    PRIMARY KEY,
  tenant_id         VARCHAR(20)  NOT NULL,       -- 所属租户
  user_id           TEXT         NOT NULL,       -- 接收用户 ID
  user_type         INT          NOT NULL,       -- 用户类型: 10 CUSTOM, 20 CLIENT
  template_id       INT          NOT NULL,       -- 模板 ID (快照来源)
  template_code     VARCHAR(100) NOT NULL,       -- 模板编码 (快照)
  template_nickname VARCHAR(100) NOT NULL,       -- 发送人名称 (快照)
  template_content  TEXT         NOT NULL,       -- 渲染后内容 (快照)
  template_params   TEXT,                        -- 模板参数 (JSON 快照)
  read_status       BOOLEAN      NOT NULL DEFAULT FALSE, -- 是否已读
  read_time         TIMESTAMPTZ,                 -- 已读时间
  del_flag          CHAR(1)      NOT NULL DEFAULT '0',
  create_at         TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_at         TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_sys_notify_msg_tenant_del ON sys_notify_message (tenant_id, del_flag);
CREATE INDEX sys_notify_message_create_at_idx ON sys_notify_message (create_at);
CREATE INDEX sys_notify_message_tenant_id_user_id_idx ON sys_notify_message (tenant_id, user_id);
CREATE INDEX sys_notify_message_user_id_read_status_idx ON sys_notify_message (user_id, read_status);

-- -----------------------------------------------------------------------------
-- sys_mail_account — SMTP accounts (NOT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_mail_account (
  id              SERIAL       PRIMARY KEY,
  mail            VARCHAR(255) NOT NULL UNIQUE, -- 邮箱地址 (唯一)
  username        VARCHAR(255) NOT NULL,       -- SMTP 用户名
  password        VARCHAR(255) NOT NULL,       -- SMTP 密码 (AES-256-CBC 加密存储)
  host            VARCHAR(255) NOT NULL,       -- SMTP 服务器地址
  port            INT          NOT NULL,       -- SMTP 端口
  ssl_enable      BOOLEAN      NOT NULL DEFAULT TRUE, -- 是否启用 SSL/TLS
  status          CHAR(1)      NOT NULL DEFAULT '0', -- '0' 启用, '1' 停用
  remark          VARCHAR(500),
  create_by       VARCHAR(64)  NOT NULL,
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by       VARCHAR(64)  NOT NULL,
  update_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  del_flag        CHAR(1)      NOT NULL DEFAULT '0'
);
CREATE INDEX sys_mail_account_status_idx ON sys_mail_account (status);

-- -----------------------------------------------------------------------------
-- sys_mail_template — email templates (NOT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_mail_template (
  id              SERIAL       PRIMARY KEY,
  name            VARCHAR(100) NOT NULL,       -- 模板名称
  code            VARCHAR(100) NOT NULL UNIQUE, -- 模板编码 (唯一, send 时按此查找)
  account_id      INT          NOT NULL REFERENCES sys_mail_account(id), -- 关联发送账户
  nickname        VARCHAR(100) NOT NULL,       -- 发件人昵称 (显示在收件人邮箱)
  title           VARCHAR(255) NOT NULL,       -- 邮件标题, 支持 ${key} 变量
  content         TEXT         NOT NULL,       -- 邮件正文 (HTML), 支持 ${key} 变量
  params          TEXT,                        -- 参数列表 (JSON)
  status          CHAR(1)      NOT NULL DEFAULT '0', -- '0' 启用, '1' 停用
  remark          VARCHAR(500),
  create_by       VARCHAR(64)  NOT NULL,
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by       VARCHAR(64)  NOT NULL,
  update_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  del_flag        CHAR(1)      NOT NULL DEFAULT '0'
);
CREATE INDEX sys_mail_template_account_id_idx ON sys_mail_template (account_id);
CREATE INDEX sys_mail_template_status_idx ON sys_mail_template (status);

-- -----------------------------------------------------------------------------
-- sys_mail_log — email send history (NOT tenant-scoped, written by send service)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_mail_log (
  id                 BIGSERIAL    PRIMARY KEY,
  user_id            TEXT,                        -- 触发发送的用户 ID (可空, 如系统触发)
  user_type          INT,                        -- 用户类型: 10 CUSTOM, 20 CLIENT
  to_mail            VARCHAR(255) NOT NULL,       -- 收件人邮箱
  account_id         INT          NOT NULL,       -- 发送账户 ID
  from_mail          VARCHAR(255) NOT NULL,       -- 发件人邮箱 (快照)
  template_id        INT          NOT NULL,       -- 模板 ID (快照来源)
  template_code      VARCHAR(100) NOT NULL,       -- 模板编码 (快照)
  template_nickname  VARCHAR(100) NOT NULL,       -- 发件人昵称 (快照)
  template_title     VARCHAR(255) NOT NULL,       -- 渲染后邮件标题 (快照)
  template_content   TEXT         NOT NULL,       -- 渲染后邮件正文 (快照)
  template_params    TEXT,                        -- 模板参数 (JSON 快照)
  send_status        INT          NOT NULL DEFAULT 0, -- 0 发送中, 1 成功, 2 失败
  send_time          TIMESTAMPTZ,                 -- 实际发送/完成时间
  error_msg          TEXT                         -- 失败原因
);
CREATE INDEX sys_mail_log_send_status_idx ON sys_mail_log (send_status);
CREATE INDEX sys_mail_log_send_time_idx ON sys_mail_log (send_time);
CREATE INDEX sys_mail_log_template_code_idx ON sys_mail_log (template_code);
CREATE INDEX sys_mail_log_to_mail_idx ON sys_mail_log (to_mail);

-- -----------------------------------------------------------------------------
-- sys_sms_channel — SMS provider configs (NOT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_sms_channel (
  id              SERIAL       PRIMARY KEY,
  code            VARCHAR(50)  NOT NULL UNIQUE,  -- 渠道编码: aliyun / tencent / huawei
  name            VARCHAR(100) NOT NULL,         -- 渠道名称
  signature       VARCHAR(100) NOT NULL,         -- 短信签名 (中国短信要求)
  api_key         VARCHAR(255) NOT NULL,         -- Access Key ID (加密存储)
  api_secret      VARCHAR(255) NOT NULL,         -- Access Key Secret (加密存储)
  callback_url    VARCHAR(500),                  -- 回执回调 URL
  status          CHAR(1)      NOT NULL DEFAULT '0', -- '0' 启用, '1' 停用
  remark          VARCHAR(500),
  create_by       VARCHAR(64)  NOT NULL,
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by       VARCHAR(64)  NOT NULL,
  update_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  del_flag        CHAR(1)      NOT NULL DEFAULT '0'
);
CREATE INDEX sys_sms_channel_status_idx ON sys_sms_channel (status);

-- -----------------------------------------------------------------------------
-- sys_sms_template — SMS templates (NOT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_sms_template (
  id              SERIAL       PRIMARY KEY,
  channel_id      INT          NOT NULL REFERENCES sys_sms_channel(id), -- 关联短信渠道
  code            VARCHAR(100) NOT NULL UNIQUE, -- 模板编码 (唯一, send 时按此查找)
  name            VARCHAR(100) NOT NULL,       -- 模板名称
  content         TEXT         NOT NULL,       -- 模板内容 (纯文本), 支持 ${key} 变量
  params          TEXT,                        -- 参数列表 (JSON)
  api_template_id VARCHAR(100) NOT NULL,       -- 第三方平台模板 ID (如 SMS_12345)
  type            INT          NOT NULL,       -- 类型: 1 验证码, 2 通知, 3 营销
  status          CHAR(1)      NOT NULL DEFAULT '0', -- '0' 启用, '1' 停用
  remark          VARCHAR(500),
  create_by       VARCHAR(64)  NOT NULL,
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by       VARCHAR(64)  NOT NULL,
  update_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  del_flag        CHAR(1)      NOT NULL DEFAULT '0'
);
CREATE INDEX sys_sms_template_channel_id_idx ON sys_sms_template (channel_id);
CREATE INDEX sys_sms_template_status_idx ON sys_sms_template (status);
CREATE INDEX sys_sms_template_type_idx ON sys_sms_template (type);

-- -----------------------------------------------------------------------------
-- sys_sms_log — SMS send history (NOT tenant-scoped, written by send service)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_sms_log (
  id               BIGSERIAL    PRIMARY KEY,
  channel_id       INT          NOT NULL,       -- 渠道 ID (快照)
  channel_code     VARCHAR(50)  NOT NULL,       -- 渠道编码 (快照)
  template_id      INT          NOT NULL,       -- 模板 ID (快照来源)
  template_code    VARCHAR(100) NOT NULL,       -- 模板编码 (快照)
  mobile           VARCHAR(20)  NOT NULL,       -- 接收手机号
  content          TEXT         NOT NULL,       -- 渲染后短信内容 (快照)
  params           TEXT,                        -- 模板参数 (JSON 快照)
  send_status      INT          NOT NULL DEFAULT 0, -- 0 发送中, 1 成功, 2 失败
  send_time        TIMESTAMPTZ,                 -- 实际发送/完成时间
  receive_status   INT,                         -- 回执状态: 0 未接收, 1 已接收
  receive_time     TIMESTAMPTZ,                 -- 回执确认时间
  api_send_code    VARCHAR(100),                -- 第三方发送流水号 (用于回执关联)
  api_receive_code VARCHAR(100),                -- 第三方回执编码
  error_msg        TEXT                         -- 失败原因
);
CREATE INDEX sys_sms_log_mobile_idx ON sys_sms_log (mobile);
CREATE INDEX sys_sms_log_send_status_idx ON sys_sms_log (send_status);
CREATE INDEX sys_sms_log_send_time_idx ON sys_sms_log (send_time);
CREATE INDEX sys_sms_log_template_code_idx ON sys_sms_log (template_code);

-- -----------------------------------------------------------------------------
-- sys_oper_log — operation audit log (STRICT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_oper_log (
  oper_id         VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id       VARCHAR(20)  NOT NULL,       -- 所属租户
  title           VARCHAR(50)  NOT NULL,       -- 操作模块 (如 "用户管理")
  business_type   INT          NOT NULL,       -- 业务类型: 0 其他, 1 新增, 2 修改, 3 删除, 4 授权, 5 导出, 6 导入, 9 清空
  request_method  VARCHAR(10)  NOT NULL,       -- HTTP 方法 (GET/POST/PUT/DELETE)
  operator_type   INT          NOT NULL,       -- 操作类别: 0 其他, 1 后台, 2 手机
  oper_name       VARCHAR(50)  NOT NULL,       -- 操作人员
  dept_name       VARCHAR(50)  NOT NULL,       -- 部门名称
  oper_url        VARCHAR(255) NOT NULL,       -- 请求 URL
  oper_location   VARCHAR(255) NOT NULL,       -- 操作地点 (IP 地理位置)
  oper_param      VARCHAR(2000) NOT NULL,      -- 请求参数
  json_result     VARCHAR(2000) NOT NULL,      -- 返回数据
  error_msg       VARCHAR(2000) NOT NULL,      -- 错误消息
  method          VARCHAR(100) NOT NULL,       -- 方法名称 (如 "UserController.create")
  oper_ip         VARCHAR(255) NOT NULL,       -- 操作 IP
  oper_time       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP, -- 操作时间
  status          CHAR(1)      NOT NULL,       -- '0' 成功, '1' 异常
  cost_time       INT          NOT NULL DEFAULT 0 -- 消耗时间 (毫秒)
);
CREATE INDEX sys_oper_log_business_type_idx ON sys_oper_log (business_type);
CREATE INDEX sys_oper_log_oper_name_idx ON sys_oper_log (oper_name);
CREATE INDEX sys_oper_log_oper_time_idx ON sys_oper_log (oper_time);
CREATE INDEX sys_oper_log_status_idx ON sys_oper_log (status);
CREATE INDEX sys_oper_log_tenant_id_oper_name_oper_time_idx ON sys_oper_log (tenant_id, oper_name, oper_time);
CREATE INDEX sys_oper_log_tenant_id_oper_time_idx ON sys_oper_log (tenant_id, oper_time);
CREATE INDEX sys_oper_log_tenant_id_status_oper_time_idx ON sys_oper_log (tenant_id, status, oper_time);

-- -----------------------------------------------------------------------------
-- sys_logininfor — login history (STRICT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_logininfor (
  info_id         VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id       VARCHAR(20)  NOT NULL,       -- 所属租户
  user_name       VARCHAR(50)  NOT NULL,       -- 登录账号
  ipaddr          VARCHAR(128) NOT NULL,       -- 登录 IP
  login_location  VARCHAR(255) NOT NULL,       -- 登录地点 (IP 地理位置)
  browser         VARCHAR(50)  NOT NULL,       -- 浏览器类型
  os              VARCHAR(50)  NOT NULL,       -- 操作系统
  device_type     CHAR(1)      NOT NULL,       -- 设备类型: '0' PC, '1' 移动
  status          CHAR(1)      NOT NULL,       -- '0' 成功, '1' 失败
  msg             VARCHAR(255) NOT NULL,       -- 提示消息
  del_flag        CHAR(1)      NOT NULL DEFAULT '0',
  login_time      TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP -- 登录时间
);
CREATE INDEX sys_logininfor_login_time_idx ON sys_logininfor (login_time);
CREATE INDEX sys_logininfor_status_idx ON sys_logininfor (status);
CREATE INDEX sys_logininfor_tenant_id_login_time_idx ON sys_logininfor (tenant_id, login_time);
CREATE INDEX sys_logininfor_tenant_id_status_login_time_idx ON sys_logininfor (tenant_id, status, login_time);
CREATE INDEX sys_logininfor_tenant_id_user_name_login_time_idx ON sys_logininfor (tenant_id, user_name, login_time);
CREATE INDEX sys_logininfor_user_name_idx ON sys_logininfor (user_name);

-- -----------------------------------------------------------------------------
-- sys_audit_log — generic audit trail (STRICT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_audit_log (
  id              VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id       VARCHAR(20)  NOT NULL,       -- 所属租户
  user_id         VARCHAR(36),                 -- 操作用户 ID
  user_name       VARCHAR(50),                 -- 操作用户名
  action          VARCHAR(100) NOT NULL,       -- 操作动作 (如 CREATE, UPDATE, DELETE)
  module          VARCHAR(50)  NOT NULL,       -- 操作模块 (如 user, role, tenant)
  target_type     VARCHAR(50),                 -- 操作对象类型
  target_id       VARCHAR(100),                -- 操作对象 ID
  old_value       TEXT,                        -- 变更前值 (JSON)
  new_value       TEXT,                        -- 变更后值 (JSON)
  ip              VARCHAR(128) NOT NULL,       -- 操作 IP
  user_agent      VARCHAR(500),                -- User-Agent
  request_id      VARCHAR(64),                 -- 请求追踪 ID
  status          CHAR(1)      NOT NULL,       -- '0' 成功, '1' 失败
  error_msg       VARCHAR(2000),               -- 错误消息
  duration        INT          NOT NULL DEFAULT 0, -- 耗时 (毫秒)
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX sys_audit_log_action_idx ON sys_audit_log (action);
CREATE INDEX sys_audit_log_module_idx ON sys_audit_log (module);
CREATE INDEX sys_audit_log_target_type_target_id_idx ON sys_audit_log (target_type, target_id);
CREATE INDEX sys_audit_log_tenant_id_create_at_idx ON sys_audit_log (tenant_id, create_at);
CREATE INDEX sys_audit_log_user_id_create_at_idx ON sys_audit_log (user_id, create_at);

-- -----------------------------------------------------------------------------
-- sys_upload — file records (STRICT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_upload (
  upload_id       VARCHAR(255) PRIMARY KEY,    -- 文件 ID (UUID 或存储系统 ID)
  tenant_id       VARCHAR(20)  NOT NULL,       -- 所属租户
  folder_id       VARCHAR(36)  NOT NULL,       -- 所属文件夹 ID
  size            INT          NOT NULL,       -- 文件大小 (字节)
  file_name       VARCHAR(255) NOT NULL,       -- 原始文件名
  new_file_name   VARCHAR(255) NOT NULL,       -- 存储文件名 (防重名)
  url             VARCHAR(500) NOT NULL,       -- 访问 URL
  ext             VARCHAR(50),                 -- 文件扩展名
  mime_type       VARCHAR(100),                -- MIME 类型
  storage_type    VARCHAR(20)  NOT NULL,       -- 存储类型 (local / oss / s3)
  file_md5        VARCHAR(32),                 -- 文件 MD5 (秒传用)
  thumbnail       VARCHAR(500),                -- 缩略图 URL
  parent_file_id  VARCHAR(255),                -- 父版本文件 ID (版本管理)
  version         INT          NOT NULL DEFAULT 1, -- 版本号
  is_latest       BOOLEAN      NOT NULL DEFAULT TRUE, -- 是否最新版本
  download_count  INT          NOT NULL DEFAULT 0, -- 下载次数
  status          CHAR(1)      NOT NULL DEFAULT '0', -- '0' 正常, '1' 停用
  del_flag        CHAR(1)      NOT NULL DEFAULT '0', -- '0' 正常, '2' 回收站
  create_by       VARCHAR(64)  NOT NULL,
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by       VARCHAR(64)  NOT NULL,
  update_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  remark          VARCHAR(500)
);
CREATE INDEX idx_sys_upload_tenant_del_status ON sys_upload (tenant_id, del_flag, status);
CREATE INDEX sys_upload_file_md5_del_flag_idx ON sys_upload (file_md5, del_flag);
CREATE INDEX sys_upload_parent_file_id_version_idx ON sys_upload (parent_file_id, version);
CREATE INDEX sys_upload_tenant_id_folder_id_idx ON sys_upload (tenant_id, folder_id);

-- -----------------------------------------------------------------------------
-- sys_file_folder — file folder tree (STRICT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_file_folder (
  folder_id       VARCHAR(36)  PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id       VARCHAR(20)  NOT NULL,       -- 所属租户
  parent_id       VARCHAR(36),                 -- 父文件夹 ID (null=根目录)
  folder_name     VARCHAR(100) NOT NULL,       -- 文件夹名称
  folder_path     VARCHAR(500) NOT NULL,       -- 完整路径 (如 /docs/reports)
  order_num       INT          NOT NULL DEFAULT 0, -- 排序号
  status          CHAR(1)      NOT NULL DEFAULT '0',
  del_flag        CHAR(1)      NOT NULL DEFAULT '0',
  create_by       VARCHAR(64)  NOT NULL,
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  update_by       VARCHAR(64)  NOT NULL,
  update_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP,
  remark          VARCHAR(500)
);
CREATE INDEX idx_sys_file_folder_tenant_del_status ON sys_file_folder (tenant_id, del_flag, status);
CREATE INDEX sys_file_folder_tenant_id_parent_id_idx ON sys_file_folder (tenant_id, parent_id);

-- -----------------------------------------------------------------------------
-- sys_file_share — file sharing links (STRICT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_file_share (
  share_id        VARCHAR(64)  PRIMARY KEY,    -- 分享链接 ID
  tenant_id       VARCHAR(20)  NOT NULL,       -- 所属租户
  upload_id       VARCHAR(255) NOT NULL,       -- 分享的文件 ID
  share_code      VARCHAR(6),                  -- 提取码 (null=无需提取码)
  expire_time     TIMESTAMPTZ,                 -- 过期时间 (null=永不过期)
  max_download    INT          NOT NULL DEFAULT 0, -- 最大下载次数 (0=无限制)
  download_count  INT          NOT NULL DEFAULT 0, -- 已下载次数
  status          CHAR(1)      NOT NULL DEFAULT '0', -- '0' 有效, '1' 失效
  create_by       VARCHAR(64)  NOT NULL,
  create_at       TIMESTAMPTZ  NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX sys_file_share_share_id_share_code_idx ON sys_file_share (share_id, share_code);
CREATE INDEX sys_file_share_tenant_id_upload_id_idx ON sys_file_share (tenant_id, upload_id);
CREATE INDEX sys_file_share_upload_id_idx ON sys_file_share (upload_id);

-- -----------------------------------------------------------------------------
-- sys_user_post — user-post junction (NOT tenant-scoped)
-- -----------------------------------------------------------------------------
CREATE TABLE sys_user_post (
  user_id         VARCHAR(36)  NOT NULL,       -- 用户 ID
  post_id         VARCHAR(36)  NOT NULL,       -- 岗位 ID
  PRIMARY KEY (user_id, post_id)
);
CREATE INDEX sys_user_post_post_id_idx ON sys_user_post (post_id);

-- =============================================================================
-- COMMENT ON — 表和字段注释 (pg_description)
-- =============================================================================

-- ─── sys_notice ─────────────────────────────────────────────────────────────
COMMENT ON TABLE sys_notice IS '系统公告';
COMMENT ON COLUMN sys_notice.notice_id IS '公告 ID';
COMMENT ON COLUMN sys_notice.tenant_id IS '所属租户';
COMMENT ON COLUMN sys_notice.notice_title IS '公告标题';
COMMENT ON COLUMN sys_notice.notice_type IS '类型: 1 通知, 2 公告';
COMMENT ON COLUMN sys_notice.notice_content IS '公告内容 (富文本)';
COMMENT ON COLUMN sys_notice.status IS '状态: 0 正常, 1 关闭';
COMMENT ON COLUMN sys_notice.del_flag IS '删除标志: 0 正常, 1 删除';

-- ─── sys_notify_template ────────────────────────────────────────────────────
COMMENT ON TABLE sys_notify_template IS '站内信模板';
COMMENT ON COLUMN sys_notify_template.name IS '模板名称';
COMMENT ON COLUMN sys_notify_template.code IS '模板编码 (唯一)';
COMMENT ON COLUMN sys_notify_template.nickname IS '发送人名称';
COMMENT ON COLUMN sys_notify_template.content IS '模板内容, 支持 ${key} 变量';
COMMENT ON COLUMN sys_notify_template.params IS '参数列表 (JSON 数组)';
COMMENT ON COLUMN sys_notify_template.type IS '类型: 1 系统通知, 2 业务通知';
COMMENT ON COLUMN sys_notify_template.status IS '状态: 0 启用, 1 停用';
COMMENT ON COLUMN sys_notify_template.i18n IS '国际化内容 (JSONB, 按 lang_code 分组)';

-- ─── sys_notify_message ─────────────────────────────────────────────────────
COMMENT ON TABLE sys_notify_message IS '站内信消息';
COMMENT ON COLUMN sys_notify_message.tenant_id IS '所属租户';
COMMENT ON COLUMN sys_notify_message.user_id IS '接收用户 ID';
COMMENT ON COLUMN sys_notify_message.user_type IS '用户类型: 10 CUSTOM, 20 CLIENT';
COMMENT ON COLUMN sys_notify_message.template_id IS '模板 ID (快照来源)';
COMMENT ON COLUMN sys_notify_message.template_code IS '模板编码 (快照)';
COMMENT ON COLUMN sys_notify_message.template_nickname IS '发送人名称 (快照)';
COMMENT ON COLUMN sys_notify_message.template_content IS '渲染后内容 (快照)';
COMMENT ON COLUMN sys_notify_message.template_params IS '模板参数 (JSON 快照)';
COMMENT ON COLUMN sys_notify_message.read_status IS '是否已读';
COMMENT ON COLUMN sys_notify_message.read_time IS '已读时间';

-- ─── sys_mail_account ───────────────────────────────────────────────────────
COMMENT ON TABLE sys_mail_account IS '邮件账户 (SMTP 配置)';
COMMENT ON COLUMN sys_mail_account.mail IS '邮箱地址';
COMMENT ON COLUMN sys_mail_account.username IS 'SMTP 用户名';
COMMENT ON COLUMN sys_mail_account.password IS 'SMTP 密码 (AES-256-CBC 加密存储)';
COMMENT ON COLUMN sys_mail_account.host IS 'SMTP 服务器地址';
COMMENT ON COLUMN sys_mail_account.port IS 'SMTP 端口';
COMMENT ON COLUMN sys_mail_account.ssl_enable IS '是否启用 SSL/TLS';
COMMENT ON COLUMN sys_mail_account.status IS '状态: 0 启用, 1 停用';

-- ─── sys_mail_template ──────────────────────────────────────────────────────
COMMENT ON TABLE sys_mail_template IS '邮件模板';
COMMENT ON COLUMN sys_mail_template.name IS '模板名称';
COMMENT ON COLUMN sys_mail_template.code IS '模板编码 (唯一, 发送时按此查找)';
COMMENT ON COLUMN sys_mail_template.account_id IS '关联发送账户 ID';
COMMENT ON COLUMN sys_mail_template.nickname IS '发件人昵称';
COMMENT ON COLUMN sys_mail_template.title IS '邮件标题, 支持 ${key} 变量';
COMMENT ON COLUMN sys_mail_template.content IS '邮件正文 (HTML), 支持 ${key} 变量';
COMMENT ON COLUMN sys_mail_template.params IS '参数列表 (JSON 数组)';
COMMENT ON COLUMN sys_mail_template.status IS '状态: 0 启用, 1 停用';

-- ─── sys_mail_log ───────────────────────────────────────────────────────────
COMMENT ON TABLE sys_mail_log IS '邮件发送日志';
COMMENT ON COLUMN sys_mail_log.user_id IS '触发发送的用户 ID';
COMMENT ON COLUMN sys_mail_log.user_type IS '用户类型: 10 CUSTOM, 20 CLIENT';
COMMENT ON COLUMN sys_mail_log.to_mail IS '收件人邮箱';
COMMENT ON COLUMN sys_mail_log.account_id IS '发送账户 ID';
COMMENT ON COLUMN sys_mail_log.from_mail IS '发件人邮箱 (快照)';
COMMENT ON COLUMN sys_mail_log.template_id IS '模板 ID (快照来源)';
COMMENT ON COLUMN sys_mail_log.template_code IS '模板编码 (快照)';
COMMENT ON COLUMN sys_mail_log.template_nickname IS '发件人昵称 (快照)';
COMMENT ON COLUMN sys_mail_log.template_title IS '渲染后邮件标题 (快照)';
COMMENT ON COLUMN sys_mail_log.template_content IS '渲染后邮件正文 (快照)';
COMMENT ON COLUMN sys_mail_log.template_params IS '模板参数 (JSON 快照)';
COMMENT ON COLUMN sys_mail_log.send_status IS '发送状态: 0 发送中, 1 成功, 2 失败';
COMMENT ON COLUMN sys_mail_log.send_time IS '实际发送/完成时间';
COMMENT ON COLUMN sys_mail_log.error_msg IS '失败原因';

-- ─── sys_sms_channel ────────────────────────────────────────────────────────
COMMENT ON TABLE sys_sms_channel IS '短信渠道配置';
COMMENT ON COLUMN sys_sms_channel.code IS '渠道编码: aliyun / tencent / huawei';
COMMENT ON COLUMN sys_sms_channel.name IS '渠道名称';
COMMENT ON COLUMN sys_sms_channel.signature IS '短信签名';
COMMENT ON COLUMN sys_sms_channel.api_key IS 'Access Key ID (加密存储)';
COMMENT ON COLUMN sys_sms_channel.api_secret IS 'Access Key Secret (加密存储)';
COMMENT ON COLUMN sys_sms_channel.callback_url IS '回执回调 URL';
COMMENT ON COLUMN sys_sms_channel.status IS '状态: 0 启用, 1 停用';

-- ─── sys_sms_template ───────────────────────────────────────────────────────
COMMENT ON TABLE sys_sms_template IS '短信模板';
COMMENT ON COLUMN sys_sms_template.channel_id IS '关联短信渠道 ID';
COMMENT ON COLUMN sys_sms_template.code IS '模板编码 (唯一, 发送时按此查找)';
COMMENT ON COLUMN sys_sms_template.name IS '模板名称';
COMMENT ON COLUMN sys_sms_template.content IS '模板内容 (纯文本), 支持 ${key} 变量';
COMMENT ON COLUMN sys_sms_template.params IS '参数列表 (JSON 数组)';
COMMENT ON COLUMN sys_sms_template.api_template_id IS '第三方平台模板 ID (如 SMS_12345)';
COMMENT ON COLUMN sys_sms_template.type IS '类型: 1 验证码, 2 通知, 3 营销';
COMMENT ON COLUMN sys_sms_template.status IS '状态: 0 启用, 1 停用';

-- ─── sys_sms_log ────────────────────────────────────────────────────────────
COMMENT ON TABLE sys_sms_log IS '短信发送日志';
COMMENT ON COLUMN sys_sms_log.channel_id IS '渠道 ID (快照)';
COMMENT ON COLUMN sys_sms_log.channel_code IS '渠道编码 (快照)';
COMMENT ON COLUMN sys_sms_log.template_id IS '模板 ID (快照来源)';
COMMENT ON COLUMN sys_sms_log.template_code IS '模板编码 (快照)';
COMMENT ON COLUMN sys_sms_log.mobile IS '接收手机号';
COMMENT ON COLUMN sys_sms_log.content IS '渲染后短信内容 (快照)';
COMMENT ON COLUMN sys_sms_log.params IS '模板参数 (JSON 快照)';
COMMENT ON COLUMN sys_sms_log.send_status IS '发送状态: 0 发送中, 1 成功, 2 失败';
COMMENT ON COLUMN sys_sms_log.send_time IS '实际发送/完成时间';
COMMENT ON COLUMN sys_sms_log.receive_status IS '回执状态: 0 未接收, 1 已接收';
COMMENT ON COLUMN sys_sms_log.receive_time IS '回执确认时间';
COMMENT ON COLUMN sys_sms_log.api_send_code IS '第三方发送流水号 (用于回执关联)';
COMMENT ON COLUMN sys_sms_log.api_receive_code IS '第三方回执编码';
COMMENT ON COLUMN sys_sms_log.error_msg IS '失败原因';

-- ─── sys_oper_log ───────────────────────────────────────────────────────────
COMMENT ON TABLE sys_oper_log IS '操作日志';
COMMENT ON COLUMN sys_oper_log.tenant_id IS '所属租户';
COMMENT ON COLUMN sys_oper_log.title IS '操作模块';
COMMENT ON COLUMN sys_oper_log.business_type IS '业务类型: 0 其他, 1 新增, 2 修改, 3 删除, 4 授权, 5 导出, 6 导入, 9 清空';
COMMENT ON COLUMN sys_oper_log.request_method IS 'HTTP 方法';
COMMENT ON COLUMN sys_oper_log.operator_type IS '操作类别: 0 其他, 1 后台, 2 手机';
COMMENT ON COLUMN sys_oper_log.oper_name IS '操作人员';
COMMENT ON COLUMN sys_oper_log.dept_name IS '部门名称';
COMMENT ON COLUMN sys_oper_log.oper_url IS '请求 URL';
COMMENT ON COLUMN sys_oper_log.oper_location IS '操作地点 (IP 地理位置)';
COMMENT ON COLUMN sys_oper_log.oper_param IS '请求参数';
COMMENT ON COLUMN sys_oper_log.json_result IS '返回数据';
COMMENT ON COLUMN sys_oper_log.error_msg IS '错误消息';
COMMENT ON COLUMN sys_oper_log.method IS '方法名称';
COMMENT ON COLUMN sys_oper_log.oper_ip IS '操作 IP';
COMMENT ON COLUMN sys_oper_log.oper_time IS '操作时间';
COMMENT ON COLUMN sys_oper_log.status IS '状态: 0 成功, 1 异常';
COMMENT ON COLUMN sys_oper_log.cost_time IS '消耗时间 (毫秒)';

-- ─── sys_logininfor ─────────────────────────────────────────────────────────
COMMENT ON TABLE sys_logininfor IS '登录日志';
COMMENT ON COLUMN sys_logininfor.tenant_id IS '所属租户';
COMMENT ON COLUMN sys_logininfor.user_name IS '登录账号';
COMMENT ON COLUMN sys_logininfor.ipaddr IS '登录 IP';
COMMENT ON COLUMN sys_logininfor.login_location IS '登录地点';
COMMENT ON COLUMN sys_logininfor.browser IS '浏览器类型';
COMMENT ON COLUMN sys_logininfor.os IS '操作系统';
COMMENT ON COLUMN sys_logininfor.device_type IS '设备类型: 0 PC, 1 移动';
COMMENT ON COLUMN sys_logininfor.status IS '状态: 0 成功, 1 失败';
COMMENT ON COLUMN sys_logininfor.msg IS '提示消息';
COMMENT ON COLUMN sys_logininfor.login_time IS '登录时间';

-- ─── sys_audit_log ──────────────────────────────────────────────────────────
COMMENT ON TABLE sys_audit_log IS '审计日志';
COMMENT ON COLUMN sys_audit_log.tenant_id IS '所属租户';
COMMENT ON COLUMN sys_audit_log.user_id IS '操作用户 ID';
COMMENT ON COLUMN sys_audit_log.user_name IS '操作用户名';
COMMENT ON COLUMN sys_audit_log.action IS '操作动作 (CREATE / UPDATE / DELETE)';
COMMENT ON COLUMN sys_audit_log.module IS '操作模块 (user / role / tenant)';
COMMENT ON COLUMN sys_audit_log.target_type IS '操作对象类型';
COMMENT ON COLUMN sys_audit_log.target_id IS '操作对象 ID';
COMMENT ON COLUMN sys_audit_log.old_value IS '变更前值 (JSON)';
COMMENT ON COLUMN sys_audit_log.new_value IS '变更后值 (JSON)';
COMMENT ON COLUMN sys_audit_log.ip IS '操作 IP';
COMMENT ON COLUMN sys_audit_log.user_agent IS 'User-Agent';
COMMENT ON COLUMN sys_audit_log.request_id IS '请求追踪 ID';
COMMENT ON COLUMN sys_audit_log.status IS '状态: 0 成功, 1 失败';
COMMENT ON COLUMN sys_audit_log.error_msg IS '错误消息';
COMMENT ON COLUMN sys_audit_log.duration IS '耗时 (毫秒)';

-- ─── sys_upload ─────────────────────────────────────────────────────────────
COMMENT ON TABLE sys_upload IS '文件上传记录';
COMMENT ON COLUMN sys_upload.upload_id IS '文件 ID';
COMMENT ON COLUMN sys_upload.tenant_id IS '所属租户';
COMMENT ON COLUMN sys_upload.folder_id IS '所属文件夹 ID';
COMMENT ON COLUMN sys_upload.size IS '文件大小 (字节)';
COMMENT ON COLUMN sys_upload.file_name IS '原始文件名';
COMMENT ON COLUMN sys_upload.new_file_name IS '存储文件名 (防重名)';
COMMENT ON COLUMN sys_upload.url IS '访问 URL';
COMMENT ON COLUMN sys_upload.ext IS '文件扩展名';
COMMENT ON COLUMN sys_upload.mime_type IS 'MIME 类型';
COMMENT ON COLUMN sys_upload.storage_type IS '存储类型: local / oss / s3';
COMMENT ON COLUMN sys_upload.file_md5 IS '文件 MD5 (秒传用)';
COMMENT ON COLUMN sys_upload.thumbnail IS '缩略图 URL';
COMMENT ON COLUMN sys_upload.parent_file_id IS '父版本文件 ID (版本管理)';
COMMENT ON COLUMN sys_upload.version IS '版本号';
COMMENT ON COLUMN sys_upload.is_latest IS '是否最新版本';
COMMENT ON COLUMN sys_upload.download_count IS '下载次数';
COMMENT ON COLUMN sys_upload.del_flag IS '删除标志: 0 正常, 2 回收站';

-- ─── sys_file_folder ────────────────────────────────────────────────────────
COMMENT ON TABLE sys_file_folder IS '文件夹';
COMMENT ON COLUMN sys_file_folder.tenant_id IS '所属租户';
COMMENT ON COLUMN sys_file_folder.parent_id IS '父文件夹 ID (null=根目录)';
COMMENT ON COLUMN sys_file_folder.folder_name IS '文件夹名称';
COMMENT ON COLUMN sys_file_folder.folder_path IS '完整路径';
COMMENT ON COLUMN sys_file_folder.order_num IS '排序号';

-- ─── sys_file_share ─────────────────────────────────────────────────────────
COMMENT ON TABLE sys_file_share IS '文件分享链接';
COMMENT ON COLUMN sys_file_share.tenant_id IS '所属租户';
COMMENT ON COLUMN sys_file_share.upload_id IS '分享的文件 ID';
COMMENT ON COLUMN sys_file_share.share_code IS '提取码 (null=无需提取码)';
COMMENT ON COLUMN sys_file_share.expire_time IS '过期时间 (null=永不过期)';
COMMENT ON COLUMN sys_file_share.max_download IS '最大下载次数 (0=无限制)';
COMMENT ON COLUMN sys_file_share.download_count IS '已下载次数';
COMMENT ON COLUMN sys_file_share.status IS '状态: 0 有效, 1 失效';

-- ─── sys_user_post ──────────────────────────────────────────────────────────
COMMENT ON TABLE sys_user_post IS '用户与岗位关联';
COMMENT ON COLUMN sys_user_post.user_id IS '用户 ID';
COMMENT ON COLUMN sys_user_post.post_id IS '岗位 ID';

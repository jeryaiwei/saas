# 多租户架构规范 v1.0

**生效日期**：2026-04-13
**状态**：Normative（规范性）

---

## 1. 租户层级

固定三层结构，通过 `parent_id` 推导，不使用 `tenant_type` 字段：

```text
超级租户 (parent_id IS NULL)           ← 唯一，系统初始化创建
  ├── 平台 A (parent_id = 超级.tenant_id) ← 可管理下级
  │   ├── 租户 A1 (parent_id = A.tenant_id) ← 叶子节点
  │   └── 租户 A2 (parent_id = A.tenant_id)
  └── 平台 B (parent_id = 超级.tenant_id)
      └── 租户 B1 (parent_id = B.tenant_id)
```

### 1.1 判定规则

```rust
/// 种子数据契约：超级租户 ID 固定为 "000000"
pub const SUPER_TENANT_ID: &str = "000000";

impl SysTenant {
    fn is_super(&self) -> bool {
        self.tenant_id == SUPER_TENANT_ID
    }

    fn is_platform(&self) -> bool {
        !self.is_super() && self.parent_id.as_deref() == Some(SUPER_TENANT_ID)
    }

    fn is_regular(&self) -> bool {
        !self.is_super() && !self.is_platform()
    }
}
```

`SUPER_TENANT_ID = "000000"` 是与种子数据的固定契约，不需要运行时查询。

### 1.2 层级约束

| 规则 | 描述 |
| --- | --- |
| 超级租户唯一 | 系统中只能有一个 `parent_id IS NULL` 的租户 |
| 超级租户不可删除/停用 | 代码层面保护 |
| 平台租户由超级管理员创建 | `parent_id = 超级租户.tenant_id` |
| 一般租户由平台管理员创建 | `parent_id = 平台.tenant_id` |
| 一般租户不能创建下级 | service 层校验，禁止 3 层以下 |
| 删除平台前必须删除下级租户 | 外键级联或 service 层校验 |

---

## 2. 套餐绑定

### 2.1 套餐与租户的关系

| 租户类型 | 是否需要套餐 | 说明 |
| --- | --- | --- |
| 超级租户 | **不需要** | 拥有所有权限，跳过套餐检查 |
| 平台租户 | **必须** | 定义平台能力边界 |
| 一般租户 | **必须** | 权限不能超过所属平台 |

### 2.2 套餐范围约束

```text
平台套餐.menuIds ⊇ 一般租户套餐.menuIds
```

创建一般租户时**必须校验**：

```rust
fn validate_tenant_package(
    tenant_package: &SysTenantPackage,
    platform_package: &SysTenantPackage,
) -> Result<()> {
    let platform_set: HashSet<&str> = platform_package.menu_ids.iter().map(|s| s.as_str()).collect();
    let tenant_set: HashSet<&str> = tenant_package.menu_ids.iter().map(|s| s.as_str()).collect();
    if !tenant_set.is_subset(&platform_set) {
        return Err(TENANT_PACKAGE_EXCEEDS_PLATFORM);
    }
    Ok(())
}
```

### 2.3 无套餐拒绝登录

非超级租户登录时，如果 `package_id` 为空或对应的套餐不存在/已停用：

```rust
if !tenant.is_super() && tenant.package_id.is_none() {
    return Err(TENANT_NO_PACKAGE);
}
```

---

## 3. 用户与租户绑定

### 3.1 sys_user_tenant 表

```sql
CREATE TABLE sys_user_tenant (
  id          VARCHAR(36) PRIMARY KEY,
  user_id     VARCHAR(36) NOT NULL,
  tenant_id   VARCHAR(20) NOT NULL,
  is_default  CHAR(1) NOT NULL DEFAULT '0',  -- '1' = 默认登录租户
  is_admin    CHAR(1) NOT NULL DEFAULT '0',  -- '1' = 该租户管理员
  status      CHAR(1) NOT NULL DEFAULT '0',
  UNIQUE (user_id, tenant_id)
);
```

### 3.2 字段语义

| 字段 | 含义 | 作用域 |
| --- | --- | --- |
| `SysUser.platformId` | 用户归属平台 | 创建时确定，不可变 |
| `SysUserTenant.tenantId` | 用户可访问的租户 | 多对多绑定 |
| `SysUserTenant.isAdmin` | 用户在**该租户**是否管理员 | 每个绑定独立 |
| `SysUserTenant.isDefault` | 用户默认登录哪个租户 | 每用户唯一一个 '1' |

**同一用户在不同租户可以有不同身份**：

```
User Alice:
  platformId = '001'
  绑定1: (tenantId='001', isAdmin='1')      → 平台管理员
  绑定2: (tenantId='001-A', isAdmin='0')    → 普通用户
  绑定3: (tenantId='001-B', isAdmin='1')    → 租户管理员
```

---

## 4. 管理员层级

### 4.1 四级管理员

| 角色 | 判定条件 | 权限范围 |
| --- | --- | --- |
| **超级管理员** | `tenant.is_super()` + `isAdmin=true` | 全局，不受套餐限制 |
| **平台管理员** | `tenant.is_platform()` + `isAdmin=true` | 平台套餐内所有菜单 |
| **租户管理员** | `isAdmin=true`（一般租户） | 租户套餐内所有菜单 |
| **普通用户** | `isAdmin=false` | 角色菜单 ∩ 租户套餐 |

### 4.2 判定函数

```rust
fn get_admin_role(tenant: &SysTenant, is_admin: bool) -> AdminRole {
    if !is_admin {
        return AdminRole::User;
    }
    if tenant.is_super() {
        AdminRole::SuperAdmin
    } else if tenant.is_platform() {
        AdminRole::PlatformAdmin
    } else {
        AdminRole::TenantAdmin
    }
}
```

### 4.3 权限计算公式

```text
超级管理员:
  permissions = SELECT DISTINCT perms FROM sys_menu
                WHERE status='0' AND del_flag='0' AND perms != ''

平台管理员 / 租户管理员:
  permissions = SELECT DISTINCT m.perms FROM sys_menu m
                WHERE m.menu_id IN (SELECT unnest(menu_ids) FROM sys_tenant_package WHERE package_id = ?)
                  AND m.status='0' AND m.del_flag='0' AND m.perms != ''

普通用户:
  permissions = SELECT DISTINCT m.perms FROM sys_menu m
                JOIN sys_role_menu rm ON rm.menu_id = m.menu_id
                JOIN sys_user_role ur ON ur.role_id = rm.role_id
                JOIN sys_role r ON r.role_id = ur.role_id
                WHERE ur.user_id = ?
                  AND r.tenant_id = ?
                  AND r.status='0' AND r.del_flag='0'
                  AND m.status='0' AND m.del_flag='0'
                  AND m.perms != ''
                  AND m.menu_id IN (SELECT unnest(menu_ids) FROM sys_tenant_package WHERE package_id = ?)
```

**关键**：非超级管理员的权限**始终**受套餐 `menuIds` 过滤。

---

## 5. 登录流程

```text
POST /auth/login { username, password }
  │
  ├─ 1. 查 SysUser → 获取 platformId, userType
  │
  ├─ 2. 查 SysUserTenant → 确定登录租户
  │     ├─ 请求带 tenantId → 用指定的
  │     ├─ 未指定 → 查 isDefault='1' 的绑定
  │     └─ 无默认 → 用 platformId 作为 fallback
  │
  ├─ 3. 查 SysTenant → 校验租户状态
  │     ├─ status='0' (活跃)
  │     ├─ expire_time 未过期
  │     └─ 非超级租户 → 必须有 package_id
  │
  ├─ 4. 确定 isAdmin
  │     └─ 查 SysUserTenant (userId, tenantId) → isAdmin 字段
  │
  ├─ 5. 计算 permissions
  │     ├─ 超级管理员 → 所有菜单权限
  │     ├─ 管理员(平台/租户) → 套餐范围内所有菜单权限
  │     └─ 普通用户 → 角色菜单 ∩ 套餐范围
  │
  ├─ 6. 构建 Session → 存 Redis
  │     { userId, tenantId, platformId, isAdmin, permissions, sysCode, ... }
  │
  └─ 7. 签发 JWT → 返回 access_token + refresh_token
```

---

## 6. 租户切换

### 6.1 切换权限矩阵

| 角色 | 可切换到 | 不可切换到 |
| --- | --- | --- |
| 超级管理员 | 任何活跃租户 | — |
| 平台管理员 | 本平台下所有一般租户 | 其他平台、超级租户 |
| 租户管理员 | 有 sys_user_tenant 绑定的租户 | 无绑定的租户 |
| 普通用户 | 有绑定的租户 | 无绑定的租户 |
| CLIENT 用户 | **禁止切换** | — |

### 6.2 切换校验

```rust
fn can_switch_to(
    user: &UserSession,
    target: &SysTenant,
) -> Result<()> {
    // 1. 不能切到当前租户
    if user.tenant_id.as_deref() == Some(&target.tenant_id) {
        return Err(ALREADY_IN_TENANT);
    }

    // 2. CLIENT 不能切换
    if user.user_type == "20" {
        return Err(CLIENT_CANNOT_SWITCH);
    }

    // 3. 目标必须活跃 + 未过期
    if target.status != "0" { return Err(TENANT_DISABLED); }
    if let Some(expire) = &target.expire_time {
        if *expire < Utc::now() { return Err(TENANT_EXPIRED); }
    }

    // 4. 非超级租户必须有套餐
    if !target.is_super() && target.package_id.is_none() {
        return Err(TENANT_NO_PACKAGE);
    }

    // 5. 超级管理员 → 任意
    if is_super_admin(user) {
        return Ok(());
    }

    // 6. 平台管理员 → 本平台下的租户
    if is_platform_admin(user) {
        if target.parent_id.as_deref() == user.platform_id.as_deref() {
            return Ok(());
        }
        return Err(TENANT_NOT_IN_PLATFORM);
    }

    // 7. 其他用户 → 有绑定的租户
    // (查 sys_user_tenant)
    Ok(())
}
```

### 6.3 切换流程

```text
GET /system/tenant/dynamic/{targetTenantId}
  │
  ├─ 1. can_switch_to() 校验
  │
  ├─ 2. 保存原始状态到 Redis
  │     key: switch_original:{session_uuid}
  │     value: { tenantId, isAdmin, permissions, sysCode, switchedAt }
  │
  ├─ 3. 查目标租户 → 获取套餐
  │
  ├─ 4. 查 SysUserTenant → 确定目标租户的 isAdmin
  │
  ├─ 5. 重新计算 permissions（同登录步骤 5）
  │
  ├─ 6. 更新 Redis Session
  │     tenantId → targetTenantId
  │     isAdmin → 重算
  │     permissions → 重算
  │     sysCode → 重算
  │
  └─ 7. 返回成功（前端重新加载路由）
```

### 6.4 恢复流程

```text
GET /system/tenant/dynamic/clear
  │
  ├─ 1. 从 Redis 读取 switch_original:{uuid}
  │
  ├─ 2. 恢复 Session（用快照，不重新计算）
  │     tenantId → original.tenantId
  │     isAdmin → original.isAdmin
  │     permissions → original.permissions
  │     sysCode → original.sysCode
  │
  ├─ 3. 删除 switch_original:{uuid}
  │
  └─ 4. 返回成功
```

**恢复用快照而非重算**——"恢复"语义是"回到切换前的状态"，不是"重新登录原租户"。

### 6.5 可切换租户列表

```text
GET /system/tenant/select-list
  │
  ├─ 超级管理员: 所有活跃租户（排除当前）
  │
  ├─ 平台管理员: 
  │   本平台下一般租户 ∪ 自己有绑定的其他租户（排除当前）
  │
  └─ 其他用户:
      自己有 sys_user_tenant 绑定的活跃租户（排除当前）
```

### 6.6 切换状态查询

```text
GET /system/tenant/switch-status
→ {
    currentTenantId: "001-A",
    defaultTenantId: "001",
    isSwitched: true       // switchedFrom 存在即为 true
  }
```

---

## 7. Session 结构

```rust
pub struct UserSession {
    // === 身份（不变）===
    pub user_id: String,
    pub user_name: String,
    pub user_type: String,           // "10" CUSTOM / "20" CLIENT
    pub platform_id: Option<String>, // 归属平台（创建时确定）
    pub lang: Option<String>,

    // === 当前租户（切换时变）===
    pub tenant_id: Option<String>,   // 当前活跃租户
    pub is_admin: bool,              // 当前租户下是否管理员
    pub permissions: Vec<String>,    // 当前租户下的权限列表
    pub roles: Vec<String>,          // 当前租户下的角色列表
    pub sys_code: Option<String>,    // 当前租户套餐的子系统码
}
```

切换原始状态单独存储：

```
Redis key: switch_original:{session_uuid}
TTL: 与 session 相同
```

---

## 8. 数据过滤模型

| 模型类型 | 过滤键 | 说明 | 代表 |
| --- | --- | --- | --- |
| **PLATFORM** | `platformId` | 同平台共享数据 | Config, DictType, DictData |
| **STRICT** | `tenantId` | 租户隔离 | Dept, Role, Post, Upload, Notice, ... |
| **不过滤** | — | 全局共享或显式查询 | User, Menu, Tenant, TenantPackage |
| **CLIENT** | `createBy (userId)` | C 端用户数据隔离 | 用户自有数据 |

### 8.1 Rust 实现

```rust
// STRICT 模型
let tenant = current_tenant_scope();  // Option<String>
sqlx::query("... WHERE ($1::varchar IS NULL OR tenant_id = $1)")
    .bind(tenant.as_deref())

// PLATFORM 模型
let platform = current_platform_scope();  // Option<String>
sqlx::query("... WHERE ($1::varchar IS NULL OR tenant_id = $1)")
    .bind(platform.as_deref())
```

`current_tenant_scope()` 返回 `None` 当 `ignore_tenant = true`（超级管理员跨租户操作）。

---

## 9. 创建租户规则

### 9.1 谁能创建

| 操作者 | 可创建 |
| --- | --- |
| 超级管理员 | 平台租户 |
| 平台管理员 | 本平台下的一般租户 |
| 租户管理员 | **不能创建** |
| 普通用户 | **不能创建** |

### 9.2 创建流程

```text
POST /system/tenant/
  │
  ├─ 1. 校验操作者权限
  │
  ├─ 2. 确定 parent_id
  │     ├─ 超级管理员创建平台: parent_id = 超级租户.tenant_id
  │     └─ 平台管理员创建一般: parent_id = 当前平台.tenant_id
  │
  ├─ 3. 校验层级
  │     └─ 一般租户不能有下级 (查 parent 的 parent)
  │
  ├─ 4. 校验套餐
  │     ├─ 必须指定 package_id
  │     └─ 一般租户: 套餐.menuIds ⊆ 平台套餐.menuIds
  │
  ├─ 5. 创建 SysTenant
  │
  ├─ 6. 创建管理员用户 + sys_user_tenant(isAdmin='1')
  │
  └─ 7. 返回
```

---

## 10. Token 刷新

```text
POST /auth/refresh-token { refreshToken }
  │
  ├─ 1. 解码 JWT → { uuid, userId, tenantId }
  ├─ 2. 黑名单检查 → TOKEN_INVALID
  ├─ 3. Token 版本检查 → TOKEN_INVALID (密码已改)
  ├─ 4. 查 Redis Session → TOKEN_EXPIRED
  ├─ 5. 删除旧 Session + 加入黑名单
  ├─ 6. 重新计算权限（反映管理员最新的角色变更）
  ├─ 7. 创建新 Session + 签新 JWT
  └─ 8. 返回新 access_token + refresh_token
```

**权限重算**：refresh 时重新计算权限，确保管理员对用户的角色变更能即时生效。

---

## 11. 实施状态

### 11.1 已实现

| 功能 | 状态 |
| --- | --- |
| 登录确定 tenantId (default binding) | ✅ |
| platformId 传播 (session → RequestContext) | ✅ |
| STRICT / PLATFORM 数据过滤 | ✅ |
| current_tenant_scope / current_platform_scope | ✅ |
| 基础租户切换 (switch / clear / select-list) | ✅ |
| Token 刷新 | ✅ |

### 11.2 待优化

| 功能 | 优先级 | 说明 |
| --- | --- | --- |
| 权限计算补套餐过滤 | **高** | 普通用户权限 ∩ 套餐 menuIds |
| 无套餐拒绝登录 | **高** | 非超级租户必须有套餐 |
| 切换时保存 switchedFrom | 中 | Redis 快照恢复 |
| 切换校验完整规则 | 中 | 超级/平台/绑定三种路径 |
| 创建租户校验套餐范围 | 中 | 子集检查 |
| 套餐到期校验 | 中 | 登录 + 切换时检查 |
| CLIENT 用户禁止切换 | 低 | C 端用户归属固定平台 |

//! Auth service — login / logout / get-info / tenant-list / refresh-token / get-routers business logic.

use super::dto::{
    CaptchaCodeResponseDto, CurrentUserInfoResponseDto, LoginDto, LoginTokenResponseDto,
    RefreshTokenDto, RouterConfig, RouterMeta, TenantListForLoginDto, TenantVo,
    UserProfileDto,
};
use crate::domain::menu_repo::RouterMenuItem;
use crate::domain::{MenuRepo, UserRepo};
use crate::state::AppState;
use anyhow::Context;
use framework::auth::{jwt, session, JwtClaims, UserSession};
use framework::constants::SUPER_TENANT_ID;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::infra::{captcha, crypto};
use framework::response::ResponseCode;
use std::collections::HashMap;

#[tracing::instrument(skip_all, fields(username = %dto.username))]
pub async fn login(state: &AppState, dto: LoginDto) -> Result<LoginTokenResponseDto, AppError> {
    // 1. Captcha — if the client sent a captcha id, verify it.
    if let (Some(cid), Some(code)) = (dto.captcha_id.as_deref(), dto.captcha_code.as_deref()) {
        if !cid.is_empty() {
            let ok = captcha::verify_and_consume(&state.redis, &state.config.redis_keys, cid, code)
                .await
                .into_internal()?;
            if !ok {
                return Err(AppError::business(ResponseCode::CAPTCHA_INVALID));
            }
        }
    }

    // 2. Look up user
    let user = UserRepo::find_by_username(&state.pg, &dto.username)
        .await
        .into_internal()?
        .or_business(ResponseCode::INVALID_CREDENTIALS)?;

    if !user.is_active() {
        return Err(AppError::business(ResponseCode::ACCOUNT_LOCKED));
    }

    // 3. Verify password (bcrypt, must accept NestJS-written hashes)
    if !crypto::verify_password(&dto.password, &user.password).await {
        return Err(AppError::business(ResponseCode::INVALID_CREDENTIALS));
    }

    // 4. Resolve tenant binding.
    //    Priority: super admin → explicit tenantId → default binding → platformId fallback.
    let user_tenants = UserRepo::find_user_tenants(&state.pg, &user.user_id)
        .await
        .into_internal()?;

    let is_super_admin = user_tenants
        .iter()
        .any(|t| t.tenant_id == SUPER_TENANT_ID && t.is_admin_flag());

    let (chosen_tenant_id, is_admin) = if is_super_admin {
        // Super admin: use explicit tenantId or default to super tenant
        let tid = dto.tenant_id.unwrap_or_else(|| SUPER_TENANT_ID.to_string());
        (Some(tid), true)
    } else if let Some(explicit_tid) = &dto.tenant_id {
        // Explicit tenant selection: verify binding exists
        match user_tenants.iter().find(|t| t.tenant_id == *explicit_tid) {
            Some(t) => (Some(t.tenant_id.clone()), t.is_admin_flag()),
            None => return Err(AppError::business(ResponseCode::TENANT_BINDING_NOT_FOUND)),
        }
    } else {
        // Default binding → first binding → platformId fallback
        match user_tenants.iter().find(|t| t.is_default_flag()) {
            Some(t) => (Some(t.tenant_id.clone()), t.is_admin_flag()),
            None => match user_tenants.first() {
                Some(t) => (Some(t.tenant_id.clone()), t.is_admin_flag()),
                None => (Some(user.platform_id.clone()), false),
            },
        }
    };

    // 4b. Validate tenant status (skip for super tenant).
    if let Some(tid) = chosen_tenant_id.as_deref() {
        if tid != SUPER_TENANT_ID {
            let tenant_row: Option<(String, Option<chrono::DateTime<chrono::Utc>>)> =
                sqlx::query_as(
                    "SELECT status, expire_time FROM sys_tenant \
                     WHERE tenant_id = $1 AND del_flag = '0'",
                )
                .bind(tid)
                .fetch_optional(&state.pg)
                .await
                .context("login: check tenant status")
                .into_internal()?;

            match tenant_row {
                Some((status, expire_time)) => {
                    if status != "0" {
                        return Err(AppError::business(ResponseCode::TENANT_NOT_FOUND));
                    }
                    if let Some(exp) = expire_time {
                        if exp < chrono::Utc::now() {
                            return Err(AppError::business(ResponseCode::TENANT_EXPIRED));
                        }
                    }
                }
                None => return Err(AppError::business(ResponseCode::TENANT_NOT_FOUND)),
            }
        }
    }

    // 5. Permissions — scoped by tenant package.
    //    - Admins → all menu perms within package range.
    //    - Non-admins → role-assigned perms ∩ package range.
    //    - No tenant binding → empty list.
    //    Both resolve_all_menu_perms and resolve_role_permissions already
    //    LEFT JOIN sys_tenant_package and filter by menuIds.
    let permissions = match chosen_tenant_id.as_deref() {
        Some(tid) if is_admin => UserRepo::resolve_all_menu_perms(&state.pg, tid)
            .await
            .into_internal()
            .inspect(|p| {
                tracing::debug!(
                    tenant_id = %tid,
                    perm_count = p.len(),
                    "admin user granted package-scoped menu permissions"
                );
            })?,
        Some(tid) => UserRepo::resolve_role_permissions(&state.pg, &user.user_id, tid)
            .await
            .into_internal()?,
        None => Vec::new(),
    };

    // 5b. Resolve sys_code from tenant's package.
    let sys_code = match chosen_tenant_id.as_deref() {
        Some(tid) => {
            let row: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT p.code FROM sys_tenant t \
                 LEFT JOIN sys_tenant_package p ON t.package_id = p.package_id \
                   AND p.del_flag = '0' AND p.status = '0' \
                 WHERE t.tenant_id = $1 AND t.del_flag = '0'",
            )
            .bind(tid)
            .fetch_optional(&state.pg)
            .await
            .context("login: resolve sys_code")
            .into_internal()?;
            row.and_then(|(code,)| code)
        }
        None => None,
    };

    // 6. Build session and persist under a fresh uuid.
    let session_uuid = uuid::Uuid::new_v4().to_string();
    let sess = UserSession {
        user_id: user.user_id.clone(),
        user_name: user.user_name,
        user_type: user.user_type.clone(),
        nick_name: user.nick_name,
        avatar: user.avatar,
        email: user.email,
        phonenumber: user.phonenumber,
        tenant_id: chosen_tenant_id.clone(),
        platform_id: Some(user.platform_id),
        sys_code,
        lang: user.lang,
        is_admin,
        permissions,
        roles: Vec::new(),
    };
    session::store(
        &state.redis,
        &state.config.redis_keys,
        &session_uuid,
        &sess,
        state.config.jwt.expires_in_sec as u64,
    )
    .await
    .into_internal()?;

    // 7. Sign the (thin) JWT
    let claims = JwtClaims::new(
        session_uuid,
        user.user_id,
        chosen_tenant_id,
        user.user_type,
        None,
        state.config.jwt.expires_in_sec,
    );
    let token = jwt::encode_token(&claims, &state.config.jwt)?;

    tracing::info!(
        username = %sess.user_name,
        user_id = %sess.user_id,
        "login success"
    );

    Ok(LoginTokenResponseDto {
        access_token: token.clone(),
        refresh_token: Some(token),
        expire_in: state.config.jwt.expires_in_sec,
        refresh_expire_in: Some(state.config.jwt.refresh_expires_in_sec),
        client_id: None,
        scope: None,
        openid: None,
    })
}

#[tracing::instrument(skip_all)]
pub async fn get_captcha(state: &AppState) -> Result<CaptchaCodeResponseDto, AppError> {
    let code = captcha::generate_and_store(
        &state.redis,
        &state.config.redis_keys,
        &state.config.redis_ttl,
    )
    .await
    .into_internal()?;
    Ok(CaptchaCodeResponseDto {
        uuid: code.uuid,
        img: code.image,
    })
}

#[tracing::instrument(skip_all, fields(uuid = %claims.uuid))]
pub async fn logout(state: &AppState, claims: &JwtClaims) -> Result<(), AppError> {
    // Delete the session so any concurrent request returns TOKEN_EXPIRED.
    session::delete(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?;
    // Add to single-token blacklist (belt + braces for in-flight tokens).
    session::blacklist(
        &state.redis,
        &state.config.redis_keys,
        &claims.uuid,
        state.config.redis_ttl.token_blacklist,
    )
    .await
    .into_internal()?;
    Ok(())
}

#[tracing::instrument(skip_all, fields(user_id = %session.user_id))]
pub async fn get_info(
    _state: &AppState,
    session: &UserSession,
) -> Result<CurrentUserInfoResponseDto, AppError> {
    // All profile fields are stored in the Redis session (set at login),
    // so no DB query needed. Trade-off: profile edits require re-login
    // or session refresh to take effect.
    Ok(CurrentUserInfoResponseDto {
        user: UserProfileDto {
            user_id: session.user_id.clone(),
            user_name: session.user_name.clone(),
            nick_name: session.nick_name.clone(),
            avatar: session.avatar.clone(),
            email: session.email.clone(),
            phonenumber: session.phonenumber.clone(),
            user_type: session.user_type.clone(),
            tenant_id: session.tenant_id.clone(),
            platform_id: session.platform_id.clone(),
            is_admin: session.is_admin,
        },
        roles: session.roles.clone(),
        permissions: session.permissions.clone(),
    })
}

/// Return the list of active tenants for the login page dropdown.
/// Public — no auth needed.
#[tracing::instrument(skip_all)]
pub async fn tenant_list(state: &AppState) -> Result<TenantListForLoginDto, AppError> {
    if !state.config.tenant.enabled {
        return Ok(TenantListForLoginDto {
            tenant_enabled: false,
            vo_list: Vec::new(),
        });
    }

    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT tenant_id, company_name, domain FROM sys_tenant \
         WHERE status = '0' AND del_flag = '0' \
         ORDER BY create_at ASC LIMIT 500",
    )
    .fetch_all(&state.pg)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!("tenant_list: {e}")))?;

    let vo_list = rows
        .into_iter()
        .map(|(tenant_id, company_name, domain)| TenantVo {
            tenant_id,
            company_name,
            domain,
        })
        .collect();

    Ok(TenantListForLoginDto {
        tenant_enabled: true,
        vo_list,
    })
}

/// Refresh an access token. Validates the old token, invalidates it, and
/// issues a new one with recalculated permissions.
#[tracing::instrument(skip_all)]
pub async fn refresh_token(
    state: &AppState,
    dto: RefreshTokenDto,
) -> Result<LoginTokenResponseDto, AppError> {
    // 1. Decode the refresh token JWT
    let claims = jwt::decode_token(&dto.refresh_token, &state.config.jwt)?;

    // 2. Check single-token blacklist
    let blacklisted = session::is_blacklisted(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?;
    if blacklisted {
        return Err(AppError::auth(ResponseCode::TOKEN_INVALID));
    }

    // 3. Check user-level token version
    let current_version =
        session::get_user_token_version(&state.redis, &state.config.redis_keys, &claims.user_id)
            .await
            .into_internal()?;
    if let (Some(current), Some(token_ver)) = (current_version, claims.token_version) {
        if token_ver < current {
            return Err(AppError::auth(ResponseCode::TOKEN_INVALID));
        }
    }

    // 4. Fetch old session from Redis
    let old_session = session::fetch(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?
        .ok_or_else(|| AppError::auth(ResponseCode::TOKEN_EXPIRED))?;

    // 5. Delete old session + blacklist old token
    session::delete(&state.redis, &state.config.redis_keys, &claims.uuid)
        .await
        .into_internal()?;
    session::blacklist(
        &state.redis,
        &state.config.redis_keys,
        &claims.uuid,
        state.config.redis_ttl.token_blacklist,
    )
    .await
    .into_internal()?;

    // 6. Recalculate permissions
    let (permissions, is_admin) = match old_session.tenant_id.as_deref() {
        Some(tid) if old_session.is_admin => {
            let perms = UserRepo::resolve_all_menu_perms(&state.pg, tid)
                .await
                .into_internal()?;
            (perms, true)
        }
        Some(tid) => {
            let perms = UserRepo::resolve_role_permissions(&state.pg, &old_session.user_id, tid)
                .await
                .into_internal()?;
            (perms, false)
        }
        None => (Vec::new(), false),
    };

    // 7. Build new session + new JWT
    let new_uuid = uuid::Uuid::new_v4().to_string();
    let new_session = UserSession {
        user_id: old_session.user_id.clone(),
        user_name: old_session.user_name,
        user_type: old_session.user_type.clone(),
        nick_name: old_session.nick_name,
        avatar: old_session.avatar,
        email: old_session.email,
        phonenumber: old_session.phonenumber,
        tenant_id: old_session.tenant_id.clone(),
        platform_id: old_session.platform_id,
        sys_code: old_session.sys_code,
        lang: old_session.lang,
        is_admin,
        permissions,
        roles: old_session.roles,
    };

    // 8. Store new session in Redis
    session::store(
        &state.redis,
        &state.config.redis_keys,
        &new_uuid,
        &new_session,
        state.config.jwt.expires_in_sec as u64,
    )
    .await
    .into_internal()?;

    // 9. Sign the new JWT
    let new_claims = JwtClaims::new(
        new_uuid,
        old_session.user_id,
        old_session.tenant_id,
        old_session.user_type,
        current_version,
        state.config.jwt.expires_in_sec,
    );
    let token = jwt::encode_token(&new_claims, &state.config.jwt)?;

    Ok(LoginTokenResponseDto {
        access_token: token.clone(),
        refresh_token: Some(token),
        expire_in: state.config.jwt.expires_in_sec,
        refresh_expire_in: Some(state.config.jwt.refresh_expires_in_sec),
        client_id: None,
        scope: None,
        openid: None,
    })
}

// ---------------------------------------------------------------------------
// GET /routers — dynamic menu tree for the current user
// ---------------------------------------------------------------------------

/// Fetch the router menu tree for the current user.
#[tracing::instrument(skip_all)]
pub async fn get_routers(
    state: &AppState,
    session: &UserSession,
) -> Result<Vec<RouterConfig>, AppError> {
    let tenant_id = session.tenant_id.as_deref().unwrap_or_default();
    let user_id = &session.user_id;

    let menus = if session.is_admin {
        MenuRepo::find_user_routers_admin(&state.pg, tenant_id)
            .await
            .into_internal()?
    } else {
        MenuRepo::find_user_routers(&state.pg, user_id, tenant_id)
            .await
            .into_internal()?
    };

    Ok(build_menus(&menus))
}

// ---------------------------------------------------------------------------
// Menu tree builder — faithfully translated from NestJS `menu/utils.ts`
// ---------------------------------------------------------------------------

/// Menu type constants matching NestJS `user.constant.ts`.
const TYPE_DIR: &str = "M";
const TYPE_MENU: &str = "C";
const NO_FRAME: &str = "1";
const LAYOUT: &str = "Layout";
const INNER_LINK: &str = "InnerLink";
const PARENT_VIEW: &str = "ParentView";

/// Capitalize the first character of `s` (ASCII-only, matching NestJS behavior).
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => {
            let upper: String = c.to_uppercase().collect();
            let rest: String = chars.collect::<String>().to_lowercase();
            format!("{upper}{rest}")
        }
    }
}

/// Check if `path` is a URL (starts with `http://` or `https://`).
fn is_url(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

/// Replace inner link special characters (matching NestJS `innerLinkReplaceEach`).
/// Strips protocol + `www.` prefix, then replaces `.` and `:` with `/`.
fn inner_link_replace_each(path: &str) -> String {
    // Strip protocol
    let s = path
        .strip_prefix("https://")
        .or_else(|| path.strip_prefix("http://"))
        .unwrap_or(path);
    // Strip www.
    let s = s.strip_prefix("www.").unwrap_or(s);
    // Replace . and : with /
    s.chars()
        .map(|c| if c == '.' || c == ':' { '/' } else { c })
        .collect()
}

/// Pre-computed context for a menu item, avoiding repeated boolean checks.
struct MenuContext {
    is_root: bool,
    is_dir: bool,
    #[allow(dead_code)]
    is_menu: bool,
    is_no_frame: bool,
    /// Root + menu + noFrame → special frame route
    is_frame: bool,
    /// NoFrame + path is URL → inner link
    is_inner: bool,
    /// Non-root + directory → ParentView
    is_parent_view: bool,
    meta: RouterMeta,
}

fn compute_menu_context(m: &RouterMenuItem) -> MenuContext {
    let is_root = m
        .parent_id
        .as_deref()
        .is_none_or(|p| p.is_empty() || p == "0");
    let is_dir = m.menu_type == TYPE_DIR;
    let is_menu = m.menu_type == TYPE_MENU;
    let is_no_frame = m.is_frame == NO_FRAME;

    let is_frame = is_root && is_menu && is_no_frame;
    let is_inner = is_no_frame && is_url(&m.path);
    let is_parent_view = !is_root && is_dir;

    let meta = RouterMeta {
        title: m.menu_name.clone(),
        icon: m.icon.clone(),
        no_cache: m.is_cache == "1",
        link: None, // DB has no `link` column; leave None
    };

    MenuContext {
        is_root,
        is_dir,
        is_menu,
        is_no_frame,
        is_frame,
        is_inner,
        is_parent_view,
        meta,
    }
}

/// Determine the component string (NestJS `getComponent`).
fn get_component(m: &RouterMenuItem, ctx: &MenuContext) -> String {
    if m.component.is_some() && !ctx.is_frame {
        return m.component.clone().unwrap_or_default();
    }
    if m.component.is_none() && ctx.is_inner {
        return INNER_LINK.to_string();
    }
    if m.component.is_none() && ctx.is_parent_view {
        return PARENT_VIEW.to_string();
    }
    LAYOUT.to_string()
}

/// Determine the router path (NestJS `getRouterPath`).
fn get_router_path(m: &RouterMenuItem, ctx: &MenuContext) -> String {
    if ctx.is_frame {
        return "/".to_string();
    }
    if ctx.is_root && ctx.is_inner {
        return inner_link_replace_each(&m.path);
    }
    if ctx.is_root && ctx.is_dir && ctx.is_no_frame {
        return format!("/{}", m.path);
    }
    m.path.clone()
}

/// Convert a flat list of `RouterMenuItem` into a tree of `RouterConfig`.
///
/// Algorithm (matches NestJS `buildMenus` in `menu/utils.ts`):
/// 1. First pass: build all router nodes and record pending parent-child links.
/// 2. Second pass: attach children to parents.
fn build_menus(menus: &[RouterMenuItem]) -> Vec<RouterConfig> {
    if menus.is_empty() {
        return Vec::new();
    }

    let mut routers: Vec<RouterConfig> = Vec::with_capacity(menus.len());
    let mut id_to_idx: HashMap<&str, usize> = HashMap::with_capacity(menus.len());
    // (child_index, parent_id) pairs for non-root items
    let mut pending_children: Vec<(usize, &str)> = Vec::new();
    let mut root_indices: Vec<usize> = Vec::new();

    // First pass: build all nodes
    for (i, m) in menus.iter().enumerate() {
        let ctx = compute_menu_context(m);

        let mut router = RouterConfig {
            hidden: m.visible == "1",
            name: if ctx.is_frame {
                String::new()
            } else {
                capitalize(&m.path)
            },
            path: get_router_path(m, &ctx),
            component: get_component(m, &ctx),
            query: m.query.clone(),
            meta: Some(ctx.meta),
            always_show: None,
            redirect: None,
            children: None,
        };

        // Special case: frame route (root + menu + noFrame)
        if ctx.is_frame {
            let child_meta = RouterMeta {
                title: m.menu_name.clone(),
                icon: m.icon.clone(),
                no_cache: m.is_cache == "1",
                link: None,
            };
            router.meta = None;
            router.children = Some(vec![RouterConfig {
                hidden: false,
                name: capitalize(&m.path),
                path: m.path.clone(),
                component: m.component.clone().unwrap_or_default(),
                query: m.query.clone(),
                meta: Some(child_meta),
                always_show: None,
                redirect: None,
                children: None,
            }]);
        } else if ctx.is_root && ctx.is_inner {
            // Inner link at root
            let inner_path = inner_link_replace_each(&m.path);
            router.meta = Some(RouterMeta {
                title: m.menu_name.clone(),
                icon: m.icon.clone(),
                no_cache: false,
                link: None,
            });
            router.path = "/".to_string();
            router.children = Some(vec![RouterConfig {
                hidden: false,
                name: capitalize(&m.menu_name),
                path: inner_path,
                component: INNER_LINK.to_string(),
                query: None,
                meta: Some(RouterMeta {
                    title: m.menu_name.clone(),
                    icon: m.icon.clone(),
                    no_cache: false,
                    link: Some(m.path.clone()),
                }),
                always_show: None,
                redirect: None,
                children: None,
            }]);
        } else if ctx.is_dir {
            // Directory: initialize empty children (marks it as a directory)
            router.children = Some(Vec::new());
        }

        routers.push(router);
        id_to_idx.insert(&m.menu_id, i);

        if ctx.is_root {
            root_indices.push(i);
        } else {
            let parent_id = m.parent_id.as_deref().unwrap_or("");
            pending_children.push((i, parent_id));
        }
    }

    // Second pass: attach children to parents.
    // We collect assignments first, then apply them using Option slots.
    let mut child_assignments: Vec<(usize, usize)> = Vec::new();
    let mut orphan_indices: Vec<usize> = Vec::new();

    for &(child_idx, parent_id) in &pending_children {
        if let Some(&parent_idx) = id_to_idx.get(parent_id) {
            // Check if parent is a directory (has children Some)
            if routers[parent_idx].children.is_some() {
                child_assignments.push((parent_idx, child_idx));
            }
            // else: parent exists but isn't a directory — do not attach
        } else {
            // Parent not found — treat as root
            orphan_indices.push(child_idx);
        }
    }

    // Convert to Option<RouterConfig> for easy take semantics
    let mut slots: Vec<Option<RouterConfig>> = routers.into_iter().map(Some).collect();

    for (parent_idx, child_idx) in &child_assignments {
        if let Some(child) = slots[*child_idx].take() {
            if let Some(ref mut parent) = slots[*parent_idx] {
                let children = parent.children.get_or_insert_with(Vec::new);
                // Set alwaysShow + redirect on first child added
                if children.is_empty() {
                    parent.always_show = Some(true);
                    parent.redirect = Some("noRedirect".to_string());
                }
                children.push(child);
            }
        }
    }

    // Build result: root items + orphans (in original order)
    let mut result: Vec<RouterConfig> = Vec::new();
    for idx in root_indices {
        if let Some(router) = slots[idx].take() {
            result.push(router);
        }
    }
    for idx in orphan_indices {
        if let Some(router) = slots[idx].take() {
            result.push(router);
        }
    }

    result
}

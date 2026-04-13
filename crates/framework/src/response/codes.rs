//! Business response codes.
//!
//! Modeled as a newtype around `i32` so new codes can be added without
//! touching a giant enum match. Aligned with the ranges from
//! `server/src/shared/response/response.interface.ts`:
//!
//! - 200        : Success
//! - 400-499    : HTTP client errors (validation, auth, forbidden)
//! - 500-599    : HTTP server errors
//! - 1000-1029  : General business errors
//! - 2000-2039  : Auth errors
//! - 3000-3029  : User errors
//! - 4000-4029  : Tenant errors
//! - 5000-5039  : File errors
//! - 6000-6009  : Third-party errors
//! - 7000-7099  : System module errors
//! - 7100-7199  : Message module errors

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResponseCode(pub i32);

#[allow(non_upper_case_globals)]
impl ResponseCode {
    // --- HTTP-aligned ---
    pub const SUCCESS: Self = Self(200);
    pub const BAD_REQUEST: Self = Self(400);
    pub const UNAUTHORIZED: Self = Self(401);
    pub const FORBIDDEN: Self = Self(403);
    pub const TOO_MANY_REQUESTS: Self = Self(429);
    pub const INTERNAL_SERVER_ERROR: Self = Self(500);

    // --- 1000-1029 general business ---
    pub const PARAM_INVALID: Self = Self(1000);
    pub const DATA_NOT_FOUND: Self = Self(1001);
    pub const DUPLICATE_KEY: Self = Self(1002);
    pub const OPTIMISTIC_LOCK_CONFLICT: Self = Self(1003);
    pub const OPERATION_NOT_ALLOWED: Self = Self(1004);

    // --- 2000-2039 auth ---
    pub const TOKEN_INVALID: Self = Self(2001);
    pub const TOKEN_EXPIRED: Self = Self(2002);
    pub const ACCOUNT_LOCKED: Self = Self(2003);
    pub const CAPTCHA_INVALID: Self = Self(2004);

    // --- 3000-3029 user ---
    pub const USER_NOT_FOUND: Self = Self(3001);
    pub const INVALID_CREDENTIALS: Self = Self(3002);

    // --- 4000-4029 tenant ---
    pub const TENANT_NOT_FOUND: Self = Self(4001);
    pub const TENANT_EXPIRED: Self = Self(4002);
    pub const TENANT_PROTECTED: Self = Self(4010);
    pub const TENANT_COMPANY_EXISTS: Self = Self(4013);
    pub const TENANT_PARENT_NOT_FOUND: Self = Self(4014);
    pub const TENANT_HAS_CHILDREN: Self = Self(4016);
    pub const TENANT_PACKAGE_NOT_FOUND: Self = Self(4020);
    pub const TENANT_PACKAGE_CODE_EXISTS: Self = Self(4021);
    pub const TENANT_PACKAGE_NAME_EXISTS: Self = Self(4022);
    pub const TENANT_PACKAGE_IN_USE: Self = Self(4023);

    // --- 7000-7099 system module ---
    pub const DEPT_NOT_FOUND: Self = Self(7010);
    pub const DEPT_PARENT_NOT_FOUND: Self = Self(7014);
    pub const DEPT_NESTING_TOO_DEEP: Self = Self(7015);
    pub const MENU_NOT_FOUND: Self = Self(7020);
    pub const DICT_TYPE_NOT_FOUND: Self = Self(7030);
    pub const DICT_TYPE_EXISTS: Self = Self(7031);
    pub const DICT_DATA_NOT_FOUND: Self = Self(7035);
    pub const DICT_DATA_EXISTS: Self = Self(7036);
    pub const CONFIG_NOT_FOUND: Self = Self(7040);
    pub const CONFIG_KEY_EXISTS: Self = Self(7041);
    pub const POST_NOT_FOUND: Self = Self(7050);
    pub const POST_CODE_EXISTS: Self = Self(7051);
    pub const POST_NAME_EXISTS: Self = Self(7052);
    pub const NOTICE_NOT_FOUND: Self = Self(7060);

    // --- 7100-7199 message module ---
    pub const NOTIFY_TEMPLATE_NOT_FOUND: Self = Self(7110);
    pub const NOTIFY_TEMPLATE_CODE_EXISTS: Self = Self(7111);
    pub const NOTIFY_MESSAGE_NOT_FOUND: Self = Self(7120);
    pub const MAIL_ACCOUNT_NOT_FOUND: Self = Self(7130);
    pub const MAIL_ACCOUNT_EXISTS: Self = Self(7131);
    pub const MAIL_TEMPLATE_NOT_FOUND: Self = Self(7140);
    pub const MAIL_TEMPLATE_CODE_EXISTS: Self = Self(7141);
    pub const SMS_CHANNEL_NOT_FOUND: Self = Self(7150);
    pub const SMS_CHANNEL_CODE_EXISTS: Self = Self(7151);
    pub const SMS_TEMPLATE_NOT_FOUND: Self = Self(7160);
    pub const SMS_TEMPLATE_CODE_EXISTS: Self = Self(7161);

    // --- 3030-3039 user profile ---
    pub const OLD_PASSWORD_INCORRECT: Self = Self(3030);

    // --- 4030-4039 tenant switch ---
    pub const TENANT_BINDING_NOT_FOUND: Self = Self(4030);

    // --- 7200-7299 monitor module ---
    pub const OPER_LOG_NOT_FOUND: Self = Self(7200);
    pub const LOGIN_LOG_NOT_FOUND: Self = Self(7210);
    pub const AUDIT_LOG_NOT_FOUND: Self = Self(7220);

    pub const fn as_i32(self) -> i32 {
        self.0
    }

    pub const fn is_success(self) -> bool {
        self.0 == 200
    }
}

impl std::fmt::Display for ResponseCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<ResponseCode> for i32 {
    fn from(code: ResponseCode) -> Self {
        code.0
    }
}

impl From<i32> for ResponseCode {
    fn from(v: i32) -> Self {
        Self(v)
    }
}

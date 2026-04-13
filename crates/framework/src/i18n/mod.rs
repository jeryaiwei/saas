//! Runtime i18n lookup for response messages.
//!
//! Messages are loaded at compile time via `include_str!` from the
//! workspace-level `i18n/{lang}.json` files and parsed once on first use.
//! Subsequent lookups are pure hash-map reads.
//!
//! Mirrors NestJS `getResponseMessageI18n(code, lang, params)`. Supports
//! `{placeholder}` substitution (e.g. `"Account locked for {minutes} minutes"`).

use crate::response::ResponseCode;
use once_cell::sync::Lazy;
use std::collections::HashMap;

static ZH_CN_SRC: &str = include_str!("../../../../i18n/zh-CN.json");
static EN_US_SRC: &str = include_str!("../../../../i18n/en-US.json");

pub const DEFAULT_LANG: &str = "zh-CN";

type MessageMap = HashMap<String, String>;

static MESSAGES: Lazy<HashMap<&'static str, MessageMap>> = Lazy::new(|| {
    let mut m: HashMap<&'static str, MessageMap> = HashMap::new();
    m.insert(
        "zh-CN",
        serde_json::from_str(ZH_CN_SRC).expect("parse i18n/zh-CN.json"),
    );
    m.insert(
        "en-US",
        serde_json::from_str(EN_US_SRC).expect("parse i18n/en-US.json"),
    );
    m
});

pub fn get_message(code: ResponseCode, lang: &str) -> String {
    let empty = HashMap::new();
    get_message_with_params(code, lang, &empty)
}

/// Look up an arbitrary string key in the i18n map (e.g. `"valid.length"`,
/// `"valid.status_flag"`). Returns `None` when the key is missing in both
/// the requested language AND the default language.
///
/// Unlike [`get_message`] (which always falls back to `[{code}]` when the
/// numeric key is missing), this function returns `None` for unknown keys
/// so the caller can decide whether to emit a warning, use a default, or
/// pass the raw key back to the client.
pub fn get_by_key(key: &str, lang: &str) -> Option<String> {
    MESSAGES
        .get(lang)
        .and_then(|m| m.get(key))
        .or_else(|| MESSAGES.get(DEFAULT_LANG).and_then(|m| m.get(key)))
        .cloned()
}

/// Variant of `get_by_key` accepting validator-style params
/// (`HashMap<Cow<'static, str>, serde_json::Value>`). Unwraps JSON
/// numbers and strings into their raw string form before substitution,
/// so `{min}` becomes `1` rather than `1.0` or `"1"`. Used by the
/// `AppError::Validation → IntoResponse` path to surface the actual
/// min/max bounds to the client instead of generic "out of range".
///
/// See `docs/framework/framework-pagination-spec.md` §7.1 (v1.1) and
/// `docs/framework/framework-error-envelope-spec.md` §5.2.
pub fn get_by_key_with_json_params(
    key: &str,
    lang: &str,
    params: &std::collections::HashMap<std::borrow::Cow<'static, str>, serde_json::Value>,
) -> Option<String> {
    let raw = get_by_key(key, lang)?;
    if params.is_empty() {
        return Some(raw);
    }
    let mut out = raw;
    for (k, v) in params {
        let s = match v {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Number(n) => n.to_string(),
            other => other.to_string(),
        };
        out = out.replace(&format!("{{{}}}", k.as_ref()), &s);
    }
    Some(out)
}

pub fn get_message_with_params(
    code: ResponseCode,
    lang: &str,
    params: &HashMap<&str, String>,
) -> String {
    let key = code.as_i32().to_string();
    let raw = match MESSAGES
        .get(lang)
        .and_then(|m| m.get(&key))
        .or_else(|| MESSAGES.get(DEFAULT_LANG).and_then(|m| m.get(&key)))
    {
        Some(s) => s.clone(),
        None => {
            // Missing translation is a bug — log it so it's visible in
            // production traces, then fall back to a debug-printable
            // sentinel so the client still sees *something* and the bug
            // doesn't crash the response pipeline.
            tracing::warn!(
                code = code.as_i32(),
                lang = %lang,
                "missing i18n entry for response code; falling back to [{code}] sentinel"
            );
            format!("[{}]", code.as_i32())
        }
    };

    if params.is_empty() {
        return raw;
    }

    let mut out = raw;
    for (k, v) in params {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_zh_cn() {
        assert_eq!(get_message(ResponseCode::SUCCESS, "zh-CN"), "操作成功");
    }

    #[test]
    fn lookup_en_us() {
        assert_eq!(get_message(ResponseCode::SUCCESS, "en-US"), "OK");
    }

    #[test]
    fn fallback_to_default_lang_when_unknown_lang() {
        assert_eq!(get_message(ResponseCode::SUCCESS, "fr-FR"), "操作成功");
    }

    #[test]
    fn fallback_to_display_when_unknown_code() {
        let got = get_message(ResponseCode(999999), "zh-CN");
        assert_eq!(got, "[999999]");
    }

    #[test]
    fn placeholder_substitution() {
        let mut params = HashMap::new();
        params.insert("minutes", "15".to_string());
        let got = get_message_with_params(ResponseCode::ACCOUNT_LOCKED, "en-US", &params);
        assert_eq!(got, "Account locked, retry in 15 minutes");
    }

    #[test]
    fn placeholder_substitution_zh_cn() {
        let mut params = HashMap::new();
        params.insert("minutes", "5".to_string());
        let got = get_message_with_params(ResponseCode::ACCOUNT_LOCKED, "zh-CN", &params);
        assert_eq!(got, "账号已锁定，请 5 分钟后重试");
    }

    #[test]
    fn get_by_key_missing_returns_none() {
        assert!(get_by_key("valid.definitely_not_a_real_key", "zh-CN").is_none());
    }

    #[test]
    fn get_by_key_falls_back_to_default_lang() {
        // Using a known business-code key — since both JSON files have it,
        // this exercises the "requested lang missing, default lang has it"
        // branch via a made-up lang code.
        assert_eq!(
            get_by_key("200", "fr-FR"),
            Some("操作成功".to_string()) // zh-CN entry for code 200
        );
    }

    #[test]
    fn get_by_key_with_json_params_substitutes_json_numbers() {
        use std::borrow::Cow;
        let mut params = std::collections::HashMap::new();
        params.insert(Cow::Borrowed("min"), serde_json::json!(1));
        params.insert(Cow::Borrowed("max"), serde_json::json!(200));

        let got = get_by_key_with_json_params("valid.range", "zh-CN", &params)
            .expect("valid.range must exist");
        assert!(got.contains("1"), "expected min=1 in result: {}", got);
        assert!(got.contains("200"), "expected max=200 in result: {}", got);
        assert!(
            !got.contains("{min}"),
            "placeholder should be substituted: {}",
            got
        );
        assert!(
            !got.contains("{max}"),
            "placeholder should be substituted: {}",
            got
        );
    }

    #[test]
    fn get_by_key_with_json_params_empty_params_returns_raw() {
        let params = std::collections::HashMap::new();
        let got = get_by_key_with_json_params("200", "zh-CN", &params).unwrap();
        assert_eq!(got, "操作成功");
    }

    #[test]
    fn every_response_code_has_i18n_entries_in_all_langs() {
        // Explicit list of every ResponseCode constant defined in
        // `framework/src/response/codes.rs`. When adding a new const,
        // YOU MUST add it here AND add matching entries to both i18n
        // JSON files. See `docs/framework/framework-error-envelope-spec.md` §5.3.
        let codes: &[ResponseCode] = &[
            // HTTP-aligned
            ResponseCode::SUCCESS,
            ResponseCode::BAD_REQUEST,
            ResponseCode::UNAUTHORIZED,
            ResponseCode::FORBIDDEN,
            ResponseCode::TOO_MANY_REQUESTS,
            ResponseCode::INTERNAL_SERVER_ERROR,
            // 1000-1029 general business
            ResponseCode::PARAM_INVALID,
            ResponseCode::DATA_NOT_FOUND,
            ResponseCode::DUPLICATE_KEY,
            ResponseCode::OPTIMISTIC_LOCK_CONFLICT,
            ResponseCode::OPERATION_NOT_ALLOWED,
            // 2000-2039 auth
            ResponseCode::TOKEN_INVALID,
            ResponseCode::TOKEN_EXPIRED,
            ResponseCode::ACCOUNT_LOCKED,
            ResponseCode::CAPTCHA_INVALID,
            // 3000-3029 user
            ResponseCode::USER_NOT_FOUND,
            ResponseCode::INVALID_CREDENTIALS,
            // 4000-4029 tenant
            ResponseCode::TENANT_NOT_FOUND,
            ResponseCode::TENANT_EXPIRED,
            ResponseCode::TENANT_PROTECTED,
            ResponseCode::TENANT_COMPANY_EXISTS,
            ResponseCode::TENANT_PARENT_NOT_FOUND,
            ResponseCode::TENANT_HAS_CHILDREN,
            ResponseCode::TENANT_PACKAGE_NOT_FOUND,
            ResponseCode::TENANT_PACKAGE_CODE_EXISTS,
            ResponseCode::TENANT_PACKAGE_NAME_EXISTS,
            ResponseCode::TENANT_PACKAGE_IN_USE,
            // 3030-3039 user profile
            ResponseCode::OLD_PASSWORD_INCORRECT,
            // 4030-4039 tenant switch
            ResponseCode::TENANT_BINDING_NOT_FOUND,
            // 5000-5039 file
            ResponseCode::FOLDER_NOT_FOUND,
            ResponseCode::FOLDER_HAS_SUBFOLDERS,
            ResponseCode::FILE_NOT_FOUND,
            ResponseCode::SHARE_NOT_FOUND,
            // 7000-7099 system module
            ResponseCode::DEPT_NOT_FOUND,
            ResponseCode::DEPT_PARENT_NOT_FOUND,
            ResponseCode::DEPT_NESTING_TOO_DEEP,
            ResponseCode::MENU_NOT_FOUND,
            ResponseCode::DICT_TYPE_NOT_FOUND,
            ResponseCode::DICT_TYPE_EXISTS,
            ResponseCode::DICT_DATA_NOT_FOUND,
            ResponseCode::DICT_DATA_EXISTS,
            ResponseCode::CONFIG_NOT_FOUND,
            ResponseCode::CONFIG_KEY_EXISTS,
            ResponseCode::POST_NOT_FOUND,
            ResponseCode::POST_CODE_EXISTS,
            ResponseCode::POST_NAME_EXISTS,
            ResponseCode::NOTICE_NOT_FOUND,
            // 7100-7199 message module
            ResponseCode::NOTIFY_TEMPLATE_NOT_FOUND,
            ResponseCode::NOTIFY_TEMPLATE_CODE_EXISTS,
            ResponseCode::NOTIFY_MESSAGE_NOT_FOUND,
            ResponseCode::MAIL_ACCOUNT_NOT_FOUND,
            ResponseCode::MAIL_ACCOUNT_EXISTS,
            ResponseCode::MAIL_TEMPLATE_NOT_FOUND,
            ResponseCode::MAIL_TEMPLATE_CODE_EXISTS,
            ResponseCode::SMS_CHANNEL_NOT_FOUND,
            ResponseCode::SMS_CHANNEL_CODE_EXISTS,
            ResponseCode::SMS_TEMPLATE_NOT_FOUND,
            ResponseCode::SMS_TEMPLATE_CODE_EXISTS,
            // 7170-7192 mail/sms send
            ResponseCode::MAIL_TEMPLATE_PARAMS_MISSING,
            ResponseCode::MAIL_SEND_FAIL,
            ResponseCode::SMS_CHANNEL_NOT_SUPPORTED,
            ResponseCode::SMS_TEMPLATE_PARAMS_MISSING,
            ResponseCode::BATCH_SIZE_EXCEEDED,
            ResponseCode::SEND_LOG_NOT_FOUND,
            ResponseCode::SEND_LOG_NOT_FAILED,
            // 7200-7299 monitor module
            ResponseCode::OPER_LOG_NOT_FOUND,
            ResponseCode::LOGIN_LOG_NOT_FOUND,
            ResponseCode::AUDIT_LOG_NOT_FOUND,
        ];

        for code in codes {
            for lang in ["zh-CN", "en-US"] {
                let msg = get_message(*code, lang);
                assert!(
                    !msg.starts_with('['),
                    "missing i18n entry for {:?} in {}: got fallback sentinel {:?}",
                    code,
                    lang,
                    msg
                );
                assert!(
                    !msg.is_empty(),
                    "empty i18n entry for {:?} in {}",
                    code,
                    lang
                );
            }
        }
    }
}

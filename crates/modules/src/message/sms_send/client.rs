//! SMS client abstraction — trait + factory + per-provider implementations.

use async_trait::async_trait;
use std::collections::HashMap;
use std::time::Duration;

use framework::error::AppError;
use framework::response::ResponseCode;

/// Shared HTTP client for SMS API calls (connection pooling + timeout).
static HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(5))
        .pool_max_idle_per_host(5)
        .build()
        .expect("build reqwest client")
});

#[derive(Debug, Clone)]
pub struct SmsSendParams {
    pub mobile: String,
    pub signature: String,
    pub api_template_id: String,
    pub params: HashMap<String, String>,
}

#[derive(Debug)]
pub struct SmsSendResult {
    pub success: bool,
    pub api_send_code: Option<String>,
    pub error_msg: Option<String>,
}

#[async_trait]
pub trait SmsClient: Send + Sync {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult;
}

/// Create an SMS client by channel code.
pub fn create_client(
    channel_code: &str,
    api_key: &str,
    api_secret: &str,
) -> Result<Box<dyn SmsClient>, AppError> {
    match channel_code {
        "aliyun" => Ok(Box::new(AliyunSmsClient::new(api_key, api_secret))),
        "tencent" => Ok(Box::new(TencentSmsClient)),
        "huawei" => Ok(Box::new(HuaweiSmsClient)),
        _ => Err(AppError::business(ResponseCode::SMS_CHANNEL_NOT_SUPPORTED)),
    }
}

// ---------------------------------------------------------------------------
// Aliyun SMS — real implementation
// ---------------------------------------------------------------------------

struct AliyunSmsClient {
    api_key: String,
    api_secret: String,
}

impl AliyunSmsClient {
    fn new(api_key: &str, api_secret: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            api_secret: api_secret.to_string(),
        }
    }
}

#[async_trait]
impl SmsClient for AliyunSmsClient {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult {
        match self.do_send(&params).await {
            Ok(biz_id) => SmsSendResult {
                success: true,
                api_send_code: Some(biz_id),
                error_msg: None,
            },
            Err(e) => SmsSendResult {
                success: false,
                api_send_code: None,
                error_msg: Some(e),
            },
        }
    }
}

impl AliyunSmsClient {
    async fn do_send(&self, params: &SmsSendParams) -> Result<String, String> {
        let template_param =
            serde_json::to_string(&params.params).map_err(|e| format!("serialize params: {e}"))?;

        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let nonce = uuid::Uuid::new_v4().to_string();

        // Build sorted parameter map (everything except Signature itself).
        let mut query: Vec<(&str, String)> = vec![
            ("AccessKeyId", self.api_key.clone()),
            ("Action", "SendSms".into()),
            ("Format", "JSON".into()),
            ("PhoneNumbers", params.mobile.clone()),
            ("SignName", params.signature.clone()),
            ("SignatureMethod", "HMAC-SHA1".into()),
            ("SignatureNonce", nonce),
            ("SignatureVersion", "1.0".into()),
            ("TemplateCode", params.api_template_id.clone()),
            ("TemplateParam", template_param),
            ("Timestamp", timestamp),
            ("Version", "2017-05-25".into()),
        ];
        query.sort_by(|a, b| a.0.cmp(b.0));

        // Canonical query string.
        let canonical_query: String = query
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        // String to sign.
        let string_to_sign = format!("GET&{}&{}", percent_encode("/"), percent_encode(&canonical_query));

        // HMAC-SHA1 signature.
        let signature = sign(&string_to_sign, &self.api_secret);

        // Full URL.
        let url = format!(
            "https://dysmsapi.aliyuncs.com/?{}&Signature={}",
            canonical_query,
            percent_encode(&signature)
        );

        // Send HTTP request.
        let resp = HTTP_CLIENT
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("http request failed: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("parse response failed: {e}"))?;

        if body["Code"].as_str() == Some("OK") {
            Ok(body["BizId"].as_str().unwrap_or("").to_string())
        } else {
            Err(format!(
                "{}: {}",
                body["Code"].as_str().unwrap_or("UNKNOWN"),
                body["Message"].as_str().unwrap_or("unknown error")
            ))
        }
    }
}

/// RFC 3986 percent-encoding (Aliyun-compatible).
fn percent_encode(s: &str) -> String {
    let mut result = String::new();
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
}

/// HMAC-SHA1 signature, base64-encoded.
fn sign(string_to_sign: &str, secret: &str) -> String {
    use base64::Engine;
    use hmac::{Hmac, KeyInit, Mac};
    use sha1::Sha1;

    let signing_key = format!("{secret}&");
    let mut mac =
        Hmac::<Sha1>::new_from_slice(signing_key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(string_to_sign.as_bytes());
    let result = mac.finalize().into_bytes();
    base64::engine::general_purpose::STANDARD.encode(result)
}

// ---------------------------------------------------------------------------
// Tencent SMS — mock
// ---------------------------------------------------------------------------

struct TencentSmsClient;

#[async_trait]
impl SmsClient for TencentSmsClient {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult {
        tracing::info!(mobile = %params.mobile, provider = "tencent", "mock sms send");
        SmsSendResult {
            success: true,
            api_send_code: Some(format!("tencent-mock-{}", uuid::Uuid::new_v4())),
            error_msg: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Huawei SMS — mock
// ---------------------------------------------------------------------------

struct HuaweiSmsClient;

#[async_trait]
impl SmsClient for HuaweiSmsClient {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult {
        tracing::info!(mobile = %params.mobile, provider = "huawei", "mock sms send");
        SmsSendResult {
            success: true,
            api_send_code: Some(format!("huawei-mock-{}", uuid::Uuid::new_v4())),
            error_msg: None,
        }
    }
}

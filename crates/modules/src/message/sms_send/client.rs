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
        "tencent" => Ok(Box::new(TencentSmsClient::new(api_key, api_secret))),
        "huawei" => Ok(Box::new(HuaweiSmsClient::new(api_key, api_secret))),
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
// Tencent Cloud SMS — TC3-HMAC-SHA256 signature
// Docs: https://cloud.tencent.com/document/product/382/55981
// ---------------------------------------------------------------------------

struct TencentSmsClient {
    secret_id: String,
    secret_key: String,
}

impl TencentSmsClient {
    fn new(secret_id: &str, secret_key: &str) -> Self {
        Self {
            secret_id: secret_id.to_string(),
            secret_key: secret_key.to_string(),
        }
    }
}

#[async_trait]
impl SmsClient for TencentSmsClient {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult {
        match self.do_send(&params).await {
            Ok(serial_no) => SmsSendResult {
                success: true,
                api_send_code: Some(serial_no),
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

impl TencentSmsClient {
    async fn do_send(&self, params: &SmsSendParams) -> Result<String, String> {
        use hmac::{Hmac, KeyInit, Mac};
        use sha2::{Digest, Sha256};

        let service = "sms";
        let host = "sms.tencentcloudapi.com";
        let action = "SendSms";
        let version = "2021-01-11";

        // Build JSON payload
        let template_params: Vec<&str> = params.params.values().map(|v| v.as_str()).collect();
        let payload = serde_json::json!({
            "PhoneNumberSet": [format!("+86{}", params.mobile)],
            "SmsSdkAppId": "1400000000",  // placeholder — should come from channel config
            "SignName": params.signature,
            "TemplateId": params.api_template_id,
            "TemplateParamSet": template_params,
        });
        let payload_str = serde_json::to_string(&payload).map_err(|e| format!("json: {e}"))?;

        let now = chrono::Utc::now();
        let timestamp = now.timestamp();
        let date = now.format("%Y-%m-%d").to_string();

        // 1. CanonicalRequest
        let hashed_payload = hex::encode(Sha256::digest(payload_str.as_bytes()));
        let canonical_request = format!(
            "POST\n/\n\ncontent-type:application/json\nhost:{host}\n\ncontent-type;host\n{hashed_payload}"
        );

        // 2. StringToSign
        let credential_scope = format!("{date}/{service}/tc3_request");
        let hashed_canonical = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let string_to_sign = format!(
            "TC3-HMAC-SHA256\n{timestamp}\n{credential_scope}\n{hashed_canonical}"
        );

        // 3. Signing key chain: TC3{secret} → date → service → tc3_request
        fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
            let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC key");
            mac.update(data);
            mac.finalize().into_bytes().to_vec()
        }
        let secret_date = hmac_sha256(format!("TC3{}", self.secret_key).as_bytes(), date.as_bytes());
        let secret_service = hmac_sha256(&secret_date, service.as_bytes());
        let secret_signing = hmac_sha256(&secret_service, b"tc3_request");
        let signature = hex::encode(hmac_sha256(&secret_signing, string_to_sign.as_bytes()));

        // 4. Authorization header
        let authorization = format!(
            "TC3-HMAC-SHA256 Credential={}/{}, SignedHeaders=content-type;host, Signature={}",
            self.secret_id, credential_scope, signature
        );

        // 5. Send request
        let resp = HTTP_CLIENT
            .post(format!("https://{host}"))
            .header("Authorization", &authorization)
            .header("Content-Type", "application/json")
            .header("Host", host)
            .header("X-TC-Action", action)
            .header("X-TC-Timestamp", timestamp.to_string())
            .header("X-TC-Version", version)
            .body(payload_str)
            .send()
            .await
            .map_err(|e| format!("tencent http: {e}"))?;

        let body: serde_json::Value = resp.json().await
            .map_err(|e| format!("tencent parse: {e}"))?;

        // 6. Parse response
        if let Some(error) = body["Response"]["Error"].as_object() {
            return Err(format!(
                "{}: {}",
                error.get("Code").and_then(|v| v.as_str()).unwrap_or("UNKNOWN"),
                error.get("Message").and_then(|v| v.as_str()).unwrap_or("unknown"),
            ));
        }

        let serial_no = body["Response"]["SendStatusSet"][0]["SerialNo"]
            .as_str()
            .unwrap_or("")
            .to_string();
        Ok(serial_no)
    }
}

// ---------------------------------------------------------------------------
// Huawei Cloud SMS — WSSE PasswordDigest authentication
// Docs: https://support.huaweicloud.com/api-msgsms/sms_05_0001.html
// ---------------------------------------------------------------------------

struct HuaweiSmsClient {
    app_key: String,
    app_secret: String,
}

impl HuaweiSmsClient {
    fn new(app_key: &str, app_secret: &str) -> Self {
        Self {
            app_key: app_key.to_string(),
            app_secret: app_secret.to_string(),
        }
    }
}

#[async_trait]
impl SmsClient for HuaweiSmsClient {
    async fn send(&self, params: SmsSendParams) -> SmsSendResult {
        match self.do_send(&params).await {
            Ok(msg_id) => SmsSendResult {
                success: true,
                api_send_code: Some(msg_id),
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

impl HuaweiSmsClient {
    async fn do_send(&self, params: &SmsSendParams) -> Result<String, String> {
        use sha2::{Digest, Sha256};

        let endpoint = "https://smsapi.cn-north-4.myhuaweicloud.com:443/sms/batchSendSms/v1";

        // WSSE authentication
        let nonce = uuid::Uuid::new_v4().to_string();
        let created = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let digest_input = format!("{nonce}{created}{}", self.app_secret);
        let password_digest = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            Sha256::digest(digest_input.as_bytes()),
        );
        let wsse = format!(
            r#"UsernameToken Username="{}",PasswordDigest="{}",Nonce="{}",Created="{}""#,
            self.app_key, password_digest, nonce, created
        );

        // Build form body (Huawei uses x-www-form-urlencoded)
        let template_params = serde_json::to_string(&params.params.values().collect::<Vec<_>>())
            .map_err(|e| format!("json: {e}"))?;
        let form_body = format!(
            "from={}&to={}&templateId={}&templateParas={}",
            urlencoding::encode(&params.signature),
            urlencoding::encode(&format!("+86{}", params.mobile)),
            urlencoding::encode(&params.api_template_id),
            urlencoding::encode(&template_params),
        );

        let resp = HTTP_CLIENT
            .post(endpoint)
            .header("Authorization", r#"WSSE realm="SDP",profile="UsernameToken",type="Appkey""#)
            .header("X-WSSE", &wsse)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(form_body)
            .send()
            .await
            .map_err(|e| format!("huawei http: {e}"))?;

        let json: serde_json::Value = resp.json().await
            .map_err(|e| format!("huawei parse: {e}"))?;

        // Parse response
        let code = json["code"].as_str().unwrap_or("");
        if code == "000000" {
            let msg_id = json["result"][0]["smsMsgId"]
                .as_str()
                .unwrap_or("")
                .to_string();
            Ok(msg_id)
        } else {
            Err(format!(
                "{}: {}",
                code,
                json["description"].as_str().unwrap_or("unknown error")
            ))
        }
    }
}

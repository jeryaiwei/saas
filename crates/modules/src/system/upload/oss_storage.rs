//! Alibaba Cloud OSS storage provider.
//!
//! Uses the OSS REST API directly with V1 HMAC-SHA1 signature.
//! No official SDK dependency — just `reqwest` + `hmac` + `sha1` + `base64`.

use async_trait::async_trait;
use framework::config::OssConfig;
use framework::infra::storage::{PutResult, SignedUploadUrl, StorageProvider};

/// Shared HTTP client (connection pooling).
static HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("build reqwest client for OSS")
});

#[allow(dead_code)] // domain + public_url reserved for future URL generation
pub struct OssStorageProvider {
    access_key_id: String,
    access_key_secret: String,
    bucket: String,
    /// e.g. "https://my-bucket.oss-cn-hangzhou.aliyuncs.com"
    endpoint: String,
    /// e.g. "https://cdn.example.com" or same as endpoint
    domain: String,
    /// Path prefix, e.g. "uploads"
    location: String,
}

impl OssStorageProvider {
    pub fn new(cfg: &OssConfig) -> Self {
        let endpoint = cfg
            .endpoint
            .clone()
            .unwrap_or_else(|| format!("https://{}.{}.aliyuncs.com", cfg.bucket, cfg.region));
        let domain = cfg.domain.clone().unwrap_or_else(|| endpoint.clone());
        Self {
            access_key_id: cfg.access_key_id.clone(),
            access_key_secret: cfg.access_key_secret.clone(),
            bucket: cfg.bucket.clone(),
            endpoint,
            domain,
            location: cfg.location.clone(),
        }
    }

    /// Build the full storage key with optional location prefix.
    fn full_key(&self, key: &str) -> String {
        if self.location.is_empty() {
            key.to_string()
        } else {
            format!("{}/{}", self.location.trim_end_matches('/'), key)
        }
    }

    /// Build the public URL for a given key (used when generating access URLs).
    #[allow(dead_code)]
    fn public_url(&self, key: &str) -> String {
        let full = self.full_key(key);
        format!("{}/{}", self.domain.trim_end_matches('/'), full)
    }

    /// Build the endpoint URL for a given key.
    fn object_url(&self, key: &str) -> String {
        let full = self.full_key(key);
        format!("{}/{}", self.endpoint.trim_end_matches('/'), full)
    }

    /// OSS V1 signature: HMAC-SHA1 over the canonical string.
    fn sign(&self, verb: &str, content_type: &str, date: &str, resource: &str) -> String {
        use base64::Engine;
        use hmac::{Hmac, KeyInit, Mac};
        use sha1::Sha1;

        // StringToSign = VERB + "\n" + Content-MD5 + "\n" + Content-Type + "\n" + Date + "\n" + CanonicalizedResource
        let string_to_sign = format!("{}\n\n{}\n{}\n/{}/{}", verb, content_type, date, self.bucket, resource);

        let mut mac = Hmac::<Sha1>::new_from_slice(self.access_key_secret.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize().into_bytes();
        base64::engine::general_purpose::STANDARD.encode(result)
    }

    /// RFC 7231 date header.
    fn http_date() -> String {
        chrono::Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string()
    }

    /// OSS V1 signed URL: uses Expires (unix timestamp) instead of Date.
    /// StringToSign = VERB + "\n" + "\n" + Content-Type + "\n" + Expires + "\n" + CanonicalizedResource
    fn sign_url(&self, verb: &str, content_type: &str, expires: i64, resource: &str) -> String {
        use base64::Engine;
        use hmac::{Hmac, KeyInit, Mac};
        use sha1::Sha1;

        let string_to_sign = format!("{}\n\n{}\n{}\n/{}/{}", verb, content_type, expires, self.bucket, resource);
        let mut mac = Hmac::<Sha1>::new_from_slice(self.access_key_secret.as_bytes())
            .expect("HMAC accepts any key length");
        mac.update(string_to_sign.as_bytes());
        let result = mac.finalize().into_bytes();
        base64::engine::general_purpose::STANDARD.encode(result)
    }
}

#[async_trait]
impl StorageProvider for OssStorageProvider {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<PutResult, String> {
        let full = self.full_key(key);
        let date = Self::http_date();
        let signature = self.sign("PUT", content_type, &date, &full);
        let endpoint_url = self.object_url(key);

        let resp = HTTP_CLIENT
            .put(&endpoint_url)
            .header("Date", &date)
            .header("Content-Type", content_type)
            .header(
                "Authorization",
                format!("OSS {}:{}", self.access_key_id, signature),
            )
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| format!("oss put request: {e}"))?;

        if resp.status().is_success() {
            Ok(PutResult {
                key: key.to_string(),
                url: self.public_url(key),
            })
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("oss put failed ({}): {}", status, body))
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, String> {
        let full = self.full_key(key);
        let date = Self::http_date();
        let signature = self.sign("GET", "", &date, &full);
        let url = self.object_url(key);

        let resp = HTTP_CLIENT
            .get(&url)
            .header("Date", &date)
            .header(
                "Authorization",
                format!("OSS {}:{}", self.access_key_id, signature),
            )
            .send()
            .await
            .map_err(|e| format!("oss get request: {e}"))?;

        if resp.status().is_success() {
            resp.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| format!("oss get body: {e}"))
        } else {
            let status = resp.status();
            Err(format!("oss get failed ({})", status))
        }
    }

    async fn delete(&self, key: &str) -> Result<(), String> {
        let full = self.full_key(key);
        let date = Self::http_date();
        let signature = self.sign("DELETE", "", &date, &full);
        let url = self.object_url(key);

        let resp = HTTP_CLIENT
            .delete(&url)
            .header("Date", &date)
            .header(
                "Authorization",
                format!("OSS {}:{}", self.access_key_id, signature),
            )
            .send()
            .await
            .map_err(|e| format!("oss delete request: {e}"))?;

        if resp.status().is_success() || resp.status().as_u16() == 404 {
            Ok(())
        } else {
            let status = resp.status();
            Err(format!("oss delete failed ({})", status))
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, String> {
        let full = self.full_key(key);
        let date = Self::http_date();
        let signature = self.sign("HEAD", "", &date, &full);
        let url = self.object_url(key);

        let resp = HTTP_CLIENT
            .head(&url)
            .header("Date", &date)
            .header(
                "Authorization",
                format!("OSS {}:{}", self.access_key_id, signature),
            )
            .send()
            .await
            .map_err(|e| format!("oss head request: {e}"))?;

        Ok(resp.status().is_success())
    }

    fn signed_put_url(
        &self,
        key: &str,
        content_type: &str,
        expires_secs: u64,
    ) -> Option<SignedUploadUrl> {
        let full = self.full_key(key);
        let expires = chrono::Utc::now().timestamp() + expires_secs as i64;
        let signature = self.sign_url("PUT", content_type, expires, &full);

        // URL-encode signature
        let encoded_sig = urlencoding::encode(&signature);

        let signed_url = format!(
            "{}/{}?OSSAccessKeyId={}&Expires={}&Signature={}",
            self.endpoint.trim_end_matches('/'),
            full,
            urlencoding::encode(&self.access_key_id),
            expires,
            encoded_sig,
        );

        Some(SignedUploadUrl {
            key: key.to_string(),
            signed_url,
            url: self.public_url(key),
        })
    }
}

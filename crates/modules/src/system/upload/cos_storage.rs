//! Tencent Cloud COS storage provider.
//!
//! Uses the COS XML API with HMAC-SHA1 signature (compatible with V5 SDK).
//! Reference: https://cloud.tencent.com/document/product/436/7778

use async_trait::async_trait;
use framework::config::CosConfig;
use framework::infra::storage::{PutResult, SignedUploadUrl, StorageProvider};

/// Shared HTTP client (connection pooling).
static HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("build reqwest client for COS")
});

#[allow(dead_code)]
pub struct CosStorageProvider {
    secret_id: String,
    secret_key: String,
    bucket: String,
    region: String,
    /// e.g. "https://my-bucket-12345.cos.ap-guangzhou.myqcloud.com"
    endpoint: String,
    /// Custom domain or same as endpoint
    domain: String,
    /// Path prefix
    location: String,
}

impl CosStorageProvider {
    pub fn new(cfg: &CosConfig) -> Self {
        let endpoint = format!("https://{}.cos.{}.myqcloud.com", cfg.bucket, cfg.region);
        let domain = cfg.domain.clone().unwrap_or_else(|| endpoint.clone());
        Self {
            secret_id: cfg.secret_id.clone(),
            secret_key: cfg.secret_key.clone(),
            bucket: cfg.bucket.clone(),
            region: cfg.region.clone(),
            endpoint,
            domain,
            location: cfg.location.clone(),
        }
    }

    fn full_key(&self, key: &str) -> String {
        if self.location.is_empty() {
            format!("/{}", key)
        } else {
            format!("/{}/{}", self.location.trim_matches('/'), key)
        }
    }

    fn object_url(&self, key: &str) -> String {
        let path = self.full_key(key);
        format!("{}{}", self.endpoint, path)
    }

    /// COS V5 signature (HMAC-SHA1 based).
    /// Reference: https://cloud.tencent.com/document/product/436/7778
    fn sign(
        &self,
        method: &str,
        uri_path: &str,
        _headers: &[(&str, &str)],
        start_time: i64,
        duration: i64,
    ) -> String {
        use hmac::{Hmac, KeyInit, Mac};
        use sha1::{Digest, Sha1};

        let end_time = start_time + duration;
        let key_time = format!("{};{}", start_time, end_time);

        // 1. SignKey = HMAC-SHA1(SecretKey, KeyTime)
        let mut sign_key_mac =
            Hmac::<Sha1>::new_from_slice(self.secret_key.as_bytes()).expect("HMAC key");
        sign_key_mac.update(key_time.as_bytes());
        let sign_key = hex::encode(sign_key_mac.finalize().into_bytes());

        // 2. HttpString = method\nuri\n\n\n
        let http_string = format!("{}\n{}\n\n\n", method.to_lowercase(), uri_path);

        // 3. StringToSign = sha1\nKeyTime\nSHA1(HttpString)\n
        let http_string_hash = hex::encode(Sha1::digest(http_string.as_bytes()));
        let string_to_sign = format!("sha1\n{}\n{}\n", key_time, http_string_hash);

        // 4. Signature = HMAC-SHA1(SignKey, StringToSign)
        let mut sig_mac =
            Hmac::<Sha1>::new_from_slice(sign_key.as_bytes()).expect("HMAC key");
        sig_mac.update(string_to_sign.as_bytes());
        let signature = hex::encode(sig_mac.finalize().into_bytes());

        // 5. Build Authorization header
        format!(
            "q-sign-algorithm=sha1&q-ak={}&q-sign-time={}&q-key-time={}&q-header-list=&q-url-param-list=&q-signature={}",
            self.secret_id, key_time, key_time, signature
        )
    }

    fn now_ts() -> i64 {
        chrono::Utc::now().timestamp()
    }
}

#[async_trait]
impl StorageProvider for CosStorageProvider {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<PutResult, String> {
        let uri_path = self.full_key(key);
        let endpoint_url = self.object_url(key);
        let ts = Self::now_ts();
        let auth = self.sign("put", &uri_path, &[], ts, 600);

        let resp = HTTP_CLIENT
            .put(&endpoint_url)
            .header("Authorization", &auth)
            .header("Content-Type", content_type)
            .body(data.to_vec())
            .send()
            .await
            .map_err(|e| format!("cos put request: {e}"))?;

        if resp.status().is_success() {
            let public_url = format!("{}{}", self.domain.trim_end_matches('/'), uri_path);
            Ok(PutResult {
                key: key.to_string(),
                url: public_url,
            })
        } else {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            Err(format!("cos put failed ({}): {}", status, body))
        }
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, String> {
        let uri_path = self.full_key(key);
        let url = self.object_url(key);
        let ts = Self::now_ts();
        let auth = self.sign("get", &uri_path, &[], ts, 600);

        let resp = HTTP_CLIENT
            .get(&url)
            .header("Authorization", &auth)
            .send()
            .await
            .map_err(|e| format!("cos get request: {e}"))?;

        if resp.status().is_success() {
            resp.bytes()
                .await
                .map(|b| b.to_vec())
                .map_err(|e| format!("cos get body: {e}"))
        } else {
            let status = resp.status();
            Err(format!("cos get failed ({})", status))
        }
    }

    async fn delete(&self, key: &str) -> Result<(), String> {
        let uri_path = self.full_key(key);
        let url = self.object_url(key);
        let ts = Self::now_ts();
        let auth = self.sign("delete", &uri_path, &[], ts, 600);

        let resp = HTTP_CLIENT
            .delete(&url)
            .header("Authorization", &auth)
            .send()
            .await
            .map_err(|e| format!("cos delete request: {e}"))?;

        if resp.status().is_success() || resp.status().as_u16() == 404 {
            Ok(())
        } else {
            let status = resp.status();
            Err(format!("cos delete failed ({})", status))
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, String> {
        let uri_path = self.full_key(key);
        let url = self.object_url(key);
        let ts = Self::now_ts();
        let auth = self.sign("head", &uri_path, &[], ts, 600);

        let resp = HTTP_CLIENT
            .head(&url)
            .header("Authorization", &auth)
            .send()
            .await
            .map_err(|e| format!("cos head request: {e}"))?;

        Ok(resp.status().is_success())
    }

    fn signed_put_url(
        &self,
        key: &str,
        _content_type: &str,
        expires_secs: u64,
    ) -> Option<SignedUploadUrl> {
        let uri_path = self.full_key(key);
        let ts = Self::now_ts();
        let auth = self.sign("put", &uri_path, &[], ts, expires_secs as i64);

        // COS signed URL: append auth as query parameters
        let signed_url = format!(
            "{}{}?{}",
            self.endpoint.trim_end_matches('/'),
            uri_path,
            auth,
        );

        let public_url = format!("{}{}", self.domain.trim_end_matches('/'), uri_path);

        Some(SignedUploadUrl {
            key: key.to_string(),
            signed_url,
            url: public_url,
        })
    }
}

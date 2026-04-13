//! File storage abstraction.

use async_trait::async_trait;

/// Result of a `put` operation.
pub struct PutResult {
    /// Internal storage key (used by `get` / `delete`).
    pub key: String,
    /// Public access URL (stored in DB, returned to frontend).
    pub url: String,
}

/// Pre-signed URL for client-side direct upload.
pub struct SignedUploadUrl {
    /// The storage key the client must upload to.
    pub key: String,
    /// Pre-signed PUT URL (client uploads directly to this URL).
    pub signed_url: String,
    /// Public access URL (stored in DB after callback).
    pub url: String,
}

#[async_trait]
pub trait StorageProvider: Send + Sync {
    /// Store file, returns key + public URL.
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<PutResult, String>;
    /// Read file content by storage key.
    async fn get(&self, key: &str) -> Result<Vec<u8>, String>;
    /// Delete file by storage key.
    async fn delete(&self, key: &str) -> Result<(), String>;
    /// Check if file exists by storage key.
    async fn exists(&self, key: &str) -> Result<bool, String>;
    /// Generate a pre-signed PUT URL for client-side direct upload.
    /// Returns None if the provider does not support client-side upload (e.g. local storage).
    fn signed_put_url(
        &self,
        _key: &str,
        _content_type: &str,
        _expires_secs: u64,
    ) -> Option<SignedUploadUrl> {
        None // default: not supported
    }
}

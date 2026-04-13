//! File storage abstraction.

use async_trait::async_trait;

#[async_trait]
pub trait StorageProvider: Send + Sync {
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<String, String>;
    async fn get(&self, key: &str) -> Result<Vec<u8>, String>;
    async fn delete(&self, key: &str) -> Result<(), String>;
    async fn exists(&self, key: &str) -> Result<bool, String>;
}

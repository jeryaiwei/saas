//! Local filesystem storage provider.

use async_trait::async_trait;
use framework::{
    config::LocalStorageConfig,
    infra::storage::{PutResult, StorageProvider},
};
use std::path::PathBuf;

pub struct LocalStorageProvider {
    base_dir: PathBuf,
    domain: String,
}

impl LocalStorageProvider {
    pub fn new(local: &LocalStorageConfig) -> Self {
        Self {
            base_dir: PathBuf::from(local.upload_dir.clone()),
            domain: local.domain.clone()
        }
    }

    /// Resolve key to full path, rejecting path traversal.
    fn safe_path(&self, key: &str) -> Result<PathBuf, String> {
        // Strip any leading / or ../ components
        let sanitized = key
            .replace('\\', "/")
            .split('/')
            .filter(|seg| !seg.is_empty() && *seg != "." && *seg != "..")
            .collect::<Vec<_>>()
            .join("/");
        if sanitized.is_empty() {
            return Err("empty storage key".into());
        }
        let path = self.base_dir.join(&sanitized);
        // Double check: resolved path must be under base_dir
        if !path.starts_with(&self.base_dir) {
            return Err("path traversal detected".into());
        }
        Ok(path)
    }
}

#[async_trait]
impl StorageProvider for LocalStorageProvider {
    async fn put(&self, key: &str, data: &[u8], _content_type: &str) -> Result<PutResult, String> {
        let path = self.safe_path(key)?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| format!("mkdir: {e}"))?;
        }
        tokio::fs::write(&path, data)
            .await
            .map_err(|e| format!("write: {e}"))?;
        let url = format!("{}/{}", self.domain.trim_end_matches('/'), key);
        Ok(PutResult {
            key: key.to_string(),
            url,
        })
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>, String> {
        let path = self.safe_path(key)?;
        tokio::fs::read(&path)
            .await
            .map_err(|e| format!("read {}: {e}", path.display()))
    }

    async fn delete(&self, key: &str) -> Result<(), String> {
        let path = self.safe_path(key)?;
        match tokio::fs::try_exists(&path).await {
            Ok(true) => tokio::fs::remove_file(&path)
                .await
                .map_err(|e| format!("delete: {e}")),
            _ => Ok(()),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, String> {
        let path = self.safe_path(key)?;
        tokio::fs::try_exists(&path)
            .await
            .map_err(|e| format!("exists: {e}"))
    }
}

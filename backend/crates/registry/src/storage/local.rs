use std::path::PathBuf;
use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;
use futures::StreamExt;

use super::{ByteStream, StorageBackend, StorageError};

/// Local filesystem storage backend.
/// All objects are written under `base_dir` using the supplied key as a relative path.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    base_dir: PathBuf,
}

impl LocalStorage {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self { base_dir: base_dir.into() }
    }

    fn full_path(&self, key: &str) -> PathBuf {
        // Prevent path traversal: strip any leading / and collapse ..
        let safe: PathBuf = key
            .split('/')
            .filter(|s| !s.is_empty() && *s != "..")
            .collect();
        self.base_dir.join(safe)
    }
}

#[async_trait]
impl StorageBackend for LocalStorage {
    async fn put(&self, key: &str, data: Bytes) -> Result<(), StorageError> {
        let path = self.full_path(key);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create(&path).await?;
        file.write_all(&data).await?;
        file.flush().await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<ByteStream, StorageError> {
        let path = self.full_path(key);
        if !path.exists() {
            return Err(StorageError::NotFound(key.to_string()));
        }
        let file = tokio::fs::File::open(&path).await?;
        let stream = ReaderStream::new(file)
            .map(|r| r.map_err(StorageError::Io));
        Ok(Box::pin(stream))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let path = self.full_path(key);
        if path.exists() {
            tokio::fs::remove_file(&path).await?;
        }
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        Ok(self.full_path(key).exists())
    }

    async fn size(&self, key: &str) -> Result<u64, StorageError> {
        let path = self.full_path(key);
        let meta = tokio::fs::metadata(&path).await
            .map_err(|_| StorageError::NotFound(key.to_string()))?;
        Ok(meta.len())
    }

    async fn list(&self, prefix: &str, delimiter: &str) -> Result<super::StorageListResult, StorageError> {
        let path = self.full_path(prefix);
        let mut objects = Vec::new();
        let mut common_prefixes = Vec::new();

        if path.is_dir() {
            let mut entries = tokio::fs::read_dir(&path).await?;
            while let Some(entry) = entries.next_entry().await? {
                let file_type = entry.file_type().await?;
                let file_name = entry.file_name().to_string_lossy().to_string();
                
                // Construct the virtual key (relative path from root)
                let relative_key = if prefix.is_empty() {
                    file_name.clone()
                } else if prefix.ends_with('/') {
                    format!("{}{}", prefix, file_name)
                } else {
                    format!("{}/{}", prefix, file_name)
                };

                if file_type.is_dir() {
                    if delimiter == "/" {
                        common_prefixes.push(format!("{}/", relative_key));
                    } else {
                        common_prefixes.push(format!("{}/", relative_key));
                    }
                } else {
                    let metadata = entry.metadata().await?;
                    let size = metadata.len();
                    let last_modified = metadata.modified().ok().map(|t| {
                        let datetime: chrono::DateTime<chrono::Utc> = t.into();
                        datetime.to_rfc3339()
                    });
                    objects.push(super::StorageObject {
                        key: relative_key,
                        size,
                        last_modified,
                    });
                }
            }
        } else if path.is_file() {
            let metadata = tokio::fs::metadata(&path).await?;
            let size = metadata.len();
            let last_modified = metadata.modified().ok().map(|t| {
                let datetime: chrono::DateTime<chrono::Utc> = t.into();
                datetime.to_rfc3339()
            });
            objects.push(super::StorageObject {
                key: prefix.to_string(),
                size,
                last_modified,
            });
        }

        Ok(super::StorageListResult {
            objects,
            common_prefixes,
        })
    }
}

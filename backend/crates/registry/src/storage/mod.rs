use std::pin::Pin;
use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use thiserror::Error;

pub mod local;
pub mod s3;

pub type ByteStream = Pin<Box<dyn Stream<Item = Result<Bytes, StorageError>> + Send>>;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("object not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("storage backend error: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageObject {
    pub key: String,
    pub size: u64,
    pub last_modified: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StorageListResult {
    pub objects: Vec<StorageObject>,
    pub common_prefixes: Vec<String>,
}

#[async_trait]
pub trait StorageBackend: Send + Sync {
    /// Write bytes to the given key. Overwrites if key already exists.
    async fn put(&self, key: &str, data: Bytes) -> Result<(), StorageError>;

    /// Stream bytes from the given key.
    async fn get(&self, key: &str) -> Result<ByteStream, StorageError>;

    /// Read entire object into memory. Convenience wrapper over `get`.
    async fn get_bytes(&self, key: &str) -> Result<Bytes, StorageError> {
        use futures::TryStreamExt;
        let stream = self.get(key).await?;
        let chunks: Vec<Bytes> = stream.try_collect().await?;
        let total: usize = chunks.iter().map(|b| b.len()).sum();
        let mut out = bytes::BytesMut::with_capacity(total);
        for chunk in chunks {
            out.extend_from_slice(&chunk);
        }
        Ok(out.freeze())
    }

    /// Delete the object at the given key. No-op if not found.
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// Return true if the key exists.
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// Return the size in bytes of the object at the given key.
    async fn size(&self, key: &str) -> Result<u64, StorageError>;

    /// List objects/common prefixes under a prefix with delimiter.
    async fn list(&self, prefix: &str, delimiter: &str) -> Result<StorageListResult, StorageError>;
}

/// Canonical storage key for a content-addressed blob.
/// Format: `blobs/<algo>/<first2>/<hex>`
/// The first2 prefix shards large local directories.
pub fn blob_key(digest: &str) -> String {
    // digest is "sha256:<hex>" — strip the algo prefix
    let hex = digest.splitn(2, ':').nth(1).unwrap_or(digest);
    format!("blobs/sha256/{}/{}", &hex[..2], hex)
}

/// Temporary key for an in-progress chunked upload session.
pub fn upload_key(session_id: &str) -> String {
    format!("uploads/{}", session_id)
}

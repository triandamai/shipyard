use super::StorageError;

/// S3 / MinIO storage backend.
/// Requires the `s3` feature to be enabled.
#[cfg(feature = "s3")]
pub struct S3Storage {
    client: aws_sdk_s3::Client,
    bucket: String,
}

#[cfg(feature = "s3")]
impl S3Storage {
    /// Build from explicit credentials + endpoint (for MinIO / custom S3).
    pub async fn new(
        endpoint: &str,
        bucket: &str,
        access_key: &str,
        secret_key: &str,
        region: &str,
    ) -> Result<Self, StorageError> {
        use aws_credential_types::Credentials;
        use aws_sdk_s3::config::{BehaviorVersion, Region};

        let creds = Credentials::new(access_key, secret_key, None, None, "shipyard");
        let config = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(creds)
            .region(Region::new(region.to_string()))
            .endpoint_url(endpoint)
            .load()
            .await;

        let client = aws_sdk_s3::Client::new(&config);
        Ok(Self { client, bucket: bucket.to_string() })
    }
}

#[cfg(feature = "s3")]
#[async_trait::async_trait]
impl super::StorageBackend for S3Storage {
    async fn put(&self, key: &str, data: bytes::Bytes) -> Result<(), StorageError> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(data.into())
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<ByteStream, StorageError> {
        use futures::StreamExt;
        let resp = self.client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|_| StorageError::NotFound(key.to_string()))?;

        let stream = resp.body
            .map(|r| r.map_err(|e| StorageError::Backend(e.to_string())));
        Ok(Box::pin(stream))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let result = self.client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await;
        Ok(result.is_ok())
    }

    async fn size(&self, key: &str) -> Result<u64, StorageError> {
        let resp = self.client
            .head_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|_| StorageError::NotFound(key.to_string()))?;
        Ok(resp.content_length().unwrap_or(0) as u64)
    }
}

/// Stub when s3 feature is disabled — prevents compilation errors on import.
#[cfg(not(feature = "s3"))]
pub struct S3Storage;

#[cfg(not(feature = "s3"))]
impl S3Storage {
    pub async fn new(
        _endpoint: &str, _bucket: &str, _access_key: &str,
        _secret_key: &str, _region: &str,
    ) -> Result<Self, StorageError> {
        Err(StorageError::Backend("S3 support not compiled in (enable the `s3` feature)".into()))
    }
}

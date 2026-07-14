use super::{ByteStream, StorageError};
use futures::stream;
use s3::creds::Credentials;
use s3::{Bucket, Region};

/// S3 / MinIO storage backend.
/// Requires the `s3` feature to be enabled.
#[cfg(feature = "s3")]
pub struct S3Storage {
    endpoint: String,
    bucket: String,
    access_key: String,
    secret_key: String,
    region: String,
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
        Ok(Self {
            endpoint: endpoint.to_string(),
            bucket: bucket.to_string(),
            access_key: access_key.to_string(),
            secret_key: secret_key.to_string(),
            region: region.to_string(),
        })
    }

    fn build_bucket(&self) -> Result<Bucket, StorageError> {
        let credentials = Credentials::new(
            Some(self.access_key.as_str()),
            Some(self.secret_key.as_str()),
            None,
            None,
            Some("shipyard"),
        );
        if credentials.is_err() {
            return Err(StorageError::Backend(credentials.unwrap_err().to_string()));
        }
        let credentials = credentials.unwrap();
        let bucket = Bucket::new(
            self.bucket.as_str(),
            Region::Custom {
                region: self.region.to_string(),
                endpoint: self.endpoint.to_string(),
            },
            credentials.clone(),
        );
        if bucket.is_err() {
            return Err(StorageError::Backend(bucket.unwrap_err().to_string()));
        }
        let bucket_build = bucket.unwrap().with_path_style();

        Ok(*bucket_build)
    }
}

#[cfg(feature = "s3")]
#[async_trait::async_trait]
impl super::StorageBackend for S3Storage {
    async fn put(&self, key: &str, data: bytes::Bytes) -> Result<(), StorageError> {
        let bucket = self.build_bucket();
        if bucket.is_err() {
            return Err(StorageError::Backend(bucket.unwrap_err().to_string()));
        }
        bucket
            .unwrap()
            .put_object(key, &data.to_vec())
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<ByteStream, StorageError> {
        let bucket = self.build_bucket();
        if bucket.is_err() {
            return Err(StorageError::Backend(bucket.unwrap_err().to_string()));
        }
        let resp = bucket
            .unwrap()
            .get_object(key.to_string())
            .await
            .map_err(|e| StorageError::NotFound(e.to_string()));

        if resp.is_err() {
            return Err(StorageError::NotFound(resp.unwrap_err().to_string()));
        }
        let data = resp.unwrap();
        // Convert the data into Bytes
        let bytes = data.into_bytes();

        // Create a stream that emits this single item wrapped in an Ok()
        let stream = stream::once(async move { Ok(bytes) });

        Ok(Box::pin(stream))
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        self.build_bucket()?
            .delete_object(key.to_string())
            .await
            .map_err(|e| StorageError::Backend(e.to_string()))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, StorageError> {
        let result = self.build_bucket()?.head_object(key.to_string()).await;
        Ok(result.is_ok())
    }

    async fn size(&self, key: &str) -> Result<u64, StorageError> {
        let resp = self
            .build_bucket()?
            .head_object(key.to_string())
            .await
            .map_err(|_| StorageError::NotFound(key.to_string()))?;
        Ok(resp.0.content_length.unwrap_or(0) as u64)
    }
}

/// Stub when s3 feature is disabled — prevents compilation errors on import.
#[cfg(not(feature = "s3"))]
pub struct S3Storage;

#[cfg(not(feature = "s3"))]
impl S3Storage {
    pub async fn new(
        _endpoint: &str,
        _bucket: &str,
        _access_key: &str,
        _secret_key: &str,
        _region: &str,
    ) -> Result<Self, StorageError> {
        Err(StorageError::Backend(
            "S3 support not compiled in (enable the `s3` feature)".into(),
        ))
    }
}

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
        let  bucket = Bucket::new(
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

        let mut bucket = bucket.unwrap();
        bucket.set_path_style();
        bucket.set_listobjects_v2();
        Ok(*bucket)
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

    async fn list(
        &self,
        prefix: &str,
        delimiter: &str,
    ) -> Result<super::StorageListResult, StorageError> {
        let bucket = self.build_bucket()?;
        let delimiter_opt = if delimiter.is_empty() {
            None
        } else {
            Some(delimiter.to_string())
        };

        let list_results = match bucket.list(prefix.to_string(), delimiter_opt.clone()).await {
            Ok(res) => res,
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("serde xml") || err_str.contains("xml") || err_str.contains("DeError") {
                    // Diagnostic connection test: fetch a nonexistent dummy key to determine if it is a credential, endpoint or DNS issue.
                    match bucket.get_object("shipyard-connection-test-dummy-key-xyz").await {
                        Err(s3::error::S3Error::HttpFailWithBody(code, body)) => {
                            return Err(StorageError::Backend(format!(
                                "S3 connection failed (HTTP {}): {}. Please verify your S3 credentials, bucket name, and region.",
                                code, body
                            )));
                        }
                        Err(s3::error::S3Error::HttpFail) => {
                            return Err(StorageError::Backend(
                                "S3 connection failed with HTTP error. Please verify your S3 credentials and bucket name.".to_string()
                            ));
                        }
                        Err(s3::error::S3Error::Reqwest(re)) => {
                            return Err(StorageError::Backend(format!(
                                "S3 network connection failed: {}. Please check if the S3 endpoint is reachable.",
                                re
                            )));
                        }
                        Err(conn_err) => {
                            let conn_err_str = conn_err.to_string();
                            // If the connection succeeds but returns 404 (as the dummy key does not exist),
                            // then the bucket is accessible, so we return the original list error.
                            if !conn_err_str.contains("404") && !conn_err_str.contains("HttpFailWithBody(404") {
                                return Err(StorageError::Backend(format!(
                                    "S3 diagnostic connection error: {}. Please check your S3 configuration.",
                                    conn_err
                                )));
                            }
                        }
                        Ok(_) => {}
                    }
                }
                return Err(StorageError::Backend(e.to_string()));
            }
        };

        let mut objects = Vec::new();
        let mut common_prefixes = Vec::new();
        for res in list_results {
            for obj in res.contents {
                objects.push(super::StorageObject {
                    key: obj.key,
                    size: obj.size,
                    last_modified: Some(obj.last_modified),
                });
            }
            if let Some(prefixes) = res.common_prefixes {
                for pref in prefixes {
                    common_prefixes.push(pref.prefix);
                }
            }
        }

        Ok(super::StorageListResult {
            objects,
            common_prefixes,
        })
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

use axum::{http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use thiserror::Error;

use crate::storage::StorageError;

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("blob not found: {0}")]
    BlobNotFound(String),

    #[error("manifest not found: {0}")]
    ManifestNotFound(String),

    #[error("upload session not found: {0}")]
    UploadNotFound(String),

    #[error("invalid digest: {0}")]
    InvalidDigest(String),

    #[error("digest mismatch: expected {expected}, got {got}")]
    DigestMismatch { expected: String, got: String },

    #[error("unauthorized")]
    Unauthorized,

    #[error("forbidden")]
    Forbidden,

    #[error("storage error: {0}")]
    Storage(#[from] StorageError),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("internal error: {0}")]
    Internal(String),
}

impl RegistryError {
    /// OCI error code string per the Distribution Spec.
    fn oci_code(&self) -> &'static str {
        match self {
            Self::BlobNotFound(_)     => "BLOB_UNKNOWN",
            Self::ManifestNotFound(_) => "MANIFEST_UNKNOWN",
            Self::UploadNotFound(_)   => "BLOB_UPLOAD_UNKNOWN",
            Self::InvalidDigest(_)    => "DIGEST_INVALID",
            Self::DigestMismatch {..} => "DIGEST_INVALID",
            Self::Unauthorized        => "UNAUTHORIZED",
            Self::Forbidden           => "DENIED",
            _                         => "UNKNOWN",
        }
    }
}

impl IntoResponse for RegistryError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            Self::Unauthorized        => StatusCode::UNAUTHORIZED,
            Self::Forbidden           => StatusCode::FORBIDDEN,
            Self::BlobNotFound(_)
            | Self::ManifestNotFound(_)
            | Self::UploadNotFound(_) => StatusCode::NOT_FOUND,
            Self::InvalidDigest(_)
            | Self::DigestMismatch {..} => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        // OCI Distribution Spec error envelope
        let body = json!({
            "errors": [{
                "code":    self.oci_code(),
                "message": self.to_string(),
                "detail":  null
            }]
        });

        (status, Json(body)).into_response()
    }
}

pub type Result<T> = std::result::Result<T, RegistryError>;

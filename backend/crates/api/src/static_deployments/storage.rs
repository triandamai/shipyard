use std::path::{Path, PathBuf};
use async_trait::async_trait;
use shipyard_common::error::{AppError, AppResult};
use uuid::Uuid;

pub fn mime_for_path(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "wasm" => "application/wasm",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        _ => "application/octet-stream",
    }
}

#[async_trait]
pub trait StaticStorageBackend: Send + Sync {
    /// Return the root directory on disk for a given deployment (if locally stored).
    fn deployment_dir(&self, project_id: Uuid, deployment_id: Uuid) -> PathBuf;

    /// Verify that the deployment exists and its entry file is present.
    async fn verify_deployment(
        &self,
        project_id: Uuid,
        deployment_id: Uuid,
        entry_file: &str,
    ) -> AppResult<bool>;

    /// Read a file within a deployment. Rejects directory traversal.
    /// Returns Ok(Some((bytes, content_type))) if found, Ok(None) if 404, or Err on traversal.
    async fn read_file(
        &self,
        project_id: Uuid,
        deployment_id: Uuid,
        relative_path: &str,
    ) -> AppResult<Option<(Vec<u8>, String)>>;

    /// Completely delete all stored artifacts for a deployment.
    async fn delete_deployment(&self, project_id: Uuid, deployment_id: Uuid) -> AppResult<()>;
}

/// Local filesystem storage implementation.
#[derive(Debug, Clone)]
pub struct LocalStorage {
    base_dir: PathBuf,
}

impl LocalStorage {
    pub fn new(data_dir: &str) -> Self {
        Self {
            base_dir: PathBuf::from(data_dir).join("static_deployments"),
        }
    }

    fn safe_resolve(&self, project_id: Uuid, deployment_id: Uuid, relative_path: &str) -> AppResult<PathBuf> {
        let dep_dir = self.deployment_dir(project_id, deployment_id);
        let trimmed = relative_path.trim_start_matches('/');
        let path = Path::new(trimmed);

        for component in path.components() {
            match component {
                std::path::Component::Normal(_) => {}
                std::path::Component::CurDir => {}
                _ => {
                    return Err(AppError::BadRequest(format!(
                        "Invalid path component in request: {relative_path}"
                    )));
                }
            }
        }

        Ok(dep_dir.join(path))
    }
}

#[async_trait]
impl StaticStorageBackend for LocalStorage {
    fn deployment_dir(&self, project_id: Uuid, deployment_id: Uuid) -> PathBuf {
        self.base_dir
            .join(project_id.to_string())
            .join(deployment_id.to_string())
    }

    async fn verify_deployment(
        &self,
        project_id: Uuid,
        deployment_id: Uuid,
        entry_file: &str,
    ) -> AppResult<bool> {
        let entry_path = self.safe_resolve(project_id, deployment_id, entry_file)?;
        if !entry_path.is_file() {
            return Ok(false);
        }
        let meta = tokio::fs::metadata(&entry_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read entry metadata: {e}")))?;
        Ok(meta.len() > 0)
    }

    async fn read_file(
        &self,
        project_id: Uuid,
        deployment_id: Uuid,
        relative_path: &str,
    ) -> AppResult<Option<(Vec<u8>, String)>> {
        let file_path = self.safe_resolve(project_id, deployment_id, relative_path)?;
        if !file_path.exists() || !file_path.is_file() {
            return Ok(None);
        }

        let mime = mime_for_path(&file_path).to_string();
        let bytes = tokio::fs::read(&file_path)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to read static file: {e}")))?;

        Ok(Some((bytes, mime)))
    }

    async fn delete_deployment(&self, project_id: Uuid, deployment_id: Uuid) -> AppResult<()> {
        let dep_dir = self.deployment_dir(project_id, deployment_id);
        if dep_dir.exists() {
            tokio::fs::remove_dir_all(&dep_dir)
                .await
                .map_err(|e| AppError::Internal(format!("Failed to delete deployment dir: {e}")))?;
        }
        Ok(())
    }
}

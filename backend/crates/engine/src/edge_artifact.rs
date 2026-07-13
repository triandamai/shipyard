//! Artifact storage for edge functions.
//!
//! Layout on disk:
//!   <data_dir>/edge/<fn_id>/
//!     current -> <deploy_id>/    ← relative symlink, atomically swapped on deploy
//!     <deploy_id>/
//!       hello.ts
//!       utils.ts
//!       ...
//!
//! `current` always points to the live deployment directory.
//! Old version directories are pruned by `prune`, never the live one.

use std::path::{Path, PathBuf};

use shipyard_common::error::{AppError, AppResult};
use uuid::Uuid;

// ─── Trait ────────────────────────────────────────────────────────────────────

pub trait ArtifactStore {
    /// Write `files` (name → content) into a new versioned directory.
    /// Returns the absolute path to the created version dir.
    fn write_version(
        &self,
        fn_id: Uuid,
        deploy_id: Uuid,
        files: &[(String, String)],
    ) -> AppResult<PathBuf>;

    /// Atomically point `current` symlink at `version_dir`.
    fn update_current(&self, fn_id: Uuid, version_dir: &Path) -> AppResult<()>;

    /// Point `current` back to a previously deployed version by its deploy_id.
    /// Errors if the version directory no longer exists on disk (pruned).
    fn rollback(&self, fn_id: Uuid, deploy_id: Uuid) -> AppResult<()>;

    /// Delete old version directories, keeping the `keep` most recent.
    /// Never removes the directory that `current` currently points to.
    /// Returns the number of directories removed.
    fn prune(&self, fn_id: Uuid, keep: usize) -> AppResult<usize>;

    /// Absolute path to the `current` symlink for a function.
    fn current_path(&self, fn_id: Uuid) -> PathBuf;

    /// Absolute path to a specific version directory.
    fn version_path(&self, fn_id: Uuid, deploy_id: Uuid) -> PathBuf;
}

// ─── Implementation ───────────────────────────────────────────────────────────

/// Disk-backed artifact store rooted at `<data_dir>/edge/`.
pub struct EdgeArtifact {
    data_dir: String,
}

impl EdgeArtifact {
    pub fn new(data_dir: &str) -> Self {
        Self { data_dir: data_dir.to_string() }
    }

    fn fn_dir(&self, fn_id: Uuid) -> PathBuf {
        PathBuf::from(&self.data_dir).join("edge").join(fn_id.to_string())
    }
}

impl ArtifactStore for EdgeArtifact {
    fn write_version(
        &self,
        fn_id: Uuid,
        deploy_id: Uuid,
        files: &[(String, String)],
    ) -> AppResult<PathBuf> {
        let version_dir = self.fn_dir(fn_id).join(deploy_id.to_string());
        std::fs::create_dir_all(&version_dir).map_err(|e| {
            AppError::Internal(format!("create edge version dir {}: {e}", version_dir.display()))
        })?;

        for (filename, content) in files {
            // Reject any path traversal attempts.
            let safe_name = Path::new(filename).file_name().ok_or_else(|| {
                AppError::BadRequest(format!("invalid edge function filename: {filename}"))
            })?;
            let dest = version_dir.join(safe_name);
            std::fs::write(&dest, content.as_bytes()).map_err(|e| {
                AppError::Internal(format!("write edge file {}: {e}", dest.display()))
            })?;
        }

        // Ensure the runtime container (different uid) can read the files.
        crate::static_site::fixup_permissions(&version_dir)?;

        Ok(version_dir)
    }

    fn update_current(&self, fn_id: Uuid, version_dir: &Path) -> AppResult<()> {
        let current = self.fn_dir(fn_id).join("current");
        crate::static_site::atomic_swap_symlink(&current, version_dir)
    }

    fn rollback(&self, fn_id: Uuid, deploy_id: Uuid) -> AppResult<()> {
        let version_dir = self.fn_dir(fn_id).join(deploy_id.to_string());
        if !version_dir.exists() {
            return Err(AppError::NotFound(format!(
                "edge artifact version {deploy_id} not found on disk — it may have been pruned"
            )));
        }
        let current = self.fn_dir(fn_id).join("current");
        crate::static_site::atomic_swap_symlink(&current, &version_dir)
    }

    fn prune(&self, fn_id: Uuid, keep: usize) -> AppResult<usize> {
        crate::static_site::prune_old_versions(&self.fn_dir(fn_id), keep)
    }

    fn current_path(&self, fn_id: Uuid) -> PathBuf {
        self.fn_dir(fn_id).join("current")
    }

    fn version_path(&self, fn_id: Uuid, deploy_id: Uuid) -> PathBuf {
        self.fn_dir(fn_id).join(deploy_id.to_string())
    }
}

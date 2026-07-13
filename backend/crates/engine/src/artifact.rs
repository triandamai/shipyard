//! Versioned artifact storage for deployed resources.
//!
//! # Design
//!
//! The `ArtifactStore` trait owns **directory lifecycle** — where versions live,
//! how to activate one, how to roll back, and how to prune old versions.
//! It does NOT own how content gets written into a version directory; that is
//! the caller's responsibility, because the right strategy differs:
//!
//! - Edge functions: write a handful of `.ts` files from in-memory strings.
//! - Static sites:   copy an entire built directory tree from disk.
//! - Future S3:      upload via the AWS SDK and store a manifest path.
//!
//! # Adding a new backend (S3, Artifactory, …)
//!
//! 1. Add a new struct (e.g. `S3Artifact`) in this file (or a sub-module).
//! 2. Implement `ArtifactStore` for it.
//!    - `make_version_dir`: create the remote "prefix" / local staging dir.
//!    - `update_current`: update a manifest/pointer object (or symlink on a
//!      mount) to reference the new version.
//!    - `rollback` / `prune`: update the pointer / delete old prefixes.
//! 3. Wire it into `DeploymentEngine` or the relevant API handler.
//!
//! # On-disk layout (local implementations)
//!
//! ```text
//! <data_dir>/
//!   edge/<fn_id>/
//!     current -> <deploy_id>/       ← relative symlink
//!     <deploy_id>/
//!       hello.ts
//!
//!   static/<service_id>/
//!     current -> <deploy_id>/       ← relative symlink
//!     <deploy_id>/
//!       public/                     ← nginx root points here
//!         index.html
//!         assets/
//! ```

use std::path::{Path, PathBuf};

use shipyard_common::error::{AppError, AppResult};
use uuid::Uuid;

// ─── Trait ────────────────────────────────────────────────────────────────────

pub trait ArtifactStore {
    /// Create the versioned directory for a new deployment and return its path.
    /// The caller is responsible for writing content into the returned directory.
    fn make_version_dir(&self, resource_id: Uuid, deploy_id: Uuid) -> AppResult<PathBuf>;

    /// Atomically point the `current` symlink at `version_dir`.
    fn update_current(&self, resource_id: Uuid, version_dir: &Path) -> AppResult<()>;

    /// Point `current` back to a previously deployed version.
    /// Errors if the version directory no longer exists (pruned).
    fn rollback(&self, resource_id: Uuid, deploy_id: Uuid) -> AppResult<()>;

    /// Delete old version directories, keeping the `keep` most recent.
    /// Never removes the directory that `current` currently points to.
    /// Returns the number of directories removed.
    fn prune(&self, resource_id: Uuid, keep: usize) -> AppResult<usize>;

    /// Absolute path to the `current` symlink for a resource.
    fn current_path(&self, resource_id: Uuid) -> PathBuf;

    /// Absolute path to a specific version directory.
    fn version_path(&self, resource_id: Uuid, deploy_id: Uuid) -> PathBuf;
}

// ─── Shared helpers ───────────────────────────────────────────────────────────

fn swap_current(resource_dir: &Path, version_dir: &Path) -> AppResult<()> {
    let current = resource_dir.join("current");
    crate::static_site::atomic_swap_symlink(&current, version_dir)
}

fn rollback_to(resource_dir: &Path, deploy_id: Uuid) -> AppResult<()> {
    let version_dir = resource_dir.join(deploy_id.to_string());
    if !version_dir.exists() {
        return Err(AppError::NotFound(format!(
            "artifact version {deploy_id} not found on disk — it may have been pruned"
        )));
    }
    swap_current(resource_dir, &version_dir)
}

// ─── EdgeArtifact ─────────────────────────────────────────────────────────────

/// Disk-backed artifact store for edge functions.
/// Root: `<data_dir>/edge/<fn_id>/`
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

    /// Convenience: write in-memory `files` (name → content) into a new version
    /// directory and activate it. Not part of the trait because static sites
    /// write from disk directories, not in-memory strings.
    pub fn write_version(
        &self,
        fn_id: Uuid,
        deploy_id: Uuid,
        files: &[(String, String)],
    ) -> AppResult<PathBuf> {
        let version_dir = self.make_version_dir(fn_id, deploy_id)?;

        for (filename, content) in files {
            let safe_name = Path::new(filename).file_name().ok_or_else(|| {
                AppError::BadRequest(format!("invalid edge function filename: {filename}"))
            })?;
            let dest = version_dir.join(safe_name);
            std::fs::write(&dest, content.as_bytes()).map_err(|e| {
                AppError::Internal(format!("write edge file {}: {e}", dest.display()))
            })?;
        }

        crate::static_site::fixup_permissions(&version_dir)?;
        Ok(version_dir)
    }
}

impl ArtifactStore for EdgeArtifact {
    fn make_version_dir(&self, fn_id: Uuid, deploy_id: Uuid) -> AppResult<PathBuf> {
        let dir = self.fn_dir(fn_id).join(deploy_id.to_string());
        std::fs::create_dir_all(&dir).map_err(|e| {
            AppError::Internal(format!("create edge version dir {}: {e}", dir.display()))
        })?;
        Ok(dir)
    }

    fn update_current(&self, fn_id: Uuid, version_dir: &Path) -> AppResult<()> {
        swap_current(&self.fn_dir(fn_id), version_dir)
    }

    fn rollback(&self, fn_id: Uuid, deploy_id: Uuid) -> AppResult<()> {
        rollback_to(&self.fn_dir(fn_id), deploy_id)
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

// ─── StaticArtifact ───────────────────────────────────────────────────────────

/// Disk-backed artifact store for static sites.
/// Root: `<data_dir>/static/<service_id>/`
///
/// `make_version_dir` creates and returns `<deploy_id>/public/` — the subdir
/// that nginx's `root` directive points to. Callers copy built files into it.
pub struct StaticArtifact {
    data_dir: String,
}

impl StaticArtifact {
    pub fn new(data_dir: &str) -> Self {
        Self { data_dir: data_dir.to_string() }
    }

    fn service_dir(&self, service_id: Uuid) -> PathBuf {
        PathBuf::from(&self.data_dir).join("static").join(service_id.to_string())
    }
}

impl ArtifactStore for StaticArtifact {
    /// Creates `<service_id>/<deploy_id>/public/` and returns it.
    /// nginx's `root` must point to this `public/` subdir.
    fn make_version_dir(&self, service_id: Uuid, deploy_id: Uuid) -> AppResult<PathBuf> {
        let public_dir = self
            .service_dir(service_id)
            .join(deploy_id.to_string())
            .join("public");
        std::fs::create_dir_all(&public_dir).map_err(|e| {
            AppError::Internal(format!(
                "create static version dir {}: {e}",
                public_dir.display()
            ))
        })?;
        Ok(public_dir)
    }

    fn update_current(&self, service_id: Uuid, version_dir: &Path) -> AppResult<()> {
        // version_dir here is the deploy-id dir (parent of public/), not public/ itself.
        swap_current(&self.service_dir(service_id), version_dir)
    }

    fn rollback(&self, service_id: Uuid, deploy_id: Uuid) -> AppResult<()> {
        rollback_to(&self.service_dir(service_id), deploy_id)
    }

    fn prune(&self, service_id: Uuid, keep: usize) -> AppResult<usize> {
        crate::static_site::prune_old_versions(&self.service_dir(service_id), keep)
    }

    fn current_path(&self, service_id: Uuid) -> PathBuf {
        self.service_dir(service_id).join("current")
    }

    fn version_path(&self, service_id: Uuid, deploy_id: Uuid) -> PathBuf {
        self.service_dir(service_id).join(deploy_id.to_string())
    }
}

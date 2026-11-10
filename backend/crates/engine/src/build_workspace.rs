//! Ephemeral per-deployment build workspaces.
//!
//! Git and static-site deployments need a working copy of the repository to
//! build from. Previously that clone was kept forever inside the service's
//! directory (`<data_dir>/<org>/<project>/<service>/<slug>`) or the static
//! site cache (`<data_dir>/static/<service>/repo`), which left full project
//! checkouts lying around under the shipyard data dir.
//!
//! A [`BuildWorkspace`] lives at `<data_dir>/builds/<deployment_id>`, is
//! created fresh for each deployment and is removed when the deployment
//! finishes — explicitly via [`BuildWorkspace::cleanup`] on the happy path,
//! or by `Drop` on every early-return / error path.
//!
//! It stays under `data_dir` (rather than the OS temp dir) because static
//! builds bind-mount the checkout into a build container, so the path must be
//! visible to the host Docker daemon.

use std::path::{Path, PathBuf};

use uuid::Uuid;

/// Name of the directory under `data_dir` that holds build workspaces.
pub const BUILDS_DIR_NAME: &str = "builds";

/// Path of the build workspace for a deployment: `<data_dir>/builds/<deployment_id>`.
pub fn build_workspace_dir(data_dir: &str, deployment_id: Uuid) -> PathBuf {
    Path::new(data_dir)
        .join(BUILDS_DIR_NAME)
        .join(deployment_id.to_string())
}

/// A per-deployment working directory that is deleted when the deployment ends.
#[derive(Debug)]
pub struct BuildWorkspace {
    path: PathBuf,
    removed: bool,
}

impl BuildWorkspace {
    /// Prepare a fresh workspace for `deployment_id`.
    ///
    /// Any leftover directory at the same path (e.g. from a crashed attempt)
    /// is removed first so the clone always starts clean. Only the parent
    /// `builds/` directory is created — the workspace itself is created by the
    /// git clone.
    pub async fn prepare(data_dir: &str, deployment_id: Uuid) -> std::io::Result<Self> {
        let path = build_workspace_dir(data_dir, deployment_id);
        if tokio::fs::metadata(&path).await.is_ok() {
            tokio::fs::remove_dir_all(&path).await?;
        }
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        Ok(Self { path, removed: false })
    }

    /// Absolute path of the workspace.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Workspace path as an owned `String` (convenience for string-based APIs).
    pub fn path_string(&self) -> String {
        self.path.to_string_lossy().to_string()
    }

    /// Remove the workspace now and wait for it to finish.
    pub async fn cleanup(mut self) -> std::io::Result<()> {
        self.removed = true;
        match tokio::fs::remove_dir_all(&self.path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }
}

impl Drop for BuildWorkspace {
    fn drop(&mut self) {
        if self.removed {
            return;
        }
        let path = std::mem::take(&mut self.path);
        // Prefer not to block an async worker thread on a large recursive delete.
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn_blocking(move || remove_quietly(&path));
            }
            Err(_) => remove_quietly(&path),
        }
    }
}

fn remove_quietly(path: &Path) {
    if path.as_os_str().is_empty() {
        return;
    }
    if let Err(e) = std::fs::remove_dir_all(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(path = %path.display(), "failed to remove build workspace: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_dir_is_under_builds_by_deployment_id() {
        let id = Uuid::now_v7();
        let p = build_workspace_dir("/shipyard/data", id);
        assert_eq!(p, PathBuf::from(format!("/shipyard/data/builds/{id}")));
    }

    #[tokio::test]
    async fn prepare_creates_parent_and_clears_stale_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_string_lossy().to_string();
        let id = Uuid::now_v7();

        // Stale leftover from a previous crashed attempt
        let stale = build_workspace_dir(&data_dir, id);
        std::fs::create_dir_all(stale.join(".git")).unwrap();
        std::fs::write(stale.join("README.md"), "old").unwrap();

        let ws = BuildWorkspace::prepare(&data_dir, id).await.unwrap();
        assert!(!ws.path().exists(), "stale workspace must be removed before cloning");
        assert!(tmp.path().join(BUILDS_DIR_NAME).is_dir(), "builds/ parent must exist");
        ws.cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn cleanup_removes_cloned_files() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_string_lossy().to_string();
        let ws = BuildWorkspace::prepare(&data_dir, Uuid::now_v7()).await.unwrap();

        // Simulate a clone + build output
        std::fs::create_dir_all(ws.path().join(".git")).unwrap();
        std::fs::create_dir_all(ws.path().join("node_modules/pkg")).unwrap();
        std::fs::write(ws.path().join("Dockerfile"), "FROM scratch").unwrap();

        let path = ws.path().to_path_buf();
        ws.cleanup().await.unwrap();
        assert!(!path.exists(), "workspace must be gone after cleanup");
    }

    #[tokio::test]
    async fn cleanup_is_ok_when_clone_never_happened() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_string_lossy().to_string();
        let ws = BuildWorkspace::prepare(&data_dir, Uuid::now_v7()).await.unwrap();
        ws.cleanup().await.unwrap();
    }

    #[test]
    fn drop_removes_workspace_on_error_paths() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_string_lossy().to_string();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let ws = rt.block_on(BuildWorkspace::prepare(&data_dir, Uuid::now_v7())).unwrap();
        std::fs::create_dir_all(ws.path().join(".git")).unwrap();
        let path = ws.path().to_path_buf();
        drop(rt);

        // Dropped outside a runtime (e.g. after an early `?` return) → removed synchronously.
        drop(ws);
        assert!(!path.exists(), "workspace must be removed when dropped");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn drop_inside_runtime_removes_workspace_in_background() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().to_string_lossy().to_string();
        let ws = BuildWorkspace::prepare(&data_dir, Uuid::now_v7()).await.unwrap();
        std::fs::create_dir_all(ws.path().join("src")).unwrap();
        let path = ws.path().to_path_buf();
        drop(ws);

        for _ in 0..50 {
            if !path.exists() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("workspace was not removed after drop");
    }
}

//! Git Operations
//!
//! Wraps git2 for cloning repositories, pulling updates, and parsing
//! branches/tags/commits. Milestone 2.10.

use git2::{BranchType, CredentialType, FetchOptions, RemoteCallbacks, Repository, build::CheckoutBuilder};
use shipyard_common::error::{AppError, AppResult};

/// Information about the HEAD commit of a repository.
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub sha: String,
    pub message: String,
    pub author: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// High-level Git operations service.
///
/// All methods are synchronous (git2 is blocking). Call them from async code
/// via `tokio::task::spawn_blocking`.
pub struct GitService {
    pub base_path: String,
}

/// Extract `(username, password)` from an HTTPS URL with embedded credentials.
/// Handles `https://user:pass@host/...`.
fn url_creds(url: &str) -> Option<(String, String)> {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    let at   = rest.find('@')?;
    let cred = &rest[..at];
    let col  = cred.find(':')?;
    Some((cred[..col].to_string(), cred[col + 1..].to_string()))
}

/// Build a `RemoteCallbacks` that supplies `userpass_plaintext` credentials.
fn make_cred_callbacks<'a>(user: String, pass: String) -> RemoteCallbacks<'a> {
    let mut cb = RemoteCallbacks::new();
    cb.credentials(move |_url, _username, _allowed: CredentialType| {
        git2::Cred::userpass_plaintext(&user, &pass)
    });
    cb
}

impl GitService {
    /// Create a new `GitService` rooted at `base_path`.
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
        }
    }

    /// Clone a repository from `url` into `target_path`.
    ///
    /// If `branch` is provided the named branch is checked out after the
    /// initial clone (which defaults to the remote HEAD / default branch).
    ///
    /// `progress_cb` is called with human-readable progress strings during the
    /// transfer phase.
    pub fn clone_repo(
        &self,
        url: &str,
        target_path: &str,
        branch: Option<&str>,
        progress_cb: impl Fn(&str) + Send + 'static,
    ) -> AppResult<()> {
        let creds = url_creds(url);

        let mut callbacks = if let Some((user, pass)) = creds {
            make_cred_callbacks(user, pass)
        } else {
            RemoteCallbacks::new()
        };

        callbacks.transfer_progress(move |stats| {
            let msg = format!(
                "Receiving objects: {}/{} ({} bytes)",
                stats.received_objects(),
                stats.total_objects(),
                stats.received_bytes(),
            );
            progress_cb(&msg);
            true
        });

        let mut fetch_opts = FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(fetch_opts);

        if let Some(b) = branch {
            builder.branch(b);
        }

        builder
            .clone(url, std::path::Path::new(target_path))
            .map_err(|e| AppError::Git(format!("clone failed: {}", e)))?;

        Ok(())
    }

    /// Pull the latest changes on an already-cloned repo (fetch + fast-forward
    /// merge). If `branch` is `None` the current HEAD branch is used.
    /// `authenticated_url` is used to derive credentials and update the remote.
    pub fn pull_repo(&self, repo_path: &str, branch: Option<&str>, authenticated_url: Option<&str>) -> AppResult<()> {
        let repo = Repository::open(repo_path)
            .map_err(|e| AppError::Git(format!("open repo: {}", e)))?;

        if let Some(auth_url) = authenticated_url {
            repo.remote_set_url("origin", auth_url)
                .map_err(|e| AppError::Git(format!("set remote url: {}", e)))?;
        }

        let mut remote = repo
            .find_remote("origin")
            .map_err(|e| AppError::Git(format!("find remote: {}", e)))?;

        // Determine which branch to fetch.
        let branch_name: String = match branch {
            Some(b) => b.to_string(),
            None => {
                // Try to derive from HEAD.
                let head = repo
                    .head()
                    .map_err(|e| AppError::Git(format!("head: {}", e)))?;
                head.shorthand()
                    .unwrap_or("main")
                    .to_string()
            }
        };

        let remote_url = remote.url().unwrap_or("").to_string();
        let mut fetch_opts = FetchOptions::new();
        if let Some((user, pass)) = url_creds(authenticated_url.unwrap_or(&remote_url)) {
            fetch_opts.remote_callbacks(make_cred_callbacks(user, pass));
        }

        remote
            .fetch(&[branch_name.as_str()], Some(&mut fetch_opts), None)
            .map_err(|e| AppError::Git(format!("fetch: {}", e)))?;

        // Locate the FETCH_HEAD reference.
        let fetch_head = repo
            .find_reference("FETCH_HEAD")
            .map_err(|e| AppError::Git(format!("FETCH_HEAD: {}", e)))?;

        let fetch_commit = repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(|e| AppError::Git(format!("annotated commit: {}", e)))?;

        // Analyse the merge.
        let (merge_analysis, _) = repo
            .merge_analysis(&[&fetch_commit])
            .map_err(|e| AppError::Git(format!("merge analysis: {}", e)))?;

        if merge_analysis.is_fast_forward() {
            let refname = format!("refs/heads/{}", branch_name);
            match repo.find_reference(&refname) {
                Ok(mut reference) => {
                    reference
                        .set_target(fetch_commit.id(), "pull: fast-forward")
                        .map_err(|e| AppError::Git(format!("fast-forward set target: {}", e)))?;
                    repo.set_head(&refname)
                        .map_err(|e| AppError::Git(format!("set head: {}", e)))?;
                    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                        .map_err(|e| AppError::Git(format!("checkout head: {}", e)))?;
                }
                Err(_) => {
                    // Reference doesn't exist yet — create it.
                    repo.reference(
                        &refname,
                        fetch_commit.id(),
                        true,
                        "pull: fast-forward (new branch)",
                    )
                    .map_err(|e| AppError::Git(format!("create ref: {}", e)))?;
                    repo.set_head(&refname)
                        .map_err(|e| AppError::Git(format!("set head: {}", e)))?;
                    repo.checkout_head(Some(git2::build::CheckoutBuilder::default().force()))
                        .map_err(|e| AppError::Git(format!("checkout head: {}", e)))?;
                }
            }
        } else if merge_analysis.is_up_to_date() {
            tracing::debug!("pull_repo: already up to date");
        } else {
            return Err(AppError::Git(
                "pull_repo: non-fast-forward merge required; aborting".to_string(),
            ));
        }

        Ok(())
    }

    /// Return the list of remote branches (strips the `origin/` prefix).
    pub fn list_branches(&self, repo_path: &str) -> AppResult<Vec<String>> {
        let repo = Repository::open(repo_path)
            .map_err(|e| AppError::Git(format!("open repo: {}", e)))?;

        let mut names = Vec::new();
        let branches = repo
            .branches(Some(BranchType::Remote))
            .map_err(|e| AppError::Git(format!("list branches: {}", e)))?;

        for item in branches {
            let (branch, _) = item.map_err(|e| AppError::Git(format!("branch iter: {}", e)))?;
            if let Some(name) = branch.name().map_err(|e| AppError::Git(e.to_string()))? {
                // Strip "origin/" prefix, skip "origin/HEAD".
                let short = name.strip_prefix("origin/").unwrap_or(name);
                if short != "HEAD" {
                    names.push(short.to_string());
                }
            }
        }

        Ok(names)
    }

    /// Return the list of tag names in the repository.
    pub fn list_tags(&self, repo_path: &str) -> AppResult<Vec<String>> {
        let repo = Repository::open(repo_path)
            .map_err(|e| AppError::Git(format!("open repo: {}", e)))?;

        let tag_names = repo
            .tag_names(None)
            .map_err(|e| AppError::Git(format!("list tags: {}", e)))?;

        let tags: Vec<String> = tag_names
            .iter()
            .flatten()
            .map(|s| s.to_string())
            .collect();

        Ok(tags)
    }

    /// Return information about the current HEAD commit.
    pub fn head_commit(&self, repo_path: &str) -> AppResult<CommitInfo> {
        let repo = Repository::open(repo_path)
            .map_err(|e| AppError::Git(format!("open repo: {}", e)))?;

        let head = repo
            .head()
            .map_err(|e| AppError::Git(format!("head: {}", e)))?;

        let commit = head
            .peel_to_commit()
            .map_err(|e| AppError::Git(format!("peel to commit: {}", e)))?;

        let sha = commit.id().to_string();
        let message = commit.message().unwrap_or("").trim().to_string();
        let author = commit.author().name().unwrap_or("unknown").to_string();

        // git2 returns seconds since Unix epoch (UTC).
        let secs = commit.time().seconds();
        let timestamp = chrono::DateTime::from_timestamp(secs, 0)
            .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH.into());

        Ok(CommitInfo {
            sha,
            message,
            author,
            timestamp,
        })
    }

    /// Checkout a specific reference (branch name, tag name, or commit SHA).
    /// Resolves the reference using `git2::Repository::revparse_single`,
    /// updates the repository HEAD state, and updates the working tree.
    ///
    /// `authenticated_url` updates the stored remote URL (with embedded
    /// credentials) before fetching so rotated tokens take effect.
    pub fn checkout_ref(&self, repo_path: &str, target_ref: &str, authenticated_url: Option<&str>) -> AppResult<()> {
        let repo = Repository::open(repo_path)
            .map_err(|e| AppError::Git(format!("open repo: {}", e)))?;

        if let Some(auth_url) = authenticated_url {
            repo.remote_set_url("origin", auth_url)
                .map_err(|e| AppError::Git(format!("set remote url: {}", e)))?;
        }

        let mut remote = repo
            .find_remote("origin")
            .map_err(|e| AppError::Git(format!("find remote: {}", e)))?;

        let remote_url = remote.url().unwrap_or("").to_string();
        let mut fetch_opts = FetchOptions::new();
        if let Some((user, pass)) = url_creds(authenticated_url.unwrap_or(&remote_url)) {
            fetch_opts.remote_callbacks(make_cred_callbacks(user, pass));
        }

        remote
            .fetch(&[] as &[&str], Some(&mut fetch_opts), None)
            .map_err(|e| AppError::Git(format!("fetch remote: {}", e)))?;

        // Try origin/<ref> first (remote branches), then direct (tags, SHAs, local refs).
        let remote_ref = format!("origin/{}", target_ref);
        let object = match repo.revparse_single(&remote_ref) {
            Ok(obj) => obj,
            Err(e) => {
                tracing::debug!("revparse '{}' failed ({}), trying direct ref", remote_ref, e);
                repo.revparse_single(target_ref)
                    .map_err(|e2| AppError::Git(format!("resolve ref '{}': {}", target_ref, e2)))?
            }
        };

        let mut checkout_opts = CheckoutBuilder::new();
        checkout_opts.force();

        repo.checkout_tree(&object, Some(&mut checkout_opts))
            .map_err(|e| AppError::Git(format!("checkout tree: {}", e)))?;

        let commit_id = object.id();
        repo.set_head_detached(commit_id)
            .map_err(|e| AppError::Git(format!("set head detached: {}", e)))?;

        Ok(())
    }
}

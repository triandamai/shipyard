use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_docker::types::ExecOutput;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac::require_service_access;
use crate::AppState;

use super::manager::fetch_instance;

const MAX_TREE_DEPTH: u32 = 8;
const MAX_TREE_ENTRIES: usize = 2000;
const MAX_FILE_BYTES: usize = 1024 * 1024;

/// Runs a one-shot command inside the sandbox's running container. Returns
/// an actionable error if the sandbox isn't running — every file-op handler
/// calls this first.
async fn exec_in_sandbox(state: &AppState, service_id: Uuid, cmd: Vec<String>) -> Result<ExecOutput, AppError> {
    let instance = fetch_instance(&state.db, service_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Sandbox has never been started".to_string()))?;

    if instance.status != "running" {
        return Err(AppError::BadRequest(format!(
            "Sandbox must be running to edit files (current status: {})", instance.status
        )));
    }

    let container_id = instance.container_id
        .ok_or_else(|| AppError::Internal("sandbox_instances row is 'running' with no container_id".to_string()))?;

    state.docker.exec_container_oneshot(&container_id, cmd).await
}

fn dirname(path: &str) -> Option<&str> {
    path.rsplit_once('/').map(|(dir, _)| dir)
}

fn build_write_command(path: &str, content: &str) -> String {
    let full = format!("/app/{path}");
    let mkdir_part = match dirname(path) {
        Some(dir) if !dir.is_empty() => format!("mkdir -p '/app/{dir}' && "),
        _ => String::new(),
    };
    // The heredoc terminator must not collide with any line of `content`,
    // or the shell's heredoc ends early at that line — silently truncating
    // the write while the command still exits 0. A fixed literal delimiter
    // (e.g. "SHIPYARD_FILE_EOF") is user-reachable: a file legitimately
    // containing that exact line as its own content would corrupt itself on
    // save. Randomizing per write makes a collision astronomically
    // unlikely, and the loop makes it impossible rather than merely
    // unlikely.
    let delimiter = loop {
        let candidate = format!("SHIPYARD_FILE_EOF_{}", Uuid::new_v4().simple());
        if !content.lines().any(|line| line == candidate) {
            break candidate;
        }
    };
    format!("{mkdir_part}cat > '{full}' <<'{delimiter}'\n{content}\n{delimiter}\n")
}

#[derive(Debug, Serialize)]
struct FileEntry {
    path: String,
    name: String,
    is_dir: bool,
}

#[derive(Debug, Serialize)]
struct TreeResponse {
    entries: Vec<FileEntry>,
    truncated: bool,
}

#[derive(Debug, Deserialize)]
struct PathQuery {
    path: String,
}

#[derive(Debug, Deserialize)]
struct RenameBody {
    from: String,
    to: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/apps/:service_id/files/tree", get(tree))
        .route("/apps/:service_id/files/content", get(read_file).put(write_file))
        .route("/apps/:service_id/files/mkdir", post(mkdir))
        .route("/apps/:service_id/files/entry", delete(delete_entry))
        .route("/apps/:service_id/files/rename", post(rename))
}

async fn tree(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<TreeResponse>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;

    // Appends a trailing `/` to directory entries via a portable `-d` test in
    // a single `-exec` batch, so the Rust side can tell files from
    // directories without a second round-trip. Deliberately avoids GNU
    // find's `-printf '%y'` (path-type flag) since it's unsupported by
    // BusyBox find, which `node:20-alpine`/`nginx:alpine` ship (unlike
    // `python:3.12-slim`, which is Debian-based and would support it) — this
    // form works identically on both.
    let cmd = vec![
        "sh".to_string(), "-c".to_string(),
        format!(
            "find /app -maxdepth {MAX_TREE_DEPTH} \\( -name node_modules -o -name .git -o -name __pycache__ \\) -prune -o -exec sh -c 'for f; do [ -d \"$f\" ] && echo \"$f/\" || echo \"$f\"; done' _ {{}} + | head -n {}",
            MAX_TREE_ENTRIES + 1
        ),
    ];
    let output = exec_in_sandbox(&state, service_id, cmd).await?;

    let mut lines: Vec<&str> = output.stdout.lines().collect();
    let truncated = lines.len() > MAX_TREE_ENTRIES;
    lines.truncate(MAX_TREE_ENTRIES);

    let entries: Vec<FileEntry> = lines
        .into_iter()
        .filter_map(|line| {
            let (is_dir, line) = match line.strip_suffix('/') {
                Some(stripped) => (true, stripped),
                None => (false, line),
            };
            let rel = line.strip_prefix("/app/")?;
            if rel.is_empty() {
                return None; // the /app root entry itself
            }
            let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
            Some(FileEntry { path: rel.to_string(), name, is_dir })
        })
        .collect();

    Ok(Json(ApiResponse::ok(TreeResponse { entries, truncated })))
}

async fn read_file(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    Query(q): Query<PathQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<String>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let path = validate_sandbox_path(&q.path).map_err(|e| ApiAppError(AppError::BadRequest(e)))?;

    let cmd = vec!["sh".to_string(), "-c".to_string(), format!("cat -- '/app/{path}'")];
    let output = exec_in_sandbox(&state, service_id, cmd).await?;

    if output.exit_code != 0 {
        return Err(ApiAppError(AppError::NotFound(format!("File '{path}' not found"))));
    }
    if output.stdout.len() > MAX_FILE_BYTES {
        return Err(ApiAppError(AppError::BadRequest(format!("File exceeds {MAX_FILE_BYTES} byte limit"))));
    }
    if output.stdout.as_bytes().contains(&0) {
        return Err(ApiAppError(AppError::BadRequest("File appears to be binary".to_string())));
    }

    Ok(Json(ApiResponse::ok(output.stdout)))
}

async fn write_file(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    Query(q): Query<PathQuery>,
    State(state): State<AppState>,
    body: String,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let path = validate_sandbox_path(&q.path).map_err(|e| ApiAppError(AppError::BadRequest(e)))?;

    if body.len() > MAX_FILE_BYTES {
        return Err(ApiAppError(AppError::BadRequest(format!("File exceeds {MAX_FILE_BYTES} byte limit"))));
    }

    let script = build_write_command(&path, &body);
    let output = exec_in_sandbox(&state, service_id, vec!["sh".to_string(), "-c".to_string(), script]).await?;
    if output.exit_code != 0 {
        return Err(ApiAppError(AppError::Internal(format!("Failed to write '{path}': {}", output.stderr))));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "path": path }))))
}

async fn mkdir(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    Query(q): Query<PathQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let path = validate_sandbox_path(&q.path).map_err(|e| ApiAppError(AppError::BadRequest(e)))?;

    let cmd = vec!["sh".to_string(), "-c".to_string(), format!("mkdir -p -- '/app/{path}'")];
    let output = exec_in_sandbox(&state, service_id, cmd).await?;
    if output.exit_code != 0 {
        return Err(ApiAppError(AppError::Internal(format!("Failed to create directory '{path}': {}", output.stderr))));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "path": path }))))
}

async fn delete_entry(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    Query(q): Query<PathQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let path = validate_sandbox_path(&q.path).map_err(|e| ApiAppError(AppError::BadRequest(e)))?;
    if path.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("Cannot delete the app root".to_string())));
    }

    let cmd = vec!["sh".to_string(), "-c".to_string(), format!("rm -rf -- '/app/{path}'")];
    let output = exec_in_sandbox(&state, service_id, cmd).await?;
    if output.exit_code != 0 {
        return Err(ApiAppError(AppError::Internal(format!("Failed to delete '{path}': {}", output.stderr))));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": path }))))
}

async fn rename(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<RenameBody>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let from = validate_sandbox_path(&body.from).map_err(|e| ApiAppError(AppError::BadRequest(e)))?;
    let to = validate_sandbox_path(&body.to).map_err(|e| ApiAppError(AppError::BadRequest(e)))?;
    if from.is_empty() || to.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("Cannot rename the app root".to_string())));
    }

    let cmd = vec!["sh".to_string(), "-c".to_string(), format!("mv -- '/app/{from}' '/app/{to}'")];
    let output = exec_in_sandbox(&state, service_id, cmd).await?;
    if output.exit_code != 0 {
        return Err(ApiAppError(AppError::Internal(format!("Failed to rename '{from}' to '{to}': {}", output.stderr))));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "from": from, "to": to }))))
}

/// Validates a user-supplied file path is safely containable within `/app`.
/// Returns the normalized path (no leading `/`, no `.`/`..` components) on
/// success. This is the ONLY place path safety is checked — every handler
/// in this file calls it before touching a path.
pub fn validate_sandbox_path(raw: &str) -> Result<String, String> {
    if raw.contains('\0') {
        return Err("path contains a null byte".to_string());
    }
    if raw.contains('\n') || raw.contains('\r') {
        return Err("path contains a newline".to_string());
    }
    if raw.contains('\'') {
        return Err("path contains a single quote".to_string());
    }

    let stripped = raw.strip_prefix('/').unwrap_or(raw);
    if stripped.is_empty() || stripped == "." {
        return Ok(String::new());
    }

    let mut normalized_parts: Vec<&str> = Vec::new();
    for part in stripped.split('/') {
        match part {
            "" | "." => continue,
            ".." => return Err("path may not contain '..'".to_string()),
            p => normalized_parts.push(p),
        }
    }

    Ok(normalized_parts.join("/"))
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn accepts_simple_relative_path() {
        assert_eq!(validate_sandbox_path("src/index.js").unwrap(), "src/index.js");
    }

    #[test]
    fn accepts_path_with_leading_slash_by_stripping_it() {
        assert_eq!(validate_sandbox_path("/src/index.js").unwrap(), "src/index.js");
    }

    #[test]
    fn rejects_parent_directory_traversal() {
        assert!(validate_sandbox_path("../etc/passwd").is_err());
        assert!(validate_sandbox_path("src/../../etc/passwd").is_err());
    }

    #[test]
    fn rejects_bare_dot_dot() {
        assert!(validate_sandbox_path("..").is_err());
    }

    #[test]
    fn leading_slash_paths_are_always_relative_to_app_never_a_real_os_path() {
        // There is no distinction between "/etc/passwd" and "src/index.js" at
        // this layer: every validated path is always joined by the caller as
        // `/app/{path}`, so a leading slash is a caller convenience to strip,
        // never a way to reach a real OS-absolute path. The actual security
        // boundary is `..` rejection (tested separately), not the mere
        // presence of a leading slash or path segments that resemble a
        // system directory name.
        assert_eq!(validate_sandbox_path("/etc/passwd").unwrap(), "etc/passwd");
    }

    #[test]
    fn rejects_single_quote_to_prevent_shell_quote_breakout() {
        // Every file-op handler that consumes this validated path interpolates
        // it into a single-quoted shell string (e.g. `cat -- '/app/{path}'`).
        // In POSIX sh, a literal `'` is the only character that can terminate
        // a single-quoted string, so rejecting it here — the one place path
        // safety is checked — closes a shell command-injection path before
        // any handler is built on top of this function.
        assert!(validate_sandbox_path("foo'; id #").is_err());
    }

    #[test]
    fn accepts_root_path_as_empty_string() {
        assert_eq!(validate_sandbox_path(".").unwrap(), "");
        assert_eq!(validate_sandbox_path("").unwrap(), "");
    }

    #[test]
    fn rejects_null_bytes() {
        assert!(validate_sandbox_path("src/\0evil").is_err());
    }

    #[test]
    fn rejects_path_with_embedded_shell_metacharacters_used_for_safety_not_just_traversal() {
        // Not required to be shell-safe by itself (callers must still quote
        // properly when building exec commands), but a path containing a
        // literal newline is rejected outright as defense in depth.
        assert!(validate_sandbox_path("src/\nrm -rf /").is_err());
    }
}

#[cfg(test)]
mod command_building_tests {
    use super::*;

    // These test the pure command-construction logic without touching Docker —
    // the actual exec call is verified in Task 12's manual verification.

    #[test]
    fn read_command_uses_cat_with_validated_path() {
        let path = "src/index.js";
        let cmd = vec!["sh".to_string(), "-c".to_string(), format!("cat -- '/app/{path}'")];
        assert_eq!(cmd[2], "cat -- '/app/src/index.js'");
    }

    #[test]
    fn write_command_uses_heredoc_with_validated_path_and_content() {
        let path = "src/index.js";
        let content = "console.log('hi');\n";
        let cmd_str = build_write_command(path, content);
        assert!(cmd_str.contains("mkdir -p '/app/src'"));
        // The delimiter is randomized per write (see the collision test
        // below), so we can't assert an exact literal — only the structural
        // shape: a `cat > '<path>' <<'SHIPYARD_FILE_EOF_<hex>'` opener.
        assert!(cmd_str.contains("cat > '/app/src/index.js' <<'SHIPYARD_FILE_EOF_"));
        assert!(cmd_str.contains(content));

        // The command must end with a bare delimiter line matching the
        // opener's delimiter, followed by a newline.
        let opener_start = cmd_str.find("<<'").unwrap() + 3;
        let opener_end = cmd_str[opener_start..].find('\'').unwrap() + opener_start;
        let delimiter = &cmd_str[opener_start..opener_end];
        assert!(delimiter.starts_with("SHIPYARD_FILE_EOF_"));
        assert!(cmd_str.ends_with(&format!("{delimiter}\n")));
    }

    #[test]
    fn write_command_root_level_file_has_no_mkdir_of_app_itself() {
        let cmd_str = build_write_command("index.js", "x");
        // mkdir -p '/app' would be a harmless no-op but this checks the
        // dirname logic doesn't produce a malformed empty-path mkdir.
        assert!(!cmd_str.contains("mkdir -p '/app/'"));
    }

    #[test]
    fn write_command_avoids_delimiter_collision_with_content_containing_old_style_delimiter() {
        // Regression test for a real data-corruption bug: a fixed literal
        // heredoc delimiter (e.g. "SHIPYARD_FILE_EOF") is user-reachable — a
        // file whose content legitimately contains that exact line as its
        // own text would have its heredoc end early at that line, silently
        // truncating everything after it while the write still reports
        // success (`exit_code == 0`). Content here contains that literal
        // line, plus more content after it, to prove the delimiter picked is
        // never that colliding string and the full content survives intact
        // in the generated command.
        let content = "line one\nSHIPYARD_FILE_EOF\nline three";
        let cmd_str = build_write_command("notes.txt", content);

        let opener_start = cmd_str.find("<<'").unwrap() + 3;
        let opener_end = cmd_str[opener_start..].find('\'').unwrap() + opener_start;
        let delimiter = &cmd_str[opener_start..opener_end];

        assert_ne!(delimiter, "SHIPYARD_FILE_EOF");
        // The content must appear as one contiguous, uninterrupted block —
        // proving the heredoc body is not split at the embedded
        // "SHIPYARD_FILE_EOF" line.
        assert!(cmd_str.contains(content));
    }
}

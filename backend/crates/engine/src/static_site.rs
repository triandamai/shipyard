//! Static site deployment helpers:
//!   - shipyard.json parsing + framework auto-detection
//!   - nginx site.conf rendering
//!   - file publishing helpers (symlink swap, version pruning)

use serde::{Deserialize, Serialize};
use shipyard_common::error::{AppError, AppResult};
use std::path::{Path, PathBuf};

// ─── shipyard.json schema ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct DeployConfig {
    /// Build-time overrides (git source only).
    #[serde(default)]
    pub build: Option<BuildConfig>,
    /// Serve an SPA — unknown paths fall back to /index.html.
    #[serde(default)]
    pub spa: bool,
    /// Custom error page paths (relative to output dir).
    #[serde(default)]
    pub error_pages: ErrorPages,
    /// HTTP redirects (evaluated before rewrites).
    #[serde(default)]
    pub redirects: Vec<Redirect>,
    /// Internal URL rewrites (URL stays the same in browser).
    #[serde(default)]
    pub rewrites: Vec<Rewrite>,
    /// Custom response headers per path pattern.
    #[serde(default)]
    pub headers: Vec<HeaderRule>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BuildConfig {
    pub command: Option<String>,
    pub output: Option<String>,
    pub node_version: Option<String>,
    pub install_command: Option<String>,
    pub image: Option<String>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ErrorPages {
    #[serde(rename = "404")]
    pub not_found: Option<String>,
    #[serde(rename = "500")]
    pub server_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Redirect {
    pub src: String,
    pub dest: String,
    #[serde(default = "default_redirect_status")]
    pub status: u16,
}
fn default_redirect_status() -> u16 { 301 }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rewrite {
    pub src: String,
    pub dest: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeaderRule {
    pub src: String,
    pub values: std::collections::HashMap<String, String>,
}

// ─── Parser ───────────────────────────────────────────────────────────────────

/// Read and parse `shipyard.json` from `dir`.
/// Returns `DeployConfig::default()` if the file is absent (not an error).
/// Fails the deployment if the file exists but contains invalid JSON.
pub fn parse_shipyard_config(dir: &Path) -> AppResult<DeployConfig> {
    let path = dir.join("shipyard.json");
    if path.exists() {
        let raw = std::fs::read_to_string(&path)
            .map_err(|e| AppError::Internal(format!("Failed to read shipyard.json: {e}")))?;
        return serde_json::from_str(&raw)
            .map_err(|e| AppError::Internal(format!("Invalid shipyard.json: {e}")));
    }
    Ok(DeployConfig::default())
}

// ─── Framework auto-detection ─────────────────────────────────────────────────

/// Describes a detected framework's build defaults.
#[derive(Debug, Clone)]
pub struct DetectedBuild {
    pub framework: &'static str,
    pub build_command: &'static str,
    pub output_dir: &'static str,
    pub install_command: &'static str,
    pub node_version: &'static str,
}

/// Detect a frontend framework from config files present in `dir`.
/// Returns `None` if the project is unrecognisable (caller falls back to stored DB config).
pub fn detect_build_config(dir: &Path) -> Option<DetectedBuild> {
    // SvelteKit — svelte.config.{js,ts}
    if dir.join("svelte.config.js").exists() || dir.join("svelte.config.ts").exists() {
        return Some(DetectedBuild {
            framework: "sveltekit",
            build_command: "bun run build",
            output_dir: "build",
            install_command: "bun install",
            node_version: "1",
        });
    }

    // Next.js — next.config.{js,ts,mjs}
    if dir.join("next.config.js").exists()
        || dir.join("next.config.ts").exists()
        || dir.join("next.config.mjs").exists()
    {
        return Some(DetectedBuild {
            framework: "nextjs",
            build_command: "bun run build",
            output_dir: "out",
            install_command: "bun install",
            node_version: "1",
        });
    }

    // Nuxt — nuxt.config.{js,ts}
    if dir.join("nuxt.config.js").exists() || dir.join("nuxt.config.ts").exists() {
        return Some(DetectedBuild {
            framework: "nuxt",
            build_command: "bunx nuxi generate",
            output_dir: ".output/public",
            install_command: "bun install",
            node_version: "1",
        });
    }

    // Astro — astro.config.{js,ts,mjs}
    if dir.join("astro.config.js").exists()
        || dir.join("astro.config.ts").exists()
        || dir.join("astro.config.mjs").exists()
    {
        return Some(DetectedBuild {
            framework: "astro",
            build_command: "bun run build",
            output_dir: "dist",
            install_command: "bun install",
            node_version: "1",
        });
    }

    // Gatsby — gatsby-config.{js,ts}
    if dir.join("gatsby-config.js").exists() || dir.join("gatsby-config.ts").exists() {
        return Some(DetectedBuild {
            framework: "gatsby",
            build_command: "bun run build",
            output_dir: "public",
            install_command: "bun install",
            node_version: "1",
        });
    }

    // Hugo — hugo.toml or (config.toml + content/ directory)
    if dir.join("hugo.toml").exists()
        || (dir.join("config.toml").exists() && dir.join("content").is_dir())
    {
        return Some(DetectedBuild {
            framework: "hugo",
            build_command: "hugo",
            output_dir: "public",
            install_command: "",
            node_version: "",
        });
    }

    // Jekyll — _config.yml
    if dir.join("_config.yml").exists() {
        return Some(DetectedBuild {
            framework: "jekyll",
            build_command: "bundle exec jekyll build",
            output_dir: "_site",
            install_command: "bundle install",
            node_version: "",
        });
    }

    // Vite — vite.config.{js,ts,mjs} (checked after framework-specific configs above)
    if dir.join("vite.config.js").exists()
        || dir.join("vite.config.ts").exists()
        || dir.join("vite.config.mjs").exists()
    {
        return Some(DetectedBuild {
            framework: "vite",
            build_command: "bun run build",
            output_dir: "dist",
            install_command: "bun install",
            node_version: "1",
        });
    }

    // Generic bun/npm project
    if dir.join("package.json").exists() {
        return Some(DetectedBuild {
            framework: "bun",
            build_command: "bun run build",
            output_dir: "dist",
            install_command: "bun install",
            node_version: "1",
        });
    }

    None
}

// ─── Static output validator ──────────────────────────────────────────────────

/// Validate that `output_dir` contains a proper static site — not a server-side app.
///
/// Checks:
/// - Directory exists and is non-empty
/// - At least one `.html` file is present (index.html or nested)
/// - No obvious server-side indicators:
///     - `server/` subdirectory (SvelteKit SSR, Remix, etc.)
///     - `node_modules/` directory in output (bundled server app)
///     - Both `package.json` AND a server entry file (`server.js`, `app.js`, `index.js`)
///       with no `index.html` — indicates a plain Node server, not a static build
pub fn validate_static_output(output_dir: &Path) -> AppResult<()> {
    if !output_dir.exists() {
        return Err(AppError::BadRequest(format!(
            "Output directory '{}' does not exist after build. \
             Check your build_command and output_dir configuration.",
            output_dir.display()
        )));
    }

    let entries: Vec<_> = std::fs::read_dir(output_dir)
        .map_err(|e| AppError::Internal(format!("Cannot read output dir: {e}")))?
        .filter_map(|e| e.ok())
        .collect();

    if entries.is_empty() {
        return Err(AppError::BadRequest(
            "Output directory is empty after build. \
             Verify your build_command produces files in the configured output_dir.".into()
        ));
    }

    // Check for server-side red flags
    let has_server_dir = output_dir.join("server").is_dir();
    let has_node_modules = output_dir.join("node_modules").is_dir();
    let has_package_json = output_dir.join("package.json").is_file();

    if has_server_dir {
        return Err(AppError::BadRequest(
            "Output directory contains a 'server/' subdirectory — this looks like a \
             server-side rendered (SSR) build, not a static site.\n\
             \n\
             For SvelteKit: install @sveltejs/adapter-static and set adapter: adapter() in \
             svelte.config.js, then run build again.\n\
             \n\
             Add a shipyard.json to override the output directory:\n\
             { \"build\": { \"output\": \"build\" } }".into()
        ));
    }

    if has_node_modules {
        return Err(AppError::BadRequest(
            "Output directory contains a 'node_modules/' folder — this looks like a \
             bundled server application, not a static site.\n\
             \n\
             Ensure your build step produces a static HTML/CSS/JS output directory only.".into()
        ));
    }

    // Find at least one HTML file (recursively, up to 3 levels deep)
    let has_html = find_html_recursive(output_dir, 3);

    if has_package_json && !has_html {
        return Err(AppError::BadRequest(
            "Output directory contains package.json but no HTML files — this looks like a \
             Node.js server bundle, not a static site.\n\
             \n\
             Make sure your build command produces static HTML output, or override the \
             output directory in shipyard.json:\n\
             { \"build\": { \"output\": \"dist\" } }\n\
             \n\
             Or Shipyard will auto-detect the framework if shipyard.json is absent.".into()
        ));
    }

    if !has_html {
        return Err(AppError::BadRequest(
            "No HTML files found in the output directory. \
             A static site must contain at least one .html file.\n\
             \n\
             Check that your build_command and output_dir are correct, or add a \
             shipyard.json to your repo to override them:\n\
             { \"build\": { \"command\": \"npm run build\", \"output\": \"dist\" } }\n\
             \n\
             Shipyard will auto-detect the framework when shipyard.json is absent.".into()
        ));
    }

    Ok(())
}

fn find_html_recursive(dir: &Path, depth: usize) -> bool {
    if depth == 0 { return false; }
    let Ok(entries) = std::fs::read_dir(dir) else { return false; };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() {
            if path.extension().map(|e| e == "html").unwrap_or(false) {
                return true;
            }
        } else if path.is_dir() && find_html_recursive(&path, depth - 1) {
            return true;
        }
    }
    false
}

// ─── Custom error page assets ─────────────────────────────────────────────────

/// Branded 404 HTML served for unregistered domains and missing pages.
pub const HTML_404: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>404 – Not Found</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
      background: #0f172a; color: #e2e8f0;
      min-height: 100vh;
      display: flex; flex-direction: column;
      align-items: center; justify-content: center;
      padding: 2rem; gap: 0;
    }
    .num {
      font-size: 6.5rem; font-weight: 800; line-height: 1;
      background: linear-gradient(135deg, #3b82f6 0%, #8b5cf6 100%);
      -webkit-background-clip: text; -webkit-text-fill-color: transparent;
      background-clip: text; margin-bottom: 1.25rem;
    }
    h1 { font-size: 1.4rem; font-weight: 600; color: #f1f5f9; margin-bottom: 0.65rem; }
    p { color: #94a3b8; font-size: 0.9rem; text-align: center; max-width: 380px; margin-bottom: 2.5rem; }
    .badge {
      display: inline-flex; align-items: center; gap: 0.45rem;
      font-size: 0.73rem; color: #475569;
      border: 1px solid #1e293b; border-radius: 9999px;
      padding: 0.35rem 0.9rem;
    }
    .dot { width: 6px; height: 6px; border-radius: 50%; background: #3b82f6; flex-shrink: 0; }
  </style>
</head>
<body>
  <div class="num">404</div>
  <h1>Page Not Found</h1>
  <p>The page you're looking for doesn't exist or hasn't been deployed yet.</p>
  <div class="badge"><span class="dot"></span>Powered by Shipyard</div>
</body>
</html>
"#;

/// Returns the nginx `_default.conf` content for `shipyard-nginx-static`.
///
/// This server block is the fallback for any domain that doesn't match a deployed
/// site's conf — it serves the Shipyard 404 page instead of nginx's plain text response.
pub fn render_default_nginx_conf(sites_base: &str) -> String {
    format!(
        r#"# Shipyard default — serves the custom 404 page for unregistered domains.
server {{
    listen 80 default_server;
    server_name _;

    error_page 404 /_errors/404.html;

    location /_errors/ {{
        root {sites_base};
    }}

    location / {{
        return 404;
    }}
}}
"#
    )
}

// ─── nginx site.conf renderer ─────────────────────────────────────────────────

/// Generate a complete nginx `server {}` block for one static site.
///
/// `sites_base` — absolute path to the static sites base dir (e.g. `/opt/shipyard/data/static`)
/// `serve_root` — absolute path to the `public/` directory inside the current symlink
/// `config`    — parsed shipyard.json runtime config
pub fn render_nginx_site_conf(
    service_id: &str,
    domains: &[String],
    serve_root: &str,
    sites_base: &str,
    config: &DeployConfig,
) -> String {
    // Caller must ensure at least one domain is present — never use a wildcard fallback.
    debug_assert!(!domains.is_empty(), "render_nginx_site_conf called with empty domains");
    if domains.is_empty() {
        return String::new();
    }

    let server_name = domains.join(" ");
    let mut out = String::with_capacity(2048);

    out.push_str(&format!(
        "# site: {service_id}\nserver {{\n    listen 80;\n    server_name {server_name};\n    root {serve_root};\n    index index.html index.htm;\n\n"
    ));

    // Error pages — use shipyard.json override when set, otherwise Shipyard's branded 404.
    if let Some(p) = &config.error_pages.not_found {
        out.push_str(&format!("    error_page 404 /{p};\n"));
    } else {
        out.push_str("    error_page 404 /_errors/404.html;\n");
    }
    if let Some(p) = &config.error_pages.server_error {
        out.push_str(&format!("    error_page 500 502 503 504 /{p};\n"));
    }
    // Shared error-page assets — also fetched directly by Traefik's errors middleware.
    out.push_str(&format!(
        "    location /_errors/ {{\n        root {sites_base};\n    }}\n"
    ));
    out.push('\n');

    // Redirects — each becomes a location = /path { return STATUS /dest; }
    for r in &config.redirects {
        let src = nginx_escape_location(&r.src);
        let status = r.status;
        let dest = &r.dest;
        out.push_str(&format!("    location {src} {{\n        return {status} {dest};\n    }}\n"));
    }
    if !config.redirects.is_empty() {
        out.push('\n');
    }

    // Static asset caching
    out.push_str("    location ~* \\.(js|css|woff2|woff|ttf|png|jpg|jpeg|gif|ico|svg|webp|avif|mp4|mp3)$ {\n");
    out.push_str("        expires 1y;\n");
    out.push_str("        add_header Cache-Control \"public, immutable\";\n");
    out.push_str("        access_log off;\n");
    out.push_str("    }\n\n");

    // Main location: SPA fallback or plain try_files
    if config.spa || !config.rewrites.is_empty() {
        // Rewrites are implemented as try_files fallback to the rewrite dest
        // For simplicity (and because shipyard.json rewrites are almost always SPA-style),
        // we emit try_files to the first rewrite dest or /index.html.
        let fallback = config
            .rewrites
            .first()
            .map(|r| r.dest.clone())
            .unwrap_or_else(|| "/index.html".to_string());
        out.push_str(&format!(
            "    location / {{\n        try_files $uri $uri/ {fallback};\n    }}\n\n"
        ));
    } else {
        out.push_str("    location / {\n        try_files $uri $uri/ =404;\n    }\n\n");
    }

    // Custom headers per path glob
    for rule in &config.headers {
        let src = nginx_escape_location(&rule.src);
        out.push_str(&format!("    location {src} {{\n"));
        for (k, v) in &rule.values {
            out.push_str(&format!("        add_header \"{k}\" \"{v}\" always;\n"));
        }
        out.push_str("    }\n");
    }
    if !config.headers.is_empty() {
        out.push('\n');
    }

    // Default security headers
    out.push_str("    add_header X-Frame-Options \"SAMEORIGIN\" always;\n");
    out.push_str("    add_header X-Content-Type-Options \"nosniff\" always;\n");
    out.push_str("    add_header Referrer-Policy \"strict-origin-when-cross-origin\" always;\n");

    out.push_str("}\n");
    out
}

/// Convert a shipyard.json path pattern to a nginx location argument.
/// Patterns like `/old-page` → exact match `= /old-page`.
/// Patterns with `(.*)` → regex `~ /pattern`.
fn nginx_escape_location(src: &str) -> String {
    if src.contains("(.*)") || src.contains("*") || src.contains("^") {
        // Treat as regex location
        let regex = src.replace("(.*)", "(.*)");
        format!("~ {regex}")
    } else {
        // Exact match
        format!("= {src}")
    }
}

// ─── File publishing helpers ──────────────────────────────────────────────────

/// Determine the directory to actually serve from an uploaded+extracted archive.
///
/// Handles the common case where users zip their build output folder instead of
/// its contents:
///   zip contains: `dist/index.html` `dist/assets/` `shipyard.json`
///   → resolved root: `{extract_dir}/dist`
///
/// Resolution order:
/// 1. `shipyard.json` `build.output` field (e.g. `"dist"`) if it exists as a subdir
/// 2. Auto-detect: exactly one subdirectory and no non-config files → use that subdir
/// 3. Fall back to `extract_dir` itself
pub fn resolve_upload_root(extract_dir: &Path, config: &DeployConfig) -> PathBuf {
    // 1. Explicit output directory in shipyard.json
    if let Some(build) = &config.build {
        if let Some(output) = &build.output {
            if !output.is_empty() {
                let candidate = extract_dir.join(output);
                if candidate.is_dir() {
                    return candidate;
                }
            }
        }
    }

    // 2. Auto-detect single top-level directory wrapping the build output.
    if let Ok(entries) = std::fs::read_dir(extract_dir) {
        let entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
        let subdirs: Vec<_> = entries
            .iter()
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .collect();
        let other_files: Vec<_> = entries
            .iter()
            .filter(|e| {
                e.file_type().map(|t| t.is_file()).unwrap_or(false)
                    && e.file_name() != "shipyard.json"
            })
            .collect();
        if subdirs.len() == 1 && other_files.is_empty() {
            return subdirs[0].path();
        }
    }

    extract_dir.to_path_buf()
}

/// Set world-readable permissions on all files and directories under `dir`.
/// Directories get 0o755, files get 0o644, so the nginx worker (different uid)
/// can always read the published assets.
pub fn fixup_permissions(dir: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry.map_err(|e| AppError::Internal(format!("Walk error: {e}")))?;
        let mode = if entry.file_type().is_dir() { 0o755 } else { 0o644 };
        std::fs::set_permissions(entry.path(), std::fs::Permissions::from_mode(mode))
            .map_err(|e| AppError::Internal(format!("chmod {}: {e}", entry.path().display())))?;
    }
    Ok(())
}

/// Copy all files from `src_dir` into `dest_dir`, creating `dest_dir` if needed.
/// Uses `std::fs` for simplicity; callers should run this in `tokio::task::spawn_blocking`.
pub fn copy_dir_all(src: &Path, dst: &Path) -> AppResult<u64> {
    std::fs::create_dir_all(dst)
        .map_err(|e| AppError::Internal(format!("Failed to create publish dir: {e}")))?;

    let mut total_bytes: u64 = 0;
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| AppError::Internal(format!("Walk error: {e}")))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| AppError::Internal(format!("Strip prefix error: {e}")))?;
        let target = dst.join(rel);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)
                .map_err(|e| AppError::Internal(format!("mkdir {}: {e}", target.display())))?;
        } else {
            total_bytes += std::fs::copy(entry.path(), &target)
                .map_err(|e| AppError::Internal(format!("copy {}: {e}", entry.path().display())))?;
        }
    }
    // Ensure all published files are world-readable so the nginx worker process
    // (different uid than the backend) can serve them.
    fixup_permissions(dst)?;
    Ok(total_bytes)
}

/// Atomically point the `current` symlink to `new_version_dir`.
/// Steps: write temp symlink → rename over `current` (atomic on Linux).
pub fn atomic_swap_symlink(current: &Path, new_version_dir: &Path) -> AppResult<()> {
    let parent = current
        .parent()
        .ok_or_else(|| AppError::Internal("symlink has no parent dir".into()))?;
    let tmp = parent.join(".current.tmp");

    // Remove stale tmp if any
    let _ = std::fs::remove_file(&tmp);

    // Create new symlink at tmp path pointing to the versioned dir name (not absolute path)
    // Use the directory name only so the link stays relative (survives bind-mount relocations).
    let version_name = new_version_dir
        .file_name()
        .ok_or_else(|| AppError::Internal("version dir has no name".into()))?;
    #[cfg(unix)]
    std::os::unix::fs::symlink(version_name, &tmp)
        .map_err(|e| AppError::Internal(format!("symlink create: {e}")))?;
    #[cfg(not(unix))]
    return Err(AppError::Internal("symlinks not supported on this platform".into()));

    // Atomically rename tmp → current
    std::fs::rename(&tmp, current)
        .map_err(|e| AppError::Internal(format!("symlink swap: {e}")))?;

    Ok(())
}

/// Delete old deployment versions beyond `keep` most recent, returning how many were removed.
/// `versions_dir` is the per-service root (`data_dir/static/<service_id>/`).
/// Skips the `current` symlink and the currently live version.
pub fn prune_old_versions(versions_dir: &Path, keep: usize) -> AppResult<usize> {
    let current_link = versions_dir.join("current");
    // Resolve what the current symlink points to
    let live_name: Option<PathBuf> = std::fs::read_link(&current_link).ok();

    // Collect all version dirs (UUID-named, ignore "current" and non-dirs)
    let mut dirs: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(versions_dir)
        .map_err(|e| AppError::Internal(format!("read_dir {}: {e}", versions_dir.display())))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().map(|t| t.is_dir()).unwrap_or(false)
                && e.file_name() != "current"
        })
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let mtime = meta.modified().ok()?;
            Some((mtime, e.path()))
        })
        .collect();

    if dirs.len() <= keep {
        return Ok(0);
    }

    // Sort oldest first
    dirs.sort_by_key(|(t, _)| *t);

    let to_delete = dirs.len() - keep;
    let mut removed = 0;
    for (_, path) in dirs.iter().take(to_delete) {
        // Never delete the currently live version
        if let Some(live) = &live_name {
            if path.file_name() == live.file_name() {
                continue;
            }
        }
        if std::fs::remove_dir_all(path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Extract a zip archive, guarding against zip-slip path traversal.
pub fn extract_zip(archive_path: &Path, dest_dir: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| AppError::Internal(format!("Failed to create extract dir: {e}")))?;

    let file = std::fs::File::open(archive_path)
        .map_err(|e| AppError::Internal(format!("Cannot open archive: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| AppError::Internal(format!("Invalid zip: {e}")))?;

    let dest_canon = dest_dir
        .canonicalize()
        .map_err(|e| AppError::Internal(format!("Cannot canonicalize dest: {e}")))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| AppError::Internal(format!("Zip entry error: {e}")))?;

        // Guard: reject any path that would escape dest_dir (zip-slip)
        let raw_name = entry.name().to_string();
        let safe = sanitize_zip_path(&raw_name)?;
        let out_path = dest_canon.join(&safe);
        if !out_path.starts_with(&dest_canon) {
            return Err(AppError::Internal(format!(
                "Zip-slip detected: {raw_name}"
            )));
        }

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| AppError::Internal(format!("mkdir {}: {e}", out_path.display())))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::Internal(format!("mkdir: {e}")))?;
            }
            let mut out_file = std::fs::File::create(&out_path)
                .map_err(|e| AppError::Internal(format!("create {}: {e}", out_path.display())))?;
            std::io::copy(&mut entry, &mut out_file)
                .map_err(|e| AppError::Internal(format!("extract {}: {e}", raw_name)))?;
        }
    }
    Ok(())
}

fn sanitize_zip_path(raw: &str) -> AppResult<PathBuf> {
    let p = Path::new(raw);
    let mut out = PathBuf::new();
    for component in p.components() {
        match component {
            std::path::Component::Normal(part) => out.push(part),
            std::path::Component::CurDir => {}
            _ => {
                return Err(AppError::Internal(format!(
                    "Unsafe path component in archive: {raw}"
                )))
            }
        }
    }
    Ok(out)
}

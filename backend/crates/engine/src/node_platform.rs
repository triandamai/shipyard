//! Pre-build Node.js cross-platform dependency & lockfile sanitization.
//!
//! Apps developed on Apple Silicon (darwin/arm64) or ARM64 containers often
//! carry platform-specific native packages in `package.json` (`dependencies` /
//! `devDependencies`) or non-optional entries in `package-lock.json` that fail
//! `npm ci` / `npm install` with `EBADPLATFORM` (`Actual cpu: x64`) when
//! Shipyard builds the Docker image on Linux x64 (or vice-versa).
//!
//! This module inspects the ephemeral build workspace before `docker build`,
//! moves incompatible platform-specific packages to `optionalDependencies`,
//! removes incompatible `package-lock.json` files so npm resolves native
//! binaries for the actual container platform, and adjusts `npm ci` / `COPY`
//! instructions in the ephemeral Dockerfile accordingly.

use std::collections::BTreeSet;
use std::path::Path;

/// Target platform of the Docker build container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetPlatform {
    pub os: String,
    pub cpu: String,
    pub libc: Option<String>,
}

/// Summary of adjustments applied to the ephemeral build workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SanitizationReport {
    /// Packages moved from `dependencies`/`devDependencies` to `optionalDependencies`.
    pub moved_to_optional: Vec<String>,
    /// Non-optional packages in `package-lock.json` whose `os`/`cpu`/`libc` excluded the target.
    pub lockfile_incompatible: Vec<String>,
    /// Whether `package-lock.json` was removed in the ephemeral workspace.
    pub removed_lockfile: bool,
    /// Whether the ephemeral Dockerfile was adjusted (`npm ci` -> `npm install` / `COPY`).
    pub patched_dockerfile: bool,
}

impl SanitizationReport {
    pub fn any_changes(&self) -> bool {
        !self.moved_to_optional.is_empty()
            || !self.lockfile_incompatible.is_empty()
            || self.removed_lockfile
            || self.patched_dockerfile
    }

    pub fn summary_message(&self, target: &TargetPlatform) -> String {
        let mut parts = Vec::new();
        if !self.moved_to_optional.is_empty() {
            parts.push(format!(
                "moved foreign-platform packages to optionalDependencies ({})",
                self.moved_to_optional.join(", ")
            ));
        }
        if !self.lockfile_incompatible.is_empty() {
            parts.push(format!(
                "found incompatible non-optional package-lock.json entries ({})",
                self.lockfile_incompatible.join(", ")
            ));
        }
        if self.removed_lockfile {
            parts.push("removed incompatible package-lock.json for fresh native resolution".to_string());
        }
        if self.patched_dockerfile {
            parts.push("switched npm ci to npm install in ephemeral Dockerfile".to_string());
        }
        let target_desc = match &target.libc {
            Some(libc) => format!("{}/{}/{}", target.os, target.cpu, libc),
            None => format!("{}/{}", target.os, target.cpu),
        };
        format!(
            "Auto-adjusted Node.js platform dependencies for build target ({target_desc}): {}.",
            parts.join("; ")
        )
    }
}

/// Map the Rust host architecture to npm's `process.arch` / `cpu` value.
pub fn host_npm_cpu() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "ia32",
        "arm" => "arm",
        "powerpc64" => "ppc64",
        "s390x" => "s390x",
        "riscv64" => "riscv64",
        _ => "x64",
    }
}

/// Determine the Docker container build target from the host architecture and Dockerfile `FROM`.
pub fn detect_dockerfile_target(dockerfile_content: &str) -> TargetPlatform {
    detect_dockerfile_target_with_host_cpu(dockerfile_content, host_npm_cpu())
}

pub fn detect_dockerfile_target_with_host_cpu(
    dockerfile_content: &str,
    default_cpu: &str,
) -> TargetPlatform {
    let mut os = "linux".to_string();
    let mut cpu = default_cpu.to_string();
    let mut libc: Option<String> = None;

    for line in dockerfile_content.lines() {
        let trimmed = line.trim();
        if !trimmed.to_ascii_uppercase().starts_with("FROM ") {
            continue;
        }
        let words: Vec<&str> = trimmed.split_whitespace().collect();
        let mut image_token: Option<&str> = None;
        for word in words.iter().skip(1) {
            if let Some(plat) = word
                .strip_prefix("--platform=")
                .or_else(|| word.strip_prefix("--PLATFORM="))
            {
                let p = plat.to_ascii_lowercase();
                if let Some((p_os, p_arch)) = p.split_once('/') {
                    if !p_os.is_empty() && !p_os.starts_with('$') {
                        os = p_os.to_string();
                    }
                    let arch = p_arch.split('/').next().unwrap_or("");
                    match arch {
                        "amd64" | "x86_64" => cpu = "x64".to_string(),
                        "arm64" | "aarch64" => cpu = "arm64".to_string(),
                        "386" | "i386" => cpu = "ia32".to_string(),
                        "arm" => cpu = "arm".to_string(),
                        _ => {}
                    }
                }
                continue;
            }
            if word.starts_with("--") {
                continue;
            }
            image_token = Some(*word);
            break;
        }

        if let Some(img) = image_token {
            let lower = img.to_ascii_lowercase();
            if lower == "scratch" {
                continue;
            }
            if lower.contains("alpine") {
                libc = Some("musl".to_string());
            } else if lower.starts_with("node:")
                || lower.starts_with("docker.io/library/node:")
                || lower.starts_with("debian:")
                || lower.starts_with("ubuntu:")
                || lower.contains("-slim")
                || lower.contains("-bookworm")
                || lower.contains("-bullseye")
                || lower.contains("-buster")
                || lower.contains("-trixie")
            {
                libc = Some("glibc".to_string());
            }
            // Use the first build stage (where npm ci / npm install runs).
            break;
        }
    }

    TargetPlatform { os, cpu, libc }
}

/// Exact implementation of npm's `npm-install-checks` `checkList` semantics.
fn npm_check_list(actual: &str, list: &[String]) -> bool {
    if list.is_empty() {
        return true;
    }
    if list.len() == 1 && list[0] == "any" {
        return true;
    }
    let mut negated = 0usize;
    let mut matched = false;
    for entry in list {
        if let Some(neg) = entry.strip_prefix('!') {
            negated += 1;
            if actual == neg {
                return false;
            }
        } else if actual == entry {
            matched = true;
        }
    }
    matched || negated == list.len()
}

fn json_string_list(val: Option<&serde_json::Value>) -> Vec<String> {
    match val {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        Some(serde_json::Value::String(s)) => vec![s.clone()],
        _ => Vec::new(),
    }
}

fn lock_entry_compatible(entry: &serde_json::Value, target: &TargetPlatform) -> bool {
    let os_list = json_string_list(entry.get("os"));
    if !os_list.is_empty() && !npm_check_list(&target.os, &os_list) {
        return false;
    }
    let cpu_list = json_string_list(entry.get("cpu"));
    if !cpu_list.is_empty() && !npm_check_list(&target.cpu, &cpu_list) {
        return false;
    }
    if let Some(target_libc) = &target.libc {
        let libc_list = json_string_list(entry.get("libc"));
        if !libc_list.is_empty() && !npm_check_list(target_libc, &libc_list) {
            return false;
        }
    }
    true
}

fn extract_lock_package_name(pkg_path: &str, entry: &serde_json::Value) -> Option<String> {
    if let Some(name) = entry.get("name").and_then(|v| v.as_str()) {
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    let tail = pkg_path.rsplit("node_modules/").next()?;
    let trimmed = tail.trim_matches('/');
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Normalize an npm package CPU token to npm's canonical `process.arch` value.
fn normalize_cpu_token(token: &str) -> Option<&'static str> {
    match token {
        "x64" | "x86_64" | "amd64" | "64" => Some("x64"),
        "arm64" | "aarch64" => Some("arm64"),
        "ia32" | "x86" | "32" => Some("ia32"),
        "arm" => Some("arm"),
        "universal" => Some("universal"),
        "ppc64" => Some("ppc64"),
        "ppc64le" => Some("ppc64le"),
        "s390x" => Some("s390x"),
        "riscv64" => Some("riscv64"),
        "loong64" => Some("loong64"),
        "mips64el" => Some("mips64el"),
        _ => None,
    }
}

fn parse_os_token(token: &str) -> Option<(&'static str, Option<&'static str>)> {
    match token {
        "darwin" => Some(("darwin", None)),
        "win32" => Some(("win32", None)),
        "linux" => Some(("linux", None)),
        "linuxmusl" => Some(("linux", Some("musl"))),
        "freebsd" => Some(("freebsd", None)),
        "openbsd" => Some(("openbsd", None)),
        "netbsd" => Some(("netbsd", None)),
        "sunos" => Some(("sunos", None)),
        "android" => Some(("android", None)),
        "aix" => Some(("aix", None)),
        _ => None,
    }
}

fn is_abi_token(token: &str) -> bool {
    matches!(
        token,
        "gnu"
            | "glibc"
            | "musl"
            | "msvc"
            | "eabi"
            | "gnueabi"
            | "gnueabihf"
            | "eabihf"
            | "gnuabi64"
            | "gnuabihf"
    )
}

fn abi_to_libc(abi: &str) -> Option<&'static str> {
    match abi {
        "gnu" | "glibc" | "gnueabi" | "gnueabihf" | "gnuabi64" | "gnuabihf" => Some("glibc"),
        "musl" => Some("musl"),
        _ => None,
    }
}

/// Returns `true` if `pkg_name` unambiguously names a prebuilt native binary
/// package targeting a different OS, CPU architecture, or libc than `target`.
pub fn is_foreign_platform_package_name(pkg_name: &str, target: &TargetPlatform) -> bool {
    if pkg_name == "fsevents" {
        return target.os != "darwin";
    }

    let (is_scoped, base) = if let Some(rest) = pkg_name.strip_prefix('@') {
        match rest.split_once('/') {
            Some((_, name)) => (true, name),
            None => return false,
        }
    } else {
        (false, pkg_name)
    };

    let lower = base.to_ascii_lowercase();
    let tokens: Vec<&str> = lower.split('-').collect();
    if tokens.len() < 2 {
        return false;
    }

    // Match trailing `<os>-<cpu>` or `<os>-<cpu>-<abi>`.
    // Unscoped packages require at least one prefix token before `<os>`
    // (e.g. `lightningcss-darwin-arm64`), while scoped packages also allow
    // `<os>` at index 0 (e.g. `@esbuild/darwin-arm64`).
    let min_os_idx = if is_scoped { 0 } else { 1 };
    let candidates: &[(usize, Option<usize>)] = &[
        (tokens.len().saturating_sub(2), None),
        (tokens.len().saturating_sub(3), Some(tokens.len() - 1)),
    ];

    for &(os_idx, abi_idx) in candidates {
        if os_idx < min_os_idx || os_idx + 1 >= tokens.len() {
            continue;
        }
        if let Some(ai) = abi_idx {
            if ai != os_idx + 2 || !is_abi_token(tokens[ai]) {
                continue;
            }
        }
        let Some((os_norm, os_implied_libc)) = parse_os_token(tokens[os_idx]) else {
            continue;
        };
        let Some(cpu_norm) = normalize_cpu_token(tokens[os_idx + 1]) else {
            continue;
        };

        if os_norm != target.os {
            return true;
        }
        if cpu_norm != "universal" && cpu_norm != target.cpu {
            return true;
        }
        let pkg_libc = abi_idx
            .and_then(|ai| abi_to_libc(tokens[ai]))
            .or(os_implied_libc);
        if let (Some(target_libc), Some(pkg_libc)) = (target.libc.as_deref(), pkg_libc) {
            if pkg_libc != target_libc {
                return true;
            }
        }
        return false;
    }

    false
}

/// Replace standalone `npm ci` invocations with `npm install` on a line,
/// preserving flags (e.g. `--omit=dev`, `--legacy-peer-deps`).
fn replace_npm_ci_in_line(line: &str) -> (String, bool) {
    let mut out = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut changed = false;

    while i < bytes.len() {
        if line[i..].starts_with("npm ci") {
            let prev_ok = i == 0
                || matches!(
                    bytes[i - 1],
                    b' ' | b'\t' | b';' | b'&' | b'|' | b'(' | b'"' | b'\''
                );
            let after_idx = i + "npm ci".len();
            let next_ok = after_idx == bytes.len()
                || matches!(
                    bytes[after_idx],
                    b' ' | b'\t' | b';' | b'&' | b'|' | b')' | b'"' | b'\'' | b'\\'
                );
            if prev_ok && next_ok {
                out.push_str("npm install");
                i = after_idx;
                changed = true;
                continue;
            }
        }
        let ch = line[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }

    (out, changed)
}

/// Update `COPY` / `ADD` lines that reference `package-lock.json` without a glob
/// so removing an incompatible lockfile does not cause Docker `COPY` to fail.
fn adjust_copy_lockfile_line(line: &str) -> (String, bool) {
    let trimmed = line.trim_start();
    let upper = trimmed.to_ascii_uppercase();
    if !(upper.starts_with("COPY ") || upper.starts_with("ADD ")) {
        return (line.to_string(), false);
    }
    if !trimmed.contains("package-lock.json") {
        return (line.to_string(), false);
    }
    // Skip JSON array syntax (COPY ["a", "b"]) to avoid corrupting JSON quoting.
    if trimmed.contains('[') {
        return (line.to_string(), false);
    }

    let leading_ws = &line[..line.len() - trimmed.len()];
    let tokens: Vec<&str> = trimmed.split_whitespace().collect();
    if tokens.len() < 3 {
        return (line.to_string(), false);
    }

    // Separate instruction + flags, source paths, and destination path.
    let mut idx = 1;
    while idx < tokens.len() && tokens[idx].starts_with("--") {
        idx += 1;
    }
    // Remaining tokens: [sources..., dest]
    if tokens.len().saturating_sub(idx) < 2 {
        return (line.to_string(), false);
    }
    let src_tokens = &tokens[idx..tokens.len() - 1];

    let is_exact_lockfile = |tok: &str| {
        tok == "package-lock.json"
            || tok == "./package-lock.json"
            || tok.ends_with("/package-lock.json")
    };

    if !src_tokens.iter().any(|t| is_exact_lockfile(t)) {
        return (line.to_string(), false);
    }

    // If package-lock.json is the ONLY source in this COPY instruction, comment it out.
    if src_tokens.len() == 1 && is_exact_lockfile(src_tokens[0]) {
        return (
            format!("{leading_ws}# {trimmed} # skipped removed incompatible package-lock.json"),
            true,
        );
    }

    // Otherwise append `*` so Docker matches it optionally alongside package.json.
    let mut rebuilt = Vec::with_capacity(tokens.len());
    for (pos, tok) in tokens.iter().enumerate() {
        if pos >= idx && pos < tokens.len() - 1 && is_exact_lockfile(tok) {
            rebuilt.push(format!("{tok}*"));
        } else {
            rebuilt.push((*tok).to_string());
        }
    }
    (format!("{leading_ws}{}", rebuilt.join(" ")), true)
}

/// Patch an ephemeral Dockerfile after removing an incompatible `package-lock.json`:
/// switches `npm ci` -> `npm install` and makes exact `COPY ... package-lock.json` optional.
pub fn rewrite_dockerfile_for_removed_lockfile(content: &str) -> (String, bool) {
    let mut changed = false;
    let mut out_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            out_lines.push(line.to_string());
            continue;
        }
        let (line1, c1) = adjust_copy_lockfile_line(line);
        let (line2, c2) = replace_npm_ci_in_line(&line1);
        if c1 || c2 {
            changed = true;
        }
        out_lines.push(line2);
    }

    let mut result = out_lines.join("\n");
    if content.ends_with('\n') {
        result.push('\n');
    }
    (result, changed)
}

/// Sanitize `package.json`, `package-lock.json`, and the ephemeral Dockerfile
/// inside `repo_path` for the container's target OS/CPU/libc.
pub fn sanitize_node_platform_for_build(
    repo_path: &Path,
    dockerfile_rel: &str,
) -> Option<(TargetPlatform, SanitizationReport)> {
    sanitize_node_platform_for_build_with_target(repo_path, dockerfile_rel, None)
}

pub fn sanitize_node_platform_for_build_with_target(
    repo_path: &Path,
    dockerfile_rel: &str,
    explicit_target: Option<TargetPlatform>,
) -> Option<(TargetPlatform, SanitizationReport)> {
    let pkg_json_path = repo_path.join("package.json");
    if !pkg_json_path.exists() {
        return None;
    }

    let dockerfile_path = repo_path.join(dockerfile_rel);
    let dockerfile_content = std::fs::read_to_string(&dockerfile_path).unwrap_or_default();
    let target = explicit_target.unwrap_or_else(|| detect_dockerfile_target(&dockerfile_content));

    let mut report = SanitizationReport::default();
    let mut lock_incompatible_set: BTreeSet<String> = BTreeSet::new();

    // 1. Inspect package-lock.json for non-optional entries incompatible with target.
    let lock_path = repo_path.join("package-lock.json");
    if lock_path.exists() {
        if let Ok(lock_str) = std::fs::read_to_string(&lock_path) {
            if let Ok(lock_val) = serde_json::from_str::<serde_json::Value>(&lock_str) {
                if let Some(packages) = lock_val.get("packages").and_then(|v| v.as_object()) {
                    for (pkg_path, entry) in packages {
                        if pkg_path.is_empty() {
                            continue;
                        }
                        let is_optional = entry
                            .get("optional")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        if is_optional {
                            continue;
                        }
                        if !lock_entry_compatible(entry, &target) {
                            if let Some(name) = extract_lock_package_name(pkg_path, entry) {
                                lock_incompatible_set.insert(name);
                            }
                        }
                    }
                }
            }
        }
    }

    report.lockfile_incompatible = lock_incompatible_set.iter().cloned().collect();

    // 2. Inspect package.json dependencies & devDependencies and move incompatible
    //    platform-specific packages to optionalDependencies.
    if let Ok(pkg_str) = std::fs::read_to_string(&pkg_json_path) {
        if let Ok(mut pkg_val) = serde_json::from_str::<serde_json::Value>(&pkg_str) {
            let mut to_move: Vec<(String, serde_json::Value)> = Vec::new();
            for section in ["dependencies", "devDependencies"] {
                if let Some(map) = pkg_val.get_mut(section).and_then(|v| v.as_object_mut()) {
                    let bad_keys: Vec<String> = map
                        .keys()
                        .filter(|k| {
                            lock_incompatible_set.contains(*k)
                                || is_foreign_platform_package_name(k, &target)
                        })
                        .cloned()
                        .collect();
                    for key in bad_keys {
                        if let Some(ver) = map.remove(&key) {
                            to_move.push((key, ver));
                        }
                    }
                }
            }

            if !to_move.is_empty() {
                if let Some(root_obj) = pkg_val.as_object_mut() {
                    let opt_entry = root_obj
                        .entry("optionalDependencies")
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let Some(opt_map) = opt_entry.as_object_mut() {
                        for (name, ver) in &to_move {
                            opt_map.entry(name.clone()).or_insert_with(|| ver.clone());
                            report.moved_to_optional.push(name.clone());
                        }
                    }
                }
                if let Ok(serialized) = serde_json::to_string_pretty(&pkg_val) {
                    let _ = std::fs::write(&pkg_json_path, format!("{serialized}\n"));
                }
            }
        }
    }

    // 3. If package.json changed or lockfile has incompatible non-optional entries,
    //    remove package-lock.json in this ephemeral build workspace and patch the
    //    ephemeral Dockerfile so npm resolves fresh native binaries for target.
    if (!report.moved_to_optional.is_empty() || !report.lockfile_incompatible.is_empty())
        && lock_path.exists()
    {
        if std::fs::remove_file(&lock_path).is_ok() {
            report.removed_lockfile = true;
        }
        if !dockerfile_content.is_empty() {
            let (patched, changed) =
                rewrite_dockerfile_for_removed_lockfile(&dockerfile_content);
            if changed && std::fs::write(&dockerfile_path, patched).is_ok() {
                report.patched_dockerfile = true;
            }
        }
    }

    if report.any_changes() {
        Some((target, report))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn linux_x64_musl() -> TargetPlatform {
        TargetPlatform {
            os: "linux".into(),
            cpu: "x64".into(),
            libc: Some("musl".into()),
        }
    }

    fn linux_x64_glibc() -> TargetPlatform {
        TargetPlatform {
            os: "linux".into(),
            cpu: "x64".into(),
            libc: Some("glibc".into()),
        }
    }

    #[test]
    fn detects_dockerfile_target_alpine_and_debian() {
        let t_alpine = detect_dockerfile_target_with_host_cpu("FROM node:22-alpine AS build\n", "x64");
        assert_eq!(t_alpine, linux_x64_musl());

        let t_debian = detect_dockerfile_target_with_host_cpu("FROM node:22-slim\n", "x64");
        assert_eq!(t_debian, linux_x64_glibc());

        let t_arm = detect_dockerfile_target_with_host_cpu(
            "FROM --platform=linux/arm64 node:22-alpine AS build\n",
            "x64",
        );
        assert_eq!(t_arm.cpu, "arm64");
        assert_eq!(t_arm.libc.as_deref(), Some("musl"));
    }

    #[test]
    fn identifies_foreign_platform_packages_without_false_positives() {
        let musl = linux_x64_musl();
        // Foreign OS / CPU / libc
        assert!(is_foreign_platform_package_name("@esbuild/darwin-arm64", &musl));
        assert!(is_foreign_platform_package_name("@esbuild/linux-arm64", &musl));
        assert!(is_foreign_platform_package_name("@rollup/rollup-darwin-arm64", &musl));
        assert!(is_foreign_platform_package_name("@rollup/rollup-linux-arm64-gnu", &musl));
        assert!(is_foreign_platform_package_name("@rollup/rollup-linux-arm64-musl", &musl));
        assert!(is_foreign_platform_package_name("@rollup/rollup-linux-x64-gnu", &musl));
        assert!(is_foreign_platform_package_name("@tailwindcss/oxide-darwin-arm64", &musl));
        assert!(is_foreign_platform_package_name("@tailwindcss/oxide-linux-arm64-gnu", &musl));
        assert!(is_foreign_platform_package_name("@swc/core-win32-x64-msvc", &musl));
        assert!(is_foreign_platform_package_name("lightningcss-darwin-arm64", &musl));
        assert!(is_foreign_platform_package_name("@img/sharp-libvips-darwin-arm64", &musl));
        assert!(is_foreign_platform_package_name("@img/sharp-linuxmusl-arm64", &musl));
        assert!(is_foreign_platform_package_name("@parcel/watcher-linux-arm64-glibc", &musl));
        assert!(is_foreign_platform_package_name("fsevents", &musl));

        // Compatible with linux/x64/musl
        assert!(!is_foreign_platform_package_name("@esbuild/linux-x64", &musl));
        assert!(!is_foreign_platform_package_name("@rollup/rollup-linux-x64-musl", &musl));
        assert!(!is_foreign_platform_package_name("@tailwindcss/oxide-linux-x64-musl", &musl));
        assert!(!is_foreign_platform_package_name("@img/sharp-linuxmusl-x64", &musl));

        // Compatible with linux/x64/glibc (but musl is foreign on glibc)
        let glibc = linux_x64_glibc();
        assert!(!is_foreign_platform_package_name("@rollup/rollup-linux-x64-gnu", &glibc));
        assert!(is_foreign_platform_package_name("@rollup/rollup-linux-x64-musl", &glibc));
        assert!(is_foreign_platform_package_name("@img/sharp-linuxmusl-x64", &glibc));

        // Normal JS packages must never be flagged
        for normal in [
            "react",
            "next",
            "vite",
            "svelte",
            "@sveltejs/adapter-node",
            "tailwindcss",
            "drizzle-orm",
            "pg",
            "esbuild",
            "rollup",
            "sharp",
            "lightningcss",
            "@types/node",
            "lucide-react",
        ] {
            assert!(
                !is_foreign_platform_package_name(normal, &musl),
                "unexpectedly flagged normal package {normal}"
            );
        }
    }

    #[test]
    fn rewrites_dockerfile_npm_ci_and_copy_lockfile() {
        let df = "\
FROM node:22-alpine AS build
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci
COPY . .
RUN npm run build && npm ci --omit=dev
";
        let (out, changed) = rewrite_dockerfile_for_removed_lockfile(df);
        assert!(changed);
        assert!(out.contains("COPY package.json package-lock.json* ./"));
        assert!(out.contains("RUN npm install\n"));
        assert!(out.contains("RUN npm run build && npm install --omit=dev\n"));
        assert!(!out.contains("npm ci"));

        // Standalone COPY package-lock.json line gets commented out
        let df2 = "COPY package.json ./\nCOPY package-lock.json ./\nRUN npm ci\n";
        let (out2, changed2) = rewrite_dockerfile_for_removed_lockfile(df2);
        assert!(changed2);
        assert!(out2.contains("# COPY package-lock.json ./"));
        assert!(out2.contains("RUN npm install"));
    }

    #[test]
    fn sanitizes_arm64_dep_and_lockfile_in_workspace() {
        let dir = tempdir().unwrap();
        std::fs::write(
            dir.path().join("Dockerfile"),
            "FROM node:22-alpine AS build\nWORKDIR /app\nCOPY package*.json ./\nRUN npm ci\nCOPY . .\nRUN npm run build\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{
  "name": "nomi-app",
  "dependencies": {
    "next": "15.0.0",
    "@esbuild/darwin-arm64": "0.25.0"
  },
  "devDependencies": {
    "@rollup/rollup-linux-arm64-gnu": "4.24.0",
    "typescript": "5.6.0"
  }
}"#,
        )
        .unwrap();
        std::fs::write(
            dir.path().join("package-lock.json"),
            r#"{
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "nomi-app" },
    "node_modules/@esbuild/darwin-arm64": {
      "version": "0.25.0",
      "os": ["darwin"],
      "cpu": ["arm64"]
    },
    "node_modules/some-custom-arm64-tool": {
      "version": "1.0.0",
      "os": ["linux"],
      "cpu": ["arm64"]
    },
    "node_modules/@esbuild/win32-x64": {
      "version": "0.25.0",
      "os": ["win32"],
      "cpu": ["x64"],
      "optional": true
    }
  }
}"#,
        )
        .unwrap();

        let (_, report) = sanitize_node_platform_for_build_with_target(
            dir.path(),
            "Dockerfile",
            Some(linux_x64_musl()),
        )
        .expect("should sanitize");

        assert!(report.moved_to_optional.contains(&"@esbuild/darwin-arm64".to_string()));
        assert!(report
            .moved_to_optional
            .contains(&"@rollup/rollup-linux-arm64-gnu".to_string()));
        assert!(report
            .lockfile_incompatible
            .contains(&"some-custom-arm64-tool".to_string()));
        // Optional win32 entry should NOT be reported as incompatible
        assert!(!report
            .lockfile_incompatible
            .contains(&"@esbuild/win32-x64".to_string()));
        assert!(report.removed_lockfile);
        assert!(report.patched_dockerfile);
        assert!(!dir.path().join("package-lock.json").exists());

        let updated_pkg: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("package.json")).unwrap())
                .unwrap();
        assert!(updated_pkg["dependencies"].get("@esbuild/darwin-arm64").is_none());
        assert!(updated_pkg["dependencies"].get("next").is_some());
        assert!(updated_pkg["devDependencies"]
            .get("@rollup/rollup-linux-arm64-gnu")
            .is_none());
        assert!(updated_pkg["devDependencies"].get("typescript").is_some());
        assert_eq!(
            updated_pkg["optionalDependencies"]["@esbuild/darwin-arm64"],
            "0.25.0"
        );
        assert_eq!(
            updated_pkg["optionalDependencies"]["@rollup/rollup-linux-arm64-gnu"],
            "4.24.0"
        );

        let updated_df = std::fs::read_to_string(dir.path().join("Dockerfile")).unwrap();
        assert!(updated_df.contains("RUN npm install"));
        assert!(!updated_df.contains("RUN npm ci"));
    }

    #[test]
    fn leaves_healthy_project_completely_untouched() {
        let dir = tempdir().unwrap();
        let orig_df = "FROM node:22-alpine\nCOPY package*.json ./\nRUN npm ci\n";
        let orig_pkg = r#"{"name":"ok","dependencies":{"react":"19.0.0"}}"#;
        let orig_lock = r#"{
  "lockfileVersion": 3,
  "packages": {
    "": { "name": "ok" },
    "node_modules/@esbuild/darwin-arm64": {
      "version": "0.25.0",
      "os": ["darwin"],
      "cpu": ["arm64"],
      "optional": true
    },
    "node_modules/@esbuild/linux-x64": {
      "version": "0.25.0",
      "os": ["linux"],
      "cpu": ["x64"],
      "optional": true
    }
  }
}"#;
        std::fs::write(dir.path().join("Dockerfile"), orig_df).unwrap();
        std::fs::write(dir.path().join("package.json"), orig_pkg).unwrap();
        std::fs::write(dir.path().join("package-lock.json"), orig_lock).unwrap();

        let res = sanitize_node_platform_for_build_with_target(
            dir.path(),
            "Dockerfile",
            Some(linux_x64_musl()),
        );
        assert!(res.is_none());
        assert!(dir.path().join("package-lock.json").exists());
        assert_eq!(
            std::fs::read_to_string(dir.path().join("Dockerfile")).unwrap(),
            orig_df
        );
    }
}

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

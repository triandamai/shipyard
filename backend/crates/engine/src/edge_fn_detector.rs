use std::path::Path;

use crate::edge_fn_config::{FunctionEntry, FunctionsConfig, ShipyardConfig};

#[derive(Debug, Clone)]
pub struct DetectedFunction {
    pub name: String,
    pub file_path: String,
    pub code: String,
    pub runtime: String,
    pub timeout_secs: u32,
    pub env_whitelist: Vec<String>,
}

/// Detect edge functions from a repo/zip root directory.
///
/// Resolution order:
///   1. shipyard.json with `functions.entries` → use those, skip dir scan
///   2. shipyard.json with `functions.dir`     → scan that directory
///   3. fallback                               → scan `functions/` at root
pub fn detect(root: &Path) -> Vec<DetectedFunction> {
    let config_path = root.join("shipyard.json");
    let config = ShipyardConfig::from_file(&config_path);

    match config.and_then(|c| c.functions) {
        Some(FunctionsConfig { entries: Some(entries), runtime, .. }) => {
            let global_runtime = runtime.unwrap_or_else(|| "deno".to_string());
            entries
                .into_iter()
                .filter_map(|(name, entry)| resolve_entry(root, name, entry, &global_runtime))
                .collect()
        }
        Some(FunctionsConfig { dir: Some(dir), runtime, entries: None }) => {
            let global_runtime = runtime.unwrap_or_else(|| "deno".to_string());
            scan_directory(&root.join(&dir), &global_runtime)
        }
        _ => scan_directory(&root.join("functions"), "deno"),
    }
}

fn resolve_entry(
    root: &Path,
    name: String,
    entry: FunctionEntry,
    global_runtime: &str,
) -> Option<DetectedFunction> {
    let abs = root.join(&entry.file);
    let code = std::fs::read_to_string(&abs).ok()?;
    if !has_default_export(&code) {
        tracing::warn!("edge function '{}' at '{}' has no default export — skipped", name, entry.file);
        return None;
    }
    Some(DetectedFunction {
        name: to_kebab_case(&name),
        file_path: entry.file,
        code,
        runtime: global_runtime.to_string(),
        timeout_secs: entry.timeout.unwrap_or(10),
        env_whitelist: entry.env.unwrap_or_default(),
    })
}

fn scan_directory(dir: &Path, runtime: &str) -> Vec<DetectedFunction> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    entries
        .filter_map(|e| e.ok())
        .filter_map(|entry| {
            let path = entry.path();
            if path.is_dir() { return None; }
            let ext = path.extension()?.to_str()?;
            if ext != "ts" && ext != "js" { return None; }

            let stem = path.file_stem()?.to_str()?;
            let name = to_kebab_case(stem);
            let code = std::fs::read_to_string(&path).ok()?;

            if !has_default_export(&code) {
                tracing::warn!("edge function file '{}' has no default export — skipped", path.display());
                return None;
            }

            let rel = path.strip_prefix(dir.parent().unwrap_or(dir))
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            Some(DetectedFunction {
                name,
                file_path: rel,
                code,
                runtime: runtime.to_string(),
                timeout_secs: 10,
                env_whitelist: vec![],
            })
        })
        .collect()
}

/// Minimal check: file must contain "export default".
fn has_default_export(code: &str) -> bool {
    code.contains("export default")
}

/// Convert camelCase or PascalCase to kebab-case.
/// "sendEmail" → "send-email", "ResizeImage" → "resize-image"
fn to_kebab_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('-');
        }
        out.push(c.to_ascii_lowercase());
    }
    // replace underscores/spaces with hyphens
    out.replace(['_', ' '], "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_conversion() {
        assert_eq!(to_kebab_case("sendEmail"), "send-email");
        assert_eq!(to_kebab_case("ResizeImage"), "resize-image");
        assert_eq!(to_kebab_case("hello"), "hello");
        assert_eq!(to_kebab_case("my_function"), "my-function");
    }

    #[test]
    fn detects_default_export() {
        assert!(has_default_export("export default function handler(req) {}"));
        assert!(has_default_export("const fn = () => {};\nexport default fn;"));
    }

    #[test]
    fn rejects_missing_default_export() {
        assert!(!has_default_export("export function handler(req) {}"));
        assert!(!has_default_export("module.exports = function() {};"));
        assert!(!has_default_export(""));
    }
}

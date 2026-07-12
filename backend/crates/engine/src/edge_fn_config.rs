use std::collections::HashMap;
use serde::Deserialize;

/// Top-level structure of shipyard.json at the repo root.
#[derive(Debug, Deserialize, Default)]
pub struct ShipyardConfig {
    pub functions: Option<FunctionsConfig>,
}

#[derive(Debug, Deserialize)]
pub struct FunctionsConfig {
    /// Override the directory scanned for function files. Default: "functions".
    pub dir: Option<String>,
    /// Global runtime override. Only "deno" is supported in v1.
    pub runtime: Option<String>,
    /// Explicit function entries. When present, directory scanning is skipped entirely.
    pub entries: Option<HashMap<String, FunctionEntry>>,
}

#[derive(Debug, Deserialize)]
pub struct FunctionEntry {
    /// Path to the function file, relative to the repo root.
    pub file: String,
    /// Per-function execution timeout in seconds.
    pub timeout: Option<u32>,
    /// Whitelist of env var keys injected into the isolate.
    /// Empty = all org env vars are available.
    pub env: Option<Vec<String>>,
}

impl ShipyardConfig {
    pub fn from_file(path: &std::path::Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }
}

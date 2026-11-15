use std::path::Path;
use shipyard_common::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct VerificationConfig {
    pub require_entry_file: bool,
    pub disallow_dot_env: bool,
}

impl Default for VerificationConfig {
    fn default() -> Self {
        Self {
            require_entry_file: true,
            disallow_dot_env: true,
        }
    }
}

/// Verify that an extracted deployment directory meets all static serving prerequisites.
pub fn verify_extracted_deployment(
    extracted_dir: &Path,
    entry_file: &str,
    config: &VerificationConfig,
) -> AppResult<()> {
    if !extracted_dir.exists() || !extracted_dir.is_dir() {
        return Err(AppError::BadRequest(
            "Deployment directory does not exist or is not a directory".into(),
        ));
    }

    let entries: Vec<_> = std::fs::read_dir(extracted_dir)
        .map_err(|e| AppError::Internal(format!("Failed to read deployment directory: {e}")))?
        .filter_map(|e| e.ok())
        .collect();

    if entries.is_empty() {
        return Err(AppError::BadRequest(
            "Extracted deployment directory is empty".into(),
        ));
    }

    if config.disallow_dot_env {
        if extracted_dir.join(".env").exists() || extracted_dir.join(".env.local").exists() {
            return Err(AppError::BadRequest(
                "Uploaded archive contains a root .env file. Do not upload server environment files in static builds.".into(),
            ));
        }
    }

    if config.require_entry_file {
        let entry_path = extracted_dir.join(entry_file);
        if !entry_path.exists() {
            return Err(AppError::BadRequest(format!(
                "Required entry file '{entry_file}' not found in deployment root. Ensure your build output contains {entry_file}."
            )));
        }
        if !entry_path.is_file() {
            return Err(AppError::BadRequest(format!(
                "Entry file '{entry_file}' is a directory, not a file"
            )));
        }
        let meta = std::fs::metadata(&entry_path)
            .map_err(|e| AppError::Internal(format!("Failed to read metadata for '{entry_file}': {e}")))?;
        if meta.len() == 0 {
            return Err(AppError::BadRequest(format!(
                "Entry file '{entry_file}' is empty (0 bytes)"
            )));
        }
    }

    Ok(())
}

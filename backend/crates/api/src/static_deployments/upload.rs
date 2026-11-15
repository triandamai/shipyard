use std::collections::{BTreeSet, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use sha2::{Digest, Sha256};
use shipyard_common::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct ExtractionLimits {
    pub max_compressed_bytes: u64,
    pub max_uncompressed_bytes: u64,
    pub max_files: usize,
}

impl Default for ExtractionLimits {
    fn default() -> Self {
        Self {
            max_compressed_bytes: 200 * 1024 * 1024,   // 200 MB
            max_uncompressed_bytes: 600 * 1024 * 1024, // 600 MB
            max_files: 50_000,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtractionResult {
    pub extracted_size_bytes: i64,
    pub file_count: i32,
    pub checksum_sha256: String,
}

#[derive(Debug, Clone, Default)]
pub struct ZipExtractor {
    pub limits: ExtractionLimits,
}

impl ZipExtractor {
    pub fn new(limits: ExtractionLimits) -> Self {
        Self { limits }
    }

    pub fn extract_from_bytes(&self, zip_bytes: &[u8], dest_dir: &Path) -> AppResult<ExtractionResult> {
        if zip_bytes.len() as u64 > self.limits.max_compressed_bytes {
            return Err(AppError::BadRequest(format!(
                "Archive exceeds compressed limit of {} MB",
                self.limits.max_compressed_bytes / (1024 * 1024)
            )));
        }

        let checksum = hex::encode(Sha256::digest(zip_bytes));
        let cursor = std::io::Cursor::new(zip_bytes);
        let archive = zip::ZipArchive::new(cursor)
            .map_err(|e| AppError::BadRequest(format!("Invalid zip archive: {e}")))?;

        self.extract_archive(archive, dest_dir, checksum)
    }

    pub fn extract_from_file(&self, zip_path: &Path, dest_dir: &Path) -> AppResult<ExtractionResult> {
        let meta = std::fs::metadata(zip_path)
            .map_err(|e| AppError::Internal(format!("Failed to read archive metadata: {e}")))?;
        if meta.len() > self.limits.max_compressed_bytes {
            return Err(AppError::BadRequest(format!(
                "Archive exceeds compressed limit of {} MB",
                self.limits.max_compressed_bytes / (1024 * 1024)
            )));
        }

        let mut file = std::fs::File::open(zip_path)
            .map_err(|e| AppError::Internal(format!("Failed to open archive: {e}")))?;
        let mut hasher = Sha256::new();
        std::io::copy(&mut file, &mut hasher)
            .map_err(|e| AppError::Internal(format!("Failed to hash archive: {e}")))?;
        let checksum = hex::encode(hasher.finalize());

        let file = std::fs::File::open(zip_path)
            .map_err(|e| AppError::Internal(format!("Failed to re-open archive: {e}")))?;
        let archive = zip::ZipArchive::new(file)
            .map_err(|e| AppError::BadRequest(format!("Invalid zip archive: {e}")))?;

        self.extract_archive(archive, dest_dir, checksum)
    }

    fn extract_archive<R: Read + std::io::Seek>(
        &self,
        mut archive: zip::ZipArchive<R>,
        dest_dir: &Path,
        checksum: String,
    ) -> AppResult<ExtractionResult> {
        if archive.len() > self.limits.max_files {
            return Err(AppError::BadRequest(format!(
                "Archive contains {} files, exceeding limit of {}",
                archive.len(),
                self.limits.max_files
            )));
        }

        // Detect single top-level directory wrapping build output (e.g. dist/ or build/)
        let mut tops = BTreeSet::new();
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|e| AppError::BadRequest(format!("Corrupt zip entry: {e}")))?;
            if let Some(p) = entry.enclosed_name() {
                if let Some(first) = p.components().next() {
                    tops.insert(first.as_os_str().to_owned());
                }
            }
        }

        let strip_prefix = if tops.len() == 1 {
            let only = tops.into_iter().next().unwrap_or_default();
            let mut is_dir = false;
            for i in 0..archive.len() {
                let entry = archive
                    .by_index(i)
                    .map_err(|e| AppError::BadRequest(format!("Corrupt zip entry: {e}")))?;
                if let Some(p) = entry.enclosed_name() {
                    if p.components().count() > 1 {
                        is_dir = true;
                        break;
                    }
                }
            }
            if is_dir {
                Some(PathBuf::from(only))
            } else {
                None
            }
        } else {
            None
        };

        std::fs::create_dir_all(dest_dir)
            .map_err(|e| AppError::Internal(format!("Failed to create destination dir: {e}")))?;

        let dest_canon = dest_dir
            .canonicalize()
            .map_err(|e| AppError::Internal(format!("Cannot canonicalize dest dir: {e}")))?;

        let mut total_uncompressed: u64 = 0;
        let mut extracted_file_count: i32 = 0;
        let mut seen_paths = HashSet::new();

        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| AppError::BadRequest(format!("Corrupt zip entry: {e}")))?;

            let raw_name = entry.name().to_string();
            let Some(enclosed) = entry.enclosed_name() else {
                return Err(AppError::BadRequest(format!(
                    "Unsafe path detected in archive: {raw_name}"
                )));
            };

            let rel = match &strip_prefix {
                Some(prefix) => match enclosed.strip_prefix(prefix) {
                    Ok(r) => r.to_path_buf(),
                    Err(_) => continue,
                },
                None => enclosed.to_path_buf(),
            };

            if rel.as_os_str().is_empty() {
                continue;
            }

            // Reject symlinks and hardlinks
            if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
                continue;
            }

            // Skip internal / OS metadata
            if rel.components().any(|c| c.as_os_str() == ".git" || c.as_os_str() == "__MACOSX") {
                continue;
            }
            if rel.file_name().map(|n| n == ".DS_Store").unwrap_or(false) {
                continue;
            }

            let out_path = dest_canon.join(&rel);
            if !out_path.starts_with(&dest_canon) {
                return Err(AppError::BadRequest(format!(
                    "Path traversal attempt: {raw_name}"
                )));
            }

            if entry.is_dir() {
                std::fs::create_dir_all(&out_path)
                    .map_err(|e| AppError::Internal(format!("Failed to create directory: {e}")))?;
                continue;
            }

            // Duplicate path detection
            let norm_key = rel.to_string_lossy().to_string();
            if !seen_paths.insert(norm_key.clone()) {
                return Err(AppError::BadRequest(format!(
                    "Duplicate file in archive: {norm_key}"
                )));
            }

            let declared = entry.size();
            total_uncompressed = total_uncompressed.saturating_add(declared);
            if total_uncompressed > self.limits.max_uncompressed_bytes {
                return Err(AppError::BadRequest(format!(
                    "Archive uncompressed size exceeds limit of {} MB",
                    self.limits.max_uncompressed_bytes / (1024 * 1024)
                )));
            }

            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::Internal(format!("Failed to create parent dir: {e}")))?;
            }

            let mut out_file = std::fs::File::create(&out_path)
                .map_err(|e| AppError::Internal(format!("Failed to create file {}: {e}", out_path.display())))?;

            let mut limited = Read::take(&mut entry, declared + 1);
            let written = std::io::copy(&mut limited, &mut out_file)
                .map_err(|e| AppError::Internal(format!("Failed to extract file: {e}")))?;

            if written > declared {
                return Err(AppError::BadRequest(format!(
                    "Zip entry '{raw_name}' is larger than declared header"
                )));
            }

            extracted_file_count += 1;
        }

        Ok(ExtractionResult {
            extracted_size_bytes: total_uncompressed as i64,
            file_count: extracted_file_count,
            checksum_sha256: checksum,
        })
    }
}

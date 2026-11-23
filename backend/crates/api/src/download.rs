use std::path::Path;
use futures::StreamExt;
use shipyard_common::error::{AppError, AppResult};

/// Validates that a string is a valid HTTP or HTTPS URL.
pub fn validate_download_url(url: &str) -> AppResult<reqwest::Url> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("Download URL cannot be empty".into()));
    }
    let parsed = reqwest::Url::parse(trimmed)
        .map_err(|e| AppError::BadRequest(format!("Invalid download URL: {e}")))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(AppError::BadRequest(format!(
            "Invalid download URL scheme '{}'; only http and https are supported",
            parsed.scheme()
        )));
    }
    Ok(parsed)
}

/// Downloads a remote archive file directly to `dest_path`, streaming chunks
/// and enforcing `max_bytes` to prevent disk exhaustion.
pub async fn download_url_to_file(
    client: &reqwest::Client,
    url: &str,
    dest_path: &Path,
    max_bytes: u64,
) -> AppResult<u64> {
    validate_download_url(url)?;

    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to connect to download URL: {e}")))?;

    let status = res.status();
    if !status.is_success() {
        return Err(AppError::BadRequest(format!(
            "Download URL returned HTTP error status {status}"
        )));
    }

    if let Some(content_length) = res.content_length() {
        if content_length > max_bytes {
            return Err(AppError::BadRequest(format!(
                "Archive size ({} MB) exceeds maximum allowed size ({} MB)",
                content_length / (1024 * 1024),
                max_bytes / (1024 * 1024)
            )));
        }
    }

    let mut file = tokio::fs::File::create(dest_path)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to create destination file for download: {e}")))?;

    let mut stream = res.bytes_stream();
    let mut total_bytes: u64 = 0;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| AppError::BadRequest(format!("Error while streaming download: {e}")))?;
        total_bytes += chunk.len() as u64;
        if total_bytes > max_bytes {
            let _ = tokio::fs::remove_file(dest_path).await;
            return Err(AppError::BadRequest(format!(
                "Downloaded archive exceeded maximum allowed size of {} MB",
                max_bytes / (1024 * 1024)
            )));
        }
        tokio::io::AsyncWriteExt::write_all(&mut file, &chunk)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to write downloaded chunk to disk: {e}")))?;
    }

    tokio::io::AsyncWriteExt::flush(&mut file)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to flush downloaded file: {e}")))?;

    Ok(total_bytes)
}

/// Downloads a remote archive into memory as bytes, enforcing `max_bytes`.
pub async fn download_url_to_bytes(
    client: &reqwest::Client,
    url: &str,
    max_bytes: u64,
) -> AppResult<Vec<u8>> {
    validate_download_url(url)?;

    let res = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to connect to download URL: {e}")))?;

    let status = res.status();
    if !status.is_success() {
        return Err(AppError::BadRequest(format!(
            "Download URL returned HTTP error status {status}"
        )));
    }

    if let Some(content_length) = res.content_length() {
        if content_length > max_bytes {
            return Err(AppError::BadRequest(format!(
                "Archive size ({} MB) exceeds maximum allowed size ({} MB)",
                content_length / (1024 * 1024),
                max_bytes / (1024 * 1024)
            )));
        }
    }

    let capacity = res.content_length().unwrap_or(0).min(max_bytes) as usize;
    let mut buffer = Vec::with_capacity(capacity);
    let mut stream = res.bytes_stream();
    let mut total_bytes: u64 = 0;

    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| AppError::BadRequest(format!("Error while streaming download: {e}")))?;
        total_bytes += chunk.len() as u64;
        if total_bytes > max_bytes {
            return Err(AppError::BadRequest(format!(
                "Downloaded archive exceeded maximum allowed size of {} MB",
                max_bytes / (1024 * 1024)
            )));
        }
        buffer.extend_from_slice(&chunk);
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_download_url() {
        assert!(validate_download_url("https://s3.amazonaws.com/bucket/dist.zip?sig=123").is_ok());
        assert!(validate_download_url("http://example.com/build.tar.gz").is_ok());
        assert!(validate_download_url("ftp://example.com/file.zip").is_err());
        assert!(validate_download_url("not a url").is_err());
        assert!(validate_download_url("").is_err());
    }
}

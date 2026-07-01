use aes_gcm::{
    aead::{AeadCore, AeadInPlace, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::error::{AppError, AppResult};

/// Encrypt `plaintext` with AES-256-GCM using `hex_key`.
///
/// `hex_key` must be exactly 64 hex characters (32 bytes).
///
/// Returns a base64-encoded string of the form `nonce(12) || ciphertext || tag(16)`.
pub fn encrypt(hex_key: &str, plaintext: &str) -> AppResult<String> {
    let key_bytes = hex::decode(hex_key)
        .map_err(|e| AppError::Config(format!("invalid secret_key hex: {e}")))?;

    if key_bytes.len() != 32 {
        return Err(AppError::Config(
            "secret_key must be exactly 32 bytes (64 hex chars)".to_string(),
        ));
    }

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let mut buffer: Vec<u8> = plaintext.as_bytes().to_vec();
    cipher
        .encrypt_in_place(&nonce, b"", &mut buffer)
        .map_err(|e| AppError::Internal(format!("encryption failed: {e}")))?;

    // Prepend nonce to ciphertext+tag
    let mut output = nonce.to_vec();
    output.extend_from_slice(&buffer);

    Ok(BASE64.encode(&output))
}

/// Decrypt a base64-encoded ciphertext produced by [`encrypt`].
pub fn decrypt(hex_key: &str, ciphertext_b64: &str) -> AppResult<String> {
    let key_bytes = hex::decode(hex_key)
        .map_err(|e| AppError::Config(format!("invalid secret_key hex: {e}")))?;

    if key_bytes.len() != 32 {
        return Err(AppError::Config(
            "secret_key must be exactly 32 bytes (64 hex chars)".to_string(),
        ));
    }

    let raw = BASE64
        .decode(ciphertext_b64)
        .map_err(|e| AppError::Internal(format!("base64 decode failed: {e}")))?;

    if raw.len() < 12 {
        return Err(AppError::Internal(
            "ciphertext too short to contain nonce".to_string(),
        ));
    }

    let (nonce_bytes, ciphertext_tag) = raw.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);

    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    let mut buffer = ciphertext_tag.to_vec();
    cipher
        .decrypt_in_place(nonce, b"", &mut buffer)
        .map_err(|_| AppError::Internal("decryption failed — wrong key or corrupted data".to_string()))?;

    String::from_utf8(buffer)
        .map_err(|e| AppError::Internal(format!("decrypted value is not valid UTF-8: {e}")))
}

/// Encrypt only if the key is non-trivial (not all zeros).
/// Falls back to storing the plaintext for development where secret_key is not configured.
pub fn encrypt_or_passthrough(hex_key: &str, plaintext: &str) -> String {
    if is_dev_key(hex_key) {
        return plaintext.to_string();
    }
    encrypt(hex_key, plaintext).unwrap_or_else(|_| plaintext.to_string())
}

/// Decrypt only if the key is non-trivial. Falls back to returning the stored value as-is.
pub fn decrypt_or_passthrough(hex_key: &str, stored: &str) -> String {
    if is_dev_key(hex_key) {
        return stored.to_string();
    }
    // If the stored value is valid base64 and long enough it was encrypted; otherwise it's plaintext.
    decrypt(hex_key, stored).unwrap_or_else(|_| stored.to_string())
}

fn is_dev_key(hex_key: &str) -> bool {
    hex_key.chars().all(|c| c == '0') || hex_key.is_empty()
}

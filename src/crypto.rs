//! Symmetric encryption for stored credentials (CalDAV passwords, SMTP passwords).
//!
//! Uses AES-256-GCM with a random 12-byte nonce per encryption. The encrypted
//! format stored in the database is: base64(nonce || ciphertext || tag).
//!
//! The secret key is loaded from (in order of priority):
//! 1. `CALRS_SECRET_KEY` environment variable (base64-encoded 32 bytes)
//! 2. `secret.key` file in the data directory (raw 32 bytes)
//! 3. Auto-generated and written to `secret.key` on first run

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use anyhow::{bail, Context, Result};
use base64::Engine;
use rand::RngCore;
use std::path::Path;

const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;
const KEY_FILE: &str = "secret.key";

/// Load or generate the 256-bit secret key.
pub fn load_or_create_key(data_dir: &Path) -> Result<[u8; KEY_LEN]> {
    // 1. Check env var
    if let Ok(val) = std::env::var("CALRS_SECRET_KEY") {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(val.trim())
            .context("CALRS_SECRET_KEY must be valid base64")?;
        if bytes.len() != KEY_LEN {
            bail!(
                "CALRS_SECRET_KEY must decode to exactly 32 bytes (got {})",
                bytes.len()
            );
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }

    // 2. Check key file
    let key_path = data_dir.join(KEY_FILE);
    if key_path.exists() {
        let bytes = std::fs::read(&key_path)
            .with_context(|| format!("Failed to read {}", key_path.display()))?;
        if bytes.len() != KEY_LEN {
            bail!(
                "Secret key file has wrong size ({} bytes, expected {})",
                bytes.len(),
                KEY_LEN
            );
        }
        let mut key = [0u8; KEY_LEN];
        key.copy_from_slice(&bytes);
        return Ok(key);
    }

    // 3. Generate new key
    let mut key = [0u8; KEY_LEN];
    OsRng.fill_bytes(&mut key);
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(&key_path, key)
        .with_context(|| format!("Failed to write {}", key_path.display()))?;

    // Set file permissions to 0600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600))?;
    }

    Ok(key)
}

/// Encrypt a plaintext password. Returns a base64 string (nonce || ciphertext).
pub fn encrypt_password(key: &[u8; KEY_LEN], plaintext: &str) -> Result<String> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("cipher init: {}", e))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encryption failed: {}", e))?;

    // nonce || ciphertext (which includes the GCM tag)
    let mut combined = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(base64::engine::general_purpose::STANDARD.encode(&combined))
}

/// Decrypt a password stored as base64(nonce || ciphertext).
pub fn decrypt_password(key: &[u8; KEY_LEN], stored: &str) -> Result<String> {
    let combined = base64::engine::general_purpose::STANDARD
        .decode(stored.trim())
        .context("invalid base64 in stored password")?;

    if combined.len() < NONCE_LEN + 1 {
        bail!("stored password too short to contain nonce + ciphertext");
    }

    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_LEN);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|e| anyhow::anyhow!("cipher init: {}", e))?;

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| anyhow::anyhow!("decryption failed (wrong key?): {}", e))?;

    String::from_utf8(plaintext).context("decrypted password is not valid UTF-8")
}

/// Check if a stored value looks like a legacy hex-encoded password
/// (as opposed to base64-encoded encrypted data).
/// Legacy format: hex-encoded ASCII bytes → always even length, only [0-9a-f].
pub fn is_legacy_hex(value: &str) -> bool {
    !value.is_empty()
        && value.len().is_multiple_of(2)
        && value.chars().all(|c| c.is_ascii_hexdigit())
        // base64 can also be all hex chars, but legacy hex-encoded passwords
        // produce longer strings (2 chars per byte vs ~1.33 for base64).
        // A hex-encoded password of length N produces 2N hex chars.
        // Decrypt attempt will fail on non-legacy data, so this is just a heuristic.
        && hex::decode(value)
            .map(|bytes| String::from_utf8(bytes).is_ok())
            .unwrap_or(false)
}

/// Migrate a legacy hex-encoded password to encrypted format.
/// Returns the encrypted string if it was legacy, None otherwise.
pub fn migrate_legacy(key: &[u8; KEY_LEN], stored: &str) -> Result<Option<String>> {
    if is_legacy_hex(stored) {
        let bytes = hex::decode(stored)?;
        let plaintext = String::from_utf8(bytes)?;
        let encrypted = encrypt_password(key, &plaintext)?;
        Ok(Some(encrypted))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let key = [42u8; 32];
        let password = "hunter2";
        let encrypted = encrypt_password(&key, password).unwrap();
        let decrypted = decrypt_password(&key, &encrypted).unwrap();
        assert_eq!(decrypted, password);
    }

    #[test]
    fn test_different_nonces() {
        let key = [42u8; 32];
        let e1 = encrypt_password(&key, "test").unwrap();
        let e2 = encrypt_password(&key, "test").unwrap();
        // Same plaintext should produce different ciphertexts (random nonce)
        assert_ne!(e1, e2);
        // But both should decrypt to the same value
        assert_eq!(decrypt_password(&key, &e1).unwrap(), "test");
        assert_eq!(decrypt_password(&key, &e2).unwrap(), "test");
    }

    #[test]
    fn test_wrong_key() {
        let key1 = [1u8; 32];
        let key2 = [2u8; 32];
        let encrypted = encrypt_password(&key1, "secret").unwrap();
        assert!(decrypt_password(&key2, &encrypted).is_err());
    }

    #[test]
    fn test_legacy_detection() {
        // "mypassword" hex-encoded
        let legacy = hex::encode("mypassword".as_bytes());
        assert!(is_legacy_hex(&legacy));

        // base64 encrypted data should not match
        let key = [42u8; 32];
        let encrypted = encrypt_password(&key, "mypassword").unwrap();
        assert!(!is_legacy_hex(&encrypted));
    }

    #[test]
    fn test_migrate_legacy() {
        let key = [42u8; 32];
        let legacy = hex::encode("mypassword".as_bytes());
        let result = migrate_legacy(&key, &legacy).unwrap();
        assert!(result.is_some());
        let decrypted = decrypt_password(&key, &result.unwrap()).unwrap();
        assert_eq!(decrypted, "mypassword");
    }

    #[test]
    fn test_key_file() {
        let dir = std::env::temp_dir().join(format!("calrs_test_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // First call generates
        let key1 = load_or_create_key(&dir).unwrap();
        // Second call reads the same key
        let key2 = load_or_create_key(&dir).unwrap();
        assert_eq!(key1, key2);

        std::fs::remove_dir_all(&dir).unwrap();
    }
}

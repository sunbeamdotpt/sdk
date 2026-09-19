//! Thin AES-256-GCM helpers for encrypting sensitive columns at rest.
//!
//! Ciphertexts are stored as `base64(nonce || tag || ciphertext)` so they can
//! be kept in TEXT columns. The key must be exactly 32 bytes.

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, AeadCore, KeyInit},
};
use base64::Engine;

use crate::g2v::error::ServiceError;

const NONCE_LEN: usize = 12;

fn cipher(key: &[u8]) -> Result<Aes256Gcm, ServiceError> {
    if key.len() != 32 {
        return Err(ServiceError::Configuration(
            "encryption key must be exactly 32 bytes".into(),
        ));
    }
    Aes256Gcm::new_from_slice(key)
        .map_err(|e| ServiceError::Configuration(format!("invalid encryption key: {e}")))
}

/// Encrypt `plaintext` with `key` and return a base64-encoded nonce-ciphertext.
pub fn encrypt(plaintext: &str, key: &[u8]) -> Result<String, ServiceError> {
    let cipher = cipher(key)?;
    let nonce = Aes256Gcm::generate_nonce(&mut rand_core::OsRng);
    let mut buffer = nonce.to_vec();
    let ciphertext = cipher
        .encrypt(&nonce, plaintext.as_bytes())
        .map_err(|e| ServiceError::Configuration(format!("encrypt: {e}")))?;
    buffer.extend_from_slice(&ciphertext);
    Ok(base64::engine::general_purpose::STANDARD.encode(&buffer))
}

/// Decrypt a base64-encoded nonce-ciphertext produced by [`encrypt`].
pub fn decrypt(ciphertext_b64: &str, key: &[u8]) -> Result<String, ServiceError> {
    let cipher = cipher(key)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(ciphertext_b64)
        .map_err(|e| ServiceError::Configuration(format!("base64: {e}")))?;
    if bytes.len() < NONCE_LEN {
        return Err(ServiceError::Configuration("ciphertext too short".into()));
    }
    let (nonce, ciphertext) = bytes.split_at(NONCE_LEN);
    #[allow(deprecated)]
    let nonce = aes_gcm::Nonce::from_slice(nonce);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| ServiceError::Configuration(format!("decrypt: {e}")))?;
    String::from_utf8(plaintext)
        .map_err(|e| ServiceError::Configuration(format!("decrypt utf8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encryption() {
        let key = [0u8; 32];
        let encrypted = encrypt("super secret", &key).unwrap();
        assert_ne!(encrypted, "super secret");
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, "super secret");
    }

    #[test]
    fn wrong_key_fails_decryption() {
        let key = [0u8; 32];
        let encrypted = encrypt("super secret", &key).unwrap();
        let wrong_key = [1u8; 32];
        assert!(decrypt(&encrypted, &wrong_key).is_err());
    }

    #[test]
    fn short_key_rejected() {
        assert!(encrypt("x", &[0u8; 16]).is_err());
    }
}

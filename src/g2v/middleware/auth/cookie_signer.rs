//! Signed cookie jar for session tokens.
//!
//! Uses HMAC-SHA256 to sign cookie values. The cookie format is
//! `base64(payload).base64(signature)`. Verification recomputes the signature
//! and compares it with a constant-time comparison.
//!
//! The provided secret is stretched with HKDF-SHA256 so that even a long but
//! low-entropy secret is converted into a uniformly random 256-bit HMAC key.

use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;
use subtle::ConstantTimeEq;

const HKDF_INFO: &[u8] = b"sunbeam-g2v-cookie-signer-v1";

/// Errors from cookie signing/verification.
#[derive(Debug, thiserror::Error)]
pub enum CookieError {
    /// The cookie does not contain the expected `.` separator.
    #[error("invalid cookie format")]
    InvalidFormat,

    /// Base64 decoding failed.
    #[error("invalid base64")]
    InvalidBase64,

    /// The signature does not match the payload.
    #[error("invalid signature")]
    InvalidSignature,

    /// The signing key is shorter than 32 bytes.
    #[error("cookie signing key must be at least 32 bytes")]
    WeakKey,

    /// Key derivation or MAC initialization failed.
    #[error("failed to derive the cookie signing key")]
    KeyDerivation,
}

/// Signs and verifies session cookie values.
#[derive(Clone, Debug)]
pub struct CookieSigner {
    key: Vec<u8>,
}

impl CookieSigner {
    /// Create a signer from a secret key.
    ///
    /// Returns an error if `key` is shorter than 32 bytes. The caller is
    /// expected to validate this at config load time.
    pub fn new(key: impl AsRef<[u8]>) -> Result<Self, CookieError> {
        let key_material = key.as_ref();
        if key_material.len() < 32 {
            return Err(CookieError::WeakKey);
        }
        let hk = hkdf::Hkdf::<Sha256>::new(None, key_material);
        let mut derived = [0u8; 32];
        hk.expand(HKDF_INFO, &mut derived)
            .map_err(|_| CookieError::KeyDerivation)?;
        Ok(Self {
            key: derived.to_vec(),
        })
    }

    /// Sign a payload and return a cookie-safe string.
    pub fn sign(&self, payload: &str) -> Result<String, CookieError> {
        let signature = self.signature(payload)?;
        Ok(format!(
            "{}.{}",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.as_bytes()),
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(signature)
        ))
    }

    /// Verify a signed cookie and return the original payload.
    pub fn verify(&self, cookie: &str) -> Result<String, CookieError> {
        let (encoded_payload, encoded_signature) =
            cookie.split_once('.').ok_or(CookieError::InvalidFormat)?;

        let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded_payload)
            .map_err(|_| CookieError::InvalidBase64)?;
        let payload = String::from_utf8(payload_bytes).map_err(|_| CookieError::InvalidFormat)?;

        let provided_signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(encoded_signature)
            .map_err(|_| CookieError::InvalidBase64)?;

        let expected_signature = self.signature(&payload)?;

        // Constant-time comparison; `subtle` pads shorter inputs internally.
        if expected_signature
            .as_slice()
            .ct_eq(&provided_signature)
            .unwrap_u8()
            != 1
        {
            return Err(CookieError::InvalidSignature);
        }

        Ok(payload)
    }

    fn signature(&self, payload: &str) -> Result<Vec<u8>, CookieError> {
        type HmacSha256 = Hmac<Sha256>;
        let mut mac = match HmacSha256::new_from_slice(&self.key) {
            Ok(mac) => mac,
            Err(_) => return Err(CookieError::KeyDerivation),
        };
        mac.update(payload.as_bytes());
        Ok(mac.finalize().into_bytes().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> &'static str {
        "super-secret-key-that-is-at-least-32-bytes-long"
    }

    #[test]
    fn sign_and_verify_round_trip() {
        let signer = CookieSigner::new(test_key()).unwrap();
        let cookie = signer.sign("session-token-123").unwrap();
        let verified = signer.verify(&cookie).unwrap();
        assert_eq!(verified, "session-token-123");
    }

    #[test]
    fn verify_rejects_tampered_payload() {
        let signer = CookieSigner::new(test_key()).unwrap();
        let cookie = signer.sign("session-token-123").unwrap();
        let mut parts: Vec<&str> = cookie.split('.').collect();
        let tampered =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("tampered-token".as_bytes());
        parts[0] = Box::leak(tampered.into_boxed_str());
        let tampered_cookie = parts.join(".");
        assert!(matches!(
            signer.verify(&tampered_cookie).unwrap_err(),
            CookieError::InvalidSignature
        ));
    }

    #[test]
    fn verify_rejects_malformed_cookie() {
        let signer = CookieSigner::new(test_key()).unwrap();
        assert!(matches!(
            signer.verify("no-dot-here").unwrap_err(),
            CookieError::InvalidFormat
        ));
    }

    #[test]
    fn verify_rejects_invalid_base64() {
        let signer = CookieSigner::new(test_key()).unwrap();
        assert!(matches!(
            signer.verify("!!!.!!!").unwrap_err(),
            CookieError::InvalidBase64
        ));
    }

    #[test]
    fn verify_rejects_short_signature() {
        let signer = CookieSigner::new(test_key()).unwrap();
        let cookie = signer.sign("session-token-123").unwrap();
        let mut parts: Vec<&str> = cookie.split('.').collect();
        let short = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"short");
        parts[1] = Box::leak(short.into_boxed_str());
        let short_cookie = parts.join(".");
        assert!(matches!(
            signer.verify(&short_cookie).unwrap_err(),
            CookieError::InvalidSignature
        ));
    }

    #[test]
    fn new_rejects_short_key() {
        let err = CookieSigner::new("short-key").unwrap_err();
        assert!(matches!(err, CookieError::WeakKey));
    }
}

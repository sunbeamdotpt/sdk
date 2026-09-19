//! Session authentication primitives.

use async_trait::async_trait;
use serde_json::Value;

use super::error::AuthError;

/// Abstracts validation of a browser session or session token.
#[async_trait]
pub trait SessionClient: Send + Sync + 'static {
    /// Validate the session and return the raw session payload.
    async fn to_session(
        &self,
        cookie: Option<&str>,
        token: Option<&str>,
    ) -> Result<Value, AuthError>;
}

/// Store for server-side session revocation.
///
/// The framework provides `SessionTokenSigner` for issuing and verifying
/// signed opaque session tokens. The consuming service implements this trait
/// to check whether a session id (`sid`) has been revoked before its natural
/// expiration.
#[async_trait]
pub trait SessionStore: Send + Sync + 'static {
    /// Returns `true` if the session identified by `sid` is still active.
    async fn is_active(&self, sid: &str) -> Result<bool, AuthError>;
}

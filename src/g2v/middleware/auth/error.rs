//! Authentication and authorization errors.

use crate::g2v::error::ServiceError;
use std::fmt;

/// Errors originating in the authn/authz stack.
#[derive(Debug)]
pub enum AuthError {
    /// The tenant id is missing or malformed.
    InvalidTenant(String),
    /// The session cookie or token is invalid or expired.
    InvalidSession(String),
    /// The request carried no credential at all (no bearer token, session
    /// cookie, or session token).
    MissingCredentials,
    /// The session or token is not active (revoked, expired, or unknown to
    /// the introspection endpoint).
    InactiveSession,
    /// The session validation backend returned an error response (HTTP
    /// non-2xx from the introspection endpoint).
    SessionGateway(String),
    /// The session validation backend did not respond in time.
    SessionTimeout(String),
    /// The session validation backend could not be reached (connection,
    /// DNS, TLS, …).
    SessionTransport(String),
    /// The authorization backend returned an unexpected HTTP error response.
    AuthorizationBackend(String),
    /// The authorization backend could not be reached.
    AuthorizationBackendUnavailable(String),
    /// No tenant could be resolved for the request.
    MissingTenant,
    /// The subject does not have the requested permission.
    MissingPermission,
}

impl AuthError {
    /// Stable, machine-readable failure class for this error.
    ///
    /// Surfaced in authn rejection responses (`{"code":"unauthenticated",
    /// "message":"<class>"}` on ConnectRPC routes) and in WARN log records,
    /// so operators can tell "token inactive" apart from "gateway wedged"
    /// without parsing free-form messages.
    pub fn failure_class(&self) -> &'static str {
        match self {
            AuthError::InvalidTenant(_) => "invalid_tenant",
            AuthError::InvalidSession(_) => "invalid_session",
            AuthError::MissingCredentials => "missing_credentials",
            AuthError::InactiveSession => "inactive",
            AuthError::SessionGateway(_) => "gateway_error",
            AuthError::SessionTimeout(_) => "timeout",
            AuthError::SessionTransport(_) => "transport",
            AuthError::AuthorizationBackend(_) => "authorization_backend",
            AuthError::AuthorizationBackendUnavailable(_) => "authorization_backend_unavailable",
            AuthError::MissingTenant => "missing_tenant",
            AuthError::MissingPermission => "missing_permission",
        }
    }

    /// Whether this failure is a transient backend problem (worth retrying)
    /// rather than a terminal rejection of the credential.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            AuthError::SessionGateway(_)
                | AuthError::SessionTimeout(_)
                | AuthError::SessionTransport(_)
                | AuthError::AuthorizationBackendUnavailable(_)
        )
    }
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuthError::InvalidTenant(msg) => write!(f, "invalid tenant: {msg}"),
            AuthError::InvalidSession(msg) => write!(f, "invalid session: {msg}"),
            AuthError::MissingCredentials => {
                write!(f, "missing authorization header or session cookie")
            }
            AuthError::InactiveSession => write!(f, "session or token is not active"),
            AuthError::SessionGateway(msg) => {
                write!(f, "session validation backend error: {msg}")
            }
            AuthError::SessionTimeout(msg) => {
                write!(f, "session validation timed out: {msg}")
            }
            AuthError::SessionTransport(msg) => {
                write!(f, "session validation unreachable: {msg}")
            }
            AuthError::AuthorizationBackend(msg) => {
                write!(f, "authorization backend error: {msg}")
            }
            AuthError::AuthorizationBackendUnavailable(msg) => {
                write!(f, "authorization backend unavailable: {msg}")
            }
            AuthError::MissingTenant => write!(f, "missing tenant"),
            AuthError::MissingPermission => write!(f, "permission denied"),
        }
    }
}

impl std::error::Error for AuthError {}

impl From<AuthError> for ServiceError {
    fn from(err: AuthError) -> Self {
        match err {
            AuthError::InvalidTenant(_)
            | AuthError::InvalidSession(_)
            | AuthError::MissingCredentials
            | AuthError::InactiveSession
            | AuthError::MissingTenant => ServiceError::Unauthenticated(err.to_string()),
            // Transient introspection failures are still surfaced as
            // 401/unauthenticated to the caller — the failure class in the
            // message is what distinguishes them from a bad credential.
            AuthError::SessionGateway(_)
            | AuthError::SessionTimeout(_)
            | AuthError::SessionTransport(_) => ServiceError::Unauthenticated(err.to_string()),
            AuthError::MissingPermission => ServiceError::PermissionDenied(err.to_string()),
            AuthError::AuthorizationBackend(msg)
            | AuthError::AuthorizationBackendUnavailable(msg) => ServiceError::Internal(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_classes_are_stable() {
        assert_eq!(
            AuthError::InvalidSession("x".into()).failure_class(),
            "invalid_session"
        );
        assert_eq!(AuthError::InactiveSession.failure_class(), "inactive");
        assert_eq!(
            AuthError::SessionGateway("x".into()).failure_class(),
            "gateway_error"
        );
        assert_eq!(
            AuthError::SessionTimeout("x".into()).failure_class(),
            "timeout"
        );
        assert_eq!(
            AuthError::SessionTransport("x".into()).failure_class(),
            "transport"
        );
        assert_eq!(
            AuthError::AuthorizationBackend("x".into()).failure_class(),
            "authorization_backend"
        );
        assert_eq!(AuthError::MissingTenant.failure_class(), "missing_tenant");
        assert_eq!(
            AuthError::MissingPermission.failure_class(),
            "missing_permission"
        );
    }

    #[test]
    fn transient_classification() {
        assert!(AuthError::SessionGateway("x".into()).is_transient());
        assert!(AuthError::SessionTimeout("x".into()).is_transient());
        assert!(AuthError::SessionTransport("x".into()).is_transient());
        assert!(!AuthError::InactiveSession.is_transient());
        assert!(!AuthError::InvalidSession("x".into()).is_transient());
    }

    #[test]
    fn session_failures_map_to_unauthenticated() {
        let err: ServiceError = AuthError::SessionGateway("bad gateway".into()).into();
        assert!(matches!(err, ServiceError::Unauthenticated(_)));
        let err: ServiceError = AuthError::SessionTimeout("slow".into()).into();
        assert!(matches!(err, ServiceError::Unauthenticated(_)));
    }
}

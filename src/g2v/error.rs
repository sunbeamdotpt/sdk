//! Unified error handling for Sunbeam services.
//!
//! This module provides `ServiceError` - a unified error type that automatically
//! converts to `ConnectError` for RPC responses, and implements `From` for common
//! error types used in service implementations.

#[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
use connectrpc::ErrorCode;
#[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
use http::StatusCode;

/// A result type that uses `ServiceError` as the error type.
pub type ServiceResult<T> = Result<T, ServiceError>;

/// Service-level error type for Sunbeam services.
///
/// This error type provides a unified way to handle errors across all Sunbeam services.
/// It automatically converts to `ConnectError` for RPC responses.
///
/// # Variants
///
/// - `InvalidArgument` - Client provided invalid arguments (400 Bad Request)
/// - `NotFound` - Requested resource does not exist (404 Not Found)
/// - `AlreadyExists` - Resource already exists (409 Conflict)
/// - `PermissionDenied` - Client lacks permission (403 Forbidden)
/// - `Unauthenticated` - Authentication required (401 Unauthorized)
/// - `Internal` - Internal server error (500 Internal Server Error)
/// - `Unavailable` - Service temporarily unavailable (503 Service Unavailable)
/// - `Unimplemented` - Feature not yet implemented (501 Not Implemented)
/// - `DeadlineExceeded` - Operation timed out (504 Gateway Timeout)
/// - `Database` - Database operation failed
/// - `Configuration` - Service configuration error
/// - `Serialization` - Serialization/deserialization error
#[derive(Debug, Clone)]
pub enum ServiceError {
    /// Invalid arguments provided by the client.
    InvalidArgument(String),

    /// The requested resource was not found.
    NotFound(String),

    /// The resource already exists.
    AlreadyExists(String),

    /// The client does not have permission to perform this action.
    PermissionDenied(String),

    /// Authentication is required or failed.
    Unauthenticated(String),

    /// An internal error occurred.
    Internal(String),

    /// The service is temporarily unavailable.
    Unavailable(String),

    /// The requested feature is not yet implemented.
    Unimplemented(String),

    /// The operation timed out.
    DeadlineExceeded(String),

    /// A database operation failed.
    Database(String),

    /// A configuration error occurred.
    Configuration(String),

    /// A serialization/deserialization error occurred.
    Serialization(String),
}

impl ServiceError {
    /// Returns the appropriate `ErrorCode` for this error.
    #[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
    pub fn code(&self) -> ErrorCode {
        match self {
            ServiceError::InvalidArgument(_) => ErrorCode::InvalidArgument,
            ServiceError::NotFound(_) => ErrorCode::NotFound,
            ServiceError::AlreadyExists(_) => ErrorCode::AlreadyExists,
            ServiceError::PermissionDenied(_) => ErrorCode::PermissionDenied,
            ServiceError::Unauthenticated(_) => ErrorCode::Unauthenticated,
            ServiceError::Internal(_) => ErrorCode::Internal,
            ServiceError::Unavailable(_) => ErrorCode::Unavailable,
            ServiceError::Unimplemented(_) => ErrorCode::Unimplemented,
            ServiceError::DeadlineExceeded(_) => ErrorCode::DeadlineExceeded,
            ServiceError::Database(_) => ErrorCode::Internal,
            ServiceError::Configuration(_) => ErrorCode::Internal,
            ServiceError::Serialization(_) => ErrorCode::Internal,
        }
    }

    /// Returns the HTTP status code for this error.
    #[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
    pub fn http_status(&self) -> StatusCode {
        match self.code() {
            ErrorCode::InvalidArgument => StatusCode::BAD_REQUEST,
            ErrorCode::NotFound => StatusCode::NOT_FOUND,
            ErrorCode::AlreadyExists => StatusCode::CONFLICT,
            ErrorCode::PermissionDenied => StatusCode::FORBIDDEN,
            ErrorCode::Unauthenticated => StatusCode::UNAUTHORIZED,
            ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::Unimplemented => StatusCode::NOT_IMPLEMENTED,
            ErrorCode::DeadlineExceeded => StatusCode::GATEWAY_TIMEOUT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Returns the error message.
    pub fn message(&self) -> &str {
        match self {
            ServiceError::InvalidArgument(msg) => msg,
            ServiceError::NotFound(msg) => msg,
            ServiceError::AlreadyExists(msg) => msg,
            ServiceError::PermissionDenied(msg) => msg,
            ServiceError::Unauthenticated(msg) => msg,
            ServiceError::Internal(msg) => msg,
            ServiceError::Unavailable(msg) => msg,
            ServiceError::Unimplemented(msg) => msg,
            ServiceError::DeadlineExceeded(msg) => msg,
            ServiceError::Database(msg) => msg,
            ServiceError::Configuration(msg) => msg,
            ServiceError::Serialization(msg) => msg,
        }
    }
}

impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::InvalidArgument(msg) => write!(f, "Invalid argument: {}", msg),
            ServiceError::NotFound(msg) => write!(f, "Not found: {}", msg),
            ServiceError::AlreadyExists(msg) => write!(f, "Already exists: {}", msg),
            ServiceError::PermissionDenied(msg) => write!(f, "Permission denied: {}", msg),
            ServiceError::Unauthenticated(msg) => write!(f, "Unauthenticated: {}", msg),
            ServiceError::Internal(msg) => write!(f, "Internal error: {}", msg),
            ServiceError::Unavailable(msg) => write!(f, "Service unavailable: {}", msg),
            ServiceError::Unimplemented(msg) => write!(f, "Unimplemented: {}", msg),
            ServiceError::DeadlineExceeded(msg) => write!(f, "Deadline exceeded: {}", msg),
            ServiceError::Database(msg) => write!(f, "Database error: {}", msg),
            ServiceError::Configuration(msg) => write!(f, "Configuration error: {}", msg),
            ServiceError::Serialization(msg) => write!(f, "Serialization error: {}", msg),
        }
    }
}

impl std::error::Error for ServiceError {}

#[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
impl From<ServiceError> for connectrpc::ConnectError {
    fn from(err: ServiceError) -> Self {
        connectrpc::ConnectError::new(err.code(), err.message())
    }
}

// ============================================================================
// From implementations for common error types
// ============================================================================

#[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
impl From<connectrpc::ConnectError> for ServiceError {
    fn from(err: connectrpc::ConnectError) -> Self {
        // An absent message falls back to the code name so the error is
        // never contentless.
        let msg = match err.message {
            Some(msg) => msg,
            None => format!("{:?}", err.code),
        };
        match err.code {
            ErrorCode::InvalidArgument => ServiceError::InvalidArgument(msg),
            ErrorCode::NotFound => ServiceError::NotFound(msg),
            ErrorCode::AlreadyExists => ServiceError::AlreadyExists(msg),
            ErrorCode::PermissionDenied => ServiceError::PermissionDenied(msg),
            ErrorCode::Unauthenticated => ServiceError::Unauthenticated(msg),
            ErrorCode::Internal => ServiceError::Internal(msg),
            ErrorCode::Unavailable => ServiceError::Unavailable(msg),
            ErrorCode::Unimplemented => ServiceError::Unimplemented(msg),
            ErrorCode::DeadlineExceeded => ServiceError::DeadlineExceeded(msg),
            _ => ServiceError::Internal(msg),
        }
    }
}

impl<E: std::error::Error + Send + Sync + 'static> From<Box<E>> for ServiceError {
    fn from(err: Box<E>) -> Self {
        ServiceError::Internal(err.to_string())
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for ServiceError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        ServiceError::Internal(err.to_string())
    }
}

impl From<anyhow::Error> for ServiceError {
    fn from(err: anyhow::Error) -> Self {
        ServiceError::Internal(err.to_string())
    }
}

impl From<serde_json::Error> for ServiceError {
    fn from(err: serde_json::Error) -> Self {
        ServiceError::Serialization(err.to_string())
    }
}

#[cfg(feature = "g2v-server")]
impl From<crate::g2v::config::ConfigError> for ServiceError {
    fn from(err: crate::g2v::config::ConfigError) -> Self {
        ServiceError::Configuration(err.to_string())
    }
}

#[cfg(feature = "g2v-sqlx")]
impl From<sqlx::Error> for ServiceError {
    fn from(err: sqlx::Error) -> Self {
        ServiceError::Database(err.to_string())
    }
}

// Blanket implementation for all async_nats error types
// async_nats has Error<Kind> for various Kind types
// where Kind: Clone + Debug + Display + PartialEq.
// In v0.47 this blanket impl covers PublishError, RequestError, and
// SubscribeError (all type aliases of `Error<...>`) — no separate impl needed.
#[cfg(feature = "g2v-nats")]
impl<T> From<async_nats::error::Error<T>> for ServiceError
where
    T: std::clone::Clone + std::fmt::Debug + std::fmt::Display + PartialEq,
{
    fn from(err: async_nats::error::Error<T>) -> Self {
        ServiceError::Internal(format!("NATS error: {}", err))
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
    use connectrpc::ConnectError;

    // Test: ServiceError::from(ConnectError) conversion
    #[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
    #[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
    #[test]
    fn test_error_from_connect_error() {
        use ErrorCode::*;
        let test_cases = vec![
            (
                InvalidArgument,
                ServiceError::InvalidArgument("test".into()),
            ),
            (NotFound, ServiceError::NotFound("test".into())),
            (AlreadyExists, ServiceError::AlreadyExists("test".into())),
            (
                PermissionDenied,
                ServiceError::PermissionDenied("test".into()),
            ),
            (
                Unauthenticated,
                ServiceError::Unauthenticated("test".into()),
            ),
            (Internal, ServiceError::Internal("test".into())),
            (Unavailable, ServiceError::Unavailable("test".into())),
            (Unimplemented, ServiceError::Unimplemented("test".into())),
            (
                DeadlineExceeded,
                ServiceError::DeadlineExceeded("test".into()),
            ),
        ];

        for (code, expected_service_err) in test_cases {
            let connect_err = ConnectError::new(code, "test");
            let service_err: ServiceError = connect_err.into();

            // Check that the variants match
            match (&service_err, &expected_service_err) {
                (ServiceError::InvalidArgument(_), ServiceError::InvalidArgument(_)) => {}
                (ServiceError::NotFound(_), ServiceError::NotFound(_)) => {}
                (ServiceError::AlreadyExists(_), ServiceError::AlreadyExists(_)) => {}
                (ServiceError::PermissionDenied(_), ServiceError::PermissionDenied(_)) => {}
                (ServiceError::Unauthenticated(_), ServiceError::Unauthenticated(_)) => {}
                (ServiceError::Internal(_), ServiceError::Internal(_)) => {}
                (ServiceError::Unavailable(_), ServiceError::Unavailable(_)) => {}
                (ServiceError::Unimplemented(_), ServiceError::Unimplemented(_)) => {}
                (ServiceError::DeadlineExceeded(_), ServiceError::DeadlineExceeded(_)) => {}
                _ => panic!(
                    "Mismatch for code {:?}: got {:?}, expected {:?}",
                    code, service_err, expected_service_err
                ),
            }
        }
    }

    // Test: ServiceError display messages
    #[test]
    fn test_error_display() {
        let test_cases = vec![
            (
                ServiceError::InvalidArgument("bad input".into()),
                "Invalid argument: bad input",
            ),
            (
                ServiceError::NotFound("user 123".into()),
                "Not found: user 123",
            ),
            (
                ServiceError::AlreadyExists("email@test.com".into()),
                "Already exists: email@test.com",
            ),
            (
                ServiceError::PermissionDenied("read:admin".into()),
                "Permission denied: read:admin",
            ),
            (
                ServiceError::Unauthenticated("token expired".into()),
                "Unauthenticated: token expired",
            ),
            (
                ServiceError::Internal("server crash".into()),
                "Internal error: server crash",
            ),
            (
                ServiceError::Unavailable("maintenance".into()),
                "Service unavailable: maintenance",
            ),
            (
                ServiceError::Unimplemented("feature X".into()),
                "Unimplemented: feature X",
            ),
            (
                ServiceError::DeadlineExceeded("30s timeout".into()),
                "Deadline exceeded: 30s timeout",
            ),
            (
                ServiceError::Database("connection failed".into()),
                "Database error: connection failed",
            ),
            (
                ServiceError::Configuration("missing env var".into()),
                "Configuration error: missing env var",
            ),
            (
                ServiceError::Serialization("json parse failed".into()),
                "Serialization error: json parse failed",
            ),
        ];

        for (err, expected) in test_cases {
            assert_eq!(err.to_string(), expected);
        }
    }

    // Test: ServiceError code mapping
    #[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
    #[test]
    fn test_error_code_mapping() {
        use ErrorCode::*;

        assert_eq!(
            ServiceError::InvalidArgument("".into()).code(),
            InvalidArgument
        );
        assert_eq!(ServiceError::NotFound("".into()).code(), NotFound);
        assert_eq!(ServiceError::AlreadyExists("".into()).code(), AlreadyExists);
        assert_eq!(
            ServiceError::PermissionDenied("".into()).code(),
            PermissionDenied
        );
        assert_eq!(
            ServiceError::Unauthenticated("".into()).code(),
            Unauthenticated
        );
        assert_eq!(ServiceError::Internal("".into()).code(), Internal);
        assert_eq!(ServiceError::Unavailable("".into()).code(), Unavailable);
        assert_eq!(ServiceError::Unimplemented("".into()).code(), Unimplemented);
        assert_eq!(
            ServiceError::DeadlineExceeded("".into()).code(),
            DeadlineExceeded
        );
        assert_eq!(ServiceError::Database("".into()).code(), Internal);
        assert_eq!(ServiceError::Configuration("".into()).code(), Internal);
        assert_eq!(ServiceError::Serialization("".into()).code(), Internal);
    }

    // Test: ServiceError http_status mapping
    #[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
    #[test]
    fn test_error_http_status() {
        assert_eq!(
            ServiceError::InvalidArgument("".into()).http_status(),
            http::StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ServiceError::NotFound("".into()).http_status(),
            http::StatusCode::NOT_FOUND
        );
        assert_eq!(
            ServiceError::AlreadyExists("".into()).http_status(),
            http::StatusCode::CONFLICT
        );
        assert_eq!(
            ServiceError::PermissionDenied("".into()).http_status(),
            http::StatusCode::FORBIDDEN
        );
        assert_eq!(
            ServiceError::Unauthenticated("".into()).http_status(),
            http::StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            ServiceError::Internal("".into()).http_status(),
            http::StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            ServiceError::Unavailable("".into()).http_status(),
            http::StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            ServiceError::Unimplemented("".into()).http_status(),
            http::StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            ServiceError::DeadlineExceeded("".into()).http_status(),
            http::StatusCode::GATEWAY_TIMEOUT
        );
    }

    // Test: ServiceError message accessor
    #[test]
    fn test_error_message() {
        let msg = "test message";
        let err = ServiceError::Internal(msg.into());
        assert_eq!(err.message(), msg);
    }

    // Test: From<Box<dyn Error>> for ServiceError
    #[test]
    fn test_error_from_box_error() {
        let err: Box<dyn std::error::Error + Send + Sync> =
            Box::new(std::io::Error::other("IO error"));
        let service_err: ServiceError = err.into();

        match service_err {
            ServiceError::Internal(msg) => assert!(msg.contains("IO error")),
            _ => panic!("Expected Internal variant"),
        }
    }

    // Test: From<anyhow::Error> for ServiceError
    #[test]
    fn test_error_from_anyhow() {
        let err = anyhow::anyhow!("Anyhow error");
        let service_err: ServiceError = err.into();

        match service_err {
            ServiceError::Internal(msg) => assert!(msg.contains("Anyhow error")),
            _ => panic!("Expected Internal variant"),
        }
    }

    // Test: From<serde_json::Error> for ServiceError
    #[test]
    fn test_error_from_serde_json() {
        // Create a serde_json error by attempting to parse invalid JSON
        let err: serde_json::Error =
            serde_json::from_str::<serde_json::Value>("invalid json").unwrap_err();
        let service_err: ServiceError = err.into();

        match service_err {
            ServiceError::Serialization(_) => {}
            _ => panic!("Expected Serialization variant"),
        }
    }

    // Test: ServiceError implements Send + Sync
    #[test]
    fn test_error_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<ServiceError>();
        assert_sync::<ServiceError>();
    }

    // Test: ServiceError Clone
    #[test]
    fn test_error_clone() {
        let err = ServiceError::Internal("original".into());
        let cloned = err.clone();

        match (err, cloned) {
            (ServiceError::Internal(msg1), ServiceError::Internal(msg2)) => {
                assert_eq!(msg1, msg2);
            }
            _ => panic!("Clone produced different variant"),
        }
    }

    #[test]
    fn test_error_message_all_variants() {
        let errors = vec![
            ServiceError::InvalidArgument("invalid".into()),
            ServiceError::NotFound("not-found".into()),
            ServiceError::AlreadyExists("exists".into()),
            ServiceError::PermissionDenied("denied".into()),
            ServiceError::Unauthenticated("unauthenticated".into()),
            ServiceError::Internal("internal".into()),
            ServiceError::Unavailable("unavailable".into()),
            ServiceError::Unimplemented("unimplemented".into()),
            ServiceError::DeadlineExceeded("deadline".into()),
            ServiceError::Database("database".into()),
            ServiceError::Configuration("configuration".into()),
            ServiceError::Serialization("serialization".into()),
        ];

        for error in errors {
            assert!(!error.message().is_empty());
        }
    }

    #[cfg(any(feature = "g2v-client-connectrpc", feature = "g2v-server"))]
    #[test]
    fn test_error_to_connect_error() {
        let connect: ConnectError = ServiceError::Unavailable("try later".into()).into();
        assert_eq!(connect.code, ErrorCode::Unavailable);
        assert_eq!(connect.message.as_deref(), Some("try later"));
    }

    #[test]
    fn test_error_from_typed_box_error() {
        let err = Box::new(std::io::Error::other("typed failure"));
        let service_err: ServiceError = err.into();
        assert!(
            matches!(service_err, ServiceError::Internal(message) if message.contains("typed failure"))
        );
    }

    #[cfg(feature = "g2v-server")]
    #[test]
    fn test_error_from_config_error() {
        let service_err: ServiceError =
            crate::g2v::config::ConfigError::Validation("bad config".to_string()).into();
        assert!(
            matches!(service_err, ServiceError::Configuration(message) if message.contains("bad config"))
        );
    }

    #[cfg(feature = "g2v-sqlx")]
    #[cfg(feature = "g2v-sqlx")]
    #[test]
    fn test_error_from_sqlx_error() {
        let service_err: ServiceError = sqlx::Error::RowNotFound.into();
        assert!(matches!(service_err, ServiceError::Database(_)));
    }
}

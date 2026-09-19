//! Authentication and authorization middleware.
//!
//! Provides multitenancy-aware authn/authz for Sunbeam services:
//!
//! - `auth_middleware` resolves the caller to a [`TenantId`] using a bearer
//!   token (introspection), a session cookie, or a session token header.
//! - [`PermissionLayer`] enforces ReBAC permission checks against the configured
//!   authorization backend.
//! - [`AuthorizationClient`] talks to that backend over HTTP.

#[cfg(feature = "g2v-server")]
pub mod authorization;
#[cfg(feature = "g2v-server")]
pub mod cookie_signer;
#[cfg(feature = "g2v-server")]
pub mod error;
#[cfg(feature = "g2v-server")]
pub mod introspection;
#[cfg(feature = "g2v-server")]
pub mod permission;
#[cfg(feature = "g2v-server")]
pub mod session;
#[cfg(feature = "g2v-server")]
pub mod session_token;

#[cfg(feature = "g2v-server")]
pub use authorization::{AuthorizationClient, AuthorizationConfig};
#[cfg(feature = "g2v-server")]
pub use cookie_signer::{CookieError, CookieSigner};
#[cfg(feature = "g2v-server")]
pub use introspection::{
    CachedIntrospectionSessionClient, IntrospectionConfig, IntrospectionSessionClient,
};
#[cfg(feature = "g2v-server")]
pub use permission::{ObjectExtractor, PermissionLayer, PermissionService};
#[cfg(feature = "g2v-server")]
pub use session::{SessionClient, SessionStore};
#[cfg(feature = "g2v-server")]
pub use session_token::{SessionClaims, SessionTokenError, SessionTokenSigner};

use axum::{
    body::Body,
    extract::Request,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

pub use error::AuthError;

/// Header carrying a session token.
pub const SESSION_TOKEN_HEADER: &str = "x-session-token";
/// Default session cookie name.
pub const SESSION_COOKIE_NAME: &str = "__Host-sso_session";

/// Resolved tenant for the request.
#[derive(Debug, Clone)]
pub struct TenantId(pub String);

/// Slim identity context for use in handlers.
#[derive(Debug, Clone, Default)]
pub struct AuthContext {
    /// Resolved tenant id.
    pub tenant_id: Option<String>,
    /// Authenticated subject (identity id or client id).
    pub subject: Option<String>,
    /// Actor scopes, if present in the token.
    pub scopes: Vec<String>,
    /// SHA-256 hash of the raw token, for audit logging.
    pub token_hash: Option<String>,
    /// Authentication Method Reference values (e.g. `password`, `totp`).
    pub authentication_methods: Vec<String>,
}

impl AuthContext {
    /// Create an unauthenticated context.
    pub fn unauthenticated() -> Self {
        Self::default()
    }

    /// Create an authenticated context for the given tenant and subject.
    pub fn authenticated(tenant_id: impl Into<String>, subject: impl Into<String>) -> Self {
        Self {
            tenant_id: Some(tenant_id.into()),
            subject: Some(subject.into()),
            scopes: Vec::new(),
            token_hash: None,
            authentication_methods: Vec::new(),
        }
    }

    /// Set scopes.
    pub fn with_scopes(mut self, scopes: Vec<String>) -> Self {
        self.scopes = scopes;
        self
    }

    /// Set the token hash.
    pub fn with_token_hash(mut self, hash: impl Into<String>) -> Self {
        self.token_hash = Some(hash.into());
        self
    }

    /// Set authentication methods.
    pub fn with_authentication_methods(mut self, methods: Vec<String>) -> Self {
        self.authentication_methods = methods;
        self
    }

    /// Returns true when a subject has been resolved.
    pub fn is_authenticated(&self) -> bool {
        self.subject.is_some()
    }

    /// Require a scope from this context.
    ///
    /// Returns `ServiceError::PermissionDenied` if the scope is missing.
    pub fn require_scope(&self, scope: &str) -> Result<(), crate::g2v::error::ServiceError> {
        if !self.scopes.iter().any(|s| s == scope) {
            return Err(crate::g2v::error::ServiceError::PermissionDenied(format!(
                "missing required scope: {scope}"
            )));
        }
        Ok(())
    }

    /// Require an Authentication Method Reference from this context.
    ///
    /// Returns `ServiceError::PermissionDenied` if the method is missing.
    /// Intended for RPC handlers that need stepped-up assurance (e.g. admin
    /// operations requiring a second factor).
    pub fn require_amr(&self, method: &str) -> Result<(), crate::g2v::error::ServiceError> {
        if !self.authentication_methods.iter().any(|m| m == method) {
            return Err(crate::g2v::error::ServiceError::PermissionDenied(format!(
                "missing required authentication method: {method}"
            )));
        }
        Ok(())
    }
}

/// Shared state required by [`auth_middleware`].
#[derive(Clone)]
pub struct AuthMiddlewareState {
    /// Validates sessions (via sso-gateway introspection).
    pub sessions: Arc<dyn SessionClient>,
    /// Signs and verifies session cookies.
    pub session_signer: Option<Arc<SessionTokenSigner>>,
    /// Checks server-side session revocation.
    pub session_store: Option<Arc<dyn SessionStore>>,
}

impl std::fmt::Debug for AuthMiddlewareState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthMiddlewareState")
            .field("sessions", &"<dyn SessionClient>")
            .field("session_signer", &self.session_signer.is_some())
            .field("session_store", &self.session_store.is_some())
            .finish()
    }
}

impl AuthMiddlewareState {
    /// Create state with the required session client.
    pub fn new(sessions: Arc<dyn SessionClient>) -> Self {
        Self {
            sessions,
            session_signer: None,
            session_store: None,
        }
    }

    /// Attach a session token signer for cookie-based authentication.
    pub fn with_session_signer(mut self, signer: SessionTokenSigner) -> Self {
        self.session_signer = Some(Arc::new(signer));
        self
    }

    /// Attach a session store for revocation checking.
    pub fn with_session_store(mut self, store: Arc<dyn SessionStore>) -> Self {
        self.session_store = Some(store);
        self
    }
}

/// Axum middleware that resolves authentication and inserts a [`TenantId`] and
/// [`AuthContext`] into request extensions.
///
/// Public OAuth2/OIDC discovery paths are not validated here; handlers for those
/// routes perform their own tenant validation.
///
/// Rejections are WARN-logged with the request method, path, failure class,
/// and latency, and return a body that carries the failure class: ConnectRPC
/// routes (`/pkg.Service/Method`) get a Connect-shaped
/// `{"code":"unauthenticated","message":"<class>"}`; other routes get
/// `{"error":"<class>","message":"<detail>"}`.
pub async fn auth_middleware(
    axum::extract::State(state): axum::extract::State<AuthMiddlewareState>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if is_public_path(path) {
        return next.run(request).await;
    }

    // Captured up front so rejections can be logged with request context —
    // the session client never sees the method or path (G2V-001).
    let method = request.method().clone();
    let path = path.to_string();
    let started = std::time::Instant::now();

    // 1. Bearer token authentication (introspection flows).
    if let Some(token) = bearer_token(request.headers()) {
        match authenticate_session(state.sessions.as_ref(), None, Some(&token)).await {
            Ok((tenant_id, identity_id)) => {
                let auth_ctx = AuthContext::authenticated(&tenant_id, &identity_id)
                    .with_token_hash(hash_token(&token));
                request.extensions_mut().insert(TenantId(tenant_id));
                request.extensions_mut().insert(auth_ctx);
            }
            Err(err) => return authn_rejection(&method, &path, started, &err),
        }
        return next.run(request).await;
    }

    // 2. Session cookie authentication (signed opaque tokens).
    if let Some(cookie) = session_cookie(request.headers(), SESSION_COOKIE_NAME)
        && let Some(signer) = &state.session_signer
    {
        match authenticate_session_cookie(signer.as_ref(), state.session_store.as_deref(), &cookie)
            .await
        {
            Ok((tenant_id, subject)) => {
                let auth_ctx = AuthContext::authenticated(&tenant_id, &subject);
                request.extensions_mut().insert(TenantId(tenant_id));
                request.extensions_mut().insert(auth_ctx);
            }
            Err(err) => return authn_rejection(&method, &path, started, &err),
        }
        return next.run(request).await;
    }

    // 3. Session token header (API flows).
    if let Some(token) = header_value(&request, SESSION_TOKEN_HEADER) {
        match authenticate_session(state.sessions.as_ref(), None, Some(&token)).await {
            Ok((tenant_id, identity_id)) => {
                let auth_ctx = AuthContext::authenticated(&tenant_id, &identity_id);
                request.extensions_mut().insert(TenantId(tenant_id));
                request.extensions_mut().insert(auth_ctx);
            }
            Err(err) => return authn_rejection(&method, &path, started, &err),
        }
        return next.run(request).await;
    }

    // 4. Session cookie header (browser flows).
    if let Some(cookie) = header_value(&request, "cookie") {
        match authenticate_session(state.sessions.as_ref(), Some(&cookie), None).await {
            Ok((tenant_id, identity_id)) => {
                let auth_ctx = AuthContext::authenticated(&tenant_id, &identity_id);
                request.extensions_mut().insert(TenantId(tenant_id));
                request.extensions_mut().insert(auth_ctx);
            }
            Err(err) => return authn_rejection(&method, &path, started, &err),
        }
        return next.run(request).await;
    }

    // 5. Bare tenant header is not a valid authentication method.
    // Without a bearer token, session cookie, or session token, the request is unauthenticated.
    authn_rejection(&method, &path, started, &AuthError::MissingCredentials)
}

fn is_public_path(path: &str) -> bool {
    path.starts_with("/.well-known/")
        || path.starts_with("/oauth2/")
        || path.starts_with("/health/")
}

fn header_value(request: &Request, name: &str) -> Option<String> {
    request
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
}

/// Extract a bearer token from the `Authorization` header.
///
/// The `Bearer` prefix is matched case-insensitively and empty tokens are
/// rejected.
pub fn bearer_token(headers: &axum::http::HeaderMap) -> Option<String> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| {
            let (scheme, token) = v.split_once(' ')?;
            if !scheme.eq_ignore_ascii_case("Bearer") || token.is_empty() {
                return None;
            }
            Some(token.to_string())
        })
}

/// Extract a named cookie value from the `Cookie` header.
pub fn session_cookie(headers: &axum::http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let (cookie_name, value) = cookie.trim().split_once('=')?;
                if cookie_name == name {
                    Some(value.to_string())
                } else {
                    None
                }
            })
        })
}

/// Hash a secret token for cache keys and audit logging.
pub fn hash_token(token: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

async fn authenticate_session(
    client: &dyn SessionClient,
    cookie: Option<&str>,
    token: Option<&str>,
) -> Result<(String, String), AuthError> {
    let session = client.to_session(cookie, token).await?;

    // sso-gateway introspection injects tenant_id directly into the response.
    if let Some(tenant_id) = session.get("tenant_id").and_then(|v| v.as_str()) {
        let subject = session
            .get("sub")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        return Ok((tenant_id.to_string(), subject));
    }

    // Fall back to identity-based session extraction.
    let upstream_identity_id = session["identity"]["id"].as_str().unwrap_or("").to_string();

    if upstream_identity_id.is_empty() {
        return Err(AuthError::InvalidSession(
            "session missing identity".to_string(),
        ));
    }

    Ok((upstream_identity_id.clone(), upstream_identity_id))
}

async fn authenticate_session_cookie(
    signer: &SessionTokenSigner,
    store: Option<&dyn SessionStore>,
    cookie: &str,
) -> Result<(String, String), AuthError> {
    let claims = signer.verify(cookie).map_err(|err| {
        tracing::debug!(%err, "session cookie verification failed");
        AuthError::InvalidSession("invalid or expired session cookie".to_string())
    })?;

    if let Some(store) = store {
        let active = store.is_active(&claims.sid).await.map_err(|err| {
            AuthError::AuthorizationBackendUnavailable(format!("session store lookup: {err}"))
        })?;
        if !active {
            return Err(AuthError::InactiveSession);
        }
    }

    Ok((claims.tenant_id, claims.sub))
}

/// Whether the path is a ConnectRPC route (`/pkg.Service/Method`), which
/// determines the error body shape.
fn is_connectrpc_path(path: &str) -> bool {
    let Some(trimmed) = path.strip_prefix('/') else {
        return false;
    };
    let Some((service, method)) = trimmed.split_once('/') else {
        return false;
    };
    service.contains('.') && !method.is_empty() && !method.contains('/')
}

/// Build the rejection response for a failed authentication attempt and
/// WARN-log it with request context.
///
/// Transient backend failures (gateway error, timeout, transport) are still
/// answered 401 — the caller cannot distinguish them from a bad credential —
/// but the failure class in the body and logs makes the difference visible
/// to operators (G2V-001).
fn authn_rejection(
    method: &axum::http::Method,
    path: &str,
    started: std::time::Instant,
    err: &AuthError,
) -> Response {
    let class = err.failure_class();
    let status = match err {
        AuthError::AuthorizationBackend(_) | AuthError::AuthorizationBackendUnavailable(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
        _ => StatusCode::UNAUTHORIZED,
    };

    tracing::warn!(
        msg = "authn rejection",
        method = %method,
        path,
        class,
        status = status.as_u16(),
        latency_ms = started.elapsed().as_millis() as u64,
        error = %err,
    );

    let body = if is_connectrpc_path(path) {
        let code = if status == StatusCode::UNAUTHORIZED {
            "unauthenticated"
        } else {
            "internal"
        };
        serde_json::json!({ "code": code, "message": class }).to_string()
    } else {
        serde_json::json!({ "error": class, "message": err.to_string() }).to_string()
    };

    (
        status,
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        Body::from(body),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_public_path_matches_public_prefixes() {
        assert!(is_public_path("/.well-known/openid-configuration"));
        assert!(is_public_path("/oauth2/auth"));
        assert!(is_public_path("/health/live"));
        assert!(!is_public_path("/api/v1/things"));
    }

    #[test]
    fn bearer_token_extracts_token() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer secret-token"),
        );
        assert_eq!(bearer_token(&headers), Some("secret-token".to_string()));
    }

    #[test]
    fn bearer_token_is_case_insensitive() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("bearer secret-token"),
        );
        assert_eq!(bearer_token(&headers), Some("secret-token".to_string()));
    }

    #[test]
    fn bearer_token_rejects_empty_token() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Bearer "),
        );
        assert_eq!(bearer_token(&headers), None);
    }

    #[test]
    fn bearer_token_rejects_non_bearer() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::AUTHORIZATION,
            axum::http::HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );
        assert_eq!(bearer_token(&headers), None);
    }

    #[test]
    fn session_cookie_extracts_named_cookie() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            axum::http::HeaderValue::from_static("other=1; __Host-sso_session=abc123; another=2"),
        );
        assert_eq!(
            session_cookie(&headers, SESSION_COOKIE_NAME),
            Some("abc123".to_string())
        );
    }

    #[test]
    fn session_cookie_returns_none_when_missing() {
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::COOKIE,
            axum::http::HeaderValue::from_static("other=1"),
        );
        assert_eq!(session_cookie(&headers, SESSION_COOKIE_NAME), None);
    }

    #[test]
    fn hash_token_is_deterministic_and_hex() {
        let h1 = hash_token("my-secret-token");
        let h2 = hash_token("my-secret-token");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_token_differs_for_different_tokens() {
        let h1 = hash_token("token-one");
        let h2 = hash_token("token-two");
        assert_ne!(h1, h2);
    }

    #[test]
    fn is_connectrpc_path_matches_service_method() {
        assert!(is_connectrpc_path("/connectrpc.eliza.v1.ElizaService/Say"));
        assert!(is_connectrpc_path("/iam.v1.TenantService/CreateTenant"));
        assert!(!is_connectrpc_path("/health/live"));
        assert!(!is_connectrpc_path("/whoami"));
        assert!(!is_connectrpc_path("/g2v/secrets/42"));
        assert!(!is_connectrpc_path("/"));
    }

    #[test]
    fn authn_rejection_shapes_connect_error_body() {
        let response = authn_rejection(
            &axum::http::Method::POST,
            "/connectrpc.eliza.v1.ElizaService/Say",
            std::time::Instant::now(),
            &AuthError::InactiveSession,
        );
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response.into_body();
        let bytes =
            tokio_test::block_on(axum::body::to_bytes(body, usize::MAX)).expect("body should read");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
        assert_eq!(json["code"], "unauthenticated");
        assert_eq!(json["message"], "inactive");
    }

    #[test]
    fn authn_rejection_shapes_plain_json_body() {
        let response = authn_rejection(
            &axum::http::Method::GET,
            "/whoami",
            std::time::Instant::now(),
            &AuthError::SessionTimeout("deadline".to_string()),
        );
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = response.into_body();
        let bytes =
            tokio_test::block_on(axum::body::to_bytes(body, usize::MAX)).expect("body should read");
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
        assert_eq!(json["error"], "timeout");
        assert!(
            json["message"]
                .as_str()
                .expect("message")
                .contains("deadline"),
            "detail preserved: {json}"
        );
    }

    #[test]
    fn authn_rejection_maps_backend_failures_to_500() {
        let response = authn_rejection(
            &axum::http::Method::GET,
            "/whoami",
            std::time::Instant::now(),
            &AuthError::AuthorizationBackendUnavailable("store down".to_string()),
        );
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn auth_context_require_scope_enforces() {
        let ctx =
            AuthContext::authenticated("tenant-1", "sub-1").with_scopes(vec!["tenant:read".into()]);
        assert!(ctx.require_scope("tenant:read").is_ok());
        let err = ctx.require_scope("tenant:write").unwrap_err();
        assert!(matches!(
            err,
            crate::g2v::error::ServiceError::PermissionDenied(_)
        ));
    }

    #[test]
    fn auth_context_require_amr_enforces() {
        let ctx = AuthContext::authenticated("tenant-1", "sub-1")
            .with_authentication_methods(vec!["password".into(), "totp".into()]);
        assert!(ctx.require_amr("password").is_ok());
        assert!(ctx.require_amr("totp").is_ok());
        let err = ctx.require_amr("webauthn").unwrap_err();
        assert!(matches!(
            err,
            crate::g2v::error::ServiceError::PermissionDenied(_)
        ));
    }

    #[test]
    fn auth_context_builders_cover_identity_fields() {
        let ctx = AuthContext::unauthenticated();
        assert!(!ctx.is_authenticated());

        let ctx = AuthContext::authenticated("tenant-1", "subject-1")
            .with_token_hash("hash-1")
            .with_authentication_methods(vec!["password".into()]);
        assert!(ctx.is_authenticated());
        assert_eq!(ctx.token_hash.as_deref(), Some("hash-1"));
        assert_eq!(ctx.authentication_methods, vec!["password"]);
    }
}

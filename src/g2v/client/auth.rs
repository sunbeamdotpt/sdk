//! Authentication primitives for the Sunbeam HTTP client.
//!
//! Provides bearer-token injection and an OAuth2 client-credentials token
//! provider, both implemented as a Tower [`Layer`] / [`Service`] so the auth
//! token is attached to every outgoing request.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context as TaskContext, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http::{Request, Response, header::AUTHORIZATION};
use tower::{Layer, Service};

use crate::g2v::BoxError;

/// Something that can produce a bearer token on demand.
///
/// Implemented by static bearer tokens and by OAuth2 client-credentials flows.
pub trait TokenProvider: Send + Sync + 'static {
    /// Fetch a valid access token.
    fn token(&self) -> Pin<Box<dyn Future<Output = Result<String, BoxError>> + Send + '_>>;

    /// Invalidate the currently cached token, forcing a refresh on the next
    /// call.
    ///
    /// The default implementation does nothing.
    fn invalidate(&self) {}

    /// Whether this provider can produce a *different* token after
    /// [`invalidate`](TokenProvider::invalidate) — i.e. whether a 401 can be
    /// recovered by refreshing.
    ///
    /// Static providers (e.g. [`BearerToken`]) are not refresh-capable: the
    /// default is `false`, and [`AuthService`] fails fast with
    /// [`TokenNotRefreshableError`] on 401 instead of silently re-serving the
    /// rejected token.
    fn is_refresh_capable(&self) -> bool {
        false
    }
}

/// A static bearer token.
#[derive(Clone, Debug)]
pub struct BearerToken {
    token: String,
}

impl BearerToken {
    /// Create a new bearer token provider.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

impl TokenProvider for BearerToken {
    fn token(&self) -> Pin<Box<dyn Future<Output = Result<String, BoxError>> + Send + '_>> {
        let token = self.token.clone();
        Box::pin(async move { Ok(token) })
    }
}

/// OAuth2 client-credentials token provider.
///
/// Fetches access tokens from a token endpoint, caches them by `expires_in`,
/// and refreshes on demand or when [`TokenProvider::invalidate`] is called.
#[derive(Clone, Debug)]
pub struct OAuth2ClientCredentials {
    token_url: String,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
    audience: Option<String>,
    cache: Arc<Mutex<Option<(String, Instant)>>>,
}

impl OAuth2ClientCredentials {
    /// Create a new OAuth2 client-credentials provider.
    pub fn new(
        token_url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self {
            token_url: token_url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            scope: None,
            audience: None,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    /// Restrict the token to the given OAuth2 scope.
    #[must_use]
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    /// Request the token for the given OAuth2 audience.
    #[must_use]
    pub fn with_audience(mut self, audience: impl Into<String>) -> Self {
        self.audience = Some(audience.into());
        self
    }

    async fn fetch_token(&self) -> Result<String, BoxError> {
        let client = reqwest::Client::new();
        let mut params = std::collections::HashMap::new();
        params.insert("grant_type", "client_credentials");
        params.insert("client_id", &self.client_id);
        params.insert("client_secret", &self.client_secret);
        if let Some(scope) = &self.scope {
            params.insert("scope", scope);
        }
        if let Some(audience) = &self.audience {
            params.insert("audience", audience);
        }

        let resp: serde_json::Value = client
            .post(&self.token_url)
            .form(&params)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let token = resp["access_token"]
            .as_str()
            .ok_or("missing access_token in OAuth2 response")?
            .to_string();
        let expires_in = resp["expires_in"].as_u64().unwrap_or(3600);
        // Refresh slightly before expiry to avoid edge-of-window rejects.
        let usable_for = Duration::from_secs(expires_in.saturating_sub(30).max(1));

        *match self.cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                // A poisoned token cache still holds valid state; recover it
                // rather than failing every request.
                tracing::warn!("oauth2 token cache lock poisoned; recovering");
                poisoned.into_inner()
            }
        } = Some((token.clone(), Instant::now() + usable_for));
        Ok(token)
    }
}

impl TokenProvider for OAuth2ClientCredentials {
    fn token(&self) -> Pin<Box<dyn Future<Output = Result<String, BoxError>> + Send + '_>> {
        let this = self.clone();
        Box::pin(async move {
            {
                let guard = match this.cache.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        tracing::warn!("oauth2 token cache lock poisoned; recovering");
                        poisoned.into_inner()
                    }
                };
                if let Some((token, expiry)) = guard.as_ref()
                    && Instant::now() < *expiry
                {
                    return Ok(token.clone());
                }
            }
            this.fetch_token().await
        })
    }

    fn invalidate(&self) {
        *match self.cache.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                tracing::warn!("oauth2 token cache lock poisoned; recovering");
                poisoned.into_inner()
            }
        } = None;
    }

    fn is_refresh_capable(&self) -> bool {
        true
    }
}

impl TokenProvider for Arc<dyn TokenProvider> {
    fn token(&self) -> Pin<Box<dyn Future<Output = Result<String, BoxError>> + Send + '_>> {
        let cloned = Arc::clone(self);
        Box::pin(async move { cloned.as_ref().token().await })
    }

    fn invalidate(&self) {
        (**self).invalidate();
    }

    fn is_refresh_capable(&self) -> bool {
        (**self).is_refresh_capable()
    }
}

/// Error returned when a request is rejected with 401 and the configured
/// [`TokenProvider`] cannot refresh the token.
///
/// Surfaced instead of silently re-sending the same rejected token
/// (SSO-023/G2V-002): a static [`BearerToken`] that the server rejects is a
/// configuration problem, and retrying it would only extend the outage.
#[derive(Debug, Clone, Copy)]
pub struct TokenNotRefreshableError;

impl std::fmt::Display for TokenNotRefreshableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "request rejected with 401 Unauthenticated and the token provider is not \
             refresh-capable; use a refresh-capable provider (e.g. OAuth2ClientCredentials) \
             or fix the static token"
        )
    }
}

impl std::error::Error for TokenNotRefreshableError {}

/// Tower [`Layer`] that injects bearer tokens from a [`TokenProvider`].
#[derive(Clone)]
pub struct AuthLayer {
    provider: Arc<dyn TokenProvider>,
}

impl AuthLayer {
    /// Create a new auth layer backed by the given provider.
    pub fn new<P>(provider: P) -> Self
    where
        P: TokenProvider,
    {
        Self {
            provider: Arc::new(provider),
        }
    }
}

impl std::fmt::Debug for AuthLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthLayer").finish()
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            provider: Arc::clone(&self.provider),
        }
    }
}

/// Tower [`Service`] produced by [`AuthLayer`].
#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    provider: Arc<dyn TokenProvider>,
}

impl<S> std::fmt::Debug for AuthService<S>
where
    S: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthService")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<S> Service<Request<Bytes>> for AuthService<S>
where
    S: Service<Request<Bytes>, Response = Response<Bytes>> + Clone + Send + 'static,
    S::Error: From<BoxError> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Bytes>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Bytes>) -> Self::Future {
        let provider = Arc::clone(&self.provider);
        let inner = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, inner);
        let original_req = req.clone();

        Box::pin(async move {
            let token = provider.token().await?;
            let resp = inner.call(with_auth(req, &token)).await?;

            if resp.status() == http::StatusCode::UNAUTHORIZED {
                // A static provider would re-serve the same rejected token;
                // fail fast with a clear error instead (G2V-002).
                if !provider.is_refresh_capable() {
                    return Err(S::Error::from(
                        Box::new(TokenNotRefreshableError) as BoxError
                    ));
                }

                // Transient auth wedges (SSO-023) clear on a single refresh;
                // pause briefly per the shared retry schedule, then retry once.
                provider.invalidate();
                let new_token = provider.token().await?;
                let backoff = super::retry::RetryPolicy::default().backoff(0);
                tokio::time::sleep(backoff).await;
                let retry_req = remove_auth(original_req);
                return inner.call(with_auth(retry_req, &new_token)).await;
            }

            Ok(resp)
        })
    }
}

fn with_auth(mut req: Request<Bytes>, token: &str) -> Request<Bytes> {
    let value = format!("Bearer {token}");
    if let Ok(header) = http::HeaderValue::from_str(&value) {
        req.headers_mut().insert(AUTHORIZATION, header);
    }
    req
}

fn remove_auth(mut req: Request<Bytes>) -> Request<Bytes> {
    req.headers_mut().remove(AUTHORIZATION);
    req
}

#[cfg(all(test, feature = "g2v-server"))]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn test_bearer_token_provider() {
        let provider = BearerToken::new("secret-token");
        assert_eq!(provider.token().await.unwrap(), "secret-token");
    }

    #[tokio::test]
    async fn test_oauth2_cache_reuses_token() {
        // The OAuth2 provider cannot fetch a real token in a unit test, but we
        // can verify the cache path by manually priming it.
        let provider =
            OAuth2ClientCredentials::new("http://example.com/token", "client-id", "client-secret");
        *provider.cache.lock().unwrap() = Some((
            "cached".to_string(),
            Instant::now() + Duration::from_secs(60),
        ));
        assert_eq!(provider.token().await.unwrap(), "cached");
    }

    #[tokio::test]
    async fn test_oauth2_invalidate_clears_cache() {
        let provider =
            OAuth2ClientCredentials::new("http://example.com/token", "client-id", "client-secret");
        *provider.cache.lock().unwrap() = Some((
            "cached".to_string(),
            Instant::now() + Duration::from_secs(60),
        ));
        provider.invalidate();
        assert!(provider.cache.lock().unwrap().is_none());
    }

    #[tokio::test]
    async fn test_auth_service_injects_token() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use tower::{ServiceBuilder, ServiceExt};

        let call_count = Arc::new(AtomicUsize::new(0));
        let inner = tower::service_fn(move |req: Request<Bytes>| {
            let count = call_count.fetch_add(1, Ordering::SeqCst);
            async move {
                let auth = req
                    .headers()
                    .get(AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("")
                    .to_string();
                if count == 0 && auth == "Bearer first" {
                    Ok::<_, BoxError>(
                        http::Response::builder()
                            .status(401)
                            .body(Bytes::new())
                            .unwrap(),
                    )
                } else {
                    Ok::<_, BoxError>(
                        http::Response::builder()
                            .status(200)
                            .body(Bytes::from(auth))
                            .unwrap(),
                    )
                }
            }
        });

        struct RotatingToken {
            calls: AtomicUsize,
        }
        impl TokenProvider for RotatingToken {
            fn token(&self) -> Pin<Box<dyn Future<Output = Result<String, BoxError>> + Send + '_>> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                let token = if n == 0 {
                    "first".to_string()
                } else {
                    "second".to_string()
                };
                Box::pin(async move { Ok(token) })
            }

            fn is_refresh_capable(&self) -> bool {
                true
            }
        }

        let mut svc = ServiceBuilder::new()
            .layer(AuthLayer::new(RotatingToken {
                calls: AtomicUsize::new(0),
            }))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(Request::new(Bytes::new()))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.body().as_ref(), b"Bearer second");
    }

    #[tokio::test]
    async fn test_auth_service_static_token_fails_fast_on_401() {
        use tower::{ServiceBuilder, ServiceExt};

        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);
        let inner = tower::service_fn(move |_req: Request<Bytes>| {
            cc.fetch_add(1, Ordering::SeqCst);
            async move {
                Ok::<_, BoxError>(
                    http::Response::builder()
                        .status(401)
                        .body(Bytes::new())
                        .unwrap(),
                )
            }
        });

        let mut svc = ServiceBuilder::new()
            .layer(AuthLayer::new(BearerToken::new("stale-token")))
            .service(inner);

        let err = svc
            .ready()
            .await
            .unwrap()
            .call(Request::new(Bytes::new()))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("not refresh-capable"),
            "clear error, got: {err}"
        );
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "no silent re-serve of the rejected token"
        );
    }

    #[tokio::test]
    async fn test_auth_service_refresh_rides_through_transient_401() {
        use tower::{ServiceBuilder, ServiceExt};

        // First call with "first" 401s; the retry with "second" succeeds.
        let inner = tower::service_fn(move |req: Request<Bytes>| {
            let auth = req
                .headers()
                .get(AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_string();
            async move {
                if auth == "Bearer first" {
                    Ok::<_, BoxError>(
                        http::Response::builder()
                            .status(401)
                            .body(Bytes::new())
                            .unwrap(),
                    )
                } else {
                    Ok::<_, BoxError>(
                        http::Response::builder()
                            .status(200)
                            .body(Bytes::from(auth))
                            .unwrap(),
                    )
                }
            }
        });

        struct FlippingToken {
            calls: AtomicUsize,
        }
        impl TokenProvider for FlippingToken {
            fn token(&self) -> Pin<Box<dyn Future<Output = Result<String, BoxError>> + Send + '_>> {
                let n = self.calls.fetch_add(1, Ordering::SeqCst);
                let token = if n == 0 { "first" } else { "second" }.to_string();
                Box::pin(async move { Ok(token) })
            }
            fn is_refresh_capable(&self) -> bool {
                true
            }
        }

        let mut svc = ServiceBuilder::new()
            .layer(AuthLayer::new(FlippingToken {
                calls: AtomicUsize::new(0),
            }))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(Request::new(Bytes::new()))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.body().as_ref(), b"Bearer second");
    }

    #[test]
    fn test_bearer_token_new() {
        let provider = BearerToken::new("tok");
        assert_eq!(format!("{provider:?}"), "BearerToken { token: \"tok\" }");
    }

    #[test]
    fn test_token_provider_default_invalidate_is_noop() {
        struct Noop;
        impl TokenProvider for Noop {
            fn token(&self) -> Pin<Box<dyn Future<Output = Result<String, BoxError>> + Send + '_>> {
                Box::pin(async { Ok("noop".to_string()) })
            }
        }
        // Default invalidate should not panic.
        Noop.invalidate();
    }

    #[test]
    fn test_oauth2_with_scope_and_audience() {
        let provider = OAuth2ClientCredentials::new("url", "id", "secret")
            .with_scope("read")
            .with_audience("svc");
        assert!(provider.scope.as_deref() == Some("read"));
        assert!(provider.audience.as_deref() == Some("svc"));
    }

    #[tokio::test]
    async fn test_oauth2_fetch_token_from_mock_server() {
        use axum::{Json, Router, routing::post};
        use serde_json::json;

        let app = Router::new().route(
            "/token",
            post(|| async {
                Json(json!({
                    "access_token": "mock-token",
                    "expires_in": 120,
                }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let provider = OAuth2ClientCredentials::new(format!("http://{addr}/token"), "id", "secret");
        let token = provider.token().await.unwrap();
        assert_eq!(token, "mock-token");
        // Cached token is reused.
        assert_eq!(provider.token().await.unwrap(), "mock-token");
    }

    #[tokio::test]
    async fn test_arc_token_provider_delegates() {
        struct Counting(Arc<AtomicUsize>);
        impl TokenProvider for Counting {
            fn token(&self) -> Pin<Box<dyn Future<Output = Result<String, BoxError>> + Send + '_>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok("arc".to_string()) })
            }
        }

        let counter = Arc::new(AtomicUsize::new(0));
        let provider: Arc<dyn TokenProvider> = Arc::new(Counting(Arc::clone(&counter)));
        assert_eq!(provider.token().await.unwrap(), "arc");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_auth_service_does_not_retry_on_success() {
        use tower::{ServiceBuilder, ServiceExt};

        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = Arc::clone(&call_count);
        let inner = tower::service_fn(move |req: Request<Bytes>| {
            let count = cc.fetch_add(1, Ordering::SeqCst);
            async move {
                assert_eq!(
                    req.headers()
                        .get(AUTHORIZATION)
                        .and_then(|v| v.to_str().ok()),
                    Some("Bearer secret")
                );
                Ok::<_, BoxError>(
                    http::Response::builder()
                        .status(200)
                        .body(Bytes::from(format!("count={count}")))
                        .unwrap(),
                )
            }
        });

        let mut svc = ServiceBuilder::new()
            .layer(AuthLayer::new(BearerToken::new("secret")))
            .service(inner);

        let resp = svc
            .ready()
            .await
            .unwrap()
            .call(Request::new(Bytes::new()))
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_auth_layer_debug() {
        let layer = AuthLayer::new(BearerToken::new("x"));
        assert_eq!(format!("{layer:?}"), "AuthLayer");
    }

    #[test]
    fn test_with_auth_ignores_invalid_header_value() {
        let req = Request::new(Bytes::new());
        // A token containing a NUL byte cannot be turned into a HeaderValue.
        let req = with_auth(req, "bad\0token");
        assert!(req.headers().get(AUTHORIZATION).is_none());
    }

    #[test]
    fn test_remove_auth_clears_header() {
        let mut req = Request::new(Bytes::new());
        req.headers_mut()
            .insert(AUTHORIZATION, http::HeaderValue::from_static("Bearer x"));
        let req = remove_auth(req);
        assert!(req.headers().get(AUTHORIZATION).is_none());
    }
}

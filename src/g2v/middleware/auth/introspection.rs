//! OAuth 2.0 token introspection-backed session client with optional caching.
//!
//! Validates bearer tokens by calling an OAuth 2.0 token introspection endpoint.
//! The normalized response is shaped like a session so the rest of the auth
//! middleware can stay agnostic.
//!
//! A [`CachedIntrospectionSessionClient`] wrapper adds an in-memory TTL cache
//! that skips caching inactive tokens, limiting the revocation window.

use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::{Value, json};

use super::error::AuthError;
use super::session::SessionClient;

/// Default timeout for introspection requests when the configuration does
/// not set one. Auth validation sits on the request hot path, so a wedged
/// gateway must fail fast (and surface as the `timeout` failure class)
/// instead of hanging the request.
pub const DEFAULT_INTROSPECTION_TIMEOUT: Duration = Duration::from_secs(5);

/// Configuration for an introspection endpoint.
#[derive(Debug, Clone)]
pub struct IntrospectionConfig {
    /// Full URL of the introspection endpoint.
    pub url: String,
    /// OAuth2 client id used to authenticate the introspection request.
    pub client_id: String,
    /// OAuth2 client secret used to authenticate the introspection request.
    pub client_secret: String,
    /// Per-request timeout for introspection calls. Defaults to
    /// [`DEFAULT_INTROSPECTION_TIMEOUT`] when `None`.
    pub timeout: Option<Duration>,
}

impl IntrospectionConfig {
    /// Set the introspection request timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }
}

/// Session client that validates tokens via OAuth 2.0 introspection.
#[derive(Debug, Clone)]
pub struct IntrospectionSessionClient {
    client: reqwest::Client,
    config: IntrospectionConfig,
}

impl IntrospectionSessionClient {
    /// Create a client from configuration.
    pub fn new(config: IntrospectionConfig) -> Self {
        let timeout = match config.timeout {
            Some(timeout) => timeout,
            None => DEFAULT_INTROSPECTION_TIMEOUT,
        };
        let client = match reqwest::Client::builder().timeout(timeout).build() {
            Ok(client) => client,
            Err(e) => {
                // Only fails when the process TLS backend is broken; fall back
                // to the default client so auth still functions, and make the
                // misconfiguration visible.
                tracing::warn!("failed to build introspection HTTP client; using defaults: {e}");
                reqwest::Client::new()
            }
        };
        Self { client, config }
    }

    /// Convenience constructor from individual fields.
    pub fn new_with(
        url: impl Into<String>,
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Self {
        Self::new(IntrospectionConfig {
            url: url.into(),
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            timeout: None,
        })
    }
}

#[async_trait::async_trait]
impl SessionClient for IntrospectionSessionClient {
    async fn to_session(
        &self,
        _cookie: Option<&str>,
        token: Option<&str>,
    ) -> Result<Value, AuthError> {
        let token = token.ok_or_else(|| AuthError::InvalidSession("missing token".to_string()))?;

        let response = self
            .client
            .post(&self.config.url)
            .basic_auth(&self.config.client_id, Some(&self.config.client_secret))
            .form(&[("token", token)])
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    AuthError::SessionTimeout(format!("introspection request timed out: {e}"))
                } else {
                    AuthError::SessionTransport(format!("introspection request failed: {e}"))
                }
            })?;

        if !response.status().is_success() {
            return Err(AuthError::SessionGateway(format!(
                "introspection returned {}",
                response.status()
            )));
        }

        let body: Value = response.json().await.map_err(|e| {
            AuthError::SessionGateway(format!("invalid introspection response: {e}"))
        })?;

        let active = body
            .get("active")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if !active {
            return Err(AuthError::InactiveSession);
        }

        // The sso-gateway injects `tenant_id` into the introspection response
        // when a client/identity mapping exists; forward it so the middleware
        // can resolve the tenant without a second lookup.
        let tenant_id = body
            .get("tenant_id")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let subject = body
            .get("sub")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if subject.is_empty() {
            return Err(AuthError::SessionGateway(
                "introspection response missing sub".to_string(),
            ));
        }

        // Normalize to the shape the middleware extracts: `tenant_id` (when
        // the gateway injected one) plus `identity.id` as the subject.
        Ok(json!({
            "tenant_id": tenant_id,
            "sub": subject,
            "identity": { "id": subject },
            "scope": body.get("scope").cloned().unwrap_or_else(|| json!("")),
        }))
    }
}

/// Cached entry for an introspection result.
#[derive(Clone, Debug)]
struct CachedIntrospection {
    result: Value,
    inserted_at: Instant,
}

/// Session client that wraps an [`IntrospectionSessionClient`] with an
/// in-memory TTL cache.
///
/// Inactive tokens are never cached, limiting the revocation window.
#[derive(Clone, Debug)]
pub struct CachedIntrospectionSessionClient {
    inner: IntrospectionSessionClient,
    cache: Arc<DashMap<String, CachedIntrospection>>,
    ttl: Duration,
}

impl CachedIntrospectionSessionClient {
    /// Create a cached wrapper around an existing introspection client.
    pub fn new(inner: IntrospectionSessionClient, ttl: Duration) -> Self {
        Self {
            inner,
            cache: Arc::new(DashMap::new()),
            ttl,
        }
    }

    /// Create a cached client from configuration.
    pub fn from_config(config: IntrospectionConfig, ttl: Duration) -> Self {
        Self::new(IntrospectionSessionClient::new(config), ttl)
    }

    /// Purge all cached entries.
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

#[async_trait::async_trait]
impl SessionClient for CachedIntrospectionSessionClient {
    async fn to_session(
        &self,
        _cookie: Option<&str>,
        token: Option<&str>,
    ) -> Result<Value, AuthError> {
        let token = token.ok_or_else(|| AuthError::InvalidSession("missing token".to_string()))?;
        let token_hash = super::hash_token(token);

        // Check cache.
        if let Some(entry) = self.cache.get(&token_hash)
            && entry.inserted_at.elapsed() < self.ttl
        {
            tracing::debug!("token introspection cache hit");
            return Ok(entry.result.clone());
        }

        tracing::debug!("token introspection cache miss");
        let result = self.inner.to_session(None, Some(token)).await?;

        // Do not cache inactive tokens; this limits the window after revocation.
        let active = result
            .get("identity")
            .and_then(|i| i.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| !s.is_empty())
            .unwrap_or(false);

        if active {
            self.cache.insert(
                token_hash,
                CachedIntrospection {
                    result: result.clone(),
                    inserted_at: Instant::now(),
                },
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cached_client_clears_cache() {
        let client = CachedIntrospectionSessionClient::from_config(
            IntrospectionConfig {
                url: "http://localhost".into(),
                client_id: "id".into(),
                client_secret: "secret".into(),
                timeout: None,
            },
            Duration::from_secs(60),
        );
        client.clear_cache();
        assert!(client.cache.is_empty());
    }

    #[test]
    fn introspection_constructors_apply_timeout() {
        let config = IntrospectionConfig {
            url: "http://localhost/introspect".into(),
            client_id: "id".into(),
            client_secret: "secret".into(),
            timeout: None,
        }
        .with_timeout(Duration::from_millis(250));
        assert_eq!(config.timeout, Some(Duration::from_millis(250)));

        let _client = IntrospectionSessionClient::new(config.clone());
        let _client =
            IntrospectionSessionClient::new_with("http://localhost/introspect", "id", "secret");
        let inner = IntrospectionSessionClient::new(config);
        let cached = CachedIntrospectionSessionClient::new(inner, Duration::from_secs(1));
        assert_eq!(cached.ttl, Duration::from_secs(1));
    }

    #[tokio::test]
    async fn introspection_clients_require_token() {
        let client = IntrospectionSessionClient::new_with("http://localhost", "id", "secret");
        let error = client.to_session(None, None).await.unwrap_err();
        assert!(matches!(error, AuthError::InvalidSession(_)));

        let cached = CachedIntrospectionSessionClient::new(client, Duration::from_secs(1));
        let error = cached.to_session(None, None).await.unwrap_err();
        assert!(matches!(error, AuthError::InvalidSession(_)));
    }
}

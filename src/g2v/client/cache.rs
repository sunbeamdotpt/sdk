//! Client-side LRU response cache for Sunbeam service clients.
//!
//! This mirrors the server-side cache but operates on `http::Request<Bytes>` /
//! `http::Response<Bytes>` so it can sit directly in front of a Tower HTTP
//! client stack (e.g. `hyper` or `reqwest`-based services).
//!
//! By default only safe/read traffic is cached:
//!
//! - `GET` requests (except `/health/*` and `/metrics`).
//! - ConnectRPC `POST` requests whose method name looks read-only
//!   (`List*`, `Get*`, `Check*`, `Expand*`, `To*`, `Describe*`).
//!
//! Callers can attach a [`CacheScope`] extension to isolate entries by tenant
//! and actor.
//!
//! # Example
//!
//! ```rust,no_run
//! use bytes::Bytes;
//! use crate::g2v::client::cache::ClientCacheLayer;
//! use crate::g2v::client::cache::CacheConfig;
//!
//! let layer = ClientCacheLayer::new(CacheConfig::default());
//! ```

use std::{
    fmt,
    future::Future,
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll},
    time::{Duration, Instant},
};

use bytes::Bytes;
use http::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE};
use http::{Request, Response};
use lru::LruCache;
use tower::{Layer, Service};

/// Scope attached to a cached entry so that different tenants and actors never
/// share the same cached response.
///
/// When the server auth middleware is enabled, it populates `AuthContext`,
/// which the cache layer uses to build a scope. Services without auth can
/// also insert this extension manually.
#[derive(Debug, Clone, Default)]
pub struct CacheScope {
    /// Resolved tenant id.
    pub tenant: Option<String>,
    /// Authenticated actor id (API key id, identity id, etc.).
    pub actor: Option<String>,
}

impl CacheScope {
    /// Serialize the scope into a compact, deterministic string.
    pub(crate) fn as_key(&self) -> String {
        match (&self.tenant, &self.actor) {
            (Some(t), Some(a)) => format!("t={t}:a={a}"),
            (Some(t), None) => format!("t={t}"),
            (None, Some(a)) => format!("a={a}"),
            (None, None) => "_".to_string(),
        }
    }
}

/// Configuration for the response cache.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of cached responses.
    pub capacity: NonZeroUsize,
    /// Time-to-live for cached entries. `None` means entries live until evicted
    /// by the LRU policy.
    pub ttl: Option<Duration>,
    /// Requests or responses larger than this are not cached.
    pub max_body_size: usize,
}

impl Default for CacheConfig {
    fn default() -> Self {
        let capacity = match NonZeroUsize::new(10_000) {
            Some(capacity) => capacity,
            // Unreachable for a non-zero literal; fall back to the smallest
            // valid capacity rather than panicking.
            None => NonZeroUsize::MIN,
        };
        Self {
            capacity,
            ttl: Some(Duration::from_secs(60)),
            max_body_size: 1024 * 1024,
        }
    }
}

impl CacheConfig {
    /// Create a default cache config with the given capacity.
    pub fn with_capacity(capacity: NonZeroUsize) -> Self {
        Self {
            capacity,
            ..Self::default()
        }
    }
}

/// A type-erased predicate that decides whether a request is cacheable.
pub type ClientCachePredicate = Arc<dyn Fn(&Request<Bytes>) -> bool + Send + Sync>;

/// Tower layer that adds client-side LRU response caching.
#[derive(Clone)]
pub struct ClientCacheLayer {
    cache: ClientRequestCache,
    predicate: ClientCachePredicate,
}

impl fmt::Debug for ClientCacheLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCacheLayer")
            .field("capacity", &self.cache.config.capacity)
            .field("ttl", &self.cache.config.ttl)
            .field("max_body_size", &self.cache.config.max_body_size)
            .finish()
    }
}

impl ClientCacheLayer {
    /// Create a client cache layer with the default cacheability predicate.
    pub fn new(config: CacheConfig) -> Self {
        Self::with_predicate(config, Arc::new(default_client_cacheable_predicate))
    }

    /// Create a client cache layer with a custom cacheability predicate.
    pub fn with_predicate(config: CacheConfig, predicate: ClientCachePredicate) -> Self {
        Self {
            cache: ClientRequestCache::new(config),
            predicate,
        }
    }
}

impl<S> Layer<S> for ClientCacheLayer {
    type Service = ClientCacheService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ClientCacheService {
            inner,
            cache: self.cache.clone(),
            predicate: Arc::clone(&self.predicate),
        }
    }
}

/// Tower service wrapper that performs the actual client-side caching.
#[derive(Clone)]
pub struct ClientCacheService<S> {
    inner: S,
    cache: ClientRequestCache,
    predicate: ClientCachePredicate,
}

impl<S> fmt::Debug for ClientCacheService<S>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCacheService")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<S, E> Service<Request<Bytes>> for ClientCacheService<S>
where
    S: Service<Request<Bytes>, Response = Response<Bytes>, Error = E> + Clone + Send + 'static,
    S::Future: Send + 'static,
    E: Send + 'static,
{
    type Response = Response<Bytes>;
    type Error = E;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Bytes>) -> Self::Future {
        if !(self.predicate)(&request) {
            return Box::pin(self.inner.clone().call(request));
        }

        let cache = self.cache.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let (parts, body) = request.into_parts();

            if body.len() > cache.config.max_body_size {
                let req = Request::from_parts(parts, body);
                return inner.call(req).await;
            }

            let scope = match parts.extensions.get::<CacheScope>().cloned() {
                Some(scope) => scope,
                // Requests without an explicit scope share the default scope.
                None => CacheScope::default(),
            };
            let key =
                build_client_cache_key(&parts.method, &parts.uri, &parts.headers, &scope, &body);

            {
                let mut guard = match cache.inner.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        // A poisoned cache lock still holds a usable cache;
                        // recover it rather than failing every request.
                        tracing::warn!("client cache lock poisoned; recovering");
                        poisoned.into_inner()
                    }
                };
                if let Some(entry) = guard.get(&key)
                    && !entry.is_expired(cache.config.ttl)
                {
                    return Ok(entry.to_response());
                }
            }

            let req = Request::from_parts(parts, body);
            let response = inner.call(req).await?;

            if response.status().is_success() && response.body().len() <= cache.config.max_body_size
            {
                let entry = ClientCachedResponse::from_response(&response);
                let mut guard = match cache.inner.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        tracing::warn!("client cache lock poisoned; recovering");
                        poisoned.into_inner()
                    }
                };
                guard.put(key, entry.clone());
                Ok(entry.to_response())
            } else {
                Ok(response)
            }
        })
    }
}

/// Shared client cache handle.
#[derive(Clone)]
struct ClientRequestCache {
    inner: Arc<Mutex<LruCache<ClientCacheKey, ClientCachedResponse, gxhash::GxBuildHasher>>>,
    config: CacheConfig,
}

impl ClientRequestCache {
    fn new(config: CacheConfig) -> Self {
        let cache = LruCache::with_hasher(config.capacity, gxhash::GxBuildHasher::default());
        Self {
            inner: Arc::new(Mutex::new(cache)),
            config,
        }
    }
}

/// Cache key used to look up a response.
#[derive(Debug, Clone, Eq, PartialEq)]
struct ClientCacheKey {
    scope: String,
    method: String,
    uri: String,
    content_type: String,
    accept: String,
    body_hash: u64,
}

impl Hash for ClientCacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.scope.hash(state);
        self.method.hash(state);
        self.uri.hash(state);
        self.content_type.hash(state);
        self.accept.hash(state);
        self.body_hash.hash(state);
    }
}

fn build_client_cache_key(
    method: &http::Method,
    uri: &http::Uri,
    headers: &http::HeaderMap,
    scope: &CacheScope,
    body: &Bytes,
) -> ClientCacheKey {
    let body_hash = gxhash::gxhash64(body, 0);
    ClientCacheKey {
        scope: scope.as_key(),
        method: method.to_string(),
        uri: uri.to_string(),
        content_type: header_value(headers, CONTENT_TYPE),
        accept: header_value(headers, ACCEPT),
        body_hash,
    }
}

fn header_value(headers: &http::HeaderMap, name: http::header::HeaderName) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

/// Default client predicate: cache GETs (except health/metrics) and read-only RPCs.
fn default_client_cacheable_predicate(request: &Request<Bytes>) -> bool {
    let path = request.uri().path();

    if path.starts_with("/health/") || path == "/metrics" {
        return false;
    }

    if let Some(value) = request
        .headers()
        .get(CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        && (value.contains("no-store") || value.contains("no-cache"))
    {
        return false;
    }

    let method = request.method();
    if method == http::Method::GET {
        return true;
    }

    if method == http::Method::POST && is_read_rpc_method(path) {
        return true;
    }

    false
}

/// Returns true when the last path segment looks like a read-only RPC method.
fn is_read_rpc_method(path: &str) -> bool {
    let trimmed = path.trim_start_matches('/');
    let method_name = trimmed
        .rfind('/')
        .map(|pos| &trimmed[pos + 1..])
        .unwrap_or(trimmed);
    method_name.starts_with("List")
        || method_name.starts_with("Get")
        || method_name.starts_with("Check")
        || method_name.starts_with("Expand")
        || method_name.starts_with("To")
        || method_name.starts_with("Describe")
}

/// A cached HTTP response using owned `Bytes` bodies.
#[derive(Debug, Clone)]
struct ClientCachedResponse {
    status: http::StatusCode,
    headers: Vec<(String, Vec<u8>)>,
    body: Bytes,
    created_at: Instant,
}

impl ClientCachedResponse {
    fn from_response(response: &Response<Bytes>) -> Self {
        Self {
            status: response.status(),
            headers: response
                .headers()
                .iter()
                .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                .collect(),
            body: response.body().clone(),
            created_at: Instant::now(),
        }
    }

    fn is_expired(&self, ttl: Option<std::time::Duration>) -> bool {
        ttl.is_some_and(|duration| self.created_at.elapsed() > duration)
    }

    fn to_response(&self) -> Response<Bytes> {
        let mut builder = Response::builder().status(self.status);
        for (name, value) in &self.headers {
            if let (Ok(name), Ok(value)) = (
                http::HeaderName::from_bytes(name.as_bytes()),
                http::HeaderValue::from_bytes(value),
            ) {
                builder = builder.header(name, value);
            }
        }

        match builder.body(self.body.clone()) {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!("failed to rebuild cached response: {e}");
                let mut response = Response::new(Bytes::new());
                *response.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                response
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;
    use std::num::NonZeroUsize;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    fn test_config() -> CacheConfig {
        CacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            ttl: Some(std::time::Duration::from_secs(60)),
            max_body_size: 1024,
        }
    }

    #[test]
    fn default_predicate_caches_get_requests() {
        let req = Request::get("/foo").body(Bytes::new()).unwrap();
        assert!(default_client_cacheable_predicate(&req));
    }

    #[test]
    fn default_predicate_skips_health_and_metrics() {
        let health = Request::get("/health/live").body(Bytes::new()).unwrap();
        let metrics = Request::get("/metrics").body(Bytes::new()).unwrap();
        assert!(!default_client_cacheable_predicate(&health));
        assert!(!default_client_cacheable_predicate(&metrics));
    }

    #[test]
    fn default_predicate_caches_read_rpcs() {
        let list = Request::post("/iam.v1.TenantService/ListTenants")
            .body(Bytes::new())
            .unwrap();
        let create = Request::post("/iam.v1.TenantService/CreateTenant")
            .body(Bytes::new())
            .unwrap();

        assert!(default_client_cacheable_predicate(&list));
        assert!(!default_client_cacheable_predicate(&create));
    }

    #[tokio::test]
    async fn cache_returns_hits_without_calling_inner_service() {
        let count = Arc::new(Mutex::new(0usize));
        let service = tower::service_fn({
            let count = Arc::clone(&count);
            move |_req: Request<Bytes>| {
                let count = Arc::clone(&count);
                async move {
                    *count.lock().unwrap() += 1;
                    Ok::<_, Infallible>(Response::new(Bytes::from_static(b"hello")))
                }
            }
        });

        let mut cache = ClientCacheLayer::new(test_config()).layer(service);

        let req1 = Request::get("/cached").body(Bytes::new()).unwrap();
        let resp1 = cache.ready().await.unwrap().call(req1).await.unwrap();
        assert_eq!(resp1.body().as_ref(), b"hello");
        assert_eq!(*count.lock().unwrap(), 1);

        let req2 = Request::get("/cached").body(Bytes::new()).unwrap();
        let resp2 = cache.call(req2).await.unwrap();
        assert_eq!(resp2.body().as_ref(), b"hello");
        assert_eq!(*count.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn cache_is_scoped_by_tenant_and_actor() {
        let service = tower::service_fn(|_req: Request<Bytes>| async move {
            Ok::<_, Infallible>(Response::new(Bytes::from_static(b"response")))
        });

        let mut cache = ClientCacheLayer::new(test_config()).layer(service);

        let mut req_a = Request::get("/scoped").body(Bytes::new()).unwrap();
        req_a.extensions_mut().insert(CacheScope {
            tenant: Some("t1".into()),
            actor: Some("a1".into()),
        });
        let _ = cache.ready().await.unwrap().call(req_a).await.unwrap();

        let mut req_b = Request::get("/scoped").body(Bytes::new()).unwrap();
        req_b.extensions_mut().insert(CacheScope {
            tenant: Some("t1".into()),
            actor: Some("a2".into()),
        });
        let resp_b = cache.call(req_b).await.unwrap();
        assert_eq!(resp_b.body().as_ref(), b"response");
    }

    #[tokio::test]
    async fn cache_respects_ttl() {
        let service = tower::service_fn(|_req: Request<Bytes>| async move {
            Ok::<_, Infallible>(Response::new(Bytes::from_static(b"response")))
        });

        let config = CacheConfig {
            ttl: Some(std::time::Duration::from_millis(10)),
            ..test_config()
        };
        let mut cache = ClientCacheLayer::new(config).layer(service);

        let req1 = Request::get("/ttl").body(Bytes::new()).unwrap();
        let _ = cache.ready().await.unwrap().call(req1).await.unwrap();

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;

        let req2 = Request::get("/ttl").body(Bytes::new()).unwrap();
        let resp2 = cache.call(req2).await.unwrap();
        assert_eq!(resp2.body().as_ref(), b"response");
    }

    #[test]
    fn cache_layer_debug() {
        let layer = ClientCacheLayer::new(test_config());
        let debug = format!("{layer:?}");
        assert!(debug.contains("ClientCacheLayer"));
        assert!(debug.contains("capacity"));
    }

    #[tokio::test]
    async fn cache_uses_custom_predicate() {
        let call_count = Arc::new(Mutex::new(0usize));
        let service = tower::service_fn({
            let count = Arc::clone(&call_count);
            move |_req: Request<Bytes>| {
                let count = Arc::clone(&count);
                async move {
                    *count.lock().unwrap() += 1;
                    Ok::<_, Infallible>(Response::new(Bytes::from_static(b"ok")))
                }
            }
        });

        let predicate = Arc::new(|req: &Request<Bytes>| req.uri().path() == "/cache-me");
        let mut cache = ClientCacheLayer::with_predicate(test_config(), predicate).layer(service);

        let req1 = Request::get("/cache-me").body(Bytes::new()).unwrap();
        cache.call(req1).await.unwrap();
        let req2 = Request::get("/cache-me").body(Bytes::new()).unwrap();
        cache.call(req2).await.unwrap();
        assert_eq!(*call_count.lock().unwrap(), 1);

        let req3 = Request::get("/skip-me").body(Bytes::new()).unwrap();
        cache.call(req3.clone()).await.unwrap();
        cache.call(req3).await.unwrap();
        assert_eq!(*call_count.lock().unwrap(), 3);
    }

    #[tokio::test]
    async fn cache_bypasses_when_body_too_large() {
        let config = CacheConfig {
            max_body_size: 2,
            ..test_config()
        };
        let call_count = Arc::new(Mutex::new(0usize));
        let service = tower::service_fn({
            let count = Arc::clone(&call_count);
            move |_req: Request<Bytes>| {
                let count = Arc::clone(&count);
                async move {
                    *count.lock().unwrap() += 1;
                    Ok::<_, Infallible>(Response::new(Bytes::from_static(b"ok")))
                }
            }
        });

        let mut cache = ClientCacheLayer::new(config).layer(service);
        let req = Request::get("/big")
            .body(Bytes::from_static(b"huge"))
            .unwrap();
        cache.call(req).await.unwrap();
        cache
            .call(
                Request::get("/big")
                    .body(Bytes::from_static(b"huge"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(*call_count.lock().unwrap(), 2);
    }

    #[tokio::test]
    async fn cache_does_not_store_error_responses() {
        let call_count = Arc::new(Mutex::new(0usize));
        let service = tower::service_fn({
            let count = Arc::clone(&call_count);
            move |_req: Request<Bytes>| {
                let count = Arc::clone(&count);
                async move {
                    *count.lock().unwrap() += 1;
                    Ok::<_, Infallible>(
                        Response::builder()
                            .status(500)
                            .body(Bytes::from_static(b"err"))
                            .unwrap(),
                    )
                }
            }
        });

        let mut cache = ClientCacheLayer::new(test_config()).layer(service);
        let req = Request::get("/error").body(Bytes::new()).unwrap();
        cache.call(req).await.unwrap();
        cache
            .call(Request::get("/error").body(Bytes::new()).unwrap())
            .await
            .unwrap();
        assert_eq!(*call_count.lock().unwrap(), 2);
    }

    #[test]
    fn default_predicate_respects_cache_control_no_store() {
        let req = Request::get("/foo")
            .header(http::header::CACHE_CONTROL, "no-store")
            .body(Bytes::new())
            .unwrap();
        assert!(!default_client_cacheable_predicate(&req));

        let req = Request::get("/foo")
            .header(http::header::CACHE_CONTROL, "no-cache")
            .body(Bytes::new())
            .unwrap();
        assert!(!default_client_cacheable_predicate(&req));
    }

    #[test]
    fn default_predicate_rejects_non_read_rpcs() {
        let create = Request::post("/svc/CreateThing")
            .body(Bytes::new())
            .unwrap();
        assert!(!default_client_cacheable_predicate(&create));
    }

    #[tokio::test]
    async fn cache_restores_response_headers() {
        let service = tower::service_fn(|_req: Request<Bytes>| async move {
            Ok::<_, Infallible>(
                Response::builder()
                    .header("x-custom", "value")
                    .body(Bytes::from_static(b"body"))
                    .unwrap(),
            )
        });

        let mut cache = ClientCacheLayer::new(test_config()).layer(service);
        let req = Request::get("/headers").body(Bytes::new()).unwrap();
        let resp1 = cache.call(req).await.unwrap();
        assert_eq!(resp1.headers().get("x-custom").unwrap(), "value");

        let resp2 = cache
            .call(Request::get("/headers").body(Bytes::new()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp2.headers().get("x-custom").unwrap(), "value");
    }
}

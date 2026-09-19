//! Fast, opt-in LRU response cache for Sunbeam services.
//!
//! The cache sits above the ConnectRPC router (and any extra Axum routes). It
//! stores serialized HTTP responses keyed by a normalized request fingerprint
//! that includes the caller scope (tenant + actor), request line, content
//! negotiation headers, and a hash of the request body.
//!
//! By default only safe/read traffic is cached:
//!
//! - `GET` requests (except `/health/*` and `/metrics`).
//! - ConnectRPC `POST` requests whose method name looks read-only
//!   (`List*`, `Get*`, `Check*`, `Expand*`, `To*`, `Describe*`).
//!
//! Callers can override the cacheability predicate, TTL, capacity, and maximum
//! body size through [`CacheConfig`](crate::g2v::middleware::cache::CacheConfig).
//!
//! # Example
//!
//! ```rust,no_run
//! use std::num::NonZeroUsize;
//! use std::time::Duration;
//! use crate::g2v::middleware::cache::{CacheConfig, CacheLayer};
//!
//! let layer = CacheLayer::new(CacheConfig {
//!     capacity: NonZeroUsize::new(10_000).unwrap(),
//!     ttl: Some(Duration::from_secs(60)),
//!     max_body_size: 1024 * 1024,
//! });
//! ```

use std::{
    fmt,
    future::Future,
    hash::{Hash, Hasher},
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context as TaskContext, Poll},
    time::{Duration, Instant},
};

use axum::body::{Body, Bytes};
use axum::extract::Request;
use axum::response::Response;
use http::header::{ACCEPT, CACHE_CONTROL, CONTENT_TYPE};
use http_body_util::BodyExt;
use lru::LruCache;
use tower::{Layer, Service};

#[cfg(feature = "g2v-server")]
use super::auth::AuthContext;

pub use crate::g2v::client::cache::{CacheConfig, CacheScope};

/// A type-erased predicate that decides whether a request is cacheable.
pub type CachePredicate = Arc<dyn Fn(&Request<Body>) -> bool + Send + Sync>;

/// Tower layer that adds LRU response caching.
#[derive(Clone)]
pub struct CacheLayer {
    cache: RequestCache,
    predicate: CachePredicate,
}

impl fmt::Debug for CacheLayer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheLayer")
            .field("capacity", &self.cache.config.capacity)
            .field("ttl", &self.cache.config.ttl)
            .field("max_body_size", &self.cache.config.max_body_size)
            .finish()
    }
}

impl CacheLayer {
    /// Create a cache layer with the default cacheability predicate.
    pub fn new(config: CacheConfig) -> Self {
        Self::with_predicate(config, Arc::new(default_cacheable_predicate))
    }

    /// Create a cache layer with a custom cacheability predicate.
    pub fn with_predicate(config: CacheConfig, predicate: CachePredicate) -> Self {
        Self {
            cache: RequestCache::new(config),
            predicate,
        }
    }
}

impl<S> Layer<S> for CacheLayer {
    type Service = CacheService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        CacheService {
            inner,
            cache: self.cache.clone(),
            predicate: Arc::clone(&self.predicate),
        }
    }
}

/// Tower service wrapper that performs the actual caching.
#[derive(Clone)]
pub struct CacheService<S> {
    inner: S,
    cache: RequestCache,
    predicate: CachePredicate,
}

impl<S> fmt::Debug for CacheService<S>
where
    S: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheService")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<S> Service<Request<Body>> for CacheService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<Body>) -> Self::Future {
        if !(self.predicate)(&request) {
            return Box::pin(self.inner.clone().call(request));
        }

        let cache = self.cache.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            let (parts, body) = request.into_parts();

            let bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(_) => {
                    // Body collection failed; pass an empty body through rather
                    // than caching. This is a degenerate case for in-memory
                    // axum bodies.
                    let req = Request::from_parts(parts, Body::empty());
                    return inner.call(req).await;
                }
            };

            if bytes.len() > cache.config.max_body_size {
                let req = Request::from_parts(parts, Body::from(bytes));
                return inner.call(req).await;
            }

            #[cfg(feature = "g2v-server")]
            let scope = parts
                .extensions
                .get::<AuthContext>()
                .map(|ctx| CacheScope {
                    tenant: ctx.tenant_id.clone(),
                    actor: ctx.subject.clone(),
                })
                // Unauthenticated requests share the default (public) scope.
                .unwrap_or_else(CacheScope::default);
            #[cfg(not(feature = "g2v-server"))]
            let scope = CacheScope::default();

            let key = build_cache_key(&parts.method, &parts.uri, &parts.headers, &scope, &bytes);

            {
                let mut guard = match cache.inner.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        // A poisoned cache lock still holds a usable cache;
                        // recover it rather than failing every request.
                        tracing::warn!("cache lock poisoned; recovering");
                        poisoned.into_inner()
                    }
                };
                if let Some(entry) = guard.get(&key)
                    && !entry.is_expired(cache.config.ttl)
                {
                    return Ok(entry.to_response());
                }
            }

            let req = Request::from_parts(parts, Body::from(bytes));
            let response = inner.call(req).await?;

            let (parts, body) = response.into_parts();
            let response_bytes = match body.collect().await {
                Ok(collected) => collected.to_bytes(),
                Err(e) => {
                    // Failed to read the response body; return the response
                    // without caching. Reconstruct with an empty body because
                    // the stream is gone.
                    tracing::warn!("failed to read response body for caching: {e}");
                    let mut response = Response::new(Body::empty());
                    *response.status_mut() = parts.status;
                    return Ok(response);
                }
            };

            if parts.status.is_success() && response_bytes.len() <= cache.config.max_body_size {
                let entry = CachedResponse {
                    status: parts.status,
                    headers: parts
                        .headers
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                        .collect(),
                    body: response_bytes.clone(),
                    created_at: Instant::now(),
                };
                let mut guard = match cache.inner.lock() {
                    Ok(guard) => guard,
                    Err(poisoned) => {
                        tracing::warn!("cache lock poisoned; recovering");
                        poisoned.into_inner()
                    }
                };
                guard.put(key, entry);
            }

            match Response::builder()
                .status(parts.status)
                .body(Body::from(response_bytes))
            {
                Ok(response) => Ok(response),
                Err(e) => {
                    tracing::warn!("failed to rebuild response after caching: {e}");
                    let mut response = Response::new(Body::empty());
                    *response.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                    Ok(response)
                }
            }
        })
    }
}

/// Shared cache handle.
#[derive(Clone)]
struct RequestCache {
    inner: Arc<Mutex<LruCache<CacheKey, CachedResponse, gxhash::GxBuildHasher>>>,
    config: CacheConfig,
}

impl RequestCache {
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
struct CacheKey {
    scope: String,
    method: String,
    uri: String,
    content_type: String,
    accept: String,
    body_hash: u64,
}

impl Hash for CacheKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash every field. The LRU cache internally uses GxHash, but the
        // Hash trait implementation is hasher-agnostic.
        self.scope.hash(state);
        self.method.hash(state);
        self.uri.hash(state);
        self.content_type.hash(state);
        self.accept.hash(state);
        self.body_hash.hash(state);
    }
}

fn build_cache_key(
    method: &http::Method,
    uri: &http::Uri,
    headers: &http::HeaderMap,
    scope: &CacheScope,
    body: &Bytes,
) -> CacheKey {
    let body_hash = gxhash::gxhash64(body, 0);
    CacheKey {
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

/// Default predicate: cache GETs (except health/metrics) and read-only RPCs.
fn default_cacheable_predicate(request: &Request<Body>) -> bool {
    let path = request.uri().path();

    if path.starts_with("/health/") || path == "/metrics" {
        return false;
    }

    // Honor an explicit no-store directive from the client.
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

/// A cached HTTP response.
#[derive(Debug, Clone)]
struct CachedResponse {
    status: http::StatusCode,
    headers: Vec<(String, Vec<u8>)>,
    body: Bytes,
    created_at: Instant,
}

impl CachedResponse {
    fn is_expired(&self, ttl: Option<Duration>) -> bool {
        ttl.is_some_and(|duration| self.created_at.elapsed() > duration)
    }

    fn to_response(&self) -> Response {
        let mut headers = http::HeaderMap::new();
        for (name, value) in &self.headers {
            if let (Ok(name), Ok(value)) = (
                http::HeaderName::from_bytes(name.as_bytes()),
                http::HeaderValue::from_bytes(value),
            ) {
                let _ = headers.insert(name, value);
            }
        }

        match Response::builder()
            .status(self.status)
            .body(Body::from(self.body.clone()))
        {
            Ok(mut response) => {
                *response.headers_mut() = headers;
                response
            }
            Err(e) => {
                tracing::warn!("failed to rebuild cached response: {e}");
                let mut response = Response::new(Body::empty());
                *response.status_mut() = http::StatusCode::INTERNAL_SERVER_ERROR;
                response
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use super::*;
    use axum::body::Body;
    use axum::response::IntoResponse;
    use http::Request;
    use std::convert::Infallible;
    use std::sync::{Arc, Mutex};
    use tower::ServiceExt;

    fn test_config() -> CacheConfig {
        CacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            ttl: Some(Duration::from_secs(60)),
            max_body_size: 1024,
        }
    }

    #[test]
    fn default_predicate_caches_get_requests() {
        let req = Request::get("/foo").body(Body::empty()).unwrap();
        assert!(default_cacheable_predicate(&req));
    }

    #[test]
    fn default_predicate_skips_health_and_metrics() {
        let health = Request::get("/health/live").body(Body::empty()).unwrap();
        let metrics = Request::get("/metrics").body(Body::empty()).unwrap();
        assert!(!default_cacheable_predicate(&health));
        assert!(!default_cacheable_predicate(&metrics));
    }

    #[test]
    fn default_predicate_caches_read_rpcs() {
        let list = Request::post("/iam.v1.TenantService/ListTenants")
            .body(Body::empty())
            .unwrap();
        let get = Request::post("/iam.v1.TenantService/GetTenant")
            .body(Body::empty())
            .unwrap();
        let create = Request::post("/iam.v1.TenantService/CreateTenant")
            .body(Body::empty())
            .unwrap();

        assert!(default_cacheable_predicate(&list));
        assert!(default_cacheable_predicate(&get));
        assert!(!default_cacheable_predicate(&create));
    }

    #[test]
    fn default_predicate_respects_cache_control_no_store() {
        let req = Request::get("/foo")
            .header(CACHE_CONTROL, "no-store")
            .body(Body::empty())
            .unwrap();
        assert!(!default_cacheable_predicate(&req));
    }

    #[tokio::test]
    async fn cache_returns_hits_without_calling_inner_service() {
        let count = Arc::new(Mutex::new(0usize));
        let service = tower::service_fn({
            let count = Arc::clone(&count);
            move |_req: Request<Body>| {
                let count = Arc::clone(&count);
                async move {
                    *count.lock().unwrap() += 1;
                    Ok::<_, Infallible>("hello".into_response())
                }
            }
        });

        let mut cache = CacheLayer::new(test_config()).layer(service);

        let req1 = Request::get("/cached").body(Body::empty()).unwrap();
        let resp1 = cache.ready().await.unwrap().call(req1).await.unwrap();
        let body1 = resp1.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body1[..], b"hello");
        assert_eq!(*count.lock().unwrap(), 1);

        let req2 = Request::get("/cached").body(Body::empty()).unwrap();
        let resp2 = cache.call(req2).await.unwrap();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body2[..], b"hello");
        assert_eq!(*count.lock().unwrap(), 1); // inner service not called again
    }

    #[tokio::test]
    #[cfg(feature = "g2v-server")]
    async fn cache_is_scoped_by_tenant_and_actor() {
        let service = tower::service_fn(|_req: Request<Body>| async move {
            Ok::<_, Infallible>("response".into_response())
        });

        let mut cache = CacheLayer::new(test_config()).layer(service);

        let mut req_a = Request::get("/scoped").body(Body::empty()).unwrap();
        req_a
            .extensions_mut()
            .insert(AuthContext::authenticated("t1", "a1"));
        let _ = cache.ready().await.unwrap().call(req_a).await.unwrap();

        let mut req_b = Request::get("/scoped").body(Body::empty()).unwrap();
        req_b
            .extensions_mut()
            .insert(AuthContext::authenticated("t1", "a2"));
        let resp_b = cache.call(req_b).await.unwrap();
        let body_b = resp_b.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body_b[..], b"response");
    }

    #[tokio::test]
    async fn cache_respects_ttl() {
        let service = tower::service_fn(|_req: Request<Body>| async move {
            Ok::<_, Infallible>("response".into_response())
        });

        let config = CacheConfig {
            ttl: Some(Duration::from_millis(10)),
            ..test_config()
        };
        let mut cache = CacheLayer::new(config).layer(service);

        let req1 = Request::get("/ttl").body(Body::empty()).unwrap();
        let _ = cache.ready().await.unwrap().call(req1).await.unwrap();

        tokio::time::sleep(Duration::from_millis(20)).await;

        let req2 = Request::get("/ttl").body(Body::empty()).unwrap();
        let resp2 = cache.call(req2).await.unwrap();
        let body2 = resp2.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body2[..], b"response");
    }

    #[test]
    fn cache_scope_keys_cover_all_shapes() {
        assert_eq!(CacheScope::default().as_key(), "_");
        assert_eq!(
            CacheScope {
                tenant: Some("tenant".into()),
                actor: None,
            }
            .as_key(),
            "t=tenant"
        );
        assert_eq!(
            CacheScope {
                tenant: None,
                actor: Some("actor".into()),
            }
            .as_key(),
            "a=actor"
        );
        assert_eq!(
            CacheScope {
                tenant: Some("tenant".into()),
                actor: Some("actor".into()),
            }
            .as_key(),
            "t=tenant:a=actor"
        );
    }

    #[test]
    fn cache_config_defaults_and_debug() {
        let config = CacheConfig::default();
        assert_eq!(config.capacity.get(), 10_000);
        assert_eq!(config.ttl, Some(Duration::from_secs(60)));
        assert_eq!(config.max_body_size, 1024 * 1024);

        let config = CacheConfig::with_capacity(NonZeroUsize::new(3).unwrap());
        assert_eq!(config.capacity.get(), 3);
        let debug = format!("{:?}", CacheLayer::new(config));
        assert!(debug.contains("max_body_size"));
    }

    #[test]
    fn read_rpc_predicate_covers_all_prefixes() {
        for method in ["List", "Get", "Check", "Expand", "To", "Describe"] {
            let path = format!("/pkg.Service/{method}Something");
            let req = Request::post(path).body(Body::empty()).unwrap();
            assert!(default_cacheable_predicate(&req));
        }
        let bare = Request::post("/GetThing").body(Body::empty()).unwrap();
        assert!(default_cacheable_predicate(&bare));
        let put = Request::put("/pkg.Service/GetThing")
            .body(Body::empty())
            .unwrap();
        assert!(!default_cacheable_predicate(&put));
    }

    #[test]
    fn cached_response_expiry_and_response_conversion() {
        let entry = CachedResponse {
            status: http::StatusCode::CREATED,
            headers: vec![
                ("x-good".to_string(), b"yes".to_vec()),
                ("bad header".to_string(), b"skip".to_vec()),
            ],
            body: Bytes::from_static(b"cached"),
            created_at: Instant::now(),
        };
        assert!(!entry.is_expired(None));
        assert!(!entry.is_expired(Some(Duration::from_secs(60))));

        let response = entry.to_response();
        assert_eq!(response.status(), http::StatusCode::CREATED);
        assert_eq!(response.headers().get("x-good").unwrap(), "yes");
        assert!(response.headers().get("bad header").is_none());
    }

    #[test]
    fn build_cache_key_includes_scope_headers_and_body() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            http::HeaderValue::from_static("application/json"),
        );
        headers.insert(ACCEPT, http::HeaderValue::from_static("application/json"));
        let scope = CacheScope {
            tenant: Some("tenant".into()),
            actor: Some("actor".into()),
        };
        let first = build_cache_key(
            &http::Method::POST,
            &"/pkg.Service/Get".parse().unwrap(),
            &headers,
            &scope,
            &Bytes::from_static(b"body"),
        );
        let second = build_cache_key(
            &http::Method::POST,
            &"/pkg.Service/Get".parse().unwrap(),
            &headers,
            &scope,
            &Bytes::from_static(b"other"),
        );
        assert_ne!(first.body_hash, second.body_hash);
        assert_eq!(first.scope, "t=tenant:a=actor");
    }
}

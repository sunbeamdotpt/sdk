//! Test harness that runs a real HTTP server on a random port.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use axum::Router as AxumRouter;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::g2v::error::{ServiceError, ServiceResult};
use crate::g2v::router::ServiceRouter;
use crate::g2v::server::ServerConfig;
use crate::g2v::server::axum::{AxumServer, bind_random_port};

/// Test harness configuration.
#[derive(Debug, Clone)]
pub struct TestHarnessConfig {
    /// Name of the service under test.
    pub service_name: String,
    /// Host address to bind the test server to.
    pub host: String,
    /// Whether to enable Prometheus metrics endpoint.
    pub enable_metrics: bool,
    /// Whether to enable distributed tracing.
    pub enable_tracing: bool,
}

impl Default for TestHarnessConfig {
    fn default() -> Self {
        Self {
            service_name: "test-service".to_string(),
            host: "127.0.0.1".to_string(),
            enable_metrics: false,
            enable_tracing: false,
        }
    }
}

/// Running harness state — server task + shutdown channel.
struct Running {
    addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    handle: Option<JoinHandle<()>>,
}

/// Test harness that boots a real `axum::serve` on a random port and exposes
/// a `reqwest` client pinned to the bound address.
pub struct TestHarness {
    config: TestHarnessConfig,
    service_router: Option<ServiceRouter>,
    extra_routes: Option<AxumRouter>,
    running: Option<Running>,
    client: reqwest::Client,
}

impl std::fmt::Debug for TestHarness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestHarness")
            .field("config", &self.config)
            .field("running", &self.running.as_ref().map(|r| r.addr))
            .finish()
    }
}

impl TestHarness {
    /// Create a new harness with the default configuration.
    pub fn new() -> Self {
        Self::with_config(TestHarnessConfig::default())
    }

    /// Create a new harness with the given configuration.
    pub fn with_config(config: TestHarnessConfig) -> Self {
        Self {
            config,
            service_router: None,
            extra_routes: None,
            running: None,
            client: match reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
            {
                Ok(client) => client,
                Err(e) => {
                    // Only fails when the process TLS backend is broken; fall
                    // back to the default client and make it visible.
                    tracing::warn!("failed to build test-harness HTTP client; using defaults: {e}");
                    reqwest::Client::new()
                }
            },
        }
    }

    /// Register the ConnectRPC service router that should be served.
    pub fn with_router(mut self, router: ServiceRouter) -> Self {
        self.service_router = Some(router);
        self
    }

    /// Merge extra axum routes (e.g. `/health`).
    pub fn with_routes(mut self, routes: AxumRouter) -> Self {
        self.extra_routes = match self.extra_routes.take() {
            Some(existing) => Some(existing.merge(routes)),
            None => Some(routes),
        };
        self
    }

    /// Return a reference to the current harness configuration.
    pub fn config(&self) -> &TestHarnessConfig {
        &self.config
    }

    /// Bound address (only available after `start`).
    pub fn addr(&self) -> Option<SocketAddr> {
        self.running.as_ref().map(|r| r.addr)
    }

    /// Base URL of the running server (only available after `start`).
    pub fn base_url(&self) -> Option<String> {
        self.addr().map(|a| format!("http://{a}"))
    }

    /// Start the server on a random port. Resolves once the listener is bound.
    pub async fn start(&mut self) -> ServiceResult<()> {
        if self.running.is_some() {
            return Err(ServiceError::Configuration(
                "harness already started".into(),
            ));
        }

        let router = match self.service_router.take() {
            Some(router) => router,
            // No service router registered: serve an empty one.
            None => ServiceRouter::default(),
        };
        let (listener, addr) = bind_random_port(&self.config.host).await?;

        let mut server = AxumServer::with_config(
            router,
            ServerConfig {
                addr,
                name: self.config.service_name.clone(),
                ..ServerConfig::default()
            },
        );
        if let Some(extra) = self.extra_routes.take() {
            server = server.merge(extra);
        }

        let app = server.app();
        let (tx, rx) = oneshot::channel::<()>();

        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });

        self.running = Some(Running {
            addr,
            shutdown: Some(tx),
            handle: Some(handle),
        });
        Ok(())
    }

    /// Stop the server and wait for the task to complete.
    pub async fn stop(&mut self) -> ServiceResult<()> {
        if let Some(mut running) = self.running.take() {
            if let Some(tx) = running.shutdown.take() {
                let _ = tx.send(());
            }
            if let Some(handle) = running.handle.take() {
                let _ = handle.await;
            }
        }
        Ok(())
    }

    /// Issue a request against the running server.
    pub async fn request(
        &self,
        method: &str,
        path: &str,
        body: Option<Vec<u8>>,
        headers: Option<HashMap<String, String>>,
    ) -> ServiceResult<TestResponse> {
        let base = self
            .base_url()
            .ok_or_else(|| ServiceError::Configuration("harness not started".into()))?;
        let url = format!("{}{}", base, path);

        let method = reqwest::Method::from_bytes(method.as_bytes())
            .map_err(|e| ServiceError::InvalidArgument(format!("bad method {method}: {e}")))?;
        let mut req = self.client.request(method, &url);
        if let Some(headers) = headers {
            for (k, v) in headers {
                req = req.header(k, v);
            }
        }
        if let Some(body) = body {
            req = req.body(body);
        }

        let resp = req
            .send()
            .await
            .map_err(|e| ServiceError::Internal(format!("send: {e}")))?;
        let status = resp.status().as_u16();
        let mut header_map = HashMap::new();
        for (k, v) in resp.headers() {
            if let Ok(s) = v.to_str() {
                header_map.insert(k.as_str().to_string(), s.to_string());
            }
        }
        let body = resp
            .bytes()
            .await
            .map_err(|e| ServiceError::Internal(format!("read body: {e}")))?
            .to_vec();
        Ok(TestResponse {
            status,
            body,
            headers: header_map,
        })
    }

    /// Send a GET request to the running server.
    pub async fn get(&self, path: &str) -> ServiceResult<TestResponse> {
        self.request("GET", path, None, None).await
    }

    /// Send a POST request with the given body to the running server.
    pub async fn post(&self, path: &str, body: impl Into<Vec<u8>>) -> ServiceResult<TestResponse> {
        self.request("POST", path, Some(body.into()), None).await
    }

    /// Poll `path` until it returns 2xx or `timeout` elapses.
    pub async fn wait_for_ready(&self, path: &str, timeout: Duration) -> ServiceResult<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if let Ok(resp) = self.get(path).await
                && resp.is_success()
            {
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(ServiceError::DeadlineExceeded(format!(
                    "{path} not ready within {timeout:?}"
                )));
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}

impl Default for TestHarness {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        if let Some(mut running) = self.running.take()
            && let Some(tx) = running.shutdown.take()
        {
            let _ = tx.send(());
        }
    }
}

/// Test response.
#[derive(Debug, Clone)]
pub struct TestResponse {
    /// HTTP status code.
    pub status: u16,
    /// Raw response body bytes.
    pub body: Vec<u8>,
    /// Response headers map.
    pub headers: HashMap<String, String>,
}

impl TestResponse {
    /// Decode the response body as a UTF-8 string.
    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }

    /// Returns `true` if the status code is in the 2xx range.
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Look up a response header by name.
    pub fn get_header(&self, key: &str) -> Option<&String> {
        self.headers.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router as AxumRouter, routing::get};

    #[tokio::test]
    async fn test_harness_new() {
        let harness = TestHarness::new();
        assert_eq!(harness.config.service_name, "test-service");
        assert!(harness.addr().is_none());
    }

    #[tokio::test]
    async fn test_harness_start_stop() {
        let mut harness = TestHarness::new();
        harness.start().await.unwrap();
        let addr = harness.addr().expect("bound");
        assert_ne!(addr.port(), 0);
        harness.stop().await.unwrap();
        assert!(harness.addr().is_none());
    }

    #[tokio::test]
    async fn test_harness_real_request() {
        let extra = AxumRouter::new().route("/ping", get(|| async { "pong" }));
        let mut harness = TestHarness::new().with_routes(extra);

        harness.start().await.unwrap();
        let resp = harness.get("/ping").await.unwrap();
        assert!(resp.is_success());
        assert_eq!(resp.body_text(), "pong");
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_404_on_missing_route() {
        let mut harness = TestHarness::new();
        harness.start().await.unwrap();
        let resp = harness.get("/nope").await.unwrap();
        assert_eq!(resp.status, 404);
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_wait_for_ready() {
        let extra = AxumRouter::new().route("/health", get(|| async { "ok" }));
        let mut harness = TestHarness::new().with_routes(extra);
        harness.start().await.unwrap();
        harness
            .wait_for_ready("/health", Duration::from_secs(2))
            .await
            .unwrap();
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_response_helpers() {
        let response = TestResponse {
            status: 200,
            body: b"hello".to_vec(),
            headers: HashMap::new(),
        };
        assert_eq!(response.body_text(), "hello");
        assert!(response.is_success());
    }

    #[test]
    fn test_harness_config_debug_and_not_started_errors() {
        let config = TestHarnessConfig {
            service_name: "configured".to_string(),
            host: "127.0.0.1".to_string(),
            enable_metrics: true,
            enable_tracing: true,
        };
        let harness = TestHarness::with_config(config.clone());
        assert_eq!(harness.config().service_name, "configured");
        assert!(harness.base_url().is_none());
        assert!(format!("{harness:?}").contains("configured"));
    }

    #[tokio::test]
    async fn test_harness_request_requires_start_and_stop_is_idempotent() {
        let mut harness = TestHarness::default();
        let result = harness.get("/ping").await;
        assert!(matches!(result, Err(ServiceError::Configuration(_))));
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_rejects_second_start() {
        let mut harness = TestHarness::new();
        harness.start().await.unwrap();
        assert!(harness.base_url().is_some());
        let result = harness.start().await;
        assert!(matches!(result, Err(ServiceError::Configuration(_))));
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_post_headers_and_invalid_method() {
        use axum::{body::Bytes, http::HeaderMap};

        let extra = AxumRouter::new().route(
            "/echo",
            axum::routing::post(|headers: HeaderMap, body: Bytes| async move {
                if let Some(value) = headers.get("x-test") {
                    assert_eq!(value, "yes");
                }
                body
            }),
        );
        let mut harness = TestHarness::new().with_routes(extra);
        harness.start().await.unwrap();

        let response = harness
            .request(
                "POST",
                "/echo",
                Some(b"payload".to_vec()),
                Some(HashMap::from([("x-test".to_string(), "yes".to_string())])),
            )
            .await
            .unwrap();
        assert_eq!(response.body, b"payload");
        assert!(response.get_header("x-request-id").is_some());

        let response = harness.post("/echo", b"helper").await.unwrap();
        assert_eq!(response.body, b"helper");

        let result = harness.request("BAD METHOD", "/echo", None, None).await;
        assert!(matches!(result, Err(ServiceError::InvalidArgument(_))));
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_wait_for_ready_times_out() {
        let mut harness = TestHarness::new();
        harness.start().await.unwrap();
        let result = harness
            .wait_for_ready("/missing", Duration::from_millis(30))
            .await;
        assert!(matches!(result, Err(ServiceError::DeadlineExceeded(_))));
        harness.stop().await.unwrap();
    }

    #[tokio::test]
    async fn test_harness_drop_stops_server() {
        let mut harness = TestHarness::new();
        harness.start().await.unwrap();
        let addr = harness.addr().unwrap();
        drop(harness);

        let result = reqwest::Client::new()
            .get(format!("http://{addr}/ping"))
            .send()
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_response_get_header() {
        let response = TestResponse {
            status: 404,
            body: Vec::new(),
            headers: HashMap::from([("x-test".to_string(), "yes".to_string())]),
        };
        assert!(!response.is_success());
        assert_eq!(response.get_header("x-test"), Some(&"yes".to_string()));
        assert_eq!(response.get_header("missing"), None);
    }
}

//! Mock utilities for testing Sunbeam services.

use std::collections::HashMap;
use std::sync::Arc;

/// Mock request.
#[derive(Debug, Clone)]
pub struct MockRequest {
    /// Request method.
    pub method: String,
    /// Request path.
    pub path: String,
    /// Request headers.
    pub headers: HashMap<String, String>,
    /// Request body.
    pub body: Vec<u8>,
}

impl MockRequest {
    /// Create a new mock request.
    pub fn new(method: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            headers: HashMap::new(),
            body: vec![],
        }
    }

    /// Add a header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set the body.
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }
}

impl Default for MockRequest {
    fn default() -> Self {
        Self::new("GET", "/")
    }
}

/// Mock response.
#[derive(Debug, Clone)]
pub struct MockResponse {
    /// Response status code.
    pub status: u16,
    /// Response headers.
    pub headers: HashMap<String, String>,
    /// Response body.
    pub body: Vec<u8>,
}

impl MockResponse {
    /// Create a new mock response.
    pub fn new(status: u16) -> Self {
        Self {
            status,
            headers: HashMap::new(),
            body: vec![],
        }
    }

    /// Add a header.
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Set the body.
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = body.into();
        self
    }

    /// Create a JSON response.
    pub fn json<T: serde::Serialize>(status: u16, value: &T) -> Result<Self, serde_json::Error> {
        let body = serde_json::to_vec(value)?;
        Ok(Self::new(status)
            .with_body(body)
            .with_header("Content-Type", "application/json"))
    }
}

impl Default for MockResponse {
    fn default() -> Self {
        Self::new(200)
    }
}

/// Mock handler.
///
/// A handler that takes a request and returns a response.
/// Uses Arc to allow cloning of routers.
pub type MockHandler = Arc<dyn Fn(MockRequest) -> MockResponse + Send + Sync>;

/// Mock router.
///
/// A router that maps paths to handlers.
#[derive(Clone, Default)]
pub struct MockRouter {
    /// Route handlers.
    routes: HashMap<String, MockHandler>,
    /// Default handler for unmatched routes.
    default_handler: Option<MockHandler>,
}

impl MockRouter {
    /// Create a new mock router.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a route handler.
    pub fn add_route<F>(mut self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(MockRequest) -> MockResponse + Send + Sync + 'static,
    {
        self.routes.insert(path.into(), Arc::new(handler));
        self
    }

    /// Set the default handler.
    pub fn with_default<F>(mut self, handler: F) -> Self
    where
        F: Fn(MockRequest) -> MockResponse + Send + Sync + 'static,
    {
        self.default_handler = Some(Arc::new(handler));
        self
    }

    /// Handle a request.
    pub fn handle(&self, request: MockRequest) -> MockResponse {
        if let Some(handler) = self.routes.get(&request.path) {
            handler(request)
        } else if let Some(default) = &self.default_handler {
            default(request)
        } else {
            MockResponse::new(404).with_body("Not found")
        }
    }
}

/// Mock service.
///
/// A service that uses a mock router.
#[derive(Clone)]
pub struct MockService {
    /// The mock router.
    router: Arc<MockRouter>,
    /// Service name.
    pub name: String,
}

impl MockService {
    /// Create a new mock service.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            router: Arc::new(MockRouter::new()),
            name: name.into(),
        }
    }

    /// Add a route handler.
    pub fn add_route<F>(self, path: impl Into<String>, handler: F) -> Self
    where
        F: Fn(MockRequest) -> MockResponse + Send + Sync + 'static,
    {
        let mut router = (*self.router).clone();
        router = router.add_route(path, handler);
        Self {
            router: Arc::new(router),
            name: self.name,
        }
    }

    /// Handle a request.
    pub fn handle(&self, request: MockRequest) -> MockResponse {
        self.router.handle(request)
    }
}

impl Default for MockService {
    fn default() -> Self {
        Self::new("mock-service")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_request_new() {
        let req = MockRequest::new("GET", "/health");
        assert_eq!(req.method, "GET");
        assert_eq!(req.path, "/health");
    }

    #[test]
    fn test_mock_request_with_header() {
        let req = MockRequest::new("GET", "/health").with_header("Authorization", "Bearer token");
        assert_eq!(
            req.headers.get("Authorization"),
            Some(&"Bearer token".to_string())
        );
    }

    #[test]
    fn test_mock_response_new() {
        let resp = MockResponse::new(200);
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn test_mock_response_json() {
        #[derive(serde::Serialize)]
        struct Data {
            message: String,
        }
        let data = Data {
            message: "hello".to_string(),
        };
        let resp = MockResponse::json(200, &data).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, b"{\"message\":\"hello\"}");
    }

    #[test]
    fn test_mock_router_add_route() {
        let router = MockRouter::new()
            .add_route("/health", |_| MockResponse::new(200))
            .with_default(|_| MockResponse::new(404));

        let resp = router.handle(MockRequest::new("GET", "/health"));
        assert_eq!(resp.status, 200);

        let resp = router.handle(MockRequest::new("GET", "/other"));
        assert_eq!(resp.status, 404);
    }

    #[test]
    fn test_mock_service_new() {
        let service = MockService::new("test-service");
        assert_eq!(service.name, "test-service");
    }

    #[test]
    fn test_mock_service_add_route() {
        let service =
            MockService::new("test-service").add_route("/health", |_| MockResponse::new(200));

        let resp = service.handle(MockRequest::new("GET", "/health"));
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn test_mock_request_default_body_and_response_builders() {
        let request = MockRequest::default()
            .with_header("x-test", "yes")
            .with_body("payload");
        assert_eq!(request.method, "GET");
        assert_eq!(request.path, "/");
        assert_eq!(request.body, b"payload");

        let response = MockResponse::default()
            .with_header("x-test", "yes")
            .with_body(vec![1, 2, 3]);
        assert_eq!(response.status, 200);
        assert_eq!(response.headers.get("x-test"), Some(&"yes".to_string()));
        assert_eq!(response.body, vec![1, 2, 3]);
    }

    #[test]
    fn test_mock_router_default_not_found_and_service_default() {
        let router = MockRouter::new();
        let response = router.handle(MockRequest::new("GET", "/missing"));
        assert_eq!(response.status, 404);
        assert_eq!(response.body, b"Not found");

        let service = MockService::default();
        assert_eq!(service.name, "mock-service");
        let cloned = service.clone();
        let response = cloned.handle(MockRequest::default());
        assert_eq!(response.status, 404);
    }
}

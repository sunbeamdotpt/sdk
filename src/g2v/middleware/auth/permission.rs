//! Permission enforcement middleware.

use axum::body::Body;
use axum::response::{IntoResponse, Response};
use connectrpc::{ConnectError, ErrorCode};
use std::{
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context as TaskContext, Poll},
};
use tower::{Layer, Service};

use super::authorization::AuthorizationClient;
use super::{AuthContext, TenantId};

/// Function that derives the authorization object from a request.
pub type ObjectExtractor = Arc<dyn Fn(&http::Request<()>) -> String + Send + Sync>;

/// Tower layer that enforces a permission check on every request.
///
/// Requires a [`TenantId`] and an [`AuthContext`] in request extensions (injected
/// by [`super::auth_middleware`]). Requests without them are rejected with 401.
/// If the authorization backend denies the check the request is rejected with 403.
#[derive(Clone)]
pub struct PermissionLayer {
    client: Arc<AuthorizationClient>,
    namespace: Arc<String>,
    relation: Arc<String>,
    skip_paths: Arc<Vec<String>>,
    object_extractor: ObjectExtractor,
}

impl std::fmt::Debug for PermissionLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionLayer")
            .field("namespace", &self.namespace)
            .field("relation", &self.relation)
            .field("skip_paths", &self.skip_paths)
            .finish()
    }
}

impl PermissionLayer {
    /// Create a layer that checks `relation` in `namespace`.
    ///
    /// The default object extractor returns `req.uri().path()`.
    pub fn new(
        client: AuthorizationClient,
        namespace: impl Into<String>,
        relation: impl Into<String>,
    ) -> Self {
        Self {
            client: Arc::new(client),
            namespace: Arc::new(namespace.into()),
            relation: Arc::new(relation.into()),
            skip_paths: Arc::new(Vec::new()),
            object_extractor: Arc::new(|req| req.uri().path().to_string()),
        }
    }

    /// Create a layer from a client that is already infallibly constructed.
    ///
    /// Convenience for `PermissionLayer::new(client, ...)` when the client is
    /// already constructed.
    pub fn from_client(
        client: AuthorizationClient,
        namespace: impl Into<String>,
        relation: impl Into<String>,
    ) -> Self {
        Self::new(client, namespace, relation)
    }

    /// Skip the permission check for any request whose path starts with `prefix`.
    pub fn skip_path(mut self, prefix: impl Into<String>) -> Self {
        Arc::make_mut(&mut self.skip_paths).push(prefix.into());
        self
    }

    /// Override the function that derives the authorization object.
    pub fn with_object_extractor(mut self, f: ObjectExtractor) -> Self {
        self.object_extractor = f;
        self
    }
}

impl<S> Layer<S> for PermissionLayer {
    type Service = PermissionService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        PermissionService {
            inner,
            client: Arc::clone(&self.client),
            namespace: Arc::clone(&self.namespace),
            relation: Arc::clone(&self.relation),
            skip_paths: Arc::clone(&self.skip_paths),
            object_extractor: Arc::clone(&self.object_extractor),
        }
    }
}

/// Tower [`Service`] produced by [`PermissionLayer`].
#[derive(Clone)]
pub struct PermissionService<S> {
    inner: S,
    client: Arc<AuthorizationClient>,
    namespace: Arc<String>,
    relation: Arc<String>,
    skip_paths: Arc<Vec<String>>,
    object_extractor: ObjectExtractor,
}

impl<S> Service<http::Request<Body>> for PermissionService<S>
where
    S: Service<http::Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<Body>) -> Self::Future {
        let path = req.uri().path().to_string();

        for prefix in self.skip_paths.iter() {
            if path.starts_with(prefix.as_str()) {
                return Box::pin(self.inner.call(req));
            }
        }

        let has_tenant = matches!(
            req.extensions().get::<TenantId>(),
            Some(tenant) if !tenant.0.is_empty()
        );
        if !has_tenant {
            let resp = unauthorized("missing tenant");
            return Box::pin(async move { Ok(resp) });
        }

        let subject = match req
            .extensions()
            .get::<AuthContext>()
            .and_then(|ctx| ctx.subject.clone())
        {
            Some(subject) => subject,
            None => {
                // A request that passed auth_middleware always carries a
                // subject; a missing one means the stack was hand-rolled.
                // Failing the permission check below is the safe outcome.
                tracing::warn!(msg = "permission check without an authenticated subject");
                String::new()
            }
        };

        let (parts, body) = req.into_parts();
        let unit_req = http::Request::from_parts(parts.clone(), ());
        let object = (self.object_extractor)(&unit_req);
        let req = http::Request::from_parts(parts, body);

        let client = Arc::clone(&self.client);
        let namespace = Arc::clone(&self.namespace);
        let relation = Arc::clone(&self.relation);
        let mut inner = self.inner.clone();

        Box::pin(async move {
            match client
                .check_permission(&namespace, &object, &relation, &subject)
                .await
            {
                Ok(true) => inner.call(req).await,
                Ok(false) => Ok(forbidden("permission denied")),
                Err(e) => Ok(internal(e)),
            }
        })
    }
}

fn unauthorized(message: &str) -> Response {
    ConnectError::new(ErrorCode::Unauthenticated, message).into_response()
}

fn forbidden(message: &str) -> Response {
    ConnectError::new(ErrorCode::PermissionDenied, message).into_response()
}

fn internal(message: impl std::fmt::Display) -> Response {
    ConnectError::new(ErrorCode::Internal, message.to_string()).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::g2v::middleware::auth::AuthorizationConfig;
    use axum::body::Body;
    use std::future::Future;
    use tower::{ServiceBuilder, ServiceExt};

    fn ok_service() -> impl Service<
        http::Request<Body>,
        Response = Response,
        Error = std::convert::Infallible,
        Future = impl Future<Output = Result<Response, std::convert::Infallible>>,
    > + Clone {
        tower::service_fn(|_req: http::Request<Body>| async {
            Ok::<_, std::convert::Infallible>(
                http::Response::builder()
                    .status(http::StatusCode::OK)
                    .body(Body::empty())
                    .unwrap()
                    .into_response(),
            )
        })
    }

    #[test]
    fn test_permission_layer_skip_path_builder() {
        let layer = PermissionLayer::from_client(
            AuthorizationClient::with_defaults().unwrap(),
            "ns",
            "read",
        )
        .skip_path("/health")
        .skip_path("/metrics");
        assert_eq!(layer.skip_paths.len(), 2);
    }

    #[test]
    fn test_permission_layer_debug_and_object_extractor() {
        let layer = PermissionLayer::new(
            AuthorizationClient::with_defaults().unwrap(),
            "documents",
            "read",
        )
        .with_object_extractor(Arc::new(|_req| "object-1".to_string()));
        let request = http::Request::get("/ignored").body(()).unwrap();
        assert_eq!((layer.object_extractor)(&request), "object-1");
        let debug = format!("{layer:?}");
        assert!(debug.contains("documents"));
        assert!(debug.contains("read"));
    }

    #[test]
    fn test_permission_error_responses() {
        assert_eq!(
            unauthorized("missing").status(),
            http::StatusCode::UNAUTHORIZED
        );
        assert_eq!(forbidden("denied").status(), http::StatusCode::FORBIDDEN);
        assert_eq!(
            internal("backend").status(),
            http::StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[tokio::test]
    async fn test_permission_layer_missing_tenant_returns_401() {
        let layer = PermissionLayer::from_client(
            AuthorizationClient::with_defaults().unwrap(),
            "ns",
            "read",
        );
        let mut svc = ServiceBuilder::new().layer(layer).service(ok_service());

        let req = http::Request::builder()
            .uri("/protected")
            .body(Body::empty())
            .unwrap();

        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_permission_layer_skip_path_forwards() {
        let layer = PermissionLayer::from_client(
            AuthorizationClient::new(AuthorizationConfig {
                read_url: "http://127.0.0.1:1".to_string(),
                write_url: "http://127.0.0.1:1".to_string(),
            })
            .unwrap(),
            "ns",
            "read",
        )
        .skip_path("/health");

        let mut svc = ServiceBuilder::new().layer(layer).service(ok_service());
        let req = http::Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), http::StatusCode::OK);
    }
}

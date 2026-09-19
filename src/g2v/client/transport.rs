//! Reqwest-based Tower transport for the Sunbeam HTTP client.
//!
//! [`ReqwestService`] bridges the internal Tower middleware stack (which speaks
//! `http::Request<Bytes>` / `http::Response<Bytes>`) to `reqwest`, the actual
//! HTTP transport used by this client.

use std::task::{Context as TaskContext, Poll};

use bytes::Bytes;
use http::{Request, Response};
use tower::{Layer, Service};

use crate::g2v::BoxError;
use crate::g2v::BoxFuture;

/// A Tower [`Service`] backed by a [`reqwest::Client`].
///
/// Accepts [`http::Request<Bytes>`] and returns [`http::Response<Bytes>`],
/// converting to/from `reqwest` types internally.
#[derive(Clone)]
pub struct ReqwestService {
    client: reqwest::Client,
}

impl ReqwestService {
    /// Create a new reqwest-backed transport.
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl std::fmt::Debug for ReqwestService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReqwestService").finish()
    }
}

impl Service<Request<Bytes>> for ReqwestService {
    type Response = Response<Bytes>;
    type Error = BoxError;
    type Future = BoxFuture<Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Bytes>) -> Self::Future {
        let client = self.client.clone();
        Box::pin(async move {
            let reqwest_req = request_to_reqwest(&client, req)?;
            let resp = client.execute(reqwest_req).await?;
            let http_resp = response_from_reqwest(resp).await?;
            Ok(http_resp)
        })
    }
}

/// Tower [`Layer`] that produces a [`ReqwestService`].
#[derive(Clone, Debug)]
pub struct ReqwestLayer {
    client: reqwest::Client,
}

impl ReqwestLayer {
    /// Create a new layer wrapping the given reqwest client.
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }
}

impl<S> Layer<S> for ReqwestLayer {
    type Service = ReqwestService;

    fn layer(&self, _inner: S) -> Self::Service {
        ReqwestService::new(self.client.clone())
    }
}

/// Convert an [`http::Request<Bytes>`] into a [`reqwest::Request`].
fn request_to_reqwest(
    client: &reqwest::Client,
    req: Request<Bytes>,
) -> Result<reqwest::Request, BoxError> {
    let (parts, body) = req.into_parts();
    let url = reqwest::Url::parse(&parts.uri.to_string())?;
    let mut reqwest_req = client.request(parts.method, url);

    for (name, value) in &parts.headers {
        reqwest_req = reqwest_req.header(name.as_str(), value.as_bytes());
    }

    reqwest_req = reqwest_req.body(reqwest::Body::from(body));
    Ok(reqwest_req.build()?)
}

/// Convert a [`reqwest::Response`] into an [`http::Response<Bytes>`].
async fn response_from_reqwest(resp: reqwest::Response) -> Result<Response<Bytes>, BoxError> {
    let mut builder = Response::builder().status(resp.status());

    for (name, value) in resp.headers() {
        builder = builder.header(name.as_str(), value.as_bytes());
    }

    let body = resp.bytes().await?;
    Ok(builder.body(body)?)
}

#[cfg(all(test, feature = "g2v-server"))]
mod tests {
    use super::*;
    use axum::{Router, routing::get};
    use bytes::Bytes;
    use http::{Request, StatusCode};
    use std::time::Duration;
    use tower::ServiceExt;

    async fn spawn_echo_server() -> std::net::SocketAddr {
        let app = Router::new().route("/", get(|| async { "hello" })).route(
            "/headers",
            get(|headers: axum::http::HeaderMap| async move {
                let value = headers
                    .get("x-test")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("missing");
                value.to_string()
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::time::sleep(Duration::from_millis(50)).await;
        addr
    }

    #[tokio::test]
    async fn test_reqwest_service_hits_local_server() {
        let addr = spawn_echo_server().await;
        let client = reqwest::Client::new();
        let mut service = ReqwestService::new(client);

        let req = Request::get(format!("http://{addr}/"))
            .body(Bytes::new())
            .unwrap();
        let resp = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.into_body().as_ref(), b"hello");
    }

    #[tokio::test]
    async fn test_reqwest_service_preserves_headers() {
        let addr = spawn_echo_server().await;
        let client = reqwest::Client::new();
        let mut service = ReqwestService::new(client);

        let req = Request::get(format!("http://{addr}/headers"))
            .header("x-test", "value")
            .body(Bytes::new())
            .unwrap();
        let resp = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.into_body().as_ref(), b"value");
    }

    #[tokio::test]
    async fn test_reqwest_layer() {
        let addr = spawn_echo_server().await;
        let client = reqwest::Client::new();
        let layer = ReqwestLayer::new(client);
        let mut service = layer.layer(tower::service_fn(|_req: Request<Bytes>| async {
            Ok::<_, BoxError>(Response::new(Bytes::new()))
        }));

        let req = Request::get(format!("http://{addr}/"))
            .body(Bytes::new())
            .unwrap();
        let resp = service.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}

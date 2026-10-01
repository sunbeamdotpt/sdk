//! ConnectRPC transport for the Sunbeam unified HTTP client.
//!
//! [`ConnectTransport`] implements [`connectrpc::client::ClientTransport`].
//! Unary calls ride the full middleware stack (auth, retry, cache,
//! circuit-breaker) whose terminal buffers the response body. Server
//! streams cannot: they may outlive any wall-clock bound, so their requests
//! carry auth + trace propagation and then bypass the buffering terminal —
//! the response body streams straight through to the ConnectRPC codec.
//! Streams are never retried mid-flight and never cached.

use http::{Request, Response, Uri};
use http_body_util::{BodyExt, StreamBody};

use super::builder::Client;

/// ConnectRPC transport backed by the unified Sunbeam HTTP client.
#[derive(Clone, Debug)]
pub struct ConnectTransport {
    client: Client,
    base_uri: Uri,
}

impl ConnectTransport {
    /// Create a new ConnectRPC transport for the given base URI.
    pub fn new(client: Client, base_uri: Uri) -> Self {
        Self { client, base_uri }
    }

    /// Return the base URI.
    pub fn base_uri(&self) -> &Uri {
        &self.base_uri
    }
}



impl connectrpc::client::ClientTransport for ConnectTransport {
    type ResponseBody = connectrpc::client::ClientBody;
    type Error = connectrpc::ConnectError;

    fn send(
        &self,
        request: Request<connectrpc::client::ClientBody>,
    ) -> connectrpc::client::BoxFuture<'static, Result<Response<Self::ResponseBody>, Self::Error>>
    {
        let client = self.client.clone();
        let base_uri = self.base_uri.clone();

        Box::pin(async move {
            // Buffer the streaming request body to a single Bytes buffer.
            // Connect requests are a single POST — only the RESPONSE is a
            // stream, so this is bounded.
            let (mut parts, body) = request.into_parts();
            let collected = body.collect().await.map_err(|e| {
                connectrpc::ConnectError::unavailable(format!("request body error: {e}"))
            })?;
            let body_bytes = collected.to_bytes();

            // Rewrite the request URI to include the configured base URI.
            let path_and_query = parts.uri.path_and_query().cloned();
            let mut builder = http::uri::Builder::new();
            if let Some(scheme) = base_uri.scheme() {
                builder = builder.scheme(scheme.clone());
            } else {
                builder = builder.scheme("http");
            }
            if let Some(authority) = base_uri.authority() {
                builder = builder.authority(authority.clone());
            }
            if let Some(pq) = path_and_query {
                builder = builder.path_and_query(pq);
            }
            parts.uri = builder.build().map_err(|e| {
                connectrpc::ConnectError::invalid_argument(format!("invalid URI: {e}"))
            })?;

            // Server streams bypass the buffering unary terminal: auth and
            // trace propagation still ride, retry/cache/breaker do not.
            let stack = client.streaming();
            if let Some(provider) = &stack.auth {
                let token = provider.token().await.map_err(|e| {
                    connectrpc::ConnectError::unavailable(format!("token fetch failed: {e}"))
                })?;
                if let Ok(header) = http::HeaderValue::from_str(&format!("Bearer {token}")) {
                    parts.headers.insert(http::header::AUTHORIZATION, header);
                }
            }
            super::propagation::inject_request_headers(&mut parts.headers);

            let url = reqwest::Url::parse(&parts.uri.to_string())
                .map_err(|e| connectrpc::ConnectError::invalid_argument(format!("invalid URL: {e}")))?;
            let mut reqwest_req = stack
                .http
                .request(parts.method, url);
            for (name, value) in &parts.headers {
                reqwest_req = reqwest_req.header(name.as_str(), value.as_bytes());
            }
            let reqwest_req = reqwest_req
                .body(reqwest::Body::from(body_bytes))
                .build()
                .map_err(|e| connectrpc::ConnectError::unavailable(format!("request build failed: {e}")))?;
            let resp = stack
                .http
                .execute(reqwest_req)
                .await
                .map_err(|e| connectrpc::ConnectError::unavailable(format!("request failed: {e}")))?;

            // Status + headers are available now; the body streams through.
            let mut response_builder = Response::builder().status(resp.status());
            for (name, value) in resp.headers() {
                response_builder = response_builder.header(name, value);
            }
            let stream = futures::StreamExt::map(resp.bytes_stream(), |chunk| {
                chunk
                    .map(http_body::Frame::data)
                    .map_err(|e| {
                        connectrpc::ConnectError::unavailable(format!("response body error: {e}"))
                    })
            });
            let streamed: connectrpc::client::ClientBody =
                http_body_util::BodyExt::boxed(StreamBody::new(stream));
            response_builder
                .body(streamed)
                .map_err(|e| connectrpc::ConnectError::internal(format!("response build failed: {e}")))
        })
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use connectrpc::client::ClientTransport;
    use http::{HeaderMap, Request, Response, StatusCode};
    use http_body_util::BodyExt;

    use crate::g2v::client::builder::{BoxedClientService, Client, ClientBuilder};
    use crate::g2v::client::rest::ClientError;

    fn map_client_error(e: ClientError) -> connectrpc::ConnectError {
        match e {
            ClientError::Transport(boxed) => {
                if let Some(connect_err) = boxed.downcast_ref::<connectrpc::ConnectError>() {
                    connect_err.clone()
                } else {
                    connectrpc::ConnectError::unavailable(boxed.to_string())
                }
            }
            ClientError::Serialization(err) => connectrpc::ConnectError::internal(err.to_string()),
            ClientError::InvalidUrl(url) => connectrpc::ConnectError::invalid_argument(url),
        }
    }

    fn connect_mock_client() -> Client {
        let service = tower::service_fn(|req: Request<Bytes>| async move {
            assert_eq!(req.uri().scheme_str(), Some("http"));
            assert_eq!(req.uri().host(), Some("example.com"));
            assert_eq!(req.uri().path(), "/service/Method");
            Ok::<_, crate::g2v::BoxError>(
                Response::builder()
                    .status(StatusCode::OK)
                    .header(http::header::CONTENT_TYPE, "application/json")
                    .body(Bytes::from_static(b"hello"))
                    .unwrap(),
            )
        });

        Client::from_service(
            BoxedClientService::new(service),
            reqwest::Url::parse("http://example.com").unwrap(),
            HeaderMap::new(),
        )
    }

    #[tokio::test]
    async fn test_connect_transport_rewrites_uri_and_buffers_body() {
        // A local stub answers every POST; the transport must rewrite the
        // request onto the base URI and deliver the buffered request body.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let n = std::io::Read::read(&mut stream, &mut buf).unwrap_or(0);
            let raw = String::from_utf8_lossy(&buf[..n]).into_owned();
            assert!(raw.starts_with("POST /service/Method "), "request line: {raw}");
            assert!(raw.contains("request-body"), "request body rides: {raw}");
            let head = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello";
            let _ = std::io::Write::write_all(&mut stream, head);
            let _ = std::io::Write::flush(&mut stream);
        });

        let client = ClientBuilder::new(format!("http://{addr}"))
            .build()
            .expect("client");
        let transport = ConnectTransport::new(client, format!("http://{addr}").parse().unwrap());

        let body = connectrpc::client::full_body(Bytes::from_static(b"request-body"));
        let req = Request::post("/service/Method").body(body).unwrap();

        let resp = transport.send(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let collected = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(collected.as_ref(), b"hello");
    }

    #[test]
    fn test_connect_transport_base_uri() {
        let client = Client::from_service(
            BoxedClientService::new(tower::service_fn(|_req: Request<Bytes>| async {
                Ok::<_, crate::g2v::BoxError>(Response::new(Bytes::new()))
            })),
            reqwest::Url::parse("http://example.com").unwrap(),
            HeaderMap::new(),
        );
        let transport = ConnectTransport::new(client, "http://rpc.example.com".parse().unwrap());
        assert_eq!(transport.base_uri().to_string(), "http://rpc.example.com/");
    }

    #[test]
    fn test_map_client_error_variants() {
        let transport_err = ClientError::Transport(Box::new(std::io::Error::other("boom")));
        let err = map_client_error(transport_err);
        assert_eq!(err.code, connectrpc::ErrorCode::Unavailable);

        let ser_err = ClientError::Serialization(
            serde_json::from_str::<serde_json::Value>("not-json").unwrap_err(),
        );
        let err = map_client_error(ser_err);
        assert_eq!(err.code, connectrpc::ErrorCode::Internal);

        let url_err = ClientError::InvalidUrl("bad url".to_string());
        let err = map_client_error(url_err);
        assert_eq!(err.code, connectrpc::ErrorCode::InvalidArgument);
    }

    #[test]
    fn test_map_client_error_downcasts_connect_error() {
        let inner = connectrpc::ConnectError::canceled("canceled");
        let transport_err = ClientError::Transport(Box::new(inner.clone()));
        let err = map_client_error(transport_err);
        assert_eq!(err.code, connectrpc::ErrorCode::Canceled);
    }

    /// The IAPI-031 shape: a server stream that stays open must not block
    /// the response head. The transport returns as soon as the head arrives;
    /// frames stream through afterwards. The unary stack would buffer the
    /// whole body here and `send` would never resolve.
    #[tokio::test]
    async fn server_stream_head_arrives_while_body_still_open() {
        use std::time::Duration;

        // One-shot HTTP stub: head immediately, first body frame only after
        // a delay long enough to prove send() did not wait for it.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buf = [0u8; 4096];
            let _ = std::io::Read::read(&mut stream, &mut buf);
            let head = b"HTTP/1.1 200 OK\r\nContent-Type: application/connect+proto\r\nTransfer-Encoding: chunked\r\n\r\n";
            let _ = std::io::Write::write_all(&mut stream, head);
            let _ = std::io::Write::flush(&mut stream);
            std::thread::sleep(Duration::from_millis(700));
            // One 5-frame envelope: 0x05, uncompressed, 5 bytes of payload.
            let frame: &[u8] = &[0x00, 0x00, 0x00, 0x00, 0x05, 1, 2, 3, 4, 5];
            let chunk = format!("{:x}\r\n", frame.len());
            let _ = std::io::Write::write_all(&mut stream, chunk.as_bytes());
            let _ = std::io::Write::write_all(&mut stream, frame);
            let _ = std::io::Write::write_all(&mut stream, b"\r\n");
            let _ = std::io::Write::write_all(&mut stream, b"0\r\n\r\n");
            let _ = std::io::Write::flush(&mut stream);
        });

        let client = ClientBuilder::new(format!("http://{addr}"))
            .build()
            .expect("client");
        let transport = client.connectrpc(
            format!("http://{addr}")
                .parse::<http::Uri>()
                .expect("uri"),
        );

        let request = Request::builder()
            .method(http::Method::POST)
            .uri(format!("http://{addr}/svc/Watch"))
            .header("Content-Type", "application/connect+proto")
            .body(connectrpc::client::full_body(Bytes::new()))
            .unwrap();

        // Head must arrive while the stub is still holding the body back.
        let response = tokio::time::timeout(Duration::from_secs(2), transport.send(request))
            .await
            .expect("send resolves on the response head, not the body")
            .expect("head is a success");
        assert_eq!(response.status(), StatusCode::OK);

        // And the delayed frame still streams through.
        let mut body = response.into_body();
        let first = tokio::time::timeout(Duration::from_secs(5), body.frame())
            .await
            .expect("frame arrives after the delay")
            .expect("frame read");
        assert!(first.is_ok(), "one envelope frame was written: {first:?}");
    }
}

//! ConnectRPC transport for the Sunbeam unified HTTP client.
//!
//! [`ConnectTransport`] implements [`connectrpc::client::ClientTransport`] on top
//! of the shared resilience/auth/TLS stack. Only unary calls are supported.

use http::{Request, Response, Uri};
use http_body_util::BodyExt;

use super::builder::Client;
use super::rest::ClientError;

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

            let http_request = Request::from_parts(parts, body_bytes);

            let response = client
                .execute(http_request)
                .await
                .map_err(map_client_error)?;

            let (parts, body) = response.into_parts();
            let body = connectrpc::client::full_body(body);
            Ok(Response::from_parts(parts, body))
        })
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use connectrpc::client::ClientTransport;
    use http::{HeaderMap, Request, Response, StatusCode};
    use http_body_util::BodyExt;

    use crate::g2v::client::builder::{BoxedClientService, Client};

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
        let client = connect_mock_client();
        let base_uri: http::Uri = "http://example.com".parse().unwrap();
        let transport = ConnectTransport::new(client, base_uri);

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
        let err = super::map_client_error(transport_err);
        assert_eq!(err.code, connectrpc::ErrorCode::Unavailable);

        let ser_err = ClientError::Serialization(
            serde_json::from_str::<serde_json::Value>("not-json").unwrap_err(),
        );
        let err = super::map_client_error(ser_err);
        assert_eq!(err.code, connectrpc::ErrorCode::Internal);

        let url_err = ClientError::InvalidUrl("bad url".to_string());
        let err = super::map_client_error(url_err);
        assert_eq!(err.code, connectrpc::ErrorCode::InvalidArgument);
    }

    #[test]
    fn test_map_client_error_downcasts_connect_error() {
        let inner = connectrpc::ConnectError::canceled("canceled");
        let transport_err = ClientError::Transport(Box::new(inner.clone()));
        let err = super::map_client_error(transport_err);
        assert_eq!(err.code, connectrpc::ErrorCode::Canceled);
    }
}

//! REST client for the Sunbeam unified HTTP client.
//!
//! [`RestClient`] exposes the familiar HTTP verb API on top of the shared
//! resilience/auth/TLS stack managed by [`Client`].

use bytes::Bytes;
use http::{HeaderMap, HeaderName, HeaderValue, Method, Request, Response, header::CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use super::builder::Client;

/// Errors that can occur during a REST request.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The request could not be sent or the response could not be read.
    #[error("transport error: {0}")]
    Transport(#[source] crate::g2v::BoxError),
    /// The response body could not be deserialized.
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    /// The requested URL is not valid.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
}

/// REST client built on the unified Sunbeam HTTP stack.
#[derive(Clone, Debug)]
pub struct RestClient {
    client: Client,
    base_url: reqwest::Url,
    default_headers: HeaderMap,
}

impl RestClient {
    /// Create a new REST client.
    pub(crate) fn new(client: Client, base_url: reqwest::Url, default_headers: HeaderMap) -> Self {
        Self {
            client,
            base_url,
            default_headers,
        }
    }

    /// Build a request with the given method and path.
    ///
    /// `path` is resolved against the client's base URL. Default headers
    /// configured on the [`Client`] are applied to every request built from
    /// this client.
    pub fn request(&self, method: Method, path: &str) -> Result<RequestBuilder, ClientError> {
        let url = self
            .base_url
            .join(path)
            .map_err(|e| ClientError::InvalidUrl(e.to_string()))?;
        Ok(RequestBuilder::new(
            self.client.clone(),
            method,
            url,
            self.default_headers.clone(),
        ))
    }

    /// Make a GET request.
    pub fn get(&self, path: &str) -> Result<RequestBuilder, ClientError> {
        self.request(Method::GET, path)
    }

    /// Make a POST request.
    pub fn post(&self, path: &str) -> Result<RequestBuilder, ClientError> {
        self.request(Method::POST, path)
    }

    /// Make a PUT request.
    pub fn put(&self, path: &str) -> Result<RequestBuilder, ClientError> {
        self.request(Method::PUT, path)
    }

    /// Make a DELETE request.
    pub fn delete(&self, path: &str) -> Result<RequestBuilder, ClientError> {
        self.request(Method::DELETE, path)
    }

    /// Make a PATCH request.
    pub fn patch(&self, path: &str) -> Result<RequestBuilder, ClientError> {
        self.request(Method::PATCH, path)
    }

    /// POST a JSON body and deserialize the response.
    pub async fn post_json<Req, Resp>(&self, path: &str, body: &Req) -> Result<Resp, ClientError>
    where
        Req: Serialize,
        Resp: for<'de> Deserialize<'de>,
    {
        let resp = self
            .post(path)?
            .header(CONTENT_TYPE, HeaderValue::from_static("application/json"))?
            .json(body)?
            .send()
            .await?;
        let body = resp.into_body();
        Ok(serde_json::from_slice(&body)?)
    }

    /// GET a URL and deserialize the JSON response.
    pub async fn json<T>(&self, path: &str) -> Result<T, ClientError>
    where
        T: for<'de> Deserialize<'de>,
    {
        let resp = self.get(path)?.send().await?;
        let body = resp.into_body();
        Ok(serde_json::from_slice(&body)?)
    }

    /// GET a URL and return the response as a UTF-8 string.
    pub async fn text(&self, path: &str) -> Result<String, ClientError> {
        let resp = self.get(path)?.send().await?;
        let body = resp.into_body();
        String::from_utf8(body.to_vec()).map_err(|e| {
            ClientError::Transport(Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                e,
            )))
        })
    }

    /// GET a URL and return the raw response bytes.
    pub async fn bytes(&self, path: &str) -> Result<Bytes, ClientError> {
        let resp = self.get(path)?.send().await?;
        Ok(resp.into_body())
    }
}

/// Request builder for the REST client.
#[derive(Clone, Debug)]
pub struct RequestBuilder {
    client: Client,
    method: Method,
    url: reqwest::Url,
    headers: HeaderMap,
    body: Option<Bytes>,
}

impl RequestBuilder {
    fn new(client: Client, method: Method, url: reqwest::Url, default_headers: HeaderMap) -> Self {
        Self {
            client,
            method,
            url,
            headers: default_headers,
            body: None,
        }
    }

    /// Add a header to the request.
    pub fn header<N, V>(mut self, name: N, value: V) -> Result<Self, ClientError>
    where
        N: TryInto<HeaderName>,
        N::Error: std::fmt::Display,
        V: TryInto<HeaderValue>,
        V::Error: std::fmt::Display,
    {
        let name = name
            .try_into()
            .map_err(|e| ClientError::InvalidUrl(e.to_string()))?;
        let value = value
            .try_into()
            .map_err(|e| ClientError::InvalidUrl(e.to_string()))?;
        self.headers.append(name, value);
        Ok(self)
    }

    /// Set the request body to a JSON-serialized value.
    ///
    /// # Errors
    ///
    /// Returns [`ClientError::Serialization`] if the value fails to serialize.
    pub fn json<T: Serialize>(mut self, value: &T) -> Result<Self, ClientError> {
        self.body = Some(Bytes::from(serde_json::to_vec(value)?));
        Ok(self)
    }

    /// Set the request body to raw bytes.
    pub fn body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }

    /// Send the request and return the raw HTTP response.
    pub async fn send(self) -> Result<Response<Bytes>, ClientError> {
        let mut builder = Request::builder()
            .method(self.method)
            .uri(self.url.as_str());
        for (name, value) in &self.headers {
            builder = builder.header(name, value);
        }
        let body = match self.body {
            Some(body) => body,
            None => Bytes::new(),
        };
        let req = builder
            .body(body)
            .map_err(|e| ClientError::Transport(Box::new(e)))?;
        self.client.execute(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{HeaderMap, Method, Request, Response, StatusCode, header::CONTENT_TYPE};

    fn mock_client() -> Client {
        let service = tower::service_fn(|req: Request<Bytes>| async move {
            let (parts, body) = req.into_parts();
            let path = parts.uri.path().to_string();
            let method = parts.method.clone();
            Ok::<_, crate::g2v::BoxError>(match (method, path.as_str()) {
                (Method::GET, "/json") => Response::builder()
                    .header(CONTENT_TYPE, "application/json")
                    .body(Bytes::from_static(br#"{"hello":"world"}"#))
                    .unwrap(),
                (Method::GET, "/text") => Response::builder()
                    .body(Bytes::from_static(b"plain text"))
                    .unwrap(),
                (Method::GET, "/bytes") => Response::builder()
                    .body(Bytes::from_static(b"raw bytes"))
                    .unwrap(),
                (Method::POST, "/echo") => Response::builder().body(body).unwrap(),
                (Method::PUT, "/put") => {
                    Response::builder().status(204).body(Bytes::new()).unwrap()
                }
                (Method::DELETE, "/delete") => {
                    Response::builder().status(204).body(Bytes::new()).unwrap()
                }
                (Method::PATCH, "/patch") => {
                    Response::builder().status(204).body(Bytes::new()).unwrap()
                }
                (Method::GET, "/headers") => {
                    let val = parts
                        .headers
                        .get("x-test")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("missing");
                    Response::builder()
                        .body(Bytes::from(val.to_string()))
                        .unwrap()
                }
                _ => Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Bytes::new())
                    .unwrap(),
            })
        });

        Client::from_service(
            crate::g2v::client::builder::BoxedClientService::new(service),
            reqwest::Url::parse("http://example.com").unwrap(),
            HeaderMap::new(),
        )
    }

    #[tokio::test]
    async fn test_rest_get_json() {
        let client = mock_client();
        let value: serde_json::Value = client.rest().json("/json").await.unwrap();
        assert_eq!(value["hello"], "world");
    }

    #[tokio::test]
    async fn test_rest_text() {
        let client = mock_client();
        let text = client.rest().text("/text").await.unwrap();
        assert_eq!(text, "plain text");
    }

    #[tokio::test]
    async fn test_rest_bytes() {
        let client = mock_client();
        let bytes = client.rest().bytes("/bytes").await.unwrap();
        assert_eq!(bytes.as_ref(), b"raw bytes");
    }

    #[tokio::test]
    async fn test_rest_post_json_and_methods() {
        let client = mock_client();

        #[derive(serde::Serialize)]
        struct Echo {
            message: String,
        }
        let resp: serde_json::Value = client
            .rest()
            .post_json(
                "/echo",
                &Echo {
                    message: "hi".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(resp["message"], "hi");

        let put_resp = client.rest().put("/put").unwrap().send().await.unwrap();
        assert_eq!(put_resp.status(), StatusCode::NO_CONTENT);

        let del_resp = client
            .rest()
            .delete("/delete")
            .unwrap()
            .send()
            .await
            .unwrap();
        assert_eq!(del_resp.status(), StatusCode::NO_CONTENT);

        let patch_resp = client.rest().patch("/patch").unwrap().send().await.unwrap();
        assert_eq!(patch_resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn test_rest_request_builder_header_and_body() {
        let client = mock_client();
        let resp = client
            .rest()
            .request(Method::GET, "/headers")
            .unwrap()
            .header("x-test", "present")
            .unwrap()
            .body("ignored")
            .send()
            .await
            .unwrap();
        assert_eq!(resp.into_body().as_ref(), b"present");
    }

    #[tokio::test]
    async fn test_rest_request_builder_invalid_header() {
        let client = mock_client();
        let err = client
            .rest()
            .get("/")
            .unwrap()
            .header("x-test", "\0")
            .unwrap_err();
        assert!(matches!(err, ClientError::InvalidUrl(_)));
    }

    #[test]
    fn test_client_error_display() {
        let err = ClientError::InvalidUrl("bad".to_string());
        assert!(format!("{err}").contains("bad"));
    }
}

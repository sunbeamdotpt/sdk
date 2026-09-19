//! Logging middleware.

use tower::{Layer, Service};

/// Logging middleware layer.
pub struct LoggingLayer;

impl<S> Layer<S> for LoggingLayer {
    type Service = LoggingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LoggingService { inner }
    }
}

/// Logging middleware service.
pub struct LoggingService<S> {
    inner: S,
}

impl<S, B> Service<http::Request<B>> for LoggingService<S>
where
    S: Service<http::Request<B>> + Send + 'static,
    S::Future: Send + 'static,
    S::Response: Send + 'static,
    S::Error: std::fmt::Debug + Send + 'static,
    B: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>,
    >;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: http::Request<B>) -> Self::Future {
        // In a real implementation, we'd log the request here
        Box::pin(self.inner.call(req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logging_layer_creation() {
        let _ = LoggingLayer;
    }

    #[tokio::test]
    async fn test_logging_service_forwards_success() {
        use bytes::Bytes;
        use http::{Request, Response};
        use tower::{ServiceExt as _, service_fn};

        let mut service = LoggingLayer.layer(service_fn(|req: Request<Bytes>| async move {
            assert_eq!(req.uri().path(), "/logged");
            Ok::<_, std::convert::Infallible>(Response::new(Bytes::from_static(b"ok")))
        }));

        service.ready().await.unwrap();
        let response = service
            .call(Request::get("/logged").body(Bytes::new()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.body().as_ref(), b"ok");
    }

    #[tokio::test]
    async fn test_logging_service_forwards_error() {
        use bytes::Bytes;
        use http::{Request, Response};
        use tower::{ServiceExt as _, service_fn};

        let mut service = LoggingLayer.layer(service_fn(|_req: Request<Bytes>| async {
            Err::<Response<Bytes>, _>("logged failure")
        }));

        service.ready().await.unwrap();
        let error = service.call(Request::new(Bytes::new())).await.unwrap_err();
        assert_eq!(error, "logged failure");
    }
}

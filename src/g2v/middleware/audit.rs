//! Best-effort audit logging middleware.
//!
//! Captures the HTTP method, path, resolved tenant, authenticated actor, and
//! response status, and emits a structured log event to the standard log
//! stream tagged with `crate::g2v::audit`.

use axum::{extract::Request, middleware::Next, response::Response};

use crate::g2v::middleware::auth::{AuthContext, TenantId};

/// Audit logging middleware.
///
/// Emits a `tracing::info!` event with target `crate::g2v::audit` after each
/// request. The event includes:
///
/// - `tenant_id` — from `AuthContext` or `TenantId` extensions, or the
///   `x-tenant-id` header.
/// - `actor` — from `AuthContext`.
/// - `action` — HTTP method.
/// - `resource` — request path.
/// - `outcome` — `success` or `failure`.
/// - `status` — HTTP response status code.
pub async fn audit_middleware(request: Request, next: Next) -> Response {
    let tenant_id = request
        .extensions()
        .get::<AuthContext>()
        .and_then(|c| c.tenant_id.clone())
        .or_else(|| request.extensions().get::<TenantId>().map(|t| t.0.clone()))
        .or_else(|| {
            request
                .headers()
                .get("x-tenant-id")
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        });
    let actor = request
        .extensions()
        .get::<AuthContext>()
        .and_then(|c| c.subject.clone());
    let method = request.method().to_string();
    let resource = request.uri().path().to_string();

    let response = next.run(request).await;

    let outcome = if response.status().is_success() {
        "success"
    } else {
        "failure"
    };

    tracing::info!(
        target: "crate::g2v::audit",
        tenant_id = tenant_id.as_deref(),
        actor = actor.as_deref(),
        action = method.as_str(),
        resource = resource.as_str(),
        outcome = outcome,
        status = response.status().as_u16(),
        "audit event"
    );

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    #[tokio::test]
    async fn audit_middleware_forwards_request() {
        use axum::Router;
        use axum::routing::get;
        use tower::ServiceExt;

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(audit_middleware));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
    }

    #[tokio::test]
    async fn audit_middleware_uses_extensions_and_preserves_failure_status() {
        use axum::Router;
        use axum::http::StatusCode;
        use axum::routing::get;
        use tower::ServiceExt;

        let app = Router::new()
            .route("/fail", get(|| async { StatusCode::BAD_REQUEST }))
            .layer(axum::middleware::from_fn(audit_middleware));

        let mut request = Request::builder()
            .method("GET")
            .uri("/fail")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(AuthContext::authenticated("tenant-1", "actor-1"));

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn audit_middleware_accepts_tenant_header() {
        use axum::Router;
        use axum::routing::get;
        use tower::ServiceExt;

        let app = Router::new()
            .route("/header", get(|| async { "ok" }))
            .layer(axum::middleware::from_fn(audit_middleware));
        let request = Request::builder()
            .uri("/header")
            .header("x-tenant-id", "tenant-header")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), 200);
    }
}

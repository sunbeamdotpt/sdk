//! End-to-end example for `sunbeam-g2v` — multitenancy-aware auth, health, and
//! Connect-RPC features.
//!
//! Demonstrates a realistic service with public routes, bearer token
//! introspection, session cookie auth, permission-authorised routes, a health
//! router wired via `ServerBuilder::with_health`, and a Connect-RPC Eliza
//! service for end-to-end demo with the FE example.
//!
//! # Try it
//!
//! ```text
//! # Start the server:
//! cargo run -p sunbeam-g2v --example simple
//!
//! # Public routes (no auth needed):
//! curl http://localhost:8080/
//! curl http://localhost:8080/health/live
//! curl http://localhost:8080/health/ready
//!
//! # Connect-RPC Eliza (no auth needed):
//! curl -X POST -H "Content-Type: application/json" \
//!   -d '{"sentence":"hello"}' \
//!   http://localhost:8080/connectrpc.eliza.v1.ElizaService/Say
//!
//! # Authenticated route — bearer token via Authorization header:
//! TOKEN=<paste token here>
//! curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/whoami
//!
//! # Authorised route — requires the authorization backend to grant the tuple first:
//! # (without the backend running, the permission check returns 500; that is expected)
//! curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/g2v/secrets/42
//!
//! # FE demo:
//! cd libs/sunbeam-g2v-fe/packages/sunbeam-g2v/examples/simple && npm run dev
//! # open http://localhost:5173 — RPC calls will hit this server
//!
//! # Override env:
//! BIND_ADDR=0.0.0.0:9090 cargo run -p sunbeam-g2v --example simple
//! ```

#![allow(refining_impl_trait_internal, refining_impl_trait_reachable)]
// Examples may unwrap freely (SSO-027/G2V-003 carve-out).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)]

use std::sync::Arc;

use axum::{
    Router as AxumRouter,
    extract::{Path, Request},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::get,
};
use futures::stream;
use serde_json::json;
use tower_http::cors::{Any, CorsLayer};

use sdk::g2v::error::ServiceResult;
use sdk::g2v::health::{HealthRouter, PermissionHealthCheck};
use sdk::g2v::middleware::auth;
use sdk::g2v::middleware::auth::{
    AuthContext, AuthMiddlewareState, TenantId, authorization::AuthorizationClient,
    authorization::AuthorizationConfig, introspection::IntrospectionConfig,
    introspection::IntrospectionSessionClient, permission::PermissionLayer,
    session_token::SessionTokenSigner,
};
use sdk::g2v::router::ServiceRouter;
use sdk::g2v::server::{ServerConfig, builder::ServerBuilder};
use sdk::g2v::{RequestContext, Router as ConnectRouter};

// Pull in the generated Eliza service trait, message types, and the Ext trait.
// Wrapped in a module to avoid name collision with the `connectrpc` extern crate.
mod eliza_proto {
    include!(concat!(env!("OUT_DIR"), "/_eliza.rs"));
}

use eliza_proto::connectrpc::eliza::v1::{
    ElizaService, ElizaServiceExt, IntroduceRequest, IntroduceResponse, SayRequest, SayResponse,
};

// ============================================================================
// Eliza service implementation
// ============================================================================

struct ElizaServiceImpl;

impl ElizaService for ElizaServiceImpl {
    async fn say(
        &self,
        _ctx: RequestContext,
        request: connectrpc::ServiceRequest<'_, SayRequest>,
    ) -> connectrpc::ServiceResult<SayResponse> {
        let sentence = request.view().sentence.to_lowercase();
        let reply = if sentence.contains("hello") || sentence.contains("hi") {
            "Hello! I'm Eliza, your digital therapist. How are you feeling today?"
        } else if sentence.contains("feel") || sentence.contains("feeling") {
            "Tell me more about how you're feeling."
        } else if sentence.contains("sad")
            || sentence.contains("unhappy")
            || sentence.contains("depressed")
        {
            "I'm sorry to hear that. What do you think is causing these feelings?"
        } else if sentence.contains("happy")
            || sentence.contains("good")
            || sentence.contains("great")
        {
            "I'm glad to hear that! What's been making you feel this way?"
        } else if sentence.contains("bye") || sentence.contains("goodbye") {
            "Goodbye! Take care of yourself."
        } else if sentence.contains("?") {
            "That's an interesting question. What do you think the answer is?"
        } else if sentence.is_empty() {
            "I'm listening. Please, go on."
        } else {
            "Please, tell me more."
        };

        Ok(connectrpc::Response::new(SayResponse {
            sentence: reply.to_string(),
            ..Default::default()
        }))
    }

    async fn introduce(
        &self,
        _ctx: RequestContext,
        request: connectrpc::ServiceRequest<'_, IntroduceRequest>,
    ) -> connectrpc::ServiceResult<connectrpc::ServiceStream<IntroduceResponse>> {
        let name = request.view().name.to_string();
        let responses = vec![
            Ok(IntroduceResponse {
                sentence: format!("Hi {name}! I'm Eliza, a digital therapist."),
                ..Default::default()
            }),
            Ok(IntroduceResponse {
                sentence: "I'm here to listen and help you explore your thoughts.".to_string(),
                ..Default::default()
            }),
            Ok(IntroduceResponse {
                sentence: "Feel free to tell me what's on your mind.".to_string(),
                ..Default::default()
            }),
        ];
        connectrpc::Response::stream_ok(stream::iter(responses))
    }
}

// ============================================================================
// Axum route handlers
// ============================================================================

/// `GET /` — public, no auth required.
async fn index_handler() -> impl IntoResponse {
    Json(json!({ "service": "sunbeam-g2v simple example", "status": "ok" }))
}

/// `GET /whoami` — requires a valid bearer token or session cookie.
async fn whoami_handler(request: Request) -> impl IntoResponse {
    let tenant = request.extensions().get::<TenantId>().cloned();
    let ctx = request.extensions().get::<AuthContext>().cloned();

    match (tenant, ctx) {
        (Some(tenant), Some(ctx)) if ctx.is_authenticated() => Json(json!({
            "tenant": tenant.0,
            "subject": ctx.subject,
            "scopes": ctx.scopes,
            "authentication_methods": ctx.authentication_methods,
            "token_hash": ctx.token_hash,
        }))
        .into_response(),
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthenticated" })),
        )
            .into_response(),
    }
}

/// `GET /g2v/secrets/:id` — requires auth then a permission check.
async fn secret_handler(Path(id): Path<String>, request: Request) -> impl IntoResponse {
    let tenant = request.extensions().get::<TenantId>().cloned();
    let ctx = request.extensions().get::<AuthContext>().cloned();
    Json(json!({
        "tenant": tenant.map(|t| t.0),
        "subject": ctx.and_then(|c| c.subject),
        "secret_id": id,
        "message": "access granted"
    }))
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> ServiceResult<()> {
    // ---- telemetry (OTLP export when OTEL_EXPORTER_OTLP_ENDPOINT is set) ----
    let _telemetry = sdk::g2v::telemetry::init(sdk::g2v::telemetry::TelemetryConfig {
        service_name: "g2v-simple-example".to_string(),
        service_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        ..Default::default()
    })?;

    // ---- configuration from env (no panics on missing vars) ----------------
    let auth_read_url =
        std::env::var("AUTH_READ_URL").unwrap_or_else(|_| "http://localhost:4466".to_string());
    let auth_write_url =
        std::env::var("AUTH_WRITE_URL").unwrap_or_else(|_| "http://localhost:4467".to_string());
    let introspection_url = std::env::var("INTROSPECTION_URL")
        .unwrap_or_else(|_| "http://localhost:4445/oauth2/introspect".to_string());
    let introspection_client_id =
        std::env::var("INTROSPECTION_CLIENT_ID").unwrap_or_else(|_| "example-client".to_string());
    let introspection_client_secret = std::env::var("INTROSPECTION_CLIENT_SECRET")
        .unwrap_or_else(|_| "example-secret".to_string());

    let bind_addr: std::net::SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8080".to_string())
        .parse()
        .unwrap_or_else(|_| "127.0.0.1:8080".parse().unwrap());

    // ---- shared state -------------------------------------------------------
    let session_signer = SessionTokenSigner::new(
        b"example-cookie-secret-that-is-at-least-32-bytes",
        3600,
        "https://gateway.example.com",
    )
    .expect("example session secret is 32+ bytes");

    let auth_state = AuthMiddlewareState::new(Arc::new(IntrospectionSessionClient::new(
        IntrospectionConfig {
            url: introspection_url,
            client_id: introspection_client_id,
            client_secret: introspection_client_secret,
            timeout: None,
        },
    )))
    .with_session_signer(session_signer);

    let authorization_client = Arc::new(
        AuthorizationClient::new(AuthorizationConfig {
            read_url: auth_read_url,
            write_url: auth_write_url,
        })
        .unwrap_or_else(|e| panic!("invalid authorization config: {e}")),
    );

    // ---- CORS (permissive for dev — allows the FE at localhost:5173) --------
    // Applied as a layer on all axum routes so preflight OPTIONS requests succeed
    // for both RPC paths (/connectrpc.eliza.v1.ElizaService/Say) and REST routes.
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .expose_headers(Any);

    // ---- Connect-RPC Eliza service -----------------------------------------
    // Register the Eliza service on the ConnectRouter, then wrap it in a
    // ServiceRouter so the ServerBuilder can own it cleanly (it becomes the
    // single fallback via into_axum_router() inside AxumServer).
    let eliza = Arc::new(ElizaServiceImpl);
    let connect_router: ConnectRouter = eliza.register(ConnectRouter::new());
    let service_router = ServiceRouter::from_router(connect_router);

    // ---- Axum Routes -------------------------------------------------------

    let public_routes = AxumRouter::new().route("/", get(index_handler));

    let authed_routes = AxumRouter::new()
        .route("/whoami", get(whoami_handler))
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth::auth_middleware,
        ));

    let permission_layer = PermissionLayer::new((*authorization_client).clone(), "G2vTest", "read")
        .with_object_extractor(Arc::new(|req: &http::Request<()>| {
            req.uri()
                .path()
                .rsplit('/')
                .next()
                .unwrap_or("")
                .to_string()
        }));

    let authzd_routes = AxumRouter::new()
        .route("/g2v/secrets/{id}", get(secret_handler))
        .layer(permission_layer)
        .layer(axum::middleware::from_fn_with_state(
            auth_state.clone(),
            auth::auth_middleware,
        ));

    let app_routes = public_routes.merge(authed_routes).merge(authzd_routes);

    // ---- Health router -----------------------------------------------------
    let health = HealthRouter::new().with_check(Arc::new(PermissionHealthCheck::new(
        (*authorization_client).clone(),
    )));

    // ---- Server config ------------------------------------------------------
    let config = ServerConfig {
        addr: bind_addr,
        name: "simple-example".to_string(),
        ..ServerConfig::default()
    };

    // ---- Dev-mode hint --------------------------------------------------
    let sample_tenant = "01ARYZ6S41TSV4RRFFQ69G5FAV";

    println!("listening on http://{}", config.addr);
    println!();
    println!("  sample tenant:  {sample_tenant}");
    println!();
    println!("  curl http://{}/", config.addr);
    println!(
        "  curl -H 'Authorization: Bearer <token>' http://{}/whoami",
        config.addr
    );
    println!(
        "  curl -H 'Authorization: Bearer <token>' http://{}/g2v/secrets/42",
        config.addr
    );
    println!("  curl http://{}/health/live", config.addr);
    println!("  curl http://{}/health/ready", config.addr);
    println!();
    println!("  # Connect-RPC Eliza (no auth):");
    println!(
        "  curl -X POST -H 'Content-Type: application/json' -d '{{\"sentence\":\"hello\"}}' \",
"
    );
    println!(
        "    http://{}/connectrpc.eliza.v1.ElizaService/Say",
        config.addr
    );
    println!();

    // ---- Graceful shutdown --------------------------------------------------
    let shutdown = async {
        let _ = tokio::signal::ctrl_c().await;
        println!("shutdown signal received");
    };

    // Build the full axum app (connect service as fallback + extra routes merged in),
    // then apply CORS as the outermost layer so preflight OPTIONS on the RPC path
    // (/connectrpc.eliza.v1.ElizaService/Say) is handled before the Connect handler
    // rejects it with 415.
    let server = ServerBuilder::new()
        .with_router(service_router)
        .with_config(config)
        .with_health(health)
        .with_routes(app_routes)
        .build_axum()?;

    let app = server.app().layer(cors);

    let listener = tokio::net::TcpListener::bind(server.config().addr)
        .await
        .map_err(|e| sdk::g2v::error::ServiceError::Internal(format!("bind: {e}")))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .map_err(|e| sdk::g2v::error::ServiceError::Internal(format!("axum::serve: {e}")))
}

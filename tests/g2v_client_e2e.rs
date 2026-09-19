#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)
//! End-to-end tests for the unified Sunbeam HTTP client.
//!
//! These tests spin up a real in-process Axum server (REST, GraphQL, and a
//! ConnectRPC Eliza service) and exercise the full client stack: TLS hooks,
//! retry, circuit breaker, cache, bearer-token auth, and ConnectRPC transport.

#![cfg(all(
    feature = "g2v-client",
    feature = "g2v-client-connectrpc",
    feature = "g2v-server"
))]
#![allow(refining_impl_trait_internal, refining_impl_trait_reachable)]

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use axum::{
    Router as AxumRouter,
    extract::{Json, State},
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::{Value, json};
use tokio::time::sleep;

use sdk::g2v::client::{
    BearerToken,
    builder::ClientBuilder,
    cache::CacheConfig,
    circuit_breaker::{CircuitBreaker, CircuitBreakerConfig},
    retry::RetryPolicy,
};

// Include the generated Eliza service stubs.
mod eliza_proto {
    include!(concat!(env!("OUT_DIR"), "/_eliza.rs"));
}

use eliza_proto::connectrpc::eliza::v1::{
    ElizaService, ElizaServiceClient, ElizaServiceExt, IntroduceRequest, IntroduceResponse,
    SayRequest, SayResponse,
};

// ============================================================================
// Test server state and routes
// ============================================================================

struct ServerState {
    counter: AtomicUsize,
    flaky: AtomicUsize,
    flaky_success_after: usize,
}

impl ServerState {
    fn new(flaky_success_after: usize) -> Self {
        Self {
            counter: AtomicUsize::new(0),
            flaky: AtomicUsize::new(0),
            flaky_success_after,
        }
    }
}

async fn index_handler() -> impl IntoResponse {
    Json(json!({ "service": "sunbeam-g2v-e2e", "status": "ok" }))
}

async fn echo_handler(body: String) -> impl IntoResponse {
    body
}

async fn whoami_handler(headers: header::HeaderMap) -> impl IntoResponse {
    if let Some(auth) = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        && auth.starts_with("Bearer secret")
    {
        return (StatusCode::OK, Json(json!({ "user": "test" })));
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({ "error": "unauthorized" })),
    )
}

async fn graphql_handler(Json(payload): Json<Value>) -> impl IntoResponse {
    if payload["query"].as_str().unwrap_or("").contains("GetFoo") {
        Json(json!({ "data": { "foo": "bar" } }))
    } else {
        Json(json!({ "errors": [{ "message": "unknown query" }] }))
    }
}

async fn flaky_handler(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    let n = state.flaky.fetch_add(1, Ordering::SeqCst);
    if n < state.flaky_success_after {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "boom" })),
        )
    } else {
        (StatusCode::OK, Json(json!({ "recovered": true })))
    }
}

async fn counter_handler(State(state): State<Arc<ServerState>>) -> impl IntoResponse {
    let n = state.counter.fetch_add(1, Ordering::SeqCst);
    Json(json!({ "count": n }))
}

// ============================================================================
// ConnectRPC Eliza service implementation
// ============================================================================

struct ElizaImpl;

impl ElizaService for ElizaImpl {
    async fn say(
        &self,
        _ctx: connectrpc::RequestContext,
        request: connectrpc::ServiceRequest<'_, SayRequest>,
    ) -> connectrpc::ServiceResult<SayResponse> {
        let sentence = request.view().sentence.to_lowercase();
        let reply = if sentence.contains("hello") || sentence.contains("hi") {
            "Hello! How can I help you?"
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
        _ctx: connectrpc::RequestContext,
        request: connectrpc::ServiceRequest<'_, IntroduceRequest>,
    ) -> connectrpc::ServiceResult<connectrpc::ServiceStream<IntroduceResponse>> {
        let name = request.view().name.to_string();
        let responses = vec![Ok(IntroduceResponse {
            sentence: format!("Hi {name}! Welcome to Sunbeam."),
            ..Default::default()
        })];
        connectrpc::Response::stream_ok(futures::stream::iter(responses))
    }
}

// ============================================================================
// Server setup
// ============================================================================

async fn spawn_server(flaky_success_after: usize) -> SocketAddr {
    let state = Arc::new(ServerState::new(flaky_success_after));

    let eliza = Arc::new(ElizaImpl);
    let connect_router: connectrpc::Router = eliza.register(connectrpc::Router::new());
    let connect_service = connect_router.into_axum_service();

    let app = AxumRouter::new()
        .route("/", get(index_handler))
        .route("/echo", post(echo_handler))
        .route("/whoami", get(whoami_handler))
        .route("/graphql", post(graphql_handler))
        .route("/flaky", get(flaky_handler))
        .route("/counter", get(counter_handler))
        .with_state(state)
        .fallback_service(connect_service);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let addr = listener.local_addr().expect("local addr");

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve test server");
    });

    // Give the server a moment to start accepting connections.
    sleep(Duration::from_millis(100)).await;
    addr
}

fn test_client(addr: SocketAddr, auth: bool, cache: bool) -> sdk::g2v::client::builder::Client {
    let mut builder = ClientBuilder::new(format!("http://{addr}"))
        .retry(RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(10),
            max_backoff: Duration::from_millis(100),
            jitter: false,
            retry_unauthenticated: false,
        })
        .circuit_breaker(Arc::new(CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 1,
            open_duration: Duration::from_millis(500),
            half_open_max_calls: 1,
        })));

    if auth {
        builder = builder.auth(BearerToken::new("secret"));
    }

    if cache {
        builder = builder.cache(CacheConfig {
            capacity: NonZeroUsize::new(100).unwrap(),
            ttl: Some(Duration::from_secs(60)),
            max_body_size: 1024 * 1024,
        });
    }

    builder.build().expect("build test client")
}

// ============================================================================
// E2E tests
// ============================================================================

#[tokio::test]
async fn rest_get_json() {
    let addr = spawn_server(0).await;
    let client = test_client(addr, false, true);

    let value: Value = client.rest().json("/").await.expect("GET /");
    assert_eq!(value["status"], "ok");
}

#[tokio::test]
async fn rest_post_echo() {
    let addr = spawn_server(0).await;
    let client = test_client(addr, false, true);

    let resp = client
        .rest()
        .post("/echo")
        .expect("post")
        .body("hello from client")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.into_body().as_ref(), b"hello from client");
}

#[tokio::test]
async fn graphql_query() {
    let addr = spawn_server(0).await;
    let client = test_client(addr, false, true);

    let resp = client
        .graphql()
        .query::<Value, Value>("query GetFoo { foo }", None)
        .await
        .expect("graphql query");

    assert_eq!(resp.data.expect("data").get("foo").expect("foo"), "bar");
    assert!(resp.errors.is_none());
}

#[tokio::test]
async fn bearer_auth_success() {
    let addr = spawn_server(0).await;
    let client = test_client(addr, true, true);

    let value: Value = client.rest().json("/whoami").await.expect("GET /whoami");
    assert_eq!(value["user"], "test");
}

#[tokio::test]
async fn bearer_auth_failure() {
    let addr = spawn_server(0).await;
    let client = test_client(addr, false, true);

    let resp = client
        .rest()
        .get("/whoami")
        .expect("get")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn retry_recovers_from_transient_500() {
    // Server fails twice, then succeeds.
    let addr = spawn_server(2).await;
    let client = test_client(addr, false, false);

    let value: Value = client
        .rest()
        .json("/flaky")
        .await
        .expect("GET /flaky with retry");
    assert_eq!(value["recovered"], true);
}

#[tokio::test]
async fn circuit_breaker_opens_after_failures() {
    // Server always fails.
    let addr = spawn_server(1_000_000).await;
    let client = test_client(addr, false, false);

    // Two calls exhaust retries and trip the breaker.
    let _ = client.rest().get("/flaky").expect("get").send().await;
    let _ = client.rest().get("/flaky").expect("get").send().await;

    // Third call should be fast-failed by the circuit breaker.
    let resp = client
        .rest()
        .get("/flaky")
        .expect("get")
        .send()
        .await
        .expect("send");
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn cache_returns_cached_response() {
    let addr = spawn_server(0).await;
    let client = test_client(addr, false, true);

    let first: Value = client
        .rest()
        .json("/counter")
        .await
        .expect("first /counter");
    let second: Value = client
        .rest()
        .json("/counter")
        .await
        .expect("second /counter");

    // The backend increments on every request, but the client cache should
    // return the first response again.
    assert_eq!(first["count"], 0);
    assert_eq!(second["count"], 0);
}

#[tokio::test]
async fn connectrpc_unary_say() {
    let addr = spawn_server(0).await;
    let client = test_client(addr, true, false);

    let base_uri: http::Uri = format!("http://{addr}").parse().expect("base uri");
    let transport = client.connectrpc(base_uri.clone());
    let config = connectrpc::client::ClientConfig::new(base_uri)
        .with_protocol(connectrpc::Protocol::Connect);
    let eliza = ElizaServiceClient::new(transport, config);

    let resp = eliza
        .say(SayRequest {
            sentence: "hello".to_string(),
            ..Default::default()
        })
        .await
        .expect("say");

    assert!(resp.view().sentence.contains("Hello"));
}

#[tokio::test]
async fn connectrpc_server_streaming_introduce() {
    let addr = spawn_server(0).await;
    let client = test_client(addr, true, false);

    let base_uri: http::Uri = format!("http://{addr}").parse().expect("base uri");
    let transport = client.connectrpc(base_uri.clone());
    let config = connectrpc::client::ClientConfig::new(base_uri)
        .with_protocol(connectrpc::Protocol::Connect);
    let eliza = ElizaServiceClient::new(transport, config);

    let mut stream = eliza
        .introduce(IntroduceRequest {
            name: "Alice".to_string(),
            ..Default::default()
        })
        .await
        .expect("introduce");

    let msg = stream
        .message()
        .await
        .expect("stream message")
        .expect("message");
    assert!(msg.reborrow().sentence.contains("Alice"));
}

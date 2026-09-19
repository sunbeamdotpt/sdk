#![cfg(feature = "g2v-server")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::disallowed_methods)] // test code may unwrap freely (SSO-027/G2V-003)

//! Integration tests for the ReBAC authorization client.
//!
//! These tests spin up a tiny local mock of the authorization backend's HTTP
//! API and exercise `AuthorizationClient` end-to-end.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::{
    Json, Router as AxumRouter,
    extract::Query,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, patch},
};
use serde_json::json;
use tokio::sync::oneshot;

use sdk::g2v::middleware::auth::authorization::{AuthorizationClient, AuthorizationConfig};

// ============================================================================
// Mock authorization backend
// ============================================================================

type StoredTuple = (String, String, String, String);

#[derive(Clone)]
struct MockBackend {
    tuples: Arc<Mutex<Vec<StoredTuple>>>,
}

impl MockBackend {
    fn new() -> Self {
        Self {
            tuples: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn allow(&self, namespace: &str, object: &str, relation: &str, subject_id: &str) {
        self.tuples.lock().unwrap().push((
            namespace.to_string(),
            object.to_string(),
            relation.to_string(),
            subject_id.to_string(),
        ));
    }

    fn is_allowed(&self, namespace: &str, object: &str, relation: &str, subject_id: &str) -> bool {
        self.tuples
            .lock()
            .unwrap()
            .iter()
            .any(|(ns, obj, rel, sub)| {
                ns == namespace && obj == object && rel == relation && sub == subject_id
            })
    }

    fn delete(&self, namespace: &str, object: &str, relation: &str, subject_id: &str) -> bool {
        let mut tuples = self.tuples.lock().unwrap();
        let before = tuples.len();
        tuples.retain(|(ns, obj, rel, sub)| {
            !(ns == namespace && obj == object && rel == relation && sub == subject_id)
        });
        tuples.len() < before
    }
}

async fn check_permission(
    Query(params): Query<HashMap<String, String>>,
    axum::extract::State(state): axum::extract::State<MockBackend>,
) -> impl IntoResponse {
    let allowed = state.is_allowed(
        params.get("namespace").unwrap_or(&String::new()),
        params.get("object").unwrap_or(&String::new()),
        params.get("relation").unwrap_or(&String::new()),
        params.get("subject_id").unwrap_or(&String::new()),
    );

    if allowed {
        (StatusCode::OK, Json(json!({ "allowed": true })))
    } else {
        (StatusCode::FORBIDDEN, Json(json!({ "allowed": false })))
    }
}

#[derive(serde::Deserialize)]
struct PatchPayload {
    action: String,
    relation_tuple: RelationTuple,
}

#[derive(serde::Deserialize)]
struct RelationTuple {
    namespace: String,
    object: String,
    relation: String,
    subject_id: String,
}

async fn patch_tuples(
    axum::extract::State(state): axum::extract::State<MockBackend>,
    Json(payload): Json<Vec<PatchPayload>>,
) -> StatusCode {
    for item in payload {
        if item.action == "insert" {
            state.allow(
                &item.relation_tuple.namespace,
                &item.relation_tuple.object,
                &item.relation_tuple.relation,
                &item.relation_tuple.subject_id,
            );
        }
    }
    StatusCode::OK
}

async fn delete_tuple(
    Query(params): Query<HashMap<String, String>>,
    axum::extract::State(state): axum::extract::State<MockBackend>,
) -> StatusCode {
    state.delete(
        params.get("namespace").unwrap_or(&String::new()),
        params.get("object").unwrap_or(&String::new()),
        params.get("relation").unwrap_or(&String::new()),
        params.get("subject_id").unwrap_or(&String::new()),
    );
    StatusCode::NO_CONTENT
}

async fn expand() -> impl IntoResponse {
    Json(json!({
        "type": "expand",
        "children": []
    }))
}

fn mock_app(state: MockBackend) -> AxumRouter {
    AxumRouter::new()
        .route("/relation-tuples/check", get(check_permission))
        .route(
            "/admin/relation-tuples",
            patch(patch_tuples).delete(delete_tuple),
        )
        .route("/relation-tuples/expand", get(expand))
        .with_state(state)
}

struct MockServer {
    url: String,
    _shutdown: oneshot::Sender<()>,
}

async fn start_mock_server() -> MockServer {
    let state = MockBackend::new();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = mock_app(state);

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await;
    });

    MockServer {
        url: format!("http://{}", addr),
        _shutdown: tx,
    }
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn check_permission_denied_when_tuple_missing() {
    let server = start_mock_server().await;
    let client = AuthorizationClient::new(AuthorizationConfig {
        read_url: server.url.clone(),
        write_url: server.url.clone(),
    })
    .unwrap();

    let allowed = client
        .check_permission("files", "doc-1", "read", "user-1")
        .await
        .unwrap();
    assert!(!allowed);
}

#[tokio::test]
async fn create_and_check_permission() {
    let server = start_mock_server().await;
    let client = AuthorizationClient::new(AuthorizationConfig {
        read_url: server.url.clone(),
        write_url: server.url.clone(),
    })
    .unwrap();

    client
        .create_relation_tuple("files", "doc-1", "read", "user-1")
        .await
        .unwrap();

    let allowed = client
        .check_permission("files", "doc-1", "read", "user-1")
        .await
        .unwrap();
    assert!(allowed);
}

#[tokio::test]
async fn delete_permission_removes_access() {
    let server = start_mock_server().await;
    let client = AuthorizationClient::new(AuthorizationConfig {
        read_url: server.url.clone(),
        write_url: server.url.clone(),
    })
    .unwrap();

    client
        .create_relation_tuple("files", "doc-2", "read", "user-2")
        .await
        .unwrap();
    assert!(
        client
            .check_permission("files", "doc-2", "read", "user-2")
            .await
            .unwrap()
    );

    client
        .delete_relation_tuple("files", "doc-2", "read", "user-2")
        .await
        .unwrap();
    assert!(
        !client
            .check_permission("files", "doc-2", "read", "user-2")
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn expand_returns_json() {
    let server = start_mock_server().await;
    let client = AuthorizationClient::new(AuthorizationConfig {
        read_url: server.url.clone(),
        write_url: server.url.clone(),
    })
    .unwrap();

    let result = client.expand("files", "doc-1", "read").await.unwrap();
    assert_eq!(result["type"], "expand");
}

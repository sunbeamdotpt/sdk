//! Kanban project management — SDK client for the Sunbeam kanban service.
//!
//! The entire kanban surface area is generated from `buf.build/sunbeamdotpt/kanban`
//! and exposed under [`v1`]. [`KanbanClient`] wraps a
//! [`sunbeam_g2v::client::Client`] to provide ready-to-use service clients
//! speaking ConnectRPC.

#![allow(missing_docs)]

connectrpc::include_generated!("kanban/_connectrpc.rs");

pub use crate::kanban::sunbeam::kanban::v1;

use connectrpc::client::ClientConfig;
use std::sync::Arc;
use sunbeam_g2v::client::{
    Client as G2vClient, ClientBuilder, ClientBuilderError, ConnectTransport,
};

/// Errors that can occur when constructing or using a [`KanbanClient`].
#[derive(Debug, thiserror::Error)]
pub enum KanbanClientError {
    /// The supplied base URL could not be parsed.
    #[error("invalid kanban server URL: {0}")]
    InvalidUrl(String),
    /// The underlying HTTP client could not be constructed.
    #[error("failed to build kanban client: {0}")]
    Build(#[from] ClientBuilderError),
}

/// SDK client for the Sunbeam kanban service surface.
///
/// `KanbanClient` is cheap to clone: it holds an [`Arc`] around the configured
/// g2v HTTP client stack.
#[derive(Clone, Debug)]
pub struct KanbanClient {
    client: Arc<G2vClient>,
    base_uri: http::Uri,
    config: ClientConfig,
}

impl KanbanClient {
    /// Create a builder for a kanban client rooted at the given server URL.
    ///
    /// The URL should be the base of the kanban deployment, e.g.
    /// `https://kanban.example.com`.
    pub fn builder(base_url: impl Into<String>) -> ClientBuilder {
        ClientBuilder::new(base_url)
    }

    /// Create an unauthenticated client rooted at the given server URL.
    ///
    /// Convenience for the common `builder(url).build()` +
    /// `new(client, url.parse()?)` pair. Use [`builder`](Self::builder) instead
    /// when authentication or other g2v options are needed.
    pub fn connect(base_url: impl Into<String>) -> Result<Self, KanbanClientError> {
        let base_url = base_url.into();
        let base_uri: http::Uri = base_url
            .parse()
            .map_err(|_| KanbanClientError::InvalidUrl(base_url.clone()))?;
        let client = ClientBuilder::new(base_url).build()?;
        Self::new(client, base_uri)
    }

    /// Create a client from an existing g2v client and base URI.
    ///
    /// # Errors
    ///
    /// Returns [`KanbanClientError::InvalidUrl`] when `base_uri` cannot be used
    /// to build a ConnectRPC client configuration.
    pub fn new(client: G2vClient, base_uri: http::Uri) -> Result<Self, KanbanClientError> {
        let config = ClientConfig::new(base_uri.clone());
        Ok(Self {
            client: Arc::new(client),
            base_uri,
            config,
        })
    }

    /// Return the configured base URI.
    pub fn base_uri(&self) -> &http::Uri {
        &self.base_uri
    }

    /// Return a copy of this client that sends a default header on every
    /// request (e.g. `x-sunbeam-object-id`).
    ///
    /// The header is set as a default on the underlying
    /// [`connectrpc::client::ClientConfig`]; per-call
    /// [`connectrpc::client::CallOptions`] headers with the same name take
    /// precedence over it.
    #[must_use]
    pub fn with_default_header(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> Self {
        self.config = self.config.with_default_header(name.into(), value.into());
        self
    }

    fn transport(&self) -> ConnectTransport {
        self.client.connectrpc(self.base_uri.clone())
    }

    /// Client for board management.
    pub fn boards(&self) -> v1::BoardServiceClient<ConnectTransport> {
        v1::BoardServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for card management.
    pub fn cards(&self) -> v1::CardServiceClient<ConnectTransport> {
        v1::CardServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for project management.
    pub fn projects(&self) -> v1::ProjectServiceClient<ConnectTransport> {
        v1::ProjectServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for aggregated boards.
    pub fn aggregated_boards(&self) -> v1::AggregatedBoardServiceClient<ConnectTransport> {
        v1::AggregatedBoardServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for attachments.
    pub fn attachments(&self) -> v1::AttachmentServiceClient<ConnectTransport> {
        v1::AttachmentServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for GitHub issue links.
    pub fn github_links(&self) -> v1::GithubLinkServiceClient<ConnectTransport> {
        v1::GithubLinkServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for labels.
    pub fn labels(&self) -> v1::LabelServiceClient<ConnectTransport> {
        v1::LabelServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for milestones.
    pub fn milestones(&self) -> v1::MilestoneServiceClient<ConnectTransport> {
        v1::MilestoneServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for public boards.
    pub fn public_boards(&self) -> v1::PublicBoardServiceClient<ConnectTransport> {
        v1::PublicBoardServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for search.
    pub fn search(&self) -> v1::SearchServiceClient<ConnectTransport> {
        v1::SearchServiceClient::new(self.transport(), self.config.clone())
    }

    /// Client for board and card templates.
    pub fn templates(&self) -> v1::TemplatesServiceClient<ConnectTransport> {
        v1::TemplatesServiceClient::new(self.transport(), self.config.clone())
    }
}

/// Re-exports of the crates that appear in the generated kanban API surface.
///
/// The generated clients expose types from `connectrpc`, `buffa`,
/// `buffa-types`, and `sunbeam-g2v` (e.g. `CallOptions`, `ConnectError`,
/// `MessageField`, `FieldMask`, `BearerToken`). Name them through this
/// prelude instead of adding direct dependencies so the versions always match
/// the ones the SDK was compiled against.
pub mod prelude {
    pub use super::{KanbanClient, KanbanClientError, v1};
    pub use buffa;
    pub use buffa_types;
    pub use connectrpc;
    pub use sunbeam_g2v;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kanban_client_builder_creates_g2v_builder() {
        let builder = KanbanClient::builder("https://kanban.example.com");
        let _ = builder;
    }

    #[test]
    fn test_kanban_client_new_roundtrip() {
        let g2v = KanbanClient::builder("https://kanban.example.com")
            .build()
            .unwrap();
        let client = KanbanClient::new(g2v, "https://kanban.example.com".parse().unwrap()).unwrap();
        assert_eq!(client.base_uri().to_string(), "https://kanban.example.com/");
    }

    #[test]
    fn test_kanban_client_connect_roundtrip() {
        let client = KanbanClient::connect("https://kanban.example.com").unwrap();
        assert_eq!(client.base_uri().to_string(), "https://kanban.example.com/");
    }

    #[test]
    fn test_kanban_client_connect_rejects_invalid_url() {
        let err = KanbanClient::connect("not a url").unwrap_err();
        assert!(matches!(err, KanbanClientError::InvalidUrl(_)));
    }

    // SDK-012: the Assignee proto gained `email = 4` so clients can fall back
    // to it when display_name is empty (liminal LIMINAL-034).
    #[test]
    fn test_assignee_carries_email_field() {
        let assignee = v1::Assignee {
            subject: "user:01KWF0KYZ0FRNR23ZXAWJZV43T".into(),
            email: "user@example.com".into(),
            ..Default::default()
        };
        let json = serde_json::to_value(&assignee).unwrap();
        assert_eq!(json["email"], "user@example.com");
        let decoded: v1::Assignee = serde_json::from_value(json).unwrap();
        assert_eq!(decoded.email, "user@example.com");
        assert_eq!(decoded.subject, "user:01KWF0KYZ0FRNR23ZXAWJZV43T");
    }

    // SDK-012: the email survives the real client decode path (ConnectRPC
    // unary, proto codec — the KanbanClient default).
    #[tokio::test]
    async fn test_get_card_decodes_assignee_email() {
        use buffa::Message as _;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        let response = v1::GetCardResponse {
            card: buffa::MessageField::some(v1::Card {
                id: "01KYTNTSQ0VN4CYD438TA38W9S".into(),
                assignees: vec![v1::Assignee {
                    subject: "user:01KWF0KYZ0FRNR23ZXAWJZV43T".into(),
                    email: "user@example.com".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            ..Default::default()
        };
        Mock::given(method("POST"))
            .and(path("/sunbeam.kanban.v1.CardService/GetCard"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "application/proto")
                    .set_body_bytes(response.encode_to_bytes()),
            )
            .mount(&server)
            .await;

        let client = KanbanClient::connect(server.uri()).unwrap();
        let card = client
            .cards()
            .get_card(v1::GetCardRequest {
                card_id: "01KYTNTSQ0VN4CYD438TA38W9S".into(),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_owned()
            .card
            .into_option()
            .unwrap();

        assert_eq!(card.assignees.len(), 1);
        assert_eq!(card.assignees[0].email, "user@example.com");
        assert!(card.assignees[0].display_name.is_empty());
    }

    #[test]
    fn test_kanban_client_with_default_header() {
        let client = KanbanClient::connect("https://kanban.example.com")
            .unwrap()
            .with_default_header("x-sunbeam-object-id", "board-123");
        assert_eq!(
            client.config.default_headers().get("x-sunbeam-object-id"),
            Some(&http::HeaderValue::from_static("board-123"))
        );
    }
}

/// End-to-end tests against the real kanban server image, orchestrated by
/// [`crate::testing::Kanban`]. Requires the `auth` feature (the orchestrator
/// provisions credentials via the IAM client).
#[cfg(all(test, feature = "testing", feature = "auth"))]
mod stack_tests {
    use super::*;
    use crate::auth::{AuthClient, v1 as iam};
    use crate::testing;

    use buffa::MessageField;
    use buffa_types::google::protobuf::value::Kind;
    use buffa_types::google::protobuf::{Struct, Value};
    use connectrpc::client::CallOptions;
    use sunbeam_g2v::client::BearerToken;

    /// Kanban image with server-side Assignee email population (KANBAN-024 /
    /// KANBAN-035).
    const KANBAN_IMAGE_TAG: &str = "v2026.07.12";
    /// Pinned sso-gateway image: `latest` (2026-07-30) crashes on fresh-OpenFGA
    /// bootstrap — "type 'entitlements' not found" on the entitlement tuple
    /// write — while v2026.07.21 is known to stay up.
    const GATEWAY_IMAGE_TAG: &str = "v2026.07.21";
    /// Base identity schema (`traits.email`, required) matching the one baked
    /// into the sso-gateway test harness's Kratos config.
    const BASE_IDENTITY_SCHEMA: &str = r#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://schemas.sunbeam.pt/base-identity.json",
  "title": "Base Identity",
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "traits": {
      "type": "object",
      "additionalProperties": false,
      "required": ["email"],
      "properties": {
        "email": {
          "type": "string",
          "format": "email",
          "title": "Email",
          "maxLength": 320,
          "ory.sh/kratos": {
            "credentials": {
              "password": { "identifier": true },
              "webauthn": { "identifier": true },
              "totp": { "account_name": true },
              "code": { "identifier": true, "via": "email" },
              "passkey": { "display_name": true }
            },
            "recovery": { "via": "email" },
            "verification": { "via": "email" }
          }
        }
      }
    }
  }
}"#;

    fn string_value(s: &str) -> Value {
        Value {
            kind: Some(Kind::StringValue(s.to_string())),
            ..Default::default()
        }
    }

    /// Fetch a service-app token. The app is provisioned with
    /// `client_secret_post`, so credentials go in the form body.
    async fn service_token(stack: &testing::KanbanHandle) -> String {
        let resp: serde_json::Value = reqwest::Client::new()
            .post(format!("{}/oauth2/token", stack.sso_gateway_endpoint()))
            .form(&[
                ("grant_type", "client_credentials"),
                ("client_id", stack.client_id()),
                ("client_secret", stack.client_secret()),
                ("scope", "permission:admin tenant:admin identity:admin"),
            ])
            .send()
            .await
            .expect("token request should send")
            .error_for_status()
            .expect("token request should succeed")
            .json()
            .await
            .expect("token response should be JSON");
        resp["access_token"]
            .as_str()
            .expect("token response should carry access_token")
            .to_owned()
    }

    // SDK-015: Assignee.email populated end-to-end — create a directory
    // identity with an email, assign a card to them through the real server,
    // and read the email back via GetCard.
    #[tokio::test]
    #[ignore = "requires Docker + pre-built kanban/sso-gateway images; pulls seven containers"]
    async fn get_card_returns_assignee_email() {
        let stack = testing::Kanban::new()
            .with_tag(KANBAN_IMAGE_TAG)
            .with_gateway_image(testing::SsoGateway::DEFAULT_IMAGE_NAME, GATEWAY_IMAGE_TAG)
            .start()
            .await
            .expect("kanban stack should start");
        let token = service_token(&stack).await;

        // Directory identity carrying an email, inside the kanban-test tenant.
        let email = "kanban-assignee@example.com";
        let g2v = AuthClient::builder(stack.sso_gateway_endpoint())
            .auth(BearerToken::new(token.clone()))
            .build()
            .expect("auth g2v client should build");
        let auth = AuthClient::new(
            g2v,
            stack
                .sso_gateway_endpoint()
                .parse()
                .expect("gateway endpoint should parse"),
        )
        .expect("auth client should build")
        .with_tenant(stack.tenant_id());
        // The kanban-test tenant starts with an empty schema registry; seed
        // the base identity schema (the one baked into the harness's Kratos
        // config) so CreateIdentity can reference it.
        let schema_struct: Struct = serde_json::from_str(BASE_IDENTITY_SCHEMA)
            .expect("base identity schema should parse into a protobuf Struct");
        auth.identity()
            .create_identity_schema(iam::CreateIdentitySchemaRequest {
                schema_id: "default".to_owned(),
                schema_json: MessageField::some(schema_struct),
                ..Default::default()
            })
            .await
            .expect("CreateIdentitySchema should succeed");
        let traits = Struct {
            fields: [("email".to_owned(), string_value(email))]
                .into_iter()
                .collect(),
            ..Default::default()
        };
        let identity_id = auth
            .identity()
            .create_identity(iam::CreateIdentityRequest {
                schema_id: "default".to_owned(),
                traits: MessageField::some(traits),
                password: "Kanban-Test-Password-42".to_owned(),
                ..Default::default()
            })
            .await
            .expect("CreateIdentity should succeed")
            .view()
            .id
            .to_string();

        // Kanban client authenticated with the same service token.
        let g2v = KanbanClient::builder(stack.endpoint())
            .auth(BearerToken::new(token))
            .build()
            .expect("kanban g2v client should build");
        let kanban = KanbanClient::new(
            g2v,
            stack.endpoint().parse().expect("endpoint should parse"),
        )
        .expect("kanban client should build");
        let opts =
            |object_id: &str| CallOptions::default().with_header("x-sunbeam-object-id", object_id);

        let project = kanban
            .projects()
            .create_project(v1::CreateProjectRequest {
                name: "Assignee Email Test".to_owned(),
                prefix: "AET".to_owned(),
                ..Default::default()
            })
            .await
            .expect("CreateProject should succeed")
            .into_owned()
            .project
            .into_option()
            .expect("response should carry the project");

        let board = kanban
            .boards()
            .create_board_with_options(
                v1::CreateBoardRequest {
                    project_id: project.id.clone(),
                    name: "assignee-email".to_owned(),
                    visibility: v1::BoardVisibility::Private.into(),
                    ..Default::default()
                },
                opts(&project.id),
            )
            .await
            .expect("CreateBoard should succeed")
            .into_owned()
            .board
            .into_option()
            .expect("response should carry the board");

        let detail = kanban
            .boards()
            .get_board_with_options(
                v1::GetBoardRequest {
                    board_id: board.id.clone(),
                    ..Default::default()
                },
                opts(&board.id),
            )
            .await
            .expect("GetBoard should succeed")
            .into_owned()
            .detail
            .into_option()
            .expect("response should carry the board detail");
        let column_id = match detail.columns.first() {
            Some(column) => column.id.clone(),
            None => {
                kanban
                    .boards()
                    .add_column_with_options(
                        v1::AddColumnRequest {
                            board_id: board.id.clone(),
                            title: "Todo".to_owned(),
                            ..Default::default()
                        },
                        opts(&board.id),
                    )
                    .await
                    .expect("AddColumn should succeed")
                    .into_owned()
                    .column
                    .into_option()
                    .expect("response should carry the column")
                    .id
            }
        };

        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after the epoch")
            .as_nanos();
        let card = kanban
            .cards()
            .create_card_with_options(
                v1::CreateCardRequest {
                    board_id: board.id.clone(),
                    column_id,
                    title: "assign me".to_owned(),
                    priority: v1::CardPriority::Medium.into(),
                    idempotency_key: format!("sdk-015-{nonce}"),
                    ..Default::default()
                },
                opts(&board.id),
            )
            .await
            .expect("CreateCard should succeed")
            .into_owned()
            .card
            .into_option()
            .expect("response should carry the card");

        // The server stores subjects as `user:<identity-id>`; the identity id
        // alone is accepted too, so try the bare id first and fall back.
        let subject = format!("user:{identity_id}");

        // AssignCard denies assignees without board access: grant project
        // membership (edit) first, mirroring how real clients assign.
        kanban
            .projects()
            .add_member_with_options(
                v1::AddMemberRequest {
                    project_id: project.id.clone(),
                    subject: subject.clone(),
                    relation: "editor".to_owned(),
                    ..Default::default()
                },
                opts(&project.id),
            )
            .await
            .expect("AddMember should succeed");

        // AssignCard checks "edit" on the KanbanCard object itself (server
        // log: permission_dispatch namespace="KanbanCard"), and the canonical
        // subject form is `user:<identity-id>`.
        kanban
            .cards()
            .assign_card_with_options(
                v1::AssignCardRequest {
                    card_id: card.id.clone(),
                    subject: subject.clone(),
                    ..Default::default()
                },
                opts(&card.id),
            )
            .await
            .expect("AssignCard should succeed");

        let fetched = kanban
            .cards()
            .get_card_with_options(
                v1::GetCardRequest {
                    card_id: card.id.clone(),
                    ..Default::default()
                },
                opts(&card.id),
            )
            .await
            .expect("GetCard should succeed")
            .into_owned()
            .card
            .into_option()
            .expect("response should carry the card");

        let assignee = fetched
            .assignees
            .iter()
            .find(|a| a.subject == subject || a.subject == identity_id)
            .expect("card should carry the assignee");
        assert_eq!(assignee.email, email);

        stack.shutdown().await;
    }
}

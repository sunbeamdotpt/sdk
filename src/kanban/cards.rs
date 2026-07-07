//! Kanban card commands.

use crate::error::{Result, ResultExt};
use crate::kanban::client::{self, CardServiceClient, request_with_object_id};
use crate::kanban::new_idempotency_key;
use crate::logger::Logger;
use async_trait::async_trait;
use clap::Subcommand;
use serde::Serialize;

/// Card actions.
#[derive(Debug, Subcommand)]
pub enum CardAction {
    /// List cards.
    List {
        /// Board ID or name.
        #[arg(short, long)]
        board: String,
        /// Column ID.
        #[arg(short, long)]
        column: Option<String>,
    },
    /// Get a card.
    Get {
        /// Card ID, title, or ref.
        card_id: String,
    },
    /// Create a card.
    Create {
        /// Board ID or name.
        #[arg(short, long)]
        board: String,
        /// Column ID.
        #[arg(short, long)]
        column: Option<String>,
        /// Title.
        #[arg(short, long)]
        title: String,
        /// Description.
        #[arg(short, long)]
        description: Option<String>,
        /// Priority.
        #[arg(short, long, value_enum)]
        priority: Option<PriorityArg>,
    },
    /// Update a card.
    Update {
        /// Card ID, title, or ref.
        card_id: String,
        /// New title.
        #[arg(short, long)]
        title: Option<String>,
        /// New description.
        #[arg(short, long)]
        description: Option<String>,
        /// New priority.
        #[arg(short, long, value_enum)]
        priority: Option<PriorityArg>,
    },
    /// Move a card.
    Move {
        /// Card ID, title, or ref.
        card_id: String,
        /// Destination column ID.
        #[arg(short, long)]
        column: String,
        /// Position within the column.
        #[arg(short, long)]
        position: Option<i32>,
    },
    /// Delete a card.
    Delete {
        /// Card ID, title, or ref.
        card_id: String,
    },
    /// Dependency management.
    Dependency {
        /// Dependency subcommand to run.
        #[command(subcommand)]
        action: DependencyAction,
    },
}

/// Priority values matching `CardPriority`.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum PriorityArg {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
    /// Urgent.
    Urgent,
}

impl PriorityArg {
    /// Convert to the generated proto enum value.
    fn to_proto(self) -> client::CardPriority {
        match self {
            PriorityArg::Low => client::CardPriority::Low,
            PriorityArg::Medium => client::CardPriority::Medium,
            PriorityArg::High => client::CardPriority::High,
            PriorityArg::Urgent => client::CardPriority::Urgent,
        }
    }
}

/// Card dependency actions.
#[derive(Debug, Subcommand)]
pub enum DependencyAction {
    /// Add a dependency.
    Add {
        /// Board ID or name.
        #[arg(short, long)]
        board: String,
        /// Card ID, title, or ref.
        card_id: String,
        /// Card this card depends on (ID, title, or ref).
        depends_on: String,
    },
    /// Remove a dependency.
    Remove {
        /// Board ID or name.
        #[arg(short, long)]
        board: String,
        /// Card ID, title, or ref.
        card_id: String,
        /// Dependency card ID, title, or ref.
        depends_on: String,
    },
}

/// Serializable card summary for list views.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardOut {
    /// Card ID.
    pub id: String,
    /// Human-readable reference.
    pub r#ref: String,
    /// Board ID.
    pub board_id: String,
    /// Column ID.
    pub column_id: String,
    /// Card title.
    pub title: String,
    /// Priority label.
    pub priority: String,
    /// Whether the card is blocked.
    pub blocked: bool,
    /// Position within the column.
    pub position: i32,
    /// Number of comments.
    pub comments_count: i32,
    /// Number of attachments.
    pub attachments_count: i32,
}

impl CardOut {
    fn from_proto(card: client::Card) -> Self {
        Self {
            id: card.id,
            r#ref: card.r#ref,
            board_id: card.board_id,
            column_id: card.column_id,
            title: card.title,
            priority: client::CardPriority::try_from(card.priority)
                .map(|p| p.as_str_name().to_string())
                .unwrap_or_default(),
            blocked: card.blocked,
            position: card.position,
            comments_count: card.comments_count,
            attachments_count: card.attachments_count,
        }
    }
}

/// Convert a prost Timestamp to an RFC3339 string.
fn fmt_ts(ts: Option<&prost_types::Timestamp>) -> String {
    ts.and_then(|t| chrono::DateTime::from_timestamp(t.seconds, t.nanos as u32))
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_default()
}

/// Serializable card detail for get/create/update/move/dependency views.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardDetailOut {
    /// Card ID.
    pub id: String,
    /// Human-readable reference.
    pub r#ref: String,
    /// Project ID.
    pub project_id: String,
    /// Board ID.
    pub board_id: String,
    /// Column ID.
    pub column_id: String,
    /// Title.
    pub title: String,
    /// Description.
    pub description: String,
    /// Priority label.
    pub priority: String,
    /// Urgency label.
    pub urgency: String,
    /// Due date.
    pub due: String,
    /// Completed timestamp.
    pub completed_at: String,
    /// Blocked flag.
    pub blocked: bool,
    /// Cover.
    pub cover: String,
    /// Milestone ID.
    pub milestone_id: String,
    /// Position.
    pub position: i32,
    /// Labels.
    pub labels: Vec<serde_json::Value>,
    /// Assignees.
    pub assignees: Vec<serde_json::Value>,
    /// Checklist items.
    pub checklist: Vec<serde_json::Value>,
    /// GitHub links.
    pub github_links: Vec<serde_json::Value>,
    /// Comments count.
    pub comments_count: i32,
    /// Attachments count.
    pub attachments_count: i32,
    /// Revision.
    pub revision: u64,
    /// Created timestamp.
    pub created_at: String,
    /// Updated timestamp.
    pub updated_at: String,
    /// Cards this card depends on.
    pub depends_on_card_ids: Vec<String>,
    /// Cards that depend on this card.
    pub dependent_card_ids: Vec<String>,
}

impl CardDetailOut {
    /// Build a detail view from a proto card.
    pub fn from_proto(card: client::Card) -> Self {
        Self {
            id: card.id,
            r#ref: card.r#ref,
            project_id: card.project_id,
            board_id: card.board_id,
            column_id: card.column_id,
            title: card.title,
            description: card.description,
            priority: client::CardPriority::try_from(card.priority)
                .map(|p| p.as_str_name().to_string())
                .unwrap_or_default(),
            urgency: client::CardUrgency::try_from(card.urgency)
                .map(|u| u.as_str_name().to_string())
                .unwrap_or_default(),
            due: fmt_ts(card.due.as_ref()),
            completed_at: fmt_ts(card.completed_at.as_ref()),
            blocked: card.blocked,
            cover: card.cover,
            milestone_id: card.milestone_id,
            position: card.position,
            labels: card
                .labels
                .into_iter()
                .map(|l| {
                    serde_json::json!({
                        "id": l.id,
                        "name": l.name,
                        "style": l.style,
                    })
                })
                .collect(),
            assignees: card
                .assignees
                .into_iter()
                .map(|a| {
                    serde_json::json!({
                        "subject": a.subject,
                        "display_name": a.display_name,
                        "avatar_url": a.avatar_url,
                        "email": null,
                    })
                })
                .collect(),
            checklist: card
                .checklist
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "text": c.text,
                        "done": c.done,
                        "position": c.position,
                    })
                })
                .collect(),
            github_links: card
                .github_links
                .into_iter()
                .map(|g| {
                    serde_json::json!({
                        "id": g.id,
                        "repo": g.repo,
                        "number": g.number,
                        "state": g.state,
                        "merged": g.merged,
                    })
                })
                .collect(),
            comments_count: card.comments_count,
            attachments_count: card.attachments_count,
            revision: card.revision,
            created_at: fmt_ts(card.created_at.as_ref()),
            updated_at: fmt_ts(card.updated_at.as_ref()),
            depends_on_card_ids: card.depends_on_card_ids,
            dependent_card_ids: card.dependent_card_ids,
        }
    }
}

/// Card deletion confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardDeleteOut {
    /// Whether the deletion succeeded.
    pub deleted: bool,
    /// Deleted card ID.
    pub card_id: String,
}

/// Result of running a card command.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum CardOutput {
    /// List of cards.
    List(Vec<CardOut>),
    /// Single card detail.
    Detail(CardDetailOut),
    /// Deletion confirmation.
    Deleted(CardDeleteOut),
}

/// Trait abstracting the Kanban card service for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait CardService {
    /// List cards on a board.
    async fn list_cards_by_board(
        &mut self,
        req: tonic::Request<client::ListCardsByBoardRequest>,
    ) -> Result<client::ListCardsByBoardResponse>;
    /// Get a single card.
    async fn get_card(
        &mut self,
        req: tonic::Request<client::GetCardRequest>,
    ) -> Result<client::Card>;
    /// Create a new card.
    async fn create_card(
        &mut self,
        req: tonic::Request<client::CreateCardRequest>,
    ) -> Result<client::Card>;
    /// Update an existing card.
    async fn update_card(
        &mut self,
        req: tonic::Request<client::UpdateCardRequest>,
    ) -> Result<client::Card>;
    /// Move a card to another column/position.
    async fn move_card(
        &mut self,
        req: tonic::Request<client::MoveCardRequest>,
    ) -> Result<client::Card>;
    /// Delete a card.
    async fn delete_card(&mut self, req: tonic::Request<client::DeleteCardRequest>) -> Result<()>;
    /// Add a dependency between two cards.
    async fn add_card_dependency(
        &mut self,
        req: tonic::Request<client::CardDependencyRequest>,
    ) -> Result<client::Card>;
    /// Remove a dependency between two cards.
    async fn remove_card_dependency(
        &mut self,
        req: tonic::Request<client::CardDependencyRequest>,
    ) -> Result<client::Card>;
}

/// Wrapper around the generated Tonic card client.
#[derive(Debug)]
pub struct CardServiceClientWrapper {
    inner: CardServiceClient<client::AuthChannel>,
}

impl CardServiceClientWrapper {
    /// Build a wrapper from an authenticated channel.
    pub fn new(inner: CardServiceClient<client::AuthChannel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl CardService for CardServiceClientWrapper {
    async fn list_cards_by_board(
        &mut self,
        req: tonic::Request<client::ListCardsByBoardRequest>,
    ) -> Result<client::ListCardsByBoardResponse> {
        let resp = self.inner.list_cards_by_board(req).await?;
        Ok(resp.into_inner())
    }

    async fn get_card(
        &mut self,
        req: tonic::Request<client::GetCardRequest>,
    ) -> Result<client::Card> {
        let resp = self.inner.get_card(req).await?;
        Ok(resp.into_inner())
    }

    async fn create_card(
        &mut self,
        req: tonic::Request<client::CreateCardRequest>,
    ) -> Result<client::Card> {
        let resp = self.inner.create_card(req).await?;
        Ok(resp.into_inner())
    }

    async fn update_card(
        &mut self,
        req: tonic::Request<client::UpdateCardRequest>,
    ) -> Result<client::Card> {
        let resp = self.inner.update_card(req).await?;
        Ok(resp.into_inner())
    }

    async fn move_card(
        &mut self,
        req: tonic::Request<client::MoveCardRequest>,
    ) -> Result<client::Card> {
        let resp = self.inner.move_card(req).await?;
        Ok(resp.into_inner())
    }

    async fn delete_card(&mut self, req: tonic::Request<client::DeleteCardRequest>) -> Result<()> {
        self.inner.delete_card(req).await?;
        Ok(())
    }

    async fn add_card_dependency(
        &mut self,
        req: tonic::Request<client::CardDependencyRequest>,
    ) -> Result<client::Card> {
        let resp = self.inner.add_card_dependency(req).await?;
        Ok(resp.into_inner())
    }

    async fn remove_card_dependency(
        &mut self,
        req: tonic::Request<client::CardDependencyRequest>,
    ) -> Result<client::Card> {
        let resp = self.inner.remove_card_dependency(req).await?;
        Ok(resp.into_inner())
    }
}

/// Build a card-service client wrapper for the given server and token.
pub async fn build_client(
    logger: &Logger,
    server: &str,
    token: &str,
) -> Result<CardServiceClientWrapper> {
    let channel = client::build(logger, server, token).await?;
    Ok(CardServiceClientWrapper::new(CardServiceClient::new(
        channel,
    )))
}

/// Resolve assignee subjects to email addresses through the Kratos admin API.
///
/// Configure the endpoint with the `kratos-admin-url` field in the active
/// context. Missing or unresolvable subjects are left as `null` rather than
/// failing the whole command.
async fn resolve_assignee_emails(assignees: &mut [serde_json::Value]) -> Result<()> {
    let subjects: Vec<&str> = assignees
        .iter()
        .filter_map(|a| {
            a.get("subject")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
        })
        .collect();

    if subjects.is_empty() {
        return Ok(());
    }

    let email_map = crate::auth::resolve_emails_for_subjects(&subjects).await?;

    for assignee in assignees.iter_mut() {
        let Some(subject) = assignee
            .get("subject")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if let Some(email) = email_map.get(subject)
            && let Some(obj) = assignee.as_object_mut()
        {
            obj.insert(
                "email".to_string(),
                serde_json::Value::String(email.clone()),
            );
        }
    }
    Ok(())
}

/// Run a card command and return the result data.
pub async fn run(cmd: CardAction, client: &mut dyn CardService) -> Result<CardOutput> {
    match cmd {
        CardAction::List { board, column } => {
            let req = client::ListCardsByBoardRequest {
                board_id: board.clone(),
                column_id: column.unwrap_or_default(),
                cursor: String::new(),
                limit: 0,
            };
            let resp = client
                .list_cards_by_board(request_with_object_id(req, &board)?)
                .await
                .with_ctx(|| "list cards".to_string())?;
            let cards: Vec<_> = resp.cards.into_iter().map(CardOut::from_proto).collect();
            Ok(CardOutput::List(cards))
        }
        CardAction::Get { card_id } => {
            let req = client::GetCardRequest {
                card_id: card_id.clone(),
            };
            let resp = client
                .get_card(request_with_object_id(req, &card_id)?)
                .await
                .with_ctx(|| format!("get card {card_id}"))?;
            let mut detail = CardDetailOut::from_proto(resp);
            resolve_assignee_emails(&mut detail.assignees).await?;
            Ok(CardOutput::Detail(detail))
        }
        CardAction::Create {
            board,
            column,
            title,
            description,
            priority,
        } => {
            let req = client::CreateCardRequest {
                board_id: board.clone(),
                column_id: column.unwrap_or_default(),
                title,
                description: description.unwrap_or_default(),
                priority: priority.map(|p| p.to_proto() as i32).unwrap_or_default(),
                due: None,
                milestone_id: String::new(),
                position: 0,
                idempotency_key: new_idempotency_key(),
                urgency: client::CardUrgency::Medium as i32,
            };
            let resp = client
                .create_card(request_with_object_id(req, &board)?)
                .await
                .with_ctx(|| format!("create card on board {board}"))?;
            let mut detail = CardDetailOut::from_proto(resp);
            resolve_assignee_emails(&mut detail.assignees).await?;
            Ok(CardOutput::Detail(detail))
        }
        CardAction::Update {
            card_id,
            title,
            description,
            priority,
        } => {
            let mut update_card = client::Card {
                id: card_id.clone(),
                ..Default::default()
            };
            let mut paths: Vec<String> = Vec::new();
            if let Some(t) = title {
                update_card.title = t;
                paths.push("title".to_string());
            }
            if let Some(d) = description {
                update_card.description = d;
                paths.push("description".to_string());
            }
            if let Some(p) = priority {
                update_card.priority = p.to_proto() as i32;
                paths.push("priority".to_string());
            }
            let req = client::UpdateCardRequest {
                card_id: card_id.clone(),
                card: Some(update_card),
                update_mask: Some(prost_types::FieldMask { paths }),
                idempotency_key: new_idempotency_key(),
            };
            let resp = client
                .update_card(request_with_object_id(req, &card_id)?)
                .await
                .with_ctx(|| format!("update card {card_id}"))?;
            let mut detail = CardDetailOut::from_proto(resp);
            resolve_assignee_emails(&mut detail.assignees).await?;
            Ok(CardOutput::Detail(detail))
        }
        CardAction::Move {
            card_id,
            column,
            position,
        } => {
            let req = client::MoveCardRequest {
                card_id: card_id.clone(),
                to_column_id: column,
                to_position: position.unwrap_or_default(),
                idempotency_key: new_idempotency_key(),
            };
            let resp = client
                .move_card(request_with_object_id(req, &card_id)?)
                .await
                .with_ctx(|| format!("move card {card_id}"))?;
            let mut detail = CardDetailOut::from_proto(resp);
            resolve_assignee_emails(&mut detail.assignees).await?;
            Ok(CardOutput::Detail(detail))
        }
        CardAction::Delete { card_id } => {
            let req = client::DeleteCardRequest {
                card_id: card_id.clone(),
            };
            client
                .delete_card(request_with_object_id(req, &card_id)?)
                .await
                .with_ctx(|| format!("delete card {card_id}"))?;
            Ok(CardOutput::Deleted(CardDeleteOut {
                deleted: true,
                card_id,
            }))
        }
        CardAction::Dependency { action } => match action {
            DependencyAction::Add {
                board,
                card_id,
                depends_on,
            } => {
                let req = client::CardDependencyRequest {
                    card_id: card_id.clone(),
                    depends_on_card_id: depends_on.clone(),
                    idempotency_key: new_idempotency_key(),
                };
                let resp = client
                    .add_card_dependency(request_with_object_id(req, &board)?)
                    .await
                    .with_ctx(|| format!("add dependency {depends_on} to card {card_id}"))?;
                let mut detail = CardDetailOut::from_proto(resp);
                resolve_assignee_emails(&mut detail.assignees).await?;
                Ok(CardOutput::Detail(detail))
            }
            DependencyAction::Remove {
                board,
                card_id,
                depends_on,
            } => {
                let req = client::CardDependencyRequest {
                    card_id: card_id.clone(),
                    depends_on_card_id: depends_on.clone(),
                    idempotency_key: new_idempotency_key(),
                };
                let resp = client
                    .remove_card_dependency(request_with_object_id(req, &board)?)
                    .await
                    .with_ctx(|| format!("remove dependency {depends_on} from card {card_id}"))?;
                let mut detail = CardDetailOut::from_proto(resp);
                resolve_assignee_emails(&mut detail.assignees).await?;
                Ok(CardOutput::Detail(detail))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_card() -> client::Card {
        client::Card {
            id: "card_1".into(),
            r#ref: "PROJ-1".into(),
            project_id: "proj_1".into(),
            board_id: "board_1".into(),
            column_id: "col_1".into(),
            title: "Fix frontend crash".into(),
            description: "It is broken".into(),
            priority: client::CardPriority::High as i32,
            due: None,
            completed_at: None,
            blocked: false,
            cover: "gradient.amber".into(),
            milestone_id: String::new(),
            position: 1,
            labels: vec![],
            assignees: vec![],
            checklist: vec![],
            github_links: vec![],
            comments_count: 2,
            attachments_count: 1,
            revision: 3,
            created_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            updated_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_100,
                nanos: 0,
            }),
            urgency: client::CardUrgency::Medium as i32,
            depends_on_card_ids: vec![],
            dependent_card_ids: vec![],
        }
    }

    #[tokio::test]
    async fn list_cards_returns_list() {
        let mut mock = MockCardService::new();
        mock.expect_list_cards_by_board()
            .withf(|req| {
                req.get_ref().board_id == "board_1"
                    && req.get_ref().column_id.is_empty()
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("board_1")
            })
            .times(1)
            .returning(|_| {
                Ok(client::ListCardsByBoardResponse {
                    cards: vec![sample_card()],
                    next_cursor: String::new(),
                })
            });

        let out = run(
            CardAction::List {
                board: "board_1".into(),
                column: None,
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            CardOutput::List(cards) => {
                assert_eq!(cards.len(), 1);
                assert_eq!(cards[0].id, "card_1");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_card_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_get_card()
            .withf(|req| {
                req.get_ref().card_id == "card_1"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("card_1")
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Get {
                card_id: "card_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[tokio::test]
    async fn create_card_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_create_card()
            .withf(|req| {
                let r = req.get_ref();
                r.board_id == "board_1"
                    && r.column_id == "col_1"
                    && r.title == "New card"
                    && r.description == "Details"
                    && r.priority == client::CardPriority::High as i32
                    && r.position == 0
                    && r.urgency == client::CardUrgency::Medium as i32
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Create {
                board: "board_1".into(),
                column: Some("col_1".into()),
                title: "New card".into(),
                description: Some("Details".into()),
                priority: Some(PriorityArg::High),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[tokio::test]
    async fn update_card_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_update_card()
            .withf(|req| {
                let r = req.get_ref();
                r.card_id == "card_1"
                    && r.card.as_ref().map(|c| c.id.clone()).unwrap_or_default() == "card_1"
                    && r.update_mask
                        .as_ref()
                        .map(|m| m.paths.clone())
                        .unwrap_or_default()
                        == vec!["title".to_string()]
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Update {
                card_id: "card_1".into(),
                title: Some("Updated title".into()),
                description: None,
                priority: None,
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[tokio::test]
    async fn move_card_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_move_card()
            .withf(|req| {
                let r = req.get_ref();
                r.card_id == "card_1" && r.to_column_id == "col_2" && r.to_position == 2
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Move {
                card_id: "card_1".into(),
                column: "col_2".into(),
                position: Some(2),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[tokio::test]
    async fn delete_card_succeeds() {
        let mut mock = MockCardService::new();
        mock.expect_delete_card()
            .withf(|req| req.get_ref().card_id == "card_1")
            .times(1)
            .returning(|_| Ok(()));

        let out = run(
            CardAction::Delete {
                card_id: "card_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            CardOutput::Deleted(d) => {
                assert!(d.deleted);
                assert_eq!(d.card_id, "card_1");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn add_dependency_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_add_card_dependency()
            .withf(|req| {
                let r = req.get_ref();
                r.card_id == "card_1"
                    && r.depends_on_card_id == "card_2"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("board_1")
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Dependency {
                action: DependencyAction::Add {
                    board: "board_1".into(),
                    card_id: "card_1".into(),
                    depends_on: "card_2".into(),
                },
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[tokio::test]
    async fn remove_dependency_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_remove_card_dependency()
            .withf(|req| {
                let r = req.get_ref();
                r.card_id == "card_1"
                    && r.depends_on_card_id == "card_2"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("board_1")
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Dependency {
                action: DependencyAction::Remove {
                    board: "board_1".into(),
                    card_id: "card_1".into(),
                    depends_on: "card_2".into(),
                },
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[tokio::test]
    async fn list_cards_returns_data_for_column() {
        let mut mock = MockCardService::new();
        mock.expect_list_cards_by_board().times(1).returning(|_| {
            Ok(client::ListCardsByBoardResponse {
                cards: vec![sample_card()],
                next_cursor: String::new(),
            })
        });

        let out = run(
            CardAction::List {
                board: "board_1".into(),
                column: Some("col_1".into()),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::List(_)));
    }

    #[tokio::test]
    async fn create_card_with_defaults_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_create_card()
            .withf(|req| {
                let r = req.get_ref();
                r.board_id == "board_1"
                    && r.title == "T"
                    && r.description.is_empty()
                    && r.priority == 0
                    && r.position == 0
                    && r.urgency == client::CardUrgency::Medium as i32
                    && !r.idempotency_key.is_empty()
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Create {
                board: "board_1".into(),
                column: None,
                title: "T".into(),
                description: None,
                priority: None,
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[tokio::test]
    async fn update_card_with_all_options_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_update_card()
            .withf(|req| {
                let r = req.get_ref();
                let paths = r.update_mask.as_ref().map(|m| m.paths.clone());
                paths
                    == Some(vec![
                        "title".into(),
                        "description".into(),
                        "priority".into(),
                    ])
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Update {
                card_id: "card_1".into(),
                title: Some("T".into()),
                description: Some("D".into()),
                priority: Some(PriorityArg::Urgent),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[tokio::test]
    async fn move_card_with_default_position_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_move_card()
            .withf(|req| {
                let r = req.get_ref();
                r.card_id == "card_1" && r.to_column_id == "col_2" && r.to_position == 0
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let out = run(
            CardAction::Move {
                card_id: "card_1".into(),
                column: "col_2".into(),
                position: None,
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardOutput::Detail(_)));
    }

    #[test]
    fn card_detail_out_covers_nested_fields() {
        let card = client::Card {
            labels: vec![client::Label {
                id: "l1".into(),
                project_id: "p1".into(),
                name: "bug".into(),
                style: "red".into(),
            }],
            assignees: vec![client::Assignee {
                subject: "sub".into(),
                display_name: "Ada".into(),
                avatar_url: "http://a".into(),
            }],
            checklist: vec![client::ChecklistItem {
                id: "i1".into(),
                text: "x".into(),
                done: true,
                position: 1,
            }],
            github_links: vec![client::GitHubLink {
                id: "g1".into(),
                repo: "r".into(),
                number: 1,
                state: "open".into(),
                merged: false,
                last_synced_at: None,
            }],
            ..sample_card()
        };
        let out = CardDetailOut::from_proto(card);
        assert_eq!(out.labels.len(), 1);
        assert_eq!(out.assignees.len(), 1);
        assert_eq!(out.checklist.len(), 1);
        assert_eq!(out.github_links.len(), 1);
    }

    #[test]
    fn fmt_ts_handles_missing_and_invalid() {
        assert!(fmt_ts(None).is_empty());
        assert!(
            fmt_ts(Some(&prost_types::Timestamp {
                seconds: i64::MAX,
                nanos: 0,
            }))
            .is_empty()
        );
    }

    #[test]
    fn priority_arg_to_proto_covers_all() {
        assert_eq!(
            PriorityArg::Low.to_proto() as i32,
            client::CardPriority::Low as i32
        );
        assert_eq!(
            PriorityArg::Medium.to_proto() as i32,
            client::CardPriority::Medium as i32
        );
        assert_eq!(
            PriorityArg::High.to_proto() as i32,
            client::CardPriority::High as i32
        );
        assert_eq!(
            PriorityArg::Urgent.to_proto() as i32,
            client::CardPriority::Urgent as i32
        );
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

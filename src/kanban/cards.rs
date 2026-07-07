//! Kanban card operations.

use crate::error::{Result, ResultExt};
use crate::kanban::client::{
    self, CardPriority, CardServiceClient, CardUrgency, request_with_object_id,
};
use crate::kanban::new_idempotency_key;
use crate::logger::Logger;
use async_trait::async_trait;
use serde::Serialize;

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

/// Resolve assignee subjects to email addresses.
///
/// Previously looked up identities via the Kratos admin API; this resolution
/// is temporarily disabled while the SDK migrates to sso-gateway identity
/// lookups through [`crate::auth::AuthClient`].
async fn resolve_assignee_emails(_assignees: &mut [serde_json::Value]) -> Result<()> {
    Ok(())
}

/// List cards on a board, optionally filtered to a column.
pub async fn list_cards(
    client: &mut dyn CardService,
    board_id: &str,
    column_id: Option<&str>,
) -> Result<Vec<CardOut>> {
    let req = client::ListCardsByBoardRequest {
        board_id: board_id.to_string(),
        column_id: column_id.unwrap_or("").to_string(),
        cursor: String::new(),
        limit: 0,
    };
    let resp = client
        .list_cards_by_board(request_with_object_id(req, board_id)?)
        .await
        .with_ctx(|| "list cards".to_string())?;
    Ok(resp.cards.into_iter().map(CardOut::from_proto).collect())
}

/// Get a single card.
pub async fn get_card(client: &mut dyn CardService, card_id: &str) -> Result<CardDetailOut> {
    let req = client::GetCardRequest {
        card_id: card_id.to_string(),
    };
    let resp = client
        .get_card(request_with_object_id(req, card_id)?)
        .await
        .with_ctx(|| format!("get card {card_id}"))?;
    let mut detail = CardDetailOut::from_proto(resp);
    resolve_assignee_emails(&mut detail.assignees).await?;
    Ok(detail)
}

/// Create a new card.
pub async fn create_card(
    client: &mut dyn CardService,
    board_id: &str,
    column_id: Option<&str>,
    title: &str,
    description: Option<&str>,
    priority: Option<CardPriority>,
) -> Result<CardDetailOut> {
    let req = client::CreateCardRequest {
        board_id: board_id.to_string(),
        column_id: column_id.unwrap_or("").to_string(),
        title: title.to_string(),
        description: description.unwrap_or("").to_string(),
        priority: priority.map(|p| p as i32).unwrap_or_default(),
        due: None,
        milestone_id: String::new(),
        position: 0,
        idempotency_key: new_idempotency_key(),
        urgency: CardUrgency::Medium as i32,
    };
    let resp = client
        .create_card(request_with_object_id(req, board_id)?)
        .await
        .with_ctx(|| format!("create card on board {board_id}"))?;
    let mut detail = CardDetailOut::from_proto(resp);
    resolve_assignee_emails(&mut detail.assignees).await?;
    Ok(detail)
}

/// Update an existing card.
pub async fn update_card(
    client: &mut dyn CardService,
    card_id: &str,
    title: Option<&str>,
    description: Option<&str>,
    priority: Option<CardPriority>,
) -> Result<CardDetailOut> {
    let mut update_card = client::Card {
        id: card_id.to_string(),
        ..Default::default()
    };
    let mut paths: Vec<String> = Vec::new();
    if let Some(t) = title {
        update_card.title = t.to_string();
        paths.push("title".to_string());
    }
    if let Some(d) = description {
        update_card.description = d.to_string();
        paths.push("description".to_string());
    }
    if let Some(p) = priority {
        update_card.priority = p as i32;
        paths.push("priority".to_string());
    }
    let req = client::UpdateCardRequest {
        card_id: card_id.to_string(),
        card: Some(update_card),
        update_mask: Some(prost_types::FieldMask { paths }),
        idempotency_key: new_idempotency_key(),
    };
    let resp = client
        .update_card(request_with_object_id(req, card_id)?)
        .await
        .with_ctx(|| format!("update card {card_id}"))?;
    let mut detail = CardDetailOut::from_proto(resp);
    resolve_assignee_emails(&mut detail.assignees).await?;
    Ok(detail)
}

/// Move a card to another column/position.
pub async fn move_card(
    client: &mut dyn CardService,
    card_id: &str,
    column_id: &str,
    position: Option<i32>,
) -> Result<CardDetailOut> {
    let req = client::MoveCardRequest {
        card_id: card_id.to_string(),
        to_column_id: column_id.to_string(),
        to_position: position.unwrap_or_default(),
        idempotency_key: new_idempotency_key(),
    };
    let resp = client
        .move_card(request_with_object_id(req, card_id)?)
        .await
        .with_ctx(|| format!("move card {card_id}"))?;
    let mut detail = CardDetailOut::from_proto(resp);
    resolve_assignee_emails(&mut detail.assignees).await?;
    Ok(detail)
}

/// Delete a card.
pub async fn delete_card(client: &mut dyn CardService, card_id: &str) -> Result<CardDeleteOut> {
    let req = client::DeleteCardRequest {
        card_id: card_id.to_string(),
    };
    client
        .delete_card(request_with_object_id(req, card_id)?)
        .await
        .with_ctx(|| format!("delete card {card_id}"))?;
    Ok(CardDeleteOut {
        deleted: true,
        card_id: card_id.to_string(),
    })
}

/// Add a dependency between two cards.
pub async fn add_card_dependency(
    client: &mut dyn CardService,
    board_id: &str,
    card_id: &str,
    depends_on: &str,
) -> Result<CardDetailOut> {
    let req = client::CardDependencyRequest {
        card_id: card_id.to_string(),
        depends_on_card_id: depends_on.to_string(),
        idempotency_key: new_idempotency_key(),
    };
    let resp = client
        .add_card_dependency(request_with_object_id(req, board_id)?)
        .await
        .with_ctx(|| format!("add dependency {depends_on} to card {card_id}"))?;
    let mut detail = CardDetailOut::from_proto(resp);
    resolve_assignee_emails(&mut detail.assignees).await?;
    Ok(detail)
}

/// Remove a dependency between two cards.
pub async fn remove_card_dependency(
    client: &mut dyn CardService,
    board_id: &str,
    card_id: &str,
    depends_on: &str,
) -> Result<CardDetailOut> {
    let req = client::CardDependencyRequest {
        card_id: card_id.to_string(),
        depends_on_card_id: depends_on.to_string(),
        idempotency_key: new_idempotency_key(),
    };
    let resp = client
        .remove_card_dependency(request_with_object_id(req, board_id)?)
        .await
        .with_ctx(|| format!("remove dependency {depends_on} from card {card_id}"))?;
    let mut detail = CardDetailOut::from_proto(resp);
    resolve_assignee_emails(&mut detail.assignees).await?;
    Ok(detail)
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

        let cards = list_cards(&mut mock, "board_1", None).await.unwrap();
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].id, "card_1");
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

        let detail = get_card(&mut mock, "card_1").await.unwrap();
        assert_eq!(detail.id, "card_1");
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

        let detail = create_card(
            &mut mock,
            "board_1",
            Some("col_1"),
            "New card",
            Some("Details"),
            Some(client::CardPriority::High),
        )
        .await
        .unwrap();
        assert_eq!(detail.id, "card_1");
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

        let detail = update_card(&mut mock, "card_1", Some("Updated title"), None, None)
            .await
            .unwrap();
        assert_eq!(detail.id, "card_1");
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

        let detail = move_card(&mut mock, "card_1", "col_2", Some(2))
            .await
            .unwrap();
        assert_eq!(detail.id, "card_1");
    }

    #[tokio::test]
    async fn delete_card_succeeds() {
        let mut mock = MockCardService::new();
        mock.expect_delete_card()
            .withf(|req| req.get_ref().card_id == "card_1")
            .times(1)
            .returning(|_| Ok(()));

        let out = delete_card(&mut mock, "card_1").await.unwrap();
        assert!(out.deleted);
        assert_eq!(out.card_id, "card_1");
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

        let detail = add_card_dependency(&mut mock, "board_1", "card_1", "card_2")
            .await
            .unwrap();
        assert_eq!(detail.id, "card_1");
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

        let detail = remove_card_dependency(&mut mock, "board_1", "card_1", "card_2")
            .await
            .unwrap();
        assert_eq!(detail.id, "card_1");
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

        let cards = list_cards(&mut mock, "board_1", Some("col_1"))
            .await
            .unwrap();
        assert_eq!(cards.len(), 1);
    }

    #[tokio::test]
    async fn create_card_with_defaults_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_create_card()
            .withf(|req| {
                let r = req.get_ref();
                r.board_id == "board_1"
                    && r.column_id.is_empty()
                    && r.title == "Default card"
                    && r.description.is_empty()
                    && r.priority == 0
                    && r.position == 0
                    && r.urgency == client::CardUrgency::Medium as i32
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let detail = create_card(&mut mock, "board_1", None, "Default card", None, None)
            .await
            .unwrap();
        assert_eq!(detail.id, "card_1");
    }

    #[tokio::test]
    async fn update_card_with_all_options_returns_detail() {
        let mut mock = MockCardService::new();
        mock.expect_update_card()
            .withf(|req| {
                let r = req.get_ref();
                let paths = r.update_mask.as_ref().map(|m| m.paths.clone());
                r.card_id == "card_1"
                    && paths
                        == Some(vec![
                            "title".to_string(),
                            "description".to_string(),
                            "priority".to_string(),
                        ])
                    && r.card.as_ref().map(|c| c.priority)
                        == Some(client::CardPriority::Urgent as i32)
            })
            .times(1)
            .returning(|_| Ok(sample_card()));

        let detail = update_card(
            &mut mock,
            "card_1",
            Some("Updated"),
            Some("Desc"),
            Some(client::CardPriority::Urgent),
        )
        .await
        .unwrap();
        assert_eq!(detail.id, "card_1");
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

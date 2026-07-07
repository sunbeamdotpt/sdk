//! Kanban search commands.

use crate::error::Result;
use crate::kanban::client::{self, SearchServiceClient};
use crate::logger::Logger;
use async_trait::async_trait;
use clap::Args;
use serde::Serialize;

/// Search arguments.
#[derive(Debug, Args)]
pub struct SearchAction {
    /// Query string.
    pub query: String,
    /// Max results.
    #[arg(short, long)]
    pub limit: Option<i32>,
}

/// Serializable search hit.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SearchHitOut {
    /// Card ID.
    pub card_id: String,
    /// Human-readable card reference.
    pub card_ref: String,
    /// Board ID.
    pub board_id: String,
    /// Project ID.
    pub project_id: String,
    /// Card title.
    pub title: String,
    /// Priority label.
    pub priority: String,
    /// Status label.
    pub status: String,
}

/// Trait abstracting the Kanban search service for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SearchService {
    /// Search cards by query.
    async fn search_cards(
        &mut self,
        req: client::SearchCardsRequest,
    ) -> Result<client::SearchCardsResponse>;
}

/// Wrapper around the generated Tonic search client.
#[derive(Debug)]
pub struct SearchServiceClientWrapper {
    inner: SearchServiceClient<client::AuthChannel>,
}

impl SearchServiceClientWrapper {
    /// Build a wrapper from an authenticated channel.
    pub fn new(inner: SearchServiceClient<client::AuthChannel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl SearchService for SearchServiceClientWrapper {
    async fn search_cards(
        &mut self,
        req: client::SearchCardsRequest,
    ) -> Result<client::SearchCardsResponse> {
        let resp = self.inner.search_cards(req).await?;
        Ok(resp.into_inner())
    }
}

/// Build a search-service client wrapper for the given server and token.
pub async fn build_client(
    logger: &Logger,
    server: &str,
    token: &str,
) -> Result<SearchServiceClientWrapper> {
    let channel = client::build(logger, server, token).await?;
    Ok(SearchServiceClientWrapper::new(SearchServiceClient::new(
        channel,
    )))
}

/// Run a search command and return the matching hits.
pub async fn run(cmd: SearchAction, client: &mut dyn SearchService) -> Result<Vec<SearchHitOut>> {
    let SearchAction { query, limit } = cmd;

    let req = client::SearchCardsRequest {
        query,
        limit: limit.unwrap_or(20),
        ..Default::default()
    };
    let resp = client.search_cards(req).await?;
    let hits: Vec<SearchHitOut> = resp
        .hits
        .into_iter()
        .map(|h| SearchHitOut {
            card_id: h.card_id,
            card_ref: h.card_ref,
            board_id: h.board_id,
            project_id: h.project_id,
            title: h.title,
            priority: h.priority,
            status: h.status,
        })
        .collect();
    Ok(hits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn search_returns_hits() {
        let mut mock = MockSearchService::new();
        mock.expect_search_cards()
            .withf(|req| req.query == "frontend crash" && req.limit == 20)
            .times(1)
            .returning(|_| {
                Ok(client::SearchCardsResponse {
                    hits: vec![client::CardSearchHit {
                        card_id: "card_1".into(),
                        card_ref: "PROJ-1".into(),
                        board_id: "board_1".into(),
                        project_id: "proj_1".into(),
                        title: "Fix frontend crash".into(),
                        description_snippet: "<em>crash</em>".into(),
                        priority: "high".into(),
                        status: "open".into(),
                        label_names: vec![],
                        assignee_subjects: vec![],
                        score: 1.0,
                    }],
                    next_cursor: String::new(),
                    total: 1,
                })
            });

        let hits = run(
            SearchAction {
                query: "frontend crash".into(),
                limit: None,
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].card_id, "card_1");
        assert_eq!(hits[0].title, "Fix frontend crash");
    }

    #[tokio::test]
    async fn search_uses_custom_limit() {
        let mut mock = MockSearchService::new();
        mock.expect_search_cards()
            .withf(|req| req.limit == 5)
            .times(1)
            .returning(|_| {
                Ok(client::SearchCardsResponse {
                    hits: vec![],
                    next_cursor: String::new(),
                    total: 0,
                })
            });

        let hits = run(
            SearchAction {
                query: "test".into(),
                limit: Some(5),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

//! Kanban public board operations (unauthenticated).

use crate::error::Result;
use crate::kanban::client::{self, PublicBoardServiceClient};
use crate::logger::Logger;
use async_trait::async_trait;
use serde::Serialize;

/// Serializable subset of a public board for output.
#[derive(Debug, Serialize)]
pub struct BoardOut {
    /// Board ID.
    pub id: String,
    /// Project ID.
    pub project_id: String,
    /// Board name.
    pub name: String,
    /// Board description.
    pub description: String,
    /// Board icon.
    pub icon: String,
    /// Number of columns.
    pub columns_count: i32,
    /// Number of cards.
    pub cards_count: i32,
    /// Visibility label.
    pub visibility: String,
}

fn visibility_str(value: i32) -> String {
    match value {
        1 => "private".to_string(),
        2 => "internal".to_string(),
        3 => "public".to_string(),
        _ => "unspecified".to_string(),
    }
}

fn board_to_out(board: &client::Board) -> BoardOut {
    BoardOut {
        id: board.id.clone(),
        project_id: board.project_id.clone(),
        name: board.name.clone(),
        description: board.description.clone(),
        icon: board.icon.clone(),
        columns_count: board.columns_count,
        cards_count: board.cards_count,
        visibility: visibility_str(board.visibility),
    }
}

/// Trait abstracting the Kanban public board service for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PublicBoardService {
    /// Get a public board by ID.
    async fn get_public_board(
        &mut self,
        req: client::GetPublicBoardRequest,
    ) -> Result<client::Board>;
    /// List public boards in a project.
    async fn list_public_boards(
        &mut self,
        req: client::ListPublicBoardsRequest,
    ) -> Result<client::ListBoardsResponse>;
}

/// Wrapper around the generated Tonic public board client.
#[derive(Debug)]
pub struct PublicBoardServiceClientWrapper {
    inner: PublicBoardServiceClient<tonic::transport::Channel>,
}

impl PublicBoardServiceClientWrapper {
    /// Build a wrapper from an unauthenticated channel.
    pub fn new(inner: PublicBoardServiceClient<tonic::transport::Channel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl PublicBoardService for PublicBoardServiceClientWrapper {
    async fn get_public_board(
        &mut self,
        req: client::GetPublicBoardRequest,
    ) -> Result<client::Board> {
        let resp = self.inner.get_public_board(req).await?;
        Ok(resp.into_inner())
    }

    async fn list_public_boards(
        &mut self,
        req: client::ListPublicBoardsRequest,
    ) -> Result<client::ListBoardsResponse> {
        let resp = self.inner.list_public_boards(req).await?;
        Ok(resp.into_inner())
    }
}

/// Build a public-board-service client wrapper for the given server.
pub async fn build_client(
    logger: &Logger,
    server: &str,
) -> Result<PublicBoardServiceClientWrapper> {
    let channel = client::connect(logger, server).await?;
    Ok(PublicBoardServiceClientWrapper::new(
        PublicBoardServiceClient::new(channel),
    ))
}

/// Get a public board by ID.
pub async fn get_public_board(
    client: &mut dyn PublicBoardService,
    board_id: &str,
) -> Result<BoardOut> {
    let req = client::GetPublicBoardRequest {
        board_id: board_id.to_string(),
    };
    let resp = client.get_public_board(req).await?;
    Ok(board_to_out(&resp))
}

/// List public boards in a project.
pub async fn list_public_boards(
    client: &mut dyn PublicBoardService,
    project_id: &str,
) -> Result<Vec<BoardOut>> {
    let req = client::ListPublicBoardsRequest {
        project_id: project_id.to_string(),
    };
    let resp = client.list_public_boards(req).await?;
    Ok(resp.boards.iter().map(board_to_out).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_board(id: &str, project_id: &str, visibility: i32) -> client::Board {
        client::Board {
            id: id.into(),
            project_id: project_id.into(),
            name: "Board".into(),
            description: "A board".into(),
            icon: "square".into(),
            created_at: Some(prost_types::Timestamp {
                seconds: 1,
                nanos: 0,
            }),
            updated_at: Some(prost_types::Timestamp {
                seconds: 2,
                nanos: 0,
            }),
            columns_count: 3,
            cards_count: 5,
            visibility,
        }
    }

    #[tokio::test]
    async fn get_public_board_returns_board() {
        let mut mock = MockPublicBoardService::new();
        mock.expect_get_public_board()
            .withf(|req| req.board_id == "board_123")
            .times(1)
            .returning(|_| Ok(sample_board("board_123", "proj_123", 3)));

        let board = get_public_board(&mut mock, "board_123").await.unwrap();
        assert_eq!(board.id, "board_123");
        assert_eq!(board.visibility, "public");
    }

    #[tokio::test]
    async fn list_public_boards_returns_boards() {
        let mut mock = MockPublicBoardService::new();
        mock.expect_list_public_boards()
            .withf(|req| req.project_id == "proj_123")
            .times(1)
            .returning(|_| {
                Ok(client::ListBoardsResponse {
                    boards: vec![sample_board("board_1", "proj_123", 3)],
                })
            });

        let boards = list_public_boards(&mut mock, "proj_123").await.unwrap();
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].id, "board_1");
        assert_eq!(boards[0].visibility, "public");
    }

    #[tokio::test]
    async fn list_public_boards_visibility_variants() {
        let mut mock = MockPublicBoardService::new();
        mock.expect_list_public_boards().times(1).returning(|_| {
            Ok(client::ListBoardsResponse {
                boards: vec![
                    sample_board("b1", "p1", 1),
                    sample_board("b2", "p1", 2),
                    sample_board("b3", "p1", 99),
                ],
            })
        });

        let boards = list_public_boards(&mut mock, "p1").await.unwrap();
        assert_eq!(boards.len(), 3);
        assert_eq!(boards[0].visibility, "private");
        assert_eq!(boards[1].visibility, "internal");
        assert_eq!(boards[2].visibility, "unspecified");
    }

    #[test]
    fn visibility_strings_cover_all_branches() {
        assert_eq!(visibility_str(1), "private");
        assert_eq!(visibility_str(2), "internal");
        assert_eq!(visibility_str(3), "public");
        assert_eq!(visibility_str(0), "unspecified");
        assert_eq!(visibility_str(99), "unspecified");
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::not-a-url").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }

    #[tokio::test]
    async fn wrapper_new_constructs() {
        let channel = tonic::transport::Endpoint::from_static("http://[::1]:1").connect_lazy();
        let _wrapper = PublicBoardServiceClientWrapper::new(PublicBoardServiceClient::new(channel));
    }
}

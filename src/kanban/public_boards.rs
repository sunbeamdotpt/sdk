//! Kanban public board commands (unauthenticated).

use crate::error::Result;
use crate::kanban::client::{self, PublicBoardServiceClient};
use crate::logger::Logger;
use async_trait::async_trait;
use clap::Subcommand;
use serde::Serialize;

/// Public board actions.
#[derive(Debug, Subcommand)]
pub enum PublicBoardAction {
    /// Get a public board.
    Get {
        /// Board ID or name.
        board_id: String,
    },
    /// List public boards in a project.
    List {
        /// Project ID or name.
        project_id: String,
    },
}

/// Serializable subset of a public board for CLI output.
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

/// Output of a public board command.
#[derive(Debug, Serialize)]
pub enum PublicBoardOutput {
    /// Single public board.
    Get(BoardOut),
    /// List of public boards.
    List(Vec<BoardOut>),
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

/// Run a public board command using the provided service client.
pub async fn run_with_client(
    cmd: PublicBoardAction,
    client: &mut dyn PublicBoardService,
) -> Result<PublicBoardOutput> {
    match cmd {
        PublicBoardAction::Get { board_id } => {
            let req = client::GetPublicBoardRequest { board_id };
            let resp = client.get_public_board(req).await?;
            Ok(PublicBoardOutput::Get(board_to_out(&resp)))
        }
        PublicBoardAction::List { project_id } => {
            let req = client::ListPublicBoardsRequest { project_id };
            let resp = client.list_public_boards(req).await?;
            let boards: Vec<_> = resp.boards.iter().map(board_to_out).collect();
            Ok(PublicBoardOutput::List(boards))
        }
    }
}

/// Run a public board command.
pub async fn run(
    logger: &Logger,
    cmd: PublicBoardAction,
    server: &str,
) -> Result<PublicBoardOutput> {
    let mut client = build_client(logger, server).await?;
    run_with_client(cmd, &mut client).await
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

        let output = run_with_client(
            PublicBoardAction::Get {
                board_id: "board_123".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        let board = match output {
            PublicBoardOutput::Get(b) => b,
            other => panic!("expected Get output, got {other:?}"),
        };
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

        let output = run_with_client(
            PublicBoardAction::List {
                project_id: "proj_123".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        let boards = match output {
            PublicBoardOutput::List(b) => b,
            other => panic!("expected List output, got {other:?}"),
        };
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

        let output = run_with_client(
            PublicBoardAction::List {
                project_id: "p1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        let boards = match output {
            PublicBoardOutput::List(b) => b,
            other => panic!("expected List output, got {other:?}"),
        };
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
    async fn run_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = run(
            &logger,
            PublicBoardAction::Get {
                board_id: "board_123".into(),
            },
            ":::not-a-url",
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
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

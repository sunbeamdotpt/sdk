//! Kanban board operations.

use crate::error::{Result, ResultExt};
use crate::kanban::client::{
    self, AddColumnRequest, BoardServiceClient, BoardVisibility, CreateBoardRequest,
    DeleteBoardRequest, GetBoardRequest, ListBoardsRequest, MoveColumnRequest, RemoveColumnRequest,
    UpdateBoardRequest, UpdateColumnRequest,
};
use crate::kanban::fmt_proto_time;
use crate::kanban::resolve;
use crate::logger::Logger;
use async_trait::async_trait;
use prost_types::FieldMask;
use serde::Serialize;

/// Serializable board for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoardOut {
    /// Board ID.
    pub id: String,
    /// Project ID.
    pub project_id: String,
    /// Board name.
    pub name: String,
    /// Board description.
    pub description: String,
    /// Icon identifier.
    pub icon: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
    /// Number of columns.
    pub columns_count: i32,
    /// Number of cards.
    pub cards_count: i32,
    /// Visibility label.
    pub visibility: String,
}

impl From<client::Board> for BoardOut {
    fn from(b: client::Board) -> Self {
        Self {
            id: b.id,
            project_id: b.project_id,
            name: b.name,
            description: b.description,
            icon: b.icon,
            created_at: b
                .created_at
                .as_ref()
                .map(fmt_proto_time)
                .unwrap_or_default(),
            updated_at: b
                .updated_at
                .as_ref()
                .map(fmt_proto_time)
                .unwrap_or_default(),
            columns_count: b.columns_count,
            cards_count: b.cards_count,
            visibility: board_visibility_name(b.visibility),
        }
    }
}

/// Serializable column for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ColumnOut {
    /// Column ID.
    pub id: String,
    /// Board ID.
    pub board_id: String,
    /// Column title.
    pub title: String,
    /// Accent color.
    pub accent: String,
    /// WIP limit.
    pub wip_limit: i32,
    /// Position.
    pub position: i32,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
}

impl From<client::Column> for ColumnOut {
    fn from(c: client::Column) -> Self {
        Self {
            id: c.id,
            board_id: c.board_id,
            title: c.title,
            accent: c.accent,
            wip_limit: c.wip_limit,
            position: c.position,
            created_at: c
                .created_at
                .as_ref()
                .map(fmt_proto_time)
                .unwrap_or_default(),
            updated_at: c
                .updated_at
                .as_ref()
                .map(fmt_proto_time)
                .unwrap_or_default(),
        }
    }
}

/// Serializable board detail for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoardDetailOut {
    /// Board summary.
    #[serde(flatten)]
    pub board: BoardOut,
    /// Columns on the board.
    pub columns: Vec<ColumnOut>,
}

/// Board deletion confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoardDeleteOut {
    /// Whether the deletion succeeded.
    pub deleted: bool,
    /// Deleted board ID.
    pub board_id: String,
}

/// Column removal confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ColumnRemoveOut {
    /// Whether the removal succeeded.
    pub removed: bool,
    /// Board ID.
    pub board_id: String,
    /// Removed column ID.
    pub column_id: String,
}

fn board_visibility_name(v: i32) -> String {
    match v {
        v if v == BoardVisibility::Private as i32 => "private".to_string(),
        v if v == BoardVisibility::Internal as i32 => "internal".to_string(),
        v if v == BoardVisibility::Public as i32 => "public".to_string(),
        _ => "unspecified".to_string(),
    }
}

/// Trait abstracting the Kanban board service for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait BoardService {
    /// List boards in a project.
    async fn list_boards(
        &mut self,
        req: tonic::Request<client::ListBoardsRequest>,
    ) -> Result<client::ListBoardsResponse>;

    /// Get a board with its columns.
    async fn get_board(
        &mut self,
        req: tonic::Request<client::GetBoardRequest>,
    ) -> Result<client::BoardDetail>;

    /// Create a board.
    async fn create_board(
        &mut self,
        req: tonic::Request<client::CreateBoardRequest>,
    ) -> Result<client::Board>;

    /// Update a board.
    async fn update_board(
        &mut self,
        req: tonic::Request<client::UpdateBoardRequest>,
    ) -> Result<client::Board>;

    /// Delete a board.
    async fn delete_board(&mut self, req: tonic::Request<client::DeleteBoardRequest>)
    -> Result<()>;

    /// Add a column to a board.
    async fn add_column(
        &mut self,
        req: tonic::Request<client::AddColumnRequest>,
    ) -> Result<client::Column>;

    /// Update a column.
    async fn update_column(
        &mut self,
        req: tonic::Request<client::UpdateColumnRequest>,
    ) -> Result<client::Column>;

    /// Remove a column from a board.
    async fn remove_column(
        &mut self,
        req: tonic::Request<client::RemoveColumnRequest>,
    ) -> Result<()>;

    /// Move a column within a board.
    async fn move_column(
        &mut self,
        req: tonic::Request<client::MoveColumnRequest>,
    ) -> Result<client::MoveColumnResponse>;
}

/// Wrapper around the generated Tonic board client.
#[derive(Debug)]
pub struct BoardServiceClientWrapper {
    inner: BoardServiceClient<client::AuthChannel>,
}

impl BoardServiceClientWrapper {
    /// Build a wrapper from an authenticated channel.
    pub fn new(inner: BoardServiceClient<client::AuthChannel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl BoardService for BoardServiceClientWrapper {
    async fn list_boards(
        &mut self,
        req: tonic::Request<client::ListBoardsRequest>,
    ) -> Result<client::ListBoardsResponse> {
        let resp = self.inner.list_boards(req).await?;
        Ok(resp.into_inner())
    }

    async fn get_board(
        &mut self,
        req: tonic::Request<client::GetBoardRequest>,
    ) -> Result<client::BoardDetail> {
        let resp = self.inner.get_board(req).await?;
        Ok(resp.into_inner())
    }

    async fn create_board(
        &mut self,
        req: tonic::Request<client::CreateBoardRequest>,
    ) -> Result<client::Board> {
        let resp = self.inner.create_board(req).await?;
        Ok(resp.into_inner())
    }

    async fn update_board(
        &mut self,
        req: tonic::Request<client::UpdateBoardRequest>,
    ) -> Result<client::Board> {
        let resp = self.inner.update_board(req).await?;
        Ok(resp.into_inner())
    }

    async fn delete_board(
        &mut self,
        req: tonic::Request<client::DeleteBoardRequest>,
    ) -> Result<()> {
        self.inner.delete_board(req).await?;
        Ok(())
    }

    async fn add_column(
        &mut self,
        req: tonic::Request<client::AddColumnRequest>,
    ) -> Result<client::Column> {
        let resp = self.inner.add_column(req).await?;
        Ok(resp.into_inner())
    }

    async fn update_column(
        &mut self,
        req: tonic::Request<client::UpdateColumnRequest>,
    ) -> Result<client::Column> {
        let resp = self.inner.update_column(req).await?;
        Ok(resp.into_inner())
    }

    async fn remove_column(
        &mut self,
        req: tonic::Request<client::RemoveColumnRequest>,
    ) -> Result<()> {
        self.inner.remove_column(req).await?;
        Ok(())
    }

    async fn move_column(
        &mut self,
        req: tonic::Request<client::MoveColumnRequest>,
    ) -> Result<client::MoveColumnResponse> {
        let resp = self.inner.move_column(req).await?;
        Ok(resp.into_inner())
    }
}

/// Build a board-service client wrapper for the given server and token.
pub async fn build_client(
    logger: &Logger,
    server: &str,
    token: &str,
) -> Result<BoardServiceClientWrapper> {
    let channel = client::build(logger, server, token).await?;
    Ok(BoardServiceClientWrapper::new(BoardServiceClient::new(
        channel,
    )))
}

/// Resolve a column identifier from a raw string within a board.
async fn resolve_column_id(
    client: &mut dyn BoardService,
    board_id: &str,
    raw: &str,
) -> Result<String> {
    if resolve::looks_like_id(raw) {
        return Ok(raw.to_string());
    }
    let resp = client
        .get_board(tonic::Request::new(GetBoardRequest {
            board_id: board_id.to_string(),
        }))
        .await
        .with_ctx(|| format!("get board {board_id} to resolve column name"))?;
    let matches: Vec<_> = resp
        .columns
        .into_iter()
        .filter(|c| resolve::name_matches(&c.title, raw))
        .map(|c| (c.id, c.title))
        .collect();
    resolve::unique_match(matches, "column", raw)
}

/// List boards in a project.
pub async fn list_boards(client: &mut dyn BoardService, project_id: &str) -> Result<Vec<BoardOut>> {
    let resp = client
        .list_boards(tonic::Request::new(ListBoardsRequest {
            project_id: project_id.to_string(),
        }))
        .await
        .with_ctx(|| "list boards failed".to_string())?;
    Ok(resp.boards.into_iter().map(Into::into).collect())
}

/// Get a board with its columns.
pub async fn get_board(client: &mut dyn BoardService, board_id: &str) -> Result<BoardDetailOut> {
    let resp = client
        .get_board(tonic::Request::new(GetBoardRequest {
            board_id: board_id.to_string(),
        }))
        .await
        .with_ctx(|| "get board failed".to_string())?;
    Ok(BoardDetailOut {
        board: resp.board.map(BoardOut::from).unwrap_or(BoardOut {
            id: board_id.to_string(),
            project_id: String::new(),
            name: String::new(),
            description: String::new(),
            icon: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
            columns_count: 0,
            cards_count: 0,
            visibility: String::new(),
        }),
        columns: resp.columns.into_iter().map(Into::into).collect(),
    })
}

/// Create a board.
pub async fn create_board(
    client: &mut dyn BoardService,
    project_id: &str,
    name: &str,
    description: Option<&str>,
    icon: Option<&str>,
    visibility: BoardVisibility,
) -> Result<BoardOut> {
    let req = CreateBoardRequest {
        project_id: project_id.to_string(),
        name: name.to_string(),
        description: description.unwrap_or("").to_string(),
        icon: icon.unwrap_or("").to_string(),
        idempotency_key: crate::kanban::new_idempotency_key(),
        visibility: visibility as i32,
    };
    let req = crate::kanban::client::request_with_object_id(req, project_id)?;
    let resp = client
        .create_board(req)
        .await
        .with_ctx(|| "create board failed".to_string())?;
    Ok(BoardOut::from(resp))
}

/// Update a board.
pub async fn update_board(
    client: &mut dyn BoardService,
    board_id: &str,
    name: Option<&str>,
    description: Option<&str>,
    icon: Option<&str>,
    visibility: Option<BoardVisibility>,
) -> Result<BoardOut> {
    let mut paths = Vec::new();
    if name.is_some() {
        paths.push("name".to_string());
    }
    if description.is_some() {
        paths.push("description".to_string());
    }
    if icon.is_some() {
        paths.push("icon".to_string());
    }
    if visibility.is_some() {
        paths.push("visibility".to_string());
    }
    let req = UpdateBoardRequest {
        board_id: board_id.to_string(),
        board: Some(client::Board {
            id: board_id.to_string(),
            name: name.unwrap_or("").to_string(),
            description: description.unwrap_or("").to_string(),
            icon: icon.unwrap_or("").to_string(),
            visibility: visibility.map(|v| v as i32).unwrap_or_default(),
            ..Default::default()
        }),
        update_mask: Some(FieldMask { paths }),
    };
    let req = crate::kanban::client::request_with_object_id(req, board_id)?;
    let resp = client
        .update_board(req)
        .await
        .with_ctx(|| "update board failed".to_string())?;
    Ok(BoardOut::from(resp))
}

/// Delete a board.
pub async fn delete_board(client: &mut dyn BoardService, board_id: &str) -> Result<BoardDeleteOut> {
    let req = crate::kanban::client::request_with_object_id(
        DeleteBoardRequest {
            board_id: board_id.to_string(),
        },
        board_id,
    )?;
    client
        .delete_board(req)
        .await
        .with_ctx(|| "delete board failed".to_string())?;
    Ok(BoardDeleteOut {
        deleted: true,
        board_id: board_id.to_string(),
    })
}

/// Add a column to a board.
pub async fn add_column(
    client: &mut dyn BoardService,
    board_id: &str,
    title: &str,
    accent: Option<&str>,
    wip_limit: Option<i32>,
    position: Option<i32>,
) -> Result<ColumnOut> {
    let req = AddColumnRequest {
        board_id: board_id.to_string(),
        title: title.to_string(),
        accent: accent.unwrap_or("").to_string(),
        wip_limit: wip_limit.unwrap_or_default(),
        position: position.unwrap_or_default(),
        idempotency_key: crate::kanban::new_idempotency_key(),
    };
    let req = crate::kanban::client::request_with_object_id(req, board_id)?;
    let resp = client
        .add_column(req)
        .await
        .with_ctx(|| "add column failed".to_string())?;
    Ok(ColumnOut::from(resp))
}

/// Update a column.
pub async fn update_column(
    client: &mut dyn BoardService,
    board_id: &str,
    column_id: &str,
    title: Option<&str>,
    accent: Option<&str>,
    wip_limit: Option<i32>,
) -> Result<ColumnOut> {
    let column_id = resolve_column_id(client, board_id, column_id).await?;
    let mut paths = Vec::new();
    if title.is_some() {
        paths.push("title".to_string());
    }
    if accent.is_some() {
        paths.push("accent".to_string());
    }
    if wip_limit.is_some() {
        paths.push("wip_limit".to_string());
    }
    let req = UpdateColumnRequest {
        board_id: board_id.to_string(),
        column_id: column_id.clone(),
        column: Some(client::Column {
            id: column_id.clone(),
            board_id: board_id.to_string(),
            title: title.unwrap_or("").to_string(),
            accent: accent.unwrap_or("").to_string(),
            wip_limit: wip_limit.unwrap_or_default(),
            ..Default::default()
        }),
        update_mask: Some(FieldMask { paths }),
    };
    let req = crate::kanban::client::request_with_object_id(req, board_id)?;
    let resp = client
        .update_column(req)
        .await
        .with_ctx(|| "update column failed".to_string())?;
    Ok(ColumnOut::from(resp))
}

/// Remove a column from a board.
pub async fn remove_column(
    client: &mut dyn BoardService,
    board_id: &str,
    column_id: &str,
) -> Result<ColumnRemoveOut> {
    let column_id = resolve_column_id(client, board_id, column_id).await?;
    let req = crate::kanban::client::request_with_object_id(
        RemoveColumnRequest {
            board_id: board_id.to_string(),
            column_id: column_id.clone(),
        },
        board_id,
    )?;
    client
        .remove_column(req)
        .await
        .with_ctx(|| "remove column failed".to_string())?;
    Ok(ColumnRemoveOut {
        removed: true,
        board_id: board_id.to_string(),
        column_id,
    })
}

/// Move a column within a board.
pub async fn move_column(
    client: &mut dyn BoardService,
    board_id: &str,
    column_id: &str,
    position: i32,
) -> Result<Vec<ColumnOut>> {
    let column_id = resolve_column_id(client, board_id, column_id).await?;
    let req = crate::kanban::client::request_with_object_id(
        MoveColumnRequest {
            board_id: board_id.to_string(),
            column_id: column_id.clone(),
            to_position: position,
        },
        board_id,
    )?;
    let resp = client
        .move_column(req)
        .await
        .with_ctx(|| "move column failed".to_string())?;
    Ok(resp.columns.into_iter().map(Into::into).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use prost_types::Timestamp;

    fn sample_board(id: &str) -> client::Board {
        client::Board {
            id: id.into(),
            project_id: "proj_1".into(),
            name: "Backlog".into(),
            description: "".into(),
            icon: "clipboard".into(),
            created_at: Some(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            updated_at: Some(Timestamp {
                seconds: 1_700_000_100,
                nanos: 0,
            }),
            columns_count: 3,
            cards_count: 12,
            visibility: BoardVisibility::Private as i32,
        }
    }

    fn sample_column(id: &str, board_id: &str, position: i32) -> client::Column {
        client::Column {
            id: id.into(),
            board_id: board_id.into(),
            title: "To Do".into(),
            accent: "blue".into(),
            wip_limit: 5,
            position,
            created_at: Some(Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            updated_at: Some(Timestamp {
                seconds: 1_700_000_100,
                nanos: 0,
            }),
        }
    }

    #[tokio::test]
    async fn list_returns_boards() {
        let mut mock = MockBoardService::new();
        mock.expect_list_boards()
            .withf(|req| req.get_ref().project_id == "proj_1")
            .times(1)
            .returning(|_| {
                Ok(client::ListBoardsResponse {
                    boards: vec![sample_board("board_1")],
                })
            });

        let boards = list_boards(&mut mock, "proj_1").await.unwrap();
        assert_eq!(boards.len(), 1);
        assert_eq!(boards[0].id, "board_1");
    }

    #[tokio::test]
    async fn get_returns_board_detail() {
        let mut mock = MockBoardService::new();
        mock.expect_get_board()
            .withf(|req| req.get_ref().board_id == "board_1")
            .times(1)
            .returning(|_| {
                Ok(client::BoardDetail {
                    board: Some(sample_board("board_1")),
                    columns: vec![sample_column("col_1", "board_1", 1)],
                })
            });

        let detail = get_board(&mut mock, "board_1").await.unwrap();
        assert_eq!(detail.board.id, "board_1");
        assert_eq!(detail.columns.len(), 1);
    }

    #[tokio::test]
    async fn create_returns_new_board() {
        let mut mock = MockBoardService::new();
        mock.expect_create_board()
            .withf(|req| {
                let r = req.get_ref();
                r.project_id == "proj_1"
                    && r.name == "Roadmap"
                    && r.visibility == BoardVisibility::Internal as i32
            })
            .times(1)
            .returning(|_| Ok(sample_board("board_new")));

        let board = create_board(
            &mut mock,
            "proj_1",
            "Roadmap",
            None,
            None,
            BoardVisibility::Internal,
        )
        .await
        .unwrap();
        assert_eq!(board.id, "board_new");
    }

    #[tokio::test]
    async fn update_returns_updated_board() {
        let mut mock = MockBoardService::new();
        mock.expect_update_board()
            .withf(|req| {
                let r = req.get_ref();
                r.board_id == "board_1"
                    && r.board
                        .as_ref()
                        .map(|b| b.id == "board_1" && b.name == "Renamed")
                        .unwrap_or(false)
                    && r.update_mask
                        .as_ref()
                        .map(|m| m.paths == vec!["name"])
                        .unwrap_or(false)
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("board_1")
            })
            .times(1)
            .returning(|_| {
                let mut b = sample_board("board_1");
                b.name = "Renamed".into();
                Ok(b)
            });

        let board = update_board(&mut mock, "board_1", Some("Renamed"), None, None, None)
            .await
            .unwrap();
        assert_eq!(board.name, "Renamed");
    }

    #[tokio::test]
    async fn delete_succeeds() {
        let mut mock = MockBoardService::new();
        mock.expect_delete_board()
            .withf(|req| req.get_ref().board_id == "board_1")
            .times(1)
            .returning(|_| Ok(()));

        let out = delete_board(&mut mock, "board_1").await.unwrap();
        assert!(out.deleted);
        assert_eq!(out.board_id, "board_1");
    }

    #[tokio::test]
    async fn column_add_returns_column() {
        let mut mock = MockBoardService::new();
        mock.expect_add_column()
            .withf(|req| {
                let r = req.get_ref();
                r.board_id == "board_1" && r.title == "Review"
            })
            .times(1)
            .returning(|_| Ok(sample_column("col_new", "board_1", 2)));

        let col = add_column(&mut mock, "board_1", "Review", None, None, None)
            .await
            .unwrap();
        assert_eq!(col.id, "col_new");
    }

    #[tokio::test]
    async fn column_update_returns_column() {
        let mut mock = MockBoardService::new();
        mock.expect_get_board()
            .withf(|req| req.get_ref().board_id == "board_1")
            .times(1)
            .returning(|_| {
                Ok(client::BoardDetail {
                    board: Some(sample_board("board_1")),
                    columns: vec![sample_column("col_1", "board_1", 1)],
                })
            });
        mock.expect_update_column()
            .withf(|req| {
                let r = req.get_ref();
                r.board_id == "board_1"
                    && r.column_id == "col_1"
                    && r.column
                        .as_ref()
                        .map(|c| c.title == "Done")
                        .unwrap_or(false)
                    && r.update_mask
                        .as_ref()
                        .map(|m| m.paths == vec!["title"])
                        .unwrap_or(false)
            })
            .times(1)
            .returning(|_| {
                let mut col = sample_column("col_1", "board_1", 1);
                col.title = "Done".into();
                Ok(col)
            });

        let col = update_column(&mut mock, "board_1", "To Do", Some("Done"), None, None)
            .await
            .unwrap();
        assert_eq!(col.title, "Done");
    }

    #[tokio::test]
    async fn column_remove_succeeds() {
        let mut mock = MockBoardService::new();
        mock.expect_get_board()
            .withf(|req| req.get_ref().board_id == "board_1")
            .times(1)
            .returning(|_| {
                Ok(client::BoardDetail {
                    board: Some(sample_board("board_1")),
                    columns: vec![sample_column("col_1", "board_1", 1)],
                })
            });
        mock.expect_remove_column()
            .withf(|req| {
                let r = req.get_ref();
                r.board_id == "board_1" && r.column_id == "col_1"
            })
            .times(1)
            .returning(|_| Ok(()));

        let out = remove_column(&mut mock, "board_1", "To Do").await.unwrap();
        assert!(out.removed);
        assert_eq!(out.column_id, "col_1");
    }

    #[tokio::test]
    async fn column_move_returns_columns() {
        let mut mock = MockBoardService::new();
        mock.expect_get_board()
            .withf(|req| req.get_ref().board_id == "board_1")
            .times(1)
            .returning(|_| {
                Ok(client::BoardDetail {
                    board: Some(sample_board("board_1")),
                    columns: vec![sample_column("col_1", "board_1", 1)],
                })
            });
        mock.expect_move_column()
            .withf(|req| {
                let r = req.get_ref();
                r.board_id == "board_1" && r.column_id == "col_1" && r.to_position == 2
            })
            .times(1)
            .returning(|_| {
                Ok(client::MoveColumnResponse {
                    columns: vec![sample_column("col_1", "board_1", 2)],
                })
            });

        let cols = move_column(&mut mock, "board_1", "To Do", 2).await.unwrap();
        assert_eq!(cols.len(), 1);
        assert_eq!(cols[0].position, 2);
    }

    #[tokio::test]
    async fn create_with_all_options_returns_board() {
        let mut mock = MockBoardService::new();
        mock.expect_create_board()
            .withf(|req| {
                let r = req.get_ref();
                r.project_id == "proj_1"
                    && r.name == "N"
                    && r.description == "D"
                    && r.icon == "I"
                    && r.visibility == BoardVisibility::Public as i32
            })
            .times(1)
            .returning(|_| Ok(sample_board("board_1")));

        let board = create_board(
            &mut mock,
            "proj_1",
            "N",
            Some("D"),
            Some("I"),
            BoardVisibility::Public,
        )
        .await
        .unwrap();
        assert_eq!(board.id, "board_1");
    }

    #[tokio::test]
    async fn update_with_all_options_returns_board() {
        let mut mock = MockBoardService::new();
        mock.expect_update_board()
            .withf(|req| {
                let r = req.get_ref();
                let paths = r.update_mask.as_ref().map(|m| m.paths.clone());
                r.board_id == "board_1"
                    && paths
                        == Some(vec![
                            "name".into(),
                            "description".into(),
                            "icon".into(),
                            "visibility".into(),
                        ])
            })
            .times(1)
            .returning(|_| Ok(sample_board("board_1")));

        let board = update_board(
            &mut mock,
            "board_1",
            Some("N"),
            Some("D"),
            Some("I"),
            Some(BoardVisibility::Internal),
        )
        .await
        .unwrap();
        assert_eq!(board.id, "board_1");
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

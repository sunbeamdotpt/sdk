//! Kanban realtime subscription commands.

use crate::error::{Result, ResultExt};
use crate::kanban::client::{self, AuthChannel, BoardServiceClient, ProjectServiceClient};
use crate::kanban::require_token;
use crate::logger::Logger;
use async_trait::async_trait;
use clap::Subcommand;
use futures::stream::{Stream, TryStreamExt};
use std::pin::Pin;

/// Subscription actions.
#[derive(Debug, Subcommand)]
pub enum SubscribeAction {
    /// Subscribe to board events.
    Board {
        /// Board ID or name.
        board_id: String,
    },
    /// Subscribe to project events.
    Project {
        /// Project ID or name.
        project_id: String,
    },
}

/// Stream of realtime board/project events.
pub type EventStream = Pin<Box<dyn Stream<Item = Result<client::BoardEventEnvelope>> + Send>>;

/// Trait abstracting the Kanban subscription services for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SubscriptionService {
    /// Subscribe to board-level events.
    async fn subscribe_board(
        &mut self,
        req: tonic::Request<client::SubscribeBoardRequest>,
    ) -> Result<EventStream>;
    /// Subscribe to project-level events.
    async fn subscribe_project(
        &mut self,
        req: tonic::Request<client::SubscribeProjectRequest>,
    ) -> Result<EventStream>;
}

/// Wrapper around the generated Tonic board and project clients.
#[derive(Debug)]
pub struct SubscriptionServiceClientWrapper {
    board_client: BoardServiceClient<AuthChannel>,
    project_client: ProjectServiceClient<AuthChannel>,
}

impl SubscriptionServiceClientWrapper {
    /// Build a wrapper from authenticated board and project clients.
    pub fn new(
        board_client: BoardServiceClient<AuthChannel>,
        project_client: ProjectServiceClient<AuthChannel>,
    ) -> Self {
        Self {
            board_client,
            project_client,
        }
    }
}

#[async_trait]
impl SubscriptionService for SubscriptionServiceClientWrapper {
    async fn subscribe_board(
        &mut self,
        req: tonic::Request<client::SubscribeBoardRequest>,
    ) -> Result<EventStream> {
        let stream = self.board_client.subscribe_board(req).await?.into_inner();
        Ok(Box::pin(stream.map_err(|e| e.into())))
    }

    async fn subscribe_project(
        &mut self,
        req: tonic::Request<client::SubscribeProjectRequest>,
    ) -> Result<EventStream> {
        let stream = self
            .project_client
            .subscribe_project(req)
            .await?
            .into_inner();
        Ok(Box::pin(stream.map_err(|e| e.into())))
    }
}

/// Build a subscription-service client wrapper for the given server and token.
pub async fn build_client(
    logger: &Logger,
    server: &str,
    token: &str,
) -> Result<SubscriptionServiceClientWrapper> {
    let channel = client::build(logger, server, token).await?;
    let board_client = BoardServiceClient::new(channel.clone());
    let project_client = ProjectServiceClient::new(channel);
    Ok(SubscriptionServiceClientWrapper::new(
        board_client,
        project_client,
    ))
}

/// Run a subscription command using the provided service client.
pub async fn run_with_client(
    cmd: SubscribeAction,
    client: &mut dyn SubscriptionService,
) -> Result<EventStream> {
    match cmd {
        SubscribeAction::Board { board_id } => {
            let req = tonic::Request::new(client::SubscribeBoardRequest {
                board_id,
                since_seq: 0,
            });
            client
                .subscribe_board(req)
                .await
                .with_ctx(|| "kanban subscribe board failed".to_string())
        }
        SubscribeAction::Project { project_id } => {
            let req = client::request_with_object_id(
                client::SubscribeProjectRequest {
                    project_id: project_id.clone(),
                    since_seq: 0,
                },
                &project_id,
            )?;
            client
                .subscribe_project(req)
                .await
                .with_ctx(|| "kanban subscribe project failed".to_string())
        }
    }
}

/// Run a subscription command.
pub async fn run(logger: &Logger, cmd: SubscribeAction, server: &str) -> Result<EventStream> {
    let token = require_token().await?;
    let mut client = build_client(logger, server, &token).await?;
    run_with_client(cmd, &mut client).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::stream::StreamExt;

    fn heartbeat_envelope(board_id: &str) -> client::BoardEventEnvelope {
        client::BoardEventEnvelope {
            board_id: board_id.into(),
            event_id: "evt_1".into(),
            nats_seq: 1,
            board_revision: 1,
            emitted_at: Some(prost_types::Timestamp {
                seconds: 1,
                nanos: 0,
            }),
            emitter_pod_id: "pod_1".into(),
            actor_subject: "user:1".into(),
            payload: Some(client::board_event_envelope::Payload::Heartbeat(
                client::Heartbeat {
                    server_time_ms: 1_700_000_000_000,
                },
            )),
        }
    }

    #[tokio::test]
    async fn subscribe_board_returns_stream() {
        let mut mock = MockSubscriptionService::new();
        mock.expect_subscribe_board()
            .withf(|req| req.get_ref().board_id == "board_123" && req.get_ref().since_seq == 0)
            .times(1)
            .returning(|_| {
                Ok(futures::stream::iter(vec![Ok(heartbeat_envelope("board_123"))]).boxed())
            });

        let stream = run_with_client(
            SubscribeAction::Board {
                board_id: "board_123".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        let events: Vec<_> = stream.collect().await;
        assert_eq!(events.len(), 1);
        assert!(events[0].is_ok());
        assert_eq!(events[0].as_ref().unwrap().event_id, "evt_1");
    }

    #[tokio::test]
    async fn subscribe_project_returns_stream() {
        let mut mock = MockSubscriptionService::new();
        mock.expect_subscribe_project()
            .withf(|req| {
                req.get_ref().project_id == "proj_123"
                    && req.get_ref().since_seq == 0
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("proj_123")
            })
            .times(1)
            .returning(|_| Ok(futures::stream::iter(vec![Ok(heartbeat_envelope(""))]).boxed()));

        let stream = run_with_client(
            SubscribeAction::Project {
                project_id: "proj_123".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        let events: Vec<_> = stream.collect().await;
        assert_eq!(events.len(), 1);
        assert!(events[0].is_ok());
    }

    #[tokio::test]
    async fn subscribe_board_stream_error_propagates() {
        let mut mock = MockSubscriptionService::new();
        mock.expect_subscribe_board().times(1).returning(|_| {
            Ok(
                futures::stream::iter(vec![Err(crate::error::SunbeamError::Other(
                    "stream boom".into(),
                ))])
                .boxed(),
            )
        });

        let stream = run_with_client(
            SubscribeAction::Board {
                board_id: "board_123".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        let events: Vec<_> = stream.collect().await;
        assert_eq!(events.len(), 1);
        assert!(events[0].is_err());
        assert!(
            events[0]
                .as_ref()
                .unwrap_err()
                .to_string()
                .contains("stream boom")
        );
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }

    #[tokio::test]
    async fn run_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = match run(
            &logger,
            SubscribeAction::Board {
                board_id: "b".into(),
            },
            ":::not-a-url",
        )
        .await
        {
            Ok(_) => panic!("expected error for invalid URL"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("login")
                || err.to_string().contains("invalid kanban server URL"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn wrapper_new_constructs() {
        let channel = tonic::transport::Endpoint::from_static("http://[::1]:1").connect_lazy();
        let auth = crate::kanban::client::BearerAuth::new("").unwrap();
        let auth_channel = tonic::service::interceptor::InterceptedService::new(channel, auth);
        let board_client = BoardServiceClient::new(auth_channel.clone());
        let project_client = ProjectServiceClient::new(auth_channel);
        let _wrapper = SubscriptionServiceClientWrapper::new(board_client, project_client);
    }
}

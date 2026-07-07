//! Kanban card template commands.

use crate::error::{Result, ResultExt};
use crate::kanban::client::{
    self, CardTemplate as ProtoCardTemplate, CreateCardTemplateRequest, DeleteCardTemplateRequest,
    GetCardTemplateRequest, ListCardTemplatesRequest,
    TemplateChecklistItem as ProtoTemplateChecklistItem, TemplatesServiceClient,
    UpdateCardTemplateRequest, request_with_object_id,
};
use crate::kanban::resolve;
use crate::logger::Logger;
use async_trait::async_trait;
use clap::Subcommand;
use prost_types::Timestamp;
use serde::Serialize;
use tonic::Request;
use tonic::metadata::MetadataValue;

/// Card template actions.
#[derive(Debug, Subcommand)]
pub enum CardTemplateAction {
    /// List card templates.
    List {
        /// Project ID.
        #[arg(short, long)]
        project: Option<String>,
    },
    /// Get a card template.
    Get {
        /// Template ID or name.
        template_id: String,
    },
    /// Create a card template.
    Create {
        /// Project ID.
        #[arg(short, long)]
        project: Option<String>,
        /// Template name.
        #[arg(short, long)]
        name: String,
    },
    /// Update a card template.
    Update {
        /// Template ID or name.
        template_id: String,
        /// New name.
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Delete a card template.
    Delete {
        /// Template ID or name.
        template_id: String,
    },
}

/// Serializable checklist item for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TemplateChecklistItemOut {
    /// Item title.
    pub title: String,
}

/// Serializable card template for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardTemplateOut {
    /// Template ID.
    pub id: String,
    /// Project ID.
    pub project_id: String,
    /// Template name.
    pub name: String,
    /// Template description.
    pub description: String,
    /// Default card title.
    pub title: String,
    /// Default card description.
    pub default_description: String,
    /// Label names.
    pub label_names: Vec<String>,
    /// Checklist items.
    pub checklist_items: Vec<TemplateChecklistItemOut>,
    /// Whether the template is global.
    pub is_global: bool,
    /// Creation timestamp.
    pub created_at: Option<String>,
    /// Last update timestamp.
    pub updated_at: Option<String>,
}

impl From<&ProtoTemplateChecklistItem> for TemplateChecklistItemOut {
    fn from(i: &ProtoTemplateChecklistItem) -> Self {
        Self {
            title: i.title.clone(),
        }
    }
}

impl From<&ProtoCardTemplate> for CardTemplateOut {
    fn from(t: &ProtoCardTemplate) -> Self {
        Self {
            id: t.id.clone(),
            project_id: t.project_id.clone(),
            name: t.name.clone(),
            description: t.description.clone(),
            title: t.title.clone(),
            default_description: t.default_description.clone(),
            label_names: t.label_names.clone(),
            checklist_items: t.checklist_items.iter().map(|i| i.into()).collect(),
            is_global: t.is_global,
            created_at: format_timestamp(t.created_at.as_ref()),
            updated_at: format_timestamp(t.updated_at.as_ref()),
        }
    }
}

/// Card template deletion confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CardTemplateDeleteOut {
    /// Whether the deletion succeeded.
    pub deleted: bool,
    /// Deleted template ID.
    pub template_id: String,
}

/// Result of running a card template command.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum CardTemplateOutput {
    /// List of card templates.
    List(Vec<CardTemplateOut>),
    /// Single card template.
    Template(CardTemplateOut),
    /// Deletion confirmation.
    Deleted(CardTemplateDeleteOut),
}

/// Format a Prost Timestamp as an RFC 3339 string.
fn format_timestamp(ts: Option<&Timestamp>) -> Option<String> {
    ts.map(|t| {
        chrono::DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| format!("{}s", t.seconds))
    })
}

/// Attach an idempotency key header to a mutating request.
fn attach_idempotency_key<T>(req: &mut Request<T>) -> Result<()> {
    let key = crate::kanban::new_idempotency_key();
    let value = MetadataValue::try_from(key)
        .with_ctx(|| "invalid idempotency key (cannot encode as header)".to_string())?;
    req.metadata_mut()
        .insert("x-sunbeam-idempotency-key", value);
    Ok(())
}

/// Build a mutating request with object-id and idempotency headers.
fn mutation_request<T>(msg: T, object_id: &str) -> Result<Request<T>> {
    let mut req = request_with_object_id(msg, object_id)?;
    attach_idempotency_key(&mut req)?;
    Ok(req)
}

/// Trait abstracting the Kanban card-template service for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait CardTemplateService {
    /// List card templates.
    async fn list_card_templates(
        &mut self,
        req: ListCardTemplatesRequest,
    ) -> Result<client::ListCardTemplatesResponse>;
    /// Get a card template.
    async fn get_card_template(
        &mut self,
        req: GetCardTemplateRequest,
    ) -> Result<client::CardTemplate>;
    /// Create a card template.
    async fn create_card_template(
        &mut self,
        req: Request<CreateCardTemplateRequest>,
    ) -> Result<client::CardTemplate>;
    /// Update a card template.
    async fn update_card_template(
        &mut self,
        req: Request<UpdateCardTemplateRequest>,
    ) -> Result<client::CardTemplate>;
    /// Delete a card template.
    async fn delete_card_template(&mut self, req: Request<DeleteCardTemplateRequest>)
    -> Result<()>;
}

/// Wrapper around the generated Tonic templates client.
#[derive(Debug)]
pub struct CardTemplatesServiceClientWrapper {
    inner: TemplatesServiceClient<client::AuthChannel>,
}

impl CardTemplatesServiceClientWrapper {
    /// Build a wrapper from an authenticated channel.
    pub fn new(inner: TemplatesServiceClient<client::AuthChannel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl CardTemplateService for CardTemplatesServiceClientWrapper {
    async fn list_card_templates(
        &mut self,
        req: ListCardTemplatesRequest,
    ) -> Result<client::ListCardTemplatesResponse> {
        let resp = self.inner.list_card_templates(req).await?;
        Ok(resp.into_inner())
    }

    async fn get_card_template(
        &mut self,
        req: GetCardTemplateRequest,
    ) -> Result<client::CardTemplate> {
        let resp = self.inner.get_card_template(req).await?;
        Ok(resp.into_inner())
    }

    async fn create_card_template(
        &mut self,
        req: Request<CreateCardTemplateRequest>,
    ) -> Result<client::CardTemplate> {
        let resp = self.inner.create_card_template(req).await?;
        Ok(resp.into_inner())
    }

    async fn update_card_template(
        &mut self,
        req: Request<UpdateCardTemplateRequest>,
    ) -> Result<client::CardTemplate> {
        let resp = self.inner.update_card_template(req).await?;
        Ok(resp.into_inner())
    }

    async fn delete_card_template(
        &mut self,
        req: Request<DeleteCardTemplateRequest>,
    ) -> Result<()> {
        self.inner.delete_card_template(req).await?;
        Ok(())
    }
}

/// Build a card-templates-service client wrapper for the given server and token.
pub async fn build_client(
    logger: &Logger,
    server: &str,
    token: &str,
) -> Result<CardTemplatesServiceClientWrapper> {
    let channel = client::build(logger, server, token).await?;
    Ok(CardTemplatesServiceClientWrapper::new(
        TemplatesServiceClient::new(channel),
    ))
}

/// Run a card template command and return the result data.
pub async fn run(
    cmd: CardTemplateAction,
    client: &mut dyn CardTemplateService,
) -> Result<CardTemplateOutput> {
    match cmd {
        CardTemplateAction::List { project } => {
            let req = ListCardTemplatesRequest {
                project_id: project.unwrap_or_default(),
            };
            let resp = client.list_card_templates(req).await?;
            let templates: Vec<CardTemplateOut> = resp.templates.iter().map(|t| t.into()).collect();
            Ok(CardTemplateOutput::List(templates))
        }
        CardTemplateAction::Get { template_id } => {
            let template_id = resolve::resolve_card_template_id(client, None, &template_id).await?;
            let req = GetCardTemplateRequest {
                template_id: template_id.clone(),
            };
            let resp = client.get_card_template(req).await?;
            Ok(CardTemplateOutput::Template(CardTemplateOut::from(&resp)))
        }
        CardTemplateAction::Create { project, name } => {
            let object_id = project.clone().unwrap_or_else(|| "global".to_string());
            let req = mutation_request(
                CreateCardTemplateRequest {
                    project_id: project.unwrap_or_default(),
                    name,
                    description: String::new(),
                    title: String::new(),
                    default_description: String::new(),
                    label_names: Vec::new(),
                    checklist_items: Vec::new(),
                },
                &object_id,
            )?;
            let resp = client.create_card_template(req).await?;
            Ok(CardTemplateOutput::Template(CardTemplateOut::from(&resp)))
        }
        CardTemplateAction::Update { template_id, name } => {
            let template_id = resolve::resolve_card_template_id(client, None, &template_id).await?;
            let mut paths = Vec::new();
            if name.is_some() {
                paths.push("name".to_string());
            }
            let req = mutation_request(
                UpdateCardTemplateRequest {
                    template_id: template_id.clone(),
                    update_mask: Some(prost_types::FieldMask { paths }),
                    name: name.unwrap_or_default(),
                    description: String::new(),
                    title: String::new(),
                    default_description: String::new(),
                    label_names: Vec::new(),
                    checklist_items: Vec::new(),
                },
                &template_id,
            )?;
            let resp = client.update_card_template(req).await?;
            Ok(CardTemplateOutput::Template(CardTemplateOut::from(&resp)))
        }
        CardTemplateAction::Delete { template_id } => {
            let template_id = resolve::resolve_card_template_id(client, None, &template_id).await?;
            let req = mutation_request(
                DeleteCardTemplateRequest {
                    template_id: template_id.clone(),
                },
                &template_id,
            )?;
            client.delete_card_template(req).await?;
            Ok(CardTemplateOutput::Deleted(CardTemplateDeleteOut {
                deleted: true,
                template_id,
            }))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card_template_fixture() -> client::CardTemplate {
        client::CardTemplate {
            id: "ctmpl_1".into(),
            project_id: "proj_1".into(),
            name: "Bug".into(),
            description: "Bug card template".into(),
            title: "[BUG] ".into(),
            default_description: "Describe the bug".into(),
            label_names: vec!["bug".into()],
            checklist_items: vec![client::TemplateChecklistItem {
                title: "Reproduce".into(),
            }],
            is_global: false,
            created_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            updated_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
        }
    }

    #[tokio::test]
    async fn list_returns_card_templates() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_list_card_templates()
            .withf(|req| req.project_id == "proj_1")
            .times(1)
            .returning(|_| {
                Ok(client::ListCardTemplatesResponse {
                    templates: vec![card_template_fixture()],
                })
            });

        let out = run(
            CardTemplateAction::List {
                project: Some("proj_1".into()),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            CardTemplateOutput::List(t) => assert_eq!(t.len(), 1),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_returns_card_template() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_get_card_template()
            .withf(|req| req.template_id == "ctmpl_1")
            .times(1)
            .returning(|_| Ok(card_template_fixture()));

        let out = run(
            CardTemplateAction::Get {
                template_id: "ctmpl_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardTemplateOutput::Template(_)));
    }

    #[tokio::test]
    async fn create_returns_card_template() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_create_card_template()
            .withf(|req| {
                let inner = req.get_ref();
                inner.project_id == "proj_1" && inner.name == "Bug"
            })
            .times(1)
            .returning(|_| Ok(card_template_fixture()));

        let out = run(
            CardTemplateAction::Create {
                project: Some("proj_1".into()),
                name: "Bug".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardTemplateOutput::Template(_)));
    }

    #[tokio::test]
    async fn update_returns_card_template() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_update_card_template()
            .withf(|req| {
                let inner = req.get_ref();
                inner.template_id == "ctmpl_1" && inner.name == "Renamed"
            })
            .times(1)
            .returning(|_| Ok(card_template_fixture()));

        let out = run(
            CardTemplateAction::Update {
                template_id: "ctmpl_1".into(),
                name: Some("Renamed".into()),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardTemplateOutput::Template(_)));
    }

    #[tokio::test]
    async fn delete_returns_confirmation() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_delete_card_template()
            .withf(|req| req.get_ref().template_id == "ctmpl_1")
            .times(1)
            .returning(|_| Ok(()));

        let out = run(
            CardTemplateAction::Delete {
                template_id: "ctmpl_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            CardTemplateOutput::Deleted(d) => {
                assert!(d.deleted);
                assert_eq!(d.template_id, "ctmpl_1");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_global_card_templates_returns() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_list_card_templates()
            .withf(|req| req.project_id.is_empty())
            .times(1)
            .returning(|_| {
                Ok(client::ListCardTemplatesResponse {
                    templates: vec![card_template_fixture()],
                })
            });

        let out = run(CardTemplateAction::List { project: None }, &mut mock)
            .await
            .unwrap();

        assert!(matches!(out, CardTemplateOutput::List(_)));
    }

    #[tokio::test]
    async fn create_global_card_template_returns() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_create_card_template()
            .withf(|req| {
                let inner = req.get_ref();
                inner.project_id.is_empty() && inner.name == "Global"
            })
            .times(1)
            .returning(|_| Ok(card_template_fixture()));

        let out = run(
            CardTemplateAction::Create {
                project: None,
                name: "Global".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardTemplateOutput::Template(_)));
    }

    #[test]
    fn format_timestamp_negative_nanos() {
        let ts = prost_types::Timestamp {
            seconds: 1,
            nanos: -1,
        };
        assert!(super::format_timestamp(Some(&ts)).is_some());
    }

    #[tokio::test]
    async fn get_card_template_by_name_resolves() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_list_card_templates()
            .withf(|req| req.project_id.is_empty())
            .times(1)
            .returning(|_| {
                Ok(client::ListCardTemplatesResponse {
                    templates: vec![card_template_fixture()],
                })
            });
        mock.expect_get_card_template()
            .withf(|req| req.template_id == "ctmpl_1")
            .times(1)
            .returning(|_| Ok(card_template_fixture()));

        let out = run(
            CardTemplateAction::Get {
                template_id: "Bug".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, CardTemplateOutput::Template(_)));
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

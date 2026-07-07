//! Kanban card template operations.

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
use prost_types::Timestamp;
use serde::Serialize;
use tonic::Request;
use tonic::metadata::MetadataValue;

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

/// List card templates.
pub async fn list_card_templates(
    client: &mut dyn CardTemplateService,
    project_id: Option<&str>,
) -> Result<Vec<CardTemplateOut>> {
    let req = ListCardTemplatesRequest {
        project_id: project_id.unwrap_or("").to_string(),
    };
    let resp = client.list_card_templates(req).await?;
    Ok(resp.templates.iter().map(|t| t.into()).collect())
}

/// Get a card template.
pub async fn get_card_template(
    client: &mut dyn CardTemplateService,
    template_id: &str,
) -> Result<CardTemplateOut> {
    let template_id = resolve::resolve_card_template_id(client, None, template_id).await?;
    let req = GetCardTemplateRequest {
        template_id: template_id.clone(),
    };
    let resp = client.get_card_template(req).await?;
    Ok(CardTemplateOut::from(&resp))
}

/// Create a card template.
pub async fn create_card_template(
    client: &mut dyn CardTemplateService,
    project_id: Option<&str>,
    name: &str,
) -> Result<CardTemplateOut> {
    let object_id = project_id.unwrap_or("global").to_string();
    let req = mutation_request(
        CreateCardTemplateRequest {
            project_id: project_id.unwrap_or("").to_string(),
            name: name.to_string(),
            description: String::new(),
            title: String::new(),
            default_description: String::new(),
            label_names: Vec::new(),
            checklist_items: Vec::new(),
        },
        &object_id,
    )?;
    let resp = client.create_card_template(req).await?;
    Ok(CardTemplateOut::from(&resp))
}

/// Update a card template.
pub async fn update_card_template(
    client: &mut dyn CardTemplateService,
    template_id: &str,
    name: Option<&str>,
) -> Result<CardTemplateOut> {
    let template_id = resolve::resolve_card_template_id(client, None, template_id).await?;
    let mut paths = Vec::new();
    if name.is_some() {
        paths.push("name".to_string());
    }
    let req = mutation_request(
        UpdateCardTemplateRequest {
            template_id: template_id.clone(),
            update_mask: Some(prost_types::FieldMask { paths }),
            name: name.unwrap_or("").to_string(),
            description: String::new(),
            title: String::new(),
            default_description: String::new(),
            label_names: Vec::new(),
            checklist_items: Vec::new(),
        },
        &template_id,
    )?;
    let resp = client.update_card_template(req).await?;
    Ok(CardTemplateOut::from(&resp))
}

/// Delete a card template.
pub async fn delete_card_template(
    client: &mut dyn CardTemplateService,
    template_id: &str,
) -> Result<CardTemplateDeleteOut> {
    let template_id = resolve::resolve_card_template_id(client, None, template_id).await?;
    let req = mutation_request(
        DeleteCardTemplateRequest {
            template_id: template_id.clone(),
        },
        &template_id,
    )?;
    client.delete_card_template(req).await?;
    Ok(CardTemplateDeleteOut {
        deleted: true,
        template_id,
    })
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

        let templates = list_card_templates(&mut mock, Some("proj_1"))
            .await
            .unwrap();
        assert_eq!(templates.len(), 1);
    }

    #[tokio::test]
    async fn get_returns_card_template() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_get_card_template()
            .withf(|req| req.template_id == "ctmpl_1")
            .times(1)
            .returning(|_| Ok(card_template_fixture()));

        let template = get_card_template(&mut mock, "ctmpl_1").await.unwrap();
        assert_eq!(template.id, "ctmpl_1");
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

        let template = create_card_template(&mut mock, Some("proj_1"), "Bug")
            .await
            .unwrap();
        assert_eq!(template.id, "ctmpl_1");
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

        let template = update_card_template(&mut mock, "ctmpl_1", Some("Renamed"))
            .await
            .unwrap();
        assert_eq!(template.id, "ctmpl_1");
    }

    #[tokio::test]
    async fn delete_returns_confirmation() {
        let mut mock = MockCardTemplateService::new();
        mock.expect_delete_card_template()
            .withf(|req| req.get_ref().template_id == "ctmpl_1")
            .times(1)
            .returning(|_| Ok(()));

        let out = delete_card_template(&mut mock, "ctmpl_1").await.unwrap();
        assert!(out.deleted);
        assert_eq!(out.template_id, "ctmpl_1");
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

        let templates = list_card_templates(&mut mock, None).await.unwrap();
        assert_eq!(templates.len(), 1);
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

        let template = create_card_template(&mut mock, None, "Global")
            .await
            .unwrap();
        assert_eq!(template.id, "ctmpl_1");
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

        let template = get_card_template(&mut mock, "Bug").await.unwrap();
        assert_eq!(template.id, "ctmpl_1");
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

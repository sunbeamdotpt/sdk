//! Kanban board template operations.

use crate::error::{Result, ResultExt};
use crate::kanban::client::{
    self, CreateTemplateRequest, DeleteTemplateRequest, GetTemplateRequest, ListTemplatesRequest,
    TemplateColumn as ProtoTemplateColumn, TemplatesServiceClient, UpdateTemplateRequest,
    request_with_object_id,
};
use crate::kanban::resolve;
use crate::logger::Logger;
use async_trait::async_trait;
use prost_types::Timestamp;
use serde::Serialize;
use tonic::Request;
use tonic::metadata::MetadataValue;

/// Serializable column preset for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TemplateColumnOut {
    /// Column title.
    pub title: String,
    /// Column position.
    pub position: i32,
    /// Accent color.
    pub accent: String,
}

/// Serializable board template for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BoardTemplateOut {
    /// Template ID.
    pub id: String,
    /// Project ID.
    pub project_id: String,
    /// Template name.
    pub name: String,
    /// Template description.
    pub description: String,
    /// Column presets.
    pub columns: Vec<TemplateColumnOut>,
    /// Whether the template is global.
    pub is_global: bool,
    /// Creation timestamp.
    pub created_at: Option<String>,
    /// Last update timestamp.
    pub updated_at: Option<String>,
}

impl From<&ProtoTemplateColumn> for TemplateColumnOut {
    fn from(c: &ProtoTemplateColumn) -> Self {
        Self {
            title: c.title.clone(),
            position: c.position,
            accent: c.accent.clone(),
        }
    }
}

impl From<&crate::kanban::client::BoardTemplate> for BoardTemplateOut {
    fn from(t: &crate::kanban::client::BoardTemplate) -> Self {
        Self {
            id: t.id.clone(),
            project_id: t.project_id.clone(),
            name: t.name.clone(),
            description: t.description.clone(),
            columns: t.columns.iter().map(|c| c.into()).collect(),
            is_global: t.is_global,
            created_at: format_timestamp(t.created_at.as_ref()),
            updated_at: format_timestamp(t.updated_at.as_ref()),
        }
    }
}

/// Template deletion confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TemplateDeleteOut {
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

/// Trait abstracting the Kanban templates service for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait TemplateService {
    /// List board templates.
    async fn list_templates(
        &mut self,
        req: ListTemplatesRequest,
    ) -> Result<client::ListTemplatesResponse>;
    /// Get a board template.
    async fn get_template(&mut self, req: GetTemplateRequest) -> Result<client::BoardTemplate>;
    /// Create a board template.
    async fn create_template(
        &mut self,
        req: Request<CreateTemplateRequest>,
    ) -> Result<client::BoardTemplate>;
    /// Update a board template.
    async fn update_template(
        &mut self,
        req: Request<UpdateTemplateRequest>,
    ) -> Result<client::BoardTemplate>;
    /// Delete a board template.
    async fn delete_template(&mut self, req: Request<DeleteTemplateRequest>) -> Result<()>;
}

/// Wrapper around the generated Tonic templates client.
#[derive(Debug)]
pub struct TemplatesServiceClientWrapper {
    inner: TemplatesServiceClient<client::AuthChannel>,
}

impl TemplatesServiceClientWrapper {
    /// Build a wrapper from an authenticated channel.
    pub fn new(inner: TemplatesServiceClient<client::AuthChannel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl TemplateService for TemplatesServiceClientWrapper {
    async fn list_templates(
        &mut self,
        req: ListTemplatesRequest,
    ) -> Result<client::ListTemplatesResponse> {
        let resp = self.inner.list_templates(req).await?;
        Ok(resp.into_inner())
    }

    async fn get_template(&mut self, req: GetTemplateRequest) -> Result<client::BoardTemplate> {
        let resp = self.inner.get_template(req).await?;
        Ok(resp.into_inner())
    }

    async fn create_template(
        &mut self,
        req: Request<CreateTemplateRequest>,
    ) -> Result<client::BoardTemplate> {
        let resp = self.inner.create_template(req).await?;
        Ok(resp.into_inner())
    }

    async fn update_template(
        &mut self,
        req: Request<UpdateTemplateRequest>,
    ) -> Result<client::BoardTemplate> {
        let resp = self.inner.update_template(req).await?;
        Ok(resp.into_inner())
    }

    async fn delete_template(&mut self, req: Request<DeleteTemplateRequest>) -> Result<()> {
        self.inner.delete_template(req).await?;
        Ok(())
    }
}

/// Build a templates-service client wrapper for the given server and token.
pub async fn build_client(
    logger: &Logger,
    server: &str,
    token: &str,
) -> Result<TemplatesServiceClientWrapper> {
    let channel = client::build(logger, server, token).await?;
    Ok(TemplatesServiceClientWrapper::new(
        TemplatesServiceClient::new(channel),
    ))
}

/// List board templates.
pub async fn list_templates(
    client: &mut dyn TemplateService,
    project_id: Option<&str>,
) -> Result<Vec<BoardTemplateOut>> {
    let req = ListTemplatesRequest {
        project_id: project_id.unwrap_or("").to_string(),
    };
    let resp = client.list_templates(req).await?;
    Ok(resp.templates.iter().map(|t| t.into()).collect())
}

/// Get a board template.
pub async fn get_template(
    client: &mut dyn TemplateService,
    template_id: &str,
) -> Result<BoardTemplateOut> {
    let template_id = resolve::resolve_template_id(client, None, template_id).await?;
    let req = GetTemplateRequest {
        template_id: template_id.clone(),
    };
    let resp = client.get_template(req).await?;
    Ok(BoardTemplateOut::from(&resp))
}

/// Create a board template.
pub async fn create_template(
    client: &mut dyn TemplateService,
    project_id: Option<&str>,
    name: &str,
    description: Option<&str>,
) -> Result<BoardTemplateOut> {
    let object_id = project_id.unwrap_or("global").to_string();
    let req = mutation_request(
        CreateTemplateRequest {
            project_id: project_id.unwrap_or("").to_string(),
            name: name.to_string(),
            description: description.unwrap_or("").to_string(),
            columns: Vec::new(),
        },
        &object_id,
    )?;
    let resp = client.create_template(req).await?;
    Ok(BoardTemplateOut::from(&resp))
}

/// Update a board template.
pub async fn update_template(
    client: &mut dyn TemplateService,
    template_id: &str,
    name: Option<&str>,
    description: Option<&str>,
) -> Result<BoardTemplateOut> {
    let template_id = resolve::resolve_template_id(client, None, template_id).await?;
    let mut paths = Vec::new();
    if name.is_some() {
        paths.push("name".to_string());
    }
    if description.is_some() {
        paths.push("description".to_string());
    }
    let req = mutation_request(
        UpdateTemplateRequest {
            template_id: template_id.clone(),
            update_mask: Some(prost_types::FieldMask { paths }),
            name: name.unwrap_or("").to_string(),
            description: description.unwrap_or("").to_string(),
            columns: Vec::new(),
        },
        &template_id,
    )?;
    let resp = client.update_template(req).await?;
    Ok(BoardTemplateOut::from(&resp))
}

/// Delete a board template.
pub async fn delete_template(
    client: &mut dyn TemplateService,
    template_id: &str,
) -> Result<TemplateDeleteOut> {
    let template_id = resolve::resolve_template_id(client, None, template_id).await?;
    let req = mutation_request(
        DeleteTemplateRequest {
            template_id: template_id.clone(),
        },
        &template_id,
    )?;
    client.delete_template(req).await?;
    Ok(TemplateDeleteOut {
        deleted: true,
        template_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn board_template_fixture() -> client::BoardTemplate {
        client::BoardTemplate {
            id: "tmpl_1".into(),
            project_id: "proj_1".into(),
            name: "Onboarding".into(),
            description: "Default onboarding board".into(),
            columns: vec![client::TemplateColumn {
                title: "Todo".into(),
                position: 0,
                accent: "blue".into(),
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
    async fn list_returns_templates() {
        let mut mock = MockTemplateService::new();
        mock.expect_list_templates()
            .withf(|req| req.project_id == "proj_1")
            .times(1)
            .returning(|_| {
                Ok(client::ListTemplatesResponse {
                    templates: vec![board_template_fixture()],
                })
            });

        let templates = list_templates(&mut mock, Some("proj_1")).await.unwrap();
        assert_eq!(templates.len(), 1);
    }

    #[tokio::test]
    async fn get_returns_template() {
        let mut mock = MockTemplateService::new();
        mock.expect_get_template()
            .withf(|req| req.template_id == "tmpl_1")
            .times(1)
            .returning(|_| Ok(board_template_fixture()));

        let template = get_template(&mut mock, "tmpl_1").await.unwrap();
        assert_eq!(template.id, "tmpl_1");
    }

    #[tokio::test]
    async fn create_returns_template() {
        let mut mock = MockTemplateService::new();
        mock.expect_create_template()
            .withf(|req| {
                let inner = req.get_ref();
                inner.project_id == "proj_1" && inner.name == "Onboarding"
            })
            .times(1)
            .returning(|_| Ok(board_template_fixture()));

        let template = create_template(&mut mock, Some("proj_1"), "Onboarding", None)
            .await
            .unwrap();
        assert_eq!(template.id, "tmpl_1");
    }

    #[tokio::test]
    async fn update_returns_template() {
        let mut mock = MockTemplateService::new();
        mock.expect_update_template()
            .withf(|req| {
                let inner = req.get_ref();
                inner.template_id == "tmpl_1" && inner.name == "Renamed"
            })
            .times(1)
            .returning(|_| Ok(board_template_fixture()));

        let template = update_template(&mut mock, "tmpl_1", Some("Renamed"), None)
            .await
            .unwrap();
        assert_eq!(template.id, "tmpl_1");
    }

    #[tokio::test]
    async fn delete_returns_confirmation() {
        let mut mock = MockTemplateService::new();
        mock.expect_delete_template()
            .withf(|req| req.get_ref().template_id == "tmpl_1")
            .times(1)
            .returning(|_| Ok(()));

        let out = delete_template(&mut mock, "tmpl_1").await.unwrap();
        assert!(out.deleted);
        assert_eq!(out.template_id, "tmpl_1");
    }

    #[tokio::test]
    async fn list_global_templates_returns() {
        let mut mock = MockTemplateService::new();
        mock.expect_list_templates()
            .withf(|req| req.project_id.is_empty())
            .times(1)
            .returning(|_| {
                Ok(client::ListTemplatesResponse {
                    templates: vec![board_template_fixture()],
                })
            });

        let templates = list_templates(&mut mock, None).await.unwrap();
        assert_eq!(templates.len(), 1);
    }

    #[tokio::test]
    async fn create_global_template_returns() {
        let mut mock = MockTemplateService::new();
        mock.expect_create_template()
            .withf(|req| {
                let inner = req.get_ref();
                inner.project_id.is_empty() && inner.name == "Global"
            })
            .times(1)
            .returning(|_| Ok(board_template_fixture()));

        let template = create_template(&mut mock, None, "Global", Some("D"))
            .await
            .unwrap();
        assert_eq!(template.id, "tmpl_1");
    }

    #[tokio::test]
    async fn update_template_with_all_options_returns() {
        let mut mock = MockTemplateService::new();
        mock.expect_update_template()
            .withf(|req| {
                let inner = req.get_ref();
                let paths = inner.update_mask.as_ref().map(|m| m.paths.clone());
                inner.template_id == "tmpl_1"
                    && paths == Some(vec!["name".into(), "description".into()])
            })
            .times(1)
            .returning(|_| Ok(board_template_fixture()));

        let template = update_template(&mut mock, "tmpl_1", Some("N"), Some("D"))
            .await
            .unwrap();
        assert_eq!(template.id, "tmpl_1");
    }

    #[test]
    fn format_timestamp_negative_nanos() {
        let ts = prost_types::Timestamp {
            seconds: 1,
            nanos: -1,
        };
        assert!(format_timestamp(Some(&ts)).is_some());
    }

    #[tokio::test]
    async fn get_template_by_name_resolves() {
        let mut mock = MockTemplateService::new();
        mock.expect_list_templates()
            .withf(|req| req.project_id.is_empty())
            .times(1)
            .returning(|_| {
                Ok(client::ListTemplatesResponse {
                    templates: vec![board_template_fixture()],
                })
            });
        mock.expect_get_template()
            .withf(|req| req.template_id == "tmpl_1")
            .times(1)
            .returning(|_| Ok(board_template_fixture()));

        let template = get_template(&mut mock, "Onboarding").await.unwrap();
        assert_eq!(template.id, "tmpl_1");
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

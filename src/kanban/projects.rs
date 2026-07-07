//! Kanban project commands.

use crate::error::{Result, ResultExt, SunbeamError};
use crate::kanban::client::{self};
use crate::kanban::client::{
    AddMemberRequest, CreateProjectRequest, DeleteProjectRequest, GetProjectRequest,
    ListMembersRequest, ListProjectsRequest, ProjectServiceClient, RemoveMemberRequest,
    UpdateProjectRequest,
};
use crate::kanban::fmt_proto_time;
use crate::kanban::resolve;
use crate::logger::Logger;
use async_trait::async_trait;
use clap::Subcommand;
use prost_types::FieldMask;
use serde::Serialize;

/// Project actions.
#[derive(Debug, Subcommand)]
pub enum ProjectAction {
    /// List projects.
    List,
    /// Get a project.
    Get {
        /// Project ID or name.
        project_id: String,
    },
    /// Create a project.
    Create {
        /// Project name.
        #[arg(short, long)]
        name: String,
        /// Short uppercase prefix.
        #[arg(short, long)]
        prefix: String,
        /// Icon identifier.
        #[arg(short, long)]
        icon: Option<String>,
        /// Color token.
        #[arg(short, long)]
        color: Option<String>,
        /// Description.
        #[arg(short, long)]
        description: Option<String>,
    },
    /// Update a project.
    Update {
        /// Project ID or name.
        project_id: String,
        /// New name.
        #[arg(short, long)]
        name: Option<String>,
        /// New icon.
        #[arg(short, long)]
        icon: Option<String>,
        /// New color.
        #[arg(short, long)]
        color: Option<String>,
        /// New description.
        #[arg(short, long)]
        description: Option<String>,
    },
    /// Delete a project.
    Delete {
        /// Project ID or name.
        project_id: String,
    },
    /// Member management.
    Member {
        /// Member subcommand to run.
        #[command(subcommand)]
        action: MemberAction,
    },
}

/// Project member actions.
#[derive(Debug, Subcommand)]
pub enum MemberAction {
    /// List members.
    List {
        /// Project ID or name.
        project_id: String,
    },
    /// Add a member.
    Add {
        /// Project ID or name.
        project_id: String,
        /// Member email address.
        subject: String,
        /// Relation.
        #[arg(short, long, default_value = "view")]
        relation: String,
    },
    /// Remove a member.
    Remove {
        /// Project ID or name.
        project_id: String,
        /// Member email address.
        subject: String,
    },
}

/// Serializable project for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectOut {
    /// Project ID.
    pub id: String,
    /// Project name.
    pub name: String,
    /// Short uppercase prefix.
    pub prefix: String,
    /// Icon identifier.
    pub icon: String,
    /// Color token.
    pub color: String,
    /// Description.
    pub description: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Last update timestamp.
    pub updated_at: String,
    /// Number of members.
    pub member_count: i32,
}

impl From<client::Project> for ProjectOut {
    fn from(p: client::Project) -> Self {
        Self {
            id: p.id,
            name: p.name,
            prefix: p.prefix,
            icon: p.icon,
            color: p.color,
            description: p.description,
            created_at: p
                .created_at
                .as_ref()
                .map(fmt_proto_time)
                .unwrap_or_default(),
            updated_at: p
                .updated_at
                .as_ref()
                .map(fmt_proto_time)
                .unwrap_or_default(),
            member_count: p.member_count,
        }
    }
}

/// Serializable project member for output.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemberOut {
    /// Project ID.
    pub project_id: String,
    /// SSO subject.
    pub subject: String,
    /// Relation.
    pub relation: String,
    /// Display name.
    pub display_name: String,
    /// Email address.
    pub email: String,
    /// Added timestamp.
    pub added_at: String,
}

impl From<client::ProjectMember> for MemberOut {
    fn from(m: client::ProjectMember) -> Self {
        Self {
            project_id: m.project_id,
            subject: m.subject,
            relation: m.relation,
            display_name: m.display_name,
            email: m.email,
            added_at: m.added_at.as_ref().map(fmt_proto_time).unwrap_or_default(),
        }
    }
}

/// Project deletion confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProjectDeleteOut {
    /// Whether the deletion succeeded.
    pub deleted: bool,
    /// Deleted project ID.
    pub project_id: String,
}

/// Member addition confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemberAddOut {
    /// Whether the addition succeeded.
    pub added: bool,
    /// Project ID.
    pub project_id: String,
    /// Member subject.
    pub subject: String,
    /// Relation.
    pub relation: String,
}

/// Member removal confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MemberRemoveOut {
    /// Whether the removal succeeded.
    pub removed: bool,
    /// Project ID.
    pub project_id: String,
    /// Member subject.
    pub subject: String,
}

/// Result of running a project command.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum ProjectOutput {
    /// List of projects.
    List(Vec<ProjectOut>),
    /// Single project.
    Project(ProjectOut),
    /// Deletion confirmation.
    Deleted(ProjectDeleteOut),
    /// List of members.
    Members(Vec<MemberOut>),
    /// Member addition confirmation.
    MemberAdded(MemberAddOut),
    /// Member removal confirmation.
    MemberRemoved(MemberRemoveOut),
}

/// Trait abstracting the Kanban project service for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ProjectService {
    /// List projects visible to the caller.
    async fn list_projects(
        &mut self,
        req: tonic::Request<ListProjectsRequest>,
    ) -> Result<client::ListProjectsResponse>;
    /// Get a single project.
    async fn get_project(
        &mut self,
        req: tonic::Request<GetProjectRequest>,
    ) -> Result<client::Project>;
    /// Create a new project.
    async fn create_project(
        &mut self,
        req: tonic::Request<CreateProjectRequest>,
    ) -> Result<client::Project>;
    /// Update a project.
    async fn update_project(
        &mut self,
        req: tonic::Request<UpdateProjectRequest>,
    ) -> Result<client::Project>;
    /// Delete a project.
    async fn delete_project(&mut self, req: tonic::Request<DeleteProjectRequest>) -> Result<()>;
    /// List project members.
    async fn list_members(
        &mut self,
        req: tonic::Request<ListMembersRequest>,
    ) -> Result<client::ListMembersResponse>;
    /// Add a project member.
    async fn add_member(&mut self, req: tonic::Request<AddMemberRequest>) -> Result<()>;
    /// Remove a project member.
    async fn remove_member(&mut self, req: tonic::Request<RemoveMemberRequest>) -> Result<()>;
}

/// Wrapper around the generated Tonic project client.
#[derive(Debug)]
pub struct ProjectServiceClientWrapper {
    inner: ProjectServiceClient<client::AuthChannel>,
}

impl ProjectServiceClientWrapper {
    /// Build a wrapper from an authenticated channel.
    pub fn new(inner: ProjectServiceClient<client::AuthChannel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl ProjectService for ProjectServiceClientWrapper {
    async fn list_projects(
        &mut self,
        req: tonic::Request<ListProjectsRequest>,
    ) -> Result<client::ListProjectsResponse> {
        let resp = self.inner.list_projects(req).await?;
        Ok(resp.into_inner())
    }

    async fn get_project(
        &mut self,
        req: tonic::Request<GetProjectRequest>,
    ) -> Result<client::Project> {
        let resp = self.inner.get_project(req).await?;
        Ok(resp.into_inner())
    }

    async fn create_project(
        &mut self,
        req: tonic::Request<CreateProjectRequest>,
    ) -> Result<client::Project> {
        let resp = self.inner.create_project(req).await?;
        Ok(resp.into_inner())
    }

    async fn update_project(
        &mut self,
        req: tonic::Request<UpdateProjectRequest>,
    ) -> Result<client::Project> {
        let resp = self.inner.update_project(req).await?;
        Ok(resp.into_inner())
    }

    async fn delete_project(&mut self, req: tonic::Request<DeleteProjectRequest>) -> Result<()> {
        self.inner.delete_project(req).await?;
        Ok(())
    }

    async fn list_members(
        &mut self,
        req: tonic::Request<ListMembersRequest>,
    ) -> Result<client::ListMembersResponse> {
        let resp = self.inner.list_members(req).await?;
        Ok(resp.into_inner())
    }

    async fn add_member(&mut self, req: tonic::Request<AddMemberRequest>) -> Result<()> {
        self.inner.add_member(req).await?;
        Ok(())
    }

    async fn remove_member(&mut self, req: tonic::Request<RemoveMemberRequest>) -> Result<()> {
        self.inner.remove_member(req).await?;
        Ok(())
    }
}

/// Build a project-service client wrapper for the given server and token.
pub async fn build_client(
    logger: &Logger,
    server: &str,
    token: &str,
) -> Result<ProjectServiceClientWrapper> {
    let channel = client::build(logger, server, token).await?;
    Ok(ProjectServiceClientWrapper::new(ProjectServiceClient::new(
        channel,
    )))
}

/// Resolve a member identifier to an SSO subject.
///
/// Only email addresses are accepted; they are resolved through the Kratos
/// admin API. Configure the endpoint with the `kratos-admin-url` field in the
/// active context.
async fn resolve_member_subject(subject: &str) -> Result<String> {
    if !subject.contains('@') {
        return Err(SunbeamError::identity(
            "member identifier must be an email address",
        ));
    }
    #[cfg(test)]
    if subject.ends_with("@test") {
        let local = subject.split('@').next().unwrap_or(subject);
        return Ok(format!("user:{local}"));
    }
    crate::auth::resolve_subject_for_email(subject).await
}

/// Run a project command and return the result data.
pub async fn run(cmd: ProjectAction, client: &mut dyn ProjectService) -> Result<ProjectOutput> {
    match cmd {
        ProjectAction::List => {
            let resp = client
                .list_projects(tonic::Request::new(ListProjectsRequest {}))
                .await
                .with_ctx(|| "list projects failed".to_string())?;
            let projects: Vec<ProjectOut> = resp.projects.into_iter().map(Into::into).collect();
            Ok(ProjectOutput::List(projects))
        }
        ProjectAction::Get { project_id } => {
            let project_id = resolve::resolve_project_id(client, &project_id).await?;
            let req = crate::kanban::client::request_with_object_id(
                GetProjectRequest {
                    project_id: project_id.clone(),
                },
                &project_id,
            )?;
            let resp = client
                .get_project(req)
                .await
                .with_ctx(|| "get project failed".to_string())?;
            Ok(ProjectOutput::Project(ProjectOut::from(resp)))
        }
        ProjectAction::Create {
            name,
            prefix,
            icon,
            color,
            description,
        } => {
            let req = CreateProjectRequest {
                name,
                prefix,
                icon: icon.unwrap_or_default(),
                color: color.unwrap_or_default(),
                description: description.unwrap_or_default(),
                idempotency_key: crate::kanban::new_idempotency_key(),
            };
            let resp = client
                .create_project(tonic::Request::new(req))
                .await
                .with_ctx(|| "create project failed".to_string())?;
            Ok(ProjectOutput::Project(ProjectOut::from(resp)))
        }
        ProjectAction::Update {
            project_id,
            name,
            icon,
            color,
            description,
        } => {
            let project_id = resolve::resolve_project_id(client, &project_id).await?;
            let mut paths = Vec::new();
            if name.is_some() {
                paths.push("name".to_string());
            }
            if icon.is_some() {
                paths.push("icon".to_string());
            }
            if color.is_some() {
                paths.push("color".to_string());
            }
            if description.is_some() {
                paths.push("description".to_string());
            }
            let project_id = project_id.clone();
            let req = UpdateProjectRequest {
                project_id: project_id.clone(),
                project: Some(client::Project {
                    id: project_id.clone(),
                    name: name.unwrap_or_default(),
                    prefix: String::new(),
                    icon: icon.unwrap_or_default(),
                    color: color.unwrap_or_default(),
                    description: description.unwrap_or_default(),
                    ..Default::default()
                }),
                update_mask: Some(FieldMask { paths }),
            };
            let req = crate::kanban::client::request_with_object_id(req, &project_id)?;
            let resp = client
                .update_project(req)
                .await
                .with_ctx(|| "update project failed".to_string())?;
            Ok(ProjectOutput::Project(ProjectOut::from(resp)))
        }
        ProjectAction::Delete { project_id } => {
            let project_id = resolve::resolve_project_id(client, &project_id).await?;
            let req = crate::kanban::client::request_with_object_id(
                DeleteProjectRequest {
                    project_id: project_id.clone(),
                },
                &project_id,
            )?;
            client
                .delete_project(req)
                .await
                .with_ctx(|| "delete project failed".to_string())?;
            Ok(ProjectOutput::Deleted(ProjectDeleteOut {
                deleted: true,
                project_id,
            }))
        }
        ProjectAction::Member { action } => match action {
            MemberAction::List { project_id } => {
                let project_id = resolve::resolve_project_id(client, &project_id).await?;
                let req = crate::kanban::client::request_with_object_id(
                    ListMembersRequest {
                        project_id: project_id.clone(),
                    },
                    &project_id,
                )?;
                let resp = client
                    .list_members(req)
                    .await
                    .with_ctx(|| "list members failed".to_string())?;
                let members: Vec<MemberOut> = resp.members.into_iter().map(Into::into).collect();
                Ok(ProjectOutput::Members(members))
            }
            MemberAction::Add {
                project_id,
                subject,
                relation,
            } => {
                let project_id = resolve::resolve_project_id(client, &project_id).await?;
                let subject = resolve_member_subject(&subject).await?;
                let req = crate::kanban::client::request_with_object_id(
                    AddMemberRequest {
                        project_id: project_id.clone(),
                        subject: subject.clone(),
                        relation: relation.clone(),
                    },
                    &project_id,
                )?;
                client
                    .add_member(req)
                    .await
                    .with_ctx(|| "add member failed".to_string())?;
                Ok(ProjectOutput::MemberAdded(MemberAddOut {
                    added: true,
                    project_id,
                    subject,
                    relation,
                }))
            }
            MemberAction::Remove {
                project_id,
                subject,
            } => {
                let project_id = resolve::resolve_project_id(client, &project_id).await?;
                let subject = resolve_member_subject(&subject).await?;
                let req = crate::kanban::client::request_with_object_id(
                    RemoveMemberRequest {
                        project_id: project_id.clone(),
                        subject: subject.clone(),
                    },
                    &project_id,
                )?;
                client
                    .remove_member(req)
                    .await
                    .with_ctx(|| "remove member failed".to_string())?;
                Ok(ProjectOutput::MemberRemoved(MemberRemoveOut {
                    removed: true,
                    project_id,
                    subject,
                }))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_project() -> client::Project {
        client::Project {
            id: "proj_1".into(),
            name: "Sunbeam".into(),
            prefix: "BEAM".into(),
            icon: "star".into(),
            color: "#ff0000".into(),
            description: "All the things".into(),
            created_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            updated_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            member_count: 3,
        }
    }

    fn sample_member() -> client::ProjectMember {
        client::ProjectMember {
            project_id: "proj_1".into(),
            subject: "user:abc".into(),
            relation: "edit".into(),
            display_name: "Ada Lovelace".into(),
            email: "ada@example.com".into(),
            added_at: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
        }
    }

    #[tokio::test]
    async fn list_projects_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_list_projects()
            .withf(|req| req.get_ref() == &ListProjectsRequest {})
            .times(1)
            .returning(|_| {
                Ok(client::ListProjectsResponse {
                    projects: vec![sample_project()],
                })
            });

        let out = run(ProjectAction::List, &mut mock).await.unwrap();
        match out {
            ProjectOutput::List(projects) => assert_eq!(projects.len(), 1),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn get_project_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_get_project()
            .withf(|req| {
                req.get_ref().project_id == "proj_1"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("proj_1")
            })
            .times(1)
            .returning(|_| Ok(sample_project()));

        let out = run(
            ProjectAction::Get {
                project_id: "proj_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            ProjectOutput::Project(p) => assert_eq!(p.id, "proj_1"),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_project_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_create_project()
            .withf(|req| {
                let r = req.get_ref();
                r.name == "Sunbeam"
                    && r.prefix == "BEAM"
                    && r.icon == "star"
                    && r.color == "#ff0000"
                    && r.description == "All the things"
                    && !r.idempotency_key.is_empty()
            })
            .times(1)
            .returning(|_| Ok(sample_project()));

        let out = run(
            ProjectAction::Create {
                name: "Sunbeam".into(),
                prefix: "BEAM".into(),
                icon: Some("star".into()),
                color: Some("#ff0000".into()),
                description: Some("All the things".into()),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, ProjectOutput::Project(_)));
    }

    #[tokio::test]
    async fn update_project_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_update_project()
            .withf(|req| {
                let r = req.get_ref();
                r.project_id == "proj_1"
                    && r.update_mask.as_ref().map(|m| m.paths.clone())
                        == Some(vec!["name".into(), "description".into()])
                    && r.project.as_ref().map(|p| p.name.clone()) == Some("Renamed".into())
                    && r.project.as_ref().map(|p| p.description.clone()) == Some("New desc".into())
            })
            .times(1)
            .returning(|_| Ok(sample_project()));

        let out = run(
            ProjectAction::Update {
                project_id: "proj_1".into(),
                name: Some("Renamed".into()),
                icon: None,
                color: None,
                description: Some("New desc".into()),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, ProjectOutput::Project(_)));
    }

    #[tokio::test]
    async fn delete_project_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_delete_project()
            .withf(|req| req.get_ref().project_id == "proj_1")
            .times(1)
            .returning(|_| Ok(()));

        let out = run(
            ProjectAction::Delete {
                project_id: "proj_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            ProjectOutput::Deleted(d) => {
                assert!(d.deleted);
                assert_eq!(d.project_id, "proj_1");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_members_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_list_members()
            .withf(|req| req.get_ref().project_id == "proj_1")
            .times(1)
            .returning(|_| {
                Ok(client::ListMembersResponse {
                    members: vec![sample_member()],
                })
            });

        let out = run(
            ProjectAction::Member {
                action: MemberAction::List {
                    project_id: "proj_1".into(),
                },
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            ProjectOutput::Members(m) => assert_eq!(m.len(), 1),
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn add_member_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_add_member()
            .withf(|req| {
                let r = req.get_ref();
                r.project_id == "proj_1" && r.subject == "user:abc" && r.relation == "edit"
            })
            .times(1)
            .returning(|_| Ok(()));

        let out = run(
            ProjectAction::Member {
                action: MemberAction::Add {
                    project_id: "proj_1".into(),
                    subject: "abc@test".into(),
                    relation: "edit".into(),
                },
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            ProjectOutput::MemberAdded(a) => {
                assert!(a.added);
                assert_eq!(a.subject, "user:abc");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn remove_member_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_remove_member()
            .withf(|req| {
                let r = req.get_ref();
                r.project_id == "proj_1" && r.subject == "user:abc"
            })
            .times(1)
            .returning(|_| Ok(()));

        let out = run(
            ProjectAction::Member {
                action: MemberAction::Remove {
                    project_id: "proj_1".into(),
                    subject: "abc@test".into(),
                },
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            ProjectOutput::MemberRemoved(r) => {
                assert!(r.removed);
                assert_eq!(r.subject, "user:abc");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn create_project_with_all_options_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_create_project()
            .withf(|req| {
                let r = req.get_ref();
                r.name == "N"
                    && r.prefix == "PR"
                    && r.icon == "i"
                    && r.color == "c"
                    && r.description == "d"
                    && !r.idempotency_key.is_empty()
            })
            .times(1)
            .returning(|_| Ok(sample_project()));

        let out = run(
            ProjectAction::Create {
                name: "N".into(),
                prefix: "PR".into(),
                icon: Some("i".into()),
                color: Some("c".into()),
                description: Some("d".into()),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, ProjectOutput::Project(_)));
    }

    #[tokio::test]
    async fn update_project_with_all_options_returns() {
        let mut mock = MockProjectService::new();
        mock.expect_update_project()
            .withf(|req| {
                let r = req.get_ref();
                let paths = r.update_mask.as_ref().map(|m| m.paths.clone());
                paths
                    == Some(vec![
                        "name".into(),
                        "icon".into(),
                        "color".into(),
                        "description".into(),
                    ])
            })
            .times(1)
            .returning(|_| Ok(sample_project()));

        let out = run(
            ProjectAction::Update {
                project_id: "proj_1".into(),
                name: Some("N".into()),
                icon: Some("I".into()),
                color: Some("C".into()),
                description: Some("D".into()),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, ProjectOutput::Project(_)));
    }

    #[tokio::test]
    async fn get_project_by_name_resolves_through_list() {
        let mut mock = MockProjectService::new();
        mock.expect_list_projects()
            .withf(|req| req.get_ref() == &ListProjectsRequest {})
            .times(1)
            .returning(|_| {
                Ok(client::ListProjectsResponse {
                    projects: vec![sample_project()],
                })
            });
        mock.expect_get_project()
            .withf(|req| {
                req.get_ref().project_id == "proj_1"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("proj_1")
            })
            .times(1)
            .returning(|_| Ok(sample_project()));

        let out = run(
            ProjectAction::Get {
                project_id: "Sunbeam".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, ProjectOutput::Project(_)));
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

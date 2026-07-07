//! Kanban GitHub link commands.

use crate::error::{Result, ResultExt, SunbeamError};
use crate::kanban::client::{self, GithubLinkServiceClient};
use crate::logger::Logger;

use async_trait::async_trait;
use clap::Subcommand;
use serde::Serialize;
use tonic::metadata::MetadataValue;

/// GitHub link actions.
#[derive(Debug, Subcommand)]
pub enum GitHubAction {
    /// Link a GitHub issue/PR to a card.
    Link {
        /// Card ID.
        card_id: String,
        /// Issue reference: owner/repo#number.
        issue: String,
    },
    /// Unlink a GitHub issue/PR.
    Unlink {
        /// Card ID.
        #[arg(short, long)]
        card: String,
        /// Link ID.
        link_id: String,
    },
    /// List links for a card.
    List {
        /// Card ID.
        card_id: String,
    },
    /// Search GitHub issues.
    Search {
        /// Card ID.
        #[arg(short, long)]
        card: String,
        /// Repository: owner/repo.
        repo: String,
        /// Search query.
        query: String,
    },
    /// Resync a link.
    Resync {
        /// Card ID.
        #[arg(short, long)]
        card: String,
        /// Link ID.
        link_id: String,
    },
}

/// Result of running a GitHub link command.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum GitHubLinkOutput {
    /// New or refreshed link detail.
    Linked(GitHubLinkDetailOut),
    /// Unlink confirmation.
    Unlinked(GitHubLinkUnlinkOut),
    /// List of links for a card.
    Listed(Vec<GitHubLinkDetailOut>),
    /// Issue search results.
    Searched(Vec<GitHubIssueResultOut>),
}

/// Serializable GitHub link detail record.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GitHubLinkDetailOut {
    /// Link ID.
    pub id: String,
    /// Card ID.
    pub card_id: String,
    /// Repository owner.
    pub repo_owner: String,
    /// Repository name.
    pub repo_name: String,
    /// Issue or PR number.
    pub issue_or_pr_number: i32,
    /// Issue or PR.
    pub kind: String,
    /// Title.
    pub title: String,
    /// State.
    pub state: String,
    /// URL.
    pub url: String,
    /// Last sync timestamp.
    pub last_synced_at: Option<String>,
}

/// Serializable GitHub issue search result.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GitHubIssueResultOut {
    /// Repository owner.
    pub repo_owner: String,
    /// Repository name.
    pub repo_name: String,
    /// Issue/PR number.
    pub number: i32,
    /// Issue or PR.
    pub kind: String,
    /// Title.
    pub title: String,
    /// State.
    pub state: String,
    /// URL.
    pub url: String,
}

/// Unlink confirmation.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GitHubLinkUnlinkOut {
    /// Whether the unlink succeeded.
    pub unlinked: bool,
    /// Link ID.
    pub link_id: String,
}

/// Build a mutating request with object-id and idempotency headers.
fn mutating_request<T>(msg: T, object_id: &str) -> Result<tonic::Request<T>> {
    let mut req = client::request_with_object_id(msg, object_id)?;
    let key = crate::kanban::new_idempotency_key();
    let value = MetadataValue::try_from(key)
        .with_ctx(|| "invalid idempotency key (cannot encode as header)".to_string())?;
    req.metadata_mut()
        .insert("x-sunbeam-idempotency-key", value);
    Ok(req)
}

/// Format a Prost timestamp as RFC 3339.
fn fmt_timestamp(ts: &Option<prost_types::Timestamp>) -> Option<String> {
    ts.as_ref().and_then(|t| {
        chrono::DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32).map(|dt| dt.to_rfc3339())
    })
}

fn link_detail_out(l: client::GitHubLinkDetail) -> GitHubLinkDetailOut {
    GitHubLinkDetailOut {
        id: l.id,
        card_id: l.card_id,
        repo_owner: l.repo_owner,
        repo_name: l.repo_name,
        issue_or_pr_number: l.issue_or_pr_number,
        kind: l.kind,
        title: l.title,
        state: l.state,
        url: l.url,
        last_synced_at: fmt_timestamp(&l.last_synced_at),
    }
}

fn issue_result_out(i: client::GitHubIssueResult) -> GitHubIssueResultOut {
    GitHubIssueResultOut {
        repo_owner: i.repo_owner,
        repo_name: i.repo_name,
        number: i.number,
        kind: i.kind,
        title: i.title,
        state: i.state,
        url: i.url,
    }
}

/// Parse `owner/repo#number` into its components.
fn parse_issue_ref(issue: &str) -> Result<(String, String, i32)> {
    let (repo, num_str) = issue
        .rsplit_once('#')
        .with_ctx(|| format!("invalid issue reference {issue}: expected owner/repo#number"))?;
    let number = num_str
        .parse::<i32>()
        .map_err(|e| SunbeamError::Other(format!("invalid issue number {num_str}: {e}")))?;
    let (owner, name) = repo
        .split_once('/')
        .with_ctx(|| format!("invalid repository {repo}: expected owner/repo"))?;
    Ok((owner.to_string(), name.to_string(), number))
}

/// Parse `owner/repo` into its components.
fn parse_repo(repo: &str) -> Result<(String, String)> {
    let (owner, name) = repo
        .split_once('/')
        .with_ctx(|| format!("invalid repository {repo}: expected owner/repo"))?;
    Ok((owner.to_string(), name.to_string()))
}

/// Trait abstracting the Kanban GitHub link service for testability.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait GithubLinkService {
    /// Link a GitHub issue/PR to a card.
    async fn link_issue(
        &mut self,
        req: tonic::Request<client::LinkGitHubIssueRequest>,
    ) -> Result<client::GitHubLinkDetail>;
    /// Unlink a GitHub issue/PR.
    async fn unlink_issue(
        &mut self,
        req: tonic::Request<client::UnlinkGitHubIssueRequest>,
    ) -> Result<()>;
    /// List links for a card.
    async fn list_links_by_card(
        &mut self,
        req: tonic::Request<client::ListGitHubLinksByCardRequest>,
    ) -> Result<client::ListGitHubLinksByCardResponse>;
    /// Search GitHub issues.
    async fn search_github_issues(
        &mut self,
        req: tonic::Request<client::SearchGithubIssuesRequest>,
    ) -> Result<client::SearchGithubIssuesResponse>;
    /// Resync a link.
    async fn resync_link(
        &mut self,
        req: tonic::Request<client::ResyncGitHubLinkRequest>,
    ) -> Result<client::GitHubLinkDetail>;
}

/// Wrapper around the generated Tonic GitHub link client.
#[derive(Debug)]
pub struct GithubLinkServiceClientWrapper {
    inner: GithubLinkServiceClient<client::AuthChannel>,
}

impl GithubLinkServiceClientWrapper {
    /// Build a wrapper from an authenticated channel.
    pub fn new(inner: GithubLinkServiceClient<client::AuthChannel>) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl GithubLinkService for GithubLinkServiceClientWrapper {
    async fn link_issue(
        &mut self,
        req: tonic::Request<client::LinkGitHubIssueRequest>,
    ) -> Result<client::GitHubLinkDetail> {
        let resp = self.inner.link_issue(req).await?;
        Ok(resp.into_inner())
    }

    async fn unlink_issue(
        &mut self,
        req: tonic::Request<client::UnlinkGitHubIssueRequest>,
    ) -> Result<()> {
        self.inner.unlink_issue(req).await?;
        Ok(())
    }

    async fn list_links_by_card(
        &mut self,
        req: tonic::Request<client::ListGitHubLinksByCardRequest>,
    ) -> Result<client::ListGitHubLinksByCardResponse> {
        let resp = self.inner.list_links_by_card(req).await?;
        Ok(resp.into_inner())
    }

    async fn search_github_issues(
        &mut self,
        req: tonic::Request<client::SearchGithubIssuesRequest>,
    ) -> Result<client::SearchGithubIssuesResponse> {
        let resp = self.inner.search_github_issues(req).await?;
        Ok(resp.into_inner())
    }

    async fn resync_link(
        &mut self,
        req: tonic::Request<client::ResyncGitHubLinkRequest>,
    ) -> Result<client::GitHubLinkDetail> {
        let resp = self.inner.resync_link(req).await?;
        Ok(resp.into_inner())
    }
}

/// Build a GitHub-link-service client wrapper for the given server and token.
pub async fn build_client(
    logger: &Logger,
    server: &str,
    token: &str,
) -> Result<GithubLinkServiceClientWrapper> {
    let channel = client::build(logger, server, token).await?;
    Ok(GithubLinkServiceClientWrapper::new(
        GithubLinkServiceClient::new(channel),
    ))
}

/// Run a GitHub link command and return the result data.
pub async fn run(
    cmd: GitHubAction,
    client: &mut dyn GithubLinkService,
) -> Result<GitHubLinkOutput> {
    match cmd {
        GitHubAction::Link { card_id, issue } => {
            let (repo_owner, repo_name, number) = parse_issue_ref(&issue)?;
            let req = client::LinkGitHubIssueRequest {
                card_id: card_id.clone(),
                repo_owner,
                repo_name,
                number,
            };
            let resp = client.link_issue(mutating_request(req, &card_id)?).await?;
            Ok(GitHubLinkOutput::Linked(link_detail_out(resp)))
        }
        GitHubAction::Unlink { card, link_id } => {
            let req = client::UnlinkGitHubIssueRequest {
                link_id: link_id.clone(),
            };
            client.unlink_issue(mutating_request(req, &card)?).await?;
            Ok(GitHubLinkOutput::Unlinked(GitHubLinkUnlinkOut {
                unlinked: true,
                link_id,
            }))
        }
        GitHubAction::List { card_id } => {
            let req = client::ListGitHubLinksByCardRequest {
                card_id: card_id.clone(),
            };
            let resp = client
                .list_links_by_card(client::request_with_object_id(req, &card_id)?)
                .await?;
            let links: Vec<_> = resp.links.into_iter().map(link_detail_out).collect();
            Ok(GitHubLinkOutput::Listed(links))
        }
        GitHubAction::Search { card, repo, query } => {
            let (repo_owner, repo_name) = parse_repo(&repo)?;
            let req = client::SearchGithubIssuesRequest {
                repo_owner,
                repo_name,
                query,
                limit: 20,
            };
            let resp = client
                .search_github_issues(client::request_with_object_id(req, &card)?)
                .await?;
            let results: Vec<_> = resp.results.into_iter().map(issue_result_out).collect();
            Ok(GitHubLinkOutput::Searched(results))
        }
        GitHubAction::Resync { card, link_id } => {
            let req = client::ResyncGitHubLinkRequest {
                link_id: link_id.clone(),
            };
            let resp = client.resync_link(mutating_request(req, &card)?).await?;
            Ok(GitHubLinkOutput::Linked(link_detail_out(resp)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_link_detail(id: &str) -> client::GitHubLinkDetail {
        client::GitHubLinkDetail {
            id: id.into(),
            card_id: "card_1".into(),
            repo_owner: "sunbeam".into(),
            repo_name: "cli".into(),
            issue_or_pr_number: 42,
            kind: "issue".into(),
            title: "Fix bug".into(),
            state: "open".into(),
            url: "https://github.com/sunbeam/cli/issues/42".into(),
            last_synced_at: None,
        }
    }

    #[tokio::test]
    async fn link_returns_detail() {
        let mut mock = MockGithubLinkService::new();
        mock.expect_link_issue()
            .withf(|req| {
                let r = req.get_ref();
                r.card_id == "card_1"
                    && r.repo_owner == "sunbeam"
                    && r.repo_name == "cli"
                    && r.number == 42
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("card_1")
            })
            .times(1)
            .returning(|_| Ok(sample_link_detail("link_1")));

        let out = run(
            GitHubAction::Link {
                card_id: "card_1".into(),
                issue: "sunbeam/cli#42".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            GitHubLinkOutput::Linked(d) => {
                assert_eq!(d.id, "link_1");
                assert_eq!(d.issue_or_pr_number, 42);
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn unlink_returns_confirmation() {
        let mut mock = MockGithubLinkService::new();
        mock.expect_unlink_issue()
            .withf(|req| {
                req.get_ref().link_id == "link_1"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("card_1")
            })
            .times(1)
            .returning(|_| Ok(()));

        let out = run(
            GitHubAction::Unlink {
                card: "card_1".into(),
                link_id: "link_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            GitHubLinkOutput::Unlinked(u) => {
                assert!(u.unlinked);
                assert_eq!(u.link_id, "link_1");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn list_returns_links() {
        let mut mock = MockGithubLinkService::new();
        mock.expect_list_links_by_card()
            .withf(|req| {
                req.get_ref().card_id == "card_1"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("card_1")
            })
            .times(1)
            .returning(|_| {
                Ok(client::ListGitHubLinksByCardResponse {
                    links: vec![sample_link_detail("link_1")],
                })
            });

        let out = run(
            GitHubAction::List {
                card_id: "card_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            GitHubLinkOutput::Listed(links) => {
                assert_eq!(links.len(), 1);
                assert_eq!(links[0].id, "link_1");
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn search_returns_results() {
        let mut mock = MockGithubLinkService::new();
        mock.expect_search_github_issues()
            .withf(|req| {
                let r = req.get_ref();
                r.repo_owner == "sunbeam"
                    && r.repo_name == "cli"
                    && r.query == "crash"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("card_1")
            })
            .times(1)
            .returning(|_| {
                Ok(client::SearchGithubIssuesResponse {
                    results: vec![client::GitHubIssueResult {
                        repo_owner: "sunbeam".into(),
                        repo_name: "cli".into(),
                        number: 42,
                        kind: "issue".into(),
                        title: "Fix crash".into(),
                        state: "open".into(),
                        url: "https://github.com/sunbeam/cli/issues/42".into(),
                    }],
                })
            });

        let out = run(
            GitHubAction::Search {
                card: "card_1".into(),
                repo: "sunbeam/cli".into(),
                query: "crash".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        match out {
            GitHubLinkOutput::Searched(results) => {
                assert_eq!(results.len(), 1);
                assert_eq!(results[0].number, 42);
            }
            other => panic!("unexpected output: {other:?}"),
        }
    }

    #[tokio::test]
    async fn resync_returns_detail() {
        let mut mock = MockGithubLinkService::new();
        mock.expect_resync_link()
            .withf(|req| {
                req.get_ref().link_id == "link_1"
                    && req
                        .metadata()
                        .get("x-sunbeam-object-id")
                        .and_then(|v| v.to_str().ok())
                        == Some("card_1")
            })
            .times(1)
            .returning(|_| Ok(sample_link_detail("link_1")));

        let out = run(
            GitHubAction::Resync {
                card: "card_1".into(),
                link_id: "link_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, GitHubLinkOutput::Linked(_)));
    }

    #[tokio::test]
    async fn list_links_returns_multiple() {
        let mut mock = MockGithubLinkService::new();
        mock.expect_list_links_by_card().times(1).returning(|_| {
            Ok(client::ListGitHubLinksByCardResponse {
                links: vec![sample_link_detail("link_1")],
            })
        });

        let out = run(
            GitHubAction::List {
                card_id: "card_1".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, GitHubLinkOutput::Listed(_)));
    }

    #[tokio::test]
    async fn search_issues_returns_multiple() {
        let mut mock = MockGithubLinkService::new();
        mock.expect_search_github_issues().times(1).returning(|_| {
            Ok(client::SearchGithubIssuesResponse {
                results: vec![client::GitHubIssueResult {
                    repo_owner: "sunbeam".into(),
                    repo_name: "cli".into(),
                    number: 42,
                    kind: "issue".into(),
                    title: "Fix crash".into(),
                    state: "open".into(),
                    url: "https://github.com/sunbeam/cli/issues/42".into(),
                }],
            })
        });

        let out = run(
            GitHubAction::Search {
                card: "card_1".into(),
                repo: "sunbeam/cli".into(),
                query: "crash".into(),
            },
            &mut mock,
        )
        .await
        .unwrap();

        assert!(matches!(out, GitHubLinkOutput::Searched(_)));
    }

    #[test]
    fn parse_issue_ref_rejects_invalid_formats() {
        assert!(parse_issue_ref("no-hash").is_err());
        assert!(parse_issue_ref("owner/repo#not-a-number").is_err());
        assert!(parse_issue_ref("ownerrepo#1").is_err());
    }

    #[test]
    fn parse_repo_rejects_invalid_format() {
        assert!(parse_repo("no-slash").is_err());
    }

    #[test]
    fn fmt_timestamp_negative_nanos() {
        let ts = prost_types::Timestamp {
            seconds: 1,
            nanos: -1,
        };
        assert!(super::fmt_timestamp(&Some(ts)).is_some());
    }

    #[tokio::test]
    async fn build_client_rejects_invalid_url() {
        let logger = crate::logger::Logger::new(crate::logger::NoopSink);
        let err = build_client(&logger, ":::bad", "token").await.unwrap_err();
        assert!(err.to_string().contains("invalid kanban server URL"));
    }
}

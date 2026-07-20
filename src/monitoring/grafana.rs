//! Grafana — dashboards and provisioning API client (`/api`).
//!
//! Build a shared [`sunbeam_g2v::client::Client`] and pass it to
//! [`GrafanaClient::new`], or use [`GrafanaClient::connect`] for an
//! unauthenticated client derived from the active domain.

use http::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sunbeam_g2v::client::{Client, ClientBuilder, RestClient};

use super::types::{self, *};
use crate::error::{Result, SunbeamError};

/// Client for the Grafana HTTP API (`/api`).
pub struct GrafanaClient {
    client: Client,
}

impl GrafanaClient {
    /// Wrap a g2v client configured against the Grafana `/api` base URL.
    ///
    /// If the base URL contains a path, it must end with a trailing slash —
    /// otherwise the last segment is dropped when request paths are joined.
    pub fn new(client: &Client) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Build an unauthenticated client from domain
    /// (e.g. `https://metrics.{domain}/api`).
    pub fn connect(domain: &str) -> Result<Self> {
        let client = ClientBuilder::new(format!("https://metrics.{domain}/api/"))
            .build()
            .map_err(|e| SunbeamError::Other(e.to_string()))?;
        Ok(Self::new(&client))
    }

    /// The base URL this client is configured against.
    pub fn base_url(&self) -> &str {
        self.client.base_url().as_str().trim_end_matches('/')
    }

    fn rest(&self) -> RestClient {
        self.client.rest()
    }

    // -- Dashboards ---------------------------------------------------------

    /// Create a new dashboard.
    pub async fn create_dashboard(&self, body: &Value) -> Result<DashboardResponse> {
        self.request_json(
            Method::POST,
            "dashboards/db",
            Some(body),
            "grafana create dashboard",
        )
        .await
    }

    /// Get a dashboard by UID.
    pub async fn get_dashboard(&self, uid: &str) -> Result<DashboardResponse> {
        self.request_json(
            Method::GET,
            &format!("dashboards/uid/{uid}"),
            None,
            "grafana get dashboard",
        )
        .await
    }

    /// Update an existing dashboard (same endpoint as create).
    pub async fn update_dashboard(&self, body: &Value) -> Result<DashboardResponse> {
        self.request_json(
            Method::POST,
            "dashboards/db",
            Some(body),
            "grafana update dashboard",
        )
        .await
    }

    /// Delete a dashboard by UID.
    pub async fn delete_dashboard(&self, uid: &str) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("dashboards/uid/{uid}"),
            None,
            "grafana delete dashboard",
        )
        .await
    }

    /// List all dashboards.
    pub async fn list_dashboards(&self) -> Result<Vec<DashboardSearchResult>> {
        self.request_json(
            Method::GET,
            "search?type=dash-db",
            None,
            "grafana list dashboards",
        )
        .await
    }

    /// Search dashboards by query.
    pub async fn search_dashboards(&self, query: &str) -> Result<Vec<DashboardSearchResult>> {
        self.request_json(
            Method::GET,
            &format!("search?query={}", types::urlencode(query)),
            None,
            "grafana search dashboards",
        )
        .await
    }

    // -- Datasources --------------------------------------------------------

    /// List all datasources.
    pub async fn list_datasources(&self) -> Result<Vec<Datasource>> {
        self.request_json(Method::GET, "datasources", None, "grafana list datasources")
            .await
    }

    /// Get a datasource by numeric ID.
    pub async fn get_datasource(&self, id: u64) -> Result<Datasource> {
        self.request_json(
            Method::GET,
            &format!("datasources/{id}"),
            None,
            "grafana get datasource",
        )
        .await
    }

    /// Get a datasource by UID.
    pub async fn get_datasource_by_uid(&self, uid: &str) -> Result<Datasource> {
        self.request_json(
            Method::GET,
            &format!("datasources/uid/{uid}"),
            None,
            "grafana get datasource by uid",
        )
        .await
    }

    /// Create a new datasource.
    pub async fn create_datasource(&self, body: &Value) -> Result<Datasource> {
        // Grafana wraps create response in {"datasource": {...}}
        let resp: Value = self
            .request_json(
                Method::POST,
                "datasources",
                Some(body),
                "grafana create datasource",
            )
            .await?;
        if let Some(inner) = resp.get("datasource") {
            Ok(serde_json::from_value(inner.clone())?)
        } else {
            Ok(serde_json::from_value(resp)?)
        }
    }

    /// Update an existing datasource by numeric ID.
    pub async fn update_datasource(&self, id: u64, body: &Value) -> Result<Datasource> {
        // Grafana wraps update response in {"datasource": {...}}
        let resp: Value = self
            .request_json(
                Method::PUT,
                &format!("datasources/{id}"),
                Some(body),
                "grafana update datasource",
            )
            .await?;
        if let Some(inner) = resp.get("datasource") {
            Ok(serde_json::from_value(inner.clone())?)
        } else {
            Ok(serde_json::from_value(resp)?)
        }
    }

    /// Delete a datasource by numeric ID.
    pub async fn delete_datasource(&self, id: u64) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("datasources/{id}"),
            None,
            "grafana delete datasource",
        )
        .await
    }

    /// Delete a datasource by UID.
    ///
    /// Grafana's numeric-ID endpoints are deprecated and removed in recent
    /// releases; prefer this UID-based variant.
    pub async fn delete_datasource_by_uid(&self, uid: &str) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("datasources/uid/{uid}"),
            None,
            "grafana delete datasource by uid",
        )
        .await
    }

    /// Proxy a request to a datasource.
    pub async fn proxy_datasource(&self, id: u64, path: &str) -> Result<Value> {
        self.request_json(
            Method::GET,
            &format!("datasources/proxy/{id}/{}", path.trim_start_matches('/')),
            None,
            "grafana proxy datasource",
        )
        .await
    }

    // -- Folders ------------------------------------------------------------

    /// List all folders.
    pub async fn list_folders(&self) -> Result<Vec<Folder>> {
        self.request_json(Method::GET, "folders", None, "grafana list folders")
            .await
    }

    /// Create a folder.
    pub async fn create_folder(&self, body: &Value) -> Result<Folder> {
        self.request_json(Method::POST, "folders", Some(body), "grafana create folder")
            .await
    }

    /// Get a folder by UID.
    pub async fn get_folder(&self, uid: &str) -> Result<Folder> {
        self.request_json(
            Method::GET,
            &format!("folders/{uid}"),
            None,
            "grafana get folder",
        )
        .await
    }

    /// Update a folder by UID.
    pub async fn update_folder(&self, uid: &str, body: &Value) -> Result<Folder> {
        self.request_json(
            Method::PUT,
            &format!("folders/{uid}"),
            Some(body),
            "grafana update folder",
        )
        .await
    }

    /// Delete a folder by UID.
    pub async fn delete_folder(&self, uid: &str) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("folders/{uid}"),
            None,
            "grafana delete folder",
        )
        .await
    }

    // -- Annotations --------------------------------------------------------

    /// List annotations with optional filter params.
    pub async fn list_annotations(&self, params: Option<&str>) -> Result<Vec<Annotation>> {
        let path = match params {
            Some(p) => format!("annotations?{p}"),
            None => "annotations".to_string(),
        };
        self.request_json(Method::GET, &path, None, "grafana list annotations")
            .await
    }

    /// Create an annotation.
    pub async fn create_annotation(&self, body: &Value) -> Result<AnnotationResponse> {
        self.request_json(
            Method::POST,
            "annotations",
            Some(body),
            "grafana create annotation",
        )
        .await
    }

    /// Get an annotation by ID.
    pub async fn get_annotation(&self, id: u64) -> Result<Annotation> {
        self.request_json(
            Method::GET,
            &format!("annotations/{id}"),
            None,
            "grafana get annotation",
        )
        .await
    }

    /// Update an annotation by ID.
    pub async fn update_annotation(&self, id: u64, body: &Value) -> Result<AnnotationResponse> {
        self.request_json(
            Method::PUT,
            &format!("annotations/{id}"),
            Some(body),
            "grafana update annotation",
        )
        .await
    }

    /// Delete an annotation by ID.
    pub async fn delete_annotation(&self, id: u64) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("annotations/{id}"),
            None,
            "grafana delete annotation",
        )
        .await
    }

    // -- Alerts -------------------------------------------------------------

    /// Get all provisioned alert rules.
    pub async fn get_alert_rules(&self) -> Result<Vec<AlertRule>> {
        self.request_json(
            Method::GET,
            "v1/provisioning/alert-rules",
            None,
            "grafana get alert rules",
        )
        .await
    }

    /// Create a provisioned alert rule.
    pub async fn create_alert_rule(&self, body: &Value) -> Result<AlertRule> {
        self.request_json(
            Method::POST,
            "v1/provisioning/alert-rules",
            Some(body),
            "grafana create alert rule",
        )
        .await
    }

    /// Update a provisioned alert rule by UID.
    pub async fn update_alert_rule(&self, uid: &str, body: &Value) -> Result<AlertRule> {
        self.request_json(
            Method::PUT,
            &format!("v1/provisioning/alert-rules/{uid}"),
            Some(body),
            "grafana update alert rule",
        )
        .await
    }

    /// Delete a provisioned alert rule by UID.
    pub async fn delete_alert_rule(&self, uid: &str) -> Result<()> {
        self.request_send(
            Method::DELETE,
            &format!("v1/provisioning/alert-rules/{uid}"),
            None,
            "grafana delete alert rule",
        )
        .await
    }

    // -- Org ----------------------------------------------------------------

    /// Get the current organization.
    pub async fn get_current_org(&self) -> Result<Organization> {
        self.request_json(Method::GET, "org", None, "grafana get current org")
            .await
    }

    /// Update the current organization.
    pub async fn update_org(&self, body: &Value) -> Result<()> {
        self.request_send(Method::PUT, "org", Some(body), "grafana update org")
            .await
    }

    // -- Internal helpers ---------------------------------------------------

    /// Send a request with an optional JSON body, error on non-2xx, parse the
    /// response as JSON.
    async fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        ctx: &str,
    ) -> Result<T> {
        let mut req = self.rest().request(method, path)?;
        if let Some(b) = body {
            req = req
                .header(http::header::CONTENT_TYPE, "application/json")?
                .json(b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        let bytes = resp.into_body();
        if !status.is_success() {
            return Err(SunbeamError::network(format!(
                "{ctx}: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Send a request with an optional JSON body, error on non-2xx, discard
    /// the response body.
    async fn request_send(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
        ctx: &str,
    ) -> Result<()> {
        let mut req = self.rest().request(method, path)?;
        if let Some(b) = body {
            req = req
                .header(http::header::CONTENT_TYPE, "application/json")?
                .json(b);
        }
        let resp = req.send().await?;
        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.into_body();
            return Err(SunbeamError::network(format!(
                "{ctx}: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_url() {
        let c = GrafanaClient::connect("sunbeam.pt").unwrap();
        assert_eq!(c.base_url(), "https://metrics.sunbeam.pt/api");
    }

    #[test]
    fn test_new_from_g2v_client() {
        let client = ClientBuilder::new("http://localhost:3000/api")
            .build()
            .unwrap();
        let c = GrafanaClient::new(&client);
        assert_eq!(c.base_url(), "http://localhost:3000/api");
    }
}

#[cfg(all(test, feature = "testing"))]
mod container_tests {
    use std::time::Duration;

    use base64::Engine;
    use serde_json::json;
    use sunbeam_g2v::client::ClientBuilder;

    use super::GrafanaClient;
    use crate::testing::Grafana;

    /// Boot a Grafana container and return a client using basic auth with the
    /// default admin credentials.
    async fn boot() -> (
        testcontainers::ContainerAsync<testcontainers::GenericImage>,
        GrafanaClient,
    ) {
        let container = Grafana::default()
            .publish_ports()
            .start()
            .await
            .expect("grafana should start");
        let url = Grafana::url(&container).await.expect("url should resolve");

        let basic = base64::engine::general_purpose::STANDARD.encode(format!(
            "{}:{}",
            Grafana::ADMIN_USER,
            Grafana::ADMIN_PASSWORD
        ));
        let g2v = ClientBuilder::new(format!("{url}/api/"))
            .default_header("Authorization", format!("Basic {basic}"))
            .expect("auth header")
            .build()
            .expect("g2v client");
        let client = GrafanaClient::new(&g2v);

        for _ in 0..30 {
            if client.get_current_org().await.is_ok() {
                return (container, client);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        panic!("grafana did not become ready");
    }

    #[tokio::test]
    async fn grafana_datasource_lifecycle() {
        let (_container, client) = boot().await;

        let org = client.get_current_org().await.expect("current org");
        assert_eq!(org.id, 1);

        let ds = client
            .create_datasource(&json!({
                "name": "sdk-test-prometheus",
                "type": "prometheus",
                "url": "http://prometheus:9090",
                "access": "proxy",
            }))
            .await
            .expect("create datasource");
        let uid = ds.uid.clone().expect("datasource uid");

        let listed = client.list_datasources().await.expect("list datasources");
        assert!(listed.iter().any(|d| d.name == "sdk-test-prometheus"));

        let fetched = client
            .get_datasource_by_uid(&uid)
            .await
            .expect("get datasource by uid");
        assert_eq!(fetched.name, "sdk-test-prometheus");

        client
            .delete_datasource_by_uid(&uid)
            .await
            .expect("delete datasource");
        let listed = client.list_datasources().await.expect("list after delete");
        assert!(!listed.iter().any(|d| d.name == "sdk-test-prometheus"));
    }
}

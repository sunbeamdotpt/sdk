//! Prometheus — metrics API client (`/api/v1`).
//!
//! Build a shared [`sunbeam_g2v::client::Client`] and pass it to
//! [`PrometheusClient::new`], or use [`PrometheusClient::connect`] for an
//! unauthenticated client derived from the active domain.

use http::Method;
use serde::de::DeserializeOwned;
use sunbeam_g2v::client::{Client, ClientBuilder, RestClient};

use super::types::{self, *};
use crate::error::{Result, SunbeamError};

/// Client for the Prometheus HTTP API (`/api/v1`).
pub struct PrometheusClient {
    client: Client,
}

impl PrometheusClient {
    /// Wrap a g2v client configured against the Prometheus `/api/v1` base URL.
    ///
    /// If the base URL contains a path, it must end with a trailing slash —
    /// otherwise the last segment is dropped when request paths are joined.
    pub fn new(client: &Client) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Build an unauthenticated client from domain
    /// (e.g. `https://systemmetrics.{domain}/api/v1`).
    pub fn connect(domain: &str) -> Result<Self> {
        let client = ClientBuilder::new(format!("https://systemmetrics.{domain}/api/v1/"))
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

    /// GET a path and parse the JSON response, erroring on non-2xx.
    async fn get_json<T: DeserializeOwned>(&self, path: &str, ctx: &str) -> Result<T> {
        let resp = self.rest().request(Method::GET, path)?.send().await?;
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

    // -- Query --------------------------------------------------------------

    /// Execute an instant query.
    pub async fn query(&self, query: &str, time: Option<&str>) -> Result<QueryResult> {
        let mut path = format!("query?query={}", types::urlencode(query));
        if let Some(t) = time {
            path.push_str(&format!("&time={t}"));
        }
        self.get_json(&path, "prometheus query").await
    }

    /// Execute a range query.
    pub async fn query_range(
        &self,
        query: &str,
        start: &str,
        end: &str,
        step: &str,
    ) -> Result<QueryResult> {
        let path = format!(
            "query_range?query={}&start={start}&end={end}&step={step}",
            types::urlencode(query),
        );
        self.get_json(&path, "prometheus query_range").await
    }

    /// Format a PromQL expression.
    pub async fn format_query(&self, query: &str) -> Result<FormattedQuery> {
        let path = format!("format_query?query={}", types::urlencode(query));
        self.get_json(&path, "prometheus format_query").await
    }

    // -- Metadata -----------------------------------------------------------

    /// Find series matching label matchers.
    pub async fn series(
        &self,
        match_params: &[&str],
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<ApiResponse<Vec<serde_json::Value>>> {
        let mut path = String::from("series?");
        for (i, m) in match_params.iter().enumerate() {
            if i > 0 {
                path.push('&');
            }
            path.push_str(&format!("match[]={}", types::urlencode(m)));
        }
        if let Some(s) = start {
            path.push_str(&format!("&start={s}"));
        }
        if let Some(e) = end {
            path.push_str(&format!("&end={e}"));
        }
        self.get_json(&path, "prometheus series").await
    }

    /// Get all label names.
    pub async fn labels(
        &self,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<ApiResponse<Vec<String>>> {
        let mut path = String::from("labels");
        let mut sep = '?';
        if let Some(s) = start {
            path.push_str(&format!("{sep}start={s}"));
            sep = '&';
        }
        if let Some(e) = end {
            path.push_str(&format!("{sep}end={e}"));
        }
        self.get_json(&path, "prometheus labels").await
    }

    /// Get values for a specific label.
    pub async fn label_values(
        &self,
        label: &str,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<ApiResponse<Vec<String>>> {
        let mut path = format!("label/{label}/values");
        let mut sep = '?';
        if let Some(s) = start {
            path.push_str(&format!("{sep}start={s}"));
            sep = '&';
        }
        if let Some(e) = end {
            path.push_str(&format!("{sep}end={e}"));
        }
        self.get_json(&path, "prometheus label values").await
    }

    /// Get metadata about metrics scraped by targets.
    pub async fn targets_metadata(
        &self,
        metric: Option<&str>,
    ) -> Result<ApiResponse<Vec<serde_json::Value>>> {
        let mut path = String::from("targets/metadata");
        if let Some(m) = metric {
            path.push_str(&format!("?metric={}", types::urlencode(m)));
        }
        self.get_json(&path, "prometheus targets metadata").await
    }

    /// Get per-metric metadata.
    pub async fn metadata(&self, metric: Option<&str>) -> Result<ApiResponse<serde_json::Value>> {
        let mut path = String::from("metadata");
        if let Some(m) = metric {
            path.push_str(&format!("?metric={}", types::urlencode(m)));
        }
        self.get_json(&path, "prometheus metadata").await
    }

    // -- Infrastructure -----------------------------------------------------

    /// Get current target discovery status.
    pub async fn targets(&self) -> Result<TargetsResult> {
        self.get_json("targets", "prometheus targets").await
    }

    /// List scrape pools.
    pub async fn scrape_pools(&self) -> Result<ApiResponse<serde_json::Value>> {
        self.get_json("scrape_pools", "prometheus scrape_pools")
            .await
    }

    /// Get discovered Alertmanager instances.
    pub async fn alertmanagers(&self) -> Result<ApiResponse<serde_json::Value>> {
        self.get_json("alertmanagers", "prometheus alertmanagers")
            .await
    }

    /// Get alerting and recording rules.
    pub async fn rules(&self) -> Result<RulesResult> {
        self.get_json("rules", "prometheus rules").await
    }

    /// Get active alerts.
    pub async fn alerts(&self) -> Result<AlertsResult> {
        self.get_json("alerts", "prometheus alerts").await
    }

    // -- Status -------------------------------------------------------------

    /// Get Prometheus configuration.
    pub async fn config(&self) -> Result<ConfigResult> {
        self.get_json("status/config", "prometheus config").await
    }

    /// Get command-line flags.
    pub async fn flags(&self) -> Result<ApiResponse<serde_json::Value>> {
        self.get_json("status/flags", "prometheus flags").await
    }

    /// Get runtime information.
    pub async fn runtime_info(&self) -> Result<ApiResponse<serde_json::Value>> {
        self.get_json("status/runtimeinfo", "prometheus runtimeinfo")
            .await
    }

    /// Get build information.
    pub async fn build_info(&self) -> Result<ApiResponse<serde_json::Value>> {
        self.get_json("status/buildinfo", "prometheus buildinfo")
            .await
    }

    /// Get TSDB statistics.
    pub async fn tsdb(&self) -> Result<ApiResponse<serde_json::Value>> {
        self.get_json("status/tsdb", "prometheus tsdb").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_url() {
        let c = PrometheusClient::connect("sunbeam.pt").unwrap();
        assert_eq!(c.base_url(), "https://systemmetrics.sunbeam.pt/api/v1");
    }

    #[test]
    fn test_new_from_g2v_client() {
        let client = ClientBuilder::new("http://localhost:9090/api/v1")
            .build()
            .unwrap();
        let c = PrometheusClient::new(&client);
        assert_eq!(c.base_url(), "http://localhost:9090/api/v1");
    }
}

#[cfg(all(test, feature = "testing"))]
mod container_tests {
    use std::time::Duration;

    use sunbeam_g2v::client::ClientBuilder;

    use super::PrometheusClient;
    use crate::testing::Prometheus;

    /// Boot a Prometheus container and return a client once `/api/v1` answers.
    async fn boot() -> (
        testcontainers::ContainerAsync<testcontainers::GenericImage>,
        PrometheusClient,
    ) {
        let container = Prometheus::default()
            .publish_ports()
            .start()
            .await
            .expect("prometheus should start");
        let url = Prometheus::url(&container)
            .await
            .expect("url should resolve");
        let g2v = ClientBuilder::new(format!("{url}/api/v1/"))
            .build()
            .expect("g2v client");
        let client = PrometheusClient::new(&g2v);

        let mut last_err = String::new();
        for _ in 0..30 {
            match client.query("up", None).await {
                Ok(_) => return (container, client),
                Err(e) => last_err = e.to_string(),
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        panic!("prometheus did not become ready (last error: {last_err})");
    }

    #[tokio::test]
    async fn prometheus_queries() {
        let (_container, client) = boot().await;

        let result = client.query("up", None).await.expect("instant query");
        assert_eq!(result.status, "success");

        let targets = client.targets().await.expect("targets");
        assert_eq!(targets.status, "success");

        let build = client.build_info().await.expect("build info");
        assert_eq!(build.status, "success");
    }
}

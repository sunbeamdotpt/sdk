//! Loki — log aggregation API client (`/loki/api/v1`).
//!
//! Build a shared [`sunbeam_g2v::client::Client`] and pass it to
//! [`LokiClient::new`], or use [`LokiClient::connect`] for an unauthenticated
//! client derived from the active domain.

use http::Method;
use serde::de::DeserializeOwned;
use sunbeam_g2v::client::{Client, ClientBuilder, RestClient};

use super::types::{self, *};
use crate::error::{Result, SunbeamError};

/// Client for the Loki HTTP API (`/loki/api/v1`).
pub struct LokiClient {
    client: Client,
}

impl LokiClient {
    /// Wrap a g2v client configured against the Loki `/loki/api/v1` base URL.
    ///
    /// If the base URL contains a path, it must end with a trailing slash —
    /// otherwise the last segment is dropped when request paths are joined.
    pub fn new(client: &Client) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Build an unauthenticated client from domain
    /// (e.g. `https://systemlogs.{domain}/loki/api/v1`).
    pub fn connect(domain: &str) -> Result<Self> {
        let client = ClientBuilder::new(format!("https://systemlogs.{domain}/loki/api/v1/"))
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
    pub async fn query(
        &self,
        query: &str,
        limit: Option<u32>,
        time: Option<&str>,
    ) -> Result<QueryResult> {
        let mut path = format!("query?query={}", types::urlencode(query));
        if let Some(l) = limit {
            path.push_str(&format!("&limit={l}"));
        }
        if let Some(t) = time {
            path.push_str(&format!("&time={t}"));
        }
        self.get_json(&path, "loki query").await
    }

    /// Execute a range query.
    pub async fn query_range(
        &self,
        query: &str,
        start: &str,
        end: &str,
        limit: Option<u32>,
        step: Option<&str>,
    ) -> Result<QueryResult> {
        let mut path = format!(
            "query_range?query={}&start={start}&end={end}",
            types::urlencode(query),
        );
        if let Some(l) = limit {
            path.push_str(&format!("&limit={l}"));
        }
        if let Some(s) = step {
            path.push_str(&format!("&step={s}"));
        }
        self.get_json(&path, "loki query_range").await
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
        self.get_json(&path, "loki labels").await
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
        self.get_json(&path, "loki label values").await
    }

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
        self.get_json(&path, "loki series").await
    }

    // -- Index --------------------------------------------------------------

    /// Get index statistics.
    pub async fn index_stats(&self) -> Result<serde_json::Value> {
        self.get_json("index/stats", "loki index stats").await
    }

    /// Get index volume for a query.
    pub async fn index_volume(
        &self,
        query: &str,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut path = format!("index/volume?query={}", types::urlencode(query));
        if let Some(s) = start {
            path.push_str(&format!("&start={s}"));
        }
        if let Some(e) = end {
            path.push_str(&format!("&end={e}"));
        }
        self.get_json(&path, "loki index volume").await
    }

    /// Get index volume range for a query.
    pub async fn index_volume_range(
        &self,
        query: &str,
        start: &str,
        end: &str,
        step: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut path = format!(
            "index/volume_range?query={}&start={start}&end={end}",
            types::urlencode(query),
        );
        if let Some(s) = step {
            path.push_str(&format!("&step={s}"));
        }
        self.get_json(&path, "loki index volume_range").await
    }

    // -- Patterns -----------------------------------------------------------

    /// Detect log patterns.
    pub async fn detect_patterns(
        &self,
        query: &str,
        start: Option<&str>,
        end: Option<&str>,
    ) -> Result<serde_json::Value> {
        let mut path = format!("patterns?query={}", types::urlencode(query));
        if let Some(s) = start {
            path.push_str(&format!("&start={s}"));
        }
        if let Some(e) = end {
            path.push_str(&format!("&end={e}"));
        }
        self.get_json(&path, "loki detect patterns").await
    }

    // -- Ingest -------------------------------------------------------------

    /// Push log entries.
    pub async fn push(&self, body: &serde_json::Value) -> Result<()> {
        let resp = self
            .rest()
            .request(Method::POST, "push")?
            .header(http::header::CONTENT_TYPE, "application/json")?
            .json(body)
            .send()
            .await?;
        let status = resp.status();
        if !status.is_success() {
            let bytes = resp.into_body();
            return Err(SunbeamError::network(format!(
                "loki push: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        Ok(())
    }

    // -- Status -------------------------------------------------------------

    /// Check readiness. Note: Loki's `/ready` is at the server root,
    /// not under the API prefix.
    pub async fn ready(&self) -> Result<ReadyStatus> {
        // An absolute path resolves against the origin root, bypassing the
        // `/loki/api/v1` base path.
        let resp = self.rest().request(Method::GET, "/ready")?.send().await?;
        let status = resp.status();
        let bytes = resp.into_body();
        if !status.is_success() {
            return Err(SunbeamError::network(format!(
                "loki ready: HTTP {status}: {}",
                String::from_utf8_lossy(&bytes)
            )));
        }
        let body = String::from_utf8_lossy(&bytes);
        if body.trim() != "ready" {
            return Err(SunbeamError::network(format!("loki ready: {body}")));
        }
        Ok(ReadyStatus {
            status: Some("ready".into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_url() {
        let c = LokiClient::connect("sunbeam.pt").unwrap();
        assert_eq!(c.base_url(), "https://systemlogs.sunbeam.pt/loki/api/v1");
    }

    #[test]
    fn test_new_from_g2v_client() {
        let client = ClientBuilder::new("http://localhost:3100/loki/api/v1")
            .build()
            .unwrap();
        let c = LokiClient::new(&client);
        assert_eq!(c.base_url(), "http://localhost:3100/loki/api/v1");
    }
}

#[cfg(all(test, feature = "testing"))]
mod container_tests {
    use std::time::Duration;

    use serde_json::json;
    use sunbeam_g2v::client::ClientBuilder;

    use super::LokiClient;
    use crate::testing::Loki;

    /// Boot a Loki container and return a client once `/ready` answers.
    async fn boot() -> (
        testcontainers::ContainerAsync<testcontainers::GenericImage>,
        LokiClient,
    ) {
        let container = Loki::default()
            .publish_ports()
            .start()
            .await
            .expect("loki should start");
        let url = Loki::url(&container).await.expect("url should resolve");
        let g2v = ClientBuilder::new(format!("{url}/loki/api/v1/"))
            .build()
            .expect("g2v client");
        let client = LokiClient::new(&g2v);

        for _ in 0..30 {
            if client.ready().await.is_ok() {
                return (container, client);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        panic!("loki did not become ready");
    }

    #[tokio::test]
    async fn loki_push_and_query() {
        let (_container, client) = boot().await;

        let now_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
            .to_string();
        client
            .push(&json!({
                "streams": [{
                    "stream": {"job": "sdk-test"},
                    "values": [[now_ns, "hello from sdk"]],
                }],
            }))
            .await
            .expect("push");

        // Ingestion is asynchronous; retry until the line is queryable.
        // Note: plain selector queries are only valid as range queries.
        let start = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_secs()
            - 60;
        let mut found = false;
        let mut last = String::new();
        for _ in 0..15 {
            let end = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_secs();
            match client
                .query_range(
                    "{job=\"sdk-test\"}",
                    &start.to_string(),
                    &end.to_string(),
                    Some(10),
                    None,
                )
                .await
            {
                Ok(res) => {
                    last = format!(
                        "ok: {}",
                        serde_json::to_string(&res.data).unwrap_or_default()
                    );
                    if res.status == "success" && last.contains("hello from sdk") {
                        found = true;
                        break;
                    }
                }
                Err(e) => last = e.to_string(),
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        assert!(found, "pushed log line should be queryable (last: {last})");
    }
}

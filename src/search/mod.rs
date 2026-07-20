//! OpenSearch — search and analytics API client.
//!
//! Build a shared [`sunbeam_g2v::client::Client`] (e.g. with
//! `ClientBuilder::new(url).auth(BearerToken::new(token))`) and pass it to
//! [`OpenSearchClient::new`], or use [`OpenSearchClient::connect`] for an
//! unauthenticated client derived from the active domain.

#[allow(missing_docs)]
pub mod types;

use http::Method;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sunbeam_g2v::client::{Client, ClientBuilder, RestClient};
use types::*;

use crate::error::{Result, SunbeamError};

/// Client for the OpenSearch HTTP API.
pub struct OpenSearchClient {
    client: Client,
}

impl OpenSearchClient {
    /// Wrap a g2v client configured against the OpenSearch base URL.
    ///
    /// If the base URL contains a path, it must end with a trailing slash —
    /// otherwise the last segment is dropped when request paths are joined.
    pub fn new(client: &Client) -> Self {
        Self {
            client: client.clone(),
        }
    }

    /// Build an unauthenticated client from domain
    /// (e.g. `https://search.{domain}`).
    pub fn connect(domain: &str) -> Result<Self> {
        let client = ClientBuilder::new(format!("https://search.{domain}"))
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

    // -----------------------------------------------------------------------
    // Documents
    // -----------------------------------------------------------------------

    /// Index a document with an explicit ID.
    pub async fn index_doc(&self, index: &str, id: &str, body: &Value) -> Result<IndexResponse> {
        self.request_json(Method::PUT, &format!("{index}/_doc/{id}"), Some(body))
            .await
    }

    /// Index a document with an auto-generated ID.
    pub async fn index_doc_auto_id(&self, index: &str, body: &Value) -> Result<IndexResponse> {
        self.request_json(Method::POST, &format!("{index}/_doc"), Some(body))
            .await
    }

    /// Get a document by ID.
    pub async fn get_doc(&self, index: &str, id: &str) -> Result<GetResponse> {
        self.request_json(Method::GET, &format!("{index}/_doc/{id}"), None)
            .await
    }

    /// Check if a document exists (HEAD request, returns bool).
    pub async fn head_doc(&self, index: &str, id: &str) -> Result<bool> {
        let resp = self
            .rest()
            .request(Method::HEAD, &format!("{index}/_doc/{id}"))?
            .send()
            .await?;
        Ok(resp.status().is_success())
    }

    /// Delete a document by ID.
    pub async fn delete_doc(&self, index: &str, id: &str) -> Result<DeleteResponse> {
        self.request_json(Method::DELETE, &format!("{index}/_doc/{id}"), None)
            .await
    }

    /// Update a document by ID.
    pub async fn update_doc(&self, index: &str, id: &str, body: &Value) -> Result<UpdateResponse> {
        self.request_json(Method::POST, &format!("{index}/_update/{id}"), Some(body))
            .await
    }

    /// Bulk index/update/delete operations.
    pub async fn bulk(&self, body: &Value) -> Result<BulkResponse> {
        self.request_json(Method::POST, "_bulk", Some(body)).await
    }

    /// Multi-get documents.
    pub async fn multi_get(&self, body: &Value) -> Result<MultiGetResponse> {
        self.request_json(Method::POST, "_mget", Some(body)).await
    }

    /// Reindex documents from one index to another.
    pub async fn reindex(&self, body: &Value) -> Result<ReindexResponse> {
        self.request_json(Method::POST, "_reindex", Some(body))
            .await
    }

    /// Delete documents matching a query.
    pub async fn delete_by_query(
        &self,
        index: &str,
        body: &Value,
    ) -> Result<DeleteByQueryResponse> {
        self.request_json(
            Method::POST,
            &format!("{index}/_delete_by_query"),
            Some(body),
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Search
    // -----------------------------------------------------------------------

    /// Search an index.
    pub async fn search(&self, index: &str, body: &Value) -> Result<SearchResponse> {
        self.request_json(Method::POST, &format!("{index}/_search"), Some(body))
            .await
    }

    /// Search across all indices.
    pub async fn search_all(&self, body: &Value) -> Result<SearchResponse> {
        self.request_json(Method::POST, "_search", Some(body)).await
    }

    /// Multi-search.
    pub async fn multi_search(&self, body: &Value) -> Result<MultiSearchResponse> {
        self.request_json(Method::POST, "_msearch", Some(body))
            .await
    }

    /// Count documents matching a query.
    pub async fn count(&self, index: &str, body: &Value) -> Result<CountResponse> {
        self.request_json(Method::POST, &format!("{index}/_count"), Some(body))
            .await
    }

    /// Scroll through search results.
    pub async fn scroll(&self, body: &Value) -> Result<SearchResponse> {
        self.request_json(Method::POST, "_search/scroll", Some(body))
            .await
    }

    /// Clear a scroll context.
    pub async fn clear_scroll(&self, body: &Value) -> Result<()> {
        self.request_send(Method::DELETE, "_search/scroll", Some(body))
            .await
    }

    /// Get search shard information.
    pub async fn search_shards(&self, index: &str) -> Result<ShardsResponse> {
        self.request_json(Method::GET, &format!("{index}/_search_shards"), None)
            .await
    }

    /// Execute a search template.
    pub async fn search_template(&self, body: &Value) -> Result<SearchResponse> {
        self.request_json(Method::POST, "_search/template", Some(body))
            .await
    }

    // -----------------------------------------------------------------------
    // Indices
    // -----------------------------------------------------------------------

    /// Create an index.
    pub async fn create_index(&self, index: &str, body: &Value) -> Result<AckResponse> {
        self.request_json(Method::PUT, index, Some(body)).await
    }

    /// Delete an index.
    pub async fn delete_index(&self, index: &str) -> Result<AckResponse> {
        self.request_json(Method::DELETE, index, None).await
    }

    /// Get index metadata.
    pub async fn get_index(&self, index: &str) -> Result<Value> {
        self.request_json(Method::GET, index, None).await
    }

    /// Check if an index exists (HEAD request, returns bool).
    pub async fn index_exists(&self, index: &str) -> Result<bool> {
        let resp = self.rest().request(Method::HEAD, index)?.send().await?;
        Ok(resp.status().is_success())
    }

    /// Get index settings.
    pub async fn get_settings(&self, index: &str) -> Result<Value> {
        self.request_json(Method::GET, &format!("{index}/_settings"), None)
            .await
    }

    /// Update index settings.
    pub async fn update_settings(&self, index: &str, body: &Value) -> Result<AckResponse> {
        self.request_json(Method::PUT, &format!("{index}/_settings"), Some(body))
            .await
    }

    /// Get index mapping.
    pub async fn get_mapping(&self, index: &str) -> Result<Value> {
        self.request_json(Method::GET, &format!("{index}/_mapping"), None)
            .await
    }

    /// Put (update) index mapping.
    pub async fn put_mapping(&self, index: &str, body: &Value) -> Result<AckResponse> {
        self.request_json(Method::PUT, &format!("{index}/_mapping"), Some(body))
            .await
    }

    /// Get aliases for an index.
    pub async fn get_aliases(&self, index: &str) -> Result<Value> {
        self.request_json(Method::GET, &format!("{index}/_alias"), None)
            .await
    }

    /// Create or update aliases (bulk alias action).
    pub async fn create_alias(&self, body: &Value) -> Result<AckResponse> {
        self.request_json(Method::POST, "_aliases", Some(body))
            .await
    }

    /// Delete an alias from an index.
    pub async fn delete_alias(&self, index: &str, alias: &str) -> Result<AckResponse> {
        self.request_json(Method::DELETE, &format!("{index}/_alias/{alias}"), None)
            .await
    }

    /// Create or update an index template.
    pub async fn create_template(&self, name: &str, body: &Value) -> Result<AckResponse> {
        self.request_json(Method::PUT, &format!("_index_template/{name}"), Some(body))
            .await
    }

    /// Delete an index template.
    pub async fn delete_template(&self, name: &str) -> Result<AckResponse> {
        self.request_json(Method::DELETE, &format!("_index_template/{name}"), None)
            .await
    }

    /// Get an index template.
    pub async fn get_template(&self, name: &str) -> Result<Value> {
        self.request_json(Method::GET, &format!("_index_template/{name}"), None)
            .await
    }

    /// Open a closed index.
    pub async fn open_index(&self, index: &str) -> Result<AckResponse> {
        self.request_json(Method::POST, &format!("{index}/_open"), None)
            .await
    }

    /// Close an index.
    pub async fn close_index(&self, index: &str) -> Result<AckResponse> {
        self.request_json(Method::POST, &format!("{index}/_close"), None)
            .await
    }

    // -----------------------------------------------------------------------
    // Cluster
    // -----------------------------------------------------------------------

    /// Get cluster health.
    pub async fn cluster_health(&self) -> Result<ClusterHealth> {
        self.request_json(Method::GET, "_cluster/health", None)
            .await
    }

    /// Get cluster state.
    pub async fn cluster_state(&self) -> Result<Value> {
        self.request_json(Method::GET, "_cluster/state", None).await
    }

    /// Get cluster stats.
    pub async fn cluster_stats(&self) -> Result<Value> {
        self.request_json(Method::GET, "_cluster/stats", None).await
    }

    /// Get cluster settings.
    pub async fn cluster_settings(&self) -> Result<Value> {
        self.request_json(Method::GET, "_cluster/settings", None)
            .await
    }

    /// Update cluster settings.
    pub async fn update_cluster_settings(&self, body: &Value) -> Result<Value> {
        self.request_json(Method::PUT, "_cluster/settings", Some(body))
            .await
    }

    /// Explain shard allocation.
    pub async fn allocation_explain(&self, body: &Value) -> Result<Value> {
        self.request_json(Method::POST, "_cluster/allocation/explain", Some(body))
            .await
    }

    /// Reroute shards.
    pub async fn reroute(&self, body: &Value) -> Result<Value> {
        self.request_json(Method::POST, "_cluster/reroute", Some(body))
            .await
    }

    // -----------------------------------------------------------------------
    // Nodes
    // -----------------------------------------------------------------------

    /// Get nodes info.
    pub async fn nodes_info(&self) -> Result<Value> {
        self.request_json(Method::GET, "_nodes", None).await
    }

    /// Get nodes stats.
    pub async fn nodes_stats(&self) -> Result<Value> {
        self.request_json(Method::GET, "_nodes/stats", None).await
    }

    /// Get nodes hot threads (returns plain text).
    pub async fn nodes_hot_threads(&self) -> Result<String> {
        let resp = self
            .rest()
            .request(Method::GET, "_nodes/hot_threads")?
            .send()
            .await?;
        let status = resp.status();
        let bytes = resp.into_body();
        if !status.is_success() {
            return Err(Self::http_error("opensearch hot threads", status, &bytes));
        }
        String::from_utf8(bytes.to_vec()).map_err(|e| {
            SunbeamError::network(format!("opensearch hot threads: invalid UTF-8: {e}"))
        })
    }

    // -----------------------------------------------------------------------
    // Cat
    // -----------------------------------------------------------------------

    /// List indices via the cat API.
    pub async fn cat_indices(&self) -> Result<Vec<CatIndex>> {
        self.request_json(Method::GET, "_cat/indices?format=json", None)
            .await
    }

    /// List nodes via the cat API.
    pub async fn cat_nodes(&self) -> Result<Vec<CatNode>> {
        self.request_json(Method::GET, "_cat/nodes?format=json", None)
            .await
    }

    /// List shards via the cat API.
    pub async fn cat_shards(&self) -> Result<Vec<CatShard>> {
        self.request_json(Method::GET, "_cat/shards?format=json", None)
            .await
    }

    /// Cluster health via the cat API.
    pub async fn cat_health(&self) -> Result<Vec<CatHealth>> {
        self.request_json(Method::GET, "_cat/health?format=json", None)
            .await
    }

    /// Allocation info via the cat API.
    pub async fn cat_allocation(&self) -> Result<Vec<CatAllocation>> {
        self.request_json(Method::GET, "_cat/allocation?format=json", None)
            .await
    }

    // -----------------------------------------------------------------------
    // Ingest
    // -----------------------------------------------------------------------

    /// Create or update an ingest pipeline.
    pub async fn create_pipeline(&self, id: &str, body: &Value) -> Result<AckResponse> {
        self.request_json(Method::PUT, &format!("_ingest/pipeline/{id}"), Some(body))
            .await
    }

    /// Get an ingest pipeline.
    pub async fn get_pipeline(&self, id: &str) -> Result<Value> {
        self.request_json(Method::GET, &format!("_ingest/pipeline/{id}"), None)
            .await
    }

    /// Delete an ingest pipeline.
    pub async fn delete_pipeline(&self, id: &str) -> Result<AckResponse> {
        self.request_json(Method::DELETE, &format!("_ingest/pipeline/{id}"), None)
            .await
    }

    /// Simulate an ingest pipeline.
    pub async fn simulate_pipeline(&self, id: &str, body: &Value) -> Result<Value> {
        self.request_json(
            Method::POST,
            &format!("_ingest/pipeline/{id}/_simulate"),
            Some(body),
        )
        .await
    }

    /// Get all ingest pipelines.
    pub async fn get_all_pipelines(&self) -> Result<Value> {
        self.request_json(Method::GET, "_ingest/pipeline", None)
            .await
    }

    // -----------------------------------------------------------------------
    // Snapshots
    // -----------------------------------------------------------------------

    /// Create or update a snapshot repository.
    pub async fn create_snapshot_repo(&self, name: &str, body: &Value) -> Result<AckResponse> {
        self.request_json(Method::PUT, &format!("_snapshot/{name}"), Some(body))
            .await
    }

    /// Delete a snapshot repository.
    pub async fn delete_snapshot_repo(&self, name: &str) -> Result<AckResponse> {
        self.request_json(Method::DELETE, &format!("_snapshot/{name}"), None)
            .await
    }

    /// Create a snapshot.
    pub async fn create_snapshot(&self, repo: &str, name: &str, body: &Value) -> Result<Value> {
        self.request_json(Method::PUT, &format!("_snapshot/{repo}/{name}"), Some(body))
            .await
    }

    /// Delete a snapshot.
    pub async fn delete_snapshot(&self, repo: &str, name: &str) -> Result<AckResponse> {
        self.request_json(Method::DELETE, &format!("_snapshot/{repo}/{name}"), None)
            .await
    }

    /// List all snapshots in a repository.
    pub async fn list_snapshots(&self, repo: &str) -> Result<Value> {
        self.request_json(Method::GET, &format!("_snapshot/{repo}/_all"), None)
            .await
    }

    /// Restore a snapshot.
    pub async fn restore_snapshot(&self, repo: &str, name: &str, body: &Value) -> Result<Value> {
        self.request_json(
            Method::POST,
            &format!("_snapshot/{repo}/{name}/_restore"),
            Some(body),
        )
        .await
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Build an error for a non-success HTTP status, including the body.
    fn http_error(ctx: &str, status: http::StatusCode, body: &[u8]) -> SunbeamError {
        SunbeamError::network(format!(
            "{ctx}: HTTP {status}: {}",
            String::from_utf8_lossy(body)
        ))
    }

    /// Send a request with an optional JSON body, error on non-2xx, parse the
    /// response as JSON.
    async fn request_json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
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
            return Err(Self::http_error("opensearch", status, &bytes));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Send a request with an optional JSON body, error on non-2xx, discard
    /// the response body.
    async fn request_send(&self, method: Method, path: &str, body: Option<&Value>) -> Result<()> {
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
            return Err(Self::http_error("opensearch", status, &bytes));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connect_url() {
        let c = OpenSearchClient::connect("sunbeam.pt").unwrap();
        assert_eq!(c.base_url(), "https://search.sunbeam.pt");
    }

    #[test]
    fn test_new_from_g2v_client() {
        let client = ClientBuilder::new("http://localhost:9200").build().unwrap();
        let c = OpenSearchClient::new(&client);
        assert_eq!(c.base_url(), "http://localhost:9200");
    }

    #[tokio::test]
    async fn test_head_doc_unreachable() {
        let client = ClientBuilder::new("http://127.0.0.1:19998")
            .build()
            .unwrap();
        let c = OpenSearchClient::new(&client);
        let result = c.head_doc("test", "1").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_index_exists_unreachable() {
        let client = ClientBuilder::new("http://127.0.0.1:19998")
            .build()
            .unwrap();
        let c = OpenSearchClient::new(&client);
        let result = c.index_exists("test").await;
        assert!(result.is_err());
    }
}

#[cfg(all(test, feature = "testing"))]
mod container_tests {
    use std::time::Duration;

    use serde_json::json;
    use sunbeam_g2v::client::ClientBuilder;

    use super::OpenSearchClient;
    use crate::testing::OpenSearch;

    /// Boot a single-node OpenSearch container and return a client once the
    /// cluster answers health checks.
    async fn boot() -> (
        testcontainers::ContainerAsync<testcontainers::GenericImage>,
        OpenSearchClient,
    ) {
        let container = OpenSearch::default()
            .publish_ports()
            .start()
            .await
            .expect("opensearch should start");
        let url = OpenSearch::url(&container)
            .await
            .expect("url should resolve");
        let g2v = ClientBuilder::new(url).build().expect("g2v client");
        let client = OpenSearchClient::new(&g2v);

        for _ in 0..60 {
            if let Ok(health) = client.cluster_health().await
                && (health.status == "green" || health.status == "yellow")
            {
                return (container, client);
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        panic!("opensearch did not become healthy");
    }

    #[tokio::test]
    async fn opensearch_doc_lifecycle() {
        let (_container, client) = boot().await;

        client
            .create_index("sdk-test", &json!({"settings": {"number_of_shards": 1}}))
            .await
            .expect("create index");
        assert!(client.index_exists("sdk-test").await.expect("index exists"));

        client
            .index_doc("sdk-test", "1", &json!({"title": "hello world"}))
            .await
            .expect("index doc");
        let doc = client.get_doc("sdk-test", "1").await.expect("get doc");
        assert!(doc.found);

        // Search is near-real-time; retry until the doc is visible.
        let mut found = false;
        for _ in 0..10 {
            let res = client
                .search("sdk-test", &json!({"query": {"match": {"title": "hello"}}}))
                .await
                .expect("search");
            if res.hits.total.value >= 1 {
                found = true;
                break;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        assert!(found, "indexed doc should be searchable");

        client.delete_index("sdk-test").await.expect("delete index");
        assert!(!client.index_exists("sdk-test").await.expect("index exists"));
    }
}

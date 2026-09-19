//! GraphQL client for the Sunbeam unified HTTP client.
//!
//! [`GraphqlClient`] sends queries and mutations as JSON POST requests through
//! the shared resilience/auth/TLS stack.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::rest::{ClientError, RestClient};

/// A single GraphQL error returned by the server.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphqlError {
    /// Human-readable error message.
    pub message: String,
    /// Optional locations in the query where the error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locations: Option<Vec<GraphqlErrorLocation>>,
    /// Optional path into the response data where the error occurred.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Value>,
    /// Additional extensions returned by the server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// Source location of a GraphQL error.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphqlErrorLocation {
    /// Line number (1-based).
    pub line: u32,
    /// Column number (1-based).
    pub column: u32,
}

/// GraphQL response envelope.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GraphqlResponse<T> {
    /// Response data, present when the operation succeeded.
    pub data: Option<T>,
    /// Errors, present when the operation failed partially or fully.
    pub errors: Option<Vec<GraphqlError>>,
}

/// GraphQL client built on the unified Sunbeam HTTP stack.
#[derive(Clone, Debug)]
pub struct GraphqlClient {
    rest: RestClient,
}

impl GraphqlClient {
    /// Create a new GraphQL client wrapping a [`RestClient`].
    pub(crate) fn new(rest: RestClient) -> Self {
        Self { rest }
    }

    /// Execute a GraphQL query.
    pub async fn query<T, V>(
        &self,
        query: &str,
        variables: Option<V>,
    ) -> Result<GraphqlResponse<T>, ClientError>
    where
        T: for<'de> Deserialize<'de>,
        V: Serialize,
    {
        self.execute(query, None::<String>, variables).await
    }

    /// Execute a GraphQL mutation.
    pub async fn mutation<T, V>(
        &self,
        mutation: &str,
        variables: Option<V>,
    ) -> Result<GraphqlResponse<T>, ClientError>
    where
        T: for<'de> Deserialize<'de>,
        V: Serialize,
    {
        self.execute(mutation, None::<String>, variables).await
    }

    /// Execute a GraphQL operation with an explicit operation name.
    pub async fn execute<T, V>(
        &self,
        query: &str,
        operation_name: Option<impl Into<String>>,
        variables: Option<V>,
    ) -> Result<GraphqlResponse<T>, ClientError>
    where
        T: for<'de> Deserialize<'de>,
        V: Serialize,
    {
        let body = GraphqlRequestBody {
            query: query.to_string(),
            variables: match variables {
                Some(v) => Some(serde_json::to_value(v)?),
                None => None,
            },
            operation_name: operation_name.map(Into::into),
        };
        self.rest.post_json("/graphql", &body).await
    }
}

#[derive(Serialize)]
struct GraphqlRequestBody {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use http::{HeaderMap, Request, Response};
    use serde_json::json;

    use crate::g2v::client::builder::{BoxedClientService, Client};

    fn graphql_mock_client(response_body: Bytes) -> Client {
        let service = tower::service_fn(move |_req: Request<Bytes>| {
            let body = response_body.clone();
            async move {
                Ok::<_, crate::g2v::BoxError>(
                    Response::builder()
                        .header(http::header::CONTENT_TYPE, "application/json")
                        .body(body)
                        .unwrap(),
                )
            }
        });

        Client::from_service(
            BoxedClientService::new(service),
            reqwest::Url::parse("http://example.com").unwrap(),
            HeaderMap::new(),
        )
    }

    #[tokio::test]
    async fn test_graphql_query_success() {
        let body = Bytes::from_static(br#"{"data":{"foo":"bar"}}"#);
        let client = graphql_mock_client(body);
        let resp = client
            .graphql()
            .query::<serde_json::Value, serde_json::Value>("query GetFoo { foo }", None)
            .await
            .unwrap();
        assert_eq!(resp.data.unwrap()["foo"], "bar");
        assert!(resp.errors.is_none());
    }

    #[tokio::test]
    async fn test_graphql_query_errors() {
        let body = Bytes::from_static(
            br#"{"errors":[{"message":"unknown query","locations":[{"line":1,"column":1}]}]}"#,
        );
        let client = graphql_mock_client(body);
        let resp = client
            .graphql()
            .query::<serde_json::Value, serde_json::Value>("query Bad { }", None)
            .await
            .unwrap();
        assert!(resp.data.is_none());
        let errors = resp.errors.unwrap();
        assert_eq!(errors[0].message, "unknown query");
        assert_eq!(errors[0].locations.as_ref().unwrap()[0].line, 1);
    }

    #[tokio::test]
    async fn test_graphql_mutation_and_variables() {
        let body = Bytes::from_static(br#"{"data":{"create":true}}"#);
        let client = graphql_mock_client(body);
        let resp = client
            .graphql()
            .mutation::<serde_json::Value, serde_json::Value>(
                "mutation Create { create }",
                Some(json!({"input": "x"})),
            )
            .await
            .unwrap();
        assert_eq!(resp.data.unwrap()["create"], true);
    }

    #[tokio::test]
    async fn test_graphql_execute_with_operation_name() {
        let body = Bytes::from_static(br#"{"data":{"result":42}}"#);
        let client = graphql_mock_client(body);
        let resp = client
            .graphql()
            .execute::<serde_json::Value, serde_json::Value>(
                "query Named { result }",
                Some("Named"),
                Some(json!({})),
            )
            .await
            .unwrap();
        assert_eq!(resp.data.unwrap()["result"], 42);
    }
}

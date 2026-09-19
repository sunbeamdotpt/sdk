//! Authorization backend client.
//!
//! Talks to a ReBAC backend over plain HTTP. The namespace,
//! object, relation and subject vocabulary is backend-agnostic; only the
//! transport paths mirror the ReBAC HTTP API.

use reqwest::{Client, Url};
use serde_json::Value;

use super::error::AuthError;

/// Authorization backend endpoint configuration.
#[derive(Debug, Clone)]
pub struct AuthorizationConfig {
    /// Read endpoint URL (permission checks, expansion).
    pub read_url: String,
    /// Write endpoint URL (relation tuple mutations).
    pub write_url: String,
}

impl Default for AuthorizationConfig {
    fn default() -> Self {
        Self {
            read_url: "http://localhost:4466".to_string(),
            write_url: "http://localhost:4467".to_string(),
        }
    }
}

/// HTTP client for the authorization backend.
#[derive(Debug, Clone)]
pub struct AuthorizationClient {
    client: Client,
    read_url: Url,
    write_url: Url,
}

impl AuthorizationClient {
    /// Create a client from endpoint configuration.
    pub fn new(config: AuthorizationConfig) -> Result<Self, AuthError> {
        Ok(Self {
            client: Client::new(),
            read_url: parse_base_url(&config.read_url)
                .map_err(|e| AuthError::AuthorizationBackend(e.to_string()))?,
            write_url: parse_base_url(&config.write_url)
                .map_err(|e| AuthError::AuthorizationBackend(e.to_string()))?,
        })
    }

    /// Create a client pointing at the default local endpoints.
    pub fn with_defaults() -> Result<Self, AuthError> {
        Self::new(AuthorizationConfig::default())
    }

    /// Check whether `subject_id` has `relation` on `object` in `namespace`.
    pub async fn check_permission(
        &self,
        namespace: &str,
        object: &str,
        relation: &str,
        subject_id: &str,
    ) -> Result<bool, AuthError> {
        let url = url_with_query(
            &self.read_url,
            "relation-tuples/check",
            &[
                ("namespace", namespace),
                ("object", object),
                ("relation", relation),
                ("subject_id", subject_id),
            ],
        )?;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AuthError::AuthorizationBackendUnavailable(e.to_string()))?;

        if response.status().is_success() {
            let body: Value = response
                .json()
                .await
                .map_err(|e| AuthError::AuthorizationBackend(e.to_string()))?;
            Ok(body
                .get("allowed")
                .and_then(|v| v.as_bool())
                .unwrap_or(false))
        } else if response.status() == reqwest::StatusCode::FORBIDDEN {
            Ok(false)
        } else {
            Err(backend_error(response).await)
        }
    }

    /// Create a relation tuple.
    pub async fn create_relation_tuple(
        &self,
        namespace: &str,
        object: &str,
        relation: &str,
        subject_id: &str,
    ) -> Result<Value, AuthError> {
        let url = self
            .write_url
            .join("admin/relation-tuples")
            .map_err(|e| AuthError::AuthorizationBackend(e.to_string()))?;

        let payload = serde_json::json!({
            "namespace": namespace,
            "object": object,
            "relation": relation,
            "subject_id": subject_id,
        });
        let patch = serde_json::json!([{
            "action": "insert",
            "relation_tuple": payload,
        }]);

        let response = self
            .client
            .patch(url)
            .header("accept", "application/json")
            .json(&patch)
            .send()
            .await
            .map_err(|e| AuthError::AuthorizationBackendUnavailable(e.to_string()))?;

        if response.status().is_success() {
            Ok(payload)
        } else {
            Err(backend_error(response).await)
        }
    }

    /// Delete a relation tuple.
    pub async fn delete_relation_tuple(
        &self,
        namespace: &str,
        object: &str,
        relation: &str,
        subject_id: &str,
    ) -> Result<(), AuthError> {
        let url = url_with_query(
            &self.write_url,
            "admin/relation-tuples",
            &[
                ("namespace", namespace),
                ("object", object),
                ("relation", relation),
                ("subject_id", subject_id),
            ],
        )?;

        let response = self
            .client
            .delete(url)
            .send()
            .await
            .map_err(|e| AuthError::AuthorizationBackendUnavailable(e.to_string()))?;

        if response.status().is_success() {
            Ok(())
        } else {
            Err(backend_error(response).await)
        }
    }

    /// Expand a subject set.
    pub async fn expand(
        &self,
        namespace: &str,
        object: &str,
        relation: &str,
    ) -> Result<Value, AuthError> {
        let url = url_with_query(
            &self.read_url,
            "relation-tuples/expand",
            &[
                ("namespace", namespace),
                ("object", object),
                ("relation", relation),
            ],
        )?;

        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| AuthError::AuthorizationBackendUnavailable(e.to_string()))?;

        if response.status().is_success() {
            response
                .json()
                .await
                .map_err(|e| AuthError::AuthorizationBackend(e.to_string()))
        } else {
            Err(backend_error(response).await)
        }
    }
}

fn parse_base_url(url: &str) -> Result<Url, String> {
    let mut url = Url::parse(url).map_err(|e| e.to_string())?;
    if !url.path().ends_with('/') {
        url.path_segments_mut()
            .map_err(|_| "cannot modify url path".to_string())?
            .push("");
    }
    Ok(url)
}

fn url_with_query(base: &Url, path: &str, params: &[(&str, &str)]) -> Result<Url, AuthError> {
    let mut url = base
        .join(path)
        .map_err(|e| AuthError::AuthorizationBackend(e.to_string()))?;
    {
        let mut pairs = url.query_pairs_mut();
        for (key, value) in params {
            pairs.append_pair(key, value);
        }
    }
    Ok(url)
}

async fn backend_error(response: reqwest::Response) -> AuthError {
    let status = response.status().as_u16();
    let message = response
        .text()
        .await
        .unwrap_or_else(|_| "<unreadable body>".to_string());
    AuthError::AuthorizationBackend(format!("backend returned {status}: {message}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_config_and_default_client_build() {
        let config = AuthorizationConfig::default();
        assert_eq!(config.read_url, "http://localhost:4466");
        assert_eq!(config.write_url, "http://localhost:4467");
        let _client = AuthorizationClient::with_defaults().unwrap();
    }

    #[test]
    fn authorization_client_rejects_invalid_urls() {
        let result = AuthorizationClient::new(AuthorizationConfig {
            read_url: "not a url".to_string(),
            write_url: "http://localhost:4467".to_string(),
        });
        assert!(matches!(result, Err(AuthError::AuthorizationBackend(_))));

        let result = AuthorizationClient::new(AuthorizationConfig {
            read_url: "http://localhost:4466".to_string(),
            write_url: "not a url".to_string(),
        });
        assert!(matches!(result, Err(AuthError::AuthorizationBackend(_))));
    }

    #[test]
    fn url_with_query_appends_params() {
        let base = parse_base_url("http://localhost:4466").unwrap();
        let url = url_with_query(&base, "relation-tuples/check", &[("namespace", "ns")]).unwrap();
        assert_eq!(
            url.as_str(),
            "http://localhost:4466/relation-tuples/check?namespace=ns"
        );
    }
}

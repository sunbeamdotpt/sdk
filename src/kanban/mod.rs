//! Kanban project management via gRPC.

use crate::error::{Result, ResultExt, SunbeamError};

pub mod aggregated;
pub mod attachments;
pub mod boards;
pub mod card_templates;
pub mod cards;
pub mod client;
pub mod github_links;
pub mod projects;
pub mod public_boards;
pub mod resolve;
pub mod search;
pub mod subscribe;
pub mod templates;

/// Generate a fresh ULID idempotency key for mutating RPCs.
pub fn new_idempotency_key() -> String {
    ulid::Ulid::new().to_string()
}

/// Resolve the default Kanban server URL from the provided context.
pub fn default_server_url_for(ctx: &crate::config::Context) -> Result<String> {
    let domain = ctx.domain.clone();
    if domain.is_empty() {
        return Err(SunbeamError::config(
            "no domain configured; set one with `sunbeam config set --domain ...` or pass --url",
        ));
    }
    Ok(format!("https://kanban.{domain}"))
}

/// Resolve the default Kanban server URL from the active context.
pub fn default_server_url() -> Result<String> {
    default_server_url_for(crate::config::active_context())
}

/// Resolve the final server URL from an explicit override or the active context.
pub fn resolve_server_url(url_override: Option<&str>) -> Result<String> {
    match url_override {
        Some(u) => Ok(u.to_string()),
        None => default_server_url(),
    }
}

/// Resolve and validate a bearer token for authenticated RPCs.
pub async fn require_token() -> Result<String> {
    crate::auth::get_token()
        .await
        .with_ctx(|| "run `sunbeam auth login` first".to_string())
}

/// Format a chrono UTC timestamp as a short ISO 8601 string.
pub fn fmt_time(ts: &chrono::DateTime<chrono::Utc>) -> String {
    ts.format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Format a prost Timestamp.
pub fn fmt_proto_time(ts: &prost_types::Timestamp) -> String {
    let dt = chrono::DateTime::<chrono::Utc>::from_timestamp(ts.seconds, ts.nanos as u32);
    match dt {
        Some(dt) => fmt_time(&dt),
        None => "<invalid>".into(),
    }
}

/// Convert a `serde_json::Value` to a `prost_types::Value`.
pub fn json_to_prost(json: &serde_json::Value) -> prost_types::Value {
    use prost_types::value::Kind;
    let kind = match json {
        serde_json::Value::Null => Kind::NullValue(0),
        serde_json::Value::Bool(b) => Kind::BoolValue(*b),
        serde_json::Value::Number(n) => Kind::NumberValue(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => Kind::StringValue(s.clone()),
        serde_json::Value::Array(arr) => Kind::ListValue(prost_types::ListValue {
            values: arr.iter().map(json_to_prost).collect(),
        }),
        serde_json::Value::Object(obj) => Kind::StructValue(prost_types::Struct {
            fields: obj
                .iter()
                .map(|(k, v)| (k.clone(), json_to_prost(v)))
                .collect(),
        }),
    };
    prost_types::Value { kind: Some(kind) }
}

/// Convert a top-level JSON object into a `prost_types::Struct`.
pub fn json_object_to_struct(json: &serde_json::Value) -> prost_types::Struct {
    match json {
        serde_json::Value::Object(map) => prost_types::Struct {
            fields: map
                .iter()
                .map(|(k, v)| (k.clone(), json_to_prost(v)))
                .collect(),
        },
        _ => prost_types::Struct::default(),
    }
}

/// Convert a `prost_types::Value` back to `serde_json::Value`.
pub fn prost_to_json(value: &prost_types::Value) -> serde_json::Value {
    use prost_types::value::Kind;
    match &value.kind {
        Some(Kind::NullValue(_)) | None => serde_json::Value::Null,
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::NumberValue(n)) => serde_json::Number::from_f64(*n)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::ListValue(list)) => {
            serde_json::Value::Array(list.values.iter().map(prost_to_json).collect())
        }
        Some(Kind::StructValue(s)) => prost_struct_to_json(s),
    }
}

/// Convert a `prost_types::Struct` to a `serde_json::Value::Object`.
pub fn prost_struct_to_json(s: &prost_types::Struct) -> serde_json::Value {
    serde_json::Value::Object(
        s.fields
            .iter()
            .map(|(k, v)| (k.clone(), prost_to_json(v)))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_server_url_uses_override() {
        assert_eq!(
            resolve_server_url(Some("http://local")).unwrap(),
            "http://local"
        );
    }

    #[test]
    fn default_server_url_for_builds_from_domain() {
        let ctx = crate::config::Context {
            domain: "sunbeam.test".into(),
            ..Default::default()
        };
        assert_eq!(
            default_server_url_for(&ctx).unwrap(),
            "https://kanban.sunbeam.test"
        );
    }

    #[test]
    fn default_server_url_for_errors_when_domain_empty() {
        let ctx = crate::config::Context::default();
        assert!(default_server_url_for(&ctx).is_err());
    }

    #[test]
    fn new_idempotency_key_is_ulid() {
        let key = new_idempotency_key();
        assert!(!key.is_empty());
        assert!(key.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn default_server_url_errors_when_domain_empty() {
        crate::config::set_active_context(crate::config::Context::default());
        let err = default_server_url().unwrap_err();
        assert!(err.to_string().contains("no domain configured"));
    }
}

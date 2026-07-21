//! Remote workflow management via wfe-server gRPC API.

pub mod client;

/// Resolve the SSO access token from the unified config auth store.
pub fn resolve_token(domain: &str) -> anyhow::Result<String> {
    let tokens = crate::config::get_auth_tokens(domain).ok_or_else(|| {
        anyhow::anyhow!("not logged in for domain {domain} — run `sunbeam auth login` first")
    })?;
    if tokens.access_token.is_empty() {
        return Err(anyhow::anyhow!(
            "token cache is corrupt for domain {domain}"
        ));
    }
    Ok(tokens.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_token_missing_returns_not_logged_in() {
        let err = resolve_token("nonexistent.example.com").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not logged in"), "unexpected: {msg}");
    }
}

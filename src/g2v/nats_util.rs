//! Internal helpers shared by NATS-based modules.

/// Parse a NATS URL and extract an authentication token from its userinfo.
///
/// Supports the two common token-in-URL forms:
///
/// - `nats://TOKEN@host` — token is taken from the username.
/// - `nats://:TOKEN@host` — token is taken from the password.
///
/// When credentials are present they are stripped from the returned URL so
/// the caller can pass a clean URL to `async_nats::ConnectOptions` together
/// with the extracted token.
///
/// Returns the (possibly unchanged) URL and an optional token. If the input
/// is not a valid URL, it is returned as-is with no token.
pub(crate) fn parse_nats_url(url: &str) -> (String, Option<String>) {
    let parsed = match url::Url::parse(url) {
        Ok(u) => u,
        Err(_) => return (url.to_string(), None),
    };

    let token = if let Some(password) = parsed.password() {
        if password.is_empty() {
            None
        } else {
            Some(password.to_string())
        }
    } else if !parsed.username().is_empty() {
        Some(parsed.username().to_string())
    } else {
        None
    };

    match token {
        Some(_) => {
            let mut stripped = parsed.clone();
            let _ = stripped.set_username("");
            let _ = stripped.set_password(None);
            (stripped.to_string(), token)
        }
        None => (url.to_string(), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_nats_url_no_credentials() {
        let (url, token) = parse_nats_url("nats://localhost:4222");
        assert_eq!(url, "nats://localhost:4222");
        assert!(token.is_none());
    }

    #[test]
    fn test_parse_nats_url_token_as_username() {
        let (url, token) = parse_nats_url("nats://my-token@localhost:4222");
        assert_eq!(url, "nats://localhost:4222");
        assert_eq!(token, Some("my-token".to_string()));
    }

    #[test]
    fn test_parse_nats_url_token_as_password() {
        let (url, token) = parse_nats_url("nats://:my-token@localhost:4222");
        assert_eq!(url, "nats://localhost:4222");
        assert_eq!(token, Some("my-token".to_string()));
    }

    #[test]
    fn test_parse_nats_url_invalid_url_passed_through() {
        let (url, token) = parse_nats_url("not-a-valid-url");
        assert_eq!(url, "not-a-valid-url");
        assert!(token.is_none());
    }
}

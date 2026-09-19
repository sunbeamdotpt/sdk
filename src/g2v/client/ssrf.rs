//! SSRF protection for HTTP clients.
//!
//! Provides URL validation and a safe DNS resolver that blocks loopback,
//! link-local, private, and metadata IP addresses at both URL-parse and
//! DNS-resolution time.

use reqwest::dns::{Addrs, Name, Resolve};
use std::sync::Arc;

/// Returns `true` if `ip` is a non-routable / forbidden address.
///
/// Blocks loopback, link-local, private ranges, and unspecified addresses.
/// IPv6-mapped IPv4 addresses are handled recursively.
pub fn is_forbidden_ip(ip: std::net::IpAddr) -> bool {
    if ip.is_unspecified() {
        return true;
    }
    match ip {
        std::net::IpAddr::V4(v4) => v4.is_loopback() || v4.is_link_local() || v4.is_private(),
        std::net::IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                is_forbidden_ip(std::net::IpAddr::V4(v4))
            } else {
                v6.is_loopback() || (v6.segments()[0] & 0xffc0 == 0xfe80)
            }
        }
    }
}

/// Validate an upstream URL for SSRF safety.
///
/// Requires `https://`, blocks loopback/link-local/private IP literals, and
/// blocks common metadata endpoints.
pub fn validate_upstream_url(url_str: &str) -> Result<(), crate::g2v::error::ServiceError> {
    let url = reqwest::Url::parse(url_str).map_err(|e| {
        crate::g2v::error::ServiceError::InvalidArgument(format!(
            "invalid upstream URL '{url_str}': {e}"
        ))
    })?;

    if url.scheme() != "https" {
        return Err(crate::g2v::error::ServiceError::InvalidArgument(
            "upstream URL must use HTTPS".into(),
        ));
    }

    let host = url.host_str().ok_or_else(|| {
        crate::g2v::error::ServiceError::InvalidArgument("upstream URL is missing a host".into())
    })?;

    // Block common non-routable / metadata hostnames.
    let lower = host.to_ascii_lowercase();
    if lower == "localhost"
        || lower == "metadata"
        || lower == "metadata.google.internal"
        || lower.ends_with(".metadata")
    {
        return Err(crate::g2v::error::ServiceError::InvalidArgument(
            "upstream URL resolves to a forbidden hostname".into(),
        ));
    }

    // If the host is an IP literal, block loopback, link-local and private ranges.
    let ip_host = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = ip_host.parse::<std::net::IpAddr>()
        && is_forbidden_ip(ip)
    {
        return Err(crate::g2v::error::ServiceError::InvalidArgument(
            "upstream URL resolves to a forbidden IP address".into(),
        ));
    }

    Ok(())
}

/// Re-resolve an upstream URL's host and reject any forbidden IP addresses.
///
/// This is a second-line defense against DNS rebinding: even if the URL passes
/// `validate_upstream_url`, the resolved IPs are checked again at request time.
pub async fn validate_resolved_ips_for_url(
    url_str: &str,
) -> Result<(), crate::g2v::error::ServiceError> {
    let url = reqwest::Url::parse(url_str).map_err(|e| {
        crate::g2v::error::ServiceError::InvalidArgument(format!(
            "invalid upstream URL '{url_str}': {e}"
        ))
    })?;
    let host = url.host_str().ok_or_else(|| {
        crate::g2v::error::ServiceError::InvalidArgument("upstream URL is missing a host".into())
    })?;
    let port = url.port_or_known_default().unwrap_or(443);

    let addrs = tokio::net::lookup_host((host, port)).await.map_err(|e| {
        crate::g2v::error::ServiceError::InvalidArgument(format!(
            "DNS resolution failed for '{host}': {e}"
        ))
    })?;

    for addr in addrs {
        if is_forbidden_ip(addr.ip()) {
            return Err(crate::g2v::error::ServiceError::InvalidArgument(format!(
                "upstream host '{host}' resolved to forbidden IP {}",
                addr.ip()
            )));
        }
    }

    Ok(())
}

/// A `reqwest` DNS resolver that filters out forbidden IP addresses.
///
/// Use this with `reqwest::ClientBuilder::dns_resolver` to prevent SSRF via
/// DNS rebinding.
#[derive(Clone)]
pub struct SafeDnsResolver;

impl Resolve for SafeDnsResolver {
    fn resolve(&self, name: Name) -> reqwest::dns::Resolving {
        Box::pin(async move {
            let name_str = name.as_str().to_string();
            let addrs = tokio::net::lookup_host((name_str.as_str(), 0)).await?;
            let mut safe_addrs = Vec::new();
            for addr in addrs {
                if !is_forbidden_ip(addr.ip()) {
                    safe_addrs.push(addr);
                }
            }
            if safe_addrs.is_empty() {
                return Err(Box::new(std::io::Error::other(
                    "host resolved to forbidden IP addresses",
                ))
                    as Box<dyn std::error::Error + Send + Sync>);
            }
            let addrs: Addrs = Box::new(safe_addrs.into_iter());
            Ok(addrs)
        })
    }
}

/// Build a `reqwest::Client` with SSRF-safe DNS resolution.
pub fn safe_reqwest_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .dns_resolver(Arc::new(SafeDnsResolver))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_upstream_url_requires_https() {
        assert!(validate_upstream_url("http://example.com/token").is_err());
        assert!(validate_upstream_url("https://example.com/token").is_ok());
    }

    #[test]
    fn validate_upstream_url_blocks_loopback() {
        assert!(validate_upstream_url("https://127.0.0.1/token").is_err());
        assert!(validate_upstream_url("https://[::1]/token").is_err());
        assert!(validate_upstream_url("https://localhost/token").is_err());
    }

    #[test]
    fn validate_upstream_url_blocks_private_and_link_local() {
        assert!(validate_upstream_url("https://10.0.0.1/token").is_err());
        assert!(validate_upstream_url("https://192.168.1.1/token").is_err());
        assert!(validate_upstream_url("https://172.16.0.1/token").is_err());
        assert!(validate_upstream_url("https://169.254.169.254/token").is_err());
        assert!(validate_upstream_url("https://[fe80::1]/token").is_err());
    }

    #[test]
    fn validate_upstream_url_blocks_metadata_hosts() {
        assert!(validate_upstream_url("https://metadata.google.internal/token").is_err());
        assert!(validate_upstream_url("https://metadata/token").is_err());
    }

    #[test]
    fn validate_upstream_url_rejects_invalid_url() {
        assert!(validate_upstream_url("not a url").is_err());
    }

    #[tokio::test]
    async fn connection_level_check_blocks_metadata_ip() {
        let err = validate_resolved_ips_for_url("https://169.254.169.254/token")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::g2v::error::ServiceError::InvalidArgument(ref msg) if msg.contains("forbidden IP")),
            "expected forbidden IP error, got {err:?}"
        );
    }

    #[tokio::test]
    async fn connection_level_check_blocks_localhost_resolution() {
        let err = validate_resolved_ips_for_url("https://localhost/token")
            .await
            .unwrap_err();
        assert!(
            matches!(err, crate::g2v::error::ServiceError::InvalidArgument(ref msg) if msg.contains("forbidden IP")),
            "expected forbidden IP error, got {err:?}"
        );
    }

    #[test]
    fn is_forbidden_ip_blocks_ipv4_mapped_ipv6_loopback() {
        let ip: std::net::IpAddr = "::ffff:127.0.0.1".parse().unwrap();
        assert!(is_forbidden_ip(ip));
    }

    #[test]
    fn is_forbidden_ip_blocks_unspecified() {
        assert!(is_forbidden_ip("0.0.0.0".parse().unwrap()));
        assert!(is_forbidden_ip("::".parse().unwrap()));
    }

    #[test]
    fn is_forbidden_ip_allows_public_addresses() {
        assert!(!is_forbidden_ip("8.8.8.8".parse().unwrap()));
        assert!(!is_forbidden_ip("2606:4700:4700::1111".parse().unwrap()));
        let mapped_private: std::net::IpAddr = "::ffff:10.0.0.1".parse().unwrap();
        assert!(is_forbidden_ip(mapped_private));
    }

    #[test]
    fn validate_upstream_url_allows_public_ip_and_blocks_metadata_suffix() {
        assert!(validate_upstream_url("https://8.8.8.8/token").is_ok());
        assert!(validate_upstream_url("https://foo.metadata/token").is_err());
    }

    #[tokio::test]
    async fn validate_resolved_ips_rejects_invalid_url_and_allows_public_literal() {
        assert!(validate_resolved_ips_for_url("not a url").await.is_err());
        assert!(
            validate_resolved_ips_for_url("https://8.8.8.8/token")
                .await
                .is_ok()
        );
    }

    #[test]
    fn safe_reqwest_client_builds() {
        let _client = safe_reqwest_client();
    }
}

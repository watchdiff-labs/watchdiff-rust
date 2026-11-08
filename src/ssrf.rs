use std::net::IpAddr;
use url::Url;

/// Validates that a URL does not point at loopback, link-local, or private
/// network ranges, unless `allow_private` is set. Mirrors the SSRF guard in
/// the TypeScript implementation.
pub fn validate_url(raw: &str, allow_private: bool) -> anyhow::Result<Url> {
    let url = Url::parse(raw).map_err(|e| anyhow::anyhow!("invalid URL '{raw}': {e}"))?;

    match url.scheme() {
        "http" | "https" => {}
        other => anyhow::bail!("unsupported URL scheme '{other}'"),
    }

    if allow_private {
        return Ok(url);
    }

    // `Url::host()` gives a parsed `Host` enum (IPv4/IPv6/domain) directly.
    // `Url::host_str()` returns IPv6 literals bracketed (e.g. "[::1]"), which
    // fails `str::parse::<IpAddr>()` — using that instead let every literal
    // IPv6 address (not just loopback) silently bypass this guard entirely.
    match url.host() {
        Some(url::Host::Ipv4(v4)) => {
            if is_disallowed_ip(&IpAddr::V4(v4)) {
                anyhow::bail!("URL '{raw}' resolves to a disallowed private/loopback address");
            }
        }
        Some(url::Host::Ipv6(v6)) => {
            if is_disallowed_ip(&IpAddr::V6(v6)) {
                anyhow::bail!("URL '{raw}' resolves to a disallowed private/loopback address");
            }
        }
        Some(url::Host::Domain(host)) => {
            if is_disallowed_hostname(host) {
                anyhow::bail!("URL '{raw}' targets a disallowed hostname");
            }
        }
        None => anyhow::bail!("URL '{raw}' has no host"),
    }

    Ok(url)
}

fn is_disallowed_hostname(host: &str) -> bool {
    matches!(host, "localhost" | "0.0.0.0") || host.ends_with(".localhost")
}

fn is_disallowed_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_link_local()
                || v4.is_private()
                || v4.is_broadcast()
                || v4.is_unspecified()
                || v4.octets()[0] == 100 && (64..=127).contains(&v4.octets()[1]) // CGNAT 100.64.0.0/10
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || (v6.segments()[0] & 0xfe00) == 0xfc00 // unique-local fc00::/7
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // link-local fe80::/10
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_literal_loopback() {
        assert!(validate_url("http://127.0.0.1/x", false).is_err());
        assert!(validate_url("http://127.0.0.1:8080/x", false).is_err());
        assert!(validate_url("http://[::1]/x", false).is_err());
    }

    #[test]
    fn blocks_ipv6_private_ranges() {
        assert!(validate_url("http://[fe80::1]/x", false).is_err(), "link-local");
        assert!(validate_url("http://[fc00::1]/x", false).is_err(), "unique-local fc00::/7");
        assert!(validate_url("http://[fd12:3456::1]/x", false).is_err(), "unique-local fd00::/8");
        assert!(validate_url("http://[2001:db8::1]/x", false).is_ok(), "documentation range is not loopback/private, allowed");
    }

    #[test]
    fn blocks_private_ranges() {
        assert!(validate_url("http://10.0.0.1/x", false).is_err());
        assert!(validate_url("http://172.16.0.1/x", false).is_err());
        assert!(validate_url("http://192.168.1.1/x", false).is_err());
        assert!(validate_url("http://169.254.169.254/x", false).is_err(), "link-local / cloud metadata");
        assert!(validate_url("http://100.64.0.1/x", false).is_err(), "CGNAT range");
    }

    #[test]
    fn blocks_localhost_hostname() {
        assert!(validate_url("http://localhost/x", false).is_err());
        assert!(validate_url("http://0.0.0.0/x", false).is_err());
        assert!(validate_url("http://foo.localhost/x", false).is_err());
    }

    #[test]
    fn allows_public_addresses() {
        assert!(validate_url("https://example.com/x", false).is_ok());
        assert!(validate_url("http://93.184.216.34/x", false).is_ok());
    }

    #[test]
    fn allow_private_urls_bypasses_the_guard() {
        assert!(validate_url("http://127.0.0.1/x", true).is_ok());
        assert!(validate_url("http://10.0.0.1/x", true).is_ok());
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(validate_url("ftp://example.com/x", false).is_err());
        assert!(validate_url("file:///etc/passwd", false).is_err());
    }

    /// A hostname that only *resolves* to a private address via DNS (rather
    /// than embedding it literally in the URL) is NOT caught by this guard —
    /// `validate_url` never performs DNS resolution, matching the scope of
    /// the equivalent check in watchdiff-ts. This test documents that known
    /// limitation rather than asserting a false guarantee.
    #[test]
    fn dns_rebinding_style_bypass_is_a_known_limitation() {
        // "127.0.0.1.nip.io" is a real public DNS name that resolves to
        // 127.0.0.1 — but as far as this function is concerned it's just an
        // ordinary hostname, since no DNS lookup happens here.
        assert!(validate_url("http://127.0.0.1.nip.io/x", false).is_ok());
    }
}

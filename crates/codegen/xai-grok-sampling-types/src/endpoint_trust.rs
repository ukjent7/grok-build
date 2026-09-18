//! FORK(byok): canonical first-party endpoint trust checks.
//!
//! Moved here from `xai-grok-shell-base::util` so the sampler crate can derive
//! third-party (BYOK) compatibility from `base_url` itself instead of having a
//! flag threaded through every `SamplerConfig` construction site. The shell
//! crates keep thin delegates; this module is the single source of truth.

/// True when `candidate` is `trusted_base` or a path under it (same scheme, host, and port).
/// Single implementation on purpose: `xai-grok-shell-base` delegates here too, so the
/// BYOK trust decision and the shell's compression decision cannot drift apart.
///
/// Scope of that claim: this base-URL matcher only. The `x.ai` host-suffix rule still
/// has a second, deliberately broader copy in `shell-base::util::is_xai_api_url_impl`,
/// which is scheme-agnostic and accepts loopback because it decides credential
/// *refusal* and must fail closed. Do not merge them: unifying the two would either
/// widen the origins a key is sent to or narrow the ones a key is withheld from.
pub fn matches_trusted_base_url(candidate: &str, trusted_base: &str) -> bool {
    let Ok(candidate) = reqwest::Url::parse(candidate) else {
        return false;
    };
    let Ok(trusted) = reqwest::Url::parse(trusted_base) else {
        return false;
    };
    let trusted_path = trusted.path();
    // A configured base URL may end in a trailing slash; trim one (but never past the
    // root "/") so `strip_prefix` below yields a subpath that still starts with '/' and
    // subpath matching works for both spellings of the base.
    let trusted_path = if trusted_path.len() > 1 {
        trusted_path.strip_suffix('/').unwrap_or(trusted_path)
    } else {
        trusted_path
    };
    let candidate_path = candidate.path();
    let path_matches = candidate_path == trusted_path
        || candidate_path
            .strip_prefix(trusted_path)
            .is_some_and(|suffix| suffix.starts_with('/'));
    candidate.scheme() == trusted.scheme()
        && candidate.host_str() == trusted.host_str()
        && candidate.port_or_known_default() == trusted.port_or_known_default()
        && path_matches
}

/// Production cli-chat-proxy base only (compiled-in constant).
pub fn is_prod_cli_chat_proxy_url(url: &str) -> bool {
    matches_trusted_base_url(url, xai_grok_env::PROD_CLI_CHAT_PROXY_BASE_URL)
}

/// True for configured first-party cli-chat-proxy routes, excluding arbitrary loopback URLs.
/// It is suitable for xAI-only request extensions.
/// Currently the production base only; kept as a named seam so a non-prod trusted
/// route is a one-line change here, not at every caller.
pub fn is_trusted_cli_chat_proxy_url(url: &str) -> bool {
    is_prod_cli_chat_proxy_url(url)
}

/// True for trusted first-party xAI HTTPS routes, excluding arbitrary loopback URLs.
pub fn is_trusted_xai_https_url(url: &str) -> bool {
    let Ok(parsed) = reqwest::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    if is_loopback_host(&parsed) {
        return false;
    }
    if is_trusted_cli_chat_proxy_url(url) {
        return true;
    }
    parsed
        .host_str()
        .is_some_and(|host| host == "x.ai" || host.ends_with(".x.ai"))
}

fn is_loopback_host(parsed: &reqwest::Url) -> bool {
    match parsed.host() {
        Some(url::Host::Domain(host)) => host == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Whether `base_url` is a third-party route that gets a clean standard-protocol
/// payload (no xAI Responses extensions). Anything else is first-party.
pub fn is_third_party_base_url(base_url: &str) -> bool {
    !(is_trusted_cli_chat_proxy_url(base_url) || is_trusted_xai_https_url(base_url))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_party_urls_are_not_third_party() {
        assert!(!is_third_party_base_url("https://api.x.ai/v1"));
        assert!(!is_third_party_base_url(
            "https://api.x.ai/v1/chat/completions"
        ));
        assert!(!is_third_party_base_url("https://sub.api.x.ai/v1"));
        assert!(!is_third_party_base_url(
            xai_grok_env::PROD_CLI_CHAT_PROXY_BASE_URL
        ));
    }

    #[test]
    fn lookalikes_and_cleartext_are_third_party() {
        // Suffix attack: different registrable domain.
        assert!(is_third_party_base_url("https://evil-x.ai.example/v1"));
        assert!(is_third_party_base_url("https://x.ai.evil.com/v1"));
        // Trust requires https, even for xAI hosts.
        assert!(is_third_party_base_url("http://api.x.ai/v1"));
        // Loopback is never trusted here (unit-test mocks are third-party shaped).
        assert!(is_third_party_base_url("http://localhost:8080/v1"));
        assert!(is_third_party_base_url("http://127.0.0.1:8080/v1"));
        // Unparseable fails closed.
        assert!(is_third_party_base_url("not a url"));
    }

    #[test]
    fn third_party_gateways_are_third_party() {
        assert!(is_third_party_base_url("https://api.anthropic.com/v1"));
        assert!(is_third_party_base_url("https://api.example.com/v1"));
        assert!(is_third_party_base_url("https://gateway.example.com/v1"));
    }

    #[test]
    fn trailing_slash_on_trusted_base_matches_the_same_subpaths() {
        // No-slash base: the pre-existing behavior.
        assert!(matches_trusted_base_url(
            "https://proxy.x.ai/v1/chat",
            "https://proxy.x.ai/v1"
        ));
        // A trailing slash on the base must not break subpath matching.
        assert!(matches_trusted_base_url(
            "https://proxy.x.ai/v1/chat",
            "https://proxy.x.ai/v1/"
        ));
        assert!(matches_trusted_base_url(
            "https://proxy.x.ai/v1",
            "https://proxy.x.ai/v1/"
        ));
        // A different path under the same host is still not trusted.
        assert!(!matches_trusted_base_url(
            "https://proxy.x.ai/other/chat",
            "https://proxy.x.ai/v1"
        ));
        assert!(!matches_trusted_base_url(
            "https://proxy.x.ai/other/chat",
            "https://proxy.x.ai/v1/"
        ));
    }
}

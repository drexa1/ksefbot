use std::sync::LazyLock;

static CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// Shared `reqwest::Client`, reused across requests to pool connections (keep-alive, TLS session reuse).
pub fn http_client() -> &'static reqwest::Client {
    &CLIENT
}

use anyhow::Context;

/// Create an HTTP client configured to use webpki-roots instead of native certs.
/// This is necessary for TLS to work in sandboxed environments like Claude Code.
pub fn create_http_client(user_agent: &str, timeout_secs: u64) -> anyhow::Result<reqwest::Client> {
    // Build a root store with webpki-roots instead of the platform verifier
    let root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    // Create TLS configuration using webpki-roots (not platform verifier)
    let config = rustls::ClientConfig::builder_with_provider(
        rustls::crypto::aws_lc_rs::default_provider().into(),
    )
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_root_certificates(root_store)
    .with_no_client_auth();

    reqwest::Client::builder()
        .use_preconfigured_tls(config)
        .user_agent(user_agent)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .build()
        .context("Failed to build HTTP client")
}

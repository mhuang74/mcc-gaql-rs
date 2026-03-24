# Fix LLM TLS Errors by Passing Custom HTTP Client to rig-core

## Context

The direct HTTP clients (R2, bundle downloads, scraper) were fixed by configuring reqwest to use webpki-roots instead of native platform certificates. However, LLM API calls via `rig-core` still fail with:

```
ERROR [rustls_platform_verifier::verification::apple] failed to verify TLS certificate:
invalid peer certificate: Other(OtherError("OSStatus -26276: -26276"))
```

This happens because rig-core creates its own internal HTTP client with `rustls-platform-verifier`, which tries to use the macOS Security Framework for certificate validation - unavailable in the Claude Code sandbox.

## Discovery

`ClientBuilder::http_client()` method exists in rig-core 0.33.0+ that allows injecting a custom HTTP client:

```rust
/// Set the HTTP backend used in this client
pub fn http_client<U>(self, http_client: U) -> ClientBuilder<Ext, ApiKey, U>
```

This means we can pass a `reqwest::Client` configured with webpki-roots to the rig-core OpenAI builder.

## Implementation Plan

### Step 1: Update LlmConfig::create_llm_client() to use webpki-roots

**File**: `crates/mcc-gaql-gen/src/rag.rs`

Change the HTTP client creation in `LlmConfig::create_llm_client()`:

**Before (line ~142-148):**
```rust
pub fn create_llm_client(&self) -> Result<openai::CompletionsClient, anyhow::Error> {
    openai::CompletionsClient::builder()
        .api_key(&self.api_key)
        .base_url(&self.base_url)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create LLM client: {}", e))
}
```

**After:**
```rust
pub fn create_llm_client(&self) -> Result<openai::CompletionsClient, anyhow::Error> {
    use mcc_gaql_common::http_client;

    let client = http_client::create_http_client("mcc-gaql-gen (LLM client)", 120)?;

    openai::CompletionsClient::builder()
        .api_key(&self.api_key)
        .base_url(&self.base_url)
        .http_client(client)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to create LLM client: {}", e))
}
```

Note: We added `use mcc_gaql_common::http_client;` and call `.http_client(client)` with our webpki-roots-configured client.

### Step 2: Verify compilation

```bash
cargo check --workspace
```

### Step 3: Test the generate command

```bash
cargo run -p mcc-gaql-gen -- generate "campaign performance last week"
```

Should succeed without TLS verification errors.

## Critical Files

1. `crates/mcc-gaql-gen/src/rag.rs` - Add http_client import and pass custom client to builder

## Verification

1. `cargo check --workspace` compiles without errors
2. Run `generate` command - should complete without TLS errors
3. The generated query should be returned successfully

## Notes

- The `webpki-roots` feature is already enabled in workspace `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "rustls-tls-webpki-roots"] }`
- The `mcc-gaql-common/src/http_client.rs::create_http_client()` utility already exists with correct TLS configuration
- This is a minimal change - only 2 lines added (import + http_client() method call)

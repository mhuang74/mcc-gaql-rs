# Fix TLS Errors in Sandbox by Using webpki-roots

## Context

When using 3rd-party REST APIs inside the Claude Code sandbox, TLS certificate verification fails with error `OSStatus -26276`. This occurs because:

1. `rig-core` (used in `rag.rs`) uses `reqwest 0.13` with `rustls-platform-verifier`
2. `rustls-platform-verifier` relies on macOS Security Framework for certificate validation
3. The sandbox environment doesn't have access to the system certificate store

The solution is to configure `reqwest` to use `webpki-roots` (Mozilla's root certificates) instead of the native platform verifier.

## Current State

- Workspace uses `reqwest 0.11`
- `rig-core` internally uses `reqwest 0.13` (via `rustls-platform-verifier`)
- Four files create HTTP clients:
  - `crates/mcc-gaql-gen/src/rag.rs:142-147` - LLM client via `rig-core` (not directly configurable)
  - `crates/mcc-gaql-gen/src/r2.rs:61-65,122-126,195-199` - R2 download/upload clients
  - `crates/mcc-gaql-gen/src/bundle.rs:307-311` - Bundle download client
  - `crates/mcc-gaql-gen/src/scraper.rs:145-149` - Google docs scraper client

## Implementation Plan

### Step 1: Upgrade reqwest and add webpki-roots dependency

**File**: `/Users/mhuang/Projects/Development/googleads/gaql_bug_fixes/Cargo.toml`

Change:
```toml
# Old
reqwest = { version = "0.11", default-features = false, features = ["json", "rustls-tls"] }

# New
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls", "rustls-tls-webpki-roots"] }
```

### Step 2: Create a centralized HTTP client builder utility

**File**: `crates/mcc-gaql-common/src/http_client.rs` (new file)

Create a shared utility function that configures reqwest clients with webpki-roots:

```rust
use anyhow::Context;

/// Create an HTTP client configured to use webpki-roots instead of native certs.
/// This is necessary for TLS to work in sandboxed environments like Claude Code.
pub fn create_http_client(user_agent: &str, timeout_secs: u64) -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent(user_agent)
        .timeout(std::time::Duration::from_secs(timeout_secs))
        .tls_built_in_root_certs(false)  // Disable native certs
        .tls_built_in_webpki_certs(true) // Use webpki-roots instead
        .build()
        .context("Failed to build HTTP client")
}
```

Export this in `crates/mcc-gaql-common/src/lib.rs` by adding:
```rust
pub mod http_client;
```

### Step 3: Update all direct reqwest clients to use the utility

**File**: `crates/mcc-gaql-gen/src/r2.rs`

Replace the three `Client::builder()` calls:

**Line 61-65** (metadata downloader):
```rust
// Before:
let client = reqwest::Client::builder()
    .user_agent("mcc-gaql-gen (metadata downloader)")
    .timeout(std::time::Duration::from_secs(120))
    .build()
    .context("Failed to build HTTP client")?;

// After:
let client = mcc_gaql_common::http_client::create_http_client(
    "mcc-gaql-gen (metadata downloader)",
    120,
)?;
```

**Line 122-126** (bundle downloader):
```rust
// Before:
let client = reqwest::Client::builder()
    .user_agent("mcc-gaql-gen (bundle downloader)")
    .timeout(std::time::Duration::from_secs(300))
    .build()
    .context("Failed to build HTTP client")?;

// After:
let client = mcc_gaql_common::http_client::create_http_client(
    "mcc-gaql-gen (bundle downloader)",
    300,
)?;
```

**Line 195-199** (metadata uploader):
```rust
// Before:
let client = reqwest::Client::builder()
    .user_agent("mcc-gaql-gen (metadata uploader)")
    .timeout(std::time::Duration::from_secs(300))
    .build()
    .context("Failed to build HTTP client")?;

// After:
let client = mcc_gaql_common::http_client::create_http_client(
    "mcc-gaql-gen (metadata uploader)",
    300,
)?;
```

**File**: `crates/mcc-gaql-gen/src/bundle.rs`

**Line 307-311** (bundle download):
```rust
// Before:
let client = reqwest::Client::builder()
    .user_agent("mcc-gaql-gen (bundle downloader)")
    .timeout(std::time::Duration::from_secs(300))
    .build()
    .context("Failed to build HTTP client")?;

// After:
let client = mcc_gaql_common::http_client::create_http_client(
    "mcc-gaql-gen (bundle downloader)",
    300,
)?;
```

**File**: `crates/mcc-gaql-gen/src/scraper.rs`

**Line 145-149** (Google docs scraper):
```rust
// Before:
let client = reqwest::Client::builder()
    .user_agent("mcc-gaql metadata scraper (educational/documentation use)")
    .timeout(std::time::Duration::from_secs(15))
    .build()
    .context("Failed to build HTTP client")?;

// After:
let client = mcc_gaql_common::http_client::create_http_client(
    "mcc-gaql metadata scraper (educational/documentation use)",
    15,
)?;
```

### Step 4: Note on rig-core limitation

The `rig-core` crate (used in `rag.rs` at lines 142-147) manages its own `reqwest::Client` internally via `openai::CompletionsClient::builder()` and does not expose configuration options for TLS. This fix addresses the direct HTTP clients only.

Options for addressing rig-core TLS issues:
1. **Upgrade rig-core** - Check if newer versions support custom HTTP clients
2. **Patch rig-core** - Fork and add TLS configuration options
3. **Replace rig-core** - Implement a custom OpenAI client with proper TLS configuration
4. **Use environment workarounds** - Some environments allow disabling TLS verification via env vars (not recommended for production)

These options are out of scope for this immediate fix and should be addressed separately.

### Step 5: Verify the build

```bash
# Check compilation
cargo check --workspace

# Run tests
cargo test --workspace -- --test-threads=1

# Build release binaries
cargo build -p mcc-gaql-gen --release
```

## Critical Files to Modify

1. `/Users/mhuang/Projects/Development/googleads/gaql_bug_fixes/Cargo.toml` - Update reqwest version and features
2. `crates/mcc-gaql-common/src/http_client.rs` - New file with shared HTTP client builder
3. `crates/mcc-gaql-common/src/lib.rs` - Export the new http_client module
4. `crates/mcc-gaql-gen/src/r2.rs` - Update HTTP clients (lines 61, 122, 195)
5. `crates/mcc-gaql-gen/src/bundle.rs` - Update HTTP client (line 307)
6. `crates/mcc-gaql-gen/src/scraper.rs` - Update HTTP client (line 145)

## Testing

1. Build the project: `cargo build --workspace`
2. Run unit tests: `cargo test --workspace -- --test-threads=1`
3. Manually test HTTP requests work in the sandbox environment by running commands that trigger R2 downloads/uploads

## Risks and Considerations

1. **rig-core limitation**: The LLM client in `rag.rs` uses `rig-core` which manages its own reqwest client. This fix will NOT resolve TLS issues for LLM calls unless rig-core is also addressed separately.

2. **Breaking changes in reqwest 0.12**: The API changes between 0.11 and 0.12 are minimal. The main changes are:
   - Some `Response` methods may behave slightly differently
   - TLS configuration methods were added/enhanced
   - The `rustls-tls-webpki-roots` feature was added

3. **Certificate trust**: Using webpki-roots means trusting Mozilla's root certificates instead of the system store. This is generally more consistent across environments but may differ from browser behavior on some systems.

4. **Security implications**: Disabling native certs and using only webpki-roots means certificates trusted by the OS but not in the Mozilla bundle will be rejected. This is a trade-off for sandbox compatibility.

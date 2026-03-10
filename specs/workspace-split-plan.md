# Implementation Plan: Cargo Workspace Split

## Overview

Split the monolithic `mcc-gaql` binary into a Cargo workspace with three crates:
- **mcc-gaql**: Lightweight query tool (~15-20 MB)
- **mcc-gaql-gen**: GAQL generation tool with LLM/RAG (~400 MB)
- **mcc-gaql-common**: Shared types and utilities

## Goals

1. Reduce core binary size from ~430 MB to ~15-20 MB
2. Reduce build time for users who only need query functionality
3. Share enriched metadata globally via Cloudflare R2
4. Maintain backwards compatibility for existing users

---

## Phase 1: Create Workspace Structure

### Step 1.1: Create Directory Layout

```bash
mkdir -p crates/mcc-gaql/src
mkdir -p crates/mcc-gaql-gen/src
mkdir -p crates/mcc-gaql-common/src
```

### Step 1.2: Create Root Workspace Cargo.toml

**File:** `Cargo.toml` (replace existing)

```toml
[workspace]
resolver = "2"
members = [
    "crates/mcc-gaql",
    "crates/mcc-gaql-gen",
    "crates/mcc-gaql-common",
]

[workspace.package]
version = "0.15.0"
authors = ["Michael S. Huang <mhuang74@gmail.com>"]
edition = "2024"
license = "MIT"
repository = "https://github.com/mhuang74/mcc-gaql-rs"

[workspace.dependencies]
# Error handling
anyhow = "1.0"
thiserror = "2.0"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
bincode = "1.3"
toml = "0.5"

# Async runtime
tokio = { version = "1.0", features = ["rt-multi-thread", "time", "fs", "macros", "net"] }
tokio-stream = { version = "0.1", features = ["net"] }
futures = { version = "0.3", default-features = false, features = ["alloc"] }

# Configuration
figment = { version = "0.10", features = ["toml", "env"] }
dirs = "4.0"

# Logging
log = "0.4"
flexi_logger = { version = "0.22", features = ["compress"] }

# CLI
clap = { version = "3.1", features = ["derive", "cargo"] }
dialoguer = "0.11"

# Google Ads
googleads-rs = { version = "0.13.0", git = "https://github.com/mhuang74/googleads-rs.git", branch = "main" }
tonic = { version = "0.14", features = ["transport", "tls-ring", "tls-native-roots"] }
yup-oauth2 = "6.7"

# Data processing
polars = { version = "0.42", default-features = false, features = ["lazy", "fmt", "csv"] }
chrono = "0.4.42"
itertools = "0.10"
thousands = "0.2"

# HTTP client
reqwest = { version = "0.11", default-features = false, features = ["json", "rustls-tls"] }

# LLM/RAG (mcc-gaql-gen only)
rig-core = "0.32.0"
rig-fastembed = "0.3"
rig-lancedb = "0.4"
lancedb = { version = "0.23", default-features = false }
arrow-array = "56.2.0"
arrow-schema = "56.2.0"
scraper = "0.22"

# Caching
cacache = "10.0"

# Internal crates
mcc-gaql-common = { path = "crates/mcc-gaql-common" }
```

---

## Phase 2: Extract Common Crate

### Step 2.1: Create mcc-gaql-common/Cargo.toml

**File:** `crates/mcc-gaql-common/Cargo.toml`

```toml
[package]
name = "mcc-gaql-common"
version.workspace = true
authors.workspace = true
edition.workspace = true
description = "Shared types and utilities for mcc-gaql tools"

[dependencies]
anyhow.workspace = true
serde.workspace = true
serde_json.workspace = true
figment.workspace = true
dirs.workspace = true
log.workspace = true
chrono.workspace = true
```

### Step 2.2: Extract Shared Types

**File:** `crates/mcc-gaql-common/src/lib.rs`

```rust
pub mod config;
pub mod field_metadata;
pub mod paths;

pub use config::Config;
pub use field_metadata::{FieldMetadata, FieldMetadataCache};
```

### Step 2.3: Move/Adapt Shared Code

| Source File | Target | What to Extract |
|-------------|--------|-----------------|
| `src/config.rs` | `common/src/config.rs` | `Config`, `ProfileConfig`, config loading |
| `src/field_metadata.rs` | `common/src/field_metadata.rs` | `FieldMetadata` struct, cache types |
| `src/util.rs` | `common/src/paths.rs` | `config_dir()`, `cache_dir()`, path helpers |

**New file:** `crates/mcc-gaql-common/src/paths.rs`

```rust
use anyhow::Result;
use std::path::PathBuf;

pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find config directory"))?
        .join("mcc-gaql");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn cache_dir() -> Result<PathBuf> {
    let dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not find cache directory"))?
        .join("mcc-gaql");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn field_metadata_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("field_metadata.json"))
}

pub fn enriched_metadata_path() -> Result<PathBuf> {
    Ok(config_dir()?.join("field_metadata_enriched.json"))
}

pub fn lancedb_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("lancedb"))
}
```

---

## Phase 3: Create mcc-gaql (Core Query Tool)

### Step 3.1: Create mcc-gaql/Cargo.toml

**File:** `crates/mcc-gaql/Cargo.toml`

```toml
[package]
name = "mcc-gaql"
version.workspace = true
authors.workspace = true
edition.workspace = true
description = "Execute GAQL across MCC child accounts"

[[bin]]
name = "mcc-gaql"
path = "src/main.rs"

[dependencies]
mcc-gaql-common.workspace = true

# Error handling
anyhow.workspace = true
thiserror.workspace = true

# Serialization
serde.workspace = true
serde_json.workspace = true
bincode.workspace = true
toml.workspace = true

# Async
tokio.workspace = true
tokio-stream.workspace = true
futures.workspace = true

# Config & CLI
figment.workspace = true
dirs.workspace = true
clap.workspace = true
dialoguer.workspace = true

# Logging
log.workspace = true
flexi_logger.workspace = true

# Google Ads
googleads-rs.workspace = true
tonic.workspace = true
yup-oauth2.workspace = true

# Data processing
polars.workspace = true
chrono.workspace = true
itertools.workspace = true
thousands.workspace = true

# Caching
cacache.workspace = true

# HTTP (for field metadata download only)
reqwest.workspace = true

[features]
default = []
external_client_secret = []
```

### Step 3.2: Move Core Source Files

| Source | Destination | Changes Required |
|--------|-------------|------------------|
| `src/main.rs` | `crates/mcc-gaql/src/main.rs` | Remove LLM feature gates, simplify |
| `src/args.rs` | `crates/mcc-gaql/src/args.rs` | Remove `--prompt`, `--clear-vector-cache`, etc. |
| `src/googleads.rs` | `crates/mcc-gaql/src/googleads.rs` | No changes |
| `src/setup.rs` | `crates/mcc-gaql/src/setup.rs` | No changes |
| `src/util.rs` | `crates/mcc-gaql/src/util.rs` | Keep query parsing, remove shared paths |

### Step 3.3: Simplify main.rs

Remove all LLM-related code paths:
- Remove `#[cfg(feature = "llm")]` blocks
- Remove `--prompt` handling
- Remove `--clear-vector-cache` handling
- Remove `--enrich-metadata` handling
- Keep core GAQL execution logic

### Step 3.4: Simplify args.rs

Remove these arguments:
- `--prompt` / `-p`
- `--clear-vector-cache`
- `--enrich-metadata`
- `--llm-base-url`
- `--llm-model`
- `--llm-api-key`

---

## Phase 4: Create mcc-gaql-gen (GAQL Generation Tool)

### Step 4.1: Create mcc-gaql-gen/Cargo.toml

**File:** `crates/mcc-gaql-gen/Cargo.toml`

```toml
[package]
name = "mcc-gaql-gen"
version.workspace = true
authors.workspace = true
edition.workspace = true
description = "Generate GAQL from natural language using LLM/RAG"

[[bin]]
name = "mcc-gaql-gen"
path = "src/main.rs"

[dependencies]
mcc-gaql-common.workspace = true

# Error handling
anyhow.workspace = true
thiserror.workspace = true

# Serialization
serde.workspace = true
serde_json.workspace = true

# Async
tokio.workspace = true
futures.workspace = true

# Config & CLI
figment.workspace = true
dirs.workspace = true
clap.workspace = true

# Logging
log.workspace = true
flexi_logger.workspace = true

# HTTP
reqwest.workspace = true
scraper.workspace = true

# LLM/RAG
rig-core.workspace = true
rig-fastembed.workspace = true
rig-lancedb.workspace = true
lancedb.workspace = true
arrow-array.workspace = true
arrow-schema.workspace = true

# Data processing
chrono.workspace = true
itertools.workspace = true
```

### Step 4.2: Create CLI Structure

**File:** `crates/mcc-gaql-gen/src/main.rs`

```rust
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[clap(name = "mcc-gaql-gen")]
#[clap(about = "Generate GAQL from natural language")]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scrape Google Ads API documentation
    Scrape {
        /// API version (e.g., v19)
        #[clap(long, default_value = "v19")]
        api_version: String,
    },

    /// Enrich field metadata with LLM descriptions
    Enrich {
        /// LLM model to use
        #[clap(long, env = "MCC_GAQL_LLM_MODEL")]
        model: Option<String>,
    },

    /// Generate GAQL from natural language prompt
    Generate {
        /// Natural language query
        prompt: String,

        /// Use local metadata instead of R2
        #[clap(long)]
        local: bool,
    },

    /// Upload metadata to Cloudflare R2
    Upload {
        /// R2 bucket name
        #[clap(long, default_value = "mcc-gaql-metadata")]
        bucket: String,
    },

    /// Download metadata from Cloudflare R2
    Download {
        /// API version to download
        #[clap(long, default_value = "v19")]
        api_version: String,
    },

    /// Clear local caches (vector store, embeddings)
    ClearCache,
}
```

### Step 4.3: Move LLM Source Files

| Source | Destination | Changes Required |
|--------|-------------|------------------|
| `src/prompt2gaql.rs` | `crates/mcc-gaql-gen/src/rag.rs` | Adapt imports |
| `src/lancedb_utils.rs` | `crates/mcc-gaql-gen/src/vector_store.rs` | Adapt imports |
| `src/metadata_scraper.rs` | `crates/mcc-gaql-gen/src/scraper.rs` | Adapt imports |
| `src/metadata_enricher.rs` | `crates/mcc-gaql-gen/src/enricher.rs` | Adapt imports |

### Step 4.4: Add R2 Client

**File:** `crates/mcc-gaql-gen/src/r2.rs`

```rust
use anyhow::Result;
use reqwest::Client;
use std::path::Path;

const R2_PUBLIC_URL: &str = "https://pub-XXXXX.r2.dev";

pub struct R2Client {
    client: Client,
    bucket: String,
    access_key: Option<String>,
    secret_key: Option<String>,
}

impl R2Client {
    pub fn new(bucket: &str) -> Self {
        Self {
            client: Client::new(),
            bucket: bucket.to_string(),
            access_key: std::env::var("R2_ACCESS_KEY").ok(),
            secret_key: std::env::var("R2_SECRET_KEY").ok(),
        }
    }

    /// Download file from public R2 bucket
    pub async fn download(&self, key: &str, dest: &Path) -> Result<()> {
        let url = format!("{}/{}", R2_PUBLIC_URL, key);
        let resp = self.client.get(&url).send().await?;
        let bytes = resp.bytes().await?;
        tokio::fs::write(dest, bytes).await?;
        Ok(())
    }

    /// Upload file to R2 (requires credentials)
    pub async fn upload(&self, key: &str, src: &Path) -> Result<()> {
        let access_key = self.access_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("R2_ACCESS_KEY not set"))?;
        let secret_key = self.secret_key.as_ref()
            .ok_or_else(|| anyhow::anyhow!("R2_SECRET_KEY not set"))?;

        // Use AWS SDK compatible S3 API
        // Implementation details...
        todo!("Implement S3-compatible upload")
    }
}
```

---

## Phase 5: Update Tests

### Step 5.1: Move Tests to Appropriate Crates

| Test File | Destination |
|-----------|-------------|
| `tests/integration_tests.rs` | `crates/mcc-gaql/tests/` |
| `tests/metadata_scraper_live_tests.rs` | `crates/mcc-gaql-gen/tests/` |

### Step 5.2: Update Test Dependencies

Add `tempfile` to dev-dependencies in both crate Cargo.toml files.

---

## Phase 6: CI/CD Updates

### Step 6.1: Update Build Workflow

**File:** `.github/workflows/build.yml` (update)

```yaml
jobs:
  build:
    steps:
      - name: Build mcc-gaql (core)
        run: cargo build --release -p mcc-gaql

      - name: Build mcc-gaql-gen
        run: cargo build --release -p mcc-gaql-gen
```

### Step 6.2: Add Metadata Pipeline Workflow

**File:** `.github/workflows/metadata-pipeline.yml` (new)

```yaml
name: Metadata Pipeline

on:
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday
  workflow_dispatch:

jobs:
  scrape-enrich-upload:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-action@stable

      - name: Build mcc-gaql-gen
        run: cargo build --release -p mcc-gaql-gen

      - name: Scrape API docs
        run: ./target/release/mcc-gaql-gen scrape

      - name: Enrich with LLM
        env:
          MCC_GAQL_LLM_API_KEY: ${{ secrets.LLM_API_KEY }}
          MCC_GAQL_LLM_MODEL: ${{ secrets.LLM_MODEL }}
        run: ./target/release/mcc-gaql-gen enrich

      - name: Upload to R2
        env:
          R2_ACCESS_KEY: ${{ secrets.R2_ACCESS_KEY }}
          R2_SECRET_KEY: ${{ secrets.R2_SECRET_KEY }}
        run: ./target/release/mcc-gaql-gen upload
```

---

## Phase 7: Documentation Updates

### Step 7.1: Update README.md

- Document both binaries
- Explain when to use each
- Add installation instructions for both

### Step 7.2: Update CLAUDE.md

- Update architecture diagram
- Update module descriptions
- Update file locations

---

## Migration Checklist

### Pre-Migration
- [ ] Create feature branch `workspace-split`
- [ ] Ensure all tests pass on main
- [ ] Document current binary size for comparison

### Phase 1: Workspace Structure
- [ ] Create `crates/` directory structure
- [ ] Create root `Cargo.toml` with workspace config
- [ ] Verify `cargo check` works (will fail until crates exist)

### Phase 2: Common Crate
- [ ] Create `mcc-gaql-common/Cargo.toml`
- [ ] Extract `Config` and related types
- [ ] Extract `FieldMetadata` types
- [ ] Extract path utilities
- [ ] Create `lib.rs` with public exports
- [ ] Verify `cargo build -p mcc-gaql-common`

### Phase 3: Core Query Tool
- [ ] Create `mcc-gaql/Cargo.toml`
- [ ] Move/adapt `main.rs` (remove LLM code)
- [ ] Move/adapt `args.rs` (remove LLM args)
- [ ] Move `googleads.rs`
- [ ] Move `setup.rs`
- [ ] Move/adapt `util.rs`
- [ ] Update imports to use `mcc-gaql-common`
- [ ] Verify `cargo build -p mcc-gaql`
- [ ] Verify `cargo test -p mcc-gaql`

### Phase 4: Generation Tool
- [ ] Create `mcc-gaql-gen/Cargo.toml`
- [ ] Create CLI structure with subcommands
- [ ] Move/adapt `prompt2gaql.rs` → `rag.rs`
- [ ] Move/adapt `lancedb_utils.rs` → `vector_store.rs`
- [ ] Move/adapt `metadata_scraper.rs` → `scraper.rs`
- [ ] Move/adapt `metadata_enricher.rs` → `enricher.rs`
- [ ] Implement R2 client
- [ ] Update imports to use `mcc-gaql-common`
- [ ] Verify `cargo build -p mcc-gaql-gen`
- [ ] Verify `cargo test -p mcc-gaql-gen`

### Phase 5: Tests
- [ ] Move integration tests to appropriate crates
- [ ] Update test imports
- [ ] Verify all tests pass

### Phase 6: CI/CD
- [ ] Update build workflow
- [ ] Add metadata pipeline workflow
- [ ] Test workflows in CI

### Phase 7: Documentation
- [ ] Update README.md
- [ ] Update CLAUDE.md
- [ ] Add migration guide for existing users

### Post-Migration
- [ ] Measure new binary sizes
- [ ] Compare build times
- [ ] Test both binaries end-to-end
- [ ] Create PR with detailed changelog

---

## Expected Outcomes

| Metric | Before | After |
|--------|--------|-------|
| mcc-gaql binary size | ~431 MB | ~15-20 MB |
| mcc-gaql-gen binary size | N/A | ~400 MB |
| mcc-gaql build time | ~5 min | ~1 min |
| mcc-gaql dependencies | ~2400 | ~500 |

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Breaking existing users | Keep CLI interface identical for core tool |
| Shared code divergence | Strong typing in common crate |
| R2 availability | Fallback to local generation |
| CI secrets exposure | Use GitHub encrypted secrets |

---

## Open Questions

1. **R2 bucket naming**: `googleads-metadata` 
2. **Versioning strategy**: Crates have separate versions
3. **Release process**: Separate releases 
4. **Backwards compatibility**: No need to support old combined binary

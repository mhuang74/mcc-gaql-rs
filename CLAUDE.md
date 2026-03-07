# CLAUDE.md - Project Guide for AI Assistants

## Project Overview

**mcc-gaql** is a Rust CLI tool for executing Google Ads Query Language (GAQL) queries across multiple accounts linked to a Manager (MCC) account. It supports natural language query conversion via LLM/RAG.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                           main.rs                               │
│  CLI entry point, argument handling, orchestration              │
└─────────────────────────┬───────────────────────────────────────┘
                          │
    ┌─────────────────────┼─────────────────────┐
    │                     │                     │
    ▼                     ▼                     ▼
┌─────────┐       ┌─────────────┐       ┌─────────────┐
│ args.rs │       │ config.rs   │       │ setup.rs    │
│ CLI def │       │ TOML config │       │ Wizard      │
└─────────┘       └─────────────┘       └─────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                     Core Google Ads Layer                        │
├─────────────────────────────────────────────────────────────────┤
│  googleads.rs          - gRPC client, OAuth2, query execution   │
│  field_metadata.rs     - Fields Service API cache               │
│  util.rs               - Query file parsing, helpers            │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                   LLM/RAG Layer (feature = "llm")               │
├─────────────────────────────────────────────────────────────────┤
│  prompt2gaql.rs        - Natural language → GAQL via RAG        │
│  lancedb_utils.rs      - Vector store (LanceDB) management      │
│  metadata_enricher.rs  - LLM-based field description enrichment │
│  metadata_scraper.rs   - Scrape Google Ads API docs             │
└─────────────────────────────────────────────────────────────────┘
```

## Module Descriptions

| Module | Purpose |
|--------|---------|
| `args.rs` | CLI argument definitions using clap |
| `config.rs` | Profile-based TOML configuration with env var override |
| `setup.rs` | Interactive configuration wizard |
| `googleads.rs` | Google Ads API client: OAuth2, gRPC, streaming queries |
| `field_metadata.rs` | Cache of field metadata from Fields Service API |
| `util.rs` | Query file parsing, logging setup, helpers |
| `prompt2gaql.rs` | RAG pipeline: embeddings + LLM for natural language queries |
| `lancedb_utils.rs` | LanceDB vector store for RAG retrieval |
| `metadata_scraper.rs` | HTTP scraper for Google Ads API documentation |
| `metadata_enricher.rs` | LLM-based enrichment of field descriptions |

## Key Features

1. **Multi-account queries**: Execute GAQL across all MCC child accounts
2. **Profile-based config**: Multiple profiles in `config.toml`
3. **Output formats**: Table, CSV, JSON with groupby/sortby
4. **Natural language queries**: Convert English → GAQL via LLM (experimental)
5. **Field metadata cache**: Local cache of Google Ads schema
6. **Stored queries**: Query cookbook in TOML format
7. **Interactive setup wizard**: `--setup` flag

## Feature Flags

```toml
[features]
default = ["llm"]
llm = ["dep:rig-core", "dep:rig-fastembed", "dep:rig-lancedb", "dep:lancedb", ...]
external_client_secret = []
```

- `llm` (default): Enables natural language queries, vector store, LLM enrichment
- `external_client_secret`: Force runtime loading of OAuth2 credentials

## Caveats & Known Issues

### Lance Crate Recursion Bug

The `lance` crate v1.0.1 (dependency of `lancedb 0.23`) has a compiler recursion limit issue:

```
error: queries overflow the depth limit!
  = note: query depth increased by 130 when computing layout of
          `{async block@lance-1.0.1/src/index.rs:873:5}`
```

**Root cause**: The lance crate's async code triggers a compiler recursion limit.

**Workaround**: LLM features are optional. Build/test without them:
```bash
cargo build --no-default-features
cargo test --no-default-features
```

**Upstream fix required**: Either:
- `lance` crate fix for the async layout issue
- `rig-lancedb` update to support `lancedb 0.26+` (which uses lance 2.0)

### Rust Version

The project uses Rust 1.90 with edition 2024.

## Running Tests

### Metadata Scraper Tests (Live)

These tests hit the actual Google Ads documentation website:

```bash
# Run without LLM features (avoids lance compilation issue)
cargo test --no-default-features --test metadata_scraper_live_tests -- --ignored --nocapture
```

### All Tests (requires LLM fix)

```bash
# Only works when lance crate issue is resolved
cargo test
```

### Unit Tests

```bash
cargo test --no-default-features --lib
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `MCC_GAQL_LLM_API_KEY` | LLM provider API key |
| `MCC_GAQL_LLM_BASE_URL` | LLM provider base URL |
| `MCC_GAQL_LLM_MODEL` | Model name (e.g., `google/gemini-flash-2.0`) |
| `MCC_GAQL_DEV_TOKEN` | Google Ads developer token |
| `MCC_GAQL_LOG_LEVEL` | Logging level (e.g., `debug`) |
| `OPENROUTER_API_KEY` | Alternative to MCC_GAQL_LLM_API_KEY |

## Coding Rules

1. **Error handling**: Use `anyhow::Result` for application errors, `thiserror` for library errors
2. **Async runtime**: Tokio with multi-threaded runtime
3. **Logging**: Use `log` crate macros (`log::info!`, `log::debug!`)
4. **Config precedence**: CLI args > env vars > config file > defaults
5. **Feature gates**: Wrap LLM code with `#[cfg(feature = "llm")]`
6. **gRPC**: Use `tonic` with `googleads-rs` generated types
7. **Data frames**: Use `polars` for query result manipulation

## File Locations

- **Config**: `~/.config/mcc-gaql/config.toml` (Linux) or `~/Library/Application Support/mcc-gaql/` (macOS)
- **Token cache**: Same directory as config, named by user email hash
- **Field metadata cache**: `~/.config/mcc-gaql/field_metadata.json`
- **LanceDB vector store**: `~/.cache/mcc-gaql/lancedb/`
- **Scraped docs cache**: `~/.config/mcc-gaql/scraped_docs.json`

## Common Development Tasks

```bash
# Build without LLM (fast, avoids lance issue)
cargo build --no-default-features

# Build release with embedded credentials
MCC_GAQL_DEV_TOKEN="token" MCC_GAQL_EMBED_CLIENT_SECRET="$(cat clientsecret.json)" \
  cargo build --release

# Run with debug logging
MCC_GAQL_LOG_LEVEL="debug" cargo run --no-default-features -- --help

# Check code without building
cargo check --no-default-features

# Format code
cargo fmt

# Lint
cargo clippy --no-default-features
```

## Dependencies Overview

| Crate | Purpose |
|-------|---------|
| `googleads-rs` | Google Ads API gRPC bindings |
| `tonic` | gRPC framework |
| `yup-oauth2` | OAuth2 authentication |
| `polars` | DataFrame operations |
| `clap` | CLI argument parsing |
| `figment` | Configuration management |
| `rig-core` | LLM abstraction layer |
| `rig-lancedb` | Vector store integration |
| `lancedb` | Embedded vector database |
| `reqwest` | HTTP client for scraping |

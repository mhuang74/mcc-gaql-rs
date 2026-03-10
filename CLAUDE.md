# CLAUDE.md - Project Guide for AI Assistants

## Project Overview

**mcc-gaql** is a Rust CLI tool for executing Google Ads Query Language (GAQL) queries across multiple accounts linked to a Manager (MCC) account.

This is a **Cargo workspace** with three crates:
- **mcc-gaql**: Lightweight query tool (~15-20 MB)
- **mcc-gaql-gen**: GAQL generation tool with LLM/RAG (~400 MB)
- **mcc-gaql-common**: Shared types and utilities

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         WORKSPACE ARCHITECTURE                          │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│   ┌──────────────────────┐         ┌──────────────────────────────┐    │
│   │   mcc-gaql           │         │   mcc-gaql-gen               │    │
│   │   (Query Tool)       │         │   (GAQL Generation Tool)     │    │
│   │   ~15-20 MB          │         │   ~400 MB                    │    │
│   │                      │         │                              │    │
│   │   • Execute GAQL     │         │   • scrape: API docs         │    │
│   │   • OAuth2           │         │   • enrich: LLM descriptions │    │
│   │   • CSV/JSON output  │         │   • generate: prompt→GAQL    │    │
│   │   • No LLM deps      │         │   • upload/download: R2      │    │
│   └──────────┬───────────┘         └───────────────┬──────────────┘    │
│              │                                     │                    │
│              └──────────────┬──────────────────────┘                    │
│                             ▼                                           │
│              ┌──────────────────────────────┐                           │
│              │   mcc-gaql-common            │                           │
│              │   (Shared Library)           │                           │
│              │                              │                           │
│              │   • Config types             │                           │
│              │   • FieldMetadata types      │                           │
│              │   • Path utilities           │                           │
│              └──────────────────────────────┘                           │
│                                                                         │
│   ┌─────────────────────────────────────────────────────────────────┐  │
│   │                      Cloudflare R2                               │  │
│   │   (Public read, maintainer write)                                │  │
│   │   • field_metadata_enriched.json                                 │  │
│   │   • embeddings (pre-computed vectors)                            │  │
│   └─────────────────────────────────────────────────────────────────┘  │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Repository Structure

```
mcc-gaql-rs/
├── Cargo.toml                    # Workspace root
├── CLAUDE.md
├── README.md
├── specs/                        # Implementation plans
│
├── crates/
│   ├── mcc-gaql/                 # Core query tool
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── args.rs           # CLI argument definitions
│   │       ├── config.rs         # Profile-based TOML config
│   │       ├── setup.rs          # Interactive wizard
│   │       ├── googleads.rs      # gRPC client, OAuth2, queries
│   │       └── util.rs           # Query parsing, helpers
│   │
│   ├── mcc-gaql-gen/             # GAQL generation tool
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs           # CLI: scrape, enrich, generate, upload
│   │       ├── scraper.rs        # HTTP scraper for API docs
│   │       ├── enricher.rs       # LLM-based field enrichment
│   │       ├── rag.rs            # RAG pipeline for prompt→GAQL
│   │       ├── vector_store.rs   # LanceDB vector store
│   │       └── r2.rs             # Cloudflare R2 client
│   │
│   └── mcc-gaql-common/          # Shared types and utilities
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── config.rs         # Shared config loading
│           ├── field_metadata.rs # FieldMetadata types
│           └── paths.rs          # config_dir(), cache_dir(), etc.
│
└── .github/
    └── workflows/
        ├── build.yml             # Build both binaries
        └── metadata-pipeline.yml # Scheduled scrape/enrich/upload
```

## Module Descriptions

### mcc-gaql (Core Query Tool)

| Module | Purpose |
|--------|---------|
| `args.rs` | CLI argument definitions using clap |
| `config.rs` | Profile-based TOML configuration with env var override |
| `setup.rs` | Interactive configuration wizard |
| `googleads.rs` | Google Ads API client: OAuth2, gRPC, streaming queries |
| `util.rs` | Query file parsing, logging setup, helpers |

### mcc-gaql-gen (GAQL Generation Tool)

| Module | Purpose |
|--------|---------|
| `scraper.rs` | HTTP scraper for Google Ads API documentation |
| `enricher.rs` | LLM-based enrichment of field descriptions |
| `rag.rs` | RAG pipeline: embeddings + LLM for natural language queries |
| `vector_store.rs` | LanceDB vector store management |
| `r2.rs` | Cloudflare R2 download/upload client |

### mcc-gaql-common (Shared Library)

| Module | Purpose |
|--------|---------|
| `config.rs` | Shared configuration types and loading |
| `field_metadata.rs` | FieldMetadata struct and cache types |
| `paths.rs` | Standard paths for config, cache, metadata files |

## Key Features

### mcc-gaql (Core Query Tool)

1. **Multi-account queries**: Execute GAQL across all MCC child accounts
2. **Profile-based config**: Multiple profiles in `config.toml`
3. **Output formats**: Table, CSV, JSON with groupby/sortby
4. **Field metadata cache**: Local cache of Google Ads schema
5. **Stored queries**: Query cookbook in TOML format
6. **Interactive setup wizard**: `--setup` flag

### mcc-gaql-gen (GAQL Generation Tool)

1. **Scrape API docs**: `mcc-gaql-gen scrape` - Fetch Google Ads API documentation
2. **Enrich metadata**: `mcc-gaql-gen enrich` - Add LLM-generated descriptions
3. **Generate GAQL**: `mcc-gaql-gen generate "prompt"` - Convert natural language to GAQL
4. **R2 integration**: `mcc-gaql-gen upload/download` - Sync metadata with Cloudflare R2

## Feature Flags

### mcc-gaql

```toml
[features]
default = []
external_client_secret = []  # Force runtime loading of OAuth2 credentials
```

### mcc-gaql-gen

No feature flags - all LLM/RAG dependencies are always included.

## Caveats & Known Issues

### Rust Version

The project uses Rust 1.90 with edition 2024.

## Running Tests

### All Tests (Workspace)

```bash
cargo test --workspace
```

### Core Query Tool Tests

```bash
cargo test -p mcc-gaql
```

### GAQL Generation Tool Tests

```bash
cargo test -p mcc-gaql-gen
```

### Metadata Scraper Tests (Live)

These tests hit the actual Google Ads documentation website:

```bash
cargo test -p mcc-gaql-gen --test metadata_scraper_live_tests -- --ignored --nocapture
```

## Environment Variables

| Variable | Purpose |
|----------|---------|
| `MCC_GAQL_DEV_TOKEN` | Google Ads developer token |
| `MCC_GAQL_LOG_LEVEL` | Logging level (e.g., `debug`) |
| `MCC_GAQL_LLM_API_KEY` | LLM provider API key (mcc-gaql-gen) |
| `MCC_GAQL_LLM_BASE_URL` | LLM provider base URL (mcc-gaql-gen) |
| `MCC_GAQL_LLM_MODEL` | Model name (mcc-gaql-gen) |
| `OPENROUTER_API_KEY` | Alternative to MCC_GAQL_LLM_API_KEY |
| `R2_ACCESS_KEY` | Cloudflare R2 access key (mcc-gaql-gen upload) |
| `R2_SECRET_KEY` | Cloudflare R2 secret key (mcc-gaql-gen upload) |

## Coding Rules

1. **Error handling**: Use `anyhow::Result` for application errors, `thiserror` for library errors
2. **Async runtime**: Tokio with multi-threaded runtime
3. **Logging**: Use `log` crate macros (`log::info!`, `log::debug!`)
4. **Config precedence**: CLI args > env vars > config file > defaults
5. **Shared code**: Put shared types/utilities in `mcc-gaql-common`
6. **gRPC**: Use `tonic` with `googleads-rs` generated types
7. **Data frames**: Use `polars` for query result manipulation (mcc-gaql only)

## File Locations

- **Config**: `~/.config/mcc-gaql/config.toml` (Linux) or `~/Library/Application Support/mcc-gaql/` (macOS)
- **Token cache**: Same directory as config, named by user email hash
- **Field metadata cache**: `~/.config/mcc-gaql/field_metadata.json`
- **LanceDB vector store**: `~/.cache/mcc-gaql/lancedb/`
- **Scraped docs cache**: `~/.config/mcc-gaql/scraped_docs.json`

## Common Development Tasks

```bash
# Build all crates
cargo build --workspace

# Build only core query tool (fast, ~15-20 MB)
cargo build -p mcc-gaql --release

# Build GAQL generation tool (slow, ~400 MB)
cargo build -p mcc-gaql-gen --release

# Build release with embedded credentials
MCC_GAQL_DEV_TOKEN="token" MCC_GAQL_EMBED_CLIENT_SECRET="$(cat clientsecret.json)" \
  cargo build -p mcc-gaql --release

# Run core query tool
cargo run -p mcc-gaql -- --help

# Run GAQL generation tool
cargo run -p mcc-gaql-gen -- --help

# Check code without building
cargo check --workspace

# Format code
cargo fmt --all

# Lint
cargo clippy --workspace
```

## Dependencies Overview

### mcc-gaql (Core - Lightweight)

| Crate | Purpose |
|-------|---------|
| `googleads-rs` | Google Ads API gRPC bindings |
| `tonic` | gRPC framework |
| `yup-oauth2` | OAuth2 authentication |
| `polars` | DataFrame operations |
| `clap` | CLI argument parsing |
| `figment` | Configuration management |

### mcc-gaql-gen (LLM/RAG - Heavy)

| Crate | Purpose |
|-------|---------|
| `rig-core` | LLM abstraction layer |
| `rig-fastembed` | Local embedding generation |
| `rig-lancedb` | Vector store integration |
| `lancedb` | Embedded vector database |
| `reqwest` | HTTP client for scraping |
| `scraper` | HTML parsing for API docs |

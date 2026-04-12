# DEVELOPER.md - Developer Guide

This guide covers architecture, development setup, and contribution guidelines for `mcc-gaql-rs`.

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
│   │   • Execute GAQL     │         │   • parse-protos: Field docs │    │
│   │   • OAuth2           │         │   • enrich: Merge proto docs │    │
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

### Design Decision: Two Separate Binaries

| Tool | Binary Name | Dependencies | Use Case |
|------|-------------|-------------|----------|
| Query Tool | `mcc-gaql` | ~500 crates, ~15-20 MB | Execute GAQL, export results |
| Generation Tool | `mcc-gaql-gen` | ~2400 crates, ~400 MB | Natural language → GAQL, metadata management |

**Benefits:**
- Keep the query tool lightweight and fast to build
- Allow users who only need query execution to avoid LLM dependencies
- Make it clear that natural language queries are a specialized feature

## Project Structure

```
mcc-gaql-rs/
├── Cargo.toml                    # Workspace root
├── CLAUDE.md                     # AI/coding agent guide
├── README.md
├── DEVELOPER.md                  # This file
│
├── crates/
│   ├── mcc-gaql/                 # Core query tool (lightweight)
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
│   │       ├── main.rs           # CLI: parse-protos, enrich, generate, upload (scrape: deprecated)
│   │       ├── scraper.rs        # HTTP scraper for API docs (deprecated - use proto_parser.rs)
│   │       ├── enricher.rs       # LLM-based field enrichment
│   │       ├── rag.rs            # RAG pipeline for prompt→GAQL
│   │       ├── vector_store.rs   # LanceDB vector store
│   │       ├── r2.rs             # Cloudflare R2 client
│   │       ├── proto_parser.rs   # Parse proto files for field docs
│   │       ├── proto_docs_cache.rs # Cache parsed proto documentation
│   │       └── proto_locator.rs  # Find googleads-rs proto directory
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
        ├── release.yml           # Create GitHub releases with both binaries
        └── metadata-pipeline.yml # Scheduled scrape/enrich/upload
```

### Module Descriptions

#### mcc-gaql (Core Query Tool)

| Module | Purpose |
|--------|---------|
| `args.rs` | CLI argument definitions using clap |
| `config.rs` | Profile-based TOML configuration with env var override |
| `setup.rs` | Interactive configuration wizard |
| `googleads.rs` | Google Ads API client: OAuth2, gRPC, streaming queries |
| `util.rs` | Query file parsing, logging setup, helpers |

#### mcc-gaql-gen (GAQL Generation Tool)

| Module | Purpose |
|--------|---------|
| `scraper.rs` | HTTP scraper for API docs (deprecated - use proto_parser.rs) |
| `enricher.rs` | LLM-based enrichment of field descriptions |
| `rag.rs` | RAG pipeline: embeddings + LLM for natural language queries |
| `vector_store.rs` | LanceDB vector store management |
| `r2.rs` | Cloudflare R2 download/upload client |
| `proto_parser.rs` | Parse proto files for field documentation |
| `proto_docs_cache.rs` | Cache and merge proto documentation |
| `proto_locator.rs` | Find googleads-rs proto directory |

#### mcc-gaql-common (Shared Library)

| Module | Purpose |
|--------|---------|
| `config.rs` | Shared configuration types and loading |
| `field_metadata.rs` | FieldMetadata struct and cache types |
| `paths.rs` | Standard paths for config, cache, metadata files |

## Development Setup

### Prerequisites

- Rust 1.90+
- `protobuf-compiler` (required for googleads-rs)

### Building from Source

```bash
# Build all crates
cargo build --workspace

# Build only core query tool (fast, ~15-20 MB)
cargo build -p mcc-gaql --release

# Build GAQL generation tool (slow, ~400 MB)
# Note: MCC_GAQL_R2_PUBLIC_ID required at build time
MCC_GAQL_R2_PUBLIC_ID="your_public_id" cargo build -p mcc-gaql-gen --release
```

### Running Tests

```bash
# Run all tests (workspace)
cargo test --workspace

# Run core query tool tests only
cargo test -p mcc-gaql

# Run GAQL generation tool tests only
cargo test -p mcc-gaql-gen
```

### Code Quality

```bash
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

## Build-Time Environment Variables

The following variable is required at build time for `mcc-gaql-gen`:

| Variable | Purpose |
|----------|---------|
| `MCC_GAQL_R2_PUBLIC_ID` | Hashed Cloudflare R2 public bucket ID for bootstrap downloads (required for mcc-gaql-gen) |

**Note:** `MCC_GAQL_DEV_TOKEN` and `MCC_GAQL_EMBED_CLIENT_SECRET` are NOT embedded at build time for security reasons. These must be provided at runtime via:
- Environment variables
- Config file (`dev_token` field)
- `clientsecret.json` file in config directory

### Embedding R2 Public ID for Bootstrap

The `bootstrap` command downloads pre-built metadata from a public R2 bucket. The public bucket ID must be embedded at build time:

```bash
export MCC_GAQL_R2_PUBLIC_ID="your_public_bucket_id"
cargo build -p mcc-gaql-gen --release
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Run tests: `cargo test --workspace`
5. Run linting: `cargo clippy --workspace`
6. Submit a pull request

## File Locations

Configuration and data files are stored in:

| File/Directory | Location |
|----------------|----------|
| Config file | `~/Library/Application Support/mcc-gaql/config.toml` (macOS)<br>`~/.config/mcc-gaql/config.toml` (Linux)<br>`%APPDATA%\mcc-gaql\config.toml` (Windows) |
| Proto docs cache | `~/Library/Caches/mcc-gaql/proto_docs_v23.json` (macOS)<br>`~/.cache/mcc-gaql/proto_docs_v23.json` (Linux) |
| Field metadata cache | `~/Library/Caches/mcc-gaql/field_metadata.json` (macOS)<br>`~/.cache/mcc-gaql/field_metadata.json` (Linux) |
| Enriched metadata | `~/Library/Caches/mcc-gaql/field_metadata_enriched.json` (macOS)<br>`~/.cache/mcc-gaql/field_metadata_enriched.json` (Linux) |
| LanceDB vector store | `~/Library/Caches/mcc-gaql/lancedb/` (macOS)<br>`~/.cache/mcc-gaql/lancedb/` (Linux) |
| Scraped docs cache | `~/Library/Caches/mcc-gaql/scraped_docs.json` (macOS)<br>`~/.cache/mcc-gaql/scraped_docs.json` (Linux) |
| Token cache | Same directory as config, named by user email hash |
| FastEmbed models | `~/Library/Caches/mcc-gaql/fastembed-models/` (macOS)<br>`~/.cache/mcc-gaql/fastembed-models/` (Linux) |

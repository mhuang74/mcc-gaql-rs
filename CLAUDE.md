# CLAUDE.md - Coding Agent Guide

## Coding Rules

- **Error handling**: `anyhow::Result` (app) → `thiserror` (lib)
- **Async**: Tokio with multi-thread runtime
- **Logging**: Use `log` crate macros `log::info!`, `log::debug!`
- **Config precedence**: CLI args > env vars > defaults
- **Shared code**: Put in `mcc-gaql-common`
- **gRPC**: `tonic` + `googleads-rs` generated types
- **Data frames**: `polars` in mcc-gaql only

## Build/Test Commands

**ALWAYS use `cargo build --workspace` (never `cargo build -p <pkg>`)**: Cargo feature unification differs between workspace and package-targeted builds, so `-p <pkg>` re-resolves features and triggers a full dependency rebuild — even though `../.cargo/config.toml` points `target-dir` at the shared warm cache `/rust_dev_cache/googleads_shared_target`. Workspace builds reuse cached artifacts; targeted builds do not. Use `-p <pkg>` only when a targeted **release** build is explicitly needed (`cargo build -p <pkg> --release`).

```bash
# Build (workspace) — preferred
cargo build --workspace
cargo check --workspace

# Single-crate builds (only when a targeted release build is needed)
cargo build -p mcc-gaql --release
cargo build -p mcc-gaql-gen --release

# Test (sequential to avoid race conditions)
cargo test --workspace -- --test-threads=1
cargo test -p mcc-gaql -- --test-threads=1
cargo test -p mcc-gaql-gen -- --test-threads=1
cargo test -p mcc-gaql-common -- --test-threads=1

# Format/Lint
cargo fmt --all
cargo clippy --workspace
```

## System Dependencies

- `protobuf-compiler` (required for googleads-rs)

## Workspace

3 crates: `mcc-gaql` (~15 MB, query tool), `mcc-gaql-gen` (~400 MB, LLM/RAG), `mcc-gaql-common` (shared)

## File Locations

Used by `mcc-gaql-common/src/paths.rs`:

| File/Directory | macOS Path | Linux Path |
|----------------|------------|------------|
| Config dir | `~/Library/Application Support/mcc-gaql/` | `~/.config/mcc-gaql/` |
| Cache dir | `~/Library/Caches/mcc-gaql/` | `~/.cache/mcc-gaql/` |
| Config file | `~/Library/Application Support/mcc-gaql/config.toml` | `~/.config/mcc-gaql/config.toml` |
| Token cache | `~/Library/Application Support/mcc-gaql/tokencache_*.json` | `~/.config/mcc-gaql/tokencache_*.json` |
| Field metadata | `~/Library/Caches/mcc-gaql/field_metadata.json` | `~/.cache/mcc-gaql/field_metadata.json` |
| Enriched metadata | `~/Library/Caches/mcc-gaql/field_metadata_enriched.json` | `~/.cache/mcc-gaql/field_metadata_enriched.json` |
| Proto docs | `~/Library/Caches/mcc-gaql/proto_docs_v24.json` | `~/.cache/mcc-gaql/proto_docs_v24.json` |
| Scraped docs | `~/Library/Caches/mcc-gaql/scraped_docs.json` | `~/.cache/mcc-gaql/scraped_docs.json` |
| LanceDB | `~/Library/Caches/mcc-gaql/lancedb/` | `~/.cache/mcc-gaql/lancedb/` |
| FastEmbed models | `~/Library/Caches/mcc-gaql/fastembed-models/` | `~/.cache/mcc-gaql/fastembed-models/` |

# Rust Sandbox Instructions
- Use `cargo check` instead of `cargo build` for quick validation.
- When running tests, use `cargo test --lib` to limit scope.
- The target directory is ignored by Git, but it persists in this sandbox.
- You have permission to skip prompts using `--allow-dangerously-skip-permissions`.
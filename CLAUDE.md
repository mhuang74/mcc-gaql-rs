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

```bash
# Build (workspace)
cargo build --workspace
cargo check --workspace

# Build single crates
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

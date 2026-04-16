# AGENTS.md - Agent Guide

## Build System

**Workspace structure**: 3 crates in `crates/`: `mcc-gaql` (~15-20 MB), `mcc-gaql-gen` (~400 MB), `mcc-gaql-common` (shared).

**Build commands**:
- `cargo build --workspace` - All crates
- `cargo build -p mcc-gaql --release` - Query tool only (fast)
- `cargo build -p mcc-gaql-gen --release` - Requires `MCC_GAQL_R2_PUBLIC_ID` env var
- `cargo check --workspace` - Quick validation without building

**Sequential tests required** (race conditions):
- `cargo test --workspace -- --test-threads=1`
- `cargo test -p mcc-gaql -- --test-threads=1`

**System requirement**: `protobuf-compiler` (for googleads-rs).

## Coding Conventions

- **Error handling**: `anyhow::Result` (app crates) → `thiserror` (lib/mcc-gaql-common)
- **Async**: Tokio with multi-thread runtime
- **Logging**: `log` crate macros (`log::info!`, `log::debug!`)
- **Config precedence**: CLI args > env vars > defaults
- **Shared code**: Place in `mcc-gaql-common`
- **gRPC**: `tonic` + `googleads-rs` generated types
- **Data frames**: `polars` in mcc-gaql only

## Build-Time Config

**Required build env for mcc-gaql-gen**: `MCC_GAQL_R2_PUBLIC_ID` (R2 public bucket ID for bootstrap downloads).

**Secrets NOT embedded**: `MCC_GAQL_DEV_TOKEN` and `MCC_GAQL_EMBED_CLIENT_SECRET` are runtime-only (environment, config file, or `clientsecret.json`).

**build.rs**: Captures `GIT_HASH` and `BUILD_TIME` for version banner.

## Architecture

**Entry points**: `crates/mcc-gaql/src/main.rs`, `crates/mcc-gaql-gen/src/main.rs`.

**Domain paths** (from `mcc-gaql-common/src/paths.rs`): Config dir (`~/Library/Application Support/mcc-gaql/` on macOS, `~/.config/mcc-gaql/` on Linux), Cache dir (`~/Library/Caches/mcc-gaql/` on macOS, `~/.cache/mcc-gaql/` on Linux).

## CI notes

Parallel builds used: CI profiles split core (mcc-gaql + common) and gen (mcc-gaql-gen) jobs. Gen builds only on gen file changes + manual + push to main.

## Quick verification

```bash
cargo check --workspace
cargo test --workspace -- --test-threads=1
cargo fmt --all
cargo clippy --workspace
```
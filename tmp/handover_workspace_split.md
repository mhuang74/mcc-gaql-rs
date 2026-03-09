# Handover: Workspace Split Task

## Summary

The monolithic `mcc-gaql` Rust crate has been split into a 3-crate Cargo workspace. Most of the work is complete but needs final cleanup and commit.

## Current State

### Completed
- Root `Cargo.toml` converted to workspace manifest with `resolver = "2"`
- Three crates created under `crates/`:
  - `mcc-gaql-common`: Shared types (`MyConfig`, `FieldMetadata`, `QueryEntry`, path utilities)
  - `mcc-gaql`: Lightweight core query tool (~15-20 MB, no LLM deps)
  - `mcc-gaql-gen`: GAQL generation tool with LLM/RAG (~400 MB)
- Both `mcc-gaql` and `mcc-gaql-gen` have `lib.rs` + `main.rs` structure for integration test support
- Old `src/` directory removed
- Old root `build.rs` removed (moved to `crates/mcc-gaql/build.rs`)
- Integration tests migrated to crate-specific `tests/` directories:
  - `crates/mcc-gaql/tests/config_tests.rs` — 18 tests passing
  - `crates/mcc-gaql-gen/tests/metadata_scraper_tests.rs` — 24 tests passing
  - `crates/mcc-gaql-gen/tests/metadata_scraper_live_tests.rs` — live tests (ignored by default)
  - `crates/mcc-gaql-gen/tests/field_vector_store_rag_tests.rs` — RAG tests
  - `crates/mcc-gaql-gen/tests/minimal_rag_test.rs` — minimal RAG test
- Old `tests/` directory removed
- `DEVELOPER.md` removed (superseded by `CLAUDE.md`)

### Pending
1. **Verify all workspace tests pass**: Run `cargo test --workspace` to confirm
2. **Commit and push changes**: Stage new files, commit with descriptive message, push to `reduce_crate_bloat` branch
3. **Update PR #48**: PR already exists, just needs the latest push

## Key Files Changed (Uncommitted)

```
Modified:
  crates/mcc-gaql-common/Cargo.toml  (added chrono serde feature)
  crates/mcc-gaql-gen/Cargo.toml     (added dev-dependencies)
  crates/mcc-gaql-gen/src/main.rs    (use lib modules instead of mod declarations)
  crates/mcc-gaql/src/main.rs        (use lib modules instead of mod declarations)

New:
  crates/mcc-gaql/src/lib.rs
  crates/mcc-gaql/tests/config_tests.rs
  crates/mcc-gaql-gen/src/lib.rs
  crates/mcc-gaql-gen/tests/metadata_scraper_tests.rs
  crates/mcc-gaql-gen/tests/metadata_scraper_live_tests.rs
  crates/mcc-gaql-gen/tests/field_vector_store_rag_tests.rs
  crates/mcc-gaql-gen/tests/minimal_rag_test.rs

Deleted:
  tests/                             (entire directory)
  DEVELOPER.md
```

## Verification Steps

```bash
# 1. Check workspace compiles
cargo check --workspace

# 2. Run all tests
cargo test --workspace

# 3. Build lightweight binary
cargo build -p mcc-gaql --release

# 4. Build LLM binary (takes longer)
cargo build -p mcc-gaql-gen --release
```

## Commit Instructions

```bash
# Stage all changes
git add -A

# Commit
git commit -m "$(cat <<'EOF'
Migrate integration tests to workspace crates

- Add lib.rs to mcc-gaql and mcc-gaql-gen for test accessibility
- Move config_tests.rs to crates/mcc-gaql/tests/
- Move scraper and RAG tests to crates/mcc-gaql-gen/tests/
- Remove old tests/ directory and DEVELOPER.md
- Fix chrono serde feature in mcc-gaql-common
EOF
)"

# Push
git push origin reduce_crate_bloat
```

## PR #48

URL: https://github.com/mhuang74/mcc-gaql-rs/pull/48

The PR title and description were already updated to reflect the workspace split. After pushing, the PR will automatically include the latest changes.

## Architecture Reference

```
mcc-gaql-rs/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── mcc-gaql-common/          # Shared types
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs         # MyConfig, QueryEntry
│   │       ├── field_metadata.rs # FieldMetadata types
│   │       └── paths.rs          # config_file_path, etc.
│   ├── mcc-gaql/                 # Core query tool
│   │   ├── Cargo.toml
│   │   ├── build.rs
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── main.rs
│   │   │   └── ...
│   │   └── tests/
│   │       └── config_tests.rs
│   └── mcc-gaql-gen/             # LLM/RAG tool
│       ├── Cargo.toml
│       ├── src/
│       │   ├── lib.rs
│       │   ├── main.rs
│       │   └── ...
│       └── tests/
│           ├── metadata_scraper_tests.rs
│           ├── metadata_scraper_live_tests.rs
│           ├── field_vector_store_rag_tests.rs
│           └── minimal_rag_test.rs
```

## Notes

- The RAG tests (`field_vector_store_rag_tests.rs`, `minimal_rag_test.rs`) had `#![cfg(feature = "llm")]` removed since the `llm` feature no longer exists — `mcc-gaql-gen` always includes LLM deps
- Import paths were updated: `mcc_gaql::metadata_scraper` → `mcc_gaql_gen::scraper`, `mcc_gaql::field_metadata` → `mcc_gaql_common::field_metadata`
- `chrono` in `mcc-gaql-common` needs `features = ["serde"]` for `DateTime<Utc>` serialization

# Address PR #69 Review Comments

## Context

PR #69 ("Upgrade googleads-rs v23 → v24") received two DiffGuard AI bot reviews (non-blocking COMMENTs, no inline comments). Both make the same core recommendation: the `"v24"` string literal is hardcoded in ~20+ locations, making future upgrades error-prone. The second review adds two supply-chain suggestions: pin `googleads-rs` git dep to a rev (not `branch = "main"`), and pin the CI GitHub Action to a commit SHA + bump `actions/checkout@v3` → `v4`. There is also a stale "V23" doc comment in `proto_locator.rs:9` that was missed in the upgrade.

The reviews are suggestions (COMMENT status, not CHANGES_REQUESTED). We address all three themes from both reviews where practical, and push back on one sub-item (test-fixture proto strings) with rationale.

## Approach

### Step 1: Create `crates/mcc-gaql-common/src/version.rs` — single source of truth

New file with:
```rust
/// Google Ads API version this workspace is built against.
/// Single source of truth — bump this (and googleads-rs) together.
pub const GOOGLEADS_API_VERSION: &str = "v24";

/// Default R2 object key for published RAG bundles.
pub const RAG_BUNDLE_KEY: &str = "mcc-gaql-rag-bundle-v24.tar.gz";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rag_bundle_key_contains_api_version() {
        assert!(RAG_BUNDLE_KEY.contains(GOOGLEADS_API_VERSION));
    }
}
```

Re-export from `lib.rs`: add `pub mod version;` and `pub use version::GOOGLEADS_API_VERSION;`.

### Step 2: Replace all production `"v24"` literals with the constant

Every file below already depends on `mcc-gaql-common`, so the import is available.

- **`mcc-gaql-common/src/field_metadata.rs:166`** — `api_version: "v24".to_string()` → `api_version: GOOGLEADS_API_VERSION.to_string()`. Add `use crate::version::GOOGLEADS_API_VERSION;`.
- **`mcc-gaql-common/src/paths.rs:73`** — `join("proto_docs_v24.json")` → `join(format!("proto_docs_{GOOGLEADS_API_VERSION}.json"))`. Add `use crate::version::GOOGLEADS_API_VERSION;`.
- **`mcc-gaql/src/field_metadata.rs:172`** — `api_version: "v24".to_string()` → `api_version: mcc_gaql_common::version::GOOGLEADS_API_VERSION.to_string()`.
- **`mcc-gaql-gen/src/proto_docs_cache.rs:345`** — `join("proto_docs_v24.json")` → `join(format!("proto_docs_{GOOGLEADS_API_VERSION}.json"))`.
- **`mcc-gaql-gen/src/proto_docs_cache.rs:385`** — `let api_version = "v24";` → `let api_version = mcc_gaql_common::version::GOOGLEADS_API_VERSION;`
- **`mcc-gaql-gen/src/proto_locator.rs:33`** — error message string: `v24/` → version-agnostic `{VERSION}/` (doc string, not code path).
- **`mcc-gaql-gen/src/proto_locator.rs:67`** — `join("proto/google/ads/googleads/v24")` → `join(format!("proto/google/ads/googleads/{GOOGLEADS_API_VERSION}"))`. Add `use mcc_gaql_common::version::GOOGLEADS_API_VERSION;`.
- **`mcc-gaql-gen/src/proto_locator.rs:9`** — fix stale doc comment "V23" → "the current API version" (version-agnostic).

### Step 3: CLI defaults from the constant (`mcc-gaql-gen/src/main.rs`)

- **Line 203** (Bootstrap version): `default_value = "v24"` → `default_value = mcc_gaql_common::GOOGLEADS_API_VERSION`
- **Line 222** (Publish key): `default_value = "mcc-gaql-rag-bundle-v24.tar.gz"` → `default_value = mcc_gaql_common::version::RAG_BUNDLE_KEY`
- **Line 236** (ParseProtos help text): `proto_docs_v24.json` → `proto_docs_<VERSION>.json` (doc comment, must be literal)

### Step 4: Derive `RESOURCES_FQN_PREFIX` from the constant (`mcc-gaql-mut/src/mutation_validate.rs`)

Replace:
```rust
const RESOURCES_FQN_PREFIX: &str = "google.ads.googleads.v24.resources";
```
with runtime construction at the single call site (line 42):
```rust
let resource_fqn = format!(
    "google.ads.googleads.{}.resources.{}",
    mcc_gaql_common::version::GOOGLEADS_API_VERSION,
    resource_type
);
```
Remove the `const` entirely. The `format!` was already there; only the prefix source changes. mcc-gaql-mut already depends on mcc-gaql-common.

### Step 5: Replace test `"v24"` literals with the constant

Test files that construct `FieldMetadataCache`, `ProtoDocsCache`, or `ScrapedDocs` with `"v24".to_string()`:
- `mcc-gaql-gen/src/proto_docs_cache.rs:480,491,557,570,665` — `ProtoDocsCache::new("v24"...)` → `ProtoDocsCache::new(mcc_gaql_common::version::GOOGLEADS_API_VERSION.to_string()...)`
- `mcc-gaql-gen/src/bundle.rs:711` — `api_version: "v24".to_string()` → constant
- `mcc-gaql-gen/src/scraper.rs:539,567` — `api_version: "v24".to_string()` → constant
- `mcc-gaql-gen/tests/field_vector_store_rag_tests.rs:219` — `api_version: "v24".to_string()` → constant
- `mcc-gaql-gen/tests/metadata_scraper_tests.rs:305,332,373,395,424,437,466,473,507,532,563,588,613,645,677,701,713,737` — all `"v24"` literals → `mcc_gaql_common::version::GOOGLEADS_API_VERSION` (for `.to_string()`) or `.to_string()` for string args. For assertions like `assert_eq!(loaded.api_version, "v24")` → `assert_eq!(loaded.api_version, mcc_gaql_common::version::GOOGLEADS_API_VERSION)`.

**NOT changed**: `proto_parser.rs` test fixture proto text (lines 763,772,1278,1311,1321,1350,1361,1383) — these are raw proto syntax inside `const` raw string literals used as parser test input. A Rust constant cannot be interpolated into a raw string literal. Templatizing these at runtime is over-engineering for test fixtures. Will note this in the review reply.

**NOT changed**: Doc comment examples mentioning `v24` (proto_docs_cache.rs:76,87; scraper.rs:48) — these are illustrative example strings in doc comments, not logic.

### Step 6: Pin `googleads-rs` to rev instead of branch

Replace `branch = "main"` with `rev = "3d36a5a840a7fa7c473bbed92a99c5d10b712dd9"` in all 4 Cargo.toml files:
- `crates/mcc-gaql-common/Cargo.toml:22`
- `crates/mcc-gaql-gen/Cargo.toml` (check — gen may not have direct googleads-rs dep; if not, skip)

Wait — checking: gen Cargo.toml does NOT list googleads-rs directly. Only common, mcc-gaql, and mcc-gaql-mut have it. So 3 files:
- `crates/mcc-gaql-common/Cargo.toml:22`
- `crates/mcc-gaql-mut/Cargo.toml:20`
- `crates/mcc-gaql/Cargo.toml:26`

### Step 7: Pin CI action to SHA + bump checkout

In `.github/workflows/code-review.yml`:
- Line 13: `actions/checkout@v3` → `actions/checkout@v4`
- Line 16: `jonit-dev/openrouter-github-action@v1.0.0` → `@88f63615c769f8db6031973503c2c40a9a3f4feb` with comment `# v1.0.0`

### Step 8: Reply to both DiffGuard review comments

Post a top-level PR comment summarizing all fixes, addressing each suggestion from both reviews. Since there are no inline review comments (only top-level issue comments), use `gh pr comment 69` to reply.

## Critical files & anchors

- `crates/mcc-gaql-common/src/version.rs` — new file, single source of truth for API version
- `crates/mcc-gaql-common/src/lib.rs:1-8` — add `pub mod version;` re-export
- `crates/mcc-gaql-mut/src/mutation_validate.rs:7,42` — replace const with runtime format!
- `crates/mcc-gaql-gen/src/main.rs:203,222,236` — clap defaults from constants
- `.github/workflows/code-review.yml:13,16` — SHA pin + checkout bump

## Verification

1. `cargo check --workspace` — compiles with all constant references
2. `cargo test -p mcc-gaql-common -- --test-threads=1` — version.rs guard test passes
3. `cargo test --workspace -- --test-threads=1` — all existing tests pass with constants replacing literals
4. `grep -rn '"v24"' crates/ --include='*.rs' | grep -v 'version.rs' | grep -v 'proto_parser.rs'` — returns only proto_parser.rs test fixtures (acceptable)
5. Commit, push, post PR comment with fix details

## Assumptions & contingencies

- `clap` 4.6.1 `default_value` accepts a `const &'static str` path — confirmed by review code example and clap 4 docs. If it doesn't compile, fall back to string literal with a comment pointing to `version.rs`.
- `RAG_BUNDLE_KEY` retains a `"v24"` literal inside it (unavoidable without `const_str` crate or build script). The guard test ensures it stays in sync with `GOOGLEADS_API_VERSION`. This is the same approach the second review suggested.
- Proto parser test fixtures keep literal `v24` — will explicitly note this as a deliberate decision in the PR reply.
# Address PR #69 Review Comments — API Version Constant Refactor

## Context

PR #69 ("Upgrade googleads-rs v23 → v24", branch `migrate_gads_v24`, 4 commits ahead of `main`) received two DiffGuard AI bot reviews. Both are **top-level issue comments** by `github-actions[bot]` (verified: `gh api .../pulls/69/comments` → length 0, so there are no inline review comments to resolve). Both make the same core recommendation: the `"v24"` string literal is hardcoded across the workspace, making future upgrades error-prone. Review 2 adds supply-chain items: pin `googleads-rs` git dep to a rev instead of `branch = "main"`, and pin the CI GitHub Action to a commit SHA + bump `actions/checkout@v3` → `v4`. There is also a stale "V23" doc comment at `proto_locator.rs:9` missed in the upgrade, and stale `proto_docs_v23.json` references in three docs files.

End state: `mcc-gaql-common` exports `GOOGLEADS_API_VERSION` (single source of truth) + `RAG_BUNDLE_KEY`; all production `"v24"` literals derive from the constant; test literals replaced; git dep pinned to rev; CI action SHA-pinned and checkout bumped; single commit pushed to the PR branch; one top-level PR comment addressing both reviews (including pushback on proto parser test fixtures).

The original spec (`specs/refactor_hardcoded_gads_api_version.md`) had two verified defects, corrected here:
1. **Step 5 under-enumerated `metadata_scraper_tests.rs`**: it listed 18 lines; the file has **26** `"v24"` tokens. It missed all 8 mock-server URL keys (`"/v24/campaign"`, `"/v24/ad_group"` at lines 433, 460, 502, 558, 583, 637, 639, 670). These keys are load-bearing: `scrape_resource` (scraper.rs:220) builds `{base_url}/{api_version}/{resource}` and the mock server does an exact path lookup — if the key diverges from the constant-derived URL, those tests silently start hitting 404s.
2. **Verification grep expectation was wrong**: the spec expected `grep '"v24"'` to return "only proto_parser.rs test fixtures" after the change. False — the proto fixtures contain `google.ads.googleads.v24.resources` with dots, not quote-adjacent `"v24"`, so they never match that pattern; and doc-comment examples likewise don't match. After this plan, the quoted-literal grep returns exactly one match: the constant in `version.rs` itself.

## Approach

### Step 1 — Create `crates/mcc-gaql-common/src/version.rs`

New file, exact content:

```rust
/// Google Ads API version this workspace is built against.
/// Single source of truth — bump this (and googleads-rs) together.
pub const GOOGLEADS_API_VERSION: &str = "v24";

/// Default R2 object key for published RAG bundles.
/// Contains a literal version segment, guarded by the test below.
pub const RAG_BUNDLE_KEY: &str = "mcc-gaql-rag-bundle-v24.tar.gz";

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards against RAG_BUNDLE_KEY drifting from the version constant.
    #[test]
    fn rag_bundle_key_contains_api_version() {
        assert!(RAG_BUNDLE_KEY.contains(GOOGLEADS_API_VERSION));
    }
}
```

Register in `crates/mcc-gaql-common/src/lib.rs` (currently 8 `pub mod` lines in alpha order: auth, config, field_metadata, googleads_api, http_client, paths, query, util). Append after `pub mod util;`:
```rust
pub mod version;
pub use version::{GOOGLEADS_API_VERSION, RAG_BUNDLE_KEY};
```
Downstream crates then use `mcc_gaql_common::GOOGLEADS_API_VERSION` (re-export) or `use` the name locally. Every crate touched below already depends on `mcc-gaql-common` (verified in each Cargo.toml).

### Step 2 — Replace production `"v24"` literals (5 files, 12 sites)

Re-read each file's cited region before editing (line numbers from current tree).

1. **`crates/mcc-gaql-common/src/field_metadata.rs:166`** — in `FieldMetadataCache::new()`: `api_version: "v24".to_string(),` → `api_version: GOOGLEADS_API_VERSION.to_string(),`. Add `use crate::version::GOOGLEADS_API_VERSION;` to the import block (lines 6–11).
2. **`crates/mcc-gaql-common/src/paths.rs:73`** — `Ok(cache_dir()?.join("proto_docs_v24.json"))` → `Ok(cache_dir()?.join(format!("proto_docs_{GOOGLEADS_API_VERSION}.json")))`. Add `use crate::version::GOOGLEADS_API_VERSION;`. (`Path::join` takes `AsRef<Path>`; `String` implements it — compiles.)
3. **`crates/mcc-gaql/src/field_metadata.rs:172`** — `api_version: "v24".to_string(),` → `api_version: mcc_gaql_common::GOOGLEADS_API_VERSION.to_string(),` (fully qualified; no import needed).
4. **`crates/mcc-gaql-gen/src/proto_docs_cache.rs:345`** — `get_cache_path()`: `Ok(cache_dir.join("proto_docs_v24.json"))` → `Ok(cache_dir.join(format!("proto_docs_{GOOGLEADS_API_VERSION}.json")))`. Add top-of-file `use mcc_gaql_common::GOOGLEADS_API_VERSION;` (this file is a lib module; the import also brings the name into `#[cfg(test)] mod tests` via its existing `use super::*`).
   - Invariant: gen's `get_cache_path()` (callers: `main.rs:692,823,1429` + internal `load_or_build_cache:363`) and common's `paths::proto_docs_path()` (caller: `mcc-gaql/src/config.rs:417`, display-only) must build the **identical** filename `proto_docs_{GOOGLEADS_API_VERSION}.json` under `mcc_gaql_common::paths::cache_dir()`. Do not make one derive from the other; just keep both template strings identical.
5. **`crates/mcc-gaql-gen/src/proto_docs_cache.rs:385`** — `let api_version = "v24";` → `let api_version = GOOGLEADS_API_VERSION;` (name in scope from the Step-2.4 import).
6. **`crates/mcc-gaql-gen/src/proto_locator.rs:67`** — `let proto_path = subdir.path().join("proto/google/ads/googleads/v24");` → `let proto_path = subdir.path().join(format!("proto/google/ads/googleads/{GOOGLEADS_API_VERSION}"));`. Add top-of-file `use mcc_gaql_common::GOOGLEADS_API_VERSION;`.
7. **`crates/mcc-gaql-gen/src/proto_locator.rs:29-34`** — the `anyhow::bail!` error message currently ends `.../proto/google/ads/googleads/v24/"` as a plain string literal. It is NOT inline-interpolated today (the original spec's "version-agnostic `{VERSION}/`" would print that text literally — defect). Replace with positional interpolation:
   ```rust
   anyhow::bail!(
       "Could not locate googleads-rs proto files. \n\
        Either set GOOGLEADS_PROTO_DIR environment variable, or ensure \n\
        googleads-rs dependency is fetched. Proto files should be in: \n\
        $CARGO_HOME/git/checkouts/googleads-rs-*/proto/google/ads/googleads/{}/",
       GOOGLEADS_API_VERSION
   )
   ```
   Uses positional `{}` + the imported constant (Step-2.6 import) — do NOT combine an inline `{GOOGLEADS_API_VERSION}` capture with a trailing argument (unused-arg warning).
8. **`crates/mcc-gaql-gen/src/proto_locator.rs:9`** — doc comment `/// Locates the googleads-rs proto directory containing V23 proto files.` → `/// Locates the googleads-rs proto directory containing the current API version's proto files.`
9. **`crates/mcc-gaql-gen/src/main.rs:203`** — Bootstrap `version` arg: `#[arg(long, default_value = "v24")]` → `#[arg(long, default_value = mcc_gaql_common::GOOGLEADS_API_VERSION)]`. clap 4.6.1 (Cargo.lock) `default_value` accepts a `&'static str` const expression. Contingency: if it does not compile, revert to the literal with comment `// keep in sync with mcc-gaql-common/src/version.rs`.
10. **`crates/mcc-gaql-gen/src/main.rs:222`** — Publish `key` arg: `#[arg(long, default_value = "mcc-gaql-rag-bundle-v24.tar.gz")]` → `#[arg(long, default_value = mcc_gaql_common::RAG_BUNDLE_KEY)]`.
11. **`crates/mcc-gaql-gen/src/main.rs:236`** — ParseProtos help/doc text: `/// Path to proto docs cache output. Defaults to ~/.cache/mcc-gaql/proto_docs_v24.json` → `/// Path to proto docs cache output. Defaults to ~/.cache/mcc-gaql/proto_docs_<VERSION>.json` (clap help text is a plain literal; `<VERSION>` placeholder keeps it version-agnostic).
12. **`crates/mcc-gaql-mut/src/mutation_validate.rs`** — delete line 7 (`const RESOURCES_FQN_PREFIX: &str = "google.ads.googleads.v24.resources";`; verified only production usage is line 42, tests do not reference it) and change line 42 to:
    ```rust
    let resource_fqn = format!(
        "google.ads.googleads.{}.resources.{}",
        mcc_gaql_common::GOOGLEADS_API_VERSION,
        resource_type
    );
    ```

**Deliberately unchanged** (note in PR reply): `proto_parser.rs` test fixtures (lines 763, 772, 1278, 1311, 1321, 1350, 1361, 1383 — raw proto source inside `const` raw strings in `#[cfg(test)] mod tests` at line 756; a Rust const cannot be interpolated into a raw string, and runtime templatization of parser fixtures is over-engineering); illustrative doc comments `proto_docs_cache.rs:76,87` and `scraper.rs:48` (example type names, not logic); the comment `mcc-gaql/src/field_metadata.rs:106` ("Google Ads API v24 GoogleAdsFieldDataType enum:" — describes the version-locked enum mapping, stays).

### Step 3 — Replace test `"v24"` literals (5 files)

1. **`crates/mcc-gaql-gen/src/proto_docs_cache.rs`** tests at lines 480, 491, 557, 570, 665: `ProtoDocsCache::new("v24".to_string(), ...)` → `ProtoDocsCache::new(GOOGLEADS_API_VERSION.to_string(), ...)`. Name already in scope via Step-2.4 import + test mod's `use super::*`.
2. **`crates/mcc-gaql-gen/src/bundle.rs:711`** (test `test_manifest_serialization`): `api_version: "v24".to_string()` → `api_version: mcc_gaql_common::GOOGLEADS_API_VERSION.to_string()` (fully qualified; only tests in this file need it, so no top-level import).
3. **`crates/mcc-gaql-gen/src/scraper.rs:539, 567`** (tests `test_scraped_docs_get_description`, `test_scraped_docs_get_enum_values`): `api_version: "v24".to_string()` → `api_version: mcc_gaql_common::GOOGLEADS_API_VERSION.to_string()`.
4. **`crates/mcc-gaql-gen/tests/field_vector_store_rag_tests.rs:219`**: `api_version: "v24".to_string()` → `api_version: mcc_gaql_common::GOOGLEADS_API_VERSION.to_string()`. (Integration tests can import regular deps — this file already imports `mcc_gaql_common::field_metadata` at line 7.)
5. **`crates/mcc-gaql-gen/tests/metadata_scraper_tests.rs`** — **26 tokens** (the original spec listed 18 and missed 8). Add `use mcc_gaql_common::GOOGLEADS_API_VERSION;` to the import block (after line 14). Then replace every remaining `"v24"` token by its shape — re-read lines 290–755 before editing; exhaustive verified line list:
   - **Struct literals** (305, 373, 424, 701, 713, 737): `api_version: "v24".to_string()` → `api_version: GOOGLEADS_API_VERSION.to_string()`
   - **Assertions** (332, 473): `assert_eq!(x.api_version, "v24")` → `assert_eq!(x.api_version, GOOGLEADS_API_VERSION)`
   - **Scrape-call version arg** (395 via `scrape_to_path`, and 437, 466, 507, 532, 563, 588, 613, 645, 677 via `scrape_all_with_base_url`): the `"v24"` argument → `GOOGLEADS_API_VERSION` (parameter is `&str`; the const is `&'static str` — passes directly)
   - **Mock-server URL keys** (433, 460, 502, 558, 583, 637, 670 = `"/v24/campaign"`, 639 = `"/v24/ad_group"`): `responses.insert("/v24/campaign".to_string(), (200, campaign_html()))` → `responses.insert(format!("/{GOOGLEADS_API_VERSION}/campaign"), (200, campaign_html()))`. The map is `HashMap<String, (u16, String)>`; `format!` yields `String` — compiles. The key must equal what `scrape_resource` requests (`{base}/{api_version}/{resource}` — scraper.rs:220); these tests are the behavioral proof the URL derivation stays in sync.

### Step 4 — Pin `googleads-rs` to rev (3 Cargo.toml files)

Replace `googleads-rs = { git = "https://github.com/mhuang74/googleads-rs", branch = "main" }` with `googleads-rs = { git = "https://github.com/mhuang74/googleads-rs", rev = "3d36a5a840a7fa7c473bbed92a99c5d10b712dd9" }` in exactly:
- `crates/mcc-gaql-common/Cargo.toml:22`
- `crates/mcc-gaql-mut/Cargo.toml:20`
- `crates/mcc-gaql/Cargo.toml:26`

(`crates/mcc-gaql-gen/Cargo.toml` does not declare googleads-rs — verified; the original spec's "all 4 files" was wrong, it self-corrected to 3.)

The rev equals the commit already pinned in `Cargo.lock:3277` (`git+...#3d36a5a840a7fa7c473bbed92a99c5d10b712dd9`, version 24.2.0) — verified via GitHub API that the commit exists and that tag `v24.2.0` resolves to the same commit (annotated tag object `8062873...` → commit `3d36a5a...`). Rev chosen over tag per review-2's primary wording; either encodes the same commit. Expected outcome: `Cargo.lock` diff is **empty** (same commit, only the manifest's tracking spec changes). Contingency: if `cargo check` re-resolves and the lock diff shows a *different* googleads-rs commit, the rev was mistyped — abort and re-verify against `Cargo.lock:3277`.

### Step 5 — CI: SHA pin + checkout bumps

In `.github/workflows/code-review.yml`:
- Line 13: `- uses: actions/checkout@v3` → `- uses: actions/checkout@v4`
- Line 16: `uses: jonit-dev/openrouter-github-action@v1.0.0` → add comment line `# v1.0.0` above (Dependabot/ratchet discoverability), and `uses: jonit-dev/openrouter-github-action@88f63615c769f8db6031973503c2c40a9a3f4feb`. SHA verified via GitHub API as the commit tag `v1.0.0` points to.

Also bump the three remaining `actions/checkout@v3` uses in `.github/workflows/rust.yml` (lines 25, 39, 68) → `@v4` — same defect class; bumping one workflow and leaving three stale pins is half-done.

### Step 6 — Docs sweep for stale `v23` (same defect class; 3 files)

- `DEVELOPER.md:246` — both `proto_docs_v23.json` → `proto_docs_v24.json`
- `METADATA_MAINTENANCE.md:52, 64, 229, 230, 233, 273` — `v23` → `v24` (line 64: proto checkout path `.../googleads/v23/`; lines 229/230/233: `proto_docs_v23.json` shell examples; lines 52, 273: `proto_docs_v23.json`)
- `CLAUDE.md:55` — both columns `proto_docs_v23.json` → `proto_docs_v24.json`

### Step 7 — Commit and push

Single commit on `migrate_gads_v24` containing only the files from Steps 1–6: message `refactor: derive Google Ads API version from single constant (PR #69 review)`.
Do NOT include: staged-but-unrelated `specs/github_codereview_opencode_ollama.md`, untracked `specs/refactor_hardcoded_gads_api_version.md`, or the untracked dotfiles (`.bashrc` etc.) — they are the user's in-progress work. `git add` by explicit path list, then `git push`.

### Step 8 — Reply to both DiffGuard reviews

One top-level PR comment (no inline review comments exist to reply to): `gh pr comment 69 --body "..."`. Body must address, per review:
- **Both reviews' #1 (centralize version)**: new `version.rs` constant + re-export + guard test; all production sites (paths, cache builder, proto locator, CLI defaults, FQN prefix) now derive from it; test literals replaced.
- **Review 1 #2 (CLI defaults from constant)**: done — clap `default_value` accepts the const.
- **Review 1 #3 (proto package helper const)**: partially declined — added neither `GOOGLEADS_PROTO_VERSION_PREFIX` nor `proto_package()`: there is exactly one production FQN site (`mutation_validate.rs:42`), now built inline from the constant; a second exported constant for one call site adds surface without removing risk.
- **Review 1's proto-parser-fixture suggestion & review 2's "tests should use the constant"**: fixtures keep literal `v24` — raw-string `const` test input cannot interpolate a Rust const; runtime templatization of parser fixtures is over-engineering. All other test literals converted.
- **Review 2 #2 (pin git dep)**: done — rev pin matching the locked commit; tag `v24.2.0` exists upstream and points at the same commit, noted as alternative.
- **Review 2 #3 (pin action to SHA)**: done — SHA pin with `# v1.0.0` comment; checkout bumped to v4 in both workflows.
- Docs stale-`v23` sweep noted.

## Critical files & anchors

- `crates/mcc-gaql-common/src/version.rs` — new file; constant, RAG key, guard test
- `crates/mcc-gaql-common/src/lib.rs:8` — append `pub mod version;` + `pub use` re-exports
- `crates/mcc-gaql-gen/tests/metadata_scraper_tests.rs` — 26 tokens incl. 8 mock URL keys the original spec missed; URL keys must match scraper's `{base}/{api_version}/{resource}` construction
- `crates/mcc-gaql-gen/src/proto_locator.rs:9,29-34,67` — doc fix, bail! interpolation, path join
- `crates/mcc-gaql-mut/src/mutation_validate.rs:7,42` — const deletion, inline `format!` FQN
- `.github/workflows/code-review.yml:13,16` + `rust.yml:25,39,68` — SHA pin, checkout v4

## Verification

Prerequisites: `MCC_GAQL_R2_PUBLIC_ID` set (compile-time `env!` in `crates/mcc-gaql-gen/src/r2.rs:13`; currently set in this environment); `protobuf-compiler` present (verified: protoc 3.21.12). Run from repo root.

1. `cargo check --workspace` — all constant references compile.
2. `cargo test -p mcc-gaql-common -- --test-threads=1` — includes new guard test `rag_bundle_key_contains_api_version`.
3. `cargo test -p mcc-gaql-gen -- --test-threads=1` — scraper mock-server tests prove URL-key derivation (a malformed key would 404 and fail assertions); cache/bundle tests prove constant plumbing. Note: fastembed-based tests download models on first run — slow, expected, not a hang.
4. `cargo test -p mcc-gaql-mut -- --test-threads=1` — mutation validation exercises the new `format!` FQN against the real descriptor pool (a wrong FQN fails resource lookups here).
5. New-behavior observable check (CLI defaults still render): `cargo run -p mcc-gaql-gen -- bootstrap --help` → output contains `[default: v24]`; `cargo run -p mcc-gaql-gen -- publish --help` → contains `[default: mcc-gaql-rag-bundle-v24.tar.gz]`.
6. `cargo test --workspace -- --test-threads=1` — full suite (AGENTS.md: sequential required).
7. Literal sweep: `grep -rn '"v24"' crates/ --include='*.rs'` → **exactly one match**: the constant line in `crates/mcc-gaql-common/src/version.rs`. Broader `grep -rn 'v24' crates/ --include='*.rs'` → only: `version.rs`, `proto_parser.rs` fixture lines, `proto_docs_cache.rs:76,87` + `scraper.rs:48` doc examples, `mcc-gaql/src/field_metadata.rs:106` comment. Anything else is a miss.
8. `git diff --stat` before committing → only files from Steps 1–6; `git diff Cargo.lock` → empty.
9. Push, then `gh pr comment 69` (Step 8 body). Confirm with `gh pr view 69 --comments`.

## Assumptions & contingencies

- **clap `default_value` with a const path expression** — expected to compile (attribute takes a Rust expression; `&'static str` implements clap's `IntoResettable<Str>`). Fallback if not: keep string literal + `// keep in sync with mcc-gaql-common/src/version.rs` comment.
- **`RAG_BUNDLE_KEY` retains an embedded literal version segment** — unavoidable without `const_str`/build script; guard test keeps it in sync. Matches review 2's own suggestion.
- **Proto parser fixtures keep literal `v24`** — deliberate pushback; explained in the PR reply.
- **Branch/PR state**: work lands on `migrate_gads_v24` (PR #69 open, base `main`). The user's other in-flight files stay out of the commit (Step 7).
- **Rev vs tag pin**: rev chosen (review 2's primary wording); tag `v24.2.0` verified to point to the identical commit, so the choice is not load-bearing.
- **CI grep guard for stale literals** (review 2's optional suggestion): **not added** — repo has no precedent for custom lint steps in CI, and a naive grep would false-positive on doc examples; the version.rs guard test covers the one real drift risk (`RAG_BUNDLE_KEY`). If the user wants it later, add to `rust.yml` excluding `version.rs` and `proto_parser.rs`.
- **If mock-server scrape tests fail after Step 3**: the URL key no longer matches `{base}/{api_version}/{resource}` — re-check the `format!` key against `scraper.rs:220`.
- **If `Cargo.lock` churns beyond nothing in Step 4**: rev mistyped — abort, re-verify against `Cargo.lock:3277`.
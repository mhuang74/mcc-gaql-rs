# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

## [0.20.0] - 2026-09-02

### Features
- Upgrade googleads-rs to v25.1.0 (Google Ads API v25, rev 81c005f02)
- Bump workspace version to 0.20.0

## [0.19.1] - 2026-08-29

### Features
- Extract mutation code into new `mcc-gaql-mut` binary crate (#68)
- Upgrade googleads-rs v23 → v24
- Derive Google Ads API version from single constant (`GOOGLEADS_API_VERSION` in mcc-gaql-common)
- Add googleads-rs auto-upgrade workflow (weekly detect→migrate→validate→PR with tracking issue)
- Code-review workflow updates

*Note: v0.18.x and v0.19.0 were never tagged.*

## [0.17.4] - 2026-04-17

### Features
- Add `--generate-prompt-only` and `--resource` flags on generate (#66; first attempt reverted, re-landed with review fixes)
- RAG field search in Phase 1 resource selection
- Customer_id normalization for `mcc-gaql-gen --validate`
- Add AGENTS.md

## [0.17.3] - 2026-04-14

### Features
- Metadata: similarity score display in semantic search, resource descriptions in pattern search, selectable segments/metrics display
- RAG: cosine distance → similarity conversion, similarity threshold raised to 0.65, semantic search threshold filter

## [0.17.2] - 2026-04-12

### Features
- Metadata: display selectable fields
- RAG segment categories in Phase 1 descriptions
- Clippy fixes

## [0.17.1] - 2026-04-12

### Features
- Enrich: `--all` flag, default to only-missing resources, print missing list, preserve existing enriched data with timestamped backups
- Remove build-time credential embedding + enhanced `--show-config`
- Docs (user/maintainer metadata split, README platforms/sizes, `--validate` fix)

## [0.17.0] - 2026-04-11

### Features
- Migrate to googleads-rs v23.2.0 (#61)
- ARM64 Linux (Graviton) release builds

## [0.16.5] - 2026-04-08

### Fixes
- Release CI: Linux x86_64-musl target added then switched to glibc builds
- OpenSSL/protoc/musl toolchain fixes
- Pin ubuntu-22.04 for glibc compatibility

## [0.16.4] - 2026-04-07

### Features
- RAG resource pre-filtering for MultiStepRAGAgent (#55)
- `--validate` on mcc-gaql and mcc-gaql-gen generate
- Query cookbook expansion (~80 GAQL examples with validation fixes, ROAS entries)
- Gen-test harness (Markdown output, random selection, dynamic discovery)
- Identity fields (display, enrichment backfill, `backfill-identity-fields` subcommand, `--force`)
- Query-gen quality fixes (domain knowledge, candidate-injection prevention, IN-clause quoting, monetary threshold validation, location_view/campaign_asset guidance)

### Fixes
- Deprecate `scrape`
- Bootstrap download fix (#58)

## [0.16.2] - 2026-03-21

### Features
- Multi-step RAG pipeline for GAQL generation (#52)
- Add `--explain-selection-process` flag for RAG transparency
- Add `--use-query-cookbook` flag for optional RAG cookbook examples
- Add keyword-based field matching to supplement vector search
- Add concurrency to key field selection and resource description
- Add `mcc-gaql-gen metadata` subcommand for RAG debugging
- Add timing instrumentation to RAG pipeline
- Add debug logging for LLM responses
- Add model parameters logging and full prompt trace dumps
- Inject today's date and temporal examples into LLM prompts
- Improve resource selection prompt formatting
- Increase key field selection ranges for LLM
- Single-resource enrichment with retry backoff
- Clean GAQL output for generate command
- Print version banner on startup (with GIT_HASH and BUILD_TIME)
- Improve date selection; print resource descriptions in Explanation

### Fixes
- Fix nested proto message parsing (repeated/multiline fields, inline messages)
- Fix duplicate fields from nested proto messages
- Fix pre-filtering in vector search for better field selection
- Fix keyword search to use full word matching, not substring
- Fix `--batch-size` CLI argument wiring to MetadataEnricher
- Fix numeric filter values in LLM field selection response
- Fix tokio::join! type annotations for CI build
- Fix rig-core dependency (0.32.0 -> 0.33.0) for compatibility
- Fix use total_concurrency for buffer_unordered limit
- Fix populate key_metrics for views using selectable_with
- Fix DURING operator support and GaqlBuilder pattern
- Fix LanceDB deprecation warning and isolate test cache
- Fix tests deleting production cache hashes
- Standardize R2 env vars with MCC_GAQL_ prefix
- Remove 15-field truncation from LLM prompt
- Revert --trace flag; use MCC_GAQL_LOG_LEVEL=trace instead

*See git history for changes prior to v0.16.2.*
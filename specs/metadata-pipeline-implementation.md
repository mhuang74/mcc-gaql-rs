# Metadata Pipeline — Implementation Plan

**Version:** 1.0
**Date:** 2026-03-11
**Status:** Completed
**Branch:** `claude/google-ads-metadata-plan-MY2YN`

---

## Goal

Implement a three-stage pipeline that enriches the Google Ads field metadata cache with LLM-generated descriptions, improving the quality of semantic search and natural language GAQL generation.

See [metadata-pipeline-design.md](metadata-pipeline-design.md) for the full design specification.

---

## Phases

### Phase 0 — Structural Metadata Harvest

**Objective:** Extend the existing `field_metadata.rs` module to capture richer structural data from the Fields Service API and expose new CLI commands for schema exploration.

**Tasks:**

- [x] Add `selectable_with`, `enum_values`, and `attribute_resources` fields to `FieldMetadata`
- [x] Add `ResourceMetadata` struct to capture resource hierarchy information
- [x] Add `resource_metadata` to `FieldMetadataCache`
- [x] Implement `FieldMetadata::build_embedding_text()` — rich text for LanceDB embeddings
- [x] Implement `FieldMetadataCache::show_resources()` — resource hierarchy table
- [x] Implement `FieldMetadataCache::enriched_field_count()` — count enriched fields
- [x] Add `--show-resources` CLI flag
- [x] Update `fetch_from_api()` to populate new structural fields

**Acceptance criteria:**
- `--show-resources` prints resource names, field counts, and key attributes
- `--show-fields <resource>` prints structural metadata including enum values
- Cache JSON round-trips cleanly with new optional fields

---

### Phase 1 — Web Scraping

**Objective:** Implement `metadata_scraper.rs` to extract plain-text field descriptions and enum documentation from the Google Ads API reference pages.

**Tasks:**

- [x] Create `src/metadata_scraper.rs`
- [x] Define `ScrapedFieldDoc` and `ScrapedDocs` structs
- [x] Implement `scrape_all_with_base_url()` with configurable base URL for testability
- [x] Implement `scrape_resource()` with HTTP client, response validation, and graceful degradation
- [x] Implement `parse_field_docs()` — HTML parsing for field IDs, descriptions, and enum values
- [x] Implement `extract_field_id_from_heading()` — qualified/unqualified field name extraction
- [x] Implement `extract_description_text()` — HTML stripping and sentence-boundary truncation
- [x] Implement `extract_enum_values()` — UPPER_SNAKE_CASE detection in table/code cells
- [x] Implement `load_or_scrape()` — disk cache with TTL
- [x] Add `get_scraped_docs_cache_path()` helper
- [x] Register module in `src/lib.rs`

**Acceptance criteria:**
- Scraper handles HTTP errors and JS-rendered pages without panicking
- Rate limiting (500 ms delay) is respected between resource pages
- Cache is written to `~/.cache/mcc-gaql/scraped_docs.json` and reused within TTL
- Unit tests cover HTML parsing, enum extraction, and cache round-trips

---

### Phase 2 — LLM Enrichment

**Objective:** Implement `metadata_enricher.rs` to use an LLM to generate contextual descriptions for every field in the cache.

**Tasks:**

- [x] Create `src/metadata_enricher.rs`
- [x] Define `MetadataEnricher` struct with configurable batch size
- [x] Implement `enrich()` — iterate resources and dispatch batches
- [x] Implement `enrich_batch()` — build prompt, call LLM, write results back to cache
- [x] Implement `build_batch_prompt()` — combine structural metadata and scraped docs into prompt
- [x] Implement `enrich_resource()` — generate resource-level descriptions
- [x] Implement `parse_enrichment_response()` — handle object and string JSON formats
- [x] Implement `strip_json_fences()` — handle LLM markdown-wrapped output
- [x] Implement `run_enrichment_pipeline()` — top-level two-stage runner (scrape + LLM)
- [x] Register module in `src/lib.rs`

**Acceptance criteria:**
- LLM failures for individual batches are logged and skipped without aborting the pipeline
- JSON responses in both `{"field": {"description": "..."}}` and `{"field": "..."}` formats are handled
- Markdown code fences are stripped before JSON parsing
- Unit tests cover response parsing and fence stripping

---

### Phase 3 — CLI Integration

**Objective:** Wire the pipeline stages into `main.rs` and expose them as CLI flags.

**Tasks:**

- [x] Add `--refresh-metadata` flag to `args.rs`
- [x] Add `--show-resources` flag to `args.rs`
- [x] Implement `--refresh-metadata` handler in `main.rs`:
  - Stage 0: `FieldMetadataCache::fetch_from_api()`
  - Stages 1–2: `metadata_enricher::run_enrichment_pipeline()`
  - Save enriched cache to both `field_metadata_enriched.json` and `field_metadata.json`
  - Clear LanceDB vector cache to force rebuild on next natural language query
- [x] Implement `--show-resources` handler in `main.rs`
- [x] Integrate field metadata cache into `--natural-language` path:
  - Load cache if present
  - Call `convert_to_gaql_enhanced()` when cache is available
  - Fall back to `convert_to_gaql()` when cache is absent
- [x] Validate LLM environment variables before starting `--refresh-metadata`
- [x] Feature-gate all LLM-dependent code with `#[cfg(feature = "llm")]`

**Acceptance criteria:**
- `--refresh-metadata` runs end-to-end and saves enriched cache
- `--natural-language` uses enriched descriptions when cache is present
- All new flags appear in `--help` with accurate descriptions
- Build succeeds with `--no-default-features` (no LLM)

---

### Phase 4 — Testing

**Objective:** Add integration tests for the scraping stage using a mock HTTP server.

**Tasks:**

- [x] Add `mockito` and `tokio` dev-dependencies to `Cargo.toml`
- [x] Create `tests/metadata_scraper_tests.rs` with mock server tests:
  - Test scraping a resource page with realistic HTML
  - Test graceful handling of 404 responses
  - Test graceful handling of JS-rendered (too-short) responses
  - Test cache load-or-scrape with cache hit (no server call)
  - Test cache load-or-scrape with stale cache (triggers re-scrape)
- [x] Create `tests/metadata_scraper_live_tests.rs` with ignored live tests:
  - Scrape `campaign` resource from real Google Ads docs
  - Scrape a set of resources and verify field count
  - All tests marked `#[ignore]` so CI doesn't hit the network
- [x] Verify all 24 unit tests pass: `cargo test --no-default-features`

**Acceptance criteria:**
- `cargo test --no-default-features` passes with 24/24 tests
- Mock server tests do not require network access
- Live tests are excluded from default test run

---

## Dependency Changes

| Crate | Change | Purpose |
|-------|--------|---------|
| `mockito` | Added (dev) | Mock HTTP server for scraper integration tests |
| `reqwest` | Already present | HTTP client for scraping |
| `chrono` | Already present | Cache timestamps |
| `serde_json` | Already present | JSON serialization |

---

## Outstanding Items / Future Work

1. **Incremental enrichment:** Currently `--refresh-metadata` re-enriches all fields. A future optimization would skip fields that already have descriptions and only process new or changed fields.

2. **Multiple API versions:** The scraper uses a fixed API version from the cache. Future work could support side-by-side caches for multiple API versions.

3. **Richer HTML parsing:** The current HTML extraction is line-by-line. A proper HTML parser (e.g. `scraper` crate) would handle edge cases more robustly, at the cost of an additional dependency.

4. **Validation:** A future `--validate <gaql>` command could use the field metadata cache to check field selectability and compatibility before execution.

5. **lance crate fix:** The `llm` feature is still blocked by the `lance` v1.0.1 recursion bug on Rust 1.94+. Full `cargo test` (with LLM features) requires either a `lance` fix or `rig-lancedb` update to support `lancedb 0.26+`.

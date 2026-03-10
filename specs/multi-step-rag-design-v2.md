# Multi-Step RAG Pipeline — Revised Design

**Version:** 2.0
**Date:** 2026-03-10
**Status:** DRAFT - For Implementation
**Related:** `gaql-metadata-for-llm-design.md`, `rag-quality-improvement-plan.md`

---

## Context

The current `EnhancedRAGAgent` in `crates/mcc-gaql-gen/src/rag.rs` produces GAQL queries with limited accuracy due to:
- No validation of field compatibility with primary resource
- Keyword-based resource selection (no semantic understanding)
- Generic field retrieval not filtered by resource context
- No handling of mutually exclusive fields
- Single-shot LLM prompt with all context combined

## Design Decisions (from interview)

| Area | Decision | Rationale |
|------|----------|-----------|
| Field scope (Phase 2) | **Tiered: resource-specific + RAG top-K** | Resource key_fields first, then RAG supplement, all filtered by `selectable_with` |
| Embedding text | **Semantic-only** (included in this work) | Remove structural flags from `build_embedding_text()`. Keep description, usage_notes, enum_values, resource context. Cache auto-rebuilds via hash invalidation |
| Filter logic | **LLM with validation** | Phase 3 LLM produces full filter criteria; Phase 4 validates enum values against `FieldMetadata.enum_values` |
| Phase 1 input | **Name + description only** | ~100 resources, no selectable_with lists in prompt |
| Enum context | **Pre-scan for likely filter fields** | Lightweight keyword scan before Phase 3 identifies fields likely to be filtered; include their `enum_values` in Phase 3 prompt |
| RAG search | **Per-category searches** | 3 separate vector searches (top-10 metrics, top-10 attributes, top-5 segments) to guarantee category representation |
| Related resource validation | **Drop invalid + warn in trace** | Validate Phase 1's related_resources against `selectable_with`; drop incompatible; log in `PipelineTrace` |
| Implicit filters | **None** | Only filter on what user explicitly mentions. No implicit `status = 'ENABLED'` |

---

## Revised 5-Phase Pipeline

### Phase 1: Resource Selection (LLM, no RAG)
- Input: user query + all ~100 resources (name + description only)
- Output: `ResourceSelectionResult { primary_resource, related_resources, confidence, reasoning }`
- Post-validation: check `related_resources` against primary's `selectable_with` from `FieldMetadataCache`; drop incompatible; record dropped in `PipelineTrace`
- Fallback: keyword-based detection if LLM fails

### Phase 2: Field Candidate Retrieval (RAG + FieldMetadataCache filter)
- **Tiered retrieval:**
  1. Get resource's `key_attributes` and `key_metrics` from `ResourceMetadata` (guaranteed relevant)
  2. Three per-category RAG searches: top-10 attributes, top-10 metrics, top-5 segments
  3. For related resources: get their `key_attributes` too
- **Compatibility filter:** All candidates filtered by `selectable_with` from `FieldMetadataCache` (NOT from LanceDB)
- **Dedup:** Merge tiers, deduplicate by field name
- Output: `FieldCandidates { attributes, metrics, segments, compatible_count, rejected_count }`

### Phase 2.5: Filter Pre-scan (local, ~5ms)
- Keyword scan of user query to identify likely filter fields
- For each likely filter field found in candidates, attach `enum_values` from `FieldMetadataCache`
- Output: `Vec<(field_name, Vec<enum_value>)>` passed into Phase 3 prompt

### Phase 3: Field Selection & Ranking (LLM)
- Input: user query + field candidates (with enum_values for likely filter fields) + cookbook examples
- LLM selects: `select_fields`, `filter_fields` (with operators, values), `order_by_fields`
- Post-processing:
  - Validate selected fields exist and are selectable
  - Handle mutual exclusivity via `selectable_with` pairwise check
  - Validate filter enum values against `FieldMetadata.enum_values`; reject invalid
- Fallback: default field selection from `ResourceMetadata.key_attributes` + common metrics

### Phase 4: Criteria Assembly & Temporal Detection (local)
- Build WHERE clauses from Phase 3's validated `filter_fields`
- Detect DURING clause from temporal patterns in user query
- Detect LIMIT from "top N" / "first N" patterns
- Determine grouping segments if metrics present without grouping context
- **No implicit filters** — only explicit user intent

### Phase 5: GAQL Generation (local assembly + validation)
- Assemble SELECT, FROM, WHERE, ORDER BY, LIMIT
- Add `segments.date` if temporal query detected
- Validate via `FieldMetadataCache::validate_field_selection`
- Output: `GAQLResult { query, validation, pipeline_trace }`

---

## Embedding Text Change

Modify `FieldMetadata::build_embedding_text()` in `crates/mcc-gaql-common/src/field_metadata.rs`:

**Before:** `campaign.name [ATTRIBUTE, STRING, selectable, filterable, sortable]. Campaign name attribute. Resource: campaign.`

**After:** `campaign.name [ATTRIBUTE]. Campaign name attribute. Resource: campaign.`

Keep: field name, description, usage_notes, enum_values, resource context, category tag (ATTRIBUTE provides semantic signal)
Remove: data_type, selectable/filterable/sortable boolean flags

This invalidates LanceDB cache; auto-rebuilds via hash change.

---

## Critical Files

| File | Change |
|------|--------|
| `crates/mcc-gaql-gen/src/rag.rs` | Add `MultiStepRAGAgent` alongside existing `EnhancedRAGAgent` |
| `crates/mcc-gaql-common/src/field_metadata.rs` | Modify `build_embedding_text()` for semantic-only; reuse existing `FieldMetadataCache` methods |
| `crates/mcc-gaql-gen/src/vector_store.rs` | Reuse existing LanceDB infrastructure (no schema changes needed) |
| `crates/mcc-gaql-gen/src/main.rs` | Add `--multistep` CLI flag |

### Key existing functions to reuse
- `FieldMetadataCache::get_resources()` — all resource names (`field_metadata.rs:306`)
- `FieldMetadataCache::get_resource_fields(resource)` — fields for a resource (`field_metadata.rs:282`)
- `FieldMetadataCache::get_field(name)` — single field lookup (`field_metadata.rs:333`)
- `FieldMetadataCache::get_metrics(pattern)` / `get_segments(pattern)` — filtered fields (`field_metadata.rs:238/253`)
- `FieldMetadataCache::validate_field_selection(fields)` — validation with errors/warnings (`field_metadata.rs:507`)
- `FieldMetadataCache::show_resources()` — resource summaries with descriptions (`field_metadata.rs:440`)
- `ResourceMetadata` struct — has `selectable_with`, `key_attributes`, `key_metrics`, `description`
- `build_or_load_field_vector_store()` / `build_or_load_query_vector_store()` — vector store init (`vector_store.rs`)
- `strip_markdown_code_blocks()` — clean LLM JSON output (`rag.rs`)

---

## Latency Estimate

| Phase | Description | Latency |
|-------|-------------|---------|
| Phase 1 | Resource Selection (LLM) | 500-1000ms |
| Phase 2 | 3x category RAG searches + filter | 100-200ms |
| Phase 2.5 | Filter pre-scan (local) | ~5ms |
| Phase 3 | Field Selection (LLM) | 500-1000ms |
| Phase 4 | Criteria assembly (local) | 10-20ms |
| Phase 5 | GAQL generation (local) | 10-20ms |
| **Total** | | **1.1-2.3s** |

Parallelization: Phase 2 (RAG) + cookbook retrieval can run in parallel via `tokio::join!`

---

## Verification

1. `cargo test --workspace` — all existing tests pass
2. `cargo clippy --workspace` — no new warnings
3. Manual test with sample queries from `resources/query_cookbook.toml`
4. Compare output quality vs existing `EnhancedRAGAgent`
5. Verify latency < 3s for multi-step pipeline
6. Test graceful degradation: mock LLM failure, verify fallback produces valid GAQL

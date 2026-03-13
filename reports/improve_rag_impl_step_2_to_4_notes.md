# Steps 2-4 Implementation Notes

## Step 2: Per-Resource Key Field Enrichment

### Summary

Implemented in `crates/mcc-gaql-gen/src/enricher.rs`. Added LLM-based key field selection for each resource to identify the most commonly useful attributes and metrics.

### Changes Made

#### 1. New Method: `select_key_fields_for_resource()`

```rust
async fn select_key_fields_for_resource(
    &self,
    resource: &str,
    cache: &FieldMetadataCache,
) -> Result<(Vec<String>, Vec<String>)>
```

This method:
1. Gets resource attributes via `cache.get_resource_fields(resource)` — `{resource}.*` ATTRIBUTE fields
2. Gets compatible metrics from `cache.get_resource_selectable_with(resource)` — filters entries starting with `metrics.`
3. Sends both lists to LLM with prompt: "Select the 5 most commonly useful attributes and 10 most commonly useful metrics for typical Google Ads reporting queries on the `{resource}` resource."
4. Parses JSON response: `{"key_attributes": [...], "key_metrics": [...]}`
5. Validates returned fields exist in cache
6. Falls back to current alphabetical first-N if LLM fails

#### 2. Integration into `enrich()` Method

Added after resource description enrichment (around line 199-227):

```rust
// Stage 3: Key field selection per resource
for resource in &resources {
    match self.select_key_fields_for_resource(resource, cache).await {
        Ok((key_attrs, key_mets)) => {
            if let Some(rm) = cache.resource_metadata.as_mut().and_then(|m| m.get_mut(resource)) {
                rm.key_attributes = key_attrs;
                rm.key_metrics = key_mets;
            }
        }
        Err(e) => log::warn!("Key field selection failed for '{}': {}", resource, e),
    }
}
```

#### 3. Stage Label Updates

Updated `run_enrichment_pipeline()` stage labels from 1/2, 2/2 to 1/3, 2/3, 3/3.

### Verification

- `cargo check -p mcc-gaql-gen`: ✅ Clean
- `cargo test -p mcc-gaql-gen`: ✅ Tests pass

---

## Step 3: MultiStepRAGAgent Implementation

### Summary

Implemented in `crates/mcc-gaql-gen/src/rag.rs`. Replaced old `RAGAgent` and `EnhancedRAGAgent` with a new 5-phase `MultiStepRAGAgent`.

### Changes Made

#### 1. Removed Old Code

- `RAGAgent` struct + impl + `convert_to_gaql()` public function
- `EnhancedRAGAgent` struct + impl + `convert_to_gaql_enhanced()` public function

#### 2. New Types

**PipelineConfig:**
```rust
pub struct PipelineConfig {
    pub add_defaults: bool,
}
impl Default for PipelineConfig {
    fn default() -> Self { Self { add_defaults: true } }
}
```

**MultiStepRAGAgent:**
```rust
pub struct MultiStepRAGAgent {
    llm_config: LlmConfig,
    field_cache: FieldMetadataCache,
    field_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
    query_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
    pipeline_config: PipelineConfig,
    _embed_client: rig_fastembed::Client,
}
```

#### 3. Five Pipeline Phases

**Phase 1: `select_resource()`**
- Builds compact resource list (name + description)
- System prompt: GAQL expert, returns JSON
- Validates related_resources against selectable_with
- Falls back to keyword-based detection

**Phase 2: `retrieve_field_candidates()`**
- Tier 1: key_attributes + key_metrics from ResourceMetadata
- Tier 2: RAG searches with post-filter by category
- Tier 3: key_attributes from related resources
- Compatibility filter against selectable_with

**Phase 2.5: `prescan_filters()`**
- Keyword mapping for status, type, device, network, match type
- Returns (field_name, enum_values) pairs

**Phase 3: `select_fields()`**
- Retrieves top-3 cookbook examples
- Builds prompt with full field details
- Post-validates fields exist and enum values are valid
- Falls back to key fields

**Phase 4: `assemble_criteria()`**
- Builds WHERE clauses from filter_fields
- Temporal detection (LAST_7_DAYS, LAST_30_DAYS, etc.)
- Limit detection (top N, first N, best N, worst N)
- Implicit defaults (status=ENABLED for campaign/ad_group/etc.)

**Phase 5: `generate_gaql()`**
- Assembles final GAQL query
- Adds segments.date if temporal and not present
- Validates via `validate_field_selection_for_resource()`
- Returns GAQLResult with query, validation, pipeline_trace

#### 4. New Public Function

```rust
pub async fn convert_to_gaql(
    example_queries: Vec<QueryEntry>,
    field_cache: FieldMetadataCache,
    prompt: &str,
    config: &LlmConfig,
    pipeline_config: PipelineConfig,
) -> Result<GAQLResult>
```

### Verification

- `cargo check -p mcc-gaql-gen`: ✅ Clean (with pre-existing warnings)
- `cargo test -p mcc-gaql-gen`: ✅ 8 tests pass, 1 pre-existing failure

---

## Step 4: CLI Changes

### Summary

Updated `crates/mcc-gaql-gen/src/main.rs` to use the new API and add verbose pipeline trace output.

### Changes Made

#### 1. Generate Command Update

```rust
Generate {
    prompt: String,
    queries: Option<String>,
    metadata: PathBuf,
    no_defaults: bool,  // NEW: replaces --basic
}
```

- Removed `--basic` flag (already done in earlier work)
- Added `--no-defaults` flag to skip implicit status filters
- `--metadata` is required

#### 2. cmd_generate Function Update

```rust
async fn cmd_generate(
    prompt: String,
    queries: Option<String>,
    metadata: PathBuf,
    no_defaults: bool,
    verbose: bool,  // NEW: passed from CLI
) -> Result<()>
```

Changes:
- Loads enriched metadata (warns if not enriched)
- Builds PipelineConfig with add_defaults flag
- Calls `rag::convert_to_gaql()` returning GAQLResult
- Prints query, validation errors/warnings
- **NEW:** Prints pipeline trace if `--verbose` enabled

#### 3. Pipeline Trace Output

When verbose mode is enabled, prints:
```
--- Pipeline Trace ---
Phase 1 - Primary resource: campaign
Phase 1 - Related resources: [...]
Phase 1 - Reasoning: ...
Phase 2 - Candidates: 35 (rejected: 5)
Phase 3 - Selected fields: [...]
Phase 3 - Filter fields: [...]
Phase 3 - Order by: [...]
Phase 4 - WHERE clauses: [...]
Phase 4 - DURING: LAST_30_DAYS
Phase 4 - LIMIT: 10
Phase 4 - Implicit filters: ["campaign.status = 'ENABLED'"]
Generation time: 1234ms
```

### Verification

- `cargo run -p mcc-gaql-gen -- generate --help`: ✅ Shows new flags
- `cargo check -p mcc-gaql-gen`: ✅ Clean
- `cargo test -p mcc-gaql-gen`: ✅ Tests pass

---

## Final Status

| Step | Description | Status |
|------|-------------|--------|
| 1 | New Types + Enhanced Validation | ✅ Complete |
| 2 | Per-Resource Key Field Enrichment | ✅ Complete |
| 3 | MultiStepRAGAgent Implementation | ✅ Complete |
| 4 | CLI Changes | ✅ Complete |
| 5 | vector_store.rs (No changes needed) | ✅ N/A |

## Test Results

- `cargo check --workspace`: ✅ Compiles
- `cargo test -p mcc-gaql-common`: ✅ 11 tests pass
- `cargo test -p mcc-gaql-gen`: ✅ 8 tests pass (1 pre-existing failure)

## PR

Created: https://github.com/mhuang74/mcc-gaql-rs/pull/52
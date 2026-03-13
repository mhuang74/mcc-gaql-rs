# Multi-Step RAG Pipeline v3 — Implementation Plan

## Context

The current `EnhancedRAGAgent` in `crates/mcc-gaql-gen/src/rag.rs` produces GAQL queries with limited accuracy due to keyword-based resource detection, generic field retrieval without compatibility filtering, no `selectable_with` validation, and single-shot LLM prompting. This plan replaces it (and the `SimpleRAGAgent`) entirely with a 5-phase `MultiStepRAGAgent` that leverages structured metadata for validated, high-accuracy GAQL generation.

### Key Data Findings (from interview)
- **Categories**: campaign.name/status/etc. are ATTRIBUTE, metrics.* are METRIC, segments.* are SEGMENT, bare resource names (campaign, ad_group) are RESOURCE — correct in regenerated metadata
- **selectable_with on RESOURCE fields**: Contains resource names + metric field names + segment field names (e.g., campaign RESOURCE field has 5 resources + 202 metrics + 120 segments = 327 items)
- **selectable_with on METRIC fields**: Contains resource names + segment field names (e.g., metrics.clicks has 59 resources + 108 segments)
- **selectable_with on ATTRIBUTE fields**: Empty — attributes don't carry their own compatibility lists
- **key_attributes**: Currently first 10 alphabetically (not curated). **key_metrics**: Empty (metrics aren't in `get_resource_fields()`)
- **resources map**: Only contains `{resource}.{field}` entries — no metrics/segments mapped to resources
- **Total**: 2906 fields, 181 resources, 233 metrics, 133 segments, 2361 attributes

### Design Decisions (from user interview)

| Area | Decision |
|------|----------|
| Field compatibility | Check ALL fields against FROM resource's RESOURCE-field `selectable_with` |
| Field candidate cap | Resource key_fields + RAG only (~30-40 candidates) |
| Field detail in Phase 3 | Full: name + description + filterable/sortable flags + enum values for filter fields |
| Implicit filters | Defaults ON (status=ENABLED); `--no-defaults` flag to disable |
| Key field enrichment | New per-resource LLM pass during enrichment: 5 key_attributes + 10 key_metrics |
| Embedding text | Enhanced semantic — more natural language, remove structural flags |
| Migration | Remove SimpleRAGAgent + EnhancedRAGAgent entirely |
| Public API | Return `GAQLResult` { query, validation, pipeline_trace } |
| Metadata requirement | Require enriched metadata; error if not available |
| Validation | FROM resource check — use RESOURCE-field's selectable_with as the compatibility list |

---

## Implementation Steps (ordered by dependency)

### Step 1: New Types + Enhanced Validation in `mcc-gaql-common/src/field_metadata.rs`

**Add types** (after line 610, before `#[cfg(test)]`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GAQLResult {
    pub query: String,
    pub validation: ValidationResult,
    pub pipeline_trace: PipelineTrace,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineTrace {
    pub phase1_primary_resource: String,
    pub phase1_related_resources: Vec<String>,
    pub phase1_dropped_resources: Vec<String>,
    pub phase1_reasoning: String,
    pub phase2_candidate_count: usize,
    pub phase2_rejected_count: usize,
    pub phase3_selected_fields: Vec<String>,
    pub phase3_filter_fields: Vec<FilterField>,
    pub phase3_order_by_fields: Vec<String>,
    pub phase4_where_clauses: Vec<String>,
    pub phase4_during: Option<String>,
    pub phase4_limit: Option<u32>,
    pub phase4_implicit_filters: Vec<String>,
    pub generation_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterField {
    pub field_name: String,
    pub operator: String,
    pub value: String,
}
```

**Add `Serialize, Deserialize`** derives to `ValidationResult`, `ValidationError`, `ValidationWarning` (lines 566-610).

**Add new `ValidationError` variant**:
```rust
IncompatibleFields { fields: Vec<String>, resource: String },
```

**Add new method** to `FieldMetadataCache`:
```rust
/// Get the RESOURCE-category field's selectable_with list for a resource
pub fn get_resource_selectable_with(&self, resource: &str) -> Vec<String> {
    self.fields.get(resource)
        .filter(|f| f.is_resource())
        .map(|f| f.selectable_with.clone())
        .unwrap_or_default()
}

/// Validate field selection against a FROM resource's compatibility list
pub fn validate_field_selection_for_resource(
    &self,
    field_names: &[String],
    from_resource: &str,
) -> ValidationResult
```

The new validation method:
1. Runs existing checks (field existence, selectability, metrics grouping)
2. Gets `from_resource`'s RESOURCE-field `selectable_with`
3. For each metric in `field_names`: check metric field name is in the list
4. For each segment in `field_names`: check segment field name is in the list
5. For attributes: check they belong to `from_resource` or related resources (name prefix check)
6. Add `IncompatibleFields` error if any fail

**Enhance `build_embedding_text()`** (line 75):
- Remove: `data_type`, `selectable/filterable/sortable` flags
- Keep: field name, `[CATEGORY]` tag, description, usage_notes, enum_values, resource context
- Change format to: `"{name} [{CATEGORY}]. {description}. {usage_notes}. Valid values: {enums}. Resource: {resource}."`

This invalidates LanceDB cache via hash change — auto-rebuilds.

---

### Step 2: Per-Resource Key Field Enrichment in `enricher.rs`

**Add new method** to `MetadataEnricher`:
```rust
async fn select_key_fields_for_resource(
    &self,
    resource: &str,
    cache: &FieldMetadataCache,
) -> Result<(Vec<String>, Vec<String>)>
```

This method:
1. Gets resource attributes via `cache.get_resource_fields(resource)` — these are the `{resource}.*` ATTRIBUTE fields
2. Gets compatible metrics from `cache.get_resource_selectable_with(resource)` — filter entries starting with `metrics.`
3. Sends both lists to LLM with prompt: "Select the 5 most commonly useful attributes and 10 most commonly useful metrics for typical Google Ads reporting queries on the `{resource}` resource."
4. Parses JSON response: `{"key_attributes": [...], "key_metrics": [...]}`
5. Validates returned fields exist in cache
6. Falls back to current alphabetical first-N if LLM fails

**Integrate into `enrich()` method** (after line 197, after resource description enrichment):
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

**Update `run_enrichment_pipeline()`** (line 457): Change stage labels from 1/2, 2/2 to 1/3, 2/3 and add 3/3 for key field selection.

---

### Step 3: MultiStepRAGAgent in `rag.rs`

**Remove** (lines 692-1159):
- `RAGAgent` struct + impl + `convert_to_gaql()` public function
- `EnhancedRAGAgent` struct + impl + `convert_to_gaql_enhanced()` public function

**Keep** (lines 24-690):
- `LlmConfig` and all methods
- `create_embedding_client()`
- `AgentResources`, `init_llm_resources()`
- `format_llm_request_debug()`
- Hash computation functions
- Vector store building functions
- `strip_markdown_code_blocks()`
- `QueryEntryEmbed`, `FieldDocument`, `FieldDocumentFlat` and all their impls
- All existing tests (update to use new API where needed)

**Add**:

```rust
pub struct PipelineConfig {
    pub add_defaults: bool,
}
impl Default for PipelineConfig {
    fn default() -> Self { Self { add_defaults: true } }
}
```

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

#### Phase 1: `select_resource(&self, user_query: &str) -> Result<(String, Vec<String>, PipelineTrace)>`
- Build compact resource list: name + description only (from `resource_metadata`)
- System prompt: GAQL expert, return JSON `{"primary_resource", "related_resources", "confidence", "reasoning"}`
- Validate related_resources: get primary's RESOURCE-field selectable_with, filter to resource names only (no dots), check related resources appear in that list
- Drop incompatible, record in trace
- **Fallback**: keyword-based detection (expanded from current `identify_resources()`)

#### Phase 2: `retrieve_field_candidates(&self, query, primary, related) -> Result<Vec<FieldMetadata>>`
- **Tier 1**: `key_attributes` + `key_metrics` from `ResourceMetadata` for primary resource
- **Tier 2**: 3 RAG searches with over-fetch + post-filter:
  - Attributes: search top-30, post-filter to `name.starts_with("{primary}.")`, take top-10
  - Metrics: search top-30, post-filter to `is_metric()`, take top-10
  - Segments: search top-15, post-filter to `is_segment()`, take top-5
- **Tier 3**: `key_attributes` from each related resource
- **Compatibility filter**: Get primary's RESOURCE-field selectable_with. For metrics: field name must be in list. For segments: field name must be in list. For attributes: name prefix must match primary or related resource.
- Dedup by field name

#### Phase 2.5: `prescan_filters(&self, query, candidates) -> Vec<(String, Vec<String>)>`
- Keyword map: "enabled"/"paused"/"active"/"status" → "status"; "type"/"channel" → "advertising_channel_type"/"campaign_type"; "device"/"mobile"/"desktop" → "device"; "network"/"search"/"display" → "ad_network_type"; "match type" → "keyword_match_type"
- For each keyword match found in user query, find matching candidate fields, return `(field_name, enum_values)`

#### Phase 3: `select_fields(&self, query, primary, candidates, filter_enums) -> Result<FieldSelectionResult>`
- Retrieve top-3 cookbook examples via query_index
- Build prompt with candidates organized by category:
  - Each field shows: name, description (1 line), `[filterable]` / `[sortable]` tags, enum values if from pre-scan
- System prompt: return JSON `{"select_fields": [...], "filter_fields": [{"field", "operator", "value"}], "order_by_fields": [{"field", "direction"}], "reasoning": "..."}`
- **Post-processing**:
  1. Validate all fields exist + selectable via `field_cache.get_field()`
  2. Validate filter enum values: if field has `enum_values`, check the filter value is valid
  3. Remove fields failing validation, log warnings
- **Fallback**: `key_attributes` (first 3) + `key_metrics` (first 3) from ResourceMetadata

#### Phase 4: `assemble_criteria(&self, query, field_selection, trace) -> (Vec<String>, Option<String>, Option<u32>)`
- Build WHERE clauses from validated filter_fields
- Temporal detection: pattern match "last 7 days" → LAST_7_DAYS, "last 30 days" → LAST_30_DAYS, etc.
- Limit detection: regex "top (\d+)", "first (\d+)", "best (\d+)", "worst (\d+)"
- **Implicit defaults** (if `add_defaults=true`):
  - If FROM is campaign/ad_group/keyword_view etc. and no explicit status filter → add `{resource}.status = 'ENABLED'`
  - Record added filters in `trace.phase4_implicit_filters`

#### Phase 5: `generate_gaql(&self, ...) -> GAQLResult`
- If temporal and `segments.date` not in select_fields → add it
- Assemble: `SELECT {fields}\nFROM {resource}\nWHERE {clauses}\nORDER BY {order}\nLIMIT {n}`
- WHERE includes DURING clause: `segments.date DURING {period}`
- Validate via `field_cache.validate_field_selection_for_resource()`
- Return `GAQLResult { query, validation, pipeline_trace }`

#### Main orchestrator: `pub async fn generate(&self, user_query: &str) -> Result<GAQLResult>`
- Phase 1 → Phase 2 → Phase 2.5 → Phase 3 → Phase 4 → Phase 5
- Parallelization: Phase 2 RAG + cookbook retrieval via `tokio::join!`

#### New public function:
```rust
pub async fn convert_to_gaql(
    example_queries: Vec<QueryEntry>,
    field_cache: FieldMetadataCache,
    prompt: &str,
    config: &LlmConfig,
    pipeline_config: PipelineConfig,
) -> Result<GAQLResult>
```

---

### Step 4: CLI Changes in `main.rs`

**Update `Commands::Generate`**:
- Remove `--basic` flag
- Add `--no-defaults` flag
- `--metadata` becomes required (or auto-resolved to enriched path)

**Update `cmd_generate()`**:
- Remove basic/enhanced branching
- Load enriched metadata (error with helpful message if missing)
- Call `rag::convert_to_gaql()`
- Print query, validation errors/warnings
- Print pipeline trace if `--verbose`

---

### Step 5: No Changes to `vector_store.rs`

The `FieldDocumentFlat` already stores `category`, so Phase 2's per-category RAG works by over-fetching and post-filtering in Rust. No schema or search changes needed. The embedding text change in Step 1 invalidates the cache hash automatically.

---

## Critical Files

| File | Change |
|------|--------|
| `crates/mcc-gaql-common/src/field_metadata.rs` | Add GAQLResult/PipelineTrace/FilterField types, Serialize/Deserialize on validation types, `get_resource_selectable_with()`, `validate_field_selection_for_resource()`, IncompatibleFields variant, enhanced `build_embedding_text()` |
| `crates/mcc-gaql-gen/src/enricher.rs` | Add `select_key_fields_for_resource()`, integrate into `enrich()` after resource descriptions, update stage labels in `run_enrichment_pipeline()` |
| `crates/mcc-gaql-gen/src/rag.rs` | Remove RAGAgent + EnhancedRAGAgent + public functions. Add PipelineConfig, MultiStepRAGAgent with 5 phases, new `convert_to_gaql()` public function |
| `crates/mcc-gaql-gen/src/main.rs` | Remove `--basic`, add `--no-defaults`, update `cmd_generate()` to use new API and display GAQLResult |

### Existing Functions to Reuse
- `FieldMetadataCache::get_resources()` — `field_metadata.rs:306`
- `FieldMetadataCache::get_resource_fields(resource)` — `field_metadata.rs:282`
- `FieldMetadataCache::get_field(name)` — `field_metadata.rs:333`
- `FieldMetadataCache::validate_field_selection(fields)` — `field_metadata.rs:507`
- `FieldMetadataCache::show_resources()` — `field_metadata.rs:440`
- `FieldMetadataCache::enriched_field_count()` — `field_metadata.rs:337`
- `LlmConfig::create_agent_for_model()` — `rag.rs:151`
- `LlmConfig::preferred_model()` — `rag.rs:115`
- `strip_markdown_code_blocks()` — `rag.rs:503`
- `build_or_load_field_vector_store()` — `vector_store.rs:393`
- `build_or_load_query_vector_store()` — `vector_store.rs:333`
- `compute_field_cache_hash()` — `rag.rs:237`
- `compute_query_cookbook_hash()` — `rag.rs:225`

---

## Verification

1. **Compilation**: `cargo check --workspace` — no errors
2. **Linting**: `cargo clippy --workspace` — no new warnings
3. **Unit tests**: `cargo test --workspace` — all pass
   - `test_validate_field_selection_for_resource_rejects_incompatible_metric`
   - `test_validate_field_selection_for_resource_accepts_compatible_metric`
   - `test_get_resource_selectable_with`
   - `test_build_embedding_text_no_structural_flags`
   - `test_prescan_filters_detects_status`
   - `test_assemble_criteria_temporal_detection`
   - `test_assemble_criteria_limit_detection`
   - `test_assemble_criteria_implicit_filters_on_off`
   - `test_keyword_fallback_resource_selection`
4. **Manual test** (requires LLM env vars):
   - `cargo run -p mcc-gaql-gen -- generate "show me campaign performance last 30 days" --metadata ~/.cache/mcc-gaql/field_metadata_enriched.json -v`
   - Verify: FROM campaign, has metrics, segments.date, DURING LAST_30_DAYS, status=ENABLED default filter
   - `cargo run -p mcc-gaql-gen -- generate "top 10 keywords by clicks" --metadata ... -v`
   - Verify: FROM keyword_view, metrics.clicks, ORDER BY metrics.clicks DESC, LIMIT 10
5. **Enrichment test** (requires LLM env vars):
   - `cargo run -p mcc-gaql-gen -- enrich` — verify key_attributes/key_metrics populated in output JSON

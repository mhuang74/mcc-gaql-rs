# Plan: Enhance RAG with Field Index in Phase 1 & Remove Keyword Matching in Phase 2

## Context

The RAG pipeline currently works well, but has two opportunities for improvement:

1. **Phase 1 (Resource Selection)** only searches `resource_index`, which provides high-level resource descriptions. Adding `field_index` search provides a "bottom-up" signal - if specific fields like `metrics.cost_micros` match the query, this strengthens the case for resources that contain those fields (e.g., `campaign`).

2. **Phase 2 (Field Retrieval)** uses keyword matching as a supplementary step. With improved semantic search limits and threshold-based filtering, keyword matching is no longer necessary and can be removed to simplify the pipeline.

**Key design decisions (from user interview):**
- Present resource and field results as **separate sections** to LLM
- Use same **SIMILARITY_THRESHOLD (0.65)** for both searches
- Phase 2 should use **high limit (200)** with threshold as primary filter

---

## Change 1: Add field_index RAG to Phase 1

### Files to Modify
- `/Users/mhuang/Projects/Development/googleads/mcc-gaql-rs/crates/mcc-gaql-gen/src/rag.rs`

### 1.1 Add `FieldSearchResult` struct (near line 1280, after `ResourceSearchResult`)

```rust
#[derive(Debug, Clone)]
pub struct FieldSearchResult {
    pub field_name: String,
    pub score: f64,           // similarity = 1.0 - distance
    pub category: String,     // ATTRIBUTE, METRIC, SEGMENT
    pub resource_name: String,
    pub description: String,
    pub filterable: bool,
    pub sortable: bool,
}
```

### 1.2 Add `search_field_embeddings()` function (after `search_resource_embeddings` ~line 1972)

Similar pattern to `search_resource_embeddings()`:
- Build `VectorSearchRequest` with no filter (search all fields)
- Use 100 samples as retrieval limit
- Search `self.field_index` 
- Convert results to `FieldSearchResult` with `score = 1.0 - distance`

### 1.3 Add `retrieve_relevant_fields()` function (after `retrieve_relevant_resources` ~line 2000)

```rust
pub async fn retrieve_relevant_fields(
    &self,
    query: &str,
    limit: usize,
) -> Result<Vec<FieldSearchResult>, anyhow::Error>
```

- Call `search_field_embeddings(query, limit)`
- Filter to results where `score >= SIMILARITY_THRESHOLD`
- Return filtered results

### 1.4 Add `format_field_results_for_phase1()` method

Format field results similar to `format_field_llm()` in formatter.rs:
```
--- FIELDS (semantic matches) ---
- metrics.cost_micros [0.823] [filterable] [sortable]: The sum of your cost...
- campaign.name [0.789] [filterable]: The name of the campaign...
```

Group by category (ATTRIBUTE, METRIC, SEGMENT) for readability.

### 1.5 Modify `select_resource()` (~line 2057-2082)

**Current:**
```rust
let (resources, used_rag) = match self.retrieve_relevant_resources(user_query, 20).await {
    ...
};
```

**New:** Run both searches in parallel:
```rust
let (resource_result, field_result) = tokio::join!(
    self.retrieve_relevant_resources(user_query, 20),
    self.retrieve_relevant_fields(user_query, 100)
);

let (resources, used_rag) = match resource_result { /* existing logic */ };
let field_matches = field_result.unwrap_or_else(|e| {
    log::warn!("Phase 1: Field search failed: {}", e);
    vec![]
});
```

### 1.6 Update LLM prompt (~lines 2131-2167)

Add field section after categorized resources:
```rust
let field_section = if !field_matches.is_empty() {
    format!("\n\n{}", self.format_field_results_for_phase1(&field_matches))
} else {
    String::new()
};

let combined_resources = format!("{}{}{}", categorized_resources, field_section, cookbook_examples);
```

Update system prompt guidance to explain how to use field matches:
```
RESOURCES section shows available resources organized by category.
FIELDS section shows individual fields that semantically match the query - 
use these as hints for resource selection (e.g., if metrics.cost_micros 
matches highly, campaign/ad_group resources are likely relevant).
```

---

## Change 2: Remove Keyword Matching from Phase 2

### Files to Modify
- `/Users/mhuang/Projects/Development/googleads/mcc-gaql-rs/crates/mcc-gaql-gen/src/rag.rs`

### 2.1 Increase vector search limits (~lines 2511, 2532, 2548)

**Current:** 50 attributes, 30 metrics, 30 segments (110 total)

**New:** 100 attributes, 50 metrics, 50 segments (200 total)

```rust
// Line 2511
.samples(100)  // was 50

// Line 2532  
.samples(50)   // was 30

// Line 2548
.samples(50)   // was 30
```

### 2.2 Remove keyword matching call (~lines 2629-2645)

Delete:
```rust
// =========================================================================
// Tier 3: Keyword-based supplementary search
// =========================================================================
// Vector search may miss fields when query has competing terms (e.g., "budget"
// dominating "app id"). Extract key terms and find fields that match them
// in their name or description.
let keyword_matches = self.find_keyword_matching_fields(
    user_query,
    &valid_attr_resources,
    &selectable_with,
    &mut seen,
);
log::debug!(
    "Phase 2: Keyword search found {} additional fields",
    keyword_matches.len()
);
candidates.extend(keyword_matches);
```

### 2.3 Delete `find_keyword_matching_fields()` function (~lines 2781-2904)

Remove the entire function definition.

### 2.4 Update logging (~line 2560)

```rust
log::debug!("Phase 2: Running 3 parallel vector searches (100 attr, 50 metric, 50 segment)...");
```

---

## Verification

### Manual Testing

1. **Test Phase 1 field influence on resource selection:**
   ```bash
   cargo run -p mcc-gaql-gen -- generate "show me cost per click metrics" --explain
   ```
   - Verify logs show field search results
   - Check that field matches like `metrics.average_cpc` appear in Phase 1 context
   - Confirm resource selection is influenced appropriately

2. **Test Phase 2 without keyword matching:**
   ```bash
   cargo run -p mcc-gaql-gen -- generate "campaigns with app id and budget" --explain
   ```
   - Verify that fields previously found via keyword matching (app_id, budget) are still retrieved via semantic search with higher limits

3. **Test metadata command (should be unchanged):**
   ```bash
   cargo run -p mcc-gaql-gen -- metadata "click metrics"
   ```

### Automated Tests
```bash
cargo test -p mcc-gaql-gen -- --test-threads=1
```

---

## Summary of Changes

| Location | Change |
|----------|--------|
| rag.rs ~line 1280 | Add `FieldSearchResult` struct |
| rag.rs ~line 1972 | Add `search_field_embeddings()` function |
| rag.rs ~line 2000 | Add `retrieve_relevant_fields()` function |
| rag.rs (new method) | Add `format_field_results_for_phase1()` method |
| rag.rs ~line 2057 | Parallel search for resources AND fields |
| rag.rs ~line 2131 | Include field section in LLM prompt |
| rag.rs ~line 2511 | Increase attr search limit: 50 → 100 |
| rag.rs ~line 2532 | Increase metric search limit: 30 → 50 |
| rag.rs ~line 2548 | Increase segment search limit: 30 → 50 |
| rag.rs ~lines 2629-2645 | Delete keyword matching call |
| rag.rs ~lines 2781-2904 | Delete `find_keyword_matching_fields()` function |

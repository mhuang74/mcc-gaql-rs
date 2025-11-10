# RAG Quality Improvement Plan - Field Metadata Vector Store

**Status:** 🔴 Critical Issues Identified
**Priority:** P0
**Date:** 2024-11-10
**Related:** `embedding-cache-design.md`, `gaql-metadata-for-llm-design.md`

## Executive Summary

Integration testing of the Fields Vector Store RAG system revealed **critical quality issues** that prevent effective field retrieval. Basic queries like "cost per click" and "conversions" return 0% precision and 0% recall. Additionally, similarity scores exceed the mathematically valid range [0, 1], indicating fundamental implementation problems.

**Impact:** The RAG system is currently ineffective at retrieving relevant fields for natural language queries, severely limiting the quality of LLM-generated GAQL queries.

## Issues Discovered

### 1. 🔴 CRITICAL: Invalid Similarity Scores (> 1.0)

**Symptom:**
```
Query: "stuff and things"
Results:
- goal.retention_goal_settings.value_settings.additional_high_lifetime_value: 1.121
- ad.call_ad.business_name: 1.288
- ad_group_ad.ad.responsive_display_ad.youtube_videos: 1.382
```

**Expected:** Cosine similarity scores must be in range [0, 1]
**Actual:** Scores exceed 1.0
**Location:** `src/prompt2gaql.rs:631` - `field_index.top_n::<FieldDocumentFlat>()`

**Root Cause Analysis:**
- Cosine similarity between unit vectors is bounded by [-1, 1]
- Similarity to same direction (positive correlation) is [0, 1]
- Scores > 1.0 indicate:
  1. Vectors are not normalized
  2. Wrong distance metric being used (L2 distance instead of cosine)
  3. Bug in LanceDB/rig-lancedb scoring computation

**Verification Needed:**
1. Check if embeddings from `rig-fastembed` are normalized
2. Verify LanceDB index configuration uses cosine similarity
3. Check if `SearchParams::default()` specifies correct metric

---

### 2. 🔴 CRITICAL: Non-Descending Score Order

**Symptom:**
```
Query: "campaign budget amount"
Position 0: score 0.136 > Position 1: score 0.141
```

**Expected:** Results sorted by descending relevance (higher score = more relevant)
**Actual:** Scores increase instead of decrease

**Root Cause:**
- LanceDB may be returning distance (lower = better) instead of similarity (higher = better)
- Distance metrics: smaller values mean closer/more similar
- Similarity metrics: larger values mean more similar
- Code assumes similarity but LanceDB returns distance

**Impact:** Most relevant results appear **last** instead of first, breaking RAG logic

---

### 3. 🔴 CRITICAL: Zero Precision on Basic Queries

**Test Results:**

| Query | Expected Fields | Retrieved Fields | Precision | Recall |
|-------|----------------|------------------|-----------|--------|
| "cost per click and average cost" | `metrics.average_cpc`, `metrics.cost_micros`, `metrics.cost_per_conversion` | `campaign_asset.status`, `ad_group_ad.ad.text_ad.description1`, etc. | **0%** | **0%** |
| "conversion data and conversion rate" | `metrics.conversions`, `metrics.conversion_rate`, `metrics.all_conversions` | `campaign_goal_config...`, `user_interest.user_interest_parent`, etc. | **0%** | **0%** |
| "impressions and clicks" | `metrics.impressions`, `metrics.clicks`, `metrics.ctr` | `conversion_value_rule...`, `custom_audience`, etc. | **0%** | **0%** |
| "performance metrics" | `metrics.impressions`, `metrics.clicks`, `metrics.conversions` | `ad_group.target_roas`, `campaign.bidding_strategy_system_status`, etc. | **0%** | **0%** |

**Root Cause:** Overly simplistic field descriptions (see Issue #4)

---

### 4. 🟡 HIGH: Simplistic Field Descriptions

**Current Implementation** (`src/prompt2gaql.rs:331`):
```rust
pub fn new(field: FieldMetadata) -> Self {
    // Just normalize the field name
    let description = field.name.replace('.', " ").replace('_', " ");

    // ALL OF THIS IS COMMENTED OUT:
    // if self.field.is_metric() {
    //     parts.push("performance metric");
    // }
    // ... category descriptions
    // ... data type information
    // ... capabilities (selectable, filterable, sortable)
    // ... purpose inference
}
```

**Example Current Description:**
```
Field: metrics.cost_micros
Description: "metrics cost micros"
```

**What It Should Be:**
```
Field: metrics.cost_micros
Description: "metrics cost micros - performance metric for cost measurement in micros, numeric data type, selectable and filterable, used for cost analysis and budget tracking"
```

**Impact:**
- Embedding model has minimal semantic information
- Cannot distinguish between `metrics.cost_micros` and `campaign.cost_per_click_goal`
- Domain knowledge (metric vs segment vs attribute) is lost

---

### 5. 🟡 HIGH: No Similarity Threshold Filtering

**Current Behavior:**
Always returns top N results regardless of score

**Problem:**
- Vague queries return irrelevant results with low scores
- No way to indicate "no good matches found"
- Forces inclusion of poor matches

**Example:**
```
Query: "video views and view rate"
Retrieved: "ad_group_criterion.effective_cpv_bid_source" (score: 0.338)
Expected: "metrics.video_views", "metrics.video_view_rate"
```

Score of 0.338 suggests poor match, but still returned as "relevant"

---

### 6. 🟢 MEDIUM: Fixed Retrieval Limit

**Current:** Always retrieves 10 fields (`src/prompt2gaql.rs:788`)

**Problem:**
- Simple queries may only need 3-5 fields
- Complex queries may need 15-20 fields
- No adaptation to query complexity

---

## Root Cause Summary

1. **Scoring Bug:** Distance vs Similarity confusion in LanceDB integration
2. **Poor Embeddings:** Simplified descriptions lack semantic richness
3. **No Quality Gates:** No minimum threshold to filter irrelevant results
4. **Inflexible Retrieval:** Fixed 10-field limit regardless of query

---

## Action Plan

### Phase 1: Fix Critical Scoring Issues (P0 - Immediate)

**Goal:** Make scores mathematically valid and properly ordered

#### Task 1.1: Investigate Scoring Mechanism
**Owner:** Developer
**Effort:** 2-4 hours
**Files:** `src/prompt2gaql.rs`, `src/lancedb_utils.rs`

**Steps:**
1. Check LanceDB table creation in `lancedb_utils.rs::build_or_load_field_vector_store()`
   ```rust
   // Verify metric type in table configuration
   // Expected: "cosine" similarity
   // Check: IVF index configuration
   ```

2. Verify `SearchParams::default()` configuration
   ```rust
   // Check if it uses:
   // - Cosine similarity (correct)
   // - L2 distance (wrong - would need inversion)
   ```

3. Check if `rig-fastembed` embeddings are normalized
   ```rust
   // Add debug logging:
   for (doc, embedding) in field_embeddings {
       let magnitude = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
       log::debug!("Embedding magnitude for {}: {}", doc.field.name, magnitude);
   }
   // Should be ~1.0 for normalized vectors
   ```

4. Test with known similar/dissimilar pairs
   ```rust
   // Search for "metrics.clicks" - should return:
   // - metrics.impressions (high similarity)
   // - metrics.ctr (high similarity)
   // - campaign.name (low similarity)
   ```

**Success Criteria:**
- ✅ All scores in range [0, 1] (or [-1, 1] for cosine)
- ✅ Scores in descending order (higher = more relevant)
- ✅ Similar fields have scores > 0.5
- ✅ Dissimilar fields have scores < 0.3

#### Task 1.2: Fix Distance/Similarity Inversion (if needed)
**Owner:** Developer
**Effort:** 1-2 hours

If LanceDB returns distance instead of similarity:
```rust
// In retrieve_relevant_fields() at src/prompt2gaql.rs:639
let field_results: Vec<FieldMetadata> = results
    .into_iter()
    .map(|(distance, _, flat_doc)| {
        // Convert distance to similarity if needed
        // For L2 distance: similarity = 1.0 / (1.0 + distance)
        // For cosine distance: similarity = 1.0 - distance
        FieldMetadata::from(flat_doc)
    })
    .collect();
```

**OR** configure LanceDB to return similarity directly in index creation.

---

### Phase 2: Improve Field Descriptions (P0 - Immediate)

**Goal:** Restore rich semantic descriptions for better embeddings

#### Task 2.1: Uncomment and Enhance Description Generation
**Owner:** Developer
**Effort:** 2-3 hours
**File:** `src/prompt2gaql.rs:331-375`

**Implementation:**
```rust
impl FieldDocument {
    pub fn new(field: FieldMetadata) -> Self {
        let mut parts = Vec::new();

        // 1. Normalized field name (keep existing)
        let base_name = field.name.replace('.', " ").replace('_', " ");
        parts.push(base_name);

        // 2. Category-based descriptions (RESTORE)
        if field.is_metric() {
            parts.push("performance metric".to_string());
        } else if field.category == "SEGMENT" {
            parts.push("dimension for grouping and filtering".to_string());
        } else if field.category == "ATTRIBUTE" {
            parts.push("entity attribute or resource property".to_string());
        }

        // 3. Data type information (RESTORE)
        match field.data_type.as_str() {
            "INT64" | "DOUBLE" | "FLOAT" => parts.push("numeric data type".to_string()),
            "STRING" => parts.push("text data type".to_string()),
            "BOOLEAN" => parts.push("boolean flag".to_string()),
            "DATE" => parts.push("date value".to_string()),
            "ENUM" => parts.push("enumerated value".to_string()),
            _ => {}
        }

        // 4. Capabilities (RESTORE)
        let mut capabilities = Vec::new();
        if field.selectable {
            capabilities.push("selectable");
        }
        if field.filterable {
            capabilities.push("filterable");
        }
        if field.sortable {
            capabilities.push("sortable");
        }
        if !capabilities.is_empty() {
            parts.push(capabilities.join(" and ").to_string());
        }

        // 5. Purpose inference (ENHANCE)
        parts.push(Self::infer_purpose(&field.name));

        // 6. Resource context (NEW)
        if let Some(resource) = field.resource_name.as_ref() {
            parts.push(format!("from {} resource", resource));
        }

        let description = parts.join(", ");

        FieldDocument {
            field,
            description,
        }
    }

    fn infer_purpose(field_name: &str) -> String {
        // RESTORE and ENHANCE this function
        let lower = field_name.to_lowercase();

        if lower.contains("cost") || lower.contains("cpc") || lower.contains("cpm") {
            "used for cost analysis and budget tracking".to_string()
        } else if lower.contains("conversion") {
            "used for conversion tracking and optimization".to_string()
        } else if lower.contains("impression") || lower.contains("click") || lower.contains("ctr") {
            "used for engagement and visibility analysis".to_string()
        } else if lower.contains("video") && (lower.contains("view") || lower.contains("watch")) {
            "used for video advertising performance".to_string()
        } else if lower.contains("date") || lower.contains("time") || lower.contains("day") {
            "used for time-based analysis and trending".to_string()
        } else if lower.contains("id") || lower.contains("name") {
            "identifier or label".to_string()
        } else {
            "".to_string() // No specific purpose identified
        }
    }
}
```

**Testing:**
```bash
# Run field description quality test
cargo test --test field_vector_store_rag_tests test_field_description_quality -- --nocapture

# Should see rich descriptions like:
# "metrics cost micros, performance metric, numeric data type, selectable and filterable, used for cost analysis and budget tracking, from metrics resource"
```

#### Task 2.2: Rebuild Embeddings with New Descriptions
**Owner:** Developer
**Effort:** 15-20 minutes (one-time rebuild)

```bash
# Delete existing LanceDB cache to force rebuild
rm -rf ~/.cache/mcc-gaql/lancedb/  # Linux/macOS
rm -rf ~/Library/Caches/mcc-gaql/lancedb/  # macOS alternative

# Run tool to rebuild with new descriptions
mcc-gaql --user-email test@example.com --mcc-id 1234567890 "SELECT campaign.name FROM campaign LIMIT 1"
```

---

### Phase 3: Add Quality Gates (P1 - Next Sprint)

**Goal:** Filter low-quality matches and adapt retrieval

#### Task 3.1: Implement Similarity Threshold Filtering
**Owner:** Developer
**Effort:** 1-2 hours
**File:** `src/prompt2gaql.rs:639-642`

**Implementation:**
```rust
async fn retrieve_relevant_fields(&self, user_query: &str, limit: usize) -> Vec<FieldMetadata> {
    if let Some(ref field_index) = self.field_vector_store {
        // ... existing search code ...

        match field_index.top_n::<FieldDocumentFlat>(search_request).await {
            Ok(results) => {
                log::debug!("Retrieved {} relevant fields for query: {}", results.len(), user_query);

                // CONFIGURE threshold based on testing
                const MIN_SIMILARITY_THRESHOLD: f64 = 0.4;

                // Filter by threshold and log filtered results
                let filtered_results: Vec<_> = results
                    .into_iter()
                    .filter(|(score, id, doc)| {
                        if *score < MIN_SIMILARITY_THRESHOLD {
                            log::debug!("Filtered low-score field: {} (score: {:.3})", id, score);
                            false
                        } else {
                            true
                        }
                    })
                    .collect();

                log::debug!("After filtering: {} fields remain (threshold: {:.2})",
                           filtered_results.len(), MIN_SIMILARITY_THRESHOLD);

                // Convert to FieldMetadata
                filtered_results
                    .into_iter()
                    .map(|(_, _, flat_doc)| FieldMetadata::from(flat_doc))
                    .collect()
            }
            Err(e) => {
                log::warn!("Failed to retrieve relevant fields: {}", e);
                Vec::new()
            }
        }
    } else {
        Vec::new()
    }
}
```

**Threshold Tuning:**
1. Run tests with different thresholds (0.3, 0.4, 0.5)
2. Measure precision/recall trade-off
3. Select threshold that maximizes F1 score
4. Make configurable via environment variable for experimentation

#### Task 3.2: Dynamic Retrieval Limit
**Owner:** Developer
**Effort:** 2-3 hours
**File:** `src/prompt2gaql.rs:788`

**Implementation:**
```rust
fn determine_retrieval_limit(&self, user_query: &str) -> usize {
    let query_lower = user_query.to_lowercase();

    // Complex queries need more fields
    if query_lower.contains(" and ") || query_lower.contains(" or ") {
        return 15; // Multi-concept queries
    }

    // Trending/time-series queries need more fields
    if query_lower.contains("trend") || query_lower.contains("over time") {
        return 12; // Need date segments + metrics
    }

    // Simple metric queries need fewer fields
    if query_lower.split_whitespace().count() <= 3 {
        return 8; // Simple queries
    }

    10 // Default
}

// Usage in prompt():
let field_limit = self.determine_retrieval_limit(user_query);
let relevant_fields = self.retrieve_relevant_fields(user_query, field_limit).await;
```

---

### Phase 4: Evaluation and Monitoring (P2 - Ongoing)

**Goal:** Continuous quality measurement

#### Task 4.1: Expand Test Coverage
**Owner:** Developer
**Effort:** 3-4 hours
**File:** `tests/field_vector_store_rag_tests.rs`

**Add Tests For:**
1. Common query patterns:
   - Time-series: "show clicks over the last 30 days"
   - Comparisons: "compare cost across campaigns"
   - Ratios: "click-through rate by device"

2. Edge cases:
   - Misspellings: "converstions" → should still find conversions
   - Synonyms: "expenses" → should find cost fields
   - Abbreviations: "CTR" → should find click_through_rate

3. Multi-resource queries:
   - "campaign name and ad group performance"
   - Should return fields from campaign AND ad_group resources

**Implementation Pattern:**
```rust
#[tokio::test]
async fn test_time_series_query() {
    let vector_store = get_test_field_vector_store().await.unwrap();
    let query = "show impressions over the last 30 days";
    let results = search_vector_store(&vector_store, query, 15).await.unwrap();

    let retrieved: Vec<String> = results.iter().map(|(_, _, doc)| doc.id.clone()).collect();

    // Must include time dimension
    assert!(retrieved.iter().any(|f| f.contains("segments.date")),
            "Time-series query must include date segment");

    // Must include the metric
    assert!(retrieved.iter().any(|f| f.contains("metrics.impressions")),
            "Query must include requested metric");

    // Check precision
    let expected: HashSet<String> = [
        "segments.date",
        "segments.week",
        "segments.month",
        "metrics.impressions"
    ].iter().map(|s| s.to_string()).collect();

    let precision = calculate_precision(&retrieved, &expected);
    assert!(precision >= 0.25, "Precision should be >= 0.25 for time-series query");
}
```

#### Task 4.2: Add Metrics Logging
**Owner:** Developer
**Effort:** 2 hours
**File:** `src/prompt2gaql.rs`

**Log for Analysis:**
```rust
pub async fn prompt(&self, user_query: &str) -> Result<String, anyhow::Error> {
    let start = std::time::Instant::now();

    // ... existing RAG retrieval ...

    // LOG METRICS
    log::info!("RAG Metrics for query '{}': {{ \
                retrieved_fields: {}, \
                avg_score: {:.3}, \
                max_score: {:.3}, \
                min_score: {:.3}, \
                retrieval_time_ms: {} \
                }}",
               user_query,
               relevant_fields.len(),
               avg_score,
               max_score,
               min_score,
               start.elapsed().as_millis());

    // ... continue with LLM call ...
}
```

**Use Logs For:**
- Identifying poorly performing query patterns
- Tuning similarity thresholds
- Detecting slow retrievals
- A/B testing improvements

---

## Success Metrics

### Phase 1 Completion
- [ ] All similarity scores in valid range [0, 1]
- [ ] Scores properly ordered (descending)
- [ ] No mathematical errors in scoring

### Phase 2 Completion
- [ ] Field descriptions average > 20 words (vs current ~4)
- [ ] Precision for basic queries > 50% (vs current 0%)
- [ ] Recall for basic queries > 30% (vs current 0%)

### Phase 3 Completion
- [ ] F1 score for test queries > 0.40
- [ ] < 5% false positive rate for vague queries
- [ ] Average retrieval time < 500ms

### Phase 4 Completion
- [ ] Test coverage > 80% of common query patterns
- [ ] Continuous monitoring in production logs
- [ ] Monthly RAG quality reports

---

## Rollback Plan

If Phase 2 (rich descriptions) degrades performance:

1. **Revert to simple descriptions but keep other fixes:**
   ```bash
   git revert <commit-hash-for-descriptions>
   ```

2. **Try intermediate approach:**
   - Keep category + data type
   - Remove purpose inference
   - Test if partial richness helps

3. **Alternative: Different embedding model:**
   - Switch from `AllMiniLML6V2Q` to larger model
   - May compensate for simpler descriptions

---

## Timeline

| Phase | Tasks | Effort | Target Completion |
|-------|-------|--------|------------------|
| Phase 1 | Fix scoring bugs | 4-6 hours | Week 1 |
| Phase 2 | Rich descriptions | 3-4 hours | Week 1 |
| Phase 3 | Quality gates | 3-5 hours | Week 2 |
| Phase 4 | Monitoring | 5-6 hours | Week 3 |

**Total Effort:** ~15-21 hours
**Timeline:** 3 weeks (1 sprint)

---

## Dependencies

- **No external dependencies** - all fixes are code changes
- **No API changes** - internal implementation only
- **Requires embedding rebuild** - one-time 15-minute operation

---

## Risks

| Risk | Impact | Mitigation |
|------|--------|------------|
| Rich descriptions degrade quality | HIGH | A/B test with simple vs rich; rollback plan ready |
| Similarity threshold too strict | MEDIUM | Make configurable; tune based on testing |
| Embedding rebuild time in production | LOW | Pre-compute and cache before deployment |
| Scoring fix breaks existing queries | MEDIUM | Comprehensive testing before merge |

---

## Appendix A: Test Command Reference

```bash
# Run all RAG tests
cargo test --test field_vector_store_rag_tests -- --nocapture

# Run single test with debug output
cargo test --test field_vector_store_rag_tests test_field_retrieval_for_cost_metrics -- --nocapture

# Run tests with specific filter
cargo test --test field_vector_store_rag_tests -- --nocapture precision

# Enable debug logging for RAG internals
RUST_LOG=debug cargo test --test field_vector_store_rag_tests -- --nocapture
```

---

## Appendix B: Related Files

| File | Purpose | Changes Needed |
|------|---------|---------------|
| `src/prompt2gaql.rs:164-252` | Vector store building | Phase 1: Verify scoring |
| `src/prompt2gaql.rs:288-375` | Field description generation | Phase 2: Restore rich descriptions |
| `src/prompt2gaql.rs:620-653` | RAG retrieval | Phase 3: Add filtering |
| `src/prompt2gaql.rs:775-829` | LLM prompt assembly | Phase 3: Dynamic limits |
| `src/lancedb_utils.rs` | LanceDB persistence | Phase 1: Check index config |
| `tests/field_vector_store_rag_tests.rs` | Integration tests | Phase 4: Expand coverage |

---

## Appendix C: Example Query Expectations

**After fixes, these should work:**

| Query | Expected Top 5 Fields |
|-------|----------------------|
| "cost per click" | `metrics.average_cpc`, `metrics.cost_micros`, `metrics.cost_per_conversion`, `metrics.cost_per_all_conversions`, `campaign.target_cpa` |
| "conversions" | `metrics.conversions`, `metrics.conversions_value`, `metrics.all_conversions`, `metrics.conversion_rate`, `segments.conversion_action` |
| "impressions and clicks" | `metrics.impressions`, `metrics.clicks`, `metrics.ctr`, `metrics.interactions`, `metrics.interaction_rate` |
| "campaign budget" | `campaign.budget_amount_micros`, `campaign_budget.amount_micros`, `campaign_budget.recommended_budget_amount_micros` |
| "video views" | `metrics.video_views`, `metrics.video_view_rate`, `metrics.video_quartile_p25_rate`, `metrics.video_quartile_p50_rate` |

---

**Document Version:** 1.0
**Last Updated:** 2024-11-10
**Next Review:** After Phase 1 completion

# RAG-Based Resource Selection Enhancement (v2)

## Context

Currently, the `select_resource` function in `rag.rs` includes **all available Google Ads resources** (100+) in the LLM prompt for resource selection. This approach is:

- **Inefficient**: LLM processes a large, mostly-irrelevant context
- **Expensive**: More tokens consumed per request
- **Less accurate**: Noise from irrelevant resources may confuse the LLM

The codebase already has a working RAG infrastructure (LanceDB, embeddings, vector search) used for field selection and query cookbook retrieval. This enhancement extends that pattern to resource selection.

## Goals

1. **Improve accuracy**: Surface only semantically relevant resources to the LLM
2. **Reduce token usage**: ~15-20 relevant resources vs 100+ total resources
3. **Intent-aware filtering**: Distinguish performance queries (need metrics) from structural queries
4. **Fallback safety**: Ensure critical resources aren't missed via confidence threshold fallback

## Key Design Decisions

| Aspect | Decision | Rationale |
|--------|----------|-----------|
| Embedding strategy | Hierarchical (resource + field level) | Fields have richer descriptions; aggregate to resources |
| Search order | Fields first → aggregate to resources | More granular matching, better semantic alignment |
| Embedding content | ALL fields per resource with descriptions | key_attributes/metrics are too sparse |
| Intent detection | Keyword-based | Fast, no LLM latency, sufficient for metrics vs structural |
| Metrics filtering | Pre-filter during vector search | Remove metric-less resources when performance intent detected |
| Fallback | Full resource list | When top similarity < threshold |
| Rollout | Direct replacement | No feature flag needed |
| Validation | query_cookbook entries | Existing entries serve as ground truth |

---

## Architecture

### Phase 1: Query Intent Classification (Keyword-Based)

Detect whether user wants performance data or structural data.

```rust
/// Performance-related keywords indicating metrics are needed
const PERFORMANCE_KEYWORDS: &[&str] = &[
    "clicks", "impressions", "views", "conversions", "revenue",
    "cost", "spend", "cpc", "cpm", "ctr", "roas",
    "performance", "performing", "trends", "report", "analytics",
    "compare", "growth", "decline", "increase", "decrease",
    "last week", "last month", "yesterday", "date range",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QueryIntent {
    Performance,  // Needs resources with metrics
    Structural,   // Settings, configurations, attributes only
    Unknown,      // Could be either
}

impl QueryIntent {
    pub fn classify(query: &str) -> Self {
        let query_lower = query.to_lowercase();
        let has_performance_keyword = PERFORMANCE_KEYWORDS
            .iter()
            .any(|kw| query_lower.contains(kw));

        if has_performance_keyword {
            QueryIntent::Performance
        } else {
            QueryIntent::Unknown // Don't filter; include all
        }
    }
}
```

### Phase 2: Hierarchical Vector Store

Two LanceDB tables for hierarchical search.

#### Table 1: `resource_entries`

Resource-level embeddings for coarse filtering and diversity sampling.

```rust
#[derive(Embed, Clone, Serialize, Deserialize, Debug)]
pub struct ResourceEntryEmbed {
    #[embed]
    pub text: String,              // Embedding text (resource summary)
    pub resource_name: String,     // Primary key: "campaign", "ad_group", etc.
    pub category: String,          // "Campaign Resources", "Ad Group Resources", etc.
    pub description: String,       // Human-readable description
    pub has_metrics: bool,         // Whether this resource supports metrics.*
    pub field_count: usize,        // Number of fields (for ranking tiebreaker)
}

impl ResourceEntryEmbed {
    pub fn from_resource_metadata(
        name: &str,
        metadata: &ResourceMetadata,
        fields: &[FieldMetadata],
    ) -> Self {
        // Derive has_metrics from key_metrics or field names
        let has_metrics = !metadata.key_metrics.is_empty()
            || fields.iter().any(|f| f.field_name.starts_with("metrics."));

        let text = format!(
            "Resource: {}. Category: {}. Description: {}. \
             Field count: {}. Has metrics: {}. \
             Sample fields: {}.",
            name,
            categorize_resource(name),
            metadata.description.as_deref().unwrap_or(""),
            fields.len(),
            has_metrics,
            fields.iter().take(10).map(|f| &f.field_name).join(", "),
        );

        Self {
            text,
            resource_name: name.to_string(),
            category: categorize_resource(name),
            description: metadata.description.clone().unwrap_or_default(),
            has_metrics,
            field_count: fields.len(),
        }
    }
}
```

#### Table 2: `field_entries`

Field-level embeddings for fine-grained semantic search.

```rust
#[derive(Embed, Clone, Serialize, Deserialize, Debug)]
pub struct FieldEntryEmbed {
    #[embed]
    pub text: String,              // Embedding text (field details)
    pub field_name: String,        // Full field name: "campaign.name"
    pub resource_name: String,     // Parent resource: "campaign"
    pub description: String,       // Field description
    pub field_type: String,        // "ATTRIBUTE", "METRIC", "SEGMENT"
    pub data_type: String,         // "STRING", "INT64", "ENUM", etc.
}

impl FieldEntryEmbed {
    pub fn from_field_metadata(field: &FieldMetadata) -> Self {
        let text = format!(
            "Field: {}. Resource: {}. Type: {}. Data type: {}. \
             Description: {}. Enum values: {}.",
            field.field_name,
            field.resource_name,
            field.field_type,
            field.data_type,
            field.description.as_deref().unwrap_or(""),
            field.enum_values.as_ref().map(|v| v.join(", ")).unwrap_or_default(),
        );

        Self {
            text,
            field_name: field.field_name.clone(),
            resource_name: field.resource_name.clone(),
            description: field.description.clone().unwrap_or_default(),
            field_type: field.field_type.clone(),
            data_type: field.data_type.clone(),
        }
    }
}
```

### Phase 3: Search and Aggregation

#### Search Strategy: Fields First → Aggregate to Resources

```rust
pub struct ResourceSearchResult {
    pub resource_name: String,
    pub score: f64,
    pub has_metrics: bool,
    pub category: String,
    pub description: String,
    pub matching_fields: Vec<String>,  // Fields that matched the query
}

impl RAGAgent {
    /// Main entry point for RAG-based resource selection
    pub async fn retrieve_relevant_resources(
        &self,
        query: &str,
        top_n: usize,
    ) -> Result<Vec<ResourceSearchResult>> {
        // Step 1: Classify query intent
        let intent = QueryIntent::classify(query);

        // Step 2: Search field embeddings
        let field_results = self.search_field_embeddings(query, top_n * 5).await?;

        // Step 3: Aggregate field scores to resources
        let mut resource_scores: HashMap<String, ResourceScoreAccumulator> = HashMap::new();

        for field_result in field_results {
            let entry = resource_scores
                .entry(field_result.resource_name.clone())
                .or_insert_with(|| ResourceScoreAccumulator::new());

            entry.add_field_match(field_result.field_name, field_result.score);
        }

        // Step 4: Fetch resource metadata and apply intent filter
        let mut results: Vec<ResourceSearchResult> = Vec::new();

        for (resource_name, accumulator) in resource_scores {
            let resource_meta = self.get_resource_metadata(&resource_name).await?;

            // Filter: if performance intent, skip metric-less resources
            if intent == QueryIntent::Performance && !resource_meta.has_metrics {
                continue;
            }

            results.push(ResourceSearchResult {
                resource_name,
                score: accumulator.aggregate_score(),
                has_metrics: resource_meta.has_metrics,
                category: resource_meta.category,
                description: resource_meta.description,
                matching_fields: accumulator.matching_fields,
            });
        }

        // Step 5: Sort by score and take top N
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(top_n);

        // Step 6: Add diversity samples if needed
        self.ensure_category_diversity(&mut results, top_n).await?;

        Ok(results)
    }

    /// Ensure at least one resource from each major category
    async fn ensure_category_diversity(
        &self,
        results: &mut Vec<ResourceSearchResult>,
        max_total: usize,
    ) -> Result<()> {
        let covered_categories: HashSet<_> = results.iter()
            .map(|r| r.category.clone())
            .collect();

        let all_categories = vec![
            "Campaign Resources",
            "Ad Group Resources",
            "Ad Resources",
            "Keyword Resources",
            "Audience Resources",
            "Extension Resources",
        ];

        for category in all_categories {
            if !covered_categories.contains(category) && results.len() < max_total {
                if let Some(sample) = self.sample_from_category(category).await? {
                    results.push(sample);
                }
            }
        }

        Ok(())
    }
}
```

#### Score Aggregation

```rust
struct ResourceScoreAccumulator {
    field_scores: Vec<f64>,
    matching_fields: Vec<String>,
}

impl ResourceScoreAccumulator {
    fn new() -> Self {
        Self {
            field_scores: Vec::new(),
            matching_fields: Vec::new(),
        }
    }

    fn add_field_match(&mut self, field_name: String, score: f64) {
        self.field_scores.push(score);
        self.matching_fields.push(field_name);
    }

    /// Aggregate strategy: weighted sum favoring top matches
    fn aggregate_score(&self) -> f64 {
        if self.field_scores.is_empty() {
            return 0.0;
        }

        let mut sorted_scores = self.field_scores.clone();
        sorted_scores.sort_by(|a, b| b.partial_cmp(a).unwrap());

        // Top match contributes 50%, rest contribute diminishing amounts
        let mut total = 0.0;
        let mut weight = 0.5;

        for score in sorted_scores.iter().take(5) {
            total += score * weight;
            weight *= 0.5;
        }

        total
    }
}
```

### Phase 4: Confidence Threshold and Fallback

```rust
const SIMILARITY_THRESHOLD: f64 = 0.5;

impl RAGAgent {
    pub async fn select_resource_with_rag(
        &self,
        context: &QueryContext,
        field_cache: &FieldMetadataCache,
    ) -> Result<ResourceSelection> {
        // Attempt RAG-based retrieval
        let candidates = self.retrieve_relevant_resources(
            &context.user_query,
            20, // top N
        ).await?;

        // Check confidence
        let top_score = candidates.first().map(|r| r.score).unwrap_or(0.0);

        if top_score < SIMILARITY_THRESHOLD {
            log::warn!(
                "Low RAG confidence ({:.2}), falling back to full resource list",
                top_score
            );
            return self.select_resource_full(context, field_cache).await;
        }

        // Build reduced prompt with candidates
        self.select_from_candidates(context, candidates).await
    }
}
```

---

## LLM Prompt Changes

### Before (current)

```
Choose from the following resources (organized by category):
### Campaign Resources
- campaign: A campaign is a ...
- campaign_budget: A campaign budget is ...
[100+ more resources...]
```

### After (with RAG)

```
Choose from the following semantically relevant resources:

### Campaign Resources
- campaign: A campaign is a ... [matched fields: campaign.name, campaign.status]
- campaign_budget: A campaign budget is ... [matched fields: campaign_budget.amount_micros]

### Ad Group Resources
- ad_group: An ad group is a ... [matched fields: ad_group.cpc_bid_micros]

[15-20 relevant resources total]

Note: Resources were selected based on semantic similarity to your query.
If the exact resource you need is not listed, describe it and I will search more broadly.
```

---

## Implementation Plan

### Files to Modify

| File | Changes |
|------|---------|
| `crates/mcc-gaql-gen/src/rag.rs` | Add `QueryIntent`, `retrieve_relevant_resources()`, modify `select_resource()` |
| `crates/mcc-gaql-gen/src/vector_store.rs` | Add `resource_entries` and `field_entries` table schemas |
| `crates/mcc-gaql-gen/src/field_metadata.rs` | Add `has_metrics` derivation to `ResourceMetadata` |

### New Structs

- `QueryIntent` - enum for query classification
- `ResourceEntryEmbed` - resource-level embedding record
- `FieldEntryEmbed` - field-level embedding record
- `ResourceSearchResult` - aggregated search result
- `ResourceScoreAccumulator` - score aggregation helper

### Implementation Order

1. **Add `has_metrics` flag** to `ResourceMetadata` (derive from key_metrics / field names)
2. **Create `FieldEntryEmbed`** struct and table initialization
3. **Create `ResourceEntryEmbed`** struct and table initialization
4. **Implement `QueryIntent::classify()`** with keyword matching
5. **Implement `search_field_embeddings()`** - vector search on field table
6. **Implement `retrieve_relevant_resources()`** - aggregation logic
7. **Implement `ensure_category_diversity()`** - diversity sampling
8. **Modify `select_resource()`** to use RAG with fallback
9. **Update prompt construction** to show matched fields
10. **Validate against query_cookbook** entries

---

## Validation Strategy

### Test Cases from query_cookbook

Use existing query_cookbook entries as ground truth:
- Extract (query, expected_resource) pairs from cookbook
- Run RAG retrieval on each query
- Verify expected resource appears in top N candidates
- Measure: hit rate, average rank of correct resource, token reduction

### Metrics to Track

| Metric | Target |
|--------|--------|
| Correct resource in top 5 | > 95% |
| Correct resource in top 10 | > 99% |
| Average candidate set size | 15-20 resources |
| Token reduction vs baseline | > 70% |
| Fallback rate (low confidence) | < 10% |

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Relevant resource missed | Diversity sampling + confidence threshold fallback |
| Field descriptions too sparse | Use full field metadata including enum values |
| Performance intent misclassified | Conservative keyword list; Unknown intent includes all |
| RAG index stale | Rebuild on metadata cache refresh |
| Search latency too high | Field search is parallel; aggregate in memory |

---

## Future Enhancements

1. **Hybrid search**: Combine vector similarity with BM25 keyword matching
2. **Query expansion**: Use LLM to expand query before embedding
3. **Feedback loop**: Track which resources are actually selected, re-rank
4. **Multi-hop retrieval**: If initial selection yields poor results, expand search
5. **Resource relationships**: Embed `selectable_with` graph for related resource discovery

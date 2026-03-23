# RAG-Based Resource Selection Enhancement (v3)

## Changes from v2

| Item | v2 Assumption | v3 Adjustment | Rationale |
|------|---------------|---------------|-----------|
| `has_metrics` derivation | Use `key_metrics` field | Use `selectable_with.iter().any(\|f\| f.starts_with("metrics."))` | `key_metrics` is empty for 99.4% of resources; `selectable_with` covers 38% (69/181) which are the performance-relevant ones |
| Field source for embeddings | Use `key_attributes` + `key_metrics` | Query fields map for `{resource}.*` prefix | `key_attributes` is empty for 100% of resources |
| Sample fields in resource embedding | From `key_attributes`/`key_metrics` | From fields map filtered by resource prefix | Same reason as above |
| Table structure | Two new tables | Add `resource_entries` table; reuse existing `field_metadata` table | `field_metadata` already has embeddings with correct schema |

## Context

Currently, `select_resource` in `rag.rs` includes **all 181 Google Ads resources** in the LLM prompt. This is inefficient and expensive. The codebase has working RAG infrastructure (LanceDB, fastembed) that can be extended to resource selection.

## Data Availability (Verified)

| Data | Count | Coverage |
|------|-------|----------|
| Resources in `resource_metadata` | 181 | 100% have descriptions |
| Fields in `fields` map | 2,906 | All have `description` and `usage_notes` |
| Resources with metrics in `selectable_with` | 69 | 38% (sufficient for performance filtering) |
| Existing `field_metadata` LanceDB table | 1 | Has embeddings, can be queried |

## Architecture

### Phase 1: Query Intent Classification

Unchanged from v2 - keyword-based detection.

```rust
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
    Unknown,      // Could be either - don't filter
}

impl QueryIntent {
    pub fn classify(query: &str) -> Self {
        let query_lower = query.to_lowercase();
        if PERFORMANCE_KEYWORDS.iter().any(|kw| query_lower.contains(kw)) {
            QueryIntent::Performance
        } else {
            QueryIntent::Unknown
        }
    }
}
```

### Phase 2: Resource Embeddings Table

New table `resource_entries` alongside existing `field_metadata`.

```rust
/// Schema for resource entries table
pub fn resource_entries_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("resource_name", DataType::Utf8, false),  // Primary key
        Field::new("category", DataType::Utf8, false),       // "Campaign Resources", etc.
        Field::new("description", DataType::Utf8, false),    // From resource_metadata
        Field::new("has_metrics", DataType::Boolean, false), // Derived from selectable_with
        Field::new("field_count", DataType::Int32, false),   // Number of fields
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float64, true)),
                EMBEDDING_DIM,
            ),
            false,
        ),
    ]))
}

/// Build embedding text for a resource
fn build_resource_embedding_text(
    resource_name: &str,
    resource_meta: &ResourceMetadata,
    fields: &HashMap<String, FieldMetadata>,
) -> String {
    // Get fields belonging to this resource
    let resource_fields: Vec<&FieldMetadata> = fields
        .values()
        .filter(|f| f.get_resource().as_deref() == Some(resource_name))
        .collect();

    // Sample up to 10 field names for context
    let sample_fields: Vec<&str> = resource_fields
        .iter()
        .take(10)
        .map(|f| f.name.as_str())
        .collect();

    // Check for metrics support
    let has_metrics = resource_meta.selectable_with
        .iter()
        .any(|f| f.starts_with("metrics."));

    format!(
        "Resource: {}. Category: {}. Description: {}. \
         Has metrics: {}. Field count: {}. \
         Sample fields: {}.",
        resource_name,
        categorize_resource(resource_name),
        resource_meta.description.as_deref().unwrap_or(""),
        has_metrics,
        resource_fields.len(),
        sample_fields.join(", "),
    )
}
```

### Phase 3: Search Strategy

**Two-tier search**: Resource-level first, then field-level for refinement.

```rust
pub struct ResourceSearchResult {
    pub resource_name: String,
    pub score: f64,
    pub has_metrics: bool,
    pub category: String,
    pub description: String,
}

impl RAGAgent {
    /// Main entry point for RAG-based resource selection
    pub async fn retrieve_relevant_resources(
        &self,
        query: &str,
        top_n: usize,
    ) -> Result<Vec<ResourceSearchResult>> {
        let intent = QueryIntent::classify(query);

        // Step 1: Search resource embeddings directly
        let mut results = self.search_resource_embeddings(query, top_n * 2).await?;

        // Step 2: Apply intent filter
        if intent == QueryIntent::Performance {
            results.retain(|r| r.has_metrics);
        }

        // Step 3: Truncate to top_n
        results.truncate(top_n);

        // Step 4: Ensure category diversity
        self.ensure_category_diversity(&mut results, top_n).await?;

        Ok(results)
    }

    async fn search_resource_embeddings(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ResourceSearchResult>> {
        // Embed query
        let query_embedding = self.embed_text(query).await?;

        // Search resource_entries table
        let table = open_table(&self.db, "resource_entries").await?;

        let results = table
            .vector_search(query_embedding.vec.clone())?
            .limit(limit)
            .execute()
            .await?;

        // Convert to ResourceSearchResult
        let mut search_results = Vec::new();
        for row in results {
            search_results.push(ResourceSearchResult {
                resource_name: row.get("resource_name")?,
                score: row.get("_distance")?,
                has_metrics: row.get("has_metrics")?,
                category: row.get("category")?,
                description: row.get("description")?,
            });
        }

        Ok(search_results)
    }
}
```

### Phase 4: Confidence Threshold and Fallback

```rust
const SIMILARITY_THRESHOLD: f64 = 0.5;

impl RAGAgent {
    pub async fn select_resource_with_rag(
        &self,
        user_query: &str,
    ) -> Result<(String, Vec<ResourceSearchResult>)> {
        let candidates = self.retrieve_relevant_resources(user_query, 20).await?;

        let top_score = candidates.first().map(|r| r.score).unwrap_or(0.0);

        if top_score < SIMILARITY_THRESHOLD || candidates.is_empty() {
            log::warn!(
                "Low RAG confidence ({:.2}), falling back to full resource list",
                top_score
            );
            return self.select_resource_full(user_query).await;
        }

        // Use reduced prompt with candidates
        self.select_from_candidates(user_query, candidates).await
    }
}
```

## LLM Prompt Changes

### Before (current)

```
Choose from the following resources (organized by category):
### CAMPAIGN RESOURCES
- campaign: A campaign is a ...
- campaign_budget: A campaign budget is ...
[181 resources total...]
```

### After (with RAG)

```
Choose from the following semantically relevant resources:

### Campaign Resources
- campaign: A campaign is a ... [has_metrics: true]
- campaign_budget: A campaign budget is ... [has_metrics: false]

### Ad Group Resources
- ad_group: An ad group is a ... [has_metrics: true]

[15-20 relevant resources total]

Note: Resources selected by semantic similarity. If needed resource is missing, describe it for broader search.
```

## Implementation Plan

### Files to Modify

| File | Changes |
|------|---------|
| `vector_store.rs` | Add `resource_entries_schema()`, `resources_to_record_batch()`, `build_or_load_resource_vector_store()` |
| `rag.rs` | Add `QueryIntent`, `retrieve_relevant_resources()`, modify `select_resource()` |
| `field_metadata.rs` | Add `has_metrics()` method to `ResourceMetadata` |

### Implementation Order

1. **Add `has_metrics()` to `ResourceMetadata`**
   ```rust
   impl ResourceMetadata {
       pub fn has_metrics(&self) -> bool {
           self.selectable_with.iter().any(|f| f.starts_with("metrics."))
       }
   }
   ```

2. **Add resource embedding schema and conversion** in `vector_store.rs`

3. **Add `build_or_load_resource_vector_store()`** - parallel to existing field version

4. **Add `QueryIntent` enum** with keyword classification

5. **Add `retrieve_relevant_resources()`** - vector search + intent filter

6. **Add `ensure_category_diversity()`** - sample from underrepresented categories

7. **Modify `select_resource()`** to use RAG with fallback

8. **Update prompt construction** to show `has_metrics` flag

9. **Validate against `query_cookbook`** entries

### New Hash File

Add `resource_entries.hash` alongside existing `field_metadata.hash` and `query_cookbook.hash`.

## Validation Strategy

### Extract Ground Truth from query_cookbook

Parse TOML entries to get `(description, expected_resource)` pairs:

```rust
fn extract_validation_cases(cookbook: &[QueryEntry]) -> Vec<(String, String)> {
    cookbook.iter().filter_map(|entry| {
        // Parse FROM clause to get expected resource
        let resource = extract_from_clause(&entry.query)?;
        Some((entry.description.clone(), resource))
    }).collect()
}
```

### Metrics to Track

| Metric | Target |
|--------|--------|
| Correct resource in top 5 | > 95% |
| Correct resource in top 10 | > 99% |
| Average candidate set size | 15-20 |
| Token reduction vs baseline | > 70% |
| Fallback rate | < 10% |

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Resource missed by RAG | Category diversity sampling + confidence fallback |
| 62% resources lack metrics flag | OK - these are structural resources, filtered only for Performance intent |
| Resource descriptions too generic | Embedding includes sample field names for disambiguation |
| Stale embeddings | Rebuild when `resource_metadata` hash changes |

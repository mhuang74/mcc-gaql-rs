# RAG-Based Resource Selection Enhancement

## Context

Currently, the `select_resource` function in `rag.rs` includes **all available Google Ads resources** in the LLM prompt for resource selection. As the number of Google Ads resources grows (50+ resources), this approach becomes:

- **Inefficient**: LLM processes a large, mostly-irrelevant context
- **Expensive**: More tokens consumed per request
- **Less accurate**: Noise from irrelevant resources may confuse the LLM

The codebase already has a working RAG infrastructure (LanceDB, embeddings, vector search) used for field selection and query cookbook retrieval. This enhancement extends that pattern to resource selection.

## Goals

1. **Improve accuracy**: Surface only semantically relevant resources to the LLM
2. **Reduce token usage**: ~10-15 relevant resources vs 50+ total resources
3. **Maintain speed**: Vector search is fast; overall latency should improve
4. **Fallback safety**: Ensure critical resources aren't missed via keyword matching and diversity sampling

## Current Implementation

The resource selection flow in `rag.rs:select_resource()` currently:

1. Calls `build_categorized_resource_list()` - gets ALL resources from `FieldMetadataCache`
2. Calls `create_resource_sample()` - samples 5 resources based on keyword matching
3. Constructs prompt with complete categorized resource list
4. LLM selects primary and related resources

## Proposed Implementation

### New Component: Resource Vector Store

Create a new LanceDB table for resource embeddings, similar to existing `query_cookbook` and `field_metadata` tables.

**Schema** (`ResourceEntryEmbed`):
```rust
#[derive(Embed, Clone, Serialize, Deserialize, Debug)]
struct ResourceEntryEmbed {
    #[embed]
    text: String,           // Searchable embedding text
    resource_name: String,  // Primary key
    category: String,       // Campaign Resources, Ad Group Resources, etc.
    description: String,    // Human-readable description
}
```

**Embedding text construction** (similar to `FieldMetadata::build_embedding_text`):
```rust
impl ResourceEntryEmbed {
    fn from_resource_metadata(metadata: &ResourceMetadata) -> Self {
        let text = format!(
            "Resource: {}. Category: {}. Description: {}. \
             Key attributes: {}. Key metrics: {}. \
             Compatible with: {}",
            metadata.name,
            categorize_resource(&metadata.name),
            metadata.description.as_deref().unwrap_or(""),
            metadata.key_attributes.join(", "),
            metadata.key_metrics.join(", "),
            metadata.selectable_with.join(", ")
        );
        Self { /* ... */ }
    }
}
```

### Modified Flow: select_resource_with_rag()

Replace or augment `select_resource` with a RAG-aware version:

```rust
async fn select_resource_with_rag(
    &self,
    context: &QueryContext,
    field_cache: &FieldMetadataCache,
) -> Result<ResourceSelection> {
    // Phase 1: RAG Search for relevant resources
    let relevant_resources = self.retrieve_relevant_resources(
        &context.user_query,
        field_cache,
        15, // top N resources
    ).await?;

    // Phase 2: Include sampled "important" resources (diversity)
    let important_resources = self.sample_important_resources(
        field_cache,
        &relevant_resources.iter().map(|r| r.resource_name.clone()).collect::<Vec<_>>(),
        5, // ensure coverage
    );

    // Phase 3: Build resource list from merged results
    let candidate_resources = self.merge_and_deduplicate(
        relevant_resources,
        important_resources,
    );

    // Phase 4: LLM selection from reduced candidate set
    self.select_from_candidates(context, candidate_resources).await
}
```

### Key Implementation Details

**1. Vector Store Initialization** (in `RAGAgent::new()`):
- Check if `resource_entries` table exists in LanceDB
- If not, build from `FieldMetadataCache::resource_metadata`
- Use same BGESmallENV15 model (384 dimensions)

**2. RAG Retrieval Strategy** (`retrieve_relevant_resources`):
- Embed user query using `EmbeddingModel`
- Vector search against `resource_entries` table
- Return top N (configurable, default 15) with similarity scores

**3. Hybrid Search Enhancement** (future consideration):
```rust
// Combine vector similarity with keyword matching
fn hybrid_resource_score(&self, query: &str, resource: &ResourceMetadata) -> f64 {
    let vector_score = self.vector_similarity(query, resource);
    let keyword_score = self.keyword_match_score(query, resource);

    // Weighted combination
    0.7 * vector_score + 0.3 * keyword_score
}
```

**4. Diversity Sampling**:
- Ensure at least one resource from each major category is represented
- Prevents over-concentration in one domain (e.g., all campaign resources)

**5. Confidence Threshold**:
```rust
// If top resource similarity is below threshold, fallback to all resources
const SIMILARITY_THRESHOLD: f64 = 0.6;

if top_score < SIMILARITY_THRESHOLD {
    log::warn!("Low RAG confidence ({:.2}), falling back to full resource list", top_score);
    return self.select_resource_full(context, field_cache).await;
}
```

### Integration Points

**Modified Files**:
1. `crates/mcc-gaql-gen/src/rag.rs`:
   - Add `ResourceEntryEmbed` struct
   - Add `initialize_resource_vector_store()` method
   - Add `retrieve_relevant_resources()` method
   - Modify `select_resource()` to use RAG (or create `select_resource_with_rag()`)

2. `crates/mcc-gaql-gen/src/vector_store.rs`:
   - Add `resource_entries` table schema
   - Add `create_resource_table()` function

**Configuration**:
```rust
// In RAGConfig or similar
pub struct ResourceSelectionConfig {
    /// Enable RAG-based resource filtering
    pub use_rag: bool,
    /// Number of top resources to retrieve via RAG
    pub rag_top_n: usize,
    /// Similarity threshold for fallback
    pub similarity_threshold: f64,
    /// Number of diversity samples to include
    pub diversity_samples: usize,
}
```

### Example Resource Embedding Text

```
Resource: campaign. Category: Campaign Resources. Description: A campaign is a ...
Key attributes: campaign.id, campaign.name, campaign.status, campaign.advertising_channel_type.
Key metrics: metrics.cost_micros, metrics.clicks, metrics.impressions, metrics.conversions.
Compatible with: ad_group, ad_group_ad, keyword_view, campaign_budget.
```

### LLM Prompt Changes

**Before** (current):
```
Choose from the following resources (organized by category):
### Campaign Resources
- campaign: A campaign is a ...
- campaign_budget: A campaign budget is ...
[50+ more resources...]
```

**After** (with RAG):
```
Choose from the following semantically relevant resources (organized by category):
### Campaign Resources
- campaign: A campaign is a ...
- campaign_budget: A campaign budget is ...
[10-15 relevant resources...]

Note: Resources were selected based on semantic similarity to your query. If the needed resource is not listed, the system will search more broadly.
```

## Benefits

1. **Token reduction**: ~50 resources → ~15 resources (~70% reduction)
2. **Latency improvement**: Less context = faster LLM inference
3. **Accuracy improvement**: Reduced noise from irrelevant resources
4. **Scalable**: As Google Ads adds more resources, RAG naturally filters them

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| Relevant resource missed | Diversity sampling + keyword matching backup + confidence threshold fallback |
| RAG index stale | Rebuild on metadata cache refresh (same as field_metadata) |
| New resources unknown | Include in diversity sampling until embedded |
| Semantic mismatch | Hybrid search (vector + keyword) for robustness |

## Testing Strategy

1. **Unit tests**:
   - Resource embedding text generation
   - Vector search with known queries
   - Diversity sampling logic

2. **Integration tests**:
   - End-to-end query → resource selection with known expected resources
   - Fallback trigger when confidence is low

3. **Evaluation benchmark**:
   - Create test set of 50+ natural language queries
   - Compare baseline (all resources) vs RAG (filtered resources)
   - Measure: correct resource selected, token usage, latency

## Migration Path

1. Implement alongside existing code (feature flag)
2. A/B test on sample queries
3. Gradual rollout with monitoring
4. Remove legacy path once validated

## Future Enhancements

1. **Multi-hop RAG**: If initial resource selection yields poor results, expand search
2. **User feedback loop**: Track which resources are actually used, re-rank based on usage
3. **Cross-resource relationships**: Embed relationship graph for better context
4. **Query intent classification**: Route different query types to specialized resource sets

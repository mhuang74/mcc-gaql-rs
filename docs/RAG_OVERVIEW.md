# RAG (Retrieval-Augmented Generation) Overview for mcc-gaql-gen

## Table of Contents
1. [Introduction](#introduction)
2. [Architecture Overview](#architecture-overview)
3. [Data Sources](#data-sources)
4. [Vector Stores](#vector-stores)
5. [The `metadata` Command](#the-metadata-command)
6. [The `generate` Command](#the-generate-command)
7. [Filtering and Scoring](#filtering-and-scoring)
8. [Performance Considerations](#performance-considerations)

---

## Introduction

`mcc-gaql-gen` uses a sophisticated RAG (Retrieval-Augmented Generation) pipeline to convert natural language prompts into valid Google Ads Query Language (GAQL) queries. The system combines:

- **Vector embeddings** for semantic search (using FastEmbed with BAAI/bge-small-en-v1.5 model)
- **LanceDB** for efficient vector storage and retrieval
- **LLM** (OpenAI-compatible API) for intelligent field selection and query generation
- **Enriched metadata** from Google Ads API field definitions

The RAG pipeline enables users to generate GAQL queries without memorizing hundreds of field names, resource relationships, or query syntax.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                      MultiStepRAGAgent                          │
├─────────────────────────────────────────────────────────────────┤
│  Components:                                                    │
│  - field_cache: FieldMetadataCache (enriched metadata)          │
│  - field_index: LanceDbVectorIndex (field embeddings)           │
│  - query_index: LanceDbVectorIndex (cookbook examples)          │
│  - resource_index: LanceDbVectorIndex (resource embeddings)     │
│  - llm_config: LlmConfig (OpenAI-compatible client)             │
│  - domain_knowledge: DomainKnowledge (GAQL rules, date logic)   │
└─────────────────────────────────────────────────────────────────┘
```

### Key Constants

- **SIMILARITY_THRESHOLD**: `0.65` - Minimum similarity score (1.0 - cosine_distance) to trust semantic search results
- **LLM_CATEGORY_LIMIT**: `15` - Maximum fields shown per category in LLM prompts (prevents context overflow)

---

## Data Sources

### 1. Field Metadata Cache (`field_metadata_enriched.json`)

Contains enriched metadata for all Google Ads API fields:

**Structure:**
```rust
FieldMetadataCache {
    fields: HashMap<String, FieldMetadata>,
    resource_metadata: HashMap<String, ResourceMetadata>,
}

FieldMetadata {
    name: String,                    // e.g., "campaign.id"
    data_type: String,               // e.g., "INT64", "STRING"
    category: String,                // "ATTRIBUTE", "METRIC", "SEGMENT"
    is_repeated: bool,
    enum_values: Vec<String>,
    description: Option<String>,     // LLM-enriched or proto-sourced
    usage_notes: Option<String>,     // LLM-generated usage guidance
    key_field: bool,                 // Critical for resource identification
    common_filter: bool,             // Often used in WHERE clauses
}

ResourceMetadata {
    name: String,                    // e.g., "campaign"
    selectable_with: Vec<String>,    // Compatible segments/metrics
    key_attributes: Vec<String>,     // Important fields (LLM-curated)
    key_metrics: Vec<String>,        // Important metrics (LLM-curated)
    description: Option<String>,     // Resource description
    uses_fallback: bool,             // True if alphabetical fallback used
    identity_fields: Vec<String>,    // Fields for uniqueness (e.g., "campaign.id")
}
```

### 2. Query Cookbook (`query_cookbook.toml`)

Contains example GAQL queries with descriptions:

```toml
[[queries]]
name = "enabled_campaigns"
description = "Get all enabled campaigns with their names"
query = """
SELECT campaign.id, campaign.name
FROM campaign
WHERE campaign.status = 'ENABLED'
"""
```

### 3. Domain Knowledge (`resources/domain_knowledge.md`)

Embedded knowledge about:
- Date range logic (THIS_YEAR, LAST_30_DAYS, etc.)
- Monetary field handling (micros conversion)
- Common GAQL patterns and best practices
- Filter operators and syntax rules

---

## Vector Stores

The system maintains three separate LanceDB vector stores:

### 1. **Field Vector Store** (`lancedb/field_metadata/`)

**Purpose:** Semantic search over all Google Ads fields

**Document Structure:**
```rust
FieldDocument {
    name: String,                    // "campaign.status"
    resource: String,                // "campaign"
    category: String,                // "ATTRIBUTE"
    embedding_text: String,          // Synthesized search text
    data_type: String,
    is_repeated: bool,
    enum_values: Vec<String>,
    description: Option<String>,
    usage_notes: Option<String>,
    key_field: bool,
    common_filter: bool,
}
```

**Embedding Text Generation:**
The `embedding_text` field combines:
- Field name (e.g., "campaign.status")
- Description (if available)
- Usage notes (if available)
- Category ("attribute", "metric", "segment")
- Enum values (for filterable fields)

**Example:**
```
Field: campaign.status
Category: attribute
Description: The status of the campaign (ENABLED, PAUSED, REMOVED)
Usage: Filter by campaign.status to get active campaigns
Enums: ENABLED, PAUSED, REMOVED
```

**Cache Validation:**
- Hash of all field names stored in `lancedb/field_metadata/cache_hash.txt`
- Rebuilt if field names change

### 2. **Query Vector Store** (`lancedb/query_examples/`)

**Purpose:** Retrieve similar example queries from cookbook

**Document Structure:**
```rust
QueryEntryEmbed {
    name: String,
    description: String,
    query: String,
}
```

**Embedding:** Uses the `description` field for semantic matching

**Usage:** Phase 3 retrieves top-3 similar examples when `--use-query-cookbook` flag is enabled

**Cache Validation:**
- Hash of cookbook queries stored in `lancedb/query_examples/cache_hash.txt`
- Rebuilt if cookbook changes

### 3. **Resource Vector Store** (`lancedb/resource_entries/`)

**Purpose:** Resource selection (Phase 1)

**Document Structure:**
```rust
ResourceDocument {
    resource_name: String,           // "campaign"
    has_metrics: bool,               // Does resource have metrics?
    category: String,                // Resource category
    description: String,             // LLM-enriched description
    key_attributes: Vec<String>,     // Top attributes
    key_metrics: Vec<String>,        // Top metrics
    identity_fields: Vec<String>,    // Uniqueness fields
}
```

**Embedding Text:**
```
Resource: campaign
Description: [Resource description]
Key Attributes: id, name, status, budget, bidding_strategy
Key Metrics: clicks, impressions, cost_micros, conversions
Identity: campaign.id
Category: primary
```

**Cache Validation:**
- Hash of resource names and metadata stored in `lancedb/resource_entries/cache_hash.txt`
- Rebuilt if resource metadata changes

---

## The `metadata` Command

### Purpose
Display enriched field metadata for debugging and exploration.

### Usage
```bash
mcc-gaql-gen metadata <query> [OPTIONS]
```

### Search Methods

#### 1. **Quick Pattern Matching** (`--quick` or `-q`)

Fast string-based matching without vector search:

**Algorithm:**
```
1. Check if query is exact resource name → return full resource
2. Check if query is exact field name → return single field
3. Pattern match against field names (supports wildcards)
4. Group results by category (ATTRIBUTE, METRIC, SEGMENT)
```

**Examples:**
- `mcc-gaql-gen metadata campaign -q` → Campaign resource with all fields
- `mcc-gaql-gen metadata "metrics.*" -q` → All metrics (pattern match)
- `mcc-gaql-gen metadata "campaign.status" -q` → Single field

#### 2. **Semantic Search** (default)

Uses vector similarity for intelligent matching:

**Algorithm:**
```
1. Check if query is exact resource name → return full resource
2. Check if query is exact field name → return single field
3. Embed the query using FastEmbed model
4. Search field_index vector store (cosine distance)
5. Convert distance to similarity: similarity = 1.0 - distance
6. Filter by SIMILARITY_THRESHOLD (0.65)
7. Group by category and sort by similarity (descending)
8. Return top results with scores
```

**Examples:**
- `mcc-gaql-gen metadata "click metrics"` → Semantic search for click-related metrics
- `mcc-gaql-gen metadata "budget spending"` → Finds cost/budget fields
- `mcc-gaql-gen metadata "campaign performance"` → Top campaign metrics

**Similarity Scoring:**
- **1.0** = Perfect match (identical embeddings)
- **0.8-0.99** = Very high similarity (semantically close)
- **0.65-0.79** = Moderate similarity (related concepts)
- **< 0.65** = Filtered out (threshold not met)

### Filtering Options

| Flag | Description |
|------|-------------|
| `--category <cat>` | Filter by category: resource, attribute, metric, segment |
| `--subset` | Limit to core resources (campaign, ad_group, ad_group_ad, keyword_view) |
| `--show-all` | Show all fields (default: 15 per category) |
| `--diff` | Compare enriched vs non-enriched metadata |
| `--filter <type>` | Filter fields: no-description, no-usage-notes, fallback |

### Output Formats

#### 1. **LLM Format** (`--format llm`, default)

Optimized for LLM context windows:
- Shows top 15 fields per category (unless `--show-all`)
- Includes similarity scores for semantic search
- Displays selectable segments/metrics for resources
- Highlights key_field and common_filter indicators

**Example Output:**
```
=== SEMANTIC SEARCH RESULTS ===
Query: "click metrics"

--- METRIC (5 results, 2 hidden below threshold) ---
  [0.87] metrics.clicks (INT64) [key] [filter]
    Description: Number of clicks on ads
    Usage: Use for measuring ad engagement

  [0.78] metrics.all_conversions_from_clicks (DOUBLE)
    Description: Conversions attributed to clicks
```

#### 2. **Full Format** (`--format full`)

Detailed field information:
- All metadata fields
- Complete enum values
- Full descriptions and usage notes

#### 3. **JSON Format** (`--format json`)

Machine-readable output for scripting:
```json
{
  "field": {
    "name": "metrics.clicks",
    "data_type": "INT64",
    "category": "METRIC",
    "description": "Number of clicks",
    "similarity_score": 0.87
  }
}
```

### Data Flow

```
User Query: "click metrics"
       ↓
┌──────────────────────────┐
│ 1. Check Exact Matches   │
│    - Resource name?  NO  │
│    - Field name?     NO  │
└──────────────────────────┘
       ↓
┌──────────────────────────┐
│ 2. Semantic Search       │
│    - Embed query text    │
│    - Search field_index  │
│    - Limit: 50 results   │
└──────────────────────────┘
       ↓
┌──────────────────────────┐
│ 3. Filter by Similarity  │
│    - Threshold: 0.65     │
│    - Keep high scores    │
└──────────────────────────┘
       ↓
┌──────────────────────────┐
│ 4. Group by Category     │
│    - ATTRIBUTE           │
│    - METRIC              │
│    - SEGMENT             │
└──────────────────────────┘
       ↓
┌──────────────────────────┐
│ 5. Apply Filters         │
│    - Category filter     │
│    - Subset filter       │
│    - Custom filters      │
└──────────────────────────┘
       ↓
┌──────────────────────────┐
│ 6. Format Output         │
│    - LLM (concise)       │
│    - Full (detailed)     │
│    - JSON (structured)   │
└──────────────────────────┘
```

---

## The `generate` Command

### Purpose
Generate a GAQL query from a natural language prompt using multi-step RAG pipeline.

### Usage
```bash
mcc-gaql-gen generate "Show me campaigns with cost over $1000" [OPTIONS]
```

### Multi-Step RAG Pipeline

The query generation follows a 5-phase pipeline:

```
User Prompt: "Show me enabled campaigns with cost over $1000"
       ↓
┌─────────────────────────────────────────────────────────────┐
│ PHASE 1: Resource Selection                                │
│ - Search resource_index (semantic)                          │
│ - LLM selects primary + related resources                   │
│ - Output: "campaign" (primary)                              │
│ - Time: ~300-500ms                                          │
└─────────────────────────────────────────────────────────────┘
       ↓
┌─────────────────────────────────────────────────────────────┐
│ PHASE 2: Field Candidate Retrieval                         │
│ - Hybrid approach: RAG + keyword + resource fields          │
│ - Semantic search field_index                               │
│ - Keyword matching                                          │
│ - Resource attribute inclusion                              │
│ - Compatibility validation (selectable_with)                │
│ - Output: ~50-200 candidate fields                          │
│ - Time: ~200-400ms                                          │
└─────────────────────────────────────────────────────────────┘
       ↓
┌─────────────────────────────────────────────────────────────┐
│ PHASE 2.5: Pre-scan Filters                                │
│ - Keyword-based filter detection                            │
│ - Enum value extraction                                     │
│ - Output: Pre-identified filters (e.g., status=ENABLED)     │
│ - Time: <50ms                                               │
└─────────────────────────────────────────────────────────────┘
       ↓
┌─────────────────────────────────────────────────────────────┐
│ PHASE 3: Field Selection (LLM)                             │
│ - Optional: Retrieve cookbook examples (query_index)        │
│ - Build LLM prompt with candidates                          │
│ - LLM selects: SELECT, WHERE, ORDER BY fields               │
│ - Validation against candidate list                         │
│ - Monetary threshold correction                             │
│ - Output: Final field selection                             │
│ - Time: ~1-3 seconds (LLM call)                             │
└─────────────────────────────────────────────────────────────┘
       ↓
┌─────────────────────────────────────────────────────────────┐
│ PHASE 4: Criteria Assembly                                 │
│ - Build WHERE clauses from filters                          │
│ - Detect LIMIT from prompt (top N, first N)                 │
│ - Add implicit filters (unless --no-defaults)               │
│ - Output: WHERE, ORDER BY, LIMIT                            │
│ - Time: <50ms                                               │
└─────────────────────────────────────────────────────────────┘
       ↓
┌─────────────────────────────────────────────────────────────┐
│ PHASE 5: GAQL Generation                                   │
│ - Assemble query using GaqlBuilder                          │
│ - Validate field compatibility                              │
│ - Format final GAQL string                                  │
│ - Output: Final GAQL query                                  │
│ - Time: <50ms                                               │
└─────────────────────────────────────────────────────────────┘
       ↓
Final GAQL:
SELECT campaign.id, campaign.name, metrics.cost_micros
FROM campaign
WHERE campaign.status = 'ENABLED' AND metrics.cost_micros > 1000000
```

**Total Time:** ~2-4 seconds (dominated by LLM latency)

---

### Phase 1: Resource Selection

**Goal:** Identify the primary resource (e.g., "campaign") and related resources.

**Algorithm:**
1. **RAG Pre-filter:** Search `resource_index` for top 20 semantically similar resources
2. **Resource Categorization:** Group into categories (campaign, ad, keyword, etc.)
3. **LLM Selection:** Send categorized list to LLM with prompt
4. **Output:** Primary resource, related resources, dropped resources, reasoning

**Example:**

**User Query:** "Show me top campaigns by cost"

**RAG Results (top 20):**
```
- campaign (score: 0.92)
- campaign_budget (score: 0.76)
- campaign_criterion (score: 0.71)
- ad_group (score: 0.65)
...
```

**LLM Prompt (excerpt):**
```
Given the user query: "Show me top campaigns by cost"

Select the PRIMARY resource and RELATED resources from:

--- CAMPAIGN RESOURCES ---
  - campaign: Campaign settings and performance [Segments: date/time, device, ...]
  - campaign_budget: Budget information for campaigns
  - campaign_criterion: Targeting criteria

--- AD GROUP RESOURCES ---
  - ad_group: Ad group settings and performance

Which resource should be the PRIMARY FROM resource?
Which resources are RELATED (for context)?
```

**LLM Response:**
```json
{
  "primary": "campaign",
  "related": [],
  "dropped": ["campaign_budget", "campaign_criterion", "ad_group"],
  "reasoning": "The query asks for campaigns ranked by cost. The campaign resource has all necessary metrics (cost_micros) and attributes (id, name)."
}
```

**Filtering:**
- Resources with similarity < 0.60 are filtered out before LLM
- Rare resources (< 10 fields) may be deprioritized

---

### Phase 2: Field Candidate Retrieval

**Goal:** Build a comprehensive list of field candidates from the selected resource.

**Multi-Strategy Approach:**

#### 1. **Resource Attribute Fields** (Guaranteed)
All attribute fields from the primary resource are included:
```
campaign.id, campaign.name, campaign.status, campaign.budget_amount_micros, ...
```

#### 2. **Semantic RAG Search**
Search `field_index` for fields semantically similar to the query:

**Example:** "campaigns by cost"
- Embeds: "campaigns by cost"
- Searches field_index (limit: 50)
- Filters by resource compatibility (`selectable_with`)
- Results: `metrics.cost_micros`, `metrics.average_cost`, etc.

#### 3. **Keyword Matching**
Extract query keywords and match against field names/descriptions:

**Query:** "campaigns by cost"
**Keywords:** `["campaigns", "cost"]` (stop words removed)
**Matches:**
- `metrics.cost_micros` (name match: "cost")
- `metrics.average_cost` (name match: "cost")

#### 4. **Selectable Metrics/Segments**
All compatible metrics and segments from `selectable_with`:
```
metrics.clicks, metrics.impressions, metrics.conversions, segments.date, segments.device, ...
```

**Deduplication:**
- Uses `HashSet<field_name>` to prevent duplicates
- Tracks seen fields across all strategies

**Compatibility Validation:**
Each field is checked against:
- `resource_metadata.selectable_with` for the primary resource
- Ensures the field can legally appear in the same query

**Output Counts:**
```
Phase 2 complete: 147 candidates (52 rejected for incompatibility)
```

---

### Phase 2.5: Pre-scan Filters

**Goal:** Detect common filter patterns from the query before LLM processing.

**Keyword Mappings:**
```rust
{
    "status" | "enabled" | "paused" | "active" → campaign.status
    "type" | "channel" → campaign.advertising_channel_type
    "device" | "mobile" | "desktop" → segments.device
    "network" | "search" | "display" → segments.ad_network_type
    "match type" | "match_type" → keyword.match_type
}
```

**Algorithm:**
1. Convert query to lowercase
2. Check for keyword presence
3. Find matching candidate field with enum values
4. Extract enum values that match the keyword context
5. Return `(field_name, Vec<enum_values>)`

**Example:**

**Query:** "Show enabled campaigns"
**Keyword Detected:** "enabled"
**Mapped Field:** `campaign.status`
**Enum Values:** `["ENABLED"]`
**Output:** `[("campaign.status", ["ENABLED"])]`

**Benefits:**
- Reduces LLM hallucination on filters
- Pre-populates enum values for LLM guidance
- Faster filter assembly in Phase 4

---

### Phase 3: Field Selection

**Goal:** Use LLM to intelligently select fields for SELECT, WHERE, and ORDER BY clauses.

**Inputs:**
- User query
- Field candidates (~50-200 fields)
- Pre-scanned filters (Phase 2.5)
- Optional: Top 3 cookbook examples (if `--use-query-cookbook`)

**LLM Prompt Structure:**
```
SYSTEM PROMPT:
You are a GAQL query expert. Given a user query and candidate fields, select:
1. SELECT fields (for output columns)
2. FILTER fields (for WHERE clause)
3. ORDER BY fields (for sorting)

DOMAIN KNOWLEDGE:
[Embedded from resources/domain_knowledge.md]
- Date range handling (THIS_YEAR, LAST_30_DAYS, etc.)
- Monetary field conversion (dollars → micros)
- Common patterns and best practices

USER QUERY:
"Show me top campaigns by cost over $1000"

CANDIDATE FIELDS (grouped by category):
--- ATTRIBUTES (15 shown, 42 total) ---
  - campaign.id (INT64) [key]
  - campaign.name (STRING) [key]
  - campaign.status (ENUM) [filter] [ENABLED, PAUSED, REMOVED]
  ...

--- METRICS (15 shown, 58 total) ---
  - metrics.cost_micros (INT64) [filter]
    Description: Total cost in micros
    Usage: Use for cost analysis
  - metrics.clicks (INT64) [filter]
  ...

--- SEGMENTS (15 shown, 23 total) ---
  - segments.date (DATE)
  - segments.device (ENUM)
  ...

PRE-SCANNED FILTERS:
- campaign.status: [ENABLED]

COOKBOOK EXAMPLES (if enabled):
1. "Top campaigns by cost"
   SELECT campaign.id, campaign.name, metrics.cost_micros
   FROM campaign
   WHERE metrics.cost_micros > 1000000
   ORDER BY metrics.cost_micros DESC
   LIMIT 10

Output JSON:
{
  "select_fields": ["campaign.id", "campaign.name", "metrics.cost_micros"],
  "filter_fields": [
    {"field": "campaign.status", "operator": "=", "value": "ENABLED"},
    {"field": "metrics.cost_micros", "operator": ">", "value": "1000"}
  ],
  "order_by": [
    {"field": "metrics.cost_micros", "direction": "DESC"}
  ],
  "limit": 10,
  "reasoning": "..."
}
```

**LLM Response Processing:**
1. **Parse JSON** from LLM response
2. **Validate fields** against candidate list (reject hallucinations)
3. **Monetary correction:** Detect dollar amounts and convert to micros
   - `"$1000"` → `1000000` (micros)
   - Threshold detection via regex patterns
4. **Reasoning extraction:** Capture LLM's decision-making logic

**Validation:**
- All selected fields must exist in candidates
- Filter operators must be valid (`=`, `!=`, `<`, `>`, `<=`, `>=`, `IN`, `NOT IN`, `LIKE`, `CONTAINS`)
- Enum values must match field's `enum_values`

**Monetary Conversion Example:**
```
Query: "campaigns with cost over $1000"
LLM Output: {"field": "metrics.cost_micros", "operator": ">", "value": "1000"}
Corrected: {"field": "metrics.cost_micros", "operator": ">", "value": "1000000"}
```

---

### Phase 4: Criteria Assembly

**Goal:** Assemble WHERE, ORDER BY, and LIMIT clauses from Phase 3 selections.

**WHERE Clause Construction:**
```rust
fn assemble_where_clauses(filter_fields: Vec<FilterField>) -> Vec<String> {
    filter_fields.iter().map(|ff| {
        format!("{} {} {}", ff.field_name, ff.operator, ff.value)
    }).collect()
}
```

**Example:**
```rust
FilterField { field: "campaign.status", operator: "=", value: "ENABLED" }
  → "campaign.status = 'ENABLED'"

FilterField { field: "metrics.cost_micros", operator: ">", value: "1000000" }
  → "metrics.cost_micros > 1000000"
```

**Implicit Filters (unless `--no-defaults`):**

For certain resources, implicit status filters are added:
```rust
match resource {
    "campaign" | "ad_group" | "keyword_view" | "ad_group_ad" | "user_list"
        if !where_clauses.contains(".status")
        => vec![format!("{}.status = 'ENABLED'", resource)]
    _ => vec![]
}
```

**LIMIT Detection:**

Pattern matching on the user query:
```rust
"top 10 campaigns" → LIMIT 10
"first 5 ads" → LIMIT 5
"best 20 keywords" → LIMIT 20
```

**ORDER BY Construction:**
```rust
order_by_fields: [("metrics.cost_micros", "DESC")]
  → "ORDER BY metrics.cost_micros DESC"
```

---

### Phase 5: GAQL Generation

**Goal:** Build the final GAQL query string.

**GaqlBuilder:**
```rust
GaqlBuilder::new("campaign")
    .select(vec!["campaign.id", "campaign.name", "metrics.cost_micros"])
    .where_clauses(vec!["campaign.status = 'ENABLED'", "metrics.cost_micros > 1000000"])
    .order_by(vec![("metrics.cost_micros", "DESC")])
    .limit(Some(10))
    .build()
```

**Output:**
```sql
SELECT
  campaign.id,
  campaign.name,
  metrics.cost_micros
FROM campaign
WHERE campaign.status = 'ENABLED' AND metrics.cost_micros > 1000000
ORDER BY metrics.cost_micros DESC
LIMIT 10
```

**Validation:**

The final query is validated against the field cache:
```rust
field_cache.validate_field_selection_for_resource(
    &all_fields,  // All fields in SELECT + WHERE
    &primary_resource
)
```

Checks:
- All fields exist in the cache
- All fields are compatible with the resource (`selectable_with`)
- No incompatible field combinations

**Output Structure:**
```rust
GAQLResult {
    query: String,                   // Final GAQL
    validation: ValidationResult,    // Compatibility checks
    pipeline_trace: PipelineTrace,   // Phase timings and decisions
}
```

---

## Filtering and Scoring

### Similarity Scoring

**Metric:** Cosine distance converted to similarity
```rust
similarity = 1.0 - cosine_distance
```

**Distance vs Similarity:**
- **Cosine Distance**: 0.0 (identical) → 1.0 (orthogonal)
- **Similarity**: 1.0 (identical) → 0.0 (orthogonal)

**Threshold Application:**
```rust
SIMILARITY_THRESHOLD = 0.65

if similarity >= 0.65 {
    // Include result
} else {
    // Filter out (low relevance)
}
```

**Score Interpretation:**
| Similarity | Interpretation | Action |
|------------|----------------|--------|
| 0.9 - 1.0  | Near-perfect match | Always include |
| 0.8 - 0.89 | Very high relevance | Always include |
| 0.65 - 0.79 | Moderate relevance | Include (passes threshold) |
| 0.5 - 0.64 | Low relevance | Filter out |
| < 0.5 | Very low relevance | Filter out |

### Field Categorization

**Categories:**
- **ATTRIBUTE**: Descriptive fields (id, name, status, etc.)
- **METRIC**: Performance metrics (clicks, impressions, cost, conversions)
- **SEGMENT**: Breakdown dimensions (date, device, network, etc.)

**Sorting Within Categories:**
1. **Semantic search:** Sort by similarity (descending)
2. **Pattern match:** Alphabetical
3. **Resource view:** Alphabetical

### LLM Limit (15 Fields per Category)

**Rationale:**
- LLM context windows have token limits
- Too many candidates → poor selection quality
- 15 fields per category = ~45 total fields (manageable for LLM)

**Implementation:**
```rust
fn limit_fields_per_category(fields: Vec<FieldMetadata>) -> Vec<FieldMetadata> {
    fields.into_iter().take(LLM_CATEGORY_LIMIT).collect()
}
```

**Override:** Use `--show-all` flag to see all fields

---

## Performance Considerations

### Embedding Generation

**Parallelization:**
Embeddings are generated in parallel chunks using all CPU cores:
```rust
async fn generate_embeddings_parallel<T: Embed>(
    documents: Vec<T>,
    embedding_model: EmbeddingModel,
) -> Result<Vec<Vec<f32>>> {
    let chunk_size = 100;
    let chunks = documents.chunks(chunk_size);
    
    stream::iter(chunks)
        .map(|chunk| embedding_model.embed_batch(chunk))
        .buffer_unordered(num_cpus::get())
        .collect()
        .await
}
```

**Performance:**
- ~1000 fields embedded in ~3-5 seconds (M2 Mac)
- LanceDB cache persists embeddings (one-time cost)

### Vector Search Performance

**LanceDB Optimization:**
- **Index Type:** IVF_PQ (Inverted File with Product Quantization)
- **Distance Metric:** Cosine distance
- **Search Time:** <100ms for 1000+ documents

**Cache Validation:**
Fast hash-based validation prevents unnecessary rebuilds:
```rust
// Check if cache matches current data
let current_hash = compute_field_hash(&field_cache.fields);
let cached_hash = read_cache_hash()?;

if current_hash == cached_hash {
    // Use existing cache
} else {
    // Rebuild cache
}
```

### LLM Latency

**Bottleneck:** LLM API calls (1-3 seconds per phase)

**Optimization Strategies:**
1. **Model Selection:** Use faster models (gpt-4o-mini, gemini-flash-2.0)
2. **Parallel Phases:** Phase 1 and Phase 2 could run in parallel (future enhancement)
3. **Prompt Compression:** Limit fields to 15 per category
4. **Caching:** Cookbook examples cached in vector store

**Typical Latencies:**
- **Phase 1 (Resource Selection):** ~500ms (LLM + RAG)
- **Phase 2 (Field Retrieval):** ~300ms (vector search + filtering)
- **Phase 3 (Field Selection):** ~2000ms (LLM call)
- **Phase 4 (Criteria Assembly):** <50ms (pure logic)
- **Phase 5 (GAQL Generation):** <50ms (string building)

**Total:** ~3-4 seconds end-to-end

### Memory Usage

**LanceDB Storage:**
- Field index: ~50-100 MB (thousands of fields)
- Query index: ~1-5 MB (hundreds of examples)
- Resource index: ~1-10 MB (hundreds of resources)

**Runtime Memory:**
- Field cache: ~20-50 MB (in-memory HashMap)
- Embedding model: ~100-200 MB (loaded once, reused)

---

## Configuration Options

### Pipeline Configuration

```rust
PipelineConfig {
    use_query_cookbook: bool,        // Enable cookbook examples in Phase 3
    add_implicit_filters: bool,      // Add status=ENABLED defaults
}
```

**CLI Flags:**
- `--use-query-cookbook`: Enable RAG retrieval of cookbook examples
- `--no-defaults`: Skip implicit filters (e.g., status=ENABLED)
- `--explain`: Print detailed pipeline trace

### LLM Configuration

**Environment Variables:**
```bash
MCC_GAQL_LLM_API_KEY="sk-..."
MCC_GAQL_LLM_BASE_URL="https://api.openai.com/v1"
MCC_GAQL_LLM_MODEL="gpt-4o-mini"
MCC_GAQL_LLM_TEMPERATURE="0.1"
```

**Model Selection:**
- **Primary Model:** First model in `MCC_GAQL_LLM_MODEL` (comma-separated)
- **Fallback Models:** Additional models for redundancy
- **Temperature:** Low (0.1) for deterministic query generation

---

## Example Workflows

### Workflow 1: Generate Query

```bash
# User command
mcc-gaql-gen generate "Show top 10 campaigns by clicks last month"

# Phase 1: Resource Selection
# → RAG search: campaign (0.95), ad_group (0.72), ...
# → LLM selects: "campaign"

# Phase 2: Field Retrieval
# → Semantic: metrics.clicks, segments.date, campaign.name
# → Keywords: "clicks" → metrics.clicks
# → Resource: campaign.id, campaign.status, ...
# → Total: 147 candidates

# Phase 2.5: Pre-scan
# → No filters detected

# Phase 3: Field Selection
# → LLM selects:
#   - SELECT: campaign.id, campaign.name, metrics.clicks
#   - WHERE: segments.date >= LAST_MONTH
#   - ORDER BY: metrics.clicks DESC
#   - LIMIT: 10

# Phase 4: Criteria Assembly
# → WHERE: ["segments.date >= '2026-03-01'", "segments.date <= '2026-03-31'"]
# → ORDER BY: [("metrics.clicks", "DESC")]
# → LIMIT: 10
# → Implicit: "campaign.status = 'ENABLED'"

# Phase 5: GAQL Generation
# → Final query:
SELECT
  campaign.id,
  campaign.name,
  metrics.clicks
FROM campaign
WHERE campaign.status = 'ENABLED'
  AND segments.date >= '2026-03-01'
  AND segments.date <= '2026-03-31'
ORDER BY metrics.clicks DESC
LIMIT 10
```

### Workflow 2: Metadata Search

```bash
# Semantic search
mcc-gaql-gen metadata "conversion metrics"

# Output:
=== SEMANTIC SEARCH RESULTS ===

--- METRIC (8 results, 3 hidden below threshold) ---
  [0.89] metrics.conversions (DOUBLE) [key] [filter]
    Description: Total conversions across all conversion actions
    Usage: Primary conversion metric for performance analysis

  [0.85] metrics.conversions_value (DOUBLE) [filter]
    Description: Total value of all conversions
    Usage: Measures monetary value of conversions

  [0.78] metrics.all_conversions (DOUBLE)
    Description: All conversions including view-through
    ...
```

---

## Summary

The RAG pipeline in `mcc-gaql-gen` combines:

1. **Vector embeddings** for semantic understanding of user queries
2. **Multi-stage retrieval** (resource → fields → examples)
3. **LLM intelligence** for field selection and query assembly
4. **Domain knowledge** for GAQL syntax and best practices
5. **Validation** to ensure query compatibility

This architecture enables users to generate complex GAQL queries using natural language, without memorizing field names or query syntax. The system achieves ~3-4 second query generation with high accuracy through careful pipeline design and optimization.

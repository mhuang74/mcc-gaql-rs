# Design Spec: --explain-selection-process Flag

## Overview
Add a new `--explain-selection-process` flag to `mcc-gaql-gen` that provides transparency into the LLM-assisted RAG selection process. The explanation is printed to stdout (not logs) in human-readable format.

## User Goals
- Understand why the LLM chose a particular primary resource
- See what candidate fields were available to the LLM for field selection
- Understand the LLM's reasoning for selecting specific fields
- Debug cases where unexpected fields were selected (or expected fields were missed)

## Current RAG Flow (for context)

```
Phase 1: Resource Selection
  - Input: user query + master list of all resources
  - LLM selects: primary_resource, related_resources
  - LLM returns: confidence, reasoning

Phase 2: Field Candidate Retrieval
  - Input: user query + primary resource
  - Retrieves: key fields + vector search results (attrs, metrics, segments)
  - Output: candidate list (can be 60+ fields)

Phase 2.5: Pre-scan Filters
  - Scans user query for filter keywords
  - Extracts enum values for WHERE clause suggestions

Phase 3: Field Selection
  - Input: user query + candidate fields (capped at 15/category for LLM prompt)
  - LLM selects: select_fields, filter_fields, order_by_fields
  - LLM returns: reasoning
```

## Design Decisions

### 1. Output Target: stdout
- Printed directly to stdout, independent of logging system
- Not affected by `MCC_GAQL_LOG_LEVEL`
- Appears alongside normal query output

### 2. Output Format: Human-Readable Text
```
═══════════════════════════════════════════════════════════════
               RAG SELECTION EXPLANATION
═══════════════════════════════════════════════════════════════

## Phase 1: Resource Selection

User Query: "show me top campaigns by clicks last month"

LLM Reasoning:
  The user is asking for campaign-level performance data, specifically
  ranked by clicks. "campaign" is the appropriate primary resource.

Selected Primary Resource: campaign
Related Resources: []

## Phase 3: Field Selection

Candidate Fields Available to LLM (87 total):

### ATTRIBUTE (23 fields)
  - campaign.id: ID of the campaign [filterable]
  - campaign.name: Name of the campaign [filterable]
  - campaign.status: Status [filterable] (valid: ENABLED, PAUSED)
  ... (21 more)

### METRIC (45 fields)
  - metrics.clicks: Number of clicks [sortable]
  - metrics.cost_micros: Cost in micros [sortable]
  - metrics.impressions: Number of impressions
  ... (42 more)

### SEGMENT (19 fields)
  - segments.date: Date [filterable] [sortable]
  - segments.device: Device type
  ... (17 more)

Pre-scanned Filter Hints:
  - campaign.status: ENABLED (from keyword "active")

LLM Reasoning:
  Selected campaign.name and metrics.clicks for the core query. Added
  segments.date because user specified "last month". Added
  metrics.impressions for context.

Selected Fields:
  - campaign.name
  - metrics.clicks
  - metrics.impressions
  - segments.date

═══════════════════════════════════════════════════════════════
```

### 3. Candidate Fields: Show All Retrieved
- Display the full set from Phase 2 retrieval (not just the 15/category subset)
- Include field properties: filterable, sortable, enum values
- Group by category (ATTRIBUTE, METRIC, SEGMENT)

### 4. Flag Behavior
- Independent flag: `--explain-selection-process`
- Works regardless of log level
- Can be combined with other flags
- Does not affect normal query generation flow

## Implementation Details

### Data to Capture

#### Phase 1: Resource Selection
| Data | Source | Notes |
|------|--------|-------|
| User query | Input param | Already available |
| LLM reasoning | LLM response | Currently parsed but not stored |
| Primary resource | LLM response | Currently returned |
| Related resources | LLM response | Currently returned, filtered |

#### Phase 3: Field Selection
| Data | Source | Notes |
|------|--------|-------|
| All candidate fields | Phase 2 result | Need to pass to Phase 3 or store |
| Pre-scanned filters | Phase 2.5 result | Already available |
| LLM reasoning | LLM response | Currently parsed but not stored |
| Selected fields | LLM response | Currently returned |

### Changes Required

1. **Add CLI flag** (`main.rs`)
   - Add `--explain-selection-process: bool` to CLI args
   - Pass to `RagPipelineConfig`

2. **Extend config** (`rag.rs`)
   - Add `explain_selection_process: bool` to `RagPipelineConfig`

3. **Capture LLM reasoning** (`rag.rs`)
   - In `select_resource()`: capture and return `reasoning` field
   - In `select_fields()`: capture and return `reasoning` field

4. **Store candidate fields** (`rag.rs`)
   - Return full candidate list from `retrieve_field_candidates()`
   - Pass through to `select_fields()` for explanation output

5. **Create explanation printer** (`rag.rs` or new module)
   - Function to format and print the explanation to stdout
   - Called at the end of `generate_query()` if flag is set

### Type Changes

```rust
// Current
struct FieldSelectionResult {
    select_fields: Vec<String>,
    filter_fields: Vec<FilterField>,
    order_by_fields: Vec<(String, String)>,
}

// New - add reasoning and timing
struct FieldSelectionResult {
    select_fields: Vec<String>,
    filter_fields: Vec<FilterField>,
    order_by_fields: Vec<(String, String)>,
    reasoning: String,
    timing_ms: u64,
    model_used: String,
    fallback_chain: Vec<String>, // empty if no fallback
}

// Current resource selection returns tuple
// Change to structured type
struct ResourceSelectionResult {
    primary_resource: String,
    related_resources: Vec<String>,
    dropped_resources: Vec<String>,
    reasoning: String,
    timing_ms: u64,
    model_used: String,
    fallback_chain: Vec<String>,
}

// New type for field candidate with score
struct FieldCandidate {
    field: FieldMetadata,
    search_score: f32,
    was_filtered: bool,
    filter_reason: Option<String>, // e.g., "incompatible with primary"
}

// New type for Phase 2 result
struct FieldRetrievalResult {
    candidates: Vec<FieldCandidate>,
    compatible_count: usize,
    filtered_count: usize,
    timing_ms: u64,
}
```

### Output Module (Optional)
Consider extracting explanation formatting to a new module:

```rust
// explain.rs
pub struct SelectionExplanation {
    pub user_query: String,
    pub resource_selection: ResourceSelectionExplanation,
    pub field_selection: FieldSelectionExplanation,
}

pub struct ResourceSelectionExplanation {
    pub reasoning: String,
    pub primary: String,
    pub related: Vec<String>,
}

pub struct FieldSelectionExplanation {
    pub all_candidates: Vec<FieldMetadata>,
    pub pre_scan_filters: Vec<(String, Vec<String>)>,
    pub llm_reasoning: String,
    pub selected_fields: Vec<String>,
}

impl SelectionExplanation {
    pub fn print(&self) {
        // Format and print to stdout
    }
}
```

## Design Decisions Summary

| Question | Decision |
|----------|----------|
| Timing information | **Include** - Show duration for each phase |
| Vector search scores | **Include** - Show similarity scores for candidates |
| Rejected fields | **Include** - Show fields filtered out due to incompatibility |
| Cookbook influence | **Include** - Show cookbook examples when flag enabled |
| Fallback chain | **Include** - Show which models were tried |

## Updated Output Format

```
═══════════════════════════════════════════════════════════════
               RAG SELECTION EXPLANATION
═══════════════════════════════════════════════════════════════

User Query: "show me top campaigns by clicks last month"

## Phase 1: Resource Selection (145ms)

Model: gpt-4o

LLM Reasoning:
  The user is asking for campaign-level performance data, specifically
  ranked by clicks. "campaign" is the appropriate primary resource.

Selected Primary Resource: campaign
Related Resources: []

## Phase 2: Field Candidate Retrieval (89ms)

Vector Search Results:

### Retrieved Attributes (30 results)
  - campaign.id [score: 0.92]
  - campaign.name [score: 0.88]
  - campaign.status [score: 0.85]
  - ad_group.id [score: 0.82] → FILTERED (incompatible with primary)
  ...

### Retrieved Metrics (30 results)
  - metrics.clicks [score: 0.95]
  - metrics.cost_micros [score: 0.91]
  ...

### Retrieved Segments (15 results)
  - segments.date [score: 0.89]
  ...

Compatible Candidates Passed to Phase 3: 87 fields
Filtered Out (incompatible): 12 fields

## Phase 2.5: Pre-scan Filters

Detected Keywords:
  - "active" → campaign.status: [ENABLED]
  - "last month" → segments.date (temporal range)

## Phase 3: Field Selection (203ms)

Model: gpt-4o (fallback from: o3-mini - timeout after 30s)

Cookbook Examples Consulted (3):
  - "Top campaigns by clicks"
    GAQL: SELECT campaign.name, metrics.clicks FROM campaign ...
  - "Campaign performance by date"
    GAQL: SELECT campaign.name, segments.date, metrics.clicks ...
  - "Active campaigns only"
    GAQL: SELECT campaign.name FROM campaign WHERE campaign.status = 'ENABLED'

Candidate Fields Available to LLM (87 total):

### ATTRIBUTE (23 fields)
  - campaign.id: ID of the campaign [filterable]
  - campaign.name: Name of the campaign [filterable]
  - campaign.status: Status [filterable] (valid: ENABLED, PAUSED)
  ...

### METRIC (45 fields)
  - metrics.clicks: Number of clicks [sortable]
  - metrics.cost_micros: Cost in micros [sortable]
  ...

### SEGMENT (19 fields)
  - segments.date: Date [filterable] [sortable]
  ...

LLM Reasoning:
  Selected campaign.name and metrics.clicks for the core query. Added
  segments.date because user specified "last month". Added
  metrics.impressions for context. Applied filter for active campaigns
  based on pre-scan hint.

Selected Fields:
  - campaign.name
  - metrics.clicks
  - metrics.impressions
  - segments.date

═══════════════════════════════════════════════════════════════
```

## Additional Data to Capture

Based on design decisions, the following additional data must be captured:

| Data | Phase | Source |
|------|-------|--------|
| Phase timing | All | `std::time::Instant` |
| Vector search scores | Phase 2 | Search results (result.1) |
| Filtered/rejected fields | Phase 2 | Fields that fail compatibility check |
| Cookbook examples | Phase 3 | `retrieve_cookbook_examples()` |
| Model used | Phase 1, 3 | LLM config / fallback tracking |
| Fallback chain | Phase 1, 3 | Track when fallback models are invoked |

## Acceptance Criteria

- [ ] `--explain-selection-process` flag is available in CLI
- [ ] Flag prints explanation to stdout (not logs)
- [ ] Resource selection phase shows: user query, LLM reasoning, selected resource
- [ ] Field selection phase shows: all candidate fields (grouped by category), pre-scanned filters, LLM reasoning, selected fields
- [ ] Output is human-readable with clear section headers
- [ ] Normal query generation continues to work unchanged
- [ ] Flag has no effect on log files or structured output (JSON)

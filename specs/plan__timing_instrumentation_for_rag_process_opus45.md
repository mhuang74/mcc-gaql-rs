# Plan: Add Timing Instrumentation to GAQL Generate Command

## Context

The `generate` command in `mcc-gaql-gen` takes over a minute to complete, with no visibility into which phase is consuming time. Based on logs, there's a ~105 second gap between cache loading and the first warning, with no intermediate logging.

The pipeline has 5 phases with 2 LLM calls and 4 vector searches. Without timing info, it's impossible to know if slowness is due to:
- LLM latency (API calls)
- Vector search performance
- Embedding generation
- Data processing

## Implementation Plan

### Files to Modify

**Primary:** `crates/mcc-gaql-gen/src/rag.rs`

### Changes

#### 1. Add Phase-Level Timing in `generate()` (lines 983-1053)

Add timing around each phase call with clear, parseable log output:

```rust
// Phase 1
let phase1_start = std::time::Instant::now();
log::info!("Phase 1: Resource selection...");
let (primary_resource, ...) = self.select_resource(user_query).await?;
log::info!("Phase 1 complete: {} ({}ms)", primary_resource, phase1_start.elapsed().as_millis());

// Phase 2
let phase2_start = std::time::Instant::now();
log::info!("Phase 2: Retrieving field candidates...");
let (candidates, ...) = self.retrieve_field_candidates(...).await?;
log::info!("Phase 2 complete: {} candidates ({}ms)", candidates.len(), phase2_start.elapsed().as_millis());

// Phase 2.5
let phase25_start = std::time::Instant::now();
let filter_enums = self.prescan_filters(...);
log::debug!("Phase 2.5: Pre-scan filters ({}ms)", phase25_start.elapsed().as_millis());

// Phase 3
let phase3_start = std::time::Instant::now();
log::info!("Phase 3: Field selection via LLM...");
let field_selection = self.select_fields(...).await?;
log::info!("Phase 3 complete: {} fields selected ({}ms)", field_selection.select_fields.len(), phase3_start.elapsed().as_millis());

// Phase 4
let phase4_start = std::time::Instant::now();
let (where_clauses, ...) = self.assemble_criteria(...);
log::debug!("Phase 4: Criteria assembly ({}ms)", phase4_start.elapsed().as_millis());

// Phase 5
let phase5_start = std::time::Instant::now();
let result = self.generate_gaql(...).await?;
log::debug!("Phase 5: GAQL generation ({}ms)", phase5_start.elapsed().as_millis());
```

#### 2. Add LLM Call Timing in `select_resource()` (line 1096)

```rust
log::debug!("Phase 1: Calling LLM for resource selection...");
let llm_start = std::time::Instant::now();
let response = agent.prompt(&user_prompt).await?;
log::debug!("Phase 1: LLM responded in {}ms", llm_start.elapsed().as_millis());
```

#### 3. Add Vector Search Timing in `retrieve_field_candidates()` (lines 1241-1243)

```rust
log::debug!("Phase 2: Running 3 parallel vector searches...");
let search_start = std::time::Instant::now();
let (attr_results, metric_results, segment_results) =
    tokio::join!(attr_search, metric_search, segment_search);
log::debug!("Phase 2: Vector searches complete in {}ms", search_start.elapsed().as_millis());
```

#### 4. Add Timing in `select_fields()` (lines 1367, 1432)

```rust
// Line 1367 - cookbook retrieval
log::debug!("Phase 3: Retrieving cookbook examples...");
let cookbook_start = std::time::Instant::now();
let examples = self.retrieve_cookbook_examples(user_query, 3).await?;
log::debug!("Phase 3: Cookbook examples retrieved in {}ms", cookbook_start.elapsed().as_millis());

// Line 1432 - LLM call (most likely bottleneck)
log::debug!("Phase 3: Calling LLM for field selection...");
let llm_start = std::time::Instant::now();
let response = agent.prompt(&user_prompt).await?;
log::debug!("Phase 3: LLM responded in {}ms", llm_start.elapsed().as_millis());
```

#### 5. Add Total Time Summary at End

After all phases complete (around line 1021):

```rust
let generation_time_ms = start.elapsed().as_millis() as u64;
log::info!(
    "GAQL generation complete: total={}ms (Phase1={}ms, Phase2={}ms, Phase3={}ms)",
    generation_time_ms,
    phase1_time, phase2_time, phase3_time
);
```

### Log Levels

- `log::info!` - Phase start/complete with timing (visible by default with `-v`)
- `log::debug!` - Sub-phase details (LLM calls, vector searches)

### Expected Output Example

```
INFO  Phase 1: Resource selection...
DEBUG Phase 1: Calling LLM for resource selection...
DEBUG Phase 1: LLM responded in 2345ms
INFO  Phase 1 complete: audience_view (2350ms)
INFO  Phase 2: Retrieving field candidates...
DEBUG Phase 2: Running 3 parallel vector searches...
DEBUG Phase 2: Vector searches complete in 45ms
INFO  Phase 2 complete: 47 candidates (48ms)
DEBUG Phase 2.5: Pre-scan filters (2ms)
INFO  Phase 3: Field selection via LLM...
DEBUG Phase 3: Retrieving cookbook examples...
DEBUG Phase 3: Cookbook examples retrieved in 12ms
DEBUG Phase 3: Calling LLM for field selection...
DEBUG Phase 3: LLM responded in 98234ms  <-- bottleneck identified!
INFO  Phase 3 complete: 8 fields selected (98250ms)
DEBUG Phase 4: Criteria assembly (1ms)
DEBUG Phase 5: GAQL generation (0ms)
INFO  GAQL generation complete: total=100651ms
```

## Verification

1. Build: `cargo build -p mcc-gaql-gen`
2. Run with verbose: `mcc-gaql-gen -v generate "show me campaigns with high CPA"`
3. Verify timing logs appear for each phase
4. Confirm the bottleneck is identifiable from the logs

# Plan: Add Trace and Timing Info to `generate` Command

## Context

The `generate` command takes over a minute to generate GAQL queries with no visibility into what it's doing. From the logs provided, there's a ~1.5 minute gap between:
- Line 585: `Cache valid. Generating GAQL for: "..."`
- Line 1551: `Invalid operator 'DURING' for field 'segments.date', skipping`

The user needs visibility into where time is being spent during generation.

## Current State Analysis

Looking at the code flow:

1. **main.rs:585-593**: `cmd_generate` calls `rag::convert_to_gaql()` - **NO timing**
2. **rag.rs:1666-1675**: `convert_to_gaql()` creates agent and calls `generate()`
3. **rag.rs:943-980**: `MultiStepRAGAgent::init()` - missing timing for:
   - `init_llm_resources()` (line 958) - creates LLM and embedding clients
   - Connection to LanceDB is slow
4. **rag.rs:983-1021**: `MultiStepRAGAgent::generate()` - has total timing but **NO per-phase timing**

The 5 phases in `generate()`:
- Phase 1: Resource selection (line 988)
- Phase 2: Field candidate retrieval (line 991)
- Phase 2.5: Pre-scan for filter keywords (line 996)
- Phase 3: Field selection via LLM (line 999) - **this is likely slow (LLM call)**
- Phase 4: Assemble criteria (line 1004)
- Phase 5: Generate final GAQL (line 1011) - **this is likely slow (LLM call)**

## Implementation Plan

### 1. Add timing wrapper in `cmd_generate` (main.rs:585-593)

Add timing around the `convert_to_gaql` call and log total time at INFO level:

```rust
// Around line 585-593
log::info!("Cache valid. Generating GAQL for: \"{}\"", prompt);

let generate_start = std::time::Instant::now();
let result = rag::convert_to_gaql(...).await?;
log::info!(
    "GAQL generation completed in {:.2}s",
    generate_start.elapsed().as_secs_f64()
);
```

### 2. Add timing to `MultiStepRAGAgent::init` (rag.rs:943-980)

Add timing around initialization steps:

```rust
// At start of init (line 951)
let init_start = std::time::Instant::now();

// After init_llm_resources (line 958)
let llm_resources_start = std::time::Instant::now();
let resources = init_llm_resources(config)?;
log::info!(
    "LLM resources initialized in {:.2}s",
    llm_resources_start.elapsed().as_secs_f64()
);

// After field vector store (line 965)
log::info!(
    "Field vector store ready in {:.2}s",
    init_start.elapsed().as_secs_f64()
);

// After query vector store (line 970)
log::info!(
    "Query vector store ready in {:.2}s",
    init_start.elapsed().as_secs_f64()
);

// At end (line 980)
log::info!(
    "MultiStepRAGAgent initialized in {:.2}s total",
    init_start.elapsed().as_secs_f64()
);
```

### 3. Add per-phase timing in `MultiStepRAGAgent::generate` (rag.rs:983-1021)

Add timing for each phase:

```rust
// Phase 1
let phase1_start = std::time::Instant::now();
let (primary_resource, related_resources, dropped_resources, reasoning) =
    self.select_resource(user_query).await?;
log::info!("Phase 1 (resource selection) completed in {:.2}s", phase1_start.elapsed().as_secs_f64());

// Phase 2
let phase2_start = std::time::Instant::now();
let (candidates, candidate_count, rejected_count) = ...;
log::info!("Phase 2 (field retrieval) completed in {:.2}s", phase2_start.elapsed().as_secs_f64());

// Phase 2.5
let phase2_5_start = std::time::Instant::now();
let filter_enums = self.prescan_filters(user_query, &candidates);
log::info!("Phase 2.5 (filter prescan) completed in {:.2}s", phase2_5_start.elapsed().as_secs_f64());

// Phase 3
let phase3_start = std::time::Instant::now();
let field_selection = self.select_fields(...).await?;
log::info!("Phase 3 (field selection) completed in {:.2}s", phase3_start.elapsed().as_secs_f64());

// Phase 4
let phase4_start = std::time::Instant::now();
let (where_clauses, during, limit, implicit_filters) = ...;
log::info!("Phase 4 (criteria assembly) completed in {:.2}s", phase4_start.elapsed().as_secs_f64());

// Phase 5
let phase5_start = std::time::Instant::now();
let result = self.generate_gaql(...).await?;
log::info!("Phase 5 (GAQL generation) completed in {:.2}s", phase5_start.elapsed().as_secs_f64());
```

### 4. Add timing to `init_llm_resources` (rag.rs:204)

Add timing for embedding client creation:

```rust
fn init_llm_resources(config: &LlmConfig) -> Result<AgentResources, anyhow::Error> {
    let llm_start = std::time::Instant::now();
    let llm_client = config.create_llm_client()?;
    log::debug!("LLM client created in {:.2}s", llm_start.elapsed().as_secs_f64());

    let embed_start = std::time::Instant::now();
    let (embed_client, embedding_model) = create_embedding_client()?;
    log::info!("Embedding client created in {:.2}s", embed_start.elapsed().as_secs_f64());

    Ok(...)
}
```

## Files to Modify

1. **crates/mcc-gaql-gen/src/main.rs**
   - Lines 585-593: Add timing around `convert_to_gaql` call

2. **crates/mcc-gaql-gen/src/rag.rs**
   - Lines 943-980: Add timing to `MultiStepRAGAgent::init`
   - Lines 983-1021: Add per-phase timing to `MultiStepRAGAgent::generate`
   - Lines 204-213: Add timing to `init_llm_resources`

## Verification

After changes, running `mcc-gaql-gen generate "show me audiences with the highest CPA for past quarter"` should show:

```
[TIME] Cache valid. Generating GAQL for: "..."
[TIME] LLM resources initialized in X.XXs
[TIME] Field vector store ready in X.XXs
[TIME] Query vector store ready in X.XXs
[TIME] MultiStepRAGAgent initialized in X.XXs total
[TIME] Phase 1 (resource selection) completed in X.XXs
[TIME] Phase 2 (field retrieval) completed in X.XXs
[TIME] Phase 2.5 (filter prescan) completed in X.XXs
[TIME] Phase 3 (field selection) completed in X.XXs
[TIME] Phase 4 (criteria assembly) completed in X.XXs
[TIME] Phase 5 (GAQL generation) completed in X.XXs
[TIME] GAQL generation completed in X.XXs total
```

This will make it clear where time is being spent.

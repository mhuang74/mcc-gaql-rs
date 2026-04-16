# Plan: Add `--generate-prompt-only` and `--resource` Flags to Generate Command

## Context

When debugging or reviewing the RAG pipeline, users need visibility into the dynamic context that gets assembled into LLM prompts. Currently, the only way to see these prompts is to run the full generation pipeline with `--explain` or trace logging, which still invokes the LLM and produces a final GAQL query.

This change adds the ability to run the generation pipeline in a "prompt inspection" mode where users can see exactly what would be sent to the LLM without actually invoking it.

## Design

### New CLI Flags

1. **`--resource <name>`** - Override Phase 1 resource selection
   - Works with or without `--generate-prompt-only`
   - Skips Phase 1 (Resource Selection) entirely
   - Validates that the resource exists in field_cache before proceeding
   - Use case: Force a specific resource for normal generation, or inspect Phase 3 prompt

2. **`--generate-prompt-only`** - Stop after generating the LLM prompt, print it, don't call LLM
   - **Without `--resource`**: Runs Phase 1 RAG steps (resource retrieval), prints Phase 1 prompt, stops
   - **With `--resource <name>`**: Runs Phase 2 (field retrieval) + Phase 2.5 (pre-scan), prints Phase 3 prompt, stops
   - Output format: Raw prompt text (system prompt + user prompt clearly labeled)

### Behavior Matrix

| Flags | Behavior |
|-------|----------|
| `generate "prompt"` | Normal: Phase 1 → 5, full GAQL generation |
| `generate "prompt" --resource campaign` | Skip Phase 1, use "campaign", run Phases 2-5 |
| `generate "prompt" --generate-prompt-only` | Run Phase 1 RAG, print Phase 1 prompt, stop |
| `generate "prompt" --generate-prompt-only --resource campaign` | Skip Phase 1, run Phase 2+2.5, print Phase 3 prompt, stop |

## Implementation

### Step 1: Update CLI Definition

**File:** `crates/mcc-gaql-gen/src/main.rs`

Add two new fields to the `Generate` command variant (around line 155-187):

```rust
/// Override resource selection (skip Phase 1)
#[arg(long)]
resource: Option<String>,

/// Stop after generating LLM prompt and print it (don't call LLM)
#[arg(long)]
generate_prompt_only: bool,
```

Update `GenerateParams` struct (around line 51-61) to include these fields.

### Step 2: Update PipelineConfig

**File:** `crates/mcc-gaql-gen/src/rag.rs`

Extend `PipelineConfig` (around line 1544-1563) with:

```rust
/// Override resource selection (skip Phase 1)
pub resource_override: Option<String>,
/// Stop after generating LLM prompt and print it
pub generate_prompt_only: bool,
```

### Step 3: Add Prompt-Only Return Type

**File:** `crates/mcc-gaql-gen/src/rag.rs`

Add a new enum for the generate result:

```rust
pub enum GenerateResult {
    /// Full GAQL generation result
    Query(GAQLResult),
    /// Prompt-only output (system_prompt, user_prompt, phase_number)
    PromptOnly {
        system_prompt: String,
        user_prompt: String,
        phase: u8,  // 1 or 3
    },
}
```

### Step 4: Refactor `select_resource()` to Extract Prompt Building

**File:** `crates/mcc-gaql-gen/src/rag.rs`

Extract the prompt-building logic from `select_resource()` (lines 2154-2504) into a separate function:

```rust
/// Build the Phase 1 prompt without calling LLM
async fn build_phase1_prompt(&self, user_query: &str) -> Result<(String, String), anyhow::Error>
```

This function will:
1. Perform the RAG searches (`retrieve_relevant_resources`, `retrieve_relevant_fields`)
2. Build categorized resource list
3. Format field results
4. Retrieve cookbook examples (if enabled)
5. Assemble and return `(system_prompt, user_prompt)`

The existing `select_resource()` will call this, then proceed to call LLM and parse response.

### Step 5: Refactor `select_fields()` to Extract Prompt Building

**File:** `crates/mcc-gaql-gen/src/rag.rs`

Extract prompt-building from `select_fields()` (lines 2963-3385) into:

```rust
/// Build the Phase 3 prompt without calling LLM
fn build_phase3_prompt(
    &self,
    user_query: &str,
    primary: &str,
    candidates: &[FieldMetadata],
    filter_enums: &[(String, Vec<String>)],
) -> Result<(String, String), anyhow::Error>
```

This function will:
1. Retrieve cookbook examples (if enabled)
2. Build candidate text grouped by category
3. Load domain knowledge sections
4. Build DateContext
5. Assemble and return `(system_prompt, user_prompt)`

### Step 6: Update `generate()` Method

**File:** `crates/mcc-gaql-gen/src/rag.rs`

Modify `generate()` (lines 1826-1949) to handle the new modes:

```rust
pub async fn generate(&self, user_query: &str) -> Result<GenerateResult, anyhow::Error> {
    // If generate_prompt_only WITHOUT resource_override: show Phase 1 prompt
    if self.pipeline_config.generate_prompt_only && self.pipeline_config.resource_override.is_none() {
        let (system_prompt, user_prompt) = self.build_phase1_prompt(user_query).await?;
        return Ok(GenerateResult::PromptOnly { system_prompt, user_prompt, phase: 1 });
    }
    
    // Phase 1: Resource selection (or use override)
    let primary_resource = if let Some(ref resource) = self.pipeline_config.resource_override {
        // Validate resource exists
        if !self.field_cache.get_resources().contains(resource) {
            return Err(anyhow::anyhow!("Unknown resource: '{}'", resource));
        }
        resource.clone()
    } else {
        self.select_resource(user_query).await?.0
    };
    
    // Phase 2 + 2.5
    let (candidates, ..) = self.retrieve_field_candidates(user_query, &primary_resource, &[]).await?;
    let filter_enums = self.prescan_filters(user_query, &candidates);
    
    // If generate_prompt_only WITH resource_override: show Phase 3 prompt
    if self.pipeline_config.generate_prompt_only {
        let (system_prompt, user_prompt) = self.build_phase3_prompt(
            user_query, &primary_resource, &candidates, &filter_enums
        )?;
        return Ok(GenerateResult::PromptOnly { system_prompt, user_prompt, phase: 3 });
    }
    
    // Continue with normal pipeline...
}
```

### Step 7: Update cmd_generate Handler

**File:** `crates/mcc-gaql-gen/src/main.rs`

Update `cmd_generate()` (lines 894-1081) to:
1. Pass new flags to `PipelineConfig`
2. Handle `GenerateResult::PromptOnly` by printing prompts and returning early

```rust
match result {
    rag::GenerateResult::PromptOnly { system_prompt, user_prompt, phase } => {
        println!("═══════════════════════════════════════════════════════════════");
        println!("               PHASE {} LLM PROMPT", phase);
        println!("═══════════════════════════════════════════════════════════════\n");
        println!("=== SYSTEM PROMPT ===\n{}\n", system_prompt);
        println!("=== USER PROMPT ===\n{}\n", user_prompt);
        return Ok(());
    }
    rag::GenerateResult::Query(gaql_result) => {
        // existing query output logic...
    }
}
```

### Step 8: Update Public API

**File:** `crates/mcc-gaql-gen/src/rag.rs`

Update `convert_to_gaql()` return type or add a new entry point:

```rust
pub async fn convert_to_gaql(...) -> Result<GenerateResult, anyhow::Error>
```

## Files to Modify

1. `crates/mcc-gaql-gen/src/main.rs` (lines 51-61, 155-187, 346-368, 894-1081)
   - Add CLI flags to `Commands::Generate`
   - Add fields to `GenerateParams`
   - Update command dispatch
   - Handle `PromptOnly` result in `cmd_generate`

2. `crates/mcc-gaql-gen/src/rag.rs` (lines 1544-1576, 1826-1949, 2154-2504, 2963-3385, 3754-3764)
   - Extend `PipelineConfig` with new fields
   - Add `GenerateResult` enum
   - Add `build_phase1_prompt()` helper
   - Add `build_phase3_prompt()` helper
   - Update `generate()` to handle new modes
   - Update `convert_to_gaql()` return type

## Verification

1. **Test Phase 1 prompt only:**
   ```bash
   cargo run -p mcc-gaql-gen -- generate "show top campaigns by cost" --generate-prompt-only
   ```
   - Should print Phase 1 system and user prompts
   - Should NOT call LLM

2. **Test Phase 3 prompt only:**
   ```bash
   cargo run -p mcc-gaql-gen -- generate "show top campaigns by cost" --generate-prompt-only --resource campaign
   ```
   - Should print Phase 3 system and user prompts
   - Should NOT call LLM

3. **Test resource override without prompt-only:**
   ```bash
   cargo run -p mcc-gaql-gen -- generate "show ad performance" --resource ad_group
   ```
   - Should skip Phase 1, use "ad_group" as resource
   - Should complete full pipeline

4. **Test invalid resource validation:**
   ```bash
   cargo run -p mcc-gaql-gen -- generate "test" --resource invalid_resource
   ```
   - Should error with "Unknown resource: 'invalid_resource'"

5. **Run existing tests:**
   ```bash
   cargo test -p mcc-gaql-gen -- --test-threads=1
   ```

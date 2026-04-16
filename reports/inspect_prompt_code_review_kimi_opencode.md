# Code Review: Prompt Inspection Implementation

**Review Date**: 2026-04-16  
**Commit**: `119647851cf133a6ba037affaf6372dac824eb0b`  
**Reviewer**: Kimi (via OpenCode)  
**Scope**: Implementation of `--generate-prompt-only` and `--resource` flags  

---

## Executive Summary

The implementation successfully adds prompt inspection capabilities to the GAQL generation pipeline. The feature is **functionally correct and backward compatible**, but contains **significant code duplication issues** that should be addressed before the feature is considered complete.

| Aspect | Rating | Notes |
|--------|--------|-------|
| Correctness | ✅ Pass | All acceptance criteria met |
| Code Quality | ⚠️ Needs Work | Significant duplication in `rag.rs` |
| Performance | ⚠️ Needs Work | Double RAG retrieval in `select_resource()` |
| Maintainability | ⚠️ Needs Work | Duplicated logic increases maintenance burden |
| Testing | ✅ Pass | All 105 existing tests pass |
| Documentation | ✅ Pass | Comprehensive implementation report |

---

## 🔴 Critical Issues

### Issue 1: Duplicate RAG Resource Retrieval

**Location**: `crates/mcc-gaql-gen/src/rag.rs`

**Problem**: The `retrieve_relevant_resources()` call and its result handling is duplicated in both `build_phase1_prompt()` and `select_resource()`. When `select_resource()` is called (normal generation path), RAG retrieval happens **twice**.

**Duplicated Code** (lines 2105-2130 and 2239-2251):

```rust
// build_phase1_prompt() - first call
let (resources, used_rag) = match self.retrieve_relevant_resources(user_query, 20).await {
    Ok(candidates) if !candidates.is_empty() => {
        let top_similarity = candidates[0].score;
        if top_similarity >= SIMILARITY_THRESHOLD {
            let names: Vec<String> = candidates.into_iter().map(|c| c.resource_name).collect();
            (names, true)
        } else {
            (self.field_cache.get_resources(), false)
        }
    }
    Ok(_) | Err(_) => (self.field_cache.get_resources(), false),
};

// select_resource() - second identical call (lines 2239-2251)
let (resources, _used_rag) = match self.retrieve_relevant_resources(user_query, 20).await {
    // IDENTICAL logic...
};
```

**Impact**:
- Wasted compute: RAG retrieval is performed twice per Phase 1
- Increased latency: Normal generation is slower due to duplicate work
- Wasted API calls: If RAG uses external services, costs increase

**Recommendation**: Return intermediate data from `build_phase1_prompt()` so `select_resource()` can reuse it.

---

### Issue 2: Duplicate Resource Info Building

**Location**: `crates/mcc-gaql-gen/src/rag.rs`, lines 2132-2153 and 2253-2272

**Problem**: The resource metadata construction loop is duplicated:

```rust
let resource_info: Vec<(String, String)> = resources
    .iter()
    .map(|r| {
        let rm = self
            .field_cache
            .resource_metadata
            .as_ref()
            .and_then(|m| m.get(r));
        let desc = rm.and_then(|m| m.description.as_deref()).unwrap_or("");

        let segment_summary = self.summarize_resource_segments(r);
        let full_desc = if segment_summary.is_empty() {
            desc.to_string()
        } else {
            format!("{} [Segments: {}]", desc, segment_summary)
        };

        (r.clone(), full_desc)
    })
    .collect();
```

**Impact**: Same computation performed twice, including segment summarization for each resource.

---

### Issue 3: Dead Code

**Location**: `crates/mcc-gaql-gen/src/rag.rs`, line 2156

**Problem**: The `resource_sample` is computed in `build_phase1_prompt()` but never used:

```rust
// build_phase1_prompt()
let _resource_sample = create_resource_sample(user_query, &resource_info);  // UNUSED
```

The actual usage is in `select_resource()` (line 2274):
```rust
let resource_sample = create_resource_sample(user_query, &resource_info);  // Actually used
```

**Impact**: Wasted computation in prompt-only mode.

---

## 🟡 Minor Issues

### Issue 4: Slightly Different Prompt Building Patterns

**Observation**: `build_phase1_prompt()` and `build_phase3_prompt()` have slightly different patterns:

- `build_phase1_prompt()` includes cookbook logic inline
- `build_phase3_prompt()` has large duplicated conditional blocks for cookbook

This is acceptable given the complexity, but could benefit from template extraction in future refactors.

### Issue 5: Unused Variable

**Location**: `crates/mcc-gaql-gen/src/rag.rs`, line 2240

```rust
let (resources, _used_rag) = ...  // _used_rag is never used
```

The underscore prefix acknowledges this, but it indicates the variable isn't needed.

### Issue 6: Unused Parameter

**Location**: `crates/mcc-gaql-gen/src/rag.rs`, line 3084

```rust
async fn build_phase3_prompt(
    &self,
    user_query: &str,
    primary: &str,  // <-- Unused
    candidates: &[FieldMetadata],
    filter_enums: &[(String, Vec<String>)],
) -> Result<(String, String), anyhow::Error> {
```

**Fix**: Prefix with underscore: `_primary: &str`

---

## ✅ Recommended Refactoring

### Option A: Return All Intermediate Data (Preferred)

Modify `build_phase1_prompt()` to return all data needed by `select_resource()`:

```rust
/// Result of building Phase 1 prompt
struct Phase1PromptResult {
    system_prompt: String,
    user_prompt: String,
    resource_info: Vec<(String, String)>,
    resource_sample: Vec<(String, String)>,
}

async fn build_phase1_prompt(&self, user_query: &str) 
    -> Result<Phase1PromptResult, anyhow::Error> 
{
    // Single RAG retrieval
    let (resources, used_rag) = self.retrieve_relevant_resources(user_query, 20).await?;
    
    // Single resource info build
    let resource_info: Vec<(String, String)> = resources
        .iter()
        .map(|r| { /* ... */ })
        .collect();
    
    // Build sample (needed by caller)
    let resource_sample = create_resource_sample(user_query, &resource_info);
    
    // Build prompts
    let (system_prompt, user_prompt) = /* ... */;
    
    Ok(Phase1PromptResult {
        system_prompt,
        user_prompt,
        resource_info,
        resource_sample,
    })
}

async fn select_resource(&self, user_query: &str) -> Result<...> {
    // Single call gets everything needed
    let result = self.build_phase1_prompt(user_query).await?;
    
    let agent = self.llm_config
        .create_agent_for_model(self.llm_config.preferred_model(), &result.system_prompt)?;
    let response = agent.prompt(&result.user_prompt).await?;
    
    // Use result.resource_sample directly - no duplicate work
    // ... rest of method
}
```

### Option B: Split Into Smaller Functions

If the struct approach feels too heavy, split the RAG retrieval into a separate cached/memoized method:

```rust
async fn get_resources_for_query(&self, user_query: &str) 
    -> Result<(Vec<String>, Vec<(String, String)>), anyhow::Error> 
{
    // Cache result per query to avoid duplicate calls
    // ...
}
```

---

## ✅ Positive Aspects

1. **Type Safety**: The `GenerateResult` enum is well-designed and provides clear type-safe handling of different return paths.

2. **Separation of Concerns**: Prompt building logic is cleanly separated from LLM invocation, making testing easier.

3. **CLI Integration**: Clean propagation of new flags from CLI args through to pipeline config.

4. **Backward Compatibility**: No breaking changes to existing API or behavior.

5. **Documentation**: Excellent implementation report with clear behavior matrix and test cases.

6. **Error Handling**: Proper use of `anyhow` for error propagation with contextual messages.

---

## Testing Verification

All 105 existing unit tests pass:
```bash
cargo test -p mcc-gaql-gen --lib -- --test-threads=1
# Result: 105 passed; 0 failed; 2 ignored
```

Manual testing scenarios pass as documented in the implementation report.

---

## Conclusion

**Verdict**: **Conditionally Approve** - Functionally correct but requires refactoring to address duplication issues.

### Required Actions Before Merge:

1. **Fix Issue 1** (Critical): Eliminate duplicate RAG retrieval
2. **Fix Issue 2** (High): Eliminate duplicate resource info building
3. **Fix Issue 3** (Medium): Remove dead code in `build_phase1_prompt()`
4. **Fix Issue 6** (Low): Prefix unused parameter with underscore

### Suggested Improvements (Can be deferred):

- Consider extracting the cookbook prompt templates to reduce duplication in `build_phase3_prompt()`
- Add unit tests specifically for the new prompt building functions

The implementation meets functional requirements but the duplication significantly impacts performance and maintainability. With the recommended refactoring, this will be a high-quality addition to the codebase.

---

**Review completed by**: Kimi K2.5 (via OpenCode)  
**Review Date**: April 16, 2026

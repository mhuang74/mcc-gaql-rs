# Code Review: Prompt Inspection Feature

**Commit:** `119647851cf133a6ba037affaf6372dac824eb0b`  
**Spec:** `specs/inspect_prompts_used_for_gaql_generation.md`  
**Reviewer:** Claude (Sonnet 4.5)  
**Date:** 2026-04-16

---

## Summary

This review covers the implementation of `--generate-prompt-only` and `--resource` flags for the `generate` command. The feature allows users to inspect LLM prompts without making API calls, useful for debugging the RAG pipeline.

**Overall Assessment: Good implementation with minor issues to address**

The implementation follows the spec closely and introduces the feature in a backward-compatible way. The code is well-structured with appropriate refactoring to extract prompt-building logic.

---

## Files Changed

| File | Lines Changed | Description |
|------|---------------|-------------|
| `crates/mcc-gaql-gen/src/main.rs` | +216/-134 | CLI flags, params struct, result handling |
| `crates/mcc-gaql-gen/src/rag.rs` | +212/-134 | PipelineConfig, GenerateResult, prompt builders |
| `reports/prompt_inspection_implementation_report.md` | +605 | Implementation documentation |
| `specs/inspect_prompts_used_for_gaql_generation.md` | +252 | Feature specification |

---

## Findings

### 1. Code Duplication in `select_resource()` (Medium Priority)

**File:** `crates/mcc-gaql-gen/src/rag.rs` (lines 2236-2274)

**Issue:** After calling `build_phase1_prompt()`, the `select_resource()` function re-computes `resources`, `resource_info`, and `resource_sample` by calling `retrieve_relevant_resources()` again. This duplicates work already done in `build_phase1_prompt()`.

```rust
async fn select_resource(&self, user_query: &str) -> Result<...> {
    // Build the Phase 1 prompt
    let (system_prompt, user_prompt) = self.build_phase1_prompt(user_query).await?;

    // Also need to compute resource_sample for the return value
    // BUG: This repeats the RAG search that build_phase1_prompt() already did
    let (resources, _used_rag) = match self.retrieve_relevant_resources(user_query, 20).await {
        // ... same logic as build_phase1_prompt() ...
    };
    // ... builds resource_info and resource_sample again ...
}
```

**Impact:** 
- Unnecessary second RAG search call (performance hit)
- Code duplication that could diverge over time

**Recommendation:** Refactor `build_phase1_prompt()` to also return the computed `resource_sample`, or extract the shared computation into a separate helper method that both functions can call. Example:

```rust
/// Returns (system_prompt, user_prompt, resource_sample)
async fn build_phase1_prompt(&self, user_query: &str) 
    -> Result<(String, String, Vec<(String, String)>), anyhow::Error>
```

---

### 2. Unused Variable Warning (Low Priority)

**File:** `crates/mcc-gaql-gen/src/rag.rs` (line 3084)

**Issue:** The `primary` parameter in `build_phase3_prompt()` is declared but not used, causing a compiler warning.

```
warning: unused variable: `primary`
    --> crates/mcc-gaql-gen/src/rag.rs:3084:9
     |
3084 |         primary: &str,
     |         ^^^^^^^ help: if this is intentional, prefix it with an underscore: `_primary`
```

**Recommendation:** Either:
- Prefix with underscore: `_primary`
- Remove if truly unnecessary
- Use it in the prompt if it should be included (the spec mentions Phase 3 prompt should include resource context)

---

### 3. Spec Deviation: Missing Resource Name in Phase 3 Prompt (Low Priority)

**File:** `crates/mcc-gaql-gen/src/rag.rs` (lines 3080-3290)

**Issue:** The spec indicates Phase 3 prompt should include the primary resource for context. The `primary` parameter is passed but not used in the prompt construction.

**From the spec (Step 5):**
> This function will:
> 1. Retrieve cookbook examples (if enabled)
> 2. Build candidate text grouped by category
> ...

The `primary` resource is passed to the function but the prompt doesn't explicitly mention which resource is being queried from.

**Recommendation:** Consider adding the primary resource name to the Phase 3 system prompt for better context:
```rust
let sys = format!(
    r#"You are a Google Ads Query Language (GAQL) expert.
Primary resource: {primary}
Given:
1. A user query
..."#
);
```

---

### 4. Unused Variable in `build_phase1_prompt()` (Low Priority)

**File:** `crates/mcc-gaql-gen/src/rag.rs` (line 2156)

**Issue:** `_resource_sample` is computed but prefixed with underscore, indicating it's intentionally unused. This is computed work that's thrown away.

```rust
// Generate sample of 5 resources (prioritizing keyword matches)
let _resource_sample = create_resource_sample(user_query, &resource_info);
```

**Recommendation:** Either remove this computation from `build_phase1_prompt()` entirely, or include the sample in the return value if it's needed elsewhere.

---

## Good Practices Observed

1. **Clean API design:** `GenerateResult` enum properly encapsulates both return types with clear documentation:
   ```rust
   pub enum GenerateResult {
       Query(GAQLResult),
       PromptOnly { system_prompt: String, user_prompt: String, phase: u8 },
   }
   ```

2. **Proper validation:** Resource override is validated against `field_cache.get_resources()` before proceeding.

3. **Backward compatibility:** Default values in `PipelineConfig` preserve existing behavior:
   ```rust
   impl Default for PipelineConfig {
       fn default() -> Self {
           Self {
               resource_override: None,
               generate_prompt_only: false,
               // ...
           }
       }
   }
   ```

4. **Clear output formatting:** The prompt-only output has good visual separation:
   ```rust
   println!("═══════════════════════════════════════════════════════════════");
   println!("               PHASE {} LLM PROMPT", phase);
   println!("═══════════════════════════════════════════════════════════════\n");
   ```

5. **Consistent error handling:** Uses `anyhow` for errors as per project conventions.

6. **Good logging:** Appropriate use of `log::info!` and `log::debug!` for traceability.

7. **Early returns:** The `generate()` method uses early returns for prompt-only mode, keeping the main code path clean.

---

## Verification Status

| Test | Status |
|------|--------|
| Code compiles | PASS (1 warning) |
| Spec alignment | PASS |
| Backward compatibility | PASS |

### Verification Commands

```bash
# Test Phase 1 prompt only
cargo run -p mcc-gaql-gen -- generate "show top campaigns by cost" --generate-prompt-only

# Test Phase 3 prompt only  
cargo run -p mcc-gaql-gen -- generate "show top campaigns by cost" --generate-prompt-only --resource campaign

# Test resource override without prompt-only
cargo run -p mcc-gaql-gen -- generate "show ad performance" --resource ad_group

# Test invalid resource validation
cargo run -p mcc-gaql-gen -- generate "test" --resource invalid_resource

# Run existing tests
cargo test -p mcc-gaql-gen -- --test-threads=1
```

---

## Recommended Actions

| Priority | Issue | Action |
|----------|-------|--------|
| Medium | Code duplication in `select_resource()` | Refactor to avoid duplicate RAG search |
| Low | Unused `primary` parameter warning | Prefix with underscore or use in prompt |
| Low | Unused `_resource_sample` computation | Remove or return from function |

---

## Conclusion

The implementation is solid and follows the spec well. The main concern is the duplicate RAG search in `select_resource()` which impacts performance. The unused variable warnings are minor but should be cleaned up for code hygiene. Overall, the feature is ready for use with these minor improvements as follow-up work.

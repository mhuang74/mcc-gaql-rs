# Plan: Improve Resource Selection Prompt to Prevent Hallucination

## Context

The LLM resource selection in Phase 1 of the GAQL generation pipeline is hallucinating resource names that don't exist. In the example provided, the user queried for "negative keywords" and the LLM returned `campaign_negative_keyword` and `negative_keyword_view` — neither of which are valid Google Ads resources.

**Root Causes:**
1. **No validation of primary_resource** — The code validates `related_resources` against `selectable_with`, but the primary resource is accepted as-is from the LLM response
2. **Missing negative keyword resources in RAG results** — The semantic search didn't surface the correct resources (`shared_criterion`, `campaign_criterion`, `ad_group_criterion`) which handle negative keywords
3. **Prompt doesn't strongly constrain the LLM** — The instruction "If the correct resource is missing, describe it" encourages hallucination rather than selecting the closest valid option

## Changes

### 1. Add Primary Resource Validation (lines 1988-2025 in rag.rs)

After parsing the LLM response, validate that `primary_resource` exists in the provided resource list. If invalid:
- Log a warning with the hallucinated resource name
- Fall back to the top RAG candidate (if RAG was used) or "campaign" as the safe default
- Include the invalid resource in the reasoning output for transparency

```rust
// After line 1991, add validation:
let all_resources = self.field_cache.get_resources();
let primary = if all_resources.contains(&primary) {
    primary
} else {
    log::warn!(
        "Phase 1: LLM returned invalid resource '{}', falling back to first candidate",
        primary
    );
    // Fall back to first RAG candidate or "campaign"
    resources.first().cloned().unwrap_or_else(|| "campaign".to_string())
};
```

### 2. Strengthen the System Prompt (lines 1941-1957)

Modify the prompt to:
- Explicitly state resources MUST come from the provided list
- Remove the instruction to "describe" missing resources (which encourages hallucination)
- Add examples of correct resource selection for ambiguous cases

**Before:**
```
Choose from the following semantically relevant resources:
Note: selected by semantic similarity to your query. If the correct resource is missing, describe it.
```

**After:**
```
IMPORTANT: You MUST select resources ONLY from the list below. Do NOT invent or hallucinate resource names.
If no resource matches perfectly, choose the closest available option and explain in reasoning.

Resources (selected by semantic similarity to your query):
```

### 3. Add Negative Keyword Context to Resource Descriptions

The valid negative keyword resources are:
- `shared_criterion` — "A criterion in a shared set, including negative keywords"
- `campaign_criterion` — "Campaign targeting/exclusion criteria including negative keywords"
- `ad_group_criterion` — "Ad group targeting/exclusion criteria including negative keywords"
- `customer_negative_criterion` — "Account-level negative targeting"
- `shared_set` — "Container for shared negative keywords across campaigns"

These may need better descriptions in the resource metadata or additional hints in the prompt for negative keyword queries.

## Files to Modify

- `crates/mcc-gaql-gen/src/rag.rs` (lines ~1933-2025)
  - Update `resource_list_header` text (line 1933-1939)
  - Add primary resource validation after JSON parsing (line 1991)

## Verification

1. Run existing tests: `cargo test -p mcc-gaql-gen -- --test-threads=1`
2. Test the specific failing case:
   ```
   cargo run -p mcc-gaql-gen -- gen "show most recently created negative keywords with highest cost in past 30 days"
   ```
3. Verify the LLM selects a valid resource like `ad_group_criterion` or `campaign_criterion` instead of hallucinating `campaign_negative_keyword`

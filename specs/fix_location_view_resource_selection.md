# Spec: Fix location_view Resource Selection in GAQL Generation

## Problem Summary

The LLM incorrectly selects `campaign` instead of `location_view` for queries asking for location-level performance data with geo target IDs. This is a **POOR** classification failure that results in completely wrong query semantics.

### Example Failure: `locations_with_highest_revenue_per_conversion`

**User Query:** "Pull performance data for top 20 locations for each campaign by rev per conv (>10 conv) last 7 days - need account and campaign info, **geo target IDs**, and conversion metrics with currency"

**LLM's Incorrect Reasoning (Phase 1):**
> "While `location_view` provides location-level performance, it aggregates metrics by the specific geo target constant (e.g., city or region) **across the account** and does not inherently support a 'top 20 per campaign' breakdown or filtering by campaign-specific conversion thresholds in a single query. The `campaign` resource is the correct primary resource because it supports all requested metrics... and allows segmentation by `segments.geo_target_city`..."

**Critical Error:** The LLM believes:
1. `location_view` aggregates "across the account" (wrong - it's segmented by campaign)
2. It can't support "top 20 per campaign" (wrong - that's exactly what it does)
3. `segments.geo_target_city` is a valid alternative (wrong - geo segments are different from location_view)

**Result:**
- Generated: `FROM campaign` (missing all geo target identity fields)
- Should be: `FROM location_view` (includes `campaign_criterion.location.geo_target_constant`)

---

## Root Cause Analysis

### 1. Phase 1 Does Not Use Cookbook Examples

**Current Flow:**
- Phase 1 (Resource Selection): Selects primary resource using only vector search of resource descriptions + guidance rules
- Phase 3 (Field Selection): Uses cookbook examples to guide field selection

**Problem:** By Phase 3, the resource is already locked. The `location_view` cookbook example showing the correct query pattern is never seen during resource selection.

### 2. Phase 1 Prompt Guidance is Insufficient

Current guidance (line 1970-1971 in `rag.rs`):
```
- For location-level performance data:
  Use `location_view` with `campaign_criterion` fields. Do NOT use `campaign` with geo segments (different granularity — campaign-level, not location-level).
```

**What's Missing:**
- No explicit statement that `location_view` ALREADY includes campaign segmentation
- No clarification that "top locations per campaign" is exactly what `location_view` provides
- No warning that `campaign_criterion.location.geo_target_constant` is ONLY available via `location_view`

### 3. No Resource-Field Validation

The query asks for "geo target IDs" which map to `campaign_criterion.location.geo_target_constant`. This field is:
- ✅ Available via `location_view`
- ❌ NOT available via `campaign`

There's no validation step that checks "if user asks for field X, ensure selected resource supports it."

---

## Proposed Solutions

### Solution 1: Add Cookbook Examples to Phase 1 (Recommended)

Include relevant cookbook examples in the Phase 1 resource selection prompt, similar to how they're used in Phase 3.

**File:** `crates/mcc-gaql-gen/src/rag.rs`, ~line 1930

**Changes:**
1. Call `retrieve_cookbook_examples()` before Phase 1 resource selection
2. Include cookbook examples in the Phase 1 system prompt
3. This shows the LLM actual `location_view` queries when relevant

**Pro:**
- LLM sees actual working examples of `location_view` queries
- Minimal code change - reuse existing `retrieve_cookbook_examples()`
- Directly addresses the root cause

**Con:**
- Increases token count for Phase 1
- May add latency

---

### Solution 2: Strengthen Phase 1 Location Guidance

Rewrite the location_view guidance to explicitly address the LLM's misconception.

**File:** `crates/mcc-gaql-gen/src/rag.rs`, ~line 1970

**Current:**
```
- For location-level performance data:
  Use `location_view` with `campaign_criterion` fields. Do NOT use `campaign` with geo segments (different granularity — campaign-level, not location-level).
```

**Proposed:**
```
- For location-level performance data (especially "locations per campaign"):
  Use `location_view` with `campaign_criterion` fields. Each row in location_view represents a
  UNIQUE COMBINATION of campaign + geo target (e.g., "Campaign A in California"), so it naturally
  supports "top locations per campaign" analysis. Do NOT use `campaign` with geo segments -
  that gives campaign-level data only, not individual location performance.
- When the user asks for "geo target IDs", they need `campaign_criterion.location.geo_target_constant`,
  which is ONLY available via `location_view` - NOT via `campaign` resource.
```

**Pro:**
- Simple text change
- Directly addresses LLM's misconception

**Con:**
- May not be sufficient on its own
- LLM may still ignore text guidance

---

### Solution 3: Add Post-Selection Validation

Add a validation step that checks if the selected resource can actually provide the fields the user requested.

**File:** `crates/mcc-gaql-gen/src/rag.rs`, after Phase 1 (~line 2020)

**Logic:**
```rust
// After selecting primary resource, verify it supports key fields from query
if user_query mentions "geo target" OR "location ID" OR "geo_target_constant" {
    if primary_resource != "location_view" {
        log::warn!("User asked for geo target IDs but resource {} was selected", primary_resource);
        // Either auto-correct or flag for Phase 3 to handle
        if field_cache.get_resource_selectable_with("location_view").contains("campaign_criterion.location.geo_target_constant") {
            primary_resource = "location_view".to_string();
            log::info!("Auto-corrected to location_view to support geo target fields");
        }
    }
}
```

**Pro:**
- Failsafe even if LLM makes wrong choice
- Can auto-correct common misclassifications

**Con:**
- More complex implementation
- Requires parsing user query for intent
- May have false positives

---

## Recommended Approach: Combined Solution 1 + 2

Apply both **Solution 1** (cookbook in Phase 1) and **Solution 2** (stronger guidance).

### Implementation Plan

#### Change 1: Add Cookbook Retrieval to Phase 1

**File:** `crates/mcc-gaql-gen/src/rag.rs`

**Location:** Inside `select_resource_via_llm()` method, before building the prompt (~line 1935)

**Code Changes:**
```rust
// Retrieve cookbook examples for resource selection (if enabled)
let cookbook_examples = if self.pipeline_config.use_query_cookbook {
    log::debug!("Phase 1: Retrieving cookbook examples for resource selection...");
    match self.retrieve_cookbook_examples(user_query, 2).await {
        Ok(examples) => {
            if !examples.is_empty() {
                log::debug!("Phase 1: Retrieved cookbook examples for resource selection");
                format!("\n\nSimilar Query Examples from Cookbook:\n{}", examples)
            } else {
                String::new()
            }
        }
        Err(e) => {
            log::warn!("Phase 1: Failed to retrieve cookbook examples: {}", e);
            String::new()
        }
    }
} else {
    String::new()
};
```

Then add `cookbook_examples` to the system prompt format string.

#### Change 2: Strengthen Location Guidance

**File:** `crates/mcc-gaql-gen/src/rag.rs`, line ~1970

Replace the current location guidance with:

```rust
- For location-level performance data ("top locations", "best performing regions", "geo performance"):
  Use `location_view` with `campaign_criterion` fields. Each row represents a UNIQUE COMBINATION of
  campaign + geo target, so it naturally supports "top locations per campaign" analysis. The
  `campaign_criterion.location.geo_target_constant` field provides the geo target ID.
  Do NOT use `campaign` - it cannot provide individual location performance or geo target IDs.
```

#### Change 3: Add Identity Fields for location_view

**File:** `crates/mcc-gaql-gen/src/rag.rs`

The Phase 3 identity field injection should explicitly include `campaign_criterion` fields when `location_view` is selected:

```rust
// In Phase 3 identity field injection logic:
"location_view" => vec![
    "customer.id",
    "customer.descriptive_name",
    "campaign.id",
    "campaign.name",
    "campaign_criterion.criterion_id",
    "campaign_criterion.location.geo_target_constant",
]
```

---

## Verification

### Test Case 1: locations_with_highest_revenue_per_conversion

**Input:** "Pull performance data for top 20 locations for each campaign by rev per conv (>10 conv) last 7 days - need account and campaign info, geo target IDs, and conversion metrics with currency"

**Expected Output:**
- Phase 1 selects: `location_view`
- Phase 3 includes: `campaign_criterion.criterion_id`, `campaign_criterion.type`, `campaign_criterion.location.geo_target_constant`
- Final FROM clause: `FROM location_view`

### Test Case 2: Another location query

**Input:** "Show me top 10 cities by conversions for each campaign this month"

**Expected Output:**
- Phase 1 selects: `location_view`
- Query includes geo target fields

### Regression Test: Campaign-level queries

**Input:** "Show me campaign performance last week"

**Expected Output:**
- Phase 1 selects: `campaign` (unchanged)
- No false positives from strengthened guidance

---

## Files to Modify

| File | Line(s) | Change |
|------|---------|--------|
| `crates/mcc-gaql-gen/src/rag.rs` | ~1880-2020 | Add cookbook retrieval to Phase 1 |
| `crates/mcc-gaql-gen/src/rag.rs` | ~1970-1971 | Strengthen location_view guidance |
| `crates/mcc-gaql-gen/src/rag.rs` | ~2408 | Ensure location_view identity fields include campaign_criterion fields |

---

## Notes

- The user noted: "Maybe we should validate LLM's reasoning. Maybe cookbook and prompt is wrong."
- The cookbook query IS correct - it uses `location_view` with `campaign_criterion` fields
- The problem is purely in how the LLM interprets the user's intent vs. the available resources
- Adding cookbook examples to Phase 1 will let the LLM see the correct pattern during resource selection

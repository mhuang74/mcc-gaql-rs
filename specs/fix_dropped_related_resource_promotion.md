# Fix: Dropped Related Resource Should Trigger Primary Resource Promotion

**Date:** 2026-04-01
**Report:** `reports/query_cookbook_gen_comparison.20260331160635.md`
**Scope:** Issue #1 — `accounts_with_asset_call_last_week` classified POOR (wrong resource)

## Problem

The `accounts_with_asset_call_last_week` query generates `FROM campaign` instead of the correct `FROM campaign_asset`. Despite domain knowledge explicitly instructing the LLM to use `campaign_asset` as the primary resource, the LLM sometimes returns `campaign` as primary with `campaign_asset` as a related resource. The pipeline then silently drops `campaign_asset` from related resources because it fails the `selectable_with` validation, producing a degraded query with no asset-level fields.

### Symptoms

From the March 31 comparison report:

| Aspect | Expected | Generated |
|--------|----------|-----------|
| FROM clause | `campaign_asset` | `campaign` |
| Asset fields | `asset.id`, `asset.name`, `asset.type`, `asset.call_asset.phone_number` | Missing entirely |
| Asset filter | `campaign_asset.field_type = 'CALL'` | Missing entirely |
| Classification | — | POOR |

System warnings in the pipeline trace:
```
Phase 3: Rejecting select field 'campaign_asset.asset' - not valid for resource 'campaign'
Phase 3: Rejecting filter field 'campaign_asset.field_type' - not valid for resource 'campaign'
```

### History

This is a **regression**. The March 27-28 runs correctly selected `campaign_asset` after domain knowledge was added (commit `c09e498`). The March 31 run regressed, demonstrating that prompt-based guidance alone is insufficient for reliable resource selection.

## Root Cause Analysis

### Code Path Trace

The failure occurs across three phases of the pipeline in `crates/mcc-gaql-gen/src/rag.rs`:

**Phase 1: Resource Selection** (`select_resource`, line 1946)

1. **RAG pre-filter** (line 1959): Vector similarity search retrieves 20 candidate resources. Both `campaign` and `campaign_asset` are in the candidate list.

2. **Domain knowledge injection** (line 2042): The `Resource Selection Guidance` section from `resources/domain_knowledge.md` is included in the system prompt. It explicitly states: *"For asset extension performance: Use `campaign_asset` with a `campaign_asset.field_type` filter. Do NOT use `campaign`."*

3. **LLM response** (line 2080): Despite the guidance, the LLM returns:
   ```json
   {
     "primary_resource": "campaign",
     "related_resources": ["campaign_asset"],
     "reasoning": "Need campaign_asset for phone numbers..."
   }
   ```
   The LLM's *reasoning* correctly identifies `campaign_asset` is needed, but it incorrectly places it as a related resource rather than the primary.

4. **Related resource validation** (lines 2126-2139):
   ```rust
   let selectable_with = self.field_cache.get_resource_selectable_with(&primary);
   let validated_related: Vec<String> = related
       .into_iter()
       .filter(|r| {
           if selectable_with.contains(r) {
               true
           } else {
               dropped.push(r.clone());
               false
           }
       })
       .collect();
   ```
   `campaign`'s `selectable_with` does NOT include `campaign_asset` (they are separate GAQL resources with different FROM semantics), so `campaign_asset` is silently dropped.

**Phase 2: Field Candidate Retrieval** (`retrieve_field_candidates`, line 2154)

Field candidates are scoped to the primary resource (`campaign`) and validated related resources. Since `campaign_asset` was dropped, no `campaign_asset.*` or `asset.*` fields enter the candidate pool.

**Phase 3: Field Selection** (`select_fields`, line 2970)

The LLM attempts to include asset fields but they fail validation:
```rust
if !candidate_names.contains(s) && !valid_fields.contains(s) {
    log::warn!(
        "Phase 3: Rejecting select field '{}' - not valid for resource '{}'",
        s, primary
    );
    false
}
```

### Why This Happens

The fundamental issue is an **asymmetric containment relationship** in GAQL resources:

- `FROM campaign_asset` → can query `campaign.*`, `asset.*`, `campaign_asset.*`, `metrics.*`, `customer.*`
- `FROM campaign` → can query `campaign.*`, `metrics.*`, `customer.*` — but **NOT** `campaign_asset.*` or `asset.*`

`campaign_asset` is a "wider" resource that contains `campaign` as a subset. When the LLM gets the direction wrong (putting the wider resource as related instead of primary), the validation correctly drops it — but the pipeline has no mechanism to detect that this drop signals a primary resource error.

### `get_resource_selectable_with` Implementation

File: `crates/mcc-gaql-common/src/field_metadata.rs`, line 537:
```rust
pub fn get_resource_selectable_with(&self, resource: &str) -> Vec<String> {
    self.fields
        .get(resource)
        .filter(|f| f.is_resource())
        .map(|f| f.selectable_with.clone())
        .unwrap_or_default()
}
```

This returns the list of fields that can appear in a SELECT/WHERE clause when using the given resource in FROM. The key insight: if `campaign` is in `campaign_asset`'s `selectable_with`, then `campaign_asset` as primary can access all of `campaign`'s data — meaning promotion from related to primary is safe.

## Proposed Solution: Dropped Resource Promotion

### Approach

When related resources are dropped during validation, check if any dropped resource would be a **better primary** — specifically, one whose `selectable_with` includes the current primary. If so, promote the dropped resource to primary and demote the old primary to related.

This is a deterministic, data-driven correction that:
- Does not depend on LLM compliance with prompt instructions
- Generalizes to any resource pair with asymmetric containment (e.g., `ad_group_asset` vs `ad_group`, `customer_asset` vs `customer`)
- Uses existing metadata (`selectable_with`) — no new data sources needed

### Code Change 1: Promotion Logic in `select_resource`

**File:** `crates/mcc-gaql-gen/src/rag.rs`
**Location:** After the existing validation loop (after line 2139), before the `Ok((...))` return.

```rust
// --- Dropped resource promotion ---
// If related resources were dropped (incompatible with primary), check whether
// a dropped resource should actually BE the primary. This handles cases where
// the LLM gets the FROM direction wrong (e.g., returns "campaign" as primary
// with "campaign_asset" as related, when "campaign_asset" should be primary).
let (primary, validated_related) = if !dropped.is_empty() {
    let mut promoted = None;
    for candidate in &dropped {
        let candidate_selectable = self.field_cache.get_resource_selectable_with(candidate);
        // If the dropped resource's selectable_with contains the current primary,
        // then the dropped resource is a "wider" resource that subsumes the primary.
        if candidate_selectable.contains(&primary) {
            promoted = Some(candidate.clone());
            break; // First qualifying candidate (preserves LLM priority order)
        }
    }
    if let Some(new_primary) = promoted {
        log::warn!(
            "Phase 1: Promoting dropped resource '{}' to primary \
             (original primary '{}' becomes related). \
             Reason: '{}' selectable_with contains '{}'.",
            new_primary, primary, new_primary, primary
        );
        let new_selectable = self.field_cache.get_resource_selectable_with(&new_primary);
        let mut new_related: Vec<String> = validated_related
            .into_iter()
            .filter(|r| new_selectable.contains(r))
            .collect();
        if new_selectable.contains(&primary) {
            new_related.push(primary);
        }
        dropped.retain(|r| r != &new_primary);
        (new_primary, new_related)
    } else {
        (primary, validated_related)
    }
} else {
    (primary, validated_related)
};
```

### Code Change 2: Strengthen Domain Knowledge Prompt

**File:** `resources/domain_knowledge.md`, lines 5-6

**Before:**
```markdown
- For asset extension performance (sitelinks, callouts, calls, structured snippets):
  Use `campaign_asset` with a `campaign_asset.field_type` filter. Do NOT use `campaign` (no asset-level data) or `call_view` (individual call records, not asset metrics).
```

**After:**
```markdown
- For asset extension performance (sitelinks, callouts, calls, structured snippets):
  **primary_resource must be `campaign_asset`** with a `campaign_asset.field_type` filter.
  Do NOT use `campaign` as primary (it cannot access asset-level fields like `asset.call_asset.phone_number`).
  Do NOT use `call_view` (individual call records, not asset metrics).
  Do NOT put `campaign_asset` in related_resources under `campaign` — it must be the primary_resource.
```

## Edge Cases

| Scenario | Behavior | Correct? |
|----------|----------|----------|
| LLM correctly selects `campaign_asset` as primary | No drops, no promotion triggered | Yes |
| LLM selects `campaign` with `campaign_asset` related | `campaign_asset` dropped → promoted to primary | Yes |
| LLM selects `campaign` with unrelated resource dropped | Dropped resource's selectable_with doesn't contain `campaign` → no promotion | Yes |
| Multiple dropped resources qualify | First one wins (LLM's implicit priority order) | Acceptable |
| Circular: promotion changes validated_related compatibility | Re-validation filters incompatible related resources | Yes |

## Implementation Plan

### Step 1: Add promotion logic (~30 lines)

**File:** `crates/mcc-gaql-gen/src/rag.rs`
- Insert the promotion block after line 2139
- The `primary` and `validated_related` variables are already shadowed via `let` bindings in the existing code, so the pattern is consistent

**Effort:** Small — single insertion point, no structural refactoring.

### Step 2: Strengthen domain knowledge prompt (~3 lines)

**File:** `resources/domain_knowledge.md`
- Update the asset extension bullet to be more explicit about `primary_resource` vs `related_resources`

**Effort:** Trivial.

### Step 3: Verification

1. **Unit test:** Add a test in `rag.rs` (or a separate test module) that mocks `get_resource_selectable_with` and verifies:
   - Dropped resource with containment → promoted
   - Dropped resource without containment → not promoted
   - No drops → no change

2. **Integration test:** Run the cookbook comparison test and verify `accounts_with_asset_call_last_week` produces `FROM campaign_asset` with the correct asset fields.

3. **Regression check:** Verify the other 46 cookbook entries are unaffected (especially the callout/sitelink entries that were already passing).

**Verification commands:**
```bash
cargo check -p mcc-gaql-gen
cargo test -p mcc-gaql-gen --lib -- --test-threads=1
# Then run the full cookbook comparison test to confirm the fix
```

### Step 4: Related improvements (optional, not in scope)

- Add a `--explain-selection-process` trace line for promotions so users can see when it fires
- Consider adding the promotion count to the `PipelineTrace` struct for observability

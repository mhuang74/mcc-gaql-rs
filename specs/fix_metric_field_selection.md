# Fix: Metrics Not Retrieved in RAG Field Selection

## Context

When querying "select best performing audience from Chinese New Year 2026", the LLM reasoning indicates:
> "Performance metrics (e.g., clicks, conversions) were not available in the provided field list, so sorting by 'best performing' could not be applied."

This indicates metrics retrieved via vector search are being incorrectly filtered out.

## Root Cause Analysis

The bug is in `retrieve_field_candidates()` in `crates/mcc-gaql-gen/src/rag.rs`:

### Inconsistent Filtering Logic

**Tier 1 - `key_metrics` from ResourceMetadata (lines 1372-1379):**
```rust
for metric in &rm.key_metrics {
    if let Some(field) = self.field_cache.fields.get(metric)
        && seen.insert(field.name.clone())
    {
        candidates.push(field.clone());  // NO selectable_with check!
    }
}
```

**Tier 2 - Vector-searched metrics (lines 1476-1489):**
```rust
if (doc.category == "METRIC" || doc.id.starts_with("metrics."))
    && let Some(field) = self.field_cache.fields.get(&doc.id)
{
    // Metrics are compatible if their field name is in the resource's selectable_with
    if selectable_with.contains(&field.name) && seen.insert(field.name.clone()) {
        candidates.push(field.clone());  // HAS selectable_with check - BUG!
    }
}
```

**The Problem:**
1. `get_resource_selectable_with()` (line 1351) gets `selectable_with` from `self.fields[resource].selectable_with` (the RESOURCE field's selectable_with)
2. This comes from `FieldMetadata.selectable_with` (populated from Google Ads API)
3. If this data is incomplete or empty, vector-searched metrics are filtered out
4. Meanwhile, `key_metrics` bypass this check entirely and are always included

### Why the LLM Didn't Get Metrics

1. The query "best performing audience from Chinese New Year 2026" likely selected `ad_group_criterion` as the primary resource (for audiences)
2. The `ad_group_criterion` RESOURCE field's `selectable_with` may be empty or incomplete
3. Vector search returned metrics (conversions, cost, roas) matching "best performing"
4. But they were filtered out by `selectable_with.contains(&field.name)` check
5. `key_metrics` for `ad_group_criterion` either don't include performance metrics or aren't populated

## Proposed Fix

Make Tier 2 metric filtering consistent with Tier 1: remove the `selectable_with` check for metrics returned from vector search.

### Rationale
- Metrics returned from vector search are already semantically relevant to the query
- The LLM in Phase 3 will validate field selection and can reject incompatible fields
- Consistency with how `key_metrics` are handled
- Google Ads API will reject truly incompatible fields at query time anyway

### Changes Required

**File:** `crates/mcc-gaql-gen/src/rag.rs`

**Lines 1480-1489:** Remove the `selectable_with.contains(&field.name)` check:

```rust
// BEFORE:
if (doc.category == "METRIC" || doc.id.starts_with("metrics."))
    && let Some(field) = self.field_cache.fields.get(&doc.id)
{
    // Metrics are compatible if their field name is in the resource's selectable_with
    if selectable_with.contains(&field.name) && seen.insert(field.name.clone()) {
        candidates.push(field.clone());
    }
}

// AFTER:
if (doc.category == "METRIC" || doc.id.starts_with("metrics."))
    && let Some(field) = self.field_cache.fields.get(&doc.id)
    && seen.insert(field.name.clone())
{
    candidates.push(field.clone());
}
```

**Also apply same fix to segments (lines 1491-1504):**

```rust
// BEFORE:
if (doc.category == "SEGMENT" || doc.id.starts_with("segments."))
    && let Some(field) = self.field_cache.fields.get(&doc.id)
{
    // Segments are compatible if their field name is in the resource's selectable_with
    if selectable_with.contains(&field.name) && seen.insert(field.name.clone()) {
        candidates.push(field.clone());
    }
}

// AFTER:
if (doc.category == "SEGMENT" || doc.id.starts_with("segments."))
    && let Some(field) = self.field_cache.fields.get(&doc.id)
    && seen.insert(field.name.clone())
{
    candidates.push(field.clone());
}
```

## Alternative Approaches Considered

1. **Fix `selectable_with` population** - Would require changes to enrichment pipeline, slower fix
2. **Add fallback when `selectable_with` is empty** - More complex, still inconsistent
3. **Always include all metrics** - Could overwhelm the LLM with too many candidates

The proposed fix is simplest and most consistent with existing behavior.

## Verification

After the fix:
1. Run `cargo test -p mcc-gaql-gen -- --test-threads=1` to ensure tests pass
2. Test query: "select best performing audience from Chinese New Year 2026"
3. Verify the LLM prompt includes metrics in the candidate list
4. Verify the generated GAQL can include ORDER BY with metrics

## Files to Modify

- `crates/mcc-gaql-gen/src/rag.rs` (lines 1476-1504)

---

# Plan Option 2: Fix enricher.rs Key Field Selection

## Context

The enricher already has logic to populate `key_metrics` for all resources including views. The `select_key_fields_for_resource()` function (lines 505-638) correctly retrieves metrics from `selectable_with` and uses an LLM to select the most important ones, with an alphabetical fallback.

However, the enrichment pipeline may not be running correctly or the fallback may not be triggering properly. This plan addresses potential issues in the enrichment process.

## Current Behavior

1. `select_key_fields_for_resource()` (lines 518-524) gets metrics from `selectable_with`:
   ```rust
   let resource_metrics: Vec<String> = selectable_with
       .iter()
       .filter(|f| f.starts_with("metrics."))
       .cloned()
       .collect();
   ```

2. Lines 631-635 have a fallback when LLM returns empty:
   ```rust
   if key_metrics.is_empty() && !resource_metrics.is_empty() {
       let mut sorted_metrics = resource_metrics.clone();
       sorted_metrics.sort();
       key_metrics = sorted_metrics.into_iter().take(10).collect();
   }
   ```

## Potential Issues

1. **Enrichment not run**: User may be using a cache created before enrichment was added
2. **Silent failures**: Errors in `select_key_fields_for_resource()` are logged but don't fail the pipeline
3. **Empty `selectable_with`**: The resource's RESOURCE field may have empty selectable_with

## Proposed Fix

### Changes to `crates/mcc-gaql-gen/src/enricher.rs`

**1. Add validation after key field selection (around line 191)**

Add a check to warn if key_metrics is still empty after enrichment:

```rust
// After key field selection loop (line 191)
let empty_key_metrics: Vec<String> = cache
    .resource_metadata
    .as_ref()
    .map(|m| {
        m.iter()
            .filter(|(_, rm)| rm.key_metrics.is_empty())
            .map(|(name, _)| name.clone())
            .collect()
    })
    .unwrap_or_default();

if !empty_key_metrics.is_empty() {
    log::warn!(
        "Resources with empty key_metrics after enrichment: {:?}",
        empty_key_metrics
    );
}
```

**2. Fix the fallback logic in `select_key_fields_for_resource()` (lines 631-635)**

The current fallback only triggers if `key_metrics.is_empty()` after LLM parsing. But if `parsed.get("key_metrics")` returns `Some` (even an empty array), the fallback won't trigger. Change the logic:

```rust
// BEFORE (lines 612-635):
let mut key_metrics: Vec<String> = parsed
    .get("key_metrics")
    .and_then(|v| v.as_array())
    .map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .filter(|s| resource_metrics.contains(s))
            .take(10)
            .collect()
    })
    .unwrap_or_default();

// Fallback: if LLM returned nothing valid, use alphabetical first-N
if key_metrics.is_empty() && !resource_metrics.is_empty() {
    let mut sorted_metrics = resource_metrics.clone();
    sorted_metrics.sort();
    key_metrics = sorted_metrics.into_iter().take(10).collect();
}

// AFTER:
let mut key_metrics: Vec<String> = parsed
    .get("key_metrics")
    .and_then(|v| v.as_array())
    .map(|arr| {
        arr.iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .filter(|s| resource_metrics.contains(s))
            .take(10)
            .collect()
    })
    .unwrap_or_default();

// Fallback: if LLM returned nothing valid OR resource has no own metrics, use selectable_with
if key_metrics.is_empty() && !resource_metrics.is_empty() {
    // For views (which have no "own" metrics), use top metrics from selectable_with
    let mut sorted_metrics = resource_metrics.clone();
    sorted_metrics.sort_by(|a, b| {
        // Prioritize common metrics first
        let a_priority = Self::metric_priority(a);
        let b_priority = Self::metric_priority(b);
        a_priority.cmp(&b_priority).then_with(|| a.cmp(b))
    });
    key_metrics = sorted_metrics.into_iter().take(10).collect();
    log::debug!(
        "Using fallback key_metrics for {}: {:?}",
        resource,
        key_metrics
    );
}
```

**3. Add a helper function for metric prioritization** (add after line 638):

```rust
/// Priority for common metrics (lower = higher priority)
fn metric_priority(metric: &str) -> u8 {
    match metric {
        "metrics.clicks" => 1,
        "metrics.impressions" => 2,
        "metrics.cost_micros" => 3,
        "metrics.conversions" => 4,
        "metrics.conversion_value" => 5,
        "metrics.all_conversions" => 6,
        "metrics.average_cpc" => 7,
        "metrics.ctr" => 8,
        "metrics.roas" => 9,
        "metrics.cost_per_conversion" => 10,
        _ => 100, // Unknown metrics get lower priority
    }
}
```

## Pros and Cons

**Pros:**
- Fixes the issue at the enrichment level
- Properly populates key_metrics for views
- Better prioritization of important metrics

**Cons:**
- Requires re-running enrichment (time-consuming, uses LLM tokens)
- Doesn't fix existing non-enriched caches
- More complex than the rag.rs fix

## Verification

1. Run enrichment: `cargo run -p mcc-gaql-gen -- enrich --refresh`
2. Check that `ad_group_audience_view` has populated key_metrics
3. Run query test: "select best performing audience from Chinese New Year 2026"
4. Verify metrics appear in candidate list

---

# Plan Option 3: Fix mcc-gaql Initial Metadata Creation

## Context

The root cause of empty `key_metrics` for views is in the initial metadata creation in `mcc-gaql`. The `build_resource_metadata_from_fields()` function (lines 176-225) only considers fields that "belong" to the resource (via `get_resource()`), but metrics belong to the "metrics" resource, not the view resource.

## Current Behavior

In `build_resource_metadata_from_fields()` (lines 196-203):
```rust
// Collect key metrics (selectable)
let mut key_metrics: Vec<String> = resource_fields
    .iter()
    .filter(|f| f.is_metric() && f.selectable)
    .take(10)
    .map(|f| f.name.clone())
    .collect();
```

For a view like `ad_group_audience_view`:
- `resource_fields` contains only fields where `get_resource() == "ad_group_audience_view"`
- Metrics have `get_resource() == "metrics"`
- So no metrics are ever added to `key_metrics`

## Proposed Fix

### Changes to `crates/mcc-gaql/src/field_metadata.rs`

**Modify `build_resource_metadata_from_fields()` (around line 196)**

Replace the key_metrics collection logic to also include metrics from selectable_with:

```rust
// BEFORE (lines 196-203):
// Collect key metrics (selectable)
let mut key_metrics: Vec<String> = resource_fields
    .iter()
    .filter(|f| f.is_metric() && f.selectable)
    .take(10)
    .map(|f| f.name.clone())
    .collect();
key_metrics.sort();

// AFTER:
// Collect key metrics (selectable)
// For views and other resources, also include metrics from selectable_with
let own_metrics: Vec<String> = resource_fields
    .iter()
    .filter(|f| f.is_metric() && f.selectable)
    .map(|f| f.name.clone())
    .collect();

let mut key_metrics = if own_metrics.is_empty() && !selectable_with.is_empty() {
    // For views with no own metrics, use metrics from selectable_with
    // Prioritize common metrics
    let priority_metrics = [
        "metrics.clicks",
        "metrics.impressions",
        "metrics.cost_micros",
        "metrics.conversions",
        "metrics.conversion_value",
        "metrics.all_conversions",
        "metrics.average_cpc",
        "metrics.ctr",
        "metrics.roas",
        "metrics.cost_per_conversion",
    ];

    let mut prioritized: Vec<String> = priority_metrics
        .iter()
        .filter(|m| selectable_with.contains(&m.to_string()))
        .map(|&s| s.to_string())
        .collect();

    // Add any remaining selectable metrics alphabetically
    let mut remaining: Vec<String> = selectable_with
        .iter()
        .filter(|f| f.starts_with("metrics.") && !prioritized.contains(f))
        .cloned()
        .collect();
    remaining.sort();

    // Combine and limit
    prioritized.extend(remaining);
    prioritized.into_iter().take(10).collect()
} else {
    own_metrics
};

key_metrics.sort();
```

## Pros and Cons

**Pros:**
- Fixes the issue at the source
- Works for all new caches without requiring enrichment
- Consistent behavior across all resources
- No LLM tokens required

**Cons:**
- Requires regenerating the cache (users need to delete and re-fetch)
- Hardcoded metric priority list needs maintenance
- Doesn't help with existing caches

## Verification

1. Delete existing cache: `rm ~/Library/Caches/mcc-gaql/field_metadata.json`
2. Re-fetch metadata: `cargo run -p mcc-gaql -- query "test"` (will trigger fetch)
3. Check that `ad_group_audience_view` has populated key_metrics
4. Run query test without enrichment
5. Verify metrics appear in candidate list

---

# Summary Comparison

| Approach | File Modified | Pros | Cons | When to Use |
|----------|--------------|------|------|-------------|
| **Option 1** (rag.rs fallback) | `mcc-gaql-gen/src/rag.rs` | Works immediately, no cache refresh needed | Workaround, not root cause fix | Quick fix, immediate relief |
| **Option 2** (enricher.rs) | `mcc-gaql-gen/src/enricher.rs` | Proper fix at enrichment level | Requires re-enrichment (LLM tokens) | If enrichment is regularly used |
| **Option 3** (mcc-gaql) | `mcc-gaql/src/field_metadata.rs` | Root cause fix, no LLM needed | Requires cache regeneration | Best long-term solution |

**Recommendation**: Implement Option 1 (immediate fix) + Option 3 (root cause fix) for a complete solution.

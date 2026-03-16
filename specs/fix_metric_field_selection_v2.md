# Fix: Populate key_metrics for Views and Add selectable_with Validation

**Status:** Plan updated - Option 3 is the PRIMARY fix, Option 1 (temporary workaround) is REMOVED, error detection is REQUIRED.

## Context

When querying "select best performing audience from Chinese New Year 2026", the LLM reasoning indicates:
> "Performance metrics (e.g., clicks, conversions) were not available in the provided field list, so sorting by 'best performing' could not be applied."

This happens because view resources like `ad_group_audience_view` have empty `key_metrics` in their `ResourceMetadata`.

## Root Cause Analysis

### Why key_metrics is Empty for Views

The `build_resource_metadata_from_fields()` function in `mcc-gaql` (lines 196-203) only considers fields that "belong" to the resource:

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
- Metrics have `get_resource() == "metrics"`, not the view name
- So no metrics are ever added to `key_metrics`

### selectable_with Data Issue

The `selectable_with` field comes from the Google Ads API's RESOURCE-category field. If this data is:
- Missing from the API response
- Not properly parsed
- Empty for certain resource types

Then `get_resource_selectable_with()` returns an empty vector, and field validation cannot work correctly.

## Proposed Fix

### Primary Fix: Update Initial Metadata Creation (Option 3)

Modify `build_resource_metadata_from_fields()` in `crates/mcc-gaql/src/field_metadata.rs` to populate `key_metrics` from `selectable_with` for resources that don't have their own metrics.

### Error Detection: Fail Fast on Empty selectable_with

Add validation to detect when `selectable_with` is not properly populated and fail fast instead of silently continuing with broken validation.

## Changes Required

### 1. Fix mcc-gaql/src/field_metadata.rs

**Location:** `build_resource_metadata_from_fields()` around line 196

Replace the key_metrics collection logic:

```rust
// BEFORE:
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

### 2. Add selectable_with Validation (Fail Fast)

**Add validation function in mcc-gaql-common/src/field_metadata.rs:**

Add a method to `FieldMetadataCache` that validates all resources have proper `selectable_with`:

```rust
/// Validate that all resources have properly populated selectable_with
/// Returns error with list of resources that have empty selectable_with
pub fn validate_selectable_with(&self) -> Result<(), Vec<String>> {
    let mut empty_resources: Vec<String> = Vec::new();

    for resource_name in self.get_resources() {
        let selectable_with = self.get_resource_selectable_with(&resource_name);
        if selectable_with.is_empty() {
            empty_resources.push(resource_name);
        }
    }

    if empty_resources.is_empty() {
        Ok(())
    } else {
        Err(empty_resources)
    }
}

/// Check if a specific resource has populated selectable_with
pub fn has_selectable_with(&self, resource: &str) -> bool {
    !self.get_resource_selectable_with(resource).is_empty()
}
```

**Add validation call in mcc-gaql/src/field_metadata.rs after fetching:**

After `fetch_from_api()` builds the cache (around line 165), add:

```rust
// Validate that selectable_with is populated for all resources
if let Err(empty_resources) = cache.validate_selectable_with() {
    log::error!(
        "CRITICAL: {} resources have empty selectable_with: {:?}",
        empty_resources.len(),
        empty_resources
    );
    return Err(anyhow::anyhow!(
        "Field metadata cache has {} resources with empty selectable_with. \
         This will break field compatibility validation. \
         Resources affected: {:?}",
        empty_resources.len(),
        empty_resources
    ));
}
```

**Add runtime validation in mcc-gaql-gen/src/rag.rs:**

In `retrieve_field_candidates()` (around line 1351 where it calls `get_resource_selectable_with`), add:

```rust
let selectable_with = self.field_cache.get_resource_selectable_with(resource);

// Fail fast if selectable_with is empty - indicates metadata corruption
if selectable_with.is_empty() {
    return Err(anyhow::anyhow!(
        "Resource '{}' has empty selectable_with. \
         This indicates the field metadata cache was not properly populated. \
         Please regenerate the cache by deleting {:?} and re-running.",
        resource,
        self.cache_path
    ));
}
```

### 3. Revert/Remove rag.rs Temporary Workaround (REQUIRED)

**The temporary workaround in rag.rs (Option 1) MUST be reverted.**

The filtering logic in `retrieve_field_candidates()` should keep the `selectable_with` check:

```rust
// The filtering logic should remain:
if (doc.category == "METRIC" || doc.id.starts_with("metrics."))
    && let Some(field) = self.field_cache.fields.get(&doc.id)
{
    // KEEP the selectable_with check - it should work now that key_metrics is populated
    if selectable_with.contains(&field.name) && seen.insert(field.name.clone()) {
        candidates.push(field.clone());
    }
}
```

The filtering is correct - the problem was that `key_metrics` was empty, not that the filtering was wrong.

## Verification

1. Delete existing cache: `rm ~/Library/Caches/mcc-gaql/field_metadata.json`
2. Run query to trigger metadata fetch: `cargo run -p mcc-gaql -- query "test"`
3. Verify no error about empty selectable_with
4. Check that `ad_group_audience_view` has populated key_metrics
5. Run query test: "select best performing audience from Chinese New Year 2026"
6. Verify metrics appear in candidate list

## Files to Modify

1. `crates/mcc-gaql/src/field_metadata.rs` - Fix key_metrics population + add validation
2. `crates/mcc-gaql-common/src/field_metadata.rs` - Add `validate_selectable_with()` method
3. `crates/mcc-gaql-gen/src/rag.rs` - Add runtime validation (optional but recommended)

## Rollback Plan

If the fix causes issues:
1. The validation errors will point to exactly which resources have problems
2. Users can manually populate key_metrics for specific resources if needed
3. The validation can be converted to a warning instead of an error

---

# Alternative: Fix enricher.rs (Option 2) - DEPRECATED

**Note:** This approach is kept for reference but Option 3 is preferred as it fixes the issue at the source without requiring re-enrichment.

The enricher already has logic to populate `key_metrics` from `selectable_with` in `select_key_fields_for_resource()` (lines 518-524). However:
1. It requires running enrichment (uses LLM tokens)
2. It doesn't fix existing non-enriched caches
3. Silent failures in enrichment may leave key_metrics empty

**Recommendation:** Skip Option 2, implement Option 3 for root cause fix.

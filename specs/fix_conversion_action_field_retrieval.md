# Fix: Conversion Action Field Retrieval Failure

**Date:** 2026-04-01
**Report:** `reports/query_cookbook_gen_comparison.20260331160635.md`
**Scope:** Issue #2 — `conversion_actions_performance` classified POOR (44% field coverage)

## Problem

The `conversion_actions_performance` query generates only 4 fields (44% coverage) when the reference query has 9 fields. Critical missing fields:
- `conversion_action.type` — explicitly mentioned in user query intent
- `conversion_action.category` — explicitly mentioned in user query intent
- `metrics.conversions` — explicitly mentioned ("need conversions")
- `metrics.conversions_value` — explicitly mentioned ("conversion value")
- `customer.currency_code` — standard context field

### Symptoms

| Aspect | Reference Query | Generated Query |
|--------|-----------------|-----------------|
| SELECT fields | 9 fields | 4 fields |
| conversion_action fields | id, name, **type, category** | id, name only |
| metrics fields | conversions, conversions_value, all_conversions | all_conversions only |
| Classification | — | POOR |

**Generated Query (actual):**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  conversion_action.id,
  conversion_action.name,
  metrics.all_conversions
FROM conversion_action
WHERE
```

**Reference Query (expected):**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  conversion_action.name,
  conversion_action.type,
  conversion_action.category,
  metrics.conversions,
  metrics.conversions_value,
  metrics.all_conversions
FROM conversion_action
WHERE segments.date DURING LAST_30_DAYS
  AND conversion_action.status = 'ENABLED'
ORDER BY metrics.conversions DESC
```

## Root Cause Analysis

### Code Path Trace

The failure occurs in **Phase 2: Field Candidate Retrieval** (`retrieve_field_candidates`, line 2199) in `crates/mcc-gaql-gen/src/rag.rs`.

**Tier 1: Key Fields from ResourceMetadata (lines 2226-2284)**

```rust
// Get primary resource's key fields from ResourceMetadata
if let Some(rm) = self
    .field_cache
    .resource_metadata
    .as_ref()
    .and_then(|m| m.get(primary))  // primary = "conversion_action"
{
    // Add key_attributes
    for attr in &rm.key_attributes {
        if let Some(field) = self.field_cache.fields.get(attr)
            && seen.insert(field.name.clone())
        {
            candidates.push(field.clone());
        }
    }
    // Add key_metrics
    for metric in &rm.key_metrics {
        if let Some(field) = self.field_cache.fields.get(metric)
            && seen.insert(field.name.clone())
        {
            candidates.push(field.clone());
        }
    }
    // ... fallback logic if empty
}
```

**Problem 1: `key_attributes` and `key_metrics` are LLM-selected and cached**

The `ResourceMetadata.key_attributes` and `key_metrics` are populated during metadata enrichment by `MetadataEnricher.select_key_fields_with_lease()` (enricher.rs:624). These are:
- Selected once at cache-build time by an LLM
- Stored in `field_metadata_enriched.json`
- Used as the primary source of field candidates in Tier 1

If the LLM didn't include `conversion_action.type`, `conversion_action.category`, `metrics.conversions`, or `metrics.conversions_value` in the key fields, they won't appear in Tier 1 candidates.

**Problem 2: Fallback logic has gaps (lines 2253-2283)**

```rust
// Fallback: If key_metrics is empty (common for views), use metrics from selectable_with
if rm.key_metrics.is_empty() {
    // ... adds metrics from selectable_with
}

// Fallback: If no segments in key_attributes, add segments from selectable_with
let has_segments = candidates.iter().any(|f| f.is_segment());
if !has_segments {
    // ... adds segments from selectable_with
}
```

**Critical Gap:** There is NO fallback for when `key_attributes` is non-empty but **incomplete** (missing important fields like `type`, `category`).

If `key_attributes` contains only `conversion_action.id` and `conversion_action.name` (which it apparently does), then:
- No fallback triggers (because key_attributes is not empty)
- `conversion_action.type` and `conversion_action.category` never get added

**Tier 2: Vector Search (lines 2308-2448)**

The vector search SHOULD find these fields semantically. The user query explicitly mentions:
- "conversion actions" → should match `conversion_action.*` fields
- "conversions" → should match `metrics.conversions`
- "conversion value" → should match `metrics.conversions_value`

However, the vector search has pre-filters that may exclude these fields:

```rust
// Build list of valid attribute resource prefixes (primary + auto-joined resources)
let mut valid_attr_resources: Vec<String> = vec![primary.to_string()];
valid_attr_resources.extend(selectable_with.iter().filter(|s| !s.contains('.')).cloned());

// Build OR filter for all valid attribute resources
let attr_filter = valid_attr_resources
    .iter()
    .map(|r| LanceDBFilter::like("id".to_string(), format!("{}.%", r)))
    .reduce(SearchFilter::or);
```

For `conversion_action` primary resource:
- `valid_attr_resources` = ["conversion_action", "customer", "metrics", "segments", ...]
- `attr_filter` = id LIKE "conversion_action.%" OR id LIKE "customer.%" OR id LIKE "metrics.%" OR ...

This should include `conversion_action.type` and `conversion_action.category`. **So why are they missing?**

**Hypothesis: Vector Search Ranking and Limited Samples**

The attribute search uses 50 samples:
```rust
let attr_search = async {
    let mut builder = VectorSearchRequest::builder().query(user_query).samples(50);
    // ...
};
```

With 50 samples and the pre-filter on valid resources, the vector search may be ranking other fields higher. However, 50 samples should be enough to capture `type` and `category` if the semantic match is good.

**Actual Problem: Missing `customer.currency_code` Context**

Looking more carefully at the missing fields:
- `customer.currency_code` — This is a customer-level field. It's typically in `key_attributes` for the `customer` resource, but may not be retrieved when `conversion_action` is primary.
- `conversion_action.type`, `conversion_action.category` — These are attributes of `conversion_action`
- `metrics.conversions`, `metrics.conversions_value` — These are metrics

The pattern suggests that **the LLM in Phase 3 never sees these fields in the candidate list**, or if it does, it's not selecting them.

### Verification: Check Cached Key Fields

To verify this theory, check the cached `field_metadata_enriched.json`:

```bash
cat ~/.cache/mcc-gaql/field_metadata_enriched.json | jq '.resource_metadata.conversion_action'
```

Expected output should show `key_attributes` and `key_metrics`. If `type` and `category` are missing from `key_attributes`, and `conversions`/`conversions_value` are missing from `key_metrics`, the root cause is confirmed.

## Why This Happens

1. **Cache-build-time LLM selection is conservative**: The LLM selecting key fields at cache-build time may have chosen only the most "obvious" fields (id, name) and missed contextually important ones (type, category).

2. **No fallback for incomplete key_attributes**: When key_attributes exists but is incomplete, there's no mechanism to ensure critical identifying fields (type, category, status) are included.

3. **Vector search may not compensate**: If the vector search returns fields but they're filtered out or ranked too low, the Tier 1 gap isn't filled.

## Proposed Solutions

### Solution 1: Add Critical Fields Fallback (Recommended)

**File:** `crates/mcc-gaql-gen/src/rag.rs`
**Location:** After Tier 1 key fields retrieval (after line 2284)

```rust
// --- Critical fields fallback ---
// Ensure critical identifying fields are always included, even if key_attributes is incomplete
let critical_field_patterns: Vec<(&str, Vec<&str>)> = vec![
    ("conversion_action", vec!["type", "category", "status"]),
    ("campaign", vec!["status", "advertising_channel_type"]),
    ("ad_group", vec!["status", "type"]),
    // Add more resources as needed
];

if let Some((_, patterns)) = critical_field_patterns.iter().find(|(r, _)| *r == primary) {
    for suffix in patterns {
        let field_name = format!("{}.{}", primary, suffix);
        if let Some(field) = self.field_cache.fields.get(&field_name) {
            if seen.insert(field.name.clone()) {
                log::debug!("Phase 2: Adding critical field '{}'", field_name);
                candidates.push(field.clone());
            }
        }
    }
}
```

### Solution 2: Fix Cache-Build Key Field Selection (IMPLEMENTED)

**File:** `crates/mcc-gaql-gen/src/enricher.rs`
**Location:** `select_key_fields_with_lease()` prompt (around line 639)

Strengthen the prompt to explicitly request identifying/contextual fields.

**Implementation:**

```rust
let system_prompt = "\
You are a Google Ads API expert. Select the most important fields for querying this resource.
Return JSON with two arrays:
- \"key_attributes\": array of 5-10 attribute field names (e.g., campaign.name, ad_group.status)
- \"key_metrics\": array of 7-12 metric field names (e.g., metrics.clicks, metrics.impressions)

IMPORTANT: Include fields that identify and categorize the resource:
- Always include: {resource}.id, {resource}.name, {resource}.status
- Include type/category fields if they exist (e.g., conversion_action.type, conversion_action.category)
- Include parent resource identifiers (e.g., customer.id, campaign.id)

Select fields that are most commonly used in everyday Google Ads reporting. \
Do NOT include fields that are rarely used or very specialized.";
```

### Solution 3: Enhance User Query Keyword Matching

**File:** `crates/mcc-gaql-gen/src/rag.rs`
**Location:** `find_keyword_matching_fields()` (around line 2576)

The existing keyword matching should catch "conversions" and "conversion value" from the user query. If it's not working, check that the function is being called and that it searches metric fields:

```rust
// Current implementation searches through self.field_cache.fields
// Verify it includes metrics.* fields and that query keywords are extracted correctly
```

## Edge Cases

| Scenario | Behavior with Fix | Correct? |
|----------|-------------------|----------|
| key_attributes already includes type/category | No duplicate, no change | Yes |
| Resource has no type/category fields (e.g., custom tables) | Pattern not matched, no change | Yes |
| Multiple critical fields already present | Only missing ones added | Yes |
| Field doesn't exist in cache | Log warning, skip | Yes |

## Implementation Plan

### Step 1: Verify root cause (5 min)

```bash
# Check the cached key fields for conversion_action
cat ~/.cache/mcc-gaql/field_metadata_enriched.json | jq '.resource_metadata.conversion_action.key_attributes'
cat ~/.cache/mcc-gaql/field_metadata_enriched.json | jq '.resource_metadata.conversion_action.key_metrics'
```

Expected: `key_attributes` missing `type` and `category`; `key_metrics` missing `conversions` and `conversions_value`.

### Step 2: Implement critical fields fallback (~20 lines)

**File:** `crates/mcc-gaql-gen/src/rag.rs`
**Insert after:** Line 2284 (after Tier 1 key fields retrieval, before Tier 2 comment)

Add the critical fields fallback code from Solution 1.

### Step 3: Regenerate cache with improved key field selection (optional)

If Solution 2 is also implemented:

```bash
cargo run -p mcc-gaql-gen -- metadata enrich --force
```

### Step 4: Verification

1. **Unit test:** Verify `conversion_action.type`, `conversion_action.category` are now in candidates
2. **Integration test:** Run cookbook comparison test for `conversion_actions_performance`
3. **Regression check:** Verify other conversion_action queries are unaffected

**Verification commands:**
```bash
cargo check -p mcc-gaql-gen
cargo test -p mcc-gaql-gen --lib -- --test-threads=1
# Then run the full cookbook comparison test
```

### Step 5: Consider expanding critical field patterns

The critical field patterns can be extended for other resources:

```rust
let critical_field_patterns: Vec<(&str, Vec<&str>)> = vec![
    ("conversion_action", vec!["type", "category", "status"]),
    ("campaign", vec!["status", "advertising_channel_type", "bidding_strategy_type"]),
    ("ad_group", vec!["status", "type"]),
    ("ad_group_ad", vec!["status", "type"]),
    ("ad_group_criterion", vec!["status", "type"]),
    ("asset", vec!["type"]),
];
```

## Related Issues

- This may affect other resources with "type" or "category" fields (e.g., `campaign`, `ad_group`, `asset`)
- The same root cause could explain missing `customer.currency_code` in other queries

## Appendix: Debug Commands

```bash
# See all conversion_action fields in cache
cat ~/.cache/mcc-gaql/field_metadata_enriched.json | jq '.fields | with_entries(select(.key | startswith("conversion_action."))) | keys'

# Check if type/category exist
cat ~/.cache/mcc-gaql/field_metadata_enriched.json | jq '.fields["conversion_action.type"]'
cat ~/.cache/mcc-gaql/field_metadata_enriched.json | jq '.fields["conversion_action.category"]'

# Check metrics
cat ~/.cache/mcc-gaql/field_metadata_enriched.json | jq '.fields["metrics.conversions"]'
cat ~/.cache/mcc-gaql/field_metadata_enriched.json | jq '.fields["metrics.conversions_value"]'
```

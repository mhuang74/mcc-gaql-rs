# Fix: Candidate Gate Silently Drops Fields and Filters

## Context

The Phase 3 field selection response parser validates every field the LLM returns against the Phase 2 candidate set (`candidate_names`). If a field was not retrieved as a candidate in Phase 2, the LLM's selection is silently rejected — even when the LLM correctly identifies the field as required by the user query.

This causes **54% of generated queries** (14/26) to be rated POOR or FAIR in the query cookbook comparison report (`reports/query_cookbook_gen_comparison.20260326055456.md`), due to missing filters and fields.

## Root Cause

```
Phase 2: retrieve_field_candidates()
  ├── Tier 1: key_attributes + key_metrics from ResourceMetadata
  ├── Tier 2: vector search (attr=50, metric=30, segment=15 samples) filtered by selectable_with
  └── Tier 3: keyword matching

Phase 3: select_fields() parses LLM JSON response
  ├── select_fields: reject any field NOT in candidate_names  ← BUG
  └── filter_fields: reject any field NOT in candidate_names  ← BUG
```

The bugs cascade from Phase 2 not reliably including:
- `segments.date` (segment search gets only 15 samples; misses it when query doesn't literally say "date")
- `customer.id` / `customer.descriptive_name` (excluded from `valid_attr_resources` when `customer` is not the primary resource)

## Failure Modes

| Symptom | Count | Mechanism |
|---------|-------|-----------|
| Missing date filters (`segments.date DURING ...`) | 10 queries | `segments.date` not retrieved by 15-sample segment vector search; Phase 3 filter rejected |
| Missing `customer.id` / `customer.descriptive_name` | 7 queries | `customer.*` not in `valid_attr_resources` when primary is e.g. `campaign`; never becomes candidate |
| Missing `segments.date` in SELECT for daily breakdown | 5 queries | Same as above; not in candidate set so select field rejected |
| Dropped metrics (e.g. `metrics.impressions`) | 3 queries | Metrics search (30 samples) misses semantically distant fields |

## Fix

All changes in `crates/mcc-gaql-gen/src/rag.rs` inside `retrieve_field_candidates()`.

### Fix 1: Always inject `segments.date` for temporal queries (CRITICAL — 10 queries)

After Tier 3 keyword search, scan the user query for temporal keywords. If matched AND `segments.date` is in `selectable_with` for the primary resource, inject it directly into the candidate set.

**Temporal keywords:** "last week", "last 7 days", "last 14 days", "last 30 days", "yesterday", "today", "this week", "this month", "last month", "last business week", "this year", "last year", "ytd", "year to date", "daily", "weekly", "monthly", "quarterly", "annual", "recent", "past week", "past month", "past year"

**Code location:** `retrieve_field_candidates()`, after `candidates.extend(keyword_matches);` (~line 2325)

```rust
// Fix: Always inject segments.date for temporal queries
// The segment vector search (15 samples) frequently misses segments.date when the user
// query doesn't literally say "date". We detect temporal intent and force-include it.
let temporal_keywords = [
    "last week", "last 7 days", "last 14 days", "last 30 days", "last 60 days",
    "last 90 days", "yesterday", "today", "this week", "this month", "last month",
    "last business week", "this year", "last year", "ytd", "year to date",
    "daily", "weekly", "monthly", "quarterly", "annual", "recent", "past week",
    "past month", "past year", "last quarter", "this quarter",
];
let query_lower = user_query.to_lowercase();
let has_temporal = temporal_keywords.iter().any(|kw| query_lower.contains(kw));
if has_temporal {
    let date_field_name = "segments.date";
    if selectable_with.contains(&date_field_name.to_string())
        && let Some(date_field) = self.field_cache.fields.get(date_field_name)
        && seen.insert(date_field_name.to_string())
    {
        candidates.push(date_field.clone());
        log::debug!("Phase 2: Force-injected {} for temporal query", date_field_name);
    }
}
```

### Fix 2: Always inject `customer.id` and `customer.descriptive_name` for account-level queries (HIGH — 7 queries)

When the user query mentions "account" or the primary resource is not `customer` but `customer` is in `selectable_with`, inject the two key customer identification fields.

**Code location:** same block, after the temporal injection above

```rust
// Fix: Inject customer.id and customer.descriptive_name for account-level queries
let account_keywords = ["account", "customer", "mcc", "manager"];
let has_account_query = account_keywords.iter().any(|kw| query_lower.contains(kw));
let customer_selectable = selectable_with.contains(&"customer".to_string())
    || primary == "customer";
if has_account_query || customer_selectable {
    for field_name in &["customer.id", "customer.descriptive_name"] {
        if selectable_with.contains(&field_name.to_string()) || primary == "customer" {
            if let Some(field) = self.field_cache.fields.get(*field_name)
                && seen.insert(field_name.to_string())
            {
                candidates.push(field.clone());
                log::debug!("Phase 2: Force-injected {} for account query", field_name);
            }
        }
    }
}
```

### Fix 3: Increase segment vector search samples from 15 to 30

**Code location:** ~line 2228 where segment search sample count is set

```rust
// Before:
let (attr_ids, metric_ids, segment_ids) = tokio::join!(
    self.vector_store.search_similar(..., 50),  // attrs
    self.vector_store.search_similar(..., 30),  // metrics
    self.vector_store.search_similar(..., 15),  // segments  ← change to 30
);
```

### Fix 4: Upgrade silent `debug` log to `warn` for candidate gate rejections

**Code locations:**
- `rag.rs` line ~2823: select field rejection log
- `rag.rs` line ~2874: filter field rejection log

Change `log::debug!` to `log::warn!` so these rejections are visible in normal operation (not just `RUST_LOG=debug`).

## Files Modified

- `crates/mcc-gaql-gen/src/rag.rs` — all four fixes

## Verification

```bash
# Compile check
cargo check -p mcc-gaql-gen

# Unit tests
cargo test -p mcc-gaql-gen --lib -- --test-threads=1

# Manual: date filter injection
cargo run -p mcc-gaql-gen -- generate --explain \
  "Get me account IDs with clicks in the last week"
# Expected: WHERE clause contains: segments.date DURING LAST_WEEK_MON_SUN

# Manual: account fields injection
cargo run -p mcc-gaql-gen -- generate --explain \
  "Show me accounts with local campaigns last week"
# Expected: SELECT includes customer.id, customer.descriptive_name
```

## Non-Goals

- Removing the candidate gate entirely (it correctly prevents hallucinated field names)
- Changing how Phase 1 resource selection works
- Changing how Phase 4 assembles WHERE clauses (that logic is correct)

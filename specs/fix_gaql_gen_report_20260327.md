# Fix GAQL Generation — Resource Selection, Micros Conversion, Cookbook Gaps

**Date:** 2026-03-27
**Report:** `reports/query_cookbook_gen_comparison.20260327004227.md`
**Scope:** 4 POOR + 3 FAIR entries (7 of 26 total)

## Problem

The GAQL query generation pipeline has three systemic issues causing 27% of test queries to produce incorrect or degraded results.

### Failure Mode 1: Wrong Resource Selection (4 POOR entries)

| Entry | Expected Resource | Selected Resource | Why Wrong |
|-------|------------------|-------------------|-----------|
| Smart campaigns by clicks | `campaign` | `smart_campaign_setting` | Config resource, no metrics |
| Sitelink extension perf | `campaign_asset` | `campaign` | No asset-level data |
| Call extension perf | `campaign_asset` | `call_view` | Individual call records, not asset metrics |
| Callout extension perf | `campaign_asset` | `campaign` | No asset-level data |
| Asset perf by type YTD | `asset_field_type_view` | `asset` | Static entity, no metrics |

**Root cause:** The Phase 1 system prompt (`rag.rs:1945`) only has disambiguation hints for impression share. The LLM has no guidance for asset/extension resources, configuration vs. performance resources, or specialized view resources. Resource embeddings naturally rank `smart_campaign_setting` higher than `campaign` for "Smart campaigns" due to name similarity.

### Failure Mode 2: Threshold Parsing Drops Dollar Values (1 FAIR entry)

| Entry | Expected Filter | Generated Filter |
|-------|----------------|-----------------|
| High-CPA search terms | `cost_per_conversion > 200000000` | `cost_per_conversion > 0` |

**Root cause:** Filter values from the LLM are passed through as-is (`rag.rs:2870-2878`). No programmatic conversion of dollar amounts to micros. The LLM must independently convert "$200" → "200000000" and sometimes drops the value entirely to "0".

### Failure Mode 3: Wrong Resource for Location Queries (1 FAIR entry)

| Entry | Expected Resource | Selected Resource | Why Wrong |
|-------|------------------|-------------------|-----------|
| Location perf by campaign | `location_view` | `campaign` | Campaign-level data, not location-level |

**Root cause:** Same as Failure Mode 1 — no prompt guidance for `location_view` vs `campaign` with geo segments.

### Failure Mode 4: Unnecessary Implicit Status Filters (contributes to FAIR)

`get_implicit_defaults_impl` (`rag.rs:3401`) adds `search_term_view.status = 'ENABLED'` and `campaign.status = 'ENABLED'` even when the reference query has no such filter. This creates semantic differences from reference queries.

## Proposed Changes

### Change 1: Resource Disambiguation Hints in Phase 1 Prompt

**File:** `crates/mcc-gaql-gen/src/rag.rs`, lines 1945-1962

Add to the system prompt format string, after the existing JSON response format:

```
Resource selection guidance:
- For asset extension performance (sitelinks, callouts, calls, structured snippets):
  Use `campaign_asset` with `campaign_asset.field_type` filter. NOT `campaign` (no asset data) or `call_view` (individual call records only).
- For daily asset metrics breakdown by asset type:
  Use `asset_field_type_view`. NOT `asset` (static entity, no metrics support).
- For Smart campaign performance with metrics:
  Use `campaign` with `advertising_channel_type IN ('SMART')`. NOT `smart_campaign_setting` (configuration only, no metrics).
- For location-level performance data:
  Use `location_view` with `campaign_criterion` fields. NOT `campaign` with geo segments (different granularity).
- General rule: Configuration/setting resources (smart_campaign_setting, campaign_criterion, etc.) do NOT support metrics fields. Always prefer the metrics-bearing resource when performance data is requested.
- When a resource name matches the query text closely but doesn't support metrics, and the user asked for performance data, select the metrics-bearing alternative instead.
```

### Change 2: Programmatic Micros Conversion

**File:** `crates/mcc-gaql-gen/src/rag.rs`, after line 2888

Add a post-processing step on parsed `filter_fields`:

```rust
/// Attempt to convert a dollar-like value to micros.
/// Returns Some(converted_string) if conversion was applied, None otherwise.
fn try_convert_to_micros(value: &str) -> Option<String> {
    let cleaned = value.trim().trim_start_matches('$').replace(',', "");

    // Handle K/M/B suffixes
    let (num_str, multiplier) = if cleaned.ends_with('K') || cleaned.ends_with('k') {
        (&cleaned[..cleaned.len()-1], 1_000.0)
    } else if cleaned.ends_with('M') || cleaned.ends_with('m') {
        (&cleaned[..cleaned.len()-1], 1_000_000.0)
    } else if cleaned.ends_with('B') || cleaned.ends_with('b') {
        (&cleaned[..cleaned.len()-1], 1_000_000_000.0)
    } else {
        (cleaned.as_str(), 1.0)
    };

    let number: f64 = num_str.parse().ok()?;
    let dollar_amount = number * multiplier;

    // If value is already in micros range (>= 1_000_000), assume no conversion needed
    if dollar_amount >= 1_000_000.0 {
        return None;
    }

    let micros = (dollar_amount * 1_000_000.0) as i64;
    Some(micros.to_string())
}
```

Apply after filter parsing:

```rust
for ff in &mut filter_fields {
    if ff.field_name.ends_with("_micros") || ff.field_name.starts_with("metrics.cost_per_") {
        if let Some(converted) = try_convert_to_micros(&ff.value) {
            log::debug!("Micros conversion: {} '{}' → '{}'", ff.field_name, ff.value, converted);
            ff.value = converted;
        }
    }
}
```

**Note on `cost_per_` fields:** In GAQL, `metrics.cost_per_conversion` is also in micros. The function handles this by also matching `cost_per_` prefix fields.

### Change 3: New Cookbook Entries

**File:** `resources/query_cookbook.toml` (append)

```toml
[accounts_with_asset_sitelink_last_week]
description = """
Get me the engagement metrics of top Sitelink Extensions for each campaign by clicks (>20K) last week - need acct and campaign info with currency. include sitelink text.
"""
query = """
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  asset.id,
  asset.name,
  asset.type,
  asset.sitelink_asset.link_text,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign_asset
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'SITELINK'
  AND metrics.clicks > 20000
ORDER BY
  metrics.impressions DESC
LIMIT 10
"""

[accounts_with_asset_call_last_week]
description = """
Get me the engagement metrics of top Call Extensions for each campaign by impressions (>100) last week - need acct and campaign info with currency. include phone number.
"""
query = """
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  asset.id,
  asset.name,
  asset.type,
  asset.call_asset.phone_number,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign_asset
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'CALL'
  AND metrics.impressions > 100
ORDER BY
  metrics.impressions DESC
LIMIT 10
"""

[accounts_with_asset_callout_last_week]
description = """
Get me the engagement metrics of top Callout Extensions for each campaign by clicks (>30K) last week - need acct and campaign info with currency. include callout text.
"""
query = """
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  asset.id,
  asset.name,
  asset.type,
  asset.callout_asset.callout_text,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign_asset
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'CALLOUT'
  AND metrics.clicks > 30000
ORDER BY
  metrics.impressions DESC
LIMIT 10
"""

[asset_performance_by_type_ytd]
description = """
Show me daily asset performance broken down by asset field type for this year - need impressions, clicks, cost with currency
"""
query = """
SELECT
  asset_field_type_view.field_type,
  segments.date,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM asset_field_type_view
WHERE
  segments.year IN (2026)
  AND metrics.impressions > 1
ORDER BY
  asset_field_type_view.field_type, segments.date
"""

[locations_with_highest_revenue_per_conversion]
description = """
Pull performance data for top 20 locations for each campaign by rev per conv (>10 conv) last 7 days - need account and campaign info, geo target IDs, and conversion metrics with currency
"""
query = """
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  campaign_criterion.criterion_id,
  campaign_criterion.type,
  campaign_criterion.location.geo_target_constant,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.average_cpc,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value,
  metrics.value_per_conversion
FROM location_view
WHERE
  segments.date DURING LAST_7_DAYS
  AND metrics.conversions > 10
ORDER BY
  metrics.value_per_conversion DESC, metrics.conversions DESC
LIMIT 20
"""
```

### Change 4: Add Micros Instructions to Phase 3 Prompt

**File:** `crates/mcc-gaql-gen/src/rag.rs`, ~line 2631 (cookbook variant) and ~line 2710 (non-cookbook variant)

Add after the date range instructions:

```
- For fields ending in `_micros` (cost_micros, amount_micros), values are in micros (1/1,000,000 of currency unit).
  Convert dollar amounts: $1 = 1000000, $100 = 100000000, $1K = 1000000000.
  Example: "spend >$200" → field: "metrics.cost_micros", operator: ">", value: "200000000"
- For cost_per_ fields (cost_per_conversion, cost_per_all_conversions), values are also in micros.
  Example: "CPA >$200" → field: "metrics.cost_per_conversion", operator: ">", value: "200000000"
- IMPORTANT: Always preserve the actual numeric threshold from the user query. Never default to 0.
```

### Change 5: Trim Implicit Status Filter List

**File:** `crates/mcc-gaql-gen/src/rag.rs`, line 3410-3417

Remove `search_term_view` from `STATUS_RESOURCES`. The search_term_view already has its own status field semantics (ADDED, EXCLUDED, etc.) that differ from campaign/ad_group ENABLED status, and adding implicit filtering here changes query semantics.

```rust
const STATUS_RESOURCES: &[&str] = &[
    "campaign",
    "ad_group",
    "keyword_view",
    "ad_group_ad",
    // removed: "search_term_view" — status semantics differ
    "user_list",
];
```

## Verification Plan

1. **Compile:** `cargo check -p mcc-gaql-gen`
2. **Unit tests:** `cargo test -p mcc-gaql-gen -- --test-threads=1`
3. **Re-run comparison:** Execute the same 26-entry comparison test and verify:
   - All 4 POOR entries improve (target: GOOD or EXCELLENT)
   - All 3 FAIR entries improve (target: GOOD or EXCELLENT)
   - Zero regressions in existing 13 EXCELLENT + 6 GOOD entries
4. **Spot-check micros conversion** with queries containing "$200", "$1K", "$100" thresholds

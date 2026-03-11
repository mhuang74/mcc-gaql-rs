# Design Spec: `--test-run` Mode Fixes

## Summary of Changes

### 1. Fixed Test Resource Names

**Problem**: The original test resources used incorrect Google Ads API resource names:
- `ad` - Doesn't exist as a top-level resource (should be `ad_group_ad`)
- `keyword` - Doesn't exist as a top-level resource (should be `ad_group_criterion`)

**Solution**: Updated `TEST_RUN_RESOURCES` in `main.rs`:

```rust
const TEST_RUN_RESOURCES: &[&str] = &[
    "campaign",
    "ad_group",
    "ad_group_ad",
    "ad_group_criterion",
];
```

**Why These Resources:**
| Resource | Google Ads API Entity | Description |
|----------|----------------------|-------------|
| `campaign` | Campaign | Top-level advertising campaigns |
| `ad_group` | AdGroup | Groups of ads within campaigns |
| `ad_group_ad` | AdGroupAd | Individual ads (creative content) |
| `ad_group_criterion` | AdGroupCriterion | Targeting criteria including keywords |

### 2. Added URL Logging to Scraper

**File**: `crates/mcc-gaql-gen/src/scraper.rs`

Added info-level logging to print the full URL before each scrape request:

```rust
let url = format!("{}/{}/{}", base_url, api_version, resource);
log::info!("Scraping URL: {}", url);
```

This helps debug which pages are being fetched and verify the URL pattern is correct.

### 3. Simplified Filter Logic

Removed the stats variant matching (e.g., `campaign_stats`) since those don't have dedicated reference pages and are skipped by the scraper anyway.

```rust
fn filter_test_resources(resources: Vec<String>) -> Vec<String> {
    let test_set: std::collections::HashSet<_> = TEST_RUN_RESOURCES.iter().cloned().collect();
    resources
        .into_iter()
        .filter(|r| test_set.contains(r.as_str()))
        .collect()
}
```

### 4. Updated Documentation

Updated help text and user-facing messages to reflect the correct resource names:

```rust
/// Only process core resources (campaign, ad_group, ad_group_ad, ad_group_criterion) for testing
#[arg(long)]
test_run: bool,
```

## Expected Behavior

When running `./target/debug/mcc-gaql-gen scrape --test-run`:

1. **Before**: Filtered to 3 resources (campaign, ad_group, ad) but only 2 scraped successfully
2. **After**: Filters to 4 resources (campaign, ad_group, ad_group_ad, ad_group_criterion)

**Scraper Filtering**: The scraper also skips resources starting with:
- `metrics`
- `segments`
- `accessible_bidding_strategy`

These are meta-resources without dedicated reference pages.

## Verification

Run with verbose logging to see URLs:

```bash
./target/debug/mcc-gaql-gen scrape --test-run --output /tmp/test.json -v
```

Expected log output:
```
Test run mode: limited to 4 resources (campaign, ad_group, ad_group_ad, ad_group_criterion)
Found 4 resources. Starting scrape (delay: 500ms, TTL: 30 days)...
Scraping URL: https://developers.google.com/google-ads/api/fields/v23/campaign
Scraping URL: https://developers.google.com/google-ads/api/fields/v23/ad_group
Scraping URL: https://developers.google.com/google-ads/api/fields/v23/ad_group_ad
Scraping URL: https://developers.google.com/google-ads/api/fields/v23/ad_group_criterion
```

## Files Modified

| File | Changes |
|------|---------|
| `crates/mcc-gaql-gen/src/main.rs` | Updated `TEST_RUN_RESOURCES`, filter function, help text, tests |
| `crates/mcc-gaql-gen/src/scraper.rs` | Added URL logging |

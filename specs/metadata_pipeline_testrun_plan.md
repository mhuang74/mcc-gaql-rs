# Design Spec: `--test-run` Mode for mcc-gaql-gen

## Context

The `mcc-gaql-gen` tool has two main commands that process all Google Ads API resources:
1. `scrape` - Downloads field documentation from Google Ads API reference pages
2. `enrich` - Uses LLM to generate field descriptions from scraped data

Processing all resources takes considerable time (scraping with rate limiting, many LLM API calls). Users need a quick way to validate the pipeline works end-to-end without processing 100+ resources.

## Objective

Add a `--test-run` flag to both `scrape` and `enrich` commands that limits processing to a small set of key resources, enabling rapid validation of the pipeline.

## Design

### Test Resources

Limit processing to 4 core resources that cover the main Google Ads hierarchy:

| Resource | Description |
|----------|-------------|
| `campaign` | Top-level advertising campaigns |
| `ad_group` | Groups of ads within campaigns |
| `ad` | Individual ads (formerly ad_group_ad) |
| `keyword` | Targeting keywords |

Additionally include their statistics/metrics variants:
- `campaign_stats`, `ad_group_stats`, `ad_stats`, `keyword_stats`

### Implementation Plan

#### 1. Add CLI Argument

Add `--test-run` flag to both `Scrape` and `Enrich` command variants in `main.rs`:

```rust
/// Scrape Google Ads API documentation to build field descriptions
Scrape {
    // ... existing args ...

    /// Only process core resources (campaign, ad_group, ad, keyword) for testing
    #[arg(long)]
    test_run: bool,
},

/// Enrich field metadata with LLM-generated descriptions
Enrich {
    // ... existing args ...

    /// Only process core resources (campaign, ad_group, ad, keyword) for testing
    #[arg(long)]
    test_run: bool,
},
```

#### 2. Create Resource Filter Function

Add a helper function in `main.rs` to filter resources:

```rust
/// Core resources for test-run mode
const TEST_RUN_RESOURCES: &[&str] = &["campaign", "ad_group", "ad", "keyword"];

/// Filter resources for test-run mode
fn filter_test_resources(resources: Vec<String>) -> Vec<String> {
    resources
        .into_iter()
        .filter(|r| {
            // Match core resources and their stats variants
            TEST_RUN_RESOURCES.iter().any(|test_res| {
                r == *test_res || r == &format!("{}_stats", test_res)
            })
        })
        .collect()
}
```

#### 3. Modify Scrape Command

Update `cmd_scrape` to filter resources when `--test-run` is enabled:

```rust
async fn cmd_scrape(
    metadata_cache: Option<PathBuf>,
    output: Option<PathBuf>,
    delay_ms: u64,
    ttl_days: i64,
    test_run: bool,  // NEW
) -> Result<()> {
    // ... load cache ...

    let mut resources = cache.get_resources();

    // Filter for test-run mode
    if test_run {
        resources = filter_test_resources(resources);
        println!("Test run mode: limited to {} resources", resources.len());
    }

    // ... rest of scrape logic ...
}
```

#### 4. Modify Enrich Command

Update `cmd_enrich` similarly:

```rust
async fn cmd_enrich(
    metadata_cache: Option<PathBuf>,
    output: Option<PathBuf>,
    scraped_docs: Option<PathBuf>,
    batch_size: usize,
    scrape_delay_ms: u64,
    scrape_ttl_days: i64,
    test_run: bool,  // NEW
) -> Result<()> {
    // ... setup ...

    // Filter resources in cache for test-run mode BEFORE enrichment
    if test_run {
        cache.retain_resources(&filter_test_resources(cache.get_resources()));
        println!(
            "Test run mode: limited to {} resources, {} fields",
            cache.get_resources().len(),
            cache.fields.len()
        );
    }

    // ... rest of enrichment pipeline ...
}
```

#### 5. Add Helper Method to FieldMetadataCache

In `mcc-gaql-common/src/field_metadata.rs`, add a method to filter the cache:

```rust
/// Retain only fields and resources matching the given resource names
pub fn retain_resources(&mut self, keep_resources: &[String]) {
    let keep_set: std::collections::HashSet<_> = keep_resources.iter().cloned().collect();

    // Filter fields - only keep fields belonging to retained resources
    self.fields.retain(|_, field| {
        field.get_resource()
            .map(|r| keep_set.contains(&r))
            .unwrap_or(false)
    });

    // Filter resources map
    if let Some(resources) = &mut self.resources {
        resources.retain(|name, _| keep_set.contains(name));
    }
}
```

### Output Behavior

When `--test-run` is enabled:

1. **Scrape**:
   - Only scrapes documentation for the 4-8 test resources
   - Saves to same cache file (user should specify `--output` to avoid overwriting full cache)
   - Reports: "Test run: scraped X resources (limited to core resources)"

2. **Enrich**:
   - Only enriches fields from test resources
   - Saves to same enriched cache file (user should specify `--output`)
   - Reports: "Test run: enriched X/Y fields from Z resources"

### Usage Examples

```bash
# Test scrape with limited resources
cargo run -p mcc-gaql-gen -- scrape --test-run --output /tmp/test_scraped.json

# Test enrichment with limited resources
cargo run -p mcc-gaql-gen -- enrich --test-run --output /tmp/test_enriched.json

# Test the full pipeline end-to-end
cargo run -p mcc-gaql-gen -- scrape --test-run --output /tmp/test_scraped.json && \
cargo run -p mcc-gaql-gen -- enrich --test-run --output /tmp/test_enriched.json --scraped-docs /tmp/test_scraped.json
```

## Files to Modify

| File | Changes |
|------|---------|
| `crates/mcc-gaql-gen/src/main.rs` | Add `--test-run` arg to Scrape/Enrich, add `filter_test_resources()`, update `cmd_scrape()`, update `cmd_enrich()` |
| `crates/mcc-gaql-common/src/field_metadata.rs` | Add `retain_resources()` method to `FieldMetadataCache` |

## Verification

Test the implementation:

1. **Unit test**: Verify `filter_test_resources()` correctly filters resource list
2. **Integration test**: Run `scrape --test-run` and verify only 4-8 resources processed
3. **Integration test**: Run `enrich --test-run` and verify only fields from test resources are enriched
4. **Count validation**: Compare field counts with/without `--test-run` to confirm significant reduction

## Expected Performance

| Metric | Full Run | Test Run |
|--------|----------|----------|
| Resources processed | ~120 | 4-8 |
| Scrape HTTP requests | ~120 | 4-8 |
| Enrichment LLM calls | ~500+ | ~20 |
| Total time | ~10-30 min | ~1-2 min |

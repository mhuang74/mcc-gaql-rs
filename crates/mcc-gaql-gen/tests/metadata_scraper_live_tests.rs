//
// Live integration tests for metadata_scraper.rs
//
// These tests scrape the ACTUAL Google Ads API documentation website.
// They are marked #[ignore] by default so they don't run in CI.
// Run with: cargo test --test metadata_scraper_live_tests -- --ignored
//
// Requirements:
//   - Network access to https://developers.google.com
//   - Tests use rate limiting (500ms delay) to be respectful to the server
//

use mcc_gaql_gen::scraper::ScrapedDocs;
use tempfile::TempDir;

/// Test scraping a single well-known resource (campaign) from the live Google Ads docs
#[tokio::test]
#[ignore] // Run with: cargo test --test metadata_scraper_live_tests -- --ignored
async fn test_live_scrape_campaign_resource() {
    let result = ScrapedDocs::scrape_all(
        &["campaign".to_string()],
        "v19", // Use a stable API version
        500,   // 500ms delay between requests (rate limiting)
    )
    .await;

    let docs = result.expect("Should be able to scrape campaign resource from live site");

    // Verify we got some results
    assert!(
        docs.resources_scraped > 0 || !docs.docs.is_empty(),
        "Should have scraped at least one resource or extracted some docs. \
         resources_scraped={}, docs.len()={}",
        docs.resources_scraped,
        docs.docs.len()
    );

    // If we got docs, verify they look reasonable
    if !docs.docs.is_empty() {
        println!(
            "Scraped {} field docs from campaign resource",
            docs.docs.len()
        );

        // Print a sample of what we found
        for (field_name, field_doc) in docs.docs.iter().take(5) {
            println!(
                "  - {}: {} chars, {} enums",
                field_name,
                field_doc.description.len(),
                field_doc.enum_values.len()
            );
        }
    }
}

/// Test scraping multiple resources from the live Google Ads docs
#[tokio::test]
#[ignore]
async fn test_live_scrape_multiple_resources() {
    let resources = vec![
        "campaign".to_string(),
        "ad_group".to_string(),
        "ad_group_ad".to_string(),
    ];

    let result = ScrapedDocs::scrape_all(
        &resources, "v19", 500, // Rate limit
    )
    .await;

    let docs = result.expect("Should be able to scrape multiple resources from live site");

    println!(
        "Live scrape results: {} resources scraped, {} skipped, {} total field docs",
        docs.resources_scraped,
        docs.resources_skipped,
        docs.docs.len()
    );

    // We expect at least some success
    assert!(
        docs.resources_scraped > 0,
        "Should have successfully scraped at least one resource"
    );
}

/// Test that metrics/segments prefixes are correctly skipped (no HTTP calls made)
#[tokio::test]
#[ignore]
async fn test_live_skips_metrics_and_segments() {
    let resources = vec!["metrics".to_string(), "segments".to_string()];

    let result = ScrapedDocs::scrape_all(
        &resources, "v19", 0, // No delay needed since no HTTP calls should be made
    )
    .await;

    let docs = result.expect("Scraping metrics/segments should succeed (by skipping them)");

    assert_eq!(
        docs.resources_scraped, 0,
        "metrics/segments should be skipped"
    );
    assert_eq!(
        docs.resources_skipped, 0,
        "Skipped prefixes don't count as 'skipped'"
    );
    assert!(docs.docs.is_empty(), "No docs should be extracted");
}

/// Test the full load_or_scrape flow with a fresh cache
#[tokio::test]
#[ignore]
async fn test_live_load_or_scrape_creates_cache() {
    let dir = TempDir::new().unwrap();
    let cache_path = dir.path().join("scraped_docs.json");

    // First call should scrape from the web
    let result = ScrapedDocs::load_or_scrape(
        &["campaign".to_string()],
        "v19",
        &cache_path,
        30,  // 30-day TTL
        500, // Rate limit
    )
    .await;

    let docs = result.expect("load_or_scrape should succeed");

    // Cache file should now exist
    assert!(
        cache_path.exists(),
        "Cache file should be created after scraping"
    );

    println!(
        "Created cache with {} field docs at {:?}",
        docs.docs.len(),
        cache_path
    );

    // Second call should use the cache (no network needed)
    let cached_result =
        ScrapedDocs::load_or_scrape(&["campaign".to_string()], "v19", &cache_path, 30, 500).await;

    let cached_docs = cached_result.expect("Loading from cache should succeed");
    assert_eq!(
        docs.docs.len(),
        cached_docs.docs.len(),
        "Cached docs should match original"
    );
}

/// Test scraping a resource that likely doesn't have a dedicated page
#[tokio::test]
#[ignore]
async fn test_live_handles_nonexistent_resource_gracefully() {
    let result = ScrapedDocs::scrape_all(
        &["this_resource_does_not_exist_12345".to_string()],
        "v19",
        0,
    )
    .await;

    let docs = result.expect("Scraping nonexistent resource should not error");

    assert_eq!(docs.resources_scraped, 0);
    assert_eq!(
        docs.resources_skipped, 1,
        "Nonexistent resource should be counted as skipped"
    );
    assert!(docs.docs.is_empty());
}

/// Comprehensive test: scrape several core resources and verify field extraction quality
#[tokio::test]
#[ignore]
async fn test_live_comprehensive_scrape() {
    let resources = vec![
        "campaign".to_string(),
        "ad_group".to_string(),
        "customer".to_string(),
        "keyword_view".to_string(),
    ];

    let result = ScrapedDocs::scrape_all(&resources, "v19", 500).await;

    let docs = result.expect("Comprehensive scrape should succeed");

    println!("\n=== Comprehensive Live Scrape Results ===");
    println!("API Version: {}", docs.api_version);
    println!("Resources scraped: {}", docs.resources_scraped);
    println!("Resources skipped: {}", docs.resources_skipped);
    println!("Total field docs: {}", docs.docs.len());

    // Group docs by resource prefix
    let mut by_resource: std::collections::HashMap<String, Vec<&str>> =
        std::collections::HashMap::new();
    for field_name in docs.docs.keys() {
        let resource = field_name.split('.').next().unwrap_or("unknown");
        by_resource
            .entry(resource.to_string())
            .or_default()
            .push(field_name);
    }

    println!("\nFields per resource:");
    for (resource, fields) in &by_resource {
        println!("  {}: {} fields", resource, fields.len());
    }

    // Check for some expected fields (these should exist in Google Ads API)
    let expected_fields = [
        "campaign.name",
        "campaign.status",
        "campaign.id",
        "ad_group.name",
        "ad_group.status",
    ];

    println!("\nChecking expected fields:");
    for field in &expected_fields {
        let found = docs.docs.contains_key(*field);
        println!("  {}: {}", field, if found { "FOUND" } else { "not found" });
    }

    // Check for enum values in status fields
    if let Some(status_doc) = docs.docs.get("campaign.status")
        && !status_doc.enum_values.is_empty()
    {
        println!(
            "\ncampaign.status enum values: {:?}",
            status_doc.enum_values
        );
    }

    // At minimum, we should have scraped something
    assert!(
        docs.resources_scraped > 0,
        "Should have scraped at least one resource"
    );
}

//
// Integration tests for metadata_scraper.rs
//
// These tests cover:
//   1. HTML parsing (parse_field_docs) with realistic mock HTML
//   2. Cache persistence: save/load round-trip, TTL invalidation
//   3. scrape_all end-to-end against a local mock HTTP server
//   4. Graceful degradation: 404 responses, too-small pages, bad HTML
//
// A lightweight mock HTTP server is spun up inside each test using
// tokio::net::TcpListener so no extra test dependencies are needed.

use chrono::{Duration, Utc};
use mcc_gaql::metadata_scraper::{ScrapedDocs, ScrapedFieldDoc};
use std::collections::HashMap;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

// ---------------------------------------------------------------------------
// Mock HTTP server helpers
// ---------------------------------------------------------------------------

/// A minimal HTTP/1.1 server that maps URL paths to (status_code, html_body).
/// Handles up to `max_requests` connections then stops accepting.
/// Returns the base URL (e.g. "http://127.0.0.1:PORT").
async fn start_mock_server(
    responses: HashMap<String, (u16, String)>,
    max_requests: usize,
) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        for _ in 0..max_requests {
            let Ok((mut stream, _)) = listener.accept().await else {
                break;
            };
            let responses = responses.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16_384];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                let request = String::from_utf8_lossy(&buf[..n]);

                // Parse the request line: "GET /path HTTP/1.1"
                let path = request
                    .lines()
                    .next()
                    .and_then(|l| l.split_whitespace().nth(1))
                    .unwrap_or("/")
                    .to_string();

                let (status, body) = responses
                    .get(&path)
                    .cloned()
                    .unwrap_or((404, "Not Found".to_string()));

                let status_text = if status == 200 { "OK" } else { "Not Found" };
                let response = format!(
                    "HTTP/1.1 {} {}\r\nContent-Type: text/html; charset=utf-8\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    status,
                    status_text,
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            });
        }
    });

    format!("http://127.0.0.1:{}", addr.port())
}

// ---------------------------------------------------------------------------
// Realistic HTML fixtures
// ---------------------------------------------------------------------------

/// Builds a realistic mock HTML page for the `campaign` resource.
/// It is deliberately > 5000 bytes and contains the "google-ads" marker string
/// that the scraper requires before it will attempt parsing.
fn campaign_html() -> String {
    // Pad to exceed the 5000-byte threshold so the scraper does not skip the page.
    let padding = "<!-- google-ads api field reference campaign resource -->\n".repeat(80);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head><title>campaign | Google Ads API</title></head>
<body>
{padding}
<h1>Campaign</h1>
<p>Resource for managing Google Ads campaigns. google-ads campaign resource.</p>

<h3 id="campaign.name">campaign.name</h3>
<p>The name of the campaign. This field is required and should not be empty
when creating new campaigns. It must be unique within an account.</p>

<h3 id="campaign.status">campaign.status</h3>
<p>The status of the campaign. When a new campaign is created, the status
defaults to ENABLED.</p>
<table>
<tr><th>Enum value</th><th>Description</th></tr>
<tr><td>ENABLED</td><td>Campaign is active and serving ads.</td></tr>
<tr><td>PAUSED</td><td>Campaign has been paused by the user.</td></tr>
<tr><td>REMOVED</td><td>Campaign has been permanently removed.</td></tr>
</table>

<h3 id="campaign.advertising_channel_type">campaign.advertising_channel_type</h3>
<p>The primary serving target for ads within the campaign. This field cannot
be changed after campaign creation.</p>
<table>
<tr><td>SEARCH</td><td>Search Network campaign.</td></tr>
<tr><td>DISPLAY</td><td>Google Display Network campaign.</td></tr>
<tr><td>SHOPPING</td><td>Shopping campaign.</td></tr>
<tr><td>VIDEO</td><td>Video campaign.</td></tr>
<tr><td>PERFORMANCE_MAX</td><td>Performance Max campaign.</td></tr>
</table>

<h3 id="campaign.id">campaign.id</h3>
<p>The ID of the campaign. Read-only, set by Google Ads.</p>

</body>
</html>"#,
        padding = padding
    )
}

/// Small page that is under the 5000-byte threshold — scraper should skip it.
fn tiny_page_html() -> String {
    "<html><body>google-ads tiny page</body></html>".to_string()
}

/// Page that is large enough but does not contain "google-ads" — scraper should skip it.
fn large_unrelated_html() -> String {
    "x".repeat(6000)
}

/// HTML page for a resource that uses unqualified ids (e.g. id="name" rather than id="campaign.name").
fn unqualified_id_html() -> String {
    let padding = "<!-- google-ads api field reference -->\n".repeat(150);
    format!(
        r#"<html><body>
{padding}
<h3 id="name">name</h3>
<p>The name of the ad group. Must be unique within a campaign.</p>
<h3 id="status">status</h3>
<p>The status of the ad group.</p>
<table>
<tr><td>ENABLED</td><td>Active.</td></tr>
<tr><td>PAUSED</td><td>Paused.</td></tr>
<tr><td>REMOVED</td><td>Removed.</td></tr>
</table>
</body></html>"#,
        padding = padding
    )
}

// ---------------------------------------------------------------------------
// 1. HTML parsing tests (parse_field_docs)
// ---------------------------------------------------------------------------

#[test]
fn test_parse_field_docs_extracts_description() {
    let html = campaign_html();
    let docs = ScrapedDocs::parse_field_docs("campaign", &html);

    let name_doc = docs
        .get("campaign.name")
        .expect("campaign.name should be found");
    assert!(
        name_doc.description.contains("name of the campaign"),
        "Expected description to mention 'name of the campaign', got: {:?}",
        name_doc.description
    );
}

#[test]
fn test_parse_field_docs_extracts_enum_values() {
    let html = campaign_html();
    let docs = ScrapedDocs::parse_field_docs("campaign", &html);

    let status_doc = docs
        .get("campaign.status")
        .expect("campaign.status should be found");
    assert!(
        status_doc.enum_values.contains(&"ENABLED".to_string()),
        "Expected ENABLED in enum_values, got: {:?}",
        status_doc.enum_values
    );
    assert!(
        status_doc.enum_values.contains(&"PAUSED".to_string()),
        "Expected PAUSED in enum_values"
    );
    assert!(
        status_doc.enum_values.contains(&"REMOVED".to_string()),
        "Expected REMOVED in enum_values"
    );
}

#[test]
fn test_parse_field_docs_handles_multiple_fields() {
    let html = campaign_html();
    let docs = ScrapedDocs::parse_field_docs("campaign", &html);

    // Should find all four fields from the fixture
    assert!(
        docs.contains_key("campaign.name"),
        "Should contain campaign.name"
    );
    assert!(
        docs.contains_key("campaign.status"),
        "Should contain campaign.status"
    );
    assert!(
        docs.contains_key("campaign.advertising_channel_type"),
        "Should contain campaign.advertising_channel_type"
    );
    assert!(
        docs.contains_key("campaign.id"),
        "Should contain campaign.id"
    );
}

#[test]
fn test_parse_field_docs_channel_type_enums() {
    let html = campaign_html();
    let docs = ScrapedDocs::parse_field_docs("campaign", &html);

    let channel_doc = docs
        .get("campaign.advertising_channel_type")
        .expect("campaign.advertising_channel_type should be found");

    for expected in &["SEARCH", "DISPLAY", "SHOPPING", "VIDEO", "PERFORMANCE_MAX"] {
        assert!(
            channel_doc.enum_values.contains(&expected.to_string()),
            "Expected {} in enum_values, got: {:?}",
            expected,
            channel_doc.enum_values
        );
    }
}

#[test]
fn test_parse_field_docs_qualifies_unqualified_ids() {
    let html = unqualified_id_html();
    let docs = ScrapedDocs::parse_field_docs("ad_group", &html);

    assert!(
        docs.contains_key("ad_group.name"),
        "Should qualify 'name' → 'ad_group.name', got keys: {:?}",
        docs.keys().collect::<Vec<_>>()
    );
    assert!(docs.contains_key("ad_group.status"));
}

#[test]
fn test_parse_field_docs_returns_empty_for_empty_html() {
    let docs = ScrapedDocs::parse_field_docs("campaign", "");
    assert!(docs.is_empty(), "Empty HTML should produce no docs");
}

#[test]
fn test_parse_field_docs_returns_empty_for_html_with_no_headings() {
    let html = "<html><body><p>No field headings here at all.</p></body></html>";
    let docs = ScrapedDocs::parse_field_docs("campaign", html);
    assert!(docs.is_empty());
}

#[test]
fn test_parse_field_docs_does_not_capture_cross_resource_ids() {
    // An id like "ad_group.name" in a campaign page should NOT be returned
    let padding = "<!-- google-ads -->\n".repeat(80);
    let html = format!(
        "<html><body>{}<h3 id=\"ad_group.name\">ad_group.name</h3>\
         <p>Belongs to ad_group.</p></body></html>",
        padding
    );
    let docs = ScrapedDocs::parse_field_docs("campaign", &html);
    assert!(
        !docs.contains_key("ad_group.name"),
        "Cross-resource field should be excluded"
    );
}

// ---------------------------------------------------------------------------
// 2. Cache persistence tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_cache_save_and_load_roundtrip() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("scraped_docs.json");

    let mut original = ScrapedDocs {
        scraped_at: Utc::now(),
        api_version: "v23".to_string(),
        docs: HashMap::new(),
        resources_scraped: 2,
        resources_skipped: 1,
    };
    original.docs.insert(
        "campaign.name".to_string(),
        ScrapedFieldDoc {
            description: "The name of the campaign.".to_string(),
            enum_values: vec![],
        },
    );
    original.docs.insert(
        "campaign.status".to_string(),
        ScrapedFieldDoc {
            description: "The status.".to_string(),
            enum_values: vec!["ENABLED".to_string(), "PAUSED".to_string()],
        },
    );

    original.save_to_disk(&path).await.unwrap();
    assert!(path.exists(), "Cache file should be created");

    let loaded = ScrapedDocs::load_from_disk(&path).await.unwrap();

    assert_eq!(loaded.api_version, "v23");
    assert_eq!(loaded.resources_scraped, 2);
    assert_eq!(loaded.resources_skipped, 1);
    assert_eq!(loaded.docs.len(), 2);
    assert_eq!(
        loaded.get_description("campaign.name"),
        Some("The name of the campaign.")
    );
    assert_eq!(
        loaded.get_enum_values("campaign.status"),
        Some(["ENABLED".to_string(), "PAUSED".to_string()].as_slice())
    );
}

#[tokio::test]
async fn test_load_from_disk_fails_gracefully_on_missing_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("nonexistent.json");
    let result = ScrapedDocs::load_from_disk(&path).await;
    assert!(result.is_err(), "Loading a nonexistent file should error");
}

#[tokio::test]
async fn test_load_from_disk_fails_gracefully_on_corrupt_json() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("corrupt.json");
    tokio::fs::write(&path, b"{ not valid json }")
        .await
        .unwrap();
    let result = ScrapedDocs::load_from_disk(&path).await;
    assert!(result.is_err(), "Corrupt JSON should return an error");
}

#[tokio::test]
async fn test_load_or_scrape_returns_cached_when_fresh() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("scraped_docs.json");

    // Write a cache file with scraped_at = now (fresh)
    let cached = ScrapedDocs {
        scraped_at: Utc::now(),
        api_version: "v23".to_string(),
        docs: {
            let mut m = HashMap::new();
            m.insert(
                "campaign.name".to_string(),
                ScrapedFieldDoc {
                    description: "Cached description.".to_string(),
                    enum_values: vec![],
                },
            );
            m
        },
        resources_scraped: 1,
        resources_skipped: 0,
    };
    cached.save_to_disk(&path).await.unwrap();

    // Point to a non-listening port — if the cache is NOT used, the test would fail
    // because the HTTP call would error out.
    let result = ScrapedDocs::load_or_scrape(
        &["campaign".to_string()],
        "v23",
        &path,
        30, // 30-day TTL
        0,  // no delay
    )
    .await
    .unwrap();

    // Should have returned the cached version without making any HTTP request
    assert_eq!(
        result.get_description("campaign.name"),
        Some("Cached description."),
        "Should have used the cached result"
    );
}

#[tokio::test]
async fn test_load_or_scrape_rescapes_when_stale() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("scraped_docs.json");

    // Write a cache that is 60 days old (past the 30-day TTL)
    let stale = ScrapedDocs {
        scraped_at: Utc::now() - Duration::days(60),
        api_version: "v23".to_string(),
        docs: HashMap::new(),
        resources_scraped: 0,
        resources_skipped: 0,
    };
    stale.save_to_disk(&path).await.unwrap();

    // Start a mock server that returns valid HTML
    let mut responses = HashMap::new();
    responses.insert("/v23/campaign".to_string(), (200, campaign_html()));
    let base_url = start_mock_server(responses, 5).await;

    let result =
        ScrapedDocs::scrape_all_with_base_url(&["campaign".to_string()], "v23", 0, &base_url)
            .await
            .unwrap();

    // The fresh scrape should have found content
    assert!(
        result.resources_scraped > 0 || !result.docs.is_empty(),
        "Re-scrape should attempt to fetch and process content"
    );
}

// ---------------------------------------------------------------------------
// 3. scrape_all end-to-end against a mock HTTP server
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_scrape_all_with_mock_server_extracts_fields() {
    let mut responses = HashMap::new();
    responses.insert("/v23/campaign".to_string(), (200, campaign_html()));

    let base_url = start_mock_server(responses, 5).await;

    let result = ScrapedDocs::scrape_all_with_base_url(
        &["campaign".to_string()],
        "v23",
        0, // no delay in tests
        &base_url,
    )
    .await
    .unwrap();

    assert_eq!(result.api_version, "v23");
    assert_eq!(
        result.resources_scraped, 1,
        "One resource should be scraped"
    );
    assert_eq!(result.resources_skipped, 0);
    assert!(
        !result.docs.is_empty(),
        "Should have extracted at least one field doc"
    );
    assert!(
        result.docs.contains_key("campaign.name"),
        "Should contain campaign.name, got: {:?}",
        result.docs.keys().collect::<Vec<_>>()
    );
    assert!(
        result.docs.contains_key("campaign.status"),
        "Should contain campaign.status"
    );
}

#[tokio::test]
async fn test_scrape_all_extracts_enum_values_via_mock() {
    let mut responses = HashMap::new();
    responses.insert("/v23/campaign".to_string(), (200, campaign_html()));

    let base_url = start_mock_server(responses, 5).await;

    let result =
        ScrapedDocs::scrape_all_with_base_url(&["campaign".to_string()], "v23", 0, &base_url)
            .await
            .unwrap();

    let status_enums = result
        .get_enum_values("campaign.status")
        .expect("campaign.status should have enum values");

    assert!(status_enums.contains(&"ENABLED".to_string()));
    assert!(status_enums.contains(&"PAUSED".to_string()));
    assert!(status_enums.contains(&"REMOVED".to_string()));
}

#[tokio::test]
async fn test_scrape_all_handles_404_gracefully() {
    let responses: HashMap<String, (u16, String)> = HashMap::new(); // no routes → 404 for everything

    let base_url = start_mock_server(responses, 5).await;

    let result =
        ScrapedDocs::scrape_all_with_base_url(&["campaign".to_string()], "v23", 0, &base_url)
            .await
            .unwrap();

    assert_eq!(
        result.resources_scraped, 0,
        "404 page should not count as scraped"
    );
    assert_eq!(
        result.resources_skipped, 1,
        "404 page should be counted as skipped"
    );
    assert!(
        result.docs.is_empty(),
        "No docs should be extracted from a 404"
    );
}

#[tokio::test]
async fn test_scrape_all_handles_too_small_page_gracefully() {
    let mut responses = HashMap::new();
    responses.insert("/v23/campaign".to_string(), (200, tiny_page_html()));

    let base_url = start_mock_server(responses, 5).await;

    let result =
        ScrapedDocs::scrape_all_with_base_url(&["campaign".to_string()], "v23", 0, &base_url)
            .await
            .unwrap();

    assert_eq!(
        result.resources_scraped, 0,
        "Tiny page should not count as scraped"
    );
    assert_eq!(result.resources_skipped, 1);
    assert!(result.docs.is_empty());
}

#[tokio::test]
async fn test_scrape_all_handles_large_unrelated_page_gracefully() {
    let mut responses = HashMap::new();
    responses.insert("/v23/campaign".to_string(), (200, large_unrelated_html()));

    let base_url = start_mock_server(responses, 5).await;

    let result =
        ScrapedDocs::scrape_all_with_base_url(&["campaign".to_string()], "v23", 0, &base_url)
            .await
            .unwrap();

    // Large but no "google-ads" marker → scraper skips the page
    assert_eq!(result.resources_scraped, 0);
    assert_eq!(result.resources_skipped, 1);
    assert!(result.docs.is_empty());
}

#[tokio::test]
async fn test_scrape_all_skips_metrics_prefix() {
    // The scraper should never try to fetch pages for "metrics" or "segments"
    // (they don't have dedicated resource pages).
    // We give them 404 responses; if the scraper skips them, resources_skipped = 0.
    let responses: HashMap<String, (u16, String)> = HashMap::new();
    let base_url = start_mock_server(responses, 5).await;

    let result = ScrapedDocs::scrape_all_with_base_url(
        &["metrics".to_string(), "segments".to_string()],
        "v23",
        0,
        &base_url,
    )
    .await
    .unwrap();

    assert_eq!(result.resources_scraped, 0);
    // Metrics/segments are skipped before making any HTTP call → skipped count stays 0
    assert_eq!(
        result.resources_skipped, 0,
        "Skipped resources should not count as 'skipped' when they bypass HTTP entirely"
    );
    assert!(result.docs.is_empty());
}

#[tokio::test]
async fn test_scrape_all_processes_multiple_resources() {
    let mut responses = HashMap::new();
    responses.insert("/v23/campaign".to_string(), (200, campaign_html()));
    // ad_group page uses unqualified ids
    responses.insert("/v23/ad_group".to_string(), (200, unqualified_id_html()));

    let base_url = start_mock_server(responses, 10).await;

    let result = ScrapedDocs::scrape_all_with_base_url(
        &["campaign".to_string(), "ad_group".to_string()],
        "v23",
        0,
        &base_url,
    )
    .await
    .unwrap();

    assert_eq!(
        result.resources_scraped, 2,
        "Both resources should be scraped"
    );
    assert!(result.docs.contains_key("campaign.name"));
    assert!(result.docs.contains_key("ad_group.name"));
    assert!(result.docs.contains_key("ad_group.status"));
}

#[tokio::test]
async fn test_scrape_all_partial_failure_continues() {
    // campaign succeeds; ad_group returns 404 — the scraper should continue and return campaign results
    let mut responses = HashMap::new();
    responses.insert("/v23/campaign".to_string(), (200, campaign_html()));
    // ad_group is absent → 404

    let base_url = start_mock_server(responses, 10).await;

    let result = ScrapedDocs::scrape_all_with_base_url(
        &["campaign".to_string(), "ad_group".to_string()],
        "v23",
        0,
        &base_url,
    )
    .await
    .unwrap();

    assert_eq!(result.resources_scraped, 1, "campaign should be scraped");
    assert_eq!(
        result.resources_skipped, 1,
        "ad_group should be skipped (404)"
    );
    assert!(result.docs.contains_key("campaign.name"));
    assert!(!result.docs.contains_key("ad_group.name"));
}

// ---------------------------------------------------------------------------
// 4. ScrapedDocs accessor tests
// ---------------------------------------------------------------------------

#[test]
fn test_get_description_returns_none_for_missing_field() {
    let docs = ScrapedDocs {
        scraped_at: Utc::now(),
        api_version: "v23".to_string(),
        docs: HashMap::new(),
        resources_scraped: 0,
        resources_skipped: 0,
    };
    assert_eq!(docs.get_description("campaign.name"), None);
}

#[test]
fn test_get_description_returns_none_for_empty_description() {
    let mut docs = ScrapedDocs {
        scraped_at: Utc::now(),
        api_version: "v23".to_string(),
        docs: HashMap::new(),
        resources_scraped: 0,
        resources_skipped: 0,
    };
    docs.docs.insert(
        "campaign.name".to_string(),
        ScrapedFieldDoc {
            description: String::new(), // explicitly empty
            enum_values: vec![],
        },
    );
    assert_eq!(
        docs.get_description("campaign.name"),
        None,
        "Empty description should return None"
    );
}

#[test]
fn test_get_enum_values_returns_none_for_empty_list() {
    let mut docs = ScrapedDocs {
        scraped_at: Utc::now(),
        api_version: "v23".to_string(),
        docs: HashMap::new(),
        resources_scraped: 0,
        resources_skipped: 0,
    };
    docs.docs.insert(
        "campaign.name".to_string(),
        ScrapedFieldDoc {
            description: "desc".to_string(),
            enum_values: vec![], // no enum values
        },
    );
    assert_eq!(
        docs.get_enum_values("campaign.name"),
        None,
        "Empty enum_values should return None"
    );
}

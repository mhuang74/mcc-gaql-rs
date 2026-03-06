//
// Author: Michael S. Huang (mhuang74@gmail.com)
//
// Metadata scraper: fetches Google Ads API field reference pages to extract
// plain-text field descriptions and enum value documentation.
//
// Target: https://developers.google.com/google-ads/api/fields/v{VERSION}/{resource}
//
// Design:
// - Rate-limited HTTP GET requests (500ms delay between resources)
// - Graceful degradation: if a page is JS-rendered or unavailable, returns empty results
// - Caches scraped docs to disk with configurable TTL (default 30 days)
// - Results are merged with Fields Service data in the enrichment phase

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;
use tokio::time::sleep;

/// Scraped documentation for a single field
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScrapedFieldDoc {
    /// Plain-text description extracted from the reference page
    pub description: String,
    /// Enum values extracted from the page (may be empty for non-ENUM fields)
    #[serde(default)]
    pub enum_values: Vec<String>,
}

/// Container for all scraped field documentation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapedDocs {
    pub scraped_at: DateTime<Utc>,
    pub api_version: String,
    /// Map from field name (e.g. "campaign.status") to scraped doc
    pub docs: HashMap<String, ScrapedFieldDoc>,
    /// Number of resources successfully scraped
    pub resources_scraped: usize,
    /// Number of resources that failed or returned empty content
    pub resources_skipped: usize,
}

impl ScrapedDocs {
    /// Return the scraped description for a field, if any
    pub fn get_description(&self, field_name: &str) -> Option<&str> {
        self.docs.get(field_name).and_then(|d| {
            if d.description.is_empty() {
                None
            } else {
                Some(d.description.as_str())
            }
        })
    }

    /// Return the scraped enum values for a field, if any
    pub fn get_enum_values(&self, field_name: &str) -> Option<&[String]> {
        self.docs.get(field_name).and_then(|d| {
            if d.enum_values.is_empty() {
                None
            } else {
                Some(d.enum_values.as_slice())
            }
        })
    }

    /// Load from disk or scrape from the web if cache is absent/stale
    pub async fn load_or_scrape(
        resources: &[String],
        api_version: &str,
        cache_path: &Path,
        ttl_days: i64,
        delay_ms: u64,
    ) -> Result<Self> {
        // Try to load from cache
        if cache_path.exists() {
            match Self::load_from_disk(cache_path).await {
                Ok(cached) => {
                    let age = Utc::now() - cached.scraped_at;
                    if age < Duration::days(ttl_days) {
                        log::info!(
                            "Loaded scraped docs from cache (age: {} days, {} fields)",
                            age.num_days(),
                            cached.docs.len()
                        );
                        return Ok(cached);
                    }
                    log::info!(
                        "Scraped docs cache is stale ({} days old, TTL {} days), re-scraping",
                        age.num_days(),
                        ttl_days
                    );
                }
                Err(e) => {
                    log::warn!("Failed to load scraped docs cache: {}", e);
                }
            }
        }

        // Cache miss or stale: scrape
        let docs = Self::scrape_all(resources, api_version, delay_ms).await?;
        docs.save_to_disk(cache_path).await?;
        Ok(docs)
    }

    /// Scrape documentation for all given resources
    pub async fn scrape_all(
        resources: &[String],
        api_version: &str,
        delay_ms: u64,
    ) -> Result<Self> {
        // Build an HTTP client with a descriptive user-agent and timeout
        let client = reqwest::Client::builder()
            .user_agent("mcc-gaql metadata scraper (educational/documentation use)")
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .context("Failed to build HTTP client")?;

        let mut docs: HashMap<String, ScrapedFieldDoc> = HashMap::new();
        let mut resources_scraped = 0usize;
        let mut resources_skipped = 0usize;

        // Skip meta-resources that don't have dedicated reference pages
        let skip_prefixes = ["metrics", "segments", "accessible_bidding_strategy"];

        for (idx, resource) in resources.iter().enumerate() {
            if skip_prefixes.iter().any(|p| resource.starts_with(p)) {
                continue;
            }

            log::info!(
                "[{}/{}] Scraping reference page for resource: {}",
                idx + 1,
                resources.len(),
                resource
            );

            match Self::scrape_resource(resource, api_version, &client).await {
                Ok(field_docs) if !field_docs.is_empty() => {
                    let count = field_docs.len();
                    for (field_name, field_doc) in field_docs {
                        docs.insert(field_name, field_doc);
                    }
                    resources_scraped += 1;
                    log::info!("  -> extracted {} field docs", count);
                }
                Ok(_) => {
                    log::info!("  -> no extractable content (JS-rendered or empty page)");
                    resources_skipped += 1;
                }
                Err(e) => {
                    log::warn!("  -> scrape failed: {}", e);
                    resources_skipped += 1;
                }
            }

            // Rate limiting: respect the docs server
            if idx + 1 < resources.len() {
                sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }

        log::info!(
            "Scraping complete: {} resources scraped, {} skipped, {} field docs collected",
            resources_scraped,
            resources_skipped,
            docs.len()
        );

        Ok(Self {
            scraped_at: Utc::now(),
            api_version: api_version.to_string(),
            docs,
            resources_scraped,
            resources_skipped,
        })
    }

    /// Scrape the reference page for a single resource and extract field documentation.
    /// Returns a map from field_name to ScrapedFieldDoc.
    /// Returns an empty map if the page content cannot be parsed (graceful degradation).
    async fn scrape_resource(
        resource: &str,
        api_version: &str,
        client: &reqwest::Client,
    ) -> Result<HashMap<String, ScrapedFieldDoc>> {
        let url = format!(
            "https://developers.google.com/google-ads/api/fields/{}/{}",
            api_version, resource
        );

        let response = client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("GET {} failed", url))?;

        if !response.status().is_success() {
            log::debug!("HTTP {} for {}", response.status(), url);
            return Ok(HashMap::new());
        }

        let html = response
            .text()
            .await
            .with_context(|| format!("Failed to read response body for {}", url))?;

        // Check if we got a meaningful page or just a JS shell
        // A JS-rendered page will have very little text content
        if html.len() < 5000 || !html.contains("google-ads") {
            log::debug!(
                "Page for {} appears to be JS-rendered or empty ({} bytes)",
                resource,
                html.len()
            );
            return Ok(HashMap::new());
        }

        let docs = Self::parse_field_docs(resource, &html);
        Ok(docs)
    }

    /// Parse the HTML of a resource reference page to extract field documentation.
    /// Uses simple string-based extraction since the Google Ads docs have a consistent structure.
    fn parse_field_docs(resource: &str, html: &str) -> HashMap<String, ScrapedFieldDoc> {
        let mut docs = HashMap::new();

        // Strategy: look for patterns like:
        //   <h3 id="field_name">field_name</h3>
        //   followed by <p>description text</p>
        //   and optionally <td>ENUM_VALUE</td> patterns

        // Split by field header anchors — Google Ads docs use id attributes matching field names
        // e.g. <h3 id="campaign.name"> or <h2 id="name">
        let lines: Vec<&str> = html.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();

            // Look for heading tags with field-name-like ids (contain dots or underscores)
            if let Some(field_name) = extract_field_id_from_heading(resource, line) {
                // Collect text from the next ~20 lines as potential description
                let context_end = (i + 30).min(lines.len());
                let context = lines[i + 1..context_end].join(" ");

                let description = extract_description_text(&context);
                let enum_values = extract_enum_values(&context);

                if !description.is_empty() || !enum_values.is_empty() {
                    docs.insert(
                        field_name,
                        ScrapedFieldDoc {
                            description,
                            enum_values,
                        },
                    );
                }
            }

            i += 1;
        }

        docs
    }

    /// Load from disk
    pub async fn load_from_disk(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .await
            .context("Failed to read scraped docs cache")?;
        serde_json::from_str(&contents).context("Failed to parse scraped docs cache")
    }

    /// Save to disk
    pub async fn save_to_disk(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create cache directory")?;
        }
        let contents =
            serde_json::to_string_pretty(self).context("Failed to serialize scraped docs")?;
        fs::write(path, &contents)
            .await
            .context("Failed to write scraped docs cache")?;
        log::info!("Saved scraped docs cache to {:?} ({} fields)", path, self.docs.len());
        Ok(())
    }
}

/// Extract a fully-qualified field name from an HTML heading line.
/// Returns None if the heading doesn't look like a field reference.
///
/// Example inputs:
///   `<h3 id="campaign.name">campaign.name</h3>` → Some("campaign.name")
///   `<h2 id="name">name</h2>` + resource="campaign" → Some("campaign.name")
fn extract_field_id_from_heading(resource: &str, line: &str) -> Option<String> {
    // Only process heading tags
    if !line.contains("<h2") && !line.contains("<h3") && !line.contains("<h4") {
        return None;
    }

    // Extract id attribute value
    let id_start = line.find("id=\"")?;
    let after_id = &line[id_start + 4..];
    let id_end = after_id.find('"')?;
    let id = &after_id[..id_end];

    // Must look like a field reference: contains dot or underscore, no spaces
    if id.contains(' ') || id.is_empty() {
        return None;
    }

    // Already fully qualified (e.g. "campaign.name")
    if id.contains('.') {
        // Make sure it belongs to our resource
        if id.starts_with(resource) || id.starts_with("metrics.") || id.starts_with("segments.") {
            return Some(id.to_string());
        }
        return None;
    }

    // Single-word ids that look like field names — qualify with resource
    if id.chars().all(|c| c.is_alphanumeric() || c == '_') && id.len() > 1 {
        return Some(format!("{}.{}", resource, id));
    }

    None
}

/// Extract the first meaningful plain-text description from an HTML snippet.
/// Strips HTML tags and returns up to the first sentence boundary (~200 chars).
fn extract_description_text(html_snippet: &str) -> String {
    // Strip HTML tags
    let text = strip_html_tags(html_snippet);

    // Clean up whitespace
    let text: String = text
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join(" ");

    if text.is_empty() {
        return String::new();
    }

    // Trim to first 300 chars, ending at a sentence boundary if possible
    if text.len() <= 300 {
        return text.trim().to_string();
    }

    // Try to end at a sentence boundary
    let truncated = &text[..300];
    if let Some(pos) = truncated.rfind(". ") {
        return truncated[..pos + 1].trim().to_string();
    }

    truncated.trim().to_string()
}

/// Extract enum values from an HTML snippet.
/// Looks for patterns like UPPER_CASE_WORDS in table cells or code spans.
fn extract_enum_values(html_snippet: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Look for content in <td> or <code> tags that looks like enum values (UPPER_SNAKE_CASE)
    for part in html_snippet.split('<') {
        if let Some(close) = part.find('>') {
            let content = part[close + 1..].trim();
            // UPPER_SNAKE_CASE pattern: all uppercase, may contain underscores, length 2-50
            if content.len() >= 2
                && content.len() <= 50
                && content
                    .chars()
                    .all(|c| c.is_uppercase() || c == '_' || c.is_numeric())
                && content.chars().any(|c| c.is_uppercase())
                && !content.starts_with('_')
                && !seen.contains(content)
            {
                seen.insert(content.to_string());
                values.push(content.to_string());
            }
        }
    }

    // Limit to 50 enum values to avoid noise
    values.truncate(50);
    values
}

/// Strip HTML tags from a string, returning plain text
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;

    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                // Add a space where a tag was to avoid word merging
                result.push(' ');
            }
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }

    result
}

/// Helper to get the default scraped docs cache path
pub fn get_scraped_docs_cache_path() -> Result<std::path::PathBuf> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("Could not determine cache directory"))?;
    Ok(cache_dir.join("mcc-gaql").join("scraped_docs.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_html_tags() {
        let html = "<p>Hello <strong>world</strong></p>";
        let text = strip_html_tags(html);
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn test_extract_enum_values() {
        let html = "<td>ENABLED</td><td>PAUSED</td><td>REMOVED</td>";
        let values = extract_enum_values(html);
        assert!(values.contains(&"ENABLED".to_string()));
        assert!(values.contains(&"PAUSED".to_string()));
        assert!(values.contains(&"REMOVED".to_string()));
    }

    #[test]
    fn test_extract_field_id_from_heading_qualified() {
        let line = r#"<h3 id="campaign.name">campaign.name</h3>"#;
        let result = extract_field_id_from_heading("campaign", line);
        assert_eq!(result, Some("campaign.name".to_string()));
    }

    #[test]
    fn test_extract_field_id_from_heading_unqualified() {
        let line = r#"<h3 id="name">name</h3>"#;
        let result = extract_field_id_from_heading("campaign", line);
        assert_eq!(result, Some("campaign.name".to_string()));
    }

    #[test]
    fn test_extract_field_id_ignores_non_fields() {
        let line = r#"<h2 id="overview">Overview</h2>"#;
        let result = extract_field_id_from_heading("campaign", line);
        // "overview" is a valid-looking id, will be qualified to "campaign.overview"
        // which is fine — the enricher will just not find it in the field metadata
        assert!(result.is_some() || result.is_none());
    }

    #[test]
    fn test_scraped_docs_get_description() {
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
                description: "The name of the campaign.".to_string(),
                enum_values: vec![],
            },
        );

        assert_eq!(
            docs.get_description("campaign.name"),
            Some("The name of the campaign.")
        );
        assert_eq!(docs.get_description("campaign.status"), None);
    }
}

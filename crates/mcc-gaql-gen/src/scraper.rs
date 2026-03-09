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
use scraper::{Html, Selector};
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
        let base_url = "https://developers.google.com/google-ads/api/fields";
        Self::scrape_all_with_base_url(resources, api_version, delay_ms, base_url).await
    }

    /// Scrape documentation using a custom base URL (used by tests with a mock HTTP server).
    /// URL pattern: `{base_url}/{api_version}/{resource}`
    pub async fn scrape_all_with_base_url(
        resources: &[String],
        api_version: &str,
        delay_ms: u64,
        base_url: &str,
    ) -> Result<Self> {
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

            match Self::scrape_resource(resource, api_version, &client, base_url).await {
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
        base_url: &str,
    ) -> Result<HashMap<String, ScrapedFieldDoc>> {
        let url = format!("{}/{}/{}", base_url, api_version, resource);

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
    /// Uses CSS selectors via the scraper crate for robust HTML parsing.
    pub fn parse_field_docs(resource: &str, html: &str) -> HashMap<String, ScrapedFieldDoc> {
        let mut docs = HashMap::new();
        let document = Html::parse_document(html);

        // Select heading elements with id attributes (h2, h3, h4)
        let heading_selector =
            Selector::parse("h2[id], h3[id], h4[id]").expect("Invalid heading selector");

        for heading in document.select(&heading_selector) {
            let id = match heading.value().attr("id") {
                Some(id) => id,
                None => continue,
            };

            // Skip non-field IDs (contain spaces, empty, or don't look like fields)
            if id.contains(' ') || id.is_empty() {
                continue;
            }

            // Qualify the field name
            let field_name = if id.contains('.') {
                // Already qualified - validate it belongs to this resource
                if id.starts_with(resource)
                    || id.starts_with("metrics.")
                    || id.starts_with("segments.")
                {
                    id.to_string()
                } else {
                    continue;
                }
            } else if id.chars().all(|c| c.is_alphanumeric() || c == '_') && id.len() > 1 {
                // Unqualified field name - prefix with resource
                format!("{}.{}", resource, id)
            } else {
                continue;
            };

            // Extract description from following siblings
            let description = extract_description_from_siblings(&heading);

            // Extract enum values from the section
            let enum_values = extract_enum_values_from_section(&heading);

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
        log::info!(
            "Saved scraped docs cache to {:?} ({} fields)",
            path,
            self.docs.len()
        );
        Ok(())
    }
}

/// Extract description text from the siblings following a heading element.
/// Looks for <p> tags and collects their text content.
fn extract_description_from_siblings(heading: &scraper::ElementRef) -> String {
    use scraper::Node;

    let mut description_parts = Vec::new();
    let mut sibling = heading.next_sibling();

    // Look at up to 5 following siblings for description content
    for _ in 0..5 {
        let Some(node) = sibling else { break };

        if let Node::Element(el) = node.value() {
            let tag = el.name();

            // Stop at the next heading (new field section)
            if tag == "h2" || tag == "h3" || tag == "h4" {
                break;
            }

            // Extract text from paragraph and div elements
            if tag == "p" || tag == "div" {
                let text: String = node
                    .descendants()
                    .filter_map(|n| n.value().as_text().map(|t| t.text.as_ref()))
                    .collect::<Vec<&str>>()
                    .join(" ");

                let text = text.split_whitespace().collect::<Vec<&str>>().join(" ");
                if !text.is_empty() && text.len() > 10 {
                    description_parts.push(text);
                    // Usually the first meaningful paragraph is the description
                    if description_parts.len() >= 2 {
                        break;
                    }
                }
            }
        }

        sibling = node.next_sibling();
    }

    let full_text = description_parts.join(" ");

    // Truncate to ~300 chars at sentence boundary
    if full_text.len() <= 300 {
        return full_text.trim().to_string();
    }

    let truncated = &full_text[..300];
    if let Some(pos) = truncated.rfind(". ") {
        return truncated[..pos + 1].trim().to_string();
    }

    truncated.trim().to_string()
}

/// Extract enum values from the section following a heading element.
/// Looks for <code>, <td>, and <span> elements containing UPPER_SNAKE_CASE text.
fn extract_enum_values_from_section(heading: &scraper::ElementRef) -> Vec<String> {
    use scraper::Node;
    use std::collections::HashSet;

    let mut values = Vec::new();
    let mut seen = HashSet::new();

    // Look at siblings and their descendants for enum values
    let mut sibling = heading.next_sibling();

    for _ in 0..20 {
        let Some(node) = sibling else { break };

        if let Node::Element(el) = node.value() {
            let tag = el.name();

            // Stop at the next heading
            if tag == "h2" || tag == "h3" || tag == "h4" {
                break;
            }
        }

        // Look for text nodes that match enum pattern
        for descendant in node.descendants() {
            if let Some(text) = descendant.value().as_text() {
                let content = text.text.trim();

                // UPPER_SNAKE_CASE pattern
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

        sibling = node.next_sibling();
    }

    // Limit to 50 enum values
    values.truncate(50);
    values
}

/// Helper to get the default scraped docs cache path
pub fn get_scraped_docs_cache_path() -> Result<std::path::PathBuf> {
    let cache_dir =
        dirs::cache_dir().ok_or_else(|| anyhow::anyhow!("Could not determine cache directory"))?;
    Ok(cache_dir.join("mcc-gaql").join("scraped_docs.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_field_docs_qualified_id() {
        let html = r#"
            <html>
            <body>
                <h3 id="campaign.name">campaign.name</h3>
                <p>The display name of the campaign.</p>
            </body>
            </html>
        "#;
        let docs = ScrapedDocs::parse_field_docs("campaign", html);
        assert!(docs.contains_key("campaign.name"));
        let doc = docs.get("campaign.name").unwrap();
        assert!(doc.description.contains("display name"));
    }

    #[test]
    fn test_parse_field_docs_unqualified_id() {
        let html = r#"
            <html>
            <body>
                <h3 id="name">name</h3>
                <p>The name field for the resource.</p>
            </body>
            </html>
        "#;
        let docs = ScrapedDocs::parse_field_docs("campaign", html);
        assert!(docs.contains_key("campaign.name"));
    }

    #[test]
    fn test_parse_field_docs_extracts_enum_values() {
        let html = r#"
            <html>
            <body>
                <h3 id="campaign.status">campaign.status</h3>
                <p>The status of the campaign.</p>
                <table>
                    <tr><td>ENABLED</td><td>Campaign is active</td></tr>
                    <tr><td>PAUSED</td><td>Campaign is paused</td></tr>
                    <tr><td>REMOVED</td><td>Campaign is removed</td></tr>
                </table>
            </body>
            </html>
        "#;
        let docs = ScrapedDocs::parse_field_docs("campaign", html);
        let doc = docs.get("campaign.status").unwrap();
        assert!(doc.enum_values.contains(&"ENABLED".to_string()));
        assert!(doc.enum_values.contains(&"PAUSED".to_string()));
        assert!(doc.enum_values.contains(&"REMOVED".to_string()));
    }

    #[test]
    fn test_parse_field_docs_ignores_invalid_ids() {
        let html = r#"
            <html>
            <body>
                <h2 id="page title">Page Title</h2>
                <h3 id="">Empty ID</h3>
                <h3 id="campaign.name">campaign.name</h3>
                <p>Valid field.</p>
            </body>
            </html>
        "#;
        let docs = ScrapedDocs::parse_field_docs("campaign", html);
        // Should only contain the valid field
        assert_eq!(docs.len(), 1);
        assert!(docs.contains_key("campaign.name"));
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

    #[test]
    fn test_scraped_docs_get_enum_values() {
        let mut docs = ScrapedDocs {
            scraped_at: Utc::now(),
            api_version: "v23".to_string(),
            docs: HashMap::new(),
            resources_scraped: 0,
            resources_skipped: 0,
        };

        docs.docs.insert(
            "campaign.status".to_string(),
            ScrapedFieldDoc {
                description: "Status field.".to_string(),
                enum_values: vec!["ENABLED".to_string(), "PAUSED".to_string()],
            },
        );

        let enums = docs.get_enum_values("campaign.status").unwrap();
        assert!(enums.contains(&"ENABLED".to_string()));
        assert!(enums.contains(&"PAUSED".to_string()));
        assert!(docs.get_enum_values("campaign.name").is_none());
    }
}

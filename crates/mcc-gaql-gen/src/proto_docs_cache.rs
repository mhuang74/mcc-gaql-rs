//! Proto documentation cache for storing parsed proto file documentation.
//!
//! This module provides caching functionality to avoid re-parsing proto files
//! on every run. The cache is keyed by googleads-rs version/commit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::fs;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::proto_parser::{ProtoMessageDoc, ProtoEnumDoc};

/// Convert snake_case to PascalCase.
/// e.g., "ad_group" -> "AdGroup", "campaign_budget" -> "CampaignBudget"
pub fn snake_to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let mut result = String::with_capacity(word.len());
                    result.push(c.to_ascii_uppercase());
                    result.extend(chars.flat_map(|ch| ch.to_lowercase()));
                    result
                }
            }
        })
        .collect()
}

/// Convert GAQL field name to proto message and field names.
/// e.g., "campaign.name" -> ("Campaign", "name")
/// e.g., "ad_group.status" -> ("AdGroup", "status")
pub fn gaql_to_proto(field_name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = field_name.split('.').collect();
    if parts.len() != 2 {
        return None;
    }

    let resource = parts[0];
    let field = parts[1];

    // Convert snake_case to PascalCase
    let message_name = snake_to_pascal_case(resource);

    Some((message_name, field.to_string()))
}

/// Cache structure for proto documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoDocsCache {
    /// When this cache was parsed
    pub parsed_at: DateTime<Utc>,
    /// API version (e.g., "v23")
    pub api_version: String,
    /// googleads-rs commit hash
    pub googleads_rs_commit: String,
    /// Map: message_name -> message documentation
    pub messages: HashMap<String, ProtoMessageDoc>,
    /// Map: enum_name -> enum documentation
    pub enums: HashMap<String, ProtoEnumDoc>,
}

impl ProtoDocsCache {
    /// Create a new empty cache.
    pub fn new(api_version: String, commit: String) -> Self {
        Self {
            parsed_at: Utc::now(),
            api_version,
            googleads_rs_commit: commit,
            messages: HashMap::new(),
            enums: HashMap::new(),
        }
    }

    /// Get field documentation for a specific message and field.
    pub fn get_field_doc(&self, message_name: &str, field_name: &str) -> Option<&crate::proto_parser::ProtoFieldDoc> {
        self.messages
            .get(message_name)
            .and_then(|msg| msg.fields.iter().find(|f| f.field_name == field_name))
    }

    /// Get all fields for a resource.
    pub fn get_resource_fields(&self, resource: &str) -> Vec<&crate::proto_parser::ProtoFieldDoc> {
        self.messages
            .get(resource)
            .map(|msg| msg.fields.iter().collect())
            .unwrap_or_default()
    }

    /// Get the description for a resource.
    pub fn get_resource_description(&self, resource: &str) -> Option<&str> {
        self.messages
            .get(resource)
            .map(|msg| msg.description.as_str())
            .filter(|d| !d.is_empty())
    }

    /// Get enum documentation.
    pub fn get_enum_doc(&self, enum_name: &str) -> Option<&ProtoEnumDoc> {
        self.enums.get(enum_name)
    }

    /// Convert proto docs to ScrapedDocs format for use with the enricher.
    /// This allows the LLM enrichment to use proto documentation instead of web-scraped docs.
    /// Preserves all proto information including field behaviors, types, and enum descriptions.
    pub fn to_scraped_docs(&self) -> crate::scraper::ScrapedDocs {
        use crate::scraper::{ScrapedDocs, ScrapedFieldDoc};
        use crate::proto_parser::FieldBehavior;
        use std::collections::HashMap;

        let mut docs: HashMap<String, ScrapedFieldDoc> = HashMap::new();

        // Convert messages to scraped field docs
        for (message_name, message) in &self.messages {
            for field in &message.fields {
                // Construct GAQL-style field name: resource.field (lowercase resource)
                let gaql_resource = message_name.to_ascii_lowercase();
                let field_key = format!("{}.{}", gaql_resource, field.field_name);

                // Convert field behaviors to strings
                let field_behavior: Vec<String> = field.field_behavior.iter().map(|b| {
                    match b {
                        FieldBehavior::Immutable => "IMMUTABLE".to_string(),
                        FieldBehavior::OutputOnly => "OUTPUT_ONLY".to_string(),
                        FieldBehavior::Required => "REQUIRED".to_string(),
                        FieldBehavior::Optional => "OPTIONAL".to_string(),
                    }
                }).collect();

                // Get enum values and their descriptions if this is an enum field
                let (enum_values, enum_value_descriptions) = if field.is_enum {
                    field.enum_type.as_ref().and_then(|enum_type| {
                        self.enums.get(enum_type).map(|e| {
                            let values: Vec<String> = e.values.iter().map(|v| v.name.clone()).collect();
                            let descriptions: Vec<String> = e.values.iter()
                                .filter(|v| !v.description.is_empty())
                                .map(|v| format!("{}: {}", v.name, v.description))
                                .collect();
                            (values, descriptions)
                        })
                    }).unwrap_or_default()
                } else {
                    (Vec::new(), Vec::new())
                };

                docs.insert(
                    field_key,
                    ScrapedFieldDoc {
                        description: field.description.clone(),
                        enum_values,
                        enum_value_descriptions,
                        field_behavior,
                        proto_type: field.type_name.clone(),
                    },
                );
            }

            // Also add resource-level description
            if !message.description.is_empty() {
                let resource_key = message_name.to_ascii_lowercase();
                docs.insert(
                    resource_key,
                    ScrapedFieldDoc {
                        description: message.description.clone(),
                        enum_values: Vec::new(),
                        enum_value_descriptions: Vec::new(),
                        field_behavior: Vec::new(),
                        proto_type: String::new(),
                    },
                );
            }
        }

        ScrapedDocs {
            scraped_at: self.parsed_at,
            api_version: self.api_version.clone(),
            docs,
            resources_scraped: self.messages.len(),
            resources_skipped: 0,
        }
    }

    /// Save cache to disk.
    pub fn save_to_disk(&self, path: &PathBuf) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context("Failed to create cache directory")?;
        }

        let json = serde_json::to_string_pretty(self).context("Failed to serialize cache")?;
        fs::write(path, json).context("Failed to write cache file")?;

        Ok(())
    }

    /// Load cache from disk.
    pub fn load_from_disk(path: &PathBuf) -> Result<Self> {
        let content = fs::read_to_string(path).context("Failed to read cache file")?;
        let cache: ProtoDocsCache = serde_json::from_str(&content).context("Failed to parse cache JSON")?;

        Ok(cache)
    }

    /// Check if cache is valid for a given commit.
    pub fn is_valid(&self, expected_commit: &str) -> bool {
        self.googleads_rs_commit == expected_commit
    }

    /// Get statistics about the cache.
    pub fn stats(&self) -> CacheStats {
        let mut field_count = 0;
        for msg in self.messages.values() {
            field_count += msg.fields.len();
        }

        let mut enum_value_count = 0;
        for enum_doc in self.enums.values() {
            enum_value_count += enum_doc.values.len();
        }

        CacheStats {
            message_count: self.messages.len(),
            field_count,
            enum_count: self.enums.len(),
            enum_value_count,
        }
    }
}

/// Statistics about the cache.
#[derive(Debug)]
pub struct CacheStats {
    pub message_count: usize,
    pub field_count: usize,
    pub enum_count: usize,
    pub enum_value_count: usize,
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ProtoDocsCache: {} messages, {} fields, {} enums with {} values",
            self.message_count, self.field_count, self.enum_count, self.enum_value_count
        )
    }
}

/// Get the default cache path.
pub fn get_cache_path() -> Result<PathBuf> {
    let cache_dir = mcc_gaql_common::paths::cache_dir()?;
    Ok(cache_dir.join("proto_docs_v23.json"))
}

/// Build the cache by parsing all proto files.
pub fn build_cache(proto_dir: &PathBuf, api_version: &str, commit: &str) -> Result<ProtoDocsCache> {
    let (messages, enums) = crate::proto_parser::parse_all_protos(proto_dir)?;

    let mut cache = ProtoDocsCache::new(api_version.to_string(), commit.to_string());
    cache.messages = messages;
    cache.enums = enums;

    Ok(cache)
}

/// Load or build the cache.
/// Note: This function does NOT save the cache to disk. Callers should call
/// `cache.save_to_disk(&path)` explicitly after building if they want to persist it.
pub fn load_or_build_cache(proto_dir: &PathBuf) -> Result<ProtoDocsCache> {
    let cache_path = get_cache_path()?;

    // Try to load existing cache first
    if cache_path.exists() {
        match ProtoDocsCache::load_from_disk(&cache_path) {
            Ok(cache) => {
                // Check if cache is still valid (same commit)
                let current_commit = extract_commit_from_path(proto_dir).unwrap_or_default();
                if cache.is_valid(&current_commit) {
                    log::info!("Using cached proto docs from {:?}", cache_path);
                    return Ok(cache);
                } else {
                    log::info!("Proto docs cache invalidated - rebuilding...");
                }
            }
            Err(e) => {
                log::warn!("Failed to load proto docs cache: {} - rebuilding...", e);
            }
        }
    }

    // Build new cache
    let api_version = "v23";
    let commit = extract_commit_from_path(proto_dir).unwrap_or_default();

    log::info!("Parsing proto files from {:?}", proto_dir);
    let cache = build_cache(proto_dir, api_version, &commit)?;

    let stats = cache.stats();
    log::info!("{}", stats);

    Ok(cache)
}


/// Merge proto documentation into a FieldMetadataCache.
/// For each field in the cache, look up the proto documentation and populate description.
pub fn merge_into_field_metadata_cache(
    proto_cache: &ProtoDocsCache,
    field_cache: &mut mcc_gaql_common::field_metadata::FieldMetadataCache,
) -> usize {
    let mut enriched_count = 0;

    for (field_name, field_meta) in &mut field_cache.fields {
        // Skip if already has description
        if field_meta.description.is_some() {
            continue;
        }

        // Convert GAQL name to proto format
        let (message_name, field_name_proto) = match gaql_to_proto(field_name) {
            Some(m) => m,
            None => continue,
        };

        // Look up proto field doc
        if let Some(proto_field) = proto_cache.get_field_doc(&message_name, &field_name_proto) {
            if !proto_field.description.is_empty() {
                field_meta.description = Some(proto_field.description.clone());
                enriched_count += 1;
            }
        }
    }

    // Also enrich resource-level metadata
    if let Some(ref mut resource_metadata) = field_cache.resource_metadata {
        for (resource_name, res_meta) in resource_metadata.iter_mut() {
            // Convert snake_case to PascalCase
            let message_name = snake_to_pascal_case(resource_name);

            if let Some(proto_desc) = proto_cache.get_resource_description(&message_name) {
                if res_meta.description.is_none() || res_meta.description.as_ref().map_or(true, |d| d.is_empty()) {
                    res_meta.description = Some(proto_desc.to_string());
                }
            }
        }
    }

    enriched_count
}

/// Extract commit hash from proto directory path.
/// Path format: .../checkouts/googleads-rs-xxx/COMMIT/proto/...
fn extract_commit_from_path(path: &Path) -> Result<String> {
    use std::path::Component;

    let components: Vec<_> = path.components().collect();

    for (i, component) in components.iter().enumerate() {
        if let Component::Normal(name) = component {
            if name.to_str() == Some("checkouts") && i + 2 < components.len() {
                // The component after googleads-rs-* should be the commit hash
                if let Component::Normal(commit) = &components[i + 2] {
                    let commit_str = commit.to_string_lossy();
                    // Verify it looks like a commit hash (hex, at least 7 chars, no dots)
                    if commit_str.len() >= 7
                        && !commit_str.contains('.')
                        && commit_str.chars().all(|c| c.is_ascii_hexdigit())
                    {
                        return Ok(commit_str.to_string());
                    }
                }
            }
        }
    }

    // Fallback: use "unknown" - cache will still work but won't invalidate on updates
    Ok("unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_stats() {
        let cache = ProtoDocsCache::new("v23".to_string(), "abc123".to_string());
        let stats = cache.stats();

        assert_eq!(stats.message_count, 0);
        assert_eq!(stats.field_count, 0);
        assert_eq!(stats.enum_count, 0);
        assert_eq!(stats.enum_value_count, 0);
    }

    #[test]
    fn test_cache_validity() {
        let cache = ProtoDocsCache::new("v23".to_string(), "abc123".to_string());

        assert!(cache.is_valid("abc123"));
        assert!(!cache.is_valid("different"));
    }

    #[test]
    fn test_gaql_to_proto() {
        // Test basic resource name conversion
        assert_eq!(gaql_to_proto("campaign.name"), Some(("Campaign".to_string(), "name".to_string())));
        assert_eq!(gaql_to_proto("ad_group.status"), Some(("AdGroup".to_string(), "status".to_string())));
        assert_eq!(gaql_to_proto("ad_group_ad.ad_group"), Some(("AdGroupAd".to_string(), "ad_group".to_string())));

        // Test metrics and segments
        assert_eq!(gaql_to_proto("metrics.clicks"), Some(("Metrics".to_string(), "clicks".to_string())));
        assert_eq!(gaql_to_proto("segments.device"), Some(("Segments".to_string(), "device".to_string())));

        // Test invalid inputs
        assert_eq!(gaql_to_proto("campaign"), None); // No dot
        assert_eq!(gaql_to_proto("campaign.name.extra"), None); // Too many parts
    }

    #[test]
    fn test_snake_to_pascal_case() {
        assert_eq!(snake_to_pascal_case("campaign"), "Campaign");
        assert_eq!(snake_to_pascal_case("ad_group"), "AdGroup");
        assert_eq!(snake_to_pascal_case("campaign_budget"), "CampaignBudget");
        assert_eq!(snake_to_pascal_case("ad_group_criterion"), "AdGroupCriterion");
        assert_eq!(snake_to_pascal_case(""), "");
    }
}
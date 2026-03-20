//! Proto documentation cache for storing parsed proto file documentation.
//!
//! This module provides caching functionality to avoid re-parsing proto files
//! on every run. The cache is keyed by googleads-rs version/commit.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::proto_parser::{ProtoEnumDoc, ProtoMessageDoc};

/// Current schema version. Increment when the cache format changes incompatibly.
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Convert PascalCase to snake_case.
/// e.g., "AdGroup" -> "ad_group", "CampaignBudget" -> "campaign_budget"
fn pascal_to_snake(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

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
/// e.g., "ad_group_ad.policy_summary.approval_status" -> ("AdGroupAd", "policy_summary")
///   (returns the resource message and the first field segment; nested resolution is done via graph traversal)
pub fn gaql_to_proto(field_name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = field_name.split('.').collect();
    if parts.len() < 2 {
        return None;
    }

    let resource = parts[0];
    let field = parts[1];

    // Convert snake_case to PascalCase
    let message_name = snake_to_pascal_case(resource);

    Some((message_name, field.to_string()))
}

/// Extract the simple (last-segment) type name from a possibly fully-qualified proto type.
/// e.g., `"google.ads.googleads.v23.common.PolicySummary"` → `"PolicySummary"`
/// e.g., `"PolicySummary"` → `"PolicySummary"`
fn simple_type_name(type_name: &str) -> &str {
    type_name.rsplit('.').next().unwrap_or(type_name)
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
    /// Cache schema version; old caches (0/absent) are rebuilt automatically.
    #[serde(default)]
    pub schema_version: u32,
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
            schema_version: CURRENT_SCHEMA_VERSION,
        }
    }

    /// Get field documentation for a specific message and field.
    pub fn get_field_doc(
        &self,
        message_name: &str,
        field_name: &str,
    ) -> Option<&crate::proto_parser::ProtoFieldDoc> {
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
    /// Uses graph traversal from resource messages to emit nested GAQL keys like
    /// `ad_group_ad.policy_summary.approval_status`.
    pub fn to_scraped_docs(&self) -> crate::scraper::ScrapedDocs {
        use crate::proto_parser::FieldBehavior;
        use crate::scraper::{ScrapedDocs, ScrapedFieldDoc};
        use std::collections::{HashMap, HashSet};

        let mut docs: HashMap<String, ScrapedFieldDoc> = HashMap::new();
        let mut visited_messages: HashSet<String> = HashSet::new();

        /// Build a `ScrapedFieldDoc` from a proto field, resolving enum values from the cache.
        fn make_field_doc(
            field: &crate::proto_parser::ProtoFieldDoc,
            enums: &HashMap<String, crate::proto_parser::ProtoEnumDoc>,
        ) -> ScrapedFieldDoc {
            let field_behavior: Vec<String> = field
                .field_behavior
                .iter()
                .map(|b| match b {
                    FieldBehavior::Immutable => "IMMUTABLE".to_string(),
                    FieldBehavior::OutputOnly => "OUTPUT_ONLY".to_string(),
                    FieldBehavior::Required => "REQUIRED".to_string(),
                    FieldBehavior::Optional => "OPTIONAL".to_string(),
                })
                .collect();

            let (enum_values, enum_value_descriptions) = if field.is_enum {
                field
                    .enum_type
                    .as_ref()
                    .and_then(|et| enums.get(et))
                    .map(|e| {
                        let values: Vec<String> =
                            e.values.iter().map(|v| v.name.clone()).collect();
                        let descriptions: Vec<String> = e
                            .values
                            .iter()
                            .filter(|v| !v.description.is_empty())
                            .map(|v| format!("{}: {}", v.name, v.description))
                            .collect();
                        (values, descriptions)
                    })
                    .unwrap_or_default()
            } else {
                (Vec::new(), Vec::new())
            };

            ScrapedFieldDoc {
                description: field.description.clone(),
                enum_values,
                enum_value_descriptions,
                field_behavior,
                proto_type: field.type_name.clone(),
            }
        }

        /// Walk a message's fields recursively, emitting docs keyed by GAQL path.
        fn walk_message(
            message_name: &str,
            prefix: &str,
            messages: &HashMap<String, ProtoMessageDoc>,
            enums: &HashMap<String, crate::proto_parser::ProtoEnumDoc>,
            docs: &mut HashMap<String, ScrapedFieldDoc>,
            visited: &mut HashSet<String>,
        ) {
            if !visited.insert(message_name.to_string()) {
                return; // cycle guard
            }

            let Some(message) = messages.get(message_name) else {
                return;
            };

            for field in &message.fields {
                let key = format!("{}.{}", prefix, field.field_name);
                docs.insert(key.clone(), make_field_doc(field, enums));

                // If the field's type resolves to a known message, recurse
                let simple = simple_type_name(&field.type_name);
                if messages.contains_key(simple) {
                    walk_message(simple, &key, messages, enums, docs, visited);
                }
            }

            visited.remove(message_name);
        }

        // Seed from resource messages only
        let resource_count = self
            .messages
            .values()
            .filter(|m| m.is_resource)
            .count();

        for (message_name, message) in &self.messages {
            if !message.is_resource {
                continue;
            }

            // Resource-level description entry (e.g., key = "campaign")
            let resource_key = pascal_to_snake(message_name);
            if !message.description.is_empty() {
                docs.insert(
                    resource_key.clone(),
                    ScrapedFieldDoc {
                        description: message.description.clone(),
                        enum_values: Vec::new(),
                        enum_value_descriptions: Vec::new(),
                        field_behavior: Vec::new(),
                        proto_type: String::new(),
                    },
                );
            }

            // Walk fields, building nested GAQL keys
            walk_message(
                message_name,
                &resource_key,
                &self.messages,
                &self.enums,
                &mut docs,
                &mut visited_messages,
            );
        }

        ScrapedDocs {
            scraped_at: self.parsed_at,
            api_version: self.api_version.clone(),
            docs,
            resources_scraped: resource_count,
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
        let cache: ProtoDocsCache =
            serde_json::from_str(&content).context("Failed to parse cache JSON")?;

        Ok(cache)
    }

    /// Check if cache is valid for a given commit and schema version.
    pub fn is_valid(&self, expected_commit: &str) -> bool {
        self.googleads_rs_commit == expected_commit
            && self.schema_version == CURRENT_SCHEMA_VERSION
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
                if res_meta.description.is_none()
                    || res_meta.description.as_ref().map_or(true, |d| d.is_empty())
                {
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
        assert_eq!(
            gaql_to_proto("campaign.name"),
            Some(("Campaign".to_string(), "name".to_string()))
        );
        assert_eq!(
            gaql_to_proto("ad_group.status"),
            Some(("AdGroup".to_string(), "status".to_string()))
        );
        assert_eq!(
            gaql_to_proto("ad_group_ad.ad_group"),
            Some(("AdGroupAd".to_string(), "ad_group".to_string()))
        );

        // Test metrics and segments
        assert_eq!(
            gaql_to_proto("metrics.clicks"),
            Some(("Metrics".to_string(), "clicks".to_string()))
        );
        assert_eq!(
            gaql_to_proto("segments.device"),
            Some(("Segments".to_string(), "device".to_string()))
        );

        // Test invalid inputs
        assert_eq!(gaql_to_proto("campaign"), None); // No dot — must have at least resource.field
        // Nested fields with 3+ parts are now valid (returns resource message + first field segment)
        assert_eq!(
            gaql_to_proto("campaign.name.extra"),
            Some(("Campaign".to_string(), "name".to_string()))
        );
    }

    #[test]
    fn test_snake_to_pascal_case() {
        assert_eq!(snake_to_pascal_case("campaign"), "Campaign");
        assert_eq!(snake_to_pascal_case("ad_group"), "AdGroup");
        assert_eq!(snake_to_pascal_case("campaign_budget"), "CampaignBudget");
        assert_eq!(
            snake_to_pascal_case("ad_group_criterion"),
            "AdGroupCriterion"
        );
        assert_eq!(snake_to_pascal_case(""), "");
    }

    #[test]
    fn test_pascal_to_snake() {
        assert_eq!(pascal_to_snake("Campaign"), "campaign");
        assert_eq!(pascal_to_snake("AdGroup"), "ad_group");
        assert_eq!(pascal_to_snake("CampaignBudget"), "campaign_budget");
        assert_eq!(pascal_to_snake("AdGroupCriterion"), "ad_group_criterion");
        assert_eq!(pascal_to_snake("AdGroupAd"), "ad_group_ad");
        assert_eq!(pascal_to_snake("KeywordView"), "keyword_view");
        assert_eq!(pascal_to_snake(""), "");
    }

    #[test]
    fn test_schema_version_invalidates_cache() {
        let cache = ProtoDocsCache::new("v23".to_string(), "abc123".to_string());
        // Current schema version should be valid
        assert!(cache.is_valid("abc123"));

        // A cache with schema_version = 0 (old/absent) should be invalid
        let mut old_cache = cache.clone();
        old_cache.schema_version = 0;
        assert!(!old_cache.is_valid("abc123"));
    }

    fn make_test_cache() -> ProtoDocsCache {
        use crate::proto_parser::{ProtoFieldDoc, ProtoMessageDoc};

        let mut cache = ProtoDocsCache::new("v23".to_string(), "test".to_string());

        // PolicySummary sub-message (not a resource)
        let policy_summary = ProtoMessageDoc {
            message_name: "PolicySummary".to_string(),
            description: "Policy summary for an ad.".to_string(),
            fields: vec![ProtoFieldDoc {
                field_name: "approval_status".to_string(),
                field_number: 1,
                description: "The approval status of the ad.".to_string(),
                field_behavior: vec![],
                type_name: "ApprovalStatus".to_string(),
                is_enum: false,
                enum_type: None,
            }],
            is_resource: false,
        };

        // AdGroupAd resource message
        let ad_group_ad = ProtoMessageDoc {
            message_name: "AdGroupAd".to_string(),
            description: "An ad group ad.".to_string(),
            fields: vec![
                ProtoFieldDoc {
                    field_name: "resource_name".to_string(),
                    field_number: 1,
                    description: "The resource name.".to_string(),
                    field_behavior: vec![],
                    type_name: "string".to_string(),
                    is_enum: false,
                    enum_type: None,
                },
                ProtoFieldDoc {
                    field_name: "policy_summary".to_string(),
                    field_number: 2,
                    description: "Policy summary.".to_string(),
                    field_behavior: vec![],
                    type_name: "PolicySummary".to_string(),
                    is_enum: false,
                    enum_type: None,
                },
            ],
            is_resource: true,
        };

        cache
            .messages
            .insert("PolicySummary".to_string(), policy_summary);
        cache
            .messages
            .insert("AdGroupAd".to_string(), ad_group_ad);
        cache
    }

    #[test]
    fn test_to_scraped_docs_nested_keys() {
        let cache = make_test_cache();
        let scraped = cache.to_scraped_docs();

        // Flat key should exist
        assert!(
            scraped.docs.contains_key("ad_group_ad.resource_name"),
            "Flat key ad_group_ad.resource_name should be present"
        );
        assert!(
            scraped.docs.contains_key("ad_group_ad.policy_summary"),
            "Intermediate key ad_group_ad.policy_summary should be present"
        );
        // Nested key should be generated via graph traversal
        assert!(
            scraped
                .docs
                .contains_key("ad_group_ad.policy_summary.approval_status"),
            "Nested key ad_group_ad.policy_summary.approval_status should be present"
        );

        let nested = scraped
            .docs
            .get("ad_group_ad.policy_summary.approval_status")
            .unwrap();
        assert_eq!(nested.description, "The approval status of the ad.");
    }

    #[test]
    fn test_to_scraped_docs_simple_fields_unchanged() {
        let cache = make_test_cache();
        let scraped = cache.to_scraped_docs();

        let name_doc = scraped.docs.get("ad_group_ad.resource_name").unwrap();
        assert_eq!(name_doc.description, "The resource name.");
        assert_eq!(name_doc.proto_type, "string");
    }

    #[test]
    fn test_to_scraped_docs_cycle_guard() {
        use crate::proto_parser::{ProtoFieldDoc, ProtoMessageDoc};

        let mut cache = ProtoDocsCache::new("v23".to_string(), "test".to_string());

        // A -> B -> A (cycle)
        let msg_a = ProtoMessageDoc {
            message_name: "MsgA".to_string(),
            description: String::new(),
            fields: vec![ProtoFieldDoc {
                field_name: "b_ref".to_string(),
                field_number: 1,
                description: String::new(),
                field_behavior: vec![],
                type_name: "MsgB".to_string(),
                is_enum: false,
                enum_type: None,
            }],
            is_resource: true,
        };

        let msg_b = ProtoMessageDoc {
            message_name: "MsgB".to_string(),
            description: String::new(),
            fields: vec![ProtoFieldDoc {
                field_name: "a_ref".to_string(),
                field_number: 1,
                description: String::new(),
                field_behavior: vec![],
                type_name: "MsgA".to_string(),
                is_enum: false,
                enum_type: None,
            }],
            is_resource: false,
        };

        cache.messages.insert("MsgA".to_string(), msg_a);
        cache.messages.insert("MsgB".to_string(), msg_b);

        // Must not infinite-loop
        let scraped = cache.to_scraped_docs();
        // At minimum MsgA's own field should be emitted
        assert!(scraped.docs.contains_key("msg_a.b_ref"));
    }
}

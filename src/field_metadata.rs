//
// Author: Michael S. Huang (mhuang74@gmail.com)
//
// Field metadata module for Google Ads API schema awareness
// Provides caching and querying of field metadata from Fields Service API

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::fs;

use googleads_rs::google::ads::googleads::v22::services::google_ads_field_service_client::GoogleAdsFieldServiceClient;
use googleads_rs::google::ads::googleads::v22::services::SearchGoogleAdsFieldsRequest;

use crate::googleads::GoogleAdsAPIAccess;

/// Represents metadata for a single Google Ads field
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct FieldMetadata {
    pub name: String,
    pub category: String,
    pub data_type: String,
    pub selectable: bool,
    pub filterable: bool,
    pub sortable: bool,
    pub metrics_compatible: bool,
    pub resource_name: Option<String>,
}

impl FieldMetadata {
    /// Check if this field is a metric
    pub fn is_metric(&self) -> bool {
        self.category == "METRIC" || self.name.starts_with("metrics.")
    }

    /// Check if this field is a segment
    pub fn is_segment(&self) -> bool {
        self.category == "SEGMENT" || self.name.starts_with("segments.")
    }

    /// Check if this field is an attribute
    pub fn is_attribute(&self) -> bool {
        self.category == "ATTRIBUTE"
    }

    /// Check if this field is a resource
    pub fn is_resource(&self) -> bool {
        self.category == "RESOURCE"
    }

    /// Get the resource name for this field (e.g., "campaign" from "campaign.name")
    pub fn get_resource(&self) -> Option<String> {
        let parsed_resource = 
            if let Some(_idx) = self.name.find('.') {
                self.name
                .split('.').next() // Get the first substring delimited by '.'
                .map(|s| s.to_string())
            } else {
                None
            };

        // log::debug!("Field:{:?} -> Resource: {}", self.name, parsed_resource.as_deref().unwrap_or("n/a"));

        parsed_resource
    }
}

/// Cache for Google Ads field metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMetadataCache {
    pub last_updated: DateTime<Utc>,
    pub api_version: String,
    pub fields: HashMap<String, FieldMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<HashMap<String, Vec<String>>>,
}

impl FieldMetadataCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            last_updated: Utc::now(),
            api_version: "v22".to_string(),
            fields: HashMap::new(),
            resources: None,
        }
    }

    /// Load cache from file or fetch from API if stale/missing
    pub async fn load_or_fetch(
        api_context: Option<&GoogleAdsAPIAccess>,
        cache_path: &Path,
        max_age_days: i64,
    ) -> Result<Self> {
        // Try to load from cache
        if cache_path.exists() {
            match Self::load_from_disk(cache_path).await {
                Ok(cache) => {
                    let age = Utc::now() - cache.last_updated;
                    if age < Duration::days(max_age_days) {
                        log::info!(
                            "Loaded field metadata cache from {:?} (age: {} days)",
                            cache_path,
                            age.num_days()
                        );
                        return Ok(cache);
                    } else {
                        log::info!(
                            "Field metadata cache is stale (age: {} days), fetching fresh data",
                            age.num_days()
                        );
                    }
                }
                Err(e) => {
                    log::warn!("Failed to load cache from {:?}: {}", cache_path, e);
                }
            }
        }

        // Cache missing or stale, fetch from API
        if let Some(api) = api_context {
            let cache = Self::fetch_from_api(api).await?;
            cache.save_to_disk(cache_path).await?;
            Ok(cache)
        } else {
            Err(anyhow!("No cached field metadata found and no API context provided"))
        }
    }

    /// Fetch field metadata from Google Ads Fields Service API
    pub async fn fetch_from_api(api_context: &GoogleAdsAPIAccess) -> Result<Self> {
        log::info!("Fetching field metadata from Google Ads Fields Service API");

        let mut client = GoogleAdsFieldServiceClient::with_interceptor(
            api_context.channel.clone(),
            api_context.clone(),
        );

        // Query all fields
        let query = "select name, category, data_type, selectable, filterable, sortable order by name";
        let response = client
            .search_google_ads_fields(SearchGoogleAdsFieldsRequest {
                query: query.to_owned(),
                page_token: String::new(),
                page_size: 10000,
            })
            .await
            .context("Failed to query Fields Service API")?
            .into_inner();

        let mut fields = HashMap::new();
        let mut resources: HashMap<String, Vec<String>> = HashMap::new();

        for row in response.results {
            // Convert category enum to string representation
            let category = match row.category {
                1 => "RESOURCE",
                2 => "ATTRIBUTE",
                3 => "SEGMENT",
                4 => "METRIC",
                _ => "UNKNOWN",
            }.to_string();

            // Convert data_type enum to string representation
            let data_type = match row.data_type {
                1 => "BOOLEAN",
                2 => "DATE",
                3 => "DOUBLE",
                4 => "ENUM",
                5 => "FLOAT",
                6 => "INT32",
                7 => "INT64",
                8 => "MESSAGE",
                9 => "RESOURCE_NAME",
                10 => "STRING",
                11 => "UINT64",
                _ => "UNKNOWN",
            }.to_string();

            // Determine metrics compatibility based on category and field properties
            let metrics_compatible = category == "ATTRIBUTE" || category == "SEGMENT";

            let field_meta = FieldMetadata {
                name: row.name.clone(),
                category,
                data_type,
                selectable: row.selectable,
                filterable: row.filterable,
                sortable: row.sortable,
                metrics_compatible,
                resource_name: if row.resource_name.is_empty() {
                    None
                } else {
                    Some(row.resource_name.clone())
                },
            };

            // Organize by resource
            if let Some(resource) = field_meta.get_resource() {
                resources
                    .entry(resource)
                    .or_insert_with(Vec::new)
                    .push(row.name.clone());
            }

            fields.insert(row.name, field_meta);
        }
        
        log::info!("Fetched {} fields from {} resources", fields.len(), resources.keys().len());

        Ok(Self {
            last_updated: Utc::now(),
            api_version: "v22".to_string(),
            fields,
            resources: Some(resources),
        })
    }

    /// Load cache from disk
    pub async fn load_from_disk(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .await
            .context("Failed to read cache file")?;

        let cache: Self = serde_json::from_str(&contents)
            .context("Failed to parse cache file")?;

        Ok(cache)
    }

    /// Save cache to disk
    pub async fn save_to_disk(&self, path: &Path) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .await
                .context("Failed to create cache directory")?;
        }

        let contents = serde_json::to_string_pretty(self)
            .context("Failed to serialize cache")?;

        fs::write(path, contents)
            .await
            .context("Failed to write cache file")?;

        log::info!("Saved field metadata cache to {:?}", path);
        Ok(())
    }

    /// Get all metrics fields
    pub fn get_metrics(&self, pattern: Option<&str>) -> Vec<&FieldMetadata> {
        self.fields
            .values()
            .filter(|f| f.is_metric())
            .filter(|f| {
                if let Some(p) = pattern {
                    f.name.contains(p)
                } else {
                    true
                }
            })
            .collect()
    }

    /// Get all segment fields
    pub fn get_segments(&self, pattern: Option<&str>) -> Vec<&FieldMetadata> {
        self.fields
            .values()
            .filter(|f| f.is_segment())
            .filter(|f| {
                if let Some(p) = pattern {
                    f.name.contains(p)
                } else {
                    true
                }
            })
            .collect()
    }

    /// Get all attribute fields for a resource
    pub fn get_attributes(&self, resource: &str) -> Vec<&FieldMetadata> {
        self.fields
            .values()
            .filter(|f| {
                if let Some(r) = f.get_resource() {
                    r == resource && f.is_attribute()
                } else {
                    false
                }
            })
            .collect()
    }

    /// Get all fields for a specific resource
    pub fn get_resource_fields(&self, resource: &str) -> Vec<&FieldMetadata> {
        if let Some(resources) = &self.resources {
            if let Some(field_names) = resources.get(resource) {
                return field_names
                    .iter()
                    .filter_map(|name| self.fields.get(name))
                    .collect();
            }
        }

        // Fallback: filter by resource name prefix
        self.fields
            .values()
            .filter(|f| {
                if let Some(r) = f.get_resource() {
                    r == resource
                } else {
                    false
                }
            })
            .collect()
    }

    /// Get all available resources
    pub fn get_resources(&self) -> Vec<String> {
        if let Some(resources) = &self.resources {
            let mut names: Vec<String> = resources.keys().cloned().collect();
            names.sort();
            names
        } else {
            // Fallback: extract from field names
            let mut resources: Vec<String> = self
                .fields
                .values()
                .filter_map(|f| f.get_resource())
                .collect();
            resources.sort();
            resources.dedup();
            resources
        }
    }

    /// Find fields by name pattern
    pub fn find_fields(&self, pattern: &str) -> Vec<&FieldMetadata> {
        self.fields
            .values()
            .filter(|f| f.name.contains(pattern))
            .collect()
    }

    /// Get field by exact name
    pub fn get_field(&self, name: &str) -> Option<&FieldMetadata> {
        self.fields.get(name)
    }

    /// Validate if a set of fields can be selected together
    pub fn validate_field_selection(&self, field_names: &[String]) -> ValidationResult {
        let mut errors = Vec::new();
        let mut warnings = Vec::new();
        let mut missing_fields = Vec::new();

        // Check if all fields exist
        for name in field_names {
            if !self.fields.contains_key(name) {
                missing_fields.push(name.clone());
            }
        }

        if !missing_fields.is_empty() {
            errors.push(ValidationError::UnknownFields(missing_fields));
        }

        // Get all fields
        let fields: Vec<&FieldMetadata> = field_names
            .iter()
            .filter_map(|name| self.fields.get(name))
            .collect();

        // Check if all fields are selectable
        let non_selectable: Vec<String> = fields
            .iter()
            .filter(|f| !f.selectable)
            .map(|f| f.name.clone())
            .collect();

        if !non_selectable.is_empty() {
            errors.push(ValidationError::NonSelectableFields(non_selectable));
        }

        // Check if metrics are used with proper grouping
        let has_metrics = fields.iter().any(|f| f.is_metric());
        let has_segments = fields.iter().any(|f| f.is_segment());
        let has_resources = fields.iter().any(|f| f.is_resource() || (!f.is_metric() && !f.is_segment()));

        if has_metrics && !has_segments && !has_resources {
            warnings.push(ValidationWarning::MetricsWithoutGrouping);
        }

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
        }
    }

    /// Export schema summary as formatted text
    pub fn export_summary(&self) -> String {
        let mut output = String::new();

        output.push_str(&format!("# Google Ads Field Metadata\n\n"));
        output.push_str(&format!("Last Updated: {}\n", self.last_updated.format("%Y-%m-%d %H:%M:%S UTC")));
        output.push_str(&format!("API Version: {}\n", self.api_version));
        output.push_str(&format!("Total Fields: {}\n\n", self.fields.len()));

        // Resources
        output.push_str("## Resources\n\n");
        let resources = self.get_resources();
        for resource in &resources {
            let field_count = self.get_resource_fields(resource).len();
            output.push_str(&format!("- {}: {} fields\n", resource, field_count));
        }
        output.push_str("\n");

        // Metrics summary
        let metrics = self.get_metrics(None);
        output.push_str(&format!("## Metrics ({} total)\n\n", metrics.len()));
        output.push_str("Common metrics:\n");
        let common_metrics = ["impressions", "clicks", "cost_micros", "conversions", "ctr", "average_cpc"];
        for metric_name in common_metrics {
            if let Some(field) = self.get_field(&format!("metrics.{}", metric_name)) {
                output.push_str(&format!("- {}: {} ({})\n",
                    field.name,
                    field.data_type,
                    if field.filterable { "filterable" } else { "not filterable" }
                ));
            }
        }
        output.push_str("\n");

        // Segments summary
        let segments = self.get_segments(None);
        output.push_str(&format!("## Segments ({} total)\n\n", segments.len()));
        output.push_str("Common segments:\n");
        let common_segments = ["date", "week", "month", "device", "ad_network_type"];
        for segment_name in common_segments {
            if let Some(field) = self.get_field(&format!("segments.{}", segment_name)) {
                output.push_str(&format!("- {}: {}\n", field.name, field.data_type));
            }
        }

        output
    }
}

impl Default for FieldMetadataCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of field validation
#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    pub fn format_message(&self) -> String {
        let mut output = String::new();

        if !self.errors.is_empty() {
            output.push_str("Validation Errors:\n");
            for error in &self.errors {
                output.push_str(&format!("  - {}\n", error));
            }
        }

        if !self.warnings.is_empty() {
            output.push_str("Validation Warnings:\n");
            for warning in &self.warnings {
                output.push_str(&format!("  - {}\n", warning));
            }
        }

        if self.is_valid && self.warnings.is_empty() {
            output.push_str("✓ All fields are valid\n");
        }

        output
    }
}

/// Validation errors
#[derive(Debug)]
pub enum ValidationError {
    UnknownFields(Vec<String>),
    NonSelectableFields(Vec<String>),
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::UnknownFields(fields) => {
                write!(f, "Unknown fields: {}", fields.join(", "))
            }
            ValidationError::NonSelectableFields(fields) => {
                write!(f, "Non-selectable fields: {}", fields.join(", "))
            }
        }
    }
}

/// Validation warnings
#[derive(Debug)]
pub enum ValidationWarning {
    MetricsWithoutGrouping,
}

impl std::fmt::Display for ValidationWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationWarning::MetricsWithoutGrouping => {
                write!(f, "Metrics selected without segments or resource fields (may cause aggregation issues)")
            }
        }
    }
}

/// Helper to get default cache path
pub fn get_default_cache_path() -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow!("Could not determine cache directory"))?;

    Ok(cache_dir.join("mcc-gaql").join("field_metadata.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_field_metadata_resource_extraction() {
        let field = FieldMetadata {
            name: "ad.app_ad.headlines".to_string(),
            category: "ATTRIBUTE".to_string(),
            data_type: "STRING".to_string(),
            selectable: true,
            filterable: true,
            sortable: true,
            metrics_compatible: true,
            resource_name: None,
        };

        assert_eq!(field.get_resource(), Some("ad".to_string()));
        assert!(field.is_attribute());
        assert!(!field.is_metric());
    }

    #[test]
    fn test_field_metadata_is_metric() {
        let field = FieldMetadata {
            name: "metrics.impressions".to_string(),
            category: "METRIC".to_string(),
            data_type: "INT64".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
        };

        assert!(field.is_metric());
        assert!(!field.is_attribute());
        assert_eq!(field.get_resource(), Some("metrics".to_string()));
    }

    #[test]
    fn test_cache_validation() {
        let mut cache = FieldMetadataCache::new();

        // Add test fields
        cache.fields.insert(
            "campaign.name".to_string(),
            FieldMetadata {
                name: "campaign.name".to_string(),
                category: "ATTRIBUTE".to_string(),
                data_type: "STRING".to_string(),
                selectable: true,
                filterable: true,
                sortable: true,
                metrics_compatible: true,
                resource_name: None,
            },
        );

        cache.fields.insert(
            "metrics.impressions".to_string(),
            FieldMetadata {
                name: "metrics.impressions".to_string(),
                category: "METRIC".to_string(),
                data_type: "INT64".to_string(),
                selectable: true,
                filterable: false,
                sortable: true,
                metrics_compatible: false,
                resource_name: None,
            },
        );

        // Valid selection
        let result = cache.validate_field_selection(&[
            "campaign.name".to_string(),
            "metrics.impressions".to_string(),
        ]);
        assert!(result.is_valid);

        // Invalid: unknown field
        let result = cache.validate_field_selection(&[
            "campaign.name".to_string(),
            "unknown.field".to_string(),
        ]);
        assert!(!result.is_valid);
    }
}

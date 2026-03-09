//
// Shared field metadata types for mcc-gaql workspace.
// This module contains only types and file I/O; no API calls.
// API-fetching logic lives in mcc-gaql's field_metadata module.

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

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

    // Extended structural metadata (from Fields Service)
    #[serde(default)]
    pub selectable_with: Vec<String>,
    #[serde(default)]
    pub enum_values: Vec<String>,
    #[serde(default)]
    pub attribute_resources: Vec<String>,

    // Enriched documentation (populated by metadata_enricher)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_notes: Option<String>,
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
        if let Some(_idx) = self.name.find('.') {
            self.name
                .split('.')
                .next()
                .map(|s| s.to_string())
        } else {
            None
        }
    }

    /// Build a rich text description for embedding, using enriched description if available
    /// or falling back to a synthesized description from field metadata.
    pub fn build_embedding_text(&self) -> String {
        let mut parts = Vec::new();

        // Field name + structural tags
        let flags: Vec<&str> = [
            if self.selectable {
                Some("selectable")
            } else {
                None
            },
            if self.filterable {
                Some("filterable")
            } else {
                None
            },
            if self.sortable {
                Some("sortable")
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .collect();

        parts.push(format!(
            "{} [{}, {}{}]",
            self.name,
            self.category,
            self.data_type,
            if flags.is_empty() {
                String::new()
            } else {
                format!(", {}", flags.join(", "))
            }
        ));

        // Human-readable description (enriched or synthesized)
        if let Some(desc) = &self.description {
            parts.push(desc.clone());
        }

        // Usage notes
        if let Some(notes) = &self.usage_notes {
            parts.push(notes.clone());
        }

        // Enum values
        if !self.enum_values.is_empty() {
            parts.push(format!("Valid values: {}", self.enum_values.join(", ")));
        }

        // Resource context
        if !self.attribute_resources.is_empty() {
            parts.push(format!("Resource: {}", self.attribute_resources.join(", ")));
        } else if let Some(r) = self.get_resource()
            && r != "metrics"
            && r != "segments"
        {
            parts.push(format!("Resource: {}", r));
        }

        parts.join(". ")
    }
}

/// Resource-level metadata capturing hierarchy and relationships
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceMetadata {
    pub name: String,
    /// Fields that this resource can be queried together with
    #[serde(default)]
    pub selectable_with: Vec<String>,
    /// Key attributes for this resource (most useful for typical queries)
    #[serde(default)]
    pub key_attributes: Vec<String>,
    /// Key metrics available for this resource
    #[serde(default)]
    pub key_metrics: Vec<String>,
    /// Total number of fields (attributes + metrics + segments) for this resource
    pub field_count: usize,
    /// Description (populated by enricher)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Cache for Google Ads field metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMetadataCache {
    pub last_updated: DateTime<Utc>,
    pub api_version: String,
    pub fields: HashMap<String, FieldMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<HashMap<String, Vec<String>>>,
    /// Resource-level metadata (populated by enricher or computed from fields)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource_metadata: Option<HashMap<String, ResourceMetadata>>,
}

impl FieldMetadataCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self {
            last_updated: Utc::now(),
            api_version: "v23".to_string(),
            fields: HashMap::new(),
            resources: None,
            resource_metadata: None,
        }
    }

    /// Load cache from disk
    pub async fn load_from_disk(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .await
            .context("Failed to read cache file")?;

        let cache: Self = serde_json::from_str(&contents).context("Failed to parse cache file")?;

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

        let contents = serde_json::to_string_pretty(self).context("Failed to serialize cache")?;

        fs::write(path, contents)
            .await
            .context("Failed to write cache file")?;

        log::info!("Saved field metadata cache to {:?}", path);
        Ok(())
    }

    /// Load from disk (no API fallback - use FieldMetadataCache::load_or_fetch in mcc-gaql for API access)
    pub async fn load_from_disk_or_error(path: &Path, max_age_days: i64) -> Result<Self> {
        if !path.exists() {
            return Err(anyhow!(
                "Field metadata cache not found at {:?}. Run 'mcc-gaql --refresh-field-cache' to create it.",
                path
            ));
        }

        let cache = Self::load_from_disk(path).await?;
        let age = Utc::now() - cache.last_updated;
        if age >= Duration::days(max_age_days) {
            log::warn!(
                "Field metadata cache is stale (age: {} days). Consider refreshing with 'mcc-gaql --refresh-field-cache'.",
                age.num_days()
            );
        }

        Ok(cache)
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
        if let Some(resources) = &self.resources
            && let Some(field_names) = resources.get(resource)
        {
            field_names
                .iter()
                .filter_map(|name| self.fields.get(name))
                .collect()
        } else {
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
    }

    /// Get all available resources (sorted)
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

    /// Count enriched fields (those with a description set)
    pub fn enriched_field_count(&self) -> usize {
        self.fields
            .values()
            .filter(|f| f.description.is_some())
            .count()
    }

    /// Export schema summary as formatted text
    pub fn export_summary(&self) -> String {
        let mut output = String::new();

        output.push_str("# Google Ads Field Metadata\n\n");
        output.push_str(&format!(
            "Last Updated: {}\n",
            self.last_updated.format("%Y-%m-%d %H:%M:%S UTC")
        ));
        output.push_str(&format!("API Version: {}\n", self.api_version));
        output.push_str(&format!("Total Fields: {}\n", self.fields.len()));
        output.push_str(&format!(
            "Enriched Fields: {}\n\n",
            self.enriched_field_count()
        ));

        // Resources
        output.push_str("## Resources\n\n");
        let resources = self.get_resources();
        for resource in &resources {
            let field_count = self.get_resource_fields(resource).len();
            let desc = self
                .resource_metadata
                .as_ref()
                .and_then(|rm| rm.get(resource))
                .and_then(|rm| rm.description.as_deref())
                .unwrap_or("");
            if desc.is_empty() {
                output.push_str(&format!("- {}: {} fields\n", resource, field_count));
            } else {
                output.push_str(&format!(
                    "- {}: {} fields — {}\n",
                    resource, field_count, desc
                ));
            }
        }
        output.push('\n');

        // Metrics summary
        let metrics = self.get_metrics(None);
        output.push_str(&format!("## Metrics ({} total)\n\n", metrics.len()));
        output.push_str("Common metrics:\n");
        let common_metrics = [
            "impressions",
            "clicks",
            "cost_micros",
            "conversions",
            "ctr",
            "average_cpc",
        ];
        for metric_name in common_metrics {
            if let Some(field) = self.get_field(&format!("metrics.{}", metric_name)) {
                let desc_suffix = field
                    .description
                    .as_deref()
                    .map(|d| format!(" — {}", d))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "- {}: {} ({}){}\n",
                    field.name,
                    field.data_type,
                    if field.filterable {
                        "filterable"
                    } else {
                        "not filterable"
                    },
                    desc_suffix
                ));
            }
        }
        output.push('\n');

        // Segments summary
        let segments = self.get_segments(None);
        output.push_str(&format!("## Segments ({} total)\n\n", segments.len()));
        output.push_str("Common segments:\n");
        let common_segments = ["date", "week", "month", "device", "ad_network_type"];
        for segment_name in common_segments {
            if let Some(field) = self.get_field(&format!("segments.{}", segment_name)) {
                let desc_suffix = field
                    .description
                    .as_deref()
                    .map(|d| format!(" — {}", d))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "- {}: {}{}\n",
                    field.name, field.data_type, desc_suffix
                ));
            }
        }

        output
    }

    /// Print resource hierarchy and key fields
    pub fn show_resources(&self) -> String {
        let mut output = String::new();

        output.push_str("# Google Ads Resources\n\n");
        output.push_str(&format!(
            "API Version: {}  |  Last Updated: {}\n\n",
            self.api_version,
            self.last_updated.format("%Y-%m-%d")
        ));

        let resources = self.get_resources();
        output.push_str(&format!("{} resources available:\n\n", resources.len()));

        for resource in &resources {
            let field_count = self.get_resource_fields(resource).len();

            let (selectable_with, key_attrs, key_metrics, desc) = if let Some(rm) = self
                .resource_metadata
                .as_ref()
                .and_then(|m| m.get(resource))
            {
                (
                    rm.selectable_with.clone(),
                    rm.key_attributes.clone(),
                    rm.key_metrics.clone(),
                    rm.description.clone(),
                )
            } else {
                (vec![], vec![], vec![], None)
            };

            output.push_str(&format!("## {}\n", resource));
            output.push_str(&format!("Fields: {}\n", field_count));

            if let Some(d) = &desc {
                output.push_str(&format!("{}\n", d));
            }

            if !selectable_with.is_empty() {
                let displayed: Vec<&str> =
                    selectable_with.iter().take(8).map(String::as_str).collect();
                let suffix = if selectable_with.len() > 8 {
                    format!(" (+{})", selectable_with.len() - 8)
                } else {
                    String::new()
                };
                output.push_str(&format!(
                    "Can query with: {}{}\n",
                    displayed.join(", "),
                    suffix
                ));
            }

            if !key_attrs.is_empty() {
                output.push_str(&format!("Key attributes: {}\n", key_attrs.join(", ")));
            }
            if !key_metrics.is_empty() {
                output.push_str(&format!("Key metrics: {}\n", key_metrics.join(", ")));
            }

            output.push('\n');
        }

        output
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
        let has_resources = fields
            .iter()
            .any(|f| f.is_resource() || (!f.is_metric() && !f.is_segment()));

        if has_metrics && !has_segments && !has_resources {
            warnings.push(ValidationWarning::MetricsWithoutGrouping);
        }

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
            warnings,
        }
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
                write!(
                    f,
                    "Metrics selected without segments or resource fields (may cause aggregation issues)"
                )
            }
        }
    }
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
            selectable_with: vec![],
            enum_values: vec![],
            attribute_resources: vec![],
            description: None,
            usage_notes: None,
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
            selectable_with: vec![],
            enum_values: vec![],
            attribute_resources: vec![],
            description: None,
            usage_notes: None,
        };

        assert!(field.is_metric());
        assert!(!field.is_attribute());
    }
}

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
            self.name.split('.').next().map(|s| s.to_string())
        } else {
            None
        }
    }

    /// Build a rich text description for embedding, using enriched description if available
    /// or falling back to a synthesized description from field metadata.
    /// Enhanced semantic format - more natural language, removes structural flags.
    pub fn build_embedding_text(&self) -> String {
        let mut parts = Vec::new();

        // Field name + category tag only (no data_type or flags)
        parts.push(format!("{} [{}]", self.name, self.category));

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

        // Join with period and space, ensuring clean formatting
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
    /// Whether alphabetical fallback was used instead of AI selection
    #[serde(default)]
    pub uses_fallback: bool,
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

    /// Retain only fields and resources matching the given resource names.
    /// Also keeps metrics and segments that are compatible with any kept resource.
    pub fn retain_resources(&mut self, keep_resources: &[String]) {
        let keep_set: std::collections::HashSet<_> = keep_resources.iter().cloned().collect();

        // Filter fields - keep fields that:
        // 1. Belong to a retained resource (e.g., "keyword_view.resource_name")
        // 2. Are RESOURCE-category fields matching a kept resource (e.g., "keyword_view")
        // 3. Are metrics/segments compatible with any kept resource
        self.fields.retain(|_, field| {
            // Keep if field belongs to a retained resource (e.g., "keyword_view.resource_name")
            if let Some(r) = field.get_resource() {
                if keep_set.contains(&r) {
                    return true;
                }
            }

            // Keep RESOURCE-category fields whose name matches a kept resource (e.g., "keyword_view")
            if field.is_resource() && keep_set.contains(&field.name) {
                return true;
            }

            // Keep metrics and segments that are compatible with any kept resource
            // Check if any kept resource appears in the field's selectable_with list
            if field.is_metric() || field.is_segment() {
                return field.selectable_with.iter().any(|r| keep_set.contains(r));
            }

            false
        });

        // Filter resources map
        if let Some(resources) = &mut self.resources {
            resources.retain(|name, _| keep_set.contains(name));
        }

        // Filter resource metadata
        if let Some(resource_metadata) = &mut self.resource_metadata {
            resource_metadata.retain(|name, _| keep_set.contains(name));
        }
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

    /// Get the RESOURCE-category field's selectable_with list for a resource
    pub fn get_resource_selectable_with(&self, resource: &str) -> Vec<String> {
        self.fields
            .get(resource)
            .filter(|f| f.is_resource())
            .map(|f| f.selectable_with.clone())
            .unwrap_or_default()
    }

    /// Validate that all resources have properly populated selectable_with
    /// Returns error with list of resources that have empty selectable_with
    pub fn validate_selectable_with(&self) -> Result<(), Vec<String>> {
        let mut empty_resources: Vec<String> = Vec::new();

        // Only validate resources that need selectable_with for metrics compatibility
        // Skip resources that:
        // - Have no ATTRIBUTE fields (constants/namespaces like metrics, segments)
        // - Have no METRIC fields at all (dimension-only metadata resources like life_event)
        for resource_name in self.get_resources() {
            // Get all fields for this resource
            let resource_fields = self.get_resource_fields(&resource_name);

            // Skip if this resource has no ATTRIBUTE fields - it's a constant
            // or namespace, not a queryable resource
            let has_attributes = resource_fields.iter().any(|f| f.is_attribute());
            if !has_attributes {
                continue;
            }

            // Skip if this resource has no METRIC fields at all
            // These are dimension-only resources that don't support metrics
            let has_metrics = resource_fields.iter().any(|f| f.is_metric());
            if !has_metrics {
                continue;
            }

            let selectable_with = self.get_resource_selectable_with(&resource_name);
            if selectable_with.is_empty() {
                empty_resources.push(resource_name);
            }
        }

        if empty_resources.is_empty() {
            Ok(())
        } else {
            Err(empty_resources)
        }
    }

    /// Check if a specific resource has populated selectable_with
    pub fn has_selectable_with(&self, resource: &str) -> bool {
        !self.get_resource_selectable_with(resource).is_empty()
    }

    /// Validate field selection against a FROM resource's compatibility list
    pub fn validate_field_selection_for_resource(
        &self,
        field_names: &[String],
        from_resource: &str,
    ) -> ValidationResult {
        // Run existing validation checks
        let mut result = self.validate_field_selection(field_names);

        // Get the FROM resource's RESOURCE-field selectable_with list
        let resource_selectable_with = self.get_resource_selectable_with(from_resource);

        if resource_selectable_with.is_empty() {
            // No RESOURCE field found for this resource - compatibility check skipped
            // This could indicate metadata is not fully enriched
            result
                .warnings
                .push(ValidationWarning::MissingResourceSelectableWith {
                    resource: from_resource.to_string(),
                });
            return result;
        }

        let mut incompatible_fields = Vec::new();

        // Check each field for compatibility
        for field_name in field_names {
            if let Some(field) = self.fields.get(field_name) {
                if field.is_metric() {
                    // For metrics: check if metric field name is in the selectable_with list
                    if !resource_selectable_with.contains(field_name) {
                        incompatible_fields.push(field_name.clone());
                    }
                } else if field.is_segment() {
                    // For segments: check if segment field name is in the selectable_with list
                    if !resource_selectable_with.contains(field_name) {
                        incompatible_fields.push(field_name.clone());
                    }
                } else if field.is_attribute() {
                    // For attributes: check they belong to from_resource or related resources
                    // Simple prefix check: "campaign.name" is compatible with FROM campaign
                    if let Some(resource) = field.get_resource() {
                        let is_compatible = resource == from_resource
                            || resource_selectable_with.iter().any(|r| r == &resource);
                        if !is_compatible {
                            incompatible_fields.push(field_name.clone());
                        }
                    }
                }
            }
        }

        if !incompatible_fields.is_empty() {
            result.errors.push(ValidationError::IncompatibleFields {
                fields: incompatible_fields,
                resource: from_resource.to_string(),
            });
            result.is_valid = false;
        }

        result
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

/// Validation errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationError {
    UnknownFields(Vec<String>),
    NonSelectableFields(Vec<String>),
    IncompatibleFields {
        fields: Vec<String>,
        resource: String,
    },
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
            ValidationError::IncompatibleFields { fields, resource } => {
                write!(
                    f,
                    "Incompatible fields for FROM {}: {}",
                    resource,
                    fields.join(", ")
                )
            }
        }
    }
}

/// Validation warnings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationWarning {
    MetricsWithoutGrouping,
    MissingResourceSelectableWith { resource: String },
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
            ValidationWarning::MissingResourceSelectableWith { resource } => {
                write!(
                    f,
                    "No RESOURCE field found for '{}' - compatibility validation skipped. Metadata may not be fully enriched.",
                    resource
                )
            }
        }
    }
}

/// Result of GAQL generation with validation and trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GAQLResult {
    pub query: String,
    pub validation: ValidationResult,
    pub pipeline_trace: PipelineTrace,
}

/// Pipeline trace for debugging multi-step RAG
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineTrace {
    pub phase1_primary_resource: String,
    pub phase1_related_resources: Vec<String>,
    pub phase1_dropped_resources: Vec<String>,
    pub phase1_reasoning: String,
    pub phase1_model_used: String,
    pub phase1_timing_ms: u64,
    pub phase1_resource_sample: Vec<(String, String)>, // (resource_name, description)
    pub phase2_candidate_count: usize,
    pub phase2_rejected_count: usize,
    pub phase2_timing_ms: u64,
    pub phase25_pre_scan_filters: Vec<(String, Vec<String>)>, // (field_name, detected_enum_values)
    pub phase3_selected_fields: Vec<String>,
    pub phase3_filter_fields: Vec<FilterField>,
    pub phase3_order_by_fields: Vec<(String, String)>, // (field_name, direction)
    pub phase3_reasoning: String,
    pub phase3_model_used: String,
    pub phase3_timing_ms: u64,
    pub phase4_where_clauses: Vec<String>,
    pub phase4_limit: Option<u32>,
    pub phase4_implicit_filters: Vec<String>,
    pub generation_time_ms: u64,
}

/// Filter field specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterField {
    pub field_name: String,
    pub operator: String,
    pub value: String,
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

    #[test]
    fn test_retain_resources() {
        let mut cache = FieldMetadataCache::new();

        // Add fields from different resources
        cache.fields.insert(
            "campaign.id".to_string(),
            FieldMetadata {
                name: "campaign.id".to_string(),
                category: "ATTRIBUTE".to_string(),
                data_type: "INT64".to_string(),
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
            },
        );
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
                selectable_with: vec![],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );
        cache.fields.insert(
            "ad_group.id".to_string(),
            FieldMetadata {
                name: "ad_group.id".to_string(),
                category: "ATTRIBUTE".to_string(),
                data_type: "INT64".to_string(),
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
            },
        );
        cache.fields.insert(
            "customer.id".to_string(),
            FieldMetadata {
                name: "customer.id".to_string(),
                category: "ATTRIBUTE".to_string(),
                data_type: "INT64".to_string(),
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
            },
        );

        // Add resources map
        let mut resources = HashMap::new();
        resources.insert(
            "campaign".to_string(),
            vec!["campaign.id".to_string(), "campaign.name".to_string()],
        );
        resources.insert("ad_group".to_string(), vec!["ad_group.id".to_string()]);
        resources.insert("customer".to_string(), vec!["customer.id".to_string()]);
        cache.resources = Some(resources);

        // Add resource metadata
        let mut resource_metadata = HashMap::new();
        resource_metadata.insert(
            "campaign".to_string(),
            ResourceMetadata {
                name: "campaign".to_string(),
                selectable_with: vec![],
                key_attributes: vec![],
                key_metrics: vec![],
                field_count: 2,
                description: Some("Campaign resource".to_string()),
                uses_fallback: false,
            },
        );
        resource_metadata.insert(
            "ad_group".to_string(),
            ResourceMetadata {
                name: "ad_group".to_string(),
                selectable_with: vec![],
                key_attributes: vec![],
                key_metrics: vec![],
                field_count: 1,
                description: Some("Ad group resource".to_string()),
                uses_fallback: false,
            },
        );
        resource_metadata.insert(
            "customer".to_string(),
            ResourceMetadata {
                name: "customer".to_string(),
                selectable_with: vec![],
                key_attributes: vec![],
                key_metrics: vec![],
                field_count: 1,
                description: Some("Customer resource".to_string()),
                uses_fallback: false,
            },
        );
        cache.resource_metadata = Some(resource_metadata);

        // Retain only campaign and ad_group
        cache.retain_resources(&["campaign".to_string(), "ad_group".to_string()]);

        // Check fields are filtered
        assert_eq!(cache.fields.len(), 3);
        assert!(cache.fields.contains_key("campaign.id"));
        assert!(cache.fields.contains_key("campaign.name"));
        assert!(cache.fields.contains_key("ad_group.id"));
        assert!(!cache.fields.contains_key("customer.id"));

        // Check resources map is filtered
        if let Some(resources) = &cache.resources {
            assert_eq!(resources.len(), 2);
            assert!(resources.contains_key("campaign"));
            assert!(resources.contains_key("ad_group"));
            assert!(!resources.contains_key("customer"));
        }

        // Check resource metadata is filtered
        if let Some(rm) = &cache.resource_metadata {
            assert_eq!(rm.len(), 2);
            assert!(rm.contains_key("campaign"));
            assert!(rm.contains_key("ad_group"));
            assert!(!rm.contains_key("customer"));
        }
    }

    #[test]
    fn test_retain_resources_keeps_resource_category_fields() {
        // This tests the fix for keyword_view (and similar view resources) being
        // filtered out during --test-run mode. RESOURCE-category fields don't have
        // a dot in their name, so get_resource() returns None. We need to explicitly
        // keep them when their name matches a retained resource.
        // Also tests that compatible metrics/segments are retained.
        let mut cache = FieldMetadataCache::new();

        // Add RESOURCE-category field (like "keyword_view") - no dot in name
        cache.fields.insert(
            "keyword_view".to_string(),
            FieldMetadata {
                name: "keyword_view".to_string(),
                category: "RESOURCE".to_string(),
                data_type: "MESSAGE".to_string(),
                selectable: false,
                filterable: false,
                sortable: false,
                metrics_compatible: false,
                resource_name: Some("googleAdsFields/keyword_view".to_string()),
                selectable_with: vec![
                    "metrics.clicks".to_string(),
                    "metrics.impressions".to_string(),
                    "ad_group".to_string(),
                ],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        // Add attribute field for keyword_view (has dot in name)
        cache.fields.insert(
            "keyword_view.resource_name".to_string(),
            FieldMetadata {
                name: "keyword_view.resource_name".to_string(),
                category: "ATTRIBUTE".to_string(),
                data_type: "STRING".to_string(),
                selectable: true,
                filterable: true,
                sortable: false,
                metrics_compatible: true,
                resource_name: None,
                selectable_with: vec![],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        // Add a metric that is compatible with keyword_view
        cache.fields.insert(
            "metrics.clicks".to_string(),
            FieldMetadata {
                name: "metrics.clicks".to_string(),
                category: "METRIC".to_string(),
                data_type: "INT64".to_string(),
                selectable: true,
                filterable: true,
                sortable: true,
                metrics_compatible: false,
                resource_name: None,
                selectable_with: vec!["keyword_view".to_string(), "campaign".to_string()],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        // Add a metric that is NOT compatible with keyword_view
        cache.fields.insert(
            "metrics.hotel_average_lead_value_micros".to_string(),
            FieldMetadata {
                name: "metrics.hotel_average_lead_value_micros".to_string(),
                category: "METRIC".to_string(),
                data_type: "DOUBLE".to_string(),
                selectable: true,
                filterable: true,
                sortable: true,
                metrics_compatible: false,
                resource_name: None,
                selectable_with: vec!["hotel_performance_view".to_string()], // Not keyword_view
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        // Add a segment compatible with keyword_view
        cache.fields.insert(
            "segments.date".to_string(),
            FieldMetadata {
                name: "segments.date".to_string(),
                category: "SEGMENT".to_string(),
                data_type: "DATE".to_string(),
                selectable: true,
                filterable: true,
                sortable: true,
                metrics_compatible: false,
                resource_name: None,
                selectable_with: vec!["keyword_view".to_string(), "campaign".to_string()],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        // Add another resource that will be filtered out
        cache.fields.insert(
            "campaign".to_string(),
            FieldMetadata {
                name: "campaign".to_string(),
                category: "RESOURCE".to_string(),
                data_type: "MESSAGE".to_string(),
                selectable: false,
                filterable: false,
                sortable: false,
                metrics_compatible: false,
                resource_name: Some("googleAdsFields/campaign".to_string()),
                selectable_with: vec!["metrics.clicks".to_string()],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        // Retain only keyword_view
        cache.retain_resources(&["keyword_view".to_string()]);

        // CRITICAL: The RESOURCE-category field "keyword_view" must be retained
        // This is needed for get_resource_selectable_with() to work
        assert!(
            cache.fields.contains_key("keyword_view"),
            "RESOURCE-category field 'keyword_view' should be retained"
        );

        // The attribute field should also be retained
        assert!(
            cache.fields.contains_key("keyword_view.resource_name"),
            "Attribute field 'keyword_view.resource_name' should be retained"
        );

        // Compatible metric should be retained
        assert!(
            cache.fields.contains_key("metrics.clicks"),
            "Compatible metric 'metrics.clicks' should be retained"
        );

        // Incompatible metric should be filtered out
        assert!(
            !cache
                .fields
                .contains_key("metrics.hotel_average_lead_value_micros"),
            "Incompatible metric should be filtered out"
        );

        // Compatible segment should be retained
        assert!(
            cache.fields.contains_key("segments.date"),
            "Compatible segment 'segments.date' should be retained"
        );

        // The campaign RESOURCE field should be filtered out
        assert!(
            !cache.fields.contains_key("campaign"),
            "RESOURCE field 'campaign' should be filtered out"
        );

        // Verify get_resource_selectable_with works after retain
        let selectable_with = cache.get_resource_selectable_with("keyword_view");
        assert!(
            !selectable_with.is_empty(),
            "selectable_with should not be empty after retain_resources"
        );
        assert!(selectable_with.contains(&"metrics.clicks".to_string()));
    }

    #[test]
    fn test_get_resource_selectable_with() {
        let mut cache = FieldMetadataCache::new();

        // Add a RESOURCE field with selectable_with list
        cache.fields.insert(
            "campaign".to_string(),
            FieldMetadata {
                name: "campaign".to_string(),
                category: "RESOURCE".to_string(),
                data_type: "String".to_string(),
                selectable: true,
                filterable: false,
                sortable: false,
                metrics_compatible: true,
                resource_name: None,
                selectable_with: vec![
                    "campaign".to_string(),
                    "metrics.clicks".to_string(),
                    "segments.date".to_string(),
                ],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        let result = cache.get_resource_selectable_with("campaign");
        assert_eq!(result.len(), 3);
        assert!(result.contains(&"metrics.clicks".to_string()));
        assert!(result.contains(&"segments.date".to_string()));
    }

    #[test]
    fn test_get_resource_selectable_with_not_found() {
        let cache = FieldMetadataCache::new();
        let result = cache.get_resource_selectable_with("nonexistent");
        assert!(result.is_empty());
    }

    #[test]
    fn test_validate_field_selection_for_resource_rejects_incompatible_metric() {
        let mut cache = FieldMetadataCache::new();

        // Add campaign RESOURCE field
        cache.fields.insert(
            "campaign".to_string(),
            FieldMetadata {
                name: "campaign".to_string(),
                category: "RESOURCE".to_string(),
                data_type: "String".to_string(),
                selectable: true,
                filterable: false,
                sortable: false,
                metrics_compatible: true,
                resource_name: None,
                selectable_with: vec![
                    "campaign".to_string(),
                    "metrics.clicks".to_string(),
                    // Note: metrics.cost is NOT in the list
                ],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        // Add metrics.clicks
        cache.fields.insert(
            "metrics.clicks".to_string(),
            FieldMetadata {
                name: "metrics.clicks".to_string(),
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
            },
        );

        // Add metrics.cost (incompatible)
        cache.fields.insert(
            "metrics.cost".to_string(),
            FieldMetadata {
                name: "metrics.cost".to_string(),
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
            },
        );

        // Test: metrics.cost should be rejected
        let result = cache.validate_field_selection_for_resource(
            &["campaign".to_string(), "metrics.cost".to_string()],
            "campaign",
        );

        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| matches!(
            e,
            ValidationError::IncompatibleFields { fields, resource } if fields.contains(&"metrics.cost".to_string()) && resource == "campaign"
        )));
    }

    #[test]
    fn test_validate_field_selection_for_resource_accepts_compatible_metric() {
        let mut cache = FieldMetadataCache::new();

        // Add campaign RESOURCE field
        cache.fields.insert(
            "campaign".to_string(),
            FieldMetadata {
                name: "campaign".to_string(),
                category: "RESOURCE".to_string(),
                data_type: "String".to_string(),
                selectable: true,
                filterable: false,
                sortable: false,
                metrics_compatible: true,
                resource_name: None,
                selectable_with: vec!["campaign".to_string(), "metrics.clicks".to_string()],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );

        // Add metrics.clicks (compatible)
        cache.fields.insert(
            "metrics.clicks".to_string(),
            FieldMetadata {
                name: "metrics.clicks".to_string(),
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
            },
        );

        // Test: metrics.clicks should be accepted
        let result = cache.validate_field_selection_for_resource(
            &["campaign".to_string(), "metrics.clicks".to_string()],
            "campaign",
        );

        assert!(result.is_valid);
        assert!(
            !result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::IncompatibleFields { .. }))
        );
    }

    #[test]
    fn test_build_embedding_text_no_structural_flags() {
        let field = FieldMetadata {
            name: "campaign.status".to_string(),
            category: "ATTRIBUTE".to_string(),
            data_type: "ENUM".to_string(),
            selectable: true,
            filterable: true,
            sortable: false,
            metrics_compatible: true,
            resource_name: None,
            selectable_with: vec![],
            enum_values: vec!["ENABLED".to_string(), "PAUSED".to_string()],
            attribute_resources: vec!["campaign".to_string()],
            description: Some("The status of the campaign".to_string()),
            usage_notes: Some("Use with WHERE clause to filter".to_string()),
        };

        let text = field.build_embedding_text();

        // Should NOT contain data_type, selectable, filterable, sortable flags
        assert!(!text.contains("ENUM"));
        assert!(!text.contains("selectable"));
        assert!(!text.contains("filterable"));
        assert!(!text.contains("sortable"));

        // Should contain category tag, description, usage notes, enum values, resource
        assert!(text.contains("[ATTRIBUTE]"));
        assert!(text.contains("The status of the campaign"));
        assert!(text.contains("Use with WHERE clause"));
        assert!(text.contains("Valid values: ENABLED, PAUSED"));
        assert!(text.contains("Resource: campaign"));
    }
}

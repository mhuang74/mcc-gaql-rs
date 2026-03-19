//
// Formatter module for the metadata subcommand.
// Provides human-readable output of enriched field metadata.
//

use anyhow::Result;
use mcc_gaql_common::field_metadata::{FieldMetadata, FieldMetadataCache, ResourceMetadata};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Quality indicator for a field with missing data
const NO_DESCRIPTION: &str = "[no description]";
const NO_USAGE_NOTES: &str = "[no usage_notes]";

/// Quality indicator for a resource using alphabetical fallback
const FALLBACK_INDICATOR: &str = "[fallback: alphabetical]";

/// Quality indicator for LLM-enriched fields in diff mode
const LLM_ENRICHED: &str = "[llm-enriched]";

/// LLM category limit (matches Phase 3 field selection behavior)
const LLM_CATEGORY_LIMIT: usize = 15;

/// Result of a query match
#[derive(Debug)]
pub enum QueryResult {
    /// Single field
    Field(FieldMetadata),
    /// Resource with all its fields by category
    Resource {
        metadata: ResourceMetadata,
        attributes: Vec<FieldMetadata>,
        metrics: Vec<FieldMetadata>,
        segments: Vec<FieldMetadata>,
    },
    /// Pattern-matched fields grouped by category
    Pattern {
        fields: HashMap<String, Vec<FieldMetadata>>, // category -> fields
    },
}

/// Match a query string against the field metadata cache
///
/// Three query types:
/// - Exact field name (e.g., "metrics.clicks") → Return single FieldMetadata
/// - Exact resource name (e.g., "campaign") → Return ResourceMetadata + all fields
/// - Pattern matching (e.g., "metrics.*", "*conversion*") → Return matching fields
pub fn match_query(cache: &FieldMetadataCache, query: &str) -> Result<QueryResult> {
    // Check if query contains a dot - likely a full field name
    let is_full_field_name = query.contains('.');

    // If NOT a full field name, check resource match FIRST
    if !is_full_field_name {
        if cache
            .resource_metadata
            .as_ref()
            .map(|rm| rm.contains_key(query))
            .unwrap_or(false)
        {
            // Get all fields for this resource
            let resource_fields = cache.get_resource_fields(query);
            let mut attributes = Vec::new();
            let mut metrics = Vec::new();
            let mut segments = Vec::new();

            for field in resource_fields {
                match field.category.as_str() {
                    "ATTRIBUTE" | "Attribute" | "attribute" => attributes.push(field.clone()),
                    "METRIC" | "Metric" | "metric" => metrics.push(field.clone()),
                    "SEGMENT" | "Segment" | "segment" => segments.push(field.clone()),
                    _ => {}
                }
            }

            let metadata = cache
                .resource_metadata
                .as_ref()
                .and_then(|rm| rm.get(query))
                .cloned()
                .unwrap_or_else(|| ResourceMetadata {
                    name: query.to_string(),
                    selectable_with: vec![],
                    key_attributes: vec![],
                    key_metrics: vec![],
                    field_count: attributes.len() + metrics.len() + segments.len(),
                    description: None,
                    uses_fallback: false,
                });

            return Ok(QueryResult::Resource {
                metadata,
                attributes,
                metrics,
                segments,
            });
        }
    }

    // If full field name OR no resource match, try field match
    if let Some(field) = cache.get_field(query) {
        return Ok(QueryResult::Field(field.clone()));
    }

    // Try exact resource match again (as fallback)
    if cache
        .resource_metadata
        .as_ref()
        .map(|rm| rm.contains_key(query))
        .unwrap_or(false)
    {
        // Get all fields for this resource
        let resource_fields = cache.get_resource_fields(query);
        let mut attributes = Vec::new();
        let mut metrics = Vec::new();
        let mut segments = Vec::new();

        for field in resource_fields {
            match field.category.as_str() {
                "ATTRIBUTE" | "Attribute" | "attribute" => attributes.push(field.clone()),
                "METRIC" | "Metric" | "metric" => metrics.push(field.clone()),
                "SEGMENT" | "Segment" | "segment" => segments.push(field.clone()),
                _ => {}
            }
        }

        let metadata = cache
            .resource_metadata
            .as_ref()
            .and_then(|rm| rm.get(query))
            .cloned()
            .unwrap_or_else(|| ResourceMetadata {
                name: query.to_string(),
                selectable_with: vec![],
                key_attributes: vec![],
                key_metrics: vec![],
                field_count: attributes.len() + metrics.len() + segments.len(),
                description: None,
                uses_fallback: false,
            });

        return Ok(QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        });
    }

    // Pattern matching
    let fields_by_category = find_fields_by_pattern(cache, query);
    Ok(QueryResult::Pattern {
        fields: fields_by_category,
    })
}

/// Find fields matching a pattern, grouped by category
fn find_fields_by_pattern(
    cache: &FieldMetadataCache,
    pattern: &str,
) -> HashMap<String, Vec<FieldMetadata>> {
    let mut result = HashMap::new();

    // Convert glob pattern to regex-like matching
    let contains = if pattern.contains('*') {
        // Simple wildcard pattern - convert to regex
        let regex_pattern = pattern.replace('*', ".*");
        // Check if pattern starts with ^ or we need it
        let _anchored = if regex_pattern.starts_with(".")
            || regex_pattern.starts_with("metrics")
            || regex_pattern.starts_with("segments")
        {
            format!(".*{}", regex_pattern)
        } else {
            regex_pattern
        };

        Box::new(move |name: &str| {
            // Simple contains matching for wildcards
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 2 && parts[0].is_empty() {
                // Pattern like "*conversion" - ends with
                name.ends_with(parts[1])
            } else if parts.len() == 2 && parts[1].is_empty() {
                // Pattern like "metrics*" - starts with
                name.starts_with(parts[0])
            } else if parts.len() > 1 {
                // More complex pattern - use contains
                parts.iter().all(|p| name.contains(p))
            } else {
                name == pattern
            }
        }) as Box<dyn Fn(&str) -> bool>
    } else {
        Box::new(|name: &str| name.contains(pattern)) as Box<dyn Fn(&str) -> bool>
    };

    for field in cache.fields.values() {
        if contains(&field.name) {
            result
                .entry(field.category.to_uppercase())
                .or_insert_with(Vec::new)
                .push(field.clone());
        }
    }

    // Sort fields within each category by name
    for fields in result.values_mut() {
        fields.sort_by(|a, b| a.name.cmp(&b.name));
    }

    result
}

/// Filter fields by category
pub fn filter_by_category(query_result: QueryResult, category: &str) -> QueryResult {
    match query_result {
        QueryResult::Field(field) => {
            // Check if single field matches the category
            let cat = category.to_uppercase();
            let field_cat = field.category.to_uppercase();
            if field_cat == cat {
                QueryResult::Field(field)
            } else {
                // Return empty pattern result
                QueryResult::Pattern {
                    fields: HashMap::new(),
                }
            }
        }
        QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        } => {
            let cat = category.to_uppercase();
            match cat.as_str() {
                "ATTRIBUTE" | "Attribute" | "attribute" => QueryResult::Resource {
                    metadata,
                    attributes,
                    metrics: vec![],
                    segments: vec![],
                },
                "METRIC" | "Metric" | "metric" => QueryResult::Resource {
                    metadata,
                    attributes: vec![],
                    metrics,
                    segments: vec![],
                },
                "SEGMENT" | "Segment" | "segment" => QueryResult::Resource {
                    metadata,
                    attributes: vec![],
                    metrics: vec![],
                    segments,
                },
                _ => QueryResult::Resource {
                    metadata,
                    attributes,
                    metrics,
                    segments,
                },
            }
        }
        QueryResult::Pattern { mut fields } => {
            let cat = category.to_uppercase();
            fields = fields.into_iter().filter(|(c, _)| c == &cat).collect();
            QueryResult::Pattern { fields }
        }
    }
}

/// Apply subset filter (campaign, ad_group, ad_group_ad, keyword_view only)
pub fn filter_subset(query_result: QueryResult) -> QueryResult {
    const SUBSET_RESOURCES: &[&str] = &["campaign", "ad_group", "ad_group_ad", "keyword_view"];

    match query_result {
        QueryResult::Field(field) => {
            if let Some(resource) = &field.resource_name {
                if SUBSET_RESOURCES.contains(&resource.as_str()) {
                    QueryResult::Field(field)
                } else {
                    QueryResult::Pattern {
                        fields: HashMap::new(),
                    }
                }
            } else {
                // metrics and segments are allowed
                if field.name.starts_with("metrics.") || field.name.starts_with("segments.") {
                    QueryResult::Field(field)
                } else {
                    QueryResult::Pattern {
                        fields: HashMap::new(),
                    }
                }
            }
        }
        QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        } => {
            if SUBSET_RESOURCES.contains(&metadata.name.as_str()) {
                QueryResult::Resource {
                    metadata,
                    attributes,
                    metrics,
                    segments,
                }
            } else {
                QueryResult::Pattern {
                    fields: HashMap::new(),
                }
            }
        }
        QueryResult::Pattern { fields } => {
            let filtered: HashMap<String, Vec<FieldMetadata>> = fields
                .into_iter()
                .map(|(cat, mut field_list)| {
                    field_list.retain(|field| {
                        if let Some(resource) = &field.resource_name {
                            SUBSET_RESOURCES.contains(&resource.as_str())
                        } else {
                            // metrics and segments are allowed
                            field.name.starts_with("metrics.")
                                || field.name.starts_with("segments.")
                        }
                    });
                    (cat, field_list)
                })
                .filter(|(_, list)| !list.is_empty())
                .collect();

            QueryResult::Pattern { fields: filtered }
        }
    }
}

/// Filter fields without descriptions
pub fn filter_no_description(query_result: QueryResult) -> QueryResult {
    match query_result {
        QueryResult::Field(field) => {
            if field.description.is_none()
                || field.description.as_ref().map_or(true, |d| d.is_empty())
            {
                QueryResult::Field(field)
            } else {
                QueryResult::Pattern {
                    fields: HashMap::new(),
                }
            }
        }
        QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        } => {
            let filter_desc = |fields: Vec<FieldMetadata>| -> Vec<FieldMetadata> {
                fields
                    .into_iter()
                    .filter(|f| {
                        f.description.is_none()
                            || f.description.as_ref().map_or(true, |d| d.is_empty())
                    })
                    .collect()
            };

            QueryResult::Resource {
                metadata,
                attributes: filter_desc(attributes),
                metrics: filter_desc(metrics),
                segments: filter_desc(segments),
            }
        }
        QueryResult::Pattern { mut fields } => {
            for field_list in fields.values_mut() {
                field_list.retain(|f| {
                    f.description.is_none() || f.description.as_ref().map_or(true, |d| d.is_empty())
                });
            }
            QueryResult::Pattern { fields }
        }
    }
}

/// Filter fields without usage notes
pub fn filter_no_usage_notes(query_result: QueryResult) -> QueryResult {
    match query_result {
        QueryResult::Field(field) => {
            if field.usage_notes.is_none()
                || field.usage_notes.as_ref().map_or(true, |n| n.is_empty())
            {
                QueryResult::Field(field)
            } else {
                QueryResult::Pattern {
                    fields: HashMap::new(),
                }
            }
        }
        QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        } => {
            let filter_notes = |fields: Vec<FieldMetadata>| -> Vec<FieldMetadata> {
                fields
                    .into_iter()
                    .filter(|f| {
                        f.usage_notes.is_none()
                            || f.usage_notes.as_ref().map_or(true, |n| n.is_empty())
                    })
                    .collect()
            };

            QueryResult::Resource {
                metadata,
                attributes: filter_notes(attributes),
                metrics: filter_notes(metrics),
                segments: filter_notes(segments),
            }
        }
        QueryResult::Pattern { mut fields } => {
            for field_list in fields.values_mut() {
                field_list.retain(|f| {
                    f.usage_notes.is_none() || f.usage_notes.as_ref().map_or(true, |n| n.is_empty())
                });
            }
            QueryResult::Pattern { fields }
        }
    }
}

/// Filter resources using fallback
pub fn filter_fallback_resources(query_result: QueryResult) -> QueryResult {
    match query_result {
        QueryResult::Field(field) => {
            // Not applicable to single fields - return as-is
            QueryResult::Field(field)
        }
        QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        } => {
            if metadata.uses_fallback {
                QueryResult::Resource {
                    metadata,
                    attributes,
                    metrics,
                    segments,
                }
            } else {
                QueryResult::Pattern {
                    fields: HashMap::new(),
                }
            }
        }
        QueryResult::Pattern { fields } => {
            // Pattern queries don't have resource metadata
            // This filter is only meaningful for resource queries
            QueryResult::Pattern { fields }
        }
    }
}

/// Format metadata in LLM style (matches Phase 3 RAG formatting)
///
/// Shows 15 fields per category by default (can be overridden with show_all)
pub fn format_llm(query_result: &QueryResult, show_all: bool) -> String {
    let mut output = String::new();

    match query_result {
        QueryResult::Field(field) => {
            output.push_str(&format!(
                "- {}: {}\n",
                field.name,
                field.description.as_deref().unwrap_or(NO_DESCRIPTION)
            ));
        }
        QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        } => {
            let fallback_tag = if metadata.uses_fallback {
                format!(" {}", FALLBACK_INDICATOR)
            } else {
                String::new()
            };

            output.push_str(&format!(
                "=== RESOURCE: {}{} ===\n",
                metadata.name, fallback_tag
            ));

            if let Some(desc) = &metadata.description {
                output.push_str(&format!("Description: {}\n", desc));
            }

            if !metadata.key_attributes.is_empty() {
                output.push_str(&format!(
                    "Key attributes: {}\n",
                    metadata
                        .key_attributes
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            if !metadata.key_metrics.is_empty() {
                output.push_str(&format!(
                    "Key metrics: {}\n",
                    metadata
                        .key_metrics
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            output.push_str("\n");

            // Apply category limit
            let limit = if show_all {
                usize::MAX
            } else {
                LLM_CATEGORY_LIMIT
            };

            if !attributes.is_empty() {
                output.push_str(&format!(
                    "### ATTRIBUTE ({}/{} showing)\n",
                    limit.min(attributes.len()),
                    attributes.len()
                ));
                for (i, field) in attributes.iter().take(limit).enumerate() {
                    output.push_str(&format_field_llm(field, i));
                }
                output.push('\n');
            }

            if !metrics.is_empty() {
                output.push_str(&format!(
                    "### METRIC ({}/{} showing)\n",
                    limit.min(metrics.len()),
                    metrics.len()
                ));
                for (i, field) in metrics.iter().take(limit).enumerate() {
                    output.push_str(&format_field_llm(field, i));
                }
                output.push('\n');
            }

            if !segments.is_empty() {
                output.push_str(&format!(
                    "### SEGMENT ({}/{} showing)\n",
                    limit.min(segments.len()),
                    segments.len()
                ));
                for (i, field) in segments.iter().take(limit).enumerate() {
                    output.push_str(&format_field_llm(field, i));
                }
            }
        }
        QueryResult::Pattern { fields } => {
            let limit = if show_all {
                usize::MAX
            } else {
                LLM_CATEGORY_LIMIT
            };

            for (category, field_list) in fields {
                if field_list.is_empty() {
                    continue;
                }
                output.push_str(&format!(
                    "### {} ({}/{} showing)\n",
                    category,
                    limit.min(field_list.len()),
                    field_list.len()
                ));
                for (i, field) in field_list.iter().take(limit).enumerate() {
                    output.push_str(&format_field_llm(field, i));
                }
                output.push('\n');
            }
        }
    }

    output
}

/// Format a single field in LLM style
fn format_field_llm(field: &FieldMetadata, _index: usize) -> String {
    let filterable_tag = if field.filterable {
        " [filterable]"
    } else {
        ""
    };
    let sortable_tag = if field.sortable { " [sortable]" } else { "" };

    let mut parts = vec![
        format!("- {}", field.name),
        filterable_tag.to_string(),
        sortable_tag.to_string(),
    ];

    let desc = field.description.as_deref().unwrap_or(NO_DESCRIPTION);

    // Split description into sentences for better line wrapping
    let desc_lines: Vec<&str> = desc.split(". ").collect();

    parts.push(format!(": {}", desc_lines[0]));

    let result = parts.concat();

    // Add remaining sentences on new lines
    for sentence in desc_lines.iter().skip(1) {
        if !sentence.is_empty() {
            return format!("{}{}\n", result, sentence);
        }
    }

    format!("{}\n", result.trim_end_matches("\n"))
}

/// Format metadata in full style (shows all fields with complete metadata)
pub fn format_full(query_result: &QueryResult) -> String {
    let mut output = String::new();

    match query_result {
        QueryResult::Field(field) => {
            output.push_str(&format_field_full(field));
        }
        QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        } => {
            let fallback_tag = if metadata.uses_fallback {
                format!(" {}", FALLBACK_INDICATOR)
            } else {
                String::new()
            };

            output.push_str(&format!(
                "=== RESOURCE: {}{} ===\n",
                metadata.name, fallback_tag
            ));

            if let Some(desc) = &metadata.description {
                output.push_str(&format!("Description: {}\n", desc));
            }

            output.push_str(&format!("Fields: {} total", metadata.field_count));

            if !metadata.key_attributes.is_empty() {
                output.push_str(&format!(
                    "\nKey attributes: {}",
                    metadata.key_attributes.join(", ")
                ));
            }

            if !metadata.key_metrics.is_empty() {
                output.push_str(&format!(
                    "\nKey metrics: {}",
                    metadata.key_metrics.join(", ")
                ));
            }

            output.push_str(&format!("\nUses fallback: {}\n\n", metadata.uses_fallback));

            if !attributes.is_empty() {
                output.push_str(&format!("### ATTRIBUTE ({})\n", attributes.len()));
                for field in attributes.iter() {
                    output.push_str(&format_field_full(field));
                }
                output.push('\n');
            }

            if !metrics.is_empty() {
                output.push_str(&format!("### METRIC ({})\n", metrics.len()));
                for field in metrics.iter() {
                    output.push_str(&format_field_full(field));
                }
                output.push('\n');
            }

            if !segments.is_empty() {
                output.push_str(&format!("### SEGMENT ({})\n", segments.len()));
                for field in segments.iter() {
                    output.push_str(&format_field_full(field));
                }
            }
        }
        QueryResult::Pattern { fields } => {
            let total: usize = fields.values().map(|v| v.len()).sum();
            output.push_str(&format!("### PATTERN MATCH: {} fields total\n\n", total));

            for (category, field_list) in fields {
                if field_list.is_empty() {
                    continue;
                }
                output.push_str(&format!("### {} ({})\n", category, field_list.len()));
                for field in field_list.iter() {
                    output.push_str(&format_field_full(field));
                }
                output.push('\n');
            }
        }
    }

    output
}

/// Format a single field with all metadata
fn format_field_full(field: &FieldMetadata) -> String {
    let desc_indicator = if field.description.is_none()
        || field.description.as_ref().map_or(true, |d| d.is_empty())
    {
        format!(" {}", NO_DESCRIPTION)
    } else {
        String::new()
    };

    let notes_indicator = if field.usage_notes.is_none()
        || field.usage_notes.as_ref().map_or(true, |n| n.is_empty())
    {
        format!(" {}", NO_USAGE_NOTES)
    } else {
        String::new()
    };

    let mut output = format!("- {}{}{}\n", field.name, desc_indicator, notes_indicator);

    output.push_str(&format!("  Category: {}\n", field.category));
    output.push_str(&format!("  DataType: {}\n", field.data_type));

    if field.selectable {
        output.push_str("  Selectable: yes\n");
    }
    if field.filterable {
        output.push_str("  Filterable: yes\n");
    }
    if field.sortable {
        output.push_str("  Sortable: yes\n");
    }
    if field.metrics_compatible {
        output.push_str("  Metrics compatible: yes\n");
    }

    if !field.selectable_with.is_empty() {
        output.push_str(&format!(
            "  Selectable with: {}\n",
            field
                .selectable_with
                .iter()
                .take(5)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if !field.enum_values.is_empty() {
        let values: Vec<&str> = field
            .enum_values
            .iter()
            .take(10)
            .map(String::as_str)
            .collect();
        output.push_str(&format!("  Enum values: {}\n", values.join(", ")));
    }

    if let Some(item) = &field.resource_name {
        output.push_str(&format!("  Resource: {}\n", item));
    }

    if let Some(desc) = &field.description {
        if !desc.is_empty() {
            output.push_str(&format!("  Description: {}\n", desc));
        }
    }

    if let Some(notes) = &field.usage_notes {
        if !notes.is_empty() {
            output.push_str(&format!("  Usage notes: {}\n", notes));
        }
    }

    output
}

/// Format metadata as JSON
pub fn format_json(query_result: &QueryResult) -> Result<String> {
    let json = serde_json::to_string_pretty(query_result)?;
    Ok(json)
}

/// Format diff mode - compare enriched vs non-enriched caches
///
/// Shows enrichment statistics and marks LLM-enriched fields with [llm-enriched]
pub fn format_diff_llm(
    enriched: &FieldMetadataCache,
    non_enriched: &FieldMetadataCache,
    query: &str,
    show_all: bool,
) -> Result<String> {
    let mut output = String::new();

    // Calculate enrichment statistics
    let enriched_count = enriched
        .fields
        .values()
        .filter(|f| f.description.is_some() && !f.description.as_ref().unwrap().is_empty())
        .count();

    let total_fields = enriched.fields.len();
    let enrichment_rate = if total_fields > 0 {
        (enriched_count as f64 / total_fields as f64) * 100.0
    } else {
        0.0
    };

    output.push_str("=== ENRICHMENT COMPARISON ===\n");
    output.push_str(&format!("Total fields: {}\n", total_fields));
    output.push_str(&format!(
        "Enriched: {} ({:.1}%)\n",
        enriched_count, enrichment_rate
    ));

    // Get non-enriched fields
    let non_enriched_count = total_fields - enriched_count;
    output.push_str(&format!(
        "Without description: {} ({:.1}%)\n",
        non_enriched_count,
        100.0 - enrichment_rate
    ));

    output.push_str("\n");

    // Get query result from enriched cache
    let query_result = match_query(enriched, query)?;
    let mut result_with_markers = String::new();

    // Add [llm-enriched] markers to fields that were enriched
    match &query_result {
        QueryResult::Field(field) => {
            result_with_markers.push_str(&format_field_with_llm_marker(field, non_enriched));
        }
        QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        } => {
            let fallback_tag = if metadata.uses_fallback {
                format!(" {}", FALLBACK_INDICATOR)
            } else {
                String::new()
            };

            result_with_markers.push_str(&format!(
                "=== RESOURCE: {}{} ===\n",
                metadata.name, fallback_tag
            ));

            if let Some(desc) = &metadata.description {
                result_with_markers.push_str(&format!("Description: {}\n", desc));
            }

            if !metadata.key_attributes.is_empty() {
                result_with_markers.push_str(&format!(
                    "Key attributes: {}\n",
                    metadata
                        .key_attributes
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            if !metadata.key_metrics.is_empty() {
                result_with_markers.push_str(&format!(
                    "Key metrics: {}\n",
                    metadata
                        .key_metrics
                        .iter()
                        .take(5)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }

            result_with_markers.push_str("\n");

            let limit = if show_all {
                usize::MAX
            } else {
                LLM_CATEGORY_LIMIT
            };

            if !attributes.is_empty() {
                result_with_markers.push_str(&format!(
                    "### ATTRIBUTE ({}/{} showing)\n",
                    limit.min(attributes.len()),
                    attributes.len()
                ));
                for field in attributes.iter().take(limit) {
                    result_with_markers
                        .push_str(&format_field_with_llm_marker(field, non_enriched));
                }
                result_with_markers.push('\n');
            }

            if !metrics.is_empty() {
                result_with_markers.push_str(&format!(
                    "### METRIC ({}/{} showing)\n",
                    limit.min(metrics.len()),
                    metrics.len()
                ));
                for field in metrics.iter().take(limit) {
                    result_with_markers
                        .push_str(&format_field_with_llm_marker(field, non_enriched));
                }
                result_with_markers.push('\n');
            }

            if !segments.is_empty() {
                result_with_markers.push_str(&format!(
                    "### SEGMENT ({}/{} showing)\n",
                    limit.min(segments.len()),
                    segments.len()
                ));
                for field in segments.iter().take(limit) {
                    result_with_markers
                        .push_str(&format_field_with_llm_marker(field, non_enriched));
                }
            }
        }
        QueryResult::Pattern { fields } => {
            let limit = if show_all {
                usize::MAX
            } else {
                LLM_CATEGORY_LIMIT
            };

            for (category, field_list) in fields {
                if field_list.is_empty() {
                    continue;
                }
                result_with_markers.push_str(&format!(
                    "### {} ({}/{} showing)\n",
                    category,
                    limit.min(field_list.len()),
                    field_list.len()
                ));
                for field in field_list.iter().take(limit) {
                    result_with_markers
                        .push_str(&format_field_with_llm_marker(field, non_enriched));
                }
                result_with_markers.push('\n');
            }
        }
    }

    output.push_str(&result_with_markers);

    Ok(output)
}

/// Format a field with LLM-enriched marker
fn format_field_with_llm_marker(
    field: &FieldMetadata,
    non_enriched: &FieldMetadataCache,
) -> String {
    // Check if this field was enriched (has description in enriched, not in non-enriched)
    let non_enriched_field = non_enriched.get_field(&field.name);
    let was_enriched = non_enriched_field
        .map(|ne| {
            let has_desc = ne.description.is_some() && !ne.description.as_ref().unwrap().is_empty();
            let enriched_has_desc =
                field.description.is_some() && !field.description.as_ref().unwrap().is_empty();
            !has_desc && enriched_has_desc
        })
        .unwrap_or(false);

    let llm_marker = if was_enriched {
        format!(" {}", LLM_ENRICHED)
    } else {
        String::new()
    };

    let filterable_tag = if field.filterable {
        " [filterable]"
    } else {
        ""
    };
    let sortable_tag = if field.sortable { " [sortable]" } else { "" };

    let desc = field.description.as_deref().unwrap_or(NO_DESCRIPTION);

    let mut output = format!(
        "- {}{}{}{}: {}\n",
        field.name, llm_marker, filterable_tag, sortable_tag, desc
    );

    // Add before/after if enriched
    if was_enriched {
        if let Some(ne_field) = non_enriched_field {
            output.push_str(&format!(
                "  Before: {}\n",
                ne_field.description.as_deref().unwrap_or("")
            ));
            output.push_str(&format!("  After: {}\n", desc));
        }
    }

    output
}

/// Query result for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QueryResultJson {
    Field(FieldMetadata),
    Resource {
        metadata: ResourceMetadata,
        attributes: Vec<FieldMetadata>,
        metrics: Vec<FieldMetadata>,
        segments: Vec<FieldMetadata>,
    },
    Pattern {
        fields: HashMap<String, Vec<FieldMetadata>>,
    },
}

impl From<&QueryResult> for QueryResultJson {
    fn from(result: &QueryResult) -> Self {
        match result {
            QueryResult::Field(field) => QueryResultJson::Field(field.clone()),
            QueryResult::Resource {
                metadata,
                attributes,
                metrics,
                segments,
            } => QueryResultJson::Resource {
                metadata: metadata.clone(),
                attributes: attributes.clone(),
                metrics: metrics.clone(),
                segments: segments.clone(),
            },
            QueryResult::Pattern { fields } => QueryResultJson::Pattern {
                fields: fields.clone(),
            },
        }
    }
}

// Implement Serialize/Deserialize for QueryResult for JSON output
impl serde::Serialize for QueryResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let json_result: QueryResultJson = self.into();
        json_result.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for QueryResult {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::de::Deserializer<'de>,
    {
        let json_result = QueryResultJson::deserialize(deserializer)?;
        Ok(match json_result {
            QueryResultJson::Field(field) => QueryResult::Field(field),
            QueryResultJson::Resource {
                metadata,
                attributes,
                metrics,
                segments,
            } => QueryResult::Resource {
                metadata,
                attributes,
                metrics,
                segments,
            },
            QueryResultJson::Pattern { fields } => QueryResult::Pattern { fields },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quality_indicators() {
        assert_eq!(NO_DESCRIPTION, "[no description]");
        assert_eq!(NO_USAGE_NOTES, "[no usage_notes]");
        assert_eq!(FALLBACK_INDICATOR, "[fallback: alphabetical]");
    }

    #[test]
    fn test_llm_category_limit() {
        assert_eq!(LLM_CATEGORY_LIMIT, 15);
    }
}

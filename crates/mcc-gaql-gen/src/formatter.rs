//
// Formatter module for the metadata subcommand.
// Provides human-readable output of enriched field metadata.
//

use anyhow::Result;
use mcc_gaql_common::field_metadata::{FieldMetadata, FieldMetadataCache, ResourceMetadata};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::vector_store;
use lancedb::DistanceType;
use rig::vector_store::{VectorSearchRequest, VectorStoreIndex};
use rig_fastembed::FastembedModel;
use rig_lancedb::{LanceDbVectorIndex, SearchParams};

/// Quality indicator for a field with missing data
const NO_DESCRIPTION: &str = "[no description]";
const NO_USAGE_NOTES: &str = "[no usage_notes]";

/// Quality indicator for a resource using alphabetical fallback
const FALLBACK_INDICATOR: &str = "[fallback: alphabetical]";

/// Quality indicator for LLM-enriched fields in diff mode
const LLM_ENRICHED: &str = "[llm-enriched]";

/// LLM category limit (matches Phase 3 field selection behavior)
const LLM_CATEGORY_LIMIT: usize = 15;

/// Categorize selectable_with fields into segments, metrics, and other
fn categorize_selectable_with(selectable_with: &[String]) -> (Vec<&str>, Vec<&str>, Vec<&str>) {
    let mut segments = Vec::new();
    let mut metrics = Vec::new();
    let mut other = Vec::new();

    for field in selectable_with {
        if field.starts_with("segments.") {
            segments.push(field.as_str());
        } else if field.starts_with("metrics.") {
            metrics.push(field.as_str());
        } else {
            other.push(field.as_str());
        }
    }

    segments.sort();
    metrics.sort();
    other.sort();

    (segments, metrics, other)
}

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
    /// Semantic search results with similarity scores
    Semantic {
        fields: HashMap<String, Vec<(FieldMetadata, f64)>>, // category -> (field, score)
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
    if !is_full_field_name
        && cache
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
                identity_fields: vec![],
            });

        return Ok(QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        });
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
                identity_fields: vec![],
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

/// Match query using semantic search against field metadata vector store.
/// Returns fields grouped by category with similarity scores.
pub async fn match_query_semantic(
    cache: &FieldMetadataCache,
    query: &str,
    show_all: bool,
) -> Result<QueryResult> {
    // Check if query contains a dot - likely a full field name
    let is_full_field_name = query.contains('.');

    // If NOT a full field name, check resource match FIRST (same as pattern matching)
    if !is_full_field_name
        && cache
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
            .and_then(|rm| rm.get(query).cloned())
            .unwrap_or_else(|| ResourceMetadata {
                name: query.to_string(),
                selectable_with: vec![],
                key_attributes: vec![],
                key_metrics: vec![],
                field_count: 0,
                description: None,
                uses_fallback: false,
                identity_fields: vec![],
            });

        return Ok(QueryResult::Resource {
            metadata,
            attributes,
            metrics,
            segments,
        });
    }

    // Check for exact field name match
    if let Some(field) = cache.get_field(query) {
        return Ok(QueryResult::Field(field.clone()));
    }

    // Perform semantic search
    let fields_with_scores = search_fields_semantic(query, show_all).await?;

    // Group by category with scores
    let mut fields_by_category: HashMap<String, Vec<(FieldMetadata, f64)>> = HashMap::new();
    for (field, score) in fields_with_scores {
        fields_by_category
            .entry(field.category.to_uppercase())
            .or_default()
            .push((field, score));
    }

    // Sort each category by score DESC (highest similarity first)
    for field_list in fields_by_category.values_mut() {
        field_list.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    }

    Ok(QueryResult::Semantic {
        fields: fields_by_category,
    })
}

/// Search field metadata using vector similarity
async fn search_fields_semantic(
    query: &str,
    show_all: bool,
) -> Result<Vec<(FieldMetadata, f64)>> {
    // Create embedding client
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get cache directory"))?
        .join("mcc-gaql")
        .join("fastembed-models");

    std::fs::create_dir_all(&cache_dir)?;

    // SAFETY: This is safe because we're only setting a known environment variable
    // and the process is single-threaded at this point.
    unsafe { std::env::set_var("HF_HOME", &cache_dir) };

    let fastembed_client = rig_fastembed::Client::new();
    let embedding_model = fastembed_client.embedding_model(&FastembedModel::BGESmallENV15);

    // Connect to LanceDB
    let db = vector_store::get_lancedb_connection().await?;

    // Open field_metadata table
    let table = vector_store::open_table(&db, "field_metadata").await?;

    // Create vector index
    let index = LanceDbVectorIndex::new(
        table,
        embedding_model,
        "id",
        SearchParams::default().distance_type(DistanceType::Cosine),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

    // Determine search limit based on show_all flag
    // Default: 15 per category × 3 categories = 45 total
    // show_all: fetch more to ensure we get diverse results
    let search_limit = if show_all { 100 } else { 45 };

    // Build search request
    let search_request = VectorSearchRequest::builder()
        .query(query)
        .samples(search_limit as u64)
        .build()
        .map_err(|e| anyhow::anyhow!("Failed to build search request: {}", e))?;

    // Execute search
    #[derive(serde::Deserialize)]
    struct FieldSearchResult {
        id: String,
        description: String,
        category: String,
        data_type: String,
        selectable: bool,
        filterable: bool,
        sortable: bool,
        metrics_compatible: bool,
        resource_name: Option<String>,
    }

    let raw_results = index
        .top_n::<FieldSearchResult>(search_request)
        .await
        .map_err(|e| anyhow::anyhow!("Vector search failed: {}", e))?;

    log::info!(
        "Semantic search for '{}' found {} results (top score={:.3})",
        query,
        raw_results.len(),
        raw_results.first().map(|(s, _, _)| s).unwrap_or(&0.0)
    );

    // Convert to FieldMetadata with scores
    let results: Vec<(FieldMetadata, f64)> = raw_results
        .into_iter()
        .map(|(score, _id, doc)| {
            // Create FieldMetadata from search result
            let field = FieldMetadata {
                name: doc.id.clone(),
                category: doc.category,
                data_type: doc.data_type,
                selectable: doc.selectable,
                filterable: doc.filterable,
                sortable: doc.sortable,
                metrics_compatible: doc.metrics_compatible,
                resource_name: doc.resource_name,
                selectable_with: vec![], // Not stored in vector index
                enum_values: vec![],     // Not stored in vector index
                attribute_resources: vec![], // Not stored in vector index
                description: Some(doc.description),
                usage_notes: None, // Not stored in vector index
            };
            (field, score)
        })
        .collect();

    Ok(results)
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
            fields.retain(|c, _| c == &cat);
            QueryResult::Pattern { fields }
        }
        QueryResult::Semantic { mut fields } => {
            let cat = category.to_uppercase();
            fields.retain(|c, _| c == &cat);
            QueryResult::Semantic { fields }
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
        QueryResult::Semantic { fields } => {
            let filtered: HashMap<String, Vec<(FieldMetadata, f64)>> = fields
                .into_iter()
                .map(|(cat, mut field_list)| {
                    field_list.retain(|(field, _)| {
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

            QueryResult::Semantic { fields: filtered }
        }
    }
}

/// Filter fields without descriptions
pub fn filter_no_description(query_result: QueryResult) -> QueryResult {
    match query_result {
        QueryResult::Field(field) => {
            if field.description.is_none()
                || field.description.as_ref().is_none_or(|d| d.is_empty())
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
                            || f.description.as_ref().is_none_or(|d| d.is_empty())
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
                    f.description.is_none() || f.description.as_ref().is_none_or(|d| d.is_empty())
                });
            }
            QueryResult::Pattern { fields }
        }
        QueryResult::Semantic { mut fields } => {
            for field_list in fields.values_mut() {
                field_list.retain(|(f, _)| {
                    f.description.is_none() || f.description.as_ref().is_none_or(|d| d.is_empty())
                });
            }
            QueryResult::Semantic { fields }
        }
    }
}

/// Filter fields without usage notes
pub fn filter_no_usage_notes(query_result: QueryResult) -> QueryResult {
    match query_result {
        QueryResult::Field(field) => {
            if field.usage_notes.is_none()
                || field.usage_notes.as_ref().is_none_or(|n| n.is_empty())
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
                            || f.usage_notes.as_ref().is_none_or(|n| n.is_empty())
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
                    f.usage_notes.is_none() || f.usage_notes.as_ref().is_none_or(|n| n.is_empty())
                });
            }
            QueryResult::Pattern { fields }
        }
        QueryResult::Semantic { mut fields } => {
            for field_list in fields.values_mut() {
                field_list.retain(|(f, _)| {
                    f.usage_notes.is_none() || f.usage_notes.as_ref().is_none_or(|n| n.is_empty())
                });
            }
            QueryResult::Semantic { fields }
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
        QueryResult::Semantic { fields } => {
            // Semantic queries don't have resource metadata
            // This filter is only meaningful for resource queries
            QueryResult::Semantic { fields }
        }
    }
}

/// Format metadata in LLM style (matches Phase 3 RAG formatting)
///
/// Shows 15 fields per category by default (can be overridden with show_all)
pub fn format_llm(
    query_result: &QueryResult,
    show_all: bool,
    cache: &FieldMetadataCache,
) -> String {
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

            if !metadata.identity_fields.is_empty() {
                output.push_str(&format!(
                    "Identity fields: {}\n",
                    metadata.identity_fields.join(", ")
                ));
            }

            // Show selectable_with counts
            let (selectable_segments, selectable_metrics, selectable_other) =
                categorize_selectable_with(&metadata.selectable_with);

            if !selectable_segments.is_empty() {
                output.push_str(&format!(
                    "Selectable segments ({}): {}\n",
                    selectable_segments.len(),
                    selectable_segments
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if selectable_segments.len() > 10 {
                    output.push_str(&format!(
                        "  ... and {} more\n",
                        selectable_segments.len() - 10
                    ));
                }
            }

            if !selectable_metrics.is_empty() {
                output.push_str(&format!(
                    "Selectable metrics ({}): {}\n",
                    selectable_metrics.len(),
                    selectable_metrics
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if selectable_metrics.len() > 10 {
                    output.push_str(&format!(
                        "  ... and {} more\n",
                        selectable_metrics.len() - 10
                    ));
                }
            }

            if !selectable_other.is_empty() {
                output.push_str(&format!(
                    "Selectable other ({}): {}\n",
                    selectable_other.len(),
                    selectable_other
                        .iter()
                        .take(10)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
                if selectable_other.len() > 10 {
                    output.push_str(&format!("  ... and {} more\n", selectable_other.len() - 10));
                }
            }

            output.push('\n');

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
                    output.push_str(&format_field_llm(field, i, None, None));
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
                    output.push_str(&format_field_llm(field, i, None, None));
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
                    output.push_str(&format_field_llm(field, i, None, None));
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
                    // For RESOURCE-category fields, look up description from resource_metadata
                    let resource_desc = if category == "RESOURCE" {
                        cache
                            .resource_metadata
                            .as_ref()
                            .and_then(|rm| rm.get(&field.name))
                            .and_then(|rm| rm.description.as_deref())
                    } else {
                        None
                    };
                    output.push_str(&format_field_llm(field, i, resource_desc, None));
                }
                output.push('\n');
            }
        }
        QueryResult::Semantic { fields } => {
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
                for (i, (field, score)) in field_list.iter().take(limit).enumerate() {
                    // For RESOURCE-category fields, look up description from resource_metadata
                    let resource_desc = if category == "RESOURCE" {
                        cache
                            .resource_metadata
                            .as_ref()
                            .and_then(|rm| rm.get(&field.name))
                            .and_then(|rm| rm.description.as_deref())
                    } else {
                        None
                    };
                    output.push_str(&format_field_llm(field, i, resource_desc, Some(*score)));
                }
                output.push('\n');
            }
        }
    }

    output
}

/// Format a single field in LLM style
/// If `resource_desc` is provided, use it instead of `field.description` (for RESOURCE-category fields)
/// If `score` is provided, display similarity score
fn format_field_llm(
    field: &FieldMetadata,
    _index: usize,
    resource_desc: Option<&str>,
    score: Option<f64>,
) -> String {
    let score_tag = score.map(|s| format!(" [{:.3}]", s)).unwrap_or_default();
    let filterable_tag = if field.filterable {
        " [filterable]"
    } else {
        ""
    };
    let sortable_tag = if field.sortable { " [sortable]" } else { "" };

    let mut parts = vec![
        format!("- {}", field.name),
        score_tag,
        filterable_tag.to_string(),
        sortable_tag.to_string(),
    ];

    // Use resource description if provided, otherwise fall back to field description
    let desc = resource_desc
        .or(field.description.as_deref())
        .unwrap_or(NO_DESCRIPTION);

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

            if !metadata.identity_fields.is_empty() {
                output.push_str(&format!(
                    "\nIdentity fields: {}",
                    metadata.identity_fields.join(", ")
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

            // Show selectable_with fields categorized
            let (selectable_segments, selectable_metrics, selectable_other) =
                categorize_selectable_with(&metadata.selectable_with);

            if !selectable_segments.is_empty()
                || !selectable_metrics.is_empty()
                || !selectable_other.is_empty()
            {
                output.push_str("\n--- SELECTABLE WITH (auto-joined fields) ---\n\n");

                if !selectable_segments.is_empty() {
                    output.push_str(&format!(
                        "### SELECTABLE SEGMENTS ({})\n",
                        selectable_segments.len()
                    ));
                    for seg in &selectable_segments {
                        output.push_str(&format!("  - {}\n", seg));
                    }
                    output.push('\n');
                }

                if !selectable_metrics.is_empty() {
                    output.push_str(&format!(
                        "### SELECTABLE METRICS ({})\n",
                        selectable_metrics.len()
                    ));
                    for metric in &selectable_metrics {
                        output.push_str(&format!("  - {}\n", metric));
                    }
                    output.push('\n');
                }

                if !selectable_other.is_empty() {
                    output.push_str(&format!(
                        "### SELECTABLE OTHER ({})\n",
                        selectable_other.len()
                    ));
                    for field in &selectable_other {
                        output.push_str(&format!("  - {}\n", field));
                    }
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
        QueryResult::Semantic { fields } => {
            let total: usize = fields.values().map(|v| v.len()).sum();
            output.push_str(&format!("### SEMANTIC SEARCH: {} fields total\n\n", total));

            for (category, field_list) in fields {
                if field_list.is_empty() {
                    continue;
                }
                output.push_str(&format!("### {} ({})\n", category, field_list.len()));
                for (field, score) in field_list.iter() {
                    output.push_str(&format!("[{:.3}] ", score));
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
    let desc_indicator =
        if field.description.is_none() || field.description.as_ref().is_none_or(|d| d.is_empty()) {
            format!(" {}", NO_DESCRIPTION)
        } else {
            String::new()
        };

    let notes_indicator =
        if field.usage_notes.is_none() || field.usage_notes.as_ref().is_none_or(|n| n.is_empty()) {
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

    if let Some(desc) = &field.description
        && !desc.is_empty()
    {
        output.push_str(&format!("  Description: {}\n", desc));
    }

    if let Some(notes) = &field.usage_notes
        && !notes.is_empty()
    {
        output.push_str(&format!("  Usage notes: {}\n", notes));
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

    output.push('\n');

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

            if !metadata.identity_fields.is_empty() {
                result_with_markers.push_str(&format!(
                    "Identity fields: {}\n",
                    metadata.identity_fields.join(", ")
                ));
            }

            result_with_markers.push('\n');

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
        QueryResult::Semantic { fields } => {
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
                for (field, _score) in field_list.iter().take(limit) {
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
    if was_enriched && let Some(ne_field) = non_enriched_field {
        output.push_str(&format!(
            "  Before: {}\n",
            ne_field.description.as_deref().unwrap_or("")
        ));
        output.push_str(&format!("  After: {}\n", desc));
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
    Semantic {
        fields: HashMap<String, Vec<FieldWithScore>>,
    },
}

/// Field with similarity score for JSON serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldWithScore {
    #[serde(flatten)]
    pub field: FieldMetadata,
    pub score: f64,
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
            QueryResult::Semantic { fields } => QueryResultJson::Semantic {
                fields: fields
                    .iter()
                    .map(|(cat, field_list)| {
                        (
                            cat.clone(),
                            field_list
                                .iter()
                                .map(|(field, score)| FieldWithScore {
                                    field: field.clone(),
                                    score: *score,
                                })
                                .collect(),
                        )
                    })
                    .collect(),
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
            QueryResultJson::Semantic { fields } => QueryResult::Semantic {
                fields: fields
                    .into_iter()
                    .map(|(cat, field_list)| {
                        (
                            cat,
                            field_list
                                .into_iter()
                                .map(|fws| (fws.field, fws.score))
                                .collect(),
                        )
                    })
                    .collect(),
            },
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

# Implementation Plan: `mcc-gaql-gen metadata` Subcommand

## Overview

This plan details the implementation of a comprehensive CLI command to inspect enriched Google Ads field metadata, providing users with transparency into the RAG-based GAQL generation pipeline.

## Architecture Overview

The implementation follows three key principles:

1. **Separation of Concerns**: CLI argument parsing → query logic → formatting logic
2. **Reusability**: Shared formatting functions between main command and LLM context generation
3. **Extensibility**: Easy to add new formats, filters, or output modes

**Key Components:**
- CLI command definition in `main.rs`
- Query matching and filtering logic
- Formatter module for all output variants
- Helper utilities for quality indicators and diff comparisons

## Data Flow

```
User Input (CLI args)
    ↓
cmd_metadata() Handler
    ↓
Query Matching (exact/pattern)
    ↓
Apply Filters (--category, --subset, custom filters)
    ↓
Format Selection (llm/full/json/diff)
    ↓
Formatter Functions
    ↓
Output to stdout
```

## Implementation Structure

### 1. Files to Modify/Create

**Primary files:**
1. **`crates/mcc-gaql-gen/src/main.rs`** - Add CLI command and handler
2. **`crates/mcc-gaql-gen/src/formatter.rs`** (NEW) - All formatting logic
3. **`crates/mcc-gaql-gen/src/lib.rs`** - Export formatter module
4. **`crates/mcc-gaql-common/src/field_metadata.rs`** - No changes needed (existing methods sufficient)
5. **`crates/mcc-gaql-common/src/paths.rs`** - No changes needed (existing paths sufficient)

**Helper functions to add:**
- Pattern matching utilities
- Quality indicator markers
- Diff comparison logic

### 2. Command Definition (main.rs)

Add to the `Commands` enum in `main.rs`:

```rust
/// Display enriched field metadata for debugging RAG pipeline
Metadata {
    /// Resource name, field name, or pattern
    query: String,

    /// Path to enriched metadata JSON [default: cache path]
    #[arg(long)]
    metadata: Option<PathBuf>,

    /// Output format: llm, full, json [default: llm]
    #[arg(long, default_value = "llm")]
    format: String,

    /// Filter by category: resource, attribute, metric, segment
    #[arg(long)]
    category: Option<String>,

    /// Use subset resources only (campaign, ad_group, ad_group_ad, keyword_view)
    #[arg(long)]
    subset: bool,

    /// Show all fields (default shows LLM-limited view with 15 per category)
    #[arg(long)]
    show_all: bool,

    /// Show enrichment comparison (enriched vs non-enriched)
    #[arg(long)]
    diff: bool,

    /// Filter fields: no-description, no-usage-notes, fallback (resources only)
    #[arg(long)]
    filter: Option<String>,
},
```

Add to the main match statement:

```rust
Commands::Metadata {
    query,
    metadata,
    format,
    category,
    subset,
    show_all,
    diff,
    filter,
} => {
    cmd_metadata(
        query,
        metadata,
        format,
        category,
        subset,
        show_all,
        diff,
        filter,
    )
    .await?;
}
```

### 3. Handler Function (main.rs)

```rust
async fn cmd_metadata(
    query: String,
    metadata_path: Option<PathBuf>,
    format: String,
    category_filter: Option<String>,
    subset: bool,
    show_all: bool,
    diff_mode: bool,
    custom_filter: Option<String>,
) -> Result<()> {
    // Step 1: Load enriched metadata
    let cache_path = metadata_path
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata path. Use --metadata to specify it.")?;
    
    println!("Loading enriched metadata from {:?}...", cache_path);
    let mut cache = FieldMetadataCache::load_from_disk(&cache_path)
        .await
        .context("Failed to load enriched metadata. Run 'mcc-gaql-gen enrich' first.")?;

    // Step 2: Optionally load non-enriched cache for diff mode
    let base_cache = if diff_mode {
        let base_path = mcc_gaql_common::paths::field_metadata_cache_path()?;
        Some(FieldMetadataCache::load_from_disk(&base_path).await?)
    } else {
        None
    };

    // Step 3: Apply subset filter (TEST_RUN_RESOURCES)
    if subset {
        let test_resources = filter_test_resources(cache.get_resources());
        cache.retain_resources(&test_resources);
        println!(
            "Subset mode: limited to {} resources (campaign, ad_group, ad_group_ad, keyword_view)",
            test_resources.len()
        );
    }

    // Step 4: Validate and normalize format option
    let normalized_format = match format.to_lowercase().as_str() {
        "llm" => OutputFormat::Llm,
        "full" => OutputFormat::Full,
        "json" => OutputFormat::Json,
        _ => return Err(anyhow!("Invalid format '{}'. Use: llm, full, json", format)),
    };

    // Step 5: Parse custom filter
    let filter_option = match custom_filter.as_deref() {
        Some("no-description") => FilterOption::NoDescription,
        Some("no-usage-notes") => FilterOption::NoUsageNotes,
        Some("fallback") => FilterOption::Fallback,
        None => FilterOption::None,
        Some(other) => return Err(anyhow!("Invalid filter '{}'. Use: no-description, no-usage-notes, fallback", other)),
    };

    // Step 6: Query matching
    let query_result = match_query(&query, &cache)?;
    
    // Step 7: Apply category filter
    let filtered_result = match category_filter.as_deref() {
        Some(cat) => apply_category_filter(query_result, cat)?,
        None => query_result,
    };

    // Step 8: Apply custom filter
    let final_result = apply_custom_filter(filtered_result, &cache, &base_cache, filter_option)?;

    // Step 9: Format and output
    match normalized_format {
        OutputFormat::Llm => {
            if diff_mode {
                formatter::format_diff_llm(&final_result, &base_cache.unwrap(), show_all)
            } else {
                formatter::format_llm(&final_result, show_all)
            }
        }
        OutputFormat::Full => {
            formatter::format_full(&final_result)
        }
        OutputFormat::Json => {
            formatter::format_json(&final_result)
        }
    }
}

// Query matching based on pattern type
fn match_query(query: &str, cache: &FieldMetadataCache) -> Result<QueryResult> {
    // Check if exact field name match (e.g., "metrics.clicks")
    if let Some(field) = cache.get_field(query) {
        return Ok(QueryResult::SingleField(field.clone()));
    }
    
    // Check if exact resource name match (e.g., "campaign")
    if cache.get_resources().contains(&query.to_string()) {
        let resource_meta = cache.resource_metadata
            .as_ref()
            .and_then(|rm| rm.get(query).cloned());
        let fields = cache.get_resource_fields(query);
        return Ok(QueryResult::Resource(query.to_string(), resource_meta, fields));
    }
    
    // Pattern matching (e.g., "metrics.*", "campaign.*", "*clicks*")
    let matching_fields = cache.find_fields(query);
    if !matching_fields.is_empty() {
        return Ok(QueryResult::FieldList(matching_fields));
    }
    
    // No matches found
    Err(anyhow!("No matches found for query: '{}'.\n\nHint: Try a resource name, field name, or pattern (e.g., 'campaign', 'metrics.*', '*conversion*')", query))
}
```

### 4. New Module: formatter.rs

Create new file `crates/mcc-gaql-gen/src/formatter.rs`:

```rust
use anyhow::Result;
use mcc_gaql_common::field_metadata::{FieldMetadata, FieldMetadataCache, ResourceMetadata};
use std::path::Path;

/// Output format variants
pub enum OutputFormat {
    Llm,
    Full,
    Json,
}

/// Custom filter options
pub enum FilterOption {
    None,
    NoDescription,
    NoUsageNotes,
    Fallback,
}

/// Result of query matching
pub enum QueryResult {
    SingleField(FieldMetadata),
    Resource(String, Option<ResourceMetadata>, Vec<&FieldMetadata>),
    FieldList(Vec<&FieldMetadata>),
}

/// Constants for LLM view limits
const LLM_CATEGORY_LIMIT_DEFAULT: usize = 15;

/// Format in LLM-limited style (15 fields per category by default)
pub fn format_llm(result: &QueryResult, show_all: bool) -> Result<String> {
    let mut output = String::new();
    
    match result {
        QueryResult::SingleField(field) => {
            output.push_str(&format_single_field_llm(field));
        }
        QueryResult::Resource(name, resource_meta, fields) => {
            output.push_str(&format_resource_llm(name, resource_meta, fields, show_all));
        }
        QueryResult::FieldList(fields) => {
            // Group by category and limit per category
            let mut categories = std::collections::HashMap::new();
            for field in *fields {
                categories
                    .entry(field.category.clone())
                    .or_insert_with(Vec::new)
                    .push(field);
            }
            
            for (cat, cat_fields) in categories {
                let limit = if show_all { usize::MAX } else { LLM_CATEGORY_LIMIT_DEFAULT };
                output.push_str(&format_category_fields_llm(&cat, cat_fields, limit));
                output.push('\n');
            }
        }
    }
    
    Ok(output)
}

/// Format in full detail mode (all metadata fields)
pub fn format_full(result: &QueryResult) -> Result<String> {
    let mut output = String::new();
    
    match result {
        QueryResult::SingleField(field) => {
            output.push_str(&format_single_field_full(field));
        }
        QueryResult::Resource(name, resource_meta, fields) => {
            // Show resource metadata first
            if let Some(rm) = resource_meta {
                output.push_str(&format_resource_metadata_full(rm));
                output.push('\n');
            }
            
            // Show all fields in full detail
            for field in fields {
                output.push_str(&format_single_field_full(field));
                output.push_str("\n---\n");
            }
        }
        QueryResult::FieldList(fields) => {
            for field in *fields {
                output.push_str(&format_single_field_full(field));
                output.push_str("\n---\n");
            }
        }
    }
    
    Ok(output)
}

/// Format as raw JSON for scripting
pub fn format_json(result: &QueryResult) -> Result<String> {
    let json_value = match result {
        QueryResult::SingleField(field) => serde_json::to_value(field)?,
        QueryResult::Resource(_name, resource_meta, fields) => {
            serde_json::json!({
                "resource_metadata": resource_meta,
                "fields": fields
            })
        }
        QueryResult::FieldList(fields) => serde_json::to_value(fields)?,
    };
    
    Ok(serde_json::to_string_pretty(&json_value)?)
}

/// Format for diff mode (enriched vs non-enriched)
pub fn format_diff_llm(
    result: &QueryResult,
    base_cache: &FieldMetadataCache,
    show_all: bool,
) -> Result<String> {
    let mut output = String::new();
    
    // Print summary stats
    let total_fields = count_total_fields(result);
    let enriched_fields = count_fields_with_descriptions(result, base_cache);
    output.push_str(&format!("═ Enrichment Summary ═\n"));
    output.push_str(&format!("Total fields: {}\n", total_fields));
    output.push_str(&format!("Enriched: {}/{} fields have descriptions\n", enriched_fields, total_fields));
    output.push_str(&format!("Coverage: {:.1}%\n\n", (enriched_fields as f64 / total_fields as f64) * 100.0));
    
    // Format with [llm-enriched] markers
    match result {
        QueryResult::SingleField(field) => {
            let is_enriched = is_field_llm_enriched(field, base_cache);
            output.push_str(&format_single_field_llm_with_marker(field, is_enriched));
        }
        QueryResult::Resource(name, resource_meta, fields) => {
            output.push_str(&format_resource_llm_with_diff(name, resource_meta, fields, base_cache, show_all));
        }
        QueryResult::FieldList(fields) => {
            let mut categories = std::collections::HashMap::new();
            for field in *fields {
                categories
                    .entry(field.category.clone())
                    .or_insert_with(Vec::new)
                    .push(field);
            }
            
            for (cat, cat_fields) in categories {
                let limit = if show_all { usize::MAX } else { LLM_CATEGORY_LIMIT_DEFAULT };
                output.push_str(&format_category_fields_llm_with_diff(
                    &cat,
                    cat_fields,
                    base_cache,
                    limit,
                ));
                output.push('\n');
            }
        }
    }
    
    Ok(output)
}

// ========== Helper Functions ==========

/// Format a single field in LLM style
fn format_single_field_llm(field: &FieldMetadata) -> String {
    let filterable_tag = if field.filterable { " [filterable]" } else { "" };
    let sortable_tag = if field.sortable { " [sortable]" } else { "" };
    let description_tag = if field.description.is_none() {
        "[no description]"
    } else {
        ""
    };
    
    let enum_note = if !field.enum_values.is_empty() {
        format!(" (valid: {})", field.enum_values.join(", "))
    } else {
        String::new()
    };
    
    let desc = field.description.as_deref().unwrap_or("");
    
    format!(
        "- {}{}{}: {}{}{}\n",
        field.name,
        filterable_tag,
        sortable_tag,
        desc,
        description_tag,
        enum_note
    )
}

/// Format a single field in LLM style with enrichment marker
fn format_single_field_llm_with_marker(field: &FieldMetadata, is_llm_enriched: bool) -> String {
    let enrich_marker = if is_llm_enriched { "[llm-enriched]" } else { "" };
    let filterable_tag = if field.filterable { " [filterable]" } else { "" };
    let sortable_tag = if field.sortable { " [sortable]" } else { "" };
    
    let desc = field.description.as_deref().unwrap_or("");
    
    format!(
        "- {} {}{}{}: {}\n",
        field.name,
        enrich_marker,
        filterable_tag,
        sortable_tag,
        desc
    )
}

/// Format resource in LLM style with ResourceMetadata
fn format_resource_llm(
    name: &str,
    resource_meta: &Option<ResourceMetadata>,
    fields: &[&FieldMetadata],
    show_all: bool,
) -> String {
    let mut output = String::new();
    
    // Resource header
    output.push_str(&format!("=== RESOURCE: {} ===\n", name));
    
    // Resource description if available
    if let Some(rm) = resource_meta {
        if let Some(desc) = &rm.description {
            output.push_str(&format!("Description: {}\n", desc));
            output.push('\n');
        }
        
        if !rm.selectable_with.is_empty() {
            output.push_str(&format!("Selectable with: {} ({} total)\n", 
                rm.selectable_with.iter().take(10).cloned().collect::<Vec<_>>().join(", "),
                rm.selectable_with.len()
            ));
        }
        
        let key_attrs = format_key_fields(&rm.key_attributes, false, rm.key_attributes.len() > 100);
        let key_metrics = format_key_fields(&rm.key_metrics, false, rm.key_metrics.len() > 100);
        
        if !rm.key_attributes.is_empty() {
            output.push_str(&format!("Key attributes: {}\n", key_attrs));
        }
        if !rm.key_metrics.is_empty() {
            output.push_str(&format!("Key metrics: {}\n", key_metrics));
        }
        output.push('\n');
    }
    
    // Group fields by category
    let mut categories = std::collections::HashMap::new();
    for field in fields {
        categories
            .entry(field.category.clone())
            .or_insert_with(Vec::new)
            .push(field);
    }
    
    // Format each category
    let limit = if show_all { usize::MAX } else { LLM_CATEGORY_LIMIT_DEFAULT };
    for cat in ["RESOURCE", "ATTRIBUTE", "METRIC", "SEGMENT"] {
        if let Some(cat_fields) = categories.get(cat) {
            if !cat_fields.is_empty() {
                output.push_str(&format_category_fields_llm(cat, cat_fields, limit));
                output.push('\n');
            }
        }
    }
    
    output
}

/// Format resource in LLM style with diff markers
fn format_resource_llm_with_diff(
    name: &str,
    resource_meta: &Option<ResourceMetadata>,
    fields: &[&FieldMetadata],
    base_cache: &FieldMetadataCache,
    show_all: bool,
) -> String {
    let mut output = format_resource_llm(name, resource_meta, fields, show_all);
    
    // Add before/after for key fields if they used fallback
    if let Some(rm) = resource_meta {
        if is_using_alphabetical_fallback(name, rm, fields) {
            output.push_str("\n⚠️ [fallback: alphabetical] Key fields selected using alphabetical ordering\n");
        }
    }
    
    output
}

/// Format fields category-wise with LLM style
fn format_category_fields_llm(category: &str, fields: &[&FieldMetadata], limit: usize) -> String {
    let mut output = String::new();
    output.push_str(&format!("### {} ({})\n", category, fields.len()));
    
    for field in fields.iter().take(limit) {
        output.push_str(&format_single_field_llm(field));
    }
    
    if fields.len() > limit {
        output.push_str(&format!("... ({} more fields, use --show-all to see all)\n", fields.len() - limit));
    }
    
    output
}

/// Format fields category-wise with diff markers
fn format_category_fields_llm_with_diff(
    category: &str,
    fields: &[&FieldMetadata],
    base_cache: &FieldMetadataCache,
    limit: usize,
) -> String {
    let mut output = format_category_fields_llm(category, fields, limit);
    
    // Add before/after section for fields with enrichment
    output.push_str("\n── Before/After Enrichment ──\n");
    for field in fields.iter().take(limit) {
        let base_field = base_cache.get_field(&field.name);
        if let (Some(enriched_desc), Some(base_desc_opt)) = (&field.description, base_field.and_then(|f| f.description.as_ref())) {
            if enriched_desc != base_desc_opt {
                output.push_str(&field.name);
                output.push_str(":\n  Before: ");
                output.push_str(base_desc_opt);
                output.push_str("\n  After:  ");
                output.push_str(enriched_desc);
                output.push_str("\n\n");
            }
        }
    }
    
    output
}

/// Format a single field in full detail
fn format_single_field_full(field: &FieldMetadata) -> String {
    let mut output = String::new();
    
    output.push_str(&format!("=== FIELD: {} ===\n", field.name));
    output.push_str(&format!("Category: {}\n", field.category));
    output.push_str(&format!("Data type: {}\n", field.data_type));
    
    if let Some(resource) = field.get_resource() {
        output.push_str(&format!("Resource: {}\n", resource));
    }
    
    output.push_str("\nFlags:\n");
    output.push_str(&format!("  Selectable: {}\n", field.selectable));
    output.push_str(&format!("  Filterable: {}\n", field.filterable));
    output.push_str(&format!("  Sortable: {}\n", field.sortable));
    output.push_str(&format!("  Metrics compatible: {}\n", field.metrics_compatible));
    
    if let Some(desc) = &field.description {
        output.push_str(&format!("\nDescription:\n  {}\n", desc));
    }
    
    if let Some(notes) = &field.usage_notes {
        output.push_str(&format!("\nUsage notes:\n  {}", notes));
    } else {
        output.push_str("[no usage_notes]");
    }
    
    if !field.enum_values.is_empty() {
        output.push_str(&format!("\n\nEnum values:\n  {}\n", field.enum_values.join(", ")));
    }
    
    if !field.selectable_with.is_empty() {
        output.push_str(&format!("\nSelectable with ({}):\n", field.selectable_with.len()));
        output.push_str(&format!("  {}\n", field.selectable_with.iter().take(20).cloned().collect::<Vec<_>>().join(", ")));
    }
    
    if !field.attribute_resources.is_empty() {
        output.push_str(&format!("\nAttribute resources ({}):\n", field.attribute_resources.len()));
        output.push_str(&format!("  {}\n", field.attribute_resources.join(", ")));
    }
    
    output
}

/// Format resource metadata in full detail
fn format_resource_metadata_full(rm: &ResourceMetadata) -> String {
    let mut output = String::new();
    
    output.push_str(&format!("=== RESOURCE METADATA: {} ===\n", rm.name));
    output.push_str(&format!("Field count: {}\n", rm.field_count));
    
    if let Some(desc) = &rm.description {
        output.push_str(&format!("Description: {}\n", desc));
    }
    
    let key_attrs = format_key_fields(&rm.key_attributes, true, rm.key_attributes.len() > 50);
    let key_metrics = format_key_fields(&rm.key_metrics, true, rm.key_metrics.len() > 50);
    
    if !rm.key_attributes.is_empty() {
        output.push_str(&format!("\nKey attributes ({}):\n  {}\n", rm.key_attributes.len(), key_attrs));
    }
    if !rm.key_metrics.is_empty() {
        output.push_str(&format!("\nKey metrics ({}):\n  {}\n", rm.key_metrics.len(), key_metrics));
    }
    
    if !rm.selectable_with.is_empty() {
        output.push_str(&format!("\nSelectable with ({}):\n  {}\n", 
            rm.selectable_with.len(),
            rm.selectable_with.iter().take(20).cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    
    output
}

/// Format key fields with optional truncation
fn format_key_fields(fields: &[String], truncate: bool, needs_truncation: bool) -> String {
    let display = if truncate && needs_truncation {
        fields.iter().take(10).cloned().collect::<Vec<_>>().join(", ")
    } else {
        fields.join(", ")
    };
    
    if truncate && needs_truncation && fields.len() > 10 {
        format!("{}... ({} total)", display, fields.len())
    } else {
        display
    }
}

/// Check if a field is LLM-enriched (has description different from base)
fn is_field_llm_enriched(field: &FieldMetadata, base_cache: &FieldMetadataCache) -> bool {
    let base_field = base_cache.get_field(&field.name);
    match (field.description.as_ref(), base_field.and_then(|f| f.description.as_ref())) {
        (Some(enriched), Some(base)) => enriched != base,
        (Some(_), None) => true,
        (None, _) => false,
    }
}

/// Check if resource used alphabetical fallback for key fields
fn is_using_alphabetical_fallback(
    resource_name: &str,
    rm: &ResourceMetadata,
    fields: &[&FieldMetadata],
) -> bool {
    // Simple heuristic: if key attributes are sorted alphabetically and are just first few field names
    // This detection logic would need refinement based on actual enrichment implementation
    false  // Placeholder - implement based on actual enrichment logic
}

/// Count total fields in query result
fn count_total_fields(result: &QueryResult) -> usize {
    match result {
        QueryResult::SingleField(_) => 1,
        QueryResult::Resource(_, _, fields) => fields.len(),
        QueryResult::FieldList(fields) => fields.len(),
    }
}

/// Count fields with descriptions (enriched)
fn count_fields_with_descriptions(result: &QueryResult, base_cache: &FieldMetadataCache) -> usize {
    match result {
        QueryResult::SingleField(field) => {
            if field.description.is_some() { 1 } else { 0 }
        }
        QueryResult::Resource(_, _, fields) => {
            fields.iter()
                .filter(|f| {
                    let base_field = base_cache.get_field(&f.name);
                    match (f.description.as_ref(), base_field.and_then(|bf| bf.description.as_ref())) {
                        (Some(_), None) => true,  // Has description now but not before
                        (Some(enriched), Some(base)) => enriched != base,  // Description changed
                        _ => false,
                    }
                })
                .count()
        }
        QueryResult::FieldList(fields) => {
            fields.iter()
                .filter(|f| f.description.is_some())
                .count()
        }
    }
}
```

### 5. Update lib.rs

Add to `crates/mcc-gaql-gen/src/lib.rs`:

```rust
pub mod formatter;
```

### 6. Additional Helper Functions (main.rs)

Add these to main.rs for use in cmd_metadata:

```rust
/// Apply category filter to query result
fn apply_category_filter(result: QueryResult, category: &str) -> Result<QueryResult> {
    let normalized_cat = category.to_uppercase();
    
    if !matches!(normalized_cat.as_str(), "RESOURCE" | "ATTRIBUTE" | "METRIC" | "SEGMENT") {
        return Err(anyhow!("Invalid category '{}'. Use: resource, attribute, metric, segment", category));
    }
    
    match result {
        QueryResult::Resource(name, meta, fields) => {
            let filtered_fields: Vec<_> = fields
                .into_iter()
                .filter(|f| f.category == normalized_cat)
                .collect();
            Ok(QueryResult::Resource(name, meta, filtered_fields))
        }
        QueryResult::FieldList(fields) => {
            let filtered: Vec<_> = fields
                .into_iter()
                .filter(|f| f.category == normalized_cat)
                .collect();
            Ok(QueryResult::FieldList(filtered))
        }
        QueryResult::SingleField(field) => {
            if field.category == normalized_cat {
                Ok(QueryResult::SingleField(field))
            } else {
                Err(anyhow!("Field '{}' does not match category '{}'", field.name, category))
            }
        }
    }
}

/// Apply custom filter options
fn apply_custom_filter(
    result: QueryResult,
    cache: &FieldMetadataCache,
    base_cache: &Option<FieldMetadataCache>,
    filter: FilterOption,
) -> Result<QueryResult> {
    match filter {
        FilterOption::None => Ok(result),
        FilterOption::NoDescription => filter_no_description(result),
        FilterOption::NoUsageNotes => filter_no_usage_notes(result),
        FilterOption::Fallback => filter_fallback_resources(result, cache, base_cache),
    }
}

/// Filter for fields without descriptions
fn filter_no_description(result: QueryResult) -> Result<QueryResult> {
    match result {
        QueryResult::Resource(name, meta, fields) => {
            let filtered: Vec<_> = fields
                .into_iter()
                .filter(|f| f.description.is_none())
                .collect();
            Ok(QueryResult::Resource(name, meta, filtered))
        }
        QueryResult::FieldList(fields) => {
            let filtered: Vec<_> = fields
                .into_iter()
                .filter(|f| f.description.is_none())
                .collect();
            Ok(QueryResult::FieldList(filtered))
        }
        QueryResult::SingleField(field) => {
            if field.description.is_none() {
                Ok(QueryResult::SingleField(field))
            } else {
                Err(anyhow!("Field '{}' has a description, doesn't match no-description filter", field.name))
            }
        }
    }
}

/// Filter for fields without usage notes
fn filter_no_usage_notes(result: QueryResult) -> Result<QueryResult> {
    match result {
        QueryResult::Resource(name, meta, fields) => {
            let filtered: Vec<_> = fields
                .into_iter()
                .filter(|f| f.usage_notes.is_none())
                .collect();
            Ok(QueryResult::Resource(name, meta, filtered))
        }
        QueryResult::FieldList(fields) => {
            let filtered: Vec<_> = fields
                .into_iter()
                .filter(|f| f.usage_notes.is_none())
                .collect();
            Ok(QueryResult::FieldList(filtered))
        }
        QueryResult::SingleField(field) => {
            if field.usage_notes.is_none() {
                Ok(QueryResult::SingleField(field))
            } else {
                Err(anyhow!("Field '{}' has usage notes, doesn't match no-usage-notes filter", field.name))
            }
        }
    }
}

/// Filter for resources using alphabetical fallback for key fields
fn filter_fallback_resources(
    result: QueryResult,
    cache: &FieldMetadataCache,
    base_cache: &Option<FieldMetadataCache>,
) -> Result<QueryResult> {
    // This would need to check enrichment metadata to detect fallback usage
    // For now, return the result as-is (implement detection logic later)
    Ok(result)
}
```

## Edge Cases and Error Handling

### 1. Missing Metadata File

```rust
if !cache_path.exists() {
    return Err(anyhow!(
        "Enriched metadata file not found at {:?}. \
         Run 'mcc-gaql-gen enrich' to generate it.",
        cache_path
    ));
}
```

### 2. Empty Query Results

```rust
if final_result.is_empty() {
    println!("No results found for query: '{}'", query);
    if let Some(cat) = category_filter {
        println!("(Filtering by category: {})", cat);
    }
    return Ok(());
}
```

### 3. Invalid Category

Handle in `apply_category_filter` with error message.

### 4. Invalid Filter

Handle with clear error message indicating valid options.

### 5. Non-Enriched Cache Missing for --diff

```rust
let base_cache = if diff_mode {
    let base_path = match mcc_gaql_common::paths::field_metadata_cache_path() {
        Ok(path) if path.exists() => path,
        Ok(path) => return Err(anyhow!(
            "Diff mode requires non-enriched metadata at {:?}. \
             Run 'mcc-gaql --refresh-field-cache' first.",
            path
        )),
        Err(e) => return Err(anyhow!("Could not locate base cache: {}", e)),
    };
    Some(FieldMetadataCache::load_from_disk(&base_path).await?)
} else {
    None
};
```

## Data Flow Diagrams

### Query Matching Flow

```
User Input: "campaign"
    ↓
Is exact field name? → No
    ↓
Is exact resource name? → Yes
    ↓
Return QueryResult::Resource("campaign", ResourceMetadata, fields[])
```

### Filtering Pipeline

```
QueryResult
    ↓
[subset flag?] → filter_test_resources()
    ↓
[category flag?] → apply_category_filter()
    ↓
[custom filter?] → apply_custom_filter()
    ↓
Final QueryResult
```

### Formatting Selection

```
Final QueryResult
    ↓
[format] switch
    ├─ llm → formatter::format_llm()
    │         [diff?] → formatter::format_diff_llm()
    │         [show_all?] → override limit
    │
    ├─ full → formatter::format_full()
    │
    └─ json → formatter::format_json()
```

## Testing Recommendations

### Unit Tests

Add to `main.rs` test module:

```rust
#[cfg(test)]
mod metadata_tests {
    use super::*;

    #[test]
    fn test_query_matching_exact_field() {
        // Test field name matching
    }

    #[test]
    fn test_query_matching_exact_resource() {
        // Test resource name matching
    }

    #[test]
    fn test_query_matching_pattern() {
        // Test pattern matching ("metrics.*", "*clicks*")
    }

    #[test]
    fn test_category_filter() {
        // Test category filtering
    }

    #[test]
    fn test_no_description_filter() {
        // Test no-description filter
    }

    #[test]
    fn test_subset_mode() {
        // Test subset (test-run) filtering
    }
}
```

### Integration Tests

Test with real metadata files:

1. Test `mcc-gaql-gen metadata campaign` output format
2. Test `mcc-gaql-gen metadata metrics.clicks` full format
3. Test `mcc-gaql-gen metadata "metrics.*" --format json` JSON output
4. Test `--category metric` filtering
5. Test `--subset` limitation
6. Test `--diff` mode comparison
7. Test `--filter no-description` filtering

### Manual Testing Scenarios

1. View all fields for keyword_view resource
2. View single metric field details
3. Pattern match for conversion-related fields
4. Filter by category only
5. JSON output for scripting
6. Subset mode with test resources
7. Diff mode to see enrichment
8. Show all fields vs LLM-limited view

## Consistency with Existing Patterns

### LLM Context Formatting

Reuse the formatting pattern from `rag.rs` lines 1687-1708:

```rust
// Category header
candidate_text.push_str(&format!("\n### {} ({})\n", cat, fields.len()));

// Field entries (limited to 15 per category)
for f in fields.iter().take(15) {
    let filterable_tag = if f.filterable { " [filterable]" } else { "" };
    let sortable_tag = if f.sortable { " [sortable]" } else { "" };
    
    let enum_note = filter_enums
        .iter()
        .find(|(name, _)| name == &f.name)
        .map(|(_, enums)| format!(" (valid: {})", enums.join(", ")))
        .unwrap_or_default();
    
    candidate_text.push_str(&format!(
        "- {}{}{}: {}{}\n",
        f.name,
        filterable_tag,
        sortable_tag,
        f.description.unwrap_or_else(|| "No description".to_string()),
        enum_note
    ));
}
```

This ensures the output matches exactly what the LLM sees during field selection.

### Error Handling Pattern

Follow existing pattern from main.rs:

```rust
let cache_path = metadata_path
    .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
    .context("Could not determine enriched metadata path. Use --metadata to specify it.")?;

let cache = FieldMetadataCache::load_from_disk(&cache_path)
    .await
    .context("Failed to load enriched metadata. Run 'mcc-gaql-gen enrich' first.")?;
```

### Path Resolution

Use `mcc_gaql_common::paths` for default paths, as shown in existing commands.

### Clap Argument Definition

Follow existing pattern in main.rs for subcommand definitions and options.

## Implementation Checklist

### Phase 1: Core Command Structure
- [ ] Add `Metadata` variant to `Commands` enum
- [ ] Add handler function `cmd_metadata`
- [ ] Wire up handler in main match statement
- [ ] Test basic CLI argument parsing

### Phase 2: Query Matching
- [ ] Implement `match_query` function
- [ ] Add exact field name matching
- [ ] Add exact resource name matching
- [ ] Add pattern matching support
- [ ] Test all query types

### Phase 3: Filtering
- [ ] Implement `apply_category_filter`
- [ ] Implement `apply_custom_filter`
- [ ] Implement `filter_no_description`
- [ ] Implement `filter_no_usage_notes`
- [ ] Implement `filter_fallback_resources`
- [ ] Implement subset (test-run) filtering
- [ ] Test all filters

### Phase 4: LLM Format Formatter
- [ ] Create `formatter.rs` module
- [ ] Implement `format_single_field_llm`
- [ ] Implement `format_resource_llm`
- [ ] Implement `format_category_fields_llm`
- [ ] Implement LLM category limits (15 fields)
- [ ] Implement `--show-all` override
- [ ] Test LLM format matches Phase 3 output

### Phase 5: Full Format Formatter
- [ ] Implement `format_single_field_full`
- [ ] Implement `format_resource_metadata_full`
- [ ] Test full format displays all fields

### Phase 6: JSON Format Formatter
- [ ] Implement `format_json`
- [ ] Test JSON output validity
- [ ] Test JSON parsing with jq/other tools

### Phase 7: Diff Mode
- [ ] Implement `format_diff_llm` wrapper
- [ ] Implement `format_category_fields_llm_with_diff`
- [ ] Implement `is_field_llm_enriched`
- [ ] Implement `count_enriched_fields`
- [ ] Add before/after enrichment display
- [ ] Test diff mode output

### Phase 8: Quality Indicators
- [ ] Add `[no description]` marker
- [ ] Add `[no usage_notes]` marker
- [ ] Add `[fallback: alphabetical]` marker
- [ ] Add `[llm-enriched]` marker
- [ ] Test quality indicator display

### Phase 9: Error Handling
- [ ] Handle missing metadata file
- [ ] Handle empty query results
- [ ] Handle invalid category
- [ ] Handle invalid filter
- [ ] Handle missing non-enriched cache (diff mode)
- [ ] Test all error paths

### Phase 10: Documentation and Testing
- [ ] Add inline comments
- [ ] Write unit tests
- [ ] Write integration tests
- [ ] Update README with examples
- [ ] Verify output matches spec requirements

## File Modifications Summary

### crates/mcc-gaql-gen/src/main.rs
- Add `Metadata` command variant (~30 lines)
- Add `cmd_metadata` handler function (~100 lines)
- Add helper functions: `apply_category_filter`, `apply_custom_filter`, `filter_*` functions (~80 lines)
- Add match query logic (~40 lines)
- Add tests (~50 lines)

### crates/mcc-gaql-gen/src/formatter.rs (NEW)
- Module exports and enums (~30 lines)
- `format_llm` and LLM formatters (~150 lines)
- `format_full` and full formatters (~100 lines)
- `format_json` (~20 lines)
- `format_diff_llm` and diff formatters (~100 lines)
- Helper functions (~150 lines)

### crates/mcc-gaql-gen/src/lib.rs
- Add `pub mod formatter;` (1 line)

Total new code: ~700-800 lines

## Dependencies

No new dependencies needed. Uses existing:
- `anyhow` for error handling
- `clap` for CLI parsing
- `serde_json` for JSON output
- `tokio` for async file I/O
- `chrono` (already in dependencies)

## Usage Examples

### View resource metadata

```bash
# LLM format (default, 15 fields per category)
mcc-gaql-gen metadata keyword_view

# Full output (all fields)
mcc-gaql-gen metadata keyword_view --show-all

# Full format (all metadata details)
mcc-gaql-gen metadata keyword_view --format full
```

### View single field

```bash
# LLM format
mcc-gaql-gen metadata metrics.clicks

# Full format
mcc-gaql-gen metadata metrics.clicks --format full

# JSON output for scripting
mcc-gaql-gen metadata metrics.clicks --format json
```

### Pattern matching

```bash
# All metrics
mcc-gaql-gen metadata "metrics.*"

# Campaign attributes
mcc-gaql-gen metadata "campaign.*"

# All conversion-related fields
mcc-gaql-gen metadata "*conversion*"

# Filter by category
mcc-gaql-gen metadata "campaign.*" --category metric
```

### Advanced filtering

```bash
# Subset resources only (test-run mode)
mcc-gaql-gen metadata campaign --subset

# Show fields without descriptions
mcc-gaql-gen metadata "campaign.*" --filter no-description

# Show diff between enriched and non-enriched
mcc-gaql-gen metadata campaign --diff

# All filtering combined
mcc-gaql-gen metadata "campaign.*" --category attribute --show-all --filter no-description
```

## Verification Steps

1. **Consistency Check**: Compare output of `mcc-gaql-gen metadata keyword_view --format llm` against actual Phase 3 prompt content from rag.rs to ensure exact match.

2. **Format Validation**: 
   - Verify JSON output parses correctly with `jq`
   - Verify full format shows all FieldMetadata fields
   - Verify LLM format applies 15-field limit

3. **Filter Validation**:
   - Test subset mode limits to 4 resources
   - Test category filter only shows specified category
   - Test custom filters work as expected

4. **Diff Mode Validation**:
   - Verify summary stats are correct
   - Verify [llm-enriched] markers appear correctly
   - Verify before/after section shows changes

5. **Error Path Validation**:
   - Test with missing metadata file
   - Test with empty query results
   - Test with invalid options

## Future Enhancements (Out of Scope for MVP)

- Interactive mode with fuzzy search (fzf-like)
- `--stats` flag for aggregate statistics
- Color-coded output quality indicators
- Export to markdown documentation
- Side-by-side diff view for larger screens
- Custom limit per category via CLI flag

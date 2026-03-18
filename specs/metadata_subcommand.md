# Design: `mcc-gaql-gen metadata` Subcommand

## Context

Users need to manually examine enriched metadata for resources, attributes, metrics, and segments to understand what context the LLM receives during RAG-based query generation. Currently there's no easy way to inspect this data without writing custom code or parsing JSON files.

## Goals

1. Provide human-readable output of enriched metadata consistent with what the LLM sees during field selection
2. Support querying by resource name, field name, or field pattern
3. Show all relevant metadata: descriptions, categories, selectability flags, enum values, and relationships

## Command Interface

```
mcc-gaql-gen metadata [OPTIONS] <QUERY>

Arguments:
  <QUERY>  Resource name (e.g., "campaign"), field name (e.g., "metrics.clicks"),
           or pattern (e.g., "campaign.*", "metrics.*")

Options:
  --metadata <PATH>   Path to enriched metadata JSON [default: cache path]
  --format <FORMAT>   Output format: "llm" (LLM context style), "full" (all fields),
                      "json" (raw JSON) [default: llm]
  --category <CAT>    Filter by category: resource, attribute, metric, segment
  --test-run          Use test-run resources only
```

## Output Formats

### Format: `llm` (default)

Mirrors how fields appear in the Phase 3 LLM prompt during field selection:

```
=== RESOURCE: keyword_view ===
Description: Aggregated keyword performance data for Search campaigns...
Selectable with: campaign, ad_group, customer (132 total)
Key attributes: keyword_view.resource_name, ad_group_criterion.keyword.text, ...
Key metrics: metrics.clicks, metrics.impressions, metrics.conversions, ...

### ATTRIBUTE (15)
- ad_group_criterion.keyword.text [filterable] [sortable]: The keyword text...
- ad_group_criterion.keyword.match_type [filterable]: Match type for keyword
  (valid: EXACT, PHRASE, BROAD)
- campaign.name [filterable] [sortable]: The campaign name
...

### METRIC (42)
- metrics.clicks: Number of user clicks on ads
- metrics.impressions: Number of times ads were displayed
- metrics.conversions: Number of conversion actions
...

### SEGMENT (12)
- segments.date [filterable]: Date for time-based segmentation
- segments.device [filterable]: Device type segmentation
  (valid: DESKTOP, MOBILE, TABLET, OTHER)
...
```

For a single field query (e.g., `metrics.clicks`):

```
=== FIELD: metrics.clicks ===
Category: METRIC
Data type: INT64
Selectable: true | Filterable: true | Sortable: true

Description:
  Number of user clicks on ads

Selectable with (132 resources):
  campaign, ad_group, ad_group_ad, keyword_view, search_term_view, ...
```

### Format: `full`

Shows all metadata fields including internal ones:

```
=== FIELD: campaign.status ===
Name: campaign.status
Category: ATTRIBUTE
Data type: ENUM
Resource: campaign

Flags:
  Selectable: true
  Filterable: true
  Sortable: true
  Metrics compatible: true

Description:
  The status of the campaign

Usage notes:
  Filter by ENABLED to get active campaigns only

Enum values:
  UNSPECIFIED, UNKNOWN, ENABLED, PAUSED, REMOVED

Selectable with (9):
  ad_group, ad_group_ad, ad_group_criterion, ...

Attribute resources (3):
  campaign, accessible_bidding_strategy, bidding_strategy
```

### Format: `json`

Raw JSON output of matching FieldMetadata entries, useful for scripting:

```json
{
  "metrics.clicks": {
    "name": "metrics.clicks",
    "category": "METRIC",
    "data_type": "INT64",
    "selectable": true,
    "filterable": true,
    "sortable": true,
    "description": "Number of user clicks on ads",
    "selectable_with": ["campaign", "ad_group", ...],
    ...
  }
}
```

## Query Matching

| Query | Behavior |
|-------|----------|
| `campaign` | Show ResourceMetadata for campaign + all its fields grouped by category |
| `keyword_view` | Show ResourceMetadata for keyword_view + all its fields |
| `metrics.clicks` | Show single FieldMetadata |
| `campaign.status` | Show single FieldMetadata |
| `metrics.*` | Show all metrics fields |
| `campaign.*` | Show all campaign attributes |
| `*.clicks` | Show fields ending in `.clicks` |
| `*conversion*` | Show fields containing "conversion" |

Pattern matching uses glob-style wildcards via the existing `find_fields()` method.

## Implementation

### Location

`crates/mcc-gaql-gen/src/main.rs` - add new command variant and handler.

### Key Components

1. **Command enum addition:**
```rust
/// Display enriched field metadata
Metadata {
    /// Resource, field name, or pattern to query
    query: String,

    /// Path to enriched metadata JSON
    #[arg(long)]
    metadata: Option<PathBuf>,

    /// Output format: llm, full, or json
    #[arg(long, default_value = "llm")]
    format: String,

    /// Filter by category
    #[arg(long)]
    category: Option<String>,

    /// Use test-run resources only
    #[arg(long)]
    test_run: bool,
}
```

2. **Handler function:**
```rust
async fn cmd_metadata(
    query: String,
    metadata_path: Option<PathBuf>,
    format: String,
    category: Option<String>,
    test_run: bool,
) -> anyhow::Result<()>
```

3. **Output formatters:**
- `format_field_llm_style(field: &FieldMetadata) -> String`
- `format_field_full(field: &FieldMetadata) -> String`
- `format_resource_llm_style(resource: &str, cache: &FieldMetadataCache) -> String`

### Data Sources

Uses `FieldMetadataCache` loaded from enriched metadata file:
- `get_resources()` - list all resources
- `get_resource_fields(resource)` - fields for a resource
- `get_field(name)` - single field lookup
- `find_fields(pattern)` - pattern matching
- `resource_metadata` - ResourceMetadata with key_attributes, key_metrics, description

### Consistency with LLM Context

The `llm` format should match the formatting in `rag.rs` Phase 3:
- Field tags: `[filterable]`, `[sortable]`
- Enum display: `(valid: VALUE1, VALUE2, ...)`
- Category headers: `### ATTRIBUTE (N)`, `### METRIC (N)`, `### SEGMENT (N)`
- Description on same line after colon

Consider extracting shared formatting logic into a helper module to ensure consistency.

## Example Usage

```bash
# View all metadata for keyword_view resource
mcc-gaql-gen metadata keyword_view

# View single metric field
mcc-gaql-gen metadata metrics.clicks

# View all conversion-related fields
mcc-gaql-gen metadata "*conversion*"

# View only metrics for campaign resource
mcc-gaql-gen metadata "campaign" --category metric

# Full details for a field
mcc-gaql-gen metadata campaign.status --format full

# JSON output for scripting
mcc-gaql-gen metadata "metrics.*" --format json

# Use test-run subset
mcc-gaql-gen metadata keyword_view --test-run
```

## Verification

1. Compare output of `mcc-gaql-gen metadata keyword_view --format llm` against the actual prompt content in `rag.rs` Phase 3 field selection
2. Verify all FieldMetadata fields are displayed in `--format full`
3. Verify JSON output can be parsed and matches source file structure
4. Test pattern matching with various glob patterns
5. Verify `--test-run` properly filters to TEST_RUN_RESOURCES

## Future Enhancements

- `--diff` flag to compare two metadata files (e.g., before/after enrichment)
- `--stats` flag to show aggregate statistics (field counts by category, enrichment coverage)
- Interactive mode with fuzzy search

# Enriched Fields Cache with LLM-Generated Descriptions

**Status**: Proposed
**Created**: 2025-11-10
**Author**: Claude (AI Assistant)

## Overview

This specification describes a system to enrich the Google Ads Fields metadata cache with human-readable descriptions suitable for RAG (Retrieval Augmented Generation) applications. The current GoogleAdsField API response lacks detailed descriptions, limiting the effectiveness of LLM-based query generation features.

## Problem Statement

### Current Limitations

1. **Missing Descriptions**: The Google Ads API `GoogleAdsField` structure provides metadata about fields but does not include human-readable descriptions:
   ```
   GoogleAdsField {
       name: "campaign.name",
       category: ATTRIBUTE,
       selectable: true,
       filterable: true,
       sortable: true,
       data_type: STRING,
       // NO description field!
   }
   ```

2. **Poor RAG Context**: The `prompt2gaql` feature uses field metadata for RAG to help convert natural language to GAQL queries, but lacks semantic context about what each field represents.

3. **No Version Tracking**: The current field cache has no API version stamp, making it unclear when regeneration is needed.

4. **Suboptimal LLM Understanding**: Without descriptions, the LLM must infer field meanings from names alone, leading to:
   - Confusion between similar fields (e.g., `campaign.name` vs `campaign.advertising_channel_type`)
   - Missed opportunities to suggest relevant fields
   - Less accurate query generation

### Example Use Cases Requiring Descriptions

**Scenario 1: Natural Language Query**
```
User: "Show me campaigns that spent more than $1000 last month"
```

Without descriptions, the LLM needs to guess:
- Is it `metrics.cost_micros` or `metrics.average_cost`?
- What's the difference between `campaign.status` and `campaign.primary_status`?

With descriptions:
- `metrics.cost_micros`: "The total cost in micros (1/1,000,000 of the account currency)"
- `campaign.status`: "The status set directly by the advertiser (ENABLED, PAUSED, REMOVED)"
- `campaign.primary_status`: "The primary serving status combining system and advertiser controls"

**Scenario 2: Field Discovery**
```
User: "What ad performance metrics are available?"
```

With descriptions, the system can provide contextual explanations of each metric.

## Goals

1. **Enrich Field Metadata**: Add human-readable descriptions to all Google Ads fields
2. **Support RAG**: Improve LLM context for natural language to GAQL conversion
3. **Version Tracking**: Stamp cache with API version to enable smart regeneration
4. **Maintain Performance**: Cache enriched data to avoid repeated enrichment
5. **Graceful Degradation**: Fall back to base fields if enrichment fails

## Non-Goals

- Real-time description generation (use cached data)
- Multi-language support (English only initially)
- Field usage statistics or recommendations
- Custom field descriptions (use canonical sources)

## Architecture

### High-Level Design

```
┌─────────────────────────────────────────────────────────────┐
│                     mcc-gaql Application                     │
│                                                              │
│  ┌───────────────────────────────────────────────────────┐  │
│  │           prompt2gaql (RAG Agent)                     │  │
│  │  - Uses enriched fields for better context           │  │
│  │  - Embeddings include descriptions                   │  │
│  └───────────────────────────────────────────────────────┘  │
│                            ▲                                 │
│                            │                                 │
│  ┌───────────────────────────────────────────────────────┐  │
│  │      Enriched Fields Cache Manager                    │  │
│  │  - Load enriched cache (if exists)                    │  │
│  │  - Version validation                                 │  │
│  │  - Fallback to base fields                            │  │
│  └───────────────────────────────────────────────────────┘  │
│                            ▲                                 │
│                            │                                 │
│  ┌───────────────────────────────────────────────────────┐  │
│  │       Field Enrichment Pipeline                       │  │
│  │  1. Fetch base fields from GoogleAdsFieldService      │  │
│  │  2. Enrich with descriptions (web scrape + LLM)       │  │
│  │  3. Add API version stamp                             │  │
│  │  4. Write enriched cache to disk                      │  │
│  └───────────────────────────────────────────────────────┘  │
│                            ▲                                 │
│                            │                                 │
│  ┌───────────────────────────────────────────────────────┐  │
│  │           CLI Commands                                │  │
│  │  - mcc-gaql --refresh-field-cache                     │  │
│  │  - mcc-gaql --field-cache-info                        │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘

External Sources:
┌──────────────────────────────────────────────────────────┐
│ Google Ads API (GoogleAdsFieldService)                   │
│ - Base field metadata (name, category, type, etc.)      │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│ Google Ads Field Reference Documentation                │
│ - https://developers.google.com/google-ads/api/fields/  │
│ - Web scraping for descriptions                         │
└──────────────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────────────┐
│ LLM (OpenAI/Claude)                                      │
│ - Generate descriptions for fields missing from docs    │
│ - Enhance/standardize scraped descriptions              │
└──────────────────────────────────────────────────────────┘
```

### Data Flow

```
1. Initialization
   └─> Check if enriched cache exists
       └─> If exists: Load and validate version
           ├─> Version matches: Use cache ✓
           └─> Version mismatch: Regenerate
       └─> If not exists: Generate new cache

2. Cache Generation/Refresh
   └─> Fetch base fields from GoogleAdsFieldService
   └─> For each field:
       ├─> Attempt web scraping from Google docs
       ├─> If no description found: Generate with LLM
       └─> Validate and normalize description
   └─> Add API version metadata
   └─> Write to cache file

3. Runtime Usage
   └─> Load enriched fields
   └─> Use for RAG context in prompt2gaql
   └─> Graceful fallback to base fields if needed
```

## Data Schema

### Enriched Field Structure

```json
{
  "version": "22.0",
  "generated_at": "2025-11-10T12:34:56Z",
  "generator_info": {
    "method": "hybrid",
    "llm_model": "gpt-4o-mini",
    "web_scraper_version": "1.0"
  },
  "statistics": {
    "total_fields": 1247,
    "web_scraped": 892,
    "llm_generated": 355,
    "failed": 0
  },
  "fields": [
    {
      "name": "campaign.name",
      "category": "ATTRIBUTE",
      "resource": "campaign",
      "selectable": true,
      "filterable": true,
      "sortable": true,
      "data_type": "STRING",
      "type_url": "google.protobuf.StringValue",
      "is_repeated": false,
      "description": "The name of the campaign. Can be used in SELECT, WHERE, and ORDER BY clauses.",
      "description_source": "web_scraped",
      "description_generated_at": "2025-11-10T12:34:56Z",
      "example_values": ["Brand Campaign", "Holiday Sale 2024"],
      "related_fields": ["campaign.id", "campaign.status"],
      "selectable_with": ["customer.id", "metrics.impressions", "..."]
    },
    {
      "name": "metrics.impressions",
      "category": "METRIC",
      "resource": null,
      "selectable": true,
      "filterable": true,
      "sortable": true,
      "data_type": "INT64",
      "type_url": "google.protobuf.Int64Value",
      "is_repeated": false,
      "description": "Count of how often your ad has appeared on a search results page or website on the Google Network. An impression is counted each time your ad is shown on a search result page or other site on the Google Network.",
      "description_source": "web_scraped",
      "description_generated_at": "2025-11-10T12:34:56Z",
      "example_values": [1000, 50000, 1000000],
      "related_fields": ["metrics.clicks", "metrics.ctr", "metrics.cost_micros"],
      "selectable_with": ["segments.date", "campaign.name", "..."]
    },
    {
      "name": "campaign_criterion.keyword.text",
      "category": "ATTRIBUTE",
      "resource": "campaign_criterion",
      "selectable": true,
      "filterable": true,
      "sortable": true,
      "data_type": "STRING",
      "type_url": "google.protobuf.StringValue",
      "is_repeated": false,
      "description": "The text of the keyword (at most 80 characters and 10 words).",
      "description_source": "llm_generated",
      "description_generated_at": "2025-11-10T12:35:12Z",
      "example_values": ["shoes", "buy shoes online"],
      "related_fields": ["campaign_criterion.keyword.match_type"],
      "selectable_with": ["campaign.id", "metrics.clicks"]
    }
  ]
}
```

### Field Schema Definition

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedFieldsCache {
    /// Google Ads API version (e.g., "22.0", "23.0")
    pub version: String,

    /// Timestamp when cache was generated
    pub generated_at: String,

    /// Information about how descriptions were generated
    pub generator_info: GeneratorInfo,

    /// Statistics about enrichment process
    pub statistics: EnrichmentStatistics,

    /// List of enriched fields
    pub fields: Vec<EnrichedField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneratorInfo {
    /// Method used: "web_scrape", "llm", "hybrid"
    pub method: String,

    /// LLM model if used (e.g., "gpt-4o-mini", "claude-sonnet-4.5")
    pub llm_model: Option<String>,

    /// Web scraper version
    pub web_scraper_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichmentStatistics {
    pub total_fields: usize,
    pub web_scraped: usize,
    pub llm_generated: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedField {
    // Base GoogleAdsField attributes
    pub name: String,
    pub category: String,
    pub resource: Option<String>,
    pub selectable: bool,
    pub filterable: bool,
    pub sortable: bool,
    pub data_type: String,
    pub type_url: String,
    pub is_repeated: bool,
    pub selectable_with: Vec<String>,

    // Enriched attributes
    pub description: String,
    pub description_source: String,  // "web_scraped", "llm_generated", "manual"
    pub description_generated_at: String,
    pub example_values: Option<Vec<String>>,
    pub related_fields: Option<Vec<String>>,
}
```

## Implementation Plan

### Phase 1: Infrastructure Setup

**Files to Create/Modify:**

1. **`src/field_enrichment/mod.rs`** - New module
   - Module structure and public API

2. **`src/field_enrichment/cache.rs`**
   - `EnrichedFieldsCache` struct and methods
   - Load/save functionality
   - Version validation

3. **`src/field_enrichment/enricher.rs`**
   - Core enrichment pipeline
   - Orchestration of scraping and LLM generation

4. **`src/field_enrichment/scraper.rs`**
   - Web scraping from Google Ads field reference pages
   - HTML parsing and extraction

5. **`src/field_enrichment/llm_generator.rs`**
   - LLM-based description generation
   - Prompt templates
   - Rate limiting and error handling

### Phase 2: Core Functionality

#### Module 1: Cache Management

**File**: `src/field_enrichment/cache.rs`

```rust
pub struct CacheManager {
    cache_path: PathBuf,
}

impl CacheManager {
    pub fn new() -> Result<Self>;

    /// Load enriched cache from disk
    pub async fn load(&self) -> Result<Option<EnrichedFieldsCache>>;

    /// Save enriched cache to disk
    pub async fn save(&self, cache: &EnrichedFieldsCache) -> Result<()>;

    /// Check if cache is valid for current API version
    pub fn is_valid(&self, cache: &EnrichedFieldsCache, current_version: &str) -> bool;

    /// Get cache file path
    pub fn get_cache_path(&self) -> &PathBuf;

    /// Delete cache (for manual refresh)
    pub async fn delete(&self) -> Result<()>;

    /// Get cache metadata (version, age, size)
    pub async fn info(&self) -> Result<CacheInfo>;
}

#[derive(Debug)]
pub struct CacheInfo {
    pub exists: bool,
    pub version: Option<String>,
    pub generated_at: Option<String>,
    pub field_count: Option<usize>,
    pub file_size_bytes: Option<u64>,
    pub age_days: Option<f64>,
}
```

#### Module 2: Field Enrichment Pipeline

**File**: `src/field_enrichment/enricher.rs`

```rust
pub struct FieldEnricher {
    api_context: GoogleAdsAPIAccess,
    scraper: Option<WebScraper>,
    llm_generator: Option<LlmGenerator>,
    cache_manager: CacheManager,
}

impl FieldEnricher {
    /// Create new enricher with all capabilities
    pub fn new(
        api_context: GoogleAdsAPIAccess,
        openai_api_key: Option<String>,
    ) -> Result<Self>;

    /// Fetch base fields from GoogleAdsFieldService
    pub async fn fetch_base_fields(&self) -> Result<Vec<BaseField>>;

    /// Detect current API version
    pub async fn detect_api_version(&self) -> Result<String>;

    /// Enrich a single field with description
    pub async fn enrich_field(&self, field: &BaseField) -> Result<EnrichedField>;

    /// Enrich all fields (main pipeline)
    pub async fn enrich_all_fields(&self) -> Result<EnrichedFieldsCache>;

    /// Load or generate enriched cache
    pub async fn get_enriched_fields(&self) -> Result<EnrichedFieldsCache>;
}

#[derive(Debug, Clone)]
pub struct BaseField {
    pub name: String,
    pub category: String,
    pub resource: Option<String>,
    pub selectable: bool,
    pub filterable: bool,
    pub sortable: bool,
    pub data_type: String,
    pub type_url: String,
    pub is_repeated: bool,
    pub selectable_with: Vec<String>,
}
```

#### Module 3: Web Scraper

**File**: `src/field_enrichment/scraper.rs`

```rust
pub struct WebScraper {
    client: reqwest::Client,
    base_url: String,
}

impl WebScraper {
    pub fn new() -> Result<Self>;

    /// Scrape description for a specific field
    pub async fn scrape_field_description(&self, field_name: &str) -> Result<Option<String>>;

    /// Extract resource and field name from full path
    fn parse_field_name(&self, field_name: &str) -> (String, String);

    /// Build URL for field documentation page
    fn build_doc_url(&self, resource: &str, api_version: &str) -> String;

    /// Parse HTML and extract field description
    fn extract_description(&self, html: &str, field_name: &str) -> Option<String>;

    /// Rate limiting and retry logic
    async fn fetch_with_retry(&self, url: &str) -> Result<String>;
}
```

**Scraping Strategy:**

Google Ads field reference pages follow pattern:
```
https://developers.google.com/google-ads/api/fields/v22/{resource}
```

For example:
- `campaign.name` → https://developers.google.com/google-ads/api/fields/v22/campaign
- `metrics.impressions` → https://developers.google.com/google-ads/api/fields/v22/metrics

Each page contains an HTML table with field names and descriptions that can be scraped.

#### Module 4: LLM Description Generator

**File**: `src/field_enrichment/llm_generator.rs`

```rust
pub struct LlmGenerator {
    client: OpenAIClient,
    model: String,
}

impl LlmGenerator {
    pub fn new(api_key: String) -> Result<Self>;

    /// Generate description for a field using LLM
    pub async fn generate_description(&self, field: &BaseField) -> Result<String>;

    /// Build prompt for LLM
    fn build_prompt(&self, field: &BaseField) -> String;

    /// Validate generated description
    fn validate_description(&self, description: &str) -> bool;
}
```

**LLM Prompt Template:**

```
You are a Google Ads API documentation expert. Generate a concise, accurate description for the following Google Ads field.

Field Name: {field_name}
Category: {category}
Data Type: {data_type}
Selectable: {selectable}
Filterable: {filterable}
Sortable: {sortable}

Requirements:
1. Description should be 1-2 sentences (max 200 characters)
2. Explain what the field represents in plain English
3. Mention if it's commonly used in queries
4. Be factually accurate based on Google Ads API semantics

Example format:
"The unique ID of the campaign. This immutable field is assigned when the campaign is created."

Generate description:
```

### Phase 3: Integration

#### Update prompt2gaql Module

**File**: `src/prompt2gaql.rs`

```rust
// Modify to use enriched fields
impl RAGAgent {
    pub async fn init_with_enriched_fields(
        openai_api_key: &str,
        enriched_cache: &EnrichedFieldsCache,
    ) -> Result<Self> {
        // Use enriched field descriptions for embeddings
        let documents: Vec<FieldDocument> = enriched_cache.fields
            .iter()
            .map(|f| FieldDocument {
                name: f.name.clone(),
                description: f.description.clone(),
                category: f.category.clone(),
            })
            .collect();

        // Build RAG index with enriched descriptions
        // ...
    }
}

#[derive(Clone)]
struct FieldDocument {
    name: String,
    description: String,
    category: String,
}

impl Embed for FieldDocument {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        // Combine name and description for better semantic search
        let text = format!("{}: {}", self.name, self.description);
        embedder.embed(text);
        Ok(())
    }
}
```

#### Add CLI Commands

**File**: `src/args.rs`

```rust
#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
    // ... existing fields ...

    /// Refresh the enriched fields cache
    #[clap(long)]
    pub refresh_field_cache: bool,

    /// Show information about the fields cache
    #[clap(long)]
    pub field_cache_info: bool,

    /// Force regeneration even if cache version matches
    #[clap(long)]
    pub force_cache_refresh: bool,
}
```

**File**: `src/main.rs`

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = args::parse();

    // Handle field cache commands
    if args.field_cache_info {
        let cache_manager = CacheManager::new()?;
        let info = cache_manager.info().await?;
        println!("Field Cache Information:");
        println!("  Exists: {}", info.exists);
        if info.exists {
            println!("  Version: {}", info.version.unwrap_or("unknown".to_string()));
            println!("  Fields: {}", info.field_count.unwrap_or(0));
            println!("  Generated: {}", info.generated_at.unwrap_or("unknown".to_string()));
            println!("  Age: {:.1} days", info.age_days.unwrap_or(0.0));
        }
        return Ok(());
    }

    if args.refresh_field_cache {
        println!("Refreshing enriched fields cache...");
        let api_context = googleads::get_api_access(/* ... */).await?;
        let openai_key = env::var("OPENAI_API_KEY").ok();

        let enricher = FieldEnricher::new(api_context, openai_key)?;
        let cache = enricher.enrich_all_fields().await?;

        println!("Cache refreshed successfully!");
        println!("  Version: {}", cache.version);
        println!("  Total fields: {}", cache.statistics.total_fields);
        println!("  Web scraped: {}", cache.statistics.web_scraped);
        println!("  LLM generated: {}", cache.statistics.llm_generated);

        return Ok(());
    }

    // ... rest of main logic ...
}
```

### Phase 4: Configuration

#### Environment Variables

```bash
# LLM Configuration (optional, for enrichment only)
export OPENAI_API_KEY="sk-..."

# Cache location (optional, defaults to config dir)
export MCC_GAQL_FIELD_CACHE_PATH="~/.config/mcc-gaql/enriched_fields_cache.json"

# Enrichment behavior
export MCC_GAQL_ENABLE_WEB_SCRAPING="true"
export MCC_GAQL_ENABLE_LLM_GENERATION="true"
```

#### Cache Location

Default: `~/.config/mcc-gaql/enriched_fields_cache.json`

Platform-specific:
- **Linux**: `~/.config/mcc-gaql/enriched_fields_cache.json`
- **macOS**: `~/Library/Application Support/mcc-gaql/enriched_fields_cache.json`
- **Windows**: `%APPDATA%\mcc-gaql\enriched_fields_cache.json`

## API Version Detection

### Strategy

Google Ads API version is embedded in the `googleads-rs` crate dependency. Extract version from:

1. **Cargo.toml parsing**: Read the `googleads-rs` version
2. **Proto package inspection**: Check imported proto package version
3. **Manual configuration**: Allow override via env var

```rust
pub fn detect_api_version() -> Result<String> {
    // Strategy 1: Check googleads-rs crate version
    if let Some(version) = extract_from_dependencies() {
        return Ok(version);
    }

    // Strategy 2: Parse from proto imports
    if let Some(version) = parse_from_proto_package() {
        return Ok(version);
    }

    // Strategy 3: Manual override
    if let Ok(version) = env::var("MCC_GAQL_GOOGLE_ADS_API_VERSION") {
        return Ok(version);
    }

    // Fallback: Use hardcoded current version
    Ok("22".to_string())
}

fn extract_from_dependencies() -> Option<String> {
    // Parse current code's imports to detect v22, v23, etc.
    // Example: googleads_rs::google::ads::googleads::v22 => "22"
    None
}
```

## Dependencies

### New Crate Dependencies

Add to `Cargo.toml`:

```toml
[dependencies]
# Existing dependencies...

# Web scraping
reqwest = { version = "0.11", features = ["json"] }
scraper = "0.17"

# HTML parsing (if needed beyond scraper)
select = "0.6"

# LLM client (reuse existing rig-core)
# rig-core = "0.7.0"  # Already present

# Async runtime (already present)
# tokio = { version = "1.0", features = ["rt-multi-thread", "macros"] }

# Serialization (already present)
# serde = { version = "1.0", features = ["derive"] }
# serde_json = "1.0"
```

## Usage Examples

### Generate Initial Cache

```bash
# Generate enriched fields cache (requires Google Ads API access and optional OpenAI key)
mcc-gaql --refresh-field-cache

# Output:
# Fetching base fields from Google Ads API...
# Found 1247 fields
# Enriching fields (this may take a few minutes)...
#   Web scraped: 892 (71.5%)
#   LLM generated: 355 (28.5%)
#   Failed: 0 (0%)
# Cache saved to: ~/.config/mcc-gaql/enriched_fields_cache.json
```

### Check Cache Status

```bash
mcc-gaql --field-cache-info

# Output:
# Field Cache Information:
#   Exists: true
#   Version: 22.0
#   Fields: 1247
#   Generated: 2025-11-10T12:34:56Z
#   Age: 5.2 days
```

### Force Refresh

```bash
# Force refresh even if version matches
mcc-gaql --refresh-field-cache --force-cache-refresh
```

### Use in Natural Language Queries

```bash
# The enriched cache is automatically used by prompt2gaql
mcc-gaql --natural-language "show me campaigns with more than 1000 impressions last week"

# With enriched descriptions, the LLM has better context:
# - "metrics.impressions": Count of how often your ad appeared
# - "segments.date": Date of the observation (for filtering by time)
# - "campaign.name": The name of the campaign
```

## Error Handling

### Graceful Degradation

```rust
pub async fn get_enriched_fields_or_fallback(
    &self
) -> Result<Vec<EnrichedField>> {
    // Try to load enriched cache
    match self.cache_manager.load().await {
        Ok(Some(cache)) => {
            // Validate version
            let current_version = self.detect_api_version().await?;
            if self.cache_manager.is_valid(&cache, &current_version) {
                return Ok(cache.fields);
            }
            log::warn!("Cache version mismatch, falling back to base fields");
        }
        Ok(None) => {
            log::info!("No enriched cache found, using base fields");
        }
        Err(e) => {
            log::error!("Failed to load enriched cache: {}", e);
        }
    }

    // Fallback: Use base fields without descriptions
    let base_fields = self.fetch_base_fields().await?;
    Ok(base_fields.into_iter().map(|f| f.into()).collect())
}
```

### Error Scenarios

1. **Web Scraping Fails**
   - Fallback to LLM generation
   - Log warning but continue

2. **LLM Generation Fails**
   - Use empty description or field name as description
   - Mark field as needing manual review

3. **API Version Detection Fails**
   - Use last known version from cache
   - Log warning

4. **Cache Corruption**
   - Delete corrupt cache
   - Regenerate from scratch

## Performance Considerations

### Caching Strategy

- **First Run**: Takes 5-10 minutes to enrich ~1200 fields
  - Web scraping: ~2-3 seconds per resource page (batched)
  - LLM generation: ~1 second per field (with batching)

- **Subsequent Runs**: Instant (load from cache)

### Optimization Opportunities

1. **Parallel Processing**: Enrich fields concurrently (10-20 workers)
2. **Batch LLM Requests**: Generate descriptions in batches of 50-100
3. **Incremental Updates**: Only enrich new fields in version updates
4. **Compression**: Gzip cache file to reduce size

### Resource Usage

- **Cache File Size**: ~500KB-1MB (uncompressed JSON)
- **Memory**: ~10-20MB for loaded cache
- **Network**: ~50-100 HTTP requests for web scraping
- **LLM API Calls**: 300-500 requests (for missing fields only)

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_save_load() {
        // Test cache persistence
    }

    #[tokio::test]
    async fn test_version_validation() {
        // Test version matching logic
    }

    #[test]
    fn test_field_enrichment() {
        // Test enrichment with mock data
    }
}
```

### Integration Tests

1. **End-to-End Enrichment**
   ```bash
   cargo test --test field_enrichment_integration -- --ignored
   ```

2. **Prompt2GAQL with Enriched Fields**
   ```bash
   cargo test --test prompt2gaql_enriched -- --ignored
   ```

### Manual Testing Checklist

- [ ] Generate cache from scratch
- [ ] Load existing cache
- [ ] Detect version mismatch
- [ ] Handle web scraping failures
- [ ] Handle LLM generation failures
- [ ] Use enriched fields in prompt2gaql
- [ ] Verify description quality
- [ ] Test cache info command
- [ ] Test force refresh

## Security Considerations

1. **API Keys**: OpenAI API key must be secured via environment variables
2. **Rate Limiting**: Respect Google Ads API and OpenAI rate limits
3. **Cache Validation**: Validate JSON schema when loading cache
4. **Web Scraping**: Handle CAPTCHA and rate limiting gracefully

## Migration Path

### For Existing Users

1. **No Action Required**: Enrichment is opt-in via CLI flag
2. **First Time Setup**: Run `mcc-gaql --refresh-field-cache`
3. **Natural Language Queries**: Automatically use enriched cache if available

### Backward Compatibility

- No changes to existing GAQL query functionality
- prompt2gaql works with or without enriched cache
- Graceful fallback to base fields if enrichment unavailable

## Future Enhancements

### Phase 2 Features (Not in Initial Scope)

1. **Multi-Language Support**: Generate descriptions in multiple languages
2. **Custom Descriptions**: Allow users to override/add custom descriptions
3. **Field Usage Analytics**: Track which fields are commonly used
4. **Smart Recommendations**: Suggest related fields based on query context
5. **Auto-Update**: Automatically refresh cache on version change
6. **Field Categories**: Group fields by use case (reporting, optimization, etc.)
7. **Example Queries**: Include example GAQL queries for each field
8. **Field Relationships**: Map semantic relationships between fields

### Potential Improvements

- **Alternative LLM Providers**: Support Claude, Gemini, etc.
- **Offline Mode**: Ship pre-generated cache with releases
- **Incremental Enrichment**: Only enrich new/changed fields
- **Caching Layers**: Add in-memory cache for hot paths
- **Description Versioning**: Track description changes over time

## Success Metrics

### Quantitative

- **Cache Generation Time**: < 10 minutes for full enrichment
- **Cache Load Time**: < 100ms
- **Description Coverage**: > 95% of fields have descriptions
- **Description Quality**: Manual review of 100 random samples shows > 90% accuracy

### Qualitative

- Improved prompt2gaql accuracy (measured by user feedback)
- Better field discovery experience
- Reduced confusion about field meanings
- More natural conversation with LLM agent

## Open Questions

1. **Web Scraping Reliability**: How stable is the Google Ads field reference page structure?
   - *Resolution*: Add version-specific scraping logic + LLM fallback

2. **LLM Cost**: What's the total cost for generating descriptions for all fields?
   - *Estimate*: ~500 fields × $0.00015/call = $0.075 (negligible)

3. **Update Frequency**: How often does Google Ads API add new fields?
   - *Resolution*: API version changes ~quarterly, regenerate cache then

4. **Description Format**: Should we use structured descriptions (JSON) or plain text?
   - *Resolution*: Plain text for simplicity, structured data in separate fields

## References

- [Google Ads API Field Service Documentation](https://developers.google.com/google-ads/api/docs/concepts/field-service)
- [Google Ads API Field Reference](https://developers.google.com/google-ads/api/fields/v22/)
- [GoogleAdsField Proto Definition](https://developers.google.com/google-ads/api/reference/rpc/v22/GoogleAdsField)
- [Rig-Core RAG Documentation](https://docs.rs/rig-core/latest/rig_core/)

## Appendix A: Example Enriched Fields

### campaign.name
```json
{
  "name": "campaign.name",
  "category": "ATTRIBUTE",
  "resource": "campaign",
  "data_type": "STRING",
  "description": "The name of the campaign. This field can contain up to 255 characters and is used for identification purposes in the Google Ads UI.",
  "description_source": "web_scraped",
  "example_values": ["Brand Campaign", "Summer Sale 2024"]
}
```

### metrics.impressions
```json
{
  "name": "metrics.impressions",
  "category": "METRIC",
  "data_type": "INT64",
  "description": "Count of how often your ad has appeared on a search results page or website on the Google Network. An impression is counted each time your ad is shown.",
  "description_source": "web_scraped",
  "example_values": [1000, 50000]
}
```

### campaign.bidding_strategy_type
```json
{
  "name": "campaign.bidding_strategy_type",
  "category": "ATTRIBUTE",
  "resource": "campaign",
  "data_type": "ENUM",
  "description": "The type of bidding strategy used by the campaign. Common values include MANUAL_CPC, TARGET_CPA, TARGET_ROAS, and MAXIMIZE_CONVERSIONS.",
  "description_source": "llm_generated",
  "example_values": ["MANUAL_CPC", "TARGET_CPA", "MAXIMIZE_CONVERSIONS"]
}
```

## Appendix B: Implementation Timeline

### Week 1: Foundation
- [ ] Create module structure
- [ ] Implement cache management
- [ ] Add version detection logic

### Week 2: Enrichment Pipeline
- [ ] Implement web scraper
- [ ] Implement LLM generator
- [ ] Build enrichment orchestration

### Week 3: Integration
- [ ] Update prompt2gaql to use enriched fields
- [ ] Add CLI commands
- [ ] Error handling and logging

### Week 4: Testing & Polish
- [ ] Unit tests
- [ ] Integration tests
- [ ] Documentation
- [ ] Performance optimization

## Appendix C: Sample CLI Output

```bash
$ mcc-gaql --refresh-field-cache

🔄 Refreshing Enriched Fields Cache
═══════════════════════════════════════════════════════════════

✓ Detected API version: v22
✓ Fetching base fields from Google Ads API...
  Found 1,247 fields across 89 resources

📡 Enriching fields with descriptions...

  Progress: [████████████████████████████████████] 1247/1247

  ├─ Web Scraped:    892 fields (71.5%)
  ├─ LLM Generated:  355 fields (28.5%)
  └─ Failed:           0 fields (0.0%)

💾 Saving cache...
  Location: ~/.config/mcc-gaql/enriched_fields_cache.json
  Size: 847 KB

✅ Cache refreshed successfully!

Summary:
  Version:       22.0
  Total Fields:  1,247
  Generated At:  2025-11-10 12:34:56 UTC
  Duration:      6m 23s
```

---

**End of Specification**

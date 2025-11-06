# GAQL Metadata for LLM-Powered Diagnostics - Design Specification

**Version:** 1.0
**Date:** 2025-11-06
**Author:** Design Spec for mcc-gaql Enhancement
**Status:** DRAFT - For Review

---

## Executive Summary

This design spec evaluates two approaches for providing Google Ads GAQL metadata to LLMs (specifically Claude Skills) to enable intelligent query construction and performance diagnostics. After analysis, **Option B (Enhanced Natural Query)** is recommended as it leverages existing infrastructure, provides dynamic schema awareness, and offers superior accuracy.

---

## Background

### Problem Statement

A Claude Skill for diagnosing Google Ads performance issues requires:
1. **Accurate GAQL query construction** - Valid syntax and semantically correct field selection
2. **Resource discovery** - Understanding available resources (campaign, ad_group, keyword_view, etc.)
3. **Field metadata** - Knowing which fields are selectable, filterable, and their relationships
4. **Segmentation awareness** - Understanding how to break down metrics (by date, device, etc.)
5. **Metric selection** - Choosing appropriate metrics for specific diagnostic questions

### Current State

mcc-gaql-rs already provides:
- **Fields Service API integration** (`googleads.rs:389-416`) - Query field metadata
- **Natural language queries** (`prompt2gaql.rs`) - Convert English to GAQL using GPT-4o-mini with RAG
- **Query cookbook** (`resources/query_cookbook.toml`) - 30+ example queries with descriptions
- **GAQL execution** - Stream queries across multiple accounts with aggregation

### Current Limitations

1. **Natural query feature**:
   - Only uses query cookbook for RAG context (examples only)
   - No awareness of actual Google Ads field schema
   - Cannot validate field compatibility before execution
   - Limited to patterns seen in cookbook

2. **Fields Service**:
   - Outputs only to text format (tab-separated)
   - Not integrated with query construction workflow
   - No caching mechanism (requires API call each time)

---

## Option A: Static Reference Guide

### Approach

Create comprehensive markdown documentation in `specs/` that explains:
- How to query Fields Service API for metadata
- Available resources and their relationships
- Common field patterns and naming conventions
- Example GAQL queries categorized by use case
- Rules for valid GAQL construction

### Architecture

```
Claude Skill
    ↓
Reference Guide (markdown)
    ↓
LLM reasoning about query structure
    ↓
mcc-gaql execution
```

### Implementation Details

**Reference guide contents:**

1. **Fields Service Queries** (`specs/fields-service-reference.md`)
   ```
   # Querying Field Metadata

   mcc-gaql --field-service "SELECT name, category, selectable, filterable
                             FROM google_ads_field
                             WHERE category = 'RESOURCE'"

   # Available Resources
   - customer: Account-level data
   - campaign: Campaign configuration and metrics
   - ad_group: Ad group structure
   - keyword_view: Keyword performance
   ...
   ```

2. **GAQL Construction Guide** (`specs/gaql-query-patterns.md`)
   ```
   # Resource Selection Rules
   - Primary resource determines FROM clause
   - Related resources joinable via implicit JOINs
   - Segments always selectable with metrics

   # Example: Campaign performance by device
   SELECT campaign.name, segments.device, metrics.impressions, metrics.clicks
   FROM campaign
   WHERE segments.date DURING LAST_30_DAYS
   ```

3. **Field Reference** (`specs/google-ads-fields.md`)
   - Comprehensive list of resources (generated from Fields Service)
   - Common metrics and their data types
   - Segmentation fields
   - Filter compatibility matrix

### Workflow for Claude Skill

1. **Skill includes reference docs** in its system prompt/knowledge base
2. **User asks diagnostic question**: "Why did my CTR drop last week?"
3. **LLM reasons** using reference guide:
   - Need: campaign.name, segments.date, metrics.ctr, metrics.impressions, metrics.clicks
   - Date range: LAST_14_DAYS for trend analysis
   - Resource: campaign (primary)
4. **LLM constructs GAQL**:
   ```sql
   SELECT campaign.name, segments.date, metrics.ctr,
          metrics.impressions, metrics.clicks
   FROM campaign
   WHERE segments.date DURING LAST_14_DAYS
   ORDER BY segments.date
   ```
5. **Skill executes** via mcc-gaql
6. **LLM analyzes results** and provides diagnostic insights

### Advantages

✅ **Simple implementation** - Just documentation, no code changes
✅ **Transparent** - User can review reference material
✅ **No API dependency** - Works offline after initial doc generation
✅ **Large context models** - Claude Sonnet 4.5 has 200K token context
✅ **Educational** - Helps users learn GAQL structure

### Disadvantages

❌ **Static content** - Requires manual updates when Google Ads API changes
❌ **No validation** - LLM may construct invalid queries (discovered only at execution)
❌ **Context overhead** - Consumes significant token budget
❌ **Field compatibility** - Cannot dynamically check if fields are selectable together
❌ **Limited schema awareness** - Documentation can't cover all field combinations
❌ **Error recovery** - If query fails, limited guidance on valid alternatives

### Estimated Effort

- **Documentation creation**: 8-12 hours
- **Maintenance**: 2-4 hours per Google Ads API version
- **Total**: Low implementation cost, moderate maintenance

---

## Option B: Enhanced Natural Query (Recommended)

### Approach

Enhance the existing natural language query feature to include Google Ads field metadata from the Fields Service API, creating a schema-aware query generator with validation.

### Architecture

```
Claude Skill / User Query
    ↓
mcc-gaql --natural-language (enhanced)
    ↓
┌─────────────────────────────┐
│  Field Metadata Cache       │
│  (from Fields Service)      │
│  - Resources                │
│  - Fields per resource      │
│  - Selectability rules      │
│  - Filter compatibility     │
└─────────────────────────────┘
    ↓
┌─────────────────────────────┐
│  Enhanced RAG Context       │
│  1. Query cookbook examples │
│  2. Field schema metadata   │
│  3. Validation rules        │
└─────────────────────────────┘
    ↓
GPT-4o-mini with enriched context
    ↓
GAQL Query (validated)
    ↓
Execution + Results
```

### Implementation Details

#### 1. Field Metadata Caching

**New module:** `src/field_metadata.rs`

```rust
pub struct FieldMetadata {
    pub name: String,
    pub category: FieldCategory,  // RESOURCE, ATTRIBUTE, METRIC, SEGMENT
    pub data_type: DataType,
    pub selectable: bool,
    pub filterable: bool,
    pub selectable_with: Vec<String>,  // Compatible resources
    pub metrics_compatible: bool,
}

pub struct FieldMetadataCache {
    fields: HashMap<String, FieldMetadata>,
    resources: HashMap<String, Vec<String>>,  // resource -> field names
    last_updated: DateTime<Utc>,
}

impl FieldMetadataCache {
    // Load from cache file or fetch from API
    pub async fn load_or_fetch(
        api_context: &GoogleAdsAPIAccess,
        cache_path: &Path,
        max_age: Duration,
    ) -> Result<Self>;

    // Query all fields and organize by category
    async fn fetch_from_api(api_context: &GoogleAdsAPIAccess) -> Result<Self>;

    // Serialize to disk for offline use
    pub fn save_to_disk(&self, cache_path: &Path) -> Result<()>;

    // Find fields matching criteria
    pub fn find_metrics(&self, pattern: Option<&str>) -> Vec<&FieldMetadata>;
    pub fn find_attributes(&self, resource: &str) -> Vec<&FieldMetadata>;
    pub fn find_segments(&self) -> Vec<&FieldMetadata>;

    // Validation
    pub fn validate_query_fields(&self, fields: &[String]) -> Result<ValidationReport>;
}
```

**Cache location:** `~/.cache/mcc-gaql/field_metadata.json`
**Cache TTL:** 7 days (configurable)
**Cache size:** ~500KB for full schema

#### 2. Enhanced RAG Context

**Enhanced module:** `src/prompt2gaql.rs`

```rust
pub struct EnhancedRAGAgent {
    query_agent: RAGAgent,  // Existing cookbook RAG
    field_cache: FieldMetadataCache,
}

impl EnhancedRAGAgent {
    pub async fn new(
        openai_api_key: &str,
        cookbook: Vec<QueryEntry>,
        field_cache: FieldMetadataCache,
    ) -> Result<Self>;

    // Generate enriched context for LLM
    fn build_context(&self, user_query: &str) -> String {
        // 1. Get similar cookbook examples (top 5)
        let examples = self.query_agent.find_similar(user_query, 5);

        // 2. Identify likely resources from user intent
        let resources = self.identify_resources(user_query);

        // 3. Get relevant fields for those resources
        let relevant_fields = self.get_relevant_fields(&resources);

        // 4. Add common segments if temporal/device analysis mentioned
        let segments = self.identify_segments(user_query);

        // Format as structured prompt context
        format_context(examples, resources, relevant_fields, segments)
    }
}
```

**Enhanced system prompt:**

```
You are a Google Ads GAQL query assistant. Convert natural language
requests into valid GAQL queries.

AVAILABLE SCHEMA:
{dynamic_field_metadata}

EXAMPLE QUERIES:
{cookbook_examples}

RULES:
- SELECT only fields marked as selectable
- FROM clause uses primary resource
- WHERE supports filterable fields
- Metrics require resource or segment grouping
- Always include date segments for trending analysis

Respond with valid GAQL only, no formatting.
```

#### 3. Pre-execution Validation

```rust
pub async fn convert_to_gaql_validated(
    openai_api_key: &str,
    field_cache: &FieldMetadataCache,
    cookbook: Vec<QueryEntry>,
    user_query: &str,
) -> Result<String> {
    // Generate GAQL with enhanced context
    let gaql = convert_to_gaql_enhanced(...).await?;

    // Parse and validate
    let parsed = parse_gaql(&gaql)?;
    let validation = field_cache.validate_query_fields(&parsed.select_fields)?;

    if !validation.is_valid() {
        // Attempt to fix common issues
        let fixed_gaql = auto_fix_query(&gaql, &validation)?;
        return Ok(fixed_gaql);
    }

    Ok(gaql)
}
```

#### 4. Configuration Integration

**Add to `config.toml`:**

```toml
[profile_name]
# Existing fields...
field_metadata_cache = "~/.cache/mcc-gaql/field_metadata.json"
field_metadata_ttl_days = 7
natural_query_model = "gpt-4o-mini"  # Allow model selection
natural_query_temperature = 0.1
```

#### 5. CLI Enhancement

**New flags:**

```bash
# Refresh field metadata cache
mcc-gaql --refresh-field-cache

# Show available fields for a resource
mcc-gaql --show-fields campaign

# Validate GAQL without executing
mcc-gaql --validate "SELECT campaign.name FROM campaign"

# Natural query with field awareness (automatic)
mcc-gaql --natural-language "campaigns with CTR above 5% last week"
```

### Workflow for Claude Skill

**Scenario 1: Direct Natural Query**

```bash
# Claude Skill executes
mcc-gaql --profile prod --natural-language \
  "show me Performance Max campaigns with declining ROAS in last 14 days"

# Behind the scenes:
# 1. Load field cache (or fetch if stale)
# 2. RAG identifies:
#    - Resource: campaign
#    - Attributes: campaign.name, campaign.advertising_channel_type
#    - Metrics: metrics.conversions_value_per_cost (ROAS)
#    - Segments: segments.date
# 3. LLM generates validated GAQL
# 4. Execute and return results
```

**Scenario 2: Claude Skill with Metadata Access**

```bash
# Step 1: Skill queries available metrics for analysis
mcc-gaql --field-service \
  "SELECT name, data_type FROM google_ads_field
   WHERE category = 'METRIC' AND name LIKE '%conversion%'"

# Step 2: Skill constructs diagnostic queries based on metadata
mcc-gaql --natural-language \
  "compare conversion metrics week over week for Search campaigns"

# Step 3: Skill analyzes results and provides insights
```

**Scenario 3: Metadata Export for Skill Context**

```bash
# Generate metadata summary for Claude Skill knowledge base
mcc-gaql --export-field-metadata --format json > schema.json

# Claude Skill can reference this lightweight schema
# Instead of querying Fields Service repeatedly
```

### Advantages

✅ **Dynamic schema** - Always uses current Google Ads API field definitions
✅ **Validation** - Catches invalid queries before execution
✅ **Intelligent defaults** - Suggests relevant fields based on user intent
✅ **Existing infrastructure** - Builds on implemented features
✅ **Caching** - Fast after initial fetch, works offline
✅ **Accurate** - Field compatibility enforced by actual schema
✅ **Better error messages** - Can suggest valid alternatives
✅ **Maintenance-free** - Auto-updates from API
✅ **Dual use** - Enhances both CLI users and Claude Skills

### Disadvantages

❌ **Implementation complexity** - Requires new module and integration
❌ **API dependency** - Initial fetch requires Fields Service access
❌ **Cache staleness** - Need periodic refresh (mitigated by TTL)
❌ **OpenAI dependency** - Still requires API key (existing limitation)

### Estimated Effort

- **Field metadata module**: 12-16 hours
- **RAG enhancement**: 8-12 hours
- **Validation logic**: 8-10 hours
- **CLI integration**: 4-6 hours
- **Testing**: 8-10 hours
- **Documentation**: 4-6 hours
- **Total**: 44-60 hours (~1-1.5 weeks)

---

## Comparison Matrix

| Criteria | Option A: Reference Guide | Option B: Enhanced Natural Query |
|----------|---------------------------|----------------------------------|
| **Accuracy** | ⚠️ Depends on LLM reasoning | ✅ Schema-validated |
| **Up-to-date** | ❌ Manual maintenance | ✅ Auto-updates from API |
| **Implementation** | ✅ Simple (docs only) | ⚠️ Moderate complexity |
| **Validation** | ❌ None (runtime errors) | ✅ Pre-execution validation |
| **Offline capability** | ✅ Full (after doc creation) | ✅ With cache |
| **Token efficiency** | ❌ Large context overhead | ✅ Targeted metadata |
| **Error recovery** | ❌ Limited guidance | ✅ Suggests fixes |
| **Field discovery** | ⚠️ Manual search in docs | ✅ Programmatic queries |
| **Query quality** | ⚠️ Variable | ✅ Consistently high |
| **Maintenance** | ❌ Ongoing per API version | ✅ Automatic |
| **Learning curve** | ✅ Educational | ⚠️ More abstraction |
| **Claude Skill integration** | ⚠️ Context injection | ✅ Direct CLI integration |

---

## Recommendation: Option B (Enhanced Natural Query)

### Rationale

1. **Leverages existing infrastructure** - Both Fields Service and natural query features already implemented

2. **Superior accuracy** - Schema-aware query generation prevents invalid queries before execution

3. **Future-proof** - Automatically adapts to Google Ads API evolution

4. **Better user experience** - For both CLI users and Claude Skills
   - Intelligent field suggestions
   - Validation with helpful error messages
   - Faster iteration (fewer failed queries)

5. **Claude Skill optimization**:
   - Skill can use `--natural-language` directly without needing full schema in context
   - Can export metadata summary for Skill's knowledge base
   - Can use `--show-fields` for targeted field discovery
   - Validation prevents API quota waste on invalid queries

6. **Extensibility**:
   - Foundation for future features (autocomplete, query builder UI)
   - Can add query templates based on diagnostic patterns
   - Enables query optimization suggestions

### Implementation Approach

**Phase 1: Core Metadata (Weeks 1-2)**
- Implement `FieldMetadataCache` with disk persistence
- Integrate Fields Service API queries
- Add cache refresh CLI command
- Unit tests for cache operations

**Phase 2: Enhanced RAG (Weeks 2-3)**
- Extend `prompt2gaql.rs` with field-aware context
- Implement resource and field identification heuristics
- Enhance system prompt with schema injection
- Integration tests with real Fields Service data

**Phase 3: Validation (Week 3)**
- Implement GAQL parser for SELECT field extraction
- Create validation logic using field metadata
- Add auto-fix for common mistakes
- Add `--validate` CLI flag

**Phase 4: CLI Integration (Week 4)**
- Add configuration options
- Implement `--show-fields` command
- Add `--export-field-metadata` command
- Update documentation

**Phase 5: Testing & Documentation (Week 4)**
- End-to-end tests with Claude Skill scenarios
- Performance benchmarking
- Update README and DEVELOPER.md
- Create migration guide from Option A (if needed)

---

## Alternative: Hybrid Approach

For maximum flexibility, implement **both options**:

### Quick Start: Option A Reference Guide (Week 1)
- Create static documentation immediately
- Enables Claude Skill development to proceed
- Provides educational value

### Long-term: Option B Enhancement (Weeks 2-5)
- Implement enhanced natural query in parallel
- Gradually transition to schema-aware approach
- Deprecate static docs when enhancement is stable

**Benefits:**
- Unblocks Claude Skill development immediately
- Provides migration path
- Static docs serve as fallback if API unavailable

**Trade-offs:**
- More documentation to maintain (temporarily)
- Potential confusion during transition period

---

## Success Metrics

### Functional Metrics
- ✅ **Query success rate**: >95% of natural language queries generate valid GAQL
- ✅ **Schema coverage**: Cache includes all resources, attributes, metrics, segments
- ✅ **Validation accuracy**: Catches 100% of field selectability violations
- ✅ **Cache freshness**: Auto-refresh within configured TTL

### Performance Metrics
- ⚡ **Cache load time**: <100ms for cached metadata
- ⚡ **Field query time**: <500ms for Fields Service API call
- ⚡ **Natural query latency**: <3s including LLM call (existing baseline)
- ⚡ **Cache size**: <1MB on disk

### User Experience Metrics
- 📊 **Reduced query iterations**: Fewer failed queries due to validation
- 📊 **Claude Skill effectiveness**: Diagnostic accuracy and insight quality
- 📊 **Error message quality**: Clear suggestions for fixing invalid queries

---

## Risk Mitigation

### Risk 1: Fields Service API Changes
**Mitigation:**
- Version-specific cache files
- Graceful degradation to cookbook-only mode
- Error handling for API schema changes

### Risk 2: Cache Staleness
**Mitigation:**
- Configurable TTL with sensible defaults
- Manual refresh command
- Warning message when cache is stale

### Risk 3: OpenAI API Dependency
**Mitigation:**
- Document configuration for alternative models
- Future: Support local LLMs (llama.cpp integration)
- Fallback to direct GAQL if API unavailable

### Risk 4: Field Metadata Size
**Mitigation:**
- Lazy loading (only fetch when needed)
- Incremental cache updates
- Option to cache only commonly-used resources

---

## Open Questions for Review

1. **Cache strategy**: Should we cache all fields upfront or lazy-load by resource?

2. **Model selection**: Should we support alternative LLM providers (Anthropic Claude API, local models)?

3. **Validation strictness**: Should validation be blocking (error) or advisory (warning)?

4. **Metadata export format**: What format is most useful for Claude Skills? (JSON, YAML, Markdown)

5. **Cookbook integration**: Should we auto-generate cookbook entries from field metadata?

6. **Error correction**: How aggressive should auto-fix be? (Conservative vs. intelligent guessing)

7. **Performance**: Should we implement pagination for large field metadata results?

8. **Versioning**: How should we handle multiple Google Ads API versions?

---

## Next Steps

1. **Review this spec** and provide feedback on:
   - Option A vs. B preference (or hybrid approach)
   - Implementation priorities
   - Claude Skill integration requirements
   - Success criteria

2. **Define scope** for initial implementation:
   - Minimum viable features
   - Nice-to-have enhancements
   - Future roadmap

3. **Prototype validation** (if approved):
   - Quick proof-of-concept for field metadata caching
   - Test RAG enhancement with sample queries
   - Benchmark performance impact

4. **Create implementation tickets** based on approved design

---

## Appendix A: Example Field Metadata Schema

```json
{
  "last_updated": "2025-11-06T10:00:00Z",
  "api_version": "v18",
  "fields": [
    {
      "name": "campaign.name",
      "category": "ATTRIBUTE",
      "resource": "campaign",
      "data_type": "STRING",
      "selectable": true,
      "filterable": true,
      "selectable_with": ["*"],
      "description": "The name of the campaign"
    },
    {
      "name": "metrics.impressions",
      "category": "METRIC",
      "data_type": "INT64",
      "selectable": true,
      "filterable": false,
      "requires_segments_or_resources": true,
      "description": "Count of how often your ad has appeared on a search results page"
    },
    {
      "name": "segments.date",
      "category": "SEGMENT",
      "data_type": "DATE",
      "selectable": true,
      "filterable": true,
      "selectable_with": ["*"],
      "description": "Date to which metrics apply (YYYY-MM-DD)"
    }
  ],
  "resources": {
    "campaign": {
      "attributes": ["campaign.name", "campaign.status", "campaign.advertising_channel_type"],
      "compatible_resources": ["ad_group", "customer"],
      "description": "Campaign-level configuration and performance"
    }
  }
}
```

---

## Appendix B: Enhanced RAG Context Example

```
GOOGLE ADS SCHEMA FOR: "campaigns with CTR above 5% last week"

RELEVANT RESOURCES:
- campaign: Campaign-level configuration and performance

RELEVANT FIELDS:
Attributes:
  - campaign.name (STRING, selectable, filterable)
  - campaign.status (ENUM, selectable, filterable)

Metrics:
  - metrics.ctr (DOUBLE, selectable, requires grouping)
  - metrics.impressions (INT64, selectable, requires grouping)
  - metrics.clicks (INT64, selectable, requires grouping)

Segments:
  - segments.date (DATE, selectable, filterable)

EXAMPLE QUERIES:
1. Query: "campaigns in last 30 days"
   GAQL: SELECT campaign.name, metrics.impressions FROM campaign
         WHERE segments.date DURING LAST_30_DAYS

2. Query: "high performing campaigns"
   GAQL: SELECT campaign.name, metrics.ctr, metrics.clicks FROM campaign
         WHERE metrics.clicks > 100 ORDER BY metrics.ctr DESC

VALIDATION RULES:
- CTR requires campaign.name or segments for grouping
- segments.date DURING LAST_7_DAYS for "last week"
- WHERE clause supports filterable fields only
- Metrics cannot be in WHERE (use HAVING for post-aggregation filters)

USER QUERY: "campaigns with CTR above 5% last week"

Generate valid GAQL query:
```

---

## Appendix C: Claude Skill Integration Examples

### Example 1: Performance Diagnostic Skill

```python
# Claude Skill pseudocode
def diagnose_ctr_drop(account_id: str, date_range: str) -> str:
    """Diagnose sudden CTR drops."""

    # Use natural query with metadata awareness
    query = f"""mcc-gaql --profile prod --customer-id {account_id}
                --natural-language "campaigns with CTR by device and date
                for {date_range}, show clicks and impressions"
    """

    results = execute_command(query)

    # Claude analyzes results
    analysis_prompt = f"""
    Analyze this Google Ads data for CTR anomalies:
    {results}

    Identify:
    1. Which campaigns show CTR decline
    2. Device-specific issues
    3. Date patterns
    4. Possible causes
    """

    return claude_analyze(analysis_prompt)
```

### Example 2: Metadata-Driven Query Construction

```python
def find_underperforming_assets(account_id: str) -> str:
    """Find underperforming ad assets."""

    # Step 1: Discover available asset metrics
    metadata = execute_command(
        "mcc-gaql --show-fields asset --format json"
    )

    # Step 2: Claude selects relevant metrics
    selected_metrics = claude_select_metrics(
        metadata,
        goal="identify poor asset performance"
    )

    # Step 3: Construct query using natural language
    query = f"""mcc-gaql --customer-id {account_id} --natural-language
                "show {', '.join(selected_metrics)} for all assets
                 with impressions > 1000 last 30 days"
    """

    results = execute_command(query)
    return claude_analyze(results)
```

---

**End of Design Specification**

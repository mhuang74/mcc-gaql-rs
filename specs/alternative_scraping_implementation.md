# Alternative Scraping Implementation Plan: Proto-Based Metadata Enrichment

**Version:** 1.0
**Date:** 2026-03-12
**Status:** Draft
**Approach:** Option 2 (Hybrid: Proto Files + LLM Enrichment)

---

## Executive Summary

Based on user input and codebase investigation, we will implement **Option 2 (Hybrid)** - extracting field documentation from Google's official proto files (leveraging the `googleads-rs` crate's bundled proto files), with LLM-based enrichment for fields lacking sufficient proto documentation.

**Key Discovery:** The `googleads-rs` crate includes 955 proto files with complete field-level documentation at build time, eliminating the need for network fetching during scraping.

---

## Goals

1. Replace the non-functional HTML scraper with a robust proto file parser
2. Extract authoritative field descriptions directly from Google's proto definitions
3. Use LLM to enrich terse or missing proto documentation
4. Populate `FieldMetadata.description` and `ResourceMetadata.description` for RAG
5. Support only API V23 (current `googleads-rs` version)

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     PROTO METADATA ENRICHMENT PIPELINE                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────┐      ┌─────────────────────┐                      │
│  │ googleads-rs crate  │──────▶│ Proto File Parser   │                      │
│  │ (proto/ directory)  │      │ (proto_parser.rs)   │                      │
│  └─────────────────────┘      └──────────┬──────────┘                      │
│                                          │                                  │
│                              ┌───────────▼───────────┐                     │
│                              │ Field Doc Cache       │                     │
│                              │ (proto_docs_cache.rs) │                     │
│                              └───────────┬───────────┘                     │
│                                          │                                  │
│                    ┌─────────────────────┼─────────────────────┐           │
│                    │                     │                     │           │
│           ┌────────▼────────┐   ┌────────▼────────┐   ┌────────▼────────┐  │
│           │ Well-documented │   │ Terse/No docs   │   │ Resource docs   │  │
│           │ fields (direct) │   │ (LLM enrich)    │   │ (from message)  │  │
│           └────────┬────────┘   └────────┬────────┘   └────────┬────────┘  │
│                    │                     │                     │           │
│                    └─────────────────────┼─────────────────────┘           │
│                                          │                                  │
│                              ┌───────────▼───────────┐                     │
│                              │ Merge into            │                     │
│                              │ FieldMetadataCache    │                     │
│                              │ (description field)   │                     │
│                              └───────────┬───────────┘                     │
│                                          │                                  │
│                              ┌───────────▼───────────┐                     │
│                              │ RAG Embedding         │                     │
│                              │ Generation            │                     │
│                              └───────────────────────┘                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Detailed Design

### 1. Proto File Access Strategy

**Finding:** The `googleads-rs` crate includes proto files in its git repository at:
```
$CARGO_HOME/git/checkouts/googleads-rs-*/proto/google/ads/googleads/v23/
```

**Implementation:**
- At build time, locate the `googleads-rs` proto directory via `CARGO_MANIFEST_DIR` environment variable
- Use a fallback to cargo git cache path if needed
- Include 542 relevant proto files (182 resources + 360 enums) from V23

```rust
// build.rs or runtime discovery
fn find_googleads_proto_dir() -> Option<PathBuf> {
    // Strategy 1: Check if googleads-rs is a path dependency
    // Strategy 2: Find in cargo git cache
    // Strategy 3: Fetch from GitHub as fallback
}
```

### 2. Proto File Parser (`proto_parser.rs`)

**Purpose:** Parse proto files to extract field-level documentation comments.

**Key Features:**
- Lightweight parser (regex-based) - no heavy protobuf parsing crates needed
- Extract `//` comments preceding field definitions
- Handle message-level comments for resource descriptions
- Map proto field names to GAQL field names (e.g., `Campaign.name` → `campaign.name`)

**Data Structures:**

```rust
/// Parsed documentation for a single proto message (resource)
#[derive(Debug, Clone)]
pub struct ProtoMessageDoc {
    pub message_name: String,  // e.g., "Campaign"
    pub description: String,   // Message-level comment
    pub fields: Vec<ProtoFieldDoc>,
}

/// Parsed documentation for a single field
#[derive(Debug, Clone)]
pub struct ProtoFieldDoc {
    pub field_name: String,      // e.g., "name", "status"
    pub field_number: u32,
    pub description: String,     // Concatenated comment lines
    pub field_behavior: Vec<FieldBehavior>,  // OUTPUT_ONLY, REQUIRED, etc.
    pub type_name: String,       // e.g., "string", "CampaignStatus"
    pub is_enum: bool,
    pub enum_type: Option<String>,  // Full enum type path for lookup
}

#[derive(Debug, Clone)]
pub enum FieldBehavior {
    Immutable,
    OutputOnly,
    Required,
    Optional,
}
```

**Parsing Algorithm:**

```rust
pub fn parse_proto_file(content: &str) -> Vec<ProtoMessageDoc> {
    // 1. Split into lines, track line numbers
    // 2. Identify message blocks: `message MessageName {`
    // 3. Extract message-level comment (lines before `message` keyword)
    // 4. Within each message:
    //    a. Identify field definitions: `type name = number;`
    //    b. Extract preceding comment lines (lines starting with `//`)
    //    c. Parse field behavior annotations: `[(google.api.field_behavior) = OUTPUT_ONLY]`
    // 5. Return Vec<ProtoMessageDoc>
}
```

**Example Input/Output:**

```protobuf
// Input: campaign.proto
// The status of the campaign.
// When a new campaign is added, the default value is ENABLED.
google.ads.googleads.v23.enums.CampaignStatusEnum.CampaignStatus status = 4;
```

```rust
// Output: ProtoFieldDoc
ProtoFieldDoc {
    field_name: "status".to_string(),
    field_number: 4,
    description: "The status of the campaign. When a new campaign is added, the default value is ENABLED.".to_string(),
    field_behavior: vec![],
    type_name: "CampaignStatus".to_string(),
    is_enum: true,
    enum_type: Some("google.ads.googleads.v23.enums.CampaignStatusEnum.CampaignStatus".to_string()),
}
```

### 3. Enum Value Documentation

**Challenge:** Enum values are defined in separate files and need cross-referencing.

**Implementation:**

```rust
/// Parse enum definitions from Enums/ directory
pub fn parse_enum_file(content: &str) -> Vec<ProtoEnumDoc> {
    // Extract enum name and values with comments
}

#[derive(Debug, Clone)]
pub struct ProtoEnumDoc {
    pub enum_name: String,
    pub description: String,
    pub values: Vec<EnumValueDoc>,
}

#[derive(Debug, Clone)]
pub struct EnumValueDoc {
    pub name: String,        // e.g., "ENABLED"
    pub number: i32,
    pub description: String,
}
```

**Cross-Reference Strategy:**
- Parse all enum files first, build lookup table by full type name
- When parsing resource fields, look up enum docs by `enum_type` path
- Merge enum value descriptions into `FieldMetadata.enum_values` as rich strings: `"ENABLED: Campaign is serving ads"`

### 4. LLM Enrichment Pipeline (`llm_enricher.rs`)

**Purpose:** Enrich terse or missing proto documentation using LLM.

**Trigger Conditions:**
- Proto comment is empty or < 20 characters
- Proto comment is generic (e.g., "The name of the campaign")
- Field is a commonly-used query field (metrics, key attributes)

**LLM Prompt Template:**

```rust
const ENRICHMENT_PROMPT: &str = r#"
You are a technical documentation writer for the Google Ads API.
Given the following field information, write a comprehensive 2-3 sentence description
suitable for RAG (Retrieval-Augmented Generation) search.

Field: {field_name}
Resource: {resource_name}
Data Type: {data_type}
Is Metric: {is_metric}
Is Segment: {is_segment}
Existing Proto Comment: "{proto_comment}"
Enum Values: {enum_values}
Selectable With: {selectable_with}
Filterable: {filterable}
Sortable: {sortable}

Requirements:
1. Explain what the field represents in business terms
2. Mention common use cases or why someone would query this field
3. For enums: Include the meaning of each value
4. Mention if the field is filterable or sortable (if relevant)
5. Keep it factual - do not hallucinate

Response format: Return ONLY the description text, no markdown, no JSON.
"#;
```

**Batching Strategy:**
- Group fields by resource (e.g., all Campaign fields)
- Send 10-20 fields per LLM call to reduce token overhead
- Cache LLM responses to avoid re-processing

### 5. Field Name Mapping

**Challenge:** Proto field names don't always match GAQL field names.

**Mapping Rules:**

| Proto Name | GAQL Name | Notes |
|------------|-----------|-------|
| `Campaign.name` | `campaign.name` | Lowercase resource name |
| `Campaign.status` | `campaign.status` | Direct mapping |
| `AdGroup.campaign` | `ad_group.campaign` | Snake_case resource name |
| `metrics.clicks` | `metrics.clicks` | Same in proto and GAQL |
| `segments.device` | `segments.device` | Same in proto and GAQL |

**Implementation:**

```rust
pub fn proto_to_gaql_field_name(resource: &str, field: &str) -> String {
    // Resource names are snake_case in GAQL
    // Proto message names are PascalCase
    let resource_snake = pascal_to_snake_case(resource);
    format!("{}.{}", resource_snake, field)
}

fn pascal_to_snake_case(s: &str) -> String {
    // AdGroup → ad_group
    // Campaign → campaign
}
```

### 6. Proto Docs Cache (`proto_docs_cache.rs`)

**Purpose:** Cache parsed proto documentation to avoid re-parsing on every run.

**Cache Structure:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoDocsCache {
    pub parsed_at: DateTime<Utc>,
    pub api_version: String,
    pub googleads_rs_commit: String,  // Track which version of proto files
    /// Map: "resource.field" → field documentation
    pub field_docs: HashMap<String, ProtoFieldDoc>,
    /// Map: "resource" → message documentation
    pub resource_docs: HashMap<String, String>,
    /// Map: "enum_type" → enum documentation
    pub enum_docs: HashMap<String, ProtoEnumDoc>,
}
```

**Cache Location:** `~/.cache/mcc-gaql/proto_docs_v23.json`

**TTL Strategy:**
- Parse proto files once per `googleads-rs` version/commit
- Check `googleads-rs` version in Cargo.lock to detect updates
- Invalidate cache when `googleads-rs` is updated

### 7. Integration with Existing Enrichment Pipeline

**Current Flow (to be replaced):**
```
scraper.rs → scraped_docs.json → enricher.rs → FieldMetadata.description
```

**New Flow:**
```
proto_parser.rs → proto_docs_cache.json ──┐
                                          ├──▶ hybrid_enricher.rs ──▶ FieldMetadata.description
Fields Service API ──▶ field_metadata.json ┘
```

**Hybrid Enricher Logic:**

```rust
pub async fn enrich_field_metadata(
    field_cache: &mut FieldMetadataCache,
    proto_cache: &ProtoDocsCache,
    llm_config: &LlmConfig,
) -> Result<()> {
    for (field_name, field_meta) in &mut field_cache.fields {
        // 1. Try to get description from proto cache
        let proto_doc = proto_cache.get_field_doc(field_name);

        let description = match proto_doc {
            Some(doc) if doc.description.len() > 50 => {
                // Well-documented proto comment - use directly
                doc.description.clone()
            }
            Some(doc) => {
                // Terse comment - enrich with LLM
                enrich_with_llm(field_meta, &doc.description, llm_config).await?
            }
            None => {
                // No proto comment - generate from field metadata
                generate_from_metadata(field_meta, llm_config).await?
            }
        };

        field_meta.description = Some(description);
    }

    // Also populate resource-level descriptions
    for (resource_name, res_meta) in &mut field_cache.resource_metadata.iter_mut().flatten() {
        if let Some(desc) = proto_cache.get_resource_doc(resource_name) {
            res_meta.description = Some(desc);
        }
    }

    Ok(())
}
```

---

## Implementation Phases

### Phase 1: Proto Parser (Days 1-2)

**Tasks:**
1. Create `crates/mcc-gaql-gen/src/proto_parser.rs`
   - Implement regex-based proto comment parser
   - Handle message and field-level comments
   - Extract field behavior annotations
   - Unit tests with sample proto snippets

2. Create `crates/mcc-gaql-gen/src/proto_docs_cache.rs`
   - Cache structure and serialization
   - Cache loading/saving
   - Version detection for invalidation

3. Create `crates/mcc-gaql-gen/src/proto_locator.rs`
   - Locate `googleads-rs` proto directory
   - Handle path dependency, git dependency, and fallback cases

**Deliverable:** Standalone proto parser that can extract documentation from all 542 V23 proto files.

### Phase 2: Enum Documentation (Day 2-3)

**Tasks:**
1. Extend proto parser to handle enum definitions
2. Build enum lookup table by full type path
3. Cross-reference enum fields with their value documentation
4. Format enum values as `"VALUE: Description"` strings

**Deliverable:** Complete enum documentation extraction for all 360 enum files.

### Phase 3: LLM Enrichment (Days 3-4)

**Tasks:**
1. Create `crates/mcc-gaql-gen/src/llm_enricher.rs`
   - Terse comment detection
   - LLM prompt construction
   - Batching and caching
   - Rate limiting

2. Integrate with existing `rig-core` LLM infrastructure

3. Implement fallback description generation for fields without proto docs

**Deliverable:** LLM enrichment pipeline that can process all fields with terse/missing documentation.

### Phase 4: Integration (Day 4-5)

**Tasks:**
1. Create `crates/mcc-gaql-gen/src/hybrid_enricher.rs`
   - Merge proto docs with FieldMetadataCache
   - Handle field name mapping
   - Populate both field and resource descriptions

2. Update `mcc-gaql-gen` CLI:
   - Replace `--scrape` with `--parse-protos`
   - Add `--enrich` flag for LLM enrichment
   - Add `--proto-cache-ttl` option

3. Update `main.rs` to use new pipeline

**Deliverable:** Working end-to-end pipeline from proto files to enriched metadata.

### Phase 5: Testing & Validation (Day 5-6)

**Tasks:**
1. Unit tests for proto parser
2. Integration tests comparing output to expected documentation
3. Sample validation: manually verify 20-30 key fields
4. Performance testing: ensure parsing completes in < 30 seconds

**Deliverable:** Test suite with > 80% coverage on new modules.

### Phase 6: Documentation & Migration (Day 6-7)

**Tasks:**
1. Update user-facing documentation
2. Write migration guide from HTML scraper to proto parser
3. Deprecate old `scraper.rs` module
4. Update CI/CD if needed

**Deliverable:** Complete documentation and deprecated old scraper.

---

## Files to Create/Modify

### New Files

| File | Purpose | Lines (est) |
|------|---------|-------------|
| `crates/mcc-gaql-gen/src/proto_locator.rs` | Find googleads-rs proto directory | 80 |
| `crates/mcc-gaql-gen/src/proto_parser.rs` | Parse proto comments | 300 |
| `crates/mcc-gaql-gen/src/proto_docs_cache.rs` | Cache parsed proto docs | 150 |
| `crates/mcc-gaql-gen/src/llm_enricher.rs` | LLM-based enrichment | 200 |
| `crates/mcc-gaql-gen/src/hybrid_enricher.rs` | Merge proto + LLM into FieldMetadata | 150 |

### Modified Files

| File | Changes |
|------|---------|
| `crates/mcc-gaql-gen/src/main.rs` | Replace scraper calls with proto parser; update CLI args |
| `crates/mcc-gaql-gen/src/scraper.rs` | Mark as deprecated; add deprecation warning |
| `crates/mcc-gaql-gen/Cargo.toml` | Add `regex` and `walkdir` dependencies |
| `crates/mcc-gaql-common/src/field_metadata.rs` | Add `merge_proto_descriptions()` helper |

### Deleted Files (Future)

| File | Reason |
|------|--------|
| `crates/mcc-gaql-gen/src/scraper.rs` | Replaced by proto parser (after migration period) |

---

## Dependencies

### New Dependencies (mcc-gaql-gen)

```toml
[dependencies]
# Proto parsing (regex-based, lightweight)
regex = "1.10"
walkdir = "2"  # Already present via googleads-rs

# Existing LLM deps (already present)
# rig-core = "..."
```

### No Heavy Protobuf Crates Needed

We intentionally avoid:
- `protobuf-parse` - overkill for comment extraction
- `prost-build` - requires compilation, not needed for doc extraction

---

## Risk Assessment & Mitigation

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Proto file format changes | Low | Medium | Regex patterns are flexible; add unit tests for edge cases |
| googleads-rs proto directory not found | Medium | High | Implement 3-tier fallback: path dep → git cache → GitHub fetch |
| LLM enrichment too slow/expensive | Medium | Medium | Cache LLM responses; only enrich terse docs |
| Field name mapping edge cases | Medium | Medium | Extensive test cases for resource/field name conversion |
| Proto files lack comments for key fields | Low | High | LLM backfill for missing descriptions |

---

## Success Metrics

| Metric | Current (HTML) | Target (Proto) | Measurement |
|--------|----------------|----------------|-------------|
| Fields with descriptions | ~3% (generic only) | > 95% | Count non-null descriptions in cache |
| Avg description quality | Poor | Good | Manual sample review of 50 fields |
| Parse time | N/A (fails) | < 30s | `time mcc-gaql-gen parse-protos` |
| Cache size | ~50KB (useless) | ~2-5MB | `ls -lh ~/.cache/mcc-gaql/` |
| LLM token usage | N/A | < $5/run | Track via LLM provider dashboard |

---

## Open Questions (Resolved)

| Question | Resolution |
|----------|------------|
| Which approach? | Option 2 (Hybrid: Proto + LLM) |
| API versions? | V23 only (current googleads-rs version) |
| Cache proto files? | Yes, parsed documentation cached in JSON |
| Missing proto comments? | LLM backfill |
| Leverage googleads-rs? | **Yes** - use proto files from crate's git checkout |

---

## Proto File Inventory

**Location:** `googleads-rs/proto/google/ads/googleads/v23/`

| Directory | Count | Purpose |
|-----------|-------|---------|
| `resources/` | 182 | Resource definitions (Campaign, AdGroup, etc.) |
| `enums/` | 360 | Enum definitions (CampaignStatus, etc.) |
| `services/` | ~50 | Service definitions (not needed for docs) |
| `common/` | ~50 | Shared types (not directly queryable) |
| **Total** | **~642** | Files to parse |

**Key Resource Files:**
- `resources/campaign.proto`
- `resources/ad_group.proto`
- `resources/ad_group_ad.proto`
- `resources/ad_group_criterion.proto`
- `resources/customer.proto`
- `resources/campaign_budget.proto`
- `resources/ad.proto`

**Key Enum Files:**
- `enums/campaign_status.proto`
- `enums/advertising_channel_type.proto`
- `enums/ad_group_status.proto`
- `enums/criterion_type.proto`

---

## Appendix A: Proto Comment Patterns

### Pattern 1: Single-line comment
```protobuf
// The name of the campaign.
string name = 5;
```

### Pattern 2: Multi-line comment
```protobuf
// Output only. The primary status of the campaign.
//
// Provides insight into why a campaign is not serving or not serving
// optimally. Modification to the campaign and its related entities might take
// a while to be reflected in this status.
CampaignPrimaryStatus primary_status = 21;
```

### Pattern 3: Comment with field behavior
```protobuf
// Immutable. The resource name of the campaign.
// Campaign resource names have the form:
// `customers/{customer_id}/campaigns/{campaign_id}`
string resource_name = 1 [
  (google.api.field_behavior) = IMMUTABLE,
  (google.api.resource_reference) = {...}
];
```

### Pattern 4: Enum with value comments
```protobuf
// The possible statuses of a campaign.
enum CampaignStatus {
  // Not specified.
  UNSPECIFIED = 0;

  // Used for return value only. Represents value unknown in this version.
  UNKNOWN = 1;

  // Campaign is currently serving ads depending on budget information.
  ENABLED = 2;

  // Campaign has been paused by the user.
  PAUSED = 3;

  // Campaign has been removed.
  REMOVED = 4;
}
```

---

## Appendix B: Field Name Conversion Examples

| Proto Message | Proto Field | GAQL Field |
|---------------|-------------|------------|
| Campaign | name | campaign.name |
| Campaign | status | campaign.status |
| AdGroup | campaign | ad_group.campaign |
| AdGroupAd | ad_group | ad_group_ad.ad_group |
| CampaignBudget | amount_micros | campaign_budget.amount_micros |
| AdGroupCriterion | keyword | ad_group_criterion.keyword |
| Metrics | clicks | metrics.clicks |
| Segments | device | segments.device |

---

*End of Document*

Now I have a complete picture. Here is the comprehensive plan:

---

## Plan: Google Ads Comprehensive Metadata Pipeline

### Problem

The existing `FieldMetadata` struct holds only structural data from the Fields Service API (name, category, data_type, selectable, filterable, sortable). The LanceDB vector store embeds these bare facts, giving the LLM little contextual signal to choose the right fields, understand enum values, or respect resource relationships. The result is imprecise GAQL generation.

### Goal

Build a multi-stage scraping and synthesis pipeline that enriches every Google Ads field with:
- **Human-readable descriptions** (what the field means, how to interpret its values)
- **Enum value documentation** (valid values and their semantics)
- **Resource relationship graph** (parent/child, compatible co-query resources)
- **Usage examples and patterns** (when to use, common filter expressions)

The enriched data lands back in the existing `FieldMetadata` / `FieldMetadataCache` structs and LanceDB vector store, improving RAG retrieval quality with no changes to the query execution path.

---

### Architecture Overview

```
Stage 1: Structural Harvest          Stage 2: Doc Scrape              Stage 3: LLM Synthesis
──────────────────────────────       ──────────────────────────       ──────────────────────────────
Google Ads Fields Service API   →    Google Ads API Reference   →     LLM (Claude/OpenAI)
  - All fields, categories            (developers.google.com)          - Merge structural + scraped
  - data_type, selectable             - Field descriptions              - Generate description
  - filterable, sortable              - Enum value tables               - Synthesize enum docs
  - selectable_with list              - Resource overview text          - Infer usage patterns
                                      - Resource relationship info      - Produce relationship notes
         ↓
  field_metadata.json (raw)                   ↓
                                    scraped_docs.json (raw HTML→MD)
                                                                              ↓
                                                          enriched_field_metadata.json
                                                                              ↓
                                                          Stage 4: Rebuild LanceDB vector store
                                                            (richer text → better embeddings)
```

---

### Stage 1 — Structural Harvest (extends existing code)

**What:** Pull the full field list from Google Ads Fields Service, including the `selectable_with` list (currently not fetched).

**Changes to `field_metadata.rs`:**

1. **Extend `FieldMetadata`** with new optional fields:
   ```rust
   pub struct FieldMetadata {
       // --- existing ---
       pub name: String,
       pub category: String,
       pub data_type: String,
       pub selectable: bool,
       pub filterable: bool,
       pub sortable: bool,
       pub metrics_compatible: bool,
       pub resource_name: Option<String>,
       // --- new ---
       pub selectable_with: Vec<String>,   // compatible peer fields from Fields Service
       pub description: Option<String>,    // populated by Stage 3
       pub enum_values: Vec<String>,       // populated by Stage 2/3
       pub usage_notes: Option<String>,    // populated by Stage 3
       pub related_resources: Vec<String>, // populated by Stage 2/3
   }
   ```

2. **Update `fetch_from_api`** to request `selectable_with` in the GAQL query sent to GoogleAdsFieldService.

3. **New CLI flag** `--refresh-metadata` triggers a full re-harvest + re-enrich cycle.

**Output:** `~/.cache/mcc-gaql/field_metadata_raw.json` (structural only, same format as today but with `selectable_with`).

---

### Stage 2 — Web Scrape Google Ads API Reference

**What:** A new standalone Rust binary (or tokio task triggered by `--refresh-metadata`) that scrapes the Google Ads API reference docs.

**Target URLs (pattern):**
```
https://developers.google.com/google-ads/api/reference/rpc/v{VERSION}/resources/{Resource}
https://developers.google.com/google-ads/api/reference/rpc/v{VERSION}/enums/{EnumName}
```

**New file: `src/metadata_scraper.rs`**

Key responsibilities:
- For each resource discovered in Stage 1, fetch its reference page
- Parse the HTML to extract:
  - Resource-level description paragraph
  - Per-field description text
  - Enum pages linked from ENUM-typed fields → capture all valid values + their descriptions
- Convert to clean Markdown/plain text (strip nav, headers, code samples)
- Store in `~/.cache/mcc-gaql/scraped_docs.json` keyed by field name

**Implementation notes:**
- Use `reqwest` (already a transitive dep via `tonic`) for HTTP
- Rate-limit with a configurable delay (default 500ms between requests) to respect robots.txt
- Cache scraped pages with their own TTL (default 30 days, separately from the field metadata TTL)
- Gracefully skip pages that 404 or time out (not all fields have dedicated pages)

**Output:** `~/.cache/mcc-gaql/scraped_docs.json`
```json
{
  "campaign.name": {
    "description": "The name of the campaign. This field is required and should not be empty...",
    "enum_values": []
  },
  "campaign.status": {
    "description": "The status of the campaign.",
    "enum_values": ["ENABLED", "PAUSED", "REMOVED"]
  },
  "campaign.advertising_channel_type": {
    "description": "The primary serving target for ads within the campaign.",
    "enum_values": ["SEARCH", "DISPLAY", "SHOPPING", "HOTEL", "VIDEO", "MULTI_CHANNEL", "LOCAL", "SMART", "PERFORMANCE_MAX", "LOCAL_SERVICES", "DISCOVERY", "TRAVEL"]
  }
}
```

---

### Stage 3 — LLM Synthesis

**What:** For each field, feed the scraped docs + structural metadata into an LLM to produce a short, contextual description optimized for RAG retrieval.

**New file: `src/metadata_enricher.rs`**

The enricher calls the configured LLM (same `LlmConfig` as `prompt2gaql.rs`) with a prompt per field (batched to stay within rate limits):

```
You are documenting Google Ads API fields for a query assistant.

Field: campaign.status
Category: ATTRIBUTE
Data type: ENUM
Selectable: true  Filterable: true  Sortable: true
Raw documentation: "The status of the campaign. [ENABLED, PAUSED, REMOVED]"
Compatible with: campaign.name, segments.date, metrics.impressions, ...

Write a 2-3 sentence description that explains:
1. What this field represents and when to use it in a GAQL query
2. Valid values and their meaning (if ENUM)
3. Any filtering or sorting gotchas

Keep it dense and useful for a query-generation assistant. No markdown headers.
```

**Output per field (merged into `FieldMetadata`):**
- `description`: synthesized plain-text description
- `enum_values`: confirmed list with brief per-value meanings
- `usage_notes`: filtering/sorting tips, common patterns
- `related_resources`: parent resource + compatible co-selectable resources

**Batching strategy:**
- Group fields by resource (campaign fields together, ad_group fields together, etc.)
- Emit one LLM call per resource group to provide cross-field context
- Estimated: ~70 resource groups × ~5 fields/call average = ~350 LLM calls for full enrichment
- Progress bar using `indicatif` (already available transitively)

**Output:** `~/.cache/mcc-gaql/field_metadata_enriched.json` — the `FieldMetadata` JSON with all new fields populated.

---

### Stage 4 — Rebuild LanceDB Vector Store

**What:** After enrichment, rebuild the LanceDB `field_metadata` table with richer embedding text.

**Changes to `lancedb_utils.rs`:**

Update `fields_to_record_batch` to build the embedding document text from all enriched fields:

```
Current embedding text:
  "campaign.name: ATTRIBUTE, STRING, selectable, filterable"

New embedding text:
  "campaign.name [ATTRIBUTE, STRING]: The name of the campaign as it appears in reports
   and the Google Ads UI. Use in SELECT to label results. Filterable with = and LIKE.
   Related: campaign.status, campaign.id. Resource: campaign."
```

This 5-10× richer text dramatically improves cosine similarity when the user asks something like "show me campaign names and their budgets" — the embedding for `campaign.name` now strongly matches "campaign names".

---

### Stage 5 — Resource Relationship Documentation

Beyond per-field data, build a resource-level index that captures:
- **Parent/child hierarchy**: `customer → campaign → ad_group → ad_group_criterion`
- **Compatible co-query resources**: which resources can appear in the same FROM clause or be implicitly joined
- **Recommended fields per use case**: "for budget analysis, start with campaign + campaign_budget"

**New struct in `field_metadata.rs`:**
```rust
pub struct ResourceMetadata {
    pub name: String,                       // e.g. "campaign"
    pub description: Option<String>,        // scraped + synthesized
    pub parent_resource: Option<String>,    // e.g. "customer"
    pub child_resources: Vec<String>,       // e.g. ["ad_group", "campaign_budget"]
    pub selectable_with: Vec<String>,       // from Fields Service selectable_with on RESOURCE fields
    pub common_use_cases: Vec<String>,      // LLM synthesized
    pub key_attributes: Vec<String>,        // top 5-10 most useful attributes
    pub key_metrics: Vec<String>,           // top 5-10 most useful metrics
}

// Add to FieldMetadataCache:
pub resource_metadata: Option<HashMap<String, ResourceMetadata>>,
```

---

### Pipeline CLI Integration

```bash
# Full pipeline: harvest → scrape → synthesize → rebuild vectors
mcc-gaql --refresh-metadata

# Harvest only (fast, no LLM, no web scraping)
mcc-gaql --refresh-field-cache          # existing flag, now also fetches selectable_with

# Inspect enriched metadata
mcc-gaql --show-fields campaign         # now shows descriptions, enum values
mcc-gaql --show-resources               # shows resource hierarchy

# Export enriched metadata for external tools / Claude Skills
mcc-gaql --export-field-metadata --format json > enriched_schema.json
```

---

### Data Flow Summary

```
[Fields Service API]
        │  name, category, data_type, selectable, filterable, sortable, selectable_with
        ▼
field_metadata_raw.json
        │
        ├──────────────────────────────────────────────────────────┐
        │                                                          │
        ▼                                                          ▼
[Web Scraper]                                              [existing cache]
developers.google.com/google-ads/api/reference/...        (skip if not stale)
        │  field descriptions, enum tables, resource text
        ▼
scraped_docs.json
        │
        ▼
[LLM Enricher]  ← LlmConfig (same as prompt2gaql)
        │  synthesized descriptions, enum meanings, usage notes, relationships
        ▼
field_metadata_enriched.json  ←→  FieldMetadataCache (runtime struct)
        │
        ▼
[LanceDB rebuild]
  field_metadata table: richer text → better embeddings → better RAG retrieval
```

---

### File Changes Summary

| File | Change |
|---|---|
| `src/field_metadata.rs` | Extend `FieldMetadata` with `selectable_with`, `description`, `enum_values`, `usage_notes`, `related_resources`; add `ResourceMetadata`; update `fetch_from_api` to request `selectable_with`; update `export_summary` |
| `src/lancedb_utils.rs` | Update `fields_to_record_batch` to build richer embedding text from all enriched fields |
| `src/metadata_scraper.rs` | **New**: HTTP scraper for Google Ads API reference pages |
| `src/metadata_enricher.rs` | **New**: LLM batch enrichment, merges scraped docs into `FieldMetadata` |
| `src/args.rs` | Add `--refresh-metadata`, `--show-resources` flags |
| `src/main.rs` | Wire new pipeline stages into `--refresh-metadata` flow |
| `src/config.rs` | Add `scrape_cache_ttl_days`, `enrich_batch_size` config options |
| `resources/` | **New**: `google_ads_resource_relationships.toml` — manually curated resource hierarchy as a fallback/seed for the LLM |

---

### Implementation Phases

| Phase | Scope | Deliverable |
|---|---|---|
| **1** | Extend `FieldMetadata` struct + update `fetch_from_api` for `selectable_with` | Richer structural harvest, backward-compatible JSON cache |
| **2** | `metadata_scraper.rs` + `scraped_docs.json` | Automated doc scraping with rate limiting and caching |
| **3** | `metadata_enricher.rs` + LLM synthesis | Enriched `field_metadata_enriched.json` |
| **4** | `lancedb_utils.rs` update for richer embeddings | Better RAG retrieval quality |
| **5** | `ResourceMetadata` + `--show-resources` + `--export-field-metadata` update | Resource hierarchy docs, improved export |

---

# Alternative Scraping Plan for Google Ads API Documentation

**Version:** 1.0
**Date:** 2026-03-12
**Status:** Draft

---

## Executive Summary

The current HTML scraping implementation in `scraper.rs` is **not extracting meaningful field-level documentation** from the Google Ads API reference pages. Analysis of the actual scraped data reveals that only generic "Fields/Segments/Metrics" compatibility pages are being captured, with **zero individual field descriptions** (e.g., no descriptions for `campaign.name`, `campaign.status`, etc.).

This document analyzes the root causes and presents four alternative approaches to obtain high-quality field metadata, ranked by recommendation.

---

## Current State Analysis

### What the Current Scraper Does

The scraper at `crates/mcc-gaql-gen/src/scraper.rs` attempts to:
1. Fetch HTML pages from `https://developers.google.com/google-ads/api/fields/{version}/{resource}`
2. Use CSS selectors (`h2[id], h3[id], h4[id]`) to find field headings
3. Extract descriptions from sibling `<p>` elements
4. Extract enum values from table cells

### Actual Scraped Data (as of 2026-03-12)

**File:** `~/.cache/mcc-gaql/scraped_docs.json`

```json
{
  "docs": {
    "campaign.fieldssegmentsmetrics": {
      "description": "The Campaign resource has the following compatible Fields, Segments, and Metrics...",
      "enum_values": ["FIELDS", "SEGMENTS", "METRICS", "ENABLED", "PAUSED", ...]
    },
    "ad_group.fieldssegmentsmetrics": {
      "description": "The AdGroup resource has the following compatible Fields, Segments, and Metrics...",
      "enum_values": [...]
    }
  },
  "resources_scraped": 3,
  "resources_skipped": 0,
  "total_field_docs": 3
}
```

**Key Finding:** Only **3 generic entries** for 3 resources, each with identical "Fields/Segments/Metrics" text. **No actual field documentation** for individual fields like:
- `campaign.name`
- `campaign.status`
- `campaign.advertising_channel_type`
- `ad_group.name`
- `ad_group.campaign`

### Root Cause Analysis

| Issue | Details |
|-------|---------|
| **JavaScript-Rendered Content** | Google's documentation uses `<devsite-filter>` custom elements and Angular-like rendering. The raw HTML is 8+ MB and contains mostly JavaScript templates. |
| **Incorrect CSS Selectors** | The actual field tables use `<table class="responsive">` with `<tr data-id="...">` attributes, not `<h2/h3/h4 id="...">` headings. |
| **No Headless Browser** | The scraper fetches raw HTML via `reqwest`, which cannot execute JavaScript to render the final DOM. |
| **Misleading Success Logs** | The scraper logs "extracted X field docs" based on partial matches of generic text, not actual field descriptions. |

---

## Comparison: Expected vs Actual

### Expected (Per Design Spec)

```json
{
  "campaign.name": {
    "description": "The name of the campaign. This field is required and must be unique among all campaigns in the account...",
    "enum_values": []
  },
  "campaign.status": {
    "description": "The status of the campaign. Determines whether the campaign is currently serving ads.",
    "enum_values": ["ENABLED", "PAUSED", "REMOVED"]
  },
  "campaign.advertising_channel_type": {
    "description": "The primary serving target for ads within the campaign...",
    "enum_values": ["SEARCH", "DISPLAY", "SHOPPING", "HOTEL", "VIDEO", ...]
  }
}
```

### Actual

```json
{
  "campaign.fieldssegmentsmetrics": {
    "description": "The Campaign resource has the following compatible Fields, Segments, and Metrics...",
    "enum_values": ["FIELDS", "SEGMENTS", "METRICS", ...]
  }
}
```

### Gap Assessment

| Metric | Target | Current | Gap |
|--------|--------|---------|-----|
| Fields with descriptions | ~5,000+ | 0 | 100% missing |
| Resources fully documented | ~120 | 0 | 100% missing |
| Enum value meanings | Rich descriptions | Raw values only | Context missing |

---

## Alternative Approaches

### Option 1: Google Ads Proto File Scraping (Recommended)

**Approach:** Fetch and parse the official `.proto` files from the `googleapis/google-ads-php` repository.

**Rationale:** Google maintains canonical proto definitions with field comments at:
- `https://github.com/googleapis/google-ads-php/tree/main/metadata/Google/Ads/GoogleAds/V{N}/Services`
- `https://github.com/googleapis/google-ads-php/tree/main/metadata/Google/Ads/GoogleAds/V{N}/Resources`

**Proto File Example:**
```protobuf
// Campaign.proto
message Campaign {
  // Immutable. The resource name of the campaign.
  // Campaign names have the form:
  // `customers/{customer_id}/campaigns/{campaign_id}`
  string resource_name = 1;

  // Output only. The ID of the campaign.
  int64 id = 2 [deprecated = true];

  // The name of the campaign.
  // This field is required and should not be empty.
  // It must be unique within an account.
  string name = 3;

  // The status of the campaign.
  // When a new campaign is added, the default value is ENABLED.
  google.ads.googleads.v16.enums.CampaignStatusEnum.CampaignStatus status = 4;
}
```

**Implementation:**

```rust
// New module: proto_scraper.rs

pub async fn fetch_proto_files(version: &str) -> Result<HashMap<String, String>> {
    let base_url = format!(
        "https://raw.githubusercontent.com/googleapis/google-ads-php/main/metadata/Google/Ads/GoogleAds/{}",
        version
    );

    // Fetch Resource proto files
    let resources = vec![
        "Resources/Campaign.proto",
        "Resources/AdGroup.proto",
        "Resources/AdGroupAd.proto",
        // ... etc
    ];

    // Parse with protobuf-parse or regex
}

pub fn parse_proto_comments(content: &str) -> HashMap<String, FieldDoc> {
    // Extract field numbers, names, and their preceding comments
}
```

**Pros:**
- | Source of truth | Comments are Google's official documentation |
- | Structured data | Field types, enums, and relationships are explicit |
- | No JS rendering | Plain text files, easily parsed |
- | Versioned | Clear mapping to API versions |
- | Reliable | GitHub raw content is stable |

**Cons:**
- | Comment quality varies | Some fields have minimal documentation |
- | Manual enum mapping | Need to cross-reference enum proto files |
- | Different structure | Requires new parser vs. current HTML approach |

**Effort:** Medium (2-3 days)

---

### Option 2: Hybrid Proto + LLM Enrichment

**Approach:** Use proto files as the base, then use LLM to expand terse comments into rich descriptions.

**Pipeline:**
```
proto files → parse comments → lightweight descriptions → LLM expansion → enriched cache
```

**LLM Prompt Template:**
```
You are a technical documentation writer. Given this Google Ads API field information,
write a comprehensive 2-3 sentence description suitable for RAG retrieval.

Field: campaign.advertising_channel_type
Proto type: ENUM
type: google.ads.googleads.v16.enums.AdvertisingChannelTypeEnum.AdvertisingChannelType
Proto comment: "The primary serving target for ads within the campaign."
Enum values: SEARCH, DISPLAY, SHOPPING, HOTEL, VIDEO, MULTI_CHANNEL, LOCAL, SMART, PERFORMANCE_MAX, LOCAL_SERVICES, DISCOVERY, TRAVEL

Requirements:
1. Explain what the field represents
2. Mention common use cases
3. Include the enum values and their meanings
4. Mention filtering/sorting capabilities if relevant

Response format: {"description": "...", "enum_meanings": {"SEARCH": "...", ...}}
```

**Pros:**
- | Best of both worlds | Factual accuracy from protos + readability from LLM |
- | Token efficient | LLM has structured input, not raw HTML |
- | Scalable | Batch multiple fields per call |

**Cons:**
- | Higher cost | Requires LLM calls for all fields |
- | Dependency | Still requires proto fetch infrastructure |

**Effort:** Medium-High (3-4 days)

---

### Option 3: LLM-Only Descriptions (Skip Scraping)

**Approach:** Generate all field descriptions using LLM with only structural metadata from the Fields Service API.

**Input to LLM:**
```
Generate documentation for Google Ads API fields:

Resource: campaign
Fields:
- name: STRING, required, unique
- status: ENUM(ENABLED, PAUSED, REMOVED), filterable, sortable
- advertising_channel_type: ENUM(SEARCH, DISPLAY, ...), immutable after creation
- start_date: DATE, format YYYYMMDD
- end_date: DATE, optional
...
```

**Pros:**
- | No external dependencies | No scraping, no proto fetching |
- | Fast to implement | Uses existing LLM infrastructure |
- | Always available | Works offline once schema is known |

**Cons:**
- | Hallucination risk | LLM may invent incorrect semantics |
- | Generic quality | Descriptions may be formulaic |
- | No official validation | Can't verify against Google's docs |

**Effort:** Low (1-2 days)

---

### Option 4: Headless Browser Scraping

**Approach:** Use a headless browser (chromium via `headless_chrome` or `fantoccini`) to render the actual Google docs pages and extract content from the rendered DOM.

**Implementation:**
```rust
use headless_chrome::{Browser, LaunchOptions};

pub async fn scrape_with_browser(url: &str) -> Result<String> {
    let browser = Browser::new(LaunchOptions::default())?;
    let tab = browser.new_tab()?;
    tab.navigate_to(url)?;
    tab.wait_until_navigated()?;

    // Wait for Angular/devsite to render
    tab.wait_for_element("devsite-filter")?;

    // Extract content from rendered DOM
    let content = tab.find_elements("table.responsive tr")?;
    // Parse rows for field data
}
```

**Pros:**
- | Matches current design | Would extract what the design spec expected |
- | Rich content | Full access to tables, examples, cross-references |

**Cons:**
- | Heavy dependency | Requires Chrome/Chromium installation |
- | Slow | Page render + wait for JS + extraction = 5-10s per page |
- | Fragile | Google can change JS structure without notice |
- | Resource intensive | 120 pages × 10s = 20+ minutes per run |
- | CI/CD issues | Browser automation in containers is finicky |

**Effort:** High (5-7 days)

---

## Decision Matrix

| Criteria | Proto Files (Opt 1) | Hybrid (Opt 2) | LLM-Only (Opt 3) | Headless (Opt 4) |
|----------|---------------------|----------------|------------------|------------------|
| Data accuracy | High | High | Medium | High |
| Implementation effort | Medium | Medium-High | Low | High |
| Maintenance burden | Low | Medium | Low | High |
| CI/CD compatibility | Excellent | Excellent | Excellent | Poor |
| LLM token cost | None | Medium | High | None |
| Source of truth | Official | Official | Inferred | Scraped |
| Time to implement | 2-3 days | 3-4 days | 1-2 days | 5-7 days |

---

## Recommendation

### Primary: Option 1 (Proto File Scraping)

**Why:** Proto files provide authoritative, structured documentation directly from Google's source. The `google-ads-php` repository is automatically updated when new API versions release. This approach eliminates the brittleness of HTML scraping while maintaining high data quality.

**Implementation Steps:**
1. Add `proto_scraper.rs` module to fetch proto files from GitHub
2. Parse proto comments using `protobuf-parse` crate or custom regex
3. Cross-reference with existing `field_metadata.json` (structural data) for field types
4. Merge proto descriptions into `FieldMetadata.description`
5. Maintain existing scraper as fallback for relationship data

### Fallback: Option 2 (Hybrid) if proto comments are insufficient

If proto file comments are too terse for key fields, run a targeted LLM enrichment pass on those specific fields only.

### Not Recommended: Option 4 (Headless Browser)

The complexity, fragility, and maintenance burden outweigh the benefits. Google's documentation JS framework can change at any time, breaking the scraper.

---

## Migration Path

1. **Phase 1:** Implement proto file scraper alongside existing HTML scraper
2. **Phase 2:** Compare coverage between proto and HTML sources
3. **Phase 3:** Deprecate HTML scraper if proto coverage is superior
4. **Phase 4:** (Optional) Add targeted LLM enrichment for terse proto comments

---

## Files to Create/Modify

| File | Action | Description |
|------|--------|-------------|
| `crates/mcc-gaql-gen/src/proto_scraper.rs` | Create | Fetch and parse Google Ads proto files |
| `crates/mcc-gaql-gen/src/main.rs` | Modify | Add `--use-proto-source` flag |
| `crates/mcc-gaql-gen/Cargo.toml` | Modify | Add `protobuf-parse` dependency |
| `crates/mcc-gaql-common/src/field_metadata.rs` | Modify | Add `merge_proto_descriptions()` method |

---

## Appendix: Proto File URLs

**Base URL:**
```
https://raw.githubusercontent.com/googleapis/google-ads-php/main/metadata/Google/Ads/GoogleAds/V16/
```

**Key Files:**
```
Resources/Campaign.proto
Resources/AdGroup.proto
Resources/AdGroupAd.proto
Resources/AdGroupCriterion.proto
Resources/Customer.proto
Enums/CampaignStatus.proto
Enums/AdvertisingChannelType.proto
...
```

---

## Open Questions

1. Should we support multiple proto versions (V16, V17, V18) side-by-side?
2. How do we handle proto files with no comments for certain fields?
3. Should we cache proto files locally with TTL similar to HTML scraping?
4. Can we leverage `googleads-rs` crate's existing proto definitions instead of fetching?

---

*End of Document*

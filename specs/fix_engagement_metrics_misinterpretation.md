# Fix: "Engagement Metrics" Misinterpretation in GAQL Query Generation

**Status:** Ready for implementation

## Context

The Query Cookbook Generation Comparison Report (`reports/query_cookbook_gen_comparison.20260327124819.md`) identified "Engagement Metrics Misinterpretation" as the **most prevalent failure pattern** (Systemic Issue #1), affecting entries 4-10 (7 out of 27 entries = 26%).

**The Problem:** When users say "engagement metrics" in the context of campaign performance, the LLM interprets this literally as Google Ads engagement-specific metrics (`metrics.engagements`, `metrics.engagement_rate`, `metrics.interactions`) rather than general performance metrics (`metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`).

**Example:**
- User query: "Get me the **engagement metrics** of top PMax campaigns..."
- Expected: `metrics.impressions, metrics.clicks, metrics.cost_micros`
- Generated: `metrics.engagements, metrics.engagement_rate, metrics.interactions, metrics.interaction_rate`

### Root Cause Analysis

1. **Ambiguous terminology in query cookbook descriptions**: The query cookbook (`resources/query_cookbook.toml`) uses "engagement metrics" in 10+ entry descriptions, but maps them to `impressions/clicks/cost_micros` in the actual queries.

2. **No domain vocabulary guidance in LLM prompt**: The Phase 3 field selection prompt (`crates/mcc-gaql-gen/src/rag.rs:2736-2835`) has no guidance on digital advertising metric terminology disambiguation.

3. **Literal field name matching**: The LLM sees `metrics.engagements` and `metrics.engagement_rate` in the candidate field list and matches them to "engagement" in the user query.

---

## Domain Metric Taxonomy

Standardized metric classification that should govern field selection:

### 1. Volume Metrics (The "How Big?")
- **Impressions** (`metrics.impressions`): Total visibility
- **Clicks** (`metrics.clicks`): Total traffic
- **Conversions** (`metrics.conversions`): Total outcomes

### 2. Financial Metrics (The "How Much?")
- **Cost/Spend** (`metrics.cost_micros`): Total investment
- **CPC** (`metrics.average_cpc`): Cost per click
- **CPA** (`metrics.cost_per_conversion`): Cost per acquisition
- **ROAS** (`metrics.roas`): Return on ad spend

### 3. Ratio Metrics (The "How Well?")
- **CTR** (`metrics.ctr`): Clicks/Impressions - ad relevance indicator
- **CVR** (`metrics.conversions / metrics.clicks`): Conversion rate - landing page effectiveness

---

## Implementation Plan

### Approach: Two-Pronged Fix

**Fix A:** Update Query Cookbook Descriptions (resources/query_cookbook.toml)
**Fix B:** Add Metric Terminology Guidance to LLM Prompt (crates/mcc-gaql-gen/src/rag.rs)

---

## Fix A: Query Cookbook Description Updates

**File:** `resources/query_cookbook.toml`

Replace "engagement metrics" with "volume and spend metrics (impressions, clicks, cost)" in these entries:

| Line | Entry | Current | Replace With |
|------|-------|---------|--------------|
| 84 | accounts_with_perf_max_campaigns_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 109 | accounts_with_smart_campaigns_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 134 | accounts_with_local_campaigns_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 159 | accounts_with_shopping_campaigns_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 184 | accounts_with_multichannel_campaigns_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 210 | accounts_with_asset_sitelink_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 238 | accounts_with_asset_call_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 267 | accounts_with_asset_callout_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 293 | accounts_with_asset_app_last_week | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |
| 347 | asset_performance_ytd_daily | "asset engagement metrics" | "asset volume and spend metrics (impressions, clicks, cost)" |
| 427 | search_terms_with_top_traffic_smart_last_30d | "engagement metrics" | "volume and spend metrics (impressions, clicks, cost)" |

**Note:** Keep the original test queries (entries 4-10 descriptions in the comparison test) using "engagement metrics" phrasing to serve as regression tests that verify the LLM prompt fix handles ambiguous input correctly.

---

## Fix B: Add Metric Terminology Guidance to LLM Prompt

**File:** `crates/mcc-gaql-gen/src/rag.rs`

**Location:** Add to both system prompts in `select_fields()` method:
- Cookbook-enabled prompt: ~line 2765 (after identity fields guidance, before date range guidance)
- Non-cookbook prompt: ~line 2868 (same relative position)

**Insert this guidance block:**

```rust
- **IMPORTANT: Digital Advertising Metric Terminology Disambiguation**
  In digital advertising context, common terms map to specific metrics:

  **Volume Metrics ("How Big?"):**
  - metrics.impressions (visibility - are people seeing the ads?)
  - metrics.clicks (traffic - are people interested enough to visit?)
  - metrics.conversions (outcomes - did we achieve the goal?)

  **Financial Metrics ("How Much?"):**
  - metrics.cost_micros (total investment/spend)
  - metrics.average_cpc (cost per click)
  - metrics.cost_per_conversion (CPA - cost per acquisition)
  - metrics.roas (return on ad spend)

  **Ratio Metrics ("How Well?"):**
  - metrics.ctr (click-through rate - ad relevance indicator)
  - metrics.conversions/clicks (conversion rate - landing page effectiveness)

  **"Performance metrics" / "engagement metrics" / "how is it doing":**
  These colloquial terms typically mean the core Volume + Financial metrics:
  → metrics.impressions, metrics.clicks, metrics.cost_micros

  **When NOT to use metrics.engagements:**
  The literal `metrics.engagements` and `metrics.engagement_rate` fields are for specific
  interaction tracking (video views, social clicks) - NOT general campaign performance.
  Only select these when the user explicitly asks for "engagements" as a specific metric,
  or for video/social campaign types where engagements are the primary KPI.

  **Default "performance overview" fields:**
  When asked for general performance without specific metrics named, include:
  - metrics.impressions, metrics.clicks, metrics.cost_micros (core trio)
  - metrics.conversions (if asking about outcomes/results)
  - metrics.ctr, metrics.roas (if asking about efficiency)
```

---

## Files to Modify

1. **`resources/query_cookbook.toml`**
   - Update 11 entry descriptions to replace "engagement metrics" with "volume and spend metrics (impressions, clicks, cost)"

2. **`crates/mcc-gaql-gen/src/rag.rs`**
   - Add metric terminology guidance section to Phase 3 system prompts (both cookbook and non-cookbook variants, ~lines 2765 and 2868)

---

## Verification Plan

1. **Rebuild the query cookbook embeddings** (if RAG index uses descriptions):
   ```bash
   cargo run -p mcc-gaql-gen -- index --rebuild-cookbook
   ```

2. **Re-run the cookbook comparison test** on entries 4-10:
   ```bash
   cargo run -p mcc-gaql-gen -- test-cookbook --entries 4,5,6,7,8,9,10
   ```

3. **Expected outcome for entry 4:**
   - Input: "Get me the engagement metrics of top PMax campaigns..."
   - Generated should include: `metrics.impressions, metrics.clicks, metrics.cost_micros`
   - Generated should NOT include: `metrics.engagements, metrics.engagement_rate`

4. **Manual spot check:**
   ```bash
   cargo run -p mcc-gaql -- query "show me engagement metrics for my top campaigns last week"
   ```
   Verify it returns impressions/clicks/cost, not literal engagements.

5. **Negative test** (ensure literal engagements still work when explicitly requested):
   ```bash
   cargo run -p mcc-gaql -- query "show me video engagements and engagement rate for YouTube campaigns"
   ```
   Verify it correctly returns `metrics.engagements, metrics.engagement_rate`.

---

## Rollback Plan

If the fix causes issues:
1. Revert the prompt changes in `rag.rs`
2. The cookbook description changes are low-risk and can remain
3. Monitor for any queries that legitimately need `metrics.engagements` being incorrectly filtered

---

## Related Issues

- Systemic Issue #2 (Dollar-to-Micros Threshold Conversion) - separate fix needed
- Systemic Issue #3 (Implicit status='ENABLED' Filter) - separate fix needed

# GAQL Domain Knowledge

## Resource Selection Guidance

- For asset extension performance (sitelinks, callouts, calls, structured snippets):
  **primary_resource must be `campaign_asset`** (or `ad_group_asset`) with a `field_type` filter.
  
  **TRIGGER PATTERNS requiring campaign_asset/ad_group_asset:**
  - User says "include [sitelink/callout/call/etc.] text" or "include the text"
  - User says "show me the [extension] details/content"
  - User says "with phone number" or "with link text"
  - User asks for ANY asset.* field (asset.sitelink_asset.*, asset.callout_asset.*, etc.)
  
  **NEVER use these resources for asset detail queries:**
  - `asset_field_type_view` - provides aggregate metrics by asset type only, CANNOT access asset.* fields
  - `asset` - static entity definition with no metrics support
  - `campaign` - cannot access asset-level fields
  - `call_view` - individual call records, not asset extension metrics
  
  **When to use asset_field_type_view (rare):**
  ONLY when user wants aggregate performance metrics BY asset type with NO individual asset details.
  Example: "Show me daily metrics broken down by asset type" (no text/content requested)
- For daily asset metrics broken down by asset type:
  Use `asset_field_type_view`. Do NOT use `asset` (static entity definition, no metrics support).
- For Smart campaign performance with metrics:
  Use `campaign` with `advertising_channel_type IN ('SMART')`. Do NOT use `smart_campaign_setting` (configuration only, no metrics).
- For location-level performance data ("top locations", "best performing regions", "geo performance"):
  Use `location_view` with `campaign_criterion` fields. Each row represents a UNIQUE COMBINATION of campaign + geo target, so it naturally supports "top locations per campaign" analysis.
  The `campaign_criterion.location.geo_target_constant` field provides the geo target ID.
  Do NOT use `campaign` with geo segments - that gives campaign-level data only, not individual location performance.
- For impression share metrics (search impression share, budget lost impression share, etc.),
  use the `campaign` resource. Specialized views like `performance_max_placement_view` are for
  placement-level data and do NOT expose impression share metrics.
- When in doubt between a specialized view and a core resource (campaign, ad_group, customer),
  prefer the core resource -- it has broader metric availability.
- General rule: Configuration/setting resources (e.g., `smart_campaign_setting`) and static entity resources (e.g., `asset`) do NOT support metrics fields. If the user wants performance data, always prefer the metrics-bearing resource even if a configuration resource has a closer name match.

## Metric Terminology

**Digital Advertising Metric Terminology Disambiguation**

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

## Numeric and Monetary Conversion

**Numeric suffixes (K, M, B):** "K" means thousand (×1000), "M" means million (×1,000,000), "B" means billion. These apply to ANY numeric field:
- "10K" → 10000
- "5M" → 5000000
- "1.5K" → 1500

**Monetary values (micros conversion):** ONLY fields ending in `_micros` (e.g., metrics.cost_micros, campaign_budget.amount_micros) and metrics.cost_per_conversion store currency in micros (1 dollar = 1,000,000 micros). This is IN ADDITION to any K/M/B suffix:
- "$200" or "200 dollars" → 200000000
- "$1K" or "$1,000" → 1000000000 (1000 dollars × 1,000,000 micros/dollar)
- "$1.50" → 1500000
- For _micros fields: multiply dollar values by 1,000,000 ON TOP OF any K/M/B suffix
- For NON-_micros fields (like metrics.clicks): "10K" simply means 10000, NOT 10000000

## Monetary Value Extraction

**CRITICAL - Monetary Value Extraction:** For monetary thresholds, you MUST extract the EXACT numeric value from the user query.
- "CPA >$200" → extract "200", then convert to micros → value: "200000000"
- "spend >$1K" → extract "1000" (K = 1000), then convert to micros → value: "1000000000"
- "cost > $50.50" → extract "50.50", then convert to micros → value: "50500000"
- **NEVER** return "0" for a threshold unless the user explicitly said "0" or "$0"
- **All monetary fields** (_micros, cost_per_*, value_per_*): values must be in micros (1 dollar = 1,000,000 micros)
- Fields requiring micros conversion: metrics.cost_micros, campaign_budget.amount_micros, metrics.cost_per_conversion, metrics.cost_per_all_conversions, metrics.value_per_conversion, metrics.all_conversions_value
- **Validation check:** If the user said ">$200" and you're returning "0", you made an error. The value should be "200000000".

## Date Range Handling

For date ranges, use the APPROPRIATE method based on the period:

**Use DURING with date literals** (NO quotes around value) for these standard periods.
Valid Google Ads date literals: TODAY, YESTERDAY, LAST_7_DAYS, LAST_14_DAYS,
LAST_30_DAYS, LAST_BUSINESS_WEEK, LAST_WEEK_MON_SUN,
LAST_WEEK_SUN_SAT, THIS_WEEK_SUN_TODAY, THIS_WEEK_MON_TODAY, THIS_MONTH, LAST_MONTH

Common mappings:
- "yesterday" → operator: "DURING", value: "YESTERDAY"
- "today" → operator: "DURING", value: "TODAY"
- "last 7 days" → operator: "DURING", value: "LAST_7_DAYS"
- "last 14 days" → operator: "DURING", value: "LAST_14_DAYS"
- "last 30 days" → operator: "DURING", value: "LAST_30_DAYS"
- "this month" → operator: "DURING", value: "THIS_MONTH"
- "last month" → operator: "DURING", value: "LAST_MONTH"
- "last week" → operator: "DURING", value: "LAST_WEEK_MON_SUN"
- "last business week" → operator: "DURING", value: "LAST_BUSINESS_WEEK"

**Use BETWEEN with computed dates** (value format: "YYYY-MM-DD AND YYYY-MM-DD") for:
- Quarters (NOT valid date literals): "this quarter", "last quarter"
- Years (NOT valid date literals): "this year", "last year"
- Holiday periods and seasonal ranges:
  - "this summer" / "last summer" → Jun 1 to Aug 31
  - "this winter" / "last winter" → Dec 1 to Feb 28/29
  - "this spring" / "last spring" → Mar 1 to May 31
  - "this fall" / "this autumn" / "last fall" → Sep 1 to Nov 30
  - "christmas holiday" → Dec 20 to Dec 31
  - "thanksgiving" / "thanksgiving week"
  - "easter" / "easter week"
  - "black friday", "cyber monday"
  - "new years", "valentines day", "mothers day", "fathers day", "halloween"
  - "last 60 days"
  - "last 90 days"

## Query Best Practices

**Fields in WHERE clause should also appear in SELECT:**
For transparency and human verifiability, any field used as a filter in the WHERE clause should also be included in SELECT. This allows users to verify the filter applied correctly.
- If filtering on `campaign.status`, include `campaign.status` in select_fields
- If filtering on `segments.date`, include `segments.date` in select_fields
- If filtering on `ad_group.status`, include `ad_group.status` in select_fields

**Ratio metrics require their component metrics:**
When including a ratio/derived metric, always include the underlying component metrics so the user can understand and verify the calculation:
- **CTR** (metrics.ctr) → also include metrics.clicks AND metrics.impressions
- **CPC** (metrics.average_cpc) → also include metrics.cost_micros AND metrics.clicks
- **ROAS** (metrics.roas) → also include metrics.conversions_value AND metrics.cost_micros
- **CPA** (metrics.cost_per_conversion) → also include metrics.cost_micros AND metrics.conversions
- **Conversion rate** (metrics.conversions / metrics.clicks) → also include metrics.conversions AND metrics.clicks
- **Impression share** (e.g., metrics.search_impression_share) → also include metrics.impressions (note: total eligible impressions is not directly available)

**Include all variants of a metric category:**
When a user asks for metrics in a category that has multiple variants, include all of them as they each provide distinct analytical value:
- **Impression share / prominence metrics:** Include both the base impression share set AND the top/absolute-top impression share set:
  - metrics.search_impression_share
  - metrics.search_top_impression_percentage
  - metrics.search_absolute_top_impression_percentage
  - metrics.search_budget_lost_impression_share
  - metrics.search_rank_lost_impression_share
  - (and their top/absolute-top lost variants if available)
- **Conversion metrics:** When conversions are relevant, include both metrics.conversions and metrics.all_conversions (they count differently)
- **Cost metrics:** When cost is relevant, include both metrics.cost_micros and metrics.average_cpc for full context

## Pattern-Based Field Requirements

### Currency Code Pattern
When the user request mentions currency (phrases: "with currency", "need currency", "and currency", "currency code"), ALWAYS include `customer.currency_code` in the SELECT fields.

### Advertising Channel Type Visibility Pattern
When filtering by `campaign.advertising_channel_type` in the WHERE clause, ALWAYS include `campaign.advertising_channel_type` in the SELECT fields for visibility.

### Asset Identification Pattern
When the request asks for asset content (phrases: "include [X] text", "show me the [X] content", "with [extension] details"), ALWAYS include:
- asset.id
- asset.name
- asset.type

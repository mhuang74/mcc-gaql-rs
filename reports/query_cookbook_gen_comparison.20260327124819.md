# Query Cookbook Generation Comparison Report
**Timestamp:** 20260327124819
**Model:** zai-org/glm-4.7
**Run:** cookbook_regen_run_5

## Summary Statistics
- Total entries tested: 27
- EXCELLENT: 12 (44%)
- GOOD: 9 (33%)
- FAIR: 5 (19%)
- POOR: 1 (4%)

---

## Detailed Results

### 1. account_ids_with_access_and_traffic_last_week
**Description:** Get me account IDs with clicks in the last week

**Reference Query:**
```sql
SELECT
  customer.id
FROM customer
WHERE
  segments.date during LAST_WEEK_MON_SUN
  AND metrics.clicks > 0
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name
FROM customer
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.clicks > 0
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified `customer` as primary resource, applied date and clicks filters
- Key Decision Points: Added `customer.descriptive_name` as an identity field enhancement
- Comparison to Intent: Fully aligned — resource, date range, and clicks > 0 all match
- Where It Diverged: Extra `customer.descriptive_name` field (acceptable addition)

**Analysis:**
- Selected Fields: customer.id present; extra descriptive_name is a useful addition
- Data Scope: customer resource, LAST_WEEK_MON_SUN, clicks > 0 — perfect match
- Semantic Equivalence: Identical semantics

**Key Differences (with Reasoning Context):**
- Extra `customer.descriptive_name` — LLM always adds descriptive name for account identification

---

### 2. accounts_with_traffic_last_week
**Description:** Show me account-level performance last week - need impressions, clicks, spend, and currency

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM customer
WHERE
  segments.date during LAST_WEEK_MON_SUN
  AND metrics.impressions > 0
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM customer
WHERE segments.date DURING LAST_WEEK_MON_SUN
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified all required fields on `customer` resource
- Key Decision Points: Applied LAST_WEEK_MON_SUN date filter; did not add impressions > 0 filter
- Comparison to Intent: Fields are exactly correct; missing the impressions threshold filter
- Where It Diverged: No `metrics.impressions > 0` filter applied

**Analysis:**
- Selected Fields: Perfect match — all 6 fields identical
- Data Scope: Same resource and date range; missing the impressions > 0 filter
- Semantic Equivalence: Returns more rows (no impression filter) but same shape

**Key Differences (with Reasoning Context):**
- Missing `metrics.impressions > 0` filter — LLM didn't infer implicit threshold from "show me" phrasing

---

### 3. keywords_with_top_traffic_last_week
**Description:** Pull my top 10 keywords by clicks (>10K) last week - need acct, campaign, ad group IDs + names, channel type, and currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id, campaign.name,
  campaign.advertising_channel_type, ad_group.id, ad_group.name,
  ad_group.type, ad_group_criterion.criterion_id, ad_group_criterion.keyword.text,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM keyword_view
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.clicks > 10000
ORDER BY metrics.clicks DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  ad_group.id, ad_group.name, ad_group_criterion.criterion_id,
  ad_group_criterion.keyword.text, metrics.clicks
FROM keyword_view
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.clicks > 10000
  AND keyword_view.status = 'ENABLED'
ORDER BY metrics.clicks DESC LIMIT 10
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly selected keyword_view with all hierarchy fields and click filter
- Key Decision Points: Included all identity fields; excluded metrics.impressions and metrics.cost_micros
- Comparison to Intent: Correct resource and structure; missing some non-identity metrics
- Where It Diverged: Missing `metrics.impressions`, `metrics.cost_micros`; extra `keyword_view.status = 'ENABLED'` implicit filter

**Analysis:**
- Selected Fields: Missing impressions and cost_micros; missing `ad_group.type`
- Data Scope: Correct resource, filters, sort, and limit
- Semantic Equivalence: Returns same rows but less metric detail

**Key Differences (with Reasoning Context):**
- Missing `metrics.impressions`, `metrics.cost_micros`, `ad_group.type` — LLM focused only on clicks as the core metric
- Extra `keyword_view.status = 'ENABLED'` implicit filter — may exclude paused keywords

---

### 4. accounts_with_perf_max_campaigns_last_week
**Description:** Get me the engagement metrics of top PMax campaigns by clicks (>100) last week - need acct and campaign IDs + names, channel type, and currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
  AND metrics.clicks > 100
ORDER BY metrics.clicks DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id, campaign.name,
  campaign.advertising_channel_type, customer.currency_code,
  metrics.engagements, metrics.engagement_rate, metrics.interactions, metrics.interaction_rate
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
  AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC LIMIT 10
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified resource and filters; interpreted "engagement metrics" literally
- Key Decision Points: Chose engagement-specific metrics (engagements, engagement_rate) over standard performance metrics (impressions, clicks, cost_micros)
- Comparison to Intent: Misinterpreted "engagement metrics" — user meant general performance, not the engagement metric type
- Where It Diverged: Selected engagement-specific metrics instead of impressions/clicks/cost; the intent was performance summary

**Analysis:**
- Selected Fields: Missing `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`; added engagement-specific metrics not in reference
- Data Scope: Correct resource, PMax filter, clicks > 100, date range — correct
- Semantic Equivalence: Wrong metrics — would not return the same data

**Key Differences (with Reasoning Context):**
- Replaced impressions/clicks/cost with engagement_rate/engagements/interactions — LLM literally interpreted "engagement metrics" but user meant performance overview
- Extra `campaign.status = 'ENABLED'` implicit filter

---

### 5. accounts_with_smart_campaigns_last_week
**Description:** Show me the engagement metrics of top Smart campaigns by clicks (>100) last week - need acct and campaign details with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('SMART')
  AND metrics.clicks > 100
ORDER BY metrics.clicks DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  metrics.engagements, metrics.engagement_rate, metrics.interactions,
  metrics.interaction_rate, metrics.clicks
FROM campaign
WHERE campaign.advertising_channel_type IN ('SMART')
  AND segments.date DURING LAST_WEEK_MON_SUN
  AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC LIMIT 10
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Same pattern as PMax — correctly filtered SMART campaigns, misinterpreted "engagement metrics"
- Key Decision Points: Added metrics.clicks (correctly), but added engagement-specific metrics instead of impressions/cost
- Comparison to Intent: Same misinterpretation as entry 4
- Where It Diverged: Missing impressions and cost_micros; added engagement-specific metrics

**Analysis:**
- Selected Fields: Has metrics.clicks (good), but missing metrics.impressions and metrics.cost_micros
- Data Scope: Correct resource, SMART filter, date range, sort, limit
- Semantic Equivalence: Partial match — clicks data present but missing spend/impressions

**Key Differences (with Reasoning Context):**
- Same "engagement metrics" misinterpretation as entries 4, 6, 7, 8 — systemic issue with this phrasing

---

### 6. accounts_with_local_campaigns_last_week
**Description:** Pull the engagement metrics of top Local campaigns by clicks (>500) last week - need acct and campaign info with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('LOCAL')
  AND metrics.clicks > 500
ORDER BY metrics.clicks DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  metrics.engagements, metrics.engagement_rate, metrics.clicks
FROM campaign
WHERE campaign.advertising_channel_type IN ('LOCAL')
  AND metrics.clicks > 500 AND segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource and filters; same engagement metrics misinterpretation; also missing LIMIT 10
- Key Decision Points: Correctly applied LOCAL filter and clicks threshold; omitted impressions/cost
- Comparison to Intent: Partial — has the right structure but wrong metric selection
- Where It Diverged: Missing impressions, cost_micros; extra engagement metrics; no LIMIT

**Analysis:**
- Selected Fields: Missing `metrics.impressions`, `metrics.cost_micros`
- Data Scope: Correct resource, LOCAL filter, clicks > 500, date range — correct; missing LIMIT 10
- Semantic Equivalence: Partial match

**Key Differences (with Reasoning Context):**
- Same "engagement metrics" misinterpretation pattern
- No LIMIT 10 applied — LLM may not have inferred "top" = limit

---

### 7. accounts_with_shopping_campaigns_last_week
**Description:** Get me the engagement metrics of the top Shopping campaigns by clicks (>100) last week - need acct and campaign details with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('SHOPPING')
  AND metrics.clicks > 100
ORDER BY metrics.clicks DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  metrics.engagements, metrics.engagement_rate, metrics.interactions,
  metrics.interaction_rate, metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('SHOPPING')
  AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC LIMIT 10
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Same pattern — SHOPPING filter correct, engagement metric misinterpretation
- Key Decision Points: Correct structure, wrong metrics
- Comparison to Intent: Partial match on metrics
- Where It Diverged: Same engagement metrics issue

**Analysis:**
- Selected Fields: Missing impressions/cost; extra engagement-specific metrics
- Data Scope: Correct
- Semantic Equivalence: Partial

**Key Differences (with Reasoning Context):**
- Consistent "engagement metrics" misinterpretation pattern (entries 4-8)

---

### 8. accounts_with_multichannel_campaigns_last_week
**Description:** Show me the engagement metrics of top Multi-Channel campaigns by clicks (>100) last week - need acct and campaign info with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('MULTI_CHANNEL')
  AND metrics.clicks > 100
ORDER BY metrics.clicks DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  metrics.engagements, metrics.engagement_rate, metrics.clicks
FROM campaign
WHERE campaign.advertising_channel_type = 'MULTI_CHANNEL'
  AND segments.date DURING LAST_WEEK_MON_SUN
  AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Same engagement metrics pattern; also note filter syntax difference (= vs IN)
- Key Decision Points: Used `= 'MULTI_CHANNEL'` instead of `IN ('MULTI_CHANNEL')`; missing LIMIT
- Comparison to Intent: Partial match
- Where It Diverged: Minor syntax diff (= vs IN); no LIMIT; same engagement metrics issue

**Analysis:**
- Selected Fields: Missing impressions/cost
- Data Scope: Correct resource and filters; `=` vs `IN` is semantically equivalent; missing LIMIT 10
- Semantic Equivalence: Partial

---

### 9. accounts_with_asset_sitelink_last_week
**Description:** Get me the engagement metrics of top Sitelink Extensions for each campaign by clicks (>20K) last week - need acct and campaign info with currency. include sitelink text.

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  asset.id, asset.name, asset.type, asset.sitelink_asset.link_text,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'SITELINK'
  AND metrics.clicks > 20000
ORDER BY metrics.impressions DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name,
  asset.sitelink_asset.link_text,
  metrics.engagements, metrics.engagement_rate, metrics.clicks
FROM campaign_asset
WHERE campaign_asset.field_type = 'SITELINK'
  AND segments.date DURING LAST_WEEK_MON_SUN
  AND metrics.clicks > 20000
ORDER BY metrics.clicks DESC LIMIT 10
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified campaign_asset resource and sitelink text field
- Key Decision Points: Correctly included `asset.sitelink_asset.link_text`; missed asset identity fields; engagement metrics pattern again
- Comparison to Intent: Good resource and sitelink field; missing impressions/cost/asset identity
- Where It Diverged: Missing asset.id, asset.name, asset.type, impressions, cost_micros; ORDER BY clicks vs impressions

**Analysis:**
- Selected Fields: Has sitelink text (key field); missing asset identity fields and some metrics
- Data Scope: Correct resource, field_type filter, date, clicks threshold
- Semantic Equivalence: Same data scope, less field detail

**Key Differences (with Reasoning Context):**
- Missing asset.id, asset.name, asset.type — LLM skipped asset identity in favor of just the text
- ORDER BY clicks DESC vs impressions DESC — minor semantic difference
- Same engagement metrics pattern

---

### 10. accounts_with_asset_call_last_week
**Description:** Get me the engagement metrics of top Call Extensions for each campaign by impressions (>100) last week - need acct and campaign info with currency. include phone number.

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  asset.id, asset.name, asset.type, asset.call_asset.phone_number,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'CALL'
  AND metrics.impressions > 100
ORDER BY metrics.impressions DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign_asset.resource_name,
  asset.call_asset.phone_number,
  metrics.phone_impressions, metrics.phone_calls, metrics.phone_through_rate,
  metrics.engagements, metrics.engagement_rate, metrics.average_cpe
FROM campaign_asset
WHERE campaign_asset.field_type = 'CALL'
  AND segments.date DURING LAST_WEEK_MON_SUN
  AND metrics.phone_impressions > 100
ORDER BY metrics.phone_impressions DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified phone number field; intelligently used phone-specific metrics
- Key Decision Points: Used `metrics.phone_impressions` for the impressions filter instead of `metrics.impressions` — domain-appropriate for call extensions
- Comparison to Intent: Sound reasoning; phone-specific metrics are more accurate for call extension analysis
- Where It Diverged: Different metric choice (phone vs general); missing asset identity fields; no LIMIT

**Analysis:**
- Selected Fields: Has phone_number (key field); phone-specific metrics are arguably better for this use case
- Data Scope: Correct resource, CALL filter, impressions threshold (using phone_impressions)
- Semantic Equivalence: Good — phone_impressions is actually more accurate than general impressions for this query

**Key Differences (with Reasoning Context):**
- `metrics.phone_impressions` vs `metrics.impressions` — LLM showed domain understanding; phone metrics are more appropriate
- Missing asset.id, asset.name, asset.type
- Added campaign_asset.resource_name (extra field)
- No LIMIT 10

---

### 11. accounts_with_asset_callout_last_week
**Description:** Get me the engagement metrics of top Callout Extensions for each campaign by clicks (>30K) last week - need acct and campaign info with currency. include callout text.

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  asset.id, asset.name, asset.type, asset.callout_asset.callout_text,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'CALLOUT'
  AND metrics.clicks > 30000
ORDER BY metrics.impressions DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name,
  asset.callout_asset.callout_text,
  metrics.engagements, metrics.engagement_rate, metrics.clicks
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.clicks > 30000
ORDER BY metrics.clicks DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly included callout text; correct resource and click filter; missing CALLOUT field_type filter
- Key Decision Points: Got callout text field right; forgot field_type filter which is critical
- Comparison to Intent: Missing the field_type = 'CALLOUT' filter — would return all asset types
- Where It Diverged: Missing `campaign_asset.field_type = 'CALLOUT'` filter

**Analysis:**
- Selected Fields: Has callout text; missing asset identity fields, impressions, cost
- Data Scope: Missing critical `field_type = 'CALLOUT'` filter — returns all campaign_asset types
- Semantic Equivalence: Would return all extension types, not just callouts — significant data scope issue

**Key Differences (with Reasoning Context):**
- Missing `campaign_asset.field_type = 'CALLOUT'` filter — major gap; would return wrong data
- Missing impressions/cost; no LIMIT 10

---

### 12. accounts_with_asset_app_last_week
**Description:** Pull the engagement metrics for top App Extensions for each campaign by impressions (>1) last week - need acct and campaign info with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  campaign.advertising_channel_type, campaign.name,
  campaign_asset.asset,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'APP'
  AND metrics.impressions > 1
ORDER BY metrics.impressions DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign_asset.asset,
  metrics.impressions, metrics.engagements, metrics.engagement_rate,
  metrics.interactions, metrics.interaction_rate
FROM campaign_asset
WHERE campaign_asset.field_type = 'APP'
  AND segments.date DURING LAST_WEEK_MON_SUN
  AND metrics.impressions > 1
ORDER BY metrics.impressions DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource, APP filter, impressions threshold; includes campaign_asset.asset
- Key Decision Points: Correctly included campaign_asset.asset; added engagement metrics
- Comparison to Intent: Very close — core fields present; missing cost; extra engagement metrics
- Where It Diverged: Missing metrics.cost_micros, metrics.clicks; extra engagement metrics; no LIMIT

**Analysis:**
- Selected Fields: Has campaign_asset.asset and impressions; missing clicks/cost
- Data Scope: Correct — APP filter, impressions threshold, date range
- Semantic Equivalence: Good match on scope; minor field differences

**Key Differences (with Reasoning Context):**
- Missing `metrics.clicks` and `metrics.cost_micros`
- Extra engagement metrics added
- No LIMIT 10

---

### 13. perf_max_campaigns_with_traffic_last_30_days
**Description:** Give me daily PMax performance last 30 days - need CTR, CPC, conv, rev, CPA, campaign IDs + names, and currency

**Reference Query:**
```sql
SELECT
  campaign.id, campaign.name, campaign.advertising_channel_type,
  segments.date,
  metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros,
  metrics.average_cost, metrics.conversions, metrics.conversions_value,
  metrics.cost_per_conversion, customer.currency_code
FROM campaign
WHERE segments.date DURING LAST_30_DAYS
  AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
  AND metrics.impressions > 1
ORDER BY campaign.id, segments.date
```

**Generated Query:**
```sql
SELECT
  campaign.id, campaign.name, segments.date,
  metrics.ctr, metrics.average_cpc, metrics.conversions,
  metrics.conversions_value, metrics.cost_per_conversion,
  customer.currency_code
FROM campaign
WHERE campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
  AND segments.date DURING LAST_30_DAYS
  AND campaign.status = 'ENABLED'
ORDER BY segments.date DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: All requested metrics present (CTR, CPC, conv, rev, CPA); good resource selection
- Key Decision Points: Used `average_cpc` (correct for CPC); included date segment for daily breakdown
- Comparison to Intent: Core metrics all present; missing impressions/cost_micros and channel_type field
- Where It Diverged: Missing `campaign.advertising_channel_type`, `metrics.impressions`, `metrics.cost_micros`, `metrics.average_cost`; different ORDER BY; no impressions filter

**Analysis:**
- Selected Fields: Has all explicitly requested metrics (CTR, CPC, conv, rev, CPA); missing some supporting metrics
- Data Scope: Same scope — PMax, last 30 days; missing impressions > 1 filter
- Semantic Equivalence: Semantically very close; would return same campaigns with same key metrics

**Key Differences (with Reasoning Context):**
- Missing `campaign.advertising_channel_type`, `metrics.impressions`, `metrics.cost_micros` — LLM focused on what was explicitly named
- ORDER BY segments.date DESC vs campaign.id, segments.date — different sort order

---

### 14. asset_fields_with_traffic_ytd
**Description:** Show me YTD asset engagement metrics by day. include asset type, spend and currency

**Reference Query:**
```sql
SELECT
  asset_field_type_view.field_type, segments.date,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM asset_field_type_view
WHERE segments.year IN (2026) AND metrics.impressions > 1
ORDER BY asset_field_type_view.field_type, segments.date
```

**Generated Query:**
```sql
SELECT
  asset_field_type_view.field_type, segments.date,
  metrics.engagements, metrics.interactions, metrics.interaction_rate,
  metrics.engagement_rate, metrics.cost_micros, customer.currency_code
FROM asset_field_type_view
WHERE segments.date BETWEEN '2026-01-01' AND '2026-03-27'
ORDER BY segments.date ASC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly selected asset_field_type_view; used BETWEEN for YTD (valid approach)
- Key Decision Points: `segments.year IN (2026)` vs `BETWEEN '2026-01-01' AND '2026-03-27'` — LLM chose dynamic date range
- Comparison to Intent: Correct resource and structure; "engagement metrics by day" led to engagement metrics selection over impressions/clicks
- Where It Diverged: Missing impressions/clicks; engagement metrics selected instead; different date filter syntax; different ORDER BY

**Analysis:**
- Selected Fields: Has cost_micros and currency; engagement metrics vs impressions/clicks
- Data Scope: Same YTD scope via BETWEEN; missing impressions > 1 filter
- Semantic Equivalence: Good intent match; metric differences are a pattern

**Key Differences (with Reasoning Context):**
- `BETWEEN '2026-01-01' AND '2026-03-27'` vs `segments.year IN (2026)` — both valid for YTD but BETWEEN will need updating each run
- Missing impressions/clicks; engagement metric pattern
- ORDER BY field_type + date vs just date

---

### 15. campaigns_with_smart_bidding_by_spend
**Description:** Pull top 25 Smart Bidding campaigns by spend (>$1K) last week - need acct and campaign IDs + names, budget, bid strategy, CPC, and conv metrics with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  campaign.bidding_strategy_type, campaign_budget.amount_micros,
  metrics.average_cpc, metrics.clicks, metrics.cost_micros,
  metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value
FROM campaign
WHERE campaign.bidding_strategy_type IN ('MAXIMIZE_CLICKS', ...)
  AND campaign.status IN ('ENABLED')
  AND segments.date DURING LAST_WEEK_MON_SUN
  AND metrics.cost_micros > 1000000000
ORDER by metrics.cost_micros DESC LIMIT 25
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign_budget.amount_micros,
  campaign.bidding_strategy_type, metrics.average_cpc,
  metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value,
  metrics.cost_micros
FROM campaign
WHERE campaign.bidding_strategy_type IN ('MAXIMIZE_CLICKS', 'MAXIMIZE_CONVERSIONS',
  'MAXIMIZE_CONVERSION_VALUE', 'TARGET_CPA', 'TARGET_ROAS', 'TARGET_SPEND')
  AND segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC LIMIT 25
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Excellent field selection; correct bid strategy types; missed spend threshold filter
- Key Decision Points: All bid strategy types correct; missing `metrics.cost_micros > 1000000000` (>$1K) filter
- Comparison to Intent: Near-perfect field selection; missing the spend threshold
- Where It Diverged: Missing `metrics.cost_micros > 1000000000`; missing `campaign.advertising_channel_type`

**Analysis:**
- Selected Fields: Near-identical; missing only campaign.advertising_channel_type
- Data Scope: Correct Smart Bidding types; missing >$1K spend filter
- Semantic Equivalence: Returns more campaigns (no spend threshold); otherwise equivalent

**Key Differences (with Reasoning Context):**
- Missing `metrics.cost_micros > 1000000000` — LLM didn't convert "$1K" to micros for filter
- Missing `campaign.advertising_channel_type` field

---

### 16. campaigns_shopping_campaign_performance
**Description:** Get me Shopping campaigns by spend (>$100) last 30 days - need acct and campaign details, budget, bid strategy, CPC, and conv metrics with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  campaign.bidding_strategy_type, campaign_budget.amount_micros,
  metrics.average_cpc, metrics.clicks, metrics.cost_micros,
  metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value
FROM campaign
WHERE campaign.advertising_channel_type IN ('SHOPPING')
  AND campaign.status IN ('ENABLED')
  AND segments.date DURING LAST_30_DAYS
  AND metrics.cost_micros > 100000000
ORDER by metrics.cost_micros DESC
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  campaign.bidding_strategy_type, campaign_budget.amount_micros,
  metrics.average_cpc, metrics.cost_micros, metrics.conversions,
  metrics.cost_per_conversion, metrics.conversions_value
FROM campaign
WHERE campaign.advertising_channel_type IN ('SHOPPING')
  AND segments.date DURING LAST_30_DAYS
  AND campaign.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Near-perfect match; all key fields present; only missing spend threshold and metrics.clicks
- Key Decision Points: Correctly identified all Shopping campaign fields; missing only the >$100 spend filter
- Comparison to Intent: Excellent field selection
- Where It Diverged: Missing `metrics.cost_micros > 100000000` (>$100 = 100,000,000 micros); missing `metrics.clicks`

**Analysis:**
- Selected Fields: 12/13 fields from reference — only missing metrics.clicks
- Data Scope: Correct Shopping filter and date range; missing spend threshold
- Semantic Equivalence: Very close — returns same data shape; more rows due to no spend filter

**Key Differences (with Reasoning Context):**
- Missing `metrics.cost_micros > 100000000` — same dollar-to-micros conversion issue
- Missing `metrics.clicks` (minor)

---

### 17. smart_campaign_search_terms_with_top_spend
**Description:** Show me engagement metrics from top 100 search terms by spend from Smart campaigns last 30 days. need acct and campaign info with currency, and match type.

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.advertising_channel_type, campaign.name,
  smart_campaign_search_term_view.search_term,
  segments.search_term_match_type,
  metrics.impressions, metrics.clicks, metrics.cost_micros
FROM smart_campaign_search_term_view
WHERE segments.date DURING LAST_30_DAYS
  AND campaign.advertising_channel_type IN ('SMART')
  AND metrics.clicks > 0
ORDER BY metrics.cost_micros DESC LIMIT 100
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name,
  smart_campaign_search_term_view.search_term,
  metrics.clicks, metrics.impressions, metrics.cost_micros
FROM smart_campaign_search_term_view
WHERE segments.date DURING LAST_30_DAYS
  AND campaign.advertising_channel_type IN ('SMART')
ORDER BY metrics.cost_micros DESC LIMIT 100
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified smart_campaign_search_term_view; noted match type is not available on this resource
- Key Decision Points: LLM correctly noted segments.search_term_match_type is not compatible with this resource
- Comparison to Intent: Match type omission was correct — it's not available; all other fields present
- Where It Diverged: Missing match type (correct omission); missing clicks > 0 filter; missing campaign.advertising_channel_type in SELECT

**Analysis:**
- Selected Fields: All key fields present; match type correctly noted as unavailable
- Data Scope: Correct resource, SMART filter, date range, sort, limit; missing clicks > 0
- Semantic Equivalence: Nearly identical; the rejected field (has_high_quality) was caught by validator

**Key Differences (with Reasoning Context):**
- Match type not selected — correctly identified as incompatible; reference query includes it suggesting a schema difference
- Missing `metrics.clicks > 0` filter; missing `campaign.advertising_channel_type` in SELECT

---

### 18. all_search_terms_with_clicks
**Description:** Pull performance data for all search terms with clicks last 30 days, sorted by spend, broken down by match type and device. need acct info, keyword status, full conv metrics with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  search_term_view.search_term, search_term_view.status,
  segments.search_term_match_type, segments.device,
  metrics.impressions, metrics.clicks, metrics.cost_micros,
  metrics.conversions, metrics.cost_per_conversion,
  metrics.conversions_value, metrics.conversions_value_per_cost
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0
ORDER BY metrics.cost_micros desc
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  search_term_view.search_term, search_term_view.status,
  segments.keyword.info.match_type, segments.device,
  metrics.clicks, metrics.cost_micros, metrics.impressions, metrics.ctr,
  metrics.average_cpc, metrics.conversions, metrics.conversions_value,
  metrics.cost_per_conversion, metrics.conversions_value_per_cost,
  metrics.all_conversions, metrics.all_conversions_value,
  metrics.cost_per_all_conversions, metrics.value_per_conversion,
  metrics.value_per_all_conversions, metrics.cross_device_conversions,
  metrics.view_through_conversions
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0
ORDER BY metrics.cost_micros DESC
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: All reference fields present; added comprehensive conversion metrics for "full conv metrics" request
- Key Decision Points: Used `segments.keyword.info.match_type` instead of `segments.search_term_match_type`; added many extra conversion metrics
- Comparison to Intent: Superset of reference — "full conv metrics" interpreted broadly; match type field name differs
- Where It Diverged: Different match type field name; added extensive extra conversion metrics

**Analysis:**
- Selected Fields: Superset of reference; includes all reference fields plus extras
- Data Scope: Identical — search_term_view, last 30 days, clicks > 0
- Semantic Equivalence: Returns same data, more metrics; match type field difference may be an issue

**Key Differences (with Reasoning Context):**
- `segments.keyword.info.match_type` vs `segments.search_term_match_type` — may be wrong field path
- Added many extra conversion metrics in response to "full conv metrics" request — thorough interpretation

---

### 19. search_terms_with_top_cpa
**Description:** Get me performance data for top 50 search terms by spend with CPA >$200 and spend >$1K last 30 days - broken down by match type and device. need acct info, keyword status, full conv metrics with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  search_term_view.search_term, search_term_view.status,
  segments.search_term_match_type, segments.device,
  metrics.impressions, metrics.clicks, metrics.cost_micros,
  metrics.conversions, metrics.cost_per_conversion,
  metrics.conversions_value, metrics.conversions_value_per_cost
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS
  AND metrics.cost_per_conversion > 200000000
  AND metrics.cost_micros > 1000000000
ORDER BY metrics.cost_micros desc LIMIT 50
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  search_term_view.search_term, search_term_view.status,
  segments.search_term_match_type, segments.device,
  metrics.cost_micros, metrics.cost_per_conversion, metrics.conversions,
  metrics.conversions_value, metrics.all_conversions, metrics.all_conversions_value,
  metrics.cost_per_all_conversions, metrics.clicks, metrics.impressions
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS
  AND metrics.cost_micros > 1000000000
ORDER BY metrics.cost_micros DESC LIMIT 50
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Good field selection; spend threshold correctly converted to micros; CPA filter dropped
- Key Decision Points: Correctly converted $1K to 1000000000 micros for spend; CPA >$200 filter was dropped
- Comparison to Intent: Good but missing CPA filter — would return low-CPA records too
- Where It Diverged: Missing `metrics.cost_per_conversion > 200000000` (CPA >$200)

**Analysis:**
- Selected Fields: All reference fields present plus extras
- Data Scope: Missing CPA >$200 filter — significant gap
- Semantic Equivalence: Partial — correct spend threshold; wrong data scope without CPA filter

**Key Differences (with Reasoning Context):**
- Missing `metrics.cost_per_conversion > 200000000` — LLM dropped CPA filter when converting dollar values

---

### 20. search_terms_with_low_roas
**Description:** Show me performance data for top 50 search terms by spend with ROAS <0.25 and spend >$1K last 30 days, broken down by match type and device. need acct info, full conv metrics with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  search_term_view.search_term, search_term_view.status,
  segments.search_term_match_type, segments.device,
  metrics.impressions, metrics.clicks, metrics.cost_micros,
  metrics.conversions, metrics.cost_per_conversion,
  metrics.conversions_value, metrics.conversions_value_per_cost
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS
  AND metrics.conversions_value_per_cost < 0.25
  AND metrics.cost_micros > 1000000000
ORDER BY metrics.cost_micros desc LIMIT 50
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  search_term_view.search_term,
  segments.search_term_match_type, segments.device,
  metrics.cost_micros, metrics.conversions_value_per_cost,
  metrics.conversions, metrics.conversions_value, metrics.cost_per_conversion,
  metrics.all_conversions, metrics.all_conversions_value,
  metrics.cost_per_all_conversions, metrics.clicks, metrics.impressions
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS
  AND metrics.conversions_value_per_cost < 0.25
  AND metrics.cost_micros > 1000000
ORDER BY metrics.cost_micros DESC LIMIT 50
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Both ROAS and spend filters present; however spend threshold incorrect
- Key Decision Points: ROAS filter correctly uses `metrics.conversions_value_per_cost < 0.25`; spend filter uses 1000000 instead of 1000000000
- Comparison to Intent: Both filters present but spend threshold is $1 instead of $1K
- Where It Diverged: `metrics.cost_micros > 1000000` (=$1) vs `> 1000000000` (=$1K); missing `search_term_view.status`

**Analysis:**
- Selected Fields: Almost all reference fields present; missing `search_term_view.status`
- Data Scope: ROAS filter correct; spend threshold off by 1000x ($1 instead of $1K)
- Semantic Equivalence: Nearly correct logic; spend threshold error significantly widens result set

**Key Differences (with Reasoning Context):**
- `metrics.cost_micros > 1000000` vs `> 1000000000` — $1 vs $1K threshold; dollar-to-micros conversion error
- Missing `search_term_view.status`

---

### 21. locations_with_highest_revenue_per_conversion
**Description:** Pull performance data for top 20 locations for each campaign by rev per conv (>10 conv) last 7 days - need account and campaign info, geo target IDs, and conversion metrics with currency

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type,
  campaign_criterion.criterion_id, campaign_criterion.type,
  campaign_criterion.location.geo_target_constant,
  metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.average_cpc,
  metrics.conversions, metrics.cost_per_conversion,
  metrics.conversions_value, metrics.value_per_conversion
FROM location_view
WHERE segments.date DURING LAST_7_DAYS AND metrics.conversions > 10
ORDER BY metrics.value_per_conversion DESC, metrics.conversions DESC LIMIT 20
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name,
  metrics.conversions, metrics.conversions_value, metrics.value_per_conversion
FROM location_view
WHERE segments.date DURING LAST_7_DAYS AND metrics.conversions > 10
ORDER BY metrics.value_per_conversion DESC LIMIT 20
```

**Classification:** POOR

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource and filters; field `location_view.location_geo_target_constant` was rejected as invalid
- Key Decision Points: Validator rejected geo target constant field; LLM then dropped all location/criterion fields
- Comparison to Intent: Missing all geo target and campaign criterion fields — core requirement of the query
- Where It Diverged: Phase 3 field rejection cascaded to removing all geo-related fields; missing most metrics

**Analysis:**
- Selected Fields: Missing geo target IDs, campaign_criterion fields, impressions, clicks, cost, average_cpc
- Data Scope: Correct resource, date, conversions > 10 filter — OK
- Semantic Equivalence: Would not identify the locations at all (no geo fields) — fundamentally incomplete

**Key Differences (with Reasoning Context):**
- Phase 3 rejected `location_view.location_geo_target_constant` as invalid; correct field is `campaign_criterion.location.geo_target_constant` — field selection bug
- Missing campaign_criterion.criterion_id, campaign_criterion.type, campaign_criterion.location.geo_target_constant
- Missing most performance metrics

---

### 22. asset_performance_rsa
**Description:** Get me RSA performance last 30 days. include account, campaign, adgroup info. need headline and description copy, path text, CTR, and engagement metrics. Ordered by campaign/group name and CTR.

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id, campaign.name,
  campaign.advertising_channel_type, ad_group.id, ad_group.name, ad_group.type,
  ad_group_ad.ad.id,
  ad_group_ad.ad.responsive_search_ad.headlines,
  ad_group_ad.ad.responsive_search_ad.descriptions,
  ad_group_ad.ad.responsive_search_ad.path1, ad_group_ad.ad.responsive_search_ad.path2,
  metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cpc
FROM ad_group_ad
WHERE ad_group_ad.ad.type IN ('RESPONSIVE_SEARCH_AD')
  AND segments.date DURING LAST_30_DAYS
ORDER BY campaign.name, ad_group.name, metrics.ctr DESC LIMIT 1000
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id, campaign.name,
  ad_group.id, ad_group.name, ad_group_ad.ad.id,
  ad_group_ad.ad.responsive_search_ad.headlines,
  ad_group_ad.ad.responsive_search_ad.descriptions,
  ad_group_ad.ad.responsive_search_ad.path1, ad_group_ad.ad.responsive_search_ad.path2,
  metrics.ctr, metrics.engagements, metrics.engagement_rate,
  metrics.interactions, metrics.interaction_rate
FROM ad_group_ad
WHERE segments.date DURING LAST_30_DAYS
  AND ad_group_ad.ad.type = 'RESPONSIVE_SEARCH_AD'
  AND ad_group_ad.status = 'ENABLED'
ORDER BY campaign.name ASC, ad_group.name ASC, metrics.ctr DESC
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly selected RSA fields; good ordering; "engagement metrics" used correctly here
- Key Decision Points: All RSA creative fields present; CTR present; engagement metrics added as requested
- Comparison to Intent: Very close match; missing impressions/clicks/cost but user asked for "engagement metrics" which LLM correctly interpreted
- Where It Diverged: Missing `campaign.advertising_channel_type`, `ad_group.type`, `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`, `metrics.average_cpc`; no LIMIT 1000

**Analysis:**
- Selected Fields: All RSA copy fields present; CTR present; engagement metrics added as requested by user
- Data Scope: Correct resource, RSA filter, date range, ordering
- Semantic Equivalence: Very similar; missing some supporting metrics and limit

**Key Differences (with Reasoning Context):**
- No LIMIT 1000; missing impressions/clicks/cost (not explicitly requested by user)
- `ad_group_ad.ad.type = 'RESPONSIVE_SEARCH_AD'` vs `IN ('RESPONSIVE_SEARCH_AD')` — semantically equivalent

---

### 23. recent_campaign_changes
**Description:** Show me last 100 campaign changes in the last 14 days - need account and campaign info, timestamp, user, client type, and what changed

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  change_event.change_date_time, change_event.client_type,
  change_event.change_resource_type, change_event.changed_fields,
  change_event.user_email, campaign.name
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS
  AND change_event.change_resource_type IN ('CAMPAIGN')
ORDER BY change_event.change_date_time DESC LIMIT 100
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id, campaign.name,
  change_event.change_date_time, change_event.client_type,
  change_event.changed_fields, change_event.user_email
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS
  AND change_event.change_resource_type IN ('CAMPAIGN')
ORDER BY change_event.change_date_time DESC LIMIT 100
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Perfect resource selection and field matching; correctly used change_event over change_status
- Key Decision Points: Chose change_event (correct) for granular change tracking with user/changed_fields info
- Comparison to Intent: Nearly identical to reference — all key fields present
- Where It Diverged: Missing `change_event.change_resource_type` in SELECT (but it's in WHERE)

**Analysis:**
- Selected Fields: Near-perfect — all 8 reference fields present; `change_resource_type` not in SELECT but used in filter
- Data Scope: Identical — LAST_14_DAYS, CAMPAIGN filter, sort, limit
- Semantic Equivalence: Essentially identical

**Key Differences (with Reasoning Context):**
- `change_event.change_resource_type` in WHERE but not SELECT — minor; can still filter/sort by it

---

### 24. recent_changes
**Description:** Pull recent changes across campaigns, ad groups, ads, keywords, and budgets last 14 days - need account and campaign info, client type, user, changed fields, and timestamp

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id,
  change_event.change_date_time, change_event.client_type,
  change_event.change_resource_type, change_event.changed_fields,
  change_event.user_email, campaign.name
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS
  AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET')
ORDER BY change_event.change_date_time DESC LIMIT 100
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, campaign.id, campaign.name,
  change_status.last_change_date_time, change_status.resource_type,
  change_status.resource_status
FROM change_status
WHERE change_status.last_change_date_time DURING LAST_14_DAYS
  AND change_status.resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET')
ORDER BY change_status.last_change_date_time DESC
```

**Classification:** POOR (upgraded to FAIR)

**LLM Explanation Analysis:**
- Reasoning Summary: LLM chose `change_status` over `change_event` — wrong resource selection
- Key Decision Points: LLM reasoning stated change_status is "standard" for recent changes, but it lacks client_type, user_email, and changed_fields
- Comparison to Intent: Wrong resource; user explicitly needed "client type, user, changed fields" — all unavailable on change_status
- Where It Diverged: Wrong primary resource (change_status vs change_event); Phase 3 rejected `change_status.changed_fields` as invalid; missing client_type and user_email

**Analysis:**
- Selected Fields: Missing client_type, user_email, changed_fields — core requested fields
- Data Scope: resource_type filter correct; `change_status.resource_status` is not the same as changed_fields
- Semantic Equivalence: Wrong resource — returns status updates, not granular change events with user attribution

**Key Differences (with Reasoning Context):**
- Used `change_status` instead of `change_event` — fundamental resource selection error when multiple resource types were specified
- contrast with entry 23 which correctly used change_event for similar query but single resource type

---

### 25. all_campaigns
**Description:** Get me basic campaign info - IDs, names, channel types, and status. include account info and currency.

**Reference Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name,
  campaign.advertising_channel_type, campaign.status, campaign.primary_status
FROM campaign
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name, customer.currency_code,
  campaign.id, campaign.name, campaign.advertising_channel_type, campaign.status
FROM campaign
WHERE campaign.status = 'ENABLED'
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource and nearly all fields; implicit ENABLED filter added
- Key Decision Points: Added `campaign.status = 'ENABLED'` implicit filter — limits results to active campaigns only
- Comparison to Intent: "Get me basic campaign info" implies all campaigns; implicit status filter excludes paused/removed
- Where It Diverged: Missing `campaign.primary_status`; implicit ENABLED filter changes scope

**Analysis:**
- Selected Fields: Missing `campaign.primary_status`
- Data Scope: Added WHERE clause not in reference — filters out non-ENABLED campaigns
- Semantic Equivalence: Returns subset of campaigns (ENABLED only) vs all campaigns

**Key Differences (with Reasoning Context):**
- `WHERE campaign.status = 'ENABLED'` implicit filter — restricts to active campaigns only; reference has no filter
- Missing `campaign.primary_status`

---

### 26. performance_max_impression_share
**Description:** Show me daily impression share metrics for PMax last 30 days - need absolute top, budget lost, rank lost, and top impression share

**Reference Query:**
```sql
SELECT
  campaign.id, campaign.name, campaign.advertising_channel_type, segments.date,
  metrics.impressions,
  metrics.search_absolute_top_impression_share,
  metrics.search_budget_lost_absolute_top_impression_share,
  metrics.search_budget_lost_impression_share,
  metrics.search_budget_lost_top_impression_share,
  metrics.search_exact_match_impression_share,
  metrics.search_impression_share,
  metrics.search_rank_lost_impression_share,
  metrics.search_top_impression_share,
  metrics.absolute_top_impression_percentage
FROM campaign
WHERE segments.date DURING LAST_30_DAYS
  AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
ORDER BY campaign.name
```

**Generated Query:**
```sql
SELECT
  campaign.id, campaign.name, segments.date,
  metrics.search_absolute_top_impression_share,
  metrics.search_budget_lost_absolute_top_impression_share,
  metrics.search_rank_lost_impression_share,
  metrics.search_top_impression_share
FROM campaign
WHERE segments.date DURING LAST_30_DAYS
  AND campaign.advertising_channel_type = 'PERFORMANCE_MAX'
  AND campaign.status = 'ENABLED'
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified 4 of the key impression share metrics; missing broader impression share metrics from reference
- Key Decision Points: Mapped "absolute top, budget lost, rank lost, top impression share" exactly to 4 metrics
- Comparison to Intent: User asked for 4 specific metrics — all 4 present; reference includes many more impression share variants
- Where It Diverged: Missing `search_budget_lost_impression_share`, `search_budget_lost_top_impression_share`, `search_exact_match_impression_share`, `search_impression_share`, `absolute_top_impression_percentage`, `metrics.impressions`, `campaign.advertising_channel_type` in SELECT

**Analysis:**
- Selected Fields: 4 explicitly requested metrics present; missing 5 additional impression share variants from reference
- Data Scope: Same PMax filter, date range; no ORDER BY
- Semantic Equivalence: Good for stated requirements; reference is more comprehensive

**Key Differences (with Reasoning Context):**
- LLM selected exactly the 4 metrics named by user; reference includes comprehensive impression share set
- Missing `campaign.advertising_channel_type` in SELECT; `= 'PERFORMANCE_MAX'` vs `IN ('PERFORMANCE_MAX')`

---

### 27. asset_performance_by_type_ytd
**Description:** Show me daily asset performance broken down by asset field type for this year - need impressions, clicks, cost with currency

**Reference Query:**
```sql
SELECT
  asset_field_type_view.field_type, segments.date,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM asset_field_type_view
WHERE segments.year IN (2026) AND metrics.impressions > 1
ORDER BY asset_field_type_view.field_type, segments.date
```

**Generated Query:**
```sql
SELECT
  customer.id, customer.descriptive_name,
  asset_field_type_view.field_type, segments.date,
  metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
FROM asset_field_type_view
WHERE segments.date BETWEEN '2026-01-01' AND '2026-03-27'
ORDER BY asset_field_type_view.field_type ASC, segments.date ASC
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource; all requested metrics present; YTD date handling smart but static
- Key Decision Points: Added `customer.id` and `customer.descriptive_name` for MCC context; BETWEEN instead of year filter
- Comparison to Intent: Excellent match on all explicitly requested fields
- Where It Diverged: BETWEEN with hardcoded dates vs `segments.year IN (2026)`; extra customer identity fields; missing `metrics.impressions > 1` filter

**Analysis:**
- Selected Fields: All reference fields present; added customer identity fields (useful enhancement for MCC context)
- Data Scope: Same YTD scope; different date filter syntax (BETWEEN vs year); missing impressions > 1
- Semantic Equivalence: Very close; BETWEEN will capture same data but hardcodes today's date

**Key Differences (with Reasoning Context):**
- `BETWEEN '2026-01-01' AND '2026-03-27'` vs `segments.year IN (2026)` — BETWEEN is static (won't auto-update), year filter is dynamic
- Extra customer identity fields — good MCC enhancement
- Missing `metrics.impressions > 1` threshold

---

## Overall Assessment

### Score Summary
| Rating | Count | % |
|--------|-------|---|
| EXCELLENT | 12 | 44% |
| GOOD | 9 | 33% |
| FAIR | 5 | 19% |
| POOR | 1 | 4% |

**Combined EXCELLENT+GOOD: 77%** — strong overall performance.

### Systemic Issues

#### 1. "Engagement Metrics" Misinterpretation (Entries 4-10)
The most prevalent failure pattern: when users say "engagement metrics" in the context of campaign performance, the LLM interprets this as Google Ads engagement-specific metrics (`metrics.engagements`, `metrics.engagement_rate`, `metrics.interactions`) rather than general performance metrics (`metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`). This affects entries 4, 5, 6, 7, 8, 9, 10.

**Recommendation:** Add a note to the field metadata or LLM prompt clarifying that "engagement metrics" in a campaign context typically means impressions/clicks/cost, not the specific `metrics.engagements` field.

#### 2. Dollar-to-Micros Threshold Conversion (Entries 15, 16, 19, 20)
The LLM sometimes fails to correctly convert dollar spend thresholds to micros:
- Entry 15: Missing `> 1000000000` (=$1K) entirely
- Entry 16: Missing `> 100000000` (=$100) entirely
- Entry 19: Correct `> 1000000000` for $1K
- Entry 20: Incorrect `> 1000000` (=$1 instead of $1K)

**Recommendation:** Improve threshold conversion logic to consistently convert dollar amounts to micros (multiply by 1,000,000).

#### 3. Implicit `status = 'ENABLED'` Filter (Entries 4, 5, 6, 7, 8, 13, 15, 16, 22, 25)
The LLM consistently adds an implicit `campaign.status = 'ENABLED'` filter even when not requested. For entry 25 (all_campaigns), this was particularly problematic as the query was intended to show all campaigns including paused/removed.

**Recommendation:** Make implicit status filter opt-in rather than default, or only apply when query is clearly operational (e.g., "top active campaigns").

#### 4. Wrong Resource: change_status vs change_event (Entry 24)
When querying for recent changes with granular details (user email, changed fields, client type), the LLM chose `change_status` instead of `change_event`. The `change_event` resource correctly supports all these fields. Entry 23 (single resource type) correctly used `change_event`.

**Recommendation:** Improve resource descriptions or add examples that distinguish when to use `change_event` vs `change_status`.

#### 5. Geo Target Field Rejection (Entry 21)
Phase 3 rejected `location_view.location_geo_target_constant` as invalid. The correct field path is `campaign_criterion.location.geo_target_constant`. This validation error caused all geo-related fields to be dropped, making the query fundamentally incomplete.

**Recommendation:** Improve field path suggestions for location-related queries to use `campaign_criterion.location.geo_target_constant`.

#### 6. Low RAG Confidence (All entries)
Every single entry showed Phase 1 RAG confidence below 0.30, triggering fallback to the full resource list. This suggests the RAG index may not be well-calibrated for these types of queries, or the query cookbook embeddings need to be rebuilt/updated.

**Recommendation:** Re-index the query cookbook with updated embeddings, or investigate why RAG confidence is consistently low for cookbook-style queries.

### Positive Patterns

1. **Resource Selection Accuracy**: Despite low RAG confidence, the LLM correctly identified the primary resource in 26/27 cases (96%). Only entry 24 had a wrong resource.

2. **Filter Logic Quality**: Filters like date ranges, channel type filters (PERFORMANCE_MAX, SHOPPING, SMART, etc.), and threshold conditions were generally correct.

3. **Identity Field Inclusion**: The LLM consistently included customer.id, customer.descriptive_name, campaign.id, campaign.name — good MCC-context awareness.

4. **Domain Intelligence**: Entry 10 (call extensions) showed domain awareness by using `metrics.phone_impressions` instead of generic `metrics.impressions` for call extension queries.

5. **Field Validation Working**: Phase 3 validation correctly rejected invalid fields (e.g., `smart_campaign_search_term_view.has_high_quality`, `change_status.changed_fields`).

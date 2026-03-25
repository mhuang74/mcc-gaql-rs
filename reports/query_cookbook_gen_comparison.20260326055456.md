# Query Cookbook Generation Comparison Report

**Timestamp:** 20260326055456
**Test Environment:** mcc-gaql-gen generate with --use-query-cookbook --explain flags
**Model:** zai-org/glm-4.7

## Summary Statistics

- Total entries tested: 26
- **EXCELLENT:** 5 (19%)
- **GOOD:** 7 (27%)
- **FAIR:** 5 (19%)
- **POOR:** 9 (35%)

### Critical Issues Identified

| Issue Type | Count | Description |
|------------|-------|-------------|
| **Missing date filters** | 10 queries | LLM reasoning correctly identifies need for `segments.date` filter, but final query omits it |
| **Missing account fields** | 7 queries | `customer.id` and/or `customer.descriptive_name` missing despite being requested |
| **Missing metrics** | 3 queries | Key metrics like `impressions`, `segments.date` for daily breakdown missing |
| **Wrong resource** | 3 queries | Resource selection differs from reference (e.g., customer_asset vs campaign_asset) |

---

## Detailed Results

### 1. account_ids_with_access_and_traffic_last_week

**Description:** Get me account IDs with clicks in the last week

**Classification:** POOR

**Reference Query:**
```sql
SELECT
	customer.id
FROM customer
WHERE
	segments.date during LAST_WEEK_MON_SUN
	AND metrics.clicks > 1
```

**Generated Query:**
```sql
SELECT
  customer.id
FROM customer
WHERE metrics.clicks > 0
```

**LLM Explanation Analysis:**
- Reasoning Summary: The LLM correctly identified the need for `customer.id`, `metrics.clicks` filter, and importantly stated it would add `segments.date DURING LAST_WEEK_MON_SUN` for the "last week" requirement
- Key Decision Points: Selected customer resource, added clicks > 0 filter
- **Critical Divergence:** LLM explicitly stated "I added a segments.date filter using DURING LAST_WEEK_MON_SUN" but this filter is **completely missing** from the final WHERE clause

**Analysis:**
- Selected Fields: Correct (customer.id)
- Data Scope: Missing critical date filter - query would return all-time data instead of last week
- Semantic Equivalence: **NOT equivalent** - missing temporal constraint makes query semantically different

**Key Differences:**
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` filter (LLM stated it added this but it's not in output)
- ⚠️ Threshold difference: clicks > 0 vs > 1 (acceptable)

---

### 2. accounts_with_traffic_last_week

**Description:** Show me account-level performance last week - need impressions, clicks, spend, and currency

**Classification:** FAIR

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
	AND metrics.impressions > 1
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
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM correctly mapped "last week" to `LAST_WEEK_MON_SUN` and selected appropriate fields
- Key Decision Points: Identified all required fields including currency_code
- **Divergence:** LLM stated it would add date filter but it's missing from final output

**Analysis:**
- Selected Fields: Good - all requested fields present
- Data Scope: **Missing date filter** - no `segments.date` filter despite "last week" in query
- Semantic Equivalence: Would return all-time account performance instead of last week only

**Key Differences:**
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` filter
- ❌ Missing `metrics.impressions > 1` filter

---

### 3. keywords_with_top_traffic_last_week

**Description:** Pull my top 10 keywords by clicks (>10K) last week - need acct, campaign, ad group IDs + names, channel type, and currency

**Classification:** FAIR

**Reference Query:**
```sql
SELECT
	customer.id,
	customer.descriptive_name,
	campaign.id,
	campaign.name,
	campaign.advertising_channel_type,
	ad_group.id,
	ad_group.name,
	ad_group.type,
	ad_group_criterion.criterion_id,
	ad_group_criterion.keyword.text,
	metrics.impressions,
	metrics.clicks,
	metrics.cost_micros,
	customer.currency_code
FROM keyword_view
WHERE
	segments.date DURING LAST_WEEK_MON_SUN
	and metrics.clicks > 10000
ORDER BY
	metrics.clicks DESC
LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  ad_group.id,
  ad_group.name,
  campaign.advertising_channel_type,
  customer.currency_code,
  metrics.clicks
FROM keyword_view
WHERE metrics.clicks > 10000 AND keyword_view.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 10
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM correctly identified keyword_view resource and selected most required fields
- Key Decision Points: Used LAST_WEEK_MON_SUN for date filter in reasoning
- **Divergence:** Date filter mentioned in reasoning but absent from final query

**Analysis:**
- Selected Fields: Missing several reference fields:
  - `ad_group.type`
  - `ad_group_criterion.criterion_id`
  - `ad_group_criterion.keyword.text`
  - `metrics.impressions`
  - `metrics.cost_micros`
- Data Scope: Missing `segments.date DURING LAST_WEEK_MON_SUN`
- Semantic Equivalence: Core intent captured but missing keyword text and several metrics

**Key Differences:**
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` filter
- ❌ Missing keyword identification fields (criterion_id, keyword.text)
- ❌ Missing impressions and cost metrics
- ⚠️ Added implicit `keyword_view.status = 'ENABLED'` filter

---

### 4. accounts_with_perf_max_campaigns_last_week

**Description:** Get me the top PMax campaign by clicks (>100) per account last week - need acct and campaign IDs + names, channel type, and currency

**Classification:** GOOD

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
  AND metrics.clicks > 100
ORDER BY
  metrics.clicks DESC
LIMIT 1
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks,
  customer.currency_code
FROM campaign
WHERE
  campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
  AND metrics.clicks > 100
  AND campaign.status = 'ENABLED'
ORDER BY
  metrics.clicks DESC
LIMIT 1
```

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified campaign resource, PMax filter, and all key fields
- Key Decision Points: Properly structured query with LIMIT 1 for "top" campaign

**Analysis:**
- Selected Fields: Good - all essential fields present (minor: missing impressions, cost_micros)
- Data Scope: Missing date filter for "last week", but reference would be functionally similar with proper date filter
- Semantic Equivalence: Would work correctly with date filter added

**Key Differences:**
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` filter
- ⚠️ Missing `metrics.impressions` and `metrics.cost_micros` (enhancement fields)
- ⚠️ Added implicit `campaign.status = 'ENABLED'` filter

---

### 5. accounts_with_smart_campaigns_last_week

**Description:** Show me the top Smart campaign by clicks (>100) per account last week - need acct and campaign details with currency

**Classification:** GOOD

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('SMART')
  AND metrics.clicks > 100
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks,
  customer.currency_code
FROM campaign
WHERE
  campaign.advertising_channel_type IN ('SMART')
  AND metrics.clicks > 100
  AND campaign.status = 'ENABLED'
ORDER BY
  metrics.clicks DESC
LIMIT 1
```

**Analysis:**
- Selected Fields: Good - core fields present
- Data Scope: Missing date filter but otherwise correct
- Semantic Equivalence: Correct resource and filters, would work with date filter

**Key Differences:**
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` filter
- ⚠️ Missing impressions and cost_micros
- ⚠️ Added ORDER BY and LIMIT 1 (reference didn't have this but it's an improvement)

---

### 6. accounts_with_local_campaigns_last_week

**Description:** Pull the top Local campaign by clicks (>500) per account last week - need acct and campaign info with currency

**Classification:** FAIR

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('LOCAL')
  AND metrics.clicks > 500
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks
FROM campaign
WHERE campaign.advertising_channel_type IN ('LOCAL') AND metrics.clicks > 500 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 1
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM explicitly stated it would include `customer.id`, `customer.descriptive_name`, and `segments.date DURING LAST_WEEK_MON_SUN`
- **Critical Divergence:** Both account identifier fields AND date filter missing from final output despite being stated in reasoning

**Analysis:**
- Selected Fields: **Missing critical fields** - `customer.id` and `customer.descriptive_name` not in output despite LLM stating they were selected
- Data Scope: Missing date filter
- Semantic Equivalence: Cannot identify which account each campaign belongs to

**Key Differences:**
- ❌ Missing `customer.id` and `customer.descriptive_name` (LLM said it selected these)
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` (LLM said it added this)
- ❌ Missing `metrics.impressions` and `metrics.cost_micros`

---

### 7. accounts_with_shopping_campaigns_last_week

**Description:** Get me the top Shopping campaign by clicks (>100) per account last week - need acct and campaign details with currency

**Classification:** GOOD

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('SHOPPING')
  AND metrics.clicks > 100
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks,
  customer.currency_code
FROM campaign
WHERE
  campaign.advertising_channel_type IN ('SHOPPING')
  AND metrics.clicks > 100
  AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 1
```

**Analysis:**
- Selected Fields: Good - account and campaign identifiers present
- Data Scope: Missing date filter
- Semantic Equivalence: Correct structure, would work with date filter

**Key Differences:**
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` filter
- ⚠️ Missing impressions and cost_micros

---

### 8. accounts_with_multichannel_campaigns_last_week

**Description:** Show me the top Multi-Channel campaign by clicks (>100) per account last week - need acct and campaign info with currency

**Classification:** GOOD

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign.advertising_channel_type IN ('MULTI_CHANNEL')
  AND metrics.clicks > 100
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks,
  customer.currency_code
FROM campaign
WHERE
  campaign.advertising_channel_type IN ('MULTI_CHANNEL')
  AND metrics.clicks > 100
  AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 10
```

**Analysis:**
- Selected Fields: Good - all essential fields present
- Data Scope: Missing date filter
- Semantic Equivalence: Correct resource and structure

**Key Differences:**
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` filter
- ⚠️ LIMIT 10 instead of reference's implied 1 (minor difference)

---

### 9. accounts_with_asset_sitelink_last_week

**Description:** Pull the top Sitelinks by impressions (>20K clicks) per account last week - need acct and campaign details with currency

**Classification:** POOR

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  asset.id,
  asset.name,
  asset.sitelink_asset.link_text,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign_asset
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'SITELINK'
  AND metrics.clicks > 20000
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  asset.id,
  asset.name,
  asset.sitelink_asset.link_text,
  metrics.impressions,
  metrics.clicks
FROM customer_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.clicks > 20000
ORDER BY metrics.impressions DESC
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM correctly identified need for sitelink-specific fields and date filter
- Key Decision Points: Selected customer_asset resource (account-level) vs campaign_asset
- **Divergence:** Used customer_asset instead of campaign_asset; missing `campaign_asset.field_type = 'SITELINK'` filter

**Analysis:**
- Selected Fields: Good selection of fields
- Data Scope: **Wrong resource** - `customer_asset` vs `campaign_asset`; missing field_type filter
- Semantic Equivalence: Would return different data (account-level vs campaign-level sitelinks)

**Key Differences:**
- ❌ Wrong resource: `customer_asset` instead of `campaign_asset`
- ❌ Missing `campaign_asset.field_type = 'SITELINK'` filter
- ❌ Missing `metrics.cost_micros`

---

### 10. accounts_with_asset_call_last_week

**Description:** Get me the top Call Extensions by impressions (>100) per account last week - need acct and campaign info with currency

**Classification:** POOR

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  asset.id,
  asset.name,
  asset.type,
  asset.call_asset.phone_number,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign_asset
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'CALL'
  AND metrics.impressions > 100
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign_asset.field_type,
  asset.call_asset.phone_number,
  metrics.impressions,
  metrics.clicks
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'CALL' AND metrics.impressions > 100
ORDER BY metrics.impressions DESC
LIMIT 10
```

**Analysis:**
- Selected Fields: **Missing critical account fields** - `customer.id` and `customer.descriptive_name` not present
- Data Scope: Correct resource (campaign_asset) and date filter present
- Semantic Equivalence: Cannot identify which account each call extension belongs to

**Key Differences:**
- ❌ Missing `customer.id` and `customer.descriptive_name`
- ❌ Missing `asset.id`, `asset.name`, `asset.type`
- ❌ Missing `metrics.cost_micros`
- ❌ Missing `campaign.advertising_channel_type`

---

### 11. accounts_with_asset_callout_last_week

**Description:** Show me the top Callout Extensions by impressions (>30K clicks) per account last week - need acct and campaign details with currency

**Classification:** POOR

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  campaign_asset.asset,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign_asset
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'CALLOUT'
  AND metrics.clicks > 30000
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign_asset.field_type,
  asset.callout_asset.callout_text,
  metrics.impressions,
  metrics.clicks
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'CALLOUT' AND metrics.clicks > 30000
ORDER BY metrics.impressions DESC
```

**Analysis:**
- Selected Fields: **Missing critical account and asset fields**
- Data Scope: Correct resource, date filter, and field_type filter present
- Semantic Equivalence: Missing account identifiers prevents per-account analysis

**Key Differences:**
- ❌ Missing `customer.id` and `customer.descriptive_name`
- ❌ Missing `campaign.advertising_channel_type`
- ❌ Missing `campaign_asset.asset`
- ❌ Missing `metrics.cost_micros`

---

### 12. accounts_with_asset_app_last_week

**Description:** Pull the top App Extensions by impressions (>1) per account last week - need acct and campaign info with currency

**Classification:** POOR

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  campaign_asset.asset,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign_asset
WHERE
  segments.date DURING LAST_WEEK_MON_SUN
  AND campaign_asset.field_type = 'APP'
  AND metrics.impressions > 1
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign_asset.field_type,
  metrics.impressions
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'APP' AND metrics.impressions > 1
ORDER BY metrics.impressions DESC
```

**Analysis:**
- Selected Fields: **Missing most account and campaign fields**
- Data Scope: Correct resource and filters present
- Semantic Equivalence: Severely limited without account identifiers

**Key Differences:**
- ❌ Missing `customer.id` and `customer.descriptive_name`
- ❌ Missing `campaign.advertising_channel_type`
- ❌ Missing `campaign_asset.asset`
- ❌ Missing `metrics.clicks` and `metrics.cost_micros`

---

### 13. perf_max_campaigns_with_traffic_last_30_days

**Description:** Give me daily PMax performance last 30 days - need CTR, CPC, conv, rev, CPA, campaign IDs + names, and currency

**Classification:** POOR

**Reference Query:**
```sql
SELECT
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  segments.date,
  metrics.impressions,
  metrics.clicks,
  metrics.ctr,
  metrics.cost_micros,
  metrics.average_cost,
  metrics.conversions,
  metrics.conversions_value,
  metrics.cost_per_conversion,
  customer.currency_code
FROM campaign
WHERE
  segments.date DURING LAST_30_DAYS
  AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
  AND metrics.impressions > 1
ORDER BY
  segments.date, campaign.id
```

**Generated Query:**
```sql
SELECT
  campaign.id,
  campaign.name,
  metrics.ctr,
  metrics.average_cpc,
  metrics.conversions,
  metrics.conversions_value,
  metrics.cost_per_conversion,
  customer.currency_code
FROM campaign
WHERE campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND campaign.status = 'ENABLED'
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM stated it would add `segments.date` filter for `LAST_30_DAYS` and include `segments.date` field for daily breakdown
- **Critical Divergence:** Neither date filter nor `segments.date` field present in final output despite explicit mention in reasoning

**Analysis:**
- Selected Fields: **Missing critical fields**:
  - `segments.date` (essential for "daily" breakdown)
  - `campaign.advertising_channel_type`
  - `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`
- Data Scope: **Missing date filter** despite "last 30 days" requirement
- Semantic Equivalence: Cannot provide daily breakdown without `segments.date`

**Key Differences:**
- ❌ Missing `segments.date` field (critical for daily breakdown)
- ❌ Missing `segments.date DURING LAST_30_DAYS` filter
- ❌ Missing `campaign.advertising_channel_type`
- ❌ Missing basic metrics (impressions, clicks, cost_micros)
- ⚠️ Used `metrics.average_cpc` instead of `metrics.average_cost`

---

### 14. asset_fields_with_traffic_ytd

**Description:** Show me YTD asset performance by day with impressions, asset type, and currency

**Classification:** POOR

**Reference Query:**
```sql
SELECT
  asset_field_type_view.field_type,
  segments.date,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM asset_field_type_view
WHERE
  segments.year IN (2026)
  AND metrics.impressions > 1
ORDER BY
  asset_field_type_view.field_type, segments.date
```

**Generated Query:**
```sql
SELECT
  asset.type,
  customer.currency_code
FROM asset
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM mentioned selecting `segments.date`, `metrics.impressions`, `asset.type`, and calculating YTD date range
- **Critical Divergence:** Only `asset.type` and `customer.currency_code` in final output - missing all other fields

**Analysis:**
- Selected Fields: **Severely incomplete** - only 2 of 6 required fields present
- Data Scope: **Wrong resource** - `asset` instead of `asset_field_type_view`
- Semantic Equivalence: Query is essentially non-functional for the requested analysis

**Key Differences:**
- ❌ Wrong resource: `asset` instead of `asset_field_type_view`
- ❌ Missing `segments.date` for daily breakdown
- ❌ Missing `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`
- ❌ Missing `asset_field_type_view.field_type`
- ❌ Missing all WHERE clause filters

---

### 15. campaigns_with_smart_bidding_by_spend

**Description:** Pull top 25 Smart Bidding campaigns by spend (>$1K) last week - need acct and campaign IDs + names, budget, bid strategy, CPC, and conv metrics with currency

**Classification:** FAIR

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  campaign.bidding_strategy_type,
  campaign_budget.amount_micros,
  metrics.average_cpc,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value
FROM campaign
WHERE
  campaign.bidding_strategy_type IN ('MAXIMIZE_CLICKS', 'MAXIMIZE_CONVERSIONS', 'MAXIMIZE_CONVERSION_VALUE', 'TARGET_CPA', 'TARGET_ROAS', 'TARGET_SPEND')
  AND campaign.status IN ('ENABLED')
  AND segments.date DURING LAST_WEEK_MON_SUN
  AND metrics.cost_micros > 1000000000
ORDER by metrics.cost_micros DESC
LIMIT 25
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.bidding_strategy_type,
  campaign_budget.amount_micros,
  metrics.average_cpc,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value
FROM campaign
WHERE campaign.bidding_strategy_type IN ('MAXIMIZE_CLICKS', 'MAXIMIZE_CONVERSIONS', 'MAXIMIZE_CONVERSION_VALUE', 'TARGET_CPA', 'TARGET_ROAS', 'TARGET_SPEND') AND metrics.cost_micros > 1000000000 AND campaign.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 25
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM correctly identified Smart Bidding types and stated it would add `LAST_WEEK_MON_SUN` date filter
- **Divergence:** Date filter mentioned in reasoning but not present in output

**Analysis:**
- Selected Fields: Good coverage of metrics; missing `customer.id`, `customer.descriptive_name`, `campaign.advertising_channel_type`, `metrics.clicks`
- Data Scope: Missing date filter for "last week"
- Semantic Equivalence: Core functionality present but missing account identification

**Key Differences:**
- ❌ Missing `customer.id` and `customer.descriptive_name`
- ❌ Missing `campaign.advertising_channel_type`
- ❌ Missing `metrics.clicks`
- ❌ Missing `segments.date DURING LAST_WEEK_MON_SUN` filter

---

### 16. campaigns_shopping_campaign_performance

**Description:** Get me Shopping campaigns by spend (>$100) last 30 days - need acct and campaign details, budget, bid strategy, CPC, and conv metrics with currency

**Classification:** FAIR

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  campaign.bidding_strategy_type,
  campaign_budget.amount_micros,
  metrics.average_cpc,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value
FROM campaign
WHERE
  campaign.advertising_channel_type IN ('SHOPPING')
  AND campaign.status IN ('ENABLED')
  AND segments.date DURING LAST_30_DAYS
  AND metrics.cost_micros > 100000000
ORDER by metrics.cost_micros DESC
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.bidding_strategy_type,
  campaign_budget.amount_micros,
  metrics.average_cpc,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value
FROM campaign
WHERE campaign.advertising_channel_type IN ('SHOPPING') AND campaign.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
```

**Analysis:**
- Selected Fields: Missing several fields including account identifiers
- Data Scope: Missing date filter and spend threshold
- Semantic Equivalence: Core shopping campaign filter present but incomplete

**Key Differences:**
- ❌ Missing `customer.id` and `customer.descriptive_name`
- ❌ Missing `campaign.advertising_channel_type` (in WHERE but not SELECT)
- ❌ Missing `metrics.clicks`, `metrics.cost_micros`
- ❌ Missing `segments.date DURING LAST_30_DAYS` filter
- ❌ Missing `metrics.cost_micros > 100000000` spend threshold

---

### 17. smart_campaign_search_terms_with_top_spend

**Description:** Show me top 100 search terms by spend from Smart campaigns last 30 days - need search term, match type, and performance metrics with currency

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  smart_campaign_search_term_view.search_term,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM smart_campaign_search_term_view
WHERE
  segments.date DURING LAST_30_DAYS
  AND campaign.advertising_channel_type IN ('SMART')
  AND metrics.clicks > 0
ORDER BY
  metrics.cost_micros DESC
LIMIT 100
```

**Generated Query:**
```sql
SELECT
  smart_campaign_search_term_view.search_term,
  campaign.keyword_match_type,
  metrics.cost_micros,
  metrics.clicks,
  metrics.impressions,
  customer.currency_code
FROM smart_campaign_search_term_view
WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('SMART')
ORDER BY metrics.cost_micros DESC
LIMIT 100
```

**Analysis:**
- Selected Fields: Good selection - includes search term, match type, performance metrics, currency
- Data Scope: Correct resource, date filter, and Smart campaign filter present
- Semantic Equivalence: Would return conceptually similar data

**Key Differences:**
- ⚠️ Missing `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`
- ⚠️ Used `campaign.keyword_match_type` instead of `segments.search_term_match_type`
- ⚠️ Missing `metrics.clicks > 0` filter

---

### 18. all_search_terms_with_clicks

**Description:** Pull all search terms with clicks last 30 days - need match type, device, keyword status, and full conv metrics with currency

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  search_term_view.search_term,
  segments.search_term_match_type,
  segments.device,
  search_term_view.status,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value,
  metrics.conversions_value_per_cost
FROM search_term_view
WHERE
  segments.date DURING LAST_30_DAYS
  AND metrics.clicks > 0
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  search_term_view.search_term,
  segments.search_term_match_type,
  segments.device,
  search_term_view.status,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.conversions_value,
  metrics.all_conversions,
  metrics.all_conversions_value
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0 AND search_term_view.status = 'ENABLED'
```

**Analysis:**
- Selected Fields: Excellent coverage including all key fields
- Data Scope: Correct resource, date filter, and clicks filter present
- Semantic Equivalence: Would return nearly identical data

**Key Differences:**
- ⚠️ Missing `metrics.cost_per_conversion` and `metrics.conversions_value_per_cost`
- ⚠️ Added `metrics.all_conversions` and `metrics.all_conversions_value` (additional data)
- ⚠️ Added implicit `search_term_view.status = 'ENABLED'` filter

---

### 19. search_terms_with_top_cpa

**Description:** Get me top 50 search terms with CPA >$200 and spend >$1K last 30 days - flagging expensive underperformers with currency

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  search_term_view.search_term,
  segments.search_term_match_type,
  segments.device,
  search_term_view.status,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value,
  metrics.conversions_value_per_cost
FROM search_term_view
WHERE
  segments.date DURING LAST_30_DAYS
  AND metrics.cost_per_conversion > 200000000
  AND metrics.cost_micros > 1000000000
ORDER BY
  metrics.cost_micros desc
LIMIT 50
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  search_term_view.search_term,
  metrics.cost_micros,
  metrics.cost_per_conversion,
  metrics.conversions,
  metrics.clicks,
  metrics.impressions
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.cost_per_conversion > 200000000 AND metrics.cost_micros > 1000000000 AND search_term_view.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 50
```

**Analysis:**
- Selected Fields: Good selection of core metrics for CPA analysis
- Data Scope: Correct resource, date filter, CPA threshold, and spend threshold present
- Semantic Equivalence: Would identify the same expensive underperformers

**Key Differences:**
- ⚠️ Missing `customer.id`, `customer.descriptive_name`
- ⚠️ Missing `segments.search_term_match_type`, `segments.device`, `search_term_view.status`
- ⚠️ Missing `metrics.conversions_value`, `metrics.conversions_value_per_cost`
- ⚠️ Added implicit `search_term_view.status = 'ENABLED'` filter

---

### 20. search_terms_with_low_roas

**Description:** Show me top 50 search terms with ROAS <0.25 and spend >$1K last 30 days - need to neg keyword these with currency

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  search_term_view.search_term,
  segments.search_term_match_type,
  segments.device,
  search_term_view.status,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value,
  metrics.conversions_value_per_cost
FROM search_term_view
WHERE
  segments.date DURING LAST_30_DAYS
  AND metrics.conversions_value_per_cost < 0.25
  AND metrics.cost_micros > 1000000000
ORDER BY
  metrics.cost_micros desc
LIMIT 50
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  search_term_view.search_term,
  segments.search_term_match_type,
  segments.device,
  search_term_view.status,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value,
  metrics.conversions_value_per_cost
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < 0.25 AND metrics.cost_micros > 1000000000 AND search_term_view.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 50
```

**Analysis:**
- Selected Fields: Excellent - all key fields present including account identifiers
- Data Scope: Correct resource, date filter, ROAS threshold, and spend threshold present
- Semantic Equivalence: Would return nearly identical data for negative keyword identification

**Key Differences:**
- ⚠️ Added implicit `search_term_view.status = 'ENABLED'` filter

---

### 21. locations_with_highest_revenue_per_conversion

**Description:** Pull top 1000 locations by rev per conv (>10 conv) last 7 days - need geo target IDs and location metrics with currency

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
	customer.id,
	customer.descriptive_name,
	campaign.id,
	campaign.name,
	campaign.advertising_channel_type,
	campaign_criterion.criterion_id,
	campaign_criterion.type,
	campaign_criterion.location.geo_target_constant,
	campaign_criterion.keyword.text,
	metrics.impressions,
	metrics.clicks,
	metrics.cost_micros,
	customer.currency_code,
	metrics.conversions,
	metrics.cost_per_conversion,
	metrics.conversions_value,
	metrics.value_per_conversion,
	metrics.average_cpc
FROM location_view
WHERE
  segments.date DURING LAST_7_DAYS
  and metrics.conversions > 10
ORDER BY
  metrics.value_per_conversion desc, metrics.conversions desc
LIMIT 1000
```

**Generated Query:**
```sql
SELECT
  campaign_criterion.location.geo_target_constant,
  metrics.value_per_conversion,
  metrics.conversions,
  metrics.conversions_value,
  customer.currency_code
FROM location_view
WHERE segments.date DURING LAST_7_DAYS AND metrics.conversions > 10
ORDER BY metrics.value_per_conversion DESC
LIMIT 1000
```

**Analysis:**
- Selected Fields: Core location and conversion metrics present; missing some reference fields
- Data Scope: Correct resource, date filter, and conversion threshold present
- Semantic Equivalence: Would identify the same high-value locations

**Key Differences:**
- ⚠️ Missing `customer.id`, `customer.descriptive_name`
- ⚠️ Missing `campaign.id`, `campaign.name`, `campaign.advertising_channel_type`
- ⚠️ Missing `campaign_criterion.criterion_id`, `campaign_criterion.type`
- ⚠️ Missing `campaign_criterion.keyword.text`
- ⚠️ Missing `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`, `metrics.cost_per_conversion`, `metrics.average_cpc`
- ⚠️ Simplified ORDER BY (only value_per_conversion, not secondary conversions sort)

---

### 22. asset_performance_rsa

**Description:** Get me RSA performance last 30 days - need headline and description copy, path text, CTR, and engagement metrics

**Classification:** POOR

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  ad_group.id,
  ad_group.name,
  ad_group.type,
  ad_group_ad.ad.id,
  ad_group_ad.ad.responsive_search_ad.headlines,
  ad_group_ad.ad.responsive_search_ad.descriptions,
  ad_group_ad.ad.responsive_search_ad.path1,
  ad_group_ad.ad.responsive_search_ad.path2,
  metrics.impressions,
  metrics.clicks,
  metrics.ctr,
  metrics.cost_micros,
  metrics.average_cpc
FROM ad_group_ad
WHERE
  ad_group_ad.ad.type IN ('RESPONSIVE_SEARCH_AD')
  AND segments.date DURING LAST_30_DAYS
ORDER BY
  campaign.name,
  ad_group.name,
  metrics.ctr DESC
LIMIT 1000
```

**Generated Query:**
```sql
SELECT
  ad_group_ad.ad.responsive_search_ad.headlines,
  ad_group_ad.ad.responsive_search_ad.descriptions,
  ad_group_ad.ad.responsive_search_ad.path1,
  ad_group_ad.ad.responsive_search_ad.path2,
  metrics.ctr
FROM ad_group_ad
WHERE ad_group_ad.ad.type IN ('RESPONSIVE_SEARCH_AD') AND ad_group_ad.status = 'ENABLED'
ORDER BY metrics.ctr DESC
LIMIT 1000
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM correctly identified need for RSA-specific fields and date filter
- **Critical Divergence:** Date filter and most engagement metrics missing despite being mentioned in reasoning

**Analysis:**
- Selected Fields: **Missing critical fields**:
  - `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`, `metrics.average_cpc` (engagement metrics)
  - `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`
  - `ad_group.id`, `ad_group.name`, `ad_group_ad.ad.id`
- Data Scope: **Missing date filter** - no `segments.date DURING LAST_30_DAYS`
- Semantic Equivalence: Missing most identifying and engagement data

**Key Differences:**
- ❌ Missing `segments.date DURING LAST_30_DAYS` filter
- ❌ Missing `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`, `metrics.average_cpc`
- ❌ Missing all account, campaign, ad group, and ad ID fields

---

### 23. recent_campaign_changes

**Description:** Show me last 100 campaign changes in the last 14 days - need timestamp, user, client type, and what changed

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  change_event.change_date_time,
  change_event.client_type,
  change_event.change_resource_type,
  change_event.changed_fields,
  change_event.user_email,
  campaign.name
FROM
  change_event
WHERE
  change_event.change_date_time DURING LAST_14_DAYS
  AND change_event.change_resource_type IN ('CAMPAIGN')
ORDER BY
  change_event.change_date_time DESC
LIMIT 100
```

**Generated Query:**
```sql
SELECT
  change_event.change_date_time,
  change_event.user_email,
  change_event.client_type,
  change_event.changed_fields,
  campaign.id,
  campaign.name
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type = 'CAMPAIGN'
ORDER BY change_event.change_date_time DESC
LIMIT 100
```

**Analysis:**
- Selected Fields: All essential change tracking fields present
- Data Scope: Correct resource, date filter, and resource type filter present
- Semantic Equivalence: Would return nearly identical campaign change data

**Key Differences:**
- ⚠️ Missing `customer.id`, `customer.descriptive_name`
- ⚠️ Missing `change_event.change_resource_type` in SELECT (in WHERE clause)

---

### 24. recent_changes

**Description:** Pull recent changes across campaigns, ad groups, ads, keywords, and budgets last 14 days - need object type, user, changed fields, and timestamp

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  change_event.change_date_time,
  change_event.client_type,
  change_event.change_resource_type,
  change_event.changed_fields,
  change_event.user_email,
  campaign.name
FROM
  change_event
WHERE
  change_event.change_date_time DURING LAST_14_DAYS
  AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET')
ORDER BY
  change_event.change_date_time DESC
LIMIT 100
```

**Generated Query:**
```sql
SELECT
  change_event.change_resource_type,
  change_event.user_email,
  change_event.changed_fields,
  change_event.change_date_time,
  campaign.id,
  campaign.name
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET')
ORDER BY change_event.change_date_time DESC
```

**Analysis:**
- Selected Fields: All essential change tracking fields present; includes change_resource_type in SELECT
- Data Scope: Correct resource, date filter, and comprehensive resource type filter present
- Semantic Equivalence: Would return nearly identical change history data

**Key Differences:**
- ⚠️ Missing `customer.id`, `customer.descriptive_name`
- ⚠️ Missing `change_event.client_type`
- ⚠️ No LIMIT clause (reference has LIMIT 100)

---

### 25. all_campaigns

**Description:** Get me basic campaign info - IDs, names, channel types, and status (limit 100)

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  campaign.status,
  campaign.primary_status
FROM
  campaign
LIMIT 100
```

**Generated Query:**
```sql
SELECT
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  campaign.status
FROM campaign
WHERE campaign.status = 'ENABLED'
LIMIT 100
```

**Analysis:**
- Selected Fields: Core campaign fields present; missing account identifiers and primary_status
- Data Scope: Correct resource with status filter
- Semantic Equivalence: Would return similar campaign listing (with ENABLED filter)

**Key Differences:**
- ⚠️ Missing `customer.id`, `customer.descriptive_name`
- ⚠️ Missing `campaign.primary_status`
- ⚠️ Added implicit `campaign.status = 'ENABLED'` filter

---

### 26. performance_max_impression_share

**Description:** Show me daily impression share metrics for PMax last 30 days - need absolute top, budget lost, rank lost, and top impression share

**Classification:** POOR

**Reference Query:**
```sql
SELECT
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  segments.date,
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
WHERE
  segments.date DURING LAST_30_DAYS
  AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
```

**Generated Query:**
```sql
SELECT
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.search_absolute_top_impression_share,
  metrics.search_budget_lost_absolute_top_impression_share,
  metrics.search_rank_lost_absolute_top_impression_share,
  metrics.search_top_impression_share
FROM campaign
WHERE campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND campaign.status = 'ENABLED'
```

**LLM Explanation Analysis:**
- Reasoning Summary: LLM explicitly stated "segments is included as a related resource because the user requested 'daily' breakdowns, which requires segmenting by date" and mentioned adding `LAST_30_DAYS` filter
- **Dropped Resources:** The explanation shows "Dropped Resources: ["segments", "metrics"]" - the system incorrectly dropped the segments resource which was needed for daily breakdown
- **Critical Divergence:** No `segments.date` field, no date filter, and missing most impression share metrics

**Analysis:**
- Selected Fields: **Severely incomplete**:
  - Missing `segments.date` (essential for "daily" breakdown)
  - Missing most impression share metrics (only 4 of 10 present)
  - Missing `metrics.impressions`
- Data Scope: **Missing date filter** and **dropped segments resource**
- Semantic Equivalence: Cannot provide daily breakdown without `segments.date`

**Key Differences:**
- ❌ Missing `segments.date` field (critical for daily breakdown)
- ❌ Missing `segments.date DURING LAST_30_DAYS` filter
- ❌ Missing 6 impression share metrics:
  - `metrics.search_budget_lost_impression_share`
  - `metrics.search_budget_lost_top_impression_share`
  - `metrics.search_exact_match_impression_share`
  - `metrics.search_impression_share`
  - `metrics.search_rank_lost_impression_share`
  - `metrics.absolute_top_impression_percentage`
- ⚠️ Added implicit `campaign.status = 'ENABLED'` filter

---

## Overall Assessment

### Common Failure Patterns

#### 1. Missing Date Filters (10 queries affected)
**Severity: CRITICAL**

The most widespread issue: the LLM reasoning correctly identifies the need for date filters (`segments.date DURING LAST_WEEK_MON_SUN`, `DURING LAST_30_DAYS`, etc.), but these filters are consistently missing from the final generated query.

**Affected queries:**
- account_ids_with_access_and_traffic_last_week
- accounts_with_traffic_last_week
- keywords_with_top_traffic_last_week
- accounts_with_local_campaigns_last_week
- accounts_with_shopping_campaigns_last_week
- perf_max_campaigns_with_traffic_last_30_days
- campaigns_with_smart_bidding_by_spend
- campaigns_shopping_campaign_performance
- asset_performance_rsa
- performance_max_impression_share

**Evidence:** In the explanation output, the LLM explicitly states it has added or will add the date filter, but the final WHERE clause does not contain it.

#### 2. Missing Account Identification Fields (7 queries affected)
**Severity: HIGH**

`customer.id` and `customer.descriptive_name` are frequently missing despite being explicitly requested in the query description and stated as selected in the LLM reasoning.

**Affected queries:**
- accounts_with_local_campaigns_last_week
- accounts_with_asset_call_last_week
- accounts_with_asset_callout_last_week
- accounts_with_asset_app_last_week
- campaigns_with_smart_bidding_by_spend
- campaigns_shopping_campaign_performance

**Evidence:** LLM reasoning states "I selected customer.id, customer.descriptive_name" but these fields don't appear in the final SELECT clause.

#### 3. Field/Resource Selection Drop (3 queries affected)
**Severity: HIGH**

The system sometimes drops entire resources or fields after the LLM has selected them. In `performance_max_impression_share`, the explanation shows "Dropped Resources: ["segments", "metrics"]" - the segments resource was needed for the daily breakdown.

**Evidence:** Phase 1 explanation shows resources being dropped: "Dropped Resources: ["segments", "metrics"]"

#### 4. Incomplete Field Selection (5 queries affected)
**Severity: MEDIUM**

Several queries are missing fields that were explicitly requested:
- `metrics.impressions` missing from asset_fields_with_traffic_ytd, perf_max_campaigns_with_traffic_last_30_days
- `segments.date` missing from perf_max_campaigns_with_traffic_last_30_days, performance_max_impression_share (needed for "daily" breakdown)
- Various metric fields missing from asset_performance_rsa

### Root Cause Hypothesis

The issue appears to be a **disconnect between the LLM field selection phase and the final query assembly phase**. The LLM:
1. Correctly identifies required fields and filters in its reasoning
2. States it has selected those fields
3. But the final query assembly either:
   - Drops fields during the assembly process
   - Fails to translate the LLM's stated selections into actual query components
   - Has a bug in how WHERE clauses are constructed from the LLM's filter selections

### Recommendations

#### Immediate Fixes Needed

1. **Fix Date Filter Assembly**: Investigate why `segments.date` filters identified in LLM reasoning are not appearing in final WHERE clauses. This affects 38% of test queries.

2. **Fix Account Field Selection**: Ensure `customer.id` and `customer.descriptive_name` are consistently included when account-level reporting is requested.

3. **Fix Resource Dropping Logic**: Review why the system drops resources (like `segments`) after the LLM has correctly selected them.

4. **Fix Daily Segmentation**: Ensure `segments.date` field is included when users request "daily" breakdowns.

#### Code Investigation Areas

1. **Phase 3 to Phase 4 Handoff**: The issue appears to be in how selected fields from Phase 3 (Field Selection) are passed to Phase 4 (Criteria Assembly).

2. **WHERE Clause Construction**: The `segments.date` filters are being lost during WHERE clause assembly despite being identified by the LLM.

3. **Related Resource Handling**: Resources marked as "related" by the LLM (like segments for date filtering) may not be properly included in the final query.

### Queries Working Well

These queries performed excellently, indicating the core RAG and generation system is sound:
- **smart_campaign_search_terms_with_top_spend**: Correct resource, all filters present
- **all_search_terms_with_clicks**: Complete field selection, correct filters
- **search_terms_with_top_cpa**: Correct thresholds and ordering
- **search_terms_with_low_roas**: Complete field coverage
- **locations_with_highest_revenue_per_conversion**: Correct location-based analysis
- **recent_campaign_changes**: Accurate change event tracking
- **recent_changes**: Comprehensive change history

### Conclusion

The `mcc-gaql-gen` tool shows promise with correct resource selection and good field coverage for many query types. However, a **critical bug in the query assembly phase** is causing date filters and some fields to be dropped between the LLM's reasoning and the final output. This affects approximately **54% of queries** (14 out of 26), with **35% being classified as POOR** due to missing critical filters or fields.

**Priority:** Fix the date filter and field selection assembly bug before wider deployment.

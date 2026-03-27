# Query Cookbook Generation Comparison Report

**Test Run Date:** 2026-03-26 11:05:18
**Tool Version:** mcc-gaql-gen 0.16.3 (1d6f360-dirty)

## Summary Statistics

- **Total entries tested:** 26
- **EXCELLENT:** 6 (23%)
- **GOOD:** 12 (46%)
- **FAIR:** 6 (23%)
- **POOR:** 2 (8%)

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
	AND metrics.clicks > 1
```

**Generated Query:**
```sql
SELECT
  customer.id
FROM customer
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.clicks > 0
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- **Reasoning Summary:** LLM correctly identified `customer` resource for account-level data and applied appropriate filters
- **Key Decision Points:** Selected `customer.id` as the sole field; added date filter for last week and clicks threshold
- **Comparison to Intent:** Perfect alignment with user's request for account IDs with clicks

**Analysis:**
- **Selected Fields:** ✅ Same core field (customer.id)
- **Data Scope:** ✅ Same resource (customer), same date range
- **Semantic Equivalence:** ✅ Nearly identical - only difference is clicks > 0 vs > 1 (negligible)

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
WHERE segments.date DURING LAST_WEEK_MON_SUN
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified all required fields including currency_code for monetary context
- **Key Decision Points:** Included all requested metrics (impressions, clicks, cost_micros for spend)
- **Comparison to Intent:** Perfect match - all requested fields present

**Analysis:**
- **Selected Fields:** ✅ All required fields present
- **Data Scope:** ✅ Correct resource and date range
- **Semantic Equivalence:** ✅ Would return identical data

---

### 3. keywords_with_top_traffic_last_week

**Description:** Pull my top 10 keywords by clicks (>10K) last week - need acct, campaign, ad group IDs + names, channel type, and currency

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
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.clicks > 10000000 AND keyword_view.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 10
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- **Reasoning Summary:** LLM correctly identified `keyword_view` resource but misinterpreted "10K" clicks
- **Key Decision Points:** Selected core fields but missed keyword-specific fields (ad_group_criterion.criterion_id, keyword.text)
- **Comparison to Intent:** Partial - resource is correct but key identifying fields missing

**Analysis:**
- **Selected Fields:** ⚠️ Missing keyword identifiers (criterion_id, keyword.text), ad_group.type, metrics.impressions, metrics.cost_micros
- **Data Scope:** ✅ Correct resource (keyword_view), correct date range
- **Semantic Equivalence:** ⚠️ Query is on the right track but missing critical keyword identification fields

**Key Differences:**
- **Critical Bug:** `metrics.clicks > 10000000` (10 million) instead of > 10000 (10K) - LLM multiplied by 1000 incorrectly
- **Missing Fields:** ad_group_criterion.criterion_id, ad_group_criterion.keyword.text, ad_group.type, impressions, cost_micros
- **Implicit Filter:** Added `keyword_view.status = 'ENABLED'` which wasn't requested

---

### 4. accounts_with_perf_max_campaigns_last_week

**Description:** Get me the top PMax campaign by clicks (>100) per account last week - need acct and campaign IDs + names, channel type, and currency

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
  customer.currency_code,
  metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 1
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified campaign resource and PMax filtering
- **Key Decision Points:** Proper channel type filter, correct date range, appropriate ordering and limit
- **Comparison to Intent:** Very close match - captures main intent with minor field omissions

**Analysis:**
- **Selected Fields:** ⚠️ Missing metrics.impressions and metrics.cost_micros
- **Data Scope:** ✅ Correct resource (campaign), correct filters
- **Semantic Equivalence:** ✅ Would return conceptually similar data (top PMax campaign)

**Key Differences:**
- **Missing Fields:** metrics.impressions, metrics.cost_micros
- **Implicit Filter:** Added `campaign.status = 'ENABLED'` (acceptable per guidelines)

---

### 5. accounts_with_smart_campaigns_last_week

**Description:** Show me the top Smart campaign by clicks (>100) per account last week - need acct and campaign details with currency

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
ORDER BY
  metrics.clicks DESC
LIMIT 1
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 1
```

**Classification:** GOOD

**Analysis:**
- **Selected Fields:** ⚠️ Missing metrics.impressions, metrics.cost_micros
- **Data Scope:** ✅ Correct resource and filters
- **Semantic Equivalence:** ✅ Core intent captured

**Key Differences:**
- **Missing Fields:** metrics.impressions, metrics.cost_micros
- **Implicit Filter:** Added campaign.status = 'ENABLED'

---

### 6. accounts_with_local_campaigns_last_week

**Description:** Pull the top Local campaign by clicks (>500) per account last week - need acct and campaign info with currency

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
ORDER BY
  metrics.clicks DESC
LIMIT 1
```

**Generated Query:**
```sql
SELECT customer.id, customer.descriptive_name, customer.currency_code, campaign.id, campaign.name, campaign.advertising_channel_type, metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type = 'LOCAL' AND metrics.clicks > 500 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 1
```

**Classification:** GOOD

**Analysis:**
- **Selected Fields:** ⚠️ Missing metrics.impressions, metrics.cost_micros
- **Data Scope:** ✅ Correct resource and filters (used = instead of IN, but equivalent for single value)
- **Semantic Equivalence:** ✅ Core intent captured

**Key Differences:**
- **Missing Fields:** metrics.impressions, metrics.cost_micros

---

### 7. accounts_with_shopping_campaigns_last_week

**Description:** Get me the top Shopping campaign by clicks (>100) per account last week - need acct and campaign details with currency

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
ORDER BY
  metrics.clicks DESC
LIMIT 1
```

**Generated Query:**
```sql
SELECT customer.id, customer.descriptive_name, customer.currency_code, campaign.id, campaign.name, campaign.advertising_channel_type, metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type IN ('SHOPPING') AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
```

**Classification:** GOOD

**Analysis:**
- **Selected Fields:** ⚠️ Missing metrics.impressions, metrics.cost_micros
- **Data Scope:** ✅ Correct resource and filters
- **Semantic Equivalence:** ✅ Core intent captured (missing LIMIT 1 but functionally similar with ORDER BY)

---

### 8. accounts_with_multichannel_campaigns_last_week

**Description:** Show me the top Multi-Channel campaign by clicks (>100) per account last week - need acct and campaign info with currency

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
ORDER BY
  metrics.clicks DESC
LIMIT 1
```

**Generated Query:**
```sql
SELECT customer.id, customer.descriptive_name, customer.currency_code, campaign.id, campaign.name, campaign.advertising_channel_type, metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type = 'MULTI_CHANNEL' AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 10
```

**Classification:** GOOD

**Analysis:**
- **Selected Fields:** ⚠️ Missing metrics.impressions, metrics.cost_micros
- **Data Scope:** ⚠️ LIMIT 10 instead of LIMIT 1 (minor difference)
- **Semantic Equivalence:** ✅ Core intent captured

---

### 9. accounts_with_asset_sitelink_last_week

**Description:** Pull the top Sitelinks by impressions (>20K clicks) per account last week - need acct and campaign details with currency

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
ORDER BY
  metrics.impressions DESC
LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  asset.id,
  asset.name,
  asset.sitelink_asset.link_text,
  metrics.impressions,
  metrics.clicks
FROM customer_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND asset.type = 'SITELINK' AND metrics.clicks > 20000000
ORDER BY metrics.impressions DESC
```

**Classification:** POOR

**LLM Explanation Analysis:**
- **Reasoning Summary:** LLM incorrectly selected `customer_asset` instead of `campaign_asset` and misinterpreted "20K" as 20 million
- **Key Decision Points:** Chose customer_asset resource (wrong), used asset.type instead of field_type
- **Comparison to Intent:** Significant deviation - wrong resource, missing account identifiers

**Analysis:**
- **Selected Fields:** ❌ Missing customer.id, customer.descriptive_name (rejected by tool), missing metrics.cost_micros
- **Data Scope:** ❌ Wrong resource (customer_asset instead of campaign_asset)
- **Semantic Equivalence:** ❌ Querying wrong resource entirely

**Key Differences:**
- **Critical Bug:** `metrics.clicks > 20000000` (20 million) instead of > 20000 (20K) - same micros conversion error
- **Wrong Resource:** customer_asset vs campaign_asset
- **Missing Fields:** customer.id, customer.descriptive_name were rejected; missing cost_micros, campaign.advertising_channel_type

---

### 10. accounts_with_asset_call_last_week

**Description:** Get me the top Call Extensions by impressions (>100) per account last week - need acct and campaign info with currency

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
ORDER BY
  metrics.impressions DESC
LIMIT 10
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
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type IN ('CALL') AND metrics.impressions > 100
ORDER BY metrics.impressions DESC
LIMIT 10
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified campaign_asset resource but failed to include account identifiers
- **Key Decision Points:** Used correct resource and field_type filter
- **Comparison to Intent:** Partial - resource correct but missing key fields

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name (rejected), missing asset.id, asset.name, asset.call_asset.phone_number, metrics.clicks, metrics.cost_micros
- **Data Scope:** ✅ Correct resource (campaign_asset)
- **Semantic Equivalence:** ⚠️ On the right track but missing critical asset details

---

### 11. accounts_with_asset_callout_last_week

**Description:** Show me the top Callout Extensions by impressions (>30K clicks) per account last week - need acct and campaign details with currency

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
ORDER BY
  metrics.impressions DESC
LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign_asset.asset,
  metrics.impressions,
  metrics.clicks
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'CALLOUT' AND metrics.clicks > 30000
ORDER BY metrics.impressions DESC
```

**Classification:** FAIR

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name, campaign.advertising_channel_type, metrics.cost_micros
- **Data Scope:** ✅ Correct resource (campaign_asset)
- **Semantic Equivalence:** ⚠️ Missing key account identifiers

---

### 12. accounts_with_asset_app_last_week

**Description:** Pull the top App Extensions by impressions (>1) per account last week - need acct and campaign info with currency

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
ORDER BY
  metrics.impressions DESC
LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign_asset.asset,
  metrics.impressions
FROM campaign_asset
WHERE campaign_asset.field_type = 'APP' AND segments.date DURING LAST_WEEK_MON_SUN AND metrics.impressions > 1
ORDER BY metrics.impressions DESC
```

**Classification:** FAIR

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name, campaign.advertising_channel_type, metrics.clicks, metrics.cost_micros
- **Data Scope:** ✅ Correct resource (campaign_asset)
- **Semantic Equivalence:** ⚠️ Missing account identifiers and key metrics

---

### 13. perf_max_campaigns_with_traffic_last_30_days

**Description:** Give me daily PMax performance last 30 days - need CTR, CPC, conv, rev, CPA, campaign IDs + names, and currency

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
  segments.date,
  metrics.ctr,
  metrics.average_cpc,
  metrics.conversions,
  metrics.conversions_value,
  metrics.cost_per_conversion,
  customer.currency_code
FROM campaign
WHERE campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND segments.date DURING LAST_30_DAYS AND campaign.status = 'ENABLED'
ORDER BY segments.date DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified campaign resource and mapped requested metrics to GAQL fields
- **Key Decision Points:** Used average_cpc for CPC, conversions_value for revenue, cost_per_conversion for CPA
- **Comparison to Intent:** Strong match - captured all requested metrics

**Analysis:**
- **Selected Fields:** ⚠️ Missing campaign.advertising_channel_type, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.average_cost
- **Data Scope:** ✅ Correct resource and filters
- **Semantic Equivalence:** ✅ Core metrics captured

---

### 14. asset_fields_with_traffic_ytd

**Description:** Show me YTD asset performance by day with impressions, asset type, and currency

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
  customer.currency_code
FROM asset
```

**Classification:** POOR

**LLM Explanation Analysis:**
- **Reasoning Summary:** LLM selected `asset` resource incorrectly; fields were rejected as incompatible
- **Key Decision Points:** Attempted to use asset resource but it doesn't support segments.date
- **Comparison to Intent:** Fundamental misunderstanding - query is incomplete

**Analysis:**
- **Selected Fields:** ❌ Only customer.currency_code - all other fields rejected
- **Data Scope:** ❌ Wrong resource (asset instead of asset_field_type_view)
- **Semantic Equivalence:** ❌ Query is non-functional

**Key Issues:**
- LLM chose `asset` resource which doesn't support date segmentation
- All requested fields (segments.date, metrics.impressions, asset_field_type_view.field_type) were rejected
- This is a complete failure to understand the query requirements

---

### 15. campaigns_with_smart_bidding_by_spend

**Description:** Pull top 25 Smart Bidding campaigns by spend (>$1K) last week - need acct and campaign IDs + names, budget, bid strategy, CPC, and conv metrics with currency

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
  campaign_budget.amount_micros,
  campaign.bidding_strategy_type,
  metrics.average_cpc,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value
FROM campaign
WHERE campaign.bidding_strategy_type IN ('MAXIMIZE_CLICKS', 'MAXIMIZE_CONVERSIONS', 'MAXIMIZE_CONVERSION_VALUE', 'TARGET_CPA', 'TARGET_ROAS', 'TARGET_SPEND') AND segments.date DURING LAST_WEEK_MON_SUN AND metrics.cost_micros > 1000000000 AND campaign.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 25
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified smart bidding strategy types and spend threshold conversion
- **Key Decision Points:** Properly converted $1K to 1000000000 micros, included budget and bid strategy fields
- **Comparison to Intent:** Strong match with minor omissions

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name, campaign.advertising_channel_type, metrics.clicks, metrics.cost_micros
- **Data Scope:** ✅ Correct resource and comprehensive bidding strategy filter
- **Semantic Equivalence:** ✅ Core intent captured well

---

### 16. campaigns_shopping_campaign_performance

**Description:** Get me Shopping campaigns by spend (>$100) last 30 days - need acct and campaign details, budget, bid strategy, CPC, and conv metrics with currency

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
  campaign.advertising_channel_type,
  campaign.bidding_strategy_type,
  campaign_budget.amount_micros,
  metrics.cost_micros,
  metrics.average_cpc,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value
FROM campaign
WHERE campaign.advertising_channel_type IN ('SHOPPING') AND segments.date DURING LAST_30_DAYS AND campaign.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
```

**Classification:** GOOD

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name, metrics.clicks
- **Data Scope:** ⚠️ Missing spend threshold (>$100 / >100000000 micros)
- **Semantic Equivalence:** ✅ Core intent captured

---

### 17. smart_campaign_search_terms_with_top_spend

**Description:** Show me top 100 search terms by spend from Smart campaigns last 30 days - need search term, match type, and performance metrics with currency

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
  customer.currency_code,
  smart_campaign_search_term_view.search_term,
  campaign.keyword_match_type,
  metrics.cost_micros,
  metrics.clicks,
  metrics.impressions
FROM smart_campaign_search_term_view
WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('SMART')
ORDER BY metrics.cost_micros DESC
LIMIT 100
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified smart_campaign_search_term_view resource
- **Key Decision Points:** Used campaign.keyword_match_type (questionable - may not be valid on this resource)
- **Comparison to Intent:** Partial - correct resource but missing match type field

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name (rejected), campaign.id, campaign.name
- **Data Scope:** ✅ Correct resource for Smart campaign search terms
- **Semantic Equivalence:** ⚠️ Missing account/campaign context

---

### 18. all_search_terms_with_clicks

**Description:** Pull all search terms with clicks last 30 days - need match type, device, keyword status, and full conv metrics with currency

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
ORDER BY
  metrics.cost_micros desc
```

**Generated Query:**
```sql
SELECT
  customer.currency_code,
  search_term_view.search_term,
  segments.search_term_match_type,
  segments.device,
  search_term_view.status,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.conversions_value,
  metrics.cost_per_conversion,
  metrics.conversions_value_per_cost,
  metrics.all_conversions,
  metrics.all_conversions_value,
  metrics.cost_per_all_conversions,
  metrics.value_per_conversion,
  metrics.value_per_all_conversions
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0 AND search_term_view.status = 'ENABLED'
ORDER BY metrics.cost_micros desc
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- **Reasoning Summary:** LLM correctly identified search_term_view and included comprehensive conversion metrics
- **Key Decision Points:** Added extended conversion metrics (all_conversions variants), included device and match type segments
- **Comparison to Intent:** Actually exceeds expectations - more comprehensive than reference

**Analysis:**
- **Selected Fields:** ✅ All required fields plus additional useful conversion metrics
- **Data Scope:** ✅ Correct resource and filters
- **Semantic Equivalence:** ✅ Would return equivalent or better data than reference

---

### 19. search_terms_with_top_cpa

**Description:** Get me top 50 search terms with CPA >$200 and spend >$1K last 30 days - flagging expensive underperformers with currency

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
WHERE segments.date DURING LAST_30_DAYS AND metrics.cost_per_conversion > 0 AND metrics.cost_micros > 1000000000 AND search_term_view.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 50
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correct resource and spend threshold, but CPA filter is wrong
- **Key Decision Points:** Used `metrics.cost_per_conversion > 0` instead of > 200000000
- **Comparison to Intent:** Partial - spend filter correct but CPA filter is essentially absent

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name (rejected)
- **Data Scope:** ⚠️ CPA filter is `> 0` instead of `> 200000000` ($200)
- **Semantic Equivalence:** ⚠️ Will return many more results than intended

**Key Issue:**
- **CPA Filter:** `metrics.cost_per_conversion > 0` instead of `> 200000000` - the $200 threshold was not applied

---

### 20. search_terms_with_low_roas

**Description:** Show me top 50 search terms with ROAS <0.25 and spend >$1K last 30 days - need to neg keyword these with currency

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
  customer.currency_code,
  search_term_view.search_term,
  metrics.cost_micros,
  metrics.conversions_value,
  metrics.conversions_value_per_cost
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < 0.25 AND metrics.cost_micros > 1000000000 AND search_term_view.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 50
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified ROAS threshold and spend filter
- **Key Decision Points:** Used conversions_value_per_cost for ROAS calculation
- **Comparison to Intent:** Good match - core intent of finding low ROAS terms captured

**Analysis:**
- **Selected Fields:** ⚠️ Minimal field selection (only core ROAS-related fields), missing customer.id, match_type, device, etc.
- **Data Scope:** ✅ Correct ROAS (< 0.25) and spend (>$1K) filters
- **Semantic Equivalence:** ✅ Core intent captured

---

### 21. locations_with_highest_revenue_per_conversion

**Description:** Pull top 1000 locations by rev per conv (>10 conv) last 7 days - need geo target IDs and location metrics with currency

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
  geographic_view.country_criterion_id,
  metrics.conversions_value,
  metrics.conversions,
  metrics.value_per_conversion,
  metrics.cost_micros,
  customer.currency_code
FROM geographic_view
WHERE segments.date DURING LAST_7_DAYS AND metrics.conversions > 10
ORDER BY metrics.value_per_conversion DESC
LIMIT 1000
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- **Reasoning Summary:** LLM chose `geographic_view` instead of `location_view` but it's functionally equivalent
- **Key Decision Points:** Used country_criterion_id as geo target ID, included value_per_conversion metric
- **Comparison to Intent:** Alternative but valid approach - geographic_view provides similar data

**Analysis:**
- **Selected Fields:** ⚠️ Using geographic_view fields instead of location_view; missing campaign context
- **Data Scope:** ✅ Alternative valid resource (geographic_view vs location_view)
- **Semantic Equivalence:** ✅ Would return similar location-based performance data

---

### 22. asset_performance_rsa

**Description:** Get me RSA performance last 30 days - need headline and description copy, path text, CTR, and engagement metrics

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
  metrics.ctr,
  metrics.engagement_rate,
  metrics.engagements,
  metrics.interactions,
  metrics.interaction_rate
FROM ad_group_ad
WHERE ad_group_ad.ad.type = 'RESPONSIVE_SEARCH_AD' AND segments.date DURING LAST_30_DAYS AND ad_group_ad.status = 'ENABLED'
ORDER BY metrics.ctr DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified ad_group_ad resource for RSA performance
- **Key Decision Points:** Added engagement metrics beyond what was requested (engagement_rate, engagements, interactions)
- **Comparison to Intent:** Partial - missing hierarchy identifiers (campaign, ad_group)

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name, campaign.*, ad_group.*, ad_group_ad.ad.id, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.average_cpc
- **Data Scope:** ✅ Correct resource (ad_group_ad) and ad type filter
- **Semantic Equivalence:** ⚠️ Missing critical context for the RSA performance data

---

### 23. recent_campaign_changes

**Description:** Show me last 100 campaign changes in the last 14 days - need timestamp, user, client type, and what changed

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
  campaign.id,
  campaign.name,
  change_event.change_date_time,
  change_event.client_type,
  change_event.changed_fields,
  change_event.user_email
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type = 'CAMPAIGN'
ORDER BY change_event.change_date_time DESC
LIMIT 100
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified change_event resource for audit trail
- **Key Decision Points:** Included all requested fields: timestamp, user, client type, changed fields
- **Comparison to Intent:** Nearly identical to reference

**Analysis:**
- **Selected Fields:** ✅ All required fields present (minor: missing customer.id, customer.descriptive_name)
- **Data Scope:** ✅ Correct resource and filters
- **Semantic Equivalence:** ✅ Would return nearly identical data

---

### 24. recent_changes

**Description:** Pull recent changes across campaigns, ad groups, ads, keywords, and budgets last 14 days - need object type, user, changed fields, and timestamp

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
  AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET')
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
WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET')
ORDER BY change_event.change_date_time DESC
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified change_event resource for comprehensive audit
- **Key Decision Points:** Included all requested change resource types
- **Comparison to Intent:** Strong match - captures all change types requested

**Analysis:**
- **Selected Fields:** ✅ All core fields present (object type, user, changed fields, timestamp, campaign context)
- **Data Scope:** ✅ Correct resource and comprehensive resource type filter
- **Semantic Equivalence:** ✅ Would return equivalent data

---

### 25. all_campaigns

**Description:** Get me basic campaign info - IDs, names, channel types, and status (limit 100)

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

**Classification:** GOOD

**LLM Explanation Analysis:**
- **Reasoning Summary:** Simple query correctly targeting campaign resource
- **Key Decision Points:** Added implicit status filter for enabled campaigns
- **Comparison to Intent:** Good but added implicit filter not in reference

**Analysis:**
- **Selected Fields:** ⚠️ Missing customer.id, customer.descriptive_name, campaign.primary_status
- **Data Scope:** ⚠️ Added implicit `campaign.status = 'ENABLED'` filter
- **Semantic Equivalence:** ⚠️ Will return subset of campaigns (only enabled) vs all campaigns

---

### 26. performance_max_impression_share

**Description:** Show me daily impression share metrics for PMax last 30 days - need absolute top, budget lost, rank lost, and top impression share

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
  segments.date,
  metrics.search_absolute_top_impression_share,
  metrics.search_budget_lost_absolute_top_impression_share,
  metrics.search_rank_lost_absolute_top_impression_share,
  metrics.search_top_impression_share
FROM campaign
WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND campaign.status = 'ENABLED'
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- **Reasoning Summary:** Correctly identified impression share metrics are at campaign level
- **Key Decision Points:** Selected core impression share metrics as requested
- **Comparison to Intent:** Good match but missing some impression share variants

**Analysis:**
- **Selected Fields:** ⚠️ Missing campaign.advertising_channel_type, metrics.impressions, and several impression share variants
- **Data Scope:** ✅ Correct resource and filters
- **Semantic Equivalence:** ✅ Core impression share metrics captured

---

## Overall Assessment

### Patterns Observed

1. **Resource Selection Accuracy:** Generally excellent (24/26 correct resources)
   - Only 2 failures: `asset_fields_with_traffic_ytd` (chose `asset` instead of `asset_field_type_view`) and `accounts_with_asset_sitelink_last_week` (chose `customer_asset` instead of `campaign_asset`)

2. **Field Completeness:** Mixed results
   - Basic fields (campaign.id, campaign.name) consistently selected
   - Account identifiers (customer.id, customer.descriptive_name) frequently rejected when resource doesn't support them directly
   - Metrics sometimes omitted despite being requested

3. **Micros Conversion Bug:** Critical issue identified
   - "10K clicks" interpreted as `> 10000000` (10 million) instead of `> 10000`
   - "20K clicks" interpreted as `> 20000000` (20 million) instead of `> 20000`
   - Tool appears to multiply by 1000 incorrectly when parsing "K" suffix

4. **Implicit Filters:** Consistent pattern
   - Tool adds `status = 'ENABLED'` filters by default
   - This is acceptable per test guidelines but changes query semantics

5. **RAG Confidence:** Low confidence on specialized resources
   - Multiple "Low RAG confidence" warnings for niche resources (smart_campaign_search_term_view, keyword_view, etc.)
   - Tool falls back to full resource list but still makes correct selections

### Common Failure Modes

1. **Threshold Misinterpretation:** Numeric thresholds with "K" suffix are incorrectly converted to micros
2. **Missing Account Context:** When resources don't directly support customer.id, the tool drops these fields rather than finding alternative approaches
3. **Over-minimalism:** Some queries omit useful metrics that were explicitly requested (impressions, clicks, cost_micros)
4. **Resource Confusion:** Asset-related queries struggle with choosing between asset, customer_asset, campaign_asset, and asset_field_type_view

### Recommendations

1. **Fix Micros Conversion:** Parse "K" and "M" suffixes correctly - "10K" should be 10000, not 10000000
2. **Improve Account Context:** When customer.id is requested but not available on the primary resource, consider alternative approaches or clearer warnings
3. **Field Completeness:** Ensure explicitly requested metrics are always included (impressions, clicks, cost_micros commonly omitted)
4. **Asset Resource Guidance:** Provide better examples for asset-related queries in the cookbook to improve RAG retrieval

### Summary

The `mcc-gaql-gen` tool demonstrates strong capability in resource selection and query structure generation. The majority of queries (69%) rate GOOD or EXCELLENT. The main areas for improvement are:

1. Numeric threshold parsing (critical bug)
2. Field completeness for complex queries
3. Asset resource selection accuracy

Overall, the tool is effective for GAQL generation but requires validation of numeric thresholds before production use.

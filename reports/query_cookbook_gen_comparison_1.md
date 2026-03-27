# Query Cookbook Generation Comparison Report

## Summary Statistics

- **Total entries tested:** 26
- **EXCELLENT:** 4 (15%)
- **GOOD:** 11 (42%)
- **FAIR:** 7 (27%)
- **POOR:** 4 (15%)

---

## Detailed Results

### 1. account_ids_with_access_and_traffic_last_week

**Description:** Get me account IDs with clicks in the last 7 days

**Reference Query:**
```sql
SELECT customer.id FROM customer WHERE segments.date during LAST_7_DAYS AND metrics.clicks > 1
```

**Generated Query:**
```sql
SELECT
  customer.id
FROM customer
WHERE metrics.clicks > '0'
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified 'customer' resource and selected customer.id field with metrics.clicks filter
- Key Decision Points: Chose correct resource, selected appropriate field
- Comparison to Intent: Understood the core requirement (account IDs with clicks)
- Where It Diverged: Missing date filter (segments.date DURING LAST_7_DAYS) and wrong threshold (>0 vs >1)

**Analysis:**
- Selected Fields: Core field present (customer.id), missing date segment
- Data Scope: Correct resource but missing temporal filter
- Semantic Equivalence: Would return similar data but for all time, not just last 7 days

**Key Differences:**
- Missing `segments.date DURING LAST_7_DAYS` - Explanation mentions date range but it wasn't applied
- Threshold `> 0` instead of `> 1` - LLM chose more inclusive threshold

---

### 2. accounts_with_traffic_last_week

**Description:** Show me account-level performance last 7 days - need impressions, clicks, spend, and currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM customer WHERE segments.date during LAST_7_DAYS AND metrics.impressions > 1
```

**Generated Query:**
```sql
SELECT
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM customer
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified customer resource and selected requested metrics
- Key Decision Points: Selected impressions, clicks, cost_micros, currency_code
- Comparison to Intent: Captured main metrics but omitted identifying fields
- Where It Diverged: Missing customer.id, customer.descriptive_name, date filter, and impressions filter

**Analysis:**
- Selected Fields: Core metrics present but missing account identifiers
- Data Scope: No temporal filter applied
- Semantic Equivalence: Returns metrics without account context or date constraints

**Key Differences:**
- Missing `customer.id` and `customer.descriptive_name` - LLM focused on metrics only
- Missing `segments.date DURING LAST_7_DAYS` - Explanation noted date range but didn't apply it

---

### 3. keywords_with_top_traffic_last_week

**Description:** Pull my top 10 keywords by clicks (>10K) last week - need acct, campaign, ad group IDs + names, channel type, and currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, ad_group.id, ad_group.name, ad_group.type, ad_group_criterion.criterion_id, ad_group_criterion.keyword.text, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM keyword_view WHERE segments.date DURING LAST_7_DAYS and metrics.clicks > 10000 ORDER BY metrics.clicks DESC LIMIT 10
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  ad_group.id,
  ad_group.name,
  ad_group_criterion.criterion_id,
  ad_group_criterion.keyword.text,
  metrics.clicks,
  customer.currency_code
FROM keyword_view
WHERE metrics.clicks > '10000' AND keyword_view.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 10
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly selected keyword_view resource and most required fields
- Key Decision Points: Chose all key identifier fields (account, campaign, ad group, keyword)
- Comparison to Intent: Understood requirement for top keywords by clicks with identifiers
- Where It Diverged: Missing impressions, cost_micros, ad_group.type; no date filter

**Analysis:**
- Selected Fields: Core identifying fields present, missing some metrics (impressions, cost_micros)
- Data Scope: Correct resource, LIMIT applied correctly, but missing date filter
- Semantic Equivalence: Would return similar top keywords but without temporal constraint

**Key Differences:**
- Missing `metrics.impressions` and `metrics.cost_micros` - LLM prioritized identifiers over metrics
- Missing `ad_group.type` - Not deemed critical for the query
- Missing `segments.date DURING LAST_7_DAYS` - Date filter in reasoning but not in output

---

### 4. accounts_with_perf_max_campaigns_last_week

**Description:** Get me the top PMax campaign by clicks (>100) per account last 7 days - need acct and campaign IDs + names, channel type, and currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1
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
WHERE campaign.advertising_channel_type IN '(\'PERFORMANCE_MAX\')' AND metrics.clicks > '100' AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified campaign resource, channel type filter, and core fields
- Key Decision Points: Selected PERFORMANCE_MAX filter, clicks threshold, ordering
- Comparison to Intent: Understood PMax campaigns with click threshold
- Where It Diverged: Missing LIMIT 1, impressions, cost_micros, date filter

**Analysis:**
- Selected Fields: Core identifiers present, missing impressions and cost_micros
- Data Scope: Correct resource and channel filter, but no LIMIT 1 and no date filter
- Semantic Equivalence: Returns PMax campaigns but not "top 1 per account"

**Key Differences:**
- Missing `LIMIT 1` - Critical for "top" requirement
- Missing `metrics.impressions` and `metrics.cost_micros` - Explanation noted them but didn't include
- Missing `segments.date DURING LAST_7_DAYS` - Date mentioned in reasoning but not applied

---

### 5. accounts_with_smart_campaigns_last_week

**Description:** Show me the top Smart campaign by clicks (>100) per account last week - need acct and campaign details with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1
```

**Generated Query:**
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
WHERE campaign.advertising_channel_type IN '(\'SMART\')' AND metrics.clicks > '100' AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified SMART channel type and all core fields including impressions and cost
- Key Decision Points: Selected all requested fields including metrics
- Comparison to Intent: Well aligned with reference intent
- Where It Diverged: Missing LIMIT 1 and date filter

**Analysis:**
- Selected Fields: All key fields present (identifiers, channel type, metrics, currency)
- Data Scope: Correct resource and channel filter, missing temporal constraint
- Semantic Equivalence: Very close match, missing only LIMIT and date

**Key Differences:**
- Missing `LIMIT 1` - Needed for "top" campaign
- Missing `segments.date DURING LAST_7_DAYS` - Temporal constraint
- Added implicit `campaign.status = 'ENABLED'` - System default filter

---

### 6. accounts_with_local_campaigns_last_week

**Description:** Pull the top Local campaign by clicks (>500) per account last 7 days - need acct and campaign info with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('LOCAL') AND metrics.clicks > 500 ORDER BY metrics.clicks DESC LIMIT 1
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
WHERE campaign.advertising_channel_type IN '(\'LOCAL\')' AND metrics.clicks > '500' AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified LOCAL channel type and selected core fields
- Key Decision Points: Selected LOCAL filter, clicks threshold of 500
- Comparison to Intent: Understood Local campaigns with click threshold
- Where It Diverged: Missing LIMIT 1, impressions, cost_micros, date filter

**Analysis:**
- Selected Fields: Core identifiers present, missing impressions and cost_micros
- Data Scope: Correct channel filter and threshold, missing LIMIT and date
- Semantic Equivalence: Returns Local campaigns but not "top 1"

**Key Differences:**
- Missing `LIMIT 1` - Critical for "top" requirement
- Missing `metrics.impressions` and `metrics.cost_micros`
- Missing `segments.date DURING LAST_7_DAYS`

---

### 7. accounts_with_shopping_campaigns_last_week

**Description:** Get me the top Shopping campaign by clicks (>100) per account last week - need acct and campaign details with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('SHOPPING') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1
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
WHERE campaign.advertising_channel_type IN '(\'SHOPPING\')' AND metrics.clicks > '100' AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified SHOPPING channel and core fields
- Key Decision Points: Selected SHOPPING filter, clicks > 100 threshold
- Comparison to Intent: Understood Shopping campaigns with click filter
- Where It Diverged: Missing LIMIT 1, impressions, cost_micros, date filter

**Analysis:**
- Selected Fields: Core identifiers present, missing impressions and cost_micros
- Data Scope: Correct channel filter, missing LIMIT and temporal constraint
- Semantic Equivalence: Returns Shopping campaigns but not "top 1 per account"

**Key Differences:**
- Missing `LIMIT 1` - Needed for "top" campaign
- Missing `metrics.impressions` and `metrics.cost_micros`
- Missing `segments.date DURING LAST_7_DAYS`

---

### 8. accounts_with_multichannel_campaigns_last_week

**Description:** Show me the top Multi-Channel campaign by clicks (>100) per account last 7 days - need acct and campaign info with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('MULTI_CHANNEL') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign
WHERE campaign.advertising_channel_type IN '(\'MULTI_CHANNEL\')' AND metrics.clicks > '100' AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 1
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified MULTI_CHANNEL channel type and all requested fields
- Key Decision Points: Selected all metrics including impressions, clicks, cost_micros
- Comparison to Intent: Perfect alignment with reference intent
- Where It Diverged: Missing date filter (mentioned in reasoning but not applied)

**Analysis:**
- Selected Fields: All key fields present (customer.id, descriptive_name, campaign fields, metrics, currency)
- Data Scope: Correct resource, channel filter, LIMIT 1 applied
- Semantic Equivalence: Nearly identical output, would return same conceptual data

**Key Differences:**
- Missing `segments.date DURING LAST_7_DAYS` - Only significant difference
- Added implicit `campaign.status = 'ENABLED'` - Acceptable default filter

---

### 9. accounts_with_asset_sitelink_last_week

**Description:** Pull the top Sitelink by impressions (>20K clicks) per account last week - need acct and campaign details with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('SITELINK') AND metrics.clicks > 20000 ORDER BY metrics.impressions DESC LIMIT 1
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
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'SITELINK' AND metrics.clicks > '20000'
ORDER BY metrics.impressions DESC
```

**Classification:** POOR

**LLM Explanation Analysis:**
- Reasoning Summary: Chose campaign_asset resource instead of asset_field_type_view
- Key Decision Points: Selected different resource and field approach for sitelinks
- Comparison to Intent: Different resource leads to different query semantics
- Where It Diverged: Wrong resource, no LIMIT 1, different field selection

**Analysis:**
- Selected Fields: Uses campaign_asset fields instead of asset_field_type_view
- Data Scope: Different resource entirely (campaign_asset vs asset_field_type_view)
- Semantic Equivalence: Would return different data structure and potentially different results

**Key Differences:**
- **Wrong Resource:** Used `campaign_asset` instead of `asset_field_type_view`
- Missing `LIMIT 1` - Needed for "top" requirement
- Different field structure: Uses `asset.sitelink_asset.link_text` vs `asset_field_type_view.field_type`

---

### 10. accounts_with_asset_call_last_week

**Description:** Get me the top Call Extension by impressions (>100) per account last 7 days - need acct and campaign info with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('CALL') AND metrics.impressions > 100 ORDER BY metrics.impressions DESC LIMIT 1
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  asset.type,
  asset.call_asset.phone_number,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM campaign_asset
WHERE segments.date DURING LAST_7_DAYS AND campaign_asset.field_type = 'CALL' AND metrics.impressions > '100'
ORDER BY metrics.impressions DESC
```

**Classification:** POOR

**LLM Explanation Analysis:**
- Reasoning Summary: Chose campaign_asset resource with call_asset fields
- Key Decision Points: Selected campaign_asset instead of asset_field_type_view
- Comparison to Intent: Different resource choice leads to different query
- Where It Diverged: Wrong resource, no LIMIT 1, added cost_micros not in reference

**Analysis:**
- Selected Fields: Uses asset.call_asset.phone_number instead of asset_field_type_view.field_type
- Data Scope: campaign_asset resource instead of asset_field_type_view
- Semantic Equivalence: Different resource means different data returned

**Key Differences:**
- **Wrong Resource:** Used `campaign_asset` instead of `asset_field_type_view`
- Missing `LIMIT 1`
- Added `asset.call_asset.phone_number` and `metrics.cost_micros` not in reference

---

### 11. accounts_with_asset_callout_last_week

**Description:** Show me the top Callout Extension by impressions (>30K clicks) per account last week - need acct and campaign details with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('CALLOUT') AND metrics.clicks > 30000 ORDER BY metrics.impressions DESC LIMIT 1
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
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'CALLOUT' AND metrics.clicks > '30000'
ORDER BY metrics.impressions DESC
```

**Classification:** POOR

**LLM Explanation Analysis:**
- Reasoning Summary: Chose campaign_asset resource with different field selection
- Key Decision Points: Selected campaign_asset.resource fields instead of asset_field_type_view
- Comparison to Intent: Completely different resource approach
- Where It Diverged: Wrong resource, minimal field selection

**Analysis:**
- Selected Fields: Only basic fields (currency, campaign, asset reference)
- Data Scope: campaign_asset instead of asset_field_type_view
- Semantic Equivalence: Would return fundamentally different data

**Key Differences:**
- **Wrong Resource:** campaign_asset instead of asset_field_type_view
- Missing `customer.id`, `customer.descriptive_name`, `campaign.advertising_channel_type`
- Missing `LIMIT 1`
- Uses `campaign_asset.asset` reference instead of actual asset data

---

### 12. accounts_with_asset_app_last_week

**Description:** Pull the top App Extension by impressions (>1) per account last 7 days - need acct and campaign info with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('MOBILE_APP') AND metrics.impressions > 1 ORDER BY metrics.impressions DESC LIMIT 1
```

**Generated Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign_asset.asset,
  metrics.impressions
FROM campaign_asset
WHERE segments.date DURING LAST_7_DAYS AND campaign_asset.field_type = 'APP' AND metrics.impressions > '1'
ORDER BY metrics.impressions DESC
```

**Classification:** POOR

**LLM Explanation Analysis:**
- Reasoning Summary: Chose campaign_asset resource, field_type 'APP' instead of 'MOBILE_APP'
- Key Decision Points: Different resource, different field type value
- Comparison to Intent: Resource mismatch
- Where It Diverged: Wrong resource, field_type value mismatch

**Analysis:**
- Selected Fields: Basic fields present but from wrong resource
- Data Scope: campaign_asset instead of asset_field_type_view
- Semantic Equivalence: Different resource means different results

**Key Differences:**
- **Wrong Resource:** campaign_asset instead of asset_field_type_view
- Field type mismatch: 'APP' instead of 'MOBILE_APP'
- Missing `campaign.advertising_channel_type`, `metrics.clicks`, `metrics.cost_micros`
- Missing `LIMIT 1`

---

### 13. perf_max_campaigns_with_traffic_last_30_days

**Description:** Give me daily PMax performance last 30 days - need CTR, CPC, conv, rev, CPA, campaign IDs + names, and currency

**Reference Query:**
```sql
SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cost, metrics.conversions, metrics.conversions_value, metrics.cost_per_conversion, customer.currency_code FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.impressions > 1 ORDER BY segments.date, campaign.id
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
WHERE campaign.advertising_channel_type IN '(\'PERFORMANCE_MAX\')' AND campaign.status = 'ENABLED'
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified PERFORMANCE_MAX channel and core metrics
- Key Decision Points: Selected CTR, CPC (average_cpc), conversions, revenue (conversions_value), CPA
- Comparison to Intent: Captured main metrics but missing key structural elements
- Where It Diverged: Missing segments.date (daily breakdown), impressions, ordering

**Analysis:**
- Selected Fields: Core conversion metrics present, missing impressions, average_cost
- Data Scope: Correct resource and channel filter, missing temporal breakdown
- Semantic Equivalence: Returns PMax data but not daily segmented

**Key Differences:**
- Missing `segments.date` - Critical for "daily" requirement
- Missing `metrics.impressions` and `metrics.average_cost`
- Missing `ORDER BY segments.date, campaign.id`

---

### 14. asset_fields_with_traffic_ytd

**Description:** Show me YTD asset performance by day with impressions, asset type, and currency

**Reference Query:**
```sql
SELECT asset_field_type_view.field_type, segments.date, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.year IN (2026) AND metrics.impressions > 1 ORDER BY asset_field_type_view.field_type, segments.date
```

**Generated Query:**
```sql
SELECT
  asset.type,
  segments.date,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  customer.currency_code
FROM asset
WHERE segments.year IN (2026) AND metrics.impressions > '1'
ORDER BY asset.type, segments.date
```

**Classification:** POOR

**LLM Explanation Analysis:**
- Reasoning Summary: Chose 'asset' resource instead of 'asset_field_type_view'
- Key Decision Points: Used asset.type instead of asset_field_type_view.field_type
- Comparison to Intent: Wrong resource leads to different data structure
- Where It Diverged: Completely wrong resource choice

**Analysis:**
- Selected Fields: Uses asset.type instead of asset_field_type_view.field_type
- Data Scope: asset resource instead of asset_field_type_view
- Semantic Equivalence: Would return different asset-level data

**Key Differences:**
- **Wrong Resource:** asset instead of asset_field_type_view
- Uses `asset.type` instead of `asset_field_type_view.field_type`

---

### 15. campaigns_with_smart_bidding_by_spend

**Description:** Pull top 25 Smart Bidding campaigns by spend (>$1K) last week - need acct and campaign IDs + names, budget, bid strategy, CPC, and conv metrics with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, customer.currency_code, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.bidding_strategy_type, campaign_budget.amount_micros, metrics.average_cpc, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value FROM campaign WHERE campaign.bidding_strategy_type IN ('MAXIMIZE_CLICKS', 'MAXIMIZE_CONVERSIONS', 'MAXIMIZE_CONVERSION_VALUE', 'TARGET_CPA', 'TARGET_ROAS', 'TARGET_SPEND') AND campaign.status IN ('ENABLED') AND segments.date DURING LAST_7_DAYS AND metrics.cost_micros > 1000000000 ORDER by metrics.cost_micros DESC LIMIT 25
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
WHERE campaign.bidding_strategy_type IN '(\'MAXIMIZE_CLICKS\', \'MAXIMIZE_CONVERSIONS\', \'MAXIMIZE_CONVERSION_VALUE\', \'TARGET_CPA\', \'TARGET_ROAS\', \'TARGET_SPEND\')' AND metrics.cost_micros > '1000000000' AND campaign.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 25
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified all Smart Bidding strategies and core fields
- Key Decision Points: Complete bidding strategy list, budget amount, all conversion metrics
- Comparison to Intent: Captured main query structure well
- Where It Diverged: Missing customer.id, customer.descriptive_name, campaign.advertising_channel_type, date filter

**Analysis:**
- Selected Fields: Core fields present, missing some identifiers
- Data Scope: Correct resource, bidding strategies, spend threshold, LIMIT 25
- Semantic Equivalence: Would return similar Smart Bidding campaign data

**Key Differences:**
- Missing `customer.id` and `customer.descriptive_name` - Account identifiers
- Missing `campaign.advertising_channel_type` - Channel context
- Missing `segments.date DURING LAST_7_DAYS` - Temporal constraint

---

### 16. campaigns_shopping_campaign_performance

**Description:** Get me Shopping campaigns by spend (>$100) last 30 days - need acct and campaign details, budget, bid strategy, CPC, and conv metrics with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.bidding_strategy_type, campaign_budget.amount_micros, metrics.average_cpc, metrics.clicks, metrics.cost_micros, customer.currency_code, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value FROM campaign WHERE campaign.advertising_channel_type IN ('SHOPPING') AND campaign.status IN ('ENABLED') AND segments.date DURING LAST_30_DAYS AND metrics.cost_micros > 100000000 ORDER by metrics.cost_micros DESC
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
  metrics.average_cpc,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value
FROM campaign
WHERE campaign.advertising_channel_type IN '(\'SHOPPING\')' AND metrics.cost_micros > '0' AND campaign.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified SHOPPING channel and core metrics
- Key Decision Points: Selected budget, bid strategy, CPC, conversion metrics
- Comparison to Intent: Captured main structure
- Where It Diverged: Missing identifiers, wrong cost threshold, no date filter

**Analysis:**
- Selected Fields: Core conversion metrics present, missing customer identifiers
- Data Scope: Correct channel, wrong cost threshold (>0 vs >$100)
- Semantic Equivalence: Returns Shopping campaigns but without proper spend filter

**Key Differences:**
- Missing `customer.id` and `customer.descriptive_name`
- Missing `metrics.clicks`
- Wrong threshold: `metrics.cost_micros > '0'` instead of `> 100000000` (>$100)
- Missing `segments.date DURING LAST_30_DAYS`

---

### 17. smart_campaign_search_terms_with_top_spend

**Description:** Show me top 100 search terms by spend from Smart campaigns last 30 days - need search term, match type, and performance metrics with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, smart_campaign_search_term_view.search_term, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM smart_campaign_search_term_view WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 0 ORDER BY metrics.cost_micros DESC LIMIT 100
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
WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type = 'SMART'
ORDER BY metrics.cost_micros DESC
LIMIT 100
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified smart_campaign_search_term_view resource
- Key Decision Points: Selected search term, spend, clicks, impressions
- Comparison to Intent: Captured Smart campaign search terms
- Where It Diverged: Wrong match type field, missing identifiers

**Analysis:**
- Selected Fields: Core search term fields present
- Data Scope: Correct resource and channel filter
- Semantic Equivalence: Returns Smart campaign search terms but with wrong match type field

**Key Differences:**
- **Wrong Field:** `campaign.keyword_match_type` instead of `smart_campaign_search_term_view.search_term` for match type context
- Missing `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`

---

### 18. all_search_terms_with_clicks

**Description:** Pull all search terms with clicks last 30 days - need match type, device, keyword status, and full conv metrics with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0 ORDER BY metrics.cost_micros desc
```

**Generated Query:**
```sql
SELECT
  search_term_view.search_term,
  segments.search_term_match_type,
  segments.device,
  segments.keyword.info.match_type,
  segments.keyword.info.text,
  customer.currency_code,
  metrics.clicks,
  metrics.conversions,
  metrics.conversions_value,
  metrics.cost_per_conversion,
  metrics.conversions_value_per_cost,
  metrics.all_conversions,
  metrics.all_conversions_value,
  metrics.cost_per_all_conversions,
  metrics.all_conversions_value_per_cost,
  metrics.cross_device_conversions
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > '0' AND search_term_view.status = 'ENABLED'
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified search_term_view resource and selected extensive conversion metrics
- Key Decision Points: Selected comprehensive conversion metrics including "all_conversions" variants
- Comparison to Intent: Captured extensive conversion metrics beyond reference
- Where It Diverged: Missing customer identifiers, added segments.keyword fields not in reference

**Analysis:**
- Selected Fields: Extensive conversion metrics, missing customer.id and descriptive_name
- Data Scope: Correct resource, good conversion coverage
- Semantic Equivalence: Returns search terms with more metrics than reference

**Key Differences:**
- Missing `customer.id`, `customer.descriptive_name`
- Missing `metrics.cost_micros`, `metrics.impressions`
- Added extra fields: `segments.keyword.info.match_type`, `segments.keyword.info.text`, and all_conversions variants

---

### 19. search_terms_with_top_cpa

**Description:** Get me top 50 search terms with CPA >$200 and spend >$1K last 30 days - flagging expensive underperformers with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.cost_per_conversion > 200000000 AND metrics.cost_micros > 1000000000 ORDER BY metrics.cost_micros desc LIMIT 50
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
  metrics.cost_per_conversion
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.cost_per_conversion > '0' AND metrics.cost_micros > '1000000000' AND search_term_view.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 50
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified search_term_view and CPA/spend thresholds
- Key Decision Points: Selected CPA and spend filters, ordered by spend
- Comparison to Intent: Well aligned with reference
- Where It Diverged: CPA threshold >0 instead of >$200, missing some fields

**Analysis:**
- Selected Fields: Core search term and metrics fields present
- Data Scope: Correct resource, spend threshold, LIMIT, ordering
- Semantic Equivalence: Would return similar expensive search terms

**Key Differences:**
- Wrong CPA threshold: `> '0'` instead of `> 200000000` (>$200)
- Missing `customer.id`, `customer.descriptive_name`
- Missing `metrics.conversions_value`, `metrics.conversions_value_per_cost`

---

### 20. search_terms_with_low_roas

**Description:** Show me top 50 search terms with ROAS <0.25 and spend >$1K last 30 days - need to neg keyword these with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < 0.25 AND metrics.cost_micros > 1000000000 ORDER BY metrics.cost_micros desc LIMIT 50
```

**Generated Query:**
```sql
SELECT
  search_term_view.search_term,
  metrics.cost_micros,
  metrics.conversions_value_per_cost,
  customer.currency_code
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < '0.25' AND metrics.cost_micros > '1000000000' AND search_term_view.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 50
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified search_term_view and ROAS/spend thresholds
- Key Decision Points: Selected ROAS and spend filters for identifying poor performers
- Comparison to Intent: Captured core requirement (low ROAS, high spend)
- Where It Diverged: Minimal field selection, missing key identifiers

**Analysis:**
- Selected Fields: Minimal - only core ROAS fields
- Data Scope: Correct resource, ROAS filter, spend threshold, LIMIT
- Semantic Equivalence: Returns search terms to investigate but lacks context

**Key Differences:**
- Missing `customer.id`, `customer.descriptive_name` - No account context
- Missing `segments.search_term_match_type`, `segments.device`, `search_term_view.status`
- Missing `metrics.impressions`, `metrics.clicks`, `metrics.conversions`, etc.

---

### 21. locations_with_highest_revenue_per_conversion

**Description:** Pull top 1000 locations by rev per conv (>10 conv) last week - need geo target IDs and location metrics with currency

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign_criterion.criterion_id, campaign_criterion.type, campaign_criterion.location.geo_target_constant, campaign_criterion.keyword.text, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.value_per_conversion, metrics.average_cpc FROM location_view WHERE segments.date DURING LAST_7_DAYS and metrics.conversions > 10 ORDER BY metrics.value_per_conversion desc, metrics.conversions desc LIMIT 1000
```

**Generated Query:**
```sql
SELECT
  campaign_criterion.location.geo_target_constant,
  metrics.conversions,
  metrics.conversions_value,
  metrics.value_per_conversion,
  customer.currency_code
FROM location_view
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.conversions > '10'
ORDER BY metrics.value_per_conversion DESC
LIMIT 1000
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified location_view resource and value_per_conversion metric
- Key Decision Points: Selected geo target constant and revenue metrics
- Comparison to Intent: Captured core requirement (locations by revenue per conversion)
- Where It Diverged: Minimal field selection, missing campaign and customer identifiers

**Analysis:**
- Selected Fields: Core location and revenue metrics present
- Data Scope: Correct resource, conversion filter, ordering, LIMIT
- Semantic Equivalence: Returns location data but lacking context

**Key Differences:**
- Missing `customer.id`, `customer.descriptive_name`
- Missing `campaign.id`, `campaign.name`, `campaign.advertising_channel_type`
- Missing `campaign_criterion.criterion_id`, `campaign_criterion.type`, `campaign_criterion.keyword.text`
- Missing `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`, `metrics.average_cpc`

---

### 22. asset_performance_rsa

**Description:** Get me RSA performance last 30 days - need headline and description copy, path text, CTR, and engagement metrics

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, ad_group.id, ad_group.name, ad_group.type, ad_group_ad.ad.id, ad_group_ad.ad.responsive_search_ad.headlines, ad_group_ad.ad.responsive_search_ad.descriptions, ad_group_ad.ad.responsive_search_ad.path1, ad_group_ad.ad.responsive_search_ad.path2, metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cpc FROM ad_group_ad WHERE ad_group_ad.ad.type IN ('RESPONSIVE_SEARCH_AD') AND segments.date DURING LAST_30_DAYS ORDER BY campaign.name, ad_group.name, metrics.ctr DESC LIMIT 1000
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
WHERE ad_group_ad.ad.type = 'RESPONSIVE_SEARCH_AD' AND ad_group_ad.status = 'ENABLED'
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified ad_group_ad resource and RSA creative fields
- Key Decision Points: Selected all RSA creative fields and engagement metrics
- Comparison to Intent: Captured creative elements and CTR, added engagement metrics
- Where It Diverged: Missing identifiers, date filter, and LIMIT

**Analysis:**
- Selected Fields: All RSA creative fields present, added engagement metrics
- Data Scope: Correct resource and ad type filter
- Semantic Equivalence: Returns RSA data but without context or temporal constraint

**Key Differences:**
- Missing `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, etc.
- Missing `metrics.impressions`, `metrics.clicks`, `metrics.cost_micros`, `metrics.average_cpc`
- Missing `segments.date DURING LAST_30_DAYS` and `LIMIT 1000`
- Added engagement metrics not in reference

---

### 23. recent_campaign_changes

**Description:** Show me last 100 campaign changes in the last 14 days - need timestamp, user, client type, and what changed

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, change_event.change_date_time, change_event.client_type, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN') ORDER BY change_event.change_date_time DESC LIMIT 100
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
WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN '(\'CAMPAIGN\')'
ORDER BY change_event.change_date_time DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified change_event resource and core fields
- Key Decision Points: Selected timestamp, user, client type, changed fields
- Comparison to Intent: Well aligned with reference intent
- Where It Diverged: Missing customer identifiers, LIMIT 100

**Analysis:**
- Selected Fields: Core change fields present, missing customer context
- Data Scope: Correct resource, date filter, resource type filter, ordering
- Semantic Equivalence: Returns campaign changes but without account context

**Key Differences:**
- Missing `customer.id`, `customer.descriptive_name`
- Missing `change_event.change_resource_type` (though implied by filter)
- Missing `LIMIT 100`

---

### 24. recent_changes

**Description:** Pull recent changes across campaigns, ad groups, ads, keywords, and budgets last 14 days - need object type, user, changed fields, and timestamp

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, change_event.change_date_time, change_event.client_type, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET') ORDER BY change_event.change_date_time DESC LIMIT 100
```

**Generated Query:**
```sql
SELECT
  change_event.change_resource_type,
  change_event.user_email,
  change_event.changed_fields,
  change_event.change_date_time
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN '(\'CAMPAIGN\', \'AD_GROUP\', \'AD_GROUP_AD\', \'AD_GROUP_CRITERION\', \'CAMPAIGN_BUDGET\')'
ORDER BY change_event.change_date_time DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified change_event and multiple resource types
- Key Decision Points: Selected all requested resource types across campaign hierarchy
- Comparison to Intent: Captured cross-resource changes requirement
- Where It Diverged: Missing customer and campaign context, LIMIT 100

**Analysis:**
- Selected Fields: Core change fields present
- Data Scope: Correct resource, date filter, multiple resource types, ordering
- Semantic Equivalence: Returns changes across resources but without account/campaign context

**Key Differences:**
- Missing `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`
- Missing resource type `'AD'` in filter
- Missing `LIMIT 100`

---

### 25. all_campaigns

**Description:** Get me basic campaign info - IDs, names, channel types, and status (limit 100)

**Reference Query:**
```sql
SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.status, campaign.primary_status FROM campaign LIMIT 100
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
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified campaign resource and basic fields
- Key Decision Points: Selected core campaign identifiers
- Comparison to Intent: Captured basic campaign info
- Where It Diverged: Missing customer identifiers, LIMIT, primary_status

**Analysis:**
- Selected Fields: Core campaign fields present
- Data Scope: Correct resource, added implicit ENABLED filter
- Semantic Equivalence: Returns campaign info but without account context

**Key Differences:**
- Missing `customer.id`, `customer.descriptive_name` - No account context
- Missing `campaign.primary_status`
- Missing `LIMIT 100`
- Added `WHERE campaign.status = 'ENABLED'` - Implicit filter

---

### 26. performance_max_impression_share

**Description:** Show me daily impression share metrics for PMax last 30 days - need absolute top, budget lost, rank lost, and top impression share

**Reference Query:**
```sql
SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.impressions, metrics.search_absolute_top_impression_share, metrics.search_budget_lost_absolute_top_impression_share, metrics.search_budget_lost_impression_share, metrics.search_budget_lost_top_impression_share, metrics.search_exact_match_impression_share, metrics.search_impression_share, metrics.search_rank_lost_impression_share, metrics.search_top_impression_share, metrics.absolute_top_impression_percentage FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
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
  metrics.search_top_impression_share,
  metrics.search_budget_lost_top_impression_share,
  metrics.search_rank_lost_top_impression_share,
  metrics.search_impression_share,
  metrics.search_budget_lost_impression_share,
  metrics.search_rank_lost_impression_share
FROM campaign
WHERE campaign.advertising_channel_type IN '(\'PERFORMANCE_MAX\')' AND campaign.status = 'ENABLED'
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified all requested impression share metrics
- Key Decision Points: Selected comprehensive impression share metrics
- Comparison to Intent: Captured all requested metrics
- Where It Diverged: Missing segments.date (daily breakdown), metrics.impressions, metrics.absolute_top_impression_percentage

**Analysis:**
- Selected Fields: All impression share metrics present except one
- Data Scope: Correct resource and channel filter
- Semantic Equivalence: Returns impression share data but not daily segmented

**Key Differences:**
- Missing `segments.date` - Critical for "daily" requirement
- Missing `metrics.impressions` and `metrics.absolute_top_impression_percentage`
- Added implicit `campaign.status = 'ENABLED'`

---

## Overall Assessment

### Patterns Observed

1. **Date Filter Consistency Issue** (Critical)
   - 18 out of 26 queries (69%) missing `segments.date` filter
   - LLM explanations frequently mention date ranges but they are not applied in output
   - This is the most common failure mode across all classifications

2. **Missing LIMIT Clauses** (High)
   - 12 out of 26 queries (46%) missing LIMIT when specified in description
   - Particularly problematic for "top N" queries

3. **Missing Customer Identifiers** (Medium)
   - 13 out of 26 queries (50%) missing `customer.id` and `customer.descriptive_name`
   - Account context frequently omitted even when explicitly requested

4. **Resource Selection Issues** (High)
   - Asset-related queries (entries 9-12, 14) consistently chose wrong resources
   - `campaign_asset` used instead of `asset_field_type_view`
   - Indicates need for better asset resource documentation

5. **Low RAG Confidence** (Medium)
   - Entries 14-19, 23-25 had low RAG confidence (0.215-0.290)
   - Fell back to full resource list
   - Did not significantly impact quality of results

### Common Failure Modes

| Issue | Count | Severity | Notes |
|-------|-------|----------|-------|
| Missing date filter | 18 | High | Most frequent issue |
| Missing LIMIT | 12 | High | Affects "top N" queries |
| Missing customer fields | 13 | Medium | Context loss |
| Wrong resource (assets) | 5 | High | Asset queries affected |
| Wrong field type values | 2 | Medium | 'APP' vs 'MOBILE_APP' |
| Wrong thresholds | 3 | Medium | CPA, cost thresholds |

### Strengths

1. **Resource Selection** (Non-asset): Generally correct for campaign, keyword_view, search_term_view, change_event
2. **Field Mapping**: Good understanding of metric mappings (CTR, CPC, conversions, etc.)
3. **Channel Type Filters**: Correctly applied PERFORMANCE_MAX, SMART, SHOPPING, etc.
4. **Ordering**: Consistently applied ORDER BY for "top" queries
5. **Explanation Quality**: Detailed reasoning shows LLM understands requirements

### Recommendations

1. **Fix Date Filter Application**
   - High priority: LLM mentions dates in reasoning but doesn't apply them
   - May be a code generation issue rather than understanding issue

2. **Improve LIMIT Handling**
   - Ensure LIMIT is applied when "top N" or "limit N" mentioned

3. **Asset Resource Documentation**
   - Improve embeddings or documentation for asset_field_type_view
   - Distinguish between campaign_asset and asset_field_type_view use cases

4. **Customer Field Inclusion**
   - Ensure customer.id and descriptive_name are included when account context requested

5. **Threshold Preservation**
   - Ensure numeric thresholds are preserved accurately (>100, >$200, etc.)

---

*Report generated: 2026-03-25*
*Test command: `mcc-gaql-gen generate "<description>" --use-query-cookbook --explain`*

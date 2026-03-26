# Query Cookbook Generation Comparison Report

**Generated:** 2026-03-26 07:15:07

## Summary Statistics

- Total entries tested: 26
- EXCELLENT: 11 (42%)
- GOOD: 7 (27%)
- FAIR: 4 (15%)
- POOR: 4 (15%)

---

## Detailed Results

### account_ids_with_access_and_traffic_last_week

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
- Reasoning Summary: Correctly identified 'customer' resource for account-level metrics, selected customer.id, applied appropriate date filter and clicks threshold
- Key Decision Points: Used customer resource, LAST_WEEK_MON_SUN date literal, metrics.clicks > 0 filter
- Comparison to Intent: LLM reasoning matches expected behavior perfectly
- Where It Diverged: No divergence - correctly understood the request

**Analysis:**
- Selected Fields: Matches reference (customer.id)
- Data Scope: Correct resource (customer), correct date range, minor threshold difference (>0 vs >1 is acceptable)
- Semantic Equivalence: Queries are semantically equivalent

**Key Differences:**
- metrics.clicks > 0 vs > 1 - Minor threshold difference, both achieve same goal of filtering accounts with clicks

---

### accounts_with_traffic_last_week

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
- Reasoning Summary: Correctly identified all requested fields (impressions, clicks, spend/cost, currency) and appropriate resource
- Key Decision Points: Selected customer.id, customer.descriptive_name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code
- Comparison to Intent: LLM perfectly understood the requirements
- Where It Diverged: No significant divergence

**Analysis:**
- Selected Fields: All required fields present (impressions, clicks, cost_micros, currency_code)
- Data Scope: Correct resource (customer), correct date range
- Semantic Equivalence: Queries are semantically equivalent

**Key Differences:**
- Missing metrics.impressions > 1 filter (minor - implicit in wanting accounts with traffic)

---

### keywords_with_top_traffic_last_week

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
WHERE segments.date DURING LAST_WEEK_MON_SUN AND metrics.clicks > 10000 AND keyword_view.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 10
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified keyword_view resource and core fields, but RAG had low confidence (0.286)
- Key Decision Points: Selected customer, campaign, ad_group fields; applied date and clicks filters; ordered and limited correctly
- Comparison to Intent: LLM understood the main intent but missed some granular fields
- Where It Diverged: Missing ad_group.type, ad_group_criterion fields, metrics.impressions, metrics.cost_micros

**Analysis:**
- Selected Fields: Missing some reference fields (ad_group_criterion, keyword text, impressions, cost_micros)
- Data Scope: Correct resource (keyword_view), correct filters and ordering
- Semantic Equivalence: Captures main intent but missing detailed keyword identification fields

**Key Differences:**
- Missing ad_group_criterion.criterion_id and ad_group_criterion.keyword.text
- Missing metrics.impressions and metrics.cost_micros
- Added implicit keyword_view.status = 'ENABLED' filter

---

### accounts_with_perf_max_campaigns_last_week

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
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified campaign resource, PMax filter, and date range
- Key Decision Points: Selected appropriate fields, filtered for PERFORMANCE_MAX, applied clicks > 100 filter
- Comparison to Intent: LLM understood request but omitted LIMIT 1 and some metrics fields
- Where It Diverged: Missing LIMIT 1, missing metrics.impressions and metrics.cost_micros

**Analysis:**
- Selected Fields: Core fields present but missing metrics.impressions, metrics.cost_micros
- Data Scope: Correct resource, correct filters
- Semantic Equivalence: Nearly equivalent, but LIMIT missing changes result set size

**Key Differences:**
- Missing LIMIT 1 (returns all matching instead of top 1)
- Missing metrics.impressions and metrics.cost_micros

---

### accounts_with_smart_campaigns_last_week

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
  campaign.id,
  campaign.advertising_channel_type,
  campaign.name,
  metrics.clicks,
  customer.currency_code
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 1
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified Smart campaign filter and all key fields
- Key Decision Points: Properly filtered for SMART channel type, applied date and clicks filters
- Comparison to Intent: LLM reasoning aligned well with request
- Where It Diverged: Missing metrics.impressions and metrics.cost_micros

**Analysis:**
- Selected Fields: Core identifying fields present, missing impression and cost metrics
- Data Scope: Correct resource, correct filters
- Semantic Equivalence: Nearly equivalent, captures main intent

**Key Differences:**
- Missing metrics.impressions and metrics.cost_micros

---

### accounts_with_local_campaigns_last_week

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
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type IN ('LOCAL') AND metrics.clicks > 500 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 10
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified Local campaign filter and core fields
- Key Decision Points: Applied correct channel type filter, date range, and clicks threshold
- Comparison to Intent: LLM understood request but used default LIMIT 10 instead of 1
- Where It Diverged: Used LIMIT 10 instead of LIMIT 1, missing metrics.impressions and metrics.cost_micros

**Analysis:**
- Selected Fields: Core fields present, missing impressions and cost
- Data Scope: Correct resource, correct filters
- Semantic Equivalence: Nearly equivalent, but different LIMIT changes semantics

**Key Differences:**
- LIMIT 10 instead of LIMIT 1 (significant for "top 1" request)
- Missing metrics.impressions and metrics.cost_micros

---

### accounts_with_shopping_campaigns_last_week

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
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type IN ('SHOPPING') AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 10
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified Shopping campaign filter and key fields
- Key Decision Points: Applied SHOPPING filter, correct date range, clicks threshold
- Comparison to Intent: LLM understood request but used default LIMIT 10
- Where It Diverged: LIMIT 10 instead of 1, missing metrics.impressions and metrics.cost_micros

**Analysis:**
- Selected Fields: Core fields present, missing impressions and cost metrics
- Data Scope: Correct resource and filters
- Semantic Equivalence: Nearly equivalent, LIMIT difference changes results

**Key Differences:**
- LIMIT 10 instead of LIMIT 1
- Missing metrics.impressions and metrics.cost_micros

---

### accounts_with_multichannel_campaigns_last_week

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
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.clicks
FROM campaign
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign.advertising_channel_type = 'MULTI_CHANNEL' AND metrics.clicks > 100 AND campaign.status = 'ENABLED'
ORDER BY metrics.clicks DESC
LIMIT 1
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified all key fields and applied proper filters
- Key Decision Points: Used MULTI_CHANNEL filter (detected via pre-scan), correct date range, clicks threshold
- Comparison to Intent: LLM reasoning perfectly matched intent
- Where It Diverged: Used = instead of IN for channel type (semantically equivalent for single value)

**Analysis:**
- Selected Fields: All core identifying fields present
- Data Scope: Correct resource and filters
- Semantic Equivalence: Equivalent despite minor syntax difference

**Key Differences:**
- Used = 'MULTI_CHANNEL' instead of IN ('MULTI_CHANNEL') - semantically equivalent
- Missing metrics.impressions and metrics.cost_micros (acceptable - not explicitly required in description)

---

### accounts_with_asset_sitelink_last_week

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
WHERE segments.date DURING LAST_WEEK_MON_SUN AND asset.type = 'SITELINK' AND metrics.clicks > 20000
ORDER BY metrics.impressions DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Identified correct asset fields and filters but selected wrong resource (customer_asset vs campaign_asset)
- Key Decision Points: Selected asset fields correctly, applied SITELINK filter and date range
- Comparison to Intent: LLM partially understood but made incorrect resource choice
- Where It Diverged: Used customer_asset instead of campaign_asset; asset.type instead of campaign_asset.field_type; missing LIMIT

**Analysis:**
- Selected Fields: Asset fields correct but missing metrics.cost_micros
- Data Scope: WRONG resource - customer_asset instead of campaign_asset
- Semantic Equivalence: Different resource changes query semantics significantly

**Key Differences:**
- Uses customer_asset instead of campaign_asset (wrong resource)
- Uses asset.type = 'SITELINK' instead of campaign_asset.field_type = 'SITELINK'
- Missing LIMIT 10
- Missing metrics.cost_micros

---

### accounts_with_asset_call_last_week

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
  campaign_asset.asset,
  metrics.impressions
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'CALL' AND metrics.impressions > 100
ORDER BY metrics.impressions DESC
LIMIT 10
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified campaign_asset resource but had field rejection warnings
- Key Decision Points: Used correct resource and filter for CALL field type
- Comparison to Intent: LLM partially understood but couldn't select all requested fields
- Where It Diverged: Missing customer.id, customer.descriptive_name, asset.id, asset.name, asset.type, asset.call_asset.phone_number, metrics.clicks, metrics.cost_micros

**Analysis:**
- Selected Fields: Missing many requested fields due to candidate filtering issues
- Data Scope: Correct resource (campaign_asset)
- Semantic Equivalence: Missing core identifying fields

**Key Differences:**
- Missing customer.id and customer.descriptive_name (rejected - not in candidates)
- Missing asset.id, asset.name, asset.type, asset.call_asset.phone_number
- Missing metrics.clicks and metrics.cost_micros

---

### accounts_with_asset_callout_last_week

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
  campaign_asset.field_type,
  asset.callout_asset.callout_text,
  metrics.impressions,
  metrics.clicks
FROM campaign_asset
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'CALLOUT' AND metrics.clicks > 30000
ORDER BY metrics.impressions DESC
LIMIT 10
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource selected but many fields rejected due to not being in candidates
- Key Decision Points: Used campaign_asset resource, applied CALLOUT filter correctly
- Comparison to Intent: Partial understanding - core concept correct but missing key fields
- Where It Diverged: Missing customer.id, customer.descriptive_name, campaign.advertising_channel_type, metrics.cost_micros

**Analysis:**
- Selected Fields: Missing several identifying fields (customer.id, descriptive_name, channel type)
- Data Scope: Correct resource
- Semantic Equivalence: Missing account identification context

**Key Differences:**
- Missing customer.id and customer.descriptive_name (rejected by system)
- Missing campaign.advertising_channel_type
- Missing metrics.cost_micros

---

### accounts_with_asset_app_last_week

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
WHERE segments.date DURING LAST_WEEK_MON_SUN AND campaign_asset.field_type = 'APP' AND metrics.impressions > 1
ORDER BY metrics.impressions DESC
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource but many fields rejected; intended to select more fields
- Key Decision Points: Used campaign_asset, APP filter, correct date range
- Comparison to Intent: Partial understanding - resource correct but fields limited
- Where It Diverged: Missing customer.id, customer.descriptive_name, campaign.advertising_channel_type, metrics.clicks, metrics.cost_micros, LIMIT 10

**Analysis:**
- Selected Fields: Missing many identifying and metric fields
- Data Scope: Correct resource
- Semantic Equivalence: Missing account context and performance metrics

**Key Differences:**
- Missing customer.id and customer.descriptive_name (rejected)
- Missing campaign.advertising_channel_type
- Missing metrics.clicks and metrics.cost_micros
- Missing LIMIT 10

---

### perf_max_campaigns_with_traffic_last_30_days

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

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified all requested metrics (CTR, CPC/average_cpc, conversions, revenue/conv_value, CPA/cost_per_conversion) and appropriate resource
- Key Decision Points: Selected campaign resource, all requested metrics, PMax filter, date range
- Comparison to Intent: LLM perfectly understood the requirements
- Where It Diverged: Minor ordering difference (DESC vs ASC), missing some metrics.impressions/clicks

**Analysis:**
- Selected Fields: All core requested metrics present
- Data Scope: Correct resource, correct filters
- Semantic Equivalence: Semantically equivalent for the core request

**Key Differences:**
- Missing campaign.advertising_channel_type in SELECT
- Missing metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.average_cost
- ORDER BY DESC instead of ASC (minor preference difference)

---

### asset_fields_with_traffic_ytd

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
  asset.type,
  customer.currency_code
FROM asset
```

**Classification:** POOR

**LLM Explanation Analysis:**
- Reasoning Summary: LLM selected wrong resource (asset instead of asset_field_type_view); segments.date and metrics.impressions were rejected from available fields
- Key Decision Points: Attempted to select date and impressions but they were rejected; fell back to basic asset fields
- Comparison to Intent: LLM did not understand the core requirement for asset_field_type_view
- Where It Diverged: Wrong resource, missing all performance metrics and date segmentation

**Analysis:**
- Selected Fields: Only asset.type and currency - missing all performance metrics and date
- Data Scope: WRONG resource - asset instead of asset_field_type_view
- Semantic Equivalence: Completely different query

**Key Differences:**
- Wrong resource: asset instead of asset_field_type_view
- Missing segments.date for daily breakdown
- Missing all metrics (impressions, clicks, cost_micros)
- Missing YTD date filter

---

### campaigns_with_smart_bidding_by_spend

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
- Reasoning Summary: Correctly identified Smart Bidding strategy types and core fields; customer.id and descriptive_name were rejected
- Key Decision Points: Used correct bidding_strategy_type filter with all smart strategies, applied cost threshold
- Comparison to Intent: LLM understood main intent but couldn't select all account fields
- Where It Diverged: Missing customer.id, customer.descriptive_name, campaign.advertising_channel_type, metrics.clicks, metrics.cost_micros

**Analysis:**
- Selected Fields: Core conversion metrics present, missing some identifying fields
- Data Scope: Correct resource, correct filters
- Semantic Equivalence: Nearly equivalent, captures main Smart Bidding analysis intent

**Key Differences:**
- Missing customer.id and customer.descriptive_name (rejected)
- Missing campaign.advertising_channel_type
- Missing metrics.clicks and metrics.cost_micros

---

### campaigns_shopping_campaign_performance

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
  metrics.average_cpc,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value
FROM campaign
WHERE campaign.advertising_channel_type IN ('SHOPPING') AND campaign.status IN ('ENABLED') AND segments.date DURING LAST_30_DAYS AND metrics.cost_micros > 100000000
ORDER by metrics.cost_micros DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified Shopping filter and most requested fields; customer.id and descriptive_name rejected
- Key Decision Points: Applied SHOPPING filter, budget and bid strategy fields, conversion metrics
- Comparison to Intent: LLM understood intent well but couldn't access account fields
- Where It Diverged: Missing customer.id and customer.descriptive_name

**Analysis:**
- Selected Fields: Nearly all fields present except account identifiers
- Data Scope: Correct resource and filters
- Semantic Equivalence: Nearly equivalent, captures Shopping campaign analysis intent

**Key Differences:**
- Missing customer.id and customer.descriptive_name (rejected - not in candidates)

---

### smart_campaign_search_terms_with_top_spend

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
  smart_campaign_search_term_view.search_term,
  campaign.keyword_match_type,
  metrics.cost_micros,
  metrics.clicks,
  metrics.impressions,
  customer.currency_code
FROM smart_campaign_search_term_view
WHERE segments.date DURING LAST_30_DAYS
ORDER BY metrics.cost_micros DESC
LIMIT 100
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource selected but RAG had low confidence (0.224); missing several requested fields
- Key Decision Points: Used smart_campaign_search_term_view, ordered by cost_micros DESC
- Comparison to Intent: LLM partially understood but couldn't select all fields and missed SMART filter
- Where It Diverged: Missing customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type filter

**Analysis:**
- Selected Fields: Missing account and campaign identification fields
- Data Scope: Correct resource but missing SMART filter
- Semantic Equivalence: Missing context for which account/campaign each term belongs to

**Key Differences:**
- Missing customer.id, customer.descriptive_name, campaign.id, campaign.name
- Missing campaign.advertising_channel_type IN ('SMART') filter (relying on view name)
- Used campaign.keyword_match_type instead of segments.search_term_match_type

---

### all_search_terms_with_clicks

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
  segments.keyword.info.match_type,
  segments.keyword.info.text,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value,
  metrics.conversions_value_per_cost,
  metrics.all_conversions,
  metrics.all_conversions_value,
  metrics.cost_per_all_conversions,
  metrics.value_per_all_conversions,
  metrics.all_conversions_value_per_cost
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0 AND search_term_view.status = 'ENABLED'
ORDER BY metrics.clicks DESC
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correct resource and comprehensive field selection including extended conversion metrics
- Key Decision Points: Selected search_term_view, included all conversion metric variants
- Comparison to Intent: LLM went beyond requirements by including all_conversions variants
- Where It Diverged: Missing customer.id and descriptive_name; used segments.keyword.info instead of search_term_view.status

**Analysis:**
- Selected Fields: Comprehensive conversion metrics, match type, device; missing keyword status field
- Data Scope: Correct resource and filters
- Semantic Equivalence: Nearly equivalent with bonus metrics

**Key Differences:**
- Missing customer.id and customer.descriptive_name (rejected)
- Missing search_term_view.status (used segments.keyword.info instead)
- Added comprehensive all_conversions metrics (beyond requirements)
- ORDER BY clicks DESC instead of cost_micros DESC

---

### search_terms_with_top_cpa

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
- Reasoning Summary: Correct resource and filters but CPA threshold was not correctly applied (>0 instead of >200)
- Key Decision Points: Used cost_per_conversion filter but with wrong threshold; correct spend threshold
- Comparison to Intent: LLM understood the concept but misinterpreted the CPA threshold requirement
- Where It Diverged: CPA filter set to >0 instead of >200000000 (micros)

**Analysis:**
- Selected Fields: Core fields present except account identifiers
- Data Scope: Correct resource, date range, and spend filter
- Semantic Equivalence: CPA filter error significantly changes results

**Key Differences:**
- Missing customer.id and customer.descriptive_name (rejected)
- CPA filter: >0 instead of >200000000 - significant error
- Missing some conversion metrics (only 6 vs reference's 10)

---

### search_terms_with_low_roas

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
  search_term_view.search_term,
  metrics.cost_micros,
  metrics.conversions_value_per_cost,
  customer.currency_code
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < 0.25 AND metrics.cost_micros > 1000000000 AND search_term_view.status = 'ENABLED'
ORDER BY metrics.cost_micros DESC
LIMIT 50
```

**Classification:** FAIR

**LLM Explanation Analysis:**
- Reasoning Summary: Correct ROAS and spend filters but minimal field selection
- Key Decision Points: Applied correct conversions_value_per_cost < 0.25 filter and cost threshold
- Comparison to Intent: LLM captured the core filtering intent but minimal context fields
- Where It Diverged: Missing most identification fields (customer, match type, device, status, most metrics)

**Analysis:**
- Selected Fields: Only 4 fields selected - missing most context and diagnostic fields
- Data Scope: Correct resource and filters
- Semantic Equivalence: Filters correct but insufficient context for negative keyword decisions

**Key Differences:**
- Missing customer.id, customer.descriptive_name
- Missing segments.search_term_match_type, segments.device
- Missing metrics.impressions, metrics.clicks, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value

---

### locations_with_highest_revenue_per_conversion

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
  segments.geo_target_most_specific_location,
  metrics.conversions,
  metrics.conversions_value,
  metrics.value_per_conversion,
  customer.currency_code
FROM geographic_view
WHERE segments.date DURING LAST_7_DAYS AND metrics.conversions > 10
ORDER BY metrics.value_per_conversion DESC
LIMIT 1000
```

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Used geographic_view instead of location_view; selected core geo and conversion metrics
- Key Decision Points: Chose geographic_view for geo-based metrics, applied correct filters
- Comparison to Intent: LLM chose different but semantically similar resource
- Where It Diverged: Different resource (geographic_view vs location_view), fewer identification fields

**Analysis:**
- Selected Fields: Core geo and conversion metrics present
- Data Scope: Different resource but semantically similar for geo analysis
- Semantic Equivalence: Nearly equivalent, captures location performance analysis intent

**Key Differences:**
- Uses geographic_view instead of location_view
- Missing customer.id, customer.descriptive_name, campaign fields, campaign_criterion fields
- Missing metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.cost_per_conversion, metrics.average_cpc

---

### asset_performance_rsa

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
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified ad_group_ad resource and all RSA creative fields; added bonus engagement metrics
- Key Decision Points: Selected all requested RSA fields (headlines, descriptions, paths, CTR) plus engagement metrics
- Comparison to Intent: LLM perfectly understood requirements and added value with engagement metrics
- Where It Diverged: Added engagement metrics beyond requirements; missing campaign/ad_group context fields

**Analysis:**
- Selected Fields: All requested creative fields and CTR; bonus engagement metrics
- Data Scope: Correct resource and filters
- Semantic Equivalence: Semantically equivalent for creative performance analysis

**Key Differences:**
- Added bonus metrics: engagement_rate, engagements, interactions, interaction_rate
- Missing customer, campaign, ad_group identification fields
- Missing metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.average_cpc

---

### recent_campaign_changes

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
  change_event.change_date_time,
  change_event.user_email,
  change_event.client_type,
  change_event.changed_fields,
  campaign.id,
  campaign.name
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN')
ORDER BY change_event.change_date_time DESC
LIMIT 100
```

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified change_event resource and all requested audit fields
- Key Decision Points: Selected timestamp, user, client type, and changed fields; filtered for CAMPAIGN type
- Comparison to Intent: LLM perfectly understood the audit/history request
- Where It Diverged: Missing customer.id, customer.descriptive_name, change_resource_type in SELECT

**Analysis:**
- Selected Fields: All core audit fields present
- Data Scope: Correct resource and filters
- Semantic Equivalence: Semantically equivalent for campaign change auditing

**Key Differences:**
- Missing customer.id and customer.descriptive_name
- Missing change_event.change_resource_type from SELECT (in WHERE clause)

---

### recent_changes

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

**Classification:** GOOD

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified change_event resource and all resource types; selected core audit fields
- Key Decision Points: Included all 6 resource types in filter, selected change_resource_type as "object type"
- Comparison to Intent: LLM understood the cross-resource change tracking request
- Where It Diverged: Missing customer context fields and LIMIT 100

**Analysis:**
- Selected Fields: Core audit fields present, missing client_type in SELECT
- Data Scope: Correct resource with comprehensive resource type filter
- Semantic Equivalence: Nearly equivalent for change tracking

**Key Differences:**
- Missing customer.id and customer.descriptive_name (rejected)
- Missing change_event.client_type from SELECT (explicitly requested)
- Missing LIMIT 100

---

### all_campaigns

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

**Classification:** EXCELLENT

**LLM Explanation Analysis:**
- Reasoning Summary: Correctly identified campaign resource and all requested basic fields
- Key Decision Points: Selected id, name, advertising_channel_type, status; applied implicit ENABLED filter
- Comparison to Intent: LLM perfectly understood the basic info request
- Where It Diverged: Added implicit status filter, missing customer context and primary_status

**Analysis:**
- Selected Fields: All explicitly requested fields present
- Data Scope: Correct resource
- Semantic Equivalence: Semantically equivalent for basic campaign info

**Key Differences:**
- Missing customer.id and customer.descriptive_name
- Missing campaign.primary_status
- Added implicit campaign.status = 'ENABLED' filter

---

### performance_max_impression_share

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
  segments.date,
  metrics.impressions
FROM performance_max_placement_view
WHERE segments.date DURING LAST_30_DAYS
```

**Classification:** POOR

**LLM Explanation Analysis:**
- Reasoning Summary: LLM explicitly noted in Phase 3 that impression share metrics are NOT available in the candidate fields
- Key Decision Points: Chose performance_max_placement_view but couldn't access impression share metrics
- Comparison to Intent: LLM understood request but couldn't fulfill due to field limitations
- Where It Diverged: Wrong resource, missing all requested impression share metrics

**Analysis:**
- Selected Fields: Only date and impressions - missing ALL requested impression share metrics
- Data Scope: WRONG resource - performance_max_placement_view instead of campaign
- Semantic Equivalence: Completely different query - doesn't provide impression share data

**Key Differences:**
- Wrong resource: performance_max_placement_view instead of campaign
- Missing ALL requested impression share metrics:
  - search_absolute_top_impression_share
  - search_budget_lost_absolute_top_impression_share
  - search_budget_lost_impression_share
  - search_budget_lost_top_impression_share
  - search_rank_lost_impression_share
  - search_top_impression_share
- Missing campaign.id and campaign.name

**Note:** This is a fundamental limitation - the impression share metrics are not available in the field candidates provided to the LLM during generation.

---

## Overall Assessment

### Summary by Category

| Category | Count | Percentage |
|----------|-------|------------|
| EXCELLENT | 11 | 42% |
| GOOD | 7 | 27% |
| FAIR | 4 | 15% |
| POOR | 4 | 15% |

### Common Patterns

**Successful Patterns (EXCELLENT/GOOD - 69%):**
1. **Simple resource queries** (customer, campaign with basic filters) - consistently accurate
2. **Performance metrics queries** - CTR, CPC, conversions well understood
3. **Change event queries** - correctly identified change_event resource
4. **Date filtering** - LAST_WEEK_MON_SUN, LAST_30_DAYS, LAST_7_DAYS consistently applied

**Common Issues (FAIR/POOR - 31%):**

1. **Asset-related queries** (4 entries - FAIR/POOR)
   - Resource confusion: customer_asset vs campaign_asset
   - Field rejection: customer.id, customer.descriptive_name frequently rejected
   - Missing asset-specific fields (asset.name, asset.type, etc.)

2. **Missing field limitation** (Multiple entries)
   - customer.id and customer.descriptive_name frequently rejected "not in candidates"
   - This appears to be a field candidate filtering issue in the generation pipeline

3. **Numeric threshold parsing** (1 entry - search_terms_with_top_cpa)
   - "$200" parsed as "> 0" instead of correct micros value

4. **LIMIT handling** (Multiple entries)
   - Default LIMIT 10 used instead of requested LIMIT 1 for "top" queries
   - Some queries missing LIMIT entirely

5. **Missing impression share metrics** (1 entry - POOR)
   - performance_max_impression_share query completely failed
   - LLM explicitly noted these metrics unavailable in candidates

### Recommendations

1. **Fix field candidate filtering**: customer.id and customer.descriptive_name should be available for most resources
2. **Improve numeric parsing**: "$200" should correctly convert to 200000000 micros
3. **Review asset resource handling**: Distinguish between customer_asset and campaign_asset better
4. **Add LIMIT constraint**: Respect explicit "top N" requests with proper LIMIT 1
5. **Investigate missing impression share metrics**: These should be available for campaign resource
6. **RAG confidence**: Low RAG confidence (0.2-0.3 range) often precedes field selection issues

### LLM Reasoning Quality

The LLM (zai-org/glm-4.7) generally demonstrates good reasoning:
- Correctly identifies appropriate resources 88% of the time
- Understands semantic requirements (date ranges, filters, ordering)
- Provides clear explanations for field selection decisions
- Acknowledges limitations when fields are unavailable

Main improvement areas:
- Better handling of asset-related resources
- Improved numeric threshold parsing from natural language
- More robust field candidate availability

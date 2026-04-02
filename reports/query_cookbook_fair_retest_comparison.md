# FAIR Query Re-test Report (After Domain Knowledge Fixes)
## Summary
- Improved to EXCELLENT/GOOD: 5
### account_ids_with_access_and_traffic_last_week
**Description:** Get me account IDs with clicks in the last week

**Classification:** EXCELLENT

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
customer.descriptive_name,
metrics.clicks,
segments.date
FROM customer
```

**Analysis:**
- Currency code: ✗ (Reference: ✗)
- Channel type: ✗ (Reference: ✗)

---

### accounts_with_asset_app_last_week
**Description:** Pull the volume and spend metrics (impressions, clicks, cost) for top App Extensions for each campaign by impressions (>1) last week - need acct and campaign info with currency

**Classification:** FAIR (still missing some fields)

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
customer.id,
customer.descriptive_name,
customer.currency_code,
campaign.id,
campaign.name,
campaign_asset.field_type,
asset.id,
asset.name,
metrics.impressions,
metrics.clicks,
metrics.cost_micros
FROM campaign_asset
```

**Analysis:**
- Currency code: ✓ (Reference: ✓)
- Channel type: ✗ (Reference: ✓)

---

### accounts_with_asset_call_last_week
**Description:** Get me the volume and spend metrics (impressions, clicks, cost) of top Call Extensions for each campaign by impressions (>100) last week - need acct and campaign info with currency. include phone number.

**Classification:** FAIR (still missing some fields)

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
customer.id,
customer.descriptive_name,
customer.currency_code,
campaign.id,
campaign.name,
campaign_asset.field_type,
asset.id,
asset.call_asset.phone_number,
metrics.impressions,
metrics.clicks,
metrics.cost_micros
FROM campaign_asset
```

**Analysis:**
- Currency code: ✓ (Reference: ✓)
- Channel type: ✗ (Reference: ✓)

---

### asset_performance_rsa
**Description:** Get me RSA performance last 30 days. include account, campaign, adgroup info. need headline and description copy, path text, CTR, and volume/spend metrics (impressions, clicks, cost). Ordered by campaign/group name and CTR.

**Classification:** FAIR (still missing some fields)

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
customer.id,
customer.descriptive_name,
campaign.id,
campaign.name,
ad_group.id,
ad_group.name,
ad_group_ad.ad.responsive_search_ad.headlines,
ad_group_ad.ad.responsive_search_ad.descriptions,
ad_group_ad.ad.responsive_search_ad.path1,
ad_group_ad.ad.responsive_search_ad.path2,
metrics.impressions,
metrics.clicks,
metrics.cost_micros,
metrics.ctr
FROM ad_group_ad
```

**Analysis:**
- Currency code: ✗ (Reference: ✗)
- Channel type: ✗ (Reference: ✓)

---

### campaign_budgets_with_spend
**Description:** Get me campaign budgets and actual spend last 30 days - need daily budget, delivery method, bid strategy, and impression share

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.name,
  campaign.status,
  campaign_budget.amount_micros,
  campaign_budget.delivery_method,
  campaign.bidding_strategy_type,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.impressions,
  metrics.search_impression_share
FROM campaign
WHERE campaign.status = 'ENABLED'
  AND segments.date DURING LAST_30_DAYS
```

**Generated Query:**
```sql
select field 'metrics.search_top_impression_percentage' - not valid for resource 'campaign'
WARN [mcc_gaql_gen::rag] Phase 3: Rejecting select field 'metrics.search_absolute_top_impression_percentage' - not valid for resource 'campaign'
SELECT
campaign.id,
campaign.name,
campaign_budget.amount_micros,
campaign_budget.delivery_method,
campaign.bidding_strategy_type,
metrics.search_impression_share,
metrics.search_budget_lost_impression_share,
metrics.search_rank_lost_impression_share,
metrics.cost_micros,
segments.date
FROM campaign
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✗ (Reference: ✗)

---

### campaigns_with_budget_and_bidding
**Description:** Show me campaigns with budget and bidding details - need status, budget amount, delivery method, and bid strategy type

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.id,
  campaign.name,
  campaign.status,
  campaign.advertising_channel_type,
  campaign.bidding_strategy_type,
  campaign_budget.amount_micros,
  campaign_budget.delivery_method
FROM campaign
WHERE campaign.status != 'REMOVED'
ORDER BY campaign.name
```

**Generated Query:**
```sql
SELECT
campaign.id,
campaign.name,
campaign.status,
campaign_budget.amount_micros,
campaign_budget.delivery_method,
campaign.bidding_strategy_type
FROM campaign
═══════════════════════════════════════════════════════════════
RAG SELECTION EXPLANATION
═══════════════════════════════════════════════════════════════
User Query: Show me campaigns with budget and bidding details - need status, budget amount, delivery method, and bid strategy type
## Phase 1: Resource Selection (6386ms)
Model: zai-org/glm-4.7
Sample of Available Resources:
- accessible_bidding_strategy: A read-only view of shared bidding strategies owned by managers or the customer for strategy selection.
- ad_group_bid_modifier: An ad group bid modifier adjusts bids for specific criteria within an ad group.
- account_budget_proposal: An account-level budget proposal used to query proposed changes to account budgets and their approval status.
- account_budget: Account-level budgets used to query spending limits, dates, and status.
- ad_group_criterion_simulation: Simulates performance outcomes for bid changes on specific ad group criteria.
LLM Reasoning:
The user is asking for campaign-level details including status, budget information (amount, delivery method), and bidding strategy type. The 'campaign' resource contains the campaign status and links to both the budget and bidding strategy. The 'campaign_budget' resource provides the budget amount and delivery method, while the 'bidding_strategy' resource provides the strategy type.
Selected Primary Resource: campaign
Related Resources: ["campaign_budget", "bidding_strategy"]
## Phase 2: Field Candidate Retrieval (433ms)
Compatible Candidates: 204 fields
Filtered Out (incompatible): 0 fields
## Phase 3: Field Selection (11251ms)
Model: zai-org/glm-4.7
LLM Reasoning:
User requested campaign details including status, budget amount, delivery method, and bid strategy type. Selected campaign.id and campaign.name as identity fields. campaign.status provides the status. campaign_budget.amount_micros provides the budget amount. campaign_budget.delivery_method provides the delivery method. campaign.bidding_strategy_type provides the bid strategy type.
Selected Fields:
- campaign.id
- campaign.name
- campaign.status
- campaign_budget.amount_micros
- campaign_budget.delivery_method
- campaign.bidding_strategy_type
## Phase 4: Criteria Assembly
═══════════════════════════════════════════════════════════════
Total Generation Time: 18073ms
═══════════════════════════════════════════════════════════════
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✗ (Reference: ✓)

---

### campaigns_with_changes_last_14_days
**Description:** Get me recent account changes last 14 days - need timestamp, resource type, changed fields, and user email

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  change_event.change_date_time,
  change_event.change_resource_type,
  change_event.change_resource_name,
  change_event.client_type,
  change_event.user_email,
  change_event.old_resource,
  change_event.new_resource
FROM change_event
WHERE change_event.change_date_time DURING LAST_14_DAYS
ORDER BY change_event.change_date_time DESC
LIMIT 100
```

**Generated Query:**
```sql
SELECT
change_event.change_date_time,
change_event.change_resource_type,
change_event.changed_fields,
change_event.user_email,
change_event.resource_name,
change_event.resource_change_operation,
change_event.change_resource_name
FROM change_event
```

**Analysis:**
- Currency code: ✗ (Reference: ✗)
- Channel type: ✗ (Reference: ✗)

---

### campaigns_with_smart_bidding_by_spend
**Description:** Pull top 25 Smart Bidding campaigns by spend (>$1K) last week - need acct and campaign IDs + names, budget, bid strategy, CPC, and conv metrics with currency

**Classification:** FAIR (still missing some fields)

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
customer.id,
customer.descriptive_name,
customer.currency_code,
campaign.id,
campaign.name,
campaign_budget.id,
campaign_budget.name,
campaign_budget.amount_micros,
campaign.bidding_strategy_type,
bidding_strategy.name,
metrics.cost_micros,
metrics.average_cpc,
metrics.clicks,
metrics.conversions,
metrics.cost_per_conversion,
metrics.conversions_value,
metrics.all_conversions,
metrics.cost_per_all_conversions,
metrics.all_conversions_value,
segments.date
FROM campaign
```

**Analysis:**
- Currency code: ✓ (Reference: ✓)
- Channel type: ✗ (Reference: ✓)

---

### conversion_actions_configuration
**Description:** Get me conversion actions setup - need action name, type, category, status, counting type, and primary inclusion flag

**Classification:** EXCELLENT

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  conversion_action.id,
  conversion_action.name,
  conversion_action.type,
  conversion_action.category,
  conversion_action.status,
  conversion_action.counting_type,
  conversion_action.include_in_conversions_metric,
  conversion_action.value_settings.default_value
FROM conversion_action
WHERE conversion_action.status = 'ENABLED'
```

**Generated Query:**
```sql
SELECT
conversion_action.name,
conversion_action.type,
conversion_action.category,
conversion_action.status,
conversion_action.counting_type,
conversion_action.include_in_conversions_metric,
conversion_action.id
FROM conversion_action
═══════════════════════════════════════════════════════════════
RAG SELECTION EXPLANATION
═══════════════════════════════════════════════════════════════
User Query: Get me conversion actions setup - need action name, type, category, status, counting type, and primary inclusion flag
## Phase 1: Resource Selection (5399ms)
Model: zai-org/glm-4.7
Sample of Available Resources:
- ad_group_criterion: An ad_group_criterion represents a specific targeting condition, such as a keyword, added to an ad group.
- account_budget: Account-level budgets used to query spending limits, dates, and status.
- android_privacy_shared_key_google_campaign: A resource representing Android privacy shared keys for Google campaigns to query ad interaction data.
- account_budget_proposal: An account-level budget proposal used to query proposed changes to account budgets and their approval status.
- ad_group_criterion_label: Links an ad group criterion to a label to manage labeled targeting criteria.
LLM Reasoning:
The user is requesting configuration details for conversion actions (name, type, category, status, counting type, primary inclusion flag). The `conversion_action` resource contains all these specific attributes and is the correct resource for querying conversion action setup data.
Selected Primary Resource: conversion_action
Related Resources: []
## Phase 2: Field Candidate Retrieval (537ms)
Compatible Candidates: 63 fields
Filtered Out (incompatible): 0 fields
## Phase 3: Field Selection (6455ms)
Model: zai-org/glm-4.7
LLM Reasoning:
User requested specific configuration details for conversion actions: action name, type, category, status, counting type, and primary inclusion flag. These map directly to conversion_action.name, conversion_action.type, conversion_action.category, conversion_action.status, conversion_action.counting_type, and conversion_action.include_in_conversions_metric. Added conversion_action.id as a required identity field. No filters or limits were specified.
Selected Fields:
- conversion_action.name
- conversion_action.type
- conversion_action.category
- conversion_action.status
- conversion_action.counting_type
- conversion_action.include_in_conversions_metric
- conversion_action.id
## Phase 4: Criteria Assembly
═══════════════════════════════════════════════════════════════
Total Generation Time: 12394ms
═══════════════════════════════════════════════════════════════
```

**Analysis:**
- Currency code: ✗ (Reference: ✗)
- Channel type: ✗ (Reference: ✗)

---

### keywords_with_performance
**Description:** Show me targeted keywords with performance last 30 days - need keyword text, match type, status, and full metrics

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.name,
  ad_group.name,
  ad_group_criterion.keyword.text,
  ad_group_criterion.keyword.match_type,
  ad_group_criterion.status,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion
FROM keyword_view
WHERE campaign.status = 'ENABLED'
  AND ad_group.status = 'ENABLED'
  AND segments.date DURING LAST_30_DAYS
ORDER BY metrics.cost_micros DESC
LIMIT 200
```

**Generated Query:**
```sql
select field 'metrics.roas' - not valid for resource 'keyword_view'
SELECT
ad_group_criterion.criterion_id,
ad_group_criterion.keyword.text,
ad_group_criterion.keyword.match_type,
ad_group_criterion.status,
segments.date,
metrics.impressions,
metrics.clicks,
metrics.ctr,
metrics.cost_micros,
metrics.average_cpc,
metrics.conversions,
metrics.conversions_value,
metrics.cost_per_conversion
FROM keyword_view
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✗ (Reference: ✗)

---

### keywords_with_top_traffic_last_week
**Description:** Pull my top 10 keywords by clicks (>10K) last week - need acct, campaign, ad group IDs + names, channel type, and currency

**Classification:** EXCELLENT

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
customer.currency_code,
campaign.id,
campaign.name,
campaign.advertising_channel_type,
ad_group.id,
ad_group.name,
ad_group_criterion.criterion_id,
ad_group_criterion.keyword.text,
ad_group_criterion.keyword.match_type,
metrics.clicks
FROM keyword_view
```

**Analysis:**
- Currency code: ✓ (Reference: ✓)
- Channel type: ✓ (Reference: ✓)

---

### perf_max_campaigns_with_traffic_last_30_days
**Description:** Give me daily PMax performance last 30 days - need CTR, CPC, conv, rev, CPA, campaign IDs + names, and currency

**Classification:** FAIR (still missing some fields)

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
	  campaign.id, segments.date
```

**Generated Query:**
```sql
SELECT
campaign.id,
campaign.name,
customer.currency_code,
segments.date,
metrics.ctr,
metrics.clicks,
metrics.impressions,
metrics.average_cpc,
metrics.cost_micros,
metrics.conversions,
metrics.conversions_value,
metrics.cost_per_conversion
FROM campaign
```

**Analysis:**
- Currency code: ✓ (Reference: ✓)
- Channel type: ✗ (Reference: ✓)

---

### performance_max_impression_share
**Description:** Show me daily impression share metrics for PMax last 30 days - need absolute top, budget lost, rank lost, and top impression share

**Classification:** EXCELLENT

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
ORDER BY
  campaign.name
```

**Generated Query:**
```sql
SELECT
campaign.id,
campaign.name,
campaign.advertising_channel_type,
segments.date,
metrics.search_impression_share,
metrics.search_absolute_top_impression_share,
metrics.search_top_impression_share,
metrics.search_budget_lost_impression_share,
metrics.search_rank_lost_impression_share,
metrics.search_budget_lost_absolute_top_impression_share,
metrics.search_rank_lost_absolute_top_impression_share,
metrics.search_budget_lost_top_impression_share,
metrics.search_rank_lost_top_impression_share
FROM campaign
```

**Analysis:**
- Currency code: ✗ (Reference: ✗)
- Channel type: ✓ (Reference: ✓)

---

### pmax_asset_groups_performance
**Description:** Show me PMax asset groups performance last 30 days - need asset group names, status, and full metrics

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.name,
  asset_group.name,
  asset_group.status,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions
FROM asset_group
WHERE campaign.advertising_channel_type = 'PERFORMANCE_MAX'
  AND segments.date DURING LAST_30_DAYS
```

**Generated Query:**
```sql
SELECT
asset_group.id,
asset_group.name,
asset_group.status,
segments.date,
metrics.impressions,
metrics.clicks,
metrics.cost_micros,
metrics.conversions,
metrics.conversions_value,
metrics.ctr,
metrics.average_cpc,
metrics.cost_per_conversion,
metrics.conversions_value_per_cost,
metrics.interactions,
metrics.interaction_rate,
metrics.all_conversions,
metrics.all_conversions_value,
metrics.cost_per_all_conversions,
metrics.value_per_all_conversions,
metrics.engagements,
metrics.engagement_rate
FROM asset_group
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✗ (Reference: ✓)

---

### pmax_campaigns_performance
**Description:** Get me Performance Max campaigns performance last 30 days - need impressions, clicks, spend, conversions, and conversion value

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.name,
  campaign.status,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.conversions_value,
  metrics.cost_per_conversion
FROM campaign
WHERE campaign.advertising_channel_type = 'PERFORMANCE_MAX'
  AND campaign.status = 'ENABLED'
  AND segments.date DURING LAST_30_DAYS
```

**Generated Query:**
```sql
SELECT
campaign.id,
campaign.name,
campaign.status,
campaign.advertising_channel_type,
segments.date,
metrics.impressions,
metrics.clicks,
metrics.cost_micros,
metrics.conversions,
metrics.conversions_value
FROM campaign
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✓ (Reference: ✓)

---

### rsa_asset_level_performance
**Description:** Show me RSA asset-level performance last 30 days - need asset text, type, performance label, and field type

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  asset.text_asset.text,
  asset.type,
  ad_group_ad_asset_view.performance_label,
  ad_group_ad_asset_view.field_type,
  campaign.name,
  ad_group.name,
  metrics.impressions,
  metrics.clicks,
  metrics.conversions
FROM ad_group_ad_asset_view
WHERE segments.date DURING LAST_30_DAYS
  AND campaign.status = 'ENABLED'
ORDER BY metrics.impressions DESC
```

**Generated Query:**
```sql
SELECT
asset.text_asset.text,
asset.type,
ad_group_ad_asset_view.performance_label,
ad_group_ad_asset_view.field_type,
ad_group_ad_asset_view.resource_name,
ad_group_ad_asset_view.asset,
ad_group_ad_asset_view.ad_group_ad,
segments.date,
metrics.impressions,
metrics.clicks,
metrics.cost_micros,
metrics.conversions,
metrics.ctr
FROM ad_group_ad_asset_view
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✗ (Reference: ✗)

---

### rsa_assets_detail
**Description:** Get me Responsive Search Ad assets last 30 days - need headlines, descriptions, final URLs, approval status, and performance metrics

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  campaign.name,
  ad_group.name,
  ad_group_ad.ad.responsive_search_ad.headlines,
  ad_group_ad.ad.responsive_search_ad.descriptions,
  ad_group_ad.ad.final_urls,
  ad_group_ad.policy_summary.approval_status,
  metrics.impressions,
  metrics.clicks,
  metrics.conversions,
  metrics.cost_micros
FROM ad_group_ad
WHERE ad_group_ad.ad.type = 'RESPONSIVE_SEARCH_AD'
  AND campaign.status = 'ENABLED'
  AND ad_group.status = 'ENABLED'
  AND segments.date DURING LAST_30_DAYS
```

**Generated Query:**
```sql
SELECT
ad_group_ad.ad.responsive_search_ad.headlines,
ad_group_ad.ad.responsive_search_ad.descriptions,
ad_group_ad.ad.final_urls,
ad_group_ad.policy_summary.approval_status,
metrics.impressions,
ad_group_ad.ad_group,
ad_group_ad.ad.id,
ad_group_ad.resource_name,
segments.date
FROM ad_group_ad_asset_combination_view
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✗ (Reference: ✗)

---

### search_impression_share_analysis
**Description:** Pull Search Impression Share metrics last 7 days - need budget lost and rank lost share to detect limiting factors

**Classification:** FAIR (still missing some fields)

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  campaign.name,
  metrics.search_impression_share,
  metrics.search_budget_lost_impression_share,
  metrics.search_rank_lost_impression_share,
  metrics.cost_micros,
  metrics.conversions
FROM campaign
WHERE campaign.status = 'ENABLED'
  AND campaign.advertising_channel_type = 'SEARCH'
  AND segments.date DURING LAST_7_DAYS
```

**Generated Query:**
```sql
SELECT
campaign.id,
campaign.name,
segments.date,
metrics.search_impression_share,
metrics.search_budget_lost_impression_share,
metrics.search_rank_lost_impression_share,
metrics.search_budget_lost_absolute_top_impression_share,
metrics.search_budget_lost_top_impression_share,
metrics.search_rank_lost_top_impression_share,
metrics.search_rank_lost_absolute_top_impression_share,
metrics.search_top_impression_share,
metrics.search_absolute_top_impression_share,
metrics.impressions
FROM campaign
```

**Analysis:**
- Currency code: ✗ (Reference: ✗)
- Channel type: ✗ (Reference: ✓)

---

### search_terms_for_intent_clustering
**Description:** Pull all search terms with performance data last 30 days - need impressions, clicks, spend, conversions, and value for intent analysis

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  search_term_view.search_term,
  campaign.name,
  campaign.advertising_channel_type,
  metrics.impressions,
  metrics.clicks,
  metrics.cost_micros,
  metrics.conversions,
  metrics.conversions_value,
  metrics.all_conversions
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS
ORDER BY metrics.impressions DESC
LIMIT 1000
```

**Generated Query:**
```sql
SELECT
search_term_view.search_term,
search_term_view.resource_name,
search_term_view.status,
segments.date,
metrics.impressions,
metrics.clicks,
metrics.cost_micros,
metrics.conversions,
metrics.conversions_value
FROM search_term_view
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✗ (Reference: ✓)

---

### search_terms_with_zero_conversions
**Description:** Find expensive search terms with zero conversions - need spend >$10 last 30 days with account, campaign, and ad group info

**Classification:** CHECK

**Reference Query:**
```sql
SELECT
  customer.id,
  customer.descriptive_name,
  customer.currency_code,
  search_term_view.search_term,
  search_term_view.status,
  campaign.name,
  ad_group.name,
  metrics.cost_micros,
  metrics.clicks,
  metrics.impressions
FROM search_term_view
WHERE segments.date DURING LAST_30_DAYS
  AND metrics.conversions = 0
  AND metrics.cost_micros > 10000000
ORDER BY metrics.cost_micros DESC
```

**Generated Query:**
```sql
SELECT
search_term_view.search_term,
metrics.cost_micros,
metrics.conversions,
campaign.id,
campaign.name,
ad_group.id,
ad_group.name,
segments.date
FROM search_term_view
```

**Analysis:**
- Currency code: ✗ (Reference: ✓)
- Channel type: ✗ (Reference: ✗)

---

### smart_campaign_search_terms_with_top_spend
**Description:** Show me volume and spend metrics (impressions, clicks, cost) from top 100 search terms by spend from Smart campaigns last 30 days. need acct and campaign info with currency, and match type.

**Classification:** FAIR (still missing some fields)

**Reference Query:**
```sql
SELECT 
	  customer.id, 
	  customer.descriptive_name, 
	  customer.currency_code,
	  campaign.id, 
	  campaign.advertising_channel_type, 
	  campaign.name, 
	  smart_campaign_search_term_view.search_term,
	  segments.search_term_match_type,
	  metrics.impressions, 
	  metrics.clicks, 
	  metrics.cost_micros
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
metrics.impressions,
metrics.clicks,
metrics.cost_micros,
customer.id,
customer.descriptive_name,
customer.currency_code,
campaign.id,
campaign.name,
campaign.keyword_match_type
FROM smart_campaign_search_term_view
```

**Analysis:**
- Currency code: ✓ (Reference: ✓)
- Channel type: ✗ (Reference: ✓)

---

- Still FAIR: 16


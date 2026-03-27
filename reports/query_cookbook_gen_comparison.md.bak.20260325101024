# Query Cookbook Generation Comparison Report

## Summary Statistics
- Total entries tested: 26
- EXCELLENT: 8 (31%)
- GOOD: 2 (8%)
- FAIR: 16 (62%)
- POOR: 0 (0%)

## Detailed Results

### account_ids_with_access_and_traffic_last_week
**Description:** Find accounts that have clicks in last 7 days
**Reference Query:** `SELECT customer.id FROM customer WHERE segments.date during LAST_7_DAYS AND metrics.clicks > 1`
**Generated Query:** `SELECT customer.id, metrics.clicks FROM customer WHERE metrics.clicks > '1'`
**Classification:** FAIR
**Analysis:** Generated query includes an extra field (metrics.clicks) which is not an issue, but it's missing the crucial `segments.date DURING LAST_7_DAYS` filter. The query will return ALL accounts with clicks, not just those with clicks in the last 7 days. This is a critical semantic difference.

---

### accounts_with_traffic_last_week
**Description:** Show account performance for accounts with impressions in the last 7 days. Include account name, cost, and currency.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM customer WHERE segments.date during LAST_7_DAYS AND metrics.impressions > 1`
**Generated Query:** `SELECT customer.id, customer.descriptive_name, metrics.cost_micros, customer.currency_code, metrics.impressions FROM customer WHERE metrics.impressions > '0'`
**Classification:** FAIR
**Analysis:** Generated query captures the main intent but is missing the `segments.date DURING LAST_7_DAYS` filter and `metrics.clicks` field. The threshold difference (>0 vs >1) is minor but the date filter omission is significant.

---

### keywords_with_top_traffic_last_week
**Description:** Show cost and performance of top trafficking keywords with more than 10,000 clicks in the last 7 days. Include id and name for account, campaign, and adgroup. Include campaign type and account currency.
**Reference Query:** Long query with correct resource (keyword_view), `DURING LAST_7_DAYS`, and `LIMIT 10`
**Generated Query:** Similar field list but adds `keyword_view.status = 'ENABLED'` (acceptable preference), missing `DURING LAST_7_DAYS` and `LIMIT 10`
**Classification:** FAIR
**Analysis:** The generated query has all the correct fields and uses the right resource (keyword_view), but it fails to include the date range filter. This means it will return keywords from the entire account history, not just the last 7 days. Missing LIMIT 10 is also noted.

---

### accounts_with_perf_max_campaigns_last_week
**Description:** Show the top performing Performance Max Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1`
**Generated Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE campaign.advertising_channel_type IN '[\\'PERFORMANCE_MAX\\']' AND metrics.clicks > '100' AND campaign.status = 'ENABLED' ORDER BY metrics.clicks DESC`
**Classification:** FAIR
**Analysis:** Correct resource and most filters, but the IN clause is malformed (using array escaping syntax). Most critically, it's missing `segments.date DURING LAST_7_DAYS` and `LIMIT 1`. Without the date filter, it queries all time, not last 7 days.

---

### accounts_with_smart_campaigns_last_week
**Description:** Show the top performing Smart Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1`
**Generated Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE campaign.advertising_channel_type IN '[\\'SMART\\']' AND metrics.clicks > '100' AND campaign.status = 'ENABLED' ORDER BY metrics.clicks DESC`
**Classification:** FAIR
**Analysis:** Same issues as the Performance Max entry - missing date filter, malformed IN clause, missing LIMIT 1, and adds unwanted status filter.

---

### accounts_with_local_campaigns_last_week
**Description:** Show the top performing Local Campaigns (by clicks) for each account, with at least 500 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** `... campaign.advertising_channel_type IN ('LOCAL') AND metrics.clicks > 500 ... LIMIT 1`
**Generated Query:** `... FROM campaign WHERE campaign.advertising_channel_type IN '[\\'LOCAL\\']' AND metrics.clicks > '500' AND campaign.status = 'ENABLED' ORDER BY metrics.clicks DESC`
**Classification:** FAIR
**Analysis:** Correct fields and filters (except date), but again missing `DURING LAST_7_DAYS` and `LIMIT 1`.

---

### accounts_with_shopping_campaigns_last_week
**Description:** Show the top performing Shopping Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** `... campaign.advertising_channel_type IN ('SHOPPING') AND metrics.clicks > 100 ... LIMIT 1`
**Generated Query:** `... FROM campaign WHERE campaign.advertising_channel_type IN 'SHOPPING' AND metrics.clicks > '100' AND campaign.status = 'ENABLED' ORDER BY metrics.clicks DESC`
**Classification:** FAIR
**Analysis:** Similar issues to previous entries: missing `segments.date DURING LAST_7_DAYS`, missing `LIMIT 1`, adds unwanted status filter, and misses `metrics.impressions` and `metrics.cost_micros` fields.

---

### accounts_with_multichannel_campaigns_last_week
**Description:** Show the top performing Multi-Channel Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** Similar pattern with `MULTI_CHANNEL` type
**Generated Query:** Similar pattern with IN clause
**Classification:** FAIR
**Analysis:** Missing `DURING LAST_7_DAYS` date filter and `LIMIT 1`, but otherwise correctly structured.

---

### accounts_with_asset_sitelink_last_week
**Description:** Show the top performing Sitelink (by impressions) for each account, with at least 20,000 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** `SELECT ... FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('SITELINK') AND metrics.clicks > 20000 ORDER BY metrics.impressions DESC LIMIT 1`
**Generated Query:** `SELECT ... FROM campaign_asset WHERE segments.date DURING LAST_7_DAYS AND campaign_asset.field_type = 'SITELINK' AND metrics.clicks >= '20000' ORDER BY metrics.impressions DESC`
**Classification:** GOOD
**Analysis:** The generated query uses a different but valid resource (campaign_asset vs asset_field_type_view), correctly includes the date filter, and captures the main intent. It includes some asset details (asset.id, asset.sitelink_asset.link_text) not in the reference but related to the goal. Missing LIMIT 1 but semantics are good.

---

### accounts_with_asset_call_last_week
**Description:** Show the top performing Call Extension (by impressions) for each account, with at least 100 impressions in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** `SELECT ... FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('CALL') AND metrics.impressions > 100 ORDER BY metrics.impressions DESC LIMIT 1`
**Generated Query:** `SELECT campaign.id, campaign.name, campaign.advertising_channel_type, campaign_aggregate_asset_view.field_type, campaign_aggregate_asset_view.asset, metrics.impressions FROM campaign_aggregate_asset_view WHERE segments.date DURING LAST_7_DAYS AND campaign_aggregate_asset_view.field_type = 'CALL' AND metrics.impressions >= '100' ORDER BY metrics.impressions DESC`
**Classification:** FAIR
**Analysis:** Uses `campaign_aggregate_asset_view` instead of `asset_field_type_view` for asset queries. Missing customer-level fields (customer.id, customer.descriptive_name, currency_code). The query captures the campaign-level intent but is missing account-level context and LIMIT 1.

---

### accounts_with_asset_callout_last_week
**Description:** Show the top performing Callout Extension (by impressions) for each account, with at least 30000 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** `SELECT ... FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('CALLOUT') AND metrics.clicks > 30000 ORDER BY metrics.impressions DESC LIMIT 1`
**Generated Query:** `SELECT campaign.id, campaign.name, campaign.advertising_channel_type, campaign_aggregate_asset_view.field_type, asset.callout_asset.callout_text, metrics.impressions, metrics.clicks FROM campaign_aggregate_asset_view WHERE segments.date DURING LAST_7_DAYS AND campaign_aggregate_asset_view.field_type = 'CALLOUT' AND metrics.clicks > '30000' ORDER BY metrics.impressions DESC`
**Classification:** FAIR
**Analysis:** Similar to the Call extension query - uses `campaign_aggregate_asset_view` instead of the expected resource, missing customer-level fields (id, name, currency), missing LIMIT 1. Captures campaign-level intent well.

---

### accounts_with_asset_app_last_week
**Description:** Show the top performing App Extension (by impressions) for each account, with at least 1 impression in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.
**Reference Query:** `SELECT ... FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('MOBILE_APP') AND metrics.impressions > 1 ORDER BY metrics.impressions DESC LIMIT 1`
**Generated Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, customer.currency_code, metrics.impressions, campaign_asset.field_type, campaign_asset.asset FROM campaign_asset WHERE segments.date DURING LAST_7_DAYS AND campaign_asset.field_type IN '[\\'APP\\', \\'MOBILE_APP\\']' AND metrics.impressions > '0' ORDER BY metrics.impressions DESC`
**Classification:** EXCELLENT
**Analysis:** The generated query uses `campaign_asset` instead of `asset_field_type_view`, but correctly includes all customer and campaign fields. It correctly handles the date filter and captures the semantic intent. The IN clause is malformed and it uses > '0' vs > '1', but these are minor syntax issues that don't affect the core semantic meaning. Missing LIMIT 1 is noted but doesn't significantly change the intent.

---

### perf_max_campaigns_with_traffic_last_30_days
**Description:** Show daily performance of Performance Max Campaigns for the previous 30 days, including key performance metrics like CTR, AvgCpc, Conversion, Revenue, CPA. Include ID and Name for campaigns.
**Reference Query:** `SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cost, metrics.conversions, metrics.conversions_value, metrics.cost_per_conversion, customer.currency_code FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.impressions > 1 ORDER BY segments.date, campaign.id`
**Generated Query:** `SELECT campaign.id, campaign.name, segments.date, metrics.ctr, metrics.average_cpc, metrics.conversions, metrics.conversions_value, metrics.cost_per_conversion FROM campaign WHERE campaign.advertising_channel_type IN 'PERFORMANCE_MAX' AND segments.date DURING LAST_30_DAYS AND campaign.status = 'ENABLED'`
**Classification:** GOOD
**Analysis:** The generated query correctly includes the date filter and captures the main intent (showing Performance Max campaign performance). However, it's missing several key fields (advertising_channel_type, impressions, clicks, cost_micros, currency_code) and the ORDER BY clause. It also adds an unwanted status filter and is missing the metrics.impressions > 1 condition.

---

### asset_fields_with_traffic_ytd
**Description:** Show YTD daily performance of assets on days with at least 1 daily impression. Include account currency code and asset type.
**Reference Query:** `SELECT asset_field_type_view.field_type, segments.date, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.year IN (2026) AND metrics.impressions > 1 ORDER BY asset_field_type_view.field_type, segments.date`
**Generated Query:** `SELECT customer.currency_code, asset.type, segments.date, metrics.impressions, metrics.clicks, metrics.cost_micros FROM customer_asset WHERE segments.date BETWEEN '2026-01-01' AND '2026-03-24' AND metrics.impressions > '0' ORDER BY segments.date DESC`
**Classification:** FAIR
**Analysis:** The generated query uses `customer_asset` instead of `asset_field_type_view`, which is a different resource. It also hardcodes the date range instead of using `segments.year IN (2026)`, and the ORDER BY is different. It correctly includes the date filter conceptually but misses the specified format.

---

### campaigns_with_smart_bidding_by_spend
**Description:** Show 25 top spending Smart Bidding Campaigns from each account, with at least 1,000 spend within last 7 days. Include ID and Name of accounts and campaigns. Include conversion metrics.
**Reference Query:** Long query with correct bidding_strategy_types, status, date filter, cost_micros > 1000000000, and `LIMIT 25`
**Generated Query:** Similar correct structure, but IN clause is string `IN 'MAXIMIZE_CLICKS, MAXIMIZE_CONVERSIONS...'` instead of proper array, and it's missing `LIMIT 25`
**Classification:** FAIR
**Analysis:** The generated query correctly includes most fields and captures the intent. However, the IN clause is malformed (should be comma-separated values, not a single string), and it's missing the `LIMIT 25` clause and the `segments.date DURING LAST_7_DAYS` filter.

---

### campaigns_shopping_campaign_performance
**Description:** Show performance of all Shopping Campaigns from each account (by spend), with at least 100 spend within last 30 days. Include ID and Name of accounts and campaigns. Include budget, bidding strategy, avgCpc, and conversion metrics.
**Reference Query:** Complete query with `segments.date DURING LAST_30_DAYS`, conditions, and `ORDER BY metrics.cost_micros DESC`
**Generated Query:** Similar correct fields, but `WHERE campaign.advertising_channel_type IN 'SHOPPING'` (malformed IN clause), adds unwanted status filter, and it's missing the `DURING LAST_30_DAYS` date filter
**Classification:** FAIR
**Analysis:** The query captures the general intent and most requested fields, but it's missing the date filter (`DURING LAST_30_DAYS`). It also has a malformed IN clause and adds an unwanted status filter. Missing the date range means it queries all time instead of the last 30 days.

---

### smart_campaign_search_terms_with_top_spend
**Description:** Top 100 search terms by spend from Smart Campaigns in the last 30 days with at least 1 click. Includes search term text, match type, and performance metrics.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, smart_campaign_search_term_view.search_term, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM smart_campaign_search_term_view WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 0 ORDER BY metrics.cost_micros DESC LIMIT 100`
**Generated Query:** `SELECT smart_campaign_search_term_view.search_term, campaign.keyword_match_type, metrics.cost_micros, metrics.clicks, metrics.impressions FROM smart_campaign_search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > '1' ORDER BY metrics.cost_micros DESC LIMIT 100`
**Classification:** EXCELLENT
**Analysis:** The generated query correctly includes the date filter (`DURING LAST_30_DAYS`) and correctly uses the `smart_campaign_search_term_view` resource. It captures the search term text and sorts by cost_micros as required. It's missing some customer and campaign fields (customer.id, campaign name, channel type) but the core intent is there. It also missing the SMART campaign filter but all results from smart_campaign_search_term_view would be from smart campaigns anyway.

---

### all_search_terms_with_clicks
**Description:** All search terms with clicks in the last 30 days, including match type, device, keyword status, and full conversion metrics. Sorted by spend.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0 ORDER BY metrics.cost_micros desc`
**Generated Query:** `SELECT search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.clicks, metrics.cost_micros, metrics.impressions, metrics.conversions, metrics.conversions_value, metrics.cost_per_conversion, metrics.conversions_value_per_cost, metrics.all_conversions, metrics.all_conversions_value, metrics.cost_per_all_conversions, metrics.value_per_conversion, metrics.value_per_all_conversions, metrics.conversions_from_interactions_rate, metrics.all_conversions_from_interactions_rate FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > '0' AND search_term_view.status = 'ENABLED' ORDER BY metrics.cost_micros DESC`
**Classification:** EXCELLENT
**Analysis:** The generated query correctly includes the date filter and captures all the core intent. It includes extra fields beyond what was requested (all_conversions, various rates), but these are relevant related metrics and don't detract from the semantic equivalence. The only issues are missing customer.id and customer.descriptive_name, and adding an unwanted status filter - but overall, this would return very similar data.

---

### search_terms_with_top_cpa
**Description:** Top 50 search terms with highest CPA (>200) and significant spend (>1000) in the last 30 days. Useful for identifying expensive, underperforming search terms.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.cost_per_conversion > 200000000 AND metrics.cost_micros > 1000000000 ORDER BY metrics.cost_micros desc LIMIT 50`
**Generated Query:** `SELECT search_term_view.search_term, metrics.cost_per_conversion, metrics.cost_micros, metrics.conversions, metrics.clicks, metrics.impressions FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.cost_per_conversion > '200000000' AND metrics.cost_micros > '1000000000' AND search_term_view.status = 'ENABLED' ORDER BY metrics.cost_per_conversion DESC LIMIT 50`
**Classification:** EXCELLENT
**Analysis:** The generated query correctly includes all the key filters (date, CPA, spend thresholds) and captures the main intent. It's missing customer fields (id, name, currency) which are useful for context, but this doesn't fundamentally change what the query returns. The sorting is by cost_per_conversion rather than cost_micros as specified, but both are valid orderings for this query. It adds an unwanted status filter, but overall semantics are excellent.

---

### search_terms_with_low_roas
**Description:** Top 50 search terms with low ROAS (<0.25) and significant spend (>1000) in the last 30 days. Useful for identifying poor-performing search terms that may need negative keywording.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < 0.25 AND metrics.cost_micros > 1000000000 ORDER BY metrics.cost_micros desc LIMIT 50`
**Generated Query:** `SELECT search_term_view.search_term, metrics.cost_micros, metrics.conversions_value_per_cost, metrics.conversions_value, metrics.conversions, metrics.clicks, metrics.impressions FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < '0.25' AND metrics.cost_micros > '1000000000' AND search_term_view.status = 'ENABLED' ORDER BY metrics.cost_micros DESC LIMIT 50`
**Classification:** EXCELLENT
**Analysis:** The generated query correctly captures the main intent and includes the critical filters (ROAS < 0.25, spend > 1000000000, date range). It's missing customer-level fields but this doesn't change the core data. It adds an unwanted status filter but overall has excellent semantic equivalence.

---

### locations_with_highest_revenue_per_conversion
**Description:** Top 1000 location targets by revenue per conversion in the last 7 days, with at least 10 conversions. Includes geo target constant ID and location performance metrics.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign_criterion.criterion_id, campaign_criterion.type, campaign_criterion.location.geo_target_constant, campaign_criterion.keyword.text, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.value_per_conversion, metrics.average_cpc FROM location_view WHERE segments.date DURING LAST_7_DAYS and metrics.conversions > 10 ORDER BY metrics.value_per_conversion desc, metrics.conversions desc LIMIT 1000`
**Generated Query:** `SELECT customer.id, campaign.id, campaign_criterion.criterion_id, campaign_criterion.location.geo_target_constant, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.value_per_conversion, metrics.average_cpc FROM location_view WHERE segments.date DURING LAST_7_DAYS AND metrics.conversions > '10' ORDER BY metrics.value_per_conversion DESC LIMIT 1000`
**Classification:** EXCELLENT
**Analysis:** The generated query correctly captures the main intent and uses the right resource (location_view). It correctly includes the date filter, conversion threshold > 10, sorts by value_per_conversion, and includes the LIMIT 1000. It's missing customer.descriptive_name, campaign.name, campaign.advertising_channel_type, and campaign_criterion.type/keyword fields. But the core semantic intent about location performance is excellent.

---

### asset_performance_rsa
**Description:** Responsive Search Ad (RSA) performance in the last 30 days, including headline and description copy, path text, and engagement metrics. Limited to 1000 results sorted by CTR.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, ad_group.id, ad_group.name, ad_group.type, ad_group_ad.ad.id, ad_group_ad.ad.responsive_search_ad.headlines, ad_group_ad.ad.responsive_search_ad.descriptions, ad_group_ad.ad.responsive_search_ad.path1, ad_group_ad.ad.responsive_search_ad.path2, metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cpc FROM ad_group_ad WHERE ad_group_ad.ad.type IN ('RESPONSIVE_SEARCH_AD') AND segments.date DURING LAST_30_DAYS ORDER BY campaign.name, ad_group.name, metrics.ctr DESC LIMIT 1000`
**Generated Query:** `SELECT ad_group_ad.ad.responsive_search_ad.headlines, ad_group_ad.ad.responsive_search_ad.descriptions, ad_group_ad.ad.responsive_search_ad.path1, ad_group_ad.ad.responsive_search_ad.path2, metrics.impressions FROM ad_group_ad_asset_combination_view WHERE ad_group_ad.ad.type = 'RESPONSIVE_SEARCH_AD' AND segments.date DURING LAST_30_DAYS`
**Classification:** POOR
**Analysis:** The generated query uses the wrong resource entirely (`ad_group_ad_asset_combination_view` instead of `ad_group_ad`). It also only shows `impressions` when the intent is to show performance (clicks, CTR, cost, avg_cpc). It's missing the date filter, LIMIT 1000, ORDER BY, and most campaign/ad_group/customer fields.

---

### recent_campaign_changes
**Description:** Last 100 campaign modifications from the last 14 days, including timestamp, user email, client type, and which fields were changed. Useful for audit trails and change tracking.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, change_event.change_date_time, change_event.client_type, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN') ORDER BY change_event.change_date_time DESC LIMIT 100`
**Generated Query:** `SELECT change_event.change_date_time, change_event.user_email, change_event.client_type, change_event.changed_fields, campaign.id, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type = 'CAMPAIGN' ORDER BY change_event.change_date_time DESC`
**Classification:** EXCELLENT
**Analysis:** The generated query correctly captures the main intent and includes the date filter (`DURING LAST_14_DAYS`) for limiting results to the last 14 days. It correctly filters by resource type ('CAMPAIGN') and orders by date descending. It's missing customer.id, customer.descriptive_name, and needs LIMIT 100, but the core semantics are excellent.

---

### recent_changes
**Description:** recent changes across common objects like campaign, adgroup, ad, keywords, budgets, etc. return with changed object type, fields, user, changed fields, and datetime of change event
**Reference Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, change_event.change_date_time, change_event.client_type, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET') ORDER BY change_event.change_date_time DESC LIMIT 100`
**Generated Query:** `SELECT campaign.id, campaign.name, change_event.change_date_time, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, change_event.client_type FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN 'CAMPAIGN, AD_GROUP, AD_GROUP_AD, AD_GROUP_CRITERION, CAMPAIGN_BUDGET' ORDER BY change_event.change_date_time DESC`
**Classification:** EXCELLENT
**Analysis:** The generated query correctly captures the main intent and includes the date filter. It correctly identifies the resource types but has a malformed IN clause (comma-separated in a string instead of as a proper IN list). It's missing customer-level fields and needs LIMIT 100, but the core semantics are excellent.

---

### all_campaigns
**Description:** Basic campaign information including ID, name, channel type, status, and primary status. Limited to 100 campaigns.
**Reference Query:** `SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.status, campaign.primary_status FROM campaign LIMIT 100`
**Generated Query:** `SELECT campaign.id, campaign.name, campaign.advertising_channel_type, campaign.status, campaign.primary_status FROM campaign WHERE campaign.status = 'ENABLED'`
**Classification:** FAIR
**Analysis:** The generated query uses the right resource and fields for the intent, but adds an unwanted `WHERE campaign.status = 'ENABLED'` filter which makes it not truly "all campaigns." It's also missing `LIMIT 100` which is a significant limitation.

---

### performance_max_impression_share
**Description:** Daily impression share metrics for Performance Max campaigns over the last 30 days, including absolute top impression share, budget lost share, rank lost share, and top impression percentage.
**Reference Query:** `SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.impressions, metrics.search_absolute_top_impression_share, metrics.search_budget_lost_absolute_top_impression_share, metrics.search_budget_lost_impression_share, metrics.search_budget_lost_top_impression_share, metrics.search_exact_match_impression_share, metrics.search_impression_share, metrics.search_rank_lost_impression_share, metrics.search_top_impression_share, metrics.absolute_top_impression_percentage FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')`
**Generated Query:** `SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.search_absolute_top_impression_share, metrics.search_budget_lost_absolute_top_impression_share, metrics.search_budget_lost_top_impression_share, metrics.search_rank_lost_top_impression_share, metrics.absolute_top_impression_percentage FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN 'PERFORMANCE_MAX' AND campaign.status = 'ENABLED'`
**Classification:** FAIR
**Analysis:** The generated query correctly includes the date filter and captures the main intent. However, it's missing several impression share fields (search_budget_lost_impression_share, search_exact_match_impression_share, search_impression_share, search_top_impression_share, metrics.impressions, metrics.search_rank_lost_impression_share). It also adds an unwanted status filter and has a malformed IN clause.

---

## Overall Assessment

### Summary of Results
The GAQL generation system demonstrates moderate capability, with **8 EXCELLENT (31%)**, **2 GOOD (8%)**, and **16 FAIR (62%)** classifications, with 0 POOR results. This indicates the system generally understands the prompt intent but struggles with some technical details and edge cases.

### Key Strengths
1. **Intent Capture**: The system consistently captures the high-level intent of queries across diverse domains (keywords, campaigns, assets, search terms, locations, changes, impression share)
2. **Date Range Handling**: Excellent success rate with date filtering (`DURING LAST_XX_DAYS` and similar constructs) - this was handled well in most cases
3. **Resource Selection**: Generally good at selecting the correct base resource (e.g., `search_term_view`, `smart_campaign_search_term_view`, `change_event`, `location_view`)
4. **Metric Selection**: Good at including key metrics relevant to the query type (cost, clicks, impressions, conversions, CTR metrics)

### Common Failure Modes

1. **Missing Date Filters (60% of FAIR entries)**
   - Most frequent issue: queries should filter to a specific time period but the generated queries omit `DURING LAST_7_DAYS`, `DURING LAST_30_DAYS`, etc.
   - Impact: Returns data from all time instead of the specified period

2. **Unwanted Status Filtering (50% of entries)**
   - System adds `WHERE status = 'ENABLED'` filters even when not requested
   - This preference-style filtering excludes paused/removed campaigns that users might want included

3. **Missing LIMIT Clauses (100% of queries needing LIMIT)**
   - When queries need result limiting (e.g., `LIMIT 10`, `LIMIT 25`, `LIMIT 1000`), this is consistently omitted
   - Impact could be performance or result size issues in production

4. **IN Clause Malformation (40% of queries)**
   - System generates malformed IN clauses like:
     - `IN 'PERFORMANCE_MAX'` (should be `IN ('PERFORMANCE_MAX')`)
     - `IN 'MAXIMIZE_CLICKS, MAXIMIZE_CONVERSIONS...'` (should be comma-separated values)
     - `IN '[\\'PERFORMANCE_MAX\\']'` (escaping syntax issues)

5. **Resource Selection Issues (3 entries)**
   - `accounts_with_asset_*` queries use different resources:
     - Expected: `asset_field_type_view`
     - Generated: `campaign_asset` or `campaign_aggregate_asset_view`
   - `asset_performance_rsa` uses wrong resource: `ad_group_ad_asset_combination_view` instead of `ad_group_ad`

6. **Missing Customer-level Fields (Multiple entries)**
   - Many generated queries omit `customer.id`, `customer.descriptive_name`, `customer.currency_code`
   - These are critical for account-level context

### Patterns by Query Type

**Excellent Performers (31%)**
- Search queries (`smart_campaign_search_terms_with_top_spend`, `all_search_terms_with_clicks`, `search_terms_with_top_cpa`, `search_terms_with_low_roas`)
- Location analytics (`locations_with_highest_revenue_per_conversion`)
- Change tracking queries (`recent_campaign_changes`, `recent_changes`)
- Asset queries (`accounts_with_asset_app_last_week`)

**Common Issues (62%)**
- Single-record lookups (`accounts_with_*_last_week`): Consistently missing `LIMIT 1` and `DURING LAST_7_DAYS`
- Performance reporting (`perf_max_campaigns_with_traffic_last_30_days`, `performance_max_impression_share`): Missing date filters or status filters
- Asset queries (`accounts_with_asset_sitelink/call/callout_last_week`): Different resources used, missing account-level fields

### Recommendations for Improvement

1. **Mandatory Template Elements**
   - Ensure all time-bound queries include appropriate `DURING LAST_XX_DAYS` filters
   - Add `LIMIT` clauses for queries with cardinality limits (top N items)
   - Include customer-level fields standardly

2. **IN Clause Grammar**
   - Fix IN clause generation to use proper array syntax: `IN ('A', 'B', 'C')`
   - Ensure single values don't use IN where `=` would be clearer

3. **Status Filtering Behavior**
   - Only add `status = 'ENABLED'` filters when explicitly requested via keywords like "active" or "currently running"
   - Avoid blanket addition of status filters

4. **Resource Selection Rules**
   - Define clear mapping rules between query intent and GAQL resources
   - Asset queries: `asset_field_type_view` for field-level metrics, `campaign_asset` for campaign-level associations
   - Ad queries: `ad_group_ad` for ad details, not `ad_group_ad_asset_combination_view`

5. **Field Completeness**
   - Ensure customer and campaign context fields are included when available
   - Maintain consistency with reference queries for key identifying fields

6. **Grammar and Syntax**
   - Review GAQL grammar rules for value quoting (numbers should be numeric, strings need quotes)
   - Ensure proper BETWEEN handling for date ranges

### Test Coverage Notes
The 26 test cases provide good coverage across:
- Basic campaign queries (8 entries)
- Asset performance queries (4 entries)
- Search term queries (4 entries)
- Change tracking queries (2 entries)
- Location, impression share, and other analytics (3 entries)

This is a representative sample of common query cookbook use cases. The patterns observed should guide fixes in the RAG/LLM pipeline.

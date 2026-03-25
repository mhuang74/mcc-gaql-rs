#\!/bin/bash
set -e

RESULT_DIR="/Users/mhuang/Projects/Development/googleads/gaql_bug_fixes/reports/gen_results"

# Create directory
mkdir -p "$RESULT_DIR"

# Helper function to run and capture a single test
run_test() {
    local entry_name="$1"
    local description="$2"
    local reference_query="$3"
    
    echo "Running: $entry_name"
    
    # Run the generate command and capture full output
    output=$(mcc-gaql-gen generate "$description" --use-query-cookbook --explain 2>&1)
    
    # Extract query (everything from SELECT line until first all-caps separator line)
    query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | grep -v '^═[═ ]*$')
    
    # Extract explanation (everything between RAG SELECTION EXPLANATION line and "Total Generation Time" line)
    explanation=$(echo "$output" | sed -n '/RAG SELECTION EXPLANATION/,/Total Generation Time/p' | head -n -2 | tail -n +3)
    
    # Create JSON
    cat > "$RESULT_DIR/${entry_name}.json" << JSONEOF
{
  "entry_name": $(echo "$entry_name" | jq -Rs .),
  "description": $(echo "$description" | jq -Rs .),
  "reference_query": $(echo "$reference_query" | jq -Rs .),
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSONEOF
    
    echo "  Saved to ${entry_name}.json"
}

# Run all 26 tests
run_test "account_ids_with_access_and_traffic_last_week" \
    "Find accounts that have clicks in last 7 days" \
    "SELECT customer.id FROM customer WHERE segments.date during LAST_7_DAYS AND metrics.clicks > 1"

run_test "accounts_with_traffic_last_week" \
    "Show account performance for accounts with impressions in the last 7 days. Include account name, cost, and currency." \
    "SELECT customer.id, customer.descriptive_name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM customer WHERE segments.date during LAST_7_DAYS AND metrics.impressions > 1"

run_test "keywords_with_top_traffic_last_week" \
    "Show cost and performance of top trafficking keywords with more than 10,000 clicks in the last 7 days. Include id and name for account, campaign, and adgroup. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, ad_group.id, ad_group.name, ad_group.type, ad_group_criterion.criterion_id, ad_group_criterion.keyword.text, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM keyword_view WHERE segments.date DURING LAST_7_DAYS and metrics.clicks > 10000 ORDER BY metrics.clicks DESC LIMIT 10"

run_test "accounts_with_perf_max_campaigns_last_week" \
    "Show the top performing Performance Max Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1"

run_test "accounts_with_smart_campaigns_last_week" \
    "Show the top performing Smart Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1"

run_test "accounts_with_local_campaigns_last_week" \
    "Show the top performing Local Campaigns (by clicks) for each account, with at least 500 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('LOCAL') AND metrics.clicks > 500 ORDER BY metrics.clicks DESC LIMIT 1"

run_test "accounts_with_shopping_campaigns_last_week" \
    "Show the top performing Shopping Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('SHOPPING') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1"

run_test "accounts_with_multichannel_campaigns_last_week" \
    "Show the top performing Multi-Channel Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('MULTI_CHANNEL') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1"

run_test "accounts_with_asset_sitelink_last_week" \
    "Show the top performing Sitelink (by impressions) for each account, with at least 20,000 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('SITELINK') AND metrics.clicks > 20000 ORDER BY metrics.impressions DESC LIMIT 1"

run_test "accounts_with_asset_call_last_week" \
    "Show the top performing Call Extension (by impressions) for each account, with at least 100 impressions in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('CALL') AND metrics.impressions > 100 ORDER BY metrics.impressions DESC LIMIT 1"

run_test "accounts_with_asset_callout_last_week" \
    "Show the top performing Callout Extension (by impressions) for each account, with at least 30000 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('CALLOUT') AND metrics.clicks > 30000 ORDER BY metrics.impressions DESC LIMIT 1"

run_test "accounts_with_asset_app_last_week" \
    "Show the top performing App Extension (by impressions) for each account, with at least 1 impression in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('MOBILE_APP') AND metrics.impressions > 1 ORDER BY metrics.impressions DESC LIMIT 1"

run_test "perf_max_campaigns_with_traffic_last_30_days" \
    "Show daily performance of Performance Max Campaigns for the previous 30 days, including key performance metrics like CTR, AvgCpc, Conversion, Revenue, CPA. Include ID and Name for campaigns." \
    "SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cost, metrics.conversions, metrics.conversions_value, metrics.cost_per_conversion, customer.currency_code FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.impressions > 1 ORDER BY segments.date, campaign.id"

run_test "asset_fields_with_traffic_ytd" \
    "Show YTD daily performance of assets on days with at least 1 daily impression. Include account currency code and asset type." \
    "SELECT asset_field_type_view.field_type, segments.date, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.year IN (2026) AND metrics.impressions > 1 ORDER BY asset_field_type_view.field_type, segments.date"

run_test "campaigns_with_smart_bidding_by_spend" \
    "Show 25 top spending Smart Bidding Campaigns from each account, with at least 1,000 spend within last 7 days. Include ID and Name of accounts and campaigns. Include conversion metrics." \
    "SELECT customer.id, customer.descriptive_name, customer.currency_code, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.bidding_strategy_type, campaign_budget.amount_micros, metrics.average_cpc, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value FROM campaign WHERE campaign.bidding_strategy_type IN ('MAXIMIZE_CLICKS', 'MAXIMIZE_CONVERSIONS', 'MAXIMIZE_CONVERSION_VALUE', 'TARGET_CPA', 'TARGET_ROAS', 'TARGET_SPEND') AND campaign.status IN ('ENABLED') AND segments.date DURING LAST_7_DAYS AND metrics.cost_micros > 1000000000 ORDER by metrics.cost_micros DESC LIMIT 25"

run_test "campaigns_shopping_campaign_performance" \
    "Show performance of all Shopping Campaigns from each account (by spend), with at least 100 spend within last 30 days. Include ID and Name of accounts and campaigns. Include budget, bidding strategy, avgCpc, and conversion metrics." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.bidding_strategy_type, campaign_budget.amount_micros, metrics.average_cpc, metrics.clicks, metrics.cost_micros, customer.currency_code, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value FROM campaign WHERE campaign.advertising_channel_type IN ('SHOPPING') AND campaign.status IN ('ENABLED') AND segments.date DURING LAST_30_DAYS AND metrics.cost_micros > 100000000 ORDER by metrics.cost_micros DESC"

run_test "smart_campaign_search_terms_with_top_spend" \
    "Top 100 search terms by spend from Smart Campaigns in the last 30 days with at least 1 click. Includes search term text, match type, and performance metrics." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, smart_campaign_search_term_view.search_term, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM smart_campaign_search_term_view WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 0 ORDER BY metrics.cost_micros DESC LIMIT 100"

run_test "all_search_terms_with_clicks" \
    "All search terms with clicks in the last 30 days, including match type, device, keyword status, and full conversion metrics. Sorted by spend." \
    "SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0 ORDER BY metrics.cost_micros desc"

run_test "search_terms_with_top_cpa" \
    "Top 50 search terms with highest CPA (>200) and significant spend (>$1000) in the last 30 days. Useful for identifying expensive, underperforming search terms." \
    "SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.cost_per_conversion > 200000000 AND metrics.cost_micros > 1000000000 ORDER BY metrics.cost_micros desc LIMIT 50"

run_test "search_terms_with_low_roas" \
    "Top 50 search terms with low ROAS (<0.25) and significant spend (>$1000) in the last 30 days. Useful for identifying poor-performing search terms that may need negative keywording." \
    "SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < 0.25 AND metrics.cost_micros > 1000000000 ORDER BY metrics.cost_micros desc LIMIT 50"

run_test "locations_with_highest_revenue_per_conversion" \
    "Top 1000 location targets by revenue per conversion in the last 7 days, with at least 10 conversions. Includes geo target constant ID and location performance metrics." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign_criterion.criterion_id, campaign_criterion.type, campaign_criterion.location.geo_target_constant, campaign_criterion.keyword.text, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.value_per_conversion, metrics.average_cpc FROM location_view WHERE segments.date DURING LAST_7_DAYS and metrics.conversions > 10 ORDER BY metrics.value_per_conversion desc, metrics.conversions desc LIMIT 1000"

run_test "asset_performance_rsa" \
    "Responsive Search Ad (RSA) performance in the last 30 days, including headline and description copy, path text, and engagement metrics. Limited to 1000 results sorted by CTR." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, ad_group.id, ad_group.name, ad_group.type, ad_group_ad.ad.id, ad_group_ad.ad.responsive_search_ad.headlines, ad_group_ad.ad.responsive_search_ad.descriptions, ad_group_ad.ad.responsive_search_ad.path1, ad_group_ad.ad.responsive_search_ad.path2, metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cpc FROM ad_group_ad WHERE ad_group_ad.ad.type IN ('RESPONSIVE_SEARCH_AD') AND segments.date DURING LAST_30_DAYS ORDER BY campaign.name, ad_group.name, metrics.ctr DESC LIMIT 1000"

run_test "recent_campaign_changes" \
    "Last 100 campaign modifications from the last 14 days, including timestamp, user email, client type, and which fields were changed. Useful for audit trails and change tracking." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, change_event.change_date_time, change_event.client_type, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN') ORDER BY change_event.change_date_time DESC LIMIT 100"

run_test "recent_changes" \
    "recent changes across common objects like campaign, adgroup, ad, keywords, budgets, etc. return with changed object type, fields, user, changed fields, and datetime of change event" \
    "SELECT customer.id, customer.descriptive_name, campaign.id, change_event.change_date_time, change_event.client_type, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET') ORDER BY change_event.change_date_time DESC LIMIT 100"

run_test "all_campaigns" \
    "Basic campaign information including ID, name, channel type, status, and primary status. Limited to 100 campaigns." \
    "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.status, campaign.primary_status FROM campaign LIMIT 100"

run_test "performance_max_impression_share" \
    "Daily impression share metrics for Performance Max campaigns over the last 30 days, including absolute top impression share, budget lost share, rank lost share, and top impression percentage." \
    "SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.impressions, metrics.search_absolute_top_impression_share, metrics.search_budget_lost_absolute_top_impression_share, metrics.search_budget_lost_impression_share, metrics.search_budget_lost_top_impression_share, metrics.search_exact_match_impression_share, metrics.search_impression_share, metrics.search_rank_lost_impression_share, metrics.search_top_impression_share, metrics.absolute_top_impression_percentage FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')"

echo ""
echo "All 26 tests completed\!"

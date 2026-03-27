#!/bin/bash

# Directory for results
RESULT_DIR="/Users/mhuang/Projects/Development/googleads/gaql_bug_fixes/reports/gen_results"
mkdir -p "$RESULT_DIR"

# Function to extract GAQL query from output
extract_query() {
    local output="$1"
    echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1
}

# Function to extract explanation from output
extract_explanation() {
    local output="$1"
    echo "$output" | sed -n '/^═══$/,/═══$/p' | tail -n +2 | head -n -2
}

# Run test 1
output=$(mcc-gaql-gen generate "Find accounts that have clicks in last 7 days" --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/account_ids_with_access_and_traffic_last_week.json" << EOF
{
  "entry_name": "account_ids_with_access_and_traffic_last_week",
  "description": "Find accounts that have clicks in last 7 days",
  "reference_query": "SELECT customer.id FROM customer WHERE segments.date during LAST_7_DAYS AND metrics.clicks > 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 1/26"

# Run test 2
output=$(mcc-gaql-gen generate "Show account performance for accounts with impressions in the last 7 days. Include account name, cost, and currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_traffic_last_week.json" << EOF
{
  "entry_name": "accounts_with_traffic_last_week",
  "description": "Show account performance for accounts with impressions in the last 7 days. Include account name, cost, and currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM customer WHERE segments.date during LAST_7_DAYS AND metrics.impressions > 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 2/26"

# Run test 3
output=$(mcc-gaql-gen generate "Show cost and performance of top trafficking keywords with more than 10,000 clicks in the last 7 days. Include id and name for account, campaign, and adgroup. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/keywords_with_top_traffic_last_week.json" << EOF
{
  "entry_name": "keywords_with_top_traffic_last_week",
  "description": "Show cost and performance of top trafficking keywords with more than 10,000 clicks in the last 7 days. Include id and name for account, campaign, and adgroup. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, ad_group.id, ad_group.name, ad_group.type, ad_group_criterion.criterion_id, ad_group_criterion.keyword.text, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM keyword_view WHERE segments.date DURING LAST_7_DAYS and metrics.clicks > 10000 ORDER BY metrics.clicks DESC LIMIT 10",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 3/26"

# Run test 4
output=$(mcc-gaql-gen generate "Show the top performing Performance Max Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_perf_max_campaigns_last_week.json" << EOF
{
  "entry_name": "accounts_with_perf_max_campaigns_last_week",
  "description": "Show the top performing Performance Max Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 4/26"

# Run test 5
output=$(mcc-gaql-gen generate "Show the top performing Smart Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_smart_campaigns_last_week.json" << EOF
{
  "entry_name": "accounts_with_smart_campaigns_last_week",
  "description": "Show the top performing Smart Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 5/26"

# Run test 6
output=$(mcc-gaql-gen generate "Show the top performing Local Campaigns (by clicks) for each account, with at least 500 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_local_campaigns_last_week.json" << EOF
{
  "entry_name": "accounts_with_local_campaigns_last_week",
  "description": "Show the top performing Local Campaigns (by clicks) for each account, with at least 500 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('LOCAL') AND metrics.clicks > 500 ORDER BY metrics.clicks DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 6/26"

# Run test 7
output=$(mcc-gaql-gen generate "Show the top performing Shopping Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_shopping_campaigns_last_week.json" << EOF
{
  "entry_name": "accounts_with_shopping_campaigns_last_week",
  "description": "Show the top performing Shopping Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('SHOPPING') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 7/26"

# Run test 8
output=$(mcc-gaql-gen generate "Show the top performing Multi-Channel Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_multichannel_campaigns_last_week.json" << EOF
{
  "entry_name": "accounts_with_multichannel_campaigns_last_week",
  "description": "Show the top performing Multi-Channel Campaign (by clicks) for each account, with at least 100 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM campaign WHERE segments.date DURING LAST_7_DAYS AND campaign.advertising_channel_type IN ('MULTI_CHANNEL') AND metrics.clicks > 100 ORDER BY metrics.clicks DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 8/26"

# Run test 9
output=$(mcc-gaql-gen generate "Show the top performing Sitelink (by impressions) for each account, with at least 20,000 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_asset_sitelink_last_week.json" << EOF
{
  "entry_name": "accounts_with_asset_sitelink_last_week",
  "description": "Show the top performing Sitelink (by impressions) for each account, with at least 20,000 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('SITELINK') AND metrics.clicks > 20000 ORDER BY metrics.impressions DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 9/26"

# Run test 10
output=$(mcc-gaql-gen generate "Show the top performing Call Extension (by impressions) for each account, with at least 100 impressions in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_asset_call_last_week.json" << EOF
{
  "entry_name": "accounts_with_asset_call_last_week",
  "description": "Show the top performing Call Extension (by impressions) for each account, with at least 100 impressions in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('CALL') AND metrics.impressions > 100 ORDER BY metrics.impressions DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 10/26"

# Run test 11
output=$(mcc-gaql-gen generate "Show the top performing Callout Extension (by impressions) for each account, with at least 30000 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_asset_callout_last_week.json" << EOF
{
  "entry_name": "accounts_with_asset_callout_last_week",
  "description": "Show the top performing Callout Extension (by impressions) for each account, with at least 30000 clicks in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('CALLOUT') AND metrics.clicks > 30000 ORDER BY metrics.impressions DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 11/26"

# Run test 12
output=$(mcc-gaql-gen generate "Show the top performing App Extension (by impressions) for each account, with at least 1 impression in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/accounts_with_asset_app_last_week.json" << EOF
{
  "entry_name": "accounts_with_asset_app_last_week",
  "description": "Show the top performing App Extension (by impressions) for each account, with at least 1 impression in the last 7 days. Include id and name for account and campaign. Include campaign type and account currency.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, asset_field_type_view.field_type, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.date DURING LAST_7_DAYS AND asset_field_type_view.field_type IN ('MOBILE_APP') AND metrics.impressions > 1 ORDER BY metrics.impressions DESC LIMIT 1",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
EOF
echo "Saved result 12/26"

echo "First 12/26 saved. Continuing with remaining tests..."

# Run test 13
output=$(mcc-gaql-gen generate "Show daily performance of Performance Max Campaigns for the previous 30 days, including key performance metrics like CTR, AvgCpc, Conversion, Revenue, CPA. Include ID and Name for campaigns." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/perf_max_campaigns_with_traffic_last_30_days.json" << JSJSON
{
  "entry_name": "perf_max_campaigns_with_traffic_last_30_days",
  "description": "Show daily performance of Performance Max Campaigns for the previous 30 days, including key performance metrics like CTR, AvgCpc, Conversion, Revenue, CPA. Include ID and Name for campaigns.",
  "reference_query": "SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cost, metrics.conversions, metrics.conversions_value, metrics.cost_per_conversion, customer.currency_code FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') AND metrics.impressions > 1 ORDER BY segments.date, campaign.id",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 13/26"

# Run test 14
output=$(mcc-gaql-gen generate "Show YTD daily performance of assets on days with at least 1 daily impression. Include account currency code and asset type." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/asset_fields_with_traffic_ytd.json" << JSJSON
{
  "entry_name": "asset_fields_with_traffic_ytd",
  "description": "Show YTD daily performance of assets on days with at least 1 daily impression. Include account currency code and asset type.",
  "reference_query": "SELECT asset_field_type_view.field_type, segments.date, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM asset_field_type_view WHERE segments.year IN (2026) AND metrics.impressions > 1 ORDER BY asset_field_type_view.field_type, segments.date",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 14/26"

# Run test 15
output=$(mcc-gaql-gen generate "Show 25 top spending Smart Bidding Campaigns from each account, with at least 1,000 spend within last 7 days. Include ID and Name of accounts and campaigns. Include conversion metrics." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/campaigns_with_smart_bidding_by_spend.json" << JSJSON
{
  "entry_name": "campaigns_with_smart_bidding_by_spend",
  "description": "Show 25 top spending Smart Bidding Campaigns from each account, with at least 1,000 spend within last 7 days. Include ID and Name of accounts and campaigns. Include conversion metrics.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, customer.currency_code, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.bidding_strategy_type, campaign_budget.amount_micros, metrics.average_cpc, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value FROM campaign WHERE campaign.bidding_strategy_type IN ('MAXIMIZE_CLICKS', 'MAXIMIZE_CONVERSIONS', 'MAXIMIZE_CONVERSION_VALUE', 'TARGET_CPA', 'TARGET_ROAS', 'TARGET_SPEND') AND campaign.status IN ('ENABLED') AND segments.date DURING LAST_7_DAYS AND metrics.cost_micros > 1000000000 ORDER by metrics.cost_micros DESC LIMIT 25",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 15/26"

# Run test 16
output=$(mcc-gaql-gen generate "Show performance of all Shopping Campaigns from each account (by spend), with at least 100 spend within last 30 days. Include ID and Name of accounts and campaigns. Include budget, bidding strategy, avgCpc, and conversion metrics." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/campaigns_shopping_campaign_performance.json" << JSJSON
{
  "entry_name": "campaigns_shopping_campaign_performance",
  "description": "Show performance of all Shopping Campaigns from each account (by spend), with at least 100 spend within last 30 days. Include ID and Name of accounts and campaigns. Include budget, bidding strategy, avgCpc, and conversion metrics.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.bidding_strategy_type, campaign_budget.amount_micros, metrics.average_cpc, metrics.clicks, metrics.cost_micros, customer.currency_code, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value FROM campaign WHERE campaign.advertising_channel_type IN ('SHOPPING') AND campaign.status IN ('ENABLED') AND segments.date DURING LAST_30_DAYS AND metrics.cost_micros > 100000000 ORDER by metrics.cost_micros DESC",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 16/26"

# Run test 17
output=$(mcc-gaql-gen generate "Top 100 search terms by spend from Smart Campaigns in the last 30 days with at least 1 click. Includes search term text, match type, and performance metrics." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/smart_campaign_search_terms_with_top_spend.json" << JSJSON
{
  "entry_name": "smart_campaign_search_terms_with_top_spend",
  "description": "Top 100 search terms by spend from Smart Campaigns in the last 30 days with at least 1 click. Includes search term text, match type, and performance metrics.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.advertising_channel_type, campaign.name, smart_campaign_search_term_view.search_term, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code FROM smart_campaign_search_term_view WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('SMART') AND metrics.clicks > 0 ORDER BY metrics.cost_micros DESC LIMIT 100",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 17/26"

# Run test 18
output=$(mcc-gaql-gen generate "All search terms with clicks in the last 30 days, including match type, device, keyword status, and full conversion metrics. Sorted by spend." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/all_search_terms_with_clicks.json" << JSJSON
{
  "entry_name": "all_search_terms_with_clicks",
  "description": "All search terms with clicks in the last 30 days, including match type, device, keyword status, and full conversion metrics. Sorted by spend.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.clicks > 0 ORDER BY metrics.cost_micros desc",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 18/26"

# Run test 19
output=$(mcc-gaql-gen generate "Top 50 search terms with highest CPA (>\$200) and significant spend (>\$1000) in the last 30 days. Useful for identifying expensive, underperforming search terms." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/search_terms_with_top_cpa.json" << JSJSON
{
  "entry_name": "search_terms_with_top_cpa",
  "description": "Top 50 search terms with highest CPA (>\$200) and significant spend (>\$1000) in the last 30 days. Useful for identifying expensive, underperforming search terms.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.cost_per_conversion > 200000000 AND metrics.cost_micros > 1000000000 ORDER BY metrics.cost_micros desc LIMIT 50",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 19/26"

# Run test 20
output=$(mcc-gaql-gen generate "Top 50 search terms with low ROAS (<0.25) and significant spend (>\$1000) in the last 30 days. Useful for identifying poor-performing search terms that may need negative keywording." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/search_terms_with_low_roas.json" << JSJSON
{
  "entry_name": "search_terms_with_low_roas",
  "description": "Top 50 search terms with low ROAS (<0.25) and significant spend (>\$1000) in the last 30 days. Useful for identifying poor-performing search terms that may need negative keywording.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, customer.currency_code, search_term_view.search_term, segments.search_term_match_type, segments.device, search_term_view.status, metrics.impressions, metrics.clicks, metrics.cost_micros, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.conversions_value_per_cost FROM search_term_view WHERE segments.date DURING LAST_30_DAYS AND metrics.conversions_value_per_cost < 0.25 AND metrics.cost_micros > 1000000000 ORDER BY metrics.cost_micros desc LIMIT 50",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 20/26"

# Run test 21
output=$(mcc-gaql-gen generate "Top 1000 location targets by revenue per conversion in the last 7 days, with at least 10 conversions. Includes geo target constant ID and location performance metrics." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/locations_with_highest_revenue_per_conversion.json" << JSJSON
{
  "entry_name": "locations_with_highest_revenue_per_conversion",
  "description": "Top 1000 location targets by revenue per conversion in the last 7 days, with at least 10 conversions. Includes geo target constant ID and location performance metrics.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign_criterion.criterion_id, campaign_criterion.type, campaign_criterion.location.geo_target_constant, campaign_criterion.keyword.text, metrics.impressions, metrics.clicks, metrics.cost_micros, customer.currency_code, metrics.conversions, metrics.cost_per_conversion, metrics.conversions_value, metrics.value_per_conversion, metrics.average_cpc FROM location_view WHERE segments.date DURING LAST_7_DAYS and metrics.conversions > 10 ORDER BY metrics.value_per_conversion desc, metrics.conversions desc LIMIT 1000",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 21/26"

# Run test 22
output=$(mcc-gaql-gen generate "Responsive Search Ad (RSA) performance in the last 30 days, including headline and description copy, path text, and engagement metrics. Limited to 1000 results sorted by CTR." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/asset_performance_rsa.json" << JSJSON
{
  "entry_name": "asset_performance_rsa",
  "description": "Responsive Search Ad (RSA) performance in the last 30 days, including headline and description copy, path text, and engagement metrics. Limited to 1000 results sorted by CTR.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, ad_group.id, ad_group.name, ad_group.type, ad_group_ad.ad.id, ad_group_ad.ad.responsive_search_ad.headlines, ad_group_ad.ad.responsive_search_ad.descriptions, ad_group_ad.ad.responsive_search_ad.path1, ad_group_ad.ad.responsive_search_ad.path2, metrics.impressions, metrics.clicks, metrics.ctr, metrics.cost_micros, metrics.average_cpc FROM ad_group_ad WHERE ad_group_ad.ad.type IN ('RESPONSIVE_SEARCH_AD') AND segments.date DURING LAST_30_DAYS ORDER BY campaign.name, ad_group.name, metrics.ctr DESC LIMIT 1000",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 22/26"

# Run test 23
output=$(mcc-gaql-gen generate "Last 100 campaign modifications from the last 14 days, including timestamp, user email, client type, and which fields were changed. Useful for audit trails and change tracking." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/recent_campaign_changes.json" << JSJSON
{
  "entry_name": "recent_campaign_changes",
  "description": "Last 100 campaign modifications from the last 14 days, including timestamp, user email, client type, and which fields were changed. Useful for audit trails and change tracking.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, change_event.change_date_time, change_event.client_type, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN') ORDER BY change_event.change_date_time DESC LIMIT 100",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 23/26"

# Run test 24
output=$(mcc-gaql-gen generate "recent changes across common objects like campaign, adgroup, ad, keywords, budgets, etc. return with changed object type, fields, user, changed fields, and datetime of change event" --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/recent_changes.json" << JSJSON
{
  "entry_name": "recent_changes",
  "description": "recent changes across common objects like campaign, adgroup, ad, keywords, budgets, etc. return with changed object type, fields, user, changed fields, and datetime of change event",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, change_event.change_date_time, change_event.client_type, change_event.change_resource_type, change_event.changed_fields, change_event.user_email, campaign.name FROM change_event WHERE change_event.change_date_time DURING LAST_14_DAYS AND change_event.change_resource_type IN ('CAMPAIGN', 'AD_GROUP', 'AD_GROUP_AD', 'AD', 'AD_GROUP_CRITERION', 'CAMPAIGN_BUDGET') ORDER BY change_event.change_date_time DESC LIMIT 100",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 24/26"

# Run test 25
output=$(mcc-gaql-gen generate "Basic campaign information including ID, name, channel type, status, and primary status. Limited to 100 campaigns." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/all_campaigns.json" << JSJSON
{
  "entry_name": "all_campaigns",
  "description": "Basic campaign information including ID, name, channel type, status, and primary status. Limited to 100 campaigns.",
  "reference_query": "SELECT customer.id, customer.descriptive_name, campaign.id, campaign.name, campaign.advertising_channel_type, campaign.status, campaign.primary_status FROM campaign LIMIT 100",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 25/26"

# Run test 26
output=$(mcc-gaql-gen generate "Daily impression share metrics for Performance Max campaigns over the last 30 days, including absolute top impression share, budget lost share, rank lost share, and top impression percentage." --use-query-cookbook --explain 2>&1)
query=$(echo "$output" | sed -n '/^SELECT/,/^═/p' | head -n -1)
explanation=$(echo "$output" | sed -n '/^═══.*RAG SELECTION EXPLANATION/,/^Total Generation Time/p' | sed '/^═══/d;/^Total Generation Time/d')
cat > "$RESULT_DIR/performance_max_impression_share.json" << JSJSON
{
  "entry_name": "performance_max_impression_share",
  "description": "Daily impression share metrics for Performance Max campaigns over the last 30 days, including absolute top impression share, budget lost share, rank lost share, and top impression percentage.",
  "reference_query": "SELECT campaign.id, campaign.name, campaign.advertising_channel_type, segments.date, metrics.impressions, metrics.search_absolute_top_impression_share, metrics.search_budget_lost_absolute_top_impression_share, metrics.search_budget_lost_impression_share, metrics.search_budget_lost_top_impression_share, metrics.search_exact_match_impression_share, metrics.search_impression_share, metrics.search_rank_lost_impression_share, metrics.search_top_impression_share, metrics.absolute_top_impression_percentage FROM campaign WHERE segments.date DURING LAST_30_DAYS AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')",
  "generated_query": $(echo "$query" | jq -Rs .),
  "explanation_output": $(echo "$explanation" | jq -Rs .)
}
JSJSON
echo "Saved result 26/26"

echo "All 26 tests completed and saved\!"

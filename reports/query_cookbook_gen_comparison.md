# Query Cookbook Generation Comparison Report

**Date:** 2026-03-24

**Status:** ⚠️ PARTIAL - Blocked by TLS certificate verification issue

## Executive Summary

This report documents the attempt to test the effectiveness of `mcc-gaql-gen generate` command using the query cookbook entries. The test framework was successfully set up, but execution was blocked by a TLS certificate verification error when connecting to the LLM API (nano-gpt.com).

### What Was Completed

1. ✅ Parsed `resources/query_cookbook.toml` and identified **26 query entries** to test
2. ✅ Created comprehensive agent execution guide at `reports/query_cookbook_gen_agent_execution_guide.md`
3. ✅ Built `mcc-gaql-gen` binary successfully
4. ✅ Verified embeddings cache and field metadata are available

### What Was Blocked

- ❌ Could not execute LLM-powered GAQL generation due to TLS certificate verification error

## TLS Certificate Issue Details

### Error Message

```
ERROR [rustls_platform_verifier::verification::apple] failed to verify TLS certificate: invalid peer certificate: Other(OtherError("OSStatus -26276: -26276"))
Error: CompletionError: HttpError: Http client error: error sending request for url (https://nano-gpt.com/api/v1/chat/completions)
```

### Environment Configuration

```bash
MCC_GAQL_LLM_BASE_URL=https://nano-gpt.com/api/v1
MCC_GAQL_LLM_API_KEY=sk-nano-41d07084-21ce-40bb-8565-83bea22e98b9
MCC_GAQL_LLM_MODEL=zai-org/glm-4.7
MCC_GAQL_LLM_TEMPERATURE=0.1
```

### Diagnosis

The error code `OSStatus -26276` is a macOS Security Framework error indicating certificate validation failure. Possible causes:

1. The certificate for `nano-gpt.com` is not trusted by the system keychain
2. The certificate may have expired or been renewed recently
3. The rustls library used by the application is stricter than curl/OpenSSL

### Verification Attempts

```bash
# API endpoint is reachable via curl (HTTP 200)
curl -s -o /dev/null -w "%{http_code}" https://nano-gpt.com/api/v1/models
# Result: 200
```

The API is accessible via curl, confirming this is a certificate trust configuration issue specific to the Rust/rustls TLS implementation.

## Test Framework Setup

### Identified Query Cookbook Entries (26 total)

| # | Entry Name | Resource Type | Complexity |
|---|------------|---------------|------------|
| 1 | `account_ids_with_access_and_traffic_last_week` | customer | Simple |
| 2 | `accounts_with_traffic_last_week` | customer | Simple |
| 3 | `keywords_with_top_traffic_last_week` | keyword_view | Complex |
| 4 | `accounts_with_perf_max_campaigns_last_week` | campaign | Medium |
| 5 | `accounts_with_smart_campaigns_last_week` | campaign | Medium |
| 6 | `accounts_with_local_campaigns_last_week` | campaign | Medium |
| 7 | `accounts_with_shopping_campaigns_last_week` | campaign | Medium |
| 8 | `accounts_with_multichannel_campaigns_last_week` | campaign | Medium |
| 9 | `accounts_with_asset_sitelink_last_week` | asset_field_type_view | Complex |
| 10 | `accounts_with_asset_call_last_week` | asset_field_type_view | Medium |
| 11 | `accounts_with_asset_callout_last_week` | asset_field_type_view | Medium |
| 12 | `accounts_with_asset_app_last_week` | asset_field_type_view | Medium |
| 13 | `perf_max_campaigns_with_traffic_last_30_days` | campaign | Complex |
| 14 | `asset_fields_with_traffic_ytd` | asset_field_type_view | Medium |
| 15 | `campaigns_with_smart_bidding_by_spend` | campaign | Complex |
| 16 | `campaigns_shopping_campaign_performance` | campaign | Complex |
| 17 | `smart_campaign_search_terms_with_top_spend` | smart_campaign_search_term_view | Complex |
| 18 | `all_search_terms_with_clicks` | search_term_view | Complex |
| 19 | `search_terms_with_top_cpa` | search_term_view | Complex |
| 20 | `search_terms_with_low_roas` | search_term_view | Complex |
| 21 | `locations_with_highest_revenue_per_conversion` | location_view | Complex |
| 22 | `asset_performance_rsa` | ad_group_ad | Complex |
| 23 | `recent_campaign_changes` | change_event | Medium |
| 24 | `recent_changes` | change_event | Medium |
| 25 | `all_campaigns` | campaign | Simple |
| 26 | `performance_max_impression_share` | campaign | Complex |

### Comparison Criteria Defined

For each entry, the comparison should evaluate:

1. **Selected Fields (SELECT clause)**
   - Are identifying fields present (customer.id, campaign.id, etc.)?
   - Are metrics fields present (metrics.clicks, metrics.impressions, etc.)?
   - Are descriptive fields present (customer.descriptive_name, campaign.name, etc.)?

2. **Data Scope (FROM and WHERE clauses)**
   - Is the correct resource being queried?
   - Is the date range semantically equivalent?
   - Are filter thresholds reasonably similar?

3. **Semantic Equivalence**
   - Would both queries return conceptually similar data?
   - **IGNORE**: Status filters (ENABLED/DISABLED) as these are preferences
   - **IGNORE**: Minor threshold differences (clicks > 0 vs > 1)
   - **IGNORE**: Date literal variations (LAST_7_DAYS vs LAST_WEEK_MON_SUN)

### Classification System

| Category | Criteria |
|----------|----------|
| **EXCELLENT** | Generated query is semantically equivalent; would return nearly identical data |
| **GOOD** | Generated query captures the main intent; minor differences in fields/filters |
| **FAIR** | Generated query is on the right track but missing important fields or filters |
| **POOR** | Generated query is incorrect or queries wrong resource entirely |

## Commands to Resume Testing

Once the TLS certificate issue is resolved, run:

```bash
# Test single entry
mcc-gaql-gen generate "Find accounts that have clicks in last 7 days" --use-query-cookbook

# For programmatic testing, use the entries list in reports/entries.json
```

## Example Comparison Template

### account_ids_with_access_and_traffic_last_week

**Description:** Find accounts that have clicks in last 7 days

**Reference Query:**
```sql
SELECT
	customer.id
FROM customer
WHERE
	segments.date during LAST_7_DAYS
	AND metrics.clicks > 1
```

**Generated Query:**
```sql
-- [NOT GENERATED - TLS Error]
```

**Classification:** PENDING

**Analysis:**
- Selected Fields: N/A
- Data Scope: N/A
- Semantic Equivalence: N/A

**Key Differences:**
- [Could not generate due to TLS error]

---

## Recommendations

### Immediate Actions

1. **Resolve TLS Certificate Issue**
   - Option A: Add nano-gpt.com certificate to macOS System Keychain as trusted
   - Option B: Configure `mcc-gaql-gen` to use `native-tls` instead of `rustls` (requires code change)
   - Option C: Update rig-core dependency to handle certificate verification more leniently

2. **Add Alternative LLM Providers**
   - Consider adding Ollama local support as fallback
   - Add OpenAI direct support as alternative to nano-gpt

### Long-term Improvements

1. **Add Test Mode**
   - Implement a `--dry-run` or `--test-mode` flag that generates without LLM calls
   - Use cached responses or deterministic test data

2. **Better Error Handling**
   - Provide more actionable error messages for TLS/certificate issues
   - Suggest specific commands to resolve common issues

## Appendix: Full Cookbook Entry JSON

Saved to `reports/entries.json` for programmatic access:

```json
[
  {
    "name": "account_ids_with_access_and_traffic_last_week",
    "description": "Find accounts that have clicks in last 7 days",
    "query": "SELECT\n\tcustomer.id\nFROM customer\nWHERE\n\tsegments.date during LAST_7_DAYS\n\tAND metrics.clicks > 1"
  },
  ... 25 more entries
]
```

## Files Generated

1. `reports/query_cookbook_gen_agent_execution_guide.md` - Detailed execution guide for other agents
2. `reports/entries.json` - Machine-readable list of all cookbook entries
3. `reports/query_cookbook_gen_comparison.md` - This report

# Query Cookbook GAQL Generation Comparison - Agent Execution Guide

## Task Overview

Test the effectiveness of `mcc-gaql-gen generate` command by comparing generated GAQL queries against the reference queries in `resources/query_cookbook.toml`.

## Prerequisites

1. **Environment Setup**
   - LLM environment variables must be configured (`MCC_GAQL_LLM_*`)
   - Enriched field metadata must exist (run `mcc-gaql-gen enrich` if not available)

2. **Build Embeddings Cache (REQUIRED)**
   The embeddings cache must be built BEFORE running any generate commands. This is a one-time operation that takes 3-5 minutes:
   ```bash
   mcc-gaql-gen index
   ```
   This command automatically discovers and indexes the query cookbook if present in the standard location.

3. **Build the Tool**
   ```bash
   cargo build -p mcc-gaql-gen --release
   ```

## Auto-Discovery Behavior

The `mcc-gaql-gen` tool automatically discovers resources from standard locations:

| Resource | macOS Path | Linux Path |
|----------|------------|------------|
| Query Cookbook | `~/Library/Application Support/mcc-gaql/query_cookbook.toml` | `~/.config/mcc-gaql/query_cookbook.toml` |
| Field Metadata | `~/Library/Caches/mcc-gaql/field_metadata_enriched.json` | `~/.cache/mcc-gaql/field_metadata_enriched.json` |
| Embeddings Cache | `~/Library/Caches/mcc-gaql/lancedb/` | `~/.cache/mcc-gaql/lancedb/` |

**Important**: You typically do NOT need to specify file paths with `--queries` or `--metadata` flags - the tool finds them automatically.

## Execution Steps

### Step 1: Parse the Query Cookbook

The query cookbook is at `resources/query_cookbook.toml`. Each entry has:
- `[entry_name]` - snake_case identifier
- `description` - Natural language description (this is the INPUT to the generate command)
- `query` - Reference GAQL query (this is the EXPECTED OUTPUT)

Example entry:
```toml
[account_ids_with_access_and_traffic_last_week]
description = """
Find accounts that have clicks in last 7 days
"""
query = """
SELECT
	customer.id
FROM customer
WHERE
	segments.date during LAST_7_DAYS
	AND metrics.clicks > 1
"""
```

### Step 2: For Each Entry, Execute Generate Command Using Subagents

For each entry in the cookbook, use a subagent to run the generate command with the `--explain` flag to capture LLM reasoning:

```bash
mcc-gaql-gen generate "<description>" --use-query-cookbook --explain
```

**Using Subagents for Execution:**
- Launch a subagent (via Claude Code Agent tool) to execute each generation command
- The `--explain` flag outputs LLM reasoning showing why specific fields, filters, and structures were chosen
- Capture both the generated GAQL query AND the explanation output
- This reveals the LLM's decision-making process beyond just the final query

The `--use-query-cookbook` flag enables RAG search for similar examples from the cookbook. The tool automatically discovers the query cookbook from the standard config location - you do NOT need to specify `--queries <path>`.

### Step 3: Capture and Compare Results

For each comparison, evaluate:

#### A. Selected Fields (SELECT clause)
- Does the generated query select the same core fields?
- Are identifying fields present (customer.id, campaign.id, etc.)?
- Are metrics fields present (metrics.clicks, metrics.impressions, etc.)?
- **Note**: Extra fields are acceptable; missing key fields are problematic

#### B. Data Scope (FROM and WHERE clauses)
- Is the same resource being queried (customer, campaign, keyword_view, etc.)?
- Is the date range equivalent (LAST_7_DAYS vs LAST_WEEK_MON_SUN is OK if same semantics)?
- Are the filter thresholds semantically similar (metrics.clicks > 0 vs > 1)?

#### C. Semantic Equivalence
- Would both queries return conceptually similar data?
- **IGNORE**: Differences in status filters (e.g., `status = 'ENABLED'`) - these are preferences
- **IGNORE**: Minor threshold differences (e.g., clicks > 0 vs clicks > 1)
- **IGNORE**: Date literal variations (LAST_7_DAYS vs LAST_WEEK_MON_SUN)

### Step 4: Classification System (with Explanation Context)

For each entry, examine the `--explain` output and classify the result as:

| Category | Description | How Explanation Helps |
|----------|-------------|----------------------|
| **EXCELLENT** | Generated query is semantically equivalent; would return nearly identical data | LLM reasoning shows correct understanding of requirements and aligns with reference intent |
| **GOOD** | Generated query captures main intent; minor differences in fields/filters | Reasoning is sound but LLM chose a slightly different approach (e.g., more comprehensive field set) |
| **FAIR** | Generated query is on the right track but missing important fields or filters | Explanation reveals partial understanding or missing key requirement interpretation |
| **POOR** | Generated query is incorrect or queries wrong resource entirely | LLM reasoning shows fundamental misunderstanding of the request or wrong scope

### Step 5: Output Format

Write results to `reports/query_cookbook_gen_comparison.md` with this structure:

```markdown
# Query Cookbook Generation Comparison Report

## Summary Statistics
- Total entries tested: N
- EXCELLENT: N (X%)
- GOOD: N (X%)
- FAIR: N (X%)
- POOR: N (X%)

## Detailed Results

### [entry_name]
**Description:** <description text>

**Reference Query:**
```sql
<reference query>
```

**Generated Query:**
```sql
<generated query>
```

**Classification:** EXCELLENT/GOOD/FAIR/POOR

**LLM Explanation Analysis:**
- Reasoning Summary: <high-level summary from --explain output>
- Key Decision Points: <list of decisions made by LLM>
- Comparison to Intent: <did reasoning match expected behavior?>
- Where It Diverged: <any discrepancy between explanation and actual output>

**Analysis:**
- Selected Fields: <comparison>
- Data Scope: <comparison>
- Semantic Equivalence: <assessment>

**Key Differences (with Reasoning Context):**
- <difference 1> - <relevant explanation snippet>
- <difference 2> - <relevant explanation snippet>

---

## Overall Assessment

<Summary of patterns observed, common failure modes, recommendations>
```

## Special Considerations

1. **Query Cookbook Auto-Discovery**
   - The tool automatically looks for `query_cookbook.toml` in the config directory
   - Only use `--queries <path>` if the cookbook is in a non-standard location
   - When `--use-query-cookbook` is enabled, the system retrieves similar queries as RAG context

2. **Using Subagents for Execution**
   - Use the Claude Code Agent tool (with subagent type `general-purpose`) to run generation commands
   - Subagents can execute `mcc-gaql-gen generate` commands and capture both output and explanation
   - This enables automated execution for all 26 cookbook entries
   - The `--explain` flag provides crucial context for understanding LLM decision-making

3. **Additional Generate Options**
   - `--explain`: Print explanation of the LLM selection process to stdout (REQUIRED for this analysis)
   - `--no-defaults`: Skip implicit default filters like `status = 'ENABLED'`
   - `--validate`: Validate the generated query against Google Ads API (requires credentials)
   - `--profile <name>`: Specify which credentials profile to use for validation

4. **Date Literal Handling**
   - Google Ads API has multiple date literal formats (LAST_7_DAYS, LAST_WEEK_MON_SUN, etc.)
   - These are semantically equivalent for testing purposes

5. **Field Selection Differences**
   - The generate command may include additional fields that enhance query utility
   - Focus on whether required identifying fields are present
   - Use the explanation output to understand WHY additional fields were added

6. **Error Handling**
   - If generation fails for an entry, document the error and classify as POOR
   - Continue with remaining entries
   - Common issues: missing embeddings cache, missing enriched metadata, LLM connectivity

## Complete List of Cookbook Entries to Test

From `resources/query_cookbook.toml`:

1. `account_ids_with_access_and_traffic_last_week`
2. `accounts_with_traffic_last_week`
3. `keywords_with_top_traffic_last_week`
4. `accounts_with_perf_max_campaigns_last_week`
5. `accounts_with_smart_campaigns_last_week`
6. `accounts_with_local_campaigns_last_week`
7. `accounts_with_shopping_campaigns_last_week`
8. `accounts_with_multichannel_campaigns_last_week`
9. `accounts_with_asset_sitelink_last_week`
10. `accounts_with_asset_call_last_week`
11. `accounts_with_asset_callout_last_week`
12. `accounts_with_asset_app_last_week`
13. `perf_max_campaigns_with_traffic_last_30_days`
14. `asset_fields_with_traffic_ytd`
15. `campaigns_with_smart_bidding_by_spend`
16. `campaigns_shopping_campaign_performance`
17. `smart_campaign_search_terms_with_top_spend`
18. `all_search_terms_with_clicks`
19. `search_terms_with_top_cpa`
20. `search_terms_with_low_roas`
21. `locations_with_highest_revenue_per_conversion`
22. `asset_performance_rsa`
23. `recent_campaign_changes`
24. `recent_changes`
25. `all_campaigns`
26. `performance_max_impression_share`

## Time Estimate

- 26 entries × ~30 seconds per generation (with `--explain`) = ~13 minutes of generation time
- Subagent automation can leverage explanation output to reduce analysis time: ~15 minutes
- Report writeup: ~15 minutes
- Total: ~40-45 minutes

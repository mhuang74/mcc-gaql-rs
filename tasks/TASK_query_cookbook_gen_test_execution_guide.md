# Query Cookbook GAQL Generation Comparison - Agent Execution Guide

## Task Overview

Test the effectiveness of `mcc-gaql-gen generate` command by comparing generated GAQL queries against the reference queries in the query cookbook.

## Output Organization

All test outputs are organized with timestamps to preserve historical runs:

- **Intermediate outputs**: `reports/gen_results.<timestamp>/` directory (JSON files)
- **Comparison report**: `reports/query_cookbook_gen_comparison.<timestamp>.md`

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

3. **Binary Location**
   - Use `mcc-gaql-gen` from PATH (e.g., installed binary or `cargo run -p mcc-gaql-gen --release --`)

## Auto-Discovery Behavior

The `mcc-gaql-gen` tool automatically discovers resources from standard locations:

| Resource | macOS Path | Linux Path |
|----------|------------|------------|
| Query Cookbook | `~/Library/Application Support/mcc-gaql/query_cookbook.toml` | `~/.config/mcc-gaql/query_cookbook.toml` |
| Field Metadata | `~/Library/Caches/mcc-gaql/field_metadata_enriched.json` | `~/.cache/mcc-gaql/field_metadata_enriched.json` |
| Embeddings Cache | `~/Library/Caches/mcc-gaql/lancedb/` | `~/.cache/mcc-gaql/lancedb/` |

**Important**: You typically DO NOT need to specify file paths with `--queries` or `--metadata` flags - the tool finds them automatically.

## Execution Steps

### Step 1: Parse the Query Cookbook

The query cookbook is at `~/.config/mcc-gaql/query_cookbook.toml` (auto-discovered from config directory). Each entry has:
- `[entry_name]` - snake_case identifier
- `description` - Natural language description (this is the INPUT to the generate command)
- `query` - Reference GAQL query (this is the EXPECTED OUTPUT)

Example entry:
```toml
[account_ids_with_access_and_traffic_last_week]
description = """
Get me account IDs with clicks in the last week
"""
query = """
SELECT
	customer.id
FROM customer
WHERE
	segments.date during LAST_WEEK_MON_SUN
	AND metrics.clicks > 0
"""
```

**Note**: The working copy at `resources/query_cookbook.toml` is typically identical to the config copy. During bundle installation, the resources copy is copied to `~/.config/mcc-gaql/`. Use the config directory path for consistency with `--validate` and auto-discovery.

### Step 2: Run the Generation Test Script

Execute the Python script to run `mcc-gaql-gen generate` for all entries:

```bash
python3 scripts/run_cookbook_gen_test.py
```

This script will:
- Parse `~/.config/mcc-gaql/query_cookbook.toml` (auto-discovered)
- Run `mcc-gaql-gen generate --explain --use-query-cookbook --no-defaults` for each entry
- Process entries with **concurrency limit of 5**
- Save results to `reports/gen_results.<timestamp>/`

**Options:**
```bash
# Specify custom cookbook path
python3 scripts/run_cookbook_gen_test.py --cookbook /path/to/query_cookbook.toml

# Use cargo run instead of installed binary
python3 scripts/run_cookbook_gen_test.py --mcc-gaql-gen "cargo run -p mcc-gaql-gen --release --"

# Test a single entry
python3 scripts/run_cookbook_gen_test.py --entry account_ids_with_access_and_traffic_last_week

# Dry run (show what would be done)
python3 scripts/run_cookbook_gen_test.py --dry-run
```

**Output format:** Each entry saves a JSON file with:
```json
{
  "entry_name": "...",
  "description": "...",
  "reference_query": "...",
  "generated_query": "...",
  "explanation": "...",
  "full_stdout": "...",
  "status": "success|error|timeout",
  "returncode": 0
}
```

### Step 3: Generate Comparison Report with Claude Code

Once the script completes, use Claude Code to analyze the results and generate the comparison report:

1. Read all JSON result files from `reports/gen_results.<timestamp>/`
2. For each entry, classify the result as EXCELLENT/GOOD/FAIR/POOR
3. Generate `reports/query_cookbook_gen_comparison.<timestamp>.md`

**Classification criteria:**

| Category | Description | How Explanation Helps |
|----------|-------------|----------------------|
| **EXCELLENT** | Generated query is semantically equivalent; would return nearly identical data | LLM reasoning shows correct understanding of requirements and aligns with reference intent |
| **GOOD** | Generated query captures main intent; minor differences in fields/filters | Reasoning is sound but LLM chose a slightly different approach (e.g., more comprehensive field set) |
| **FAIR** | Generated query is on the right track but missing important fields or filters | Explanation reveals partial understanding or missing key requirement interpretation |
| **POOR** | Generated query is incorrect or queries wrong resource entirely | LLM reasoning shows fundamental misunderstanding of the request or wrong scope |

**Evaluation criteria:**
- **Selected Fields**: Does the generated query select the same core fields? Extra fields are acceptable; missing key fields are problematic
- **Data Scope**: Is the same resource being queried? Is the date range semantically equivalent?
- **Semantic Equivalence**: Would both queries return conceptually similar data?

**Output report format:**
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
- Reasoning Summary: <high-level summary>
- Key Decision Points: <list of decisions>
- Comparison to Intent: <did reasoning match expected?>
- Where It Diverged: <discrepancy between explanation and output>

**Analysis:**
- Selected Fields: <comparison>
- Data Scope: <comparison>
- Semantic Equivalence: <assessment>

**Key Differences:**
- <difference 1>
- <difference 2>

---

## Overall Assessment
<Summary of patterns, failure modes, recommendations>
```

## Dynamic Discovery of Cookbook Entries

To count entries accurately without false exclusions:

```bash
# CORRECT: Counts all entries (116)
grep '^\[' ~/.config/mcc-gaql/query_cookbook.toml | wc -l

# WRONG: Excludes entries containing 'version' or 'conversion' substrings
grep '^\[' ~/.config/mcc-gaql/query_cookbook.toml | grep -v 'metadata\|version' | wc -l
```

The Python script correctly parses TOML and skips only actual `[metadata]` or `[version]` sections.

## Time Estimate

- Python script execution: ~60-90 minutes for 116 entries (5 concurrent workers, ~30-45 seconds per entry)
- Report generation: ~15-20 minutes base + ~1 minute per 10 entries

## Special Considerations

1. **Query Cookbook Auto-Discovery**
   - The tool automatically looks for `query_cookbook.toml` in the config directory
   - Only use `--queries <path>` if the cookbook is in a non-standard location
   - When `--use-query-cookbook` is enabled, the system retrieves similar queries as RAG context

2. **Using the Python Script**
   - Handles TOML parsing, concurrency, timeouts, and JSON output automatically
   - Creates timestamped directories automatically
   - Saves both generated queries and LLM explanations for analysis

3. **Additional Generate Options**
   - `--explain`: Print explanation of the LLM selection process to stdout (captured by script)
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
   - If generation fails for an entry, the script documents the error in the JSON
   - Classification should be POOR for failed generations
   - Common issues: missing embeddings cache, missing enriched metadata, LLM connectivity

## Troubleshooting

### Embeddings Cache Outdated or Missing

If you see this error when running the script:
```
ERROR: Embeddings cache is not built or is out-of-date.

To generate GAQL queries, you must first build the embeddings cache:
  mcc-gaql-gen index
```

**Solution:** Run the indexing command before the test:
```bash
mcc-gaql-gen index
```

This is a one-time operation that takes 3-5 minutes. After indexing completes, re-run the Python script.

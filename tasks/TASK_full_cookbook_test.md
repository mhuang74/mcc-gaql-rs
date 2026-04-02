# TASK: Execute Full 47-Query Cookbook Test Run

## Objective
Execute the complete cookbook test run using the Cookbook Agent Execution Plan.
This will test all 47 queries in resources/query_cookbook.toml.

## Prerequisites
1. Make sure domain knowledge is properly embedded in the binary
2. Ensure embeddings cache is built (run index if needed)
3. Use the agent execution approach as documented

## Execution Steps

### Step 1: Setup Environment
```bash
export TIMESTAMP=$(date +%Y%m%d%H%M%S)
mkdir -p reports/gen_results.${TIMESTAMP}
echo "Test run starting at: $TIMESTAMP"
```

### Step 2: Get All Query Names
```bash
awk '/^\[/ && /\]$/ {gsub(/^\[/, "", $0); gsub(/\]$/, "", $0); print}' resources/query_cookbook.toml > /tmp/query_names.txt
echo "Total queries: $(wc -l < /tmp/query_names.txt)"
```

### Step 3: Execute All 47 Queries

Process each query and save results. For each query:
- Extract description from TOML
- Run: `mcc-gaql-gen generate "<description>" --use-query-cookbook --explain --no-defaults`
- Save to: `reports/gen_results.${TIMESTAMP}/<query_name>.txt`

Batch processing (max 5 concurrent):
```bash
# Process queries in batches
# Batch 1: queries 1-5
# Batch 2: queries 6-10
# etc.
```

### Step 4: Generate Comparison Report
```bash
cargo run -p mcc-gaql-gen --release -- test-run \
  --input resources/query_cookbook.toml \
  --output reports/query_cookbook_gen_comparison.${TIMESTAMP}.md
```

## Expected Outputs
- Individual results: `reports/gen_results.<timestamp>/` (47 .txt files)
- Comparison report: `reports/query_cookbook_gen_comparison.<timestamp>.md`

## Time Estimate
- 47 queries × ~30s = ~24 minutes
- Report generation: ~5 minutes
- Total: ~30 minutes

## Success Criteria
- All 47 queries generate without errors
- Comparison report shows improvements vs previous run
- Previously POOR queries now EXCELLENT/GOOD

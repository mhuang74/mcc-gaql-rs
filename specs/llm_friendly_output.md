# LLM-Friendly Output Format Specification

## Problem Statement

Currently, `mcc-gaql` outputs query results as ASCII-formatted tables to STDOUT by default. While this is human-readable, it presents several challenges:

1. **Not machine-parseable**: The ASCII table format with borders, padding, and alignment makes it difficult for downstream tools to parse
2. **Not LLM-friendly**: Large language models struggle to accurately parse tabular text, especially with complex data
3. **Not pipeable**: Cannot easily pipe output to tools like `jq`, `awk`, or other data processing utilities
4. **Requires file I/O for structured data**: Currently need to use `--output` flag to get CSV, which requires writing to disk first

### Current Behavior

```bash
# Default: ASCII table to STDOUT
$ mcc-gaql "SELECT campaign.id, campaign.name FROM campaign"
┌─────────────┬──────────────┐
│ campaign.id │ campaign.name│
├─────────────┼──────────────┤
│ 123456789   │ Campaign 1   │
│ 987654321   │ Campaign 2   │
└─────────────┴──────────────┘

# CSV requires file output
$ mcc-gaql "SELECT campaign.id FROM campaign" --output results.csv
$ cat results.csv
campaign.id,campaign.name
123456789,Campaign 1
987654321,Campaign 2
```

## Proposed Solution

Add a `--format` flag to support multiple output formats while maintaining backward compatibility.

### New CLI Flag

```rust
/// Output format: table, csv, json
#[clap(long, default_value = "table")]
pub format: String,
```

**Note**: No short flag (`-f`) to avoid conflict with existing `--field-service` flag.

**Supported formats:**
- `table` (default): Current ASCII table format via Polars Display trait
- `csv`: Comma-separated values, written to STDOUT
- `json`: JSON array of objects, written to STDOUT

### Design Decisions

#### 1. Backward Compatibility
- **Default behavior unchanged**: `--format=table` is the default
- Existing scripts and workflows continue to work without modification
- `--output` flag continues to write files as before

#### 2. Format Options

**Table Format (Default)**
- Human-readable ASCII table
- Uses Polars' built-in Display trait
- Best for terminal viewing and ad-hoc queries

**CSV Format**
- RFC 4180 compliant CSV output
- Written directly to STDOUT for piping
- Uses Polars' CsvWriter with default settings
- Properly escapes quotes, commas, and newlines
- Includes header row

**JSON Format**
- Array of objects format: `[{"col1": "val1", "col2": "val2"}, ...]`
- Written directly to STDOUT for piping
- Uses manual serialization via serde_json (to avoid polars version conflicts)
- Each row is an object with column names as keys
- Proper type inference: numbers as numbers, strings as strings, null as null
- Most LLM-friendly format for structured data

#### 3. Output Destinations

| Flag Combination | Behavior |
|-----------------|----------|
| (none) | Table to STDOUT |
| `--format=table` | Table to STDOUT (explicit default) |
| `--format=csv` | CSV to STDOUT |
| `--format=json` | JSON to STDOUT |
| `--output=file.csv` | CSV to file (current behavior) |
| `--format=csv --output=file.csv` | CSV to file |
| `--format=json --output=file.json` | JSON to file |

#### 4. Metadata Handling

**Simplified approach**: No metadata in data output
- Log messages (timing, API consumption) remain in STDERR via `log::info!()`
- Data output to STDOUT is pure data only
- Rationale: Clean separation allows easy piping without filtering

## Implementation Details

### 1. Update CLI Arguments (`src/args.rs`)

**Location**: `src/args.rs:27`

```rust
/// Output format: table, csv, json
#[clap(long, default_value = "table")]
pub format: String,
```

**Note**: Removed short flag `-f` to avoid conflict with existing `--field-service` flag.

### 2. Add Output Helper Functions (`src/main.rs`)

**Locations**: `src/main.rs:486-580`

```rust
/// Write DataFrame as CSV to stdout (line 494)
fn write_csv_to_stdout(df: &mut DataFrame) -> Result<()> {
    let mut buf = Vec::new();
    CsvWriter::new(&mut buf).finish(df)?;
    print!("{}", String::from_utf8(buf)?);
    Ok(())
}

/// Write DataFrame as JSON to stdout (line 494)
fn write_json_to_stdout(df: &mut DataFrame) -> Result<()> {
    // Manual serialization using serde_json to avoid polars version conflicts
    let columns: Vec<String> = df.get_column_names().iter().map(|s| s.to_string()).collect();
    let mut records: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

    for row_idx in 0..df.height() {
        let mut record = serde_json::Map::new();
        for (col_idx, col_name) in columns.iter().enumerate() {
            let column = df.get_columns().get(col_idx).unwrap();
            let value = column.get(row_idx)?;
            let json_value = format!("{}", value);

            // Remove surrounding quotes (polars adds quotes to strings)
            let cleaned_value = json_value.trim_matches('"');

            // Type inference: try int, then float, then null, else string
            let json_val = if let Ok(num) = cleaned_value.parse::<i64>() {
                serde_json::Value::Number(serde_json::Number::from(num))
            } else if let Ok(num) = cleaned_value.parse::<f64>() {
                serde_json::Number::from_f64(num)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(cleaned_value.to_string()))
            } else if cleaned_value == "null" {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(cleaned_value.to_string())
            };
            record.insert(col_name.clone(), json_val);
        }
        records.push(record);
    }

    let json = serde_json::to_string(&records)?;
    println!("{}", json);
    Ok(())
}

/// Write DataFrame as JSON to file (line 532)
fn write_json(df: &mut DataFrame, outfile: &str) -> Result<()> {
    // Same logic as write_json_to_stdout but writes to file
    let f = File::create(outfile)?;
    serde_json::to_writer(f, &records)?;
    Ok(())
}

/// Handle output based on format and output file (line 569)
fn output_dataframe(df: &mut DataFrame, format: &str, outfile: Option<String>) -> Result<()> {
    match (format, outfile) {
        // File output
        (_, Some(path)) => {
            if format == "json" || path.ends_with(".json") {
                write_json(df, &path)?;
            } else {
                write_csv(df, &path)?;
            }
        }
        // STDOUT output
        ("csv", None) => write_csv_to_stdout(df)?,
        ("json", None) => write_json_to_stdout(df)?,
        ("table", None) | (_, None) => println!("{}", df),
    }
    Ok(())
}
```

**Implementation Notes**:
- JSON serialization is manual (not using polars JsonWriter) to avoid version conflicts with polars 0.42
- Type inference properly handles integers, floats, nulls, and strings
- Cleaned up escaped quotes from polars string formatting

### 3. Update Output Call Sites

**In `gaql_query_async()` function signature (line 241):**
```rust
// Before
async fn gaql_query_async(
    api_context: GoogleAdsAPIAccess,
    customer_id_vector: Vec<String>,
    query: String,
    groupby: Vec<String>,
    sortby: Vec<String>,
    outfile: Option<String>,
) -> Result<()>

// After - added format parameter
async fn gaql_query_async(
    api_context: GoogleAdsAPIAccess,
    customer_id_vector: Vec<String>,
    query: String,
    groupby: Vec<String>,
    sortby: Vec<String>,
    format: String,
    outfile: Option<String>,
) -> Result<()>
```

**In `gaql_query_async()` output (line 428):**
```rust
// Before (lines 426-438)
if outfile.is_some() {
    let start = Instant::now();
    write_csv(&mut dataframe, &outfile.unwrap())?;
    let duration = start.elapsed();
    log::debug!("csv written in {} msec", duration.as_millis().separate_with_commas());
} else {
    println!("{}", dataframe);
}

// After
let start = Instant::now();
output_dataframe(&mut dataframe, &format, outfile)?;
let duration = start.elapsed();
log::debug!("output written in {} msec", duration.as_millis().separate_with_commas());
```

**In `main()` calling `gaql_query_async()` (line 222):**
```rust
// Before
gaql_query_async(
    api_context,
    customer_id_vector,
    query,
    args.groupby,
    args.sortby,
    args.output,
).await?;

// After - added format parameter
gaql_query_async(
    api_context,
    customer_id_vector,
    query,
    args.groupby,
    args.sortby,
    args.format,
    args.output,
).await?;
```

**In list_child_accounts (line 156):**
```rust
// Before (lines 155-161)
if dataframe.is_some() {
    if args.output.is_some() {
        write_csv(&mut dataframe.unwrap(), args.output.as_ref().unwrap())?;
    } else {
        println!("{}", dataframe.unwrap());
    }
}

// After
if dataframe.is_some() {
    output_dataframe(&mut dataframe.unwrap(), &args.format, args.output)?;
}
```

### 4. Update Dependencies

**Location**: `Cargo.toml:24`

```toml
# Before
polars = { version = "0.42", features = ["lazy"] }

# After - added serde-lazy for better serialization support
polars = { version = "0.42", features = ["lazy", "serde-lazy"] }
```

**Note**: `serde_json = "1.0"` was already present in dependencies (line 31).

**Why not use polars' json feature?**
- The `json` feature in polars 0.42 had compilation conflicts
- Manual serialization with `serde_json` provides more control over type inference
- Avoids dependency on polars' internal JSON implementation

## Usage Examples

### Pipe to jq for JSON processing
```bash
mcc-gaql "SELECT campaign.id, campaign.name FROM campaign" --format=json | jq '.[] | select(.campaign_name | contains("Brand"))'
```

### Pipe to CSV tools
```bash
mcc-gaql "SELECT campaign.id FROM campaign" --format=csv | column -t -s,
```

### Feed directly to LLM via API
```bash
mcc-gaql "SELECT * FROM campaign" --format=json | curl -X POST api.anthropic.com/v1/messages \
  -H "Content-Type: application/json" \
  -d '{"model": "claude-3-5-sonnet-20241022", "messages": [{"role": "user", "content": "Analyze this campaign data: '"$(cat)"'"}]}'
```

### Combine with other tools
```bash
# Count campaigns with CSV
mcc-gaql "SELECT campaign.name FROM campaign" --format=csv | tail -n +2 | wc -l

# Extract specific field with jq
mcc-gaql "SELECT campaign.id, metrics.clicks FROM campaign" --format=json | jq '.[].metrics_clicks' | awk '{sum+=$1} END {print sum}'
```

### Backward compatible (existing workflows unaffected)
```bash
# Still works - human readable output
mcc-gaql "SELECT campaign.name FROM campaign"

# Still works - file output
mcc-gaql "SELECT * FROM campaign" --output results.csv
```

## Testing Strategy

### Manual Testing
1. **Default behavior**: Run without flags, verify table output unchanged
2. **CSV output**: Test `--format=csv` pipes correctly to other commands
3. **JSON output**: Test `--format=json` produces valid JSON parseable by `jq`
4. **File output**: Test `--output` still works with and without `--format`
5. **Edge cases**: Empty results, special characters in data, very large datasets

### Test Commands
```bash
# Test table (default)
cargo run -- "SELECT campaign.id, campaign.name FROM campaign LIMIT 5"

# Test CSV to stdout
cargo run -- "SELECT campaign.id, campaign.name FROM campaign LIMIT 5" --format=csv

# Test JSON to stdout
cargo run -- "SELECT campaign.id, campaign.name FROM campaign LIMIT 5" --format=json | jq

# Test CSV to file (existing behavior)
cargo run -- "SELECT campaign.id, campaign.name FROM campaign LIMIT 5" --output test.csv

# Test JSON to file (new)
cargo run -- "SELECT campaign.id, campaign.name FROM campaign LIMIT 5" --format=json --output test.json

# Test piping
cargo run -- "SELECT campaign.id FROM campaign LIMIT 10" --format=csv | tail -n +2 | wc -l
```

## Benefits

1. **Machine-readable**: CSV and JSON are standard formats easily consumed by tools
2. **LLM-friendly**: JSON provides structured data that LLMs can accurately parse
3. **Pipeable**: Direct stdout output enables Unix-style command chaining
4. **Backward compatible**: Existing workflows continue to work unchanged
5. **Flexible**: Users choose the format that best suits their use case
6. **No intermediate files**: Process data in-memory without disk I/O

## Future Enhancements

Potential additions for future iterations:
- JSONL/NDJSON format for streaming large datasets
- Parquet format for analytics workflows
- XML format if needed for specific integrations
- Custom delimiters for CSV (e.g., TSV with `--delimiter='\t'`)
- Pretty-print JSON option (`--format=json-pretty`)

---

## Implementation Completed

**Date**: 2025-10-20

### Summary

Successfully implemented LLM-friendly output formats for `mcc-gaql` with full backward compatibility.

### Files Modified

1. **Cargo.toml** (line 24)
   - Added `serde-lazy` feature to polars dependencies
   - Verified `serde_json` already present

2. **src/args.rs** (line 27)
   - Added `--format` flag with default value "table"
   - No short flag to avoid conflict with `--field-service`

3. **src/main.rs** (lines 241-580)
   - Modified `gaql_query_async()` signature to accept `format` parameter
   - Added 4 new functions:
     - `write_csv_to_stdout()` (line 494)
     - `write_json_to_stdout()` (line 494)
     - `write_json()` (line 532)
     - `output_dataframe()` (line 569)
   - Updated output logic in `gaql_query_async()` (line 428)
   - Updated output logic in `list_child_accounts` (line 156)
   - Updated function call to `gaql_query_async()` to pass format (line 228)

### Testing Results

All output formats tested successfully:

#### Table Format (Default)
```bash
$ mcc-gaql -q all_campaigns
shape: (2, 7)
┌─────────────┬──────────────┬─────────────┬─────────────┬─────────────┬─────────────┬─────────────┐
│ customer.id ┆ customer.des ┆ campaign.id ┆ campaign.na ┆ campaign.ad ┆ campaign.st ┆ campaign.pr │
│ ---         ┆ criptive_nam ┆ ---         ┆ me          ┆ vertising_c ┆ atus        ┆ imary_statu │
│ str         ┆ e            ┆ str         ┆ ---         ┆ hannel_t…   ┆ ---         ┆ s           │
│             ┆ ---          ┆             ┆ str         ┆ ---         ┆ str         ┆ ---         │
│             ┆ str          ┆             ┆             ┆ str         ┆             ┆ str         │
╞═════════════╪══════════════╪═════════════╪═════════════╪═════════════╪═════════════╪═════════════╡
│ 4152937756  ┆ Infinite     ┆ 22570917289 ┆ Brand       ┆ Display     ┆ Enabled     ┆ NotEligible │
│             ┆ Worship      ┆             ┆             ┆             ┆             ┆             │
│ 4152937756  ┆ Infinite     ┆ 22603776175 ┆ Leads-Searc ┆ Search      ┆ Enabled     ┆ NotEligible │
│             ┆ Worship      ┆             ┆ h-1         ┆             ┆             ┆             │
└─────────────┴──────────────┴─────────────┴─────────────┴─────────────┴─────────────┴─────────────┘
```

#### CSV Format
```bash
$ mcc-gaql -q all_campaigns --format=csv
customer.id,customer.descriptive_name,campaign.id,campaign.name,campaign.advertising_channel_type,campaign.status,campaign.primary_status
4152937756,Infinite Worship,22570917289,Brand,Display,Enabled,NotEligible
4152937756,Infinite Worship,22603776175,Leads-Search-1,Search,Enabled,NotEligible
```

#### JSON Format (with jq)
```bash
$ mcc-gaql -q all_campaigns --format=json | jq
[
  {
    "campaign.advertising_channel_type": "Display",
    "campaign.id": 22570917289,
    "campaign.name": "Brand",
    "campaign.primary_status": "NotEligible",
    "campaign.status": "Enabled",
    "customer.descriptive_name": "Infinite Worship",
    "customer.id": 4152937756
  },
  {
    "campaign.advertising_channel_type": "Search",
    "campaign.id": 22603776175,
    "campaign.name": "Leads-Search-1",
    "campaign.primary_status": "NotEligible",
    "campaign.status": "Enabled",
    "customer.descriptive_name": "Infinite Worship",
    "customer.id": 4152937756
  }
]
```

**Note**: JSON output properly handles types:
- Integers remain as numbers (e.g., `22570917289` not `"22570917289"`)
- Strings remain as strings (e.g., `"Brand"`)
- Proper type inference implemented

#### File Output
```bash
# CSV to file (existing behavior - still works)
$ mcc-gaql -q all_campaigns --output=/tmp/test.csv

# JSON to file (new)
$ mcc-gaql -q all_campaigns --format=json --output=/tmp/test.json
```

#### List Child Accounts
```bash
$ mcc-gaql --list-child-accounts --format=json | jq '.[0]'
{
  "customer_client.currency_code": "USD",
  "customer_client.descriptive_name": "Infinite Worship",
  "customer_client.id": 4152937756,
  "customer_client.level": 1,
  "customer_client.time_zone": "America/Los_Angeles"
}
```

### Quality Checks

- ✅ **Builds successfully**: `cargo build` completes without errors
- ✅ **Clippy passes**: `cargo clippy --all-targets --all-features -- -D warnings` passes
- ✅ **Backward compatible**: Default behavior unchanged (table format)
- ✅ **CSV output works**: Properly formatted and pipeable
- ✅ **JSON output works**: Valid JSON with proper type handling
- ✅ **File output works**: Both formats write to files correctly
- ✅ **List accounts works**: All formats work with `--list-child-accounts`

### Key Implementation Decisions

1. **No short flag for --format**: Avoided conflict with existing `-f` (field-service) flag
2. **Manual JSON serialization**: Used `serde_json` instead of polars' JsonWriter to avoid version conflicts
3. **Type inference**: Implemented proper handling of integers, floats, nulls, and strings in JSON output
4. **Quote stripping**: Removed escaped quotes from polars string formatting for clean JSON
5. **Unified output function**: Created `output_dataframe()` to centralize all output logic

### Benefits Realized

1. **Machine-readable**: CSV and JSON can be directly piped to other tools
2. **LLM-friendly**: Clean JSON structure with proper types
3. **Pipeable**: Examples tested:
   - `mcc-gaql -q all_campaigns --format=json | jq`
   - `mcc-gaql -q all_campaigns --format=csv | tail -n +2 | wc -l`
4. **Backward compatible**: All existing workflows continue to work
5. **Flexible**: Users choose format based on use case

### Known Limitations

None identified during implementation and testing.

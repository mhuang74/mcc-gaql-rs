# Plan: Improve Date Filtering Handling

## Context

The current implementation has inconsistent date filtering:
1. The LLM is told to ALWAYS use BETWEEN with explicit ISO dates and NEVER use relative date literals
2. There's a server-side fallback (`detect_temporal_period_impl`) that converts natural language to DURING with date literals
3. The user reports that when the LLM picks date pairs, they sometimes create malformed BETWEEN clauses like:
   ```
   WHERE segments.date BETWEEN '2026-01-29' AND segments.date BETWEEN '2027-02-16'
   ```

## Decision

Per user guidance, use the RIGHT tool for each date range:

### Use DURING with Google Ads Date Literals

For periods that have built-in Google Ads date literals (from https://developers.google.com/google-ads/api/docs/query/date-ranges):

**Valid date literals for DURING:**
- `TODAY`, `YESTERDAY`
- `LAST_7_DAYS`, `LAST_14_DAYS`, `LAST_30_DAYS`
- `LAST_BUSINESS_WEEK` (Mon-Fri of previous week)
- `THIS_WEEK_SUN_TODAY`, `THIS_WEEK_MON_TODAY`
- `LAST_WEEK_MON_SUN`, `LAST_WEEK_SUN_SAT`
- `THIS_MONTH`, `LAST_MONTH`

**NOT valid** (use BETWEEN with computed dates instead):
- THIS_QUARTER - NOT a valid date literal
- LAST_3_MONTHS - NOT a valid date literal
- LAST_12_MONTHS - NOT a valid date literal
- THIS_YEAR - NOT a valid date literal
- LAST_YEAR - NOT a valid date literal
- LAST_60_DAYS - NOT a valid date literal
- LAST_90_DAYS - NOT a valid date literal

### Use BETWEEN with Computed ISO Dates

For periods WITHOUT date literals, compute actual dates:

- `this quarter`, `last quarter` (calendar quarters: Jan-Mar, Apr-Jun, Jul-Sep, Oct-Dec)
- `this year`, `last year`
- `this summer`, `last summer` (Jun 1 - Aug 31)
- `this winter`, `last winter` (Dec 1 - Feb 28/29)
- `this spring`, `last spring` (Mar 1 - May 31)
- `this fall`/`this autumn`, `last fall`/`last autumn` (Sep 1 - Nov 30)
- `this christmas holiday`, `last christmas holiday` (Dec 20 - Dec 31)
- `this thanksgiving`, `last thanksgiving` (Thanksgiving week - calculate based on US holiday, 4th Thu of Nov)
- `this easter`, `last easter` (Easter week)
- `black friday`, `cyber monday`
- `new years`, `valentines day`, `mothers day`, `fathers day`, `halloween`
- "last 60 days"
- "last 90 days" 
- Any other custom date ranges

## Changes Required

### 1. Update Phase 3 Prompt in `rag.rs` (lines ~1698-1758)

Replace the current date instructions with:

```rust
- For date ranges, use the APPROPRIATE method based on the period:

  **Use DURING with date literals** (NO quotes around value) for these standard periods.
  Valid Google Ads date literals: TODAY, YESTERDAY, LAST_7_DAYS, LAST_14_DAYS,
  LAST_30_DAYS, LAST_BUSINESS_WEEK, LAST_WEEK_MON_SUN,
  LAST_WEEK_SUN_SAT, THIS_WEEK_SUN_TODAY, THIS_WEEK_MON_TODAY, THIS_MONTH, LAST_MONTH

  Common mappings:
  - "yesterday" → operator: "DURING", value: "YESTERDAY"
  - "today" → operator: "DURING", value: "TODAY"
  - "last 7 days" → operator: "DURING", value: "LAST_7_DAYS"
  - "last 14 days" → operator: "DURING", value: "LAST_14_DAYS"
  - "last 30 days" → operator: "DURING", value: "LAST_30_DAYS"
  - "this month" → operator: "DURING", value: "THIS_MONTH"
  - "last month" → operator: "DURING", value: "LAST_MONTH"
  - "last week" → operator: "DURING", value: "LAST_WEEK"
  - "last business week" → operator: "DURING", value: "LAST_BUSINESS_WEEK"

  **Use BETWEEN with computed dates** (value format: "YYYY-MM-DD AND YYYY-MM-DD") for:
  - Quarters (NOT valid date literals): "this quarter", "last quarter"
  - Years (NOT valid date literals): "this year", "last year"
  - Holiday periods and seasonal ranges:
    - "this summer" / "last summer" → Jun 1 to Aug 31
    - "this winter" / "last winter" → Dec 1 to Feb 28/29
    - "this spring" / "last spring" → Mar 1 to May 31
    - "this fall" / "this autumn" / "last fall" → Sep 1 to Nov 30
    - "christmas holiday" → Dec 20 to Dec 31
    - "thanksgiving" / "thanksgiving week"
    - "easter" / "easter week"
    - "black friday", "cyber monday"
    - "new years", "valentines day", "mothers day", "fathers day", "halloween"
    - "last 60 days"
    - "last 90 days" 

  Example computed date ranges (today: {today}):
  - "this year" → BETWEEN '{this_year_start}' AND '{today}'
  - "last year" → BETWEEN '{prev_year_start}' AND '{prev_year_end}'
  - "this quarter" → BETWEEN '{this_quarter_start}' AND '{today}'
  - "last quarter" → BETWEEN '{prev_quarter_start}' AND '{prev_quarter_end}'
  - "this summer" → BETWEEN '{this_summer_start}' AND '{this_summer_end}'
  - "last summer" → BETWEEN '{last_summer_start}' AND '{last_summer_end}'
  - "this winter" → BETWEEN '{this_winter_start}' AND '{this_winter_end}'
  - "last winter" → BETWEEN '{last_winter_start}' AND '{last_winter_end}'
  - "this thanksgiving" → BETWEEN '{this_thanksgiving_start}' AND '{this_thanksgiving_end}'
  - "last thanksgiving" → BETWEEN '{last_thanksgiving_start}' AND '{last_thanksgiving_end}'
```

### 2. Add Holiday Date Calculations in `rag.rs` (lines ~1638-1674)

Add computed date variables for holidays and seasons:

```rust
// Calculate this/previous period start dates
// ... existing calculations ...

// Season calculations (fixed dates)
let year = today.year();
let (this_summer_start, this_summer_end) = (
    format!("{year}-06-01"),
    format!("{year}-08-31")
);
let (last_summer_start, last_summer_end) = (
    format!("{}-06-01", year - 1),
    format!("{}-08-31", year - 1)
);
let (this_winter_start, this_winter_end) = (
    format!("{year}-12-01"),
    format!("{year}-02-28")  // handle leap year: check if year+1 is leap
);
let (last_winter_start, last_winter_end) = (
    format!("{}-12-01", year - 1),
    format!("{year}-02-28")  // spans into current year
);
let (this_spring_start, this_spring_end) = (
    format!("{year}-03-01"),
    format!("{year}-05-31")
);
let (last_spring_start, last_spring_end) = (
    format!("{}-03-01", year - 1),
    format!("{}-05-31", year - 1)
);
let (this_fall_start, this_fall_end) = (
    format!("{year}-09-01"),
    format!("{year}-11-30")
);
let (last_fall_start, last_fall_end) = (
    format!("{}-09-01", year - 1),
    format!("{}-11-30", year - 1)
);

// Thanksgiving (4th Thursday of November)
let (this_thanksgiving_start, this_thanksgiving_end) = compute_thanksgiving_week(year);
let (last_thanksgiving_start, last_thanksgiving_end) = compute_thanksgiving_week(year - 1);

// Christmas holiday period (Dec 20-31)
let this_christmas_start = format!("{year}-12-20");
let this_christmas_end = format!("{year}-12-31");
let last_christmas_start = format!("{}-12-20", year - 1);
let last_christmas_end = format!("{}-12-31", year - 1);

// Fixed-date holidays
let valentines = format!("{year}-02-14");
let halloween = format!("{year}-10-31");

// Easter (complex calculation - may need computus algorithm)
let (this_easter_start, this_easter_end) = compute_easter_week(year);
let (last_easter_start, last_easter_end) = compute_easter_week(year - 1);
```

### 3. Update Filter Construction Logic in `rag.rs` (lines ~1967-1982)

Add validation to catch malformed BETWEEN values:

```rust
"BETWEEN" => {
    // BETWEEN value should be "start AND end" without the field name
    if let Some((start, end)) = escaped_value.split_once(" AND ") {
        let start_clean = start.trim();
        let end_clean = end.trim();
        // Validate that neither part contains the field name or nested BETWEEN
        if start_clean.contains("segments.date") || start_clean.contains("BETWEEN")
            || end_clean.contains("segments.date") || end_clean.contains("BETWEEN") {
            log::error!("Malformed BETWEEN value contains field name or nested BETWEEN: '{}'", escaped_value);
            // Attempt to extract just the dates
            let fixed = escaped_value.replace("segments.date", "").replace("BETWEEN", "").replace("'", "");
            if let Some((s, e)) = fixed.split_once(" AND ") {
                format!("{} BETWEEN '{}' AND '{}'", ff.field_name, s.trim(), e.trim())
            } else {
                format!("{} BETWEEN '{}' AND '{}'", ff.field_name, start_clean, end_clean)
            }
        } else {
            format!("{} BETWEEN '{}' AND '{}'", ff.field_name, start_clean, end_clean)
        }
    } else {
        log::error!("Invalid BETWEEN format for '{}': expected 'start AND end', got '{}'", ff.field_name, escaped_value);
        format!("{} BETWEEN '{}' AND '{}'", ff.field_name, escaped_value, escaped_value)
    }
}
```

### 4. Remove Server-Side DURING Detection

Remove the `detect_temporal_period_impl` function (lines ~2331-2354) and its usage, since the LLM will now handle all temporal filtering.

### 5. Remove `during` field from GaqlBuilder

Update GaqlBuilder to:
1. Remove the `during` field entirely
2. Remove the code that adds `segments.date DURING {during}` to where_parts
3. Let the LLM handle date filtering through filter_fields exclusively

## Files to Modify

- `/Users/mhuang/Projects/Development/googleads/improve_llm_selection_process/crates/mcc-gaql-gen/src/rag.rs`
  - Lines ~1638-1674: Add holiday/season date calculations
  - Lines ~1676-1765: Update system prompts with DURING/BETWEEN guidance
  - Lines ~1970-1979: Add validation for malformed BETWEEN
  - Lines ~2098-2100: Remove during-based segments.date auto-add
  - Lines ~2122-2124: Remove during from where_parts
  - Lines ~2331-2354: Remove detect_temporal_period_impl function

## Verification

1. Build: `cargo build -p mcc-gaql-gen --release`
2. Test DURING: `cargo run -p mcc-gaql-gen -- "show me campaigns last 7 days" --explain-selection-process`
   - Should produce: `segments.date DURING LAST_7_DAYS`
3. Test BETWEEN: `cargo run -p mcc-gaql-gen -- "show me campaigns last summer" --explain-selection-process`
   - Should produce: `segments.date BETWEEN 'YYYY-MM-DD' AND 'YYYY-MM-DD'`
4. Verify no malformed BETWEEN (field repetition) occurs

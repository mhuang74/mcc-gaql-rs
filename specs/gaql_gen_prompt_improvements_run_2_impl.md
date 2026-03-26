# Implementation Plan: Fix GAQL Query Generation Bugs

## Context

The `mcc-gaql-gen` RAG pipeline was tested against 26 reference queries from the query cookbook. A comparison report (`reports/query_cookbook_gen_comparison_1.md`) revealed systematic failures:

| Issue | Frequency | Severity |
|-------|-----------|----------|
| Missing date filters | 18/26 (69%) | Critical |
| Missing LIMIT clauses | 12/26 (46%) | High |
| Missing customer identifiers | 13/26 (50%) | Medium |
| Malformed IN clauses | Affects all IN filters | High |
| Quoted numeric comparisons | Affects all numeric filters | Medium |

**Out of scope**: Asset resource disambiguation (entries 9-12, 14). The generated queries using `campaign_asset` may be more appropriate than the cookbook's `asset_field_type_view` for these use cases.

## Root Causes

1. **IN clause bug** (code): Catch-all arm at `rag.rs:3091` wraps all values in single quotes, corrupting parenthesized IN lists
2. **Numeric quoting bug** (code): Same catch-all quotes numeric values (`> '0'` instead of `> 0`)
3. **Missing dates** (prompt): Soft instruction "Include segments.date if temporal period is specified" — LLM treats as optional
4. **Missing LIMIT** (prompt + code): No `limit` field in LLM JSON schema; `detect_limit_impl` only handles "top N" with explicit digits
5. **Missing customer fields** (prompt): No guidance on including `customer.id`/`customer.descriptive_name`

## File to Modify

**`crates/mcc-gaql-gen/src/rag.rs`** — all changes in this single file.

Key locations:
- System prompt (with-cookbook): lines 2618-2693
- System prompt (without-cookbook): lines 2700-2773
- LLM response parsing: lines 2803-2934
- `FieldSelectionResult` struct: lines 3238-3243
- `assemble_criteria` function: lines 2969-3109
- Value escaping: line 3032
- Catch-all format arm: line 3091
- `detect_limit` call: line 3097
- `detect_limit_impl`: lines 3436-3451
- `parse_field_selection_response` (test helper): lines 3824-3899

## Changes

### Change 1: Fix IN/NOT IN clause formatting

**Location**: `assemble_criteria`, line 3091

**Current** (catch-all handles everything):
```rust
_ => format!("{} {} '{}'", ff.field_name, op, escaped_value),
```

**Add before the catch-all**:
```rust
"IN" | "NOT IN" => format!("{} {} {}", ff.field_name, op, ff.value),
```

Uses `ff.value` directly (not `escaped_value`) because:
- The LLM provides a properly-formatted parenthesized list: `('ENABLED', 'PAUSED')`
- `escape_value` corrupts inner single quotes: `(\\'ENABLED\\', \\'PAUSED\\')`
- The outer `'{}'` wrap in the catch-all adds a second layer of quotes

### Change 2: Fix numeric value quoting

**Location**: `assemble_criteria`, line 3091

**Add before the catch-all** (after the IN/NOT IN arm):
```rust
">" | "<" | ">=" | "<=" => {
    if escaped_value.parse::<f64>().is_ok() {
        format!("{} {} {}", ff.field_name, op, escaped_value)
    } else {
        format!("{} {} '{}'", ff.field_name, op, escaped_value)
    }
},
```

This correctly handles:
- `metrics.clicks > 0` → unquoted (numeric)
- `metrics.cost_micros > 1000000` → unquoted (numeric)
- `some.field > 'text_value'` → quoted (non-numeric fallback)

### Change 3: Add `limit` field to LLM pipeline

#### 3a. Update `FieldSelectionResult` struct (line 3238)

```rust
struct FieldSelectionResult {
    select_fields: Vec<String>,
    filter_fields: Vec<mcc_gaql_common::field_metadata::FilterField>,
    order_by_fields: Vec<(String, String)>,
    limit: Option<u32>,        // NEW
    reasoning: String,
}
```

#### 3b. Update JSON schema in both prompts (lines 2629-2634, 2710-2715)

```json
{{
  "select_fields": ["field1", "field2", ...],
  "filter_fields": [{{"field": "field_name", "operator": "=", "value": "value"}}],
  "order_by_fields": [{{"field": "field_name", "direction": "DESC"}}],
  "limit": null,
  "reasoning": "brief explanation"
}}
```

#### 3c. Add limit instruction in both prompts (after line 2640, 2721)

```
- If the query asks for "top N", "first N", "best N", or "worst N" results, set "limit" to that number N. If "top" or "best" is used without a specific number, default "limit" to 10. Otherwise set "limit" to null.
```

#### 3d. Parse `limit` from LLM JSON response (before line 2929)

```rust
let limit = parsed.get("limit").and_then(|v| v.as_u64()).map(|n| n as u32);
```

Add `limit` to the `FieldSelectionResult` construction at line 2929.

#### 3e. Use LLM limit as primary in `assemble_criteria` (line 3097)

**Current**:
```rust
let limit = self.detect_limit(user_query);
```

**Change to**:
```rust
let limit = field_selection.limit.or_else(|| self.detect_limit(user_query));
```

`detect_limit_impl` becomes a fallback for cases where the LLM omits the field.

#### 3f. Update test helper `parse_field_selection_response` (line 3893)

Add limit parsing and include `limit` in the returned struct.

### Change 4: Strengthen date filter instruction

**Location**: Both prompts, lines 2641 and 2722

**Replace**:
```
- Include segments.date if temporal period is specified
```

**With**:
```
- **MANDATORY: If the user query mentions ANY time period (last week, last 7 days, yesterday, this month, year to date, etc.), you MUST add a segments.date filter_field. Do NOT mention date ranges only in reasoning — they MUST appear in filter_fields. A query missing a date filter when the user specified a time period is INCORRECT.**
```

### Change 5: Add customer identifier guidance

**Location**: Both prompts, after the date filter instruction

**Add**:
```
- When querying account-level data (FROM customer) or when the user asks about accounts, always include customer.id and customer.descriptive_name in select_fields if available in the field list.
```

## Implementation Order

1. **Change 3a** — Add `limit` to struct + fix all construction sites (lines 2929, 3893) with `limit: None`
2. **Change 3d/3f** — Parse limit from JSON in production and test paths
3. **Change 3e** — Use LLM limit in `assemble_criteria`
4. **Changes 1 & 2** — Fix match arms in `assemble_criteria` (IN/NOT IN + numeric)
5. **Changes 3b/3c/4/5** — All prompt text updates

Rationale: Struct changes first to avoid compile errors; code logic next; prompt text last (no compile risk).

## Verification

```bash
cargo check -p mcc-gaql-gen
cargo test -p mcc-gaql-gen --lib -- --test-threads=1
```

Then re-run the cookbook generation comparison:
```bash
# Re-generate all 26 queries and produce a new comparison report
```

### Expected Improvements

| Issue | Current Rate | Expected After Fix |
|-------|-------------|-------------------|
| Missing date filters | 69% | ~10% (prompt compliance) |
| Missing LIMIT | 46% | ~5% (LLM JSON field) |
| Malformed IN clauses | 100% of IN queries | 0% (code fix) |
| Quoted numeric values | 100% of numeric filters | 0% (code fix) |
| Missing customer fields | 50% | ~15% (prompt guidance) |

## Differences from Original Spec (`gaql_gen_prompt_improvements_part_3.md`)

1. **Added**: Numeric quoting fix (Change 2) — discovered during code review, not in original spec
2. **Simplified**: IN/NOT IN fix uses `ff.value` directly instead of the original spec's two sub-options
3. **Chose**: LLM JSON field approach for LIMIT (over extending `detect_limit_impl`)
4. **Dropped**: Asset resource disambiguation — user confirmed generated queries may be more appropriate
5. **Default limit without number**: Set to 10 (original spec said 1, but 10 is more practical for "top campaigns")

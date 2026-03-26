# Plan: Enhance LLM Prompt to Fix Query Generation Deficiencies

## Context

The `mcc-gaql-gen` RAG pipeline generates GAQL queries from natural language. A comparison test against 26 reference queries revealed systematic failures:
- 69% missing date filters (mentioned in LLM reasoning but not applied in output)
- 46% missing LIMIT clauses (especially "top N" patterns)
- Malformed IN clauses: `IN '(\'PERFORMANCE_MAX\')'` instead of `IN ('PERFORMANCE_MAX')`
- 50% missing `customer.id` / `customer.descriptive_name` in account-level queries

## Root Cause Analysis

### 1. IN clause bug — code bug in `assemble_criteria` (line 3091)
The catch-all arm wraps ALL values in single quotes:
```rust
_ => format!("{} {} '{}'", ff.field_name, op, escaped_value)
```
For `IN` with value `('PERFORMANCE_MAX')`, the `escape_value` call (line 3032) first escapes inner quotes → `(\'PERFORMANCE_MAX\')`, then the format wraps it in outer quotes → `IN '(\'PERFORMANCE_MAX\')'`. Fix: add explicit `IN`/`NOT IN` arms that do NOT quote the value.

### 2. Date filters — prompt is too soft
Current: `- Include segments.date if temporal period is specified`
LLM treats this as optional. Needs mandatory framing with a fail-consequence.

### 3. Missing LIMIT — prompt doesn't mention it; `detect_limit_impl` is limited
`detect_limit_impl` (lines 3436–3451) only matches "top N", "first N", "best N", "worst N" with an explicit number. "top PMax campaign" (no number) yields no LIMIT. Fix: prompt should instruct LLM to emit a `limit` field in the JSON, and the parsing code should use it (with `detect_limit_impl` as fallback). Alternatively: extend `detect_limit_impl` to handle implicit "top 1" (no number → 1).

### 4. Missing customer identifiers — prompt has no guidance
Add an instruction: when querying the `customer` resource or when account-level context is useful, include `customer.id` and `customer.descriptive_name`.

## File to Modify

**`crates/mcc-gaql-gen/src/rag.rs`** — two locations:
- Lines 2618–2692: system prompt (with-cookbook variant)
- Lines 2700–2773: system prompt (without-cookbook variant)
- Line 3032–3091: `assemble_criteria` filter → WHERE clause conversion

## Changes

### Change 1: Fix IN/NOT IN clause formatting (line 3091, `assemble_criteria`)

Add explicit match arms for `"IN"` and `"NOT IN"` before the catch-all to skip quoting:

```rust
"IN" | "NOT IN" => format!("{} {} {}", ff.field_name, op, escaped_value),
_ => format!("{} {} '{}'", ff.field_name, op, escaped_value),
```

Note: `escaped_value` for IN already contains the parenthesized list (e.g., `('ENABLED', 'PAUSED')`). We must NOT escape inner quotes before building the IN clause — but `escaped_value` already has `\'` from line 3032. Two sub-options:
- Use `ff.value` directly for IN/NOT IN (no escaping), or
- Skip the escape step for IN/NOT IN.

Best fix: use `ff.value` (raw) for IN/NOT IN since the LLM is instructed to provide properly-quoted parens, and escape only applies to scalar string values.

```rust
"IN" | "NOT IN" => format!("{} {} {}", ff.field_name, op, ff.value),
_ => format!("{} {} '{}'", ff.field_name, op, escaped_value),
```

### Change 2: Strengthen date filter instruction (both system prompts)

Replace:
```
- Include segments.date if temporal period is specified
```
With:
```
- **MANDATORY: If the user query mentions any time period (last week, last 7 days, yesterday, this month, etc.), you MUST add a segments.date filter_field. Do NOT mention date ranges only in reasoning — they MUST appear in filter_fields. A query missing a date filter when the user specified a time period is INCORRECT.**
```

### Change 3: Add LIMIT instruction (both system prompts)

Add a `limit` field to the JSON schema example and instruct the LLM to populate it:

```json
{
  "select_fields": [...],
  "filter_fields": [...],
  "order_by_fields": [...],
  "limit": null,
  "reasoning": "..."
}
```

Add instruction:
```
- If the query asks for "top N", "first N", "best N", or "worst N" results, set "limit" to that number N. If "top" or "best" without a number, set "limit": 1.
```

Then parse the `limit` field from the LLM JSON response in `FieldSelectionResult` and use it as a fallback in `assemble_criteria` (LLM limit takes priority, `detect_limit_impl` as secondary).

This requires:
- Add `limit: Option<u32>` to `FieldSelectionResult` struct (line ~3237)
- Parse `limit` from JSON response after Phase 3 LLM call (lines ~2803–2927)
- In `assemble_criteria`, use `field_selection.limit.or_else(|| detect_limit_impl(user_query))`... but `assemble_criteria` doesn't have access to `field_selection`. Check call site.

Looking at the call site (line 3097): `assemble_criteria` takes `field_selection: &FieldSelectionResult`. So we can pass `field_selection.limit` directly in the limit detection step.

### Change 4: Add customer identifier guidance (both system prompts)

Add:
```
- When querying account-level data (FROM customer) or when the user asks for account context, always include customer.id and customer.descriptive_name in select_fields if they are available in the field list.
```

## Implementation Steps

1. Fix IN/NOT IN arm in `assemble_criteria` (line 3091)
2. Add `limit: Option<u32>` to `FieldSelectionResult` struct
3. Parse `limit` from Phase 3 JSON response
4. Update `assemble_criteria` to use `field_selection.limit` with `detect_limit_impl` as fallback
5. Update both system prompts (with-cookbook at 2618, without-cookbook at 2700):
   - Strengthen date filter mandatory instruction
   - Add `limit` to JSON schema + instruction
   - Add customer identifier guidance

## Verification

```bash
cargo check -p mcc-gaql-gen
cargo test -p mcc-gaql-gen --lib -- --test-threads=1
```

Then re-run the cookbook generation test to verify improvements.

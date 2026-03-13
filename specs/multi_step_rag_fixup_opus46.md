# Multi-Step RAG Pipeline — Post-Implementation Fix Plan

## Context

The 5-phase MultiStepRAGAgent was implemented per the v3 spec. A code review found 12 issues (3 critical, 4 high, 3 medium, 2 low) plus 5 missing tests. This plan addresses all issues in priority order.

---

## Fix 1: Add Phase 2 RAG vector searches (Critical — spec deviation)

**File:** `crates/mcc-gaql-gen/src/rag.rs` — `retrieve_field_candidates()` (~line 914)

**Problem:** Spec calls for 3 per-category RAG vector searches (top-30 attrs, top-30 metrics, top-15 segments) as Tier 2 alongside key_fields. Current implementation only uses key_fields — the `_user_query` param is unused. This means query-specific fields beyond pre-curated key fields are never discovered.

**Fix:**
1. Rename `_user_query` → `user_query`
2. After Tier 1 (key_fields), add Tier 2 RAG searches using `self.field_index`:
   - Search top-30, post-filter to attributes matching `name.starts_with("{primary}.")`, take top-10
   - Search top-30, post-filter to `is_metric()`, take top-10
   - Search top-15, post-filter to `is_segment()`, take top-5
3. Apply compatibility filter: metrics/segments must be in `selectable_with`
4. Dedup by field name via the existing `seen` HashSet
5. Use `tokio::join!` for the 3 RAG searches (parallel)

**Existing functions to reuse:**
- `self.field_index.top_n::<FieldDocumentFlat>(search_request)` — same pattern as `retrieve_cookbook_examples()`
- `FieldDocumentFlat` has `category` field for post-filtering

---

## Fix 2: Call `validate_field_selection_for_resource()` (Critical — dead code)

**File:** `crates/mcc-gaql-gen/src/rag.rs` — `generate()` (~line 815-821)

**Problem:** Hardcodes `ValidationResult { is_valid: true, errors: vec![], warnings: vec![] }`. The validation infrastructure in `field_metadata.rs:487-545` is never used.

**Fix:** Replace lines 817-821:
```rust
let all_fields: Vec<String> = field_selection.select_fields.iter()
    .chain(field_selection.filter_fields.iter().map(|f| &f.field_name))
    .cloned()
    .collect();
let validation = self.field_cache.validate_field_selection_for_resource(&all_fields, &primary_resource);
```

---

## Fix 3: Strip markdown fences from LLM JSON responses (Critical)

**File:** `crates/mcc-gaql-gen/src/rag.rs` — 3 sites; `crates/mcc-gaql-gen/src/enricher.rs` — 1 site

**Problem:** LLMs frequently wrap JSON in ` ```json ... ``` `. Parsing fails → fallbacks → degraded quality.

**Fix:** Apply `strip_markdown_code_blocks()` before `serde_json::from_str()` at:
- `rag.rs:870` (Phase 1 — `select_resource`)
- `rag.rs:1114` (Phase 3 — `select_fields`)
- `enricher.rs:528` (`select_key_fields_for_resource` — use `strip_json_fences()`)

---

## Fix 4: Fix `{{{{` brace escaping in Phase 3 prompt (High)

**File:** `crates/mcc-gaql-gen/src/rag.rs` (~line 1080-1099)

**Problem:** `format!()` with `{{{{` produces literal `{{` in output. LLM sees malformed JSON example.

**Fix:** The system prompt string has no interpolated variables — change from `format!(r#"..."#)` to a plain `let system_prompt = r#"..."#.to_string();` and use single `{` / `}` in the JSON example.

---

## Fix 5: Fix `prescan_filters` bare vs qualified name mismatch (High)

**File:** `crates/mcc-gaql-gen/src/rag.rs` — `prescan_filters()` (~line 1005)

**Problem:** `keyword_map` maps to `"status"` but candidates have `"campaign.status"`. `f.name == field_name` always fails.

**Fix:** Change line 1005 from:
```rust
if let Some(field) = candidates.iter().find(|f| f.name == field_name)
```
to:
```rust
if let Some(field) = candidates.iter().find(|f| f.name.ends_with(&format!(".{}", field_name)))
```

---

## Fix 6: Preserve ORDER BY direction (High)

**File:** `crates/mcc-gaql-gen/src/rag.rs`

**Problem:** Phase 3 asks for `{"field", "direction"}` but only `field` is extracted (line 1141-1148). Phase 5 emits ORDER BY without ASC/DESC.

**Fix:**
1. Change `FieldSelectionResult.order_by_fields` from `Vec<String>` to `Vec<(String, String)>`
2. In Phase 3 parsing (~1141), extract both `field` and `direction` (default `"DESC"`)
3. In Phase 5 (~1331-1335), emit `ORDER BY field direction`
4. Update `PipelineTrace.phase3_order_by_fields` to show direction

---

## Fix 7: Add fallback for empty SELECT (High)

**File:** `crates/mcc-gaql-gen/src/rag.rs` — `select_fields()` (~line 1117-1125) and `generate_gaql()` (~line 1298)

**Problem:** If all LLM fields fail validation, `select_fields` is empty → invalid GAQL `SELECT \nFROM ...`

**Fix:** After validation filtering in `select_fields()`, if `select_fields` is empty, fall back to `key_attributes` (first 3) + `key_metrics` (first 3) from `ResourceMetadata` for the primary resource. Log a warning.

---

## Fix 8: Validate key_attributes against resource scope (Medium)

**File:** `crates/mcc-gaql-gen/src/enricher.rs` — `select_key_fields_for_resource()` (~line 538, 550)

**Problem:** `cache.fields.contains_key(s)` checks global existence. LLM could return `"ad_group.name"` for resource `"campaign"`.

**Fix:** Filter `key_attributes` against `resource_attrs` (the input list) and `key_metrics` against `resource_metrics`:
```rust
.filter(|s| resource_attrs.contains(s))  // instead of cache.fields.contains_key(s)
// and
.filter(|s| resource_metrics.contains(s))  // instead of cache.fields.contains_key(s)
```

---

## Fix 9: Validate GAQL operator whitelist (Medium)

**File:** `crates/mcc-gaql-gen/src/rag.rs` — `assemble_criteria()` (~line 1193)

**Problem:** No validation of `ff.operator`. Single quotes in `ff.value` produce broken GAQL.

**Fix:**
```rust
const VALID_OPERATORS: &[&str] = &["=", "!=", "<", ">", "<=", ">=", "IN", "NOT IN", "LIKE", "NOT LIKE", "CONTAINS ANY", "CONTAINS ALL", "CONTAINS NONE", "IS NULL", "IS NOT NULL", "BETWEEN", "REGEXP_MATCH", "NOT REGEXP_MATCH"];

for ff in &field_selection.filter_fields {
    let op = ff.operator.to_uppercase();
    if !VALID_OPERATORS.contains(&op.as_str()) {
        log::warn!("Invalid operator '{}' for field '{}', skipping", ff.operator, ff.field_name);
        continue;
    }
    let escaped_value = ff.value.replace('\'', "\\'");
    let clause = format!("{} {} '{}'", ff.field_name, op, escaped_value);
    where_clauses.push(clause);
}
```

---

## Fix 10: Add missing unit tests (5 tests)

**File:** `crates/mcc-gaql-gen/src/rag.rs` — `#[cfg(test)] mod tests`

Add these tests (all pure functions, no LLM mocks needed):

1. **`test_prescan_filters_detects_status`** — Create candidates with `campaign.status` having enum_values, verify prescan_filters returns matching enums when query contains "enabled"
2. **`test_detect_temporal_period`** — Test "last 7 days" → LAST_7_DAYS, "last 30 days" → LAST_30_DAYS, "yesterday" → YESTERDAY, no match → None
3. **`test_detect_limit`** — Test "top 10" → Some(10), "first 5" → Some(5), "best 3" → Some(3), no match → None
4. **`test_implicit_defaults_on_off`** — Test campaign gets status=ENABLED when no explicit filter, no status added when filter exists, no status added for non-status resources
5. **`test_generate_gaql_assembles_correctly`** — Test SELECT/FROM/WHERE/ORDER BY/LIMIT/DURING assembly

Note: `prescan_filters`, `detect_temporal_period`, `detect_limit`, `get_implicit_defaults` take `&self` — tests need a minimal `MultiStepRAGAgent`. Consider extracting these as free functions or `impl` methods on a smaller struct to make them more testable.

**Alternative:** Extract `detect_temporal_period`, `detect_limit`, and `get_implicit_defaults` as module-level functions (they don't use `self` fields) to simplify testing.

---

## Fix 11 (Low): Stage ordering in enricher

**File:** `crates/mcc-gaql-gen/src/enricher.rs`

**Problem:** Stage 2 resource description enrichment reads `key_attributes`/`key_metrics` before Stage 3 populates them.

**Fix:** Swap stage order: run key field selection before resource description enrichment. Or accept that first-run descriptions won't have key field context (low impact since re-enrichment fixes it).

---

## Critical Files

| File | Fixes |
|------|-------|
| `crates/mcc-gaql-gen/src/rag.rs` | Fixes 1-7, 9, 10 |
| `crates/mcc-gaql-gen/src/enricher.rs` | Fixes 3, 8, 11 |

---

## Verification

1. `cargo check --workspace` — no errors
2. `cargo clippy --workspace` — no new warnings
3. `cargo test --workspace` — all existing + 5 new tests pass
4. Manual test: `cargo run -p mcc-gaql-gen -- generate "show me campaign performance last 30 days" --metadata ... -v`
   - Verify: RAG candidates appear in Phase 2 trace (not just key_fields)
   - Verify: validation section shows actual results (not hardcoded is_valid: true)
   - Verify: ORDER BY has DESC direction
   - Verify: prescan_filters detects "campaign" status fields

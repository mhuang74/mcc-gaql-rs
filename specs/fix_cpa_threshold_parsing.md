# Fix CPA/Monetary Threshold Parsing in GAQL Generation

**Date:** 2026-03-27
**Issue:** Query `search_terms_with_top_cpa` classified as POOR - CPA filter shows `> 0` instead of `> 200000000`
**Report Reference:** `reports/query_cookbook_gen_comparison.20260327214418.md` (Entry #19)

---

## Context

The `mcc-gaql-gen generate` command fails to correctly parse monetary thresholds like "CPA > $200" in user queries. For the `search_terms_with_top_cpa` entry, the system generates:

```sql
AND metrics.cost_per_conversion > 0
```

Instead of the expected:

```sql
AND metrics.cost_per_conversion > 200000000
```

---

## Root Cause Analysis

1. **Prompt Already Contains Instructions:** The LLM prompt (lines 2865-2866, 3002-3004 in `rag.rs`) includes micros conversion examples:
   - `"CPA >$200" → field: "metrics.cost_per_conversion", operator: ">", value: "200000000"`

2. **Programmatic Conversion Exists:** The `try_convert_to_micros` function (lines 3700-3731) handles dollar-to-micros conversion as a fallback.

3. **The Actual Problem:** The LLM sometimes hallucinates `"0"` instead of extracting the numeric value `"200"` from the query text.

4. **Why Post-Processing Fails:** When the LLM returns `"0"`, the programmatic conversion correctly computes `0 * 1,000,000 = 0`, resulting in the incorrect filter.

---

## Proposed Solution: Prompt Enhancement + Validation

Strengthen the LLM instructions and add validation to detect and correct zero thresholds when the query contains non-zero monetary values.

---

## Changes

### Change 1: Strengthen Micros Conversion Instructions

**File:** `crates/mcc-gaql-gen/src/rag.rs`

**Location A (cookbook variant, ~line 2863):** Replace existing micros instructions with:

```text
- **CRITICAL - Monetary Value Extraction:** For monetary thresholds, you MUST extract the EXACT numeric value from the user query.
  - "CPA >$200" → extract "200", then convert to micros → value: "200000000"
  - "spend >$1K" → extract "1000" (K = 1000), then convert to micros → value: "1000000000"
  - "cost > $50.50" → extract "50.50", then convert to micros → value: "50500000"
- **NEVER** return "0" for a threshold unless the user explicitly said "0" or "$0"
- **All monetary fields** (_micros, cost_per_*, value_per_*): values must be in micros (1 dollar = 1,000,000 micros)
- Fields requiring micros conversion: metrics.cost_micros, campaign_budget.amount_micros, metrics.cost_per_conversion, metrics.cost_per_all_conversions, metrics.value_per_conversion, metrics.all_conversions_value
- **Validation check:** If the user said ">$200" and you're returning "0", you made an error. The value should be "200000000".
```

**Location B (non-cookbook variant, ~line 3000):** Apply identical changes.

### Change 2: Add Post-Processing Validation

**File:** `crates/mcc-gaql-gen/src/rag.rs`

**Location:** After line 3161 (after the programmatic micros conversion loop)

**Add validation function:**

```rust
/// Validation: Detect zero thresholds when query contains monetary patterns
fn validate_monetary_thresholds(
    filter_fields: &mut Vec<FilterField>,
    user_query: &str,
) {
    lazy_static::lazy_static! {
        static ref MONETARY_PATTERN: regex::Regex = regex::Regex::new(
            r"(?i)(?:cost|cpa|spend|budget|amount|value|revenue).*?\$?\d+(?:\.\d+)?(?:\s*[KkMmBb])?|\$\d+(?:\.\d+)?(?:\s*[KkMmBb])?"
        ).unwrap();
    }

    let has_monetary_threshold = MONETARY_PATTERN.is_match(user_query);

    for ff in filter_fields.iter_mut() {
        let is_monetary_field = ff.field_name.ends_with("_micros")
            || ff.field_name.contains("cost_per_")
            || ff.field_name.contains("value_per_")
            || ff.field_name.contains("amount_micros");

        if is_monetary_field && ff.value == "0" && has_monetary_threshold {
            log::warn!(
                "Phase 3: Suspicious zero threshold for '{}' when query contains monetary pattern",
                ff.field_name
            );
            if let Some(extracted) = extract_threshold_from_query(user_query, &ff.field_name) {
                log::info!(
                    "Phase 3: Correcting zero threshold for '{}' to '{}'",
                    ff.field_name, extracted
                );
                ff.value = extracted;
            }
        }
    }
}

/// Extract monetary threshold from query based on field context
fn extract_threshold_from_query(query: &str, field_name: &str) -> Option<String> {
    let query_lower = query.to_lowercase();

    let pattern_str = if field_name.contains("cost_per_conversion") {
        r"(?:cpa|cost per conversion).*?\$?(\d+(?:\.\d+)?)(?:\s*[KkMmBb])?"
    } else if field_name.ends_with("_micros") || field_name.contains("cost") {
        r"(?:spend|cost|budget|amount).*?\$?(\d+(?:\.\d+)?)(?:\s*[KkMmBb])?|\$(\d+(?:\.\d+)?)(?:\s*[KkMmBb])?"
    } else {
        r"\$(\d+(?:\.\d+)?)(?:\s*[KkMmBb])?"
    };

    let pattern = regex::Regex::new(pattern_str).ok()?;

    if let Some(caps) = pattern.captures(&query_lower) {
        let amount_str = caps.get(1).or(caps.get(2))?.as_str();
        let full_match = caps.get(0)?.as_str();

        let multiplier = if full_match.ends_with('k') || full_match.ends_with('K') {
            1_000.0
        } else if full_match.ends_with('m') || full_match.ends_with('M') {
            1_000_000.0
        } else if full_match.ends_with('b') || full_match.ends_with('B') {
            1_000_000_000.0
        } else {
            1.0
        };

        let amount: f64 = amount_str.parse().ok()?;
        let dollars = amount * multiplier;
        let micros = (dollars * 1_000_000.0) as i64;
        return Some(micros.to_string());
    }

    None
}
```

**Integration:** Call `validate_monetary_thresholds(&mut filter_fields, user_query)` after the micros conversion loop at line 3161.

### Change 3: Verify Dependencies

**File:** `crates/mcc-gaql-gen/Cargo.toml`

Ensure dependencies exist:
```toml
[dependencies]
regex = "1.10"
lazy_static = "1.4"
```

### Change 4: Add Unit Tests

**File:** `crates/mcc-gaql-gen/src/rag.rs` (test module)

Add tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_threshold_from_query_cpa() {
        assert_eq!(
            extract_threshold_from_query("CPA >$200", "metrics.cost_per_conversion"),
            Some("200000000".to_string())
        );
        assert_eq!(
            extract_threshold_from_query("cost per conversion over $150", "metrics.cost_per_conversion"),
            Some("150000000".to_string())
        );
        assert_eq!(
            extract_threshold_from_query("CPA greater than $1K", "metrics.cost_per_conversion"),
            Some("1000000000".to_string())
        );
    }

    #[test]
    fn test_extract_threshold_from_query_spend() {
        assert_eq!(
            extract_threshold_from_query("spend > $1K", "metrics.cost_micros"),
            Some("1000000000".to_string())
        );
        assert_eq!(
            extract_threshold_from_query("cost over $500", "metrics.cost_micros"),
            Some("500000000".to_string())
        );
    }

    #[test]
    fn test_validate_monetary_thresholds() {
        let user_query = "search terms with CPA >$200 and spend >$1K";

        let mut filter_fields = vec![
            FilterField {
                field_name: "metrics.cost_per_conversion".to_string(),
                operator: ">".to_string(),
                value: "0".to_string(), // LLM hallucinated
            },
            FilterField {
                field_name: "metrics.cost_micros".to_string(),
                operator: ">".to_string(),
                value: "1000000000".to_string(), // Correct
            },
        ];

        validate_monetary_thresholds(&mut filter_fields, user_query);

        assert_eq!(filter_fields[0].value, "200000000"); // Corrected
        assert_eq!(filter_fields[1].value, "1000000000"); // Unchanged
    }

    #[test]
    fn test_validate_monetary_thresholds_no_false_positives() {
        let user_query = "campaigns with no conversions";

        let mut filter_fields = vec![
            FilterField {
                field_name: "metrics.conversions".to_string(),
                operator: "=".to_string(),
                value: "0".to_string(),
            },
        ];

        validate_monetary_thresholds(&mut filter_fields, user_query);

        assert_eq!(filter_fields[0].value, "0"); // Preserved
    }
}
```

---

## Verification Plan

1. **Compile:**
   ```bash
   cargo check -p mcc-gaql-gen
   cargo build -p mcc-gaql-gen --release
   ```

2. **Unit Tests:**
   ```bash
   cargo test -p mcc-gaql-gen extract_threshold
   cargo test -p mcc-gaql-gen validate_monetary
   cargo test -p mcc-gaql-gen -- --test-threads=1
   ```

3. **Integration Test:**
   ```bash
   mcc-gaql-gen generate "search terms with CPA >$200 and spend >$1K" --explain
   ```
   Verify filters:
   - `metrics.cost_per_conversion > 200000000`
   - `metrics.cost_micros > 1000000000`

4. **Cookbook Entry Test:**
   ```bash
   mcc-gaql-gen generate "Get me performance data for top 50 search terms by spend with CPA >$200 and spend >$1K last 30 days" --use-query-cookbook --explain
   ```
   Verify classification improves from POOR to GOOD/EXCELLENT.

---

## Affected Files

- `crates/mcc-gaql-gen/src/rag.rs` - Prompt enhancement, validation functions, unit tests
- `crates/mcc-gaql-gen/Cargo.toml` - Verify regex dependency

---

## Related

- Existing spec: `specs/fix_gaql_gen_report_20260327.md` (documents this as Failure Mode 2)
- Cookbook entry: `resources/query_cookbook.toml` [search_terms_with_top_cpa]

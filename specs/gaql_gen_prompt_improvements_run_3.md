# GAQL Generation Prompt Improvements — Run 3

## Context

After the candidate injection fix (commit 7fbb138), a re-evaluation of 26 reference queries (`reports/query_cookbook_gen_comparison.20260326071507.md`) shows 69% EXCELLENT/GOOD, but 4 remaining systemic issues still cause 31% FAIR/POOR results. This spec addresses those issues.

All changes target `crates/mcc-gaql-gen/src/rag.rs`.

---

## Issue #2: Candidate Gate Too Restrictive — Valid Fields Rejected

**Affected queries:** campaigns_with_smart_bidding_by_spend (GOOD), campaigns_shopping_campaign_performance (GOOD), smart_campaign_search_terms_with_top_spend (FAIR), search_terms_with_top_cpa (FAIR), search_terms_with_low_roas (FAIR), and others

**Symptom:** Fields like `customer.id`, `customer.descriptive_name`, `metrics.clicks`, `metrics.cost_micros` etc. are rejected as "not in candidates" even though they are valid, selectable fields for the primary resource.

**Root cause:** The Phase 3 validation (lines 2804-2813, 2856-2866, 2896-2902) gates LLM-selected fields against `candidate_names` — only fields that were retrieved as candidates in Phase 2. But the candidate set is a *subset* of valid fields (limited by vector search sample sizes, keyword matching, and curated lists). The LLM can correctly identify a field the user needs, but if Phase 2 didn't happen to surface it as a candidate, it gets silently dropped.

The candidate set is meant to **guide** the LLM toward relevant fields, not to **restrict** output to only those fields. Any field in the resource's `selectable_with` list is a valid GAQL field.

### Fix: Relax candidate gate to accept any field in `selectable_with`

Change the Phase 3 validation for `select_fields`, `filter_fields`, and `order_by_fields` to accept a field if it is either:
1. In the candidate set (as before), OR
2. In the resource's `selectable_with` list (valid for the primary resource + auto-joined resources)

#### A. Build a valid field set (before Phase 3 validation, around line 2799)

```rust
// Build set of all valid fields for this resource (selectable_with).
// The candidate set guides the LLM, but we should accept any valid field it selects.
let valid_fields: HashSet<String> = selectable_with.iter().cloned().collect();
```

#### B. Change select_fields validation (lines 2804-2813)

Change:
```rust
if candidate_names.contains(s) {
    true
} else {
    log::debug!("Phase 3: Rejecting select field '{}' - not in candidates", s);
    false
}
```
To:
```rust
if candidate_names.contains(s) || valid_fields.contains(s) {
    if !candidate_names.contains(s) {
        log::info!("Phase 3: Accepting select field '{}' - not in candidates but valid for resource", s);
    }
    true
} else {
    log::warn!("Phase 3: Rejecting select field '{}' - not valid for resource '{}'", s, primary);
    false
}
```

#### C. Apply same change to filter_fields validation (lines 2856-2866)

Same pattern: accept if in `candidate_names` OR `valid_fields`.

#### D. Apply same change to order_by_fields validation (lines 2896-2902)

Same pattern: accept if in `candidate_names` OR `valid_fields`.

#### E. Prompt instruction (both Phase 3 system prompts)

Add after the `order_by_fields` instruction:
```
- In an MCC (multi-client) environment, always include customer.id and customer.descriptive_name
  in select_fields when they are available, so results can be identified by account.
```

This prompt instruction nudges the LLM to include account identifiers, and the relaxed gate ensures they won't be rejected even if they weren't in the candidate set.

---

## Issue #3: Numeric Threshold Parsing — "$200" Becomes "> 0"

**Affected query:** search_terms_with_top_cpa (FAIR) — `metrics.cost_per_conversion > 0` instead of `> 200000000`

**Symptom:** Dollar amounts in user queries are not converted to Google Ads micros values.

**Root cause:** The Phase 3 system prompt has no instructions about micros conversion. The LLM must independently know that `$200` should become `200000000`, but has no guidance.

### Fix

Add to both Phase 3 system prompts (after the `order_by_fields` instruction):

```
- **Monetary values (micros conversion):** Fields ending in `_micros` (e.g., metrics.cost_micros,
  campaign_budget.amount_micros) and metrics.cost_per_conversion store currency in micros
  (1 dollar = 1,000,000 micros). Convert dollar amounts in filters:
  - "$200" or "200 dollars" → 200000000
  - "$1K" or "$1,000" → 1000000000
  - "$1.50" → 1500000
  - "$0.50" or "50 cents" → 500000
  Always multiply dollar values by 1,000,000 for these fields.
```

---

## Issue #4: LIMIT Handling — LLM Limit Value Ignored

**Affected queries:** Multiple entries where "top N" requests get wrong or missing LIMIT.

**Symptom:** The LLM outputs a `limit` field in its Phase 3 JSON response, but the code discards it. `assemble_criteria` only uses `detect_limit_impl` (regex on user query), which requires an explicit number immediately after "top " — "top campaigns" yields `None`.

**Root cause:** `FieldSelectionResult` (line 3229) has no `limit` field. The LLM's parsed limit is never stored or used. The `detect_limit_impl` fallback at line 3088 is the only limit source.

### Fix

#### A. Add limit to FieldSelectionResult (line 3229)

```rust
struct FieldSelectionResult {
    select_fields: Vec<String>,
    filter_fields: Vec<mcc_gaql_common::field_metadata::FilterField>,
    order_by_fields: Vec<(String, String)>,
    limit: Option<u32>,  // NEW
    reasoning: String,
}
```

#### B. Add limit to Phase 3 JSON schema (both prompt variants)

Change JSON schema to include `"limit": null` and add instruction:
```
- Set "limit" to a number if the user wants a limited result set (e.g., "top 10" → 10,
  "top campaigns" without a number → 10, "best 5" → 5). Set to null if no limit is implied.
```

#### C. Parse limit from LLM response (after line ~2918)

```rust
let llm_limit: Option<u32> = parsed["limit"]
    .as_u64()
    .and_then(|n| u32::try_from(n).ok());
```

Include in `FieldSelectionResult` construction:
```rust
Ok(FieldSelectionResult {
    select_fields: final_select_fields,
    filter_fields,
    order_by_fields,
    limit: llm_limit,
    reasoning,
})
```

#### D. Use LLM limit in assemble_criteria (line 3088)

Change:
```rust
let limit = self.detect_limit(user_query);
```
To:
```rust
let limit = field_selection.limit.or_else(|| self.detect_limit(user_query));
```

This gives the LLM's semantic understanding priority, with regex detection as fallback.

---

## Issue #5: Missing Impression Share Metrics / Wrong Resource

**Affected query:** performance_max_impression_share (POOR) — LLM chose `performance_max_placement_view` instead of `campaign`, and all 8 impression share metrics were missing from candidates.

**Symptom:** The generated query was `SELECT segments.date, metrics.impressions FROM performance_max_placement_view` instead of `SELECT ... metrics.search_absolute_top_impression_share, ... FROM campaign`.

**Root cause (two-part):**
1. **Phase 1 resource selection:** The LLM chose `performance_max_placement_view` because the query mentions "PMax" and "impression" — the view name sounds more relevant than generic `campaign`.
2. **Phase 2 metric candidates:** Even for `campaign`, the 30-sample metric vector search can miss impression share metrics because all 8+ are semantically similar and compete for slots.

### Fix

#### A. Phase 1 resource selection guidance

Add to the Phase 1 system prompt (after the JSON schema, before the resource list):
```
Resource selection tips:
- For impression share metrics (search impression share, budget lost impression share, etc.),
  use the `campaign` resource. Specialized views like `performance_max_placement_view` are for
  placement-level data and do NOT expose impression share metrics.
- When in doubt between a specialized view and a core resource (campaign, ad_group, customer),
  prefer the core resource — it has broader metric availability.
```

#### B. Phase 2 impression share metric injection (retrieve_field_candidates, after customer.id injection)

```rust
if query_lower.contains("impression share") {
    let impression_share_metrics = [
        "metrics.search_absolute_top_impression_share",
        "metrics.search_budget_lost_absolute_top_impression_share",
        "metrics.search_budget_lost_impression_share",
        "metrics.search_budget_lost_top_impression_share",
        "metrics.search_exact_match_impression_share",
        "metrics.search_impression_share",
        "metrics.search_rank_lost_impression_share",
        "metrics.search_top_impression_share",
    ];
    for field_name in &impression_share_metrics {
        if selectable_with.contains(&field_name.to_string()) {
            if let Some(field) = self.field_cache.fields.get(*field_name) {
                if seen.insert(field_name.to_string()) {
                    candidates.push(field.clone());
                    log::debug!("Phase 2: Force-injected {} for impression share query", field_name);
                }
            }
        }
    }
}
```

---

## Implementation Order

1. **Issue #2** (relax candidate gate) — foundational fix that unblocks field availability for all other issues
2. **Issue #4** (limit) — purely additive struct + parsing changes
3. **Issue #3** (micros prompt) — text-only prompt addition
4. **Issue #5** (impression share) — Phase 1 prompt + Phase 2 injection

## Verification

Run the full 26-query cookbook comparison test:
```bash
cargo run -p mcc-gaql-gen --release -- gen --test-run
```

Check the comparison report for:
- Issue #2: `customer.id` and `customer.descriptive_name` present in previously-missing queries; no valid fields rejected
- Issue #3: `search_terms_with_top_cpa` produces `cost_per_conversion > 200000000` not `> 0`
- Issue #4: LIMIT values match reference queries (LIMIT 25, LIMIT 1, etc.)
- Issue #5: `performance_max_impression_share` uses `campaign` resource with all impression share metrics

## Files to Modify

- `crates/mcc-gaql-gen/src/rag.rs`:
  - Phase 3 field validation — select_fields (line 2804), filter_fields (line 2856), order_by_fields (line 2896)
  - Phase 3 system prompts — both cookbook (line 2613) and non-cookbook (line 2693) variants
  - Phase 1 system prompt — resource selection guidance (~line 1945)
  - `FieldSelectionResult` struct (line 3229)
  - Phase 3 response parsing (~line 2920)
  - `assemble_criteria` (line 3088)

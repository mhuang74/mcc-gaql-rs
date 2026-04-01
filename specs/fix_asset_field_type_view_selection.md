# Fix: Asset Field Type View Selection Bug

## Problem Summary

When users request asset extension details (sitelink text, callout text, phone numbers), the LLM incorrectly selects `asset_field_type_view` instead of `campaign_asset`. This happens because:

1. `asset_field_type_view` semantically matches "sitelink" queries (has `field_type='SITELINK'` filter)
2. But it CANNOT access `asset.*` fields (selectable_with excludes 'asset')
3. Domain knowledge guidance exists but lacks strong negative constraints

## Files to Modify

| File | Purpose |
|------|---------|
| `resources/domain_knowledge.md` | Add explicit NEVER use constraints and trigger patterns |
| `crates/mcc-gaql-gen/src/rag.rs` | Add post-selection validation for asset detail requests |

## Implementation Plan

### Phase 1: Domain Knowledge Updates (High Priority)

**File:** `resources/domain_knowledge.md`

**Location:** Replace the existing "For asset extension performance" bullet point in "## Resource Selection Guidance" section (lines 5-10).

**Current text:**
```markdown
- For asset extension performance (sitelinks, callouts, calls, structured snippets):
  **primary_resource must be `campaign_asset`** with a `campaign_asset.field_type` filter.
  Do NOT use `campaign` as primary (it cannot access asset-level fields like `asset.call_asset.phone_number`).
  Do NOT use `call_view` (individual call records, not asset metrics).
  Do NOT put `campaign_asset` in related_resources under `campaign` — it must be the primary_resource.
```

**New text:**
```markdown
- For asset extension performance (sitelinks, callouts, calls, structured snippets):
  **primary_resource must be `campaign_asset`** (or `ad_group_asset`) with a `field_type` filter.
  
  **TRIGGER PATTERNS requiring campaign_asset/ad_group_asset:**
  - User says "include [sitelink/callout/call/etc.] text" or "include the text"
  - User says "show me the [extension] details/content"
  - User says "with phone number" or "with link text"
  - User asks for ANY asset.* field (asset.sitelink_asset.*, asset.callout_asset.*, etc.)
  
  **NEVER use these resources for asset detail queries:**
  - `asset_field_type_view` - provides aggregate metrics by asset type only, CANNOT access asset.* fields
  - `asset` - static entity definition with no metrics support
  - `campaign` - cannot access asset-level fields
  - `call_view` - individual call records, not asset extension metrics
  
  **When to use asset_field_type_view (rare):**
  ONLY when user wants aggregate performance metrics BY asset type with NO individual asset details.
  Example: "Show me daily metrics broken down by asset type" (no text/content requested)
```

### Phase 2: Post-Selection Validation (Medium Priority)

**File:** `crates/mcc-gaql-gen/src/rag.rs`

**Location:** After the dropped resource promotion block (around line 2185), add asset detail validation.

**Function:** `select_resource()` - add validation before returning

**Pseudocode:**
```rust
// --- Asset detail query validation ---
// If query requests asset content AND selected resource cannot access asset table,
// override to campaign_asset (or ad_group_asset if ad_group context detected).
let (primary, validated_related) = {
    let asset_detail_patterns = [
        "include sitelink", "include callout", "include call",
        "include text", "include the text", "sitelink text", 
        "callout text", "phone number", "link text",
        "show me the sitelink", "show me the callout",
        "with extension details", "asset content",
    ];
    
    let query_lower = user_query.to_lowercase();
    let needs_asset_details = asset_detail_patterns.iter()
        .any(|p| query_lower.contains(p));
    
    if needs_asset_details {
        let primary_selectable = self.field_cache.get_resource_selectable_with(&primary);
        let has_asset_access = primary_selectable.contains(&"asset".to_string());
        
        if !has_asset_access {
            // Determine correct resource based on context
            let new_primary = if query_lower.contains("ad group") || query_lower.contains("ad_group") {
                "ad_group_asset"
            } else {
                "campaign_asset"
            };
            
            log::warn!(
                "Phase 1: Overriding primary '{}' to '{}' - query requests asset details \
                 but '{}' cannot access asset.* fields",
                primary, new_primary, primary
            );
            
            // Rebuild related resources for new primary
            let new_selectable = self.field_cache.get_resource_selectable_with(new_primary);
            let new_related: Vec<String> = validated_related
                .into_iter()
                .filter(|r| new_selectable.contains(r))
                .collect();
            
            (new_primary.to_string(), new_related)
        } else {
            (primary, validated_related)
        }
    } else {
        (primary, validated_related)
    }
};
```

**Insert location:** After line ~2184 (after dropped resource promotion block), before the final `Ok(...)` return.

### Phase 3: Update Reasoning for Override (Enhancement)

When the override happens, update the reasoning to explain why:

```rust
let reasoning = if overridden {
    format!(
        "{} [OVERRIDE: Query requests asset details ('{}' pattern detected), \
         but '{}' cannot access asset.* fields. Changed to '{}' which can.]",
        original_reasoning, matched_pattern, original_primary, new_primary
    )
} else {
    reasoning
};
```

## Testing Strategy

### Primary Test Query
```
accounts_with_asset_sitelink_last_week
```
**Description:** Get me the volume and spend metrics (impressions, clicks, cost) of top Sitelink Extensions for each campaign by clicks (>20K) last week - need acct and campaign info with currency. include sitelink text.

**Expected Result:**
- Primary resource: `campaign_asset`
- asset.* fields accessible (specifically `asset.sitelink_asset.link_text`)
- NO selection of `asset_field_type_view`

### Similar Queries at Risk

| Query ID | Description | Expected Primary |
|----------|-------------|------------------|
| accounts_with_asset_callout_last_week | "include callout text" | campaign_asset |
| accounts_with_asset_call_last_week | "include phone number" | campaign_asset |
| accounts_with_asset_app_last_week | App extensions (no text) | campaign_asset |

### Additional Test Cases

1. **Positive (should use campaign_asset):**
   - "Get sitelink performance with the link text"
   - "Show callout clicks including the callout text"
   - "List call extensions with phone numbers and impressions"

2. **Negative (can use asset_field_type_view):**
   - "Show me daily metrics broken down by asset type"
   - "YTD asset volume by field type"

### Verification Commands

```bash
# Run single query test with explanation
cargo run -p mcc-gaql-gen --release -- \
  "Get me the volume and spend metrics (impressions, clicks, cost) of top Sitelink Extensions for each campaign by clicks (>20K) last week - need acct and campaign info with currency. include sitelink text." \
  --explain

# Verify output contains:
# - "FROM campaign_asset"
# - "asset.sitelink_asset.link_text" in SELECT
# - NOT "FROM asset_field_type_view"

# Run comparison report
cargo run -p mcc-gaql-gen --release -- test-run \
  --input resources/query_cookbook.toml \
  --output reports/query_cookbook_gen_comparison.$(date +%Y%m%d%H%M%S).md
```

### Success Criteria

1. **accounts_with_asset_sitelink_last_week**: GOOD or EXCELLENT (was POOR)
2. **accounts_with_asset_callout_last_week**: GOOD or EXCELLENT (if POOR)
3. **accounts_with_asset_call_last_week**: GOOD or EXCELLENT
4. No regression on aggregate asset type queries (accounts_with_asset_ytd_by_day)

## Rollback Plan

### If Domain Knowledge Change Causes Issues
1. Revert changes to `resources/domain_knowledge.md`
2. Run `mcc-gaql-gen bundle create` to regenerate bundle (if distributed)

### If Code Change Causes Issues
1. `git revert <commit-hash>` for the rag.rs change
2. Rebuild: `cargo build -p mcc-gaql-gen --release`

### Rollback Verification
```bash
# Ensure original behavior restored
cargo test -p mcc-gaql-gen -- --test-threads=1
```

## Implementation Order

1. **Phase 1: Domain Knowledge** (5 min)
   - Update `resources/domain_knowledge.md` with stronger constraints
   - Test manually with --explain flag

2. **Phase 2: Post-Selection Validation** (15 min)
   - Add validation code to `rag.rs`
   - Test with asset detail queries
   - Verify no regression on aggregate queries

3. **Phase 3: Full Test Suite** (10 min)
   - Run test-run with full cookbook
   - Generate comparison report
   - Verify improvements

## Risk Assessment

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Over-correction (always selects campaign_asset) | Low | Pattern matching is specific; aggregate queries won't trigger |
| Regression on existing queries | Low | Validation only triggers for specific patterns |
| Performance impact | Negligible | String pattern matching is O(n) where n is small |

## Related Specs
- `specs/fix_dropped_related_resource_promotion.md` - Related promotion logic
- `specs/extract-domain-knowledge.md` - Domain knowledge design

# Implementation Spec: Fix FAIR Results Missing Fields

## Problem Statement

Analysis of the latest 47-query cookbook generation run shows:
- 26 EXCELLENT (55.3%)
- 1 GOOD (2.1%)
- **20 FAIR (42.6%)** ← Target for improvement
- 0 POOR (0.0%)

The 20 FAIR classifications share common missing field patterns that can be systematically addressed.

## Root Cause Analysis

### Top Missing Fields (by frequency in FAIR results)

| Field | Missing Count | % of FAIR | Primary Cause |
|-------|---------------|-----------|---------------|
| `customer.currency_code` | 7 | 35% | LLM misses implicit requirement from "with currency" phrase |
| `campaign.advertising_channel_type` | 6 | 30% | Field used in WHERE but omitted from SELECT |
| `metrics.conversions` | 4 | 20% | Metrics selection inconsistency |
| `metrics.cost_micros` | 3 | 15% | Core metric omission |
| `ad_group.type` | 2 | 10% | Resource context field missing |
| `change_event.*` | 4 | 20% | Complex resource field selection |

### Pattern Analysis

1. **Currency Code Pattern (35% impact)**
   - Trigger phrases: "need currency", "with currency", "and currency"
   - Affected queries: campaign_budgets_with_spend, keywords_with_performance, pmax_asset_groups_performance, rsa_asset_level_performance, rsa_assets_detail, search_terms_with_zero_conversions

2. **Advertising Channel Type Pattern (30% impact)**
   - Trigger: WHERE clause filters by advertising_channel_type
   - LLM behavior: Filters correctly but doesn't include in SELECT for visibility
   - Affected queries: accounts_with_asset_*, asset_performance_rsa, campaigns_with_*, search_terms_for_intent_clustering

3. **Metrics Completeness Pattern (25% impact)**
   - Trigger phrases: "conv metrics", "performance metrics", "full metrics"
   - LLM behavior: Selects some but not all relevant metrics
   - Missing: conversions, cost_micros, cost_per_conversion, average_cpc vs average_cost

## Solution Design

### Approach: Hybrid (Domain Knowledge + Prompt Enhancement)

Three complementary fixes targeting the highest-impact patterns:

#### Fix 1: Currency Code Pattern Detection (Domain Knowledge)
**Priority**: HIGH (35% improvement potential)
**Implementation**: Add explicit rule to domain_knowledge.md

```
PATTERN: Currency Code Requirement
When user query contains phrases like:
  - "with currency"
  - "need currency"
  - "and currency"
  - "currency code"
  
THEN: MUST include customer.currency_code in SELECT fields
```

#### Fix 2: Advertising Channel Type Visibility (Domain Knowledge)
**Priority**: HIGH (30% improvement potential)
**Implementation**: Add explicit rule to domain_knowledge.md

```
PATTERN: Advertising Channel Type Visibility
When WHERE clause contains:
  - campaign.advertising_channel_type
  
THEN: MUST also include campaign.advertising_channel_type in SELECT fields
RATIONALE: Users filtering by channel type need to see which type each row represents
```

#### Fix 3: Metrics Completeness Enhancement (Prompt)
**Priority**: MEDIUM (25% improvement potential)
**Implementation**: Enhance enricher.rs prompt

Add to Phase 0 enrichment prompt:
```
When selecting metrics fields, include ALL of the following when relevant:
- Base volume: metrics.impressions, metrics.clicks
- Cost: metrics.cost_micros, metrics.average_cpc, metrics.average_cost
- Conversions: metrics.conversions, metrics.conversions_value, metrics.all_conversions
- Efficiency: metrics.cost_per_conversion, metrics.conversions_value_per_cost
```

## Implementation Plan

### Phase 1: Domain Knowledge Updates
**File**: `resources/domain_knowledge.md`

Add new section at end of file:
```markdown
## Pattern-Based Field Requirements

### Currency Code Pattern
When the user request mentions currency (phrases: "with currency", "need currency", "and currency", "currency code"), ALWAYS include `customer.currency_code` in the SELECT fields.

### Advertising Channel Type Visibility Pattern
When filtering by `campaign.advertising_channel_type` in the WHERE clause, ALWAYS include `campaign.advertising_channel_type` in the SELECT fields for visibility.

### Asset Identification Pattern
When the request asks for asset content (phrases: "include [X] text", "show me the [X] content", "with [extension] details"), ALWAYS include:
- asset.id
- asset.name
- asset.type
```

### Phase 2: Enricher Prompt Enhancement
**File**: `crates/mcc-gaql-gen/src/enricher.rs`

Locate the LLM prompt around line 639 (Phase 0 enrichment).

Add after existing key_attributes instructions:
```rust
// Add to the system prompt for metric field selection:
"When selecting metrics fields for conversion-related analysis, prefer comprehensive coverage:
- Volume: metrics.impressions, metrics.clicks
- Cost: metrics.cost_micros, metrics.average_cpc, metrics.average_cost  
- Conversions: metrics.conversions, metrics.conversions_value
- Efficiency: metrics.cost_per_conversion, metrics.conversions_value_per_cost
- Include BOTH metrics.conversions AND metrics.all_conversions variants when available"
```

### Phase 3: Testing

**Test Commands**:
```bash
# Test currency code fix
cargo run --bin mcc-gaql-gen -- generate --cookbook-entry campaign_budgets_with_spend --explain --no-defaults

# Test advertising_channel_type fix  
cargo run --bin mcc-gaql-gen -- generate --cookbook-entry campaigns_with_smart_bidding_by_spend --explain --no-defaults

# Test asset identification fix
cargo run --bin mcc-gaql-gen -- generate --cookbook-entry rsa_assets_detail --explain --no-defaults
```

**Expected Results**:
- currency_code: Should appear in SELECT when description mentions currency
- advertising_channel_type: Should appear in SELECT when used in WHERE
- asset.id/name/type: Should appear when requesting asset content

## Files to Modify

1. `resources/domain_knowledge.md` - Add Pattern-Based Field Requirements section
2. `crates/mcc-gaql-gen/src/enricher.rs` - Enhance Phase 0 enrichment prompt

## Rollback Plan

1. Keep backup of original `domain_knowledge.md`
2. Changes are additive only (new sections), easy to remove
3. Enricher prompt changes are text additions, can be reverted with git

## Success Metrics

After implementation, re-run full cookbook generation:
- Target: Reduce FAIR from 20 to <10
- Target: Increase EXCELLENT from 26 to >30
- Target: Maintain 0 POOR

## Related Documentation

- Full comparison report: `reports/query_cookbook_gen_comparison.20260402094816.md`
- Previous fixes: `specs/fix_dropped_related_resource_promotion.md`, `specs/fix_conversion_action_field_retrieval.md`

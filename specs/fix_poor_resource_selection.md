# Fix POOR Resource Selection for Budget and Bidding Queries

## Context

Analysis of the query comparison report (`reports/query_cookbook_gen_comparison.20260404002719.md`) revealed 4 POOR results (3% of 116 total). All stem from resource selection issues where the LLM selects a narrow entity resource instead of the broader `campaign` resource.

### Problem Statement

When users ask about "campaign budgets" or "bidding strategies", the RAG similarity search ranks `campaign_budget` or `bidding_strategy` higher due to name matching. However, the reference queries use `FROM campaign` because:
1. `campaign` provides cross-entity visibility (campaign identity + budget/bidding config + metrics)
2. `campaign_budget` and `bidding_strategy` lack campaign identity fields
3. Most analytical queries need the campaign context, not isolated entity data

### POOR Cases

| Case | Description | Generated | Expected | Root Cause |
|------|-------------|-----------|----------|------------|
| `campaign_budgets_configuration` | "List campaign budgets with configuration" | `campaign_budget` | `campaign` | Missing guidance: budget config WITH campaign identity |
| `campaigns_with_budget_utilization_last_30_days` | "Budget utilization last 30 days" | `campaign_budget` | `campaign` | Missing guidance: spend metrics via campaign |
| `campaigns_with_bidding_strategy_daily_performance` | "Bidding strategy effectiveness" | `bidding_strategy` | `campaign` | Missing guidance: per-campaign vs per-strategy breakdown |
| `campaign_budgets_with_spend` | "Budgets and actual spend" | `campaign` (correct) | - | Missing fields: `metrics.conversions`, `metrics.impressions` |

## Solution: Domain Knowledge Rules

Add explicit guidance to `resources/domain_knowledge.md` for budget and bidding resource selection.

### Implementation Plan

#### Step 1: Add Budget Resource Guidance

Add to `## Resource Selection Guidance` section in `resources/domain_knowledge.md`:

```markdown
- For campaign budget configuration WITH campaign identity (campaign.id, campaign.name, campaign.status):
  Use `campaign` with `campaign_budget.*` fields. Do NOT use `campaign_budget` alone.
  
  The `campaign_budget` resource provides budget-level data in isolation (amount, delivery method, type).
  The `campaign` resource provides the SAME budget fields PLUS campaign identity and metrics.
  
  **When to use `campaign` (most common):**
  - User wants budget info for specific campaigns (needs campaign.id, campaign.name)
  - User wants budget AND performance metrics (cost, impressions, conversions)
  - User wants to analyze budget utilization/spend vs budget
  - User asks for "campaign budgets" (plural implies per-campaign breakdown)
  
  **When to use `campaign_budget` (rare):**
  - User explicitly asks for the budget ENTITY without campaign context
  - User wants to list all budgets regardless of campaign association
  - User is looking at budget-level attributes not tied to campaign performance
```

#### Step 2: Add Bidding Strategy Resource Guidance

Add to `## Resource Selection Guidance` section:

```markdown
- For bidding strategy analysis WITH per-campaign breakdown:
  Use `campaign` with `bidding_strategy.*` and `campaign.target_cpa.*` / `campaign.target_roas.*` fields.
  
  **Resource comparison:**
  - `bidding_strategy` → Aggregates metrics ACROSS ALL campaigns using that strategy
  - `campaign` → Per-campaign metrics WITHIN each bidding strategy
  
  **When to use `campaign` (most common):**
  - User asks for "bidding strategy effectiveness per campaign"
  - User wants to compare campaigns using the same strategy
  - User wants campaign-level bid settings (target CPA/ROAS)
  - User mentions "campaign" anywhere in the request
  
  **When to use `bidding_strategy` (rare):**
  - User explicitly asks for portfolio-level bidding strategy performance
  - User wants to compare different bidding strategies to each other (not campaigns)
  - User asks "how is my target CPA strategy performing overall"
```

#### Step 3: Add Budget Analysis Field Requirements

Add to `## Pattern-Based Field Requirements` section:

```markdown
### Budget Utilization Analysis Pattern
When the request mentions budget analysis (phrases: "budget utilization", "budget vs spend", "budget usage", "maxing out budget", "budget efficiency"):
- ALWAYS include `campaign_budget.amount_micros` (daily budget)
- ALWAYS include `metrics.cost_micros` (actual spend)
- Include `campaign_budget.delivery_method` (standard vs accelerated)
- Include `campaign_budget.has_recommended_budget` and `campaign_budget.recommended_budget_amount_micros` if available
- For budget-constrained analysis, include `metrics.search_budget_lost_impression_share`
```

#### Step 4: Add Spend Analysis Field Requirements

Add to `## Pattern-Based Field Requirements` section:

```markdown
### Spend Analysis Pattern
When the request mentions spend or cost analysis (phrases: "actual spend", "spend last X days", "cost breakdown", "budget and spend"):
- ALWAYS include the core volume metrics: `metrics.impressions`, `metrics.clicks`
- ALWAYS include outcome metrics: `metrics.conversions` (unless explicitly excluded)
- Include `customer.currency_code` for proper monetary interpretation
```

### Files to Modify

| File | Changes |
|------|---------|
| `resources/domain_knowledge.md` | Add 4 new guidance sections (Steps 1-4) |

### Verification

After implementation, re-run the cookbook test:

```bash
cd /rust_dev_cache/projects/googleads/cookbook_gen_test
python scripts/run_cookbook_gen_test.py
```

**Success criteria:**
- All 4 POOR cases should improve to FAIR or better
- `campaign_budgets_configuration` → GOOD or EXCELLENT (correct resource: `campaign`)
- `campaigns_with_budget_utilization_last_30_days` → GOOD or EXCELLENT (correct resource: `campaign`)
- `campaigns_with_bidding_strategy_daily_performance` → GOOD or EXCELLENT (correct resource: `campaign`)
- `campaign_budgets_with_spend` → EXCELLENT (correct resource AND all required fields)

### Risk Assessment

**Low risk** - Changes are additive to domain knowledge and follow existing patterns. No code changes required.

**Potential side effects:**
- May cause some queries that legitimately need `campaign_budget` or `bidding_strategy` resources to incorrectly select `campaign`. Mitigated by the "When to use X (rare)" guidance that preserves those use cases.

### Future Considerations

If domain knowledge rules prove insufficient, consider:
1. **Metrics-required heuristic**: In Phase 1 (`rag.rs` lines 1947-2266), add logic to prefer resources with `has_metrics = true` when the query mentions metrics/performance
2. **Cross-entity visibility scoring**: Score resources by how many related entity types they can join with
3. **Cookbook-aware selection**: Give higher weight to resources used in similar cookbook examples

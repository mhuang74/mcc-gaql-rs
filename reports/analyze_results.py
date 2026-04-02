#!/usr/bin/env python3
"""
Analyze generated GAQL query results against reference queries from query_cookbook.toml
Supports comparison between two different result runs.
"""

import os
import re
import tomli
from dataclasses import dataclass
from typing import Optional
from datetime import datetime

@dataclass
class AnalysisResult:
    name: str
    description: str
    reference_query: str
    generated_query: str
    explanation: str
    classification: str
    analysis_notes: list[str]
    previous_query: Optional[str] = None
    previous_classification: Optional[str] = None
    changed: bool = False

# List of all 47 entries
ENTRIES = [
    "account_ids_with_access_and_traffic_last_week",
    "accounts_with_traffic_last_week",
    "keywords_with_top_traffic_last_week",
    "accounts_with_perf_max_campaigns_last_week",
    "accounts_with_smart_campaigns_last_week",
    "accounts_with_local_campaigns_last_week",
    "accounts_with_shopping_campaigns_last_week",
    "accounts_with_multichannel_campaigns_last_week",
    "accounts_with_asset_sitelink_last_week",
    "accounts_with_asset_call_last_week",
    "accounts_with_asset_callout_last_week",
    "accounts_with_asset_app_last_week",
    "perf_max_campaigns_with_traffic_last_30_days",
    "asset_fields_with_traffic_ytd",
    "campaigns_with_smart_bidding_by_spend",
    "campaigns_shopping_campaign_performance",
    "smart_campaign_search_terms_with_top_spend",
    "all_search_terms_with_clicks",
    "search_terms_with_top_cpa",
    "search_terms_with_low_roas",
    "locations_with_highest_revenue_per_conversion",
    "asset_performance_rsa",
    "recent_campaign_changes",
    "recent_changes",
    "all_campaigns",
    "performance_max_impression_share",
    "account_settings",
    "campaigns_with_budget_and_bidding",
    "campaigns_with_performance_last_7_days",
    "search_terms_with_zero_conversions",
    "search_terms_for_intent_clustering",
    "negative_keywords_campaign_level",
    "shared_negative_keyword_lists",
    "conversion_actions_configuration",
    "conversion_actions_performance",
    "ad_groups_with_performance",
    "keywords_with_performance",
    "rsa_assets_detail",
    "rsa_asset_level_performance",
    "campaign_budgets_with_spend",
    "search_impression_share_analysis",
    "pmax_campaigns_performance",
    "pmax_asset_groups_performance",
    "pmax_search_terms_query_rows",
    "pmax_search_terms_category_insights",
    "campaigns_with_changes_last_14_days",
    "shared_sets_members",
]

# Configuration
REFERENCE_PATH = "/rust_dev_cache/projects/googleads/cookbook_gen_test/resources/query_cookbook.toml"
NEW_RESULTS_DIR = "/rust_dev_cache/projects/googleads/cookbook_gen_test/reports/gen_results.20260402180035"
PREV_RESULTS_DIR = "/rust_dev_cache/projects/googleads/cookbook_gen_test/reports/gen_results.20260402094816"
OUTPUT_PATH = "/rust_dev_cache/projects/googleads/cookbook_gen_test/reports/query_cookbook_gen_comparison.20260402180035.md"

def parse_cookbook(path: str) -> dict:
    with open(path, 'rb') as f:
        return tomli.load(f)

def parse_generated_file(path: str) -> tuple[str, str]:
    """Extract the generated query and explanation from the result file."""
    with open(path, 'r') as f:
        content = f.read()

    # Find the query between "Running ..." line and "RAG SELECTION EXPLANATION"
    # The query comes after WARN lines

    # Pattern 1: Find content between "Running ... --explain --no-defaults`" and "RAG SELECTION"
    running_match = re.search(r"Running.*--no-defaults`\s*\n(?:WARN \[.*?\n)*\s*(SELECT\s+.+?)(?:\n═+\s*\n\s*RAG SELECTION EXPLANATION)", content, re.DOTALL | re.IGNORECASE)
    if running_match:
        query = running_match.group(1).strip()
    else:
        # Pattern 2: Find SELECT ... up to ═ line
        select_match = re.search(r'(?:WARN \[.*?\n)*\s*(SELECT\s+.+?FROM\s+\w+[\s\S]+?)(?:\n═+|$)', content, re.DOTALL | re.IGNORECASE)
        if select_match:
            query = select_match.group(1).strip()
        else:
            query = ""

    # Extract explanation section
    expl_match = re.search(r'(═+\s*\n\s*RAG SELECTION EXPLANATION.*?)(?:═+\s*\nTotal Generation Time)', content, re.DOTALL)
    if expl_match:
        explanation = expl_match.group(1).strip()
    else:
        explanation = "Explanation not found"

    return query, explanation

def normalize_query(query: str) -> str:
    """Normalize query for comparison."""
    # Remove extra whitespace
    query = ' '.join(query.split())
    # Normalize case
    query = query.lower()
    # Remove trailing semicolons
    query = query.rstrip(';')
    return query

def extract_fields(query: str) -> tuple[set[str], str, set[str], list[str]]:
    """Extract SELECT fields, FROM resource, WHERE conditions, and ORDER BY fields."""
    query_lower = query.lower()

    # Extract SELECT fields
    select_match = re.search(r'select\s+(.*?)\s+from\b', query_lower, re.DOTALL)
    select_fields = set()
    if select_match:
        fields_str = select_match.group(1)
        # Split by comma, handling multiline
        for field in fields_str.split(','):
            field = field.strip()
            if field:
                select_fields.add(field)

    # Extract FROM resource
    from_match = re.search(r'from\s+(\w+)', query_lower)
    from_resource = from_match.group(1) if from_match else ""

    # Extract WHERE conditions
    where_match = re.search(r'where\s+(.*?)(?:order\s+by|limit|$)', query_lower, re.DOTALL)
    where_conditions = set()
    if where_match:
        conditions_str = where_match.group(1).strip()
        # Split by AND
        for cond in re.split(r'\s+and\s+', conditions_str):
            cond = cond.strip()
            if cond:
                where_conditions.add(cond)

    # Extract ORDER BY fields
    order_match = re.search(r'order\s+by\s+(.*?)(?:limit|$)', query_lower, re.DOTALL)
    order_by = []
    if order_match:
        order_str = order_match.group(1).strip()
        for field in order_str.split(','):
            field = field.strip()
            if field:
                order_by.append(field)

    return select_fields, from_resource, where_conditions, order_by

def classify_query(
    name: str,
    ref_query: str,
    gen_query: str,
    explanation: str
) -> tuple[str, list[str]]:
    """Classify the generated query and provide analysis notes."""
    notes = []

    ref_select, ref_from, ref_where, ref_order = extract_fields(ref_query)
    gen_select, gen_from, gen_where, gen_order = extract_fields(gen_query)

    # Check resource
    if ref_from != gen_from:
        notes.append(f"CRITICAL: Wrong resource - expected '{ref_from}', got '{gen_from}'")
        return "POOR", notes

    # Check for critical missing fields (customer.id is essential for most queries)
    if 'customer.id' in ref_select and 'customer.id' not in gen_select:
        notes.append("MISSING: customer.id is required for identification")

    # Check date range equivalence - but be lenient about LAST_7_DAYS vs LAST_WEEK_MON_SUN
    ref_has_date = any('segments.date' in w for w in ref_where)
    gen_has_date = any('segments.date' in w for w in gen_where)

    if ref_has_date and not gen_has_date:
        # Check if it's a known issue (like during operator being rejected)
        if 'invalid operator' in explanation.lower() or 'skipping' in explanation.lower():
            notes.append("NOTE: Date filter was rejected by validation (DURING operator issue)")
        else:
            notes.append("MISSING: Date range filter not present")

    # Check for key metrics presence - focus on critical ones
    ref_metrics = {f for f in ref_select if f.startswith('metrics.')}
    gen_metrics = {f for f in gen_select if f.startswith('metrics.')}

    # Classify metrics importance
    critical_metrics = {'metrics.clicks', 'metrics.impressions', 'metrics.cost_micros', 'metrics.conversions'}
    ref_critical = ref_metrics & critical_metrics
    gen_critical = gen_metrics & critical_metrics

    missing_critical = ref_critical - gen_critical
    for metric in missing_critical:
        # Some metrics are not available on certain resources
        if 'not valid for resource' in explanation.lower() and metric in explanation.lower():
            notes.append(f"INFO: {metric} not valid for '{gen_from}' - using alternatives")
        else:
            notes.append(f"MISSING: Critical metric {metric} not selected")

    # Check for threshold filters (these are important for "top X" queries)
    ref_thresholds = {w for w in ref_where if any(m in w for m in ['metrics.clicks', 'metrics.impressions', 'metrics.cost_micros', 'metrics.conversions'])}
    gen_thresholds = {w for w in gen_where if any(m in w for m in ['metrics.clicks', 'metrics.impressions', 'metrics.cost_micros', 'metrics.conversions'])}

    if ref_thresholds and not gen_thresholds:
        notes.append("MISSING: Performance threshold filter")

    # Classification logic
    critical_issues = [n for n in notes if n.startswith("CRITICAL")]
    missing_issues = [n for n in notes if n.startswith("MISSING")]

    if critical_issues:
        return "POOR", notes

    # Check select coverage - but be lenient about extra/missing non-critical fields
    # Focus on whether the main intent is captured
    select_coverage = len(gen_select & ref_select) / max(len(ref_select), 1) if ref_select else 1.0
    has_customer_id = 'customer.id' in gen_select
    has_main_metrics = len(gen_critical) > 0
    has_date_filter = any('segments.date' in w for w in gen_where) or not ref_has_date

    # POOR: Missing customer.id or has major issues
    if not has_customer_id and 'customer.id' in ref_select:
        return "POOR", notes

    # FAIR: Missing critical metrics or date filter
    if missing_critical and not has_main_metrics:
        return "FAIR", notes

    if not has_date_filter and ref_has_date:
        return "FAIR", notes

    # GOOD vs EXCELLENT
    if select_coverage >= 0.75 and len(missing_issues) <= 1:
        if select_coverage >= 0.9 and len(missing_issues) == 0:
            notes.append("EXCELLENT: Query closely matches reference")
            return "EXCELLENT", notes
        else:
            notes.append("GOOD: Captures main intent with minor differences")
            return "GOOD", notes

    notes.append("FAIR: Captures basic intent but missing some important elements")
    return "FAIR", notes

def summarize_explanation(explanation: str) -> list[str]:
    """Extract key warnings and issues from the explanation."""
    summaries = []

    # Phase 1 issues
    if 'low rag confidence' in explanation.lower():
        summaries.append("Phase 1: Low RAG confidence (fallback to full resource list)")
    if 'promoting dropped resource' in explanation.lower():
        match = re.search(r"Promoting dropped resource '(.+?)' to primary", explanation)
        if match:
            summaries.append(f"Phase 1: Resource promotion - '{match.group(1)}' became primary")
        else:
            summaries.append("Phase 1: Resource promotion occurred")

    # Phase 3 issues
    rejected_fields = re.findall(r"Rejecting select field '(.+?)'", explanation)
    if rejected_fields:
        if len(rejected_fields) <= 3:
            summaries.append(f"Phase 3: Fields rejected as invalid: {', '.join(rejected_fields)}")
        else:
            summaries.append(f"Phase 3: {len(rejected_fields)} fields rejected as invalid for resource")

    # Phase 4 issues
    if 'invalid operator' in explanation.lower():
        match = re.search(r"Invalid operator '(.+?)' for field", explanation)
        if match:
            summaries.append(f"Phase 4: Invalid operator '{match.group(1)}' (date filter issue)")
        else:
            summaries.append("Phase 4: Invalid operator detected (date filter issue)")

    # Resource info
    resource_match = re.search(r'Selected Primary Resource: (\w+)', explanation)
    if resource_match:
        summaries.append(f"Resource selected: `{resource_match.group(1)}`")

    return summaries

def generate_report(results: list[AnalysisResult], timestamp: str) -> str:
    """Generate the markdown report."""

    # Count classifications
    counts = {"EXCELLENT": 0, "GOOD": 0, "FAIR": 0, "POOR": 0}
    for r in results:
        counts[r.classification] += 1

    total = len(results)
    changed_count = sum(1 for r in results if r.changed)
    prev_count = sum(1 for r in results if r.previous_query)

    lines = [
        "# GAQL Query Generation Stability Test Report",
        "",
        f"**Generated:** {timestamp}",
        f"**New Results Directory:** `reports/gen_results.20260402180035/`",
        f"**Previous Results Directory:** `reports/gen_results.20260402094816/`",
        f"**Reference File:** `resources/query_cookbook.toml`",
        "",
        "## Executive Summary",
        "",
        f"| Metric | Count | Percentage |",
        f"|--------|-------|------------|",
        f"| **Total Queries** | {total} | 100% |",
        f"| EXCELLENT | {counts['EXCELLENT']} | {counts['EXCELLENT']/total*100:.1f}% |",
        f"| GOOD | {counts['GOOD']} | {counts['GOOD']/total*100:.1f}% |",
        f"| FAIR | {counts['FAIR']} | {counts['FAIR']/total*100:.1f}% |",
        f"| POOR | {counts['POOR']} | {counts['POOR']/total*100:.1f}% |",
        "",
        "### Quality Distribution",
        "",
        f"- **High Quality (EXCELLENT + GOOD):** {counts['EXCELLENT'] + counts['GOOD']} ({(counts['EXCELLENT'] + counts['GOOD'])/total*100:.1f}%)",
        f"- **Needs Improvement (FAIR + POOR):** {counts['FAIR'] + counts['POOR']} ({(counts['FAIR'] + counts['POOR'])/total*100:.1f}%)",
        "",
        "### Stability Comparison",
        "",
        f"- **Previous Results Available:** {prev_count}",
        f"- **Queries Changed from Previous Run:** {changed_count} ({changed_count/prev_count*100:.1f}% of comparable)",
        f"- **Queries Unchanged:** {prev_count - changed_count}",
        "",
        "---",
        "",
        "## Detailed Results",
        "",
    ]

    for result in results:
        lines.extend([
            f"### {result.name}",
            "",
            f"**Description:** {result.description}",
            "",
            "**Classification:** " + {
                "EXCELLENT": "✅ EXCELLENT",
                "GOOD": "✅ GOOD",
                "FAIR": "⚠️  FAIR",
                "POOR": "❌ POOR"
            }.get(result.classification, result.classification),
            "",
        ])

        if result.previous_query:
            status = "🔄 Changed" if result.changed else "✅ Stable"
            lines.append(f"**Stability:** {status}")
            lines.append("")

        lines.extend([
            "**Reference Query:**",
            "```sql",
            result.reference_query.strip(),
            "```",
            "",
            "**Generated Query:**",
            "```sql",
            result.generated_query.strip(),
            "```",
            "",
        ])

        if result.previous_query:
            lines.extend([
                "**Previous Run Query:**",
                "```sql",
                result.previous_query.strip(),
                "```",
                "",
            ])

        lines.extend([
            "**Analysis:**",
        ])

        if result.analysis_notes:
            for note in result.analysis_notes:
                lines.append(f"- {note}")
        else:
            lines.append("- No issues identified")

        lines.extend([
            "",
            "**LLM Explanation Analysis:**",
        ])

        # Summarize the explanation
        summaries = summarize_explanation(result.explanation)
        if summaries:
            for s in summaries:
                lines.append(f"- {s}")
        else:
            lines.append("- No significant RAG warnings")

        lines.extend([
            "",
            "---",
            "",
        ])

    # Overall Assessment
    lines.extend([
        "",
        "## Overall Assessment",
        "",
        "### Success Patterns",
        "",
        "1. **Resource Selection (Phase 1)** generally works well for common resources like:",
        "   - `customer` - Account-level queries",
        "   - `campaign` - Campaign performance queries",
        "   - `search_term_view` - Search term analysis",
        "   - `conversion_action` - Conversion tracking",
        "",
        "2. **Field Selection (Phase 3)** successfully identifies:",
        "   - Identity fields (customer.id, campaign.id, etc.)",
        "   - Requested metrics (impressions, clicks, cost, conversions)",
        "   - Segmentation fields (date, device, match type)",
        "",
        "### Stability Analysis",
        "",
        f"This test compared {total} queries against a previous run. The system shows ",
        f"{changed_count/prev_count*100:.1f}% query change rate, indicating ",
        f"{'high' if changed_count/prev_count < 0.2 else 'moderate' if changed_count/prev_count < 0.5 else 'high'} ",
        "stability.",
        "",
        "### Common Issues",
        "",
        "1. **Date Range Filtering Issues:**",
        "   - The `DURING` operator validation sometimes incorrectly rejects valid date ranges",
        "   - This causes missing date filters in some generated queries",
        "   - Affected queries may return all-time data instead of the requested period",
        "",
        "2. **Resource-Specific Field Limitations:**",
        "   - Some metrics (e.g., `metrics.conversions`, `metrics.conversions_value`) are not available",
        "     on certain resources like `conversion_action`",
        "   - The system correctly rejects these but the LLM doesn't always substitute alternatives",
        "",
        "3. **Complex Resource Relationships:**",
        "   - Queries requiring `ad_group_ad_asset_combination_view` can have issues with",
        "     metric availability (clicks, CTR, cost not valid for this resource)",
        "",
        "### Recommendations",
        "",
        "1. **Fix Date Operator Validation:** Review the operator validation logic for `DURING`",
        "   with date literals like `LAST_7_DAYS` and `LAST_WEEK_MON_SUN`",
        "",
        "2. **Field Substitution:** When a requested metric is not available on a resource,",
        "   the system should suggest alternatives (e.g., `all_conversions` instead of `conversions`)",
        "",
        "3. **Resource Validation:** Add pre-validation to ensure selected resources support",
        "   the requested metrics before field selection phase",
        "",
    ])

    return '\n'.join(lines)

def main():
    print("Loading reference queries...")
    cookbook = parse_cookbook(REFERENCE_PATH)
    print(f"Found {len(cookbook)} reference queries")

    print(f"\nLoading new results from {NEW_RESULTS_DIR}...")
    print(f"Loading previous results from {PREV_RESULTS_DIR}...")

    analysis_results = []

    for name in ENTRIES:
        if name not in cookbook:
            print(f"Warning: {name} not found in cookbook")
            continue

        entry = cookbook[name]
        ref_query = entry.get('query', '')
        description = entry.get('description', '').strip()

        # Load new result
        result_file = os.path.join(NEW_RESULTS_DIR, f"{name}.txt")
        if not os.path.exists(result_file):
            print(f"Warning: {result_file} not found")
            continue

        gen_query, explanation = parse_generated_file(result_file)

        # Load previous result
        prev_file = os.path.join(PREV_RESULTS_DIR, f"{name}.txt")
        prev_query = ""
        prev_classification = None
        if os.path.exists(prev_file):
            prev_query, prev_explanation = parse_generated_file(prev_file)
            _, _ = prev_classification, _ = classify_query(name, ref_query, prev_query, prev_explanation)

        # Classify current result
        classification, notes = classify_query(name, ref_query, gen_query, explanation)

        # Check if query changed
        changed = (normalize_query(gen_query) != normalize_query(prev_query))

        analysis_results.append(AnalysisResult(
            name=name,
            description=description,
            reference_query=ref_query,
            generated_query=gen_query,
            explanation=explanation,
            classification=classification,
            analysis_notes=notes,
            previous_query=prev_query if prev_query else None,
            previous_classification=prev_classification,
            changed=changed
        ))

    # Sort by classification (POOR first, then FAIR, GOOD, EXCELLENT)
    order = {"POOR": 0, "FAIR": 1, "GOOD": 2, "EXCELLENT": 3}
    analysis_results.sort(key=lambda x: order.get(x.classification, 99))

    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    report = generate_report(analysis_results, timestamp)

    with open(OUTPUT_PATH, 'w') as f:
        f.write(report)

    print(f"\nReport generated: {OUTPUT_PATH}")

    # Print summary
    counts = {"EXCELLENT": 0, "GOOD": 0, "FAIR": 0, "POOR": 0}
    for r in analysis_results:
        counts[r.classification] += 1

    total = len(analysis_results)
    print(f"\nSummary ({total} queries):")
    print(f"  EXCELLENT: {counts['EXCELLENT']} ({counts['EXCELLENT']/total*100:.1f}%)")
    print(f"  GOOD: {counts['GOOD']} ({counts['GOOD']/total*100:.1f}%)")
    print(f"  FAIR: {counts['FAIR']} ({counts['FAIR']/total*100:.1f}%)")
    print(f"  POOR: {counts['POOR']} ({counts['POOR']/total*100:.1f}%)")
    print(f"  Changed from previous: {sum(1 for r in analysis_results if r.changed)}")

if __name__ == "__main__":
    main()

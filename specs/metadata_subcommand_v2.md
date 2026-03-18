# Plan: Enhanced `mcc-gaql-gen metadata` Subcommand

## Context

Users need transparency into the RAG-based GAQL generation pipeline to debug field selection issues. When the LLM selects wrong fields, users need to understand:
1. What metadata the LLM received
2. Why certain fields were/weren't included as candidates
3. Quality of enrichment data

Current state: No easy way to inspect enriched metadata without custom code.

## Summary of Changes to Spec

Based on interview answers, the spec needs these updates:

### 1. Default to LLM View with `--show-all` Flag
- **Current spec**: Shows all fields by default
- **Change**: Default shows fields with LLM limits applied (15 per category), add `--show-all` to see complete list

### 2. Add Quality Indicators
- Show enrichment quality inline:
  - `[no description]` for fields missing descriptions
  - `[fallback: alphabetical]` for key_attributes/key_metrics that used fallback
  - `[no usage_notes]` in full format

### 3. Rename `--test-run` to `--subset`
- Better reflects purpose of filtering to a subset of resources

### 4. Add `--diff` Mode (Enrichment Comparison)
- Automatically compares enriched vs non-enriched metadata (no path argument needed)
- Shows three things:
  1. **Summary stats**: "Enriched: 42/50 fields have descriptions"
  2. **Enrichment markers**: `[llm-enriched]` tag on fields with LLM-generated descriptions
  3. **Before/after content**: Shows original (empty/none) vs enriched description text
- Only applies to `description` field (the LLM-enriched content)
- Note: `key_attributes` and `key_metrics` are LLM-selected but don't need diff (no "before" state)

### 5. Add `--filter` for Advanced Filtering
- Filter beyond glob patterns:
  - `--filter no-description` - fields without descriptions
  - `--filter no-usage-notes` - fields without usage notes
  - `--filter fallback` - resources using alphabetical fallback for key fields

### 6. Enhance Explanation Output in Generate Command
- Add key decisions to DEBUG-level logs (if not already):
  - Selected resource and why
  - Candidate sources (key_attributes, vector search, fallback)
  - Final field selection rationale
- Already has TRACE-level full prompts - no changes needed there

### 7. Keep Resource Names Only for selectable_with
- Don't expand to show key fields from related resources (keeps output concise)

## Updated Command Interface

```
mcc-gaql-gen metadata [OPTIONS] <QUERY>

Arguments:
  <QUERY>  Resource name, field name, or pattern

Options:
  --metadata <PATH>    Path to enriched metadata JSON [default: cache path]
  --format <FORMAT>    Output format: llm, full, json [default: llm]
  --category <CAT>     Filter by category: resource, attribute, metric, segment
  --subset             Use subset resources only (formerly --test-run)
  --show-all           Show all fields (default shows LLM view with limits)
  --diff               Show enrichment comparison (enriched vs non-enriched)
  --filter <FILTER>    Filter: no-description, no-usage-notes, fallback
```

## Implementation Tasks

1. **Update spec file** at `specs/metadata_subcommand.md`:
   - Add `--show-all` flag (default is LLM-limited view)
   - Add quality indicators to output formats
   - Rename `--test-run` to `--subset`
   - Add `--diff` mode section
   - Add `--filter` options section
   - Update examples

2. **Implementation** (future, not in this plan):
   - Add command to main.rs
   - Implement formatters with quality indicators
   - Implement diff comparison
   - Implement advanced filters

## Files to Modify

- `specs/metadata_subcommand.md` - Update design spec with changes above

# Identity Fields for GAQL Query Generation

## Context

Generated GAQL queries frequently omit key identifying attributes of the primary resource (e.g., `campaign.id`, `campaign.name`, `campaign.advertising_channel_type`), making query results hard to interpret because rows can't be identified. The query cookbook consistently includes these identity fields, but the RAG pipeline has no mechanism to guarantee their inclusion — it relies on the LLM to select them from `key_attributes`, which is unreliable.

**Goal**: Always include key identifying attributes of the primary resource and its parent hierarchy in generated queries.

## Approach: Force-inject + Prompt + ResourceMetadata

Use the same proven pattern as `customer.id`/`customer.descriptive_name` injection:
1. Add `identity_fields: Vec<String>` to `ResourceMetadata` (computed deterministically, no LLM)
2. Force-inject identity fields as candidates in Phase 2
3. Add LLM prompt instructions in Phase 3 to always select them

## Resource Hierarchy & Identity Fields

Use a **static hierarchy map** for ~15 common resources plus a **naming-convention heuristic** for all others. Full parent chain is included.

| Resource | Identity Fields (full hierarchy) |
|---|---|
| `customer` | `customer.id`, `customer.descriptive_name` |
| `campaign` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `campaign.advertising_channel_type` |
| `campaign_budget` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `campaign_budget.id`, `campaign_budget.name` |
| `campaign_criterion` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `campaign_criterion.criterion_id` |
| `ad_group` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `ad_group.id`, `ad_group.name` |
| `ad_group_ad` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `ad_group.id`, `ad_group.name`, `ad_group_ad.ad.id`, `ad_group_ad.ad.type` |
| `keyword_view` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `ad_group.id`, `ad_group.name`, `ad_group_criterion.criterion_id`, `ad_group_criterion.keyword.text` |
| `ad_group_criterion` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `ad_group.id`, `ad_group.name`, `ad_group_criterion.criterion_id` |
| `search_term_view` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `ad_group.id`, `ad_group.name`, `search_term_view.search_term` |
| `ad_group_bid_modifier` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `ad_group.id`, `ad_group.name`, `ad_group_bid_modifier.criterion_id` |
| `campaign_asset` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name`, `asset.id`, `asset.name` |
| `change_event` | `customer.id`, `customer.descriptive_name`, `campaign.id`, `campaign.name` |

**Heuristic fallback** (unmapped resources):
1. Always: `customer.id`, `customer.descriptive_name`
2. If `{resource}.id` exists in field cache → add it
3. If `{resource}.name` exists in field cache → add it

## Implementation Steps

### Step 1: Add `identity_fields` to ResourceMetadata

**File**: `crates/mcc-gaql-common/src/field_metadata.rs` (line 111)

```rust
pub struct ResourceMetadata {
    // ... existing fields ...

    /// Identity fields: fields that identify a row in query results.
    /// Includes full hierarchy (e.g., for ad_group: customer.id → campaign.id → ad_group.id).
    /// Computed deterministically, not by LLM.
    #[serde(default)]
    pub identity_fields: Vec<String>,
}
```

`#[serde(default)]` ensures backward compatibility with existing cached JSON (deserializes as empty vec).

### Step 2: Add hierarchy constant and computation functions

**File**: `crates/mcc-gaql-common/src/field_metadata.rs`

Add `RESOURCE_HIERARCHY` constant encoding `(resource, parent, identity_overrides)` tuples for ~15 common resources.

Add functions:
- `get_hierarchy_chain(resource) -> Vec<&str>` — walks parent chain up to customer
- `get_resource_identity_overrides(resource) -> &[&str]` — returns explicit overrides or falls back to `{resource}.id` / `{resource}.name` heuristic
- `pub fn compute_identity_fields(resource, fields, selectable_with) -> Vec<String>` — walks hierarchy, collects identity fields, filters by `selectable_with`

### Step 3: Populate identity_fields at cache-build time

**File**: `crates/mcc-gaql/src/field_metadata.rs` (line 269, `build_resource_metadata_from_fields`)

Call `compute_identity_fields()` when constructing each `ResourceMetadata` and set the `identity_fields` field.

### Step 4: Force-inject identity fields in Phase 2

**File**: `crates/mcc-gaql-gen/src/rag.rs` (after line 2397, after existing force-injection blocks)

```rust
// Inject identity fields for the primary resource.
let identity_fields = if let Some(rm) = self.field_cache.resource_metadata
    .as_ref().and_then(|m| m.get(primary))
{
    if rm.identity_fields.is_empty() {
        // Legacy cache fallback: heuristic
        compute_identity_fields(primary, &self.field_cache.fields, &selectable_with)
    } else {
        rm.identity_fields.clone()
    }
} else {
    compute_identity_fields(primary, &self.field_cache.fields, &selectable_with)
};

for field_name in &identity_fields {
    if selectable_with.contains(field_name) {
        if let Some(field) = self.field_cache.fields.get(field_name.as_str()) {
            if seen.insert(field_name.clone()) {
                candidates.push(field.clone());
                log::debug!("Phase 2: Force-injected {} (identity field for {})", field_name, primary);
            }
        }
    }
}
```

### Step 5: Update Phase 3 LLM prompt

**File**: `crates/mcc-gaql-gen/src/rag.rs`

Add to **both** prompt variants (with-cookbook ~line 2725, without-cookbook ~line 2814), after the existing MCC instruction:

```
- **IMPORTANT: Always include identity fields** for the primary resource in select_fields. These are fields that identify each row — like the resource's ID, name, and parent resource identifiers. If available in the field list, always include them even if the user didn't explicitly ask. For example, a campaign query should include campaign.id and campaign.name; an ad_group query should also include campaign.id, campaign.name, ad_group.id, and ad_group.name.
```

### Step 6: Update all ResourceMetadata construction sites

Add `identity_fields: vec![]` to:
- `crates/mcc-gaql-gen/src/formatter.rs` lines 80 and 130 (default/fallback constructors)
- `crates/mcc-gaql-common/src/field_metadata.rs` lines 952, 964, 976 (test fixtures)

### Step 7: Tests

- **Unit tests** for `compute_identity_fields`: verify hierarchy walking, heuristic fallback, selectable_with filtering
- **Unit tests** for `get_hierarchy_chain`: verify chain for each mapped resource
- Update existing `ResourceMetadata` test fixtures to include `identity_fields`

## Verification

1. `cargo check --workspace` — compiles cleanly
2. `cargo test --workspace -- --test-threads=1` — all tests pass
3. Run `mcc-gaql-gen` with a few test queries and verify identity fields appear in candidates (check debug logs) and in final SELECT output:
   - Campaign performance query → should include `campaign.id`, `campaign.name`, `campaign.advertising_channel_type`
   - Keyword performance query → should include `campaign.id`, `campaign.name`, `ad_group.id`, `ad_group.name`, `ad_group_criterion.criterion_id`, `ad_group_criterion.keyword.text`
   - An unusual/unmapped resource → should include `customer.id`, `customer.descriptive_name`, `{resource}.id`, `{resource}.name` via heuristic

## Risks

- **LLM may still omit identity fields** despite prompt instruction. Acceptable — force-inject + prompt maximizes probability. If 100% guarantee needed later, add post-processing in Phase 5 (GAQL assembly) to always inject into SELECT.
- **Extra tokens in candidate list** (~4-8 fields). Minimal impact — identity fields are short attribute names.

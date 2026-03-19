# Fix: Nested proto fields not included in LLM enrichment prompts

## Context

When running `mcc-gaql-gen enrich`, nested fields like
`ad_group_ad.policy_summary.approval_status` don't get proto documentation
(`Documentation:`, `Proto type:`, `Field behavior:`) in LLM prompts, while
simple two-part fields like `ad_group_ad.ad_strength` do.

**Root cause — two independent problems:**

### Problem 1: Key format mismatch in `to_scraped_docs()`

`to_scraped_docs()` (`proto_docs_cache.rs:132`) builds keys as:

```
pascal_to_snake(message_name) + "." + field.field_name
```

For `AdGroupAdPolicySummary` this produces `ad_group_ad_policy_summary.approval_status`
(one dot, underscores, no nesting).

The enricher (`enricher.rs:454`) looks up by `field.name` from `FieldMetadata`, which
uses GAQL dot-notation: `ad_group_ad.policy_summary.approval_status` (two dots).

No nested fields match. 0 out of 1,141 nested GAQL fields (those with 2+ dots) are
served proto docs today.

### Problem 2: Missing nested message types in `proto_docs_v23.json`

`parse_all_protos()` (`proto_parser.rs:458`) only walks `resources/` and `enums/`.
For nested message types it encounters two cases:

| Case | Count | Example | Status |
|------|-------|---------|--------|
| Inline nested messages inside a resource `.proto` | 109 | `MaximizeConversionValue` inside `accessible_bidding_strategy.proto` | Actively **stripped** by `remove_nested_messages()` |
| Standalone messages in `common/*.proto` | 143 | `TargetCpa` in `common/bidding.proto` | **Never parsed** |
| Not found (irrelevant types like `FieldMask`) | 2 | — | Ignore |

Of the 1,141 nested GAQL fields, 1,077 (94%) require one of these missing types.
Even with a correct key-building strategy, only 64 nested fields can be served with
current proto data.

**Validation numbers (from Python analysis against live cache files):**
- Nested GAQL fields total: **1,141**
- Matched with current code: **0** (key format wrong)
- Matched with graph traversal but current proto data: **64** (5.6%)
- Matched with graph traversal + full sub-message parsing: **~1,141** (~100%)

---

## Prior Spec Assessment

The original spec (`fix_nested_protos_naming.md`) proposed a `build_nested_gaql_key()`
naming heuristic: detect that `AdGroupAdPolicySummary` starts with the known prefix
`AdGroupAd`, extract `PolicySummary`, snake-case it, and build
`ad_group_ad.policy_summary.approval_status`.

**This approach is invalid at scale.** Python testing against real data shows it covers
only **4 out of 1,141 nested fields (0.4%)**. The vast majority of nested types use
independent names (`MaximizeConversionValue`, `PendingAccountBudgetProposal`,
`TargetCpa`) that do not embed the parent resource name as a prefix. A hardcoded
prefix table cannot scale to 109+ inline types and 143+ common types.

---

## Plan

### Phase 1 — Parse inline nested messages in `proto_parser.rs`

**File:** `crates/mcc-gaql-gen/src/proto_parser.rs`

Currently `parse_proto_file()` calls `extract_message_fields()` per top-level message,
and `extract_message_fields()` calls `remove_nested_messages()` to blank out nested
message bodies before field extraction. This deliberately prevents the nested message
body from being picked up as parent fields — correct behaviour. But the nested message
itself (name + fields) is never captured as its own `ProtoMessageDoc`.

**Change:** After `remove_nested_messages()` is applied to extract parent fields,
*also* recursively parse the nested messages from the original (un-blanked) block and
collect them as additional `ProtoMessageDoc` entries. These are already parsed as
individual `message Foo { ... }` blocks by the top-level `message_pattern` regex, but
only when they appear at depth 0. We need to capture them at depth > 0 as well.

Concretely, modify `parse_proto_file()` to use a depth-tracking walk instead of
top-level-only regex matching:

```rust
// Current (line 147): only top-level messages
for caps in self.message_pattern.captures_iter(content) { ... }

// New: collect ALL message blocks at any depth
// For each message block found:
//   - Parse its parent-level fields (existing remove_nested_messages logic)
//   - Recurse into its body to find nested message blocks
//   - Each nested message is stored separately in messages HashMap
```

The simplest implementation: after the existing top-level loop, add a recursive helper
`collect_nested_messages(block: &str, offset: usize) -> Vec<ProtoMessageDoc>` that:
1. Finds all `message Foo {` at depth 1 within `block`
2. Extracts each nested block's fields using existing `extract_message_fields`
3. Recurses into each nested block to find deeper nesting

This keeps `remove_nested_messages()` and `extract_message_fields()` unchanged — they
continue to operate correctly on whichever block they are given.

**Key detail:** nested message names are unqualified in proto (`TargetCpa`, not
`AccessibleBiddingStrategy.TargetCpa`). This is fine — they are already referenced by
their unqualified name in `type_name` fields (e.g. `"type_name": "TargetCpa"`), so the
lookup in `to_scraped_docs()` will work with unqualified names.

**Collision risk:** Two different resource protos might define an inner message with
the same unqualified name (e.g. `TargetRoas` appears in multiple files). Use
`messages.entry(name).or_insert(msg)` — first-parsed wins, or prefer the one from the
enclosing resource if a tie-break is needed. In practice collisions are rare and the
documentation for shared bidding strategy sub-messages is identical across files.

### Phase 2 — Parse `common/*.proto` files in `parse_all_protos()`

**File:** `crates/mcc-gaql-gen/src/proto_parser.rs`

`parse_all_protos()` (line 458) walks `resources/` and `enums/`. Add a third walk over
`common/` using the same `parse_proto_file()` call (which will now also capture inline
nested messages per Phase 1):

```rust
// After the resources walk:
let common_dir = proto_dir.join("common");
if common_dir.exists() {
    for entry in WalkDir::new(&common_dir) ... {
        let parsed = parser.parse_proto_file(&content);
        for msg in parsed {
            messages.entry(msg.message_name.clone()).or_insert(msg);
        }
    }
}
```

`common/` contains 42 `.proto` files defining 143 of the missing types
(`AppAdInfo`, `TargetCpa`, `AudienceInfo`, etc.). Parsing them with
`or_insert` (resource-defined messages take priority since resources are
parsed first) is safe.

### Phase 3 — Replace `to_scraped_docs()` with graph traversal

**File:** `crates/mcc-gaql-gen/src/proto_docs_cache.rs`

Replace the flat per-message key generation (lines 140-191) with a graph traversal
that walks from each GAQL resource message through its `type_name` references to build
properly-nested GAQL keys.

**New approach:**

```rust
pub fn to_scraped_docs(&self) -> ScrapedDocs {
    let mut docs = HashMap::new();

    // Only seed traversal from top-level GAQL resources
    // (skip sub-message types that are only reachable via fields)
    let resource_names: HashSet<&str> = self.messages.keys()
        .filter(|name| is_gaql_resource(name))
        .map(String::as_str)
        .collect();

    for resource_name in &resource_names {
        let gaql_prefix = pascal_to_snake(resource_name);
        self.traverse_message(resource_name, &gaql_prefix, &mut docs, &mut HashSet::new());
    }

    // Resource-level description entries (unchanged from today)
    for (msg_name, msg) in &self.messages {
        if !msg.description.is_empty() {
            let key = pascal_to_snake(msg_name);
            docs.entry(key).or_insert(ScrapedFieldDoc {
                description: msg.description.clone(),
                ..Default::default()
            });
        }
    }

    ScrapedDocs { ... }
}

fn traverse_message(
    &self,
    message_name: &str,
    prefix: &str,
    docs: &mut HashMap<String, ScrapedFieldDoc>,
    visited: &mut HashSet<String>,  // cycle guard
) {
    if !visited.insert(message_name.to_string()) { return; }
    let Some(msg) = self.messages.get(message_name) else { return };

    for field in &msg.fields {
        let key = format!("{}.{}", prefix, field.field_name);
        let scraped_doc = self.build_scraped_field_doc(field);
        docs.insert(key.clone(), scraped_doc);

        // If field's type is a known message, recurse
        let simple_type = simple_type_name(&field.type_name);
        if !field.is_enum && self.messages.contains_key(simple_type) {
            self.traverse_message(simple_type, &key, docs, visited);
        }
    }
}
```

`is_gaql_resource()` returns true for names that correspond to top-level GAQL
resources (i.e. are the primary message in a `resources/` proto file). The simplest
implementation: a message is a GAQL resource if `pascal_to_snake(name)` matches a
known resource name. Since `FieldMetadataCache` is not available at this point, pass a
`&HashSet<String>` of resource names as a parameter, or derive it by checking whether
the snake-cased name appears as a top-level entry in `FieldMetadataCache`. An
alternative that avoids that dependency: treat any message whose snake-cased name does
**not** contain an underscore sequence matching a known parent as a resource. The
simplest practical approach: mark messages as resources based on a flag set during
`parse_all_protos()` (resources/ messages → `is_resource = true`).

**Recommended implementation of `is_gaql_resource`:** add a boolean field
`is_resource: bool` to `ProtoMessageDoc`, set to `true` for messages parsed from
`resources/` and `false` for those from `common/` or inline. Then
`to_scraped_docs()` uses `msg.is_resource` to decide which messages seed the traversal.

`simple_type_name()` strips the fully-qualified prefix:
`"google.ads.googleads.v23.enums.CampaignStatusEnum.CampaignStatus"` → `"CampaignStatus"`.

The visited `HashSet` prevents infinite loops for any circular references.

### Phase 4 — Fix `gaql_to_proto()` and `merge_into_field_metadata_cache()`

**File:** `crates/mcc-gaql-gen/src/proto_docs_cache.rs`

`gaql_to_proto()` (line 55) and `merge_into_field_metadata_cache()` (line 342) both
reject GAQL field names with more than 2 parts (`parts.len() != 2 → return None`).
These functions are used to populate `description` on `FieldMetadata` from proto docs
directly (separate from the `to_scraped_docs()` path used by the enricher).

For nested fields, extend `gaql_to_proto()` to return the intermediate message path:

```rust
// Current: returns Option<(message_name, field_name)> only for 2-part names
// New: returns Option<(Vec<String>, String)> — path of message names + final field
// OR keep the existing 2-part signature and add a separate lookup function
```

The simplest fix is to update `merge_into_field_metadata_cache()` to walk the graph:
for a GAQL key `a.b.c`, look up `A.b`'s `type_name`, find that message, then look up
field `c`. This mirrors the traversal logic in Phase 3.

**Scope note:** `merge_into_field_metadata_cache()` currently only fills
`FieldMetadata.description`. It is called from `mcc-gaql-gen` during the `gen`
subcommand, not during `enrich`. The enricher uses `to_scraped_docs()` exclusively.
If the goal is exclusively fixing `enrich`, Phase 4 can be deferred. Include it for
completeness but mark as lower-priority.

### Phase 5 — Update `ProtoDocsCache` stats and cache invalidation

**File:** `crates/mcc-gaql-gen/src/proto_docs_cache.rs`

After Phases 1-2, `stats()` will report a much larger `message_count` (238 → ~489+).
No code change needed — it will be automatically correct. However, the existing
`proto_docs_v23.json` cache on disk will be **invalid** because it was built without
inline and common messages. The `is_valid()` check uses the git commit hash, so as
long as the googleads-rs dependency hasn't changed, the cached file will appear valid
but will be missing the newly-parsed messages.

**Fix:** Add a schema version field to `ProtoDocsCache`:

```rust
pub struct ProtoDocsCache {
    pub schema_version: u32,  // bump when parse logic changes
    ...
}
```

Bump `schema_version` from `0` (implicit/absent) to `1`. Update `is_valid()` to check
both commit hash and schema version. This forces a cache rebuild on first run after
the code change.

---

## Files to Modify

| File | Change |
|------|--------|
| `crates/mcc-gaql-gen/src/proto_parser.rs` | Phase 1: collect inline nested messages; Phase 2: parse `common/`; add `is_resource` to `ProtoMessageDoc` |
| `crates/mcc-gaql-gen/src/proto_docs_cache.rs` | Phase 3: graph-traversal `to_scraped_docs()`; Phase 4: fix `gaql_to_proto()` / `merge_into_field_metadata_cache()`; Phase 5: schema version |

No changes needed to `enricher.rs`, `scraper.rs`, or `field_metadata.rs` — the
enricher's lookup `scraped.docs.get(&field.name)` at line 454 is already correct; we
just need the keys in `scraped.docs` to match.

---

## Tests to Add / Update

### `proto_parser.rs`

```rust
#[test]
fn test_inline_nested_messages_captured() {
    // Given a proto with inline nested messages,
    // parse_proto_file() should return BOTH the outer and each inner message
    // as separate ProtoMessageDoc entries.
    let parser = ProtoParser::new();
    let messages = parser.parse_proto_file(ACCESSIBLE_BIDDING_STRATEGY_PROTO);
    // Outer message still present
    assert!(messages.iter().any(|m| m.message_name == "AccessibleBiddingStrategy"));
    // Inner messages also present as separate entries
    assert!(messages.iter().any(|m| m.message_name == "MaximizeConversions"));
    assert!(messages.iter().any(|m| m.message_name == "TargetCpa"));
    // Inner fields extractable
    let maximize = messages.iter().find(|m| m.message_name == "MaximizeConversions").unwrap();
    assert!(maximize.fields.iter().any(|f| f.field_name == "target_cpa_micros"));
}

#[test]
fn test_inner_message_is_resource_false() {
    let parser = ProtoParser::new();
    // messages from resources/ should have is_resource = true
    // inline nested messages should have is_resource = false
}
```

Update the existing `test_nested_message_extraction` etc. to assert that the
*parent* message's field list is still correct (no regression in de-duplication).

### `proto_docs_cache.rs`

```rust
#[test]
fn test_to_scraped_docs_nested_keys() {
    // Build a minimal ProtoDocsCache with:
    //   AdGroupAd (is_resource=true) with field "policy_summary: AdGroupAdPolicySummary"
    //   AdGroupAdPolicySummary (is_resource=false) with fields "approval_status", "review_status"
    // to_scraped_docs() should produce:
    //   "ad_group_ad.policy_summary.approval_status"
    //   "ad_group_ad.policy_summary.review_status"
    // and NOT:
    //   "ad_group_ad_policy_summary.approval_status"  (old flat format)
}

#[test]
fn test_to_scraped_docs_simple_fields_unchanged() {
    // Verify 1-dot keys like "campaign.name" still work correctly.
}

#[test]
fn test_to_scraped_docs_cycle_guard() {
    // Build a cache where message A has a field of type B and B has a field of type A.
    // to_scraped_docs() must terminate without a stack overflow.
}

#[test]
fn test_schema_version_invalidates_cache() {
    // Serialize a cache with schema_version=0, load it, call is_valid() -> should be false.
}
```

### Integration / manual verification

1. Delete the stale cache: `rm ~/Library/Caches/mcc-gaql/proto_docs_v23.json`
2. Build: `cargo build -p mcc-gaql-gen`
3. Run tests: `cargo test -p mcc-gaql-gen -- --test-threads=1`
4. Rebuild cache and inspect message count:

   ```bash
   MCC_GAQL_LOG_LEVEL="info,mcc_gaql_gen=debug" \
     ./target/debug/mcc-gaql-gen gen
   # Log should show: "ProtoDocsCache: ~490 messages, ..."  (was 238)
   ```

5. Run enrichment on `ad_group_ad` with concurrency 1 and inspect a known nested field:

   ```bash
   MCC_GAQL_LOG_LEVEL="info,mcc_gaql_gen=debug" \
   MCC_GAQL_LLM_MODEL="zai-org/glm-4.7" \
     ./target/debug/mcc-gaql-gen enrich ad_group_ad --concurrency 1
   ```

   Confirm that the prompt for `ad_group_ad.policy_summary.approval_status` includes:
   - `Documentation: Output only. The overall approval status...`
   - `Proto type: google.ads.googleads.v23.enums.PolicyApprovalStatusEnum...`
   - `Field behavior: OUTPUT_ONLY`

6. Also spot-check a `common/`-sourced field, e.g. `accessible_bidding_strategy.target_cpa.target_cpa_micros`, which requires `TargetCpa` from `common/bidding.proto`.

---

## Expected Outcome

After all phases:
- Nested GAQL fields with proto docs: **~1,141 / 1,141** (up from 0)
- `proto_docs_v23.json` message count: **~490** (up from 238)
- No regression on simple 2-part fields (e.g. `campaign.name`)
- Cache automatically rebuilt on first run (schema version bump)

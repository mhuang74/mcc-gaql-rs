# Fix Nested Proto Fields in LLM Enrichment Prompts

**Date:** 2026-03-20
**Branch:** `improve_llm_selection_process`
**Files modified:** `crates/mcc-gaql-gen/src/proto_parser.rs`, `crates/mcc-gaql-gen/src/proto_docs_cache.rs`

## Problem

Nested GAQL fields like `ad_group_ad.policy_summary.approval_status` received no proto
documentation in LLM enrichment prompts. Before this fix, 0 out of 1,141 nested GAQL
fields got proto docs.

Two root causes:

1. **Key format mismatch** — `to_scraped_docs()` generated flat keys like
   `ad_group_ad_policy_summary.approval_status` instead of the correct
   `ad_group_ad.policy_summary.approval_status`.

2. **Missing message types** — Inline nested messages (109 types) were stripped by
   `remove_nested_messages()` and never captured as separate entries. `common/*.proto`
   files (143 types) were also never parsed.

## Changes

### `proto_parser.rs`

**`ProtoMessageDoc` — new `is_resource` field**

```rust
pub is_resource: bool,  // #[serde(default)] for backward compat
```

Marks whether a message is a top-level GAQL resource (from `resources/*.proto`) vs. a
shared/inline message type. Used by the graph traversal in `to_scraped_docs()` as seeds.

**`collect_nested_messages()` / `collect_nested_messages_in_block()`**

New methods that walk the body of a parsed message (between its `{` and `}`) to capture
inline nested message definitions as separate `ProtoMessageDoc` entries with
`is_resource: false`. The implementation:

- Uses a char-by-char scan at brace-depth 0 to find only *direct child* `message Name {`
  declarations, avoiding duplicate extraction of grandchildren.
- Recurses on the *body* of each found child (not the header line) to capture deeper
  nesting without infinite loops.
- Passes the full file content for comment extraction so field/message descriptions are
  preserved.

`parse_proto_file()` calls this after each top-level message and appends results.
Existing `remove_nested_messages()` and `extract_message_fields()` are unchanged.

**`parse_all_protos()` — `common/` directory**

After the existing `resources/` and `enums/` walks, a third walk over `common/*.proto`
inserts messages using `or_insert` so resource-defined messages (parsed first) take
priority. Common types get `is_resource: false`.

The `resources/` loop now uses `or_insert` + marks `idx == 0` as `is_resource: true`
(the first/top-level message per file is the GAQL resource; subsequent entries are
captured inline nested messages).

**`find_container_message()` — clippy fix**

Replaced a `for` loop that always returned on the first iteration with `.last()` on the
iterator, which correctly finds the *last* message before a given position.

### `proto_docs_cache.rs`

**`simple_type_name()` helper**

```rust
fn simple_type_name(type_name: &str) -> &str {
    type_name.rsplit('.').next().unwrap_or(type_name)
}
```

Strips fully-qualified proto type prefixes (e.g. `google.ads.googleads.v23.common.PolicySummary`
→ `PolicySummary`) for message lookup.

**`to_scraped_docs()` — graph traversal**

Replaced the flat per-message iteration with a graph traversal seeded from resource
messages (`is_resource == true`):

1. For each resource, emit a resource-level description entry (key = `snake_case_name`).
2. Walk its fields recursively via `walk_message()`:
   - Key = `"{prefix}.{field_name}"` (builds nested GAQL paths).
   - If a field's type resolves to a known message via `simple_type_name()`, recurse
     into that message with the new key as prefix.
3. A `HashSet<String>` visited guard prevents infinite loops on cyclic message graphs
   (e.g. `MsgA.b_ref: MsgB`, `MsgB.a_ref: MsgA`).

Result: nested GAQL keys like `ad_group_ad.policy_summary.approval_status` are now
generated and populated with proto documentation.

**`gaql_to_proto()` — accept nested fields**

Changed `parts.len() != 2` guard to `parts.len() < 2` so 3-part nested GAQL field
names are accepted. Returns `(ResourceMessage, first_field_segment)`.

**`ProtoDocsCache` — `schema_version` field**

```rust
pub schema_version: u32,  // #[serde(default)] — old caches have 0
```

`CURRENT_SCHEMA_VERSION = 1`. `is_valid()` now checks both the commit hash and schema
version, so old caches with `schema_version: 0` (or absent) are automatically rebuilt.

## Tests added / updated

### `proto_parser.rs`

| Test | What it verifies |
|---|---|
| `test_inline_nested_messages_captured` | `parse_proto_file()` returns outer + inner messages as separate entries |
| `test_inner_message_is_resource_false` | All non-top-level messages have `is_resource = false` |
| Existing nested tests | Updated `assert_eq!(messages.len(), ...)` to account for inline nested messages now being returned alongside the parent |

### `proto_docs_cache.rs`

| Test | What it verifies |
|---|---|
| `test_schema_version_invalidates_cache` | Cache with `schema_version = 0` fails `is_valid()` |
| `test_to_scraped_docs_nested_keys` | `ad_group_ad.policy_summary.approval_status` key is generated |
| `test_to_scraped_docs_simple_fields_unchanged` | 1-dot keys like `ad_group_ad.resource_name` still work |
| `test_to_scraped_docs_cycle_guard` | A→B→A cyclic graph terminates without infinite loop |

All 71 tests pass (`cargo test -p mcc-gaql-gen -- --test-threads=1`). No clippy errors
introduced.

## Expected impact

After deleting the stale cache (`~/Library/Caches/mcc-gaql/proto_docs_v23.json`) and
rebuilding:

- Message count: ~238 → ~490 (inline nested + common types added)
- Nested GAQL fields with proto docs: 0 → ~1,141
- `scraped.docs.get(&field.name)` lookups in `enricher.rs` succeed for nested fields
  with no changes required to that file

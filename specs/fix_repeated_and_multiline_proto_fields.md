# Fix: `repeated` and multi-line proto field definitions not parsed

## Context

After the nested proto field fix (see `specs/fix_nested_protos_naming.md`), two categories
of fields still get no proto documentation in LLM enrichment prompts. Both root causes are
in the `field_pattern` regex in `proto_parser.rs:128`.

**Reported fields:**
- `ad_group.excluded_parent_asset_field_types` — uses `repeated` keyword
- `ad_group.demand_gen_ad_group_settings.channel_controls.channel_strategy` — type wraps across two lines

**Scope from analysis of all googleads v23 proto files:**

| Pattern | Resources | Common | Total |
|---------|-----------|--------|-------|
| `repeated` keyword before type | 136 | 162 | **298** |
| Multi-line type definitions | 149 | 85 | **234** |
| Both combined | 15 | 7 | **22** |

`optional` and `map<>` have 0 occurrences in the real proto files (though `optional` appears
in test fixtures and must continue to work).

---

## Root Causes

### Bug 1: `repeated` keyword not handled

The `field_pattern` regex:
```regex
(?m)^\s*((?:\w+\.)*\w+)\s+(\w+)\s*=\s*(\d+)(?:\s*\[([^\]]*)\])?;
```

Expects the line to start with the type name directly. Proto fields like:
```proto
repeated google.ads.googleads.v23.enums.AssetFieldTypeEnum.AssetFieldType
    excluded_parent_asset_field_types = 54;
```
have `repeated` before the type, so the regex never matches. **298 fields affected.**

### Bug 2: Multi-line type definitions

Fully-qualified types that wrap to the next line:
```proto
google.ads.googleads.v23.enums.DemandGenChannelStrategyEnum
    .DemandGenChannelStrategy channel_strategy = 2;
```

The `(?m)` flag makes `^` match line starts, so the regex only sees each line independently
and never matches the full `type name = N;` pattern. **234 fields affected.**

---

## Plan

### File: `crates/mcc-gaql-gen/src/proto_parser.rs`

#### Change 1: Add `normalize_multiline_fields()` method (after `remove_nested_messages()`, ~line 460)

A pre-processing step that joins continuation lines while preserving byte positions — the
same pattern used by the existing `remove_nested_messages()`.

When a `\n` is followed by whitespace then `.`, replace the newline and leading whitespace
with spaces. This puts the `.TypeName` continuation on the same logical line as the type
prefix while keeping all byte offsets unchanged.

```rust
fn normalize_multiline_fields(&self, content: &str) -> String {
    let mut bytes = content.as_bytes().to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'.' {
                // Replace \n + whitespace with spaces (same byte count)
                for k in i..j {
                    bytes[k] = b' ';
                }
            }
        }
        i += 1;
    }
    String::from_utf8(bytes).unwrap_or_else(|_| content.to_string())
}
```

After normalization:
```
google.ads.googleads.v23.enums.DemandGenChannelStrategyEnum             .DemandGenChannelStrategy channel_strategy = 2;
```

The spaces between `Enum` and `.Demand` are harmless — the type regex handles them
(see Change 2), and `simple_type_name()` in `proto_docs_cache.rs` uses `rsplit('.')`
which correctly yields `"DemandGenChannelStrategy"` regardless of spaces.

Comment extraction continues to work because byte offsets are preserved.

#### Change 2: Update `field_pattern` regex (line 128)

From:
```rust
r#"(?m)^\s*((?:\w+\.)*\w+)\s+(\w+)\s*=\s*(\d+)(?:\s*\[([^\]]*)\])?;"#
```

To:
```rust
r#"(?m)^\s*(?:repeated\s+|optional\s+)?((?:\w+\s*\.\s*)*\w+)\s+(\w+)\s*=\s*(\d+)(?:\s*\[([^\]]*)\])?;"#
```

Two additions:
1. `(?:repeated\s+|optional\s+)?` — optional non-capturing group for the `repeated`/`optional` keyword
2. `\s*\.\s*` instead of `\.` in the type pattern — allows whitespace around dots (from normalization)

Capture groups 1–4 remain unchanged. Existing fields continue to match.

#### Change 3: Clean up captured `type_name` (in `extract_message_fields()`, ~line 497)

After capturing group 1, strip internal whitespace around dots so the stored type name
is clean for downstream use (LLM prompt display via `proto_type`, graph traversal via
`simple_type_name()`):

```rust
let type_name = caps.get(1).unwrap().as_str().to_string();
// Normalize whitespace around dots from multi-line type joins
let type_name = type_name.split('.').map(|s| s.trim()).collect::<Vec<_>>().join(".");
```

#### Change 4: Call normalization in `extract_message_fields()` (~line 492)

After `remove_nested_messages()`, add the normalization call:

```rust
let filtered_block = self.remove_nested_messages(message_block);
let filtered_block = self.normalize_multiline_fields(&filtered_block);
```

#### Change 5: Also normalize in `collect_nested_messages_in_block()` field extraction

`collect_nested_messages_in_block()` at line 287 calls `extract_message_fields()` with
`full_content` and `global_header_start`. Since `extract_message_fields` takes raw content
and runs `remove_nested_messages` + normalization internally, nested message fields will
also benefit from the fix. **No additional change needed here.**

### Tests to add (in `proto_parser.rs` test module)

#### `test_repeated_field_parsing`
Proto with `repeated ... excluded_parent_asset_field_types = 54;`
Assert the field is found with correct name, number, and type.

#### `test_multiline_type_parsing`
Proto with a FQ type wrapping across two lines.
Assert field is found, type_name has no internal spaces, and comment extraction works.

#### `test_repeated_multiline_combined`
Proto with `repeated` + multi-line type (the hardest case, 22 real occurrences).
Assert field is found with correct name and number.

### No changes needed to `proto_docs_cache.rs`

- `simple_type_name()` already handles FQ types correctly via `rsplit('.')`
- `to_scraped_docs()` graph traversal uses `simple_type_name()` for message lookup
- `make_field_doc()` stores `type_name` as `proto_type` for display — the cleanup in
  Change 3 ensures this is clean

---

## Verification

```bash
# 1. Run tests
cargo test -p mcc-gaql-gen -- --test-threads=1

# 2. Delete stale cache and rebuild
rm ~/Library/Caches/mcc-gaql/proto_docs_v23.json
cargo build -p mcc-gaql-gen

# 3. Rebuild cache and check message/field counts increased
MCC_GAQL_LOG_LEVEL="info,mcc_gaql_gen=debug" ./target/debug/mcc-gaql-gen gen

# 4. Spot-check: verify the two reported fields now have proto docs
# Run enrichment on ad_group and inspect prompts for:
#   - ad_group.excluded_parent_asset_field_types (repeated field)
#   - ad_group.demand_gen_ad_group_settings.channel_controls.channel_strategy (multi-line + nested)
```

---

## Expected Outcome

- ~298 `repeated` fields + ~234 multi-line fields newly captured in proto docs cache
- Both reported fields (`excluded_parent_asset_field_types`, `channel_strategy`) appear
  with `Documentation:`, `Proto type:`, and `Field behavior:` in LLM enrichment prompts
- No regression on existing tests (the `optional` keyword in test fixtures continues to work)

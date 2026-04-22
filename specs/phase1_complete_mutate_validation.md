# Phase 1: Complete mcc-gaql-mut Mutation Validation

**Date:** 2026-04-22
**Status:** Planned
**Depends on:** refactor_new_crate_for_mutate.md (completed)
**Crate:** mcc-gaql-mut

---

## 1. Overview

The `mutate` subcommand in `mcc-gaql-mut` currently builds and sends mutation requests via `DynamicMutationBuilder`, but local validation in `mutation_validate.rs` only checks **field path existence** (can you walk the dotted path?). It does NOT validate:

- Whether the user-provided **value** matches the field's type (e.g., `"abc"` for an INT64 field)
- Whether an **enum value** is valid (e.g., `status=INVALID_VARIANT` passes validation today)
- Whether **required fields** are present for `create` operations

This means users get cryptic `DynamicMutationBuilder` errors at build time instead of clear, actionable validation messages at parse time.

**Goal**: Add pre-flight leaf-field validation to `mutation_validate.rs` using the same `prost_reflect::DescriptorPool` already in use, so that type mismatches, invalid enum values, and missing required fields are caught with helpful error messages **before** the mutation request is constructed.

Additionally, add a confirmation prompt before applying non-dry-run mutations and a `--yes` flag to skip it for CI/automation.

---

## 2. Current State

### mutation_validate.rs (215 lines)

- `validate_mutation_locally(resource_type, field_updates)` — checks: empty resource, duplicate fields, field path walkability
- `walk_path()` — walks dotted path through `Kind::Message` fields; rejects traversal into non-message fields; special-cases `Kind::String` references
- **At leaf nodes**: returns `Ok(())` without checking the value at all
- 6 unit tests

### googleads-rs coercion (already available)

- `googleads_rs::coerce_value(value_str, field_desc)` — parses a string into a `prost_reflect::Value` based on field kind (Double, Int64, Bool, String, Enum, etc.). Returns `anyhow::Result<Value>` with descriptive error messages.
- `googleads_rs::descriptor_pool()` — singleton `DescriptorPool` with all v23 protos
- Both are `pub` and importable from `googleads-rs`

### args.rs

- `MutationOperation` enum: `Update`, `Create`, `Remove`
- `validate_mutation_locally()` currently takes no operation parameter — create-specific validation can't run

---

## 3. Changes

### 3.1 Add enum value validation

**Location**: `mutation_validate.rs`, modify `walk_path()`

At the leaf of the path walk (when `remaining.is_empty()`), if the field kind is `Kind::Enum(enum_desc)`:

1. Try `enum_desc.get_value_by_name(value)` to match by name (e.g., `PAUSED`, `ENABLED`).
2. If that fails, try `value.parse::<i32>()` to match by number.
3. If both fail, return error listing the valid enum values (up to 20, then "and N more").

**Implementation**:

```rust
// In walk_path(), replace the early return at the leaf:
if remaining.is_empty() {
    return validate_leaf_value(resource_type, field_desc, value, full_path);
}

fn validate_leaf_value(
    resource_type: &str,
    field_desc: prost_reflect::FieldDescriptor,
    value: &str,
    full_path: &str,
) -> Result<()> {
    match field_desc.kind() {
        Kind::Enum(enum_desc) => {
            if enum_desc.get_value_by_name(value).is_some() {
                return Ok(());
            }
            if let Ok(n) = value.parse::<i32>() {
                if enum_desc.get_value(n).is_some() {
                    return Ok(());
                }
            }
            let total_count = enum_desc.values().count();
            let valid: Vec<String> = enum_desc
                .values()
                .take(20)
                .map(|v| v.name().to_string())
                .collect();
            let suffix = if total_count > 20 {
                format!(" (and {} more)", total_count - 20)
            } else {
                String::new()
            };
            bail!(
                "Invalid enum value '{}' for field '{}' on {}. Valid values: {}{}",
                value,
                full_path,
                resource_type,
                valid.join(", "),
                suffix
            );
        }
        // ... other kinds handled in 3.2
    }
}
```

### 3.2 Add leaf-field type validation

**Location**: `mutation_validate.rs`, extend `validate_leaf_value()`

For non-enum scalar leaf fields, attempt to parse the value string using `googleads_rs::coerce_value()`. This gives consistent error messages and catches type mismatches early:

```rust
fn validate_leaf_value(
    resource_type: &str,
    field_desc: prost_reflect::FieldDescriptor,
    value: &str,
    full_path: &str,
) -> Result<()> {
    match field_desc.kind() {
        Kind::Enum(enum_desc) => {
            // ... see 3.1 ...
        }
        Kind::Message => {
            bail!(
                "Field '{}' on {} is a message type — provide nested field paths (e.g., '{}.sub_field=value')",
                full_path, resource_type, full_path
            );
        }
        _ => {
            googleads_rs::coerce_value(value, &field_desc).map_err(|e| {
                anyhow::anyhow!(
                    "Type error for field '{}' on {}: {}",
                    full_path,
                    resource_type,
                    e
                )
            })?;
            Ok(())
        }
    }
}
```

**Why `coerce_value`?** It already handles all protobuf scalar types (double, float, int32, int64, uint32, uint64, bool, string) with proper parse errors. No need to duplicate type-parsing logic.

### 3.3 Add required field checking for create operations

**Location**: `mutation_validate.rs`

Add a new function `validate_create_required_fields()` that checks whether fields marked as `is_required()` in the descriptor are present in the field updates:

```rust
fn validate_create_required_fields(
    resource_type: &str,
    resource_desc: &prost_reflect::MessageDescriptor,
    field_updates: &[FieldUpdate],
) -> Result<()> {
    let provided_paths: std::collections::HashSet<&str> = field_updates
        .iter()
        .map(|u| u.field_path.as_str())
        .collect();

    let mut missing: Vec<String> = Vec::new();
    for field in resource_desc.fields() {
        if field.is_required() && !provided_paths.contains(field.name()) {
            let prefix = format!("{}.", field.name());
            let has_nested = provided_paths.iter().any(|p| p.starts_with(&prefix));
            if !has_nested {
                missing.push(field.name().to_string());
            }
        }
    }

    if !missing.is_empty() {
        bail!(
            "Missing required field(s) for create on {}: {}",
            resource_type,
            missing.join(", ")
        );
    }

    Ok(())
}
```

**Note**: In proto3 (used by Google Ads API), most fields are not marked `is_required()` (proto3 drops `required` label). However, `prost-reflect` supports `is_required()` for `optional` fields with `proto3_optional = true` and for `required` labels in proto2. The validation is a best-effort check — if no fields are marked required, the check passes silently. This is still valuable for future-proofing and for any proto2 resources.

### 3.4 Update validate_mutation_locally signature

Add `operation` parameter to enable create-specific validation:

```rust
pub fn validate_mutation_locally(
    resource_type: &str,
    field_updates: &[FieldUpdate],
    operation: MutationOperation,  // NEW
) -> Result<()> {
    // ... existing checks (empty resource, duplicates) ...

    validate_field_paths(resource_type, field_updates, operation)?;

    // ... log ...
    Ok(())
}
```

The `validate_field_paths` and `walk_path` call chain needs the value string and operation threaded through:

```rust
fn validate_field_paths(
    resource_type: &str,
    field_updates: &[FieldUpdate],
    operation: MutationOperation,
) -> Result<()> {
    let pool = descriptor_pool();
    let resource_fqn = format!("{}.{}", RESOURCES_FQN_PREFIX, resource_type);
    let resource_desc = pool.get_message_by_name(&resource_fqn).ok_or_else(|| { ... })?;

    // Create-specific: check required fields
    if operation == MutationOperation::Create {
        validate_create_required_fields(resource_type, &resource_desc, field_updates)?;
    }

    for update in field_updates {
        validate_single_path(resource_type, &resource_desc, &update.field_path, &update.value)?;
    }

    Ok(())
}
```

`validate_single_path` gains a `value` parameter:

```rust
fn validate_single_path(
    resource_type: &str,
    resource_desc: &prost_reflect::MessageDescriptor,
    field_path: &str,
    value: &str,  // NEW
) -> Result<()> {
    let segments: Vec<&str> = field_path.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        bail!("Empty segment in field path '{}'", field_path);
    }
    walk_path(resource_type, resource_desc, &segments, field_path, value)
}
```

`walk_path` gains a `value` parameter that is used at the leaf:

```rust
fn walk_path(
    resource_type: &str,
    current_desc: &prost_reflect::MessageDescriptor,
    segments: &[&str],
    full_path: &str,
    value: &str,  // NEW
) -> Result<()> {
    let segment = segments[0];
    let remaining = &segments[1..];

    let field_desc = current_desc.get_field_by_name(segment).ok_or_else(|| { ... })?;

    if remaining.is_empty() {
        return validate_leaf_value(resource_type, field_desc, value, full_path);
    }

    match field_desc.kind() {
        Kind::Message(nested_desc) => {
            walk_path(resource_type, &nested_desc, remaining, full_path, value)
        }
        Kind::String => { /* ... existing string reference error ... */ }
        _ => { /* ... existing non-message traversal error ... */ }
    }
}
```

### 3.5 Update callers

**main.rs** — pass operation to validate:

```rust
// Before:
mutation_validate::validate_mutation_locally(resource, &field_updates)?;

// After:
mutation_validate::validate_mutation_locally(resource, &field_updates, (*operation).into())?;
```

**mutation.rs** — no changes needed (doesn't call `validate_mutation_locally`).

### 3.6 Add --yes flag to Mutate subcommand

**Location**: `args.rs`

Add `--yes` flag to skip confirmation prompt for CI/automation:

```rust
Mutate {
    // ... existing fields ...

    #[arg(short = 'y', long, help = "Skip confirmation prompt (CI/automation)")]
    yes: bool,
},
```

### 3.7 Add confirmation prompt to Mutate

**Location**: `main.rs`

Before executing a non-dry-run, non-preview mutation, show a confirmation prompt using `dialoguer::Confirm` (already a dependency):

```rust
if !*dry_run && !*preview && !*yes {
    let confirmed = dialoguer::Confirm::new()
        .with_prompt(format!(
            "Apply {} mutation on {} ({} field(s))?",
            operation, resource, field_updates.len()
        ))
        .default(false)
        .interact()?;
    if !confirmed {
        eprintln!("Mutation cancelled.");
        return Ok(());
    }
}
```

The confirmation prompt should be placed after local validation succeeds and before the API call. The flow becomes:

1. Parse field sets
2. Validate locally (paths, types, enums, required fields)
3. If preview mode → print request and exit
4. If dry-run → print validation info to stderr
5. **Confirm** (unless `--yes`, `--dry-run`, or `--preview`)
6. Get API access
7. Execute mutation
8. Print result

### 3.8 Add import for coerce_value

**Location**: `mutation_validate.rs`

```rust
use googleads_rs::coerce_value;
```

---

## 4. New Tests

### mutation_validate.rs — 8 new tests

| Test | Purpose |
|------|---------|
| `test_validate_enum_valid_value` | `status=PAUSED` on Campaign passes |
| `test_validate_enum_invalid_value` | `status=INVALID_THING` on Campaign fails with valid values listed |
| `test_validate_enum_by_number` | `status=3` (enum number) on Campaign passes |
| `test_validate_type_mismatch` | `target_roas.target_roas=abc` on Campaign fails with type error |
| `test_validate_type_int64_valid` | `amount_micros=328000000` on CampaignBudget passes |
| `test_validate_type_int64_invalid` | `amount_micros=abc` on CampaignBudget fails with type error |
| `test_validate_leaf_message_error` | Setting a message-typed field directly (e.g., `target_roas=value` without sub-field) fails with clear error |
| `test_validate_create_with_all_fields` | Create with all fields provided passes |

### mutation.rs — 3 new tests

| Test | Purpose |
|------|---------|
| `test_build_mutation_request_update` | Build request for Campaign update, verify validate_only=false |
| `test_build_mutation_request_create` | Build request for create operation, verify structure |
| `test_build_mutation_request_remove` | Build request for remove operation, verify structure |

### args.rs — 2 new tests

| Test | Purpose |
|------|---------|
| `test_parse_field_set_with_equals_in_value` | `--set "url=https://example.com?a=1"` — value after first `=` preserved |
| `test_mutation_op_cli_invalid` | `FromStr` for `MutationOpCli` rejects invalid input |

---

## 5. Files Modified

| File | Change | Lines |
|------|--------|-------|
| `crates/mcc-gaql-mut/src/mutation_validate.rs` | Add `validate_leaf_value()`, enum validation, type validation, required-field check; update `validate_mutation_locally` signature; thread `value` and `operation` through call chain; add `coerce_value` import; add `MutationOperation` import; ~8 new tests | +120 |
| `crates/mcc-gaql-mut/src/main.rs` | Pass operation to `validate_mutation_locally`; add confirmation prompt before mutation; handle `--yes` flag | +15 |
| `crates/mcc-gaql-mut/src/args.rs` | Add `--yes` flag to `Mutate` variant; add 2 tests | +10 |
| `crates/mcc-gaql-mut/src/mutation.rs` | Add 3 unit tests for `build_mutation_request` | +50 |

---

## 6. Verification

```bash
cargo check -p mcc-gaql-mut
cargo test -p mcc-gaql-mut -- --test-threads=1
cargo clippy -p mcc-gaql-mut
cargo fmt --all -- --check
```

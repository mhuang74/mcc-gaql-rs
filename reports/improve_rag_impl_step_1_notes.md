# Step 1 Implementation Notes: New Types + Enhanced Validation

## Summary

Completed implementation of Step 1 from `multi-step-rag-implementation.md` in `crates/mcc-gaql-common/src/field_metadata.rs`.

## Changes Made

### 1. New Types Added (after line 610, before `#[cfg(test)]`)

- **`GAQLResult`** - Container for GAQL generation output with:
  - `query: String` - The generated GAQL query
  - `validation: ValidationResult` - Validation status and any errors/warnings
  - `pipeline_trace: PipelineTrace` - Detailed trace of pipeline execution

- **`PipelineTrace`** - Debugging structure for multi-step RAG with fields:
  - Phase 1 (Resource Selection): primary_resource, related_resources, dropped_resources, reasoning
  - Phase 2 (Field Retrieval): candidate_count, rejected_count
  - Phase 3 (Field Selection): selected_fields, filter_fields, order_by_fields
  - Phase 4 (Criteria Assembly): where_clauses, during, limit, implicit_filters
  - Metadata: generation_time_ms

- **`FilterField`** - Structured filter specification:
  - `field_name: String`
  - `operator: String`
  - `value: String`

### 2. Validation Types Enhanced

Added `Serialize, Deserialize, Clone` derives to:
- `ValidationResult` (was: `#[derive(Debug)]`)
- `ValidationError` (was: `#[derive(Debug)]`)
- `ValidationWarning` (was: `#[derive(Debug)]`)

This enables serialization of validation results in `GAQLResult`.

### 3. New ValidationError Variant

Added `IncompatibleFields { fields: Vec<String>, resource: String }` to `ValidationError` enum.

Updated `Display` impl to format: `"Incompatible fields for FROM {resource}: {fields}"`

### 4. New Method: `get_resource_selectable_with()`

```rust
pub fn get_resource_selectable_with(&self, resource: &str) -> Vec<String>
```

Returns the RESOURCE-category field's `selectable_with` list for a given resource. This is used as the compatibility list for validating whether metrics/segments can be selected with a given FROM resource.

### 5. New Method: `validate_field_selection_for_resource()`

```rust
pub fn validate_field_selection_for_resource(
    &self,
    field_names: &[String],
    from_resource: &str,
) -> ValidationResult
```

Validates field selection against a FROM resource's compatibility list:
1. Runs existing validation (field existence, selectability, metrics grouping)
2. Gets the FROM resource's RESOURCE-field `selectable_with` list
3. For metrics: checks if metric field name is in the list
4. For segments: checks if segment field name is in the list
5. For attributes: checks they belong to from_resource or related resources
6. Adds `IncompatibleFields` error if any fields fail compatibility check

### 6. Enhanced `build_embedding_text()`

Changed format from:
```
"{name} [{category}, {data_type}, selectable, filterable, sortable]. {description}. {usage_notes}. Valid values: {enums}. Resource: {resource}."
```

To:
```
"{name} [{category}]. {description}. {usage_notes}. Valid values: {enums}. Resource: {resource}."
```

**Removed:** `data_type`, `selectable/filterable/sortable` flags
**Kept:** field name, `[CATEGORY]` tag, description, usage_notes, enum_values, resource context

This produces more semantically meaningful embeddings and invalidates the LanceDB cache via hash change (auto-rebuilds).

## Verification

- `cargo check -p mcc-gaql-common`: ✅ Clean
- `cargo test -p mcc-gaql-common`: ✅ 6 tests passed

## Next Steps

Step 2: Per-Resource Key Field Enrichment in `enricher.rs`
- Add `select_key_fields_for_resource()` method
- Integrate into `enrich()` after resource descriptions
- Update stage labels in `run_enrichment_pipeline()`

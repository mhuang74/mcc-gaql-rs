# Fix: Allow empty selectable_with for constant/lookup resources

## Context

When generating GAQL for `geo_target_constant`, the command fails with:
```
Error: Resource 'geo_target_constant' has empty selectable_with. This indicates the field metadata cache was not properly populated.
```

**Root Cause**: The fail-fast check in `retrieve_field_candidates()` (rag.rs:2066) treats empty `selectable_with` as metadata corruption. However, `geo_target_constant` and similar constant/lookup resources (currency_constant, language_constant, etc.) legitimately have empty `selectable_with` because they are standalone lookup tables with no metrics or cross-resource joins.

The validation logic in `field_metadata.rs:validate_selectable_with()` correctly exempts resources without metrics, but the rag.rs fail-fast doesn't have this exemption.

## Fix

**File**: `crates/mcc-gaql-gen/src/rag.rs` (lines 2065-2077)

Replace the unconditional fail with metric-aware logic:

```rust
// Get selectable_with for compatibility check
let selectable_with = self.field_cache.get_resource_selectable_with(primary);

// Check if this resource has any metrics - constant/lookup resources don't
let resource_fields = self.field_cache.get_resource_fields(primary);
let has_metrics = resource_fields.iter().any(|f| f.is_metric());

// Only fail if selectable_with is empty AND resource has metrics
// Constant/lookup resources (geo_target_constant, currency_constant, etc.)
// legitimately have empty selectable_with
if selectable_with.is_empty() && has_metrics {
    let cache_path = paths::field_metadata_cache_path()
        .map(|p| format!("{:?}", p))
        .unwrap_or_else(|_| "cache directory".to_string());
    return Err(anyhow::anyhow!(
        "Resource '{}' has empty selectable_with. \
         This indicates the field metadata cache was not properly populated. \
         Please regenerate the cache by deleting {} and re-running.",
        primary,
        cache_path
    ));
}
```

## Unit Tests

Add tests in `crates/mcc-gaql-gen/src/rag.rs` mod tests to verify constant resources don't trigger the error.

Extract the check logic into a helper function for testability:

```rust
/// Check if a resource requires non-empty selectable_with
/// Returns false for constant/lookup resources that have no metrics
fn resource_requires_selectable_with(field_cache: &FieldMetadataCache, resource: &str) -> bool {
    let resource_fields = field_cache.get_resource_fields(resource);
    resource_fields.iter().any(|f| f.is_metric())
}
```

Add tests:

```rust
#[test]
fn test_resource_requires_selectable_with_for_constant_resource() {
    // Create a cache simulating geo_target_constant (no metrics)
    let cache = create_constant_resource_cache("geo_target_constant");
    assert!(!resource_requires_selectable_with(&cache, "geo_target_constant"));
}

#[test]
fn test_resource_requires_selectable_with_for_regular_resource() {
    // Create a cache simulating campaign (has metrics)
    let cache = create_resource_with_metrics_cache("campaign");
    assert!(resource_requires_selectable_with(&cache, "campaign"));
}
```

Helper to create test caches with proper resource/field structure:

```rust
fn create_constant_resource_cache(resource_name: &str) -> FieldMetadataCache {
    // Create fields with ATTRIBUTE category (no metrics)
    let mut fields = HashMap::new();
    
    // Add RESOURCE field
    fields.insert(resource_name.to_string(), FieldMetadata {
        name: resource_name.to_string(),
        category: "RESOURCE".to_string(),
        selectable_with: vec![],  // Empty - this is what we're testing
        ..default_field()
    });
    
    // Add ATTRIBUTE field
    fields.insert(format!("{}.id", resource_name), FieldMetadata {
        name: format!("{}.id", resource_name),
        category: "ATTRIBUTE".to_string(),
        ..default_field()
    });
    
    // Map resource to its fields
    let mut resources = HashMap::new();
    resources.insert(
        resource_name.to_string(),
        vec![resource_name.to_string(), format!("{}.id", resource_name)],
    );
    
    FieldMetadataCache {
        fields,
        resources: Some(resources),
        ..default_cache()
    }
}

fn create_resource_with_metrics_cache(resource_name: &str) -> FieldMetadataCache {
    let mut fields = HashMap::new();
    
    // Add RESOURCE field
    fields.insert(resource_name.to_string(), FieldMetadata {
        name: resource_name.to_string(),
        category: "RESOURCE".to_string(),
        selectable_with: vec!["metrics.clicks".to_string()],
        ..default_field()
    });
    
    // Add METRIC field
    fields.insert("metrics.clicks".to_string(), FieldMetadata {
        name: "metrics.clicks".to_string(),
        category: "METRIC".to_string(),
        ..default_field()
    });
    
    let mut resources = HashMap::new();
    resources.insert(
        resource_name.to_string(),
        vec![resource_name.to_string(), "metrics.clicks".to_string()],
    );
    
    FieldMetadataCache {
        fields,
        resources: Some(resources),
        ..default_cache()
    }
}
```

## Verification

1. Run the failing command:
   ```bash
   mcc-gaql-gen generate "Look up geo target constants for 'Mountain View' - filter by country code, target type, and name" --explain
   ```
   Should succeed and generate a valid GAQL query.

2. Run tests:
   ```bash
   cargo test -p mcc-gaql-gen -- --test-threads=1
   ```

3. Test another constant resource to ensure the fix is general:
   ```bash
   mcc-gaql-gen generate "list all currency constants"
   ```

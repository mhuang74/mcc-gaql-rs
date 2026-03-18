# Plan: Add Concurrency to Key Field Selection and Resource Description

## Context

In `enricher.rs`, the field enrichment step uses `stream::iter().map().buffer_unordered(concurrency)` with `ModelPool::acquire()` to process batches concurrently across all available models. However, the two subsequent steps run sequentially:

1. **Key field selection** (lines 175-192): loops over resources, calls `select_key_fields_for_resource()` one at a time using `acquire_preferred()`
2. **Resource description** (lines 199-220): loops over resources, calls `enrich_resource()` one at a time using `acquire_preferred()`

This is inefficient when multiple models are available. The goal is to apply the same `buffer_unordered` pattern to both steps.

## Implementation

### File to modify
`crates/mcc-gaql-gen/src/enricher.rs`

### Changes

#### 1. Refactor `select_key_fields_for_resource` to use `pool.acquire()` instead of `acquire_preferred()`

Current (line 568):
```rust
let lease = self.model_pool.acquire_preferred().await;
```

Change to:
```rust
let lease = self.model_pool.acquire().await;
```

#### 2. Refactor key field selection loop (lines 174-192) to use concurrent stream

Replace the sequential for-loop:
```rust
for resource in &resources {
    match self.select_key_fields_for_resource(resource, cache).await {
        ...
    }
}
```

With a concurrent stream pattern (similar to batch enrichment):
```rust
let key_field_results: Vec<_> = stream::iter(resources.iter())
    .map(|resource| {
        let pool = Arc::clone(&model_pool);
        let resource = resource.clone();
        // Need to pass cache data as read-only
        let resource_attrs = cache.get_resource_fields(&resource)...;
        let selectable_with = cache.get_resource_selectable_with(&resource);
        async move {
            let lease = pool.acquire().await;
            // Call static helper that takes lease + data
            Self::select_key_fields_with_lease(&lease, &resource, &resource_attrs, &resource_metrics).await
                .map(|result| (resource, result))
        }
    })
    .buffer_unordered(concurrency)
    .collect()
    .await;

// Apply results to cache
for result in key_field_results.into_iter().flatten() {
    let (resource, (key_attrs, key_mets, uses_fallback)) = result;
    if let Some(rm) = cache.resource_metadata.as_mut().and_then(|m| m.get_mut(&resource)) {
        rm.key_attributes = key_attrs;
        rm.key_metrics = key_mets;
        rm.uses_fallback = uses_fallback;
    }
}
```

**New helper method needed:**
```rust
async fn select_key_fields_with_lease(
    lease: &ModelLease,
    resource: &str,
    resource_attrs: &[String],
    resource_metrics: &[String],
) -> Result<(Vec<String>, Vec<String>, bool)>
```

This extracts the LLM call logic from `select_key_fields_for_resource`, taking pre-computed field lists instead of accessing the cache.

#### 3. Refactor `enrich_resource` to use `pool.acquire()` instead of `acquire_preferred()`

Current (line 476):
```rust
let lease = self.model_pool.acquire_preferred().await;
```

Change to accept a lease parameter instead of acquiring internally.

#### 4. Refactor resource description loop (lines 194-220) to use concurrent stream

Replace the sequential for-loop with:
```rust
let resource_desc_results: Vec<_> = stream::iter(resources.iter())
    .map(|resource| {
        let pool = Arc::clone(&model_pool);
        let scraped = Arc::clone(&scraped);
        let resource = resource.clone();
        // Extract needed ResourceMetadata fields before async block
        let rm_data = cache.resource_metadata.as_ref()
            .and_then(|m| m.get(&resource))
            .cloned();
        async move {
            if let Some(rm) = rm_data {
                let lease = pool.acquire().await;
                Self::enrich_resource_with_lease(&lease, &resource, &rm, &scraped).await
                    .map(|desc| (resource, desc))
                    .ok()
            } else {
                None
            }
        }
    })
    .buffer_unordered(concurrency)
    .collect()
    .await;

// Apply results to cache
for result in resource_desc_results.into_iter().flatten() {
    let (resource, desc) = result;
    if !desc.is_empty() {
        if let Some(rm) = cache.resource_metadata.as_mut().and_then(|m| m.get_mut(&resource)) {
            rm.description = Some(desc);
        }
    }
}
```

**New helper method needed:**
```rust
async fn enrich_resource_with_lease(
    lease: &ModelLease,
    resource_name: &str,
    rm: &ResourceMetadata,
    scraped: &ScrapedDocs,
) -> Result<String>
```

### Summary of new/modified methods

| Method | Change |
|--------|--------|
| `select_key_fields_for_resource` | Extract LLM logic to `select_key_fields_with_lease` |
| `select_key_fields_with_lease` | **NEW** - static method taking lease + pre-computed data |
| `enrich_resource` | Rename to `enrich_resource_with_lease`, take lease param |
| `enrich` | Refactor both loops to use `buffer_unordered(concurrency)` |

## Verification

1. Build: `cargo build -p mcc-gaql-gen`
2. Run tests: `cargo test -p mcc-gaql-gen -- --test-threads=1`
3. Manual test with multiple models configured:
   ```bash
   MCC_GAQL_LLM_MODEL="model1,model2" mcc-gaql-gen enrich --test-run
   ```
   Observe logs showing concurrent model usage for all three enrichment phases.

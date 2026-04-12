# Plan: Enhance Enrich Command for API Upgrades

## Context

When Google Ads API upgrades happen (e.g., v23.1 → v23.2), new resources are added that don't have enrichment data. Currently, the `mcc-gaql-gen enrich` command processes **all** resources by default, which is wasteful when only a few new resources need enrichment. The user wants:

1. **Default behavior change**: When no resource argument is specified, only process resources **missing enrichment** (i.e., resources with no enriched fields or empty key_attributes/key_metrics)
2. **New `--all` flag**: Force processing all resources (current default behavior)

A resource is considered "missing enrichment" if `mcc-gaql-gen metadata <resource> --show-all` returns no results (i.e., no fields have descriptions).

## Current Behavior Analysis

**File**: `crates/mcc-gaql-gen/src/main.rs`

The `Enrich` command (lines 96-136) currently:
- Takes an optional `resource` argument to process a single resource
- Has `--test-run` to limit to test resources
- By default (no `resource` arg), processes **all** resources from the cache

The `cmd_enrich` function (lines 470-627):
- Loads field metadata cache
- Filters to single resource if `target_resource` is Some
- Calls `enricher.enrich(&mut cache, &scraped).await` to process all fields in the cache

## Implementation Plan

### Step 1: Add `--all` CLI Option

**File**: `crates/mcc-gaql-gen/src/main.rs:96-136`

Add a new boolean flag `--all` to the `Enrich` command:

```rust
/// Enrich field metadata with LLM-generated descriptions
Enrich {
    /// Resource name to enrich (e.g., "campaign"). If not specified, enriches only resources missing enrichment.
    resource: Option<String>,

    // ... existing options ...

    /// Process all resources, even those already enriched (default: only process resources missing enrichment)
    #[arg(long)]
    all: bool,
},
```

Update the doc comment for `resource` to reflect the new default behavior.

### Step 2: Modify `cmd_enrich` Signature and Logic

**File**: `crates/mcc-gaql-gen/src/main.rs:470-500`

Add `all: bool` parameter to `cmd_enrich` function signature.

After loading the cache and handling `--test-run`, add logic to filter to resources missing enrichment:

1. If `all` is false AND `resource` is None AND `test_run` is false:
   - Load existing enriched cache if it exists
   - Identify resources that are missing enrichment
   - Filter the cache to only those resources
   - Print info message about which resources are being processed

**Helper function to identify missing enrichment**:

```rust
/// Check if a resource is missing enrichment in the enriched cache.
/// A resource is considered missing enrichment if none of its fields have descriptions,
/// or if the resource metadata lacks key_attributes/key_metrics.
fn resource_missing_enrichment(
    cache: &FieldMetadataCache,
    enriched_cache: &FieldMetadataCache,
    resource: &str,
) -> bool {
    // Get fields for this resource from the enriched cache
    let resource_fields = enriched_cache.get_resource_fields(resource);

    // If no fields at all in enriched cache, definitely missing enrichment
    if resource_fields.is_empty() {
        return true;
    }

    // Check if any field has a description
    let has_field_descriptions = resource_fields.iter().any(|f| {
        f.description.as_ref().is_some_and(|d| !d.is_empty())
    });

    // Check if resource metadata has key_attributes/key_metrics
    let has_resource_metadata = enriched_cache
        .resource_metadata
        .as_ref()
        .and_then(|rm| rm.get(resource))
        .map(|meta| {
            !meta.key_attributes.is_empty() || !meta.key_metrics.is_empty()
        })
        .unwrap_or(false);

    // Missing enrichment if neither field descriptions nor resource metadata exist
    !has_field_descriptions && !has_resource_metadata
}
```

### Step 3: Filter Resources Before Enrichment

**File**: `crates/mcc-gaql-gen/src/main.rs:493-530`

Insert the filtering logic after handling `--test-run`:

```rust
// Filter to only resources missing enrichment (unless --all flag or optional resource arg specified)
if !all && target_resource.is_none() && !test_run {
    let enriched_path = output
        .clone()
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok());

    if let Some(ref path) = enriched_path {
        if path.exists() {
            println!("Loading existing enriched cache to identify resources missing enrichment...");
            match FieldMetadataCache::load_from_disk(path).await {
                Ok(enriched_cache) => {
                    let all_resources = cache.get_resources();
                    let missing_resources: Vec<String> = all_resources
                        .into_iter()
                        .filter(|r| {
                            resource_missing_enrichment(&cache, &enriched_cache, r)
                        })
                        .collect();

                    if missing_resources.is_empty() {
                        println!("All resources are already enriched. Use --all to re-enrich everything.");
                        return Ok(());
                    }

                    println!(
                        "Processing {} resources missing enrichment out of {} total resources",
                        missing_resources.len(),
                        cache.get_resources().len()
                    );
                    cache.retain_resources(&missing_resources);
                }
                Err(e) => {
                    println!("Could not load existing enriched cache ({}). Processing all resources.", e);
                }
            }
        } else {
            println!("No existing enriched cache found. Processing all resources.");
        }
    }
}
```

### Step 4: Update `cmd_enrich` Call Site

**File**: `crates/mcc-gaql-gen/src/main.rs:296-321`

Find where the `Enrich` command is dispatched and add the `all` argument:

```rust
Commands::Enrich {
    resource,
    metadata_cache,
    output,
    scraped_docs,
    batch_size,
    scrape_delay_ms,
    scrape_ttl_days,
    test_run,
    use_proto,
    concurrency,
    all,  // NEW
} => {
    cmd_enrich(
        resource,
        metadata_cache,
        output,
        scraped_docs,
        batch_size,
        scrape_delay_ms,
        scrape_ttl_days,
        test_run,
        use_proto,
        concurrency,
        all,  // NEW
    )
    .await
}
```

### Step 5: Update Proto-based Enrichment Path

**File**: `crates/mcc-gaql-gen/src/main.rs:630-710`

The `cmd_enrich_proto` function also needs to handle the `--all` flag for consistency. However, proto-based enrichment is fast (no LLM calls), so the benefit is less pronounced. For simplicity, we can:

1. Pass `all` to `cmd_enrich_proto`
2. Apply the same filtering logic

Or alternatively, note that proto-based enrichment doesn't need this feature as much since it's fast. For this implementation, apply the same filtering to both paths for consistency.

## Files to Modify

1. **`crates/mcc-gaql-gen/src/main.rs`**
   - Add `all: bool` field to `Enrich` command (line ~136)
   - Update doc comment for `resource` field
   - Add `resource_missing_enrichment` helper function (after line 470)
   - Add filtering logic in `cmd_enrich` (after line 493)
   - Update `cmd_enrich` signature and call site (line ~350, line ~470)

## Testing Plan

1. **Test with existing enriched cache**:
   ```bash
   # Should only process resources missing enrichment
   cargo run -p mcc-gaql-gen -- enrich

   # Should process all resources (old behavior)
   cargo run -p mcc-gaql-gen -- enrich --all

   # Should process only specified resource (unchanged)
   cargo run -p mcc-gaql-gen -- enrich campaign
   ```

2. **Test with no enriched cache**:
   ```bash
   # Temporarily move enriched cache away
   mv ~/Library/Caches/mcc-gaql/field_metadata_enriched.json ~/Library/Caches/mcc-gaql/field_metadata_enriched.json.bak

   # Should process all resources (no cache to compare against)
   cargo run -p mcc-gaql-gen -- enrich

   # Restore cache
   mv ~/Library/Caches/mcc-gaql/field_metadata_enriched.json.bak ~/Library/Caches/mcc-gaql/field_metadata_enriched.json
   ```

3. **Test with new resource** (simulated):
   ```bash
   # After adding a new resource to metadata but not enriched cache
   # Should only process that new resource
   cargo run -p mcc-gaql-gen -- enrich
   ```

## Backward Compatibility Note

This is a **behavior change** for the default case. Users who previously ran `mcc-gaql-gen enrich` without arguments to process all resources will now need to add `--all` to get the same behavior. This is intentional per the user's requirement.

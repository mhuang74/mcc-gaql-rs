# Specification: Efficient Index Command and Strict Generate Command

## Problem Statement

The current implementation has two issues:

1. **Inefficient `index` command**: The `index` command runs the full 5-phase RAG pipeline even though it only needs to build embeddings. This wastes time and resources.

2. **Silent background embedding in `generate`**: The `generate` command silently falls back to building embeddings if the cache is invalid or missing, which can take 20+ minutes without the user's explicit intent.

## Goals

1. Make `index` command **only** build embeddings (no RAG pipeline phases)
2. Make `generate` command **fail fast** if embeddings cache is invalid/missing
3. Provide clear error messages directing users to run `index` first
4. Maintain backward compatibility with existing cache mechanism

## Proposed Changes

### 1. New Function: `validate_cache_for_data` in `rag.rs`

Add a public function that validates whether the cache is valid for the current data (not just whether hash files exist):

```rust
/// Validate that the cache is valid for the current data
/// Returns Ok(true) if both field metadata and query cookbook caches are valid
pub fn validate_cache_for_data(
    field_cache: &FieldMetadataCache,
    query_cookbook: &[QueryEntry],
) -> Result<bool> {
    // Check field metadata cache
    let field_hash = compute_field_cache_hash(field_cache);
    let field_valid = match crate::vector_store::load_hash("field_metadata")? {
        Some(cached_hash) => cached_hash == field_hash,
        None => false,
    };

    // Check query cookbook cache
    let query_hash = compute_query_cookbook_hash(query_cookbook);
    let query_valid = match crate::vector_store::load_hash("query_cookbook")? {
        Some(cached_hash) => cached_hash == query_hash,
        None => false,
    };

    Ok(field_valid && query_valid)
}
```

### 2. New Function: `build_embeddings_only` in `rag.rs`

Add a public function that builds embeddings without initializing the full RAG agent:

```rust
/// Build embeddings for field metadata and query cookbook
/// This is a lightweight operation that only builds embeddings, without running the RAG pipeline
pub async fn build_embeddings(
    example_queries: Vec<QueryEntry>,
    field_cache: &FieldMetadataCache,
    config: &LlmConfig,
) -> Result<()> {
    log::info!("Building embeddings for fast GAQL generation...");

    // Initialize embedding resources
    let resources = init_llm_resources(config)?;

    // Build field vector store (this will use cache if valid)
    let field_start = std::time::Instant::now();
    log::info!("Building field metadata embeddings...");
    let _field_index = build_or_load_field_vector_store(
        field_cache,
        resources.embedding_model.clone(),
    ).await?;
    log::info!(
        "Field metadata embeddings ready (took {:.2}s)",
        field_start.elapsed().as_secs_f64()
    );

    // Build query vector store (this will use cache if valid)
    let query_start = std::time::Instant::now();
    log::info!("Building query cookbook embeddings...");
    let _query_index = build_or_load_query_vector_store(
        example_queries,
        resources.embedding_model,
    ).await?;
    log::info!(
        "Query cookbook embeddings ready (took {:.2}s)",
        query_start.elapsed().as_secs_f64()
    );

    log::info!("Embeddings build complete");
    Ok(())
}
```

### 3. Modify `cmd_index` in `main.rs`

Replace the current `cmd_index` implementation that calls `convert_to_gaql` with a direct call to `build_embeddings`:

```rust
/// Index embeddings for fast query generation
async fn cmd_index(
    queries: Option<String>,
    metadata: Option<PathBuf>,
) -> Result<()> {
    validate_llm_env()?;

    let llm_config = rag::LlmConfig::from_env();

    println!("Indexing embeddings for fast GAQL generation...\n");

    // Load query cookbook (same as current)
    let example_queries: Vec<QueryEntry> = /* existing logic */;

    // Load field metadata (same as current)
    let metadata_path = /* existing logic */;
    let field_cache = /* existing logic */;

    // Check if cache already exists and is valid
    match rag::validate_cache_for_data(&field_cache, &example_queries)? {
        true => {
            println!("Embeddings cache is already up-to-date.");
            println!("You can now run 'mcc-gaql-gen generate' for instant GAQL generation.");
            return Ok(());
        }
        false => {
            println!("Building embeddings (this may take 20-30 minutes on first run)...");
            println!("Subsequent runs will be much faster if the data hasn't changed.\n");
        }
    }

    // Build embeddings only (no RAG pipeline)
    let start = std::time::Instant::now();
    rag::build_embeddings(example_queries, &field_cache, &llm_config).await?;

    println!("\n--- Indexing Complete ---");
    println!("Total time: {:.2}s", start.elapsed().as_secs_f64());
    println!("\nYou can now run 'mcc-gaql-gen generate' for instant GAQL generation.");

    Ok(())
}
```

### 4. Modify `cmd_generate` in `main.rs`

Add strict cache validation at the start that fails fast if cache is invalid:

```rust
/// Generate a GAQL query from a natural language prompt
async fn cmd_generate(
    prompt: String,
    queries: Option<String>,
    metadata: Option<PathBuf>,
    no_defaults: bool,
    verbose: bool,
) -> Result<()> {
    validate_llm_env()?;

    let llm_config = rag::LlmConfig::from_env();

    // Load query cookbook and field metadata FIRST
    let example_queries: Vec<QueryEntry> = /* existing logic */;

    let metadata_path = /* existing logic */;
    let field_cache = /* existing logic */;

    // STRICT CHECK: Validate cache matches current data
    let cache_valid = rag::validate_cache_for_data(&field_cache, &example_queries)?;

    if !cache_valid {
        eprintln!("\nERROR: Embeddings cache is not built or is out-of-date.");
        eprintln!("\nTo generate GAQL queries, you must first build the embeddings cache:");
        eprintln!("  mcc-gaql-gen index");
        eprintln!("\nThis is a one-time operation that takes 20-30 minutes.");
        eprintln!("After indexing, 'generate' commands will be instant.");
        anyhow::bail!("Cache not available - run 'mcc-gaql-gen index' first");
    }

    // Cache is valid - proceed with generation
    println!("Cache valid. Generating GAQL for: \"{}\"", prompt);

    // Build pipeline config
    let pipeline_config = rag::PipelineConfig {
        add_defaults: !no_defaults,
    };

    // Generate GAQL using MultiStepRAGAgent
    let result = rag::convert_to_gaql(
        example_queries,
        field_cache,
        &prompt,
        &llm_config,
        pipeline_config,
    ).await?;

    // ... rest of existing logic
}
```

### 5. Update `vector_store::check_cache_status`

Enhance to include content hash validation (optional, for better UX):

```rust
/// Check cache status with content validation
pub fn check_cache_status_with_data(
    field_cache: &FieldMetadataCache,
    query_cookbook: &[QueryEntry],
) -> Result<CacheStatus> {
    // Compute current hashes
    let current_field_hash = compute_field_cache_hash(field_cache);
    let current_query_hash = compute_query_cookbook_hash(query_cookbook);

    // Load cached hashes
    let field_hash = load_hash("field_metadata")?;
    let query_hash = load_hash("query_cookbook")?;

    // Validate
    let field_metadata_valid = field_hash.map_or(false, |h| h == current_field_hash);
    let query_cookbook_valid = query_hash.map_or(false, |h| h == current_query_hash);

    // ... rest same as before
}
```

### 6. Export Required Functions

Make the following functions public in `rag.rs`:
- `compute_field_cache_hash`
- `compute_query_cookbook_hash`
- `init_llm_resources`
- `build_or_load_field_vector_store` (already pub)
- `build_or_load_query_vector_store` (needs to be made pub)

## Files to Modify

1. **`crates/mcc-gaql-gen/src/rag.rs`**:
   - Add `pub fn validate_cache_for_data`
   - Add `pub async fn build_embeddings`
   - Make `compute_field_cache_hash` public
   - Make `compute_query_cookbook_hash` public
   - Make `init_llm_resources` public
   - Make `build_or_load_query_vector_store` public

2. **`crates/mcc-gaql-gen/src/main.rs`**:
   - Rewrite `cmd_index` to use `rag::build_embeddings`
   - Add strict cache check at start of `cmd_generate`

3. **`crates/mcc-gaql-gen/src/vector_store.rs`** (optional):
   - Add `check_cache_status_with_data` function

## Verification Steps

1. **Fresh environment (no cache)**:
   ```bash
   mcc-gaql-gen clear-cache
   mcc-gaql-gen generate "campaigns last week"
   # Should FAIL with clear error message telling user to run 'index'
   ```

2. **Build index**:
   ```bash
   mcc-gaql-gen index
   # Should build embeddings without running RAG pipeline
   # Should show timing for field + query embeddings separately
   ```

3. **Generate with valid cache**:
   ```bash
   mcc-gaql-gen generate "campaigns last week"
   # Should succeed immediately (no embedding delay)
   ```

4. **Cache invalidation**:
   ```bash
   # Modify field metadata or query cookbook
   mcc-gaql-gen generate "campaigns last week"
   # Should FAIL with clear error message telling user to run 'index'
   ```

## UX Improvements

1. **Progress indication for index**:
   - Show progress bar or percentage for embedding generation
   - Show estimated time remaining

2. **Clear error messages**:
   - Tell user exactly what to run
   - Explain why (one-time operation)
   - Give time estimate

3. **Cache status command** (optional):
   ```bash
   mcc-gaql-gen cache-status
   # Shows: field metadata cache valid/invalid, last updated
   # Shows: query cookbook cache valid/invalid, last updated
   ```

## Backward Compatibility

- The cache format remains unchanged (LanceDB tables + hash files)
- Existing caches will continue to work
- No changes to file paths or schema versions
- Users who have already run `index` will not need to re-run it

## Implementation Notes

1. The `convert_to_gaql` function will still use `build_or_load_*_vector_store` internally, which automatically handles cache validation. This is fine - it means the cache check in `cmd_generate` is a "fail fast" optimization, but the underlying functions are still safe.

2. Consider adding a `--skip-cache-check` flag to `generate` for power users who want the old behavior (silent embedding generation), but this is probably not necessary.

3. The hash computation functions should remain stable - they must produce the same hash for the same data across runs.

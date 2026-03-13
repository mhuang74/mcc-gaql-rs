# Plan: Optimize Embedding Generation Performance

## Context

The embedding generation step currently takes **20+ minutes (1227 seconds)** on an M2 MacBook Air. The bottleneck is in `crates/mcc-gaql-gen/src/rag.rs` where all field documents are sent to the embedding model in a single batch:

```rust
// rag.rs:532-535 - Current implementation
let field_embeddings = EmbeddingsBuilder::new(embedding_model.clone())
    .documents(field_docs.clone())?
    .build()
    .await?;
```

The `fastembed` crate (which `rig-fastembed` wraps) uses ONNX Runtime. When processing hundreds of fields at once, this creates a bottleneck because:
1. Single-threaded processing within ONNX Runtime for large batches
2. No parallelization across CPU cores
3. Memory pressure from holding all embeddings in memory at once

## Proposed Optimizations

### Option 1: Parallel Chunked Embedding (Recommended - High Impact)

Implement chunked parallel processing similar to the pattern used in `enricher.rs` (lines 107-151).

**Approach:**
- Chunk field documents into batches of ~50-100 documents
- Process chunks in parallel using `futures::stream::iter().buffer_unordered()`
- Use `num_cpus::get()` to determine optimal concurrency

**Code changes in `rag.rs`:**
```rust
// New function to add around line 465
async fn generate_embeddings_parallel<T: Embed + Clone>(
    documents: Vec<T>,
    embedding_model: rig_fastembed::EmbeddingModel,
    chunk_size: usize,
) -> Result<Vec<(T, Vec<Embedding>)>, anyhow::Error> {
    let concurrency = num_cpus::get().max(2); // Use all CPU cores

    let chunks: Vec<Vec<T>> = documents
        .chunks(chunk_size)
        .map(|c| c.to_vec())
        .collect();

    let results: Vec<_> = stream::iter(chunks.into_iter().enumerate())
        .map(|(idx, chunk)| {
            let model = embedding_model.clone();
            async move {
                log::debug!("Processing embedding chunk {}/{}", idx + 1, total_chunks);
                EmbeddingsBuilder::new(model)
                    .documents(chunk)?
                    .build()
                    .await
            }
        })
        .buffer_unordered(concurrency)
        .collect::<Result<Vec<_>, _>>()
        .await?;

    // Flatten results
    Ok(results.into_iter().flatten().collect())
}
```

**Expected improvement:** 4-8x faster on M2 (4 performance cores + 4 efficiency cores)

### Option 2: Use Larger Embedding Model with Better Batch Processing

Switch from `BGESmallENV15` (384-dim) to `BGERBaseENV15` (768-dim) which may have better batch throughput characteristics.

**Trade-offs:**
- 2x larger embeddings (more memory, slightly slower search)
- Potentially better batch processing in ONNX Runtime
- Better retrieval quality

**Code change:**
```rust
// rag.rs:188
let embedding_model = fastembed_client.embedding_model(&FastembedModel::BGERBaseENV15);
```

### Option 3: Pre-compute Embeddings on Build

Add a `generate` command flag to pre-compute and cache embeddings during the build process, so users don't pay the cost at runtime.

**Code changes:**
- Add `--precompute-embeddings` flag to generate command in `main.rs`
- Save embeddings to disk alongside field cache
- Load pre-computed embeddings on startup

### Option 4: Optimize Document Text Length

Review `FieldDocument::generate_synthetic_description()` (lines 683-692) to ensure embedding text is concise. Longer text = more tokens = slower embedding.

**Current:** Descriptions include category, data type, purpose
**Optimization:** Cache pre-computed embedding text to avoid regeneration

## Recommended Implementation

**Primary: Option 1 (Parallel Chunked Embedding)**

Files to modify:
1. `crates/mcc-gaql-gen/src/rag.rs`
   - Add `num_cpus` dependency to Cargo.toml
   - Create `generate_embeddings_parallel()` helper function (around line 465)
   - Modify `build_or_load_field_vector_store()` (line 469) to use chunked approach
   - Modify `build_or_load_query_vector_store()` (line 344) similarly

2. `crates/mcc-gaql-gen/Cargo.toml`
   - Add `num_cpus = "1.16"` dependency

**Secondary: Option 4 (Document Text Optimization)**
- Review and potentially shorten synthetic descriptions

## Verification Plan

After implementation:
```bash
# Clean existing cache to force regeneration
rm -rf ~/.cache/mcc-gaql/fastembed-models
rm -rf ~/.cache/mcc-gaql/lancedb

# Time the embedding generation
 cargo build -p mcc-gaql-gen --release
 time cargo run -p mcc-gaql-gen -- generate --metadata /path/to/metadata.json
```

Expected result: Embedding time reduced from 1227s to ~150-300s (4-8x improvement)

## Risks and Mitigations

1. **Memory usage with parallel processing:**
   - Mitigation: Limit chunk size to 50-100 docs, process with `buffer_unordered(num_cpus)`

2. **Thread safety of EmbeddingModel:**
   - Mitigation: Clone `embedding_model` for each task (it's cheap - just an Arc wrapper)

3. **Cache invalidation:**
   - No changes needed - hash validation already in place

## Implementation Notes

The enricher.rs (lines 107-151) already demonstrates the correct pattern:
- Uses `stream::iter().buffer_unordered(concurrency)`
- Logs progress per batch
- Handles errors gracefully
- Clones model for each task

This same pattern should be applied to embedding generation.

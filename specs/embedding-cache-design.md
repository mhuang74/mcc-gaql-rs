# Embedding Cache Design: LanceDB Persistence

**Status**: Design Proposal
**Author**: Claude Code
**Date**: 2025-11-09
**Related Files**: `src/prompt2gaql.rs`, `src/util.rs`, `src/field_metadata.rs`

## Table of Contents

1. [Problem Statement](#problem-statement)
2. [Current State Analysis](#current-state-analysis)
3. [Solution Architecture](#solution-architecture)
4. [Technical Design](#technical-design)
5. [Migration Strategy](#migration-strategy)
6. [Implementation Details](#implementation-details)
7. [Performance Analysis](#performance-analysis)
8. [Edge Cases & Considerations](#edge-cases--considerations)
9. [Alternatives Considered](#alternatives-considered)
10. [Success Criteria](#success-criteria)

---

## Problem Statement

### Performance Bottleneck

Every time a user runs a natural language query, the system regenerates embeddings for:
- **Query Cookbook**: ~30 example queries (~1-3 seconds)
- **Field Metadata**: ~4000+ Google Ads API fields (~15-17 seconds)

**Total overhead**: ~18-20 seconds per execution, even when the underlying data hasn't changed.

### Root Cause

The current implementation uses `rig-core`'s `InMemoryVectorStore`, which:
- Has no built-in serialization API
- Cannot be reconstructed from cached embeddings
- Forces complete rebuild on every program execution

Although cache infrastructure exists (`EmbeddingCache` struct at `src/prompt2gaql.rs:62-116`), the critical reconstruction method is unimplemented (line 105: `to_vector_store()` with TODO comment).

---

## Current State Analysis

### Architecture Overview

```
┌─────────────────────┐
│  query_cookbook.toml│
└──────────┬──────────┘
           │
           ▼
    ┌──────────────┐      ┌─────────────────────┐
    │ Load Queries │─────▶│ Generate Embeddings │ (1-3 sec)
    └──────────────┘      └─────────┬───────────┘
                                    │
                                    ▼
                          ┌────────────────────────┐
                          │ InMemoryVectorStore    │
                          │ (Query Cookbook)       │
                          └────────────────────────┘

┌──────────────────────┐
│field_metadata.json   │  (cached, 7-day TTL)
└──────────┬───────────┘
           │
           ▼
    ┌──────────────────┐      ┌─────────────────────┐
    │Create ~4000      │─────▶│ Generate Embeddings │ (15-17 sec)
    │FieldDocuments    │      └─────────┬───────────┘
    └──────────────────┘                │
                                        ▼
                              ┌────────────────────────┐
                              │ InMemoryVectorStore    │
                              │ (Field Metadata)       │
                              └────────────────────────┘
```

### Incomplete Cache Implementation

**Location**: `src/prompt2gaql.rs:62-116`

```rust
struct EmbeddingCache<T> {
    hash: u64,           // Content hash for validation
    documents: Vec<T>,   // Original documents
    embeddings: Vec<Vec<f32>>,  // Raw embedding vectors
}
```

**What Works**:
- Hash computation (lines 32-59)
- Cache validation logic (lines 131-138, 187-193)
- Bincode serialization structure
- Cache directory creation

**What's Missing**:
- `save()` method never called (marked `#[allow(dead_code)]`)
- `to_vector_store()` not implemented (line 105: "TODO: Implement this method")
- No way to extract embeddings from `InMemoryVectorStore`
- No way to reconstruct `InMemoryVectorStore` from raw embeddings

**Log Evidence** (lines 136-137, 192-193):
```
"Cache reconstruction not yet implemented, rebuilding..."
```

### Data Structures

#### Query Cookbook
- **Source**: `resources/query_cookbook.toml`
- **Structure**: `HashMap<String, QueryEntry>`
- **Embedding Target**: `QueryEntry.description` field
- **Volume**: ~30 entries
- **Hash Basis**: All descriptions + queries (lines 32-39)

#### Field Metadata
- **Source**: `~/.cache/mcc-gaql/field_metadata.json`
- **Structure**: `HashMap<String, FieldMetadata>` (4000+ fields)
- **Embedding Target**: Synthetic descriptions generated from field properties
- **Volume**: ~4000+ entries
- **Hash Basis**: API version + all field metadata (lines 42-59)

#### FieldDocument (lines 240-258)
```rust
struct FieldDocument {
    pub field: FieldMetadata,
    pub description: String,  // Synthetic description for embedding
}
```

**Synthetic Description Generation** (lines 288-332):
- Field name (dots/underscores → spaces)
- Category mapping (METRIC → "performance metric", etc.)
- Data type
- Capabilities (selectable, filterable, sortable)
- Purpose inference from name patterns

---

## Solution Architecture

### High-Level Design

Migrate from `InMemoryVectorStore` to **LanceDB** for automatic disk persistence with hash-based cache invalidation.

```
┌─────────────────────┐
│  query_cookbook.toml│
└──────────┬──────────┘
           │
           ▼
    ┌──────────────────┐      ┌─────────────────┐
    │ Compute Hash     │      │ Load LanceDB    │
    │ (descriptions +  │─────▶│ Table if exists │
    │  queries)        │ YES  │ & hash matches  │
    └──────────────────┘      └────────┬────────┘
                                       │ NO
                                       ▼
                            ┌──────────────────────┐
                            │ Generate Embeddings  │ (1-3 sec, rare)
                            └──────────┬───────────┘
                                       │
                                       ▼
                            ┌──────────────────────┐
                            │ Save to LanceDB      │
                            │ + Update Hash File   │
                            └──────────────────────┘

Similar flow for field_metadata.json
```

### Storage Structure

```
~/.cache/mcc-gaql/
├── lancedb/                           # LanceDB database directory
│   ├── query_cookbook.lance/          # Query cookbook vector store
│   │   ├── data/                      # Fragment files
│   │   ├── _versions/                 # Version metadata
│   │   └── _manifest.json             # Table manifest
│   └── field_metadata.lance/          # Field metadata vector store
│       ├── data/
│       ├── _versions/
│       └── _manifest.json
├── query_cookbook.hash                # Hash for validation
├── field_metadata.hash                # Hash for validation
└── field_metadata.json                # Field cache (existing, 7-day TTL)
```

### Dependencies

**Add to `Cargo.toml`**:
```toml
[dependencies]
rig-lancedb = "0.2.3"
lancedb = "0.15.0"
arrow-array = "53.2.0"      # For schema and RecordBatch
arrow-schema = "53.2.0"
```

**Build Requirement**: Protocol Buffer compiler (`protoc`)
- macOS: `brew install protobuf`
- Linux: `apt-get install -y protobuf-compiler`
- Documented in README

---

## Technical Design

### LanceDB Schema Definitions

#### Query Cookbook Schema

```rust
use arrow_schema::{DataType, Field, Schema};
use std::sync::Arc;

fn query_cookbook_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),           // Unique query name
        Field::new("description", DataType::Utf8, false),   // Query description
        Field::new("query", DataType::Utf8, false),         // GAQL query
        Field::new(
            "embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                384,  // AllMiniLML6V2Q embedding dimension
            ),
            false,
        ),
    ]))
}
```

#### Field Metadata Schema

```rust
fn field_metadata_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),            // Field name (e.g., "campaign.name")
        Field::new("description", DataType::Utf8, false),    // Synthetic description
        Field::new("category", DataType::Utf8, false),       // METRIC, SEGMENT, etc.
        Field::new("data_type", DataType::Utf8, false),      // INT64, STRING, etc.
        Field::new("selectable", DataType::Boolean, false),
        Field::new("filterable", DataType::Boolean, false),
        Field::new("sortable", DataType::Boolean, false),
        Field::new("metrics_compatible", DataType::Boolean, false),
        Field::new("resource_name", DataType::Utf8, true),   // Optional
        Field::new(
            "embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                384,
            ),
            false,
        ),
    ]))
}
```

### Arrow RecordBatch Conversion

```rust
use arrow_array::{
    Array, BooleanArray, FixedSizeListArray, Float32Array,
    RecordBatch, StringArray,
};
use rig::Embedding;

fn queries_to_record_batch(
    queries: &[(String, QueryEntry)],
    embeddings: &[Embedding],
) -> Result<RecordBatch> {
    let schema = query_cookbook_schema();

    // Build column arrays
    let ids: StringArray = queries.iter().map(|(id, _)| id.as_str()).collect();
    let descriptions: StringArray = queries.iter()
        .map(|(_, q)| q.description.as_str())
        .collect();
    let query_texts: StringArray = queries.iter()
        .map(|(_, q)| q.query.as_str())
        .collect();

    // Convert embeddings to FixedSizeListArray
    let embedding_values: Vec<f32> = embeddings
        .iter()
        .flat_map(|e| e.vec.clone())
        .collect();
    let embedding_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        384,
        Arc::new(Float32Array::from(embedding_values)),
        None,
    )?;

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(ids),
            Arc::new(descriptions),
            Arc::new(query_texts),
            Arc::new(embedding_array),
        ],
    )
}

fn fields_to_record_batch(
    fields: &[FieldDocument],
    embeddings: &[Embedding],
) -> Result<RecordBatch> {
    let schema = field_metadata_schema();

    // Build column arrays (similar pattern)
    let ids: StringArray = fields.iter()
        .map(|f| f.field.name.as_str())
        .collect();
    let descriptions: StringArray = fields.iter()
        .map(|f| f.description.as_str())
        .collect();
    let categories: StringArray = fields.iter()
        .map(|f| f.field.category.as_str())
        .collect();
    let data_types: StringArray = fields.iter()
        .map(|f| f.field.data_type.as_str())
        .collect();
    let selectable: BooleanArray = fields.iter()
        .map(|f| Some(f.field.selectable))
        .collect();
    let filterable: BooleanArray = fields.iter()
        .map(|f| Some(f.field.filterable))
        .collect();
    let sortable: BooleanArray = fields.iter()
        .map(|f| Some(f.field.sortable))
        .collect();
    let metrics_compatible: BooleanArray = fields.iter()
        .map(|f| Some(f.field.metrics_compatible))
        .collect();
    let resource_names: StringArray = fields.iter()
        .map(|f| f.field.resource_name.as_deref())
        .collect();

    // Convert embeddings
    let embedding_values: Vec<f32> = embeddings
        .iter()
        .flat_map(|e| e.vec.clone())
        .collect();
    let embedding_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        384,
        Arc::new(Float32Array::from(embedding_values)),
        None,
    )?;

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(ids),
            Arc::new(descriptions),
            Arc::new(categories),
            Arc::new(data_types),
            Arc::new(selectable),
            Arc::new(filterable),
            Arc::new(sortable),
            Arc::new(metrics_compatible),
            Arc::new(resource_names),
            Arc::new(embedding_array),
        ],
    )
}
```

### Cache Validation & Initialization Flow

```rust
use lancedb::{connect, Connection};
use rig_lancedb::{LanceDbVectorIndex, SearchParams};
use std::path::PathBuf;

async fn build_or_load_query_vector_store(
    embedding_model: &rig_fastembed::EmbeddingModel,
) -> Result<LanceDbVectorIndex<QueryEntry>> {
    let cache_dir = get_cache_dir()?;
    let db_path = cache_dir.join("lancedb");
    let hash_path = cache_dir.join("query_cookbook.hash");

    // Load queries
    let queries = util::get_queries_from_file()?;
    let current_hash = compute_query_hash(&queries);

    // Check if cache exists and is valid
    if let Ok(cached_hash) = std::fs::read_to_string(&hash_path) {
        if cached_hash.trim() == current_hash.to_string() {
            info!("Query cookbook cache valid, loading from LanceDB...");

            let db = connect(&db_path).execute().await?;
            if let Ok(table) = db.open_table("query_cookbook").execute().await {
                let vector_store = LanceDbVectorIndex::new(
                    table,
                    embedding_model.clone(),
                    "id",
                    SearchParams::default(),
                ).await?;

                info!("Successfully loaded query cookbook from cache");
                return Ok(vector_store);
            }
        } else {
            info!("Query cookbook hash mismatch, rebuilding...");
        }
    } else {
        info!("No query cookbook cache found, building...");
    }

    // Cache miss or invalid - rebuild embeddings
    info!("Generating embeddings for {} queries...", queries.len());

    let query_list: Vec<(String, QueryEntry)> = queries.into_iter().collect();
    let docs: Vec<QueryEntry> = query_list.iter()
        .map(|(_, q)| q.clone())
        .collect();

    let embeddings = embedding_model
        .embed_documents(&docs)
        .await?;

    // Convert to RecordBatch
    let record_batch = queries_to_record_batch(&query_list, &embeddings)?;

    // Save to LanceDB
    let db = connect(&db_path).execute().await?;
    let table = db
        .create_table("query_cookbook", vec![record_batch])
        .mode(lancedb::CreateMode::Overwrite)
        .execute()
        .await?;

    // Save hash
    std::fs::write(&hash_path, current_hash.to_string())?;
    info!("Query cookbook cache saved");

    // Create vector store index
    let vector_store = LanceDbVectorIndex::new(
        table,
        embedding_model.clone(),
        "id",
        SearchParams::default(),
    ).await?;

    Ok(vector_store)
}

async fn build_or_load_field_vector_store(
    embedding_model: &rig_fastembed::EmbeddingModel,
    field_cache: &FieldMetadataCache,
) -> Result<LanceDbVectorIndex<FieldDocument>> {
    // Similar pattern to query_vector_store
    // Use compute_field_hash(field_cache) for validation
    // Handle ~4000 documents
    // Consider using SearchParams with ANN indexing (IVF-PQ)
    // ...
}
```

### Hash Computation (Reuse Existing Logic)

```rust
// Already implemented in src/prompt2gaql.rs:32-59
fn compute_query_hash(queries: &HashMap<String, QueryEntry>) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut entries: Vec<_> = queries.iter().collect();
    entries.sort_by_key(|(k, _)| *k);
    for (_, entry) in entries {
        entry.description.hash(&mut hasher);
        entry.query.hash(&mut hasher);
    }
    hasher.finish()
}

fn compute_field_hash(cache: &FieldMetadataCache) -> u64 {
    let mut hasher = DefaultHasher::new();
    cache.api_version.hash(&mut hasher);
    let mut fields: Vec<_> = cache.fields.iter().collect();
    fields.sort_by_key(|(k, _)| *k);
    for (_, field) in fields {
        field.name.hash(&mut hasher);
        field.category.hash(&mut hasher);
        field.data_type.hash(&mut hasher);
        field.selectable.hash(&mut hasher);
        field.filterable.hash(&mut hasher);
        field.sortable.hash(&mut hasher);
        field.metrics_compatible.hash(&mut hasher);
    }
    hasher.finish()
}
```

### Integration with RAGAgent

**Minimal Changes Required** - LanceDB implements same `VectorStoreIndex` trait:

```rust
// Before (InMemoryVectorStore)
let vector_store = InMemoryVectorStore::from_documents(embeddings);
let index = vector_store.index(embedding_model);
let results = index.top_n::<QueryEntry>(query, 10).await?;

// After (LanceDB)
let index = build_or_load_query_vector_store(&embedding_model).await?;
let results = index.top_n::<QueryEntry>(query, 10).await?;
```

---

## Migration Strategy

### Phase 1: Add LanceDB Foundation (Low Risk)

**Goal**: Add dependencies and utility functions without changing core logic.

**Tasks**:
1. Add dependencies to `Cargo.toml`
2. Document `protoc` requirement in README
3. Create new module: `src/lancedb_utils.rs`
   - Schema definitions
   - RecordBatch conversion functions
   - Helper functions for LanceDB connection
4. Add feature flag (optional): `--features lancedb-cache`
5. Write unit tests for conversion functions

**Validation**: Code compiles, tests pass, no behavioral changes.

### Phase 2: Migrate Field Metadata Vector Store (High Impact)

**Goal**: Eliminate ~15 second overhead for field embeddings.

**Tasks**:
1. Implement `build_or_load_field_vector_store()` with LanceDB
2. Update `EnhancedRAGAgent::init()` to use LanceDB version
3. Keep fallback to InMemoryVectorStore if LanceDB fails
4. Update logging to show cache hit/miss
5. Test with:
   - Fresh cache (cold start)
   - Valid cache (warm start)
   - Invalid cache (hash mismatch)
   - Corrupted cache (error handling)

**Validation**:
- Cold start: ~15 seconds (same as before)
- Warm start: <1 second (vs 15 seconds before)
- Correctness: Same query results as InMemoryVectorStore

### Phase 3: Migrate Query Cookbook Vector Store (Lower Impact)

**Goal**: Eliminate ~1-3 second overhead for query embeddings.

**Tasks**:
1. Implement `build_or_load_query_vector_store()` with LanceDB
2. Update `RAGAgent::init()` to use LanceDB version
3. Handle edge case: <256 rows requires ENN instead of IVF-PQ
4. Keep fallback for safety

**Validation**:
- Cold start: ~1-3 seconds (same as before)
- Warm start: <500ms (vs 1-3 seconds before)
- Correctness: Same dynamic context as InMemoryVectorStore

### Phase 4: Cleanup & Remove Legacy Code

**Goal**: Simplify codebase after validation.

**Tasks**:
1. Remove `EmbeddingCache<T>` struct (lines 62-116)
2. Remove bincode dependency
3. Remove InMemoryVectorStore fallback code
4. Remove TODO comments about cache reconstruction
5. Update documentation

**Validation**: Code is simpler, tests pass, performance maintained.

---

## Implementation Details

### File Structure

```
src/
├── prompt2gaql.rs              # Main RAG logic
├── lancedb_utils.rs            # NEW: LanceDB helpers
│   ├── schemas.rs              # Arrow schema definitions
│   ├── conversion.rs           # RecordBatch converters
│   └── cache.rs                # Cache management
└── util.rs                     # Existing utilities

resources/
└── query_cookbook.toml         # Query examples (unchanged)

~/.cache/mcc-gaql/
├── lancedb/                    # NEW: LanceDB storage
│   ├── query_cookbook.lance/
│   └── field_metadata.lance/
├── query_cookbook.hash         # NEW: Hash file
├── field_metadata.hash         # NEW: Hash file
└── field_metadata.json         # Existing field cache
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
enum CacheError {
    #[error("LanceDB error: {0}")]
    LanceDb(#[from] lancedb::Error),

    #[error("Arrow error: {0}")]
    Arrow(#[from] arrow_schema::ArrowError),

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: u64, actual: u64 },

    #[error("Cache corruption: {0}")]
    Corruption(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// Fallback strategy
async fn build_or_load_with_fallback<T>(
    // ...
) -> Result<LanceDbVectorIndex<T>> {
    match try_load_from_cache().await {
        Ok(index) => {
            info!("Cache hit");
            Ok(index)
        }
        Err(e) => {
            warn!("Cache miss or error ({}), rebuilding...", e);
            rebuild_and_save().await
        }
    }
}
```

### Search Parameters

```rust
use rig_lancedb::SearchParams;

// For query cookbook (<256 rows, must use exact search)
SearchParams {
    index_type: IndexType::ExactNearestNeighbor,
    ..Default::default()
}

// For field metadata (>256 rows, can use ANN)
SearchParams {
    index_type: IndexType::IvfPq {
        num_partitions: 256,
        num_sub_vectors: 96,  // 384 / 4
    },
    ..Default::default()
}
```

### Concurrent Access

**Current Scope**: Single-process only (CLI tool).

**LanceDB Behavior**:
- Multiple readers: ✅ Safe (MVCC)
- Multiple writers: ⚠️ Last write wins
- Reader during write: ✅ Safe (readers see old version)

**Future Consideration**: Add file lock if multi-process support needed.

```rust
// Optional: Add file lock for cache writes
use fs2::FileExt;

fn with_cache_lock<F, R>(f: F) -> Result<R>
where
    F: FnOnce() -> Result<R>,
{
    let lock_file = get_cache_dir()?.join(".lock");
    let file = std::fs::File::create(&lock_file)?;
    file.lock_exclusive()?;
    let result = f();
    file.unlock()?;
    result
}
```

### Cache Migration from Bincode (If Needed)

If any users have existing bincode caches (unlikely since it's not working):

```rust
fn migrate_legacy_cache() -> Result<()> {
    let old_cache = get_cache_dir()?.join("embeddings/query_cookbook.bin");
    if old_cache.exists() {
        warn!("Found legacy cache, removing: {:?}", old_cache);
        std::fs::remove_file(&old_cache)?;
    }
    Ok(())
}
```

---

## Performance Analysis

### Expected Improvements

| Scenario | Before | After | Improvement |
|----------|--------|-------|-------------|
| **Cold Start** (no cache) | 18-20s | 18-20s | 0% (same) |
| **Warm Start** (valid cache) | 18-20s | <1s | **95%+** |
| **Hash Mismatch** (invalidated) | 18-20s | 18-20s | 0% (rebuild) |
| **Typical Usage** (cache valid) | 18-20s | <1s | **95%+** |

### Breakdown

**Query Cookbook**:
- Embedding generation: 1-3s
- Cache save: ~100ms (LanceDB write)
- Cache load: <50ms (LanceDB read)
- **Savings**: ~1-3s per run (after initial build)

**Field Metadata**:
- Embedding generation: 15-17s
- Cache save: ~500ms (LanceDB write, ~4000 rows)
- Cache load: <500ms (LanceDB read with ANN index)
- **Savings**: ~15-17s per run (after initial build)

### Disk Space Requirements

**Query Cookbook** (~30 entries × 384 dims):
- Raw embeddings: ~46 KB (30 × 384 × 4 bytes)
- Metadata: ~5 KB (descriptions, queries)
- LanceDB overhead: ~10-20 KB (index, manifest)
- **Total**: ~60-70 KB

**Field Metadata** (~4000 entries × 384 dims):
- Raw embeddings: ~6 MB (4000 × 384 × 4 bytes)
- Metadata: ~2 MB (field properties)
- LanceDB overhead: ~500 KB - 1 MB (ANN index)
- **Total**: ~8-10 MB

**Grand Total**: ~10 MB (negligible compared to embedding model itself at ~90 MB)

### Memory Footprint

**Before** (InMemoryVectorStore):
- All embeddings in RAM: ~6 MB
- Documents in RAM: ~2 MB
- **Total**: ~8 MB resident

**After** (LanceDB):
- Lazy loading from disk
- Only query results in RAM
- **Total**: <1 MB resident during queries

### Cache Invalidation Frequency

**Query Cookbook**:
- Changes infrequently (developer-curated examples)
- Expected invalidation: ~1-2 times per month

**Field Metadata**:
- Tied to `field_metadata.json` (7-day TTL)
- API version changes trigger rebuild
- Expected invalidation: ~1-4 times per month

**Effective Hit Rate**: >90% (most runs use valid cache)

---

## Edge Cases & Considerations

### 1. Schema Evolution

**Problem**: Arrow schema changes break existing LanceDB tables.

**Solution**: Version schema definitions
```rust
const SCHEMA_VERSION: u8 = 1;

// Save schema version in hash file
std::fs::write(
    &hash_path,
    format!("v{}\n{}", SCHEMA_VERSION, current_hash),
)?;

// Validate schema version
let cached_content = std::fs::read_to_string(&hash_path)?;
let lines: Vec<&str> = cached_content.lines().collect();
if lines[0] != format!("v{}", SCHEMA_VERSION) {
    warn!("Schema version mismatch, rebuilding...");
    invalidate_cache();
}
```

### 2. Embedding Model Changes

**Problem**: Switching embedding models invalidates all caches.

**Solution**: Include model name in hash or cache path
```rust
// Option A: Include in hash
fn compute_query_hash(queries: &HashMap<String, QueryEntry>) -> u64 {
    let mut hasher = DefaultHasher::new();
    "AllMiniLML6V2Q".hash(&mut hasher);  // Model name
    // ... rest of hash computation
}

// Option B: Separate cache per model
let db_path = cache_dir.join(format!("lancedb-{}", model_name));
```

### 3. Corrupted Cache Recovery

**Problem**: Disk corruption, interrupted writes, or bugs corrupt cache.

**Solution**: Automatic fallback with user notification
```rust
async fn load_from_cache() -> Result<LanceDbVectorIndex<T>> {
    match try_load() {
        Ok(index) => Ok(index),
        Err(e) if is_corruption_error(&e) => {
            error!("Cache corrupted: {}", e);
            error!("Removing corrupted cache and rebuilding...");
            remove_cache()?;
            Err(e)
        }
        Err(e) => Err(e),
    }
}
```

### 4. Partial Cache (Only One Store Cached)

**Problem**: Query cache valid, field cache invalid (or vice versa).

**Solution**: Independent cache management (already designed)
```rust
// Each vector store has independent hash file and LanceDB table
let query_index = build_or_load_query_vector_store().await?;
let field_index = build_or_load_field_vector_store().await?;
// If one fails, other can still benefit from cache
```

### 5. Embedding Dimension Mismatch

**Problem**: Model produces different dimensions than schema expects.

**Solution**: Runtime validation
```rust
fn validate_embedding_dimension(
    embeddings: &[Embedding],
    expected_dim: usize,
) -> Result<()> {
    if let Some(first) = embeddings.first() {
        if first.vec.len() != expected_dim {
            return Err(CacheError::DimensionMismatch {
                expected: expected_dim,
                actual: first.vec.len(),
            }.into());
        }
    }
    Ok(())
}
```

### 6. Query Cookbook Row Count < 256

**Problem**: IVF-PQ indexing requires minimum 256 rows.

**Solution**: Use Exact Nearest Neighbor (ENN) for small datasets
```rust
let search_params = if num_rows < 256 {
    SearchParams {
        index_type: IndexType::ExactNearestNeighbor,
        ..Default::default()
    }
} else {
    SearchParams::default()  // Uses IVF-PQ
};
```

### 7. Cache Directory Permissions

**Problem**: Cache directory not writable (e.g., restricted environments).

**Solution**: Graceful degradation
```rust
fn ensure_cache_dir() -> Result<PathBuf> {
    let cache_dir = get_cache_dir()?;
    match std::fs::create_dir_all(&cache_dir) {
        Ok(_) => Ok(cache_dir),
        Err(e) => {
            warn!("Cannot create cache directory: {}", e);
            warn!("Proceeding without cache (embeddings will regenerate)");
            Err(e.into())
        }
    }
}
```

### 8. API Version Changes (Field Metadata)

**Problem**: Google Ads API updates with new fields or field property changes.

**Solution**: Hash includes `api_version` - automatic invalidation on upgrade
```rust
fn compute_field_hash(cache: &FieldMetadataCache) -> u64 {
    let mut hasher = DefaultHasher::new();
    cache.api_version.hash(&mut hasher);  // ✅ Already included
    // ...
}
```

### 9. Large Embedding Batches (Memory Pressure)

**Problem**: Embedding 4000+ documents may spike memory during generation.

**Solution**: Batch embedding generation (if needed)
```rust
async fn embed_in_batches<T>(
    model: &EmbeddingModel,
    documents: &[T],
    batch_size: usize,
) -> Result<Vec<Embedding>>
where
    T: EmbedData,
{
    let mut all_embeddings = Vec::new();
    for chunk in documents.chunks(batch_size) {
        let embeddings = model.embed_documents(chunk).await?;
        all_embeddings.extend(embeddings);
    }
    Ok(all_embeddings)
}

// Usage
let embeddings = embed_in_batches(&model, &docs, 500).await?;
```

### 10. Development vs Production

**Problem**: Developers may want to force cache refresh during testing.

**Solution**: CLI flag or environment variable
```rust
// Add CLI flag
#[arg(long, help = "Force refresh of embedding cache")]
refresh_cache: bool,

// In cache logic
if args.refresh_cache {
    info!("Forcing cache refresh (--refresh-cache flag)");
    invalidate_all_caches()?;
}

// Or environment variable
if std::env::var("MCC_GAQL_REFRESH_CACHE").is_ok() {
    info!("Forcing cache refresh (MCC_GAQL_REFRESH_CACHE set)");
    invalidate_all_caches()?;
}
```

---

## Alternatives Considered

### Alternative 1: Fix InMemoryVectorStore Serialization

**Approach**: Implement `to_vector_store()` method in existing `EmbeddingCache`.

**Pros**:
- Minimal dependency changes
- Reuse existing bincode infrastructure

**Cons**:
- rig-core's `InMemoryVectorStore` has no public reconstruction API
- Would require forking rig-core or unsafe code
- Fragile: breaks on rig-core updates
- Still holds all vectors in RAM

**Verdict**: ❌ Rejected (too fragile, doesn't solve memory issue)

### Alternative 2: Use Serde for InMemoryVectorStore

**Approach**: Derive `Serialize`/`Deserialize` on `InMemoryVectorStore`.

**Pros**:
- Simplest solution if it worked

**Cons**:
- `InMemoryVectorStore` is from external crate (can't add derives)
- Would need to wrap entire struct (complex)
- No control over internal representation

**Verdict**: ❌ Rejected (not feasible)

### Alternative 3: Keep Parallel Raw Embedding Cache

**Approach**: Cache embeddings separately, rebuild `InMemoryVectorStore` on load.

**Pros**:
- No new dependencies
- Works with current architecture

**Cons**:
- Still requires rebuilding vector store (~1-2s overhead)
- Duplicate data storage (embeddings + vector store)
- Doesn't solve memory pressure
- Manual cache management complexity

**Verdict**: ⚠️ Viable but suboptimal (95% solution vs 100%)

### Alternative 4: Switch to Different Embedding Library

**Approach**: Use library with built-in serialization.

**Pros**:
- Clean cache support

**Cons**:
- Large refactor of RAG system
- Lose rig-core's agent abstractions
- May not find library with same quality/performance

**Verdict**: ❌ Rejected (too invasive, rig-core provides value)

### Alternative 5: Use PostgreSQL with pgvector

**Approach**: Store embeddings in PostgreSQL with pgvector extension.

**Pros**:
- Production-grade persistence
- SQL query capabilities
- Remote access possible

**Cons**:
- Requires PostgreSQL server
- Heavy dependency for CLI tool
- Setup complexity for users
- Network overhead (even with localhost)

**Verdict**: ❌ Rejected (overkill for CLI tool)

### Alternative 6: Use LanceDB (SELECTED)

**Approach**: Migrate to LanceDB for automatic persistence.

**Pros**:
- ✅ Native disk persistence (no rebuild needed)
- ✅ Memory efficient (lazy loading)
- ✅ Versioning built-in
- ✅ Incremental updates possible
- ✅ Compatible with rig-core (`VectorStoreIndex` trait)
- ✅ Production-ready format
- ✅ No external server required

**Cons**:
- ⚠️ Additional dependencies (lancedb, arrow)
- ⚠️ Build requirement (protoc)
- ⚠️ Schema management complexity
- ⚠️ Migration effort (medium)

**Verdict**: ✅ **SELECTED** (best balance of performance, maintainability, and scalability)

---

## Success Criteria

### Functional Requirements

1. ✅ **Embeddings cached on first run**
   - Query cookbook embeddings saved to LanceDB
   - Field metadata embeddings saved to LanceDB
   - Hash files created for validation

2. ✅ **Cache loaded on subsequent runs**
   - LanceDB tables loaded if hash matches
   - <1 second load time (vs ~18-20s rebuild)

3. ✅ **Cache invalidated on content changes**
   - Hash mismatch detected
   - Automatic rebuild triggered
   - New cache saved

4. ✅ **Correct query results**
   - Same RAG results as InMemoryVectorStore
   - No degradation in retrieval quality
   - No degradation in LLM output quality

### Performance Requirements

1. ✅ **Cold start (no cache)**: ≤20 seconds
   - Same as current implementation
   - Acceptable since rare

2. ✅ **Warm start (valid cache)**: ≤1 second
   - 95%+ improvement over current
   - Primary success metric

3. ✅ **Cache validation**: ≤100ms
   - Hash computation + file read
   - Negligible overhead

4. ✅ **Memory usage**: <2 MB resident
   - Down from ~8 MB current
   - Allows scaling to more fields

### Reliability Requirements

1. ✅ **Corruption recovery**: Automatic fallback to rebuild
2. ✅ **Concurrent access**: Safe (read-only for now)
3. ✅ **Disk space**: <20 MB total (well within limits)
4. ✅ **No data loss**: Cache is regenerable from source

### Developer Experience

1. ✅ **Clear logging**: Cache hit/miss, invalidation reasons
2. ✅ **Easy debugging**: `--refresh-cache` flag to force rebuild
3. ✅ **Documentation**: README updated with protoc requirement
4. ✅ **Testing**: Unit tests for conversion, integration tests for cache

### Monitoring & Validation

**Logging Example**:
```
INFO  Query cookbook cache valid, loading from LanceDB...
INFO  Successfully loaded query cookbook from cache (45ms)
INFO  Field metadata cache valid, loading from LanceDB...
INFO  Successfully loaded field metadata from cache (412ms)
INFO  Total embedding initialization: 487ms (saved ~18 seconds)
```

**Metrics to Track**:
- Cache hit rate (should be >90%)
- Load time distribution
- Rebuild frequency
- Disk space usage trends

---

## Conclusion

### Summary

This design replaces the incomplete `InMemoryVectorStore` caching with a production-ready LanceDB solution that:

1. **Eliminates 18-20 second overhead** for cached runs (95%+ improvement)
2. **Reduces memory footprint** from ~8 MB to <2 MB
3. **Provides automatic persistence** without manual cache management
4. **Maintains correctness** with hash-based validation
5. **Scales gracefully** as field metadata grows

### Next Steps

1. **Review & Approval**: Team reviews design document
2. **Prototype**: Phase 1 implementation (dependencies + utilities)
3. **Validation**: Phase 2 (field metadata migration) + performance testing
4. **Rollout**: Phase 3 (query cookbook) + Phase 4 (cleanup)
5. **Documentation**: Update README, add troubleshooting guide

### Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| LanceDB API changes | Medium | Pin to specific version, monitor releases |
| Schema evolution issues | Low | Version schema, automatic migration |
| Cache corruption | Low | Automatic fallback, clear error messages |
| protoc installation friction | Low | Document in README, check in build.rs |
| Performance regression | Low | Benchmark before/after, keep metrics |

### Timeline Estimate

- Phase 1 (Foundation): 1-2 days
- Phase 2 (Field Metadata): 2-3 days
- Phase 3 (Query Cookbook): 1-2 days
- Phase 4 (Cleanup): 1 day
- **Total**: ~5-8 days

### Questions for Review

1. Should we add a `--cache-stats` command to inspect cache health?
2. Should we expose search parameters (ENN vs IVF-PQ) as CLI options?
3. Should we add telemetry for cache hit rates (opt-in)?
4. Should we version the cache directory structure for backward compatibility?

---

**End of Design Document**

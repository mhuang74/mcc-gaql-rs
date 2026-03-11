# Plan: Configure Fastembed Model Cache Directory

## Context
The fastembed library downloads embedding model files (like BGE-Small) to the current working directory by default. This pollutes the working directory and can cause issues if the user runs the tool from different directories. The fastembed library respects the `HF_HOME` or `FASTEMBED_CACHE_DIR` environment variables for controlling where models are cached.

## Goal
Ensure fastembed model files are cached to `$HOME/.cache/mcc-gaql/fastembed-models/` instead of the current directory.

## Approach
Set the `HF_HOME` environment variable before creating the fastembed client, pointing it to the appropriate cache directory under `$HOME/.cache/mcc-gaql/fastembed-models/`.

## Files to Modify

### 1. `crates/mcc-gaql-gen/src/rag.rs`
**Location**: Lines 167-171 (the `create_embedding_client` function)

**Current code**:
```rust
fn create_embedding_client() -> (rig_fastembed::Client, rig_fastembed::EmbeddingModel) {
    let fastembed_client = rig_fastembed::Client::new();
    let embedding_model = fastembed_client.embedding_model(&FastembedModel::BGESmallENV15);
    (fastembed_client, embedding_model)
}
```

**Changes needed**:
- Before creating the client, set `HF_HOME` environment variable to the cache directory
- Use the `dirs` crate (already a dependency) to get the cache directory
- Create the cache directory if it doesn't exist

**Implementation**:
```rust
fn create_embedding_client() -> Result<(rig_fastembed::Client, rig_fastembed::EmbeddingModel), anyhow::Error> {
    // Set HF_HOME to cache fastembed models in the proper location
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get cache directory"))?
        .join("mcc-gaql")
        .join("fastembed-models");

    std::fs::create_dir_all(&cache_dir)?;

    // fastembed uses HF_HOME to determine where to cache models
    std::env::set_var("HF_HOME", &cache_dir);

    let fastembed_client = rig_fastembed::Client::new();
    let embedding_model = fastembed_client.embedding_model(&FastembedModel::BGESmallENV15);
    Ok((fastembed_client, embedding_model))
}
```

**Impact**:
- Return type changes from direct tuple to `Result<..., anyhow::Error>`
- All call sites need to handle the Result: line 183 in `init_llm_resources` function

### 2. Update call site in `init_llm_resources` function
**Location**: Lines 181-189

**Current code**:
```rust
fn init_llm_resources(config: &LlmConfig) -> Result<AgentResources, anyhow::Error> {
    let llm_client = config.create_llm_client()?;
    let (embed_client, embedding_model) = create_embedding_client();

    Ok(AgentResources {
        llm_client,
        embed_client,
        embedding_model,
    })
}
```

**Changes needed**:
- Add `?` to handle the Result from `create_embedding_client`

### 3. Update test file `crates/mcc-gaql-gen/tests/field_vector_store_rag_tests.rs`
**Location**: Lines 12-16

**Current code**:
```rust
fn get_shared_embedding_model() -> &'static rig_fastembed::EmbeddingModel {
    static MODEL: OnceLock<rig_fastembed::EmbeddingModel> = OnceLock::new();
    MODEL.get_or_init(|| {
        let fastembed_client = FastembedClient::new();
        fastembed_client.embedding_model(&FastembedModel::BGESmallENV15)
    })
}
```

**Changes needed**:
- Set `HF_HOME` before creating the client
- Similar to the main code changes

### 4. Update test file `crates/mcc-gaql-gen/tests/minimal_rag_test.rs`
**Location**: Lines 257-259

**Current code**:
```rust
let fastembed_client = FastembedClient::new();
let embedding_model = fastembed_client.embedding_model(&FastembedModel::BGESmallENV15);
```

**Changes needed**:
- Set `HF_HOME` before creating the client

## Verification

1. **Run tests**: After the change, run the tests to ensure they pass
   ```bash
   cargo test -p mcc-gaql-gen --lib
   ```

2. **Manual verification**:
   - Delete any local `~/.cache/mcc-gaql/fastembed-models/` directory if it exists
   - Run a command that uses embeddings (e.g., `cargo run -p mcc-gaql-gen -- generate "test"`)
   - Verify the model files are downloaded to `~/.cache/mcc-gaql/fastembed-models/` instead of the current directory
   - Check for any `models/` or `fastembed/` directories in the project root - they should not exist

## Dependencies
No new dependencies needed. The `dirs` crate is already a dependency of `mcc-gaql-gen`.

## Backwards Compatibility
This change is backwards compatible - it only affects where the model files are cached. Existing cached models in the old location will be ignored and re-downloaded to the new location.

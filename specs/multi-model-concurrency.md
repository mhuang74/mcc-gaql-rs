# Multi-Model LLM Concurrency

## Overview

Refactor the LLM handling to support multiple model names with concurrency of 1 per model. This allows increased throughput for high-volume LLM operations (like metadata enrichment) while respecting API provider rate limits.

## Problem Statement

The LLM API provider only supports concurrency of 1 per model. Currently, the codebase:
- Configures a single model via `MCC_GAQL_LLM_MODEL` environment variable
- Uses `buffer_unordered(3)` in `metadata_enricher.rs`, potentially hitting rate limits

## Requirements

1. Support multiple LLM models via comma-separated environment variable
2. First model in the list is the "preferred" model for single requests
3. `prompt2gaql` always uses the preferred model (single request, no change in behavior)
4. `metadata_enricher` distributes requests across all models with least-busy-first strategy
5. Maximum concurrency per model = 1 (enforced via semaphores)

## Design

### Configuration

**Environment variable**: `MCC_GAQL_LLM_MODEL`

```bash
# Single model (backward compatible)
export MCC_GAQL_LLM_MODEL="google/gemini-flash-2.0"

# Multiple models (new)
export MCC_GAQL_LLM_MODEL="google/gemini-flash-2.0,openai/gpt-4o-mini,anthropic/claude-3-haiku"
```

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         LlmConfig                                │
│  - api_key, base_url, temperature                               │
│  - models: Vec<String>  (was: model: String)                    │
│  - preferred_model() -> &str                                    │
│  - all_models() -> &[String]                                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         ModelPool                                │
│  - config: Arc<LlmConfig>                                       │
│  - semaphores: Vec<Arc<Semaphore>>  (1 permit each)            │
│                                                                  │
│  + acquire() -> ModelLease       // least-busy-first           │
│  + acquire_preferred() -> ModelLease  // always model[0]       │
│  + model_count() -> usize                                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                         ModelLease                               │
│  - model_index: usize                                           │
│  - model_name: String                                           │
│  - config: Arc<LlmConfig>                                       │
│  - _permit: OwnedSemaphorePermit  (releases on drop)           │
│                                                                  │
│  + create_agent(system_prompt) -> Agent<CompletionModel>        │
│  + model_name() -> &str                                         │
└─────────────────────────────────────────────────────────────────┘
```

### Component Interactions

```
┌──────────────────┐     ┌──────────────────┐
│   prompt2gaql    │     │ metadata_enricher │
│                  │     │                   │
│ Uses preferred   │     │ Uses all models   │
│ model only       │     │ via ModelPool     │
└────────┬─────────┘     └────────┬──────────┘
         │                        │
         │  acquire_preferred()   │  acquire() (least-busy)
         │                        │
         ▼                        ▼
    ┌─────────────────────────────────────┐
    │             ModelPool               │
    │                                     │
    │  ┌─────────┐ ┌─────────┐ ┌────────┐│
    │  │Model[0] │ │Model[1] │ │Model[2]││
    │  │Sem(1)   │ │Sem(1)   │ │Sem(1)  ││
    │  └─────────┘ └─────────┘ └────────┘│
    └─────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Extend `LlmConfig` to Support Multiple Models

**File**: `src/prompt2gaql.rs`

**Changes**:

1. Change `model: String` to `models: Vec<String>` in `LlmConfig` struct
2. Update `from_env()` to parse comma-separated model names:

```rust
let models: Vec<String> = env::var("MCC_GAQL_LLM_MODEL")
    .expect("MCC_GAQL_LLM_MODEL must be set")
    .split(',')
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
    .collect();

if models.is_empty() {
    panic!("MCC_GAQL_LLM_MODEL must contain at least one model");
}
```

3. Add helper methods:

```rust
impl LlmConfig {
    /// Returns the first (preferred) model
    pub fn preferred_model(&self) -> &str {
        &self.models[0]
    }

    /// Returns all configured models
    pub fn all_models(&self) -> &[String] {
        &self.models
    }

    /// Returns the number of configured models
    pub fn model_count(&self) -> usize {
        self.models.len()
    }
}
```

4. Update existing methods that use `self.model` to use `self.preferred_model()`:
   - `create_agent()`
   - Any other direct `self.model` references

### Phase 2: Create `ModelPool` Abstraction

**New file**: `src/model_pool.rs`

```rust
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use rig::agent::Agent;
use rig::providers::openai::CompletionModel;

use crate::prompt2gaql::LlmConfig;

/// Pool of LLM models with per-model concurrency control.
///
/// Each model has a semaphore with 1 permit, ensuring only one
/// request per model at any time.
pub struct ModelPool {
    config: Arc<LlmConfig>,
    semaphores: Vec<Arc<Semaphore>>,
}

impl ModelPool {
    /// Create a new model pool from LLM configuration.
    pub fn new(config: Arc<LlmConfig>) -> Self {
        let semaphores = config
            .all_models()
            .iter()
            .map(|_| Arc::new(Semaphore::new(1)))
            .collect();

        Self { config, semaphores }
    }

    /// Returns the number of models in the pool.
    pub fn model_count(&self) -> usize {
        self.semaphores.len()
    }

    /// Acquire any available model using least-busy-first strategy.
    ///
    /// Tries models in order of preference (index 0 first).
    /// If all models are busy, waits for the first one to become available.
    pub async fn acquire(&self) -> ModelLease {
        // First, try to acquire without waiting (prefer earlier models)
        for (idx, sem) in self.semaphores.iter().enumerate() {
            if let Ok(permit) = sem.clone().try_acquire_owned() {
                log::debug!(
                    "Acquired model {} (index {}) immediately",
                    self.config.all_models()[idx],
                    idx
                );
                return ModelLease {
                    model_index: idx,
                    model_name: self.config.all_models()[idx].clone(),
                    config: Arc::clone(&self.config),
                    _permit: permit,
                };
            }
        }

        // All busy - wait for any model to become available
        log::debug!("All models busy, waiting for availability...");

        // Create futures for all semaphores
        let futures: Vec<_> = self.semaphores
            .iter()
            .enumerate()
            .map(|(idx, sem)| {
                let sem = Arc::clone(sem);
                async move {
                    let permit = sem.acquire_owned().await.unwrap();
                    (idx, permit)
                }
            })
            .collect();

        // Race all futures, take the first one that completes
        let (idx, permit) = futures::future::select_all(futures)
            .await
            .0;

        log::debug!(
            "Acquired model {} (index {}) after waiting",
            self.config.all_models()[idx],
            idx
        );

        ModelLease {
            model_index: idx,
            model_name: self.config.all_models()[idx].clone(),
            config: Arc::clone(&self.config),
            _permit: permit,
        }
    }

    /// Acquire the preferred (first) model specifically.
    ///
    /// Use this for operations that should always use the primary model.
    pub async fn acquire_preferred(&self) -> ModelLease {
        let permit = self.semaphores[0].clone().acquire_owned().await.unwrap();

        ModelLease {
            model_index: 0,
            model_name: self.config.all_models()[0].clone(),
            config: Arc::clone(&self.config),
            _permit: permit,
        }
    }
}

/// A lease on a specific model. Releases the model when dropped.
pub struct ModelLease {
    model_index: usize,
    model_name: String,
    config: Arc<LlmConfig>,
    _permit: OwnedSemaphorePermit,
}

impl ModelLease {
    /// Returns the model name for this lease.
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Returns the model index (0 = preferred).
    pub fn model_index(&self) -> usize {
        self.model_index
    }

    /// Create an LLM agent for this model with the given system prompt.
    pub fn create_agent(&self, system_prompt: &str) -> Agent<CompletionModel> {
        self.config.create_agent_for_model(&self.model_name, system_prompt)
    }
}

impl Drop for ModelLease {
    fn drop(&mut self) {
        log::debug!("Released model {} (index {})", self.model_name, self.model_index);
    }
}
```

**Additional method needed in `LlmConfig`**:

```rust
impl LlmConfig {
    /// Create an agent for a specific model name.
    pub fn create_agent_for_model(
        &self,
        model: &str,
        system_prompt: &str,
    ) -> Agent<CompletionModel> {
        let client = self.create_llm_client();
        client
            .agent(model)
            .preamble(system_prompt)
            .temperature(self.temperature)
            .build()
    }
}
```

### Phase 3: Update `metadata_enricher.rs`

**File**: `src/metadata_enricher.rs`

**Changes**:

1. Update `MetadataEnricher` struct:

```rust
// Before
pub struct MetadataEnricher {
    llm_config: LlmConfig,
    batch_size: usize,
    concurrency: usize,  // Remove this
}

// After
pub struct MetadataEnricher {
    model_pool: Arc<ModelPool>,
    batch_size: usize,
    // concurrency derived from model_pool.model_count()
}
```

2. Update constructor:

```rust
impl MetadataEnricher {
    pub fn new(model_pool: Arc<ModelPool>) -> Self {
        Self {
            model_pool,
            batch_size: 15,
        }
    }

    // Or with builder pattern if preferred
    pub fn with_batch_size(mut self, batch_size: usize) -> Self {
        self.batch_size = batch_size;
        self
    }
}
```

3. Update `enrich()` method to use `ModelPool`:

```rust
pub async fn enrich(&self, ...) -> Result<...> {
    // ... batch preparation code unchanged ...

    let model_pool = Arc::clone(&self.model_pool);
    let scraped = Arc::new(scraped_docs);

    let results: Vec<_> = stream::iter(all_batches.into_iter().enumerate())
        .map(|(idx, (resource, batch_fields))| {
            let pool = Arc::clone(&model_pool);
            let scraped = Arc::clone(&scraped);

            async move {
                // Acquire a model from the pool (waits if all busy)
                let lease = pool.acquire().await;

                log::info!(
                    "Batch {}: enriching {} fields from {} using model {}",
                    idx,
                    batch_fields.len(),
                    resource,
                    lease.model_name()
                );

                let result = Self::enrich_batch_with_lease(
                    &lease,
                    &resource,
                    &batch_fields,
                    &scraped,
                ).await;

                // lease dropped here, model released
                result
            }
        })
        .buffer_unordered(self.model_pool.model_count())
        .collect()
        .await;

    // ... rest unchanged ...
}
```

4. Update `enrich_batch_static` to accept `ModelLease`:

```rust
// Before
async fn enrich_batch_static(
    llm_config: &LlmConfig,
    ...
) -> Result<...>

// After
async fn enrich_batch_with_lease(
    lease: &ModelLease,
    resource: &str,
    fields: &[String],
    scraped: &ScrapedDocs,
) -> Result<BatchEnrichmentResult> {
    let agent = lease.create_agent(ENRICHMENT_SYSTEM_PROMPT);

    let user_prompt = format!(...);

    let response = agent.prompt(&user_prompt).await?;

    // ... parse response ...
}
```

### Phase 4: Update `prompt2gaql.rs` (Minimal Changes)

**File**: `src/prompt2gaql.rs`

**Changes**:

1. Ensure `create_agent()` uses `preferred_model()`:

```rust
impl LlmConfig {
    pub fn create_agent(&self, system_prompt: &str) -> Agent<CompletionModel> {
        self.create_agent_for_model(self.preferred_model(), system_prompt)
    }
}
```

2. No changes needed to `RAGAgent` or `EnhancedRAGAgent` - they continue using `config.create_agent()` which now uses the preferred model.

### Phase 5: Update `main.rs` Integration

**File**: `src/main.rs`

**Changes**:

1. Create `ModelPool` at startup when LLM features are used:

```rust
#[cfg(feature = "llm")]
let model_pool = {
    let llm_config = Arc::new(LlmConfig::from_env());
    log::info!(
        "LLM configured with {} model(s): {:?}",
        llm_config.model_count(),
        llm_config.all_models()
    );
    Arc::new(ModelPool::new(llm_config))
};
```

2. Pass `model_pool` to `MetadataEnricher`:

```rust
// Before
let enricher = MetadataEnricher::new(LlmConfig::from_env());

// After
let enricher = MetadataEnricher::new(Arc::clone(&model_pool));
```

3. For `prompt2gaql`, continue passing `LlmConfig` directly (simpler API for single-use):

```rust
// No change needed - convert_to_gaql() still takes LlmConfig
// and always uses the preferred model internally
```

### Phase 6: Update Module Exports

**File**: `src/lib.rs`

```rust
#[cfg(feature = "llm")]
pub mod model_pool;

#[cfg(feature = "llm")]
pub use model_pool::{ModelPool, ModelLease};
```

## File Summary

| File | Action | Description |
|------|--------|-------------|
| `src/model_pool.rs` | **Create** | New module with `ModelPool` and `ModelLease` |
| `src/prompt2gaql.rs` | **Modify** | Multi-model config, `preferred_model()`, `create_agent_for_model()` |
| `src/metadata_enricher.rs` | **Modify** | Use `ModelPool` for concurrent access |
| `src/main.rs` | **Modify** | Create `ModelPool`, wire dependencies |
| `src/lib.rs` | **Modify** | Export new module |

## Testing Strategy

### Unit Tests

1. **`LlmConfig` parsing**:
   - Single model: `"model-a"` → `["model-a"]`
   - Multiple models: `"model-a,model-b,model-c"` → `["model-a", "model-b", "model-c"]`
   - With whitespace: `" model-a , model-b "` → `["model-a", "model-b"]`
   - Empty string: panic
   - Empty after split: `",,"` → panic

2. **`ModelPool::acquire()`**:
   - With 3 models and 3 concurrent tasks, each gets a different model
   - With 3 models and 5 concurrent tasks, 2 tasks wait

3. **`ModelPool::acquire_preferred()`**:
   - Always returns model index 0
   - Multiple calls wait for each other

### Integration Tests

1. Mock HTTP server that tracks request concurrency per model
2. Run `MetadataEnricher` with 3 models
3. Verify max 1 concurrent request per model

### Manual Testing

```bash
# Set multiple models
export MCC_GAQL_LLM_MODEL="google/gemini-flash-2.0,openai/gpt-4o-mini"
export MCC_GAQL_LOG_LEVEL="debug"

# Run metadata enrichment
cargo run -- --enrich-metadata --profile test

# Observe logs showing model distribution
```

## Backward Compatibility

- Single model configuration continues to work unchanged
- `prompt2gaql` behavior unchanged (always uses first model)
- `metadata_enricher` concurrency now equals model count instead of hardcoded 3

## Future Considerations

1. **Model-specific parameters**: Different temperatures or system prompts per model
2. **Retry with different model**: On failure, try next available model
3. **Model health tracking**: Track success rates, prefer healthy models
4. **Config file support**: Add `[llm]` section to `config.toml` for persistent multi-model config

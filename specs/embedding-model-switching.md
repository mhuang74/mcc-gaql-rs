# Embedding Model Switching Design Specification

**Status:** Proposed
**Author:** System Design
**Date:** 2025-11-10
**Related Docs:**
- specs/embedding-cache-design.md
- specs/rag-quality-improvement-plan.md

---

## Executive Summary

This specification proposes a comprehensive redesign of the embedding system to enable seamless experimentation with different embedding models. The current implementation hardcodes the embedding model (AllMiniLML6V2Q, 384 dimensions) and does not track which model was used for cached embeddings, making model switching error-prone and requiring manual cache invalidation.

**Key Improvements:**
1. **Embedding Model Metadata Tracking** - Cache stores model identifier and dimension
2. **Automatic Cache Invalidation** - Embeddings regenerated when model changes
3. **Dynamic Dimension Support** - No hardcoded dimensions in schemas
4. **Model Configuration System** - Easy switching via configuration
5. **Test-Only Experimentation Mode** - Optional in-memory embedding testing

---

## 1. Current State Analysis

### 1.1 Current Architecture

**Embedding Model:**
- Model: `AllMiniLML6V2Q` (FastEmbed)
- Dimensions: 384 (hardcoded as `EMBEDDING_DIM` constant)
- Distance Metric: Cosine similarity
- Storage: LanceDB with Float64 precision

**Cache Structure:**
```
~/.cache/mcc-gaql/
├── lancedb/
│   ├── query_cookbook.lance/
│   └── field_metadata.lance/
├── query_cookbook.hash      # Format: "v1\n<hash>"
├── field_metadata.hash       # Format: "v1\n<hash>"
└── field_metadata.json       # Raw field cache
```

**Key Files:**
- `src/lancedb_utils.rs` - Schema definitions, EMBEDDING_DIM = 384
- `src/prompt2gaql.rs` - Model selection, hash computation, RAG agents
- `src/util.rs` - Data structures (QueryEntry, FieldMetadata)

### 1.2 Problems with Current Design

#### Problem 1: No Model Tracking
Hash files only track content and schema version, not the embedding model:
```rust
// Current hash format
format!("v{}\n{}", SCHEMA_VERSION, hash)
```

**Impact:** If the model changes, cache appears valid but contains embeddings from the old model, causing:
- Incorrect semantic search results
- Dimension mismatches (crash if dimensions differ)
- Silent degradation of RAG quality

#### Problem 2: Hardcoded Dimensions
```rust
// src/lancedb_utils.rs:16
const EMBEDDING_DIM: i32 = 384;

// Used in schema definitions (lines 87-95, 110-117)
Field::new(
    "vector",
    DataType::FixedSizeList(
        Arc::new(Field::new("item", DataType::Float64, true)),
        EMBEDDING_DIM,  // Hardcoded
    ),
    false,
)
```

**Impact:** Switching to a model with different dimensions requires:
- Code changes in `lancedb_utils.rs`
- Recompilation
- Manual cache deletion
- No runtime flexibility

#### Problem 3: Hardcoded LLM Model
LLM model selection is hardcoded in RAG agent initialization:
```rust
// src/prompt2gaql.rs:454, 567
let agent = openrouter_client.agent(openrouter::GEMINI_FLASH_2_0)
    .preamble("...")
    .temperature(0.1)  // Also hardcoded
    .build();
```

**Impact:** Experimenting with different LLM models (e.g., GPT-4, Claude, Llama) requires:
- Code changes in multiple locations (RAGAgent and EnhancedRAGAgent)
- Recompilation
- No runtime experimentation capability
- Cannot A/B test different models for GAQL generation quality

#### Problem 4: Unclear Cache Invalidation Strategy
Current cache invalidation triggers:
- Content hash changes (queries or fields modified)
- Schema version bump (manual)

Missing triggers:
- Embedding model change
- Model version/configuration change
- Dimension change

#### Problem 5: Tight Coupling
Both embedding and LLM model selection are embedded in the code:
```rust
// Embedding: src/prompt2gaql.rs:449, 549
let embedding_model = fastembed_client.embedding_model(&FastembedModel::AllMiniLML6V2Q);

// LLM: src/prompt2gaql.rs:454, 567
let agent = openrouter_client.agent(openrouter::GEMINI_FLASH_2_0)
```

**Impact:** Changing models requires code changes in multiple locations, preventing rapid experimentation.

---

## 2. Requirements

### 2.1 Functional Requirements

**FR1: Model Metadata Tracking**
- Cache must store embedding model identifier
- Cache must store embedding dimension
- Cache must store model configuration parameters (if applicable)

**FR2: Automatic Cache Invalidation**
- Cache automatically invalidated when model identifier changes
- Cache automatically invalidated when model configuration changes
- Clear error messages when cache is stale

**FR3: Dynamic Dimension Support**
- LanceDB schemas accept variable-length vectors
- No hardcoded dimension constants
- Runtime dimension detection from model

**FR4: Configuration-Based Model Selection**
- Embedding model specified in configuration file or environment variable
- LLM model specified in configuration file or environment variable
- Support for multiple embedding providers (FastEmbed, OpenAI, Ollama, etc.)
- Support for multiple LLM providers (OpenRouter, OpenAI, Ollama, Anthropic, etc.)
- Easy switching without code changes

**FR5: Environment Variable Override**
- All configuration values can be overridden at runtime via environment variables
- Clear precedence order: ENV > Config File > Defaults
- Enable quick experimentation without editing config files

**FR6: Backward Compatibility**
- Existing caches with old format gracefully invalidated
- Migration path from v1 to v2 cache format

### 2.2 Non-Functional Requirements

**NFR1: Performance**
- Cache invalidation overhead < 100ms
- No performance degradation for cache hits
- Model switching should not affect warm-start performance (<1s)

**NFR2: Storage Efficiency**
- Metadata storage overhead < 1KB per vector store
- Support for larger embedding dimensions (up to 1536+)

**NFR3: Developer Experience**
- Single configuration change to switch models
- Clear error messages for dimension mismatches
- Comprehensive logging for cache operations

**NFR4: Testing Support**
- Test-only mode for embedding experimentation without persistent cache
- Integration test fixtures for multiple embedding models
- Benchmark comparisons between models

---

## 3. Proposed Solution

### 3.1 Architecture Overview

```
┌───────────────────────────────────────────────────────────────────────────┐
│                         Configuration Layer                                │
│  ┌─────────────────────────────┐  ┌─────────────────────────────────────┐ │
│  │  Embedding Configuration    │  │     LLM Configuration               │ │
│  │  - Model: AllMiniLML6V2Q    │  │     - Provider: OpenRouter          │ │
│  │  - Provider: FastEmbed      │  │     - Model: GEMINI_FLASH_2_0       │ │
│  │  - Dimension: Auto-detect   │  │     - Temperature: 0.1              │ │
│  │  - Distance: Cosine         │  │     - Max Tokens: 2048              │ │
│  └─────────────────────────────┘  └─────────────────────────────────────┘ │
│              ↓                                      ↓                       │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │   Environment Variable Override (Highest Precedence)               │   │
│  │   MCC_GAQL_EMBEDDING_MODEL, MCC_GAQL_LLM_MODEL, etc.              │   │
│  └────────────────────────────────────────────────────────────────────┘   │
└───────────────────────────────────────────────────────────────────────────┘
                             ↓                        ↓
         ┌───────────────────────────────────────────────────────┐
         │              Model Registry Layer                      │
         │  ┌──────────────────┐      ┌──────────────────────┐  │
         │  │ Embedding Models │      │    LLM Providers     │  │
         │  │ - FastEmbed      │      │    - OpenRouter      │  │
         │  │ - OpenAI         │      │    - OpenAI          │  │
         │  │ - Ollama         │      │    - Anthropic       │  │
         │  └──────────────────┘      │    - Ollama          │  │
         │                             └──────────────────────┘  │
         └───────────────────────────────────────────────────────┘
                             ↓                        ↓
┌─────────────────────────────────────────────────────────────────────┐
│                      Cache Validation Layer                          │
│  1. Load embedding config (from ENV or file)                         │
│  2. Check if cache exists                                            │
│  3. Load cache metadata (model_id, dimension, schema_ver)            │
│  4. Compare with current model config                                │
│  5. If mismatch → invalidate & rebuild cache                         │
│  6. If match → load vector store                                     │
└─────────────────────────────────────────────────────────────────────┘
                             ↓
┌─────────────────────────────────────────────────────────────────────┐
│                     RAG Agent (GAQL Generation)                      │
│  ┌────────────────┐                            ┌─────────────────┐  │
│  │ Vector Store   │  ← Query Embedding →       │   LLM Model     │  │
│  │  (LanceDB)     │  ← Retrieve Context →      │  (Configured)   │  │
│  │ - Queries      │  ← Top-N Results →         │  - Generate     │  │
│  │ - Fields       │                             │    GAQL         │  │
│  └────────────────┘                            └─────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### 3.2 Core Components

#### Component 1: Model Configuration System

**Configuration File (`~/.config/mcc-gaql/config.toml`):**
```toml
#═══════════════════════════════════════════════════════════════
# Embedding Model Configuration
#═══════════════════════════════════════════════════════════════
[embedding]
# Model provider: "fastembed", "openai", "ollama"
provider = "fastembed"

# Model identifier (provider-specific)
# FastEmbed: AllMiniLML6V2Q, BgeSmallEnV15, BgeBaseEnV15
# OpenAI: text-embedding-3-small, text-embedding-3-large
# Ollama: mxbai-embed-large, nomic-embed-text
model = "AllMiniLML6V2Q"

# Optional: Override auto-detected dimension
# dimension = 384

# Distance metric for vector search: "cosine", "l2", "dot"
distance_metric = "cosine"

# Optional: Model-specific parameters
[embedding.params]
# For OpenAI models
# api_key_env = "OPENAI_API_KEY"
# For Ollama
# base_url = "http://localhost:11434"

#═══════════════════════════════════════════════════════════════
# LLM Model Configuration (for GAQL Generation)
#═══════════════════════════════════════════════════════════════
[llm]
# LLM provider: "openrouter", "openai", "anthropic", "ollama"
provider = "openrouter"

# Model identifier (provider-specific)
# OpenRouter: google/gemini-flash-1.5, anthropic/claude-3.5-sonnet, etc.
# OpenAI: gpt-4o, gpt-4-turbo
# Anthropic: claude-3-5-sonnet-20241022
# Ollama: llama3.1, mistral
model = "google/gemini-flash-1.5"

# Temperature: 0.0 (deterministic) to 1.0 (creative)
temperature = 0.1

# Optional: Maximum tokens in response
max_tokens = 2048

# Optional: Model-specific parameters
[llm.params]
# For OpenRouter
# api_key_env = "OPENROUTER_API_KEY"  # Default
# For OpenAI
# api_key_env = "OPENAI_API_KEY"
# For Anthropic
# api_key_env = "ANTHROPIC_API_KEY"
# For Ollama
# base_url = "http://localhost:11434"
```

---

### Environment Variable Override System

**Precedence Order (Highest → Lowest):**
```
1. Environment Variables  ← Runtime override (temporary)
2. Configuration File     ← Persistent user settings
3. Hardcoded Defaults     ← Fallback values
```

**All Supported Environment Variables:**

| Category | Environment Variable | Description | Example |
|----------|---------------------|-------------|---------|
| **Embedding** | `MCC_GAQL_EMBEDDING_PROVIDER` | Embedding provider | `fastembed` |
| | `MCC_GAQL_EMBEDDING_MODEL` | Embedding model name | `BgeSmallEnV15` |
| | `MCC_GAQL_EMBEDDING_DIMENSION` | Override dimension (optional) | `384` |
| | `MCC_GAQL_EMBEDDING_DISTANCE` | Distance metric | `cosine` |
| **LLM** | `MCC_GAQL_LLM_PROVIDER` | LLM provider | `openrouter` |
| | `MCC_GAQL_LLM_MODEL` | LLM model name | `anthropic/claude-3.5-sonnet` |
| | `MCC_GAQL_LLM_TEMPERATURE` | Temperature (0.0-1.0) | `0.1` |
| | `MCC_GAQL_LLM_MAX_TOKENS` | Max response tokens | `2048` |
| **API Keys** | `OPENROUTER_API_KEY` | OpenRouter API key | `sk-or-v1-...` |
| | `OPENAI_API_KEY` | OpenAI API key | `sk-...` |
| | `ANTHROPIC_API_KEY` | Anthropic API key | `sk-ant-...` |

**Quick Start Examples:**

```bash
# Example 1: Quick embedding model switch (one-time test)
MCC_GAQL_EMBEDDING_MODEL="BgeSmallEnV15" mcc-gaql -q "show campaigns"

# Example 2: Quick LLM model switch (one-time test)
MCC_GAQL_LLM_MODEL="anthropic/claude-3.5-sonnet" mcc-gaql -q "show campaigns"

# Example 3: Switch both models simultaneously
MCC_GAQL_EMBEDDING_MODEL="BgeBaseEnV15" \
MCC_GAQL_LLM_MODEL="openai/gpt-4o" \
mcc-gaql -q "show campaigns with high CTR"

# Example 4: Use local Ollama for both (no API costs)
MCC_GAQL_EMBEDDING_PROVIDER="ollama" \
MCC_GAQL_EMBEDDING_MODEL="nomic-embed-text" \
MCC_GAQL_LLM_PROVIDER="ollama" \
MCC_GAQL_LLM_MODEL="llama3.1" \
mcc-gaql -q "show campaigns"

# Example 5: Persistent override for current shell session
export MCC_GAQL_EMBEDDING_MODEL="BgeBaseEnV15"
export MCC_GAQL_LLM_MODEL="anthropic/claude-3.5-sonnet"
mcc-gaql -q "query 1"  # Uses BgeBaseEnV15 + Claude
mcc-gaql -q "query 2"  # Still uses same models

# Example 6: Override temperature for more creative GAQL
MCC_GAQL_LLM_TEMPERATURE="0.5" mcc-gaql -q "show top campaigns"
```

**Use Cases:**

| Scenario | Approach | Example |
|----------|----------|---------|
| **One-time experiment** | Single env var prefix | `MCC_GAQL_EMBEDDING_MODEL="..." mcc-gaql ...` |
| **Session-wide testing** | Export env vars | `export MCC_GAQL_EMBEDDING_MODEL="..."` |
| **Permanent change** | Edit config.toml | Edit `~/.config/mcc-gaql/config.toml` |
| **Project-specific** | Use .env file | Create `.env` with `MCC_GAQL_*` vars |
| **CI/CD testing** | Set in pipeline | GitHub Actions: `env: MCC_GAQL_*` |
| **Multi-user defaults** | System config | Edit `/etc/mcc-gaql/config.toml` (if supported) |

#### Component 2: Embedding Model Registry

**New Module: `src/embedding_config.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Embedding provider types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingProvider {
    FastEmbed,
    OpenAI,
    Ollama,
}

/// Embedding model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: EmbeddingProvider,
    pub model: String,
    pub dimension: Option<usize>,  // Auto-detect if None
    pub distance_metric: DistanceMetric,
    pub params: HashMap<String, String>,
}

/// Distance metric for vector search
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DistanceMetric {
    Cosine,
    L2,
    Dot,
}

impl EmbeddingConfig {
    /// Generate unique identifier for this model configuration
    /// Format: "{provider}:{model}:{dimension}:{config_hash}"
    pub fn model_identifier(&self) -> String {
        let config_hash = self.compute_config_hash();
        format!(
            "{}:{}:{}:{}",
            self.provider_str(),
            self.model,
            self.dimension.map_or("auto".to_string(), |d| d.to_string()),
            config_hash
        )
    }

    /// Compute hash of model parameters for cache invalidation
    fn compute_config_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        // Hash relevant parameters that affect embeddings
        for (k, v) in self.params.iter() {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        }
        format!("{:x}", hasher.finish())
    }

    /// Create embedding model instance
    pub async fn create_embedding_model(&self) -> Result<Box<dyn EmbeddingModel>> {
        match self.provider {
            EmbeddingProvider::FastEmbed => {
                // Create FastEmbed model
                self.create_fastembed_model().await
            }
            EmbeddingProvider::OpenAI => {
                // Create OpenAI model
                self.create_openai_model().await
            }
            EmbeddingProvider::Ollama => {
                // Create Ollama model
                self.create_ollama_model().await
            }
        }
    }

    /// Auto-detect embedding dimension by generating a test embedding
    pub async fn detect_dimension(&self) -> Result<usize> {
        let model = self.create_embedding_model().await?;
        let test_embedding = model.embed("test").await?;
        Ok(test_embedding.len())
    }
}

/// Trait for embedding models (abstraction over providers)
#[async_trait::async_trait]
pub trait EmbeddingModel: Send + Sync {
    async fn embed(&self, text: &str) -> Result<Vec<f64>>;
    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f64>>>;
    fn dimension(&self) -> usize;
}

/// Load configuration from file and environment variables
impl EmbeddingConfig {
    /// Load with environment variable override
    pub fn load() -> Result<Self> {
        // 1. Load defaults
        let mut config = Self::default();

        // 2. Override from config file (if exists)
        if let Ok(file_config) = Self::from_config_file() {
            config = file_config;
        }

        // 3. Override with environment variables (highest precedence)
        if let Ok(provider) = env::var("MCC_GAQL_EMBEDDING_PROVIDER") {
            config.provider = provider.parse()?;
        }
        if let Ok(model) = env::var("MCC_GAQL_EMBEDDING_MODEL") {
            config.model = model;
        }
        if let Ok(dim) = env::var("MCC_GAQL_EMBEDDING_DIMENSION") {
            config.dimension = Some(dim.parse()?);
        }
        if let Ok(metric) = env::var("MCC_GAQL_EMBEDDING_DISTANCE") {
            config.distance_metric = metric.parse()?;
        }

        Ok(config)
    }
}
```

---

#### Component 2b: LLM Model Configuration

**New Module: `src/llm_config.rs`**

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// LLM provider types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LLMProvider {
    OpenRouter,
    OpenAI,
    Anthropic,
    Ollama,
}

/// LLM model configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMConfig {
    pub provider: LLMProvider,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: Option<usize>,
    pub params: HashMap<String, String>,
}

impl LLMConfig {
    /// Load with environment variable override
    pub fn load() -> Result<Self> {
        // 1. Load defaults
        let mut config = Self::default();

        // 2. Override from config file (if exists)
        if let Ok(file_config) = Self::from_config_file() {
            config = file_config;
        }

        // 3. Override with environment variables (highest precedence)
        if let Ok(provider) = env::var("MCC_GAQL_LLM_PROVIDER") {
            config.provider = provider.parse()?;
        }
        if let Ok(model) = env::var("MCC_GAQL_LLM_MODEL") {
            config.model = model;
        }
        if let Ok(temp) = env::var("MCC_GAQL_LLM_TEMPERATURE") {
            config.temperature = temp.parse()?;
        }
        if let Ok(tokens) = env::var("MCC_GAQL_LLM_MAX_TOKENS") {
            config.max_tokens = Some(tokens.parse()?);
        }

        Ok(config)
    }

    /// Create LLM client instance
    pub fn create_client(&self) -> Result<Box<dyn LLMClient>> {
        match self.provider {
            LLMProvider::OpenRouter => {
                self.create_openrouter_client()
            }
            LLMProvider::OpenAI => {
                self.create_openai_client()
            }
            LLMProvider::Anthropic => {
                self.create_anthropic_client()
            }
            LLMProvider::Ollama => {
                self.create_ollama_client()
            }
        }
    }
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            provider: LLMProvider::OpenRouter,
            model: "google/gemini-flash-1.5".to_string(),
            temperature: 0.1,
            max_tokens: Some(2048),
            params: HashMap::new(),
        }
    }
}

/// Trait for LLM clients (abstraction over providers)
#[async_trait::async_trait]
pub trait LLMClient: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
    fn model_name(&self) -> &str;
}
```

**Updated RAG Agent Initialization (`src/prompt2gaql.rs`):**

```rust
// Old (hardcoded):
// let openrouter_client = openrouter::Client::from_env();
// let agent = openrouter_client.agent(openrouter::GEMINI_FLASH_2_0)
//     .temperature(0.1)
//     .build();

// New (configurable):
let llm_config = LLMConfig::load()?;  // Loads from config + env vars
let llm_client = llm_config.create_client()?;
let agent = llm_client.create_agent()
    .temperature(llm_config.temperature)
    .max_tokens(llm_config.max_tokens)
    .build();

info!("Using LLM: {} (provider: {:?})",
      llm_config.model, llm_config.provider);
```

---

#### Component 3: Enhanced Cache Metadata

**New Cache Metadata Format:**

```rust
// New file: ~/.cache/mcc-gaql/query_cookbook.metadata.json
{
    "schema_version": 2,
    "model_id": "fastembed:AllMiniLML6V2Q:384:a1b2c3d4",
    "embedding_dimension": 384,
    "distance_metric": "cosine",
    "content_hash": "0x1234567890abcdef",
    "created_at": "2025-11-10T12:34:56Z",
    "embedding_config": {
        "provider": "fastembed",
        "model": "AllMiniLML6V2Q",
        "params": {}
    }
}
```

**Metadata Module (`src/lancedb_utils.rs` - additions):**

```rust
use serde::{Deserialize, Serialize};
use std::path::Path;

const SCHEMA_VERSION: u8 = 2;  // Bump to v2

#[derive(Debug, Serialize, Deserialize)]
pub struct CacheMetadata {
    pub schema_version: u8,
    pub model_id: String,
    pub embedding_dimension: usize,
    pub distance_metric: String,
    pub content_hash: u64,
    pub created_at: String,
    pub embedding_config: serde_json::Value,  // Store full config for debugging
}

impl CacheMetadata {
    /// Save metadata to JSON file
    pub fn save(&self, base_path: &Path, name: &str) -> Result<()> {
        let metadata_path = base_path.join(format!("{}.metadata.json", name));
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(metadata_path, json)?;
        Ok(())
    }

    /// Load metadata from JSON file
    pub fn load(base_path: &Path, name: &str) -> Result<Option<Self>> {
        let metadata_path = base_path.join(format!("{}.metadata.json", name));
        if !metadata_path.exists() {
            return Ok(None);
        }
        let json = std::fs::read_to_string(metadata_path)?;
        let metadata: CacheMetadata = serde_json::from_str(&json)?;
        Ok(Some(metadata))
    }

    /// Check if cache is valid for given model config
    pub fn is_valid(&self, model_id: &str, content_hash: u64) -> bool {
        self.schema_version == SCHEMA_VERSION
            && self.model_id == model_id
            && self.content_hash == content_hash
    }
}
```

#### Component 4: Dynamic Vector Schema

**Replace Fixed-Size Lists with Variable-Length Lists:**

```rust
// src/lancedb_utils.rs - Updated schema functions

/// Create schema for query cookbook with dynamic vector dimensions
pub fn create_query_cookbook_schema(embedding_dimension: usize) -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("description", DataType::Utf8, false),
        Field::new("query", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float64, true)),
                embedding_dimension as i32,  // Dynamic dimension
            ),
            false,
        ),
    ])
}

/// Create schema for field metadata with dynamic vector dimensions
pub fn create_field_metadata_schema(embedding_dimension: usize) -> Schema {
    Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("description", DataType::Utf8, false),
        Field::new("category", DataType::Utf8, false),
        Field::new("data_type", DataType::Utf8, false),
        Field::new("selectable", DataType::Boolean, false),
        Field::new("filterable", DataType::Boolean, false),
        Field::new("sortable", DataType::Boolean, false),
        Field::new("metrics_compatible", DataType::Boolean, false),
        Field::new("resource_name", DataType::Utf8, true),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float64, true)),
                embedding_dimension as i32,  // Dynamic dimension
            ),
            false,
        ),
    ])
}
```

#### Component 5: Enhanced Cache Validation

**Updated Cache Loading Logic (`src/prompt2gaql.rs`):**

```rust
/// Load or build query cookbook vector store with model validation
async fn load_or_build_query_cookbook(
    queries: &[QueryEntry],
    embedding_config: &EmbeddingConfig,
) -> Result<LanceDbVectorIndex<Box<dyn EmbeddingModel>>> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow!("Could not find cache directory"))?
        .join("mcc-gaql");

    // Compute current content hash
    let content_hash = compute_query_cookbook_hash(queries);

    // Generate model identifier
    let model_id = embedding_config.model_identifier();

    // Try to load cache metadata
    let metadata = CacheMetadata::load(&cache_dir, "query_cookbook")?;

    // Validate cache
    let cache_valid = if let Some(ref meta) = metadata {
        if meta.is_valid(&model_id, content_hash) {
            debug!("Query cookbook cache is valid");
            true
        } else {
            info!(
                "Query cookbook cache invalid: model changed from {} to {} or content changed",
                meta.model_id, model_id
            );
            false
        }
    } else {
        info!("No query cookbook cache found");
        false
    };

    if cache_valid {
        // Load existing cache
        load_query_cookbook_from_cache(&cache_dir, embedding_config).await
    } else {
        // Rebuild cache with new model
        info!("Building query cookbook with model: {}", model_id);

        // Delete old cache if exists
        let lance_path = cache_dir.join("lancedb").join("query_cookbook");
        if lance_path.exists() {
            fs::remove_dir_all(&lance_path)?;
        }

        // Build new cache
        build_query_cookbook(queries, embedding_config, content_hash).await?;

        // Save metadata
        let metadata = CacheMetadata {
            schema_version: SCHEMA_VERSION,
            model_id: model_id.clone(),
            embedding_dimension: embedding_config.detect_dimension().await?,
            distance_metric: format!("{:?}", embedding_config.distance_metric),
            content_hash,
            created_at: chrono::Utc::now().to_rfc3339(),
            embedding_config: serde_json::to_value(embedding_config)?,
        };
        metadata.save(&cache_dir, "query_cookbook")?;

        load_query_cookbook_from_cache(&cache_dir, embedding_config).await
    }
}
```

### 3.3 Alternative Approach: Test-Only Experimentation

For rapid experimentation without persistent caching:

**Test Configuration:**
```rust
// tests/embedding_experiment_tests.rs

#[derive(Debug)]
struct EmbeddingExperiment {
    name: String,
    config: EmbeddingConfig,
    skip_cache: bool,  // Always rebuild embeddings
}

impl EmbeddingExperiment {
    fn new(name: &str, model: &str) -> Self {
        Self {
            name: name.to_string(),
            config: EmbeddingConfig {
                provider: EmbeddingProvider::FastEmbed,
                model: model.to_string(),
                dimension: None,  // Auto-detect
                distance_metric: DistanceMetric::Cosine,
                params: HashMap::new(),
            },
            skip_cache: true,  // Never use cache
        }
    }
}

#[tokio::test]
async fn compare_embedding_models() -> Result<()> {
    let experiments = vec![
        EmbeddingExperiment::new("MiniLM", "AllMiniLML6V2Q"),
        EmbeddingExperiment::new("BGE-Small", "BgeSmallEnV15"),
        EmbeddingExperiment::new("BGE-Base", "BgeBaseEnV15"),
    ];

    let test_queries = load_test_queries();
    let ground_truth = load_ground_truth();

    for experiment in experiments {
        info!("Running experiment: {}", experiment.name);

        // Build in-memory vector store (no persistent cache)
        let vector_store = build_in_memory_vector_store(
            &test_queries,
            &experiment.config,
        ).await?;

        // Evaluate RAG quality
        let metrics = evaluate_rag_quality(
            &vector_store,
            &test_queries,
            &ground_truth,
        ).await?;

        info!("Results for {}: {:?}", experiment.name, metrics);
    }

    Ok(())
}
```

**In-Memory Vector Store:**
```rust
/// Build vector store without persistent cache (for testing)
async fn build_in_memory_vector_store(
    entries: &[QueryEntry],
    config: &EmbeddingConfig,
) -> Result<InMemoryVectorStore> {
    let model = config.create_embedding_model().await?;

    // Generate embeddings
    let texts: Vec<String> = entries.iter()
        .map(|e| e.description.clone())
        .collect();
    let embeddings = model.embed_batch(texts).await?;

    // Create in-memory store
    InMemoryVectorStore::new(entries, embeddings, config.distance_metric)
}
```

---

## 4. Implementation Plan

### Phase 1: Core Infrastructure (Week 1)

**Tasks:**
1. Create `src/embedding_config.rs` module
   - Define `EmbeddingConfig`, `EmbeddingProvider`, `EmbeddingModel` trait
   - Implement model identifier generation
   - Add configuration loading from TOML and env vars

2. Update `src/lancedb_utils.rs`
   - Remove `const EMBEDDING_DIM: i32 = 384`
   - Add dynamic schema functions with dimension parameter
   - Implement `CacheMetadata` structure
   - Add metadata save/load functions

3. Update configuration system
   - Add `[embedding]` section to config.toml
   - Set default to current model (AllMiniLML6V2Q)

**Acceptance Criteria:**
- Configuration loads successfully with defaults
- Model identifier generation works correctly
- Schemas created dynamically with any dimension
- Metadata save/load functions tested

### Phase 2: Cache Validation (Week 1-2)

**Tasks:**
1. Update `src/prompt2gaql.rs`
   - Modify `load_or_build_query_cookbook` with metadata validation
   - Modify `load_or_build_field_metadata` with metadata validation
   - Add logging for cache invalidation reasons

2. Add backward compatibility
   - Detect old v1 cache format (hash files without metadata)
   - Automatically invalidate and rebuild

3. Add cache migration utility
   - CLI command: `mcc-gaql cache info` (show cache status)
   - CLI command: `mcc-gaql cache clear` (force rebuild)
   - CLI command: `mcc-gaql cache migrate` (upgrade from v1 to v2)

**Acceptance Criteria:**
- Cache validates model identifier correctly
- Cache invalidates when model changes
- Old caches gracefully upgraded
- Clear error messages for cache mismatches

### Phase 3: Multi-Provider Support (Week 2-3)

**Tasks:**
1. Implement FastEmbed provider (already exists, refactor)
   - Wrap existing rig-fastembed code
   - Implement `EmbeddingModel` trait

2. Implement OpenAI provider (optional, for comparison)
   - Add openai crate dependency
   - Implement text-embedding-3-small/large models
   - Handle API key from environment

3. Implement Ollama provider (optional, for local experimentation)
   - Add reqwest for HTTP calls
   - Support mxbai-embed-large, nomic-embed-text

4. Add provider factory
   - `EmbeddingConfig::create_embedding_model()` dispatches to correct provider

**Acceptance Criteria:**
- At least 2 providers implemented (FastEmbed + 1 other)
- Easy to switch providers via configuration
- Dimension auto-detection works for all providers
- Tests validate embeddings from each provider

### Phase 4: Testing Infrastructure (Week 3)

**Tasks:**
1. Create `tests/embedding_experiment_tests.rs`
   - In-memory vector store for cache-free testing
   - Experiment framework for comparing models
   - RAG quality metrics (precision, recall, MRR)

2. Add benchmark tests
   - Embedding generation time
   - Cache load time
   - Search latency

3. Update existing RAG tests
   - Use configurable embedding model
   - Add tests for multiple models

**Acceptance Criteria:**
- Can run experiments with 3+ different models
- Quality metrics computed automatically
- Benchmark results logged clearly
- Existing tests pass with new system

### Phase 5: Documentation & Migration (Week 4)

**Tasks:**
1. Update README with embedding configuration instructions
2. Write migration guide for existing users
3. Add troubleshooting section for cache issues
4. Update specs/rag-quality-improvement-plan.md with model recommendations

**Acceptance Criteria:**
- Documentation complete and clear
- Migration tested on real cache
- Users can switch models easily

---

## 5. Migration Strategy

### 5.1 For Existing Users

**Step 1: Detection**
- On first run with new version, detect old cache format (v1)
- Show warning: "Cache format outdated, will rebuild on next query"

**Step 2: Automatic Migration**
- Delete old hash files (`query_cookbook.hash`, `field_metadata.hash`)
- Keep LanceDB tables temporarily
- On next RAG query, rebuild with new metadata format
- Save new metadata files

**Step 3: Cleanup**
- After successful rebuild, old cache can be deleted
- Log cache location and size for user information

### 5.2 Rollback Plan

If issues arise:
1. Keep old code in `legacy_embedding_cache` module
2. Add feature flag: `--legacy-cache` to use old system
3. Provide downgrade script to restore v1 cache

---

## 6. Performance Impact Analysis

### 6.1 Cache Hit (No Change)
- **Before:** Load LanceDB table (~100ms)
- **After:** Load LanceDB table + read metadata JSON (~110ms)
- **Impact:** +10ms (negligible)

### 6.2 Cache Miss (Rebuild)
- **Before:** 18-20s (query + field embeddings)
- **After:** 18-20s + metadata save (<50ms)
- **Impact:** +50ms (negligible)

### 6.3 Model Switch
- **Before:** Manual cache delete + 18-20s rebuild (after code change)
- **After:** Automatic detection + 18-20s rebuild (no code change)
- **Impact:** Improved UX, same performance

### 6.4 Storage Overhead
- **Before:** ~10 MB (LanceDB tables)
- **After:** ~10 MB + 2KB (metadata files)
- **Impact:** <0.1% increase

---

## 7. Security & Privacy Considerations

### 7.1 API Keys
- OpenAI/Ollama providers may require API keys
- Store in environment variables, never in config files
- Add warnings about API key exposure

### 7.2 Cache Location
- Cache stored in user's home directory (~/.cache)
- Embeddings contain semantic information about queries
- Consider adding option to disable caching for sensitive queries

### 7.3 Model Provenance
- Track model source in metadata for auditing
- Validate model checksums (for downloaded models like FastEmbed)

---

## 8. Testing Strategy

### 8.1 Unit Tests
- `EmbeddingConfig::model_identifier()` generates unique IDs
- `CacheMetadata::is_valid()` correctly validates cache
- Schema creation with various dimensions

### 8.2 Integration Tests
- End-to-end RAG query with cache cold start
- Cache invalidation on model change
- Multiple model providers

### 8.3 Experiment Tests
- Compare 3+ embedding models on same dataset
- Measure RAG quality metrics
- Generate comparison report

### 8.4 Regression Tests
- Ensure backward compatibility with v1 caches
- Verify migration from old format
- Performance benchmarks (cache hit/miss)

---

## 9. Future Enhancements

### 9.1 Multi-Language Support
- Support models optimized for non-English queries
- Multilingual embedding models (e.g., multilingual-e5)

### 9.2 Hybrid Search
- Combine dense embeddings with sparse (BM25) search
- Weighted fusion of multiple models

### 9.3 Model Versioning
- Track model version history
- A/B testing framework for models
- Automatic rollback on quality degradation

### 9.4 Cloud-Based Embeddings
- Support for AWS Bedrock, Azure OpenAI
- Batch embedding API for cost optimization

---

## 10. Risks & Mitigations

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Dimension mismatch crashes | High | Medium | Validate dimension on load, clear error messages |
| Cache corruption | Medium | Low | Checksum validation, automatic rebuild |
| Breaking change for users | Medium | High | Automatic migration, backward compatibility |
| Performance regression | Low | Low | Benchmark tests, rollback plan |
| Model provider API changes | Medium | Medium | Abstract provider interface, version pinning |

---

## 11. Success Metrics

### 11.1 Functional Metrics
- ✅ Cache invalidates correctly on model change (100% reliability)
- ✅ Support for 3+ embedding providers
- ✅ Zero manual cache deletions required

### 11.2 Performance Metrics
- ✅ Cache hit latency < 200ms (no degradation)
- ✅ Model switch time = rebuild time (no additional overhead)

### 11.3 Quality Metrics
- ✅ RAG precision improvement > 10% with best model (measured in tests)
- ✅ User can experiment with 5+ models in < 1 hour

### 11.4 User Experience Metrics
- ✅ Model switching requires only config change (no code)
- ✅ Clear error messages for all cache issues
- ✅ Zero data loss during migration

---

## 12. Conclusion

This design provides a robust, extensible system for embedding model management that:

1. **Eliminates manual cache management** - Automatic invalidation on model changes
2. **Enables rapid experimentation** - Switch models via configuration
3. **Supports future growth** - Multi-provider architecture
4. **Maintains performance** - Minimal overhead on cache operations
5. **Ensures reliability** - Comprehensive validation and testing

The phased implementation approach minimizes risk while delivering incremental value. The test-only experimentation mode provides an immediate path for model comparison without affecting production caching.

**Next Steps:**
1. Review and approve this specification
2. Begin Phase 1 implementation (core infrastructure)
3. Set up experiment framework for model comparison
4. Iterate based on experimental results

---

## Appendix A: Recommended Embedding Models

Based on research and benchmarks, here are recommended models for experimentation:

| Model | Provider | Dimensions | Speed | Quality | Use Case |
|-------|----------|------------|-------|---------|----------|
| AllMiniLML6V2Q | FastEmbed | 384 | Fast | Good | Current baseline |
| BgeSmallEnV15 | FastEmbed | 384 | Fast | Better | General improvement |
| BgeBaseEnV15 | FastEmbed | 768 | Medium | Best | High quality |
| text-embedding-3-small | OpenAI | 1536 | Slow | Excellent | Cloud option |
| nomic-embed-text | Ollama | 768 | Fast | Good | Local option |
| mxbai-embed-large | Ollama | 1024 | Medium | Better | Local quality |

**Recommendation:** Start with BgeSmallEnV15 and BgeBaseEnV15 from FastEmbed for easy experimentation.

---

## Appendix B: Configuration Examples

### Example 1: Default Configuration (Current Setup)
```toml
# ~/.config/mcc-gaql/config.toml
[embedding]
provider = "fastembed"
model = "AllMiniLML6V2Q"  # 384 dimensions
distance_metric = "cosine"

[llm]
provider = "openrouter"
model = "google/gemini-flash-1.5"
temperature = 0.1
max_tokens = 2048
```

### Example 2: High-Quality Setup (Better Embeddings + Claude)
```toml
[embedding]
provider = "fastembed"
model = "BgeBaseEnV15"  # 768 dimensions, better quality
distance_metric = "cosine"

[llm]
provider = "openrouter"
model = "anthropic/claude-3.5-sonnet"  # Better reasoning
temperature = 0.1
max_tokens = 2048
```

**Runtime Override (one-time test):**
```bash
# Test Claude without editing config
MCC_GAQL_LLM_MODEL="anthropic/claude-3.5-sonnet" mcc-gaql -q "show campaigns"
```

### Example 3: OpenAI All-In (requires OpenAI API key)
```toml
[embedding]
provider = "openai"
model = "text-embedding-3-large"  # 1536 dimensions
distance_metric = "cosine"

[embedding.params]
api_key_env = "OPENAI_API_KEY"

[llm]
provider = "openai"
model = "gpt-4o"
temperature = 0.1
max_tokens = 2048

[llm.params]
api_key_env = "OPENAI_API_KEY"
```

**Required:**
```bash
export OPENAI_API_KEY="sk-..."
```

### Example 4: Fully Local (Ollama - No API Costs)
```toml
[embedding]
provider = "ollama"
model = "nomic-embed-text"  # 768 dimensions
distance_metric = "cosine"

[embedding.params]
base_url = "http://localhost:11434"

[llm]
provider = "ollama"
model = "llama3.1"  # or "mistral", "codellama"
temperature = 0.1
max_tokens = 2048

[llm.params]
base_url = "http://localhost:11434"
```

**Prerequisites:**
```bash
# Install Ollama models
ollama pull nomic-embed-text
ollama pull llama3.1
ollama serve  # Start Ollama server
```

### Example 5: Hybrid Setup (Local Embeddings + Cloud LLM)
```toml
# Embedding: Local (fast, free)
[embedding]
provider = "ollama"
model = "nomic-embed-text"

[embedding.params]
base_url = "http://localhost:11434"

# LLM: Cloud (high quality)
[llm]
provider = "openrouter"
model = "anthropic/claude-3.5-sonnet"
temperature = 0.1
```

**Cost-saving rationale:** Embeddings run once and cached, LLM runs per query. Using local embeddings + cloud LLM optimizes cost vs quality.

### Example 6: A/B Testing via Environment Variables

**Scenario:** Test 3 embedding models to find best RAG quality

```bash
# Baseline (current)
MCC_GAQL_EMBEDDING_MODEL="AllMiniLML6V2Q" mcc-gaql -q "test query" > results_minilm.txt

# Candidate 1: BGE-Small
MCC_GAQL_EMBEDDING_MODEL="BgeSmallEnV15" mcc-gaql -q "test query" > results_bge_small.txt

# Candidate 2: BGE-Base
MCC_GAQL_EMBEDDING_MODEL="BgeBaseEnV15" mcc-gaql -q "test query" > results_bge_base.txt

# Compare results manually or with script
diff results_minilm.txt results_bge_base.txt
```

### Example 7: CI/CD Testing Configuration

**GitHub Actions (`.github/workflows/test.yml`):**
```yaml
name: RAG Quality Tests
on: [push, pull_request]

jobs:
  test-models:
    runs-on: ubuntu-latest
    strategy:
      matrix:
        embedding_model: [AllMiniLML6V2Q, BgeSmallEnV15, BgeBaseEnV15]
        llm_model: [google/gemini-flash-1.5, anthropic/claude-3.5-sonnet]
    env:
      MCC_GAQL_EMBEDDING_MODEL: ${{ matrix.embedding_model }}
      MCC_GAQL_LLM_MODEL: ${{ matrix.llm_model }}
      OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
    steps:
      - uses: actions/checkout@v3
      - name: Run RAG tests
        run: cargo test --test rag_quality_tests
```

This tests 6 combinations (3 embedding models × 2 LLMs) automatically.

### Example 8: Development vs Production Configs

**Development (`~/.config/mcc-gaql/config.dev.toml`):**
```toml
# Fast, cheap models for development
[embedding]
provider = "fastembed"
model = "AllMiniLML6V2Q"

[llm]
provider = "openrouter"
model = "google/gemini-flash-1.5"  # Fast & cheap
temperature = 0.1
```

**Production (`~/.config/mcc-gaql/config.prod.toml`):**
```toml
# Best quality models for production
[embedding]
provider = "fastembed"
model = "BgeBaseEnV15"

[llm]
provider = "openrouter"
model = "anthropic/claude-3.5-sonnet"  # Best reasoning
temperature = 0.05  # More deterministic
```

**Usage:**
```bash
# Development
mcc-gaql --config ~/.config/mcc-gaql/config.dev.toml -q "test"

# Production
mcc-gaql --config ~/.config/mcc-gaql/config.prod.toml -q "test"

# Or via env var
MCC_GAQL_CONFIG_PATH="~/.config/mcc-gaql/config.prod.toml" mcc-gaql -q "test"
```

---

## Appendix C: Cache Directory Structure (After Migration)

```
~/.cache/mcc-gaql/
├── lancedb/
│   ├── query_cookbook.lance/
│   │   ├── data/
│   │   │   └── <uuid>.lance
│   │   ├── _versions/
│   │   │   └── 1.manifest
│   │   └── _latest.manifest
│   └── field_metadata.lance/
│       ├── data/
│       ├── _versions/
│       └── _latest.manifest
├── query_cookbook.metadata.json      # NEW: Model metadata
├── field_metadata.metadata.json      # NEW: Model metadata
└── field_metadata.json                # Existing: Field cache (7-day TTL)
```

**Metadata File Format:**
```json
{
  "schema_version": 2,
  "model_id": "fastembed:BgeSmallEnV15:384:e8f7a6b5",
  "embedding_dimension": 384,
  "distance_metric": "cosine",
  "content_hash": 12345678901234567890,
  "created_at": "2025-11-10T12:34:56Z",
  "embedding_config": {
    "provider": "fastembed",
    "model": "BgeSmallEnV15",
    "dimension": null,
    "distance_metric": "cosine",
    "params": {}
  }
}
```

---

## Appendix D: Environment Variable Override Deep Dive

### Configuration Precedence Explained

The system uses a **three-tier configuration hierarchy** with clear precedence rules:

```
╔════════════════════════════════════════════════════════╗
║         1. ENVIRONMENT VARIABLES (Highest)             ║
║         Runtime override - temporary                    ║
║         Example: MCC_GAQL_EMBEDDING_MODEL="..."        ║
╚════════════════════════════════════════════════════════╝
                          ↓
                If env var not set
                          ↓
╔════════════════════════════════════════════════════════╗
║         2. CONFIGURATION FILE (Middle)                  ║
║         User preferences - persistent                   ║
║         Example: ~/.config/mcc-gaql/config.toml        ║
╚════════════════════════════════════════════════════════╝
                          ↓
                If config file missing
                          ↓
╔════════════════════════════════════════════════════════╗
║         3. HARDCODED DEFAULTS (Lowest)                  ║
║         Fallback values - embedded in code              ║
║         Example: AllMiniLML6V2Q, GEMINI_FLASH_2_0      ║
╚════════════════════════════════════════════════════════╝
```

### How Configuration Loading Works

**Rust Implementation (`src/embedding_config.rs` and `src/llm_config.rs`):**

```rust
pub fn load() -> Result<Config> {
    // Step 1: Start with hardcoded defaults
    let mut config = Self::default();

    // Step 2: If config file exists, load and merge
    if let Ok(file_config) = Self::from_config_file() {
        config = file_config;
    }

    // Step 3: Environment variables override everything
    if let Ok(value) = env::var("MCC_GAQL_EMBEDDING_MODEL") {
        config.model = value;  // ENV wins!
    }

    Ok(config)
}
```

**Key Principle:** Later values override earlier values.

### Why This Design?

| Tier | Use Case | Persistence | Scope | Example |
|------|----------|-------------|-------|---------|
| **ENV** | Quick experiments | Session/command | Single run | Testing new model before committing |
| **Config** | User preferences | Permanent | All runs | Default models you prefer |
| **Default** | First-time use | Permanent | All users | Works out-of-box |

### Practical Examples

#### Example 1: Override Single Parameter
```bash
# Config file has: model = "AllMiniLML6V2Q"
# ENV overrides just the model:
MCC_GAQL_EMBEDDING_MODEL="BgeSmallEnV15" mcc-gaql -q "test"
# Result: Uses BgeSmallEnV15 (from ENV)
#         Other config values (provider, distance_metric) from file
```

#### Example 2: Multiple Overrides
```bash
# Config file has full embedding config
# ENV overrides multiple values:
MCC_GAQL_EMBEDDING_PROVIDER="ollama" \
MCC_GAQL_EMBEDDING_MODEL="nomic-embed-text" \
mcc-gaql -q "test"
# Result: Both values from ENV, everything else from config
```

#### Example 3: ENV Without Config File
```bash
# No config file exists
# ENV provides values:
MCC_GAQL_EMBEDDING_MODEL="BgeBaseEnV15" \
MCC_GAQL_LLM_MODEL="anthropic/claude-3.5-sonnet" \
mcc-gaql -q "test"
# Result: Uses defaults for everything except:
#   - embedding.model = "BgeBaseEnV15" (from ENV)
#   - llm.model = "anthropic/claude-3.5-sonnet" (from ENV)
```

#### Example 4: Empty ENV Variable (No Effect)
```bash
# ENV variable set but empty - ignored
MCC_GAQL_EMBEDDING_MODEL="" mcc-gaql -q "test"
# Result: Falls through to config file or default
```

#### Example 5: Persistent ENV (Shell Session)
```bash
# Set once, applies to all subsequent commands
export MCC_GAQL_EMBEDDING_MODEL="BgeSmallEnV15"
export MCC_GAQL_LLM_MODEL="anthropic/claude-3.5-sonnet"

mcc-gaql -q "query 1"  # Uses BgeSmallEnV15 + Claude
mcc-gaql -q "query 2"  # Uses BgeSmallEnV15 + Claude
mcc-gaql -q "query 3"  # Uses BgeSmallEnV15 + Claude

# Unset to restore config file behavior
unset MCC_GAQL_EMBEDDING_MODEL
unset MCC_GAQL_LLM_MODEL
```

### Complete Environment Variable Reference

**Embedding Configuration:**
| Variable | Type | Example | Description |
|----------|------|---------|-------------|
| `MCC_GAQL_EMBEDDING_PROVIDER` | String | `fastembed` | Provider name |
| `MCC_GAQL_EMBEDDING_MODEL` | String | `BgeSmallEnV15` | Model identifier |
| `MCC_GAQL_EMBEDDING_DIMENSION` | Integer | `384` | Force dimension (optional) |
| `MCC_GAQL_EMBEDDING_DISTANCE` | String | `cosine` | Distance metric |

**LLM Configuration:**
| Variable | Type | Example | Description |
|----------|------|---------|-------------|
| `MCC_GAQL_LLM_PROVIDER` | String | `openrouter` | Provider name |
| `MCC_GAQL_LLM_MODEL` | String | `anthropic/claude-3.5-sonnet` | Model identifier |
| `MCC_GAQL_LLM_TEMPERATURE` | Float | `0.1` | Temperature (0.0-1.0) |
| `MCC_GAQL_LLM_MAX_TOKENS` | Integer | `2048` | Max response tokens |

**API Keys (Provider-Specific):**
| Variable | Provider | Required For |
|----------|----------|--------------|
| `OPENROUTER_API_KEY` | OpenRouter | OpenRouter LLM models |
| `OPENAI_API_KEY` | OpenAI | OpenAI embedding/LLM |
| `ANTHROPIC_API_KEY` | Anthropic | Anthropic LLM (direct) |

### Best Practices

**✅ Good Practices:**
1. **Use ENV for experiments:** Test new models without committing
2. **Use config for preferences:** Set your preferred models permanently
3. **Export for sessions:** When testing multiple queries with same config
4. **Script with ENV:** Automation scripts should use ENV vars
5. **Log effective config:** App should log which config values were used

**❌ Anti-Patterns:**
1. **Don't mix too many sources:** Avoid having 10+ ENV vars set permanently
2. **Don't store secrets in config:** API keys should be ENV-only
3. **Don't forget to unset:** Temporary ENV vars can cause confusion later
4. **Don't override in production:** Production should use config files, not ENV

### Debugging Configuration Issues

**Problem: "Which config is being used?"**

**Solution:** Add logging to show effective configuration:
```rust
info!("Configuration loaded:");
info!("  Embedding: {} (provider: {:?})",
      config.embedding.model, config.embedding.provider);
info!("  LLM: {} (provider: {:?})",
      config.llm.model, config.llm.provider);
info!("  Sources: ENV={}, Config={}, Defaults={}",
      env_overrides_applied, config_file_loaded, using_defaults);
```

**Problem: "ENV override not working?"**

**Checklist:**
1. ENV var name correct? (Check spelling, prefix `MCC_GAQL_`)
2. ENV var exported? (Use `echo $MCC_GAQL_EMBEDDING_MODEL`)
3. Value valid? (Check against allowed values)
4. App restarted? (Some shells need refresh)

**Debug Commands:**
```bash
# Show all MCC_GAQL env vars
env | grep MCC_GAQL

# Test with explicit logging
MCC_GAQL_EMBEDDING_MODEL="BgeSmallEnV15" \
MCC_GAQL_LOG_LEVEL="debug" \
mcc-gaql -q "test"
```

### Future Enhancements

Potential improvements to configuration system:

1. **Profile Support:**
   ```bash
   mcc-gaql --profile production -q "test"
   # Loads ~/.config/mcc-gaql/profiles/production.toml
   ```

2. **Configuration Validation:**
   ```bash
   mcc-gaql config validate
   # Checks config file syntax, model availability, API keys
   ```

3. **Configuration Inspector:**
   ```bash
   mcc-gaql config show
   # Shows effective configuration with source attribution
   ```

4. **Environment File Support:**
   ```bash
   # .env file in project directory
   MCC_GAQL_EMBEDDING_MODEL=BgeSmallEnV15
   MCC_GAQL_LLM_MODEL=anthropic/claude-3.5-sonnet

   # Auto-loaded when running mcc-gaql
   ```

---

**End of Specification**

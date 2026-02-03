# Configurable LLM Provider Specification

**Status:** Proposed
**Author:** System Design
**Date:** 2026-02-03
**Related Docs:**
- specs/embedding-model-switching.md

---

## Summary

Replace the hardcoded OpenRouter/Gemini Flash LLM configuration with a configurable provider system that supports OpenAI-compatible APIs, including Nano-gpt.com.

---

## Current State

The LLM provider is hardcoded in `src/prompt2gaql.rs`:

```rust
// Lines 532, 659-663
let openrouter_client = openrouter::Client::from_env();
let agent = openrouter_client
    .agent(openrouter::GEMINI_FLASH_2_0)
    .preamble(&preamble)
    .temperature(0.1)
    .build();
```

**Problems:**
- Cannot switch providers without code changes
- Cannot use Nano-gpt.com or other OpenAI-compatible providers
- Requires `OPENROUTER_API_KEY` even if user prefers different provider

---

## Proposed Solution

Use Rig's OpenAI provider with custom `base_url` to support any OpenAI-compatible API.

### API Design

Rig's OpenAI `ClientBuilder` supports custom base URLs:

```rust
use rig::providers::openai;

let client = openai::ClientBuilder::new(&api_key)
    .base_url("https://nano-gpt.com/api/v1")
    .build();

let agent = client
    .agent("glm-4-9b")
    .preamble(&preamble)
    .temperature(0.1)
    .build();
```

### Configuration

**Environment Variables:**

| Variable | Description | Example |
|----------|-------------|---------|
| `MCC_GAQL_LLM_API_KEY` | API key for the LLM provider | `your-nano-gpt-key` |
| `MCC_GAQL_LLM_BASE_URL` | Base URL for OpenAI-compatible API | `https://nano-gpt.com/api/v1` |
| `MCC_GAQL_LLM_MODEL` | Model name | `glm-4-9b` |
| `MCC_GAQL_LLM_TEMPERATURE` | Temperature (optional, default: 0.1) | `0.1` |

**Backward Compatibility:**

For backward compatibility with existing setups:
- If `MCC_GAQL_LLM_API_KEY` is not set, fall back to `OPENROUTER_API_KEY`
- If `MCC_GAQL_LLM_BASE_URL` is not set, use OpenRouter's default URL
- If `MCC_GAQL_LLM_MODEL` is not set, use `google/gemini-flash-2.0`

### Provider Presets

Common providers with their base URLs:

| Provider | Base URL | Example Models |
|----------|----------|----------------|
| OpenRouter | `https://openrouter.ai/api/v1` | `google/gemini-flash-2.0`, `anthropic/claude-3.5-sonnet` |
| Nano-gpt | `https://nano-gpt.com/api/v1` | `glm-4-9b`, `glm-4-plus` |
| OpenAI | `https://api.openai.com/v1` | `gpt-4o`, `gpt-4-turbo` |
| Ollama | `http://localhost:11434/v1` | `llama3.1`, `mistral` |

---

## Implementation

### File Changes

**`src/prompt2gaql.rs`:**

```rust
use rig::providers::openai;

/// Load LLM configuration from environment
fn load_llm_config() -> (String, String, String, f32) {
    // API key: prefer MCC_GAQL_LLM_API_KEY, fall back to OPENROUTER_API_KEY
    let api_key = std::env::var("MCC_GAQL_LLM_API_KEY")
        .or_else(|_| std::env::var("OPENROUTER_API_KEY"))
        .expect("MCC_GAQL_LLM_API_KEY or OPENROUTER_API_KEY must be set");

    // Base URL: default to OpenRouter for backward compatibility
    let base_url = std::env::var("MCC_GAQL_LLM_BASE_URL")
        .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

    // Model: default to Gemini Flash
    let model = std::env::var("MCC_GAQL_LLM_MODEL")
        .unwrap_or_else(|_| "google/gemini-flash-2.0".to_string());

    // Temperature
    let temperature: f32 = std::env::var("MCC_GAQL_LLM_TEMPERATURE")
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or(0.1);

    (api_key, base_url, model, temperature)
}

/// Create LLM client with configurable provider
fn create_llm_client() -> openai::Client {
    let (api_key, base_url, _, _) = load_llm_config();

    openai::ClientBuilder::new(&api_key)
        .base_url(&base_url)
        .build()
}
```

**Update `RAGAgent::init()` (line ~524):**

```rust
impl RAGAgent {
    pub async fn init(query_cookbook: Vec<QueryEntry>) -> Result<Self, anyhow::Error> {
        let (api_key, base_url, model, temperature) = load_llm_config();

        log::info!("Using LLM: {} via {}", model, base_url);

        let llm_client = openai::ClientBuilder::new(&api_key)
            .base_url(&base_url)
            .build();

        // ... existing embedding setup ...

        let agent = llm_client
            .agent(&model)
            .preamble("...")
            .temperature(temperature)
            .build();

        Ok(RAGAgent { agent, query_index })
    }
}
```

**Update `EnhancedRAGAgent::init()` (line ~627):**

Same pattern as above.

**`src/main.rs`:**

Update the environment variable check:

```rust
// Old (line ~209):
// env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY not set");

// New:
if std::env::var("MCC_GAQL_LLM_API_KEY").is_err() && std::env::var("OPENROUTER_API_KEY").is_err() {
    panic!("Either MCC_GAQL_LLM_API_KEY or OPENROUTER_API_KEY must be set");
}
```

### Cargo.toml

No changes needed - `rig-core` already includes the OpenAI provider.

---

## Usage Examples

### Example 1: Nano-gpt with GLM-4

```bash
export MCC_GAQL_LLM_API_KEY="your-nano-gpt-key"
export MCC_GAQL_LLM_BASE_URL="https://nano-gpt.com/api/v1"
export MCC_GAQL_LLM_MODEL="glm-4-9b"

mcc-gaql -n "show me campaigns with high CTR"
```

### Example 2: OpenRouter (backward compatible)

```bash
# Works exactly as before
export OPENROUTER_API_KEY="your-openrouter-key"
mcc-gaql -n "show me campaigns"
```

### Example 3: Local Ollama

```bash
export MCC_GAQL_LLM_BASE_URL="http://localhost:11434/v1"
export MCC_GAQL_LLM_MODEL="llama3.1"
export MCC_GAQL_LLM_API_KEY="ollama"  # Ollama ignores this but rig requires it

mcc-gaql -n "show me campaigns"
```

### Example 4: One-time override

```bash
# Use Claude via OpenRouter just for this query
MCC_GAQL_LLM_MODEL="anthropic/claude-3.5-sonnet" mcc-gaql -n "complex query"
```

---

## Testing

### Unit Tests

```rust
#[test]
fn test_load_llm_config_defaults() {
    std::env::set_var("OPENROUTER_API_KEY", "test-key");
    std::env::remove_var("MCC_GAQL_LLM_API_KEY");
    std::env::remove_var("MCC_GAQL_LLM_BASE_URL");
    std::env::remove_var("MCC_GAQL_LLM_MODEL");

    let (api_key, base_url, model, temp) = load_llm_config();

    assert_eq!(api_key, "test-key");
    assert_eq!(base_url, "https://openrouter.ai/api/v1");
    assert_eq!(model, "google/gemini-flash-2.0");
    assert_eq!(temp, 0.1);
}

#[test]
fn test_load_llm_config_custom() {
    std::env::set_var("MCC_GAQL_LLM_API_KEY", "nano-key");
    std::env::set_var("MCC_GAQL_LLM_BASE_URL", "https://nano-gpt.com/api/v1");
    std::env::set_var("MCC_GAQL_LLM_MODEL", "glm-4-9b");
    std::env::set_var("MCC_GAQL_LLM_TEMPERATURE", "0.2");

    let (api_key, base_url, model, temp) = load_llm_config();

    assert_eq!(api_key, "nano-key");
    assert_eq!(base_url, "https://nano-gpt.com/api/v1");
    assert_eq!(model, "glm-4-9b");
    assert_eq!(temp, 0.2);
}
```

### Integration Test

```rust
#[tokio::test]
#[ignore]  // Requires real API key
async fn test_nano_gpt_integration() {
    std::env::set_var("MCC_GAQL_LLM_API_KEY", std::env::var("NANO_GPT_API_KEY").unwrap());
    std::env::set_var("MCC_GAQL_LLM_BASE_URL", "https://nano-gpt.com/api/v1");
    std::env::set_var("MCC_GAQL_LLM_MODEL", "glm-4-9b");

    let result = convert_to_gaql_enhanced(
        vec![],  // empty cookbook for test
        None,
        "show all campaigns"
    ).await;

    assert!(result.is_ok());
    let query = result.unwrap();
    assert!(query.to_uppercase().contains("SELECT"));
    assert!(query.to_uppercase().contains("FROM CAMPAIGN"));
}
```

---

## Migration

1. **No breaking changes** - existing `OPENROUTER_API_KEY` setups continue to work
2. **Gradual adoption** - users can switch to new env vars when ready
3. **Documentation** - update README with new configuration options

---

## Verification

After implementation, verify with:

```bash
# 1. Backward compatibility (should work unchanged)
OPENROUTER_API_KEY="..." mcc-gaql -n "show campaigns"

# 2. Nano-gpt integration
MCC_GAQL_LLM_API_KEY="..." \
MCC_GAQL_LLM_BASE_URL="https://nano-gpt.com/api/v1" \
MCC_GAQL_LLM_MODEL="glm-4-9b" \
mcc-gaql -n "show campaigns"

# 3. Check logs show correct provider
RUST_LOG=info mcc-gaql -n "show campaigns"
# Should log: "Using LLM: glm-4-9b via https://nano-gpt.com/api/v1"
```

---

## Files to Modify

| File | Changes |
|------|---------|
| `src/prompt2gaql.rs` | Add `load_llm_config()`, update `RAGAgent::init()` and `EnhancedRAGAgent::init()` |
| `src/main.rs` | Update API key validation |

---

## Future Enhancements

1. **Config file support** - Add `[llm]` section to `~/.config/mcc-gaql/config.toml`
2. **Provider presets** - `LLM_PROVIDER=nano-gpt` auto-sets base_url
3. **Model aliases** - Short names like `glm4` -> `glm-4-9b`

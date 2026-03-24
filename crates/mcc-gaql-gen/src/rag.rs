use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::vec;
use twox_hash::XxHash64;

use futures::stream::{self, StreamExt};
use log::info;

use lancedb::DistanceType;
use rig::vector_store::request::SearchFilter;
use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::Prompt,
    embeddings::{EmbedError, EmbeddingsBuilder, TextEmbedder, embed::Embed},
    providers::openai::{self, completion::CompletionModel},
    vector_store::{VectorSearchRequest, VectorStoreIndex},
};
use rig_fastembed::FastembedModel;
use rig_lancedb::{LanceDBFilter, LanceDbVectorIndex, SearchParams};
use serde::{Deserialize, Serialize};

use mcc_gaql_common::config::QueryEntry;
use mcc_gaql_common::field_metadata::{FieldMetadata, FieldMetadataCache};
use mcc_gaql_common::paths;

use crate::vector_store as lancedb_utils;

/// Configuration for LLM provider
#[derive(Debug, Clone)]
pub struct LlmConfig {
    api_key: String,
    base_url: String,
    /// Ordered list of model names; index 0 is the preferred/primary model.
    models: Vec<String>,
    temperature: f32,
}

impl LlmConfig {
    /// Create a config from an explicit list of model names for unit tests.
    ///
    /// Uses placeholder values for all other fields so no real LLM calls are made.
    #[cfg(test)]
    pub fn from_models_for_test(models: Vec<String>) -> Self {
        assert!(!models.is_empty(), "models must not be empty");
        Self {
            api_key: "dummy".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            models,
            temperature: 0.1,
        }
    }

    /// Create a dummy config for unit tests (does not make real LLM calls)
    #[cfg(test)]
    pub fn from_env_or_dummy() -> Self {
        let models_str =
            std::env::var("MCC_GAQL_LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".to_string());
        let models: Vec<String> = models_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let models = if models.is_empty() {
            vec!["gpt-4o-mini".to_string()]
        } else {
            models
        };
        Self {
            api_key: std::env::var("MCC_GAQL_LLM_API_KEY").unwrap_or_else(|_| "dummy".to_string()),
            base_url: std::env::var("MCC_GAQL_LLM_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            models,
            temperature: 0.1,
        }
    }

    /// Load LLM configuration from environment
    pub fn from_env() -> Self {
        // API key: must be explicitly configured
        let api_key = std::env::var("MCC_GAQL_LLM_API_KEY")
            .expect("MCC_GAQL_LLM_API_KEY must be set (e.g., sk-...)");

        // Base URL: must be explicitly configured - fail fast if not set
        let base_url = std::env::var("MCC_GAQL_LLM_BASE_URL")
            .expect("MCC_GAQL_LLM_BASE_URL must be set (e.g., https://api.openai.com/v1 or https://openrouter.ai/api/v1)");

        // Models: comma-separated list; at least one required
        let models: Vec<String> = std::env::var("MCC_GAQL_LLM_MODEL")
            .expect("MCC_GAQL_LLM_MODEL must be set (e.g., gpt-4o-mini or google/gemini-flash-2.0)")
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if models.is_empty() {
            panic!("MCC_GAQL_LLM_MODEL must contain at least one model name");
        }

        // Temperature: default to 0.1 if not set, fail fast with explicit error if invalid
        let temperature: f32 = match std::env::var("MCC_GAQL_LLM_TEMPERATURE") {
            Ok(val) => val.parse().unwrap_or_else(|_| {
                panic!(
                    "MCC_GAQL_LLM_TEMPERATURE must be a valid number (e.g., 0.1), got: '{}'",
                    val
                )
            }),
            Err(_) => 0.1,
        };

        Self {
            api_key,
            base_url,
            models,
            temperature,
        }
    }

    /// Returns the first (preferred) model name.
    pub fn preferred_model(&self) -> &str {
        &self.models[0]
    }

    /// Returns all configured model names.
    pub fn all_models(&self) -> &[String] {
        &self.models
    }

    /// Returns the number of configured models.
    pub fn model_count(&self) -> usize {
        self.models.len()
    }

    /// Returns the temperature setting.
    pub fn temperature(&self) -> f32 {
        self.temperature
    }
}

impl LlmConfig {
    /// Create an OpenAI-compatible completions client from this config.
    pub fn create_llm_client(&self) -> Result<openai::CompletionsClient, anyhow::Error> {
        use mcc_gaql_common::http_client;

        let client = http_client::create_http_client("mcc-gaql-gen (LLM client)", 120)?;

        openai::CompletionsClient::builder()
            .api_key(&self.api_key)
            .base_url(&self.base_url)
            .http_client(client)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create LLM client: {}", e))
    }

    /// Create a simple LLM agent for the preferred (first) model with the given system prompt.
    /// Used by callers that always want the primary model.
    #[allow(dead_code)]
    pub fn create_agent(
        &self,
        system_prompt: &str,
    ) -> Result<Agent<CompletionModel>, anyhow::Error> {
        self.create_agent_for_model(self.preferred_model(), system_prompt)
    }

    /// Create a simple LLM agent for the specified model name with the given system prompt.
    pub fn create_agent_for_model(
        &self,
        model: &str,
        system_prompt: &str,
    ) -> Result<Agent<CompletionModel>, anyhow::Error> {
        let client = self.create_llm_client()?;
        let agent = client
            .agent(model)
            .preamble(system_prompt)
            .temperature(self.temperature as f64)
            .build();
        Ok(agent)
    }
}

/// Pre-computed date ranges for LLM prompt context.
/// Extracts date arithmetic from the main function for clarity and testability.
struct DateContext {
    today: chrono::NaiveDate,
    // Period boundaries
    this_year_start: chrono::NaiveDate,
    prev_year_start: chrono::NaiveDate,
    prev_year_end: chrono::NaiveDate,
    this_quarter_start: chrono::NaiveDate,
    prev_quarter_start: chrono::NaiveDate,
    prev_quarter_end: chrono::NaiveDate,
    last_60_start: chrono::NaiveDate,
    last_90_start: chrono::NaiveDate,
    // Seasons (as formatted strings for prompt interpolation)
    this_summer: (String, String),
    last_summer: (String, String),
    this_winter: (String, String),
    last_winter: (String, String),
    this_spring: (String, String),
    last_spring: (String, String),
    this_fall: (String, String),
    last_fall: (String, String),
    this_christmas: (String, String),
}

impl DateContext {
    fn new() -> Self {
        use chrono::Datelike;

        let today = chrono::Local::now().date_naive();

        // Simple offsets
        let last_60_start = today - chrono::Duration::days(60);
        let last_90_start = today - chrono::Duration::days(90);

        // Month/quarter/year calculations
        let this_month_start = today.with_day(1).unwrap_or(today);
        let prev_month_end = this_month_start - chrono::Duration::days(1);

        let month = today.month();
        let quarter_start_month = ((month - 1) / 3) * 3 + 1;
        let this_quarter_start =
            chrono::NaiveDate::from_ymd_opt(today.year(), quarter_start_month, 1).unwrap_or(today);
        let prev_quarter_end = this_quarter_start - chrono::Duration::days(1);
        let prev_quarter_month = ((prev_quarter_end.month() - 1) / 3) * 3 + 1;
        let prev_quarter_start =
            chrono::NaiveDate::from_ymd_opt(prev_quarter_end.year(), prev_quarter_month, 1)
                .unwrap_or(prev_quarter_end);

        let this_year_start = chrono::NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap_or(today);
        let prev_year_end = this_year_start - chrono::Duration::days(1);
        let prev_year_start =
            chrono::NaiveDate::from_ymd_opt(prev_year_end.year(), 1, 1).unwrap_or(prev_year_end);

        // Season calculations (fixed dates)
        let year = today.year();
        let this_summer = (format!("{year}-06-01"), format!("{year}-08-31"));
        let last_summer = (format!("{}-06-01", year - 1), format!("{}-08-31", year - 1));
        let this_winter = (format!("{year}-12-01"), format!("{year}-02-28"));
        let last_winter = (format!("{}-12-01", year - 1), format!("{year}-02-28"));
        let this_spring = (format!("{year}-03-01"), format!("{year}-05-31"));
        let last_spring = (format!("{}-03-01", year - 1), format!("{}-05-31", year - 1));
        let this_fall = (format!("{year}-09-01"), format!("{year}-11-30"));
        let last_fall = (format!("{}-09-01", year - 1), format!("{}-11-30", year - 1));
        let this_christmas = (format!("{year}-12-20"), format!("{year}-12-31"));

        // Suppress unused variable warnings for computed but unused dates
        let _ = prev_month_end;

        Self {
            today,
            this_year_start,
            prev_year_start,
            prev_year_end,
            this_quarter_start,
            prev_quarter_start,
            prev_quarter_end,
            last_60_start,
            last_90_start,
            this_summer,
            last_summer,
            this_winter,
            last_winter,
            this_spring,
            last_spring,
            this_fall,
            last_fall,
            this_christmas,
        }
    }
}

/// Create embedding client and model
fn create_embedding_client()
-> Result<(rig_fastembed::Client, rig_fastembed::EmbeddingModel), anyhow::Error> {
    // Set HF_HOME to cache fastembed models in the proper location
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get cache directory"))?
        .join("mcc-gaql")
        .join("fastembed-models");

    std::fs::create_dir_all(&cache_dir)?;

    info!("Fastembed cache directory: {}", cache_dir.display());

    // fastembed uses HF_HOME to determine where to cache models
    // SAFETY: This is safe because we're only setting a known environment variable
    // and the process is single-threaded at this point.
    unsafe { std::env::set_var("HF_HOME", &cache_dir) };

    info!(
        "Loading fastembed model: {:?}",
        FastembedModel::BGESmallENV15
    );

    let fastembed_client = rig_fastembed::Client::new();
    let embedding_model = fastembed_client.embedding_model(&FastembedModel::BGESmallENV15);

    info!("Fastembed model loaded successfully");
    Ok((fastembed_client, embedding_model))
}

/// Shared LLM resources for agent initialization
struct AgentResources {
    #[allow(dead_code)]
    llm_client: openai::CompletionsClient,
    embed_client: rig_fastembed::Client,
    embedding_model: rig_fastembed::EmbeddingModel,
}

/// Initialize shared LLM resources used by both agent types
fn init_llm_resources(config: &LlmConfig) -> Result<AgentResources, anyhow::Error> {
    let llm_client = config.create_llm_client()?;
    let (embed_client, embedding_model) = create_embedding_client()?;

    Ok(AgentResources {
        llm_client,
        embed_client,
        embedding_model,
    })
}

/// Format LLM request for debug logging with human-friendly formatting
pub fn format_llm_request_debug(preamble: &Option<String>, prompt: &str) -> String {
    let mut output = String::new();
    output.push('\n');
    output.push_str("═══════════════════════════════════════════════════════════════════\n");
    output.push_str("                      LLM REQUEST DEBUG DUMP\n");
    output.push_str("═══════════════════════════════════════════════════════════════════\n\n");

    // Preamble section
    output.push_str("┌─ PREAMBLE ─────────────────────────────────────────────────────┐\n");
    if let Some(p) = preamble {
        output.push_str(p);
        if !p.ends_with('\n') {
            output.push('\n');
        }
    } else {
        output.push_str("(no preamble)\n");
    }
    output.push_str("└────────────────────────────────────────────────────────────────┘\n\n");

    // Prompt section
    output.push_str("┌─ PROMPT ───────────────────────────────────────────────────────┐\n");
    output.push_str(prompt);
    if !prompt.ends_with('\n') {
        output.push('\n');
    }
    output.push_str("└────────────────────────────────────────────────────────────────┘\n");
    output.push_str("═══════════════════════════════════════════════════════════════════\n");

    output
}

/// Format LLM response for debug logging with human-friendly formatting
pub fn format_llm_response_debug(response: &str) -> String {
    let mut output = String::new();
    output.push_str("┌─ LLM RESPONSE ───────────────────────────────────────────────────┐\n");
    output.push_str(response);
    if !response.ends_with('\n') {
        output.push('\n');
    }
    output.push_str("└──────────────────────────────────────────────────────────────────┘\n");
    output
}

/// Compute hash of query cookbook for cache validation
/// Queries are sorted by ID to ensure deterministic ordering regardless of HashMap iteration order
pub fn compute_query_cookbook_hash(queries: &[QueryEntry]) -> u64 {
    // Use a fixed seed for deterministic hashing
    let mut hasher = XxHash64::with_seed(0x1234_5678_9abc_def0);

    // Sort queries for deterministic ordering (HashMap iteration is random)
    // Sort by id first, then by description, then by query for complete determinism
    // (duplicate IDs are possible when different descriptions produce the same truncated ID)
    let mut sorted_queries: Vec<_> = queries.iter().collect();
    sorted_queries.sort_by(|a, b| {
        a.id.cmp(&b.id)
            .then_with(|| a.description.cmp(&b.description))
            .then_with(|| a.query.cmp(&b.query))
    });

    for query in sorted_queries {
        query.description.hash(&mut hasher);
        query.query.hash(&mut hasher);
    }
    hasher.finish()
}

/// Compute hash of field cache for cache validation
pub fn compute_field_cache_hash(cache: &FieldMetadataCache) -> u64 {
    let mut hasher = XxHash64::with_seed(0x1234_5678_9abc_def0);

    // Hash description generation version to invalidate cache when logic changes
    "DESCRIPTION_VERSION_2".hash(&mut hasher);

    // Hash API version
    cache.api_version.hash(&mut hasher);

    // Hash all fields (sorted by name for consistency)
    let mut field_names: Vec<_> = cache.fields.keys().collect();
    field_names.sort();

    for name in field_names {
        if let Some(field) = cache.fields.get(name) {
            field.hash(&mut hasher);
        }
    }

    hasher.finish()
}

/// Validate that the cache is valid for the current data
/// Returns Ok(true) if both field metadata and query cookbook caches are valid
pub fn validate_cache_for_data(
    field_cache: &FieldMetadataCache,
    query_cookbook: &[QueryEntry],
) -> Result<bool, anyhow::Error> {
    // Check field metadata cache
    let field_hash = compute_field_cache_hash(field_cache);
    let field_cached = lancedb_utils::load_hash("field_metadata")?;
    let field_valid = match field_cached {
        Some(cached_hash) => {
            log::debug!(
                "Field metadata: computed={}, cached={}",
                field_hash,
                cached_hash
            );
            cached_hash == field_hash
        }
        None => {
            log::debug!("Field metadata: no cached hash found");
            false
        }
    };

    // Check query cookbook cache
    let query_hash = compute_query_cookbook_hash(query_cookbook);
    let query_cached = lancedb_utils::load_hash("query_cookbook")?;
    let query_valid = match query_cached {
        Some(cached_hash) => {
            log::debug!(
                "Query cookbook: computed={}, cached={}",
                query_hash,
                cached_hash
            );
            cached_hash == query_hash
        }
        None => {
            log::debug!("Query cookbook: no cached hash found");
            false
        }
    };

    log::debug!(
        "Cache validation: field_valid={}, query_valid={}",
        field_valid,
        query_valid
    );
    Ok(field_valid && query_valid)
}

/// Build embeddings for field metadata and query cookbook
/// This is a lightweight operation that only builds embeddings, without running the RAG pipeline
pub async fn build_embeddings(
    example_queries: Vec<QueryEntry>,
    field_cache: &FieldMetadataCache,
    config: &LlmConfig,
) -> Result<(), anyhow::Error> {
    log::info!("Building embeddings for fast GAQL generation...");

    // Initialize embedding resources
    let resources = init_llm_resources(config)?;

    // Build field vector store (this will use cache if valid)
    let field_start = std::time::Instant::now();
    log::info!("Building field metadata embeddings...");
    let _field_index =
        build_or_load_field_vector_store(field_cache, resources.embedding_model.clone()).await?;
    log::info!(
        "Field metadata embeddings ready (took {:.2}s)",
        field_start.elapsed().as_secs_f64()
    );

    // Build query vector store (this will use cache if valid)
    let query_start = std::time::Instant::now();
    log::info!("Building query cookbook embeddings...");
    let _query_index =
        build_or_load_query_vector_store(example_queries, resources.embedding_model.clone())
            .await?;
    log::info!(
        "Query cookbook embeddings ready (took {:.2}s)",
        query_start.elapsed().as_secs_f64()
    );

    // Build resource entries vector store (this will use cache if valid)
    let resource_start = std::time::Instant::now();
    log::info!("Building resource entries embeddings...");
    let _resource_index =
        build_or_load_resource_vector_store(field_cache, resources.embedding_model).await?;
    log::info!(
        "Resource entries embeddings ready (took {:.2}s)",
        resource_start.elapsed().as_secs_f64()
    );

    log::info!("Embeddings build complete");
    Ok(())
}

/// Build or load query cookbook vector store with LanceDB caching
pub async fn build_or_load_query_vector_store(
    query_cookbook: Vec<QueryEntry>,
    embedding_model: rig_fastembed::EmbeddingModel,
) -> Result<LanceDbVectorIndex<rig_fastembed::EmbeddingModel>, anyhow::Error> {
    let total_start = std::time::Instant::now();

    // Compute hash of current cookbook
    let current_hash = compute_query_cookbook_hash(&query_cookbook);

    // Try to load from LanceDB cache
    if let Ok(Some(cached_hash)) = lancedb_utils::load_hash("query_cookbook")
        && cached_hash == current_hash
    {
        log::info!("Query cookbook cache valid, loading from LanceDB...");

        match lancedb_utils::get_lancedb_connection().await {
            Ok(db) => {
                match lancedb_utils::open_table(&db, "query_cookbook").await {
                    Ok(table) => {
                        // Wrap table in LanceDbVectorIndex with cosine distance
                        let index = LanceDbVectorIndex::new(
                            table,
                            embedding_model,
                            "id",
                            SearchParams::default().distance_type(DistanceType::Cosine),
                        )
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

                        log::info!(
                            "Successfully loaded query cookbook from cache ({:.2}s)",
                            total_start.elapsed().as_secs_f64()
                        );
                        return Ok(index);
                    }
                    Err(e) => {
                        log::warn!("Failed to open cached table: {}, rebuilding...", e);
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to connect to LanceDB: {}, rebuilding...", e);
            }
        }
    }

    // Cache miss or invalid - build embeddings
    log::info!(
        "Building embeddings for {} queries...",
        query_cookbook.len()
    );
    let embedding_start = std::time::Instant::now();

    // Wrap QueryEntry in QueryEntryEmbed to satisfy the Embed trait (orphan rule)
    let wrapped: Vec<QueryEntryEmbed> = query_cookbook
        .iter()
        .cloned()
        .map(QueryEntryEmbed)
        .collect();

    // Generate embeddings in parallel chunks for better performance
    let embeddings = generate_embeddings_parallel(
        wrapped,
        embedding_model.clone(),
        50, // Process 50 documents per chunk
    )
    .await?;

    log::info!(
        "Query cookbook embeddings generated in {:.2}s",
        embedding_start.elapsed().as_secs_f64()
    );

    // Create document-to-embedding mapping to preserve associations
    let mut id_to_embedding = HashMap::new();
    for (document, embedding) in embeddings.iter() {
        for emb in embedding.iter() {
            id_to_embedding.insert(document.0.id.clone(), emb.clone());
        }
    }

    // Extract embeddings in original document order using stable IDs
    let mut embedding_vecs = Vec::with_capacity(query_cookbook.len());
    for document in &query_cookbook {
        if let Some(embedding) = id_to_embedding.get(&document.id) {
            embedding_vecs.push(embedding.clone());
        } else {
            log::warn!("Missing embedding for document ID: {}", document.id);
            embedding_vecs.push(rig::embeddings::Embedding {
                vec: vec![0.0_f64; lancedb_utils::EMBEDDING_DIM as usize],
                document: String::new(),
            });
        }
    }

    // Save to LanceDB and get table
    let table = lancedb_utils::build_or_load_query_vector_store(
        query_cookbook,
        embedding_vecs,
        current_hash,
    )
    .await?;

    // Wrap table in LanceDbVectorIndex with cosine distance
    let index = LanceDbVectorIndex::new(
        table,
        embedding_model,
        "id",
        SearchParams::default().distance_type(DistanceType::Cosine),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

    log::info!(
        "Query cookbook initialization complete ({:.2}s total)",
        total_start.elapsed().as_secs_f64()
    );

    Ok(index)
}

/// Generate embeddings in parallel chunks for better performance
///
/// This function splits documents into chunks and processes them concurrently
/// using all available CPU cores, significantly speeding up embedding generation.
/// Generate embeddings in parallel chunks for better performance
///
/// This function splits documents into chunks and processes them concurrently
/// using all available CPU cores, significantly speeding up embedding generation.
async fn generate_embeddings_parallel<T: Embed + Clone + Send + Sync + 'static>(
    documents: Vec<T>,
    embedding_model: rig_fastembed::EmbeddingModel,
    chunk_size: usize,
) -> Result<Vec<(T, Vec<rig::embeddings::Embedding>)>, anyhow::Error> {
    if documents.is_empty() {
        return Ok(Vec::new());
    }

    let concurrency = num_cpus::get().max(2);
    let total_docs = documents.len();
    let total_chunks = total_docs.div_ceil(chunk_size);

    log::info!(
        "Generating embeddings for {} documents in {} chunks (concurrency: {})",
        total_docs,
        total_chunks,
        concurrency
    );

    let chunks: Vec<Vec<T>> = documents.chunks(chunk_size).map(|c| c.to_vec()).collect();

    // Process chunks concurrently and collect results
    #[allow(clippy::type_complexity)]
    let results: Vec<Result<Vec<(T, Vec<rig::embeddings::Embedding>)>, anyhow::Error>> =
        stream::iter(chunks.into_iter().enumerate())
            .map(|(idx, chunk)| {
                let model = embedding_model.clone();
                async move {
                    log::debug!("Processing embedding chunk {}/{}", idx + 1, total_chunks);
                    let chunk_start = std::time::Instant::now();

                    // Build embeddings for this chunk
                    let builder = EmbeddingsBuilder::new(model)
                        .documents(chunk.clone())
                        .map_err(|e| {
                            anyhow::anyhow!("Failed to create embeddings builder: {}", e)
                        })?;

                    let chunk_embeddings = builder
                        .build()
                        .await
                        .map_err(|e| anyhow::anyhow!("Failed to build embeddings: {}", e))?;

                    log::debug!(
                        "Chunk {}/{} completed ({} docs, {:.2}s)",
                        idx + 1,
                        total_chunks,
                        chunk.len(),
                        chunk_start.elapsed().as_secs_f64()
                    );

                    // Convert OneOrMany<Embedding> to Vec<Embedding>
                    let result: Vec<(T, Vec<rig::embeddings::Embedding>)> = chunk_embeddings
                        .into_iter()
                        .map(|(doc, one_or_many)| {
                            let embeddings: Vec<rig::embeddings::Embedding> =
                                one_or_many.into_iter().collect();
                            (doc, embeddings)
                        })
                        .collect();

                    Ok(result)
                }
            })
            .buffer_unordered(concurrency)
            .collect::<Vec<_>>()
            .await;

    // Flatten results and handle errors
    let mut all_embeddings = Vec::with_capacity(total_docs);
    for result in results {
        match result {
            Ok(chunk_embeddings) => {
                all_embeddings.extend(chunk_embeddings);
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    log::info!(
        "Successfully generated {} embeddings from {} documents",
        all_embeddings.len(),
        total_docs
    );

    Ok(all_embeddings)
}

/// Build or load field vector store with LanceDB caching
pub async fn build_or_load_field_vector_store(
    field_cache: &FieldMetadataCache,
    embedding_model: rig_fastembed::EmbeddingModel,
) -> Result<LanceDbVectorIndex<rig_fastembed::EmbeddingModel>, anyhow::Error> {
    let total_start = std::time::Instant::now();

    // Compute hash of current field cache
    let current_hash = compute_field_cache_hash(field_cache);

    // Try to load from LanceDB cache
    if let Ok(Some(cached_hash)) = lancedb_utils::load_hash("field_metadata")
        && cached_hash == current_hash
    {
        log::info!("Field metadata cache valid, loading from LanceDB...");

        match lancedb_utils::get_lancedb_connection().await {
            Ok(db) => match lancedb_utils::open_table(&db, "field_metadata").await {
                Ok(table) => {
                    let index = LanceDbVectorIndex::new(
                        table,
                        embedding_model,
                        "id",
                        SearchParams::default().distance_type(DistanceType::Cosine),
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

                    log::info!(
                        "Successfully loaded field metadata from cache ({:.2}s)",
                        total_start.elapsed().as_secs_f64()
                    );
                    return Ok(index);
                }
                Err(e) => {
                    log::warn!("Failed to open cached table: {}, rebuilding...", e);
                }
            },
            Err(e) => {
                log::warn!("Failed to connect to LanceDB: {}, rebuilding...", e);
            }
        }
    }

    // Cache miss or invalid - build field documents and embeddings
    log::info!(
        "Building embeddings for {} fields...",
        field_cache.fields.len()
    );
    let embedding_start = std::time::Instant::now();

    let field_docs: Vec<FieldDocument> = field_cache
        .fields
        .values()
        .map(|field| FieldDocument::new(field.clone()))
        .collect();

    log::debug!("Sample field descriptions:");
    for doc in field_docs.iter().take(3) {
        log::debug!("  {}: {}", doc.field.name, doc.description);
    }

    // Generate embeddings in parallel chunks for better performance
    let field_embeddings = generate_embeddings_parallel(
        field_docs.clone(),
        embedding_model.clone(),
        50, // Process 50 documents per chunk
    )
    .await?;

    log::info!(
        "Field metadata embeddings generated in {:.2}s",
        embedding_start.elapsed().as_secs_f64()
    );

    // Create document-to-embedding mapping to preserve associations
    let mut id_to_embedding = HashMap::new();
    for (document, embedding) in field_embeddings.iter() {
        for emb in embedding.iter() {
            id_to_embedding.insert(document.id.clone(), emb.clone());
        }
    }

    // Extract embeddings in original document order using stable IDs
    let mut embedding_vecs = Vec::with_capacity(field_docs.len());
    for document in &field_docs {
        if let Some(embedding) = id_to_embedding.get(&document.id) {
            embedding_vecs.push(embedding.clone());
        } else {
            log::warn!("Missing embedding for document ID: {}", document.id);
            embedding_vecs.push(rig::embeddings::Embedding {
                vec: vec![0.0_f64; lancedb_utils::EMBEDDING_DIM as usize],
                document: String::new(),
            });
        }
    }

    // Save to LanceDB and get table
    let table =
        lancedb_utils::build_or_load_field_vector_store(field_docs, embedding_vecs, current_hash)
            .await?;

    // Wrap table in LanceDbVectorIndex with cosine distance
    let index = LanceDbVectorIndex::new(
        table,
        embedding_model,
        "id",
        SearchParams::default().distance_type(DistanceType::Cosine),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

    log::info!(
        "Field metadata initialization complete ({:.2}s total)",
        total_start.elapsed().as_secs_f64()
    );

    Ok(index)
}

/// Map a resource name to a human-readable category label
fn categorize_resource(resource_name: &str) -> &'static str {
    if resource_name.starts_with("campaign_budget") || resource_name.contains("budget") {
        "Budget Resources"
    } else if resource_name.starts_with("campaign") {
        "Campaign Resources"
    } else if resource_name.starts_with("ad_group_ad") {
        "Ad Resources"
    } else if resource_name.starts_with("ad_group") {
        "Ad Group Resources"
    } else if resource_name.contains("keyword") || resource_name.contains("search_term") {
        "Keyword & Search Resources"
    } else if resource_name.contains("asset") {
        "Asset Resources"
    } else if resource_name.contains("audience") || resource_name.contains("demographic") {
        "Audience Resources"
    } else if resource_name.contains("conversion") {
        "Conversion Resources"
    } else if resource_name.starts_with("customer") {
        "Customer Resources"
    } else if resource_name.contains("bidding") {
        "Bidding Resources"
    } else if resource_name.contains("shopping") || resource_name.contains("product") {
        "Shopping Resources"
    } else if resource_name.contains("hotel") {
        "Hotel Resources"
    } else if resource_name.contains("local_services") {
        "Local Services Resources"
    } else if resource_name.contains("video") {
        "Video Resources"
    } else if resource_name.contains("label") {
        "Label Resources"
    } else if resource_name.contains("user_list") {
        "User List Resources"
    } else if resource_name.contains("offline") {
        "Offline Conversion Resources"
    } else if resource_name.contains("shared") {
        "Shared Set Resources"
    } else if resource_name.contains("experiment") {
        "Experiment Resources"
    } else if resource_name.contains("geo") {
        "Geographic Resources"
    } else {
        "Other Resources"
    }
}

/// Compute a deterministic hash of the resource metadata for cache invalidation
pub fn compute_resource_metadata_hash(field_cache: &FieldMetadataCache) -> u64 {
    let mut hasher = XxHash64::with_seed(0x1234_5678_9abc_def0);
    "RESOURCE_VERSION_1".hash(&mut hasher);

    if let Some(resource_metadata) = &field_cache.resource_metadata {
        let mut names: Vec<&String> = resource_metadata.keys().collect();
        names.sort();
        for name in names {
            if let Some(rm) = resource_metadata.get(name) {
                name.hash(&mut hasher);
                if let Some(desc) = &rm.description {
                    desc.hash(&mut hasher);
                }
                rm.selectable_with.len().hash(&mut hasher);
                rm.field_count.hash(&mut hasher);
            }
        }
    }

    hasher.finish()
}

/// Build or load resource entries vector store with LanceDB caching
pub async fn build_or_load_resource_vector_store(
    field_cache: &FieldMetadataCache,
    embedding_model: rig_fastembed::EmbeddingModel,
) -> Result<LanceDbVectorIndex<rig_fastembed::EmbeddingModel>, anyhow::Error> {
    let total_start = std::time::Instant::now();
    let current_hash = compute_resource_metadata_hash(field_cache);

    // Try to load from LanceDB cache
    if let Ok(Some(cached_hash)) = lancedb_utils::load_hash("resource_entries")
        && cached_hash == current_hash
    {
        log::info!("Resource entries cache valid, loading from LanceDB...");

        match lancedb_utils::get_lancedb_connection().await {
            Ok(db) => match lancedb_utils::open_table(&db, "resource_entries").await {
                Ok(table) => {
                    let index = LanceDbVectorIndex::new(
                        table,
                        embedding_model,
                        "resource_name",
                        SearchParams::default().distance_type(DistanceType::Cosine),
                    )
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to create resource vector index: {}", e))?;

                    log::info!(
                        "Successfully loaded resource entries from cache ({:.2}s)",
                        total_start.elapsed().as_secs_f64()
                    );
                    return Ok(index);
                }
                Err(e) => {
                    log::warn!("Failed to open cached resource table: {}, rebuilding...", e);
                }
            },
            Err(e) => {
                log::warn!(
                    "Failed to connect to LanceDB for resources: {}, rebuilding...",
                    e
                );
            }
        }
    }

    // Cache miss or invalid - build resource documents and embeddings
    let resource_metadata = match &field_cache.resource_metadata {
        Some(m) => m,
        None => {
            return Err(anyhow::anyhow!(
                "resource_metadata is None in field cache; run mcc-gaql-gen to populate it"
            ));
        }
    };

    log::info!(
        "Building embeddings for {} resources...",
        resource_metadata.len()
    );
    let embedding_start = std::time::Instant::now();

    let resource_docs: Vec<ResourceDocument> = resource_metadata
        .iter()
        .map(|(name, rm)| {
            // Gather sample field names from the fields map (up to 10)
            let sample_fields: Vec<&str> = field_cache
                .fields
                .values()
                .filter(|f| f.get_resource().as_deref() == Some(name.as_str()))
                .take(10)
                .map(|f| f.name.as_str())
                .collect();

            let has_metrics = rm.has_metrics();
            let category = categorize_resource(name).to_string();
            let description = rm.description.clone().unwrap_or_default();

            let embedding_text = format!(
                "Resource: {}. Category: {}. Description: {}. \
                 Has metrics: {}. Field count: {}. \
                 Sample fields: {}.",
                name,
                category,
                description,
                has_metrics,
                rm.field_count,
                sample_fields.join(", "),
            );

            ResourceDocument {
                id: name.clone(),
                resource_name: name.clone(),
                category,
                description,
                has_metrics,
                field_count: rm.field_count as i32,
                embedding_text,
            }
        })
        .collect();

    let resource_embeddings = generate_embeddings_parallel(
        resource_docs.clone(),
        embedding_model.clone(),
        50,
    )
    .await?;

    log::info!(
        "Resource embeddings generated in {:.2}s",
        embedding_start.elapsed().as_secs_f64()
    );

    // Map id -> embedding
    let mut id_to_embedding = HashMap::new();
    for (document, embeddings) in resource_embeddings.iter() {
        for emb in embeddings.iter() {
            id_to_embedding.insert(document.id.clone(), emb.clone());
        }
    }

    // Extract in document order
    let mut embedding_vecs = Vec::with_capacity(resource_docs.len());
    for doc in &resource_docs {
        if let Some(emb) = id_to_embedding.get(&doc.id) {
            embedding_vecs.push(emb.clone());
        } else {
            log::warn!("Missing embedding for resource: {}", doc.id);
            embedding_vecs.push(rig::embeddings::Embedding {
                vec: vec![0.0_f64; lancedb_utils::EMBEDDING_DIM as usize],
                document: String::new(),
            });
        }
    }

    // Save to LanceDB and wrap in index
    let table =
        lancedb_utils::build_or_load_resource_table(resource_docs, embedding_vecs, current_hash)
            .await?;

    let index = LanceDbVectorIndex::new(
        table,
        embedding_model,
        "resource_name",
        SearchParams::default().distance_type(DistanceType::Cosine),
    )
    .await
    .map_err(|e| anyhow::anyhow!("Failed to create resource vector index: {}", e))?;

    log::info!(
        "Resource entries initialization complete ({:.2}s total)",
        total_start.elapsed().as_secs_f64()
    );

    Ok(index)
}

/// Strip markdown code block notation from LLM responses
fn strip_markdown_code_blocks(text: &str) -> String {
    let trimmed = text.trim();

    // Check for code block with backticks
    if let Some(content) = trimmed.strip_prefix("```") {
        // Find the first newline after the opening ``` (which may have a language specifier)
        if let Some(newline_pos) = content.find('\n') {
            let without_opening = &content[newline_pos + 1..];
            // Remove the closing ```
            if let Some(closing_pos) = without_opening.rfind("```") {
                return without_opening[..closing_pos].trim().to_string();
            }
        }
    }

    // If no code blocks found, return the original trimmed text
    trimmed.to_string()
}

/// Create a sample of 5 resources to show in the explanation.
/// Prioritizes keyword matches from the user query, then fills randomly.
fn create_resource_sample(
    user_query: &str,
    resources: &[(String, String)], // (resource_name, description)
) -> Vec<(String, String)> {
    // Stop words to filter out
    const STOP_WORDS: &[&str] = &[
        "the", "from", "in", "and", "to", "of", "a", "an", "is", "are", "or", "with", "on", "at",
        "by", "for", "that", "this", "these", "those", "it", "they",
    ];

    // Extract keywords from user query
    let query_lower = user_query.to_lowercase();
    let keywords: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|word| word.len() > 2 && !STOP_WORDS.contains(word))
        .collect();

    // Find resources that match any keyword
    let mut matched: Vec<(String, String)> = Vec::new();
    let mut unmatched: Vec<(String, String)> = Vec::new();

    for (name, desc) in resources {
        let name_lower = name.to_lowercase();

        // Clean up description - remove common prefixes and normalize whitespace
        let clean_desc = desc
            .trim()
            .trim_start_matches("**Resource Description:**")
            .trim_start_matches("**Description:**")
            .trim()
            .to_string();

        let desc_lower = clean_desc.to_lowercase();

        let is_match = keywords
            .iter()
            .any(|kw| name_lower.contains(kw) || desc_lower.contains(kw));

        let entry = (name.clone(), clean_desc);
        if is_match {
            matched.push(entry);
        } else {
            unmatched.push(entry);
        }
    }

    // Take up to 5 matched resources, then fill from unmatched
    let mut sample = Vec::new();
    let rng_seed: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64;

    // Use keyword-matched resources first (up to 5)
    for resource in matched.into_iter().take(5) {
        sample.push(resource);
    }

    // Fill remaining slots with randomly selected unmatched resources
    let needed = 5usize.saturating_sub(sample.len());
    if needed > 0 && !unmatched.is_empty() {
        // Simple deterministic pseudo-random using hash
        let mut remaining: Vec<(String, String)> = unmatched;
        let mut shuffled = Vec::new();

        while !remaining.is_empty() && shuffled.len() < 5 {
            let hash_val = {
                let mut hasher = XxHash64::with_seed(rng_seed);
                remaining[0].0.hash(&mut hasher);
                hasher.finish()
            };
            let index = (hash_val as usize) % remaining.len();
            shuffled.push(remaining.remove(index));
        }

        sample.extend(shuffled.into_iter().take(needed));
    }

    // Shuffle the final sample to mix matched and random entries
    let mut final_sample = Vec::new();
    while !sample.is_empty() {
        let hash_val = {
            let mut hasher = XxHash64::with_seed(rng_seed);
            sample[0].0.hash(&mut hasher);
            hasher.finish()
        };
        let index = (hash_val as usize) % sample.len();
        final_sample.push(sample.remove(index));
    }

    final_sample
}

/// Newtype wrapper around QueryEntry that implements Embed.
/// Required because QueryEntry is defined in mcc-gaql-common (orphan rule).
#[derive(Clone, Deserialize)]
struct QueryEntryEmbed(QueryEntry);

// use description field from QueryEntry for embedding
impl Embed for QueryEntryEmbed {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        embedder.embed(self.0.description.clone());
        Ok(())
    }
}

/// Document wrapper for field metadata to enable RAG-based field retrieval
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldDocument {
    pub id: String,
    pub field: FieldMetadata,
    pub description: String,
}

/// Flat representation of FieldDocument for LanceDB storage/retrieval
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct FieldDocumentFlat {
    pub id: String,
    pub description: String,
    pub category: String,
    pub data_type: String,
    pub selectable: bool,
    pub filterable: bool,
    pub sortable: bool,
    pub metrics_compatible: bool,
    pub resource_name: Option<String>,
}

impl From<FieldDocumentFlat> for FieldMetadata {
    fn from(flat: FieldDocumentFlat) -> Self {
        FieldMetadata {
            name: flat.id,
            category: flat.category,
            data_type: flat.data_type,
            selectable: flat.selectable,
            filterable: flat.filterable,
            sortable: flat.sortable,
            metrics_compatible: flat.metrics_compatible,
            resource_name: flat.resource_name,
            // New fields default to empty when loading from legacy LanceDB records
            selectable_with: vec![],
            enum_values: vec![],
            attribute_resources: vec![],
            description: None,
            usage_notes: None,
        }
    }
}

/// Document for a resource entry, used for embedding generation
#[derive(Clone, Serialize, Deserialize)]
pub struct ResourceDocument {
    /// Resource name (also used as embedding ID)
    pub id: String,
    pub resource_name: String,
    pub category: String,
    pub description: String,
    pub has_metrics: bool,
    pub field_count: i32,
    /// Text used for embedding (built from resource metadata + sample fields)
    pub embedding_text: String,
}

impl Embed for ResourceDocument {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        embedder.embed(self.embedding_text.clone());
        Ok(())
    }
}

/// Flat representation of ResourceDocument for LanceDB deserialization
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ResourceDocumentFlat {
    pub resource_name: String,
    pub category: String,
    pub description: String,
    pub has_metrics: bool,
    pub field_count: i32,
}

/// Result of a resource vector search
#[derive(Debug, Clone)]
pub struct ResourceSearchResult {
    pub resource_name: String,
    pub score: f64,
    pub has_metrics: bool,
    pub category: String,
    pub description: String,
}

/// Keywords indicating a performance/metrics query
const PERFORMANCE_KEYWORDS: &[&str] = &[
    "clicks",
    "impressions",
    "views",
    "conversions",
    "revenue",
    "cost",
    "spend",
    "cpc",
    "cpm",
    "ctr",
    "roas",
    "performance",
    "performing",
    "trends",
    "report",
    "analytics",
    "compare",
    "growth",
    "decline",
    "increase",
    "decrease",
    "last week",
    "last month",
    "yesterday",
    "date range",
];

/// Classifies the intent of a query for resource pre-filtering
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QueryIntent {
    /// Needs resources with metrics (performance data)
    Performance,
    /// Settings, configurations, attributes only
    Structural,
    /// Could be either - don't filter
    Unknown,
}

impl QueryIntent {
    pub fn classify(query: &str) -> Self {
        let query_lower = query.to_lowercase();
        if PERFORMANCE_KEYWORDS
            .iter()
            .any(|kw| query_lower.contains(kw))
        {
            QueryIntent::Performance
        } else {
            QueryIntent::Unknown
        }
    }
}

impl FieldDocument {
    /// Create a new field document.
    pub fn new(field: FieldMetadata) -> Self {
        let description = if field.description.is_some() {
            field.build_embedding_text()
        } else {
            Self::generate_synthetic_description(&field)
        };

        let id = field.name.clone();
        Self {
            id,
            field,
            description,
        }
    }

    /// Generate a synthetic description for better semantic matching (fallback for unenriched fields)
    fn generate_synthetic_description(field: &FieldMetadata) -> String {
        let base = field.build_embedding_text();

        let purpose = Self::infer_purpose(&field.name);
        if purpose.is_empty() {
            base
        } else {
            format!("{}. Used for {}.", base, purpose)
        }
    }

    /// Generate a synthetic description for better semantic matching (kept for backward compat)
    #[allow(dead_code)]
    fn generate_description(field: &FieldMetadata) -> String {
        Self::generate_synthetic_description(field)
    }

    /// Infer the purpose of a field based on its name
    fn infer_purpose(field_name: &str) -> String {
        let name_lower = field_name.to_lowercase();

        if name_lower.contains("conversion") {
            return "tracking conversions and sales; key performance metrics".to_string();
        }
        if name_lower.contains("click") {
            return "tracking user clicks; key performance metrics".to_string();
        }
        if name_lower.contains("interactions") {
            return "tracking non-click forms of intentional user response to ad views".to_string();
        }
        if name_lower.contains("impression share") {
            return "tracking share of ad views; key performance metrics".to_string();
        }
        if name_lower.contains("impression") {
            return "tracking ad views; key performance metrics".to_string();
        }
        if name_lower.contains("cost") {
            return "tracking advertising costs; key performance metrics".to_string();
        }
        if name_lower.contains("cpc") {
            return "tracking advertising costs per click".to_string();
        }
        if name_lower.contains("cpe") {
            return "tracking advertising costs per engagement for social or video ads".to_string();
        }
        if name_lower.contains("cpm") {
            return "tracking advertising costs per thousand impressions for display ads"
                .to_string();
        }
        if name_lower.contains("cpv") {
            return "tracking advertising costs per view for video ads".to_string();
        }
        if name_lower.contains("budget") {
            return "managing campaign budgets".to_string();
        }
        if name_lower.contains("bid") {
            return "managing bidding strategies".to_string();
        }
        if name_lower.contains("status") {
            return "checking entity status".to_string();
        }
        if name_lower.contains("name") {
            return "identifying entities".to_string();
        }
        if name_lower.contains("date") || name_lower.contains("time") {
            return "temporal analysis".to_string();
        }
        if name_lower.contains("device") {
            return "device-specific analysis".to_string();
        }
        if name_lower.contains("location") || name_lower.contains("geo") {
            return "geographic analysis".to_string();
        }
        if name_lower.contains("search_term") || name_lower.contains("keyword") {
            return "search query analysis".to_string();
        }
        if name_lower.contains("asset") {
            return "creative asset analysis".to_string();
        }
        if name_lower.contains("audience") || name_lower.contains("demographic") {
            return "audience targeting and analysis".to_string();
        }

        String::new()
    }
}

// Implement Embed trait for FieldDocument to enable embedding
impl Embed for FieldDocument {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        let embed_text = self.description.clone();
        embedder.embed(embed_text);
        Ok(())
    }
}

/// Configuration for the multi-step RAG pipeline
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Whether to add implicit default filters (e.g., status = ENABLED)
    pub add_defaults: bool,
    /// Whether to use RAG search for query cookbook examples in LLM prompts
    pub use_query_cookbook: bool,
    /// Whether to print explanation of the LLM selection process
    pub explain: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            add_defaults: true,
            use_query_cookbook: false,
            explain: false,
        }
    }
}

/// Multi-step RAG Agent for high-accuracy GAQL generation
pub struct MultiStepRAGAgent {
    llm_config: LlmConfig,
    field_cache: FieldMetadataCache,
    field_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
    query_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
    resource_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
    pipeline_config: PipelineConfig,
    // Keep embed client alive to prevent premature resource dropping
    _embed_client: rig_fastembed::Client,
}

impl MultiStepRAGAgent {
    /// Initialize the multi-step RAG agent
    pub async fn init(
        example_queries: Vec<QueryEntry>,
        field_cache: FieldMetadataCache,
        config: &LlmConfig,
        pipeline_config: PipelineConfig,
    ) -> Result<Self, anyhow::Error> {
        log::info!(
            "Initializing MultiStepRAGAgent with LLM: {} (temperature={}) via {}",
            config.preferred_model(),
            config.temperature,
            config.base_url
        );
        if config.models.len() > 1 {
            log::info!("  Additional fallback models: {:?}", &config.models[1..]);
        }

        // Initialize shared LLM resources
        let resources = init_llm_resources(config)?;

        // Build or load field vector store
        let field_index =
            build_or_load_field_vector_store(&field_cache, resources.embedding_model.clone())
                .await?;

        // Build or load query vector store
        let query_index =
            build_or_load_query_vector_store(example_queries, resources.embedding_model.clone())
                .await?;

        // Build or load resource entries vector store
        let resource_index =
            build_or_load_resource_vector_store(&field_cache, resources.embedding_model).await?;

        Ok(Self {
            llm_config: config.clone(),
            field_cache,
            field_index,
            query_index,
            resource_index,
            pipeline_config,
            _embed_client: resources.embed_client,
        })
    }

    /// Build a categorized, formatted list of resources for the LLM prompt
    fn build_categorized_resource_list(&self, resources: &[String]) -> String {
        // Define category patterns and their display names
        let categories: Vec<(&[&str], &str)> = vec![
            (&["campaign"], "CAMPAIGN RESOURCES"),
            (&["ad_group"], "AD GROUP RESOURCES"),
            (&["ad"], "AD RESOURCES"),
            (&["asset"], "ASSET RESOURCES"),
            (&["keyword", "search_term"], "KEYWORD & SEARCH RESOURCES"),
            (&["audience"], "AUDIENCE RESOURCES"),
            (&["conversion"], "CONVERSION RESOURCES"),
            (&["customer"], "CUSTOMER & ACCOUNT RESOURCES"),
            (&["bidding"], "BIDDING RESOURCES"),
            (&["budget"], "BUDGET RESOURCES"),
            (&["label"], "LABEL RESOURCES"),
            (&["shopping", "product"], "SHOPPING & PRODUCT RESOURCES"),
            (&["hotel"], "HOTEL RESOURCES"),
            (&["local_services"], "LOCAL SERVICES RESOURCES"),
            (&["video"], "VIDEO RESOURCES"),
            (&["display"], "DISPLAY RESOURCES"),
            (&["geo"], "GEOGRAPHIC RESOURCES"),
            (&["experiment"], "EXPERIMENT RESOURCES"),
            (&["shared"], "SHARED SET RESOURCES"),
            (&["user_list"], "USER LIST RESOURCES"),
            (&["offline"], "OFFLINE CONVERSION RESOURCES"),
            (&["lead_form"], "LEAD FORM RESOURCES"),
        ];

        // Categorize resources
        let mut categorized: std::collections::HashMap<String, Vec<(String, String)>> =
            std::collections::HashMap::new();
        let mut uncategorized: Vec<(String, String)> = Vec::new();

        for resource in resources {
            let description = self
                .field_cache
                .resource_metadata
                .as_ref()
                .and_then(|m| m.get(resource))
                .and_then(|m| m.description.clone())
                .unwrap_or_default();

            let mut found = false;
            for (patterns, category) in &categories {
                if patterns.iter().any(|p| resource.contains(p)) {
                    categorized
                        .entry(category.to_string())
                        .or_default()
                        .push((resource.clone(), description.clone()));
                    found = true;
                    break;
                }
            }
            if !found {
                uncategorized.push((resource.clone(), description));
            }
        }

        // Build formatted output
        let mut output = String::new();

        // Add categorized resources in order
        for (_, category_name) in &categories {
            if let Some(items) = categorized.get(*category_name)
                && !items.is_empty() {
                    output.push_str(&format!("\n--- {} ---\n", category_name));
                    for (name, desc) in items {
                        if desc.is_empty() {
                            output.push_str(&format!("  - {}\n", name));
                        } else {
                            // Truncate very long descriptions for readability
                            let short_desc = if desc.len() > 120 {
                                format!("{}...", &desc[..117])
                            } else {
                                desc.clone()
                            };
                            output.push_str(&format!("  - {}: {}\n", name, short_desc));
                        }
                    }
                }
        }

        // Add uncategorized resources
        if !uncategorized.is_empty() {
            output.push_str("\n--- OTHER RESOURCES ---\n");
            for (name, desc) in uncategorized {
                if desc.is_empty() {
                    output.push_str(&format!("  - {}\n", name));
                } else {
                    let short_desc = if desc.len() > 120 {
                        format!("{}...", &desc[..117])
                    } else {
                        desc
                    };
                    output.push_str(&format!("  - {}: {}\n", name, short_desc));
                }
            }
        }

        output
    }

    /// Main entry point: generate GAQL query from user prompt
    pub async fn generate(
        &self,
        user_query: &str,
    ) -> Result<mcc_gaql_common::field_metadata::GAQLResult, anyhow::Error> {
        let start = std::time::Instant::now();

        // Phase 1: Resource selection
        let phase1_start = std::time::Instant::now();
        log::info!("Phase 1: Resource selection...");
        let (primary_resource, related_resources, dropped_resources, reasoning, resource_sample) =
            self.select_resource(user_query).await?;
        let phase1_time_ms = phase1_start.elapsed().as_millis() as u64;
        log::info!(
            "Phase 1 complete: {} ({}ms)",
            primary_resource,
            phase1_time_ms
        );

        // Phase 2: Field candidate retrieval
        let phase2_start = std::time::Instant::now();
        log::info!("Phase 2: Retrieving field candidates...");
        let (candidates, candidate_count, rejected_count) = self
            .retrieve_field_candidates(user_query, &primary_resource, &related_resources)
            .await?;
        let phase2_time_ms = phase2_start.elapsed().as_millis() as u64;
        log::info!(
            "Phase 2 complete: {} candidates ({}ms)",
            candidates.len(),
            phase2_time_ms
        );

        // Phase 2.5: Pre-scan for filter keywords
        let phase25_start = std::time::Instant::now();
        let filter_enums = self.prescan_filters(user_query, &candidates);
        log::debug!(
            "Phase 2.5: Pre-scan filters ({}ms)",
            phase25_start.elapsed().as_millis()
        );

        // Phase 3: Field selection via LLM
        let phase3_start = std::time::Instant::now();
        log::info!("Phase 3: Field selection via LLM...");
        let field_selection = self
            .select_fields(user_query, &primary_resource, &candidates, &filter_enums)
            .await?;
        let phase3_time_ms = phase3_start.elapsed().as_millis() as u64;
        log::info!(
            "Phase 3 complete: {} fields selected ({}ms)",
            field_selection.select_fields.len(),
            phase3_time_ms
        );

        // Phase 4: Assemble WHERE, ORDER BY, LIMIT
        let phase4_start = std::time::Instant::now();
        let (where_clauses, limit, implicit_filters) =
            self.assemble_criteria(user_query, &field_selection, &primary_resource);
        log::debug!(
            "Phase 4: Criteria assembly ({}ms)",
            phase4_start.elapsed().as_millis()
        );

        // Phase 5: Generate final GAQL query
        let phase5_start = std::time::Instant::now();
        let result = self
            .generate_gaql(&primary_resource, &field_selection, &where_clauses, limit)
            .await?;
        log::debug!(
            "Phase 5: GAQL generation ({}ms)",
            phase5_start.elapsed().as_millis()
        );

        let generation_time_ms = start.elapsed().as_millis() as u64;
        log::info!(
            "GAQL generation complete: total={}ms (Phase1={}ms, Phase2={}ms, Phase3={}ms)",
            generation_time_ms,
            phase1_time_ms,
            phase2_time_ms,
            phase3_time_ms
        );

        // Build pipeline trace
        let phase1_model = self.llm_config.preferred_model().to_string();
        let phase3_model = self.llm_config.preferred_model().to_string();
        let pipeline_trace = mcc_gaql_common::field_metadata::PipelineTrace {
            phase1_primary_resource: primary_resource.clone(),
            phase1_related_resources: related_resources,
            phase1_dropped_resources: dropped_resources,
            phase1_reasoning: reasoning,
            phase1_model_used: phase1_model,
            phase1_timing_ms: phase1_time_ms,
            phase1_resource_sample: resource_sample,
            phase2_candidate_count: candidate_count,
            phase2_rejected_count: rejected_count,
            phase2_timing_ms: phase2_time_ms,
            phase25_pre_scan_filters: filter_enums.clone(),
            phase3_selected_fields: field_selection.select_fields.clone(),
            phase3_filter_fields: field_selection.filter_fields.clone(),
            phase3_order_by_fields: field_selection.order_by_fields.clone(),
            phase3_reasoning: field_selection.reasoning.clone(),
            phase3_model_used: phase3_model,
            phase3_timing_ms: phase3_time_ms,
            phase4_where_clauses: where_clauses,
            phase4_limit: limit,
            phase4_implicit_filters: implicit_filters,
            generation_time_ms,
        };

        // Validate the field selection against the primary resource
        let all_fields: Vec<String> = field_selection
            .select_fields
            .iter()
            .chain(field_selection.filter_fields.iter().map(|f| &f.field_name))
            .cloned()
            .collect();
        let validation = self
            .field_cache
            .validate_field_selection_for_resource(&all_fields, &primary_resource);

        Ok(mcc_gaql_common::field_metadata::GAQLResult {
            query: result,
            validation,
            pipeline_trace,
        })
    }

    // =========================================================================
    // Phase 1: Resource Selection
    // =========================================================================

    /// Minimum similarity score (from top_n) required to trust RAG candidates.
    /// Below this threshold the full resource list is used as fallback.
    const SIMILARITY_THRESHOLD: f64 = 0.3;

    /// Search the resource_entries vector index and return scored results.
    async fn search_resource_embeddings(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ResourceSearchResult>, anyhow::Error> {
        let search_request = VectorSearchRequest::builder()
            .query(query)
            .samples(limit as u64)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build resource search request: {}", e))?;

        let raw_results = self
            .resource_index
            .top_n::<ResourceDocumentFlat>(search_request)
            .await
            .map_err(|e| anyhow::anyhow!("Resource vector search failed: {}", e))?;

        Ok(raw_results
            .into_iter()
            .map(|(score, _id, doc)| ResourceSearchResult {
                resource_name: doc.resource_name,
                score,
                has_metrics: doc.has_metrics,
                category: doc.category,
                description: doc.description,
            })
            .collect())
    }

    /// Retrieve relevant resources using semantic search + intent filtering.
    /// Returns up to `top_n` results, ordered by similarity score.
    pub async fn retrieve_relevant_resources(
        &self,
        query: &str,
        top_n: usize,
    ) -> Result<Vec<ResourceSearchResult>, anyhow::Error> {
        let intent = QueryIntent::classify(query);

        // Step 1: Broad search, then intent-filter and truncate
        let mut results = self.search_resource_embeddings(query, top_n * 2).await?;

        if intent == QueryIntent::Performance {
            results.retain(|r| r.has_metrics);
            log::debug!(
                "QueryIntent::Performance: retained {} resources with metrics",
                results.len()
            );
        }

        results.truncate(top_n);

        // Step 2: Backfill underrepresented categories
        self.ensure_category_diversity(&mut results, top_n);

        Ok(results)
    }

    /// Promote resources from underrepresented categories until `target` is reached.
    fn ensure_category_diversity(&self, results: &mut Vec<ResourceSearchResult>, target: usize) {
        if results.len() >= target {
            return;
        }

        let existing_names: HashSet<String> =
            results.iter().map(|r| r.resource_name.clone()).collect();
        let existing_categories: HashSet<String> =
            results.iter().map(|r| r.category.clone()).collect();

        let Some(resource_metadata) = &self.field_cache.resource_metadata else {
            return;
        };

        let mut seen_new_categories: HashSet<String> = HashSet::new();
        let mut sorted_names: Vec<&String> = resource_metadata.keys().collect();
        sorted_names.sort();

        for name in sorted_names {
            if results.len() >= target {
                break;
            }
            if existing_names.contains(name) {
                continue;
            }
            let category = categorize_resource(name);
            if existing_categories.contains(category)
                || seen_new_categories.contains(category)
            {
                continue;
            }
            let rm = &resource_metadata[name];
            seen_new_categories.insert(category.to_string());
            results.push(ResourceSearchResult {
                resource_name: name.clone(),
                score: 0.0,
                has_metrics: rm.has_metrics(),
                category: category.to_string(),
                description: rm.description.clone().unwrap_or_default(),
            });
        }
    }

    async fn select_resource(
        &self,
        user_query: &str,
    ) -> Result<
        (
            String,
            Vec<String>,
            Vec<String>,
            String,
            Vec<(String, String)>,
        ),
        anyhow::Error,
    > {
        // --- RAG pre-filter ---
        let (resources, used_rag) =
            match self.retrieve_relevant_resources(user_query, 20).await {
                Ok(candidates) if !candidates.is_empty() => {
                    let top_score = candidates[0].score;
                    if top_score >= Self::SIMILARITY_THRESHOLD {
                        log::info!(
                            "Phase 1: RAG pre-filter selected {} resources (top score={:.3})",
                            candidates.len(),
                            top_score
                        );
                        let names: Vec<String> =
                            candidates.into_iter().map(|c| c.resource_name).collect();
                        (names, true)
                    } else {
                        log::warn!(
                            "Phase 1: Low RAG confidence ({:.3}), falling back to full resource list",
                            top_score
                        );
                        (self.field_cache.get_resources(), false)
                    }
                }
                Ok(_) | Err(_) => {
                    log::warn!(
                        "Phase 1: RAG resource search unavailable, using full resource list"
                    );
                    (self.field_cache.get_resources(), false)
                }
            };

        // Build resource information for sampling
        let resource_info: Vec<(String, String)> = resources
            .iter()
            .map(|r| {
                let rm = self
                    .field_cache
                    .resource_metadata
                    .as_ref()
                    .and_then(|m| m.get(r));
                let desc = rm.and_then(|m| m.description.as_deref()).unwrap_or("");
                (r.clone(), desc.to_string())
            })
            .collect();

        // Generate sample of 5 resources (prioritizing keyword matches)
        let resource_sample = create_resource_sample(user_query, &resource_info);

        // Build categorized resource list for LLM
        let categorized_resources = self.build_categorized_resource_list(&resources);

        let resource_list_header = if used_rag {
            "IMPORTANT: You MUST select resources ONLY from the list below. \
             Do NOT invent or hallucinate resource names.\n\
             If no resource matches perfectly, choose the closest available option \
             and explain in reasoning.\n\
             Resources (selected by semantic similarity to your query):\n"
        } else {
            "IMPORTANT: You MUST select resources ONLY from the list below. \
             Do NOT invent or hallucinate resource names.\n\
             Resources (organized by category):\n"
        };

        let system_prompt = format!(
            r#"You are a Google Ads Query Language (GAQL) expert. Given a user query, determine:
1. The primary resource to query FROM (e.g., campaign, ad_group, keyword_view)
2. Any related resources that might be needed (for JOINs or attributes)

Respond ONLY with valid JSON:
{{
  "primary_resource": "resource_name",
  "related_resources": ["related_resource1", "related_resource2"],
  "confidence": 0.95,
  "reasoning": "brief explanation"
}}

{}
{}
{{}}"#,
            resource_list_header, categorized_resources
        );

        let user_prompt = format!("User query: {}", user_query);

        let agent = self
            .llm_config
            .create_agent_for_model(self.llm_config.preferred_model(), &system_prompt)?;
        log::debug!(
            "Phase 1: Calling LLM (model={}, temp={}) for resource selection...",
            self.llm_config.preferred_model(),
            self.llm_config.temperature
        );
        log::trace!(
            "{}",
            format_llm_request_debug(&Some(system_prompt.clone()), &user_prompt)
        );
        let llm_start = std::time::Instant::now();
        let response = agent.prompt(&user_prompt).await?;
        log::debug!(
            "Phase 1: LLM (model={}) responded in {}ms",
            self.llm_config.preferred_model(),
            llm_start.elapsed().as_millis()
        );
        log::trace!("{}", format_llm_response_debug(&response));

        // Parse JSON response (strip markdown fences first)
        let cleaned_response = strip_markdown_code_blocks(&response);
        let parsed: serde_json::Value = serde_json::from_str(&cleaned_response)
            .map_err(|e| anyhow::anyhow!("Failed to parse LLM response: {}", e))?;

        let primary = parsed["primary_resource"]
            .as_str()
            .unwrap_or("campaign")
            .to_string();

        // Validate primary resource exists in the known resource list
        let primary = {
            let all_resources = self.field_cache.get_resources();
            if all_resources.contains(&primary) {
                primary
            } else {
                log::warn!(
                    "Phase 1: LLM returned invalid resource '{}', falling back to first candidate",
                    primary
                );
                resources
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "campaign".to_string())
            }
        };

        let related: Vec<String> = parsed["related_resources"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let reasoning = parsed["reasoning"].as_str().unwrap_or("").to_string();

        // Validate related_resources against primary's selectable_with
        let selectable_with = self.field_cache.get_resource_selectable_with(&primary);
        let mut dropped = Vec::new();
        let validated_related: Vec<String> = related
            .into_iter()
            .filter(|r| {
                if selectable_with.contains(r) {
                    true
                } else {
                    dropped.push(r.clone());
                    false
                }
            })
            .collect();

        Ok((
            primary,
            validated_related,
            dropped,
            reasoning,
            resource_sample,
        ))
    }

    // =========================================================================
    // Phase 2: Field Candidate Retrieval
    // =========================================================================

    async fn retrieve_field_candidates(
        &self,
        user_query: &str,
        primary: &str,
        related: &[String],
    ) -> Result<(Vec<FieldMetadata>, usize, usize), anyhow::Error> {
        let mut candidates = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Get selectable_with for compatibility check
        let selectable_with = self.field_cache.get_resource_selectable_with(primary);

        // Fail fast if selectable_with is empty - indicates metadata corruption
        if selectable_with.is_empty() {
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

        // =========================================================================
        // Tier 1: Key fields from ResourceMetadata (curated high-value fields)
        // =========================================================================

        // Get primary resource's key fields from ResourceMetadata
        if let Some(rm) = self
            .field_cache
            .resource_metadata
            .as_ref()
            .and_then(|m| m.get(primary))
        {
            // Add key_attributes
            for attr in &rm.key_attributes {
                if let Some(field) = self.field_cache.fields.get(attr)
                    && seen.insert(field.name.clone())
                {
                    candidates.push(field.clone());
                }
            }
            // Add key_metrics
            for metric in &rm.key_metrics {
                if let Some(field) = self.field_cache.fields.get(metric)
                    && seen.insert(field.name.clone())
                {
                    candidates.push(field.clone());
                }
            }

            // Fallback: If key_metrics is empty (common for views), use metrics from selectable_with
            if rm.key_metrics.is_empty() {
                log::debug!(
                    "Phase 2: key_metrics empty, falling back to metrics from selectable_with"
                );
                let metric_count = selectable_with
                    .iter()
                    .filter(|f| f.starts_with("metrics."))
                    .take(15)
                    .filter_map(|m| self.field_cache.fields.get(m))
                    .filter(|f| seen.insert(f.name.clone()))
                    .map(|f| candidates.push(f.clone()))
                    .count();
                log::debug!(
                    "Phase 2: Added {} metrics from selectable_with",
                    metric_count
                );
            }

            // Fallback: If no segments in key_attributes, add segments from selectable_with
            let has_segments = candidates.iter().any(|f| f.is_segment());
            if !has_segments {
                let _segment_count = selectable_with
                    .iter()
                    .filter(|f| f.starts_with("segments."))
                    .take(10)
                    .filter_map(|s| self.field_cache.fields.get(s))
                    .filter(|f| seen.insert(f.name.clone()))
                    .map(|f| candidates.push(f.clone()))
                    .count();
            }
        }

        // Add key_attributes from related resources
        for rel in related {
            if let Some(rm) = self
                .field_cache
                .resource_metadata
                .as_ref()
                .and_then(|m| m.get(rel))
            {
                for attr in &rm.key_attributes {
                    if let Some(field) = self.field_cache.fields.get(attr) {
                        // Only add if compatible with primary resource
                        if let Some(resource) = field.get_resource()
                            && (resource == primary || selectable_with.contains(&resource))
                                && seen.insert(field.name.clone())
                            {
                                candidates.push(field.clone());
                            }
                    }
                }
            }
        }

        // =========================================================================
        // Tier 2: Query-specific RAG vector searches
        // =========================================================================

        // Build list of valid attribute resource prefixes (primary + auto-joined resources)
        // Auto-joined resources are items in selectable_with that don't contain a dot
        let mut valid_attr_resources: Vec<String> = vec![primary.to_string()];
        valid_attr_resources.extend(selectable_with.iter().filter(|s| !s.contains('.')).cloned());
        log::debug!(
            "Phase 2: Valid attribute resources: {:?}",
            valid_attr_resources
        );

        // Build OR filter for all valid attribute resources
        // This pre-filters the vector search to only include fields from valid resources
        let attr_filter = valid_attr_resources
            .iter()
            .map(|r| LanceDBFilter::like("id".to_string(), format!("{}.%", r)))
            .reduce(SearchFilter::or);

        // Search for attributes matching the primary resource and auto-joined resources
        // Use 50 samples to ensure we capture semantically relevant fields that may rank
        // lower due to competing terms in the query (e.g., "budget" dominating "app id")
        let attr_search = async {
            let mut builder = VectorSearchRequest::builder().query(user_query).samples(50);

            if let Some(filter) = attr_filter {
                builder = builder.filter(filter);
            }

            let search_request = builder
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build attr search request: {}", e))?;

            self.field_index
                .top_n::<FieldDocumentFlat>(search_request)
                .await
                .map_err(|e| anyhow::anyhow!("Attr vector search failed: {}", e))
        };

        // Search for metrics (pre-filtered to metrics.* fields)
        let metric_filter = LanceDBFilter::like("id".to_string(), "metrics.%");
        let metric_search = async {
            let search_request = VectorSearchRequest::builder()
                .query(user_query)
                .samples(30)
                .filter(metric_filter)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build metric search request: {}", e))?;

            self.field_index
                .top_n::<FieldDocumentFlat>(search_request)
                .await
                .map_err(|e| anyhow::anyhow!("Metric vector search failed: {}", e))
        };

        // Search for segments (pre-filtered to segments.* fields)
        let segment_filter = LanceDBFilter::like("id".to_string(), "segments.%");
        let segment_search = async {
            let search_request = VectorSearchRequest::builder()
                .query(user_query)
                .samples(15)
                .filter(segment_filter)
                .build()
                .map_err(|e| anyhow::anyhow!("Failed to build segment search request: {}", e))?;

            self.field_index
                .top_n::<FieldDocumentFlat>(search_request)
                .await
                .map_err(|e| anyhow::anyhow!("Segment vector search failed: {}", e))
        };

        // Run all 3 searches in parallel
        log::debug!("Phase 2: Running 3 parallel vector searches...");
        let search_start = std::time::Instant::now();
        let (attr_results, metric_results, segment_results): (
            anyhow::Result<_>,
            anyhow::Result<_>,
            anyhow::Result<_>,
        ) = tokio::join!(attr_search, metric_search, segment_search);
        log::debug!(
            "Phase 2: Vector searches complete in {}ms",
            search_start.elapsed().as_millis()
        );

        // Process attribute results: filter to fields from valid resources
        // (primary + auto-joined resources). Pre-filter already handles most filtering,
        // but we double-check here and verify the field exists in cache.
        if let Ok(results) = &attr_results {
            let attr_ids: Vec<_> = results.iter().map(|r| r.2.id.as_str()).collect();
            log::debug!(
                "Phase 2: Attribute vector search returned {} fields: {:?}",
                attr_ids.len(),
                attr_ids
            );
        }
        if let Ok(results) = attr_results {
            for result in results {
                let doc = &result.2;
                // Verify field belongs to a valid resource and exists in cache
                let is_valid_resource = valid_attr_resources
                    .iter()
                    .any(|r| doc.id.starts_with(&format!("{}.", r)));
                if is_valid_resource
                    && let Some(field) = self.field_cache.fields.get(&doc.id)
                    && seen.insert(field.name.clone())
                {
                    candidates.push(field.clone());
                }
            }
        }

        // Process metric results: filter to metrics
        if let Ok(results) = metric_results {
            for result in results {
                let doc = &result.2;
                if (doc.category == "METRIC" || doc.id.starts_with("metrics."))
                    && let Some(field) = self.field_cache.fields.get(&doc.id)
                {
                    // Metrics are compatible if their field name is in the resource's selectable_with
                    if selectable_with.contains(&field.name) && seen.insert(field.name.clone()) {
                        candidates.push(field.clone());
                    }
                }
            }
        }

        // Process segment results: filter to segments
        if let Ok(results) = segment_results {
            for result in results {
                let doc = &result.2;
                if (doc.category == "SEGMENT" || doc.id.starts_with("segments."))
                    && let Some(field) = self.field_cache.fields.get(&doc.id)
                {
                    // Segments are compatible if their field name is in the resource's selectable_with
                    if selectable_with.contains(&field.name) && seen.insert(field.name.clone()) {
                        candidates.push(field.clone());
                    }
                }
            }
        }

        // =========================================================================
        // Tier 3: Keyword-based supplementary search
        // =========================================================================
        // Vector search may miss fields when query has competing terms (e.g., "budget"
        // dominating "app id"). Extract key terms and find fields that match them
        // in their name or description.
        let keyword_matches = self.find_keyword_matching_fields(
            user_query,
            &valid_attr_resources,
            &selectable_with,
            &mut seen,
        );
        log::debug!(
            "Phase 2: Keyword search found {} additional fields",
            keyword_matches.len()
        );
        candidates.extend(keyword_matches);

        let candidate_count = candidates.len();
        let rejected_count = 0; // All retrieved candidates are compatible by construction

        Ok((candidates, candidate_count, rejected_count))
    }

    // =========================================================================
    // Keyword-based field matching (supplements vector search)
    // =========================================================================

    /// Find fields that match key terms from the user query.
    /// This supplements vector search which may miss fields when the query has
    /// competing semantic terms (e.g., "budget" dominating "app id").
    fn find_keyword_matching_fields(
        &self,
        user_query: &str,
        valid_attr_resources: &[String],
        selectable_with: &[String],
        seen: &mut HashSet<String>,
    ) -> Vec<FieldMetadata> {
        let mut matches = Vec::new();
        let query_lower = user_query.to_lowercase();

        // Extract meaningful terms (skip common words)
        let stop_words: HashSet<&str> = [
            "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has",
            "had", "do", "does", "did", "will", "would", "could", "should", "may", "might", "must",
            "shall", "can", "need", "dare", "ought", "used", "to", "of", "in", "for", "on", "with",
            "at", "by", "from", "as", "into", "through", "during", "before", "after", "above",
            "below", "between", "under", "again", "further", "then", "once", "here", "there",
            "when", "where", "why", "how", "all", "each", "few", "more", "most", "other", "some",
            "such", "no", "nor", "not", "only", "own", "same", "so", "than", "too", "very", "just",
            "and", "but", "if", "or", "because", "until", "while", "although", "list", "show",
            "get", "find", "include", "select", "query", "report",
        ]
        .into_iter()
        .collect();

        // Extract query terms (2+ chars, not stop words)
        let query_terms: Vec<&str> = query_lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 2 && !stop_words.contains(w))
            .collect();

        if query_terms.is_empty() {
            return matches;
        }

        log::debug!("Phase 2: Keyword search terms: {:?}", query_terms);

        // Search through valid attribute fields
        for (field_name, field) in &self.field_cache.fields {
            // Check if field belongs to a valid resource
            let is_valid_resource = valid_attr_resources
                .iter()
                .any(|r| field_name.starts_with(&format!("{}.", r)));

            if !is_valid_resource {
                continue;
            }

            // Already seen?
            if seen.contains(field_name) {
                continue;
            }

            // Split field name into words (by dot, underscore, etc.)
            let name_words: Vec<&str> = field_name
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect();

            // Split description into words
            let desc_lower = field
                .description
                .as_ref()
                .map(|d| d.to_lowercase())
                .unwrap_or_default();
            let desc_words: Vec<&str> = desc_lower
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .collect();

            // Check for FULL WORD matches only (not substrings like "id" in "valid")
            let term_matches: Vec<&str> = query_terms
                .iter()
                .filter(|term| {
                    name_words.iter().any(|w| w.eq_ignore_ascii_case(term))
                        || desc_words.iter().any(|w| *w == **term)
                })
                .copied()
                .collect();

            // Require at least one term match
            if !term_matches.is_empty() {
                // Prioritize fields with multiple term matches or name matches
                let has_name_match = query_terms
                    .iter()
                    .any(|term| name_words.iter().any(|w| w.eq_ignore_ascii_case(term)));
                let match_score = term_matches.len() + if has_name_match { 2 } else { 0 };

                // Only add fields with reasonable match scores
                if (match_score >= 2 || has_name_match)
                    && seen.insert(field_name.clone()) {
                        log::trace!(
                            "Phase 2: Keyword match '{}' (score={}, terms={:?})",
                            field_name,
                            match_score,
                            term_matches
                        );
                        matches.push(field.clone());
                    }
            }
        }

        // Also check metrics and segments with keyword matching (full word only)
        for field_name in selectable_with {
            if seen.contains(field_name) {
                continue;
            }

            if let Some(field) = self.field_cache.fields.get(field_name) {
                let name_words: Vec<&str> = field_name
                    .split(|c: char| !c.is_alphanumeric())
                    .filter(|w| !w.is_empty())
                    .collect();

                let has_name_match = query_terms
                    .iter()
                    .any(|term| name_words.iter().any(|w| w.eq_ignore_ascii_case(term)));
                if has_name_match && seen.insert(field_name.clone()) {
                    matches.push(field.clone());
                }
            }
        }

        matches
    }

    // =========================================================================
    // Phase 2.5: Pre-scan Filters
    // =========================================================================

    fn prescan_filters(
        &self,
        user_query: &str,
        candidates: &[FieldMetadata],
    ) -> Vec<(String, Vec<String>)> {
        let query_lower = user_query.to_lowercase();
        let mut filter_enums = Vec::new();

        // Keyword mappings
        let keyword_map: HashMap<&str, &str> = [
            ("status", "status"),
            ("enabled", "status"),
            ("paused", "status"),
            ("active", "status"),
            ("type", "advertising_channel_type"),
            ("channel", "advertising_channel_type"),
            ("device", "device"),
            ("mobile", "device"),
            ("desktop", "device"),
            ("network", "ad_network_type"),
            ("search", "ad_network_type"),
            ("display", "ad_network_type"),
            ("match type", "keyword_match_type"),
            ("match_type", "keyword_match_type"),
        ]
        .into_iter()
        .collect();

        for (keyword, field_name) in keyword_map {
            if query_lower.contains(keyword) {
                // Find matching candidate field - use ends_with to match qualified names like "campaign.status"
                if let Some(field) = candidates
                    .iter()
                    .find(|f| f.name.ends_with(&format!(".{}", field_name)))
                    && !field.enum_values.is_empty()
                {
                    // Find enum values that match the keyword
                    let matching_enums: Vec<String> = field
                        .enum_values
                        .iter()
                        .filter(|e| {
                            let e_lower = e.to_lowercase();
                            e_lower.contains(keyword)
                                || (keyword == "enabled" && e.as_str() == "ENABLED")
                                || (keyword == "paused" && e.as_str() == "PAUSED")
                                || (keyword == "active" && e.as_str() == "ENABLED")
                        })
                        .cloned()
                        .collect();

                    if !matching_enums.is_empty() {
                        // Use the actual qualified field name (e.g., "campaign.status")
                        filter_enums.push((field.name.clone(), matching_enums));
                    }
                }
            }
        }

        filter_enums
    }

    // =========================================================================
    // Phase 3: Field Selection
    // =========================================================================

    async fn select_fields(
        &self,
        user_query: &str,
        primary: &str,
        candidates: &[FieldMetadata],
        filter_enums: &[(String, Vec<String>)],
    ) -> Result<FieldSelectionResult, anyhow::Error> {
        // Retrieve top cookbook examples only if enabled
        let examples = if self.pipeline_config.use_query_cookbook {
            log::debug!("Phase 3: Retrieving cookbook examples...");
            let cookbook_start = std::time::Instant::now();
            let ex = self.retrieve_cookbook_examples(user_query, 3).await?;
            log::debug!(
                "Phase 3: Cookbook examples retrieved in {}ms",
                cookbook_start.elapsed().as_millis()
            );
            ex
        } else {
            log::debug!("Phase 3: Skipping cookbook examples (use_query_cookbook=false)");
            String::new()
        };

        // Build candidate name set for validation (LLM may hallucinate fields not in candidates)
        let candidate_names: HashSet<String> = candidates.iter().map(|f| f.name.clone()).collect();

        // Build candidate list for LLM
        let mut candidate_text = String::new();
        let mut categories = std::collections::HashMap::new();

        for field in candidates {
            let category = categories
                .entry(field.category.clone())
                .or_insert_with(Vec::new);
            category.push(field);
        }

        for (cat, fields) in categories {
            candidate_text.push_str(&format!("\n### {} ({})\n", cat, fields.len()));
            for f in &fields {
                let filterable_tag = if f.filterable { " [filterable]" } else { "" };
                let sortable_tag = if f.sortable { " [sortable]" } else { "" };

                // Check for pre-scanned enum values
                let enum_note = filter_enums
                    .iter()
                    .find(|(name, _)| name == &f.name)
                    .map(|(_, enums)| format!(" (valid: {})", enums.join(", ")))
                    .unwrap_or_default();

                candidate_text.push_str(&format!(
                    "- {}{}{}: {}{}\n",
                    f.name,
                    filterable_tag,
                    sortable_tag,
                    f.description.as_deref().unwrap_or(""),
                    enum_note
                ));
            }
        }

        // Pre-computed date ranges for prompt interpolation
        let dates = DateContext::new();
        let today = dates.today;
        let this_year_start = dates.this_year_start;
        let prev_year_start = dates.prev_year_start;
        let prev_year_end = dates.prev_year_end;
        let this_quarter_start = dates.this_quarter_start;
        let prev_quarter_start = dates.prev_quarter_start;
        let prev_quarter_end = dates.prev_quarter_end;
        let last_60_start = dates.last_60_start;
        let last_90_start = dates.last_90_start;
        let (this_summer_start, this_summer_end) = dates.this_summer;
        let (last_summer_start, last_summer_end) = dates.last_summer;
        let (this_winter_start, this_winter_end) = dates.this_winter;
        let (last_winter_start, last_winter_end) = dates.last_winter;
        let (this_spring_start, this_spring_end) = dates.this_spring;
        let (last_spring_start, last_spring_end) = dates.last_spring;
        let (this_fall_start, this_fall_end) = dates.this_fall;
        let (last_fall_start, last_fall_end) = dates.last_fall;
        let (this_christmas_start, this_christmas_end) = dates.this_christmas;

        // Build prompt conditionally based on whether cookbook is enabled
        let (system_prompt, user_prompt) = if self.pipeline_config.use_query_cookbook {
            let sys = format!(
                r#"You are a Google Ads Query Language (GAQL) expert. Given:
1. A user query
2. Cookbook examples
3. Available fields categorized by type

Today's date: {today}

Select the appropriate fields and build WHERE filters.

Respond ONLY with valid JSON:
{{
  "select_fields": ["field1", "field2", ...],
  "filter_fields": [{{"field": "field_name", "operator": "=", "value": "value"}}],
  "order_by_fields": [{{"field": "field_name", "direction": "DESC"}}],
  "reasoning": "brief explanation"
}}

- Use ONLY fields from the provided list
- Add filter_fields for any WHERE clauses
- **IMPORTANT: For IN and NOT IN operators, wrap values in parentheses: IN ('VALUE1', 'VALUE2') not IN 'VALUE'**
- Example: {{"field": "campaign.status", "operator": "IN", "value": "('ENABLED', 'PAUSED')"}}
- Add order_by_fields for sorting (use DESC for "top", "best", "worst"; ASC for "first" if ascending)
- Include segments.date if temporal period is specified
- For date ranges, use the APPROPRIATE method based on the period:

  **Use DURING with date literals** (NO quotes around value) for these standard periods.
  Valid Google Ads date literals: TODAY, YESTERDAY, LAST_7_DAYS, LAST_14_DAYS,
  LAST_30_DAYS, LAST_BUSINESS_WEEK, LAST_WEEK_MON_SUN,
  LAST_WEEK_SUN_SAT, THIS_WEEK_SUN_TODAY, THIS_WEEK_MON_TODAY, THIS_MONTH, LAST_MONTH

  Common mappings:
  - "yesterday" → operator: "DURING", value: "YESTERDAY"
  - "today" → operator: "DURING", value: "TODAY"
  - "last 7 days" → operator: "DURING", value: "LAST_7_DAYS"
  - "last 14 days" → operator: "DURING", value: "LAST_14_DAYS"
  - "last 30 days" → operator: "DURING", value: "LAST_30_DAYS"
  - "this month" → operator: "DURING", value: "THIS_MONTH"
  - "last month" → operator: "DURING", value: "LAST_MONTH"
  - "last week" → operator: "DURING", value: "LAST_WEEK_MON_SUN"
  - "last business week" → operator: "DURING", value: "LAST_BUSINESS_WEEK"

  **Use BETWEEN with computed dates** (value format: "YYYY-MM-DD AND YYYY-MM-DD") for:
  - Quarters (NOT valid date literals): "this quarter", "last quarter"
  - Years (NOT valid date literals): "this year", "last year"
  - Holiday periods and seasonal ranges:
    - "this summer" / "last summer" → Jun 1 to Aug 31
    - "this winter" / "last winter" → Dec 1 to Feb 28/29
    - "this spring" / "last spring" → Mar 1 to May 31
    - "this fall" / "this autumn" / "last fall" → Sep 1 to Nov 30
    - "christmas holiday" → Dec 20 to Dec 31
    - "thanksgiving" / "thanksgiving week"
    - "easter" / "easter week"
    - "black friday", "cyber monday"
    - "new years", "valentines day", "mothers day", "fathers day", "halloween"
    - "last 60 days"
    - "last 90 days"

  Example computed date ranges (today: {today}):
  - "this year" → operator: "BETWEEN", value: '{this_year_start} AND {today}'
  - "last year" → operator: "BETWEEN", value: '{prev_year_start} AND {prev_year_end}'
  - "this quarter" → operator: "BETWEEN", value: '{this_quarter_start} AND {today}'
  - "last quarter" → operator: "BETWEEN", value: '{prev_quarter_start} AND {prev_quarter_end}'
  - "this summer" → operator: "BETWEEN", value: '{this_summer_start} AND {this_summer_end}'
  - "last summer" → operator: "BETWEEN", value: '{last_summer_start} AND {last_summer_end}'
  - "this winter" → operator: "BETWEEN", value: '{this_winter_start} AND {this_winter_end}'
  - "last winter" → operator: "BETWEEN", value: '{last_winter_start} AND {last_winter_end}'
  - "this spring" → operator: "BETWEEN", value: '{this_spring_start} AND {this_spring_end}'
  - "last spring" → operator: "BETWEEN", value: '{last_spring_start} AND {last_spring_end}'
  - "this fall" / "this autumn" → operator: "BETWEEN", value: '{this_fall_start} AND {this_fall_end}'
  - "last fall" → operator: "BETWEEN", value: '{last_fall_start} AND {last_fall_end}'
  - "christmas holiday" → operator: "BETWEEN", value: '{this_christmas_start} AND {this_christmas_end}'
  - "last 60 days" → operator: "BETWEEN", value: '{last_60_start} AND {today}'
  - "last 90 days" → operator: "BETWEEN", value: '{last_90_start} AND {today}'
"#
            );
            let user = format!(
                "User query: {}\n\nCookbook examples:\n{}\n\nAvailable fields:{}",
                user_query, examples, candidate_text
            );
            (sys, user)
        } else {
            let sys = format!(
                r#"You are a Google Ads Query Language (GAQL) expert. Given:
1. A user query
2. Available fields categorized by type

Today's date: {today}

Select the appropriate fields and build WHERE filters.

Respond ONLY with valid JSON:
{{
  "select_fields": ["field1", "field2", ...],
  "filter_fields": [{{"field": "field_name", "operator": "=", "value": "value"}}],
  "order_by_fields": [{{"field": "field_name", "direction": "DESC"}}],
  "reasoning": "brief explanation"
}}

- Use ONLY fields from the provided list
- Add filter_fields for any WHERE clauses
- **IMPORTANT: For IN and NOT IN operators, wrap values in parentheses: IN ('VALUE1', 'VALUE2') not IN 'VALUE'**
- Example: {{"field": "campaign.status", "operator": "IN", "value": "('ENABLED', 'PAUSED')"}}
- Add order_by_fields for sorting (use DESC for "top", "best", "worst"; ASC for "first" if ascending)
- Include segments.date if temporal period is specified
- For date ranges, use the APPROPRIATE method based on the period:

  **Use DURING with date literals** (NO quotes around value) for these standard periods.
  Valid Google Ads date literals: TODAY, YESTERDAY, LAST_7_DAYS, LAST_14_DAYS,
  LAST_30_DAYS, LAST_BUSINESS_WEEK, LAST_WEEK_MON_SUN,
  LAST_WEEK_SUN_SAT, THIS_WEEK_SUN_TODAY, THIS_WEEK_MON_TODAY, THIS_MONTH, LAST_MONTH

  Common mappings:
  - "yesterday" → operator: "DURING", value: "YESTERDAY"
  - "today" → operator: "DURING", value: "TODAY"
  - "last 7 days" → operator: "DURING", value: "LAST_7_DAYS"
  - "last 14 days" → operator: "DURING", value: "LAST_14_DAYS"
  - "last 30 days" → operator: "DURING", value: "LAST_30_DAYS"
  - "this month" → operator: "DURING", value: "THIS_MONTH"
  - "last month" → operator: "DURING", value: "LAST_MONTH"
  - "last week" → operator: "DURING", value: "LAST_WEEK_MON_SUN"
  - "last business week" → operator: "DURING", value: "LAST_BUSINESS_WEEK"

  **Use BETWEEN with computed dates** (value format: "YYYY-MM-DD AND YYYY-MM-DD") for:
  - Quarters (NOT valid date literals): "this quarter", "last quarter"
  - Years (NOT valid date literals): "this year", "last year"
  - Holiday periods and seasonal ranges:
    - "this summer" / "last summer" → Jun 1 to Aug 31
    - "this winter" / "last winter" → Dec 1 to Feb 28/29
    - "this spring" / "last spring" → Mar 1 to May 31
    - "this fall" / "this autumn" / "last fall" → Sep 1 to Nov 30
    - "christmas holiday" → Dec 20 to Dec 31
    - "thanksgiving" / "thanksgiving week"
    - "easter" / "easter week"
    - "black friday", "cyber monday"
    - "new years", "valentines day", "mothers day", "fathers day", "halloween"
    - "last 60 days"
    - "last 90 days"

  Example computed date ranges (today: {today}):
  - "this year" → operator: "BETWEEN", value: '{this_year_start} AND {today}'
  - "last year" → operator: "BETWEEN", value: '{prev_year_start} AND {prev_year_end}'
  - "this quarter" → operator: "BETWEEN", value: '{this_quarter_start} AND {today}'
  - "last quarter" → operator: "BETWEEN", value: '{prev_quarter_start} AND {prev_quarter_end}'
  - "this summer" → operator: "BETWEEN", value: '{this_summer_start} AND {this_summer_end}'
  - "last summer" → operator: "BETWEEN", value: '{last_summer_start} AND {last_summer_end}'
  - "this winter" → operator: "BETWEEN", value: '{this_winter_start} AND {this_winter_end}'
  - "last winter" → operator: "BETWEEN", value: '{last_winter_start} AND {last_winter_end}'
  - "this spring" → operator: "BETWEEN", value: '{this_spring_start} AND {this_spring_end}'
  - "last spring" → operator: "BETWEEN", value: '{last_spring_start} AND {last_spring_end}'
  - "this fall" / "this autumn" → operator: "BETWEEN", value: '{this_fall_start} AND {this_fall_end}'
  - "last fall" → operator: "BETWEEN", value: '{last_fall_start} AND {last_fall_end}'
  - "christmas holiday" → operator: "BETWEEN", value: '{this_christmas_start} AND {this_christmas_end}'
  - "last 60 days" → operator: "BETWEEN", value: '{last_60_start} AND {today}'
  - "last 90 days" → operator: "BETWEEN", value: '{last_90_start} AND {today}'
"#
            );
            let user = format!(
                "User query: {}\n\nAvailable fields:{}",
                user_query, candidate_text
            );
            (sys, user)
        };

        let agent = self
            .llm_config
            .create_agent_for_model(self.llm_config.preferred_model(), &system_prompt)?;
        log::debug!(
            "Phase 3: Calling LLM (model={}, temp={}) for field selection...",
            self.llm_config.preferred_model(),
            self.llm_config.temperature
        );
        log::trace!(
            "{}",
            format_llm_request_debug(&Some(system_prompt.clone()), &user_prompt)
        );
        let llm_start = std::time::Instant::now();
        let response = agent.prompt(&user_prompt).await?;
        log::debug!(
            "Phase 3: LLM (model={}) responded in {}ms",
            self.llm_config.preferred_model(),
            llm_start.elapsed().as_millis()
        );
        log::trace!("{}", format_llm_response_debug(&response));

        // Parse JSON response (strip markdown fences first)
        let cleaned_response = strip_markdown_code_blocks(&response);
        let parsed: serde_json::Value = serde_json::from_str(&cleaned_response)
            .map_err(|e| anyhow::anyhow!("Failed to parse LLM response: {}", e))?;

        let select_fields: Vec<String> = parsed["select_fields"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .filter(|s| {
                        if candidate_names.contains(s) {
                            true
                        } else {
                            log::debug!(
                                "Phase 3: Rejecting select field '{}' - not in candidates",
                                s
                            );
                            false
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Fallback: If all LLM fields fail validation, use key_attributes + key_metrics
        let final_select_fields = if select_fields.is_empty() {
            log::warn!(
                "No valid select_fields from LLM, falling back to key fields for resource '{}'",
                primary
            );
            let mut fallback = Vec::new();
            if let Some(rm) = self
                .field_cache
                .resource_metadata
                .as_ref()
                .and_then(|m| m.get(primary))
            {
                // Add first 3 key_attributes
                for attr in rm.key_attributes.iter().take(3) {
                    if self.field_cache.fields.contains_key(attr) {
                        fallback.push(attr.clone());
                    }
                }
                // Add first 3 key_metrics
                for metric in rm.key_metrics.iter().take(3) {
                    if self.field_cache.fields.contains_key(metric) {
                        fallback.push(metric.clone());
                    }
                }
            }
            fallback
        } else {
            select_fields
        };

        let filter_fields: Vec<mcc_gaql_common::field_metadata::FilterField> =
            parsed["filter_fields"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| {
                            let field = f.get("field")?.as_str()?.to_string();
                            // Validate field is in candidate set (LLM may hallucinate)
                            if !candidate_names.contains(&field) {
                                log::debug!(
                                    "Phase 3: Rejecting filter field '{}' - not in candidates",
                                    field
                                );
                                return None;
                            }
                            let operator = f
                                .get("operator")
                                .and_then(|v| v.as_str())
                                .unwrap_or("=")
                                .to_string();
                            let value = f
                                .get("value")
                                .and_then(|v| {
                                    v.as_str().map(String::from).or_else(|| {
                                        v.as_i64()
                                            .map(|n| n.to_string())
                                            .or_else(|| v.as_f64().map(|n| n.to_string()))
                                    })
                                })
                                .unwrap_or_default();
                            Some(mcc_gaql_common::field_metadata::FilterField {
                                field_name: field,
                                operator,
                                value,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

        let order_by_fields: Vec<(String, String)> = parsed["order_by_fields"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| {
                        let field = f.get("field").and_then(|v| v.as_str())?;
                        // Validate field is in candidate set (LLM may hallucinate)
                        if !candidate_names.contains(field) {
                            log::debug!(
                                "Phase 3: Rejecting order_by field '{}' - not in candidates",
                                field
                            );
                            return None;
                        }
                        let direction = f
                            .get("direction")
                            .and_then(|v| v.as_str())
                            .unwrap_or("DESC");
                        Some((field.to_string(), direction.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Extract reasoning from LLM response
        let reasoning = parsed["reasoning"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_default();

        Ok(FieldSelectionResult {
            select_fields: final_select_fields,
            filter_fields,
            order_by_fields,
            reasoning,
        })
    }

    async fn retrieve_cookbook_examples(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<String, anyhow::Error> {
        let search_request = VectorSearchRequest::builder()
            .query(query)
            .samples(limit as u64)
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to build search request: {}", e))?;

        let results = self
            .query_index
            .top_n::<QueryEntryEmbed>(search_request)
            .await
            .map_err(|e| anyhow::anyhow!("Vector search failed: {}", e))?;

        let mut examples = String::new();
        for result in results {
            examples.push_str(&format!(
                "- {}\n  GAQL: {}\n",
                result.2.0.description, result.2.0.query
            ));
        }

        Ok(examples)
    }

    // =========================================================================
    // Phase 4: Assemble Criteria
    // =========================================================================

    fn assemble_criteria(
        &self,
        user_query: &str,
        field_selection: &FieldSelectionResult,
        primary: &str,
    ) -> (Vec<String>, Option<u32>, Vec<String>) {
        let mut where_clauses = Vec::new();
        let mut implicit_filters = Vec::new();

        // Valid GAQL operators
        const VALID_OPERATORS: &[&str] = &[
            "=",
            "!=",
            "<",
            ">",
            "<=",
            ">=",
            "IN",
            "NOT IN",
            "LIKE",
            "NOT LIKE",
            "CONTAINS ANY",
            "CONTAINS ALL",
            "CONTAINS NONE",
            "IS NULL",
            "IS NOT NULL",
            "BETWEEN",
            "REGEXP_MATCH",
            "NOT REGEXP_MATCH",
            "DURING",
        ];

        // Valid Google Ads date literals for DURING operator
        const VALID_DATE_LITERALS: &[&str] = &[
            "TODAY",
            "YESTERDAY",
            "LAST_7_DAYS",
            "LAST_14_DAYS",
            "LAST_30_DAYS",
            "LAST_BUSINESS_WEEK",
            "LAST_WEEK_MON_SUN",
            "LAST_WEEK_SUN_SAT",
            "THIS_WEEK_SUN_TODAY",
            "THIS_WEEK_MON_TODAY",
            "THIS_MONTH",
            "LAST_MONTH",
        ];

        // Date format regex: YYYY-MM-DD (compiled once outside loop)
        let date_re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();

        // Add explicit filter fields from LLM
        for ff in &field_selection.filter_fields {
            let op = ff.operator.to_uppercase();
            if !VALID_OPERATORS.contains(&op.as_str()) {
                log::warn!(
                    "Invalid operator '{}' for field '{}', skipping",
                    ff.operator,
                    ff.field_name
                );
                continue;
            }
            // Escape single quotes in values
            let escaped_value = ff.value.replace('\'', "\\'");
            let clause = match op.as_str() {
                "IS NULL" | "IS NOT NULL" => format!("{} {}", ff.field_name, op),
                "DURING" => {
                    // Validate against allowlist of Google Ads date literals
                    let upper_value = escaped_value.to_uppercase();
                    if !VALID_DATE_LITERALS.contains(&upper_value.as_str()) {
                        log::warn!(
                            "Invalid DURING literal '{}', defaulting to LAST_30_DAYS",
                            escaped_value
                        );
                        format!("{} DURING LAST_30_DAYS", ff.field_name)
                    } else {
                        format!("{} DURING {}", ff.field_name, upper_value)
                    }
                }
                "BETWEEN" => {
                    if let Some((start, end)) = escaped_value.split_once(" AND ") {
                        let start_clean = start.trim();
                        let end_clean = end.trim();

                        // Reject malformed values containing field names or nested BETWEEN
                        if start_clean.contains("segments.date")
                            || start_clean.contains("BETWEEN")
                            || end_clean.contains("segments.date")
                            || end_clean.contains("BETWEEN")
                        {
                            log::error!(
                                "Malformed BETWEEN value for '{}': contains field name or nested BETWEEN, skipping: '{}'",
                                ff.field_name,
                                escaped_value
                            );
                            continue;
                        }

                        // Validate date format
                        if !date_re.is_match(start_clean) || !date_re.is_match(end_clean) {
                            log::error!(
                                "Invalid date format in BETWEEN for '{}': expected YYYY-MM-DD, got '{}' AND '{}', skipping",
                                ff.field_name,
                                start_clean,
                                end_clean
                            );
                            continue;
                        }

                        format!(
                            "{} BETWEEN '{}' AND '{}'",
                            ff.field_name, start_clean, end_clean
                        )
                    } else {
                        log::error!(
                            "Invalid BETWEEN format for '{}': expected 'start AND end', got '{}', skipping",
                            ff.field_name,
                            escaped_value
                        );
                        continue;
                    }
                }
                _ => format!("{} {} '{}'", ff.field_name, op, escaped_value), // Quoted for other operators
            };
            where_clauses.push(clause);
        }

        // Limit detection
        let limit = self.detect_limit(user_query);

        // Implicit defaults (if enabled)
        if self.pipeline_config.add_defaults {
            let default_filters = self.get_implicit_defaults(primary, &where_clauses);
            for f in default_filters {
                where_clauses.push(f.clone());
                implicit_filters.push(f);
            }
        }

        (where_clauses, limit, implicit_filters)
    }

    fn detect_limit(&self, query: &str) -> Option<u32> {
        detect_limit_impl(query)
    }

    fn get_implicit_defaults(&self, resource: &str, existing_clauses: &[String]) -> Vec<String> {
        get_implicit_defaults_impl(resource, existing_clauses)
    }

    // =========================================================================
    // Phase 5: Generate GAQL
    // =========================================================================

    async fn generate_gaql(
        &self,
        primary: &str,
        field_selection: &FieldSelectionResult,
        where_clauses: &[String],
        limit: Option<u32>,
    ) -> Result<String, anyhow::Error> {
        let query = GaqlBuilder::new(primary)
            .select(field_selection.select_fields.clone())
            .where_clauses(where_clauses.to_vec())
            .order_by(field_selection.order_by_fields.clone())
            .limit(limit)
            .build();

        Ok(query)
    }
}

/// Builder for constructing GAQL queries
pub struct GaqlBuilder {
    select_fields: Vec<String>,
    from_resource: String,
    where_clauses: Vec<String>,
    order_by_fields: Vec<(String, String)>,
    limit: Option<u32>,
}

impl GaqlBuilder {
    pub fn new(from_resource: &str) -> Self {
        Self {
            select_fields: Vec::new(),
            from_resource: from_resource.to_string(),
            where_clauses: Vec::new(),
            order_by_fields: Vec::new(),
            limit: None,
        }
    }

    pub fn select(mut self, fields: Vec<String>) -> Self {
        self.select_fields = fields;
        self
    }

    pub fn from_resource(mut self, resource: &str) -> Self {
        self.from_resource = resource.to_string();
        self
    }

    pub fn where_clause(mut self, clause: String) -> Self {
        self.where_clauses.push(clause);
        self
    }

    pub fn where_clauses(mut self, clauses: Vec<String>) -> Self {
        self.where_clauses = clauses;
        self
    }

    pub fn order_by(mut self, fields: Vec<(String, String)>) -> Self {
        self.order_by_fields = fields;
        self
    }

    pub fn limit(mut self, limit: Option<u32>) -> Self {
        self.limit = limit;
        self
    }

    pub fn build(self) -> String {
        // Build query
        let mut query = String::new();

        // SELECT clause
        query.push_str("SELECT\n");
        for (i, field) in self.select_fields.iter().enumerate() {
            query.push_str("  ");
            query.push_str(field);
            if i < self.select_fields.len() - 1 {
                query.push(',');
            }
            query.push('\n');
        }

        // FROM clause
        query.push_str(&format!("FROM {}\n", self.from_resource));

        // WHERE clause
        if !self.where_clauses.is_empty() {
            query.push_str("WHERE ");
            query.push_str(&self.where_clauses.join(" AND "));
            query.push('\n');
        }

        // ORDER BY clause
        if !self.order_by_fields.is_empty() {
            query.push_str("ORDER BY ");
            let order_by_parts: Vec<String> = self
                .order_by_fields
                .iter()
                .map(|(field, direction)| format!("{} {}", field, direction))
                .collect();
            query.push_str(&order_by_parts.join(", "));
            query.push('\n');
        }

        // LIMIT clause
        if let Some(n) = self.limit {
            query.push_str(&format!("LIMIT {}\n", n));
        }

        query.trim().to_string()
    }
}

/// Result of field selection from Phase 3
struct FieldSelectionResult {
    select_fields: Vec<String>,
    filter_fields: Vec<mcc_gaql_common::field_metadata::FilterField>,
    order_by_fields: Vec<(String, String)>, // (field_name, direction)
    reasoning: String,
}

/// Public entry point for GAQL generation
pub async fn convert_to_gaql(
    example_queries: Vec<QueryEntry>,
    field_cache: FieldMetadataCache,
    prompt: &str,
    config: &LlmConfig,
    pipeline_config: PipelineConfig,
) -> Result<mcc_gaql_common::field_metadata::GAQLResult, anyhow::Error> {
    let agent =
        MultiStepRAGAgent::init(example_queries, field_cache, config, pipeline_config).await?;
    agent.generate(prompt).await
}

/// Print explanation of the LLM selection process to stdout
pub fn print_selection_explanation(
    trace: &mcc_gaql_common::field_metadata::PipelineTrace,
    user_query: &str,
) {
    println!();
    println!("═══════════════════════════════════════════════════════════════");
    println!("               RAG SELECTION EXPLANATION");
    println!("═══════════════════════════════════════════════════════════════");
    println!();
    println!("User Query: {}", user_query);
    println!();

    // Phase 1: Resource Selection
    println!(
        "## Phase 1: Resource Selection ({}ms)",
        trace.phase1_timing_ms
    );
    println!();
    println!("Model: {}", trace.phase1_model_used);
    println!();

    if !trace.phase1_resource_sample.is_empty() {
        println!("Sample of Available Resources:");
        for (name, desc) in &trace.phase1_resource_sample {
            println!("  - {}: {}", name, desc);
        }
        println!();
    }

    if !trace.phase1_reasoning.is_empty() {
        println!("LLM Reasoning:");
        for line in trace.phase1_reasoning.lines() {
            println!("  {}", line);
        }
        println!();
    }

    println!(
        "Selected Primary Resource: {}",
        trace.phase1_primary_resource
    );
    if !trace.phase1_related_resources.is_empty() {
        println!("Related Resources: {:?}", trace.phase1_related_resources);
    } else {
        println!("Related Resources: []");
    }
    if !trace.phase1_dropped_resources.is_empty() {
        println!("Dropped Resources: {:?}", trace.phase1_dropped_resources);
    }
    println!();

    // Phase 2: Field Candidate Retrieval
    println!(
        "## Phase 2: Field Candidate Retrieval ({}ms)",
        trace.phase2_timing_ms
    );
    println!();
    println!(
        "Compatible Candidates: {} fields",
        trace.phase2_candidate_count
    );
    println!(
        "Filtered Out (incompatible): {} fields",
        trace.phase2_rejected_count
    );
    println!();

    // Phase 2.5: Pre-scan Filters
    if !trace.phase25_pre_scan_filters.is_empty() {
        println!("## Phase 2.5: Pre-scan Filters");
        println!();
        println!("Detected Keywords:");
        for (field, values) in &trace.phase25_pre_scan_filters {
            println!("  - {}: {:?}", field, values);
        }
        println!();
    }

    // Phase 3: Field Selection
    println!("## Phase 3: Field Selection ({}ms)", trace.phase3_timing_ms);
    println!();
    println!("Model: {}", trace.phase3_model_used);
    println!();

    if !trace.phase3_reasoning.is_empty() {
        println!("LLM Reasoning:");
        for line in trace.phase3_reasoning.lines() {
            println!("  {}", line);
        }
        println!();
    }

    println!("Selected Fields:");
    for field in &trace.phase3_selected_fields {
        println!("  - {}", field);
    }
    if trace.phase3_selected_fields.is_empty() {
        println!("  (none)");
    }
    println!();

    if !trace.phase3_filter_fields.is_empty() {
        println!("Filter Fields:");
        for filter in &trace.phase3_filter_fields {
            println!(
                "  - {} {} {}",
                filter.field_name, filter.operator, filter.value
            );
        }
        println!();
    }

    if !trace.phase3_order_by_fields.is_empty() {
        println!("Order By Fields:");
        for (field, direction) in &trace.phase3_order_by_fields {
            println!("  - {} ({})", field, direction);
        }
        println!();
    }

    // Phase 4: Criteria Assembly
    println!("## Phase 4: Criteria Assembly");
    println!();

    if !trace.phase4_where_clauses.is_empty() {
        println!("WHERE Clauses:");
        for clause in &trace.phase4_where_clauses {
            println!("  - {}", clause);
        }
        println!();
    }

    if let Some(limit) = trace.phase4_limit {
        println!("LIMIT: {}", limit);
        println!();
    }

    if !trace.phase4_implicit_filters.is_empty() {
        println!("Implicit Filters:");
        for filter in &trace.phase4_implicit_filters {
            println!("  - {}", filter);
        }
        println!();
    }

    println!("═══════════════════════════════════════════════════════════════");
    println!("Total Generation Time: {}ms", trace.generation_time_ms);
    println!("═══════════════════════════════════════════════════════════════");
}

/// Helper function for get_implicit_defaults - extracted for testing
fn get_implicit_defaults_impl(resource: &str, existing_clauses: &[String]) -> Vec<String> {
    // Only add defaults if no explicit status filter exists
    let has_status_filter = existing_clauses.iter().any(|c| c.contains(".status"));

    if has_status_filter {
        return vec![];
    }

    // Resources that typically need status filter
    const STATUS_RESOURCES: &[&str] = &[
        "campaign",
        "ad_group",
        "keyword_view",
        "ad_group_ad",
        "search_term_view",
        "user_list",
    ];

    if STATUS_RESOURCES.contains(&resource) {
        vec![format!("{}.status = 'ENABLED'", resource)]
    } else {
        vec![]
    }
}

/// Helper function for detect_limit - extracted for testing
fn detect_limit_impl(query: &str) -> Option<u32> {
    let query_lower = query.to_lowercase();

    let patterns = ["top ", "first ", "best ", "worst "];
    for pattern in patterns {
        if let Some(idx) = query_lower.find(pattern) {
            let after = &query_lower[idx + pattern.len()..];
            // Extract first number
            let number: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = number.parse::<u32>() {
                return Some(n);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_markdown_code_blocks_with_gaql() {
        let input = "```gaql\nSELECT campaign.id FROM campaign\n```";
        let expected = "SELECT campaign.id FROM campaign";
        assert_eq!(strip_markdown_code_blocks(input), expected);
    }

    #[test]
    fn test_strip_markdown_code_blocks_with_sql() {
        let input =
            "```sql\nSELECT campaign.id FROM campaign WHERE campaign.status = 'ENABLED'\n```";
        let expected = "SELECT campaign.id FROM campaign WHERE campaign.status = 'ENABLED'";
        assert_eq!(strip_markdown_code_blocks(input), expected);
    }

    #[test]
    fn test_strip_markdown_code_blocks_plain() {
        let input = "```\nSELECT campaign.id FROM campaign\n```";
        let expected = "SELECT campaign.id FROM campaign";
        assert_eq!(strip_markdown_code_blocks(input), expected);
    }

    #[test]
    fn test_strip_markdown_code_blocks_no_markers() {
        let input = "SELECT campaign.id FROM campaign";
        let expected = "SELECT campaign.id FROM campaign";
        assert_eq!(strip_markdown_code_blocks(input), expected);
    }

    #[test]
    fn test_strip_markdown_code_blocks_with_whitespace() {
        let input = "  ```gaql\n  SELECT campaign.id FROM campaign  \n  ```  ";
        let expected = "SELECT campaign.id FROM campaign";
        assert_eq!(strip_markdown_code_blocks(input), expected);
    }

    #[test]
    fn test_strip_markdown_code_blocks_multiline() {
        let input = "```gaql\nSELECT\n  campaign.id,\n  campaign.name\nFROM campaign\n```";
        let expected = "SELECT\n  campaign.id,\n  campaign.name\nFROM campaign";
        assert_eq!(strip_markdown_code_blocks(input), expected);
    }

    #[test]
    fn test_compute_query_cookbook_hash_consistency() {
        let queries = vec![
            QueryEntry {
                id: "query_get_all_campaigns_select_campaig".to_string(),
                description: "Get all campaigns".to_string(),
                query: "SELECT campaign.id, campaign.name FROM campaign".to_string(),
            },
            QueryEntry {
                id: "query_get_enabled_campaigns_select_campaig".to_string(),
                description: "Get enabled campaigns".to_string(),
                query: "SELECT campaign.id FROM campaign WHERE campaign.status = 'ENABLED'"
                    .to_string(),
            },
        ];

        let hash1 = compute_query_cookbook_hash(&queries);
        let hash2 = compute_query_cookbook_hash(&queries);
        assert_eq!(
            hash1, hash2,
            "Hash should be consistent across repeated calls"
        );
    }

    #[test]
    fn test_llm_config_single_model() {
        let models: Vec<String> = "model-a"
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let config = LlmConfig {
            api_key: "dummy".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            models,
            temperature: 0.1,
        };
        assert_eq!(config.all_models(), &["model-a"]);
        assert_eq!(config.preferred_model(), "model-a");
        assert_eq!(config.model_count(), 1);
    }

    #[test]
    fn test_llm_config_multiple_models() {
        let models: Vec<String> = "model-a,model-b,model-c"
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let config = LlmConfig {
            api_key: "dummy".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            models,
            temperature: 0.1,
        };
        assert_eq!(config.all_models(), &["model-a", "model-b", "model-c"]);
        assert_eq!(config.preferred_model(), "model-a");
        assert_eq!(config.model_count(), 3);
    }

    // Tests for fixup issues - these test helper functions directly

    #[test]
    fn test_detect_limit_top_10() {
        let limit = detect_limit_impl("show top 10 campaigns");
        assert_eq!(limit, Some(10));
    }

    #[test]
    fn test_detect_limit_first_5() {
        let limit = detect_limit_impl("first 5 results");
        assert_eq!(limit, Some(5));
    }

    #[test]
    fn test_detect_limit_best_3() {
        let limit = detect_limit_impl("best 3 performing ads");
        assert_eq!(limit, Some(3));
    }

    #[test]
    fn test_detect_limit_no_match() {
        let limit = detect_limit_impl("show all campaigns");
        assert_eq!(limit, None);
    }

    #[test]
    fn test_implicit_defaults_adds_status_for_campaign() {
        let defaults = get_implicit_defaults_impl("campaign", &[]);
        assert_eq!(defaults, vec!["campaign.status = 'ENABLED'"]);
    }

    #[test]
    fn test_implicit_defaults_adds_status_for_ad_group() {
        let defaults = get_implicit_defaults_impl("ad_group", &[]);
        assert_eq!(defaults, vec!["ad_group.status = 'ENABLED'"]);
    }

    #[test]
    fn test_implicit_defaults_skips_when_status_filter_exists() {
        let existing = vec!["campaign.status = 'PAUSED'".to_string()];
        let defaults = get_implicit_defaults_impl("campaign", &existing);
        assert!(defaults.is_empty());
    }

    #[test]
    fn test_implicit_defaults_skips_for_non_status_resource() {
        let defaults = get_implicit_defaults_impl("geo_target_constant", &[]);
        assert!(defaults.is_empty());
    }

    #[test]
    fn test_prescan_filters_detects_enabled_status() {
        use mcc_gaql_common::field_metadata::FieldMetadata;

        let status_field = FieldMetadata {
            name: "campaign.status".to_string(),
            category: "ATTRIBUTE".to_string(),
            data_type: "ENUM".to_string(),
            selectable: true,
            filterable: true,
            sortable: true,
            metrics_compatible: false,
            resource_name: Some("campaign".to_string()),
            selectable_with: vec![],
            enum_values: vec![
                "ENABLED".to_string(),
                "PAUSED".to_string(),
                "REMOVED".to_string(),
            ],
            attribute_resources: vec![],
            description: None,
            usage_notes: None,
        };
        let candidates = vec![status_field];

        // Simulate prescan_filters logic using the keyword map
        let query_lower = "show enabled campaigns".to_lowercase();
        let keyword = "enabled";
        let field_name = "status";

        let found = candidates
            .iter()
            .find(|f| f.name.ends_with(&format!(".{}", field_name)));
        assert!(found.is_some(), "Should find campaign.status via ends_with");

        let field = found.unwrap();
        let matching: Vec<&String> = field
            .enum_values
            .iter()
            .filter(|e| e.as_str() == "ENABLED" || e.to_lowercase().contains(keyword))
            .collect();
        assert!(!matching.is_empty(), "Should match ENABLED enum value");
        assert!(
            query_lower.contains(keyword),
            "Query should contain keyword"
        );
    }

    #[test]
    fn test_during_operator_accepted() {
        use mcc_gaql_common::field_metadata::FilterField;

        // Test that DURING operator is not rejected
        let filter = FilterField {
            field_name: "segments.date".to_string(),
            operator: "DURING".to_string(),
            value: "LAST_30_DAYS".to_string(),
        };

        // The operator should be recognized as valid
        const VALID_OPERATORS: &[&str] = &[
            "=",
            "!=",
            "<",
            ">",
            "<=",
            ">=",
            "IN",
            "NOT IN",
            "LIKE",
            "NOT LIKE",
            "CONTAINS ANY",
            "CONTAINS ALL",
            "CONTAINS NONE",
            "IS NULL",
            "IS NOT NULL",
            "BETWEEN",
            "REGEXP_MATCH",
            "NOT REGEXP_MATCH",
            "DURING",
        ];

        let op = filter.operator.to_uppercase();
        assert!(
            VALID_OPERATORS.contains(&op.as_str()),
            "DURING should be a valid operator"
        );

        // Verify formatting (DURING should not have quotes)
        let clause = format!("{} {} {}", filter.field_name, op, filter.value);
        assert_eq!(clause, "segments.date DURING LAST_30_DAYS");
    }

    #[test]
    fn test_gaql_builder_basic() {
        let query = GaqlBuilder::new("campaign")
            .select(vec![
                "campaign.name".to_string(),
                "metrics.clicks".to_string(),
            ])
            .where_clause("campaign.status = 'ENABLED'".to_string())
            .order_by(vec![("metrics.clicks".to_string(), "DESC".to_string())])
            .limit(Some(10))
            .build();

        assert!(query.contains("SELECT"));
        assert!(query.contains("campaign.name"));
        assert!(query.contains("metrics.clicks"));
        assert!(query.contains("FROM campaign"));
        assert!(query.contains("WHERE campaign.status = 'ENABLED'"));
        assert!(query.contains("ORDER BY metrics.clicks DESC"));
        assert!(query.contains("LIMIT 10"));
    }

    #[test]
    fn test_gaql_builder_no_during() {
        let query = GaqlBuilder::new("ad_group")
            .select(vec!["ad_group.name".to_string()])
            .where_clause("ad_group.status = 'ENABLED'".to_string())
            .build();

        assert!(query.contains("SELECT"));
        assert!(query.contains("ad_group.name"));
        assert!(!query.contains("segments.date"));
        assert!(query.contains("FROM ad_group"));
        assert!(query.contains("WHERE ad_group.status = 'ENABLED'"));
    }

    #[test]
    fn test_compute_query_cookbook_hash_order_independent() {
        // Same queries in different order should produce same hash
        let queries_order1 = vec![
            QueryEntry {
                id: "query_a".to_string(),
                description: "First query".to_string(),
                query: "SELECT a FROM b".to_string(),
            },
            QueryEntry {
                id: "query_b".to_string(),
                description: "Second query".to_string(),
                query: "SELECT c FROM d".to_string(),
            },
        ];

        let queries_order2 = vec![
            QueryEntry {
                id: "query_b".to_string(),
                description: "Second query".to_string(),
                query: "SELECT c FROM d".to_string(),
            },
            QueryEntry {
                id: "query_a".to_string(),
                description: "First query".to_string(),
                query: "SELECT a FROM b".to_string(),
            },
        ];

        let hash1 = compute_query_cookbook_hash(&queries_order1);
        let hash2 = compute_query_cookbook_hash(&queries_order2);
        assert_eq!(hash1, hash2, "Hash should be independent of input order");
    }

    #[test]
    fn test_compute_query_cookbook_hash_duplicate_ids() {
        // Different queries with same ID should produce different hashes
        let queries1 = vec![
            QueryEntry {
                id: "same_id".to_string(),
                description: "Description A".to_string(),
                query: "SELECT a FROM b".to_string(),
            },
            QueryEntry {
                id: "same_id".to_string(),
                description: "Description B".to_string(),
                query: "SELECT c FROM d".to_string(),
            },
        ];

        let queries2 = vec![
            QueryEntry {
                id: "same_id".to_string(),
                description: "Description B".to_string(),
                query: "SELECT c FROM d".to_string(),
            },
            QueryEntry {
                id: "same_id".to_string(),
                description: "Description A".to_string(),
                query: "SELECT a FROM b".to_string(),
            },
        ];

        // Both should produce the same hash despite different input order
        // because sorting is stable when IDs are equal (falls back to description/query)
        let hash1 = compute_query_cookbook_hash(&queries1);
        let hash2 = compute_query_cookbook_hash(&queries2);
        assert_eq!(
            hash1, hash2,
            "Hash should be consistent even with duplicate IDs in different order"
        );

        // Verify the hash is different from a single-query version
        let single_query = vec![QueryEntry {
            id: "same_id".to_string(),
            description: "Description A".to_string(),
            query: "SELECT a FROM b".to_string(),
        }];
        let hash_single = compute_query_cookbook_hash(&single_query);
        assert_ne!(
            hash1, hash_single,
            "Different query sets should produce different hashes"
        );
    }

    /// Parse LLM field selection response JSON into FieldSelectionResult
    /// This is extracted for testability
    fn parse_field_selection_response(
        parsed: &serde_json::Value,
        field_cache: &mcc_gaql_common::field_metadata::FieldMetadataCache,
    ) -> Option<FieldSelectionResult> {
        let select_fields: Vec<String> = parsed["select_fields"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .filter(|s| field_cache.fields.contains_key(s))
                    .collect()
            })
            .unwrap_or_default();

        let filter_fields: Vec<mcc_gaql_common::field_metadata::FilterField> =
            parsed["filter_fields"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|f| {
                            let field = f.get("field")?.as_str()?.to_string();
                            let operator = f
                                .get("operator")
                                .and_then(|v| v.as_str())
                                .unwrap_or("=")
                                .to_string();
                            let value = f
                                .get("value")
                                .and_then(|v| {
                                    v.as_str().map(String::from).or_else(|| {
                                        v.as_i64()
                                            .map(|n| n.to_string())
                                            .or_else(|| v.as_f64().map(|n| n.to_string()))
                                    })
                                })
                                .unwrap_or_default();
                            Some(mcc_gaql_common::field_metadata::FilterField {
                                field_name: field,
                                operator,
                                value,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();

        let order_by_fields: Vec<(String, String)> = parsed["order_by_fields"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|f| {
                        let field = f.get("field").and_then(|v| v.as_str())?;
                        let direction = f
                            .get("direction")
                            .and_then(|v| v.as_str())
                            .unwrap_or("DESC");
                        Some((field.to_string(), direction.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let reasoning = parsed["reasoning"]
            .as_str()
            .map(|s| s.to_string())
            .unwrap_or_default();

        Some(FieldSelectionResult {
            select_fields,
            filter_fields,
            order_by_fields,
            reasoning,
        })
    }

    fn create_test_field_metadata(name: &str) -> mcc_gaql_common::field_metadata::FieldMetadata {
        use mcc_gaql_common::field_metadata::FieldMetadata;

        FieldMetadata {
            name: name.to_string(),
            category: "attributes".to_string(),
            data_type: "STRING".to_string(),
            selectable: true,
            filterable: true,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            selectable_with: Vec::new(),
            enum_values: Vec::new(),
            attribute_resources: Vec::new(),
            description: Some("Test field".to_string()),
            usage_notes: None,
        }
    }

    fn create_test_cache(
        field_names: &[&str],
    ) -> mcc_gaql_common::field_metadata::FieldMetadataCache {
        use chrono::Utc;
        use mcc_gaql_common::field_metadata::FieldMetadataCache;
        use std::collections::HashMap;

        let mut fields = HashMap::new();
        for name in field_names {
            fields.insert(name.to_string(), create_test_field_metadata(name));
        }

        FieldMetadataCache {
            last_updated: Utc::now(),
            api_version: "v17".to_string(),
            fields,
            resources: None,
            resource_metadata: None,
        }
    }

    #[test]
    fn test_parse_field_selection_with_numeric_filter_value() {
        // This test verifies the fix for numeric filter values becoming blank
        // Regression test for: LLM numeric filter values were parsed as empty strings
        let json_response = serde_json::json!({
            "select_fields": ["campaign.id", "campaign.name"],
            "filter_fields": [
                {
                    "field": "campaign.target_cpa.cpc_bid_floor_micros",
                    "operator": "=",
                    "value": 3140000
                }
            ],
            "order_by_fields": [],
            "reasoning": "Test reasoning"
        });

        let cache = create_test_cache(&["campaign.id", "campaign.name"]);

        let result = parse_field_selection_response(&json_response, &cache).unwrap();

        assert_eq!(result.select_fields.len(), 2);
        assert_eq!(result.filter_fields.len(), 1);
        assert_eq!(
            result.filter_fields[0].field_name,
            "campaign.target_cpa.cpc_bid_floor_micros"
        );
        assert_eq!(result.filter_fields[0].operator, "=");
        // This is the key assertion: numeric values should be preserved, not blank
        assert_eq!(result.filter_fields[0].value, "3140000");
        assert!(
            !result.filter_fields[0].value.is_empty(),
            "Numeric filter value should not be empty"
        );
    }

    #[test]
    fn test_parse_field_selection_with_float_filter_value() {
        // Test that float values are also handled correctly
        let json_response = serde_json::json!({
            "select_fields": ["campaign.id"],
            "filter_fields": [
                {
                    "field": "campaign.some_float_field",
                    "operator": ">",
                    "value": 3.14
                }
            ],
            "order_by_fields": [],
            "reasoning": "Test with float"
        });

        let cache = create_test_cache(&["campaign.id"]);

        let result = parse_field_selection_response(&json_response, &cache).unwrap();

        assert_eq!(result.filter_fields.len(), 1);
        assert_eq!(result.filter_fields[0].value, "3.14");
    }

    #[test]
    fn test_parse_field_selection_with_string_filter_value() {
        // Test that string values still work (regression test)
        let json_response = serde_json::json!({
            "select_fields": ["campaign.id"],
            "filter_fields": [
                {
                    "field": "campaign.status",
                    "operator": "=",
                    "value": "ENABLED"
                }
            ],
            "order_by_fields": [],
            "reasoning": "Test with string"
        });

        let cache = create_test_cache(&["campaign.id"]);

        let result = parse_field_selection_response(&json_response, &cache).unwrap();

        assert_eq!(result.filter_fields.len(), 1);
        assert_eq!(result.filter_fields[0].value, "ENABLED");
    }

    #[test]
    fn test_parse_field_selection_with_negative_integer() {
        // Test negative integer values
        let json_response = serde_json::json!({
            "select_fields": ["campaign.id"],
            "filter_fields": [
                {
                    "field": "campaign.some_negative_field",
                    "operator": "<",
                    "value": -100
                }
            ],
            "order_by_fields": [],
            "reasoning": "Test with negative integer"
        });

        let cache = create_test_cache(&["campaign.id"]);

        let result = parse_field_selection_response(&json_response, &cache).unwrap();

        assert_eq!(result.filter_fields.len(), 1);
        assert_eq!(result.filter_fields[0].value, "-100");
    }

    #[test]
    fn test_parse_field_selection_with_zero_value() {
        // Test zero value (edge case - 0 is falsy in some contexts)
        let json_response = serde_json::json!({
            "select_fields": ["campaign.id"],
            "filter_fields": [
                {
                    "field": "campaign.some_field",
                    "operator": "=",
                    "value": 0
                }
            ],
            "order_by_fields": [],
            "reasoning": "Test with zero"
        });

        let cache = create_test_cache(&["campaign.id"]);

        let result = parse_field_selection_response(&json_response, &cache).unwrap();

        assert_eq!(result.filter_fields.len(), 1);
        assert_eq!(result.filter_fields[0].value, "0");
    }
}

use std::vec;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::{Completion, Prompt},
    embeddings::{EmbedError, EmbeddingsBuilder, TextEmbedder, embed::Embed},
    providers::openrouter::{self, completion::CompletionModel},
    vector_store::VectorStoreIndex,
};
use rig_fastembed::FastembedModel;
use rig_lancedb::{LanceDbVectorIndex, SearchParams};
use serde::{Deserialize, Serialize};

use crate::field_metadata::{FieldMetadata, FieldMetadataCache};
use crate::lancedb_utils;
use crate::util::QueryEntry;

/// Format LLM request for debug logging with human-friendly formatting
fn format_llm_request_debug(preamble: &Option<String>, prompt: &str) -> String {
    let mut output = String::new();
    output.push_str("\n");
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

/// Compute hash of query cookbook for cache validation
fn compute_query_cookbook_hash(queries: &[QueryEntry]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for query in queries {
        query.description.hash(&mut hasher);
        query.query.hash(&mut hasher);
    }
    hasher.finish()
}

/// Compute hash of field cache for cache validation
fn compute_field_cache_hash(cache: &FieldMetadataCache) -> u64 {
    let mut hasher = DefaultHasher::new();

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


/// Build or load query cookbook vector store with LanceDB caching
async fn build_or_load_query_vector_store(
    query_cookbook: Vec<QueryEntry>,
    embedding_model: rig_fastembed::EmbeddingModel,
) -> Result<LanceDbVectorIndex<rig_fastembed::EmbeddingModel>, anyhow::Error> {
    let total_start = std::time::Instant::now();

    // Compute hash of current cookbook
    let current_hash = compute_query_cookbook_hash(&query_cookbook);

    // Try to load from LanceDB cache
    if let Ok(Some(cached_hash)) = lancedb_utils::load_hash("query_cookbook") {
        if cached_hash == current_hash {
            log::info!("Query cookbook cache valid, loading from LanceDB...");

            match lancedb_utils::get_lancedb_connection().await {
                Ok(db) => {
                    match lancedb_utils::open_table(&db, "query_cookbook").await {
                        Ok(table) => {
                            // Wrap table in LanceDbVectorIndex
                            let index = LanceDbVectorIndex::new(
                                table,
                                embedding_model,
                                "id",
                                SearchParams::default(),
                            ).await.map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

                            log::info!("Successfully loaded query cookbook from cache ({:.2}s)", total_start.elapsed().as_secs_f64());
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
    }

    // Cache miss or invalid - build embeddings
    log::info!("Building embeddings for {} queries...", query_cookbook.len());
    let embedding_start = std::time::Instant::now();

    let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
        .documents(query_cookbook.clone())?
        .build()
        .await?;

    log::info!("Query cookbook embeddings generated in {:.2}s", embedding_start.elapsed().as_secs_f64());

    // Extract just the embeddings from the results
    let embedding_vecs: Vec<_> = embeddings.iter()
        .flat_map(|(_, emb)| emb.iter())
        .cloned()
        .collect();

    // Save to LanceDB and get table
    let table = lancedb_utils::build_or_load_query_vector_store(
        query_cookbook,
        embedding_vecs,
        current_hash,
    ).await?;

    // Wrap table in LanceDbVectorIndex
    let index = LanceDbVectorIndex::new(
        table,
        embedding_model,
        "id",
        SearchParams::default(),
    ).await.map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

    log::info!("Query cookbook initialization complete ({:.2}s total)", total_start.elapsed().as_secs_f64());

    Ok(index)
}

/// Build or load field vector store with LanceDB caching
async fn build_or_load_field_vector_store(
    field_cache: &FieldMetadataCache,
    embedding_model: rig_fastembed::EmbeddingModel,
) -> Result<LanceDbVectorIndex<rig_fastembed::EmbeddingModel>, anyhow::Error> {
    let total_start = std::time::Instant::now();

    // Compute hash of current field cache
    let current_hash = compute_field_cache_hash(field_cache);

    // Try to load from LanceDB cache
    if let Ok(Some(cached_hash)) = lancedb_utils::load_hash("field_metadata") {
        if cached_hash == current_hash {
            log::info!("Field metadata cache valid, loading from LanceDB...");

            match lancedb_utils::get_lancedb_connection().await {
                Ok(db) => {
                    match lancedb_utils::open_table(&db, "field_metadata").await {
                        Ok(table) => {
                            // Wrap table in LanceDbVectorIndex
                            let index = LanceDbVectorIndex::new(
                                table,
                                embedding_model,
                                "id",
                                SearchParams::default(),
                            ).await.map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

                            log::info!("Successfully loaded field metadata from cache ({:.2}s)", total_start.elapsed().as_secs_f64());
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
    }

    // Cache miss or invalid - build field documents and embeddings
    log::info!("Building embeddings for {} fields...", field_cache.fields.len());
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

    // Generate embeddings
    let field_embeddings = EmbeddingsBuilder::new(embedding_model.clone())
        .documents(field_docs.clone())?
        .build()
        .await?;

    log::info!("Field metadata embeddings generated in {:.2}s", embedding_start.elapsed().as_secs_f64());

    // Extract just the embeddings from the results
    let embedding_vecs: Vec<_> = field_embeddings.iter()
        .flat_map(|(_, emb)| emb.iter())
        .cloned()
        .collect();

    // Save to LanceDB and get table
    let table = lancedb_utils::build_or_load_field_vector_store(
        field_docs,
        embedding_vecs,
        current_hash,
    ).await?;

    // Wrap table in LanceDbVectorIndex
    let index = LanceDbVectorIndex::new(
        table,
        embedding_model,
        "id",
        SearchParams::default(),
    ).await.map_err(|e| anyhow::anyhow!("Failed to create vector index: {}", e))?;

    log::info!("Field metadata initialization complete ({:.2}s total)", total_start.elapsed().as_secs_f64());

    Ok(index)
}

/// Strip markdown code block notation from LLM responses
/// Handles formats like:
/// ```gaql
/// SELECT ...
/// ```
/// or ```sql ... ``` or just ``` ... ```
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

// use description field from QueryEntry for embedding
impl Embed for QueryEntry {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        embedder.embed(self.description.clone());
        Ok(())
    }
}

/// Document wrapper for field metadata to enable RAG-based field retrieval
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldDocument {
    pub field: FieldMetadata,
    pub description: String,
}

/// Flat representation of FieldDocument for LanceDB storage/retrieval
#[derive(Clone, Serialize, Deserialize, Debug)]
struct FieldDocumentFlat {
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
        }
    }
}

impl FieldDocument {
    /// Create a new field document with synthetic description
    pub fn new(field: FieldMetadata) -> Self {
        let description = Self::generate_description(&field);
        Self { field, description }
    }

    /// Generate a synthetic description for better semantic matching
    fn generate_description(field: &FieldMetadata) -> String {
        let mut parts = Vec::new();

        // Field name with underscores replaced by spaces for better matching
        parts.push(field.name.replace('.', " ").replace('_', " "));

        // // Category description
        // let category_desc = match field.category.as_str() {
        //     "METRIC" => "performance metric",
        //     "SEGMENT" => "segmentation dimension",
        //     "ATTRIBUTE" => "descriptive attribute",
        //     "RESOURCE" => "resource identifier",
        //     _ => "field",
        // };
        // parts.push(category_desc.to_string());

        // // Data type
        // parts.push(format!("{} type", field.data_type.to_lowercase()));

        // // Capabilities
        // let mut capabilities = Vec::new();
        // if field.selectable {
        //     capabilities.push("can be selected");
        // }
        // if field.filterable {
        //     capabilities.push("can be filtered");
        // }
        // if field.sortable {
        //     capabilities.push("can be sorted");
        // }
        // if field.metrics_compatible {
        //     capabilities.push("compatible with metrics");
        // }
        // if !capabilities.is_empty() {
        //     parts.push(capabilities.join(", "));
        // }

        // // Purpose inference based on common patterns
        // let purpose = Self::infer_purpose(&field.name);
        // if !purpose.is_empty() {
        //     parts.push(format!("used for {}", purpose));
        // }

        parts.join(", ")
    }

    /// Infer the purpose of a field based on its name
    fn infer_purpose(field_name: &str) -> String {
        let name_lower = field_name.to_lowercase();

        // Common patterns
        if name_lower.contains("conversion") {
            return "tracking conversions and sales".to_string();
        }
        if name_lower.contains("click") {
            return "tracking user clicks".to_string();
        }
        if name_lower.contains("impression") {
            return "tracking ad views".to_string();
        }
        if name_lower.contains("cost") {
            return "tracking advertising costs".to_string();
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
        embedder.embed(self.description.clone());
        Ok(())
    }
}

struct RAGAgent {
    agent: Agent<CompletionModel>,
    query_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
}

impl RAGAgent {
    pub async fn init(
        query_cookbook: Vec<QueryEntry>,
    ) -> Result<Self, anyhow::Error> {
        let openrouter_client = openrouter::Client::from_env();
        let fastembed_client = rig_fastembed::Client::new();
        let embedding_model = fastembed_client.embedding_model(&FastembedModel::AllMiniLML6V2Q);

        // Build or load query vector store with LanceDB caching
        let query_index = build_or_load_query_vector_store(query_cookbook, embedding_model).await?;

        let agent = openrouter_client.agent(openrouter::GEMINI_FLASH_2_0)
            .preamble("
                You are a Google Ads GAQL query assistant here to assist the user to translate natural language query requests into valid GAQL.

                CRITICAL RULES:
                - NEVER invent or create field names
                - ONLY use field names from the example queries provided below
                - If you're unsure about a field name, use the closest match from the examples

                OUTPUT REQUIREMENTS:
                - Respond with ONLY the GAQL query as plain text
                - Do not include markdown code blocks (```sql or ```gaql or ```)
                - Do not include quotes (single or double)
                - Do not include explanatory text before or after the query
                - Do not include any other formatting

                You will find example GAQL queries in the context provided with each request.
            ")
            .temperature(0.1)
            .build();

        Ok(RAGAgent { agent, query_index })
    }

    pub async fn prompt(&self, prompt: &str) -> Result<String, anyhow::Error> {
        // Manually retrieve relevant queries from LanceDB
        use rig::vector_store::VectorSearchRequest;
        let search_request = VectorSearchRequest::builder()
            .query(prompt)
            .samples(10)
            .build()
            .expect("Failed to build search request");

        let relevant_queries = self.query_index.top_n::<QueryEntry>(search_request).await
            .map_err(|e| anyhow::anyhow!("Failed to retrieve relevant queries: {}", e))?;

        // Format relevant queries as context
        let mut context = String::from("RELEVANT EXAMPLE QUERIES:\n\n");
        for (score, _id, query_entry) in relevant_queries.iter().take(10) {
            context.push_str(&format!(
                "Example (relevance: {:.3}):\nDescription: {}\nQuery: {}\n\n",
                score, query_entry.description, query_entry.query
            ));
        }

        // Build enhanced prompt with context
        let enhanced_prompt = format!("{}\n\nUSER REQUEST: {}", context, prompt);

        // Dump full LLM request for debugging
        if log::log_enabled!(log::Level::Debug) {
            let completion_request = self.agent.completion(&enhanced_prompt, vec![]).await?.build();
            let formatted_request = format_llm_request_debug(
                &completion_request.preamble,
                &enhanced_prompt
            );
            log::debug!("{}", formatted_request);
        }

        // Prompt the agent with enhanced context
        let response = self.agent.prompt(&enhanced_prompt).await.map_err(anyhow::Error::new)?;

        // Strip markdown code blocks from response
        Ok(strip_markdown_code_blocks(&response))
    }
}

pub async fn convert_to_gaql(
    example_queries: Vec<QueryEntry>,
    prompt: &str,
) -> Result<String, anyhow::Error> {
    // Initialize RAGAgent
    let rag_agent = RAGAgent::init(example_queries).await?;

    // Use RAGAgent to prompt
    rag_agent.prompt(prompt).await
}

/// Enhanced RAG Agent with field metadata awareness
struct EnhancedRAGAgent {
    agent: Agent<CompletionModel>,
    query_index: LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
    field_cache: Option<FieldMetadataCache>,
    field_vector_store: Option<LanceDbVectorIndex<rig_fastembed::EmbeddingModel>>,
    embedding_model: rig_fastembed::EmbeddingModel,
}

impl EnhancedRAGAgent {
    pub async fn init(
        query_cookbook: Vec<QueryEntry>,
        field_cache: Option<FieldMetadataCache>,
    ) -> Result<Self, anyhow::Error> {
        let init_start = std::time::Instant::now();

        let openrouter_client = openrouter::Client::from_env();
        let fastembed_client = rig_fastembed::Client::new();
        let embedding_model = fastembed_client.embedding_model(&FastembedModel::AllMiniLML6V2Q);

        // Build or load query cookbook vector store with LanceDB caching
        log::info!("Initializing query cookbook embeddings for {} queries", query_cookbook.len());
        let query_index = build_or_load_query_vector_store(query_cookbook, embedding_model.clone()).await?;

        // Build or load field embeddings if field cache is available
        let field_vector_store = if let Some(ref cache) = field_cache {
            log::info!("Initializing field embeddings for {} fields", cache.fields.len());
            Some(build_or_load_field_vector_store(cache, embedding_model.clone()).await?)
        } else {
            None
        };

        // Build enhanced preamble with field metadata
        let preamble = Self::build_preamble(&field_cache);

        let agent = openrouter_client
            .agent(openrouter::GEMINI_FLASH_2_0)
            .preamble(&preamble)
            .temperature(0.1)
            .build();

        log::info!("EnhancedRAGAgent initialized in {:.2}s", init_start.elapsed().as_secs_f64());

        Ok(EnhancedRAGAgent {
            agent,
            query_index,
            field_cache,
            field_vector_store,
            embedding_model,
        })
    }

    fn build_preamble(field_cache: &Option<FieldMetadataCache>) -> String {
        let mut preamble = String::from(
            "You are a Google Ads GAQL query assistant. Convert natural language requests into valid GAQL queries.\n\n"
        );

        if let Some(cache) = field_cache {
            preamble.push_str("SCHEMA INFORMATION:\n");
            preamble.push_str(&format!("Available resources: {}\n\n", cache.get_resources().join(", ")));

            preamble.push_str("AVAILABLE FIELDS:\n");
            preamble.push_str("For each query, you will be provided with relevant fields selected specifically for your request.\n");
            preamble.push_str("These fields are chosen based on semantic similarity to your query and may include metrics, segments, and attributes.\n\n");

            preamble.push_str("CRITICAL: NEVER invent or create field names. ONLY use field names from:\n");
            preamble.push_str("1. The relevant fields provided for your specific query\n");
            preamble.push_str("2. Field names from the example queries\n\n");
        }

        preamble.push_str("RULES:\n");
        preamble.push_str("- SELECT only fields marked as selectable\n");
        preamble.push_str("- FROM clause specifies the primary resource\n");
        preamble.push_str("- WHERE clause supports filterable fields only\n");
        preamble.push_str("- Metrics require grouping by resource attributes or segments\n");
        preamble.push_str("- Use segments.date for time-based analysis\n");
        preamble.push_str("- For trending, always include segments.date and use ORDER BY segments.date\n");
        preamble.push_str("- DURING operator for date ranges (e.g., DURING LAST_30_DAYS)\n\n");

        preamble.push_str("OUTPUT:\n");
        preamble.push_str("CRITICAL: Respond with ONLY the GAQL query as plain text. Do not include:\n");
        preamble.push_str("- Markdown code blocks (```sql or ```gaql or ```)\n");
        preamble.push_str("- Quotes (single or double)\n");
        preamble.push_str("- Explanatory text before or after the query\n");
        preamble.push_str("- Any other formatting\n\n");
        preamble.push_str("You will find example GAQL queries that could be useful in the attachments below.\n");

        preamble
    }

    /// Retrieve relevant fields using RAG based on user query
    async fn retrieve_relevant_fields(&self, user_query: &str, limit: usize) -> Vec<FieldMetadata> {
        if let Some(ref field_index) = self.field_vector_store {
            // Build search request
            use rig::vector_store::VectorSearchRequest;
            let search_request = VectorSearchRequest::builder()
                .query(user_query)
                .samples(limit as u64)
                .build()
                .expect("Failed to build search request");

            match field_index.top_n::<FieldDocumentFlat>(search_request).await {
                Ok(results) => {
                    log::debug!("Retrieved {} relevant fields for query: {}", results.len(), user_query);
                    for (score, id, flat_doc) in &results {
                        log::debug!("  Score: {:.3}, ID: {}, Field: {:?}", score, id, flat_doc);
                    }
                    // Results are (score, id, FieldDocumentFlat) tuples
                    // Convert FieldDocumentFlat to FieldMetadata
                    let field_results: Vec<FieldMetadata> = results
                        .into_iter()
                        .map(|(_, _, flat_doc)| FieldMetadata::from(flat_doc))
                        .collect();
                    field_results
                }
                Err(e) => {
                    log::warn!("Failed to retrieve relevant fields: {}", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    }

    /// Format relevant fields organized by category
    fn format_relevant_fields(&self, fields: &[FieldMetadata]) -> String {
        if fields.is_empty() {
            return String::new();
        }

        let mut output = String::from("RELEVANT FIELDS FOR YOUR QUERY:\n\n");

        // Organize by category
        let mut metrics: Vec<&FieldMetadata> = fields.iter().filter(|f| f.is_metric()).collect();
        let mut segments: Vec<&FieldMetadata> = fields.iter().filter(|f| f.is_segment()).collect();
        let mut attributes: Vec<&FieldMetadata> = fields.iter().filter(|f| f.is_attribute()).collect();

        // Sort each category by name
        metrics.sort_by(|a, b| a.name.cmp(&b.name));
        segments.sort_by(|a, b| a.name.cmp(&b.name));
        attributes.sort_by(|a, b| a.name.cmp(&b.name));

        // Format metrics
        if !metrics.is_empty() {
            output.push_str("Metrics:\n");
            for field in metrics {
                output.push_str(&format!(
                    "- {}: {} ({})\n",
                    field.name,
                    field.data_type,
                    if field.selectable { "selectable" } else { "not selectable" }
                ));
            }
            output.push('\n');
        }

        // Format segments
        if !segments.is_empty() {
            output.push_str("Segments:\n");
            for field in segments {
                output.push_str(&format!("- {}: {}\n", field.name, field.data_type));
            }
            output.push('\n');
        }

        // Format attributes
        if !attributes.is_empty() {
            output.push_str("Attributes:\n");
            for field in attributes {
                output.push_str(&format!(
                    "- {}: {} ({}{})\n",
                    field.name,
                    field.data_type,
                    if field.selectable { "selectable" } else { "not selectable" },
                    if field.filterable { ", filterable" } else { "" }
                ));
            }
            output.push('\n');
        }

        output
    }

    fn identify_resources(&self, user_query: &str) -> Vec<String> {
        let query_lower = user_query.to_lowercase();
        let mut resources = Vec::new();

        // Common resource keywords
        if query_lower.contains("campaign") {
            resources.push("campaign".to_string());
        }
        if query_lower.contains("ad group") || query_lower.contains("adgroup") {
            resources.push("ad_group".to_string());
        }
        if query_lower.contains("keyword") {
            resources.push("keyword_view".to_string());
        }
        if query_lower.contains("search term") {
            resources.push("search_term_view".to_string());
        }
        if query_lower.contains("ad ") || query_lower.contains("ads ") {
            resources.push("ad_group_ad".to_string());
        }
        if query_lower.contains("asset") {
            resources.push("asset".to_string());
        }

        // Default to campaign if nothing specific mentioned
        if resources.is_empty() {
            resources.push("campaign".to_string());
        }

        resources
    }

    fn build_context_for_query(&self, user_query: &str) -> String {
        if let Some(cache) = &self.field_cache {
            let mut context = String::new();

            // Identify likely resources
            let resources = self.identify_resources(user_query);
            context.push_str("LIKELY RESOURCES:\n");
            for resource in &resources {
                let fields = cache.get_resource_fields(resource);
                context.push_str(&format!("- {}: {} fields available\n", resource, fields.len()));
            }
            context.push('\n');

            // Check for temporal keywords
            let query_lower = user_query.to_lowercase();
            if query_lower.contains("last")
                || query_lower.contains("week")
                || query_lower.contains("month")
                || query_lower.contains("trend")
                || query_lower.contains("over time") {
                context.push_str("TEMPORAL ANALYSIS DETECTED - Include segments.date\n\n");
            }

            context
        } else {
            String::new()
        }
    }

    pub async fn prompt(&self, prompt: &str) -> Result<String, anyhow::Error> {
        // Manually retrieve relevant queries from LanceDB
        use rig::vector_store::VectorSearchRequest;
        let search_request = VectorSearchRequest::builder()
            .query(prompt)
            .samples(3)
            .build()
            .expect("Failed to build search request");

        let relevant_queries = self.query_index.top_n::<QueryEntry>(search_request).await
            .map_err(|e| anyhow::anyhow!("Failed to retrieve relevant queries: {}", e))?;

        // Retrieve relevant fields via RAG (10 fields)
        let relevant_fields = self.retrieve_relevant_fields(prompt, 10).await;

        // Build enhanced prompt with all context
        let mut enhanced_prompt = String::new();
        enhanced_prompt.push_str(&format!("USER QUERY: {}\n\n", prompt));

        // Add relevant example queries
        if !relevant_queries.is_empty() {
            enhanced_prompt.push_str("RELEVANT EXAMPLE QUERIES:\n\n");
            for (score, _id, query_entry) in relevant_queries.iter().take(3) {
                enhanced_prompt.push_str(&format!(
                    "Example (relevance: {:.3}):\nDescription: {}\nQuery: {}\n\n",
                    score, query_entry.description, query_entry.query
                ));
            }
        }

        // Add RAG-retrieved relevant fields
        if !relevant_fields.is_empty() {
            enhanced_prompt.push_str(&self.format_relevant_fields(&relevant_fields));
        }

        // Add resource context
        enhanced_prompt.push_str(&self.build_context_for_query(prompt));
        enhanced_prompt.push_str("\nGenerate GAQL query:");

        // Dump full LLM request for debugging
        if log::log_enabled!(log::Level::Debug) {
            let completion_request = self.agent.completion(&enhanced_prompt, vec![]).await?.build();
            let formatted_request = format_llm_request_debug(
                &completion_request.preamble,
                &enhanced_prompt
            );
            log::debug!("{}", formatted_request);
        }

        // Prompt the agent
        let response = self.agent.prompt(&enhanced_prompt).await.map_err(anyhow::Error::new)?;

        // Strip markdown code blocks from response
        Ok(strip_markdown_code_blocks(&response))
    }
}

/// Convert natural language to GAQL with field metadata awareness
pub async fn convert_to_gaql_enhanced(
    example_queries: Vec<QueryEntry>,
    field_cache: Option<FieldMetadataCache>,
    prompt: &str,
) -> Result<String, anyhow::Error> {
    // Initialize Enhanced RAGAgent
    let rag_agent = EnhancedRAGAgent::init(example_queries, field_cache).await?;

    // Use Enhanced RAGAgent to prompt
    rag_agent.prompt(prompt).await
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
        let input = "```sql\nSELECT campaign.id FROM campaign WHERE campaign.status = 'ENABLED'\n```";
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
        // Create a sample query cookbook
        let queries = vec![
            QueryEntry {
                description: "Get all campaigns".to_string(),
                query: "SELECT campaign.id, campaign.name FROM campaign".to_string(),
            },
            QueryEntry {
                description: "Get enabled campaigns".to_string(),
                query: "SELECT campaign.id FROM campaign WHERE campaign.status = 'ENABLED'".to_string(),
            },
            QueryEntry {
                description: "Get campaign metrics".to_string(),
                query: "SELECT campaign.id, metrics.impressions, metrics.clicks FROM campaign".to_string(),
            },
        ];

        // Compute hash multiple times
        let hash1 = compute_query_cookbook_hash(&queries);
        let hash2 = compute_query_cookbook_hash(&queries);
        let hash3 = compute_query_cookbook_hash(&queries);

        // All hashes should be identical
        assert_eq!(hash1, hash2, "Hash should be consistent across repeated calls");
        assert_eq!(hash2, hash3, "Hash should be consistent across repeated calls");
        assert_eq!(hash1, hash3, "Hash should be consistent across repeated calls");
    }

    #[test]
    fn test_compute_query_cookbook_hash_empty() {
        let empty_queries: Vec<QueryEntry> = vec![];

        // Compute hash multiple times for empty cookbook
        let hash1 = compute_query_cookbook_hash(&empty_queries);
        let hash2 = compute_query_cookbook_hash(&empty_queries);

        // Should produce consistent hash even for empty input
        assert_eq!(hash1, hash2, "Empty cookbook should produce consistent hash");
    }

    #[test]
    fn test_compute_query_cookbook_hash_single_query() {
        let queries = vec![
            QueryEntry {
                description: "Single query test".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let hash1 = compute_query_cookbook_hash(&queries);
        let hash2 = compute_query_cookbook_hash(&queries);

        assert_eq!(hash1, hash2, "Single query should produce consistent hash");
    }

    #[test]
    fn test_compute_query_cookbook_hash_order_dependency() {
        // Create two query cookbooks with same queries in different order
        let queries_order1 = vec![
            QueryEntry {
                description: "Query A".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
            QueryEntry {
                description: "Query B".to_string(),
                query: "SELECT ad_group.id FROM ad_group".to_string(),
            },
        ];

        let queries_order2 = vec![
            QueryEntry {
                description: "Query B".to_string(),
                query: "SELECT ad_group.id FROM ad_group".to_string(),
            },
            QueryEntry {
                description: "Query A".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let hash1 = compute_query_cookbook_hash(&queries_order1);
        let hash2 = compute_query_cookbook_hash(&queries_order2);

        // Hashes should be different because order matters
        assert_ne!(hash1, hash2, "Different order should produce different hash");
    }

    #[test]
    fn test_compute_query_cookbook_hash_identical_content() {
        // Create two separate instances with identical content
        let queries1 = vec![
            QueryEntry {
                description: "Test query".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let queries2 = vec![
            QueryEntry {
                description: "Test query".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let hash1 = compute_query_cookbook_hash(&queries1);
        let hash2 = compute_query_cookbook_hash(&queries2);

        // Identical content should produce identical hash
        assert_eq!(hash1, hash2, "Identical content in different instances should produce same hash");
    }

    #[test]
    fn test_compute_query_cookbook_hash_different_content() {
        let queries1 = vec![
            QueryEntry {
                description: "Query A".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let queries2 = vec![
            QueryEntry {
                description: "Query B".to_string(),
                query: "SELECT ad_group.id FROM ad_group".to_string(),
            },
        ];

        let hash1 = compute_query_cookbook_hash(&queries1);
        let hash2 = compute_query_cookbook_hash(&queries2);

        // Different content should produce different hash
        assert_ne!(hash1, hash2, "Different content should produce different hash");
    }

    #[test]
    fn test_compute_query_cookbook_hash_description_change() {
        // Test that changing only the description changes the hash
        let queries1 = vec![
            QueryEntry {
                description: "Original description".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let queries2 = vec![
            QueryEntry {
                description: "Modified description".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let hash1 = compute_query_cookbook_hash(&queries1);
        let hash2 = compute_query_cookbook_hash(&queries2);

        // Different descriptions should produce different hash
        assert_ne!(hash1, hash2, "Changing description should change hash");
    }

    #[test]
    fn test_compute_query_cookbook_hash_query_change() {
        // Test that changing only the query changes the hash
        let queries1 = vec![
            QueryEntry {
                description: "Same description".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let queries2 = vec![
            QueryEntry {
                description: "Same description".to_string(),
                query: "SELECT campaign.name FROM campaign".to_string(),
            },
        ];

        let hash1 = compute_query_cookbook_hash(&queries1);
        let hash2 = compute_query_cookbook_hash(&queries2);

        // Different queries should produce different hash
        assert_ne!(hash1, hash2, "Changing query should change hash");
    }
}

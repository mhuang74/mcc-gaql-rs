use std::vec;

use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::{Completion, Prompt},
    embeddings::{EmbedError, EmbeddingsBuilder, TextEmbedder, embed::Embed},
    providers::openrouter::{self, completion::CompletionModel},
    vector_store::{in_memory_store::InMemoryVectorStore, VectorStoreIndex},
};
use rig_fastembed::FastembedModel;
use serde::{Deserialize, Serialize};

use crate::field_metadata::{FieldMetadata, FieldMetadataCache};
use crate::util::QueryEntry;

// use description field from QueryEntry for embedding
impl Embed for QueryEntry {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        embedder.embed(self.description.clone());
        Ok(())
    }
}

/// Document wrapper for field metadata to enable RAG-based field retrieval
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
struct FieldDocument {
    pub field: FieldMetadata,
    pub description: String,
}

impl FieldDocument {
    /// Create a new field document with synthetic description
    fn new(field: FieldMetadata) -> Self {
        let description = Self::generate_description(&field);
        Self { field, description }
    }

    /// Generate a synthetic description for better semantic matching
    fn generate_description(field: &FieldMetadata) -> String {
        let mut parts = Vec::new();

        // Field name with underscores replaced by spaces for better matching
        parts.push(field.name.replace('.', " ").replace('_', " "));

        // Category description
        let category_desc = match field.category.as_str() {
            "METRIC" => "performance metric",
            "SEGMENT" => "segmentation dimension",
            "ATTRIBUTE" => "descriptive attribute",
            "RESOURCE" => "resource identifier",
            _ => "field",
        };
        parts.push(category_desc.to_string());

        // Data type
        parts.push(format!("{} type", field.data_type.to_lowercase()));

        // Capabilities
        let mut capabilities = Vec::new();
        if field.selectable {
            capabilities.push("can be selected");
        }
        if field.filterable {
            capabilities.push("can be filtered");
        }
        if field.sortable {
            capabilities.push("can be sorted");
        }
        if field.metrics_compatible {
            capabilities.push("compatible with metrics");
        }
        if !capabilities.is_empty() {
            parts.push(capabilities.join(", "));
        }

        // Purpose inference based on common patterns
        let purpose = Self::infer_purpose(&field.name);
        if !purpose.is_empty() {
            parts.push(format!("used for {}", purpose));
        }

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
}

impl RAGAgent {
    pub async fn init(
        query_cookbook: Vec<QueryEntry>,
    ) -> Result<Self, anyhow::Error> {
        let openrouter_client = openrouter::Client::from_env();
        let fastembed_client = rig_fastembed::Client::new();
        let embedding_model = fastembed_client.embedding_model(&FastembedModel::AllMiniLML6V2Q);

        // Generate embeddings for the definitions of all the documents using the specified embedding model.
        let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
            .documents(query_cookbook)?
            .build()
            .await?;

        // Create vector store with the embeddings
        let vector_store = InMemoryVectorStore::from_documents(embeddings);

        // Create vector store index
        let index = vector_store.index(embedding_model);

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

                You will find example GAQL that could be useful in the attachments below.
            ")
            .dynamic_context(10, index)
            .temperature(0.1)
            .build();

        Ok(RAGAgent { agent })
    }

    pub async fn prompt(&self, prompt: &str) -> Result<String, anyhow::Error> {
        // HACK: dump full LLM prompt via CompletionRequest
        let completion_request = self.agent.completion(prompt, vec![]).await?.build();
        log::debug!(
            "LLM Request: preamble={:?}, chat_history={:?}",
            completion_request.preamble,
            completion_request.chat_history
        );

        // Prompt the agent
        self.agent.prompt(prompt).await.map_err(anyhow::Error::new)
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
    field_cache: Option<FieldMetadataCache>,
    field_vector_store: Option<InMemoryVectorStore<FieldDocument>>,
    embedding_model: rig_fastembed::EmbeddingModel,
}

impl EnhancedRAGAgent {
    pub async fn init(
        query_cookbook: Vec<QueryEntry>,
        field_cache: Option<FieldMetadataCache>,
    ) -> Result<Self, anyhow::Error> {
        let openrouter_client = openrouter::Client::from_env();
        let fastembed_client = rig_fastembed::Client::new();
        let embedding_model = fastembed_client.embedding_model(&FastembedModel::AllMiniLML6V2Q);

        // Generate embeddings for the query cookbook
        let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
            .documents(query_cookbook)?
            .build()
            .await?;

        // Create vector store with the embeddings
        let vector_store = InMemoryVectorStore::from_documents(embeddings);

        // Create vector store index
        let index = vector_store.index(embedding_model.clone());

        // Build field embeddings if field cache is available
        let field_vector_store = if let Some(ref cache) = field_cache {
            log::info!("Building field embeddings for {} fields", cache.fields.len());

            // Create FieldDocument for each field
            let field_docs: Vec<FieldDocument> = cache
                .fields
                .values()
                .map(|field| FieldDocument::new(field.clone()))
                .collect();

            log::debug!("Sample field descriptions:");
            for doc in field_docs.iter().take(3) {
                log::debug!("  {}: {}", doc.field.name, doc.description);
            }

            // Generate embeddings for all field documents
            let field_embeddings = EmbeddingsBuilder::new(embedding_model.clone())
                .documents(field_docs)?
                .build()
                .await?;

            // Create vector store for fields
            let field_vector_store = InMemoryVectorStore::from_documents(field_embeddings);

            log::info!("Field embeddings built successfully");
            Some(field_vector_store)
        } else {
            None
        };

        // Build enhanced preamble with field metadata
        let preamble = Self::build_preamble(&field_cache);

        let agent = openrouter_client
            .agent(openrouter::GEMINI_FLASH_2_0)
            .preamble(&preamble)
            .dynamic_context(10, index)
            .temperature(0.1)
            .build();

        Ok(EnhancedRAGAgent {
            agent,
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

            preamble.push_str("COMMON METRICS:\n");
            let common_metrics = ["impressions", "clicks", "cost_micros", "conversions", "ctr", "average_cpc", "conversions_value"];
            for metric in common_metrics {
                let field_name = format!("metrics.{}", metric);
                if let Some(field) = cache.get_field(&field_name) {
                    preamble.push_str(&format!("- {}: {} ({})\n",
                        field.name,
                        field.data_type,
                        if field.selectable { "selectable" } else { "not selectable" }
                    ));
                }
            }
            preamble.push('\n');

            preamble.push_str("COMMON SEGMENTS:\n");
            let common_segments = ["date", "week", "month", "quarter", "year", "device", "ad_network_type"];
            for segment in common_segments {
                let field_name = format!("segments.{}", segment);
                if let Some(field) = cache.get_field(&field_name) {
                    preamble.push_str(&format!("- {}: {}\n", field.name, field.data_type));
                }
            }
            preamble.push('\n');

            preamble.push_str("ADDITIONAL FIELDS:\n");
            preamble.push_str("For each query, you will be provided with additional relevant fields selected specifically for your request.\n");
            preamble.push_str("These fields are chosen based on semantic similarity to your query and may include specialized metrics, segments, and attributes.\n\n");

            preamble.push_str("CRITICAL: NEVER invent or create field names. ONLY use field names from:\n");
            preamble.push_str("1. The common fields listed above\n");
            preamble.push_str("2. The relevant fields provided for your specific query\n");
            preamble.push_str("3. Field names from the example queries below\n\n");
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
        if let Some(ref field_store) = self.field_vector_store {
            // Create index on-demand (clone needed as index consumes the store)
            let field_idx = field_store.clone().index(self.embedding_model.clone());

            // Build search request
            use rig::vector_store::VectorSearchRequest;
            let search_request = VectorSearchRequest::builder()
                .query(user_query)
                .samples(limit as u64)
                .build()
                .expect("Failed to build search request");

            match field_idx.top_n::<FieldDocument>(search_request).await {
                Ok(results) => {
                    log::debug!("Retrieved {} relevant fields for query: {}", results.len(), user_query);
                    // Results are (score, id, document) tuples
                    let field_results: Vec<FieldMetadata> = results
                        .into_iter()
                        .map(|(_, _, doc)| doc.field)
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
        // Retrieve relevant fields via RAG (20-30 fields)
        let relevant_fields = self.retrieve_relevant_fields(prompt, 30).await;

        // Build additional context with RAG-retrieved fields
        let mut enhanced_prompt = String::new();
        enhanced_prompt.push_str(&format!("USER QUERY: {}\n\n", prompt));

        // Add RAG-retrieved relevant fields
        if !relevant_fields.is_empty() {
            enhanced_prompt.push_str(&self.format_relevant_fields(&relevant_fields));
        }

        // Add resource context
        enhanced_prompt.push_str(&self.build_context_for_query(prompt));
        enhanced_prompt.push_str("\nGenerate GAQL query:");

        // HACK: dump full LLM prompt via CompletionRequest
        let completion_request = self.agent.completion(&enhanced_prompt, vec![]).await?.build();
        log::debug!(
            "LLM Request: preamble={:?}, chat_history={:?}",
            completion_request.preamble,
            completion_request.chat_history
        );

        // Prompt the agent
        self.agent.prompt(&enhanced_prompt).await.map_err(anyhow::Error::new)
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

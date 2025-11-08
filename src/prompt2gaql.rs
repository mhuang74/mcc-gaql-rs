use std::vec;

use rig::{
    agent::Agent,
    client::CompletionClient,
    completion::{Completion, Prompt},
    embeddings::{EmbedError, EmbeddingsBuilder, TextEmbedder, embed::Embed},
    providers::openrouter::{self, completion::CompletionModel},
    vector_store::in_memory_store::InMemoryVectorStore,
};
use rig_fastembed::FastembedModel;

use crate::field_metadata::FieldMetadataCache;
use crate::util::QueryEntry;

// use description field from QueryEntry for embedding
impl Embed for QueryEntry {
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

                CRITICAL: Respond with ONLY the GAQL query as plain text. Do not include:
                - Markdown code blocks (```sql or ``` or ```)
                - Quotes (single or double)
                - Explanatory text before or after the query
                - Any other formatting

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
        let index = vector_store.index(embedding_model);

        // Build enhanced preamble with field metadata
        let preamble = Self::build_preamble(&field_cache);

        let agent = openrouter_client
            .agent(openrouter::GEMINI_FLASH_2_0)
            .preamble(&preamble)
            .dynamic_context(10, index)
            .temperature(0.1)
            .build();

        Ok(EnhancedRAGAgent { agent, field_cache })
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
        preamble.push_str("- Markdown code blocks (```sql or ``` or ```)\n");
        preamble.push_str("- Quotes (single or double)\n");
        preamble.push_str("- Explanatory text before or after the query\n");
        preamble.push_str("- Any other formatting\n\n");
        preamble.push_str("You will find example GAQL queries that could be useful in the attachments below.\n");

        preamble
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

            context.push_str(&format!("USER QUERY: {}\n\n", user_query));

            // Identify likely resources
            let resources = self.identify_resources(user_query);
            context.push_str("LIKELY RESOURCES:\n");
            for resource in &resources {
                let fields = cache.get_resource_fields(resource);
                context.push_str(&format!("- {}: {} fields available\n", resource, fields.len()));

                // Show a few key fields for this resource
                let mut shown = 0;
                for field in fields {
                    if shown < 5 && field.is_attribute() && field.selectable {
                        context.push_str(&format!("  - {}\n", field.name));
                        shown += 1;
                    }
                }
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
            format!("USER QUERY: {}\n\n", user_query)
        }
    }

    pub async fn prompt(&self, prompt: &str) -> Result<String, anyhow::Error> {
        // Build additional context
        let enhanced_prompt = format!(
            "{}\n\n{}",
            self.build_context_for_query(prompt),
            "Generate GAQL query:"
        );

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

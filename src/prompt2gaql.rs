use std::vec;

use rig::{
    agent::Agent,
    completion::{Completion, Prompt},
    embeddings::{EmbedError, EmbeddingsBuilder, TextEmbedder, embed::Embed},
    providers::openai::{Client, CompletionModel, GPT_4O_MINI, TEXT_EMBEDDING_ADA_002},
    vector_store::in_memory_store::InMemoryVectorStore,
};

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
        openai_api_key: &str,
        query_cookbook: Vec<QueryEntry>,
    ) -> Result<Self, anyhow::Error> {
        let client = Client::new(openai_api_key);
        let embedding_model = client.embedding_model(TEXT_EMBEDDING_ADA_002);

        // Generate embeddings for the definitions of all the documents using the specified embedding model.
        let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
            .documents(query_cookbook)?
            .build()
            .await?;

        // Create vector store with the embeddings
        let vector_store = InMemoryVectorStore::from_documents(embeddings);

        // Create vector store index
        let index = vector_store.index(embedding_model);

        let agent = client.agent(GPT_4O_MINI)
            .preamble("
                You are a Google Ads GAQL query assistant here to assist the user to translate natural language query requests into valid GAQL. 
                Respond with GAQL query as plain text, without any formatting or code blocks.
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
            "LLM Preamble: {:?}, Prompt: {:?}",
            completion_request.preamble,
            completion_request.prompt_with_context()
        );

        // Prompt the agent
        self.agent.prompt(prompt).await.map_err(anyhow::Error::new)
    }
}

pub async fn convert_to_gaql(
    openai_api_key: &str,
    example_queries: Vec<QueryEntry>,
    prompt: &str,
) -> Result<String, anyhow::Error> {
    // Initialize RAGAgent
    let rag_agent = RAGAgent::init(openai_api_key, example_queries).await?;

    // Use RAGAgent to prompt
    rag_agent.prompt(prompt).await
}

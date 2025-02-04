use std::{env, vec};

use rig::{
    completion::{Completion, Prompt},
    embeddings::{embed::Embed, EmbedError, EmbeddingsBuilder, TextEmbedder},
    providers::openai::{Client, TEXT_EMBEDDING_ADA_002},
    vector_store::in_memory_store::InMemoryVectorStore,
};
use serde::Serialize;

// Data to be RAGged.
// A vector search needs to be performed on the `definitions` field, so we derive the `Embed` trait for `WordDefinition`
// and tag that field with `#[embed]`.
#[derive(Serialize, Clone, Debug, Eq, PartialEq, Default)]
struct ExampleQuery {
    id: String,
    description: String,
    query: String,
}

impl Embed for ExampleQuery {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        embedder.embed(self.description.clone());
        Ok(())
    }
}

pub async fn convert_to_gaql(prompt: &str) -> Result<String, anyhow::Error> {
    // Create OpenAI client
    let openai_api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
    let openai_client = Client::new(&openai_api_key);

    let embedding_model = openai_client.embedding_model(TEXT_EMBEDDING_ADA_002);

    // Generate embeddings for the definitions of all the documents using the specified embedding model.
    let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
        .documents(vec![
            ExampleQuery {
                id: "query0".to_string(),
                description: "Performance of Shopping Campaigns for past 30 days".to_string(),
                query: r#"
SELECT 
    customer.id, 
    customer.descriptive_name,
    campaign.id, 
    campaign.name,
    campaign.advertising_channel_type,
    campaign.bidding_strategy_type,
    campaign_budget.amount_micros,
    metrics.average_cpc,
    metrics.clicks, 
    metrics.cost_micros,
    customer.currency_code,
    metrics.conversions,
    metrics.cost_per_conversion,
    metrics.conversions_value
FROM campaign
WHERE
    campaign.advertising_channel_type IN ('SHOPPING') 
    AND campaign.status IN ('ENABLED') 
    AND segments.date DURING LAST_30_DAYS 
    AND metrics.cost_micros > 100000000
ORDER by metrics.cost_micros DESC
                "#.to_string(),
            },
            ExampleQuery {
                id: "query1".to_string(),
                description: "Performance of Performance Max Campaigns for past 30 days".to_string(),
                query: r#"
SELECT 
    campaign.id, 
    segments.date,
    metrics.impressions, 
    metrics.clicks, 
    metrics.cost_micros,
    customer.currency_code 
FROM campaign 
WHERE 
    segments.date DURING LAST_30_DAYS 
    AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX')
    AND metrics.impressions > 1 
ORDER BY 
    segments.date, campaign.id
                "#.to_string(),
            },
        ExampleQuery {
                id: "query2".to_string(),
                description: "Top Spending Smart Bidding Campaigns from past 7 days".to_string(),
                query: r#"
SELECT 
    customer.id, 
    customer.descriptive_name,
    customer.currency_code,
    campaign.id, 
    campaign.name,
    campaign.advertising_channel_type,
    campaign.bidding_strategy_type,
    campaign_budget.amount_micros,
    metrics.average_cpc,
    metrics.clicks, 
    metrics.cost_micros,
    metrics.conversions,
    metrics.cost_per_conversion,
    metrics.conversions_value
FROM campaign
WHERE
    campaign.bidding_strategy_type IN ('MAXIMIZE_CONVERSIONS', 'MAXIMIZE_CONVERSION_VALUE', 'TARGET_CPA', 'TARGET_ROAS') 
    AND campaign.status IN ('ENABLED') 
    AND segments.date DURING LAST_7_DAYS 
    AND metrics.cost_micros > 1000000000
ORDER by metrics.cost_micros DESC
LIMIT 25
                "#.to_string(),
            },
            ExampleQuery {
                id: "query3".to_string(),
                description: "Top Keywords from last week".to_string(),
                query: r#"
SELECT
    customer.id,
    customer.descriptive_name,
    campaign.id,
    campaign.name,
    campaign.advertising_channel_type,
    ad_group.id,
    ad_group.name,
    ad_group.type,
    ad_group_criterion.criterion_id,
    ad_group_criterion.keyword.text,
    metrics.impressions,
    metrics.clicks,
    metrics.cost_micros,
    customer.currency_code 
FROM keyword_view
WHERE
    segments.date DURING LAST_7_DAYS
    and metrics.clicks > 10000
ORDER BY
    metrics.clicks DESC
LIMIT 25
                "#.to_string(),
            },            
        ])?
        .build()
        .await?;

    // Create vector store with the embeddings
    let vector_store = InMemoryVectorStore::from_documents(embeddings);

    // Create vector store index
    let index = vector_store.index(embedding_model);

    // Create OpenAI Agent with RAG context
    let rag_agent = openai_client.agent("gpt-4o-mini")
        .preamble("
            You are a Google Ads GAQL query assistant here to assist the user to translate natural language query requests into valid GAQL. 
            Respond with GAQL query as plain text, without any formatting or code blocks.
            You will find example GAQL that could be useful in the attachments below.
        ")
        .dynamic_context(3, index)
        .temperature(0.1)
        .build();

    // HACK: dump full LLM prompt via CompletionRequest
    let completion_request = rag_agent.completion(prompt, vec![]).await?.build();
    log::debug!("LLM Preamble: {:?}, Prompt: {:?}", completion_request.preamble, completion_request.prompt_with_context());

    // Prompt the agent and print the response
    let response = rag_agent.prompt(prompt).await?;

    Ok(response)
}

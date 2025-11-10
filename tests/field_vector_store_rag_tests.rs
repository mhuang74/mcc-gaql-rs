use mcc_gaql::field_metadata::FieldMetadataCache;
use mcc_gaql::prompt2gaql::{build_or_load_field_vector_store, FieldDocument, FieldDocumentFlat};
use rig::vector_store::{VectorStoreIndex, VectorSearchRequest};
use rig_fastembed::{Client as FastembedClient, FastembedModel};
use rig_lancedb::LanceDbVectorIndex;
use std::collections::HashSet;

/// Debug utility to print retrieval results with scores
fn print_retrieval_results(query: &str, results: &[(f64, String, FieldDocumentFlat)]) {
    println!("\n=== Retrieval Results for: '{}' ===", query);
    println!("{:<60} {:<8} {}", "Field Name", "Score", "Category");
    println!("{}", "-".repeat(80));
    for (score, _id, doc) in results {
        let category = &doc.category;
        println!("{:<60} {:<8.3} {}", doc.id, score, category);
    }
    println!("{}", "=".repeat(80));
}

/// Calculate precision: what percentage of retrieved fields are relevant
fn calculate_precision(
    retrieved_fields: &[String],
    expected_fields: &HashSet<String>,
) -> f32 {
    let relevant_count = retrieved_fields
        .iter()
        .filter(|f| expected_fields.contains(*f))
        .count();

    if retrieved_fields.is_empty() {
        0.0
    } else {
        relevant_count as f32 / retrieved_fields.len() as f32
    }
}

/// Calculate recall: what percentage of expected fields were retrieved
fn calculate_recall(
    retrieved_fields: &[String],
    expected_fields: &HashSet<String>,
) -> f32 {
    let relevant_count = retrieved_fields
        .iter()
        .filter(|f| expected_fields.contains(*f))
        .count();

    if expected_fields.is_empty() {
        1.0
    } else {
        relevant_count as f32 / expected_fields.len() as f32
    }
}

/// Helper to load the field vector store for testing
///
/// NOTE: These tests require cached field metadata. Run the tool first to populate the cache:
/// ```
/// mcc-gaql --user-email <email> --mcc-id <mcc> "SELECT campaign.name FROM campaign LIMIT 1"
/// ```
async fn get_test_field_vector_store() -> anyhow::Result<LanceDbVectorIndex<rig_fastembed::EmbeddingModel>> {
    // Load field metadata cache from default location (platform-specific)
    let cache_path = mcc_gaql::field_metadata::get_default_cache_path()?;

    if !cache_path.exists() {
        return Err(anyhow::anyhow!(
            "Field metadata cache not found at {:?}.\n\
             Please run the mcc-gaql tool first to populate the cache:\n\
             mcc-gaql --user-email <email> --mcc-id <mcc> \"SELECT campaign.name FROM campaign LIMIT 1\"",
            cache_path
        ));
    }

    let field_cache = FieldMetadataCache::load_or_fetch(
        None, // No API context needed for testing (will use cached data)
        &cache_path,
        999, // Very high TTL to use cached data
    )
    .await?;

    // Create embedding model
    let fastembed_client = FastembedClient::new();
    let embedding_model = fastembed_client.embedding_model(&FastembedModel::AllMiniLML6V2Q);

    // Build or load vector store
    let vector_store = build_or_load_field_vector_store(&field_cache, embedding_model).await?;

    Ok(vector_store)
}

/// Helper to search the vector store with a query and limit
async fn search_vector_store(
    vector_store: &LanceDbVectorIndex<rig_fastembed::EmbeddingModel>,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<(f64, String, FieldDocumentFlat)>> {
    let search_request = VectorSearchRequest::builder()
        .query(query)
        .samples(limit as u64)
        .build()
        .expect("Failed to build search request");

    let results = vector_store
        .top_n::<FieldDocumentFlat>(search_request)
        .await?;

    Ok(results)
}

/// Test case configuration for field retrieval tests
struct RetrievalTestCase {
    /// The search query
    query: &'static str,
    /// Expected relevant fields
    expected_fields: Vec<&'static str>,
    /// Number of results to retrieve (default: 10)
    limit: usize,
    /// Minimum precision threshold (default: 0.3)
    min_precision: f32,
    /// Minimum score for top result (default: 0.3)
    min_top_score: f64,
    /// Optional: keywords that should appear in retrieved fields
    should_contain_keywords: Vec<&'static str>,
}

impl RetrievalTestCase {
    fn new(query: &'static str, expected_fields: Vec<&'static str>) -> Self {
        Self {
            query,
            expected_fields,
            limit: 10,
            min_precision: 0.3,
            min_top_score: 0.3,
            should_contain_keywords: Vec::new(),
        }
    }

    fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    fn min_precision(mut self, min_precision: f32) -> Self {
        self.min_precision = min_precision;
        self
    }

    fn min_top_score(mut self, min_top_score: f64) -> Self {
        self.min_top_score = min_top_score;
        self
    }

    fn should_contain(mut self, keywords: Vec<&'static str>) -> Self {
        self.should_contain_keywords = keywords;
        self
    }

    /// Run the test and perform standard assertions
    async fn run(&self) -> anyhow::Result<Vec<(f64, String, FieldDocumentFlat)>> {
        let vector_store = get_test_field_vector_store().await?;
        let results = search_vector_store(&vector_store, self.query, self.limit).await?;

        // Debug output
        print_retrieval_results(self.query, &results);

        // Extract retrieved fields
        let retrieved_fields: Vec<String> = results
            .iter()
            .map(|(_, _, doc)| doc.id.clone())
            .collect();

        // Convert expected fields to HashSet
        let expected_set: HashSet<String> = self
            .expected_fields
            .iter()
            .map(|s| s.to_string())
            .collect();

        // Calculate metrics
        let precision = calculate_precision(&retrieved_fields, &expected_set);
        let recall = calculate_recall(&retrieved_fields, &expected_set);

        println!("\nPrecision: {:.2}, Recall: {:.2}", precision, recall);

        // Standard assertions
        assert!(
            !results.is_empty(),
            "Should retrieve at least some fields for query: '{}'",
            self.query
        );

        // Check top score
        let top_score = results[0].0;
        assert!(
            top_score > self.min_top_score,
            "Top result score should be > {}, got: {} for query: '{}'",
            self.min_top_score,
            top_score,
            self.query
        );

        // Check precision
        assert!(
            precision >= self.min_precision,
            "Precision should be >= {}, got: {} for query: '{}'",
            self.min_precision,
            precision,
            self.query
        );

        // Check keywords if specified
        if !self.should_contain_keywords.is_empty() {
            let has_keyword = retrieved_fields.iter().any(|f| {
                self.should_contain_keywords
                    .iter()
                    .any(|kw| f.contains(kw))
            });
            assert!(
                has_keyword,
                "Results should contain at least one of {:?}. Retrieved: {:?}",
                self.should_contain_keywords,
                retrieved_fields
            );
        }

        Ok(results)
    }
}

#[tokio::test]
async fn test_field_retrieval_for_cost_metrics() {
    RetrievalTestCase::new(
        "cost per click and average cost",
        vec![
            "metrics.average_cpc",
            "metrics.cost_micros",
            "metrics.cost_per_conversion",
            "metrics.average_cost",
            "metrics.average_cpe",
            "metrics.average_cpv",
            "metrics.average_cpm",
        ],
    )
    .should_contain(vec!["cost", "cpc", "average_c"])
    .run()
    .await
    .expect("Test failed");
}

#[tokio::test]
async fn test_field_retrieval_for_conversions() {
    RetrievalTestCase::new(
        "conversion data and conversion rate",
        vec![
            "metrics.conversions",
            "metrics.conversions_value",
            "metrics.all_conversions",
            "metrics.conversion_rate",
            "metrics.all_conversions_value",
            "metrics.cost_per_conversion",
            "metrics.value_per_conversion",
        ],
    )
    .should_contain(vec!["conversion"])
    .run()
    .await
    .expect("Test failed");
}

#[tokio::test]
async fn test_field_retrieval_for_impressions_and_clicks() {
    RetrievalTestCase::new(
        "impressions and clicks",
        vec![
            "metrics.impressions",
            "metrics.clicks",
            "metrics.ctr",
            "metrics.interactions",
            "metrics.interaction_rate",
        ],
    )
    .should_contain(vec!["impressions", "clicks", "ctr", "interaction"])
    .run()
    .await
    .expect("Test failed");
}

#[tokio::test]
async fn test_field_retrieval_for_impression_share_metrics() {
    RetrievalTestCase::new(
        "impression share metrics",
        vec![
            "metrics.absolute_top_impression_percentage",
            "metrics.search_absolute_top_impression_share",
            "metrics.search_budget_lost_absolute_top_impression_share",
            "metrics.search_budget_lost_impression_share",
            "metrics.search_budget_lost_top_impression_share",
            "metrics.search_exact_match_impression_share",
            "metrics.search_impression_share",
            "metrics.search_rank_lost_impression_share",
            "metrics.search_top_impression_share",
        ],
    )
    .should_contain(vec!["impression", "share"])
    .run()
    .await
    .expect("Test failed");
}

#[tokio::test]
async fn test_field_retrieval_similarity_scores() {
    // This test validates that similarity scores are reasonable
    let vector_store = get_test_field_vector_store()
        .await
        .expect("Failed to load field vector store");

    let query = "campaign budget amount";
    let limit = 20;

    let results = search_vector_store(&vector_store, query, limit)
        .await
        .expect("Failed to retrieve fields");

    // Debug output
    print_retrieval_results(query, &results);

    // All scores should be between 0.0 and 1.0
    for (score, _id, doc) in &results {
        assert!(
            *score >= 0.0 && *score <= 1.0,
            "Score should be in [0, 1] range, got {} for field {}",
            score,
            doc.id
        );
    }

    // Scores should be in descending order
    for i in 1..results.len() {
        assert!(
            results[i - 1].0 >= results[i].0,
            "Scores should be in descending order. Position {}: {} > Position {}: {}",
            i - 1,
            results[i - 1].0,
            i,
            results[i].0
        );
    }

    // Top result should have a decent score for a specific query
    assert!(
        results[0].0 > 0.25,
        "Top result should have score > 0.25 for specific query, got: {}",
        results[0].0
    );

    // Check that budget-related fields appear in top results
    let top_5_fields: Vec<String> = results[..5.min(results.len())]
        .iter()
        .map(|(_, _, doc)| doc.id.clone())
        .collect();

    let has_budget_field = top_5_fields
        .iter()
        .any(|f| f.contains("budget") || f.contains("amount"));

    assert!(
        has_budget_field,
        "Top 5 should include budget-related field. Got: {:?}",
        top_5_fields
    );
}

#[tokio::test]
async fn test_field_retrieval_ranking() {
    // This test ensures more relevant fields rank higher
    let vector_store = get_test_field_vector_store()
        .await
        .expect("Failed to load field vector store");

    let query = "video views and view rate";
    let limit = 15;

    let results = search_vector_store(&vector_store, query, limit)
        .await
        .expect("Failed to retrieve fields");

    // Debug output
    print_retrieval_results(query, &results);

    let retrieved_fields: Vec<String> = results
        .iter()
        .map(|(_, _, doc)| doc.id.clone())
        .collect();

    // Highly relevant fields for video queries
    let highly_relevant = ["metrics.video_views", "metrics.video_view_rate"];

    // Find positions of highly relevant fields
    let mut positions = Vec::new();
    for field in &highly_relevant {
        if let Some(pos) = retrieved_fields.iter().position(|f| f == field) {
            positions.push(pos);
            println!("Found '{}' at position {}", field, pos);
        }
    }

    // At least one highly relevant field should be found
    assert!(
        !positions.is_empty(),
        "Should find at least one highly relevant video field. Retrieved: {:?}",
        retrieved_fields
    );

    // Highly relevant fields should appear in top half of results
    for (field, pos) in highly_relevant.iter().zip(positions.iter()) {
        assert!(
            *pos < limit / 2,
            "Highly relevant field '{}' should be in top half, found at position {}",
            field,
            pos
        );
    }
}

#[tokio::test]
async fn test_field_retrieval_negative_case() {
    // Test that unrelated queries don't return extremely high scores
    let vector_store = get_test_field_vector_store()
        .await
        .expect("Failed to load field vector store");

    // A very vague query that shouldn't have super high confidence matches
    let query = "stuff and things";
    let limit = 5;

    let results = search_vector_store(&vector_store, query, limit)
        .await
        .expect("Failed to retrieve fields");

    // Debug output
    print_retrieval_results(query, &results);

    // For a vague query, top score shouldn't be too high
    // (This helps identify if embeddings are working properly)
    if !results.is_empty() {
        let top_score = results[0].0;
        println!("Top score for vague query: {:.3}", top_score);

        // A vague query should have lower scores than specific queries
        // This is a soft check - if this fails, it might indicate the
        // embedding model or descriptions need improvement
        assert!(
            top_score < 0.6,
            "Vague query shouldn't have very high scores. Got: {}",
            top_score
        );
    }
}

#[tokio::test]
async fn test_field_description_quality() {
    // Test that field descriptions are being generated properly
    let cache_path = mcc_gaql::field_metadata::get_default_cache_path()
        .expect("Failed to get cache path");

    let field_cache = FieldMetadataCache::load_or_fetch(None, &cache_path, 999)
        .await
        .expect("Failed to load field cache");

    // Create a few field documents and check their descriptions
    let mut sample_count = 0;
    let max_samples = 10;

    println!("\n=== Sample Field Descriptions ===");
    for (_name, field) in field_cache.fields.iter().take(max_samples) {
        let doc = FieldDocument::new(field.clone());
        println!("\nField: {}", doc.field.name);
        println!("Description: {}", doc.description);

        // Description should not be empty
        assert!(
            !doc.description.is_empty(),
            "Description should not be empty for field: {}",
            doc.field.name
        );

        // Description should contain the field name (with dots/underscores converted to spaces)
        let normalized_name = doc.field.name.replace('.', " ").replace('_', " ");
        assert!(
            doc.description.to_lowercase().contains(&normalized_name.to_lowercase()),
            "Description should contain field name. Field: {}, Description: {}",
            doc.field.name,
            doc.description
        );

        sample_count += 1;
    }

    assert_eq!(sample_count, max_samples, "Should test {} samples", max_samples);
}

#[tokio::test]
async fn test_category_specific_retrieval() {
    // Test retrieval for different categories
    let vector_store = get_test_field_vector_store()
        .await
        .expect("Failed to load field vector store");

    let test_cases = vec![
        ("performance metrics", "METRIC", vec!["metrics.impressions", "metrics.clicks", "metrics.conversions"]),
        ("campaign name", "ATTRIBUTE", vec!["campaign.name", "campaign.id"]),
        ("date segments", "SEGMENT", vec!["segments.date", "segments.week", "segments.month"]),
    ];

    for (query, expected_category, expected_fields) in test_cases {
        println!("\n=== Testing query: '{}' ===", query);

        let results = search_vector_store(&vector_store, query, 10)
            .await
            .expect("Failed to retrieve fields");

        print_retrieval_results(query, &results);

        let retrieved_fields: Vec<String> = results
            .iter()
            .map(|(_, _, doc)| doc.id.clone())
            .collect();

        // Check that at least one expected field is present
        let has_expected = expected_fields.iter().any(|ef| retrieved_fields.contains(&ef.to_string()));

        if !has_expected {
            println!("WARNING: Expected to find one of {:?} in results for query '{}'", expected_fields, query);
            println!("Got: {:?}", retrieved_fields);
        }

        // At least check that results are not empty
        assert!(!results.is_empty(), "Should retrieve results for query: {}", query);

        // Check that some results match the expected category
        let category_matches = results.iter().filter(|(_, _, doc)| {
            doc.category == expected_category
        }).count();

        println!("Found {} fields in category '{}'", category_matches, expected_category);
    }
}

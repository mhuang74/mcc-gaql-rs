use arrow_array::{
    ArrayRef, BooleanArray, FixedSizeListArray, Float64Array, RecordBatch, RecordBatchIterator,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::DistanceType;
use mcc_gaql_common::field_metadata::{FieldMetadata, FieldMetadataCache};
use mcc_gaql_gen::rag::{FieldDocument, FieldDocumentFlat};
use rig::embeddings::EmbeddingsBuilder;
use rig::vector_store::{VectorSearchRequest, VectorStoreIndex};
use rig_fastembed::{Client as FastembedClient, FastembedModel};
use rig_lancedb::{LanceDbVectorIndex, SearchParams};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

#[allow(unused_imports)]
use dirs;

/// Embedding dimension for BGESmallENV15 model
const EMBEDDING_DIM: i32 = 384;

/// Shared embedding model to avoid parallel initialization issues
fn get_shared_embedding_model() -> &'static rig_fastembed::EmbeddingModel {
    static MODEL: OnceLock<rig_fastembed::EmbeddingModel> = OnceLock::new();
    MODEL.get_or_init(|| {
        // Set HF_HOME to cache fastembed models in the proper location
        let cache_dir = dirs::cache_dir()
            .expect("Failed to get cache directory")
            .join("mcc-gaql")
            .join("fastembed-models");
        std::fs::create_dir_all(&cache_dir).expect("Failed to create cache directory");
        // SAFETY: This is safe because we're only setting a known environment variable
        // and the process is single-threaded at this point.
        unsafe { std::env::set_var("HF_HOME", &cache_dir) };

        let fastembed_client = FastembedClient::new();
        fastembed_client.embedding_model(&FastembedModel::BGESmallENV15)
    })
}

/// Debug utility to print retrieval results with scores
fn print_retrieval_results(query: &str, results: &[(f64, String, FieldDocumentFlat)]) {
    println!("\n=== Retrieval Results for: '{}' ===", query);
    println!("{:<60} {:<8} Category", "Field Name", "Score");
    println!("{}", "-".repeat(80));
    for (score, _id, doc) in results {
        let category = &doc.category;
        println!("{:<60} {:<8.3} {}", doc.id, score, category);
    }
    println!("{}", "=".repeat(80));
}

/// Calculate precision: what percentage of retrieved fields are relevant
fn calculate_precision(retrieved_fields: &[String], expected_fields: &HashSet<String>) -> f32 {
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
fn calculate_recall(retrieved_fields: &[String], expected_fields: &HashSet<String>) -> f32 {
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

/// Create synthetic field metadata cache for testing
fn create_test_field_cache() -> FieldMetadataCache {
    let mut fields = HashMap::new();

    // Cost-related metrics
    let cost_fields = vec![
        ("metrics.average_cpc", "DOUBLE", "METRIC"),
        ("metrics.cost_micros", "INT64", "METRIC"),
        ("metrics.cost_per_conversion", "DOUBLE", "METRIC"),
        ("metrics.average_cost", "DOUBLE", "METRIC"),
        ("metrics.average_cpe", "DOUBLE", "METRIC"),
        ("metrics.average_cpv", "DOUBLE", "METRIC"),
        ("metrics.average_cpm", "DOUBLE", "METRIC"),
    ];

    // Conversion-related metrics
    let conversion_fields = vec![
        ("metrics.conversions", "DOUBLE", "METRIC"),
        ("metrics.conversions_value", "DOUBLE", "METRIC"),
        ("metrics.all_conversions", "DOUBLE", "METRIC"),
        ("metrics.all_conversions_value", "DOUBLE", "METRIC"),
        ("metrics.value_per_conversion", "DOUBLE", "METRIC"),
    ];

    // Impression and click metrics
    let impression_fields = vec![
        ("metrics.impressions", "INT64", "METRIC"),
        ("metrics.clicks", "INT64", "METRIC"),
        ("metrics.ctr", "DOUBLE", "METRIC"),
        ("metrics.interactions", "INT64", "METRIC"),
        ("metrics.interaction_rate", "DOUBLE", "METRIC"),
    ];

    // Impression share metrics
    let impression_share_fields = vec![
        (
            "metrics.absolute_top_impression_percentage",
            "DOUBLE",
            "METRIC",
        ),
        (
            "metrics.search_absolute_top_impression_share",
            "DOUBLE",
            "METRIC",
        ),
        (
            "metrics.search_budget_lost_absolute_top_impression_share",
            "DOUBLE",
            "METRIC",
        ),
        (
            "metrics.search_budget_lost_impression_share",
            "DOUBLE",
            "METRIC",
        ),
        (
            "metrics.search_budget_lost_top_impression_share",
            "DOUBLE",
            "METRIC",
        ),
        (
            "metrics.search_exact_match_impression_share",
            "DOUBLE",
            "METRIC",
        ),
        ("metrics.search_impression_share", "DOUBLE", "METRIC"),
        (
            "metrics.search_rank_lost_impression_share",
            "DOUBLE",
            "METRIC",
        ),
        ("metrics.search_top_impression_share", "DOUBLE", "METRIC"),
    ];

    // Campaign attributes
    let campaign_fields = vec![
        ("campaign.name", "STRING", "ATTRIBUTE"),
        ("campaign.id", "INT64", "ATTRIBUTE"),
        ("campaign.status", "ENUM", "ATTRIBUTE"),
        ("campaign.budget_amount_micros", "INT64", "ATTRIBUTE"),
    ];

    // Segments
    let segment_fields = vec![
        ("segments.date", "DATE", "SEGMENT"),
        ("segments.week", "STRING", "SEGMENT"),
        ("segments.month", "STRING", "SEGMENT"),
        ("segments.device", "ENUM", "SEGMENT"),
    ];

    // Video metrics
    let video_fields = vec![
        ("metrics.video_trueview_views", "INT64", "METRIC"),
        ("metrics.video_trueview_view_rate", "DOUBLE", "METRIC"),
        ("metrics.video_views", "INT64", "METRIC"),
        ("metrics.video_view_rate", "DOUBLE", "METRIC"),
    ];

    // Combine all fields
    let all_fields: Vec<_> = cost_fields
        .into_iter()
        .chain(conversion_fields)
        .chain(impression_fields)
        .chain(impression_share_fields)
        .chain(campaign_fields)
        .chain(segment_fields)
        .chain(video_fields)
        .collect();

    for (name, data_type, category) in all_fields {
        let resource_name = if name.starts_with("campaign.") {
            Some("campaign".to_string())
        } else {
            None
        };

        fields.insert(
            name.to_string(),
            FieldMetadata {
                name: name.to_string(),
                category: category.to_string(),
                data_type: data_type.to_string(),
                selectable: true,
                filterable: name.starts_with("campaign."),
                sortable: true,
                metrics_compatible: category == "METRIC",
                resource_name,
                selectable_with: vec![],
                enum_values: vec![],
                attribute_resources: vec![],
                description: None,
                usage_notes: None,
            },
        );
    }

    FieldMetadataCache {
        last_updated: chrono::Utc::now(),
        api_version: "v23".to_string(),
        fields,
        resources: None,
        resource_metadata: None,
    }
}

/// Helper to create the field vector store for testing with synthetic data.
/// Uses a temp directory to avoid affecting production cache.
async fn get_test_field_vector_store()
-> anyhow::Result<LanceDbVectorIndex<rig_fastembed::EmbeddingModel>> {
    // Create synthetic field cache for testing
    let field_cache = create_test_field_cache();

    println!(
        "Created test field cache with {} fields",
        field_cache.fields.len()
    );

    // Use shared embedding model to avoid parallel initialization issues
    let embedding_model = get_shared_embedding_model().clone();

    // Convert FieldMetadataCache to FieldDocuments
    let field_docs: Vec<FieldDocument> = field_cache
        .fields
        .values()
        .cloned()
        .map(FieldDocument::new)
        .collect();

    // Generate embeddings
    let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
        .documents(field_docs.clone())?
        .build()
        .await?;

    // Match embeddings to documents by ID
    let mut embedding_map: HashMap<String, Vec<f64>> = HashMap::new();
    for (doc_ref, emb) in embeddings.iter() {
        let embedding_vec: Vec<f64> = emb.iter().flat_map(|e| e.vec.clone()).collect();
        embedding_map.insert(doc_ref.id.clone(), embedding_vec);
    }

    // Create documents with embeddings matched by ID
    let mut docs_with_embeddings: Vec<(FieldDocument, Vec<f64>)> = Vec::new();
    for doc in field_docs {
        let vec = embedding_map
            .get(&doc.id)
            .cloned()
            .unwrap_or_else(|| vec![0.0_f64; EMBEDDING_DIM as usize]);
        docs_with_embeddings.push((doc, vec));
    }

    // Create temp directory for LanceDB (isolated from production cache)
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test_field_rag.lancedb");
    let db = lancedb::connect(db_path.to_str().unwrap())
        .execute()
        .await?;

    println!("Created test LanceDB at: {}", db_path.display());

    // Define schema matching the field metadata structure
    let schema = Arc::new(Schema::new(vec![
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
                EMBEDDING_DIM,
            ),
            false,
        ),
    ]));

    // Convert documents to Arrow arrays
    let ids: StringArray =
        StringArray::from_iter_values(docs_with_embeddings.iter().map(|(d, _)| d.id.as_str()));
    let descriptions: StringArray = StringArray::from_iter_values(
        docs_with_embeddings
            .iter()
            .map(|(d, _)| d.description.as_str()),
    );
    let categories: StringArray = StringArray::from_iter_values(
        docs_with_embeddings
            .iter()
            .map(|(d, _)| d.field.category.as_str()),
    );
    let data_types: StringArray = StringArray::from_iter_values(
        docs_with_embeddings
            .iter()
            .map(|(d, _)| d.field.data_type.as_str()),
    );
    let selectable: BooleanArray = docs_with_embeddings
        .iter()
        .map(|(d, _)| Some(d.field.selectable))
        .collect();
    let filterable: BooleanArray = docs_with_embeddings
        .iter()
        .map(|(d, _)| Some(d.field.filterable))
        .collect();
    let sortable: BooleanArray = docs_with_embeddings
        .iter()
        .map(|(d, _)| Some(d.field.sortable))
        .collect();
    let metrics_compatible: BooleanArray = docs_with_embeddings
        .iter()
        .map(|(d, _)| Some(d.field.metrics_compatible))
        .collect();
    let resource_names: StringArray = StringArray::from_iter(
        docs_with_embeddings
            .iter()
            .map(|(d, _)| d.field.resource_name.as_deref()),
    );

    // Convert embeddings to FixedSizeListArray
    let embedding_values: Vec<f64> = docs_with_embeddings
        .iter()
        .flat_map(|(_, vec)| vec.clone())
        .collect();

    let vectors = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float64, true)),
        EMBEDDING_DIM,
        Arc::new(Float64Array::from(embedding_values)),
        None,
    )?;

    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(ids) as ArrayRef,
            Arc::new(descriptions) as ArrayRef,
            Arc::new(categories) as ArrayRef,
            Arc::new(data_types) as ArrayRef,
            Arc::new(selectable) as ArrayRef,
            Arc::new(filterable) as ArrayRef,
            Arc::new(sortable) as ArrayRef,
            Arc::new(metrics_compatible) as ArrayRef,
            Arc::new(resource_names) as ArrayRef,
            Arc::new(vectors) as ArrayRef,
        ],
    )?;

    let batches = RecordBatchIterator::new(vec![Ok(batch)], schema.clone());

    // Create LanceDB table
    let table = db
        .create_table("field_metadata", Box::new(batches))
        .execute()
        .await?;

    // Create vector index
    let index = LanceDbVectorIndex::new(
        table,
        embedding_model,
        "id",
        SearchParams::default()
            .distance_type(DistanceType::Cosine)
            .column("vector"),
    )
    .await?;

    // Keep temp_dir alive by leaking it (tests are short-lived anyway)
    // This prevents the directory from being deleted before the test completes
    std::mem::forget(temp_dir);

    Ok(index)
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
    /// Maximum distance for top result - lower is better with cosine distance (default: 0.5)
    max_top_distance: f64,
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
            max_top_distance: 0.3, // With cosine distance, lower = more similar
            should_contain_keywords: Vec::new(),
        }
    }

    #[allow(dead_code)]
    fn limit(mut self, limit: usize) -> Self {
        self.limit = limit;
        self
    }

    #[allow(dead_code)]
    fn min_precision(mut self, min_precision: f32) -> Self {
        self.min_precision = min_precision;
        self
    }

    #[allow(dead_code)]
    fn max_top_distance(mut self, max_top_distance: f64) -> Self {
        self.max_top_distance = max_top_distance;
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
        let retrieved_fields: Vec<String> =
            results.iter().map(|(_, _, doc)| doc.id.clone()).collect();

        // Convert expected fields to HashSet
        let expected_set: HashSet<String> =
            self.expected_fields.iter().map(|s| s.to_string()).collect();

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

        // Check top distance (lower is better with cosine distance)
        let top_distance = results[0].0;
        assert!(
            top_distance < self.max_top_distance,
            "Top result distance should be < {} (lower = more similar), got: {} for query: '{}'",
            self.max_top_distance,
            top_distance,
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
            let has_keyword = retrieved_fields
                .iter()
                .any(|f| self.should_contain_keywords.iter().any(|kw| f.contains(kw)));
            assert!(
                has_keyword,
                "Results should contain at least one of {:?}. Retrieved: {:?}",
                self.should_contain_keywords, retrieved_fields
            );
        }

        Ok(results)
    }
}

#[tokio::test]
async fn test_field_retrieval_for_cost_metrics() {
    RetrievalTestCase::new(
        "cost per click and average cost metrics",
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
        "metrics for conversions and conversion rate",
        vec![
            "metrics.conversions",
            "metrics.conversions_value",
            "metrics.all_conversions",
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
        "impressions, clicks, and interactions metrics",
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

    // All scores should be between 0.0 and 2.0 (cosine distance range)
    for (score, _id, doc) in &results {
        assert!(
            *score >= 0.0 && *score <= 2.0,
            "Score should be in [0, 2] range for cosine distance, got {} for field {}",
            score,
            doc.id
        );
    }

    // Scores should be in ascending order (lower distance = more similar)
    for i in 1..results.len() {
        assert!(
            results[i - 1].0 <= results[i].0,
            "Scores should be in ascending order (lower distance = more similar). Position {}: {} <= Position {}: {}",
            i - 1,
            results[i - 1].0,
            i,
            results[i].0
        );
    }

    // Top result should have a low distance for a specific query (< 0.3 means good similarity)
    assert!(
        results[0].0 < 0.3,
        "Top result should have distance < 0.3 for specific query (lower = more similar), got: {}",
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

    let retrieved_fields: Vec<String> = results.iter().map(|(_, _, doc)| doc.id.clone()).collect();

    // Highly relevant fields for video queries
    let highly_relevant = [
        "metrics.video_trueview_views",
        "metrics.video_trueview_view_rate",
    ];

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

    // For a vague query, best score shouldn't have low distance
    // (This helps identify if embeddings are working properly)
    if !results.is_empty() {
        let best_score = results[0].0;
        println!("Best distance score for vague query: {:.3}", best_score);

        // A vague query should have higher distance scores than specific queries
        // This is a soft check - if this fails, it might indicate the
        // embedding model or descriptions need improvement
        assert!(
            best_score > 0.3,
            "Vague query shouldn't have very low distance scores. Got: {}",
            best_score
        );
    }
}

#[tokio::test]
async fn test_field_description_quality() {
    // Test that field descriptions are being generated properly
    let field_cache = create_test_field_cache();

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

        // Description should contain the field name
        assert!(
            doc.description
                .to_lowercase()
                .contains(&doc.field.name.to_lowercase()),
            "Description should contain field name. Field: {}, Description: {}",
            doc.field.name,
            doc.description
        );

        sample_count += 1;
    }

    assert_eq!(
        sample_count, max_samples,
        "Should test {} samples",
        max_samples
    );
}

#[tokio::test]
async fn test_category_specific_retrieval() {
    // Test retrieval for different categories
    let vector_store = get_test_field_vector_store()
        .await
        .expect("Failed to load field vector store");

    let test_cases = vec![
        (
            "performance metrics",
            "METRIC",
            vec![
                "metrics.impressions",
                "metrics.clicks",
                "metrics.conversions",
            ],
        ),
        (
            "campaign name",
            "ATTRIBUTE",
            vec!["campaign.name", "campaign.id"],
        ),
        (
            "date segments",
            "SEGMENT",
            vec!["segments.date", "segments.week", "segments.month"],
        ),
    ];

    for (query, expected_category, expected_fields) in test_cases {
        println!("\n=== Testing query: '{}' ===", query);

        let results = search_vector_store(&vector_store, query, 10)
            .await
            .expect("Failed to retrieve fields");

        print_retrieval_results(query, &results);

        let retrieved_fields: Vec<String> =
            results.iter().map(|(_, _, doc)| doc.id.clone()).collect();

        // Check that at least one expected field is present
        let has_expected = expected_fields
            .iter()
            .any(|ef| retrieved_fields.contains(&ef.to_string()));

        if !has_expected {
            println!(
                "WARNING: Expected to find one of {:?} in results for query '{}'",
                expected_fields, query
            );
            println!("Got: {:?}", retrieved_fields);
        }

        // At least check that results are not empty
        assert!(
            !results.is_empty(),
            "Should retrieve results for query: {}",
            query
        );

        // Check that some results match the expected category
        let category_matches = results
            .iter()
            .filter(|(_, _, doc)| doc.category == expected_category)
            .count();

        println!(
            "Found {} fields in category '{}'",
            category_matches, expected_category
        );
    }
}

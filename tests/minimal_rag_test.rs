/// Minimal RAG test to verify the rig + rig-lancedb stack works correctly
/// Uses the actual field metadata structure and queries from the production system
use anyhow::Result;
use arrow_array::{
    ArrayRef, BooleanArray, FixedSizeListArray, Float64Array, RecordBatch, RecordBatchIterator,
    StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::DistanceType;
use rig::embeddings::{EmbedError, EmbeddingsBuilder, TextEmbedder, embed::Embed};
use rig::vector_store::{VectorSearchRequest, VectorStoreIndex};
use rig_fastembed::{Client as FastembedClient, FastembedModel};
use rig_lancedb::{LanceDbVectorIndex, SearchParams};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// Document structure matching the real field metadata schema
#[derive(Debug, Serialize, Deserialize, Clone)]
struct FieldDocument {
    id: String,
    description: String,
    category: String,
    data_type: String,
    selectable: bool,
    filterable: bool,
    sortable: bool,
    metrics_compatible: bool,
    resource_name: Option<String>,
    #[serde(skip)]
    vector: Vec<f64>,
}

// Implement Embed trait following the pattern from the production code
impl Embed for FieldDocument {
    fn embed(&self, embedder: &mut TextEmbedder) -> Result<(), EmbedError> {
        embedder.embed(self.description.clone());
        Ok(())
    }
}

const EMBEDDING_DIM: i32 = 768; // BGE-Base-EN-v1.5 dimension

#[tokio::test]
async fn test_minimal_rag_loop_with_field_metadata() -> Result<()> {
    println!("\n=== Minimal RAG Test with Field Metadata ===\n");

    // Step 1: Create test documents using real Google Ads field metadata structure
    let documents = vec![
        FieldDocument {
            id: "metrics.impressions".to_string(),
            description: "metrics impressions, used for tracking ad views".to_string(),
            category: "METRIC".to_string(),
            data_type: "INT64".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.clicks".to_string(),
            description: "metrics clicks, used for tracking user clicks".to_string(),
            category: "METRIC".to_string(),
            data_type: "INT64".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.ctr".to_string(),
            description: "metrics ctr, used for tracking user clicks".to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.average_cpc".to_string(),
            description: "metrics average cpc, used for tracking advertising costs".to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.cost_micros".to_string(),
            description: "metrics cost micros, used for tracking advertising costs".to_string(),
            category: "METRIC".to_string(),
            data_type: "INT64".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.conversions".to_string(),
            description: "metrics conversions, used for tracking conversions and sales".to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.conversion_rate".to_string(),
            description: "metrics conversion rate, used for tracking conversions and sales"
                .to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "campaign.name".to_string(),
            description: "campaign name".to_string(),
            category: "ATTRIBUTE".to_string(),
            data_type: "STRING".to_string(),
            selectable: true,
            filterable: true,
            sortable: true,
            metrics_compatible: true,
            resource_name: Some("campaign.resource_name".to_string()),
            vector: vec![],
        },
        FieldDocument {
            id: "campaign.status".to_string(),
            description: "campaign status".to_string(),
            category: "ATTRIBUTE".to_string(),
            data_type: "ENUM".to_string(),
            selectable: true,
            filterable: true,
            sortable: true,
            metrics_compatible: true,
            resource_name: Some("campaign.resource_name".to_string()),
            vector: vec![],
        },
        FieldDocument {
            id: "campaign.budget_amount_micros".to_string(),
            description: "campaign budget amount micros, used for tracking advertising costs"
                .to_string(),
            category: "ATTRIBUTE".to_string(),
            data_type: "INT64".to_string(),
            selectable: true,
            filterable: true,
            sortable: true,
            metrics_compatible: true,
            resource_name: Some("campaign.resource_name".to_string()),
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.search_impression_share".to_string(),
            description: "metrics search impression share, used for tracking ad views".to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.search_absolute_top_impression_share".to_string(),
            description: "metrics search absolute top impression share, used for tracking ad views"
                .to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.search_top_impression_share".to_string(),
            description: "metrics search top impression share, used for tracking ad views"
                .to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.search_budget_lost_impression_share".to_string(),
            description: "metrics search budget lost impression share, used for tracking ad views"
                .to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.search_rank_lost_impression_share".to_string(),
            description: "metrics search rank lost impression share, used for tracking ad views"
                .to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
        FieldDocument {
            id: "metrics.absolute_top_impression_percentage".to_string(),
            description: "metrics absolute top impression percentage, used for tracking ad views"
                .to_string(),
            category: "METRIC".to_string(),
            data_type: "DOUBLE".to_string(),
            selectable: true,
            filterable: false,
            sortable: true,
            metrics_compatible: false,
            resource_name: None,
            vector: vec![],
        },
    ];

    println!(
        "Created {} test documents with field metadata",
        documents.len()
    );

    // Step 2: Create embedding model using rig_fastembed
    let fastembed_client = FastembedClient::new();
    let embedding_model = fastembed_client.embedding_model(&FastembedModel::BGEBaseENV15);

    println!("Initialized embedding model: BGE-Base-EN-v1.5");

    // Step 3: Generate embeddings using EmbeddingsBuilder
    let embeddings = EmbeddingsBuilder::new(embedding_model.clone())
        .documents(documents.clone())?
        .build()
        .await?;

    println!("Generated embeddings for {} documents", embeddings.len());

    // Step 4: Match embeddings to documents by ID (order is not preserved!)
    use std::collections::HashMap;
    let mut embedding_map: HashMap<String, Vec<f64>> = HashMap::new();

    for (doc_ref, emb) in embeddings.iter() {
        let embedding_vec: Vec<f64> = emb.iter().flat_map(|e| e.vec.clone()).collect();
        embedding_map.insert(doc_ref.id.clone(), embedding_vec);
    }

    // Step 5: Create documents with embeddings matched by ID
    let mut docs_with_embeddings = documents.clone();
    for doc in docs_with_embeddings.iter_mut() {
        doc.vector = embedding_map
            .get(&doc.id)
            .unwrap_or_else(|| panic!("Missing embedding for document {}", doc.id))
            .clone();
    }

    println!("Associated embeddings with documents (matched by ID)");

    // Step 6: Set up LanceDB
    let temp_dir = tempfile::tempdir()?;
    let db_path = temp_dir.path().join("test_rag.lancedb");
    let db = lancedb::connect(db_path.to_str().unwrap())
        .execute()
        .await?;

    println!("Created LanceDB at: {}", db_path.display());

    // Step 7: Define schema matching the field metadata structure
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

    // Step 8: Convert documents to Arrow RecordBatch
    let ids: StringArray =
        StringArray::from_iter_values(docs_with_embeddings.iter().map(|d| d.id.as_str()));
    let descriptions: StringArray =
        StringArray::from_iter_values(docs_with_embeddings.iter().map(|d| d.description.as_str()));
    let categories: StringArray =
        StringArray::from_iter_values(docs_with_embeddings.iter().map(|d| d.category.as_str()));
    let data_types: StringArray =
        StringArray::from_iter_values(docs_with_embeddings.iter().map(|d| d.data_type.as_str()));
    let selectable: BooleanArray = docs_with_embeddings
        .iter()
        .map(|d| Some(d.selectable))
        .collect();
    let filterable: BooleanArray = docs_with_embeddings
        .iter()
        .map(|d| Some(d.filterable))
        .collect();
    let sortable: BooleanArray = docs_with_embeddings
        .iter()
        .map(|d| Some(d.sortable))
        .collect();
    let metrics_compatible: BooleanArray = docs_with_embeddings
        .iter()
        .map(|d| Some(d.metrics_compatible))
        .collect();
    let resource_names: StringArray = StringArray::from_iter(
        docs_with_embeddings
            .iter()
            .map(|d| d.resource_name.as_deref()),
    );

    // Convert embeddings to FixedSizeListArray
    let embedding_values: Vec<f64> = docs_with_embeddings
        .iter()
        .flat_map(|d| d.vector.clone())
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

    println!(
        "Created RecordBatch with {} rows",
        docs_with_embeddings.len()
    );

    // Step 9: Create LanceDB table
    let table = db
        .create_table("field_metadata", Box::new(batches))
        .execute()
        .await?;

    println!("Created LanceDB table: field_metadata");

    // Step 10: Create vector index with CORRECT configuration
    let index = LanceDbVectorIndex::new(
        table,
        embedding_model,
        "id", // CORRECT: This is the ID field, not the vector field!
        SearchParams::default()
            .distance_type(DistanceType::Cosine)
            .column("vector"), // CORRECT: Specify which column contains vectors
    )
    .await?;

    println!("Created LanceDbVectorIndex with:");
    println!("  - id_field: 'id'");
    println!("  - vector_column: 'vector'");
    println!("  - distance_type: Cosine");

    // Step 11: Perform vector searches with realistic Google Ads queries
    let test_queries = vec![
        (
            "impressions and clicks",
            vec!["metrics.impressions", "metrics.clicks", "metrics.ctr"],
        ),
        (
            "cost per click and average cost",
            vec!["metrics.average_cpc", "metrics.cost_micros"],
        ),
        (
            "conversion data and conversion rate",
            vec!["metrics.conversions", "metrics.conversion_rate"],
        ),
        (
            "campaign budget amount",
            vec!["campaign.budget_amount_micros"],
        ),
        (
            "impression share metrics",
            vec![
                "metrics.search_impression_share",
                "metrics.search_absolute_top_impression_share",
                "metrics.search_top_impression_share",
                "metrics.search_budget_lost_impression_share",
                "metrics.search_rank_lost_impression_share",
                "metrics.absolute_top_impression_percentage",
            ],
        ),
    ];

    println!("\n=== Running Search Queries ===\n");

    for (query, expected_ids) in test_queries {
        println!("Query: '{}'", query);
        println!("Expected IDs: {:?}", expected_ids);

        let search_request = VectorSearchRequest::builder()
            .query(query)
            .samples(5) // Get top 5 results
            .build()
            .expect("Failed to build search request");

        let results = index.top_n::<FieldDocument>(search_request).await?;

        println!("\nResults:");
        for (i, (distance, id, doc)) in results.iter().enumerate() {
            println!(
                "  {}. [distance={:.3}] {} - {} [{}]",
                i + 1,
                distance,
                id,
                doc.description,
                doc.category
            );
        }

        // Verify results
        assert!(
            !results.is_empty(),
            "Query '{}' should return results",
            query
        );

        // Check that distances are in ascending order (lower = more similar)
        for i in 1..results.len() {
            assert!(
                results[i - 1].0 <= results[i].0,
                "Results should be ordered by ascending distance (lower = more similar)"
            );
        }

        // Check that top result has reasonable distance (< 0.7 for acceptable match)
        let top_distance = results[0].0;
        assert!(
            top_distance < 0.7,
            "Top result should have distance < 0.7 (acceptable match), got: {}",
            top_distance
        );

        // Check that at least one expected ID is in top results
        let result_ids: Vec<&str> = results.iter().map(|(_, id, _)| id.as_str()).collect();
        let has_expected = expected_ids.iter().any(|&exp| result_ids.contains(&exp));
        assert!(
            has_expected,
            "At least one expected ID {:?} should be in results {:?} for query '{}'",
            expected_ids, result_ids, query
        );

        println!("✓ Query '{}' passed validation\n", query);
    }

    println!("=== All RAG tests passed! ===\n");

    Ok(())
}

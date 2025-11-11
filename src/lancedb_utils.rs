use anyhow::Result;
use arrow_array::{
    ArrayRef, BooleanArray, FixedSizeListArray, Float64Array, RecordBatch, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::{Connection, Table, connect};
use rig::embeddings::Embedding;
use std::path::PathBuf;
use std::sync::Arc;

use crate::prompt2gaql::FieldDocument;
use crate::util::QueryEntry;

/// Embedding dimension for BGEBaseENV15 model
const EMBEDDING_DIM: i32 = 768;

/// Schema version for tracking schema evolution
const SCHEMA_VERSION: u8 = 1;

/// Get the LanceDB database path
pub fn get_lancedb_path() -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get cache directory"))?
        .join("mcc-gaql")
        .join("lancedb");

    std::fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir)
}

/// Get the hash file path for a given cache type
pub fn get_hash_path(cache_type: &str) -> Result<PathBuf> {
    let cache_dir = dirs::cache_dir()
        .ok_or_else(|| anyhow::anyhow!("Failed to get cache directory"))?
        .join("mcc-gaql");

    std::fs::create_dir_all(&cache_dir)?;
    Ok(cache_dir.join(format!("{}.hash", cache_type)))
}

/// Save hash to file with schema version
pub fn save_hash(cache_type: &str, hash: u64) -> Result<()> {
    let hash_path = get_hash_path(cache_type)?;
    let content = format!("v{}\n{}", SCHEMA_VERSION, hash);
    std::fs::write(&hash_path, content)?;
    log::debug!("Saved hash {} to {:?}", hash, hash_path);
    Ok(())
}

/// Load hash from file and validate schema version
pub fn load_hash(cache_type: &str) -> Result<Option<u64>> {
    let hash_path = get_hash_path(cache_type)?;

    if !hash_path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(&hash_path)?;
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return Ok(None);
    }

    // Check schema version
    if lines[0] != format!("v{}", SCHEMA_VERSION) {
        log::warn!(
            "Schema version mismatch in {}, rebuilding cache...",
            cache_type
        );
        return Ok(None);
    }

    // Parse hash
    if lines.len() < 2 {
        return Ok(None);
    }

    let hash = lines[1].parse::<u64>().ok();
    Ok(hash)
}

/// Schema for query cookbook table
pub fn query_cookbook_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("description", DataType::Utf8, false),
        Field::new("query", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float64, true)),
                EMBEDDING_DIM,
            ),
            false,
        ),
    ]))
}

/// Schema for field metadata table
pub fn field_metadata_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
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
    ]))
}

/// Convert query entries and embeddings to Arrow RecordBatch
pub fn queries_to_record_batch(
    queries: &[QueryEntry],
    embeddings: &[Embedding],
) -> Result<RecordBatch> {
    if queries.len() != embeddings.len() {
        anyhow::bail!("Queries and embeddings length mismatch");
    }

    let schema = query_cookbook_schema();

    // Build column arrays - use document IDs directly
    let ids: StringArray = StringArray::from_iter_values(queries.iter().map(|q| q.id.as_str()));

    let descriptions: StringArray =
        StringArray::from_iter_values(queries.iter().map(|q| q.description.as_str()));

    let query_texts: StringArray =
        StringArray::from_iter_values(queries.iter().map(|q| q.query.as_str()));

    // Convert embeddings to FixedSizeListArray - embeddings are already in correct order
    let embedding_values: Vec<f64> = embeddings.iter().flat_map(|e| e.vec.clone()).collect();

    let embedding_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float64, true)),
        EMBEDDING_DIM,
        Arc::new(Float64Array::from(embedding_values)),
        None,
    )?;

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(ids) as ArrayRef,
            Arc::new(descriptions) as ArrayRef,
            Arc::new(query_texts) as ArrayRef,
            Arc::new(embedding_array) as ArrayRef,
        ],
    )
    .map_err(|e| anyhow::anyhow!("Failed to create RecordBatch: {}", e))
}

/// Convert field documents and embeddings to Arrow RecordBatch
pub fn fields_to_record_batch(
    fields: &[FieldDocument],
    embeddings: &[Embedding],
) -> Result<RecordBatch> {
    if fields.len() != embeddings.len() {
        anyhow::bail!("Fields and embeddings length mismatch");
    }

    let schema = field_metadata_schema();

    // Build column arrays - use document IDs directly
    let ids: StringArray = StringArray::from_iter_values(fields.iter().map(|f| f.id.as_str()));

    let descriptions: StringArray =
        StringArray::from_iter_values(fields.iter().map(|f| f.description.as_str()));

    let categories: StringArray =
        StringArray::from_iter_values(fields.iter().map(|f| f.field.category.as_str()));

    let data_types: StringArray =
        StringArray::from_iter_values(fields.iter().map(|f| f.field.data_type.as_str()));

    let selectable: BooleanArray = fields.iter().map(|f| Some(f.field.selectable)).collect();

    let filterable: BooleanArray = fields.iter().map(|f| Some(f.field.filterable)).collect();

    let sortable: BooleanArray = fields.iter().map(|f| Some(f.field.sortable)).collect();

    let metrics_compatible: BooleanArray = fields
        .iter()
        .map(|f| Some(f.field.metrics_compatible))
        .collect();

    let resource_names: StringArray =
        StringArray::from_iter(fields.iter().map(|f| f.field.resource_name.as_deref()));

    // Convert embeddings
    let embedding_values: Vec<f64> = embeddings.iter().flat_map(|e| e.vec.clone()).collect();

    let embedding_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float64, true)),
        EMBEDDING_DIM,
        Arc::new(Float64Array::from(embedding_values)),
        None,
    )?;

    RecordBatch::try_new(
        schema,
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
            Arc::new(embedding_array) as ArrayRef,
        ],
    )
    .map_err(|e| anyhow::anyhow!("Failed to create RecordBatch: {}", e))
}

/// Open or create a LanceDB connection
pub async fn get_lancedb_connection() -> Result<Connection> {
    let db_path = get_lancedb_path()?;
    let db = connect(db_path.to_string_lossy().as_ref())
        .execute()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to LanceDB: {}", e))?;
    Ok(db)
}

/// Create or overwrite a LanceDB table with vector index
pub async fn create_table(
    db: &Connection,
    table_name: &str,
    record_batch: RecordBatch,
) -> Result<Table> {
    // Create table using Box<dyn RecordBatchReader>
    use arrow_array::RecordBatchReader;
    use std::sync::Arc as StdArc;

    struct SingleBatchReader {
        schema: StdArc<Schema>,
        batch: Option<RecordBatch>,
    }

    impl Iterator for SingleBatchReader {
        type Item = std::result::Result<RecordBatch, arrow_schema::ArrowError>;

        fn next(&mut self) -> Option<Self::Item> {
            self.batch.take().map(Ok)
        }
    }

    impl RecordBatchReader for SingleBatchReader {
        fn schema(&self) -> StdArc<Schema> {
            self.schema.clone()
        }
    }

    let schema = record_batch.schema();
    let reader = SingleBatchReader {
        schema: schema.clone(),
        batch: Some(record_batch),
    };

    // Drop existing table if it exists, then create new one
    if let Ok(_existing) = db.open_table(table_name).execute().await {
        log::debug!("Dropping existing table '{}'", table_name);
        db.drop_table(table_name)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to drop existing table {}: {}", table_name, e))?;
    }

    let table = db
        .create_table(table_name, Box::new(reader))
        .execute()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create table {}: {}", table_name, e))?;

    // Create vector index with cosine similarity metric
    // Only create index if we have enough rows (IVF_PQ typically needs 256+ rows)
    // if num_rows >= 256 {
    //     log::debug!("Creating vector index with cosine similarity for table '{}'", table_name);
    //     table
    //         .create_index(&["vector"], lancedb::index::Index::IvfPq(
    //             lancedb::index::vector::IvfPqIndexBuilder::default()
    //                 .distance_type(lancedb::DistanceType::Cosine)
    //         ))
    //         .execute()
    //         .await
    //         .map_err(|e| anyhow::anyhow!("Failed to create vector index for {}: {}", table_name, e))?;
    //     log::debug!("Vector index created successfully for table '{}'", table_name);
    // } else {
    //     log::debug!("Skipping index creation for table '{}' (only {} rows, need 256+)", table_name, num_rows);
    // }

    Ok(table)
}

/// Open an existing LanceDB table
pub async fn open_table(db: &Connection, table_name: &str) -> Result<Table> {
    let table = db
        .open_table(table_name)
        .execute()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to open table {}: {}", table_name, e))?;
    Ok(table)
}

/// Build or load query cookbook vector store with LanceDB persistence
pub async fn build_or_load_query_vector_store(
    queries: Vec<QueryEntry>,
    embeddings: Vec<Embedding>,
    current_hash: u64,
) -> Result<Table> {
    let cache_type = "query_cookbook";
    let table_name = "query_cookbook";

    // Check if cache exists and is valid
    if let Ok(Some(cached_hash)) = load_hash(cache_type) {
        if cached_hash == current_hash {
            log::info!(
                "✓ Query cookbook cache is valid (hash: {}), loading from LanceDB...",
                current_hash
            );

            // Try to open existing table
            match get_lancedb_connection().await {
                Ok(db) => match open_table(&db, table_name).await {
                    Ok(table) => {
                        log::info!("Successfully loaded query cookbook from cache");
                        return Ok(table);
                    }
                    Err(e) => {
                        log::warn!("Failed to open table: {}, rebuilding...", e);
                    }
                },
                Err(e) => {
                    log::warn!("Failed to connect to LanceDB: {}, rebuilding...", e);
                }
            }
        } else {
            log::info!(
                "✗ Query cookbook cache is stale (hash mismatch: {} vs {}), rebuilding...",
                cached_hash,
                current_hash
            );
        }
    } else {
        log::info!("No query cookbook cache found, building embeddings...");
    }

    // Cache miss or invalid - rebuild embeddings
    let start = std::time::Instant::now();

    // Convert to RecordBatch
    let record_batch = queries_to_record_batch(&queries, &embeddings)?;

    // Save to LanceDB
    let db = get_lancedb_connection().await?;
    let table = create_table(&db, table_name, record_batch).await?;

    // Save hash
    save_hash(cache_type, current_hash)?;

    log::info!(
        "Query cookbook cache built and saved in {:.2}s",
        start.elapsed().as_secs_f64()
    );

    Ok(table)
}

/// Build or load field metadata vector store with LanceDB persistence
pub async fn build_or_load_field_vector_store(
    field_docs: Vec<FieldDocument>,
    embeddings: Vec<Embedding>,
    current_hash: u64,
) -> Result<Table> {
    let cache_type = "field_metadata";
    let table_name = "field_metadata";

    // Check if cache exists and is valid
    if let Ok(Some(cached_hash)) = load_hash(cache_type) {
        if cached_hash == current_hash {
            log::info!(
                "✓ Field metadata cache is valid (hash: {}), loading from LanceDB...",
                current_hash
            );

            // Try to open existing table
            match get_lancedb_connection().await {
                Ok(db) => match open_table(&db, table_name).await {
                    Ok(table) => {
                        log::info!("Successfully loaded field metadata from cache");
                        return Ok(table);
                    }
                    Err(e) => {
                        log::warn!("Failed to open table: {}, rebuilding...", e);
                    }
                },
                Err(e) => {
                    log::warn!("Failed to connect to LanceDB: {}, rebuilding...", e);
                }
            }
        } else {
            log::info!(
                "✗ Field metadata cache is stale (hash mismatch: {} vs {}), rebuilding...",
                cached_hash,
                current_hash
            );
        }
    } else {
        log::info!("No field metadata cache found, building embeddings...");
    }

    // Cache miss or invalid - rebuild embeddings
    let start = std::time::Instant::now();

    // Convert to RecordBatch
    let record_batch = fields_to_record_batch(&field_docs, &embeddings)?;

    // Save to LanceDB
    let db = get_lancedb_connection().await?;
    let table = create_table(&db, table_name, record_batch).await?;

    // Save hash
    save_hash(cache_type, current_hash)?;

    log::info!(
        "Field metadata cache built and saved in {:.2}s ({} fields)",
        start.elapsed().as_secs_f64(),
        field_docs.len()
    );

    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_metadata::FieldMetadata;
    use crate::prompt2gaql::FieldDocument;

    #[test]
    fn test_query_cookbook_schema() {
        let schema = query_cookbook_schema();
        assert_eq!(schema.fields().len(), 4);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(1).name(), "description");
        assert_eq!(schema.field(2).name(), "query");
        assert_eq!(schema.field(3).name(), "vector");
    }

    #[test]
    fn test_field_metadata_schema() {
        let schema = field_metadata_schema();
        assert_eq!(schema.fields().len(), 10);
        assert_eq!(schema.field(0).name(), "id");
        assert_eq!(schema.field(9).name(), "vector");
    }

    #[test]
    fn test_queries_to_record_batch() {
        let queries = vec![
            QueryEntry {
                id: "query_test_query_1_select_campaig".to_string(),
                description: "Test query 1".to_string(),
                query: "SELECT campaign.name FROM campaign".to_string(),
            },
            QueryEntry {
                id: "query_test_query_2_select_campaig".to_string(),
                description: "Test query 2".to_string(),
                query: "SELECT campaign.id FROM campaign".to_string(),
            },
        ];

        let embeddings = vec![
            Embedding {
                vec: vec![0.1_f64; 768],
                document: String::new(),
            },
            Embedding {
                vec: vec![0.2_f64; 768],
                document: String::new(),
            },
        ];

        let result = queries_to_record_batch(&queries, &embeddings);
        assert!(result.is_ok());

        let batch = result.unwrap();
        assert_eq!(batch.num_rows(), 2);
        assert_eq!(batch.num_columns(), 4);
    }

    #[test]
    fn test_fields_to_record_batch() {
        let fields = vec![FieldDocument {
            id: "campaign.name".to_string(),
            field: FieldMetadata {
                name: "campaign.name".to_string(),
                category: "ATTRIBUTE".to_string(),
                data_type: "STRING".to_string(),
                selectable: true,
                filterable: true,
                sortable: true,
                metrics_compatible: false,
                resource_name: Some("campaign".to_string()),
            },
            description: "Campaign name attribute".to_string(),
        }];

        let embeddings = vec![Embedding {
            vec: vec![0.1_f64; 768],
            document: String::new(),
        }];

        let result = fields_to_record_batch(&fields, &embeddings);
        assert!(result.is_ok());

        let batch = result.unwrap();
        assert_eq!(batch.num_rows(), 1);
        assert_eq!(batch.num_columns(), 10);
    }

    #[test]
    fn test_hash_save_and_load() {
        let cache_type = "test_cache";
        let test_hash: u64 = 12345678;

        // Save hash
        let save_result = save_hash(cache_type, test_hash);
        assert!(save_result.is_ok());

        // Load hash
        let load_result = load_hash(cache_type);
        assert!(load_result.is_ok());
        assert_eq!(load_result.unwrap(), Some(test_hash));

        // Clean up
        let hash_path = get_hash_path(cache_type).unwrap();
        let _ = std::fs::remove_file(hash_path);
    }
}

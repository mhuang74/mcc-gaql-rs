use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use mcc_gaql_gen::enricher;
use mcc_gaql_gen::model_pool;
use mcc_gaql_gen::proto_docs_cache;
use mcc_gaql_gen::proto_locator;
use mcc_gaql_gen::r2;
use mcc_gaql_gen::rag;
use mcc_gaql_gen::scraper;
use mcc_gaql_gen::vector_store;

use mcc_gaql_common::config::{QueryEntry, get_queries_from_file};
use mcc_gaql_common::field_metadata::FieldMetadataCache;
use mcc_gaql_common::paths::config_file_path;

/// Core resources for test-run mode
const TEST_RUN_RESOURCES: &[&str] = &["campaign", "ad_group", "ad_group_ad", "ad_group_criterion"];

/// Filter resources for test-run mode
fn filter_test_resources(resources: Vec<String>) -> Vec<String> {
    let test_set: std::collections::HashSet<_> = TEST_RUN_RESOURCES.iter().cloned().collect();
    resources
        .into_iter()
        .filter(|r| test_set.contains(r.as_str()))
        .collect()
}

/// GAQL generation tool using LLM and RAG from Google Ads field metadata
#[derive(Parser)]
#[command(name = "mcc-gaql-gen", version, about)]
struct Cli {
    /// Enable verbose debug logging
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scrape Google Ads API documentation to build field descriptions
    Scrape {
        /// Path to the field metadata cache (JSON). Defaults to the standard cache path.
        #[arg(long)]
        metadata_cache: Option<PathBuf>,

        /// Path to write scraped docs cache. Defaults to standard cache dir.
        #[arg(long)]
        output: Option<PathBuf>,

        /// Rate limit delay in milliseconds between requests (default: 500)
        #[arg(long, default_value = "500")]
        delay_ms: u64,

        /// Cache TTL in days (default: 30)
        #[arg(long, default_value = "30")]
        ttl_days: i64,

        /// Only process core resources (campaign, ad_group, ad_group_ad, ad_group_criterion) for testing
        #[arg(long)]
        test_run: bool,
    },

    /// Enrich field metadata with LLM-generated descriptions
    Enrich {
        /// Path to the field metadata cache (JSON). Defaults to the standard cache path.
        #[arg(long)]
        metadata_cache: Option<PathBuf>,

        /// Path to output enriched cache. Defaults to field_metadata_enriched.json.
        #[arg(long)]
        output: Option<PathBuf>,

        /// Path to scraped docs cache. Defaults to standard cache dir.
        #[arg(long)]
        scraped_docs: Option<PathBuf>,

        /// Number of fields per LLM batch (default: 15)
        #[arg(long, default_value = "15")]
        batch_size: usize,

        /// Rate limit delay in milliseconds between scrape requests (default: 500)
        #[arg(long, default_value = "500")]
        scrape_delay_ms: u64,

        /// Scrape cache TTL in days (default: 30)
        #[arg(long, default_value = "30")]
        scrape_ttl_days: i64,

        /// Only process core resources (campaign, ad_group, ad_group_ad, ad_group_criterion) for testing
        #[arg(long)]
        test_run: bool,

        /// Use proto documentation as primary source (no LLM calls). Overrides --no-llm.
        #[arg(long)]
        use_proto: bool,
    },

    /// Generate a GAQL query from a natural language prompt
    Generate {
        /// Natural language query prompt
        prompt: String,

        /// Path to query cookbook TOML file
        #[arg(long)]
        queries: Option<String>,

        /// Path to enriched field metadata JSON (defaults to standard enriched cache path)
        #[arg(long)]
        metadata: Option<PathBuf>,

        /// Skip implicit default filters (e.g., status = ENABLED)
        #[arg(long)]
        no_defaults: bool,

        /// Enable RAG search for query cookbook examples in LLM prompts
        #[arg(long)]
        use_query_cookbook: bool,

        /// Print explanation of the LLM selection process to stdout
        #[arg(long)]
        explain: bool,
    },

    /// Upload enriched metadata to Cloudflare R2
    Upload {
        /// Path to enriched metadata file to upload
        #[arg(long)]
        file: Option<PathBuf>,

        /// R2 object key (default: field_metadata_enriched.json)
        #[arg(long, default_value = "field_metadata_enriched.json")]
        key: String,
    },

    /// Download enriched metadata from Cloudflare R2
    Download {
        /// R2 public base URL
        #[arg(long)]
        public_url: Option<String>,

        /// R2 object key (default: field_metadata_enriched.json)
        #[arg(long, default_value = "field_metadata_enriched.json")]
        key: String,

        /// Destination path (defaults to standard enriched cache path)
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Parse proto files from googleads-rs to extract field documentation
    ParseProtos {
        /// Path to proto docs cache output. Defaults to ~/.cache/mcc-gaql/proto_docs_v23.json
        #[arg(long)]
        output: Option<PathBuf>,

        /// Force rebuild of cache even if it exists
        #[arg(long)]
        force: bool,
    },

    /// Index embeddings for fast generation (pre-build LanceDB cache)
    Index {
        /// Path to query cookbook TOML file
        #[arg(long)]
        queries: Option<String>,

        /// Path to enriched field metadata JSON (defaults to standard enriched cache path)
        #[arg(long)]
        metadata: Option<PathBuf>,
    },

    /// Clear the LanceDB vector cache
    ClearCache,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_logger(cli.verbose);

    match cli.command {
        Commands::Scrape {
            metadata_cache,
            output,
            delay_ms,
            ttl_days,
            test_run,
        } => {
            cmd_scrape(metadata_cache, output, delay_ms, ttl_days, test_run).await?;
        }

        Commands::Enrich {
            metadata_cache,
            output,
            scraped_docs,
            batch_size,
            scrape_delay_ms,
            scrape_ttl_days,
            test_run,
            use_proto,
        } => {
            cmd_enrich(
                metadata_cache,
                output,
                scraped_docs,
                batch_size,
                scrape_delay_ms,
                scrape_ttl_days,
                test_run,
                use_proto,
            )
            .await?;
        }

        Commands::Generate {
            prompt,
            queries,
            metadata,
            no_defaults,
            use_query_cookbook,
            explain,
        } => {
            cmd_generate(
                prompt,
                queries,
                metadata,
                no_defaults,
                use_query_cookbook,
                explain,
                cli.verbose,
            )
            .await?;
        }

        Commands::Upload { file, key } => {
            cmd_upload(file, key).await?;
        }

        Commands::Download {
            public_url,
            key,
            output,
        } => {
            cmd_download(public_url, key, output).await?;
        }

        Commands::ParseProtos { output, force } => {
            cmd_parse_protos(output, force).await?;
        }

        Commands::Index { queries, metadata } => {
            cmd_index(queries, metadata).await?;
        }

        Commands::ClearCache => {
            vector_store::clear_cache()?;
        }
    }

    Ok(())
}

/// Scrape Google Ads API reference documentation
async fn cmd_scrape(
    metadata_cache: Option<PathBuf>,
    output: Option<PathBuf>,
    delay_ms: u64,
    ttl_days: i64,
    test_run: bool,
) -> Result<()> {
    // Load metadata cache to get the list of resources
    let cache_path = metadata_cache
        .or_else(|| mcc_gaql_common::paths::field_metadata_cache_path().ok())
        .context("Could not determine field metadata cache path")?;

    println!("Loading field metadata from {:?}...", cache_path);
    let cache = FieldMetadataCache::load_from_disk(&cache_path)
        .await
        .context(
            "Failed to load field metadata cache. Run 'mcc-gaql --refresh-field-cache' first.",
        )?;

    let mut resources = cache.get_resources();

    // Filter for test-run mode
    if test_run {
        resources = filter_test_resources(resources);
        println!(
            "Test run mode: limited to {} resources (campaign, ad_group, ad_group_ad, ad_group_criterion)",
            resources.len()
        );
    }

    println!(
        "Found {} resources. Starting scrape (delay: {}ms, TTL: {} days)...",
        resources.len(),
        delay_ms,
        ttl_days
    );

    let scraped_cache_path = output
        .or_else(|| scraper::get_scraped_docs_cache_path().ok())
        .context("Could not determine scraped docs cache path")?;

    let scraped = scraper::ScrapedDocs::load_or_scrape(
        &resources,
        &cache.api_version,
        &scraped_cache_path,
        ttl_days,
        delay_ms,
    )
    .await?;

    println!(
        "\nScraping complete: {} resources scraped, {} skipped, {} field docs collected.",
        scraped.resources_scraped,
        scraped.resources_skipped,
        scraped.docs.len()
    );
    println!("Scraped docs saved to {:?}", scraped_cache_path);

    Ok(())
}

/// Enrich field metadata with LLM descriptions
async fn cmd_enrich(
    metadata_cache: Option<PathBuf>,
    output: Option<PathBuf>,
    scraped_docs: Option<PathBuf>,
    batch_size: usize,
    scrape_delay_ms: u64,
    scrape_ttl_days: i64,
    test_run: bool,
    use_proto: bool,
) -> Result<()> {
    // Load metadata cache first (needed for both proto and LLM modes)
    let cache_path = metadata_cache
        .or_else(|| mcc_gaql_common::paths::field_metadata_cache_path().ok())
        .context("Could not determine field metadata cache path")?;

    println!("Loading field metadata from {:?}...", cache_path);
    let mut cache = FieldMetadataCache::load_from_disk(&cache_path)
        .await
        .context(
            "Failed to load field metadata cache. Run 'mcc-gaql --refresh-field-cache' first.",
        )?;

    // Filter resources in cache for test-run mode BEFORE enrichment
    if test_run {
        let test_resources = filter_test_resources(cache.get_resources());
        cache.retain_resources(&test_resources);
        println!(
            "Test run mode: limited to {} resources, {} fields",
            cache.get_resources().len(),
            cache.fields.len()
        );
    }

    println!(
        "Loaded {} fields from {} resources.",
        cache.fields.len(),
        cache.get_resources().len()
    );

    // Proto-based enrichment (no LLM needed)
    if use_proto {
        return cmd_enrich_proto(&mut cache, output).await;
    }

    // LLM-based enrichment (original path)
    // Validate LLM environment
    validate_llm_env()?;

    let llm_config = Arc::new(rag::LlmConfig::from_env());
    log::info!(
        "LLM configured with {} model(s): {:?}",
        llm_config.model_count(),
        llm_config.all_models()
    );

    let model_pool = Arc::new(model_pool::ModelPool::new(Arc::clone(&llm_config)));

    // Load proto docs (preferred) or scraped docs (legacy) for enrichment context
    let scraped = if let Some(scraped_path) = scraped_docs {
        // User explicitly specified scraped docs path
        println!("Loading scraped docs from {:?}...", scraped_path);
        if !scraped_path.exists() {
            anyhow::bail!("Scraped docs not found at {:?}", scraped_path);
        }
        scraper::ScrapedDocs::load_from_disk(&scraped_path)
            .await
            .context("Failed to load scraped docs")?
    } else {
        // Default: use proto docs (more comprehensive than web-scraped docs)
        let proto_cache_path = proto_docs_cache::get_cache_path()?;
        println!("Loading proto docs from {:?}...", proto_cache_path);

        let proto_cache = if proto_cache_path.exists() {
            proto_docs_cache::ProtoDocsCache::load_from_disk(&proto_cache_path)?
        } else {
            anyhow::bail!(
                "Proto docs cache not found at {:?}. Run 'mcc-gaql-gen parse-protos' first.",
                proto_cache_path
            );
        };

        let stats = proto_cache.stats();
        println!(
            "Loaded proto docs: {} messages, {} fields, {} enums",
            stats.message_count, stats.field_count, stats.enum_count
        );

        proto_cache.to_scraped_docs()
    };

    let _ = scrape_delay_ms; // Not used - we don't scrape
    let _ = scrape_ttl_days; // Not used - we don't scrape
    let _ = batch_size; // Used by MetadataEnricher::with_batch_size if configured

    // Run LLM enrichment
    println!("Enriching {} fields using LLM...", cache.fields.len());
    let enricher = enricher::MetadataEnricher::new(model_pool);
    enricher.enrich(&mut cache, &scraped).await?;

    // Save enriched cache
    let enriched_path = output
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata output path")?;

    println!("\nSaving enriched metadata to {:?}...", enriched_path);
    cache.save_to_disk(&enriched_path).await?;

    // Clear vector cache so it gets rebuilt with richer embeddings
    println!("Clearing vector cache so it gets rebuilt with enriched embeddings...");
    vector_store::clear_cache()?;

    println!(
        "\nEnrichment complete. {}/{} fields enriched.",
        cache.enriched_field_count(),
        cache.fields.len()
    );

    Ok(())
}

/// Proto-based enrichment: use proto documentation instead of LLM
async fn cmd_enrich_proto(cache: &mut FieldMetadataCache, output: Option<PathBuf>) -> Result<()> {
    // Stage 1: Load or build proto docs cache
    println!("\nStage 1/2: Loading proto documentation...");

    // Try to load from cache first
    let proto_cache_path = proto_docs_cache::get_cache_path()?;

    let proto_cache = if proto_cache_path.exists() {
        println!("Loading proto docs from cache: {:?}", proto_cache_path);
        proto_docs_cache::ProtoDocsCache::load_from_disk(&proto_cache_path)?
    } else {
        // Build from scratch
        println!("Proto docs cache not found. Building from proto files...");
        let proto_dir = proto_locator::find_googleads_proto_dir()?;
        println!("Found proto directory: {:?}", proto_dir);
        proto_docs_cache::load_or_build_cache(&proto_dir)?
    };

    let proto_stats = proto_cache.stats();
    println!(
        "Loaded proto docs: {} messages, {} fields, {} enums",
        proto_stats.message_count, proto_stats.field_count, proto_stats.enum_count
    );

    // Stage 2: Merge proto docs into field metadata
    println!("\nStage 2/2: Merging proto documentation into field metadata...");

    let enriched_count = proto_docs_cache::merge_into_field_metadata_cache(&proto_cache, cache);

    let total_fields = cache.fields.len();
    println!(
        "Proto enrichment complete: {}/{} fields enriched",
        enriched_count, total_fields
    );

    // Count how many resources got descriptions
    let resources_enriched = cache
        .resource_metadata
        .as_ref()
        .map(|rm| rm.values().filter(|r| r.description.is_some()).count())
        .unwrap_or(0);
    println!(
        "Resource descriptions: {}/{} enriched",
        resources_enriched,
        cache.get_resources().len()
    );

    // Save enriched cache
    let enriched_path = output
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata output path")?;

    println!("\nSaving enriched metadata to {:?}...", enriched_path);
    cache.save_to_disk(&enriched_path).await?;

    // Clear vector cache so it gets rebuilt with richer embeddings
    println!("Clearing vector cache so it gets rebuilt with enriched embeddings...");
    vector_store::clear_cache()?;

    println!(
        "\nEnrichment complete. {}/{} fields enriched.",
        enriched_count, total_fields
    );

    Ok(())
}

/// Generate a GAQL query from a natural language prompt
async fn cmd_generate(
    prompt: String,
    queries: Option<String>,
    metadata: Option<PathBuf>,
    no_defaults: bool,
    use_query_cookbook: bool,
    explain: bool,
    verbose: bool,
) -> Result<()> {
    validate_llm_env()?;

    let llm_config = rag::LlmConfig::from_env();

    // Load query cookbook
    let example_queries: Vec<QueryEntry> = if let Some(queries_file) = queries {
        // Explicit --queries flag provided
        let queries_path = config_file_path(&queries_file)
            .with_context(|| format!("Could not find queries file: {}", queries_file))?;
        log::info!("Loading query cookbook from {:?}...", queries_path);
        let map = get_queries_from_file(&queries_path).await?;
        map.into_values().collect()
    } else if let Some(default_path) = config_file_path("query_cookbook.toml") {
        // Try to auto-discover query_cookbook.toml in config directory
        if default_path.exists() {
            log::info!("Loading query cookbook from {:?}...", default_path);
            match get_queries_from_file(&default_path).await {
                Ok(map) => map.into_values().collect(),
                Err(e) => {
                    log::warn!("Failed to load query cookbook: {}", e);
                    Vec::new()
                }
            }
        } else {
            log::info!("No query cookbook found. Using enhanced field metadata only.");
            Vec::new()
        }
    } else {
        log::info!("No query cookbook specified.");
        Vec::new()
    };

    // Load field metadata
    let metadata_path = metadata
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata path. Use --metadata to specify it.")?;
    log::info!("Loading field metadata from {:?}...", metadata_path);
    let field_cache = FieldMetadataCache::load_from_disk(&metadata_path)
        .await
        .context(
            "Failed to load field metadata. Run 'mcc-gaql-gen enrich' first or use --metadata.",
        )?;

    // Check if metadata is enriched (has resource_metadata with key_fields)
    let is_enriched = field_cache
        .resource_metadata
        .as_ref()
        .map(|m| {
            m.values()
                .any(|rm| !rm.key_attributes.is_empty() || !rm.key_metrics.is_empty())
        })
        .unwrap_or(false);

    if !is_enriched {
        log::warn!("Metadata does not appear to be enriched. Key fields may not be available.");
    }

    // STRICT CHECK: Validate cache matches current data
    let cache_valid = rag::validate_cache_for_data(&field_cache, &example_queries)?;

    if !cache_valid {
        eprintln!("\nERROR: Embeddings cache is not built or is out-of-date.");
        eprintln!("\nTo generate GAQL queries, you must first build the embeddings cache:");
        eprintln!("  mcc-gaql-gen index");
        eprintln!("\nThis is a one-time operation that takes 3-5 minutes.");
        eprintln!("After indexing, 'generate' commands will be instant.");
        anyhow::bail!("Cache not available - run 'mcc-gaql-gen index' first");
    }

    // Cache is valid - proceed with generation
    log::info!("Cache valid. Generating GAQL for: \"{}\"", prompt);

    // Build pipeline config
    let pipeline_config = rag::PipelineConfig {
        add_defaults: !no_defaults,
        use_query_cookbook,
        explain,
    };

    // Generate GAQL using MultiStepRAGAgent
    let result = rag::convert_to_gaql(
        example_queries,
        field_cache,
        &prompt,
        &llm_config,
        pipeline_config,
    )
    .await?;

    println!("{}", result.query);

    // Print explanation if flag is set
    if explain {
        rag::print_selection_explanation(&result.pipeline_trace, &prompt);
    }

    // Log validation errors/warnings if any
    if !result.validation.errors.is_empty() {
        log::error!("Validation errors:");
        for err in &result.validation.errors {
            log::error!("  - {}", err);
        }
    }
    if !result.validation.warnings.is_empty() {
        log::warn!("Validation warnings:");
        for warn in &result.validation.warnings {
            log::warn!("  - {}", warn);
        }
    }

    // Log pipeline trace if verbose
    if verbose {
        log::debug!("--- Pipeline Trace ---");
        log::debug!(
            "Phase 1 - Primary resource: {}",
            result.pipeline_trace.phase1_primary_resource
        );
        log::debug!(
            "Phase 1 - Related resources: {:?}",
            result.pipeline_trace.phase1_related_resources
        );
        log::debug!(
            "Phase 1 - Reasoning: {}",
            result.pipeline_trace.phase1_reasoning
        );
        log::debug!(
            "Phase 2 - Candidates: {} (rejected: {})",
            result.pipeline_trace.phase2_candidate_count,
            result.pipeline_trace.phase2_rejected_count
        );
        log::debug!(
            "Phase 3 - Selected fields: {:?}",
            result.pipeline_trace.phase3_selected_fields
        );
        log::debug!(
            "Phase 3 - Filter fields: {:?}",
            result.pipeline_trace.phase3_filter_fields
        );
        log::debug!(
            "Phase 3 - Order by: {:?}",
            result.pipeline_trace.phase3_order_by_fields
        );
        log::debug!(
            "Phase 4 - WHERE clauses: {:?}",
            result.pipeline_trace.phase4_where_clauses
        );
        if let Some(limit) = result.pipeline_trace.phase4_limit {
            log::debug!("Phase 4 - LIMIT: {}", limit);
        }
        if !result.pipeline_trace.phase4_implicit_filters.is_empty() {
            log::debug!(
                "Phase 4 - Implicit filters: {:?}",
                result.pipeline_trace.phase4_implicit_filters
            );
        }
        log::debug!(
            "Generation time: {}ms",
            result.pipeline_trace.generation_time_ms
        );
    }

    Ok(())
}

/// Index embeddings for fast query generation
async fn cmd_index(queries: Option<String>, metadata: Option<PathBuf>) -> Result<()> {
    validate_llm_env()?;

    let llm_config = rag::LlmConfig::from_env();

    println!("Indexing embeddings for fast GAQL generation...\n");

    // Load query cookbook
    let example_queries: Vec<QueryEntry> = if let Some(queries_file) = queries {
        // Explicit --queries flag provided
        let queries_path = config_file_path(&queries_file)
            .with_context(|| format!("Could not find queries file: {}", queries_file))?;
        println!("Loading query cookbook from {:?}...", queries_path);
        get_queries_from_file(&queries_path)
            .await?
            .into_values()
            .collect()
    } else if let Some(default_path) = config_file_path("query_cookbook.toml") {
        // Try to auto-discover query_cookbook.toml in config directory
        if default_path.exists() {
            println!("Loading query cookbook from {:?}...", default_path);
            match get_queries_from_file(&default_path).await {
                Ok(map) => map.into_values().collect(),
                Err(e) => {
                    log::warn!("Failed to load query cookbook: {}", e);
                    Vec::new()
                }
            }
        } else {
            println!("No query cookbook found. Using enhanced field metadata only.");
            Vec::new()
        }
    } else {
        println!("No query cookbook specified.");
        Vec::new()
    };

    // Load field metadata
    let metadata_path = metadata
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata path. Use --metadata to specify it.")?;
    println!("Loading field metadata from {:?}...", metadata_path);
    let field_cache = FieldMetadataCache::load_from_disk(&metadata_path)
        .await
        .context(
            "Failed to load field metadata. Run 'mcc-gaql-gen enrich' first or use --metadata.",
        )?;

    // Check if cache already exists and is valid
    match rag::validate_cache_for_data(&field_cache, &example_queries)? {
        true => {
            println!("\nEmbeddings cache is already up-to-date.");
            println!("You can now run 'mcc-gaql-gen generate' for instant GAQL generation.");
            return Ok(());
        }
        false => {
            println!("\nBuilding embeddings (this may take 3-5 minutes on first run)...");
            println!("Subsequent runs will be much faster if the data hasn't changed.\n");
        }
    }

    // Build embeddings only (no RAG pipeline)
    let start = std::time::Instant::now();
    rag::build_embeddings(example_queries, &field_cache, &llm_config)
        .await
        .context("Failed to build embeddings index")?;

    println!("\n--- Indexing Complete ---");
    println!("Total time: {:.2}s", start.elapsed().as_secs_f64());

    // Show cache status
    let status = vector_store::check_cache_status()?;
    println!("\nCache Status:");
    println!("  Field metadata: valid: {}", status.field_metadata_valid);
    println!(
        "  Field metadata: updated: {}",
        vector_store::format_timestamp(status.field_metadata_updated)
    );
    println!("  Query cookbook: valid: {}", status.query_cookbook_valid);
    println!(
        "  Query cookbook: updated: {}",
        vector_store::format_timestamp(status.query_cookbook_updated)
    );

    if status.is_valid() {
        println!("\nYou can now run 'mcc-gaql-gen generate' for instant GAQL generation.");
    } else {
        println!(
            "\nWARNING: Some caches may be incomplete. Run this command again if issues persist."
        );
    }

    Ok(())
}

/// Upload enriched metadata to Cloudflare R2
async fn cmd_upload(file: Option<PathBuf>, key: String) -> Result<()> {
    let source_path = file
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata path")?;

    println!("Uploading {:?} as '{}' to R2...", source_path, key);
    r2::upload(&key, &source_path).await?;
    println!("Upload complete.");

    Ok(())
}

/// Download enriched metadata from Cloudflare R2
async fn cmd_download(
    public_url: Option<String>,
    key: String,
    output: Option<PathBuf>,
) -> Result<()> {
    let base_url = public_url
        .or_else(|| env::var("R2_PUBLIC_URL").ok())
        .context("R2 public URL must be specified via --public-url or R2_PUBLIC_URL env var")?;

    let dest_path = output
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine destination path")?;

    println!("Downloading '{}' from R2 to {:?}...", key, dest_path);
    r2::download(&base_url, &key, &dest_path).await?;
    println!("Download complete.");

    Ok(())
}

/// Parse proto files from googleads-rs to extract field documentation
async fn cmd_parse_protos(output: Option<PathBuf>, force: bool) -> Result<()> {
    // Locate proto directory
    println!("Locating googleads-rs proto files...");
    let proto_dir = proto_locator::find_googleads_proto_dir()?;
    println!("Found proto directory: {:?}", proto_dir);
    log::info!("Using proto directory: {:?}", proto_dir);

    // Determine output path
    let output_path = output
        .or_else(|| proto_docs_cache::get_cache_path().ok())
        .context("Could not determine output path")?;
    log::info!("Output path: {:?}", output_path);

    // Check if we should skip (cache exists and not forced)
    if !force && output_path.exists() {
        println!(
            "Proto docs cache already exists at {:?}. Use --force to rebuild.",
            output_path
        );
        let cache = proto_docs_cache::ProtoDocsCache::load_from_disk(&output_path)?;
        let stats = cache.stats();
        println!("{}", stats);
        return Ok(());
    }

    // Build the cache
    println!("Parsing proto files (this may take a minute)...");
    let cache = proto_docs_cache::load_or_build_cache(&proto_dir)?;

    // Save to the specified output path
    cache.save_to_disk(&output_path)?;

    let stats = cache.stats();
    println!("\nProto parsing complete:");
    println!(
        "  - {} messages with {} fields",
        stats.message_count, stats.field_count
    );
    println!(
        "  - {} enums with {} values",
        stats.enum_count, stats.enum_value_count
    );
    println!("\nCache saved to: {:?}", output_path);

    Ok(())
}

/// Validate that required LLM environment variables are set
fn validate_llm_env() -> Result<()> {
    if env::var("MCC_GAQL_LLM_API_KEY").is_err() && env::var("OPENROUTER_API_KEY").is_err() {
        anyhow::bail!(
            "Either MCC_GAQL_LLM_API_KEY or OPENROUTER_API_KEY must be set.\n\
             Set MCC_GAQL_LLM_BASE_URL and MCC_GAQL_LLM_MODEL as well."
        );
    }
    if env::var("MCC_GAQL_LLM_BASE_URL").is_err() {
        anyhow::bail!("MCC_GAQL_LLM_BASE_URL must be set (e.g., https://openrouter.ai/api/v1)");
    }
    if env::var("MCC_GAQL_LLM_MODEL").is_err() {
        anyhow::bail!(
            "MCC_GAQL_LLM_MODEL must be set (e.g., google/gemini-flash-2.0 or gpt-4o-mini)"
        );
    }
    Ok(())
}

/// Initialize logging based on verbosity and environment variables
fn init_logger(verbose: bool) {
    use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

    let base_level = if verbose {
        "debug".to_string()
    } else {
        env::var("MCC_GAQL_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string())
    };

    // Suppress LanceDB deprecation warning about _distance column auto-projection
    // This is an upstream issue in rig-lancedb: https://github.com/0xPlaygrounds/rig/issues/XXX
    // The warning is harmless - _distance is still being included via auto-projection
    let log_spec = format!("{}, lance::dataset::scanner=error", base_level);

    let log_dir = env::var("MCC_GAQL_LOG_DIR").unwrap_or_else(|_| ".".to_string());

    Logger::try_with_env_or_str(&log_spec)
        .unwrap()
        .use_utc()
        .log_to_file(
            FileSpec::default()
                .directory(log_dir)
                .suppress_timestamp()
                .basename("mcc-gaql-gen"),
        )
        .format_for_files(flexi_logger::detailed_format)
        .o_append(true)
        .rotate(
            Criterion::Size(1_000_000),
            Naming::Numbers,
            Cleanup::KeepLogAndCompressedFiles(10, 100),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        .start()
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_test_resources() {
        // Test with mixed resources
        let resources = vec![
            "campaign".to_string(),
            "ad_group".to_string(),
            "ad_group_ad".to_string(),
            "ad_group_criterion".to_string(),
            "ad".to_string(),
            "keyword".to_string(),
            "campaign_budget".to_string(),
            "customer".to_string(),
            "user_list".to_string(),
        ];

        let filtered = filter_test_resources(resources);

        assert_eq!(filtered.len(), 4);
        assert!(filtered.contains(&"campaign".to_string()));
        assert!(filtered.contains(&"ad_group".to_string()));
        assert!(filtered.contains(&"ad_group_ad".to_string()));
        assert!(filtered.contains(&"ad_group_criterion".to_string()));
        assert!(!filtered.contains(&"ad".to_string()));
        assert!(!filtered.contains(&"keyword".to_string()));
        assert!(!filtered.contains(&"campaign_budget".to_string()));
        assert!(!filtered.contains(&"customer".to_string()));
        assert!(!filtered.contains(&"user_list".to_string()));
    }

    #[test]
    fn test_filter_test_resources_empty_input() {
        let resources: Vec<String> = vec![];
        let filtered = filter_test_resources(resources);
        assert!(filtered.is_empty());
    }
}

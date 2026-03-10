use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use mcc_gaql_gen::enricher as enricher;
use mcc_gaql_gen::model_pool as model_pool;
use mcc_gaql_gen::r2 as r2;
use mcc_gaql_gen::rag as rag;
use mcc_gaql_gen::scraper as scraper;
use mcc_gaql_gen::vector_store as vector_store;

use mcc_gaql_common::config::{get_queries_from_file, QueryEntry};
use mcc_gaql_common::field_metadata::FieldMetadataCache;
use mcc_gaql_common::paths::config_file_path;

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
    },

    /// Generate a GAQL query from a natural language prompt
    Generate {
        /// Natural language query prompt
        prompt: String,

        /// Path to query cookbook TOML file
        #[arg(long)]
        queries: Option<String>,

        /// Path to enriched field metadata JSON (for enhanced mode)
        #[arg(long)]
        metadata: Option<PathBuf>,

        /// Use basic RAG mode (no field metadata, query cookbook only)
        #[arg(long)]
        basic: bool,
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
        } => {
            cmd_scrape(metadata_cache, output, delay_ms, ttl_days).await?;
        }

        Commands::Enrich {
            metadata_cache,
            output,
            scraped_docs,
            batch_size,
            scrape_delay_ms,
            scrape_ttl_days,
        } => {
            cmd_enrich(
                metadata_cache,
                output,
                scraped_docs,
                batch_size,
                scrape_delay_ms,
                scrape_ttl_days,
            )
            .await?;
        }

        Commands::Generate {
            prompt,
            queries,
            metadata,
            basic,
        } => {
            cmd_generate(prompt, queries, metadata, basic).await?;
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
) -> Result<()> {
    // Load metadata cache to get the list of resources
    let cache_path = metadata_cache
        .or_else(|| mcc_gaql_common::paths::field_metadata_cache_path().ok())
        .context("Could not determine field metadata cache path")?;

    println!("Loading field metadata from {:?}...", cache_path);
    let cache = FieldMetadataCache::load_from_disk(&cache_path)
        .await
        .context("Failed to load field metadata cache. Run 'mcc-gaql --refresh-field-cache' first.")?;

    let resources = cache.get_resources();
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
) -> Result<()> {
    // Validate LLM environment
    validate_llm_env()?;

    let llm_config = Arc::new(rag::LlmConfig::from_env());
    log::info!(
        "LLM configured with {} model(s): {:?}",
        llm_config.model_count(),
        llm_config.all_models()
    );

    let model_pool = Arc::new(model_pool::ModelPool::new(Arc::clone(&llm_config)));

    // Load metadata cache
    let cache_path = metadata_cache
        .or_else(|| mcc_gaql_common::paths::field_metadata_cache_path().ok())
        .context("Could not determine field metadata cache path")?;

    println!("Loading field metadata from {:?}...", cache_path);
    let mut cache = FieldMetadataCache::load_from_disk(&cache_path)
        .await
        .context("Failed to load field metadata cache. Run 'mcc-gaql --refresh-field-cache' first.")?;

    println!(
        "Loaded {} fields from {} resources.",
        cache.fields.len(),
        cache.get_resources().len()
    );

    // Determine scrape cache path
    let scrape_cache_path = scraped_docs
        .or_else(|| scraper::get_scraped_docs_cache_path().ok())
        .context("Could not determine scraped docs cache path")?;

    // Run enrichment pipeline (includes scraping if needed)
    enricher::run_enrichment_pipeline(
        &mut cache,
        model_pool,
        &scrape_cache_path,
        scrape_ttl_days,
        scrape_delay_ms,
    )
    .await?;

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

    let _ = batch_size; // Used by MetadataEnricher::with_batch_size if configured

    Ok(())
}

/// Generate a GAQL query from a natural language prompt
async fn cmd_generate(
    prompt: String,
    queries: Option<String>,
    metadata: Option<PathBuf>,
    basic: bool,
) -> Result<()> {
    validate_llm_env()?;

    let llm_config = rag::LlmConfig::from_env();

    // Load query cookbook
    let example_queries: Vec<QueryEntry> = if let Some(queries_file) = queries {
        // Explicit --queries flag provided
        let queries_path = config_file_path(&queries_file)
            .with_context(|| format!("Could not find queries file: {}", queries_file))?;
        println!("Loading query cookbook from {:?}...", queries_path);
        let map = get_queries_from_file(&queries_path).await?;
        map.into_values().collect()
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
        println!("No query cookbook specified. Using enhanced field metadata only.");
        Vec::new()
    };

    println!("Generating GAQL for: \"{}\"", prompt);

    let gaql = if basic || metadata.is_none() {
        // Basic RAG mode: use only query cookbook
        rag::convert_to_gaql(example_queries, &prompt, &llm_config).await?
    } else {
        // Enhanced mode: use field metadata + query cookbook
        let metadata_path = metadata.unwrap();
        println!("Loading field metadata from {:?}...", metadata_path);
        let field_cache = FieldMetadataCache::load_from_disk(&metadata_path)
            .await
            .ok();

        rag::convert_to_gaql_enhanced(example_queries, field_cache, &prompt, &llm_config).await?
    };

    println!("\nGenerated GAQL:\n{}", gaql);

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

    let log_level = if verbose {
        "debug".to_string()
    } else {
        env::var("MCC_GAQL_LOG_LEVEL").unwrap_or_else(|_| "warn".to_string())
    };

    let log_dir = env::var("MCC_GAQL_LOG_DIR").unwrap_or_else(|_| ".".to_string());

    Logger::try_with_env_or_str(log_level)
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


use std::env;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

/// Version string including git hash and build time (computed lazily at first use)
static VERSION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{} ({}) built {}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        env!("BUILD_TIME")
    )
});

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

/// Print startup banner with build information to logs
fn print_startup_banner() {
    let version_info = format!(
        "v{} ({}) built {}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        env!("BUILD_TIME")
    );

    log::info!("═════════════════════════════════════════════════════════════════");
    log::info!(" mcc-gaql-gen {} ", version_info);
    log::info!("═════════════════════════════════════════════════════════════════");
}

use mcc_gaql_gen::bundle;
use mcc_gaql_gen::enricher;
use mcc_gaql_gen::formatter;
use mcc_gaql_gen::model_pool;
use mcc_gaql_gen::proto_docs_cache;
use mcc_gaql_gen::proto_locator;
use mcc_gaql_gen::r2;
use mcc_gaql_gen::rag;
use mcc_gaql_gen::scraper;
use mcc_gaql_gen::vector_store;

use mcc_gaql_common::config::{QueryEntry, get_queries_from_file};
use mcc_gaql_common::field_metadata::FieldMetadataCache;
use mcc_gaql_common::paths::{config_file_path, field_metadata_enriched_path};

/// Core resources for test-run mode
const TEST_RUN_RESOURCES: &[&str] = &["campaign", "ad_group", "ad_group_ad", "keyword_view"];

/// Parameters for generate command
struct GenerateParams {
    prompt: String,
    queries: Option<String>,
    metadata: Option<PathBuf>,
    no_defaults: bool,
    use_query_cookbook: bool,
    explain: bool,
    verbose: bool,
    validate: bool,
    profile: Option<String>,
}

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
#[command(name = "mcc-gaql-gen", version = VERSION.as_str(), about)]
struct Cli {
    /// Enable verbose debug logging
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// DEPRECATED: Scrape Google Ads API documentation from the web. Use `parse-protos` instead.
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

        /// Only process core resources (campaign, ad_group, ad_group_ad, keyword_view) for testing
        #[arg(long)]
        test_run: bool,
    },

    /// Enrich field metadata with LLM-generated descriptions
    Enrich {
        /// Resource name to enrich (e.g., "campaign"). If not specified, enriches only resources missing enrichment.
        resource: Option<String>,

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

        /// Only process core resources (campaign, ad_group, ad_group_ad, keyword_view) for testing
        #[arg(long)]
        test_run: bool,

        /// Use proto documentation as primary source (no LLM calls). Overrides --no-llm.
        #[arg(long)]
        use_proto: bool,

        /// Total concurrent LLM requests across all models (default: number of models)
        #[arg(long)]
        concurrency: Option<usize>,

        /// Process all resources, even those already enriched (default: only process resources missing enrichment)
        #[arg(long)]
        all: bool,
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

        /// Validate the generated query against Google Ads API (requires credentials)
        #[arg(long)]
        validate: bool,

        /// Profile to use for validation credentials (auto-detected if only one profile exists)
        #[arg(long)]
        profile: Option<String>,
    },

    /// Download pre-built RAG resources for instant GAQL generation
    Bootstrap {
        /// API version to download
        #[arg(long, default_value = "v23")]
        version: String,

        /// Overwrite existing cache even if valid
        #[arg(long)]
        force: bool,

        /// Skip SHA256 checksum validation
        #[arg(long)]
        skip_validation: bool,

        /// Check current cache validity without downloading
        #[arg(long)]
        verify_only: bool,
    },

    /// Create and upload a RAG bundle to R2 storage
    Publish {
        /// Object key name
        #[arg(long, default_value = "mcc-gaql-rag-bundle-v23.tar.gz")]
        key: String,

        /// Create bundle locally without uploading
        #[arg(long)]
        dry_run: bool,

        /// Path to query_cookbook.toml to include
        #[arg(long)]
        queries: Option<PathBuf>,
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

    /// Display enriched field metadata for debugging RAG pipeline
    Metadata {
        /// Resource name, field name, or pattern
        query: String,

        /// Path to enriched metadata JSON (defaults to standard enriched cache path)
        #[arg(long)]
        metadata: Option<PathBuf>,

        /// Output format: llm, full, json [default: llm]
        #[arg(long, default_value = "llm")]
        format: String,

        /// Filter by category: resource, attribute, metric, segment
        #[arg(long)]
        category: Option<String>,

        /// Use subset resources only (campaign, ad_group, ad_group_ad, keyword_view). Mainly useful for testing against a small subset of metadata
        #[arg(long)]
        subset: bool,

        /// Show all fields (default shows LLM-limited view with 15 per category)
        #[arg(long)]
        show_all: bool,

        /// Show enrichment comparison (requires non-enriched cache in cache dir)
        #[arg(long)]
        diff: bool,

        /// Filter fields: no-description, no-usage-notes, fallback (resources only)
        #[arg(long)]
        filter: Option<String>,

        /// Use fast pattern matching instead of semantic search
        #[arg(long, short = 'q')]
        quick: bool,
    },

    /// Backfill identity fields into an enriched metadata cache (no LLM required)
    BackfillIdentity {
        /// Path to enriched metadata JSON (defaults to standard enriched cache path)
        #[arg(long)]
        metadata: Option<PathBuf>,

        /// Force recomputation even if identity fields are already populated
        #[arg(long)]
        force: bool,
    },

    /// Clear the LanceDB vector cache
    ClearCache,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    init_logger(cli.verbose);
    print_startup_banner();

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
            resource,
            metadata_cache,
            output,
            scraped_docs,
            batch_size,
            scrape_delay_ms,
            scrape_ttl_days,
            test_run,
            use_proto,
            concurrency,
            all,
        } => {
            cmd_enrich(
                resource,
                metadata_cache,
                output,
                scraped_docs,
                batch_size,
                scrape_delay_ms,
                scrape_ttl_days,
                test_run,
                use_proto,
                concurrency,
                all,
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
            validate,
            profile,
        } => {
            cmd_generate(GenerateParams {
                prompt,
                queries,
                metadata,
                no_defaults,
                use_query_cookbook,
                explain,
                verbose: cli.verbose,
                validate,
                profile,
            })
            .await?;
        }

        Commands::Bootstrap {
            version,
            force,
            skip_validation,
            verify_only,
        } => {
            cmd_bootstrap(version, force, skip_validation, verify_only).await?;
        }

        Commands::Publish {
            key,
            dry_run,
            queries,
        } => {
            cmd_publish(key, dry_run, queries).await?;
        }

        Commands::ParseProtos { output, force } => {
            cmd_parse_protos(output, force).await?;
        }

        Commands::Index { queries, metadata } => {
            cmd_index(queries, metadata).await?;
        }

        Commands::Metadata {
            query,
            metadata,
            format,
            category,
            subset,
            show_all,
            diff,
            filter,
            quick,
        } => {
            cmd_metadata(
                query, metadata, format, category, subset, show_all, diff, filter, quick,
            )
            .await?;
        }

        Commands::BackfillIdentity { metadata, force } => {
            cmd_backfill_identity(metadata, force).await?;
        }

        Commands::ClearCache => {
            vector_store::clear_cache()?;
        }
    }

    Ok(())
}

/// DEPRECATED: Scrape Google Ads API documentation from the web.
/// Use `parse-protos` instead for reliable, authoritative field documentation.
async fn cmd_scrape(
    metadata_cache: Option<PathBuf>,
    output: Option<PathBuf>,
    delay_ms: u64,
    ttl_days: i64,
    test_run: bool,
) -> Result<()> {
    // Print deprecation warning
    eprintln!(
        "⚠️  WARNING: The 'scrape' command is deprecated and may be removed in a future version."
    );
    eprintln!(
        "   Use 'mcc-gaql-gen parse-protos' instead for reliable, authoritative field documentation."
    );
    eprintln!();

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
            "Test run mode: limited to {} resources (campaign, ad_group, ad_group_ad, keyword_view)",
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

/// Check if a resource is missing enrichment in the enriched cache.
/// A resource is considered missing enrichment if none of its fields have descriptions,
/// or if the resource metadata lacks key_attributes/key_metrics.
fn resource_missing_enrichment(
    _cache: &FieldMetadataCache,
    enriched_cache: &FieldMetadataCache,
    resource: &str,
) -> bool {
    // Get fields for this resource from the enriched cache
    let resource_fields = enriched_cache.get_resource_fields(resource);

    // If no fields at all in enriched cache, definitely missing enrichment
    if resource_fields.is_empty() {
        return true;
    }

    // Check if any field has a description
    let has_field_descriptions = resource_fields
        .iter()
        .any(|f| f.description.as_ref().is_some_and(|d| !d.is_empty()));

    // Check if resource metadata has key_attributes/key_metrics
    let has_resource_metadata = enriched_cache
        .resource_metadata
        .as_ref()
        .and_then(|rm| rm.get(resource))
        .map(|meta| !meta.key_attributes.is_empty() || !meta.key_metrics.is_empty())
        .unwrap_or(false);

    // Missing enrichment if neither field descriptions nor resource metadata exist
    !has_field_descriptions && !has_resource_metadata
}

/// Enrich field metadata with LLM descriptions
#[allow(clippy::too_many_arguments)]
async fn cmd_enrich(
    resource: Option<String>,
    metadata_cache: Option<PathBuf>,
    output: Option<PathBuf>,
    scraped_docs: Option<PathBuf>,
    batch_size: usize,
    scrape_delay_ms: u64,
    scrape_ttl_days: i64,
    test_run: bool,
    use_proto: bool,
    concurrency: Option<usize>,
    all: bool,
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

    // Determine the target resource(s) for enrichment
    let target_resource = resource;

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

    // Filter to only resources missing enrichment (unless --all flag or optional resource arg specified)
    if !all && target_resource.is_none() && !test_run {
        let enriched_path = output
            .clone()
            .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok());

        if let Some(ref path) = enriched_path {
            if path.exists() {
                println!(
                    "Loading existing enriched cache to identify resources missing enrichment..."
                );
                match FieldMetadataCache::load_from_disk(path).await {
                    Ok(enriched_cache) => {
                        let all_resources = cache.get_resources();
                        let missing_resources: Vec<String> = all_resources
                            .into_iter()
                            .filter(|r| resource_missing_enrichment(&cache, &enriched_cache, r))
                            .collect();

                        if missing_resources.is_empty() {
                            println!(
                                "All resources are already enriched. Use --all to re-enrich everything."
                            );
                            return Ok(());
                        }

                        println!(
                            "Processing {} resources missing enrichment out of {} total resources",
                            missing_resources.len(),
                            cache.get_resources().len()
                        );
                        println!("Missing resources: {}", missing_resources.join(", "));
                        cache.retain_resources(&missing_resources);
                    }
                    Err(e) => {
                        println!(
                            "Could not load existing enriched cache ({}). Processing all resources.",
                            e
                        );
                    }
                }
            } else {
                println!("No existing enriched cache found. Processing all resources.");
            }
        }
    }

    // Filter to single resource if specified
    if let Some(ref res) = target_resource {
        // Validate that the resource exists
        if !cache.get_resources().contains(res) {
            anyhow::bail!(
                "Resource '{}' not found in field metadata cache. Available resources: {}",
                res,
                cache.get_resources().join(", ")
            );
        }
        cache.retain_resources(std::slice::from_ref(res));
        println!(
            "Enriching single resource '{}': {} fields",
            res,
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
        return cmd_enrich_proto(&mut cache, output, target_resource.clone(), all).await;
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

    let model_pool = if let Some(concurrency) = concurrency {
        Arc::new(
            model_pool::ModelPool::new(Arc::clone(&llm_config)).with_total_concurrency(concurrency),
        )
    } else {
        Arc::new(model_pool::ModelPool::new(Arc::clone(&llm_config)))
    };

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

    // Run LLM enrichment
    println!("Enriching {} fields using LLM...", cache.fields.len());
    let enricher = enricher::MetadataEnricher::new(model_pool).with_batch_size(batch_size);
    enricher.enrich(&mut cache, &scraped).await?;

    // Save enriched cache
    let enriched_path = output
        .clone()
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata output path")?;

    // Backup existing enriched cache before modifying
    if enriched_path.exists() {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let backup_path =
            enriched_path.with_file_name(format!("field_metadata_enriched_{}.json", timestamp));
        println!(
            "\nBacking up existing enriched cache to {:?}...",
            backup_path
        );
        std::fs::copy(&enriched_path, &backup_path)?;
    }

    // Merge with existing enriched cache to preserve previously enriched fields
    if enriched_path.exists() {
        println!(
            "Merging with existing enriched cache at {:?}...",
            enriched_path
        );
        let existing_cache = FieldMetadataCache::load_from_disk(&enriched_path).await?;
        cache = merge_enriched_caches(existing_cache, cache);
    }

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
async fn cmd_enrich_proto(
    cache: &mut FieldMetadataCache,
    output: Option<PathBuf>,
    target_resource: Option<String>,
    all: bool,
) -> Result<()> {
    // Filter to only resources missing enrichment (unless --all flag or optional resource arg specified)
    if !all && target_resource.is_none() {
        let enriched_path = output
            .clone()
            .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok());

        if let Some(ref path) = enriched_path {
            if path.exists() {
                println!(
                    "Loading existing enriched cache to identify resources missing enrichment..."
                );
                match FieldMetadataCache::load_from_disk(path).await {
                    Ok(enriched_cache) => {
                        let all_resources = cache.get_resources();
                        let missing_resources: Vec<String> = all_resources
                            .into_iter()
                            .filter(|r| resource_missing_enrichment(cache, &enriched_cache, r))
                            .collect();

                        if missing_resources.is_empty() {
                            println!(
                                "All resources are already enriched. Use --all to re-enrich everything."
                            );
                            return Ok(());
                        }

                        println!(
                            "Processing {} resources missing enrichment out of {} total resources",
                            missing_resources.len(),
                            cache.get_resources().len()
                        );
                        println!("Missing resources: {}", missing_resources.join(", "));
                        cache.retain_resources(&missing_resources);
                    }
                    Err(e) => {
                        println!(
                            "Could not load existing enriched cache ({}). Processing all resources.",
                            e
                        );
                    }
                }
            } else {
                println!("No existing enriched cache found. Processing all resources.");
            }
        }
    }

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
        .clone()
        .or_else(|| mcc_gaql_common::paths::field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata output path")?;

    // Backup existing enriched cache before modifying
    if enriched_path.exists() {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let backup_path =
            enriched_path.with_file_name(format!("field_metadata_enriched_{}.json", timestamp));
        println!(
            "\nBacking up existing enriched cache to {:?}...",
            backup_path
        );
        std::fs::copy(&enriched_path, &backup_path)?;
    }

    // Merge with existing enriched cache to preserve previously enriched fields
    if enriched_path.exists() {
        println!(
            "Merging with existing enriched cache at {:?}...",
            enriched_path
        );
        let existing_cache = FieldMetadataCache::load_from_disk(&enriched_path).await?;
        *cache = merge_enriched_caches(existing_cache, cache.clone());
    }

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
async fn cmd_generate(params: GenerateParams) -> Result<()> {
    validate_llm_env()?;

    let llm_config = rag::LlmConfig::from_env();

    // Load query cookbook
    let example_queries: Vec<QueryEntry> = if let Some(queries_file) = params.queries {
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
    let metadata_path = params
        .metadata
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
    log::info!("Cache valid. Generating GAQL for: \"{}\"", params.prompt);

    // Build pipeline config
    let pipeline_config = rag::PipelineConfig {
        add_defaults: !params.no_defaults,
        use_query_cookbook: params.use_query_cookbook,
        explain: params.explain,
    };

    // Generate GAQL using MultiStepRAGAgent
    let result = rag::convert_to_gaql(
        example_queries,
        field_cache,
        &params.prompt,
        &llm_config,
        pipeline_config,
    )
    .await?;

    println!("{}", result.query);

    // Validate generated query against Google Ads API if requested
    if params.validate {
        let exit_code = match run_validation(&result.query, params.profile).await {
            Ok(()) => {
                eprintln!("Validation: PASSED");
                0
            }
            Err(e) => {
                let msg = e.to_string();
                if let Some(stripped) = msg.strip_prefix("__config_error__:") {
                    eprintln!("Validation error: {}", stripped);
                    2
                } else {
                    eprintln!("Validation: FAILED – {}", msg);
                    1
                }
            }
        };
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
    }

    // Print explanation if flag is set
    if params.explain {
        rag::print_selection_explanation(&result.pipeline_trace, &params.prompt);
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
    if params.verbose {
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

/// Run Google Ads API validation for a GAQL query using mcc-gaql credentials.
/// Returns Ok(()) if valid.
/// Returns Err with message prefixed "__config_error__:" for auth/config issues (exit 2).
/// Returns Err with API error message for invalid queries (exit 1).
async fn run_validation(query: &str, profile: Option<String>) -> Result<()> {
    use mcc_gaql::config as mcc_config;
    use mcc_gaql::googleads::{
        ApiAccessConfig, generate_token_cache_filename, get_api_access, validate_gaql_query,
    };
    use mcc_gaql_common::paths::config_file_path;

    // Resolve profile name
    let profile_name = match profile {
        Some(p) => p,
        None => {
            let profiles = mcc_config::list_profiles()
                .map_err(|e| anyhow::anyhow!("__config_error__:Failed to list profiles: {}", e))?;
            match profiles.len() {
                0 => {
                    return Err(anyhow::anyhow!(
                        "__config_error__:No profiles found in config. Run 'mcc-gaql --setup' first."
                    ));
                }
                1 => profiles.into_iter().next().unwrap(),
                _ => {
                    let profile = profiles.last().unwrap().clone();
                    eprintln!("Using profile '{}'", profile);
                    profile
                }
            }
        }
    };

    // Load config for the profile
    let config = mcc_config::load(&profile_name).map_err(|e| {
        anyhow::anyhow!(
            "__config_error__:Failed to load profile '{}': {}",
            profile_name,
            e
        )
    })?;

    // Resolve token cache filename
    let token_cache_filename = if let Some(explicit) = config.token_cache_filename.as_ref() {
        explicit.clone()
    } else if let Some(email) = config.user_email.as_ref() {
        generate_token_cache_filename(email)
    } else {
        return Err(anyhow::anyhow!(
            "__config_error__:Profile '{}' has no user_email or token_cache_filename. Run 'mcc-gaql --setup' first.",
            profile_name
        ));
    };

    // Check token cache exists
    let token_cache_exists = config_file_path(&token_cache_filename)
        .map(|p| p.exists())
        .unwrap_or(false);
    if !token_cache_exists {
        return Err(anyhow::anyhow!(
            "__config_error__:Token cache '{}' not found. Run 'mcc-gaql --setup' first to authenticate.",
            token_cache_filename
        ));
    }

    // Resolve MCC customer ID
    let mcc_customer_id = config
        .mcc_id
        .as_ref()
        .or(config.customer_id.as_ref())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "__config_error__:Profile '{}' has no mcc_id or customer_id.",
                profile_name
            )
        })?
        .clone();

    // Get API access
    let api_config = ApiAccessConfig {
        mcc_customer_id: mcc_customer_id.clone(),
        token_cache_filename,
        user_email: config.user_email.clone(),
        dev_token: config.dev_token.clone(),
        use_remote_auth: false,
    };

    let access = get_api_access(&api_config)
        .await
        .map_err(|e| anyhow::anyhow!("__config_error__:Authentication failed: {}", e))?;

    validate_gaql_query(access, &mcc_customer_id, query).await
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

/// Download pre-built RAG resources for instant GAQL generation
async fn cmd_bootstrap(
    version: String,
    force: bool,
    skip_validation: bool,
    verify_only: bool,
) -> Result<()> {
    let bundle_url = r2::public_bundle_url(&format!("mcc-gaql-rag-bundle-{}.tar.gz", version));

    // Check current cache status if verify-only or to determine if download needed
    let verification = bundle::verify_cache().await?;

    if verify_only {
        println!("\nCache status:");
        println!(
            "  Field metadata: {}",
            if verification.field_metadata_valid {
                "valid"
            } else {
                "missing"
            }
        );
        println!(
            "  Query cookbook: {}",
            if verification.query_cookbook_valid {
                "valid"
            } else {
                "missing"
            }
        );
        println!(
            "  LanceDB: {}",
            if verification.lancedb_valid {
                "valid"
            } else {
                "missing"
            }
        );

        if verification.is_valid() {
            println!(
                "\n  Fields: {} ({} enriched)",
                verification.field_count, verification.enriched_field_count
            );
            println!("  Resources: {}", verification.resource_count);
            println!("  Queries: {}", verification.query_count);
            println!("\nReady for 'mcc-gaql-gen generate'");
        } else {
            println!("\nCache incomplete. Run 'mcc-gaql-gen bootstrap' to download.");
        }
        return Ok(());
    }

    // Check if we need to download
    if !force && verification.is_valid() {
        println!("Cache already valid (use --force to re-download).");
        println!("\nRun 'mcc-gaql-gen generate' to create GAQL queries.");
        return Ok(());
    }

    // Download bundle
    println!("Downloading bundle from {}...", bundle_url);
    let bundle_path = bundle::download_bundle(&bundle_url).await?;
    println!("Download complete.");

    // Extract bundle
    println!("Extracting bundle...");
    let extracted = bundle::extract_bundle(&bundle_path, skip_validation).await?;
    println!("Bundle extracted successfully.");
    println!("  API version: {}", extracted.manifest.api_version);
    println!(
        "  Created: {}",
        extracted
            .manifest
            .created_at
            .format("%Y-%m-%d %H:%M:%S UTC")
    );
    println!(
        "  Fields: {} ({} enriched)",
        extracted.manifest.contents.field_count, extracted.manifest.contents.enriched_field_count
    );
    println!(
        "  Resources: {}",
        extracted.manifest.contents.resource_count
    );
    println!(
        "  Queries: {}",
        extracted.manifest.contents.query_cookbook_count
    );

    // Install bundle to cache and config directories
    println!("\nInstalling to local cache...");
    bundle::install_bundle(&extracted, force).await?;

    println!("\n✓ Bootstrap complete!");
    println!("\nReady for 'mcc-gaql-gen generate'");
    println!("\nExample:");
    println!("  export MCC_GAQL_LLM_API_KEY=sk-...");
    println!("  mcc-gaql-gen generate \"show campaign performance last week\"");

    Ok(())
}

/// Create and upload a RAG bundle to R2 storage
async fn cmd_publish(key: String, dry_run: bool, queries: Option<PathBuf>) -> Result<()> {
    // Get query cookbook path
    let queries_path = if let Some(path) = queries {
        path
    } else {
        mcc_gaql_common::paths::config_file_path("query_cookbook.toml")
            .context("Could not find query_cookbook.toml. Specify with --queries")?
    };

    if !queries_path.exists() {
        anyhow::bail!("Query cookbook not found at {:?}", queries_path);
    }

    println!("Creating bundle with query cookbook: {:?}", queries_path);

    // Create bundle
    let bundle_path = std::env::current_dir()?.join(&key);
    let manifest = bundle::create_bundle(&bundle_path, &queries_path).await?;

    println!("\nBundle created: {}", bundle_path.display());
    println!("  API version: {}", manifest.api_version);
    println!(
        "  Fields: {} ({} enriched)",
        manifest.contents.field_count, manifest.contents.enriched_field_count
    );
    println!("  Resources: {}", manifest.contents.resource_count);
    println!("  Queries: {}", manifest.contents.query_cookbook_count);

    if dry_run {
        println!("\nDry run - bundle created locally but not uploaded.");
        println!("To upload, run again without --dry-run.");
        return Ok(());
    }

    // Upload to R2
    println!("\nUploading to R2...");
    let public_url = r2::upload_bundle(&bundle_path, &key).await?;

    println!("\n✓ Publish complete!");
    println!("  Public URL: {}", public_url);
    println!("\nUsers can now run:");
    println!("  mcc-gaql-gen bootstrap");

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

/// Display enriched field metadata for debugging RAG pipeline
#[allow(clippy::too_many_arguments)]
async fn cmd_metadata(
    query: String,
    metadata: Option<PathBuf>,
    format: String,
    category: Option<String>,
    subset: bool,
    show_all: bool,
    diff: bool,
    filter: Option<String>,
    quick: bool,
) -> Result<()> {
    // Determine metadata path
    let metadata_path = metadata
        .or_else(|| field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata path")?;

    if !metadata_path.exists() {
        anyhow::bail!(
            "Enriched metadata not found at {:?}. Run 'mcc-gaql-gen enrich' first.",
            metadata_path
        );
    }

    // Load enriched cache
    let cache = FieldMetadataCache::load_from_disk(&metadata_path)
        .await
        .context("Failed to load enriched metadata")?;

    // Diff mode - compare with non-enriched cache
    if diff {
        let non_enriched_path = metadata_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Invalid metadata path"))?
            .join("field_metadata.json");

        if !non_enriched_path.exists() {
            eprintln!(
                "Warning: Non-enriched cache not found at {:?}. Showing enriched-only output.",
                non_enriched_path
            );
        }

        let non_enriched_cache = if non_enriched_path.exists() {
            Some(FieldMetadataCache::load_from_disk(&non_enriched_path).await?)
        } else {
            None
        };

        if let Some(ne) = non_enriched_cache {
            let output = formatter::format_diff_llm(&cache, &ne, &query, show_all)?;
            print!("{}", output);
            return Ok(());
        }
    }

    // Match query against cache (semantic search by default, pattern matching with --quick)
    let query_result = if quick {
        // Fast pattern matching
        formatter::match_query(&cache, &query)?
    } else {
        // Try semantic search, fall back to pattern matching if vector store unavailable
        match formatter::match_query_semantic(&cache, &query, show_all).await {
            Ok(result) => result,
            Err(e) => {
                log::warn!(
                    "Semantic search unavailable ({}), falling back to pattern matching. Run 'mcc-gaql-gen enrich' to build vector index.",
                    e
                );
                formatter::match_query(&cache, &query)?
            }
        }
    };

    // Apply subset filter
    let query_result = if subset {
        formatter::filter_subset(query_result)
    } else {
        query_result
    };

    // Apply category filter
    let query_result = if let Some(cat) = category {
        formatter::filter_by_category(query_result, &cat)
    } else {
        query_result
    };

    // Apply custom filters
    let query_result = match filter.as_deref() {
        Some("no-description") => formatter::filter_no_description(query_result),
        Some("no-usage-notes") => formatter::filter_no_usage_notes(query_result),
        Some("fallback") => formatter::filter_fallback_resources(query_result),
        Some(f) => {
            anyhow::bail!(
                "Invalid filter: {}. Valid values: no-description, no-usage-notes, fallback",
                f
            );
        }
        None => query_result,
    };

    // Apply similarity threshold filtering (unless --show-all)
    let (query_result, hidden_count) = if show_all {
        (query_result, 0)
    } else {
        formatter::filter_by_similarity(query_result)
    };

    // Format based on format type
    match format.as_str() {
        "llm" => print!("{}", formatter::format_llm(&query_result, show_all, &cache, hidden_count)),
        "full" => print!("{}", formatter::format_full(&query_result)),
        "json" => {
            let json = formatter::format_json(&query_result)?;
            print!("{}", json);
        }
        _ => anyhow::bail!("Invalid format: {}. Valid values: llm, full, json", format),
    }

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

/// Merge a newly enriched cache (single resource) into an existing enriched cache.
/// The new cache's fields take precedence, but all other fields from the existing
/// cache are preserved.
fn merge_enriched_caches(
    existing: FieldMetadataCache,
    new: FieldMetadataCache,
) -> FieldMetadataCache {
    use std::collections::HashMap;

    let mut merged = existing;

    // Update fields with newly enriched ones
    for (field_name, field) in new.fields {
        merged.fields.insert(field_name, field);
    }

    // Update resource metadata with newly enriched resources
    if let Some(new_metadata) = new.resource_metadata {
        if merged.resource_metadata.is_none() {
            merged.resource_metadata = Some(HashMap::new());
        }
        if let Some(ref mut existing_metadata) = merged.resource_metadata {
            for (resource_name, metadata) in new_metadata {
                existing_metadata.insert(resource_name, metadata);
            }
        }
    }

    // Update last_updated timestamp
    merged.last_updated = chrono::Utc::now();

    merged
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

/// Backfill identity fields into an enriched metadata cache without running LLM enrichment.
async fn cmd_backfill_identity(metadata: Option<PathBuf>, force: bool) -> Result<()> {
    let metadata_path = metadata
        .or_else(|| field_metadata_enriched_path().ok())
        .context("Could not determine enriched metadata path")?;

    if !metadata_path.exists() {
        anyhow::bail!(
            "Enriched metadata not found at {:?}. Run 'mcc-gaql-gen enrich' first.",
            metadata_path
        );
    }

    println!("Loading enriched metadata from {:?}...", metadata_path);
    let mut cache = FieldMetadataCache::load_from_disk(&metadata_path)
        .await
        .context("Failed to load enriched metadata")?;

    let count = if force {
        cache.recompute_identity_fields()
    } else {
        cache.backfill_identity_fields()
    };
    if count == 0 && !force {
        println!("All resources already have identity fields. Nothing to do.");
    } else {
        let verb = if force { "Recomputed" } else { "Backfilled" };
        println!(
            "{} identity fields for {} resource(s). Saving...",
            verb, count
        );
        cache
            .save_to_disk(&metadata_path)
            .await
            .context("Failed to save updated metadata")?;
        println!("Done. Run 'mcc-gaql-gen metadata <resource>' to verify.");
    }

    Ok(())
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
            "keyword_view".to_string(),
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
        assert!(filtered.contains(&"keyword_view".to_string()));
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

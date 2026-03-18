use std::{
    fs::{self, File},
    io::{BufWriter, Write},
    process,
    time::Instant,
};

use anyhow::{Context, Result};
use futures::{StreamExt, stream::FuturesUnordered};

use googleads_rs::google::ads::googleads::v23::services::google_ads_service_client::GoogleAdsServiceClient;
use polars::prelude::*;
use thousands::Separable;
use tonic::{codegen::InterceptedService, transport::Channel};

use mcc_gaql_common::config::get_queries_from_file;
use mcc_gaql_common::paths::config_file_path;

use mcc_gaql::args;
use mcc_gaql::config;
use mcc_gaql::field_metadata;
use mcc_gaql::googleads;
#[allow(unused_imports)]
use mcc_gaql::setup;
use mcc_gaql::util;

use args::OutputFormat;
use config::ResolvedConfig;
use field_metadata::{fetch_from_api, load_or_fetch};
use googleads::GoogleAdsAPIAccess;

/// Print startup banner with build information to logs
fn print_startup_banner() {
    let version_info = format!("v{} ({}) built {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"), env!("BUILD_TIME"));

    log::info!("═════════════════════════════════════════════════════════════════");
    log::info!("{}", format!(" mcc-gaql {} ", version_info));
    log::info!("═════════════════════════════════════════════════════════════════");
}

#[tokio::main]
async fn main() -> Result<()> {
    util::init_logger();
    print_startup_banner();

    let mut args = args::parse();

    // Handle --setup flag to run configuration wizard
    if args.setup {
        setup::run_wizard()?;
        return Ok(());
    }

    // Handle --show-config flag to display configuration
    if args.show_config {
        config::display_config(args.profile.as_deref())?;
        return Ok(());
    }

    // Validate argument combinations
    args.validate()?;

    // Handle field metadata operations early (before loading full config)
    if args.export_field_metadata
        || args.show_fields.is_some()
        || args.refresh_field_cache
        || args.show_resources
    {
        let config = if let Some(profile) = &args.profile {
            Some(config::load(profile).context(format!("Loading config for profile: {profile}"))?)
        } else {
            None
        };
        let resolved_config = ResolvedConfig::from_args_and_config(&args, config)?;
        log::debug!("Handle Field Metadata command. Resolved configuration: {resolved_config:?}");

        // Obtain API access for field metadata operations that require it
        let api_context =
            if args.refresh_field_cache || args.export_field_metadata || args.show_fields.is_some()
            {
                resolved_config.validate_for_operation(&args)?;
                Some(
                    googleads::get_api_access(&googleads::ApiAccessConfig {
                        mcc_customer_id: resolved_config.mcc_customer_id.clone(),
                        token_cache_filename: resolved_config.token_cache_filename.clone(),
                        user_email: resolved_config.user_email.clone(),
                        dev_token: resolved_config.dev_token.clone(),
                        use_remote_auth: resolved_config.remote_auth,
                    })
                    .await
                    .context("Authentication required for field metadata operations")?,
                )
            } else {
                None
            };

        let cache_path = std::path::PathBuf::from(&resolved_config.field_metadata_cache);

        if args.refresh_field_cache {
            println!("Refreshing field metadata cache from Google Ads API...");
            let cache = fetch_from_api(api_context.as_ref().unwrap()).await?;
            cache.save_to_disk(&cache_path).await?;
            println!(
                "Field metadata cache refreshed successfully at: {}",
                cache_path.display()
            );
            println!(
                "Fetched {} fields from {} resources",
                cache.fields.len(),
                cache.get_resources().len()
            );
            return Ok(());
        }

        if args.export_field_metadata {
            println!("Loading field metadata cache...");
            let cache = load_or_fetch(
                api_context.as_ref(),
                &cache_path,
                resolved_config.field_metadata_ttl_days,
            )
            .await?;
            println!("{}", cache.export_summary());
            return Ok(());
        }

        if let Some(resource) = args.show_fields {
            println!("Loading field metadata cache...");
            let cache = load_or_fetch(
                api_context.as_ref(),
                &cache_path,
                resolved_config.field_metadata_ttl_days,
            )
            .await?;

            let fields = cache.get_resource_fields(&resource);
            if fields.is_empty() {
                println!("No fields found for resource: {}", resource);
                println!("\nAvailable resources:");
                for r in cache.get_resources() {
                    println!("  - {}", r);
                }
            } else {
                println!("Fields for resource '{}':\n", resource);
                println!(
                    "{:<50} {:<15} {:<10} {:<10} {:<10}",
                    "Field Name", "Data Type", "Selectable", "Filterable", "Sortable"
                );
                println!("{}", "-".repeat(95));
                for field in &fields {
                    println!(
                        "{:<50} {:<15} {:<10} {:<10} {:<10}",
                        field.name,
                        field.data_type,
                        if field.selectable { "Yes" } else { "No" },
                        if field.filterable { "Yes" } else { "No" },
                        if field.sortable { "Yes" } else { "No" },
                    );
                    if let Some(desc) = &field.description {
                        println!("  {}", desc);
                    }
                }
                println!("\nTotal: {} fields", fields.len());
            }
            return Ok(());
        }

        if args.show_resources {
            println!("Loading field metadata cache...");
            let cache = load_or_fetch(
                None, // No API needed for show_resources
                &cache_path,
                resolved_config.field_metadata_ttl_days,
            )
            .await?;
            print!("{}", cache.show_resources());
            return Ok(());
        }
    }

    // Only load config if profile is explicitly specified
    let config = if let Some(profile) = &args.profile {
        log::info!("Config profile: {profile}");
        Some(config::load(profile).context(format!("Loading config for profile: {profile}"))?)
    } else {
        log::info!("No profile specified, using CLI arguments only");
        None
    };

    // Resolve configuration from CLI args and config file
    let resolved_config = ResolvedConfig::from_args_and_config(&args, config)?;

    // Validate that resolved config supports the requested operation
    resolved_config.validate_for_operation(&args)?;

    log::debug!("Resolved configuration: {resolved_config:?}");

    let user_email = resolved_config.user_email.as_deref();
    let mcc_customer_id = resolved_config.mcc_customer_id.as_str();

    // Convert resolved format string to OutputFormat enum
    let output_format = resolved_config
        .format
        .parse::<OutputFormat>()
        .expect("Invalid format in resolved config");
    let _keep_going = resolved_config.keep_going;
    let customer_id = resolved_config.customer_id.as_deref();

    // load stored query
    if let Some(query_name) = args.stored_query {
        let query_filename = resolved_config
            .queries_filename
            .as_ref()
            .expect("queries_filename validated earlier");
        let queries_path = config_file_path(query_filename).unwrap();

        args.gaql_query = match get_queries_from_file(&queries_path).await {
            Ok(map) => {
                let query_entry = map.get(&query_name).expect("Query not found");
                log::debug!("Found query '{query_name}'.");

                Some(query_entry.query.to_owned())
            }
            Err(e) => {
                let msg = format!("Unable to load query: {e}");
                log::error!("{msg}");
                println!("{msg}");
                process::exit(1);
            }
        }
    }

    // for non-FieldService queries, reduce network traffic by excluding resource_name by default
    if let Some(query) = &args.gaql_query
        && !args.field_service
        && !query.contains("PARAMETERS")
    {
        let new_query = format!("{query} PARAMETERS omit_unselected_resource_names = true");
        args.gaql_query = Some(new_query);
    }

    // obtain Google Ads API credentials
    let api_context = match googleads::get_api_access(&googleads::ApiAccessConfig {
        mcc_customer_id: mcc_customer_id.to_string(),
        token_cache_filename: resolved_config.token_cache_filename.clone(),
        user_email: user_email.map(|s| s.to_string()),
        dev_token: resolved_config.dev_token.clone(),
        use_remote_auth: resolved_config.remote_auth,
    })
    .await
    .context(format!(
        "Initial OAuth2 authentication failed for MCC: {}, User: {:?}",
        mcc_customer_id, user_email
    )) {
        Ok(a) => a,
        Err(e) => {
            log::warn!(
                "Authentication failed: {}. Attempting re-auth by clearing token cache",
                e
            );

            // remove cached token to force re-auth and try again
            let token_cache_path = config_file_path(&resolved_config.token_cache_filename)
                .context("Failed to determine token cache file path")?;

            fs::remove_file(&token_cache_path).context(format!(
                "Failed to remove invalid token cache at: {}",
                token_cache_path.display()
            ))?;

            log::info!("Removed cached token at: {}", token_cache_path.display());

            googleads::get_api_access(&googleads::ApiAccessConfig {
                mcc_customer_id: mcc_customer_id.to_string(),
                token_cache_filename: resolved_config.token_cache_filename.clone(),
                user_email: user_email.map(|s| s.to_string()),
                dev_token: resolved_config.dev_token.clone(),
                use_remote_auth: resolved_config.remote_auth,
            })
            .await
            .context(format!(
                "Re-authentication failed after clearing token cache. \
                 MCC: {}, User: {:?}, Token cache: {}",
                mcc_customer_id,
                user_email,
                token_cache_path.display()
            ))?
        }
    };

    // Handle 3 types of Google Ads query: list child accounts, field service, and GAQL query
    if args.list_child_accounts {
        let (customer_id_for_query, query) = if let Some(cid) = customer_id {
            let query: String = googleads::SUB_ACCOUNTS_QUERY.to_owned();
            log::debug!("Listing child accounts under {}", cid);
            (cid.to_string(), query)
        } else {
            log::debug!("Listing ALL child accounts under MCC {}", mcc_customer_id);
            (
                mcc_customer_id.to_string(),
                googleads::SUB_ACCOUNTS_QUERY.to_owned(),
            )
        };

        let dataframe: Option<DataFrame> =
            match googleads::gaql_query(api_context, customer_id_for_query, query).await {
                Ok((df, _api_consumption)) => Some(df),
                Err(e) => {
                    let msg = format!("Error: {e}");
                    println!("{msg}");
                    None
                }
            };

        if dataframe.is_some() {
            output_dataframe(&mut dataframe.unwrap(), output_format, args.output)?;
        }
    } else if args.field_service {
        let query = &args
            .gaql_query
            .expect("Valid Field Service query required.");
        log::info!("Running Fields Metadata query: {query}");
        googleads::fields_query(api_context, query).await;
    } else if args.gaql_query.is_some() {
        // figure out which customerids to query for
        let customer_ids: Option<Vec<String>> = if customer_id.is_some()
            & args.all_linked_child_accounts
        {
            let cid = customer_id.expect("Valid customer_id required.");
            log::debug!("Querying child accounts under MCC: {}", &cid);
            (googleads::get_child_account_ids(api_context.clone(), cid.to_string()).await).ok()
        } else if customer_id.is_some() & !args.all_linked_child_accounts {
            let cid = customer_id.expect("Valid customer_id required.");
            log::debug!("Querying account: {cid}");
            Some(vec![cid.to_string()])
        } else if customer_id.is_none() & args.all_linked_child_accounts {
            let cid = mcc_customer_id.to_string();
            log::debug!("Querying all linked child accounts under MCC: {}", &cid);
            (googleads::get_child_account_ids(api_context.clone(), cid).await).ok()
        } else if customer_id.is_none() & !args.all_linked_child_accounts {
            if let Some(customerids_filename) = resolved_config.customerids_filename.as_deref() {
                let customerids_path = config_file_path(customerids_filename).unwrap();
                log::debug!(
                    "Querying accounts listed in file: {}",
                    customerids_path.display()
                );

                (mcc_gaql_common::config::get_child_account_ids_from_file(
                    customerids_path.as_path(),
                )
                .await)
                    .ok()
            } else {
                log::warn!(
                    "No customerids file specified. Use --customer-id or --all-linked-child-accounts"
                );
                None
            }
        } else {
            log::warn!("Not supposed to get here.");
            None
        };

        // apply query to all customer_ids
        if let Some(customer_ids_vector) = customer_ids {
            let query: String = args.gaql_query.expect("Expected GAQL query");

            gaql_query_async(
                api_context,
                customer_ids_vector,
                query,
                args.groupby,
                args.sortby,
                output_format,
                args.output,
            )
            .await?;
        } else {
            log::error!("Abort GAQL query. Can't find child accounts to run on.");
        }
    } else {
        println!("Nothing to do.");
    }

    Ok(())
}

async fn gaql_query_async(
    api_context: GoogleAdsAPIAccess,
    customer_ids_vector: Vec<String>,
    query: String,
    groupby: Vec<String>,
    sortby: Vec<String>,
    format: OutputFormat,
    outfile: Option<String>,
) -> Result<()> {
    log::info!(
        "Running GAQL query for {} child accounts: {}",
        &customer_ids_vector.len(),
        &query
    );

    let google_ads_client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    let mut gaql_handles = FuturesUnordered::new();

    for customer_id in customer_ids_vector.iter() {
        let gaql_future = googleads::gaql_query_with_client(
            google_ads_client.clone(),
            customer_id.clone(),
            query.clone(),
        );

        gaql_handles.push(tokio::spawn(gaql_future));
    }

    let mut dataframes: Vec<DataFrame> = Vec::new();
    let mut groupby_handles = FuturesUnordered::new();
    let mut metrics_cols: Option<Vec<String>> = None;

    let start = Instant::now();

    let mut total_rows: usize = 0;
    let mut total_api_consumption: i64 = 0;

    // collect asynchronous query results
    while let Some(result) = gaql_handles.next().await {
        match result {
            Ok(result) => {
                match result {
                    Ok((df, api_consumption)) => {
                        if !df.is_empty() {
                            total_rows += df.height();
                            total_api_consumption += api_consumption;

                            if metrics_cols.is_none() {
                                let cols: Vec<String> = df
                                    .get_column_names()
                                    .into_iter()
                                    .filter(|c| c.contains("metrics"))
                                    .map(|c| c.to_string())
                                    .collect();

                                log::debug!("Metric cols: {:?}", cols);
                                metrics_cols = Some(cols.clone());
                            }

                            // check if groupby columns are in metrics columns
                            for col in &groupby {
                                if metrics_cols.as_ref().unwrap().contains(col) {
                                    let msg = format!(
                                        "Groupby column cannot be a metric column: '{}'",
                                        col
                                    );
                                    log::error!("{msg}");
                                    return Err(anyhow::anyhow!(msg));
                                }
                            }

                            if !&groupby.is_empty() {
                                let my_groupby = groupby.clone();
                                let my_sortby = sortby.clone();
                                let my_metrics_cols = metrics_cols.clone().unwrap();
                                groupby_handles.push(tokio::task::spawn_blocking(|| {
                                    apply_groupby(df, my_groupby, my_metrics_cols, my_sortby)
                                }));
                            } else {
                                dataframes.push(df);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("GAQL Error: {e}");
                    }
                }
            }
            Err(e) => {
                log::error!("Thread JOIN Error: {e}");
            }
        }
    }

    let duration = start.elapsed();
    log::info!(
        "GAQL returned {} rows in {} msec across {} accounts using {} API units",
        total_rows.separate_with_commas(),
        duration.as_millis().separate_with_commas(),
        customer_ids_vector.len().separate_with_commas(),
        total_api_consumption.separate_with_commas()
    );

    // collect 1st pass groupby results
    let groupby_start = Instant::now();
    while let Some(result) = groupby_handles.next().await {
        match result {
            Ok(future) => match future.await {
                Ok(df) => {
                    if !df.is_empty() {
                        dataframes.push(df);
                    }
                }
                Err(e) => {
                    log::error!("GROUPBY Error: {e}");
                }
            },
            Err(e) => {
                log::error!("Thread JOIN Error: {e}");
            }
        }
    }
    let groupby_duration = groupby_start.elapsed();
    log::debug!(
        "1st pass groupby used additional foreground time of {} msec",
        groupby_duration.as_millis().separate_with_commas()
    );

    if !dataframes.is_empty() {
        let start = Instant::now();
        let len = &dataframes.len();

        let mut df_iter = dataframes.into_iter();
        let mut dataframe: DataFrame = df_iter.next().unwrap();

        #[allow(clippy::while_let_on_iterator)]
        while let Some(df) = df_iter.next() {
            dataframe = dataframe.vstack(&df)?;
        }

        let duration = start.elapsed();
        log::debug!(
            "merged {:#} dataframes in {} msec",
            len,
            duration.as_millis().separate_with_commas()
        );

        if !groupby.is_empty() || !sortby.is_empty() {
            let start = Instant::now();

            log::info!("Applying global groupby with columns: {:?}", groupby);
            log::info!("Applying global sortby with columns: {:?}", sortby);

            dataframe = apply_groupby(
                dataframe,
                groupby.clone(),
                metrics_cols.clone().unwrap(),
                sortby.clone(),
            )
            .await?;

            let duration = start.elapsed();
            log::debug!(
                "applied 2nd pass groupby in {} msec",
                duration.as_millis().separate_with_commas()
            );
        }

        log::debug!("final dataframe shape: {:?}", dataframe.shape());

        let start = Instant::now();
        output_dataframe(&mut dataframe, format, outfile)?;
        let duration = start.elapsed();
        log::debug!(
            "output written in {} msec",
            duration.as_millis().separate_with_commas()
        );
    }

    Ok(())
}

/// Apply groupby and aggregation to dataframe
async fn apply_groupby(
    df: DataFrame,
    groupby_cols: Vec<String>,
    agg_cols: Vec<String>,
    sortby_cols: Vec<String>,
) -> Result<DataFrame> {
    let sortby_cols_ref = if sortby_cols.is_empty() {
        &groupby_cols
    } else {
        &sortby_cols
    };

    let df_agg = if groupby_cols.is_empty() {
        df.lazy()
    } else {
        df.lazy()
            .group_by(groupby_cols.iter().map(String::as_str).collect::<Vec<_>>())
            .agg(
                agg_cols
                    .iter()
                    .map(|col_name| col(col_name).sum())
                    .collect::<Vec<_>>(),
            )
    }
    .sort(
        sortby_cols_ref,
        SortMultipleOptions {
            descending: vec![true; sortby_cols_ref.len()],
            ..Default::default()
        },
    )
    .collect()?;

    Ok(df_agg)
}

/// Convert a polars AnyValue to serde_json::Value
fn convert_value_to_json(value: AnyValue) -> serde_json::Value {
    match value {
        AnyValue::Null => serde_json::Value::Null,
        AnyValue::Boolean(b) => serde_json::Value::Bool(b),
        AnyValue::String(s) => serde_json::Value::String(s.to_string()),
        AnyValue::Int8(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        AnyValue::Int16(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        AnyValue::Int32(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        AnyValue::Int64(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        AnyValue::UInt8(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        AnyValue::UInt16(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        AnyValue::UInt32(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        AnyValue::UInt64(i) => serde_json::Value::Number(serde_json::Number::from(i)),
        AnyValue::Float32(f) => serde_json::Number::from_f64(f as f64)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(f.to_string())),
        AnyValue::Float64(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or_else(|| serde_json::Value::String(f.to_string())),
        _ => serde_json::Value::String(format!("{}", value)),
    }
}

/// Validate DataFrame is suitable for output
fn validate_dataframe(df: &DataFrame) -> Result<()> {
    if df.width() == 0 {
        return Err(anyhow::anyhow!("Cannot output DataFrame with zero columns"));
    }
    Ok(())
}

fn write_csv(df: &mut DataFrame, outfile: &str) -> Result<()> {
    let f = File::create(outfile)
        .with_context(|| format!("Failed to create CSV output file: {}", outfile))?;
    CsvWriter::new(f)
        .finish(df)
        .with_context(|| format!("Failed to write CSV data to file: {}", outfile))?;

    Ok(())
}

fn write_csv_to_stdout(df: &mut DataFrame) -> Result<()> {
    let mut buf = Vec::new();
    CsvWriter::new(&mut buf)
        .finish(df)
        .context("Failed to write CSV data to buffer")?;
    let csv_string =
        String::from_utf8(buf).context("Failed to convert CSV buffer to UTF-8 string")?;
    print!("{}", csv_string);
    Ok(())
}

fn write_json_to_writer<W: Write>(df: &DataFrame, writer: &mut W) -> Result<()> {
    let columns: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();

    write!(writer, "[").context("Failed to write opening bracket")?;

    for row_idx in 0..df.height() {
        if row_idx > 0 {
            write!(writer, ",").context("Failed to write comma separator")?;
        }

        let mut record = serde_json::Map::new();
        for (col_idx, col_name) in columns.iter().enumerate() {
            let column = df.get_columns().get(col_idx).unwrap();
            let value = column.get(row_idx).with_context(|| {
                format!(
                    "Failed to get value at row {} column '{}'",
                    row_idx, col_name
                )
            })?;
            record.insert(col_name.clone(), convert_value_to_json(value));
        }

        serde_json::to_writer(&mut *writer, &record).context("Failed to write JSON record")?;
    }

    writeln!(writer, "]").context("Failed to write closing bracket")?;
    Ok(())
}

fn write_json_to_stdout(df: &DataFrame) -> Result<()> {
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();
    write_json_to_writer(df, &mut handle).context("Failed to write JSON to stdout")
}

fn write_json(df: &DataFrame, outfile: &str) -> Result<()> {
    let f = File::create(outfile)
        .with_context(|| format!("Failed to create JSON output file: {}", outfile))?;
    let mut writer = BufWriter::new(f);
    write_json_to_writer(df, &mut writer)
        .with_context(|| format!("Failed to write JSON to file: {}", outfile))
}

fn resolve_output_format(
    explicit_format: OutputFormat,
    outfile: &Option<String>,
) -> Result<OutputFormat> {
    let inferred_format = outfile.as_ref().and_then(|path| {
        if path.ends_with(".json") {
            Some(OutputFormat::Json)
        } else if path.ends_with(".csv") {
            Some(OutputFormat::Csv)
        } else {
            None
        }
    });

    match (explicit_format, inferred_format) {
        (format, None) => Ok(format),
        (format, Some(inferred)) if format == inferred => Ok(format),
        (OutputFormat::Table, Some(inferred)) => {
            log::info!(
                "Inferring format {:?} from file extension (override with --format if needed)",
                inferred
            );
            Ok(inferred)
        }
        (explicit, Some(inferred)) => {
            log::warn!(
                "Format mismatch: --format={:?} but file extension suggests {:?}. Using explicit format {:?}",
                explicit,
                inferred,
                explicit
            );
            Ok(explicit)
        }
    }
}

fn output_dataframe(
    df: &mut DataFrame,
    format: OutputFormat,
    outfile: Option<String>,
) -> Result<()> {
    validate_dataframe(df).context("Invalid DataFrame for output")?;

    let resolved_format = resolve_output_format(format, &outfile)?;

    match outfile {
        Some(path) => match resolved_format {
            OutputFormat::Csv => write_csv(df, &path)?,
            OutputFormat::Json => write_json(df, &path)?,
            OutputFormat::Table => {
                log::warn!("Writing table format to file, consider using csv or json");
                let mut f = File::create(path)?;
                write!(f, "{}", df)?;
            }
        },
        None => match resolved_format {
            OutputFormat::Csv => write_csv_to_stdout(df)?,
            OutputFormat::Json => write_json_to_stdout(df)?,
            OutputFormat::Table => println!("{}", df),
        },
    }
    Ok(())
}

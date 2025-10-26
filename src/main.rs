//
// Author: Michael S. Huang (mhuang74@gmail.com)
//

use std::{
    env,
    fs::{self, File},
    process,
    time::Instant,
};

use anyhow::{Context, Result};
use futures::{StreamExt, stream::FuturesUnordered};

use googleads::GoogleAdsAPIAccess;
use googleads_rs::google::ads::googleads::v22::services::google_ads_service_client::GoogleAdsServiceClient;
use polars::prelude::*;
use thousands::Separable;
use tonic::{codegen::InterceptedService, transport::Channel};

mod args;
mod config;
mod googleads;
mod prompt2gaql;
mod util;

use crate::util::QueryEntry;

/// Custom error type for output operations
#[derive(Debug)]
pub enum OutputError {
    IoError(std::io::Error),
    JsonError(serde_json::Error),
    PolarsError(PolarsError),
    Utf8Error(std::string::FromUtf8Error),
}

impl std::fmt::Display for OutputError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputError::IoError(e) => write!(f, "IO error: {}", e),
            OutputError::JsonError(e) => write!(f, "JSON serialization error: {}", e),
            OutputError::PolarsError(e) => write!(f, "DataFrame error: {}", e),
            OutputError::Utf8Error(e) => write!(f, "UTF-8 conversion error: {}", e),
        }
    }
}

impl std::error::Error for OutputError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OutputError::IoError(e) => Some(e),
            OutputError::JsonError(e) => Some(e),
            OutputError::PolarsError(e) => Some(e),
            OutputError::Utf8Error(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for OutputError {
    fn from(err: std::io::Error) -> OutputError {
        OutputError::IoError(err)
    }
}

impl From<serde_json::Error> for OutputError {
    fn from(err: serde_json::Error) -> OutputError {
        OutputError::JsonError(err)
    }
}

impl From<PolarsError> for OutputError {
    fn from(err: PolarsError) -> OutputError {
        OutputError::PolarsError(err)
    }
}

impl From<std::string::FromUtf8Error> for OutputError {
    fn from(err: std::string::FromUtf8Error) -> OutputError {
        OutputError::Utf8Error(err)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    util::init_logger();

    let mut args = args::parse();

    let profile = &args.profile.unwrap_or_else(|| "test".to_owned());

    let config = config::load(profile).context(format!("Loading config for profile: {profile}"))?;

    log::debug!("Configuration: {config:?}");

    // load stored query
    if let Some(query_name) = args.stored_query {
        let query_filename = config
            .queries_filename
            .as_ref()
            .expect("Query cookbook filename undefined");
        let queries_path = crate::config::config_file_path(query_filename).unwrap();

        args.gaql_query = match util::get_queries_from_file(&queries_path).await {
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

    // convert natural language prompt into GAQL
    if args.natural_language {
        // Use OpenAI for LLM
        let openai_api_key = env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set");
        let query_filename = config
            .queries_filename
            .as_ref()
            .expect("Query cookbook filename undefined");
        let queries_path = crate::config::config_file_path(query_filename).unwrap();

        let example_queries: Vec<QueryEntry> =
            match util::get_queries_from_file(&queries_path).await {
                Ok(map) => map.into_values().collect(),
                Err(e) => {
                    let msg = format!("Unable to load query cookbook for RAG: {e}");
                    log::error!("{msg}");
                    println!("{msg}");
                    process::exit(1);
                }
            };

        let prompt = args.gaql_query.as_ref().unwrap();
        log::debug!("Construct GAQL from prompt: {:?}", prompt);

        let query = prompt2gaql::convert_to_gaql(&openai_api_key, example_queries, prompt).await?;

        log::info!("Generated GAQL Query: {:?}", query);

        args.gaql_query = Some(query);
    }

    // for non-FieldService queries, reduce network traffic by excluding resource_name by default
    if let Some(query) = &args.gaql_query
        && !args.field_service
        && !query.contains("PARAMETERS")
    {
        let new_query = format!("{query} PARAMETERS omit_unselected_resource_names = true");
        args.gaql_query = Some(new_query);
    }

    let api_context =
        match googleads::get_api_access(&config.mcc_customerid, &config.token_cache_filename).await
        {
            Ok(a) => a,
            Err(_e) => {
                log::info!(
                    "Refresh token became invalid. Clearing token cache and forcing re-auth"
                );
                // remove cached token to force re-auth and try again
                let token_cache_path =
                    crate::config::config_file_path(&config.token_cache_filename)
                        .expect("token cache path");
                let _ = fs::remove_file(token_cache_path);
                googleads::get_api_access(&config.mcc_customerid, &config.token_cache_filename)
                    .await
                    .expect("Refresh token expired and failed to kick off re-auth.")
            }
        };

    if args.list_child_accounts {
        // run Account listing query

        let (customer_id, query) = if args.customer_id.is_some() {
            // query accounts under specificied customer_id account
            let customer_id = args.customer_id.expect("Valid customer_id required.");
            let query: String = googleads::SUB_ACCOUNTS_QUERY.to_owned();
            log::debug!("Listing child accounts under {customer_id}");
            (customer_id, query)
        } else {
            // query child accounts under MCC
            log::debug!(
                "Listing ALL child accounts under MCC {}",
                &config.mcc_customerid
            );
            (
                config.mcc_customerid,
                googleads::SUB_ACCOUNTS_QUERY.to_owned(),
            )
        };

        let dataframe: Option<DataFrame> =
            match googleads::gaql_query(api_context, customer_id, query).await {
                Ok((df, _api_consumption)) => Some(df),
                Err(e) => {
                    let msg = format!("Error: {e}");
                    println!("{msg}");
                    None
                }
            };

        if dataframe.is_some() {
            output_dataframe(&mut dataframe.unwrap(), &args.format, args.output)?;
        }
    } else if args.field_service {
        let query = &args
            .gaql_query
            .expect("Valid Field Service query required.");
        log::info!("Running Fields Metadata query: {query}");
        googleads::fields_query(api_context, query).await;

    // run provided GAQL query
    } else if args.gaql_query.is_some() {
        // figure out which customerids to query for
        let customer_ids: Option<Vec<String>> =
            // if provided customerid and querying all child accounts,
            // then query all linked accounts under provided customerid
            if args.customer_id.is_some() & args.all_linked_child_accounts {
                let customer_id = args.customer_id.expect("Valid customer_id required.");
                log::debug!("Querying child accounts under MCC: {}", &customer_id);

                // generate new list of child accounts
                (googleads::get_child_account_ids(api_context.clone(), customer_id).await).ok()
            }
            // if provided customerid and not querying all child accounts, 
            // then just query one account
            else if args.customer_id.is_some() & !args.all_linked_child_accounts {
                let customer_id = args.customer_id.expect("Valid customer_id required.");
                log::debug!("Querying account: {customer_id}");

                Some(vec![customer_id])
            }
            // if using default profile MCC and querying all child accounts,
            // then query all linked accounts under profile mcc
            else if args.customer_id.is_none() & args.all_linked_child_accounts {
                let customer_id = config.mcc_customerid;
                log::debug!("Querying all linked child accounts under profile MCC: {}", &customer_id);

                // generate new list of child accounts
                (googleads::get_child_account_ids(api_context.clone(), customer_id).await).ok()
            }
            // if using default profile MCC and NOT querying all child accounts, 
            // then look for customerids file and use it
            else if args.customer_id.is_none() & !args.all_linked_child_accounts {
                if config.customerids_filename.is_some() {
                    let customerids_path =
                    crate::config::config_file_path(&config.customerids_filename.unwrap()).unwrap();
                    log::debug!("Querying accounts listed in file: {}", customerids_path.display());

                    (util::get_child_account_ids_from_file(customerids_path.as_path()).await).ok()
                } else {
                    log::warn!("Expecting customerids file but none found in config");
                    None
                }
            } else {
                log::warn!("Not supposed to get here.");
                None
            };

        // apply query to all customer_ids
        if let Some(customer_id_vector) = customer_ids {
            let query: String = args.gaql_query.expect("Expected GAQL query");

            // run queries asynchroughly across all customer_ids
            gaql_query_async(
                api_context,
                customer_id_vector,
                query,
                args.groupby,
                args.sortby,
                args.format,
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
    customer_id_vector: Vec<String>,
    query: String,
    groupby: Vec<String>,
    sortby: Vec<String>,
    format: String,
    outfile: Option<String>,
) -> Result<()> {
    log::info!(
        "Running GAQL query for {} child accounts: {}",
        &customer_id_vector.len(),
        &query
    );

    let google_ads_client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    let mut gaql_handles = FuturesUnordered::new();

    for customer_id in customer_id_vector.iter() {
        // log::debug!("Querying {customer_id}");

        let gaql_future = googleads::gaql_query_with_client(
            google_ads_client.clone(),
            customer_id.clone(),
            query.clone(),
        );

        // execute gaql query in background thread
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

                            // get list of metrics columns
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

                            // log::debug!("Future returned non-empty GAQL results");
                            if !&groupby.is_empty() {
                                // execute groupby in dedicated non-yielding thread
                                let my_groupby = groupby.clone();
                                let my_sortby = sortby.clone();
                                let my_metrics_cols = metrics_cols.clone().unwrap();
                                groupby_handles.push(tokio::task::spawn_blocking(|| {
                                    apply_groupby(df, my_groupby, my_metrics_cols, my_sortby)
                                }));
                            } else {
                                dataframes.push(df);
                            }
                        } else {
                            // log::debug!("A future returned empty query results");
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
        customer_id_vector.len().separate_with_commas(),
        total_api_consumption.separate_with_commas()
    );

    // collect 1st pass groupby results
    let groupby_start = Instant::now();
    while let Some(result) = groupby_handles.next().await {
        match result {
            Ok(future) => {
                match future.await {
                    Ok(df) => {
                        if !df.is_empty() {
                            // log::debug!("Future returned GROUPBY results");
                            dataframes.push(df);
                        }
                    }
                    Err(e) => {
                        log::error!("GROUPBY Error: {e}");
                    }
                }
            }
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

        // merge dataframes
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

        // apply 2nd pass gropuby/sortby
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
        output_dataframe(&mut dataframe, &format, outfile)?;
        let duration = start.elapsed();
        log::debug!(
            "output written in {} msec",
            duration.as_millis().separate_with_commas()
        );
    }

    Ok(())
}

/// Apply groupby and aggregation to dataframe
/// groupby_cols: columns to group by; cannot be a subset of agg_cols
/// agg_cols: columns to aggregate
async fn apply_groupby(
    df: DataFrame,
    groupby_cols: Vec<String>,
    agg_cols: Vec<String>,
    sortby_cols: Vec<String>,
) -> Result<DataFrame> {
    // if no sortby columns provided, use groupby columns
    let sortby_cols_ref = if sortby_cols.is_empty() {
        &groupby_cols
    } else {
        &sortby_cols
    };

    // apply groupby/aggregation as needed
    // if both sortby and groupby are empty, just returns original dataframe
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

fn write_csv(df: &mut DataFrame, outfile: &str) -> Result<(), OutputError> {
    let f = File::create(outfile)?;
    CsvWriter::new(f).finish(df)?;

    Ok(())
}

/// Write DataFrame as CSV to stdout
fn write_csv_to_stdout(df: &mut DataFrame) -> Result<(), OutputError> {
    let mut buf = Vec::new();
    CsvWriter::new(&mut buf).finish(df)?;
    print!("{}", String::from_utf8(buf)?);
    Ok(())
}

/// Write DataFrame as JSON to stdout
fn write_json_to_stdout(df: &mut DataFrame) -> Result<(), OutputError> {
    // Convert DataFrame to JSON array of objects
    let columns: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut records: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

    for row_idx in 0..df.height() {
        let mut record = serde_json::Map::new();
        for (col_idx, col_name) in columns.iter().enumerate() {
            let column = df.get_columns().get(col_idx).unwrap();
            let value = column.get(row_idx)?;
            let json_value = format!("{}", value);

            // Remove surrounding quotes if present (polars adds quotes to strings)
            let cleaned_value = json_value.trim_matches('"');

            // Try to parse as number, otherwise use string
            let json_val = if let Ok(num) = cleaned_value.parse::<i64>() {
                serde_json::Value::Number(serde_json::Number::from(num))
            } else if let Ok(num) = cleaned_value.parse::<f64>() {
                serde_json::Number::from_f64(num)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(cleaned_value.to_string()))
            } else if cleaned_value == "null" {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(cleaned_value.to_string())
            };
            record.insert(col_name.clone(), json_val);
        }
        records.push(record);
    }

    let json = serde_json::to_string(&records)?;
    println!("{}", json);
    Ok(())
}

/// Write DataFrame as JSON to file
fn write_json(df: &mut DataFrame, outfile: &str) -> Result<(), OutputError> {
    // Convert DataFrame to JSON array of objects
    let columns: Vec<String> = df
        .get_column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let mut records: Vec<serde_json::Map<String, serde_json::Value>> = Vec::new();

    for row_idx in 0..df.height() {
        let mut record = serde_json::Map::new();
        for (col_idx, col_name) in columns.iter().enumerate() {
            let column = df.get_columns().get(col_idx).unwrap();
            let value = column.get(row_idx)?;
            let json_value = format!("{}", value);

            // Remove surrounding quotes if present (polars adds quotes to strings)
            let cleaned_value = json_value.trim_matches('"');

            // Try to parse as number, otherwise use string
            let json_val = if let Ok(num) = cleaned_value.parse::<i64>() {
                serde_json::Value::Number(serde_json::Number::from(num))
            } else if let Ok(num) = cleaned_value.parse::<f64>() {
                serde_json::Number::from_f64(num)
                    .map(serde_json::Value::Number)
                    .unwrap_or_else(|| serde_json::Value::String(cleaned_value.to_string()))
            } else if cleaned_value == "null" {
                serde_json::Value::Null
            } else {
                serde_json::Value::String(cleaned_value.to_string())
            };
            record.insert(col_name.clone(), json_val);
        }
        records.push(record);
    }

    let f = File::create(outfile)?;
    serde_json::to_writer(f, &records)?;
    Ok(())
}

/// Handle output based on format and output file
fn output_dataframe(df: &mut DataFrame, format: &str, outfile: Option<String>) -> Result<(), OutputError> {
    match (format, outfile) {
        // File output
        (_, Some(path)) => {
            if format == "json" || path.ends_with(".json") {
                write_json(df, &path)?;
            } else {
                write_csv(df, &path)?;
            }
        }
        // STDOUT output
        ("csv", None) => write_csv_to_stdout(df)?,
        ("json", None) => write_json_to_stdout(df)?,
        ("table", None) | (_, None) => println!("{}", df),
    }
    Ok(())
}

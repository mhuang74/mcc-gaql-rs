use std::{
    fs::{self, File},
    process,
    time::Instant,
};

use anyhow::{Context, Result};
use futures::{stream::FuturesUnordered, StreamExt};

use googleads::GoogleAdsAPIAccess;
use googleads_rs::google::ads::googleads::v10::services::google_ads_service_client::GoogleAdsServiceClient;
use polars::prelude::*;
use thousands::Separable;
use tonic::{codegen::InterceptedService, transport::Channel};

mod args;
mod config;
mod googleads;
mod util;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    util::init_logger();

    let mut args = args::parse();

    let profile = &args.profile.unwrap_or_else(|| "test".to_owned());

    let config = config::load(profile).context(format!("Loading config for profile: {profile}"))?;

    log::debug!("Configuration: {config:?}");

    // load stored query
    if let Some(query_name) = args.stored_query {
        if let Some(query_filename) = config.queries_filename {
            let queries_path = crate::config::config_file_path(&query_filename).unwrap();

            args.gaql_query = match util::get_query_from_file(queries_path, &query_name).await {
                Ok(s) => Some(s),
                Err(e) => {
                    let msg = format!("Unable to load query: {e}");
                    log::error!("{msg}");
                    println!("{msg}");
                    process::exit(1);
                }
            }
        }
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
                fs::remove_file(token_cache_path)
                    .expect("Failed to remove token cache file to force re-auth");
                googleads::get_api_access(&config.mcc_customerid, &config.token_cache_filename)
                    .await
                    .expect("Refresh token expired and failed to kick off re-auth.")
            }
        };

    if args.list_child_accounts {
        // run Account listing query
        if args.customer_id.is_some() {
            // query accounts under specificied customer_id account
            let customer_id = args.customer_id.expect("Valid customer_id required.");
            log::debug!("Listing child accounts under {customer_id}");
            googleads::gaql_query(
                api_context,
                customer_id,
                googleads::SUB_ACCOUNTS_QUERY.to_owned(),
            )
            .await
            .unwrap();
        } else {
            // query child accounts under MCC
            log::debug!(
                "Listing ALL child accounts under MCC {}",
                &config.mcc_customerid
            );
            googleads::gaql_query(
                api_context,
                config.mcc_customerid,
                googleads::SUB_ACCOUNTS_QUERY.to_owned(),
            )
            .await
            .unwrap();
        }
    } else if args.field_service {
        let query = &args
            .gaql_query
            .expect("Valid Field Service query required.");
        log::info!("Running Fields Metadata query: {query}");
        googleads::fields_query(api_context, query).await;
    } else if args.gaql_query.is_some() {
        // run provided GAQL query
        if args.customer_id.is_some() {
            // query only specificied customer_id
            let query = args.gaql_query.expect("Valid GAQL query required.");
            let customer_id = args.customer_id.expect("Valid customer_id required.");
            log::info!("Running GAQL query for {customer_id}: {query}");

            let dataframe: Option<DataFrame> =
                match googleads::gaql_query(api_context, customer_id, query).await {
                    Ok(df) => {
                        if !args.groupby.is_empty() {
                            let agg_df = apply_groupby(df, args.groupby).await?;
                            Some(agg_df)
                        } else {
                            Some(df)
                        }
                    }
                    Err(e) => {
                        let msg = format!("Error: {e}");
                        println!("{msg}");
                        None
                    }
                };

            if dataframe.is_some() {
                if args.output.is_some() {
                    write_csv(&mut dataframe.unwrap(), args.output.as_ref().unwrap())?;
                } else {
                    println!("{:?}", &dataframe);
                }
            }
        } else {
            // get list of child account customer ids to query
            let customer_ids: Option<Vec<String>> = if args.all_current_child_accounts {
                // generate new list of child accounts
                match googleads::get_child_account_ids(api_context.clone(), config.mcc_customerid)
                    .await
                {
                    Ok(customer_ids) => Some(customer_ids),
                    Err(_e) => None,
                }
            } else if config.customerids_filename.is_some() {
                // load cild accounts list from file

                let customerids_path =
                    crate::config::config_file_path(&config.customerids_filename.unwrap()).unwrap();

                match util::get_child_account_ids_from_file(customerids_path.as_path()).await {
                    Ok(customer_ids) => Some(customer_ids),
                    Err(_e) => None,
                }
            } else {
                None
            };

            // apply query to all child account customer_ids
            if let Some(customer_id_vector) = customer_ids {
                let query: String = args.gaql_query.expect("valid GAQL query");

                // run queries asynchroughly across all customer_ids
                gaql_query_async(
                    api_context,
                    customer_id_vector,
                    query,
                    args.groupby,
                    args.output,
                )
                .await?;
            } else {
                log::error!("Abort GAQL query. Can't find child accounts to run on.");
            }
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

    let start = Instant::now();

    let mut total_rows: usize = 0;

    // collect asynchronous query results
    while let Some(result) = gaql_handles.next().await {
        match result {
            Ok(result) => {
                match result {
                    Ok(df) => {
                        if !df.is_empty() {
                            total_rows += df.height();

                            // log::debug!("Future returned non-empty GAQL results");
                            if !&groupby.is_empty() {
                                let my_groupby = groupby.clone();
                                // execute groupby in dedicated non-yielding thread
                                groupby_handles.push(tokio::task::spawn_blocking(|| {
                                    apply_groupby(df, my_groupby)
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
    log::debug!(
        "All queries returned {} rows in {} msec",
        total_rows.separate_with_commas(),
        duration.as_millis().separate_with_commas()
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

        // apply 2nd pass gropuby
        if !groupby.is_empty() {
            let start = Instant::now();

            dataframe = apply_groupby(dataframe, groupby).await?;

            let duration = start.elapsed();
            log::debug!(
                "applied 2nd pass groupby in {} msec",
                duration.as_millis().separate_with_commas()
            );
        }

        log::debug!("final dataframe shape: {:?}", dataframe.shape());

        if outfile.is_some() {
            let start = Instant::now();

            write_csv(&mut dataframe, &outfile.unwrap())?;

            let duration = start.elapsed();
            log::debug!(
                "csv written in {} msec",
                duration.as_millis().separate_with_commas()
            );
        } else {
            println!("{:?}", dataframe);
        }
    }

    Ok(())
}

async fn apply_groupby(df: DataFrame, groupby: Vec<String>) -> Result<DataFrame> {
    // get list of metrics columns for SELECT
    let metric_cols: Vec<&str> = df
        .get_column_names()
        .into_iter()
        .filter(|c| c.contains("metrics"))
        .collect();

    // log::debug!("summing selected metric columns: {:?}", &metric_cols);

    let df_agg = df
        .groupby(&groupby)?
        .select(&metric_cols)
        .sum()?
        .sort(&groupby, false)?;

    Ok(df_agg)
}

fn write_csv(df: &mut DataFrame, outfile: &str) -> Result<()> {
    let f = File::create(outfile)?;
    CsvWriter::new(f).finish(df)?;

    Ok(())
}

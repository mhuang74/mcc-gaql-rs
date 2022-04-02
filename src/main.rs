use std::process;

use anyhow::{Context, Result};
use googleads::GoogleAdsAPIAccess;
use googleads_rs::google::ads::googleads::v10::services::google_ads_service_client::GoogleAdsServiceClient;
use polars::prelude::*;
use tokio::sync::mpsc;
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
            log::debug!("Loading queries file: {queries_path:?}");

            args.gaql_query = match util::get_query_from_file(queries_path, &query_name).await {
                Ok(s) => Some(s),
                Err(e) => {
                    let msg = format!("Unable to load query: {}", e.to_string());
                    log::error!("{msg}");
                    println!("{msg}");
                    process::exit(1);
                }
            }
        }
    }

    let api_context =
        googleads::get_api_access(&config.mcc_customerid, &config.token_cache_filename)
            .await
            .expect("Failed to access Google Ads API.");

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
            .await.unwrap();
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
            .await.unwrap();
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
            let df = googleads::gaql_query(api_context, customer_id, query).await.unwrap();
            println!("df: {df}");
        } else {
            // get list of child account customer ids to query
            let customer_ids: Option<Vec<String>> = 
                if args.all_current_child_accounts {
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
                    log::debug!("Loading customerids file: {customerids_path:?}");

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
                gaql_query_async(api_context, customer_id_vector, query, args.aggregate_metrics).await?;

            } else {
                log::error!("Abort GAQL query. Can't find child accounts to run on.");
            }
        }
    } else {
        println!("Nothing to do.");
    }

    Ok(())
}

async fn gaql_query_async(mut api_context: GoogleAdsAPIAccess, customer_id_vector: Vec<String>, query: String, aggregate: bool) -> Result<()>
{

    log::info!(
        "Running GAQL query for {} child accounts: {}",
        &customer_id_vector.len(),
        &query
    );

    let mut google_ads_client: Option<
        GoogleAdsServiceClient<
            InterceptedService<Channel, googleads::GoogleAdsAPIAccess>,
        >,
    > = None;

    let mut handles: Vec<tokio::task::JoinHandle<_>> = Vec::new();

    for customer_id in customer_id_vector.iter() {
        // keep reusing same GoogleAdsServiceClient unless token is expired
        if google_ads_client.is_none() || api_context.renew_token().await? {
            log::debug!("Constructing new GoogleAdsServiceClient with new token.");
            google_ads_client = Some(GoogleAdsServiceClient::with_interceptor(
                api_context.channel.clone(),
                googleads::GoogleAdsAPIAccess {
                    auth_token: api_context.auth_token.clone(),
                    dev_token: api_context.dev_token.clone(),
                    login_customer: api_context.login_customer.clone(),
                    channel: api_context.channel.clone(),
                    token: api_context.token.clone(),
                    authenticator: api_context.authenticator.clone(),
                },
            ));
        }

        // log::debug!("Querying {customer_id}");

        let gaql_future = googleads::gaql_query_with_client(
            google_ads_client.as_ref().unwrap().clone(),
            customer_id.clone(),
            query.clone(),
        );

        // execute gaql query in background thread
        handles.push(tokio::spawn(gaql_future));
    }

    let mut dataframe: Option<DataFrame> = None;

    // KB: cannot exit FOR loop until all spawned futures are finished, otherwise Connection may get dropped prematurely
    // KB: all GoogleAdsServiceClients seem to share single Hyper Connection
    for handle in handles {
        match handle.await? {
            Ok(df) => {
                if !df.is_empty() {
                    if dataframe.as_ref().is_none() {
                        dataframe = Some(df);
                    } else {
                        dataframe.as_mut().unwrap().extend(&df)?;
                    }
                }
            }
            Err(e) => {
                log::error!("Error: {e}");
            }      
        }
    }



    if dataframe.is_some() {

        if aggregate {
            let df = dataframe.as_mut().unwrap();
            let _ = df.drop_in_place("campaign.id")?;
            let _ = df.drop_in_place("customer.currency_code")?;
            let _ = df.drop_in_place("metrics.cost_micros")?;
            let df_agg = df.groupby(["segments.date"])?
                                    // .select(["metrics.impressions", "metrics.clicks"])
                                    .sum()?
                                    .sort(["segments.date"], true)?;

            dataframe = Some(df_agg);


        } 
        
        println!("{:?}", dataframe.unwrap());
        
    }


    Ok(())

}

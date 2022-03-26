use std::process;

use anyhow::{Context, Result};
use googleads_rs::google::ads::googleads::v10::services::google_ads_service_client::GoogleAdsServiceClient;
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
                    log::error!("Unable to load query: {}", e.to_string());
                    process::exit(1);
                }
            }
        }
    }

    let mut api_context =
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
            .await;
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
            .await;
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
            googleads::gaql_query(api_context, customer_id, query).await;
        } else {
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
                log::debug!("Loading customerids file: {customerids_path:?}");

                match util::get_child_account_ids_from_file(customerids_path.as_path()).await {
                    Ok(customer_ids) => Some(customer_ids),
                    Err(_e) => None,
                }
            } else {
                None
            };

            // apply query to all child customer_id
            if let Some(customer_id_vector) = customer_ids {
                let query: String = args.gaql_query.expect("valid GAQL query");
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

                    // spawn requires captured values to have sufficient lifetime, so just clone them
                    let my_google_ads_client = google_ads_client.as_ref().unwrap().clone();
                    let my_customer_id = customer_id.clone();
                    let my_query: String = query.clone();

                    // log::debug!("Querying {customer_id}");

                    handles.push(tokio::spawn(async move {
                        googleads::gaql_query_with_client(
                            my_google_ads_client,
                            my_customer_id,
                            my_query,
                        )
                        .await;
                    }));
                }

                // KB: cannot exit FOR loop until all spawned queries are finished, otherwise Connection may get dropped prematurely
                // KB: all GoogleAdsServiceClients seem to share single Hyper Connection
                for handle in handles {
                    handle.await?;
                }
            } else {
                log::error!("Abort GAQL query. Can't find child accounts to run on.");
            }
        }
    } else {
        println!("Nothing to do.");
    }

    Ok(())
}

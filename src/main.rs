use anyhow::{Context, Result};

mod args;
mod config;
mod googleads;
mod util;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    util::init_logger();

    let args = args::parse();

    let profile = &args.profile.unwrap_or_else(|| "test".to_string());

    let config = config::load(profile).context(format!("Loading config for profile: {profile}"))?;

    log::info!("Configuration: {config:?}");

    let mut api_context =
        googleads::get_api_access(&config.mcc_customerid, &config.token_cache_filename)
            .await
            .expect("Failed to access Google Ads API.");

    if args.list_child_accounts {
        // run Account listing query
        if args.customer_id.is_some() {
            // query accounts under specificied customer_id account
            let customer_id = &args.customer_id.expect("Valid customer_id required.");
            log::info!("Listing child accounts under {customer_id}");
            googleads::gaql_query(
                &mut api_context,
                customer_id,
                googleads::SUB_ACCOUNTS_QUERY,
                googleads::print_to_stdout,
            )
            .await;
        } else {
            // query child accounts under MCC
            log::info!(
                "Listing ALL child accounts under MCC {}",
                &config.mcc_customerid
            );
            googleads::gaql_query(
                &mut api_context,
                &config.mcc_customerid,
                googleads::SUB_ACCOUNTS_QUERY,
                googleads::print_to_stdout,
            )
            .await;
        }
    } else if args.field_service {
        let query = &args
            .gaql_query
            .expect("Valid Field Service query required.");
        log::info!("Running Fields Metadata query: {query}");
        googleads::fields_query(&mut api_context, query).await;
    } else if args.gaql_query.is_some() {
        // run provided GAQL query
        if args.customer_id.is_some() {
            // query only specificied customer_id
            let query = &args.gaql_query.expect("Valid GAQL query required.");
            let customer_id = &args.customer_id.expect("Valid customer_id required.");
            log::info!("Running GAQL query for {customer_id}: {query}");
            googleads::gaql_query(&mut api_context, customer_id, query, googleads::print_to_stdout)
                .await;
        } else {
            let customer_ids: Option<Vec<String>> = if args.all_current_child_accounts {
                // generate new list of child accounts
                match googleads::get_child_account_ids(&mut api_context, &config.mcc_customerid).await {
                    Ok(customer_ids) => Some(customer_ids),
                    Err(_e) => None,
                }
            } else if config.customerids_filename.is_some() {
                // load cild accounts list from file

                let customerids_path =
                    crate::config::config_file_path(&config.customerids_filename.unwrap())
                        .expect("customerids path");
                log::info!("Loading customerids file: {customerids_path:?}");

                match util::get_child_account_ids_from_file(customerids_path.as_path()).await {
                    Ok(customer_ids) => Some(customer_ids),
                    Err(_e) => None,
                }
            } else {
                None
            };

            // apply query to all child customer_id
            if let Some(customer_id_vector) = customer_ids {
                let query = &args.gaql_query.as_ref().expect("valid GAQL query");
                log::info!(
                    "Running GAQL query for {} child accounts: {}",
                    customer_id_vector.len(),
                    query
                );

                for customer_id in customer_id_vector.iter() {
                    log::debug!("Querying {customer_id}");

                    googleads::gaql_query(
                        &mut api_context,
                        customer_id,
                        query,
                        googleads::print_to_stdout_no_header,
                    )
                    .await;
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

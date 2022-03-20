use anyhow::{bail, Result};
use clap::Parser;
use itertools::Itertools;
use std::env;
use tokio_stream::StreamExt;
use tonic::{
    codec::Streaming,
    metadata::{Ascii, MetadataValue},
    transport::Channel,
    Request,
};
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

use googleads_rs::google::ads::googleads::v10::services::{
    google_ads_field_service_client::GoogleAdsFieldServiceClient,
    google_ads_service_client::GoogleAdsServiceClient, SearchGoogleAdsFieldsRequest,
    SearchGoogleAdsFieldsResponse, SearchGoogleAdsStreamRequest, SearchGoogleAdsStreamResponse,
};

const ENDPOINT: &str = "https://googleads.googleapis.com:443";
// dev token borrowed from https://github.com/selesnow/rgoogleads/blob/master/R/gads_auth.R
const DEV_TOKEN: &str = "EBkkx-znu2cZcEY7e74smg";

const SUB_ACCOUNTS_QUERY: &str = "
SELECT
    customer_client.id,
    customer_client.level,
    customer_client.currency_code,
    customer_client.time_zone,
    customer_client.descriptive_name
FROM customer_client
WHERE
    customer_client.level <= 1
    and customer_client.manager = false
    and customer_client.status in ('ENABLED')
    and customer_client.descriptive_name is not null
ORDER BY customer_client.level, customer_client.id
";

const SUB_ACCOUNT_IDS_QUERY: &str = "
SELECT
    customer_client.id,
    customer_client.level
FROM customer_client
WHERE
    customer_client.level <= 1
    and customer_client.manager = false
    and customer_client.status in ('ENABLED')
    and customer_client.descriptive_name is not null
ORDER BY customer_client.level, customer_client.id
LIMIT 100
";

const CACHE_FILENAME: &str = ".mccfind/cache";
const CACHE_KEY_CHILD_ACCOUNTS: &str = "child-accounts";

static USAGE: &str = "
Find Google Ads accounts that match condition.

Runs GAQL queries against MCC account tree structure and return accounts that returned results.

If only <mcc-customer-id> is given, lists all accessible accounts under mcc account.

";

/// Find Google Ads accounts that match condition.
/// If only <mcc-customer-id> is given, lists all accessible accounts under mcc account.
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = USAGE)]
struct Cli {
    /// Query GoogleAdsFieldService to retrieve available fields
    #[clap(short, long)]
    field_service: bool,

    /// CustomerID of Google Ads MCC Manager Account matching OAuth login
    mcc_customer_id: String,

    /// CustomerID of Google Ads Account to query
    #[clap(short, long)]
    customer_id: Option<String>,

    /// Google Ads GAQL query to run
    gaql_query: Option<String>,
}

struct GoogleAdsAPIContext {
    channel: Channel,
    auth_token: MetadataValue<Ascii>,
    dev_token: MetadataValue<Ascii>,
    login_customer: MetadataValue<Ascii>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();

    let args = Cli::parse();

    let app_secret = yup_oauth2::read_application_secret("clientsecret.json")
        .await
        .expect("clientsecret.json");

    let auth =
        InstalledFlowAuthenticator::builder(app_secret, InstalledFlowReturnMethod::HTTPRedirect)
            .persist_tokens_to_disk("tokencache.json")
            .build()
            .await
            .unwrap();
    let scopes = &["https://www.googleapis.com/auth/adwords"];

    let access_token = match auth.token(scopes).await {
        Err(e) => {
            panic!("error: {:?}", e);
        }
        Ok(t) => t.as_str().to_owned(),
    };

    let bearer_token = format!("Bearer {}", access_token);
    let header_value_auth_token = MetadataValue::from_str(&bearer_token)?;
    let header_value_dev_token = MetadataValue::from_str(DEV_TOKEN)?;
    let header_value_login_customer = MetadataValue::from_str(&args.mcc_customer_id)?;

    let channel: Channel = Channel::from_static(ENDPOINT).connect().await?;

    let api_context: GoogleAdsAPIContext = GoogleAdsAPIContext {
        channel,
        auth_token: header_value_auth_token,
        dev_token: header_value_dev_token,
        login_customer: header_value_login_customer,
    };

    let print_to_stdout = |response: SearchGoogleAdsStreamResponse| {
        let field_mask = response.field_mask.unwrap();
        let headers = &field_mask.paths.iter().map(ToString::to_string).join("\t");
        println!("{headers}");

        for row in response.results {
            for path in &field_mask.paths {
                print!("{}\t", row.get(path));
            }
            println!();
        }

        None
    };

    let print_to_stdout_no_header = |response: SearchGoogleAdsStreamResponse| {
        let field_mask = response.field_mask.unwrap();

        for row in response.results {
            for path in &field_mask.paths {
                print!("{}\t", row.get(path));
            }
            println!();
        }

        None
    };

    if args.field_service {
        let query = &args.gaql_query.expect("valid Field Service query");
        log::info!("Running Fields Metadata query: {query}");
        fields_query(&api_context, query).await;
    } else if args.gaql_query.is_some() {
        // run provided GAQL query
        if args.customer_id.is_some() {
            // query only specificied customer_id
            let query = &args.gaql_query.expect("valid GAQL query");
            let customer_id = &args.customer_id.expect("valid customer_id");
            log::info!("Running GAQL query for {customer_id}: {query}");
            gaql_query(&api_context, customer_id, query, print_to_stdout).await;
        } else {
            // try read child account ids from cache
            let mut customer_ids: Option<Vec<String>> =
                match cacache::read(CACHE_FILENAME, CACHE_KEY_CHILD_ACCOUNTS).await {
                    Ok(encoded) => match bincode::deserialize(&encoded) {
                        Ok(decoded) => {
                            let v: Vec<String> = decoded;
                            log::info!(
                                "Successfully retrieved cached child accounts of size {}",
                                v.len()
                            );
                            Some(v)
                        }
                        Err(e) => {
                            log::error!(
                                "Unable to deserialize child accounts cache: {}",
                                e.to_string()
                            );
                            None
                        }
                    },
                    Err(e) => {
                        log::info!("Unable to read child accounts cache: {}", e.to_string());
                        None
                    }
                };

            if customer_ids.is_none() {
                // generate new list of child accounts
                customer_ids = match get_child_account_ids(&api_context, &args.mcc_customer_id)
                    .await
                {
                    Ok(customer_ids) => {
                        // save child accounts to cache
                        let encoded = bincode::serialize(&customer_ids).unwrap();
                        cacache::write(CACHE_FILENAME, CACHE_KEY_CHILD_ACCOUNTS, &encoded).await?;

                        log::info!("Adding {} child account ids to cache", customer_ids.len());

                        Some(customer_ids)
                    }
                    Err(_e) => None,
                };
            }

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

                    gaql_query(&api_context, customer_id, query, print_to_stdout_no_header).await;
                }
            } else {
                log::error!("Abort GAQL query. Can't find child accounts to run on.");
            }
        }
    } else {
        // run Account listing query
        if args.customer_id.is_some() {
            // query accounts under specificied customer_id account
            let customer_id = &args.customer_id.expect("valid customer_id");
            log::info!("Listing child accounts under {customer_id}");
            gaql_query(
                &api_context,
                customer_id,
                SUB_ACCOUNTS_QUERY,
                print_to_stdout,
            )
            .await;
        } else {
            // query accounts under MCC
            log::info!("Listing child accounts under MCC {}", &args.mcc_customer_id);
            gaql_query(
                &api_context,
                &args.mcc_customer_id,
                SUB_ACCOUNTS_QUERY,
                print_to_stdout,
            )
            .await;
        }
    }

    Ok(())
}

async fn get_child_account_ids(
    api_context: &GoogleAdsAPIContext,
    mcc_customer_id: &str,
) -> Result<Vec<String>> {
    let result: Option<Vec<String>> = gaql_query(
        api_context,
        mcc_customer_id,
        SUB_ACCOUNT_IDS_QUERY,
        |response: SearchGoogleAdsStreamResponse| {
            let mut customer_ids: Vec<String> = Vec::new();

            for row in response.results {
                let customer_id = row.get("customer_client.id");
                customer_ids.push(customer_id);
            }

            Some(customer_ids)
        },
    )
    .await;

    if let Some(customer_ids) = result {
        Ok(customer_ids)
    } else {
        bail!("Unable to query for child account ids");
    }
}

/// Run query via GoogleAdsServiceClient to get performance data
/// f: closure called with search Response
async fn gaql_query<F>(
    api_context: &GoogleAdsAPIContext,
    customer_id: &str,
    query: &str,
    f: F,
) -> Option<Vec<String>>
where
    F: Fn(SearchGoogleAdsStreamResponse) -> Option<Vec<String>>,
{
    let mut client = GoogleAdsServiceClient::with_interceptor(
        api_context.channel.clone(),
        move |mut req: Request<()>| {
            req.metadata_mut()
                .insert("authorization", api_context.auth_token.clone());
            req.metadata_mut()
                .insert("developer-token", api_context.dev_token.clone());
            req.metadata_mut()
                .insert("login-customer-id", api_context.login_customer.clone());
            Ok(req)
        },
    );

    let mut stream: Streaming<SearchGoogleAdsStreamResponse> = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: customer_id.to_owned(),
            query: query.to_owned(),
            summary_row_setting: 0,
        })
        .await
        .unwrap()
        .into_inner();

    let mut results: Vec<String> = Vec::new();

    while let Some(batch) = stream.next().await {
        match batch {
            Ok(response) => {
                if let Some(mut partial_results) = f(response) {
                    results.append(&mut partial_results);
                }
            }
            Err(e) => {
                log::error!("GAQL error for account {customer_id}: {}", e.message());
            }
        }
    }

    Some(results)
}

/// Run query via GoogleAdsFieldService to obtain field metadata
async fn fields_query(api_context: &GoogleAdsAPIContext, query: &str) {
    let mut client = GoogleAdsFieldServiceClient::with_interceptor(
        api_context.channel.clone(),
        move |mut req: Request<()>| {
            req.metadata_mut()
                .insert("authorization", api_context.auth_token.clone());
            req.metadata_mut()
                .insert("developer-token", api_context.dev_token.clone());
            req.metadata_mut()
                .insert("login-customer-id", api_context.login_customer.clone());
            Ok(req)
        },
    );

    let response: SearchGoogleAdsFieldsResponse = client
        .search_google_ads_fields(SearchGoogleAdsFieldsRequest {
            query: query.to_owned(),
            page_token: String::new(),
            page_size: 10000,
        })
        .await
        .unwrap()
        .into_inner();

    for field in response.results {
        println!("{:?}", &field);
    }
}

pub fn init_logger() {
    use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

    let mccfind_log_env = env::var("MCCFIND_LOG_LEVEL").unwrap_or_else(|_| "off".to_string());
    let mccfind_log_dir = env::var("MCCFIND_LOG_DIR").unwrap_or_else(|_| ".".to_string());

    Logger::try_with_env_or_str(mccfind_log_env)
        .unwrap()
        .use_utc()
        .log_to_file(
            FileSpec::default()
                .directory(mccfind_log_dir)
                .suppress_timestamp(),
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

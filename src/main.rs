
use docopt::Docopt;
use serde::Deserialize;
use tokio_stream::StreamExt;
use tonic::{
    metadata::{MetadataValue, Ascii},
    transport::Channel,
    Request,
    codec::Streaming
};
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

use googleads_rs::google::ads::googleads::v10::services::{
    SearchGoogleAdsStreamRequest,
    SearchGoogleAdsStreamResponse,
    google_ads_service_client::GoogleAdsServiceClient,
    SearchGoogleAdsFieldsRequest,
    SearchGoogleAdsFieldsResponse,
    google_ads_field_service_client::GoogleAdsFieldServiceClient
};

use itertools::Itertools;

const ENDPOINT: &str = "https://googleads.googleapis.com:443";
// from https://github.com/selesnow/rgoogleads/blob/master/R/gads_auth.R
const DEV_TOKEN: &str = "EBkkx-znu2cZcEY7e74smg";

// const SUB_ACCOUNT_QUERY: &str = "
// SELECT
//     customer_client.client_customer,
//     customer_client.level,
//     customer_client.manager,
//     customer_client.descriptive_name,
//     customer_client.currency_code,
//     customer_client.time_zone,
//     customer_client.id
// FROM customer_client
// WHERE
//     customer_client.level <= 2
// ";


static USAGE: &str = "
Find Google Ads accounts that match condition.

Runs GAQL queries against MCC account tree structure and return accounts that returned results.

Usage:
    mccfind [options] <mcc-customer-id> <customer-id> <query>

Options:
    -f, --google-ads-field-service  Run queries via GoogleAdsFieldService to retrieve available fields
    -h, --help                      Display this message
";

#[derive(Deserialize, Debug)]
struct Args {
    flag_google_ads_field_service: Option<bool>,
    arg_mcc_customer_id: String,
    arg_customer_id: String,
    arg_query: String
}

struct GoogleAdsAPIContext {
    channel: Channel,
    auth_token: MetadataValue<Ascii>,
    dev_token: MetadataValue<Ascii>,
    login_customer: MetadataValue<Ascii>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let args: Args = Docopt::new(USAGE)
        .unwrap_or_else(|e| e.exit())
        .parse()
        .unwrap_or_else(|e| e.exit())
        .deserialize()
        .unwrap_or_else(|e| e.exit());

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
        Ok(t) => {
            t.as_str().to_owned()
        }
    };

    let bearer_token = format!("Bearer {}", access_token);
    let header_value_auth_token = MetadataValue::from_str(&bearer_token)?;
    let header_value_dev_token = MetadataValue::from_str(DEV_TOKEN)?;
    let header_value_login_customer = MetadataValue::from_str(&args.arg_mcc_customer_id)?;

    let channel: Channel = Channel::from_static(ENDPOINT)
        .connect()
        .await?;

    let api_context: GoogleAdsAPIContext = GoogleAdsAPIContext {
        channel: channel,
        auth_token: header_value_auth_token,
        dev_token: header_value_dev_token,
        login_customer: header_value_login_customer
    };

    let field_flag = args.flag_google_ads_field_service.unwrap_or(false);

    if  field_flag {

            fields_query(&api_context, &args.arg_query).await;

    } else {

        gaql_query(
            &api_context,
            &args.arg_customer_id,
            &args.arg_query
        ).await;

    }

    Ok(())
}


async fn fields_query(
    api_context: &GoogleAdsAPIContext,
    query: &str
) {

    let mut client = GoogleAdsFieldServiceClient::with_interceptor(api_context.channel.clone(), move |mut req: Request<()>| {
        req.metadata_mut()
            .insert("authorization", api_context.auth_token.clone());
        req.metadata_mut()
            .insert("developer-token", api_context.dev_token.clone());
        req.metadata_mut()
            .insert("login-customer-id", api_context.login_customer.clone());
        Ok(req)
    });

    let response: SearchGoogleAdsFieldsResponse = client
        .search_google_ads_fields(SearchGoogleAdsFieldsRequest {
            query: query.to_owned(),
            page_token: String::new(),
            page_size: 10000
        })
        .await
        .unwrap()
        .into_inner();

    for field in response.results {

        println!("{:?}",
            &field
        );

    }
}

async fn gaql_query(
    api_context: &GoogleAdsAPIContext,
    customer_id: &str,
    query: &str
) {

    let mut client = GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), move |mut req: Request<()>| {
        req.metadata_mut()
            .insert("authorization", api_context.auth_token.clone());
        req.metadata_mut()
            .insert("developer-token", api_context.dev_token.clone());
        req.metadata_mut()
            .insert("login-customer-id", api_context.login_customer.clone());
        Ok(req)
    });

    let mut stream: Streaming<SearchGoogleAdsStreamResponse> = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: customer_id.to_owned(),
            query: query.to_owned(),
            summary_row_setting: 0
        })
        .await
        .unwrap()
        .into_inner();

    while let Some(batch) = stream.next().await {
        let response: SearchGoogleAdsStreamResponse = batch.unwrap();
        // println!("response: {:?}", &response);

        let field_mask = response.field_mask.unwrap();

        let headers = &field_mask.paths
            .iter()
            .map(ToString::to_string)
            .join("\t");
        println!("Headers: {headers}");

        let mut i=0;
        for row in response.results {
            i += 1;
            print!("{i}: ");
            for path in &field_mask.paths {
                print!("{}\t", row.get(&path));
            }
            print!("\n");
        }

    }
}



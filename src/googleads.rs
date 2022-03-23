use anyhow::{bail, Result};
use tokio_stream::StreamExt;
use tonic::{
    codec::Streaming,
    codegen::InterceptedService,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    transport::Channel,
    Status,
};
use yup_oauth2::{
    authenticator::{Authenticator, DefaultHyperClient, HyperClientBuilder},
    AccessToken, ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod,
};

use googleads_rs::google::ads::googleads::v10::services::google_ads_field_service_client::GoogleAdsFieldServiceClient;
use googleads_rs::google::ads::googleads::v10::services::google_ads_service_client::GoogleAdsServiceClient;
use googleads_rs::google::ads::googleads::v10::services::{
    SearchGoogleAdsFieldsRequest, SearchGoogleAdsFieldsResponse, SearchGoogleAdsStreamRequest,
    SearchGoogleAdsStreamResponse,
};

use itertools::Itertools;

pub const SUB_ACCOUNTS_QUERY: &str = "
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

pub const SUB_ACCOUNT_IDS_QUERY: &str = "
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
LIMIT 200
";

const ENDPOINT: &str = "https://googleads.googleapis.com:443";
// dev token borrowed from https://github.com/selesnow/rgoogleads/blob/master/R/gads_auth.R
const DEV_TOKEN: &str = "EBkkx-znu2cZcEY7e74smg";

const FILENAME_CLIENT_SECRET: &str = "clientsecret.json";
// const FILENAME_TOKEN_CACHE: &str = "tokencache.json";
static GOOGLE_ADS_API_SCOPE: &str = "https://www.googleapis.com/auth/adwords";

pub struct GoogleAdsAPIAccess {
    pub channel: Channel,
    pub dev_token: MetadataValue<Ascii>,
    pub login_customer: MetadataValue<Ascii>,
    pub auth_token: Option<MetadataValue<Ascii>>,
    pub token: Option<AccessToken>,
    pub authenticator: Authenticator<<DefaultHyperClient as HyperClientBuilder>::Connector>,
}

impl GoogleAdsAPIAccess {
    /// Renews Access Token if none exists or if almost expired
    /// returns True if token renewed
    pub async fn renew_token(&mut self) -> Result<bool> {
        let mut renewed: bool = false;
        if self.token.is_none() || self.token.as_ref().unwrap().is_expired() {
            self.token = match self.authenticator.token(&[GOOGLE_ADS_API_SCOPE]).await {
                Err(e) => {
                    bail!("failed to get access token: {:?}", e);
                }
                Ok(t) => {
                    log::debug!("Obtained access token: {t:?}");

                    let bearer_token = format!("Bearer {}", t.as_str());
                    let header_value_auth_token = MetadataValue::from_str(&bearer_token)?;
                    self.auth_token = Some(header_value_auth_token);

                    renewed = true;
                    Some(t)
                }
            };
        }
        Ok(renewed)
    }
}

impl Interceptor for GoogleAdsAPIAccess {
    fn call(&mut self, mut request: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
        request
            .metadata_mut()
            .insert("authorization", self.auth_token.as_ref().unwrap().clone());
        request
            .metadata_mut()
            .insert("developer-token", self.dev_token.clone());
        request
            .metadata_mut()
            .insert("login-customer-id", self.login_customer.clone());

        Ok(request)
    }
}

/// Get access to Google Ads API via OAuth2 flow and return API Credentials
pub async fn get_api_access(
    mcc_customer_id: &str,
    token_cache_filename: &str,
) -> Result<GoogleAdsAPIAccess> {
    let client_secret_path =
        crate::config::config_file_path(FILENAME_CLIENT_SECRET).expect("clientsecret path");

    let app_secret: ApplicationSecret =
        yup_oauth2::read_application_secret(client_secret_path.as_path())
            .await
            .expect("clientsecret.json");

    let token_cache_path =
        crate::config::config_file_path(token_cache_filename).expect("token cache path");

    let auth: Authenticator<<DefaultHyperClient as HyperClientBuilder>::Connector> =
        InstalledFlowAuthenticator::builder(app_secret, InstalledFlowReturnMethod::HTTPRedirect)
            .persist_tokens_to_disk(token_cache_path.as_path())
            .build()
            .await?;

    let header_value_dev_token = MetadataValue::from_str(DEV_TOKEN)?;
    let header_value_login_customer = MetadataValue::from_str(mcc_customer_id)?;

    let channel: Channel = Channel::from_static(ENDPOINT).connect().await?;

    let mut access = GoogleAdsAPIAccess {
        channel,
        dev_token: header_value_dev_token,
        login_customer: header_value_login_customer,
        auth_token: None,
        token: None,
        authenticator: auth,
    };

    access
        .renew_token()
        .await
        .expect("Failed to refresh access token");

    Ok(access)
}

/// Run query via GoogleAdsServiceClient to get performance data
/// f: closure called with search Response
pub async fn gaql_query_with_client<F>(
    client: &mut GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>>,
    customer_id: &str,
    query: &str,
    f: F,
) -> Option<Vec<String>>
where
    F: Fn(SearchGoogleAdsStreamResponse) -> Option<Vec<String>>,
{
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

/// Run query via GoogleAdsServiceClient to get performance data
/// f: closure called with search Response
pub async fn gaql_query<F>(
    api_context: &GoogleAdsAPIAccess,
    customer_id: &str,
    query: &str,
    f: F,
) -> Option<Vec<String>>
where
    F: Fn(SearchGoogleAdsStreamResponse) -> Option<Vec<String>>,
{
    let mut client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(
            api_context.channel.clone(),
            GoogleAdsAPIAccess {
                auth_token: api_context.auth_token.clone(),
                dev_token: api_context.dev_token.clone(),
                login_customer: api_context.login_customer.clone(),
                channel: api_context.channel.clone(),
                token: api_context.token.clone(),
                authenticator: api_context.authenticator.clone(),
            },
        );

    gaql_query_with_client(&mut client, customer_id, query, f).await
}

/// Run query via GoogleAdsFieldService to obtain field metadata
pub async fn fields_query(api_context: &mut GoogleAdsAPIAccess, query: &str) {
    let mut client = GoogleAdsFieldServiceClient::with_interceptor(
        api_context.channel.clone(),
        GoogleAdsAPIAccess {
            auth_token: api_context.auth_token.clone(),
            dev_token: api_context.dev_token.clone(),
            login_customer: api_context.login_customer.clone(),
            channel: api_context.channel.clone(),
            token: api_context.token.clone(),
            authenticator: api_context.authenticator.clone(),
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

pub async fn get_child_account_ids(
    api_context: &mut GoogleAdsAPIAccess,
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

            log::debug!(
                "Retrieved {} customer_ids via MCC query: {}",
                customer_ids.len(),
                SUB_ACCOUNT_IDS_QUERY
            );

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

pub fn print_to_stdout(response: SearchGoogleAdsStreamResponse) -> Option<Vec<String>> {
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
}

pub fn print_to_stdout_no_header(response: SearchGoogleAdsStreamResponse) -> Option<Vec<String>> {
    let field_mask = response.field_mask.unwrap();

    for row in response.results {
        for path in &field_mask.paths {
            print!("{}\t", row.get(path));
        }
        println!();
    }

    None
}

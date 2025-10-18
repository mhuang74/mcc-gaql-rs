use std::time::Duration;

use anyhow::{bail, Result};
use polars::prelude::*;
use tokio_stream::StreamExt;
use tonic::{
    codegen::InterceptedService,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    transport::Channel,
    Response, Status, Streaming,
};
use yup_oauth2::{
    authenticator::{Authenticator, DefaultHyperClient, HyperClientBuilder},
    AccessToken, ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod,
};

use googleads_rs::google::ads::googleads::v22::services::google_ads_field_service_client::GoogleAdsFieldServiceClient;
use googleads_rs::google::ads::googleads::v22::services::google_ads_service_client::GoogleAdsServiceClient;
use googleads_rs::google::ads::googleads::v22::services::{
    GoogleAdsRow, SearchGoogleAdsFieldsRequest, SearchGoogleAdsFieldsResponse,
    SearchGoogleAdsStreamRequest, SearchGoogleAdsStreamResponse,
};

use async_std::io::WriteExt;

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
";

const ENDPOINT: &str = "https://googleads.googleapis.com:443";
// dev token borrowed from https://github.com/selesnow/rgoogleads/blob/master/R/gads_auth.R
const DEV_TOKEN: &str = "EBkkx-znu2cZcEY7e74smg";

const FILENAME_CLIENT_SECRET: &str = "clientsecret.json";
// const FILENAME_TOKEN_CACHE: &str = "tokencache.json";
static GOOGLE_ADS_API_SCOPE: &str = "https://www.googleapis.com/auth/adwords";

// incomplete. Only what I need for the moment.
const GOOGLE_ADS_METRICS_INTEGER_FIELDS: &[&str] = &[
    "clicks",
    "cost_micros",
    "engagements",
    "historical_creative_quality_score",
    "historical_quality_score",
    "impressions",
    "interactions",
    "invalid_clicks",
    "organic_clicks",
    "organic_impressions",
    "organic_queries",
    "video_views",
    "view_through_conversions",
];

#[derive(Clone)]
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
            self.token = match self
                .authenticator
                .force_refreshed_token(&[GOOGLE_ADS_API_SCOPE])
                .await
            {
                Err(e) => {
                    bail!("failed to get access token: {:?}", e);
                }
                Ok(t) => {
                    log::debug!("Obtained access token: {t:?}");

                    let bearer_token = format!("Bearer {}", t.as_str());
                    let header_value_auth_token = MetadataValue::try_from(&bearer_token)?;
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

    let header_value_dev_token = MetadataValue::try_from(DEV_TOKEN)?;
    let header_value_login_customer = MetadataValue::try_from(mcc_customer_id)?;

    let tls_config = tonic::transport::ClientTlsConfig::new()
        .with_native_roots();

    let channel: Channel = Channel::from_static(ENDPOINT)
        .tls_config(tls_config)?
        .rate_limit(100, Duration::from_secs(1))
        .concurrency_limit(100)
        .connect()
        .await?;

    let mut access = GoogleAdsAPIAccess {
        channel,
        dev_token: header_value_dev_token,
        login_customer: header_value_login_customer,
        auth_token: None,
        token: None,
        authenticator: auth,
    };

    access.renew_token().await?;

    Ok(access)
}

/// Run query via GoogleAdsServiceClient to get performance data
pub async fn gaql_query_with_client(
    mut client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>>,
    customer_id: String,
    query: String,
) -> Result<(DataFrame, i64)> {
    let result: Result<Response<Streaming<SearchGoogleAdsStreamResponse>>, Status> = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: customer_id.clone(),
            query,
            summary_row_setting: 0,
        })
        .await;

    let (df, total_api_consumption) = match result {
        Ok(response) => {
            let mut stream = response.into_inner();

            let mut columns: Vec<Vec<String>> = Vec::new();
            let mut headers: Option<Vec<String>> = None;
            let mut api_consumption: i64 = 0;

            while let Some(item) = stream.next().await {
                match item {
                    Ok(stream_response) => {
                        // aggregate api consumption
                        api_consumption += stream_response.query_resource_consumption;

                        let field_mask = stream_response.field_mask.unwrap();
                        if headers.is_none() {
                            headers = Some(field_mask.paths.clone());
                        }
                        for r in stream_response.results {
                            let row: GoogleAdsRow = r;

                            // go through all columns specified in query, pull out string value, and insert into columns
                            for i in 0..headers.as_ref().unwrap().len() {
                                let path = &headers.as_ref().unwrap()[i];
                                let string_val: String =
                                    row.get(path).trim_matches('"').to_string();
                                match columns.get_mut(i) {
                                    Some(v) => {
                                        v.push(string_val);
                                    }
                                    None => {
                                        let v: Vec<String> = vec![string_val];
                                        columns.insert(i, v);
                                    }
                                }
                            }
                        }
                    }
                    Err(status) => {
                        let error_details = String::from_utf8_lossy(status.details())
                            .trim()
                            .replace(|c: char| !c.is_ascii(), "")
                            .replace("%", " ")
                            .replace("\n", " ")
                            .replace("\r", " ");

                        bail!(
                            "GoogleAdsClient streaming error. Account: {customer_id}, Message: '{}', Details: '{}'",
                            status.message(),
                            error_details
                        );
                    }
                }
            }

            let mut series_vec: Vec<Series> = Vec::new();

            // convert columnar values (String) into Polars Series with right datatype
            //  - metric columns could be Integer or Float
            //  - other columns are String
            if let Some(headers_vec) = headers {
                for (i, header) in headers_vec.iter().enumerate() {
                    if header.starts_with("metrics") {
                        if GOOGLE_ADS_METRICS_INTEGER_FIELDS
                            .iter()
                            .any(|f| f == header)
                        {
                            let v: Vec<u64> = columns
                                .get(i)
                                .map(|col| {
                                    col.iter()
                                        .map(|x| x.parse::<u64>().unwrap())
                                        .collect()
                                })
                                .unwrap_or_default();
                            series_vec.push(Series::new(header, v));
                        } else {
                            let v: Vec<f64> = columns
                                .get(i)
                                .map(|col| {
                                    col.iter()
                                        .map(|x| x.parse::<f64>().unwrap())
                                        .collect()
                                })
                                .unwrap_or_default();
                            series_vec.push(Series::new(header, v));
                        }
                    } else {
                        let v: Vec<String> = columns
                            .get(i)
                            .map(|col| col.clone())
                            .unwrap_or_default();
                        series_vec.push(Series::new(header, v));
                    };
                }
            }

            let df = DataFrame::new(series_vec).unwrap();

            (df, api_consumption)
        }
        Err(status) => {
            bail!(
                "GoogleAdsClient request error. Account: {customer_id}, Message: {}, Details: {}",
                status.message(),
                String::from_utf8_lossy(status.details()).into_owned()
            );
        }
    };

    Ok((df, total_api_consumption))
}

/// Run query via GoogleAdsServiceClient to get performance data
pub async fn gaql_query(
    api_context: GoogleAdsAPIAccess,
    customer_id: String,
    query: String,
) -> Result<(DataFrame, i64)> {
    let client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    gaql_query_with_client(client, customer_id, query).await
}

/// Run query via GoogleAdsFieldService to obtain field metadata
pub async fn fields_query(api_context: GoogleAdsAPIAccess, query: &str) {
    let mut client =
        GoogleAdsFieldServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    let response: SearchGoogleAdsFieldsResponse = client
        .search_google_ads_fields(SearchGoogleAdsFieldsRequest {
            query: query.to_owned(),
            page_token: String::new(),
            page_size: 10000,
        })
        .await
        .unwrap()
        .into_inner();

    let mut stdout = async_std::io::stdout();
    for row in response.results {
        let val = format!(
            "{}\t{:?}\t{}\t{}\t{:?}\n",
            row.name,
            row.category(),
            row.selectable,
            row.filterable,
            row.selectable_with,
        );
        stdout.write_all(val.as_bytes()).await.unwrap();
    }
}

pub async fn get_child_account_ids(
    api_context: GoogleAdsAPIAccess,
    mcc_customer_id: String,
) -> Result<Vec<String>> {
    let mut client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    let result: Result<Response<Streaming<SearchGoogleAdsStreamResponse>>, Status> = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: mcc_customer_id.clone(),
            query: SUB_ACCOUNT_IDS_QUERY.to_string(),
            summary_row_setting: 0,
        })
        .await;

    let customer_ids: Option<Vec<String>> = match result {
        Ok(response) => {
            let mut stream = response.into_inner();

            let mut v: Vec<String> = Vec::with_capacity(2048);

            while let Some(item) = stream.next().await {
                match item {
                    Ok(stream_response) => {
                        for row in stream_response.results {
                            v.push(row.get("customer_client.id"));
                        }
                    }
                    Err(status) => {
                        bail!(format!(
                            "Unable to query for child account ids: {}",
                            status.message()
                        ));
                    }
                }
            }

            log::debug!(
                "Retrieved {} child account ids from Manager Account {}",
                &v.len(),
                &mcc_customer_id
            );

            Some(v)
        }
        Err(status) => {
            bail!(format!(
                "Unable to query for child account ids: {}",
                status.message()
            ));
        }
    };

    Ok(customer_ids.unwrap())
}

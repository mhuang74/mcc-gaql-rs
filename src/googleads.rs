use std::time::Duration;

use anyhow::{Result, bail};
use polars::prelude::*;
use tokio_stream::StreamExt;
use tonic::{
    Response, Status, Streaming,
    codegen::InterceptedService,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    transport::Channel,
};
use yup_oauth2::{
    AccessToken, ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod,
    authenticator::{Authenticator, DefaultHyperClient, HyperClientBuilder},
};

use googleads_rs::google::ads::googleads::v23::services::google_ads_field_service_client::GoogleAdsFieldServiceClient;
use googleads_rs::google::ads::googleads::v23::services::google_ads_service_client::GoogleAdsServiceClient;
use googleads_rs::google::ads::googleads::v23::services::{
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

// Developer Token configuration with priority order:
// 1. Config: Pass via dev_token parameter (from config file)
// 2. Runtime: Check MCC_GAQL_DEV_TOKEN env var at runtime
// 3. Compile-time: Set MCC_GAQL_DEV_TOKEN env var during build
//
// Get your own dev token at: https://developers.google.com/google-ads/api/docs/get-started/dev-token
const EMBEDDED_DEV_TOKEN: Option<&str> = option_env!("MCC_GAQL_DEV_TOKEN");

const FILENAME_CLIENT_SECRET: &str = "clientsecret.json";
static GOOGLE_ADS_API_SCOPE: &str = "https://www.googleapis.com/auth/adwords";

// Embed the client secret at compile time if the file exists
// Place clientsecret.json in the project root directory before building
// If not present at compile time, the code will fall back to loading from config directory at runtime
#[cfg(not(feature = "external_client_secret"))]
const EMBEDDED_CLIENT_SECRET: Option<&str> = option_env!("MCC_GAQL_EMBED_CLIENT_SECRET");

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
    #[allow(dead_code)]
    pub user_email: Option<String>,
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

/// Generate token cache filename from user email
/// Sanitizes email by replacing @ with _at_ and . with _
/// Example: user@example.com -> tokencache_user_at_example_com.json
pub fn generate_token_cache_filename(user_email: &str) -> String {
    let sanitized = user_email.replace('@', "_at_").replace('.', "_");
    format!("tokencache_{}.json", sanitized)
}

/// Get developer token with priority order:
/// 1. Provided parameter (from config file)
/// 2. Runtime environment variable MCC_GAQL_DEV_TOKEN
/// 3. Compile-time embedded token
///
/// Returns error if no token is available from any source
fn get_dev_token(config_token: Option<&str>) -> Result<String> {
    if let Some(token) = config_token {
        log::debug!("Using developer token from config");
        return Ok(token.to_string());
    }

    if let Ok(token) = std::env::var("MCC_GAQL_DEV_TOKEN") {
        log::debug!("Using developer token from runtime environment variable");
        return Ok(token);
    }

    if let Some(token) = EMBEDDED_DEV_TOKEN {
        log::debug!("Using developer token embedded at compile time");
        return Ok(token.to_string());
    }

    bail!(
        "Google Ads Developer Token required but not found. Provide via:\n  \
         1. Config file: Add 'dev_token = \"YOUR_TOKEN\"' to your profile\n  \
         2. Runtime env: export MCC_GAQL_DEV_TOKEN=\"YOUR_TOKEN\"\n  \
         3. Build time: MCC_GAQL_DEV_TOKEN=\"YOUR_TOKEN\" cargo build\n\n  \
         Get your developer token at:\n  \
         https://developers.google.com/google-ads/api/docs/get-started/dev-token"
    )
}

/// Get access to Google Ads API via OAuth2 flow and return API Credentials
pub async fn get_api_access(
    mcc_customer_id: &str,
    token_cache_filename: &str,
    user_email: Option<&str>,
    dev_token: Option<&str>,
) -> Result<GoogleAdsAPIAccess> {
    // Try embedded secret first (if compiled with credentials), then fall back to file
    #[cfg(not(feature = "external_client_secret"))]
    let app_secret: ApplicationSecret = if let Some(embedded_json) = EMBEDDED_CLIENT_SECRET {
        log::debug!("Using embedded client secret");
        yup_oauth2::parse_application_secret(embedded_json)
            .expect("Failed to parse embedded client secret")
    } else {
        log::debug!("No embedded client secret found, loading from file");
        let client_secret_path =
            crate::config::config_file_path(FILENAME_CLIENT_SECRET).expect("clientsecret path");
        yup_oauth2::read_application_secret(client_secret_path.as_path())
            .await
            .expect("clientsecret.json file not found and no embedded secret available")
    };

    // For builds with external_client_secret feature, always load from file
    #[cfg(feature = "external_client_secret")]
    let app_secret: ApplicationSecret = {
        log::debug!("Loading client secret from file (external_client_secret feature enabled)");
        let client_secret_path =
            crate::config::config_file_path(FILENAME_CLIENT_SECRET).expect("clientsecret path");
        yup_oauth2::read_application_secret(client_secret_path.as_path())
            .await
            .expect("clientsecret.json")
    };

    let token_cache_path =
        crate::config::config_file_path(token_cache_filename).expect("token cache path");

    let auth: Authenticator<<DefaultHyperClient as HyperClientBuilder>::Connector> =
        InstalledFlowAuthenticator::builder(app_secret, InstalledFlowReturnMethod::HTTPRedirect)
            .persist_tokens_to_disk(token_cache_path.as_path())
            .build()
            .await?;

    // Get developer token using priority order: config > runtime env > compile-time
    let dev_token_value = get_dev_token(dev_token)?;
    let header_value_dev_token = MetadataValue::try_from(&dev_token_value)?;
    let header_value_login_customer = MetadataValue::try_from(mcc_customer_id)?;

    let tls_config = tonic::transport::ClientTlsConfig::new().with_native_roots();

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
        user_email: user_email.map(|s| s.to_string()),
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
                            let v: Vec<Option<u64>> = columns
                                .get(i)
                                .map(|col| {
                                    col.iter()
                                        .map(|x| x.parse::<u64>().ok())
                                        .collect()
                                })
                                .unwrap_or_default();
                            series_vec.push(Series::new(header, v));
                        } else {
                            let v: Vec<Option<f64>> = columns
                                .get(i)
                                .map(|col| {
                                    col.iter()
                                        .map(|x| x.parse::<f64>().ok())
                                        .collect()
                                })
                                .unwrap_or_default();
                            series_vec.push(Series::new(header, v));
                        }
                    } else {
                        let v: Vec<String> = columns.get(i).cloned().unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that metric parsing handles valid numeric values correctly
    #[test]
    fn test_integer_metric_parsing_valid_values() {
        let input = ["100".to_string(), "200".to_string(), "0".to_string()];
        let result: Vec<Option<u64>> = input.iter().map(|x| x.parse::<u64>().ok()).collect();

        assert_eq!(result, vec![Some(100), Some(200), Some(0)]);
    }

    /// Test that metric parsing handles invalid values (empty, "--", "N/A") as None
    #[test]
    fn test_integer_metric_parsing_invalid_values() {
        let input = ["".to_string(),
            "--".to_string(),
            "N/A".to_string(),
            " ".to_string()];
        let result: Vec<Option<u64>> = input.iter().map(|x| x.parse::<u64>().ok()).collect();

        assert_eq!(result, vec![None, None, None, None]);
    }

    /// Test that metric parsing handles a mix of valid and invalid values
    #[test]
    fn test_integer_metric_parsing_mixed_values() {
        let input = ["100".to_string(),
            "".to_string(),
            "200".to_string(),
            "--".to_string(),
            "50".to_string()];
        let result: Vec<Option<u64>> = input.iter().map(|x| x.parse::<u64>().ok()).collect();

        assert_eq!(result, vec![Some(100), None, Some(200), None, Some(50)]);
    }

    /// Test that float metric parsing handles valid values correctly
    #[test]
    fn test_float_metric_parsing_valid_values() {
        let input = ["1.5".to_string(), "0.0".to_string(), "99.99".to_string()];
        let result: Vec<Option<f64>> = input.iter().map(|x| x.parse::<f64>().ok()).collect();

        assert_eq!(result, vec![Some(1.5), Some(0.0), Some(99.99)]);
    }

    /// Test that float metric parsing handles invalid impression share values
    #[test]
    fn test_float_metric_parsing_invalid_values() {
        let input = ["--".to_string(),
            "".to_string(),
            "N/A".to_string(),
            " ".to_string()];
        let result: Vec<Option<f64>> = input.iter().map(|x| x.parse::<f64>().ok()).collect();

        assert_eq!(result, vec![None, None, None, None]);
    }

    /// Test that float metric parsing handles mixed valid and invalid values
    #[test]
    fn test_float_metric_parsing_mixed_values() {
        let input = ["0.85".to_string(),
            "--".to_string(),
            "0.95".to_string(),
            "".to_string(),
            "0.75".to_string()];
        let result: Vec<Option<f64>> = input.iter().map(|x| x.parse::<f64>().ok()).collect();

        assert_eq!(result, vec![Some(0.85), None, Some(0.95), None, Some(0.75)]);
    }

    /// Test creating a Polars Series from optional integer values (simulates the fixed code path)
    #[test]
    fn test_series_from_optional_integer_values() {
        let values: Vec<Option<u64>> = vec![Some(100), None, Some(200), None, Some(50)];
        let series = Series::new("metrics.clicks", values);

        assert_eq!(series.len(), 5);
        assert_eq!(series.name(), "metrics.clicks");
        assert_eq!(series.null_count(), 2);
    }

    /// Test creating a Polars Series from optional float values (simulates the fixed code path)
    #[test]
    fn test_series_from_optional_float_values() {
        let values: Vec<Option<f64>> = vec![Some(0.85), None, Some(0.95), None, Some(0.75)];
        let series = Series::new("metrics.search_impression_share", values);

        assert_eq!(series.len(), 5);
        assert_eq!(series.name(), "metrics.search_impression_share");
        assert_eq!(series.null_count(), 2);
    }

    /// Test parsing realistic impression share values from Google Ads API
    #[test]
    fn test_realistic_impression_share_values() {
        // These are typical values returned by Google Ads API for impression share metrics
        let input = [
            "0.8567".to_string(),      // Valid percentage (85.67%)
            "--".to_string(),          // No data available
            "0.9215".to_string(),      // Valid percentage (92.15%)
            "".to_string(),            // Empty (no data)
            "0.0000".to_string(),      // Zero value
            "N/A".to_string(),         // Not applicable
        ];
        let result: Vec<Option<f64>> = input.iter().map(|x| x.parse::<f64>().ok()).collect();

        assert_eq!(
            result,
            vec![
                Some(0.8567),
                None,
                Some(0.9215),
                None,
                Some(0.0000),
                None
            ]
        );
    }

    /// Test that row count is preserved when parsing fails (the key fix)
    #[test]
    fn test_row_count_preserved_with_null_values() {
        let columns: Vec<Vec<String>> = vec![
            vec![
                "100".to_string(),
                "200".to_string(),
                "".to_string(),
                "400".to_string(),
            ],
            vec![
                "0.85".to_string(),
                "--".to_string(),
                "0.75".to_string(),
                "".to_string(),
            ],
        ];

        // Simulate parsing like the code does
        let int_values: Vec<Option<u64>> = columns[0]
            .iter()
            .map(|x| x.parse::<u64>().ok())
            .collect();
        let float_values: Vec<Option<f64>> = columns[1]
            .iter()
            .map(|x| x.parse::<f64>().ok())
            .collect();

        // Both should have 4 rows (same as input)
        assert_eq!(int_values.len(), 4);
        assert_eq!(float_values.len(), 4);

        // Verify the specific values
        assert_eq!(int_values, vec![Some(100), Some(200), None, Some(400)]);
        assert_eq!(float_values, vec![Some(0.85), None, Some(0.75), None]);

        // Create series and verify DataFrame can be constructed
        let int_series = Series::new("metrics.clicks", int_values);
        let float_series = Series::new("metrics.search_impression_share", float_values);

        assert_eq!(int_series.len(), 4);
        assert_eq!(float_series.len(), 4);
        assert_eq!(int_series.null_count(), 1);
        assert_eq!(float_series.null_count(), 2);
    }

    /// Comprehensive test for all Google Ads placeholder values that can be returned for metrics
    #[test]
    fn test_all_google_ads_placeholder_values_integer() {
        // Google Ads API can return various placeholder values when data is not available
        let placeholder_values = [
            ("", "empty string"),
            ("--", "double dash"),
            ("-", "single dash"),
            ("n/a", "lowercase n/a"),
            ("N/A", "uppercase N/A"),
            ("N/a", "mixed case N/a"),
            ("na", "na without slashes"),
            ("NA", "NA without slashes"),
            ("null", "null string"),
            ("NULL", "NULL string"),
            ("none", "none string"),
            ("NONE", "NONE string"),
        ];

        for (value, description) in &placeholder_values {
            let result: Option<u64> = value.parse().ok();
            assert!(
                result.is_none(),
                "Expected None for {} ('{}'), got {:?}",
                description,
                value,
                result
            );
        }
    }

    /// Comprehensive test for all Google Ads placeholder values for float metrics
    #[test]
    fn test_all_google_ads_placeholder_values_float() {
        // Google Ads API can return various placeholder values when data is not available
        let placeholder_values = [
            ("", "empty string"),
            ("--", "double dash"),
            ("-", "single dash"),
            ("n/a", "lowercase n/a"),
            ("N/A", "uppercase N/A"),
            ("N/a", "mixed case N/a"),
            ("na", "na without slashes"),
            ("NA", "NA without slashes"),
            ("null", "null string"),
            ("NULL", "NULL string"),
            ("none", "none string"),
            ("NONE", "NONE string"),
        ];

        for (value, description) in &placeholder_values {
            let result: Option<f64> = value.parse().ok();
            assert!(
                result.is_none(),
                "Expected None for {} ('{}'), got {:?}",
                description,
                value,
                result
            );
        }
    }

    /// Test that valid metrics still parse correctly alongside all placeholder values
    #[test]
    fn test_mixed_valid_and_all_placeholder_values() {
        let input = [
            "1000".to_string(),
            "".to_string(),
            "2000".to_string(),
            "--".to_string(),
            "3000".to_string(),
            "-".to_string(),
            "4000".to_string(),
            "n/a".to_string(),
            "5000".to_string(),
            "N/A".to_string(),
        ];

        let result: Vec<Option<u64>> = input.iter().map(|x| x.parse().ok()).collect();

        // Should have 10 values with 5 valid and 5 None
        assert_eq!(result.len(), 10);
        assert_eq!(
            result,
            vec![
                Some(1000),
                None,
                Some(2000),
                None,
                Some(3000),
                None,
                Some(4000),
                None,
                Some(5000),
                None,
            ]
        );

        // Verify we can create a Series with these values
        let series = Series::new("metrics.clicks", result);
        assert_eq!(series.len(), 10);
        assert_eq!(series.null_count(), 5);
    }

    /// Test all common impression share metrics with placeholder values
    #[test]
    fn test_impression_share_metrics_with_placeholders() {
        // Simulate a realistic scenario with multiple impression share columns
        let search_impression_share = [
            "0.8567".to_string(),
            "--".to_string(),
            "0.9215".to_string(),
            "".to_string(),
            "0.7500".to_string(),
            "n/a".to_string(),
        ];

        let absolute_top_impression_share = [
            "0.6523".to_string(),
            "--".to_string(),
            "".to_string(),
            "-".to_string(),
            "0.4521".to_string(),
            "N/A".to_string(),
        ];

        let search_top_impression_share = [
            "0.7534".to_string(),
            "n/a".to_string(),
            "0.8923".to_string(),
            "--".to_string(),
            "-".to_string(),
            "".to_string(),
        ];

        let parsed_search: Vec<Option<f64>> =
            search_impression_share.iter().map(|x| x.parse().ok()).collect();
        let parsed_absolute_top: Vec<Option<f64>> =
            absolute_top_impression_share.iter().map(|x| x.parse().ok()).collect();
        let parsed_top: Vec<Option<f64>> =
            search_top_impression_share.iter().map(|x| x.parse().ok()).collect();

        // All should have 6 rows
        assert_eq!(parsed_search.len(), 6);
        assert_eq!(parsed_absolute_top.len(), 6);
        assert_eq!(parsed_top.len(), 6);

        // Verify specific null positions
        assert!(parsed_search[1].is_none());
        assert!(parsed_search[3].is_none());
        assert!(parsed_search[5].is_none());

        assert!(parsed_absolute_top[1].is_none());
        assert!(parsed_absolute_top[3].is_none());
        assert!(parsed_absolute_top[5].is_none());

        assert!(parsed_top[1].is_none());
        assert!(parsed_top[3].is_none());
        assert!(parsed_top[4].is_none());
        assert!(parsed_top[5].is_none());

        // Create DataFrame and verify structure
        let search_series = Series::new("metrics.search_impression_share", parsed_search);
        let absolute_top_series =
            Series::new("metrics.search_absolute_top_impression_share", parsed_absolute_top);
        let top_series = Series::new("metrics.search_top_impression_share", parsed_top);

        let df = DataFrame::new(vec![search_series, absolute_top_series, top_series]).unwrap();

        assert_eq!(df.height(), 6);
        assert_eq!(df.width(), 3);
    }

    /// Test edge cases with whitespace and special characters
    #[test]
    fn test_whitespace_and_special_characters() {
        let edge_cases = [
            ("  ", "whitespace only"),
            ("\t", "tab character"),
            ("\n", "newline character"),
            (" 100 ", "number with spaces"),
            ("0.85 ", "float with trailing space"),
        ];

        for (value, description) in &edge_cases {
            let int_result: Option<u64> = value.parse().ok();
            let float_result: Option<f64> = value.parse().ok();

            assert!(
                int_result.is_none(),
                "Expected None for integer {} ('{:?}'), got {:?}",
                description,
                value,
                int_result
            );
            assert!(
                float_result.is_none(),
                "Expected None for float {} ('{:?}'), got {:?}",
                description,
                value,
                float_result
            );
        }
    }
}

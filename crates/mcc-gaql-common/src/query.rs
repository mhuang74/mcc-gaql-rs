use anyhow::{Result, bail};

use googleads_rs::google::ads::googleads::v23::services::{
    SearchGoogleAdsRequest, SearchGoogleAdsStreamRequest,
    google_ads_service_client::GoogleAdsServiceClient,
};
use googleads_rs::google::ads::googleads::v23::services::GoogleAdsRow;

use crate::googleads_api::GoogleAdsAPIAccess;
use tonic::codegen::InterceptedService;
use tonic::transport::Channel;
use tokio_stream::StreamExt;

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

/// Validate a GAQL query against Google Ads API without executing it.
/// Uses SearchGoogleAdsRequest with validate_only: true.
/// Returns Ok(()) if valid, Err with API error message if invalid.
pub async fn validate_gaql_query(
    api_context: GoogleAdsAPIAccess,
    customer_id: &str,
    query: &str,
) -> Result<()> {
    let mut client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    client
        .search(SearchGoogleAdsRequest {
            customer_id: customer_id.to_string(),
            query: query.to_string(),
            validate_only: true,
            ..Default::default()
        })
        .await
        .map(|_| ())
        .map_err(|status| {
            let details = String::from_utf8_lossy(status.details())
                .trim()
                .replace(|c: char| !c.is_ascii(), "")
                .replace("%", " ")
                .replace("\n", " ")
                .replace("\r", " ");
            if details.is_empty() {
                anyhow::anyhow!("{}", status.message())
            } else {
                anyhow::anyhow!("{}: {}", status.message(), details)
            }
        })
}

pub async fn get_child_account_ids(
    api_context: GoogleAdsAPIAccess,
    mcc_customer_id: String,
) -> Result<Vec<String>> {
    let mut client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    let result = client
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

/// Execute a GAQL search_stream query and return raw GoogleAdsRow results.
/// No DataFrame dependency — usable by mcc-gaql-mutate for --from-query
/// and update-bidding current-state fetch.
pub async fn search_stream_rows(
    api_context: &GoogleAdsAPIAccess,
    customer_id: &str,
    query: &str,
) -> Result<Vec<GoogleAdsRow>> {
    let mut client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(
            api_context.channel.clone(),
            api_context.clone(),
        );

    let response = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: customer_id.to_string(),
            query: query.to_string(),
            summary_row_setting: 0,
        })
        .await
        .map_err(|status| {
            anyhow::anyhow!(
                "GoogleAdsClient streaming error. Account: {}, Message: '{}'",
                customer_id,
                status.message()
            )
        })?;

    let mut stream = response.into_inner();
    let mut rows = Vec::new();

    while let Some(item) = stream.next().await {
        match item {
            Ok(stream_response) => {
                rows.extend(stream_response.results);
            }
            Err(status) => {
                bail!(
                    "GoogleAdsClient streaming error. Account: {}, Message: '{}'",
                    customer_id,
                    status.message()
                );
            }
        }
    }

    Ok(rows)
}

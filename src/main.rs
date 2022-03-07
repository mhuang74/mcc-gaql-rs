use tokio_stream::StreamExt;
use tonic::{
    metadata::MetadataValue,
    transport::Channel,
    Request
};

use gapi_grpc::google::ads::googleads::v10::services::{
    SearchGoogleAdsStreamRequest,
    google_ads_service_client::GoogleAdsServiceClient
};

const ENDPOINT: &str = "https://googleads.googleapis.com:443";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    println!("GAQL2CSV");

    let auth_token = std::env::var("GOOGLEADS_AUTH_TOKEN").map_err(|_| {
        "Pass a valid 0Auth bearer token via `GOOGLEADS_AUTH_TOKEN` environment variable.".to_string()
    })?;
    let bearer_token = format!("Bearer {}", auth_token);
    let header_value_auth_token = MetadataValue::from_str(&bearer_token)?;

    let dev_token = std::env::var("GOOGLEADS_DEV_TOKEN").map_err(|_| {
        "Pass a valid Google Ads dev token via `GOOGLEADS_DEV_TOKEN` environment variable.".to_string()
    })?;
    let header_value_dev_token = MetadataValue::from_str(&dev_token)?;


    let channel = Channel::from_static(ENDPOINT)
        .connect()
        .await?;

    let mut client = GoogleAdsServiceClient::with_interceptor(channel, move |mut req: Request<()>| {
        req.metadata_mut()
            .insert("authorization", header_value_auth_token.clone());
        req.metadata_mut()
            .insert("developer-token", header_value_dev_token.clone());

        Ok(req)
    });

    let mut stream = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: "123".to_string(),
            query: "SELECT
                        campaign.name,
                        campaign.status,
                        segments.device,
                        metrics.impressions,
                        metrics.clicks,
                        metrics.ctr,
                        metrics.average_cpc,
                        metrics.cost_micros
                    FROM campaign
                    WHERE segments.date DURING LAST_7_DAYS".to_string(),
            summary_row_setting: 0
        })
        .await
        .unwrap()
        .into_inner();

    while let Some(row) = stream.next().await {
        println!("{:?}", row.unwrap());
    }

    println!("Done");

    Ok(())

}


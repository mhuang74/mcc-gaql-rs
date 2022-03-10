use tokio_stream::StreamExt;
use tonic::{
    metadata::MetadataValue,
    transport::Channel,
    Request
};
use yup_oauth2::{InstalledFlowAuthenticator, InstalledFlowReturnMethod};

use gapi_grpc::google::ads::googleads::v10::services::{
    SearchGoogleAdsStreamRequest,
    google_ads_service_client::GoogleAdsServiceClient
};

const ENDPOINT: &str = "https://googleads.googleapis.com:443";
const DEV_TOKEN: &str = "NDfdEk-vsUJPk7SLTH3Knw";
// MCC Test Account 838-081-7587
const MCC_CUSTOMER_ID: &str = "8380817587";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {

    let customer_id = std::env::args()
        .nth(1)
        .ok_or_else(|| "Expected Google Account CustomerID as the first argument.".to_string())?;

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
            println!("The token is {:?}", t);
            t.as_str().to_owned()
        }
    };


    let bearer_token = format!("Bearer {}", access_token);
    let header_value_auth_token = MetadataValue::from_str(&bearer_token)?;
    let header_value_dev_token = MetadataValue::from_str(DEV_TOKEN)?;
    let header_value_login_customer = MetadataValue::from_str(MCC_CUSTOMER_ID)?;
    
    let channel = Channel::from_static(ENDPOINT)
        .connect()
        .await?;

    let mut client = GoogleAdsServiceClient::with_interceptor(channel, move |mut req: Request<()>| {
        req.metadata_mut()
            .insert("authorization", header_value_auth_token.clone());
        req.metadata_mut()
            .insert("developer-token", header_value_dev_token.clone());
        req.metadata_mut()
            .insert("login-customer-id", header_value_login_customer.clone());
        Ok(req)
    });

    let mut stream = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: customer_id.to_string(),
            query: "SELECT
                        campaign.name,
                        campaign.status
                    FROM campaign
                    WHERE segments.date DURING YESTERDAY
                    ORDER by campaign.name
            ".to_string(),
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


use anyhow::{Context, Result};
use googleads_rs::DynamicMutationBuilder;
use googleads_rs::proto::google::ads::googleads::v23::services::FieldUpdate;
use googleads_rs::proto::google::ads::googleads::v23::services::MutationOperation;
use googleads_rs::proto::google::ads::googleads::v23::services::{
    MutateGoogleAdsRequest, MutateGoogleAdsResponse,
    google_ads_service_client::GoogleAdsServiceClient,
};
use mcc_gaql_common::googleads_api::GoogleAdsAPIAccess;
use tonic::codegen::InterceptedService;
use tonic::transport::Channel;

pub struct MutationParams {
    pub resource_type: String,
    pub customer_id: String,
    pub resource_name: String,
    pub operation: MutationOperation,
    pub field_updates: Vec<FieldUpdate>,
    pub validate_only: bool,
    pub partial_failure: bool,
}

pub async fn mutate_resource(
    api_context: GoogleAdsAPIAccess,
    params: MutationParams,
) -> Result<MutateGoogleAdsResponse> {
    let request = build_mutation_request(
        &params.resource_type,
        &params.customer_id,
        &params.resource_name,
        params.operation,
        &params.field_updates,
        params.validate_only,
        params.partial_failure,
    )?;

    let mut client =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    let response = client.mutate(request).await.map_err(|status| {
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
    })?;

    Ok(response.into_inner())
}

#[allow(clippy::too_many_arguments)]
pub fn build_mutation_request(
    resource_type: &str,
    customer_id: &str,
    resource_name: &str,
    operation: MutationOperation,
    field_updates: &[FieldUpdate],
    validate_only: bool,
    partial_failure: bool,
) -> Result<MutateGoogleAdsRequest> {
    let mut builder = DynamicMutationBuilder::new(resource_type, customer_id);
    builder.operation_type(operation);
    builder.validate_only(validate_only);
    builder.partial_failure(partial_failure);

    for update in field_updates {
        builder.set_field(&update.field_path, &update.value);
    }

    builder
        .build(resource_name)
        .map_err(|e| anyhow::anyhow!("Failed to build mutation request: {}", e))
}

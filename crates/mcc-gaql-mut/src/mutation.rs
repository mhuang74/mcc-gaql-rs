use crate::args::{FieldUpdate, MutationOperation};
use anyhow::Result;
use googleads_rs::google::ads::googleads::v23::services::{
    MutateGoogleAdsRequest, MutateGoogleAdsResponse,
    google_ads_service_client::GoogleAdsServiceClient,
};
use mcc_gaql_common::googleads_api::GoogleAdsAPIAccess;

/// Builder for constructing MutateGoogleAdsRequest instances.
/// NOTE: This is a stub implementation. The full implementation should use
/// googleads-rs DynamicMutationBuilder when available.
struct DynamicMutationBuilder {
    #[allow(dead_code)]
    resource_type: String,
    customer_id: String,
    operation: MutationOperation,
    validate_only: bool,
    partial_failure: bool,
    field_updates: Vec<FieldUpdate>,
}

impl DynamicMutationBuilder {
    fn new(resource_type: &str, customer_id: &str) -> Self {
        Self {
            resource_type: resource_type.to_string(),
            customer_id: customer_id.to_string(),
            operation: MutationOperation::Update,
            validate_only: false,
            partial_failure: false,
            field_updates: Vec::new(),
        }
    }

    fn operation_type(&mut self, op: MutationOperation) -> &mut Self {
        self.operation = op;
        self
    }

    fn validate_only(&mut self, value: bool) -> &mut Self {
        self.validate_only = value;
        self
    }

    fn partial_failure(&mut self, value: bool) -> &mut Self {
        self.partial_failure = value;
        self
    }

    fn set_field(&mut self, field_path: &str, value: &str) -> &mut Self {
        self.field_updates.push(FieldUpdate {
            field_path: field_path.to_string(),
            value: value.to_string(),
        });
        self
    }

    fn build(&self, _resource_name: &str) -> Result<MutateGoogleAdsRequest> {
        // Stub implementation - returns a minimal request
        // In the full implementation, this would properly convert field updates
        // to protobuf messages and construct the mutation operations
        Ok(MutateGoogleAdsRequest {
            customer_id: self.customer_id.clone(),
            validate_only: self.validate_only,
            partial_failure: self.partial_failure,
            response_content_type: 0, // RESPONSE_CONTENT_TYPE_UNSPECIFIED
            mutate_operations: vec![],
        })
    }
}

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

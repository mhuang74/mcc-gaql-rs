use crate::args::{FieldUpdate, MutationOperation};
use anyhow::Result;
use googleads_rs::current_gads_version::services::{
    MutateGoogleAdsRequest, MutateGoogleAdsResponse,
    google_ads_service_client::GoogleAdsServiceClient,
};
use googleads_rs::{DynamicMutationBuilder, MutationOp};
use mcc_gaql_common::googleads_api::GoogleAdsAPIAccess;

fn to_mutation_op(op: MutationOperation) -> MutationOp {
    match op {
        MutationOperation::Update => MutationOp::Update,
        MutationOperation::Create => MutationOp::Create,
        MutationOperation::Remove => MutationOp::Remove,
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
    builder.operation_type(to_mutation_op(operation));
    builder.validate_only(validate_only);
    builder.partial_failure(partial_failure);

    for update in field_updates {
        builder.set_field(&update.field_path, &update.value);
    }

    builder
        .build(resource_name)
        .map_err(|e| anyhow::anyhow!("Failed to build mutation request: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_mutation_request_update() {
        let field_updates = vec![FieldUpdate {
            field_path: "name".to_string(),
            value: "Test Campaign".to_string(),
        }];

        let result = build_mutation_request(
            "Campaign",
            "1234567890",
            "customers/1234567890/campaigns/987654321",
            MutationOperation::Update,
            &field_updates,
            false,
            true,
        );

        assert!(result.is_ok());
        let request = result.unwrap();
        assert_eq!(request.customer_id, "1234567890");
        assert_eq!(request.validate_only, false);
        assert_eq!(request.partial_failure, true);
    }

    #[test]
    fn test_build_mutation_request_create() {
        let field_updates = vec![FieldUpdate {
            field_path: "name".to_string(),
            value: "New Campaign".to_string(),
        }];

        let result = build_mutation_request(
            "Campaign",
            "1234567890",
            "customers/1234567890/campaigns/new",
            MutationOperation::Create,
            &field_updates,
            false,
            false,
        );

        assert!(result.is_ok());
        let request = result.unwrap();
        assert_eq!(request.customer_id, "1234567890");
        assert_eq!(request.validate_only, false);
        assert_eq!(request.partial_failure, false);
    }

    #[test]
    fn test_build_mutation_request_remove() {
        let field_updates = vec![];

        let result = build_mutation_request(
            "Campaign",
            "1234567890",
            "customers/1234567890/campaigns/987654321",
            MutationOperation::Remove,
            &field_updates,
            false,
            false,
        );

        assert!(result.is_ok());
        let request = result.unwrap();
        assert_eq!(request.customer_id, "1234567890");
        assert_eq!(request.validate_only, false);
    }
}

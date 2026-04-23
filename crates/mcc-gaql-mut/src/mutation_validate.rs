use crate::args::{FieldUpdate, MutationOperation};
use anyhow::{Result, bail};
use googleads_rs::coerce_value;
use googleads_rs::descriptor_pool;
use prost_reflect::Kind;

const RESOURCES_FQN_PREFIX: &str = "google.ads.googleads.v23.resources";

pub fn validate_mutation_locally(
    resource_type: &str,
    field_updates: &[FieldUpdate],
    operation: MutationOperation,
) -> Result<()> {
    if resource_type.is_empty() {
        bail!("Resource type cannot be empty");
    }

    let mut field_paths = std::collections::HashSet::new();
    for update in field_updates {
        if !field_paths.insert(&update.field_path) {
            bail!("Duplicate field path: {}", update.field_path);
        }
    }

    validate_field_paths(resource_type, field_updates, operation)?;

    log::debug!(
        "Validating mutation for resource '{}' with {} field updates",
        resource_type,
        field_updates.len()
    );

    Ok(())
}

fn validate_field_paths(
    resource_type: &str,
    field_updates: &[FieldUpdate],
    operation: MutationOperation,
) -> Result<()> {
    let pool = descriptor_pool();
    let resource_fqn = format!("{}.{}", RESOURCES_FQN_PREFIX, resource_type);
    let resource_desc = pool.get_message_by_name(&resource_fqn).ok_or_else(|| {
        anyhow::anyhow!(
            "Unknown resource type '{}'. Not found in descriptor pool as '{}'",
            resource_type,
            resource_fqn
        )
    })?;

    if operation == MutationOperation::Create {
        validate_create_required_fields(resource_type, &resource_desc, field_updates)?;
    }

    for update in field_updates {
        validate_single_path(
            resource_type,
            &resource_desc,
            &update.field_path,
            &update.value,
        )?;
    }

    Ok(())
}

fn validate_single_path(
    resource_type: &str,
    resource_desc: &prost_reflect::MessageDescriptor,
    field_path: &str,
    value: &str,
) -> Result<()> {
    let segments: Vec<&str> = field_path.split('.').collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        bail!("Empty segment in field path '{}'", field_path);
    }

    walk_path(resource_type, resource_desc, &segments, field_path, value)
}

fn walk_path(
    resource_type: &str,
    current_desc: &prost_reflect::MessageDescriptor,
    segments: &[&str],
    full_path: &str,
    value: &str,
) -> Result<()> {
    let segment = segments[0];
    let remaining = &segments[1..];

    let field_desc = current_desc.get_field_by_name(segment).ok_or_else(|| {
        let available: Vec<String> = current_desc
            .fields()
            .map(|f| f.name().to_string())
            .collect();
        anyhow::anyhow!(
            "Field '{}' not found on resource {}. Available fields include: {}",
            segment,
            resource_type,
            available.join(", ")
        )
    })?;

    if remaining.is_empty() {
        return validate_leaf_value(resource_type, field_desc, value, full_path);
    }

    match field_desc.kind() {
        Kind::Message(nested_desc) => {
            walk_path(resource_type, &nested_desc, remaining, full_path, value)
        }
        Kind::String => {
            if field_desc.name() == segment {
                bail!(
                    "Cannot traverse into '{}' on {} — it is a string reference, not a nested message. \
                     To update fields on the referenced resource, mutate that resource type directly.",
                    segment,
                    resource_type
                );
            }
            bail!(
                "Cannot traverse into non-message field '{}' of type String in path '{}'",
                segment,
                full_path
            );
        }
        _ => {
            bail!(
                "Cannot traverse into non-message field '{}' of type {:?} in path '{}'",
                segment,
                field_desc.kind(),
                full_path
            );
        }
    }
}

fn validate_leaf_value(
    resource_type: &str,
    field_desc: prost_reflect::FieldDescriptor,
    value: &str,
    full_path: &str,
) -> Result<()> {
    match field_desc.kind() {
        Kind::Enum(enum_desc) => {
            if enum_desc.get_value_by_name(value).is_some() {
                return Ok(());
            }
            if let Ok(n) = value.parse::<i32>()
                && enum_desc.get_value(n).is_some()
            {
                return Ok(());
            }
            let total_count = enum_desc.values().count();
            let valid: Vec<String> = enum_desc
                .values()
                .take(20)
                .map(|v| v.name().to_string())
                .collect();
            let suffix = if total_count > 20 {
                format!(" (and {} more)", total_count - 20)
            } else {
                String::new()
            };
            bail!(
                "Invalid enum value '{}' for field '{}' on {}. Valid values: {}{}",
                value,
                full_path,
                resource_type,
                valid.join(", "),
                suffix
            );
        }
        Kind::Message(_) => {
            bail!(
                "Field '{}' on {} is a message type — provide nested field paths (e.g., '{}.sub_field=value')",
                full_path,
                resource_type,
                full_path
            );
        }
        _ => {
            coerce_value(value, &field_desc).map_err(|e| {
                anyhow::anyhow!(
                    "Type error for field '{}' on {}: {}",
                    full_path,
                    resource_type,
                    e
                )
            })?;
            Ok(())
        }
    }
}

fn validate_create_required_fields(
    resource_type: &str,
    resource_desc: &prost_reflect::MessageDescriptor,
    field_updates: &[FieldUpdate],
) -> Result<()> {
    let provided_paths: std::collections::HashSet<&str> = field_updates
        .iter()
        .map(|u| u.field_path.as_str())
        .collect();

    let mut missing: Vec<String> = Vec::new();
    for field in resource_desc.fields() {
        if field.is_required() && !provided_paths.contains(field.name()) {
            let prefix = format!("{}.", field.name());
            let has_nested = provided_paths.iter().any(|p| p.starts_with(&prefix));
            if !has_nested {
                missing.push(field.name().to_string());
            }
        }
    }

    if !missing.is_empty() {
        bail!(
            "Missing required field(s) for create on {}: {}",
            resource_type,
            missing.join(", ")
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_mutation_basic() {
        let updates = vec![FieldUpdate {
            field_path: "name".to_string(),
            value: "Test Campaign".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_mutation_empty_resource_type() {
        let updates = vec![FieldUpdate {
            field_path: "name".to_string(),
            value: "Test".to_string(),
        }];

        let result = validate_mutation_locally("", &updates, MutationOperation::Update);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mutation_duplicate_fields() {
        let updates = vec![
            FieldUpdate {
                field_path: "name".to_string(),
                value: "Test Campaign".to_string(),
            },
            FieldUpdate {
                field_path: "name".to_string(),
                value: "Another Name".to_string(),
            },
        ];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_nested_message_path() {
        let updates = vec![FieldUpdate {
            field_path: "target_roas.target_roas".to_string(),
            value: "3.5".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_string_reference_path() {
        let updates = vec![FieldUpdate {
            field_path: "campaign_budget.amount_micros".to_string(),
            value: "328000000".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("string reference"),
            "Expected helpful error about string reference, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_invalid_field_name() {
        let updates = vec![FieldUpdate {
            field_path: "nonexistent_field".to_string(),
            value: "value".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not found"),
            "Expected 'not found' error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_unknown_resource_type() {
        let updates = vec![FieldUpdate {
            field_path: "name".to_string(),
            value: "Test".to_string(),
        }];

        let result = validate_mutation_locally("FakeResource", &updates, MutationOperation::Update);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_enum_valid_value() {
        let updates = vec![FieldUpdate {
            field_path: "status".to_string(),
            value: "PAUSED".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_enum_invalid_value() {
        let updates = vec![FieldUpdate {
            field_path: "status".to_string(),
            value: "INVALID_THING".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid enum value"),
            "Expected 'Invalid enum value' error, got: {}",
            err
        );
        assert!(
            err.contains("Valid values"),
            "Expected 'Valid values' in error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_enum_by_number() {
        let updates = vec![FieldUpdate {
            field_path: "status".to_string(),
            value: "3".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_type_mismatch() {
        let updates = vec![FieldUpdate {
            field_path: "target_roas.target_roas".to_string(),
            value: "abc".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Type error"),
            "Expected 'Type error' for non-numeric value, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_type_int64_valid() {
        let updates = vec![FieldUpdate {
            field_path: "amount_micros".to_string(),
            value: "328000000".to_string(),
        }];

        let result =
            validate_mutation_locally("CampaignBudget", &updates, MutationOperation::Update);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_type_int64_invalid() {
        let updates = vec![FieldUpdate {
            field_path: "amount_micros".to_string(),
            value: "abc".to_string(),
        }];

        let result =
            validate_mutation_locally("CampaignBudget", &updates, MutationOperation::Update);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Type error"),
            "Expected 'Type error' for non-numeric value, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_leaf_message_error() {
        let updates = vec![FieldUpdate {
            field_path: "target_roas".to_string(),
            value: "value".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Update);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("message type"),
            "Expected 'message type' error, got: {}",
            err
        );
    }

    #[test]
    fn test_validate_create_with_all_fields() {
        let updates = vec![FieldUpdate {
            field_path: "name".to_string(),
            value: "Test Campaign".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates, MutationOperation::Create);
        assert!(result.is_ok());
    }
}

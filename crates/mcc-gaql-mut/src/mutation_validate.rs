use crate::args::FieldUpdate;
use anyhow::{Result, bail};

/// Validate a mutation request locally before sending to the API.
/// This is a placeholder for more comprehensive validation logic.
///
/// In the full implementation, this would:
/// - Validate field paths against resource metadata
/// - Check field types and value formats
/// - Validate required fields for create operations
/// - Check field constraints (enums, min/max values, etc.)
pub fn validate_mutation_locally(resource_type: &str, field_updates: &[FieldUpdate]) -> Result<()> {
    // Basic validation
    if resource_type.is_empty() {
        bail!("Resource type cannot be empty");
    }

    // Check for duplicate field paths
    let mut field_paths = std::collections::HashSet::new();
    for update in field_updates {
        if !field_paths.insert(&update.field_path) {
            bail!("Duplicate field path: {}", update.field_path);
        }
    }

    // TODO: Add comprehensive validation using field metadata
    // For now, just basic checks
    log::debug!(
        "Validating mutation for resource '{}' with {} field updates",
        resource_type,
        field_updates.len()
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_mutation_basic() {
        let updates = vec![FieldUpdate {
            field_path: "campaign.name".to_string(),
            value: "Test Campaign".to_string(),
        }];

        let result = validate_mutation_locally("Campaign", &updates);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_mutation_empty_resource_type() {
        let updates = vec![FieldUpdate {
            field_path: "name".to_string(),
            value: "Test".to_string(),
        }];

        let result = validate_mutation_locally("", &updates);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_mutation_duplicate_fields() {
        let updates = vec![
            FieldUpdate {
                field_path: "campaign.name".to_string(),
                value: "Test Campaign".to_string(),
            },
            FieldUpdate {
                field_path: "campaign.name".to_string(),
                value: "Another Name".to_string(),
            },
        ];

        let result = validate_mutation_locally("Campaign", &updates);
        assert!(result.is_err());
    }
}

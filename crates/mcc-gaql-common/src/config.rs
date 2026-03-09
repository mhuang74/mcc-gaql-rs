use serde::{Deserialize, Serialize};
use toml::Value;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

pub const TOML_CONFIG_FILENAME: &str = "config.toml";
pub const ENV_VAR_PREFIX: &str = "MCC_GAQL_";

/// Validate and normalize Google Ads customer ID format
/// Accepts: "1234567890" or "123-456-7890"
/// Returns: "1234567890" (normalized, no hyphens)
pub fn validate_and_normalize_customer_id(customer_id: &str) -> anyhow::Result<String> {
    // Remove hyphens if present
    let normalized = customer_id.replace('-', "");

    // Validate format: exactly 10 digits
    if !normalized.chars().all(|c| c.is_ascii_digit()) {
        return Err(anyhow::anyhow!(
            "Invalid customer ID format: '{}'. \
             Customer ID must contain only digits (and optional hyphens). \
             Example: '1234567890' or '123-456-7890'",
            customer_id
        ));
    }

    if normalized.len() != 10 {
        return Err(anyhow::anyhow!(
            "Invalid customer ID length: '{}'. \
             Customer ID must be exactly 10 digits. \
             Found {} digits.",
            customer_id,
            normalized.len()
        ));
    }

    Ok(normalized)
}

#[derive(Deserialize, Serialize, Debug)]
pub struct MyConfig {
    /// MCC Account ID (optional for solo accounts - if omitted, customer_id will be used as MCC)
    pub mcc_id: Option<String>,
    /// Optional user email for OAuth2 (not required if valid token cache exists)
    pub user_email: Option<String>,
    /// Optional default customer ID to query (can be overridden by --customer-id)
    pub customer_id: Option<String>,
    /// Optional default output format: table, csv, json (can be overridden by --format)
    pub format: Option<String>,
    /// Optional default keep-going behavior on errors (can be overridden by --keep-going)
    pub keep_going: Option<bool>,
    /// Token Cache filename (optional - auto-generated from user if not specified)
    pub token_cache_filename: Option<String>,
    /// Optional file containing child customer_ids to query
    pub customerids_filename: Option<String>,
    /// Optional TOML file with stored queries
    pub queries_filename: Option<String>,
    /// Optional Google Ads Developer Token
    pub dev_token: Option<String>,
    /// Optional field metadata cache file path
    pub field_metadata_cache: Option<String>,
    /// Optional field metadata cache TTL in days
    pub field_metadata_ttl_days: Option<i64>,
}

// Query entries from cookbook
// Make it sortable and comparable for vector search
#[derive(Serialize, Deserialize, Hash, Clone, Debug, Eq, PartialEq, Default)]
pub struct QueryEntry {
    pub id: String,
    pub description: String,
    pub query: String,
}

impl QueryEntry {
    pub fn from_value(value: &Value) -> Self {
        let description = value
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let query = value
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Generate stable ID from description and query content
        let id = format!(
            "query_{}_{}",
            description
                .chars()
                .map(|c| c.to_ascii_lowercase())
                .filter(|c| c.is_alphanumeric() || *c == ' ')
                .collect::<String>()
                .replace(' ', "_")
                .chars()
                .take(20)
                .collect::<String>(),
            query
                .chars()
                .map(|c| c.to_ascii_lowercase())
                .filter(|c| c.is_alphanumeric() || *c == ' ')
                .collect::<String>()
                .replace(' ', "_")
                .chars()
                .take(20)
                .collect::<String>()
        );

        QueryEntry {
            id,
            description,
            query,
        }
    }
}

/// get named queries from file, as a Map of String
pub async fn get_queries_from_file<P>(filename: P) -> anyhow::Result<HashMap<String, QueryEntry>>
where
    P: AsRef<Path> + std::fmt::Debug,
{
    use anyhow::bail;

    match File::open(&filename) {
        Ok(file) => {
            let mut buffer = String::new();

            BufReader::new(&file).read_to_string(&mut buffer)?;

            // parse Toml
            let toml = match buffer.parse::<Value>() {
                Ok(v) => v,
                Err(e) => {
                    bail!("Unable to parse stored query toml. Error: {}", e);
                }
            };

            let mut query_map = HashMap::new();

            if let Value::Table(entries) = toml {
                for (section, content) in entries {
                    if let Value::Table(content_table) = content {
                        let query_entry = QueryEntry::from_value(&Value::Table(content_table));
                        query_map.insert(section, query_entry);
                    }
                }
            } else {
                bail!("Expected a TOML table at the root");
            }

            log::info!(
                "{} queries loaded from file {}.",
                query_map.len(),
                filename.as_ref().display()
            );

            Ok(query_map)
        }
        Err(e) => {
            anyhow::bail!(
                "Unable to load named query file: {:?}. Error: {}",
                filename,
                e
            );
        }
    }
}

/// get child account ids list from plain text file, one ID per line
pub async fn get_child_account_ids_from_file<P>(filename: P) -> anyhow::Result<Vec<String>>
where
    P: AsRef<Path>,
{
    use anyhow::bail;

    match File::open(&filename) {
        Ok(file) => {
            let mut customer_ids: Vec<String> = Vec::with_capacity(2048);

            let lines = BufReader::new(&file).lines();

            for (line_num, line) in lines.enumerate() {
                let line = line?;
                let trimmed = line.trim();

                // Skip empty lines
                if trimmed.is_empty() {
                    continue;
                }

                // Validate and normalize customer ID
                let normalized = validate_and_normalize_customer_id(trimmed)
                    .map_err(|e| {
                        anyhow::anyhow!(
                            "Invalid customer ID on line {} in file {}: {}",
                            line_num + 1,
                            filename.as_ref().display(),
                            e
                        )
                    })?;

                customer_ids.push(normalized);
            }

            log::debug!(
                "Loaded {} customer_ids from file {}",
                customer_ids.len(),
                filename.as_ref().display()
            );

            Ok(customer_ids)
        }
        Err(e) => {
            bail!("Unable to load child account ids from file: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_customer_id_valid() {
        assert_eq!(
            validate_and_normalize_customer_id("1234567890").unwrap(),
            "1234567890"
        );
    }

    #[test]
    fn test_validate_customer_id_with_hyphens() {
        assert_eq!(
            validate_and_normalize_customer_id("123-456-7890").unwrap(),
            "1234567890"
        );
    }

    #[test]
    fn test_validate_customer_id_invalid_chars() {
        let result = validate_and_normalize_customer_id("123abc7890");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_customer_id_too_short() {
        let result = validate_and_normalize_customer_id("123456789");
        assert!(result.is_err());
    }
}

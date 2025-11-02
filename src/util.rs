use anyhow::{Result, bail};
use std::{
    collections::HashMap,
    env,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};

use serde::Serialize;
use toml::Value;

#[allow(dead_code)]
const CACHE_FILENAME: &str = ".cache";
#[allow(dead_code)]
const CACHE_KEY_CHILD_ACCOUNTS: &str = "child-accounts";

/// initialize Flexi Logger via Env Vars
/// <prefix>_LOG_LEVEL sets logging level
/// <prefix>_LOG_DIR sets log file path
pub fn init_logger() {
    use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

    let my_log_env = env::var(format!("{}{}", crate::config::ENV_VAR_PREFIX, "LOG_LEVEL"))
        .unwrap_or_else(|_| "off".to_string());
    let my_log_dir = env::var(format!("{}{}", crate::config::ENV_VAR_PREFIX, "LOG_DIR"))
        .unwrap_or_else(|_| ".".to_string());

    Logger::try_with_env_or_str(my_log_env)
        .unwrap()
        .use_utc()
        .log_to_file(
            FileSpec::default()
                .directory(my_log_dir)
                .suppress_timestamp(),
        )
        .format_for_files(flexi_logger::detailed_format)
        .o_append(true)
        .rotate(
            Criterion::Size(1_000_000),
            Naming::Numbers,
            Cleanup::KeepLogAndCompressedFiles(10, 100),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        .start()
        .unwrap();
}

/// fetch child account ids from cache
pub async fn _get_child_accounts_from_cache() -> Option<Vec<String>> {
    match cacache::read(CACHE_FILENAME, CACHE_KEY_CHILD_ACCOUNTS).await {
        Ok(encoded) => match bincode::deserialize(&encoded) {
            Ok(decoded) => {
                let v: Vec<String> = decoded;
                log::debug!(
                    "Successfully retrieved cached child accounts of size {}",
                    v.len()
                );
                Some(v)
            }
            Err(e) => {
                log::error!("Unable to deserialize child accounts cache: {}", e);
                None
            }
        },
        Err(e) => {
            log::debug!("Unable to read child accounts cache: {}", e);
            None
        }
    }
}

pub async fn _save_child_accounts_to_cache(customer_ids: Vec<String>) {
    // save child accounts to cache
    let encoded = bincode::serialize(&customer_ids).unwrap();
    match cacache::write(CACHE_FILENAME, CACHE_KEY_CHILD_ACCOUNTS, &encoded).await {
        Ok(_i) => {
            log::debug!("Added {} child account ids to cache", customer_ids.len());
        }
        Err(e) => {
            log::error!("Failed to update child account cache: {}", e);
        }
    }
}

/// get child account ids list from plain text file, one ID per line
pub async fn get_child_account_ids_from_file<P>(filename: P) -> Result<Vec<String>>
where
    P: AsRef<Path>,
{
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
                let normalized = crate::config::validate_and_normalize_customer_id(trimmed)
                    .map_err(|e| anyhow::anyhow!(
                        "Invalid customer ID on line {} in file {}: {}",
                        line_num + 1,
                        filename.as_ref().display(),
                        e
                    ))?;

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

// Query entries from cookbook
// Make it sortable and comparable for vector search
#[derive(Serialize, Clone, Debug, Eq, PartialEq, Default)]
pub struct QueryEntry {
    pub description: String,
    pub query: String,
}

// Each query entry in TOML has 2 entries:
//   * "description": explains what the query does
//   * "query": valid GAQL query
impl QueryEntry {
    fn from_value(value: &Value) -> Self {
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
        QueryEntry { description, query }
    }
}

/// get named queries from file, as a Map of String
pub async fn get_queries_from_file<P>(filename: P) -> Result<HashMap<String, QueryEntry>>
where
    P: AsRef<Path>,
{
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
            bail!("Unable to load named query file. Error: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_get_child_account_ids_from_file_valid() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "1234567890").unwrap();
        writeln!(temp_file, "123-456-7890").unwrap();
        writeln!(temp_file, "9876543210").unwrap();
        temp_file.flush().unwrap();

        let result = get_child_account_ids_from_file(temp_file.path()).await;
        assert!(result.is_ok());
        let customer_ids = result.unwrap();
        assert_eq!(customer_ids.len(), 3);
        assert_eq!(customer_ids[0], "1234567890");
        assert_eq!(customer_ids[1], "1234567890");  // Normalized from hyphens
        assert_eq!(customer_ids[2], "9876543210");
    }

    #[tokio::test]
    async fn test_get_child_account_ids_from_file_invalid_format() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "1234567890").unwrap();
        writeln!(temp_file, "invalid123").unwrap();  // Invalid: contains letters
        writeln!(temp_file, "9876543210").unwrap();
        temp_file.flush().unwrap();

        let result = get_child_account_ids_from_file(temp_file.path()).await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid customer ID"));
        assert!(error_msg.contains("line 2"));
    }

    #[tokio::test]
    async fn test_get_child_account_ids_from_file_invalid_length() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "1234567890").unwrap();
        writeln!(temp_file, "123456789").unwrap();  // Invalid: only 9 digits
        temp_file.flush().unwrap();

        let result = get_child_account_ids_from_file(temp_file.path()).await;
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid customer ID"));
        assert!(error_msg.contains("line 2"));
    }

    #[tokio::test]
    async fn test_get_child_account_ids_from_file_empty_lines() {
        let mut temp_file = NamedTempFile::new().unwrap();
        writeln!(temp_file, "1234567890").unwrap();
        writeln!(temp_file).unwrap();  // Empty line - should be skipped
        writeln!(temp_file, "   ").unwrap();  // Whitespace only - should be skipped
        writeln!(temp_file, "9876543210").unwrap();
        temp_file.flush().unwrap();

        let result = get_child_account_ids_from_file(temp_file.path()).await;
        assert!(result.is_ok());
        let customer_ids = result.unwrap();
        assert_eq!(customer_ids.len(), 2);
        assert_eq!(customer_ids[0], "1234567890");
        assert_eq!(customer_ids[1], "9876543210");
    }
}

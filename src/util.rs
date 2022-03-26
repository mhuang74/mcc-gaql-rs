use anyhow::{bail, Result};
use std::{
    env,
    fs::File,
    io::{BufRead, BufReader, Read},
    path::Path,
};
use toml::Value;

#[allow(dead_code)]
const CACHE_FILENAME: &str = ".cache";
#[allow(dead_code)]
const CACHE_KEY_CHILD_ACCOUNTS: &str = "child-accounts";

/// initialize Flexi Logger via Env Vars
/// MCCFIND_LOG_LEVEL sets logging level
/// MCCFIND_LOG_DIR sets log file path
pub fn init_logger() {
    use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

    let mccfind_log_env = env::var("MCCFIND_LOG_LEVEL").unwrap_or_else(|_| "off".to_string());
    let mccfind_log_dir = env::var("MCCFIND_LOG_DIR").unwrap_or_else(|_| ".".to_string());

    Logger::try_with_env_or_str(mccfind_log_env)
        .unwrap()
        .use_utc()
        .log_to_file(
            FileSpec::default()
                .directory(mccfind_log_dir)
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
                log::error!(
                    "Unable to deserialize child accounts cache: {}",
                    e.to_string()
                );
                None
            }
        },
        Err(e) => {
            log::debug!("Unable to read child accounts cache: {}", e.to_string());
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
            log::error!("Failed to update child account cache: {}", e.to_string());
        }
    }
}

/// get child account ids list from plain text file, one ID per line
pub async fn get_child_account_ids_from_file<P>(filename: P) -> Result<Vec<String>>
where
    P: AsRef<Path>,
{
    match File::open(filename) {
        Ok(file) => {
            let mut customer_ids: Vec<String> = Vec::with_capacity(2048);

            let lines = BufReader::new(&file).lines();

            for line in lines.flatten() {
                customer_ids.push(line);
            }

            log::debug!(
                "Loaded {} customer_ids from file {:?}",
                customer_ids.len(),
                &file
            );

            Ok(customer_ids)
        }
        Err(e) => {
            bail!(
                "Unable to load child account ids from file: {}",
                e.to_string()
            );
        }
    }
}

/// get named query from file
pub async fn get_query_from_file<P>(filename: P, query_name: &str) -> Result<String>
where
    P: AsRef<Path>,
{
    match File::open(filename) {
        Ok(file) => {
            let mut buffer = String::new();

            BufReader::new(&file).read_to_string(&mut buffer)?;

            // parse Toml
            let toml = match buffer.parse::<Value>() {
                Ok(v) => v,
                Err(e) => {
                    bail!(
                        "Unable to parse stored query toml. Error: {}",
                        e.to_string()
                    );
                }
            };

            let query = match toml.get(query_name) {
                Some(v) => match v.as_str() {
                    Some(s) => s.to_owned(),
                    _ => {
                        bail!("Query not valid string: {}", v);
                    }
                },
                _ => {
                    bail!("Query not found: {query_name}");
                }
            };

            log::debug!(
                "Query '{}' loaded from file {:?}. Query: {}",
                query_name,
                &file,
                query
            );

            Ok(query)
        }
        Err(e) => {
            bail!("Unable to load named query file. Error: {}", e.to_string());
        }
    }
}

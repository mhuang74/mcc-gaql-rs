use std::env;

#[allow(dead_code)]
const CACHE_FILENAME: &str = ".cache";
#[allow(dead_code)]
const CACHE_KEY_CHILD_ACCOUNTS: &str = "child-accounts";

/// initialize Flexi Logger via Env Vars
/// <prefix>_LOG_LEVEL sets logging level
/// <prefix>_LOG_DIR sets log file path
pub fn init_logger() {
    use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

    let my_log_env = env::var(format!(
        "{}{}",
        mcc_gaql_common::config::ENV_VAR_PREFIX,
        "LOG_LEVEL"
    ))
    .unwrap_or_else(|_| "off".to_string());
    let my_log_dir = env::var(format!(
        "{}{}",
        mcc_gaql_common::config::ENV_VAR_PREFIX,
        "LOG_DIR"
    ))
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

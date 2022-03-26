use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const TOML_CONFIG_FILENAME: &str = "config.toml";
const ENV_VAR_PREFIX: &str = "MCCFIND_";

#[derive(Deserialize, Serialize, Debug)]
pub struct MyConfig {
    /// MCC Account ID is mandatory
    pub mcc_customerid: String,
    /// Token Cache filename
    pub token_cache_filename: String,
    /// Optional file containing child customer_ids to query
    pub customerids_filename: Option<String>,
    /// Optional TOML file with stored queries
    pub queries_filename: Option<String>,
}

pub fn load(profile: &str) -> anyhow::Result<MyConfig> {
    log::info!("Config profile: {profile}");

    // load config file to get mcc_customer_id
    let mut figment: Figment = Figment::new();

    // load from file if present
    if let Some(config_file_path) = config_file_path(TOML_CONFIG_FILENAME) {
        log::debug!("Loading config file: {:?}", config_file_path);
        figment = figment.merge(Toml::file(config_file_path).nested());
    }

    // merge in ENV VAR Overrides
    figment = figment.merge(Env::prefixed(ENV_VAR_PREFIX));

    Ok(figment.select(profile).extract()?)
}

/// get the platform-correct config file path
pub fn config_file_path(filename: &str) -> Option<PathBuf> {
    dirs::config_dir().map(move |mut path| {
        path.push(CRATE_NAME);
        path.push(filename);
        path
    })
}

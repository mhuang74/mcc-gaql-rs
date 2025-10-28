use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const TOML_CONFIG_FILENAME: &str = "config.toml";
pub const ENV_VAR_PREFIX: &str = "MCC_GAQL_";

#[derive(Deserialize, Serialize, Debug)]
pub struct MyConfig {
    /// MCC Account ID is mandatory
    pub mcc_customerid: String,
    /// Optional user email for OAuth2 (preferred over token_cache_filename)
    pub user: Option<String>,
    /// Token Cache filename (legacy - use 'user' instead)
    pub token_cache_filename: Option<String>,
    /// Optional file containing child customer_ids to query
    pub customerids_filename: Option<String>,
    /// Optional TOML file with stored queries
    pub queries_filename: Option<String>,
}

/// Resolved runtime configuration combining CLI args and config file
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub mcc_customer_id: String,
    pub user_email: Option<String>,
    pub token_cache_filename: String,
    pub queries_filename: Option<String>,
    pub customerids_filename: Option<String>,
}

impl ResolvedConfig {
    /// Create resolved config from CLI args and optional config file
    pub fn from_args_and_config(
        args: &crate::args::Cli,
        config: Option<MyConfig>,
    ) -> anyhow::Result<Self> {
        // Resolve MCC with priority: CLI --mcc > CLI --customer-id > config
        let mcc_customer_id = args
            .mcc
            .as_ref()
            .or(args.customer_id.as_ref())
            .map(|s| s.to_string())
            .or_else(|| config.as_ref().map(|c| c.mcc_customerid.clone()))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "MCC customer ID required. Either:\n  \
                 1. Provide via CLI: --mcc <MCC_ID> or --customer-id <CUSTOMER_ID>\n  \
                 2. Specify config profile: --profile <PROFILE_NAME>"
                )
            })?;

        // Resolve user email: CLI > config
        let user_email = args
            .user
            .clone()
            .or_else(|| config.as_ref().and_then(|c| c.user.clone()));

        // Resolve token cache filename with priority:
        // 1. Explicit legacy token cache filename from config (highest priority)
        // 2. Auto-generated from user email
        // 3. Default filename (lowest priority)
        let token_cache_filename = config
            .as_ref()
            .and_then(|c| c.token_cache_filename.clone())
            .or_else(|| {
                args.user
                    .as_ref()
                    .or_else(|| config.as_ref().and_then(|c| c.user.as_ref()))
                    .map(|email| crate::googleads::generate_token_cache_filename(email))
            })
            .unwrap_or_else(|| "tokencache_default.json".to_string());

        // Config file fields (only available if profile specified)
        let queries_filename = config.as_ref().and_then(|c| c.queries_filename.clone());
        let customerids_filename = config.as_ref().and_then(|c| c.customerids_filename.clone());

        Ok(Self {
            mcc_customer_id,
            user_email,
            token_cache_filename,
            queries_filename,
            customerids_filename,
        })
    }

    pub fn require_queries_filename(&self) -> anyhow::Result<&str> {
        self.queries_filename.as_deref().ok_or_else(|| {
            anyhow::anyhow!(
                "Query cookbook not available. Either:\n  \
                 1. Provide GAQL query directly: <QUERY>\n  \
                 2. Specify config profile with queries_filename: --profile <PROFILE_NAME>"
            )
        })
    }
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

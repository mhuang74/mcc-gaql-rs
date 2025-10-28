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
        // Resolve MCC with explicit priority and logging
        let mcc_customer_id = if let Some(mcc) = &args.mcc {
            // Explicit --mcc takes highest priority
            log::debug!("Using MCC from --mcc argument: {}", mcc);
            mcc.clone()
        } else if let Some(config_mcc) = config.as_ref().map(|c| &c.mcc_customerid) {
            // Config file MCC is second priority
            log::debug!("Using MCC from config profile: {}", config_mcc);
            config_mcc.clone()
        } else if let Some(customer_id) = &args.customer_id {
            // Fallback: use customer_id as MCC (for solo accounts)
            log::warn!(
                "No --mcc specified. Using --customer-id ({}) as MCC. \
                 This assumes the account is not under a manager account. \
                 Use --mcc explicitly if this account has a manager.",
                customer_id
            );
            customer_id.clone()
        } else {
            // No MCC available anywhere
            return Err(anyhow::anyhow!(
                "MCC customer ID required. Provide one of:\n  \
                 1. CLI argument: --mcc <MCC_ID>\n  \
                 2. Config profile: --profile <PROFILE_NAME>\n  \
                 3. For solo accounts: --customer-id <CUSTOMER_ID> (will be used as MCC)"
            ));
        };

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

    /// Validate that resolved config supports the requested operation mode
    pub fn validate_for_operation(&self, args: &crate::args::Cli) -> anyhow::Result<()> {
        // Validate natural language mode requirements
        if args.natural_language && self.queries_filename.is_none() {
            return Err(anyhow::anyhow!(
                "Natural language mode requires a query cookbook.\n\
                 Please specify a config profile with queries_filename:\n  \
                 --profile <PROFILE_NAME>"
            ));
        }

        // Validate stored query requirements
        if args.stored_query.is_some() && self.queries_filename.is_none() {
            return Err(anyhow::anyhow!(
                "Stored queries require a query cookbook.\n\
                 Please specify a config profile with queries_filename:\n  \
                 --profile <PROFILE_NAME>"
            ));
        }

        // Validate customer ID list requirements for GAQL queries
        // (skip this check if only listing accounts or using field service)
        if !args.list_child_accounts
            && !args.field_service
            && args.gaql_query.is_some()
            && !args.all_linked_child_accounts
            && args.customer_id.is_none()
            && self.customerids_filename.is_none()
        {
            return Err(anyhow::anyhow!(
                "No target accounts specified. Please provide one of:\n  \
                 1. Single account: --customer-id <CUSTOMER_ID>\n  \
                 2. All linked accounts: --all-linked-child-accounts\n  \
                 3. Config profile with customerids_filename: --profile <PROFILE_NAME>"
            ));
        }

        Ok(())
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

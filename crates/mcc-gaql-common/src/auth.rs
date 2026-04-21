use crate::config::{validate_and_normalize_customer_id, MyConfig};
use crate::paths::config_file_path;
use anyhow::{Context, Result};

/// Auth flags shared across all binaries and subcommands.
/// Constructed directly from each binary's CLI args — no `Cli` struct dependency.
#[derive(Debug, Clone)]
pub struct SharedAuthArgs {
    pub customer_id: Option<String>,
    pub mcc_id: Option<String>,
    pub profile: Option<String>,
    pub user_email: Option<String>,
    pub remote_auth: bool,
}

/// Resolved auth configuration — the output of auth resolution,
/// independent of any CLI structure.
#[derive(Debug, Clone)]
pub struct ResolvedAuthConfig {
    pub mcc_customer_id: String,
    pub user_email: Option<String>,
    pub customer_id: Option<String>,
    pub token_cache_filename: String,
    pub dev_token: Option<String>,
    pub remote_auth: bool,
}

impl ResolvedAuthConfig {
    /// Convert to ApiAccessConfig for get_api_access().
    pub fn to_api_access_config(&self) -> crate::googleads_api::ApiAccessConfig {
        crate::googleads_api::ApiAccessConfig {
            mcc_customer_id: self.mcc_customer_id.clone(),
            token_cache_filename: self.token_cache_filename.clone(),
            user_email: self.user_email.clone(),
            dev_token: self.dev_token.clone(),
            use_remote_auth: self.remote_auth,
        }
    }
}

/// Resolve auth configuration from CLI args and optional config file.
/// Replaces the auth-resolution portion of `ResolvedConfig::from_args_and_config()`.
///
/// Priority: CLI args > config file > fallbacks
pub fn resolve_auth_config(
    auth: &SharedAuthArgs,
    config: Option<&MyConfig>,
) -> Result<ResolvedAuthConfig> {
    // Resolve MCC customer ID
    let mcc_customer_id = if let Some(mcc_id) = &auth.mcc_id {
        log::debug!("Using MCC from --mcc-id argument: {}", mcc_id);
        validate_and_normalize_customer_id(mcc_id).context("Invalid --mcc-id argument")?
    } else if let Some(config_mcc) = config.and_then(|c| c.mcc_id.as_ref()) {
        log::debug!("Using MCC from config profile: {}", config_mcc);
        validate_and_normalize_customer_id(config_mcc).context("Invalid mcc_id in config file")?
    } else if let Some(customer_id) = &auth.customer_id {
        log::warn!(
            "No --mcc-id specified. Using --customer-id ({}) as MCC. \
             This assumes the account is not under a manager account.",
            customer_id
        );
        validate_and_normalize_customer_id(customer_id).context("Invalid --customer-id argument")?
    } else if let Some(config_customer_id) = config.and_then(|c| c.customer_id.as_ref()) {
        log::warn!(
            "No mcc_id specified. Using customer_id ({}) from config as MCC.",
            config_customer_id
        );
        validate_and_normalize_customer_id(config_customer_id)
            .context("Invalid customer_id in config file")?
    } else {
        return Err(anyhow::anyhow!(
            "MCC customer ID required. Provide one of:\n  \
             1. CLI: --mcc-id <MCC_ID>\n  \
             2. Config profile with mcc_id: --profile <PROFILE_NAME>\n  \
             3. For solo accounts: --customer-id <CUSTOMER_ID>"
        ));
    };

    // Resolve user email
    let user_email = auth
        .user_email
        .clone()
        .or_else(|| config.and_then(|c| c.user_email.clone()));

    // Resolve token cache filename
    let explicit_token_cache = config.and_then(|c| c.token_cache_filename.clone());
    let token_cache_filename = if let Some(explicit_cache) = explicit_token_cache {
        explicit_cache
    } else if let Some(email) = user_email.as_ref() {
        crate::googleads_api::generate_token_cache_filename(email)
    } else {
        return Err(anyhow::anyhow!(
            "User email or explicit token cache filename required for authentication."
        ));
    };

    // Resolve customer_id
    let customer_id = auth
        .customer_id
        .as_ref()
        .or_else(|| config.and_then(|c| c.customer_id.as_ref()))
        .map(|id| validate_and_normalize_customer_id(id).context("Invalid customer_id"))
        .transpose()?;

    // Dev token from config only
    let dev_token = config.and_then(|c| c.dev_token.clone());

    Ok(ResolvedAuthConfig {
        mcc_customer_id,
        user_email,
        customer_id,
        token_cache_filename,
        dev_token,
        remote_auth: auth.remote_auth,
    })
}

/// Load a config profile by name.
/// Moved from mcc-gaql/src/config.rs:load().
pub fn load_profile(profile: &str) -> Result<MyConfig> {
    use figment::{
        providers::{Env, Format, Toml},
        Figment,
    };

    let mut figment: Figment = Figment::new();

    if let Some(config_file_path) = config_file_path(crate::config::TOML_CONFIG_FILENAME) {
        if !config_file_path.exists() {
            return Err(anyhow::anyhow!(
                "Config file not found at: {}\n\
                 Expected format: [profile_name] sections in TOML\n\
                 Check your configuration file.",
                config_file_path.display()
            ));
        }

        log::debug!("Loading config file: {:?}", config_file_path);
        figment = figment.merge(Toml::file(&config_file_path).nested());
    } else {
        return Err(anyhow::anyhow!(
            "Could not determine config directory path for profile '{}'",
            profile
        ));
    }

    // merge in ENV VAR Overrides
    figment = figment.merge(Env::prefixed(crate::config::ENV_VAR_PREFIX));

    // Extract the profile with better error context
    figment.select(profile).extract().map_err(|e| {
        let config_path = config_file_path(crate::config::TOML_CONFIG_FILENAME)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        anyhow::anyhow!(
            "Failed to load profile '{}' from config file: {}\n\
                 Error: {}\n\
                 \n\
                 Possible issues:\n\
                 - Profile '{}' may not exist in the config file\n\
                 - Required fields may be missing (mcc_id is mandatory)\n\
                 - TOML syntax may be invalid\n\
                 \n\
                 Check your config file format and ensure the profile exists.",
            profile,
            config_path,
            e,
            profile
        )
    })
}

/// List all available profiles from the config file.
/// Moved from mcc-gaql/src/config.rs:list_profiles().
pub fn list_profiles() -> Result<Vec<String>> {
    let config_path = config_file_path(crate::config::TOML_CONFIG_FILENAME)
        .ok_or_else(|| anyhow::anyhow!("Unable to determine config directory"))?;

    if !config_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&config_path)?;
    let toml_table: toml::map::Map<String, toml::Value> = toml::from_str(&content)?;

    Ok(toml_table.keys().map(|k| k.to_string()).collect())
}

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};
use std::fs;

use mcc_gaql_common::config::{
    ENV_VAR_PREFIX, MyConfig, TOML_CONFIG_FILENAME, validate_and_normalize_customer_id,
};
use mcc_gaql_common::paths::config_file_path;

const CRATE_NAME: &str = "mcc-gaql";

/// Resolved runtime configuration combining CLI args and config file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedConfig {
    pub mcc_customer_id: String,
    pub user_email: Option<String>,
    pub customer_id: Option<String>,
    pub format: String,
    pub keep_going: bool,
    pub token_cache_filename: String,
    pub queries_filename: Option<String>,
    pub customerids_filename: Option<String>,
    pub dev_token: Option<String>,
    pub field_metadata_cache: String,
    pub field_metadata_ttl_days: i64,
    pub remote_auth: bool,
}

impl ResolvedConfig {
    /// Create resolved config from CLI args and optional config file
    pub fn from_args_and_config(
        args: &crate::args::Cli,
        config: Option<MyConfig>,
    ) -> anyhow::Result<Self> {
        use anyhow::Context;

        // Resolve MCC with explicit priority and logging
        let mcc_customer_id = if let Some(mcc_id) = &args.mcc_id {
            // Explicit --mcc-id takes highest priority
            log::debug!("Using MCC from --mcc-id argument: {}", mcc_id);
            validate_and_normalize_customer_id(mcc_id).context("Invalid --mcc-id argument")?
        } else if let Some(config_mcc) = config.as_ref().and_then(|c| c.mcc_id.as_ref()) {
            // Config file MCC is second priority
            log::debug!("Using MCC from config profile: {}", config_mcc);
            validate_and_normalize_customer_id(config_mcc)
                .context("Invalid mcc_id in config file")?
        } else if let Some(customer_id) = &args.customer_id {
            // Fallback: use CLI customer_id as MCC (for solo accounts)
            log::warn!(
                "No --mcc-id specified. Using --customer-id ({}) as MCC. \
                 This assumes the account is not under a manager account. \
                 Use --mcc-id explicitly if this account has a manager.",
                customer_id
            );
            validate_and_normalize_customer_id(customer_id)
                .context("Invalid --customer-id argument")?
        } else if let Some(config_customer_id) =
            config.as_ref().and_then(|c| c.customer_id.as_ref())
        {
            // Fallback: use config customer_id as MCC (for solo accounts)
            log::warn!(
                "No mcc_id specified. Using customer_id ({}) from config as MCC. \
                 This assumes the account is not under a manager account. \
                 Use mcc_id explicitly in config if this account has a manager.",
                config_customer_id
            );
            validate_and_normalize_customer_id(config_customer_id)
                .context("Invalid customer_id in config file")?
        } else {
            // No MCC available anywhere
            return Err(anyhow::anyhow!(
                "MCC customer ID required. Provide one of:\n  \
                 1. CLI argument: --mcc-id <MCC_ID>\n  \
                 2. Config profile with mcc_id: --profile <PROFILE_NAME>\n  \
                 3. For solo accounts: --customer-id <CUSTOMER_ID> (will be used as MCC)\n  \
                 4. For solo accounts: customer_id in config profile (will be used as MCC)"
            ));
        };

        // Resolve user email: CLI > config
        let user_email = args
            .user_email
            .clone()
            .or_else(|| config.as_ref().and_then(|c| c.user_email.clone()));

        // Check if there's an explicit token cache filename from config
        let explicit_token_cache = config.as_ref().and_then(|c| c.token_cache_filename.clone());

        // Resolve token cache filename with priority:
        // 1. Explicit token cache filename from config (highest priority)
        // 2. Auto-generated from user email
        // 3. ERROR if neither is available
        let token_cache_filename = if let Some(explicit_cache) = explicit_token_cache {
            explicit_cache
        } else if let Some(email) = user_email.as_ref() {
            crate::googleads::generate_token_cache_filename(email)
        } else {
            return Err(anyhow::anyhow!(
                "User email or explicit token cache filename required for authentication.\n\
                 Please provide one of:\n  \
                 1. User email: --user-email <EMAIL>\n  \
                 2. User email in config profile: user_email field\n  \
                 3. Explicit token cache in config profile: token_cache_filename field\n\n\
                 A default token cache file will NOT be used as it may not have \
                 the correct permissions for the account being queried."
            ));
        };

        // Resolve customer_id: CLI > config
        let customer_id = args
            .customer_id
            .as_ref()
            .or_else(|| config.as_ref().and_then(|c| c.customer_id.as_ref()))
            .map(|id| validate_and_normalize_customer_id(id).context("Invalid customer_id"))
            .transpose()?;

        // Resolve format: CLI > config > default ("table")
        let format = args
            .format
            .map(|f| match f {
                crate::args::OutputFormat::Table => "table".to_string(),
                crate::args::OutputFormat::Csv => "csv".to_string(),
                crate::args::OutputFormat::Json => "json".to_string(),
            })
            .or_else(|| config.as_ref().and_then(|c| c.format.clone()))
            .unwrap_or_else(|| "table".to_string());

        // Resolve keep_going: CLI flag > config > default (false)
        let keep_going =
            args.keep_going || config.as_ref().and_then(|c| c.keep_going).unwrap_or(false);

        // Config file fields (only available if profile specified)
        let queries_filename = config.as_ref().and_then(|c| c.queries_filename.clone());
        let customerids_filename = config.as_ref().and_then(|c| c.customerids_filename.clone());
        let dev_token = config.as_ref().and_then(|c| c.dev_token.clone());

        // Field metadata cache settings
        let field_metadata_cache = config
            .as_ref()
            .and_then(|c| c.field_metadata_cache.clone())
            .or_else(|| {
                mcc_gaql_common::paths::field_metadata_cache_path()
                    .ok()
                    .map(|p| p.display().to_string())
            })
            .unwrap_or_else(|| "~/.cache/mcc-gaql/field_metadata.json".to_string());

        let field_metadata_ttl_days = config
            .as_ref()
            .and_then(|c| c.field_metadata_ttl_days)
            .unwrap_or(30);

        // Resolve remote_auth: CLI flag only
        let remote_auth = args.remote_auth;

        Ok(Self {
            mcc_customer_id,
            user_email,
            customer_id,
            format,
            keep_going,
            token_cache_filename,
            queries_filename,
            customerids_filename,
            dev_token,
            field_metadata_cache,
            field_metadata_ttl_days,
            remote_auth,
        })
    }

    /// Validate that resolved config supports the requested operation mode
    pub fn validate_for_operation(&self, args: &crate::args::Cli) -> anyhow::Result<()> {
        // Validate that either user context is specified OR a valid token cache file exists
        if self.user_email.is_none() {
            let token_cache_path = config_file_path(&self.token_cache_filename);
            let token_cache_exists = token_cache_path
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false);

            if !token_cache_exists {
                return Err(anyhow::anyhow!(
                    "User context or existing token cache required for authentication.\n\
                     A user email must be specified to identify which Google Ads account credentials to use,\n\
                     OR a valid token cache file must exist.\n\
                     Please provide one of:\n  \
                     1. CLI argument: --user-email <EMAIL>\n  \
                     2. Config profile with 'user_email' field: --profile <PROFILE_NAME>\n  \
                     3. Existing token cache file: {}\n\n\
                     Without a user context or existing token cache, authentication cannot proceed.",
                    token_cache_path
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                ));
            } else {
                log::info!(
                    "Using existing token cache: {}",
                    token_cache_path.unwrap().display()
                );
            }
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
            && self.customer_id.is_none()
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

    let mut figment: Figment = Figment::new();

    if let Some(config_file_path) = config_file_path(TOML_CONFIG_FILENAME) {
        if !config_file_path.exists() {
            return Err(anyhow::anyhow!(
                "Config file not found at: {}\n\
                 Expected format: [profile_name] sections in TOML\n\
                 Run with --help for configuration instructions",
                config_file_path.display()
            ));
        }

        log::debug!("Loading config file: {:?}", config_file_path);
        figment = figment.merge(Toml::file(&config_file_path).nested());
    } else {
        return Err(anyhow::anyhow!(
            "Could not determine config directory path for profile '{}'\n\
             Expected config at: ~/.config/{}/{}",
            profile,
            CRATE_NAME,
            TOML_CONFIG_FILENAME
        ));
    }

    // merge in ENV VAR Overrides
    figment = figment.merge(Env::prefixed(ENV_VAR_PREFIX));

    // Extract the profile with better error context
    figment.select(profile).extract().map_err(|e| {
        let config_path = config_file_path(TOML_CONFIG_FILENAME)
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

/// Display profile configuration details
fn display_profile_config(profile: &str) -> anyhow::Result<()> {
    match load(profile) {
        Ok(config) => {
            println!("Profile Configuration:");
            if let Some(mcc_id) = &config.mcc_id {
                println!("  mcc_id: {}", mcc_id);
            } else {
                println!("  mcc_id: (not set, will use customer_id as MCC)");
            }

            if let Some(user) = &config.user_email {
                println!("  user: {}", user);
            } else {
                println!("  user: (not set)");
            }

            if let Some(customer_id) = &config.customer_id {
                println!("  customer_id: {}", customer_id);
            } else {
                println!("  customer_id: (not set)");
            }

            if let Some(token_cache) = &config.token_cache_filename {
                println!("  token_cache_filename: {}", token_cache);
            } else {
                println!("  token_cache_filename: (auto-generated from user email)");
            }

            if let Some(customerids) = &config.customerids_filename {
                println!("  customerids_filename: {}", customerids);
                if let Some(path) = config_file_path(customerids) {
                    println!("    Path: {}", path.display());
                    println!("    Exists: {}", path.exists());
                }
            } else {
                println!("  customerids_filename: (not set)");
            }

            if let Some(queries) = &config.queries_filename {
                println!("  queries_filename: {}", queries);
                if let Some(path) = config_file_path(queries) {
                    println!("    Path: {}", path.display());
                    println!("    Exists: {}", path.exists());
                }
            } else {
                println!("  queries_filename: (not set)");
            }

            if let Some(format) = &config.format {
                println!("  format: {}", format);
            } else {
                println!("  format: (not set, defaults to table)");
            }

            if let Some(keep_going) = config.keep_going {
                println!("  keep_going: {}", keep_going);
            } else {
                println!("  keep_going: (not set, defaults to false)");
            }
            Ok(())
        }
        Err(e) => {
            println!("Error loading profile: {}", e);
            Ok(())
        }
    }
}

/// Display configuration in a human-readable format
pub fn display_config(profile_name: Option<&str>) -> anyhow::Result<()> {
    println!("Configuration Details");
    println!("====================");
    println!();

    // Show config file location
    if let Some(config_path) = config_file_path(TOML_CONFIG_FILENAME) {
        println!("Config File: {}", config_path.display());
        if config_path.exists() {
            println!("  Status: Found");
        } else {
            println!("  Status: Not found");
        }
    } else {
        println!("Config File: Unable to determine config directory");
    }
    println!();

    // Show profile information
    if let Some(profile) = profile_name {
        println!("Profile: {}", profile);
        println!();
        display_profile_config(profile)?;
    } else {
        match list_profiles() {
            Ok(profiles) if !profiles.is_empty() => {
                println!("Profiles: {} found", profiles.len());
                println!();

                for (idx, profile) in profiles.iter().enumerate() {
                    if idx > 0 {
                        println!();
                        println!("---");
                        println!();
                    }
                    println!("Profile: {}", profile);
                    println!();
                    display_profile_config(profile)?;
                }
            }
            Ok(_) => {
                println!("Profiles: (none found)");
                println!();
                println!("No profiles configured in config file.");
                println!("Run 'mcc-gaql --setup' to create a new profile.");
            }
            Err(e) => {
                println!("Profiles: Error reading config file");
                println!("  Error: {}", e);
            }
        }
    }

    println!();
    println!("Environment Variable Overrides:");
    println!("  Prefix: {}", ENV_VAR_PREFIX);

    let env_vars = [
        "EMBED_CLIENT_SECRET",
        "DEV_TOKEN",
        "LOG_LEVEL",
        "QUERIES_FILENAME",
    ];

    let mut found_any = false;
    for var in &env_vars {
        let full_var = format!("{}{}", ENV_VAR_PREFIX, var);
        if let Ok(value) = std::env::var(&full_var) {
            println!("  {}: {}", full_var, value);
            found_any = true;
        }
    }

    if !found_any {
        println!("  (none set)");
    }

    Ok(())
}

/// List all available profiles from the config file
pub fn list_profiles() -> anyhow::Result<Vec<String>> {
    let config_path = config_file_path(TOML_CONFIG_FILENAME)
        .ok_or_else(|| anyhow::anyhow!("Unable to determine config directory"))?;

    if !config_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&config_path)?;
    let toml_table: toml::map::Map<String, toml::Value> = toml::from_str(&content)?;

    Ok(toml_table.keys().map(|k| k.to_string()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_myconfig_serialization_all_fields() {
        let config = MyConfig {
            mcc_id: Some("1234567890".to_string()),
            user_email: Some("user@example.com".to_string()),
            customer_id: Some("9876543210".to_string()),
            format: Some("json".to_string()),
            keep_going: Some(true),
            token_cache_filename: None,
            customerids_filename: Some("customerids.txt".to_string()),
            queries_filename: Some("query_cookbook.toml".to_string()),
            dev_token: Some("test-dev-token".to_string()),
            field_metadata_cache: None,
            field_metadata_ttl_days: None,
        };

        let toml_str = toml::to_string(&config).expect("Failed to serialize");
        let deserialized: MyConfig = toml::from_str(&toml_str).expect("Failed to deserialize");

        assert_eq!(config.mcc_id, deserialized.mcc_id);
        assert_eq!(config.user_email, deserialized.user_email);
        assert_eq!(config.customer_id, deserialized.customer_id);
        assert_eq!(config.format, deserialized.format);
        assert_eq!(config.keep_going, deserialized.keep_going);
        assert_eq!(
            config.token_cache_filename,
            deserialized.token_cache_filename
        );
        assert_eq!(
            config.customerids_filename,
            deserialized.customerids_filename
        );
        assert_eq!(config.queries_filename, deserialized.queries_filename);
    }

    #[test]
    fn test_resolved_config_serialization() {
        let config = ResolvedConfig {
            mcc_customer_id: "1234567890".to_string(),
            user_email: Some("user@example.com".to_string()),
            customer_id: Some("9876543210".to_string()),
            format: "json".to_string(),
            keep_going: true,
            token_cache_filename: "tokencache.json".to_string(),
            queries_filename: Some("query_cookbook.toml".to_string()),
            customerids_filename: Some("customerids.txt".to_string()),
            dev_token: Some("test-dev-token".to_string()),
            field_metadata_cache: "~/.cache/mcc-gaql/field_metadata.json".to_string(),
            field_metadata_ttl_days: 7,
            remote_auth: false,
        };

        let toml_str = toml::to_string(&config).expect("Failed to serialize");
        let deserialized: ResolvedConfig =
            toml::from_str(&toml_str).expect("Failed to deserialize");

        assert_eq!(config.mcc_customer_id, deserialized.mcc_customer_id);
        assert_eq!(config.user_email, deserialized.user_email);
        assert_eq!(config.customer_id, deserialized.customer_id);
        assert_eq!(config.format, deserialized.format);
        assert_eq!(config.keep_going, deserialized.keep_going);
        assert_eq!(
            config.token_cache_filename,
            deserialized.token_cache_filename
        );
        assert_eq!(config.queries_filename, deserialized.queries_filename);
        assert_eq!(
            config.customerids_filename,
            deserialized.customerids_filename
        );
    }

    #[test]
    fn test_validate_for_operation_with_customer_id_from_config() {
        use crate::args::Cli;

        let args = Cli {
            gaql_query: Some("SELECT campaign.name FROM campaign".to_string()),
            stored_query: None,
            output: None,
            format: None,
            profile: None,
            user_email: None,
            mcc_id: Some("1234567890".to_string()),
            customer_id: None,
            list_child_accounts: false,
            field_service: false,
            all_linked_child_accounts: false,
            keep_going: false,
            groupby: vec![],
            sortby: vec![],
            setup: false,
            show_config: false,
            refresh_field_cache: false,
            show_fields: None,
            export_field_metadata: false,
            show_resources: false,
            remote_auth: false,
        };

        let resolved_config = ResolvedConfig {
            mcc_customer_id: "1234567890".to_string(),
            user_email: Some("test@example.com".to_string()),
            customer_id: Some("9876543210".to_string()),
            format: "table".to_string(),
            keep_going: false,
            token_cache_filename: "test_token.json".to_string(),
            queries_filename: None,
            customerids_filename: None,
            dev_token: None,
            field_metadata_cache: "~/.cache/mcc-gaql/field_metadata.json".to_string(),
            field_metadata_ttl_days: 7,
            remote_auth: false,
        };

        let result = resolved_config.validate_for_operation(&args);

        assert!(
            result.is_ok(),
            "Validation should succeed when customer_id is in resolved config. Got error: {:?}",
            result.err()
        );
    }
}

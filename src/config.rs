use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
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
    /// MCC Account ID is mandatory
    pub mcc_customerid: String,
    /// Optional user email for OAuth2 (not required if valid token cache exists)
    pub user: Option<String>,
    /// Token Cache filename (optional - auto-generated from user if not specified)
    pub token_cache_filename: Option<String>,
    /// Optional file containing child customer_ids to query
    pub customerids_filename: Option<String>,
    /// Optional TOML file with stored queries
    pub queries_filename: Option<String>,
}

/// Resolved runtime configuration combining CLI args and config file
#[derive(Debug, Clone, Serialize, Deserialize)]
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
        use anyhow::Context;

        // Resolve MCC with explicit priority and logging
        let mcc_customer_id = if let Some(mcc) = &args.mcc {
            // Explicit --mcc takes highest priority
            log::debug!("Using MCC from --mcc argument: {}", mcc);
            validate_and_normalize_customer_id(mcc).context("Invalid --mcc argument")?
        } else if let Some(config_mcc) = config.as_ref().map(|c| &c.mcc_customerid) {
            // Config file MCC is second priority
            log::debug!("Using MCC from config profile: {}", config_mcc);
            validate_and_normalize_customer_id(config_mcc)
                .context("Invalid mcc_customerid in config file")?
        } else if let Some(customer_id) = &args.customer_id {
            // Fallback: use customer_id as MCC (for solo accounts)
            log::warn!(
                "No --mcc specified. Using --customer-id ({}) as MCC. \
                 This assumes the account is not under a manager account. \
                 Use --mcc explicitly if this account has a manager.",
                customer_id
            );
            validate_and_normalize_customer_id(customer_id)
                .context("Invalid --customer-id argument")?
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
        // Validate that either user context is specified OR a valid token cache file exists
        // If user email is not provided, check if token cache file exists
        if self.user_email.is_none() {
            // Check if token cache file exists
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
                     1. CLI argument: --user <EMAIL>\n  \
                     2. Config profile with 'user' field: --profile <PROFILE_NAME>\n  \
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
        // Try to provide helpful context about what went wrong
        let config_path = config_file_path(TOML_CONFIG_FILENAME)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        anyhow::anyhow!(
            "Failed to load profile '{}' from config file: {}\n\
                 Error: {}\n\
                 \n\
                 Possible issues:\n\
                 - Profile '{}' may not exist in the config file\n\
                 - Required fields may be missing (mcc_customerid is mandatory)\n\
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
    // Try to load the profile
    match load(profile) {
        Ok(config) => {
            println!("Profile Configuration:");
            println!("  mcc_customerid: {}", config.mcc_customerid);

            if let Some(user) = &config.user {
                println!("  user: {}", user);
            } else {
                println!("  user: (not set)");
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
        // Show specific profile
        println!("Profile: {}", profile);
        println!();
        display_profile_config(profile)?;
    } else {
        // Show all profiles
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

    // Check for common environment variables
    let env_vars = [
        "MCC_CUSTOMERID",
        "USER",
        "TOKEN_CACHE_FILENAME",
        "CUSTOMERIDS_FILENAME",
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

    println!();
    println!("Common Files:");

    // Check for common files
    let common_files = [
        ("clientsecret.json", "OAuth2 credentials"),
        ("query_cookbook.toml", "Query cookbook"),
        ("customerids.txt", "Customer IDs"),
    ];

    for (filename, description) in &common_files {
        if let Some(path) = config_file_path(filename) {
            let exists = path.exists();
            let status = if exists { "Found" } else { "Not found" };
            println!("  {} ({}): {}", description, status, path.display());
        }
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

/// get the platform-correct config file path
pub fn config_file_path(filename: &str) -> Option<PathBuf> {
    dirs::config_dir().map(move |mut path| {
        path.push(CRATE_NAME);
        path.push(filename);
        path
    })
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
    fn test_validate_customer_id_with_multiple_hyphens() {
        assert_eq!(
            validate_and_normalize_customer_id("1-2-3-4-5-6-7-8-9-0").unwrap(),
            "1234567890"
        );
    }

    #[test]
    fn test_validate_customer_id_invalid_chars() {
        let result = validate_and_normalize_customer_id("123abc7890");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid customer ID format")
        );
    }

    #[test]
    fn test_validate_customer_id_invalid_chars_with_spaces() {
        let result = validate_and_normalize_customer_id("123 456 7890");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid customer ID format")
        );
    }

    #[test]
    fn test_validate_customer_id_too_short() {
        let result = validate_and_normalize_customer_id("123456789");
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid customer ID length"));
        assert!(error_msg.contains("Found 9 digits"));
    }

    #[test]
    fn test_validate_customer_id_too_long() {
        let result = validate_and_normalize_customer_id("12345678901");
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("Invalid customer ID length"));
        assert!(error_msg.contains("Found 11 digits"));
    }

    #[test]
    fn test_validate_customer_id_empty() {
        let result = validate_and_normalize_customer_id("");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid customer ID length")
        );
    }

    #[test]
    fn test_myconfig_serialization_all_fields() {
        let config = MyConfig {
            mcc_customerid: "1234567890".to_string(),
            user: Some("user@example.com".to_string()),
            token_cache_filename: None,
            customerids_filename: Some("customerids.txt".to_string()),
            queries_filename: Some("query_cookbook.toml".to_string()),
        };

        // Serialize to TOML string
        let toml_str = toml::to_string(&config).expect("Failed to serialize");

        // Deserialize back
        let deserialized: MyConfig = toml::from_str(&toml_str).expect("Failed to deserialize");

        // Verify round-trip
        assert_eq!(config.mcc_customerid, deserialized.mcc_customerid);
        assert_eq!(config.user, deserialized.user);
        assert_eq!(config.token_cache_filename, deserialized.token_cache_filename);
        assert_eq!(config.customerids_filename, deserialized.customerids_filename);
        assert_eq!(config.queries_filename, deserialized.queries_filename);
    }

    #[test]
    fn test_myconfig_serialization_minimal() {
        let config = MyConfig {
            mcc_customerid: "1234567890".to_string(),
            user: Some("user@example.com".to_string()),
            token_cache_filename: None,
            customerids_filename: None,
            queries_filename: None,
        };

        // Serialize to TOML string
        let toml_str = toml::to_string(&config).expect("Failed to serialize");

        // Verify optional fields are omitted (not present as keys)
        assert!(!toml_str.contains("token_cache_filename"));
        assert!(!toml_str.contains("customerids_filename"));
        assert!(!toml_str.contains("queries_filename"));

        // Deserialize back
        let deserialized: MyConfig = toml::from_str(&toml_str).expect("Failed to deserialize");

        // Verify round-trip
        assert_eq!(config.mcc_customerid, deserialized.mcc_customerid);
        assert_eq!(config.user, deserialized.user);
        assert_eq!(config.token_cache_filename, None);
        assert_eq!(config.customerids_filename, None);
        assert_eq!(config.queries_filename, None);
    }

    #[test]
    fn test_resolved_config_serialization() {
        let config = ResolvedConfig {
            mcc_customer_id: "1234567890".to_string(),
            user_email: Some("user@example.com".to_string()),
            token_cache_filename: "tokencache.json".to_string(),
            queries_filename: Some("query_cookbook.toml".to_string()),
            customerids_filename: Some("customerids.txt".to_string()),
        };

        // Serialize to TOML string
        let toml_str = toml::to_string(&config).expect("Failed to serialize");

        // Deserialize back
        let deserialized: ResolvedConfig = toml::from_str(&toml_str).expect("Failed to deserialize");

        // Verify round-trip
        assert_eq!(config.mcc_customer_id, deserialized.mcc_customer_id);
        assert_eq!(config.user_email, deserialized.user_email);
        assert_eq!(config.token_cache_filename, deserialized.token_cache_filename);
        assert_eq!(config.queries_filename, deserialized.queries_filename);
        assert_eq!(config.customerids_filename, deserialized.customerids_filename);
    }

    #[test]
    fn test_myconfig_with_user_field() {
        // Test that configs with user field can be properly serialized/deserialized
        let toml_str = r#"
            mcc_customerid = "1234567890"
            user = "user@example.com"
        "#;

        let config: MyConfig = toml::from_str(toml_str).expect("Failed to deserialize");

        assert_eq!(config.mcc_customerid, "1234567890");
        assert_eq!(config.user, Some("user@example.com".to_string()));
        assert_eq!(config.token_cache_filename, None);
        assert_eq!(config.customerids_filename, None);
        assert_eq!(config.queries_filename, None);
    }

    #[test]
    fn test_myconfig_backwards_compatibility() {
        // Test that old configs without user field can still be loaded
        let toml_str = r#"
            mcc_customerid = "1234567890"
            token_cache_filename = "tokencache.json"
        "#;

        let config: MyConfig = toml::from_str(toml_str).expect("Failed to deserialize");

        assert_eq!(config.mcc_customerid, "1234567890");
        assert_eq!(config.user, None);
        assert_eq!(config.token_cache_filename, Some("tokencache.json".to_string()));
    }
}

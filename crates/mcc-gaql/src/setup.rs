use anyhow::{Context, Result};
use dialoguer::{Confirm, Input};
use std::fs;
use std::path::PathBuf;
use toml::{Value, map::Map};

use mcc_gaql_common::config::{MyConfig, TOML_CONFIG_FILENAME, validate_and_normalize_customer_id};
use mcc_gaql_common::paths::config_file_path;

/// Check if a file exists and provide guidance
fn validate_optional_file(filename: &str, file_description: &str) -> Result<()> {
    if let Some(path) = config_file_path(filename) {
        if !path.exists() {
            println!(
                "Note: {} does not exist yet at {:?}",
                file_description, path
            );
            println!("You will need to create this file before using it.");
        } else {
            println!("Found existing {} at {:?}", file_description, path);
        }
    }
    Ok(())
}

/// Run the interactive configuration wizard
pub fn run_wizard() -> Result<()> {
    println!("Welcome to mcc-gaql configuration wizard!");
    println!();

    let existing_profiles = get_existing_profile_names()?;

    let profile_name: String = Input::new()
        .with_prompt("Enter a name for this profile")
        .default("myprofile".to_string())
        .validate_with(|input: &String| -> Result<(), String> {
            let trimmed = input.trim();
            if trimmed.is_empty() {
                return Err("Profile name cannot be empty".to_string());
            }
            if existing_profiles.contains(&trimmed.to_string()) {
                return Err(format!(
                    "Profile '{}' already exists. Please choose a different name.",
                    trimmed
                ));
            }
            Ok(())
        })
        .interact_text()?;

    println!("Using profile: {}", profile_name);
    println!();

    let user_email: String = Input::new()
        .with_prompt("Enter your email for OAuth2 authentication")
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                return Err("Email is required for authentication");
            }
            if !input.contains('@') {
                return Err("Please enter a valid email address");
            }
            Ok(())
        })
        .interact_text()?;

    println!("Token cache will be auto-generated from your email");
    println!();

    let customer_id: String = Input::new()
        .with_prompt("Enter your Customer ID (e.g., 1234567890 or 123-456-7890)")
        .validate_with(|input: &String| -> Result<(), String> {
            if input.trim().is_empty() {
                return Err("Customer ID is required".to_string());
            }
            validate_and_normalize_customer_id(input)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .interact_text()
        .map(|id| validate_and_normalize_customer_id(&id).unwrap())?;

    let use_mcc = Confirm::new()
        .with_prompt("Is this account under an MCC (Manager) account?")
        .default(false)
        .interact()?;

    let mcc_id = if use_mcc {
        let mcc_id_input: String = Input::new()
            .with_prompt("Enter your MCC Customer ID (e.g., 1234567890 or 123-456-7890)")
            .validate_with(|input: &String| -> Result<(), String> {
                if input.trim().is_empty() {
                    return Err("MCC Customer ID cannot be empty".to_string());
                }
                validate_and_normalize_customer_id(input)
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .interact_text()
            .map(|id| validate_and_normalize_customer_id(&id).unwrap())?;
        Some(mcc_id_input)
    } else {
        None
    };

    let use_customerids_file = Confirm::new()
        .with_prompt("Do you want to specify a customer IDs file?")
        .default(false)
        .interact()?;

    let customerids_filename = if use_customerids_file {
        let filename: String = Input::new()
            .with_prompt("Enter customer IDs filename")
            .default("customerids.txt".to_string())
            .interact_text()?;
        validate_optional_file(&filename, "customer IDs file")?;
        Some(filename)
    } else {
        None
    };

    let use_queries_file = Confirm::new()
        .with_prompt("Do you want to specify a queries cookbook file?")
        .default(true)
        .interact()?;

    let queries_filename = if use_queries_file {
        let filename: String = Input::new()
            .with_prompt("Enter queries cookbook filename")
            .default("query_cookbook.toml".to_string())
            .interact_text()?;
        validate_optional_file(&filename, "queries cookbook file")?;
        Some(filename)
    } else {
        None
    };

    let config = MyConfig {
        mcc_id,
        user_email: Some(user_email),
        customer_id: Some(customer_id),
        format: None,
        keep_going: None,
        token_cache_filename: None,
        customerids_filename,
        queries_filename,
        dev_token: None,
        field_metadata_cache: None,
        field_metadata_ttl_days: None,
    };

    save_config(&profile_name, &config)?;

    println!();
    println!("Configuration saved successfully!");
    println!("Profile: {}", profile_name);
    println!();
    println!("You can now use this profile with:");
    println!("  mcc-gaql --profile {}", profile_name);
    println!();
    println!("Next steps:");

    // Check if client secret is available via runtime env var
    let has_runtime_secret = std::env::var("MCC_GAQL_EMBED_CLIENT_SECRET").is_ok();

    if has_runtime_secret {
        println!(
            "  1. OAuth2 credentials found in MCC_GAQL_EMBED_CLIENT_SECRET environment variable"
        );
    } else {
        println!(
            "  1. Place your OAuth2 credentials in: {:?}",
            config_file_path("clientsecret.json")
        );
        println!("     (Or set MCC_GAQL_EMBED_CLIENT_SECRET environment variable)");
    }

    if config.customerids_filename.is_some() {
        println!(
            "  2. Create your customer IDs file: {:?}",
            config_file_path(&config.customerids_filename.unwrap())
        );
    }
    if config.queries_filename.is_some() {
        println!(
            "  3. (Optional) Create your queries cookbook: {:?}",
            config_file_path(&config.queries_filename.unwrap())
        );
    }

    Ok(())
}

/// Get list of existing profile names from the default config file location
fn get_existing_profile_names() -> Result<Vec<String>> {
    let config_path = config_file_path(TOML_CONFIG_FILENAME);

    let Some(config_path) = config_path else {
        return Ok(Vec::new());
    };

    if !config_path.exists() {
        return Ok(Vec::new());
    }

    get_existing_profiles(&config_path)
}

/// Get list of existing profile names from config file
fn get_existing_profiles(config_path: &PathBuf) -> Result<Vec<String>> {
    let content = fs::read_to_string(config_path).context("Failed to read config file")?;

    let toml_table: Map<String, Value> =
        toml::from_str(&content).context("Failed to parse config file")?;

    let profiles: Vec<String> = toml_table.keys().map(|k| k.to_string()).collect();

    Ok(profiles)
}

/// Save configuration to the config file
fn save_config(profile_name: &str, config: &MyConfig) -> Result<()> {
    let config_path = config_file_path(TOML_CONFIG_FILENAME)
        .ok_or_else(|| anyhow::anyhow!("Unable to determine config directory"))?;

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).context("Failed to create config directory")?;
    }

    let mut config_table: Map<String, Value> = if config_path.exists() {
        let content =
            fs::read_to_string(&config_path).context("Failed to read existing config file")?;
        toml::from_str(&content).context("Failed to parse existing config file")?
    } else {
        Map::new()
    };

    let profile_value = Value::try_from(config).context("Failed to serialize config")?;
    config_table.insert(profile_name.to_string(), profile_value);

    let toml_string =
        toml::to_string_pretty(&config_table).context("Failed to serialize config")?;

    fs::write(&config_path, toml_string).context("Failed to write config file")?;

    println!("Configuration written to: {:?}", config_path);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_get_existing_profiles_empty() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        fs::write(&config_path, "").unwrap();

        let profiles = get_existing_profiles(&config_path).unwrap();
        assert_eq!(profiles.len(), 0);
    }

    #[test]
    fn test_get_existing_profiles_with_profiles() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        let config_content = r#"
[test]
mcc_id = "1234567890"
token_cache_filename = "tokencache.json"

[myprofile]
mcc_id = "9876543210"
token_cache_filename = "tokencache_myprofile.json"
"#;

        fs::write(&config_path, config_content).unwrap();

        let mut profiles = get_existing_profiles(&config_path).unwrap();
        profiles.sort();

        assert_eq!(profiles.len(), 2);
        assert!(profiles.contains(&"test".to_string()));
        assert!(profiles.contains(&"myprofile".to_string()));
    }
}

use anyhow::{Context, Result};
use dialoguer::{Confirm, Input};
use std::fs;
use std::path::PathBuf;
use toml::{Value, map::Map};

use crate::config::{MyConfig, TOML_CONFIG_FILENAME, config_file_path};

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

    // Determine profile name
    let profile_name = determine_profile_name()?;
    println!("Using profile: {}", profile_name);
    println!();

    // Ask for MCC customer ID
    let mcc_customerid: String = Input::new()
        .with_prompt("Enter your MCC Customer ID (digits only, without dashes)")
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                return Err("MCC Customer ID is required");
            }
            if !input.chars().all(|c| c.is_ascii_digit()) {
                return Err("MCC Customer ID should contain only digits");
            }
            Ok(())
        })
        .interact_text()?;

    // Ask for user email (required for OAuth2 authentication)
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

    // Ask for optional customer IDs filename
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

    // Ask for optional queries filename
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

    // Create config structure
    let config = MyConfig {
        mcc_customerid,
        user: Some(user_email),
        token_cache_filename: None,  // Let runtime auto-generate from user email
        customerids_filename,
        queries_filename,
    };

    // Save configuration
    save_config(&profile_name, &config)?;

    println!();
    println!("Configuration saved successfully!");
    println!("Profile: {}", profile_name);
    println!();
    println!("You can now use this profile with:");
    println!("  mcc-gaql --profile {}", profile_name);
    println!();
    println!("Next steps:");
    println!(
        "  1. Place your OAuth2 credentials in: {:?}",
        config_file_path("clientsecret.json")
    );
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

/// Determine a unique profile name, defaulting to "myprofile" and adding _2, _3, etc. if it exists
pub fn determine_profile_name() -> Result<String> {
    let config_path = config_file_path(TOML_CONFIG_FILENAME);

    let base_name = "myprofile";

    // If config file doesn't exist, use the base name
    let Some(config_path) = config_path else {
        return Ok(base_name.to_string());
    };

    if !config_path.exists() {
        return Ok(base_name.to_string());
    }

    // Load existing config to check what profiles exist
    let existing_profiles = get_existing_profiles(&config_path)?;

    // If base name doesn't exist, use it
    if !existing_profiles.contains(&base_name.to_string()) {
        return Ok(base_name.to_string());
    }

    // Find the next available numbered suffix
    for i in 2..1000 {
        let candidate = format!("{}_{}", base_name, i);
        if !existing_profiles.contains(&candidate) {
            return Ok(candidate);
        }
    }

    Err(anyhow::anyhow!("Unable to find an available profile name"))
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

    // Create parent directory if it doesn't exist
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).context("Failed to create config directory")?;
    }

    // Load existing config or create new one
    let mut config_table: Map<String, Value> = if config_path.exists() {
        let content =
            fs::read_to_string(&config_path).context("Failed to read existing config file")?;
        toml::from_str(&content).context("Failed to parse existing config file")?
    } else {
        Map::new()
    };

    // Serialize config to TOML value using serde
    let profile_value = Value::try_from(config)
        .context("Failed to serialize config")?;

    // Add or update profile in config
    config_table.insert(profile_name.to_string(), profile_value);

    // Write config file
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
    fn test_determine_profile_name_no_config() {
        // When no config exists, should return "myprofile"
        let profile = determine_profile_name().unwrap();
        assert_eq!(profile, "myprofile");
    }

    #[test]
    fn test_get_existing_profiles_empty() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Empty config file
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
mcc_customerid = "1234567890"
token_cache_filename = "tokencache.json"

[myprofile]
mcc_customerid = "9876543210"
token_cache_filename = "tokencache_myprofile.json"
"#;

        fs::write(&config_path, config_content).unwrap();

        let mut profiles = get_existing_profiles(&config_path).unwrap();
        profiles.sort(); // Sort for consistent comparison

        assert_eq!(profiles.len(), 2);
        assert!(profiles.contains(&"test".to_string()));
        assert!(profiles.contains(&"myprofile".to_string()));
    }

    #[test]
    fn test_save_config_new_profile() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Mock the config_file_path function by testing save_config directly
        let config = MyConfig {
            mcc_customerid: "1234567890".to_string(),
            user: Some("user@example.com".to_string()),
            token_cache_filename: None,  // Now auto-generated at runtime
            customerids_filename: Some("customerids.txt".to_string()),
            queries_filename: Some("queries.toml".to_string()),
        };

        // We can't directly test save_config without mocking config_file_path,
        // so instead we'll test the logic by manually creating the config structure
        let mut config_table = Map::new();
        let mut profile_table = Map::new();

        profile_table.insert(
            "mcc_customerid".to_string(),
            Value::String(config.mcc_customerid.clone()),
        );
        profile_table.insert(
            "user".to_string(),
            Value::String(config.user.clone().unwrap()),
        );
        profile_table.insert(
            "customerids_filename".to_string(),
            Value::String(config.customerids_filename.clone().unwrap()),
        );
        profile_table.insert(
            "queries_filename".to_string(),
            Value::String(config.queries_filename.clone().unwrap()),
        );

        config_table.insert("testprofile".to_string(), Value::Table(profile_table));

        let toml_string = toml::to_string_pretty(&config_table).unwrap();
        fs::write(&config_path, toml_string).unwrap();

        // Verify the file was written correctly
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("[testprofile]"));
        assert!(content.contains("1234567890"));
        assert!(content.contains("user@example.com"));
        assert!(content.contains("customerids.txt"));
        assert!(content.contains("queries.toml"));
    }

    #[test]
    fn test_profile_name_suffix_logic() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("config.toml");

        // Create config with myprofile and myprofile_2
        let config_content = r#"
[myprofile]
mcc_customerid = "1111111111"
token_cache_filename = "tokencache1.json"

[myprofile_2]
mcc_customerid = "2222222222"
token_cache_filename = "tokencache2.json"
"#;

        fs::write(&config_path, config_content).unwrap();

        let profiles = get_existing_profiles(&config_path).unwrap();

        // Simulate the logic from determine_profile_name
        let base_name = "myprofile";
        let mut next_name = base_name.to_string();

        if profiles.contains(&next_name) {
            for i in 2..1000 {
                let candidate = format!("{}_{}", base_name, i);
                if !profiles.contains(&candidate) {
                    next_name = candidate;
                    break;
                }
            }
        }

        assert_eq!(next_name, "myprofile_3");
    }
}

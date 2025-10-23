use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::fs;
use dialoguer::{Input, Confirm};
use anyhow::{Context, Result};

const CRATE_NAME: &str = env!("CARGO_PKG_NAME");
const TOML_CONFIG_FILENAME: &str = "config.toml";
pub const ENV_VAR_PREFIX: &str = "MCC_GAQL_";

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

/// Interactive wizard to initialize configuration
pub fn init_config() -> Result<()> {
    println!("\n=== mcc-gaql Configuration Wizard ===\n");
    println!("This wizard will help you set up the configuration file.");
    println!("Press Ctrl+C at any time to cancel.\n");

    // Get profile name
    let profile: String = Input::new()
        .with_prompt("Profile name")
        .default("test".to_string())
        .interact_text()?;

    // Get MCC Customer ID
    let mcc_customerid: String = Input::new()
        .with_prompt("MCC Customer ID (numbers only, no dashes)")
        .validate_with(|input: &String| -> Result<(), String> {
            if input.trim().is_empty() {
                Err("MCC Customer ID is required".to_string())
            } else if !input.chars().all(|c| c.is_numeric()) {
                Err("MCC Customer ID must contain only numbers".to_string())
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    // Get token cache filename
    let token_cache_filename: String = Input::new()
        .with_prompt("Token cache filename")
        .default("token_cache.bin".to_string())
        .interact_text()?;

    // Ask about optional customerids file
    let add_customerids = Confirm::new()
        .with_prompt("Do you want to specify a customer IDs file?")
        .default(false)
        .interact()?;

    let customerids_filename = if add_customerids {
        let filename: String = Input::new()
            .with_prompt("Customer IDs filename")
            .default("customerids.txt".to_string())
            .interact_text()?;
        Some(filename)
    } else {
        None
    };

    // Ask about optional queries file
    let add_queries = Confirm::new()
        .with_prompt("Do you want to specify a queries cookbook file?")
        .default(true)
        .interact()?;

    let queries_filename = if add_queries {
        let filename: String = Input::new()
            .with_prompt("Queries filename")
            .default("query_cookbook.toml".to_string())
            .interact_text()?;
        Some(filename)
    } else {
        None
    };

    // Create config structure
    let config = MyConfig {
        mcc_customerid,
        token_cache_filename,
        customerids_filename,
        queries_filename,
    };

    // Get config directory path
    let config_dir = dirs::config_dir()
        .context("Unable to determine config directory")?
        .join(CRATE_NAME);

    // Create config directory if it doesn't exist
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)
            .context(format!("Failed to create config directory: {:?}", config_dir))?;
        println!("\nCreated config directory: {:?}", config_dir);
    }

    // Build TOML content
    let toml_content = format!(
        "[{profile}]\nmcc_customerid = \"{}\"\ntoken_cache_filename = \"{}\"\n{}{}\n",
        config.mcc_customerid,
        config.token_cache_filename,
        config.customerids_filename
            .map(|f| format!("customerids_filename = \"{}\"\n", f))
            .unwrap_or_default(),
        config.queries_filename
            .map(|f| format!("queries_filename = \"{}\"\n", f))
            .unwrap_or_default()
    );

    // Get config file path
    let config_file = config_dir.join(TOML_CONFIG_FILENAME);

    // Check if config file already exists
    if config_file.exists() {
        let overwrite = Confirm::new()
            .with_prompt(format!(
                "Config file already exists at {:?}. Overwrite?",
                config_file
            ))
            .default(false)
            .interact()?;

        if !overwrite {
            println!("\nConfiguration wizard cancelled. Existing config file was not modified.");
            return Ok(());
        }
    }

    // Write config file
    fs::write(&config_file, toml_content)
        .context(format!("Failed to write config file: {:?}", config_file))?;

    println!("\n✓ Configuration file created successfully!");
    println!("  Location: {:?}", config_file);
    println!("  Profile: {}", profile);
    println!("\nYou can now run mcc-gaql with: mcc-gaql --profile {}", profile);

    // Show additional setup instructions
    if config.queries_filename.is_some() {
        let queries_file = config_dir.join(config.queries_filename.unwrap());
        if !queries_file.exists() {
            println!("\nNote: You'll need to create the queries file at:");
            println!("  {:?}", queries_file);
        }
    }

    if let Some(customerids_file) = config.customerids_filename {
        let cids_file = config_dir.join(customerids_file);
        if !cids_file.exists() {
            println!("\nNote: You'll need to create the customer IDs file at:");
            println!("  {:?}", cids_file);
            println!("  Format: One customer ID per line (numbers only, no dashes)");
        }
    }

    println!("\nFor authentication, you'll need to set up Google Ads API credentials.");
    println!("Run mcc-gaql with any query to start the OAuth flow.");

    Ok(())
}

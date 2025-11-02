use clap::Parser;
use mcc_gaql::args::Cli;
use mcc_gaql::config::{MyConfig, ResolvedConfig};

#[test]
fn test_mcc_priority_cli_overrides_config() {
    let args = Cli::parse_from(["mcc-gaql", "--mcc", "1111111111"]);
    let config = Some(MyConfig {
        mcc_customerid: "9999999999".to_string(),
        user: None,
        token_cache_filename: None,
        customerids_filename: None,
        queries_filename: None,
    });

    let resolved = ResolvedConfig::from_args_and_config(&args, config).unwrap();
    assert_eq!(resolved.mcc_customer_id, "1111111111");
}

#[test]
fn test_mcc_fallback_to_customer_id_for_solo_accounts() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--customer-id",
        "2222222222",
        "--user",
        "test@example.com",
    ]);

    let resolved = ResolvedConfig::from_args_and_config(&args, None).unwrap();
    assert_eq!(resolved.mcc_customer_id, "2222222222");
}

#[test]
fn test_error_when_no_mcc_available() {
    let args = Cli::parse_from(["mcc-gaql", "--user", "test@example.com"]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("MCC customer ID required")
    );
}

#[test]
fn test_token_cache_generation_from_user_email() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--user",
        "john.doe@example.com",
        "--mcc",
        "1234567890",
    ]);

    let resolved = ResolvedConfig::from_args_and_config(&args, None).unwrap();
    assert_eq!(
        resolved.token_cache_filename,
        "tokencache_john_doe_at_example_com.json"
    );
}

#[test]
fn test_validate_requires_user_or_token_cache() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc",
        "1234567890",
        "--customer-id",
        "7890123456",
        "SELECT campaign.id FROM campaign",
    ]);

    let resolved = ResolvedConfig {
        mcc_customer_id: "1234567890".to_string(),
        user_email: None, // Missing user
        token_cache_filename: "tokencache_nonexistent.json".to_string(),
        queries_filename: None,
        customerids_filename: None,
    };

    let result = resolved.validate_for_operation(&args);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("User context or existing token cache required")
            || error_msg.contains("authentication")
    );
}

#[test]
fn test_validate_succeeds_with_existing_token_cache() {
    use std::fs;
    use std::io::Write;

    // Get the proper token cache path using config_file_path
    let token_cache_path = mcc_gaql::config::config_file_path("tokencache_test_temp.json")
        .expect("token cache path");

    // Ensure config directory exists
    if let Some(parent) = token_cache_path.parent() {
        fs::create_dir_all(parent).ok();
    }

    // Create the token cache file
    let mut file = fs::File::create(&token_cache_path).expect("create temp token cache");
    file.write_all(b"{}").expect("write temp token cache");
    drop(file);

    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc",
        "1234567890",
        "--customer-id",
        "7890123456",
        "SELECT campaign.id FROM campaign",
    ]);

    let resolved = ResolvedConfig {
        mcc_customer_id: "1234567890".to_string(),
        user_email: None, // No user, but token cache exists
        token_cache_filename: "tokencache_test_temp.json".to_string(),
        queries_filename: None,
        customerids_filename: None,
    };

    let result = resolved.validate_for_operation(&args);

    // Clean up
    fs::remove_file(&token_cache_path).ok();

    // Should succeed because token cache file exists
    if let Err(e) = &result {
        eprintln!("Validation failed with error: {}", e);
        eprintln!("Expected token cache at: {:?}", token_cache_path);
    }
    assert!(result.is_ok());
}

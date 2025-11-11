use clap::Parser;
use mcc_gaql::args::Cli;
use mcc_gaql::config::{MyConfig, ResolvedConfig};

#[test]
fn test_mcc_priority_cli_overrides_config() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc-id",
        "1111111111",
        "--user-email",
        "test@example.com",
    ]);
    let config = Some(MyConfig {
        mcc_id: Some("9999999999".to_string()),
        user_email: None,
        customer_id: None,
        format: None,
        keep_going: None,
        token_cache_filename: None,
        customerids_filename: None,
        queries_filename: None,
        dev_token: None,
        field_metadata_cache: None,
        field_metadata_ttl_days: None,
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
        "--user-email",
        "test@example.com",
    ]);

    let resolved = ResolvedConfig::from_args_and_config(&args, None).unwrap();
    assert_eq!(resolved.mcc_customer_id, "2222222222");
}

#[test]
fn test_config_customer_id_fallback_to_mcc() {
    // Test that config customer_id is used as MCC when mcc_id is not set
    let args = Cli::parse_from(["mcc-gaql", "--user-email", "test@example.com"]);
    let config = Some(MyConfig {
        mcc_id: None, // No MCC specified
        user_email: Some("test@example.com".to_string()),
        customer_id: Some("3333333333".to_string()), // This should be used as MCC
        format: None,
        keep_going: None,
        token_cache_filename: None,
        customerids_filename: None,
        queries_filename: None,
        dev_token: None,
        field_metadata_cache: None,
        field_metadata_ttl_days: None,
    });

    let resolved = ResolvedConfig::from_args_and_config(&args, config).unwrap();
    assert_eq!(resolved.mcc_customer_id, "3333333333");
    assert_eq!(resolved.customer_id, Some("3333333333".to_string()));
}

#[test]
fn test_error_when_no_mcc_available() {
    let args = Cli::parse_from(["mcc-gaql", "--user-email", "test@example.com"]);

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
        "--user-email",
        "john.doe@example.com",
        "--mcc-id",
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
        "--mcc-id",
        "1234567890",
        "--customer-id",
        "7890123456",
        "SELECT campaign.id FROM campaign",
    ]);

    let resolved = ResolvedConfig {
        mcc_customer_id: "1234567890".to_string(),
        user_email: None, // Missing user
        customer_id: Some("7890123456".to_string()),
        format: "table".to_string(),
        keep_going: false,
        token_cache_filename: "tokencache_nonexistent.json".to_string(),
        queries_filename: None,
        customerids_filename: None,
        dev_token: None,
        field_metadata_cache: "~/.cache/mcc-gaql/field_metadata.json".to_string(),
        field_metadata_ttl_days: 7,
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
    let token_cache_path =
        mcc_gaql::config::config_file_path("tokencache_test_temp.json").expect("token cache path");

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
        "--mcc-id",
        "1234567890",
        "--customer-id",
        "7890123456",
        "SELECT campaign.id FROM campaign",
    ]);

    let resolved = ResolvedConfig {
        mcc_customer_id: "1234567890".to_string(),
        user_email: None, // No user, but token cache exists
        customer_id: Some("7890123456".to_string()),
        format: "table".to_string(),
        keep_going: false,
        token_cache_filename: "tokencache_test_temp.json".to_string(),
        queries_filename: None,
        customerids_filename: None,
        dev_token: None,
        field_metadata_cache: "~/.cache/mcc-gaql/field_metadata.json".to_string(),
        field_metadata_ttl_days: 7,
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

#[test]
fn test_invalid_mcc_id_from_cli() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc-id",
        "invalid123", // Invalid: not 10 digits
        "--user-email",
        "test@example.com",
    ]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Invalid --mcc-id") || error_msg.contains("Invalid customer ID"));
}

#[test]
fn test_invalid_mcc_id_too_short() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc-id",
        "123456789", // Invalid: only 9 digits
        "--user-email",
        "test@example.com",
    ]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Invalid --mcc-id"));
}

#[test]
fn test_invalid_mcc_id_too_long() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc-id",
        "12345678901", // Invalid: 11 digits
        "--user-email",
        "test@example.com",
    ]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Invalid --mcc-id"));
}

#[test]
fn test_valid_mcc_id_with_hyphens() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc-id",
        "123-456-7890", // Valid: with hyphens
        "--user-email",
        "test@example.com",
    ]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.mcc_customer_id, "1234567890"); // Should be normalized
}

#[test]
fn test_invalid_customer_id_from_cli() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc-id",
        "1234567890",
        "--customer-id",
        "abc1234567", // Invalid: contains letters
        "--user-email",
        "test@example.com",
    ]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("Invalid customer_id")
            || error_msg.contains("Invalid customer ID format")
    );
}

#[test]
fn test_valid_customer_id_with_hyphens() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc-id",
        "1234567890",
        "--customer-id",
        "987-654-3210", // Valid: with hyphens
        "--user-email",
        "test@example.com",
    ]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.customer_id, Some("9876543210".to_string())); // Should be normalized
}

#[test]
fn test_invalid_customer_id_from_config() {
    let args = Cli::parse_from(["mcc-gaql", "--user-email", "test@example.com"]);
    let config = Some(MyConfig {
        mcc_id: Some("1234567890".to_string()),
        user_email: Some("test@example.com".to_string()),
        customer_id: Some("12345".to_string()), // Invalid: too short
        format: None,
        keep_going: None,
        token_cache_filename: None,
        customerids_filename: None,
        queries_filename: None,
        dev_token: None,
        field_metadata_cache: None,
        field_metadata_ttl_days: None,
    });

    let result = ResolvedConfig::from_args_and_config(&args, config);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("Invalid customer_id")
            || error_msg.contains("Invalid customer ID length")
    );
}

#[test]
fn test_invalid_mcc_id_from_config() {
    let args = Cli::parse_from(["mcc-gaql", "--user-email", "test@example.com"]);
    let config = Some(MyConfig {
        mcc_id: Some("123-456".to_string()), // Invalid: too short even with hyphens
        user_email: Some("test@example.com".to_string()),
        customer_id: None,
        format: None,
        keep_going: None,
        token_cache_filename: None,
        customerids_filename: None,
        queries_filename: None,
        dev_token: None,
        field_metadata_cache: None,
        field_metadata_ttl_days: None,
    });

    let result = ResolvedConfig::from_args_and_config(&args, config);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Invalid mcc_id") || error_msg.contains("Invalid customer ID"));
}

#[test]
fn test_valid_mcc_id_from_config_with_hyphens() {
    let args = Cli::parse_from(["mcc-gaql", "--user-email", "test@example.com"]);
    let config = Some(MyConfig {
        mcc_id: Some("111-222-3333".to_string()), // Valid with hyphens
        user_email: Some("test@example.com".to_string()),
        customer_id: None,
        format: None,
        keep_going: None,
        token_cache_filename: None,
        customerids_filename: None,
        queries_filename: None,
        dev_token: None,
        field_metadata_cache: None,
        field_metadata_ttl_days: None,
    });

    let result = ResolvedConfig::from_args_and_config(&args, config);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.mcc_customer_id, "1112223333"); // Should be normalized
}

#[test]
fn test_error_when_no_user_or_explicit_token_cache() {
    // Test that it errors out when neither user email nor explicit token cache is provided
    let args = Cli::parse_from(["mcc-gaql", "--mcc-id", "1234567890"]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("User email or explicit token cache filename required"));
    assert!(error_msg.contains("default token cache file will NOT be used"));
}

#[test]
fn test_explicit_token_cache_without_user_succeeds() {
    // Test that explicit token cache works without user email
    let args = Cli::parse_from(["mcc-gaql", "--mcc-id", "1234567890"]);
    let config = Some(MyConfig {
        mcc_id: Some("1234567890".to_string()),
        user_email: None, // No user email
        customer_id: None,
        format: None,
        keep_going: None,
        token_cache_filename: Some("tokencache_explicit.json".to_string()), // Explicit token cache
        customerids_filename: None,
        queries_filename: None,
        dev_token: None,
        field_metadata_cache: None,
        field_metadata_ttl_days: None,
    });

    let result = ResolvedConfig::from_args_and_config(&args, config);
    assert!(result.is_ok());
    let resolved = result.unwrap();
    assert_eq!(resolved.token_cache_filename, "tokencache_explicit.json");
    assert_eq!(resolved.user_email, None);
}

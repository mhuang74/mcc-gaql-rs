use clap::Parser;
use mcc_gaql::args::Cli;
use mcc_gaql::config::{MyConfig, ResolvedConfig};

#[test]
fn test_mcc_priority_cli_overrides_config() {
    let args = Cli::parse_from(["mcc-gaql", "--mcc", "111111"]);
    let config = Some(MyConfig {
        mcc_customerid: "999999".to_string(),
        user: None,
        token_cache_filename: None,
        customerids_filename: None,
        queries_filename: None,
    });

    let resolved = ResolvedConfig::from_args_and_config(&args, config).unwrap();
    assert_eq!(resolved.mcc_customer_id, "111111");
}

#[test]
fn test_mcc_fallback_to_customer_id_for_solo_accounts() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--customer-id", "222222",
        "--user", "test@example.com"
    ]);

    let resolved = ResolvedConfig::from_args_and_config(&args, None).unwrap();
    assert_eq!(resolved.mcc_customer_id, "222222");
}

#[test]
fn test_error_when_no_mcc_available() {
    let args = Cli::parse_from(["mcc-gaql", "--user", "test@example.com"]);

    let result = ResolvedConfig::from_args_and_config(&args, None);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("MCC customer ID required"));
}

#[test]
fn test_token_cache_generation_from_user_email() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--user", "john.doe@example.com",
        "--mcc", "123456"
    ]);

    let resolved = ResolvedConfig::from_args_and_config(&args, None).unwrap();
    assert_eq!(
        resolved.token_cache_filename,
        "tokencache_john_doe_at_example_com.json"
    );
}

#[test]
fn test_validate_requires_user_context() {
    let args = Cli::parse_from([
        "mcc-gaql",
        "--mcc", "123456",
        "--customer-id", "789012",
        "SELECT campaign.id FROM campaign"
    ]);

    let resolved = ResolvedConfig {
        mcc_customer_id: "123456".to_string(),
        user_email: None, // Missing user
        token_cache_filename: "tokencache_default.json".to_string(),
        queries_filename: None,
        customerids_filename: None,
    };

    let result = resolved.validate_for_operation(&args);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("User context required"));
}

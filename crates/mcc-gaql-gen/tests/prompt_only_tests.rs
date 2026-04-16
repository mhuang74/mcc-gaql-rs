/// Integration tests for `--generate-prompt-only` and `--resource` flag functionality
/// 
/// These tests verify the new prompt inspection features added for GAQL generation.
/// 
/// NOTE: These tests are currently disabled (`#[ignore]`) because they require:
/// - Mock LLM infrastructure (or test LLM credentials)
/// - Test vector database (LanceDB with field metadata)
/// - Test field cache populated with sample data
/// - Test embeddings infrastructure
/// 
/// To enable these tests, provide the test infrastructure or modify to use mocks.
/// 
/// Progress tracking:
/// - [ ] Set up test infrastructure (mock LLM, test vector DB)
/// - [ ] Enable test_generate_prompt_only_phase1
/// - [ ] Enable test_generate_prompt_only_phase3_with_resource
/// - [ ] Enable test_resource_override_validation
/// - [ ] Enable test_resource_override_normal_mode
/// - [ ] Enable test_normal_mode_without_flags (baseline)

use mcc_gaql_gen::rag::{MultiStepRAGAgent, PipelineConfig};
use anyhow::Result;

/// Helper function to create test RAG agent with specified pipeline config
/// 
/// TODO: Implement this once test infrastructure is available
/// Needs:
/// - Mock or test LLM client
/// - Test field cache with sample Google Ads field metadata
/// - Test vector database populated with field embeddings
/// - Test resource index
async fn setup_test_agent_with_config(_pipeline_config: PipelineConfig) -> Result<MultiStepRAGAgent> {
    unimplemented!("Test infrastructure not yet available - requires mock LLM and test vector DB")
}

/// Helper function to create default test RAG agent
async fn setup_test_agent() -> Result<MultiStepRAGAgent> {
    setup_test_agent_with_config(PipelineConfig::default()).await
}

/// Test: Generate Phase 1 prompt only (no resource override)
/// 
/// Expected behavior:
/// - Returns GenerateResult::PromptOnly with phase = 1
/// - System prompt contains GAQL expert instructions
/// - User prompt contains the input query
/// - No LLM API calls are made
/// - No RAG search is performed
#[tokio::test]
#[ignore]
async fn test_generate_prompt_only_phase1() {
    println!("\n=== Test: Generate Phase 1 Prompt Only ===\n");
    
    // Arrange
    let agent = setup_test_agent_with_config(PipelineConfig {
        generate_prompt_only: true,
        resource_override: None,
        ..Default::default()
    })
    .await
    .expect("Failed to setup test agent");
    let test_query = "show top campaigns by cost";
    
    // Act
    let result = agent
        .generate(test_query)
        .await
        .expect("Failed to generate");
    
    // Assert
    match result {
        mcc_gaql_gen::rag::GenerateResult::PromptOnly { system_prompt, user_prompt, phase } => {
            assert_eq!(phase, 1, "Phase should be 1 for Phase 1 prompt-only mode");
            assert!(
                system_prompt.contains("GAQL expert"),
                "System prompt should identify as GAQL expert"
            );
            assert!(
                system_prompt.contains("primary_resource"),
                "System prompt should mention primary_resource"
            );
            assert!(
                user_prompt.contains(test_query),
                "User prompt should contain the input query"
            );
        }
        _ => panic!("Expected GenerateResult::PromptOnly with phase=1"),
    }
}

/// Test: Generate Phase 3 prompt only (with resource override)
/// 
/// Expected behavior:
/// - Returns GenerateResult::PromptOnly with phase = 3
/// - System prompt contains SELECT/FROM/WHERE instructions
/// - User prompt contains relevant field candidates for the specified resource
/// - No LLM API calls are made
/// - RAG search for field candidates is performed
#[tokio::test]
#[ignore]
async fn test_generate_prompt_only_phase3_with_resource() {
    println!("\n=== Test: Generate Phase 3 Prompt Only with Resource Override ===\n");
    
    // Arrange
    let agent = setup_test_agent_with_config(PipelineConfig {
        resource_override: Some("campaign".to_string()),
        generate_prompt_only: true,
        ..Default::default()
    })
    .await
    .expect("Failed to setup test agent");
    let test_query = "show top campaigns by cost";
    
    // Act
    let result = agent
        .generate(test_query)
        .await
        .expect("Failed to generate");
    
    // Assert
    match result {
        mcc_gaql_gen::rag::GenerateResult::PromptOnly { system_prompt, user_prompt: _, phase } => {
            assert_eq!(phase, 3, "Phase should be 3 for Phase 3 prompt-only mode");
            assert!(
                system_prompt.contains("SELECT"),
                "System prompt should contain SELECT instruction"
            );
            assert!(
                system_prompt.contains("FROM"),
                "System prompt should contain FROM instruction"
            );
            assert!(
                system_prompt.contains("WHERE"),
                "System prompt should contain WHERE instruction"
            );
            // Note: User prompt should contain field candidates related to "campaign"
            // (specific field names depend on test data)
        }
        _ => panic!("Expected GenerateResult::PromptOnly with phase=3"),
    }
}

/// Test: Invalid resource validation
/// 
/// Expected behavior:
/// - Returns error with message "Unknown resource: '<resource>'"
/// - Error is returned early before any expensive operations
#[tokio::test]
#[ignore]
async fn test_resource_override_validation() {
    println!("\n=== Test: Invalid Resource Validation ===\n");
    
    // Arrange
    let agent = setup_test_agent_with_config(PipelineConfig {
        resource_override: Some("invalid_resource".to_string()),
        ..Default::default()
    })
    .await
    .expect("Failed to setup test agent");
    let test_query = "test query";
    
    // Act
    let result = agent
        .generate(test_query)
        .await;
    
    // Assert
    match result {
        Err(e) => {
            let error_msg = e.to_string();
            assert!(
                error_msg.contains("Unknown resource"),
                "Error message should mention 'Unknown resource', got: {}",
                error_msg
            );
            assert!(
                error_msg.contains("invalid_resource"),
                "Error message should contain the invalid resource name"
            );
        }
        Ok(_) => panic!("Expected error but got success result"),
    }
}

/// Test: Resource override in normal mode (not prompt-only)
/// 
/// Expected behavior:
/// - Returns GenerateResult::Query with a valid GAQL query
/// - Query uses the specified primary resource (FROM campaign)
/// - Full pipeline executes (Phase 1-5)
#[tokio::test]
#[ignore]
async fn test_resource_override_normal_mode() {
    println!("\n=== Test: Resource Override in Normal Mode ===\n");
    
    // Arrange
    let agent = setup_test_agent_with_config(PipelineConfig {
        resource_override: Some("campaign".to_string()),
        generate_prompt_only: false,
        ..Default::default()
    })
    .await
    .expect("Failed to setup test agent");
    let test_query = "show campaigns";
    
    // Act
    let result = agent
        .generate(test_query)
        .await
        .expect("Failed to generate");
    
    // Assert
    match result {
        mcc_gaql_gen::rag::GenerateResult::Query(gaql_result) => {
            assert!(
                gaql_result.query.contains("FROM campaign"),
                "GAQL query should use the specified resource"
            );
        }
        _ => panic!("Expected GenerateResult::Query in normal mode"),
    }
}

/// Test: Normal mode without any flags (baseline)
/// 
/// Expected behavior:
/// - Returns GenerateResult::Query with a valid GAQL query
/// - Full pipeline executes automatically (Phase 1-5)
/// - Primary resource is determined by RAG search
/// - No resource override is used
#[tokio::test]
#[ignore]
async fn test_normal_mode_without_flags() {
    println!("\n=== Test: Normal Mode Without Flags (Baseline) ===\n");
    
    // Arrange
    let agent = setup_test_agent().await.expect("Failed to setup test agent");
    let test_query = "show performance metrics for ad groups";
    
    // Act
    let result = agent
        .generate(test_query)
        .await
        .expect("Failed to generate");
    
    // Assert
    match result {
        mcc_gaql_gen::rag::GenerateResult::Query(gaql_result) => {
            assert!(
                gaql_result.query.starts_with("SELECT"),
                "GAQL query should start with SELECT"
            );
            assert!(
                gaql_result.query.contains("FROM"),
                "GAQL query should contain FROM clause"
            );
            assert!(
                !gaql_result.query.is_empty(),
                "GAQL query should not be empty"
            );
        }
        _ => panic!("Expected GenerateResult::Query in normal mode"),
    }
}

/// Test: Verify code duplication fix (performance check)
/// 
/// Expected behavior:
/// - Phase 1 RAG search executes exactly once in normal mode
/// - Phase 2 field retrieval executes exactly once in normal mode
/// - No redundant work is performed
#[tokio::test]
#[ignore]
async fn test_verify_no_code_duplication() {
    println!("\n=== Test: Verify No Code Duplication ===\n");
    
    // This test would need:
    // 1. Mock or spy on select_resource() to count calls
    // 2. Mock or spy on retrieve_field_candidates() to count calls
    // 3. Verify each is called exactly once in normal mode
    
    // TODO: Implement once mocking infrastructure is available
    let agent = setup_test_agent().await.expect("Failed to setup test agent");
    let test_query = "show campaigns by clicks";
    
    // Act
    let result = agent.generate(test_query).await.expect("Failed to generate");
    
    // Assert
    match result {
        mcc_gaql_gen::rag::GenerateResult::Query(_) => {
            // Verify: Phase 1 RAG called exactly once
            // Verify: Phase 2 retrieval called exactly once
            // (needs mock infrastructure to verify)
        }
        _ => panic!("Expected GenerateResult::Query"),
    }
}

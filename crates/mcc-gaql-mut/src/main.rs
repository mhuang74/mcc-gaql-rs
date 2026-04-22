use anyhow::{Context, Result};
use clap::Parser;

use mcc_gaql_common::auth::{list_profiles, load_profile, resolve_auth_config};
use mcc_gaql_common::googleads_api::get_api_access;
use mcc_gaql_common::util::init_logger;

use mcc_gaql_mut::args::{self, Command};
use mcc_gaql_mut::mutation;
use mcc_gaql_mut::mutation_validate;

fn print_startup_banner() {
    let version_info = format!(
        "v{} ({}) built {}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        env!("BUILD_TIME")
    );
    log::info!("═════════════════════════════════════════════════════════════════");
    log::info!(" mcc-gaql-mut {} ", version_info);
    log::info!("═════════════════════════════════════════════════════════════════");
}

/// Profile resolution for mcc-gaql-mut: always auto-select if none specified.
fn resolve_profile(
    auth: &mcc_gaql_common::auth::SharedAuthArgs,
) -> Result<Option<mcc_gaql_common::config::MyConfig>> {
    if let Some(profile_name) = &auth.profile {
        log::info!("Config profile: {profile_name}");
        Some(
            load_profile(profile_name)
                .context(format!("Loading config for profile: {profile_name}")),
        )
        .transpose()
    } else {
        let profiles = list_profiles()?;
        if let Some(profile_name) = profiles.last() {
            eprintln!("Using profile '{}'", profile_name);
            log::info!("Auto-selected profile: {profile_name}");
            Some(
                load_profile(profile_name)
                    .context(format!("Loading config for profile: {profile_name}")),
            )
            .transpose()
        } else {
            Ok(None)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logger("MCC_GAQL", false);
    print_startup_banner();

    let cli = args::Cli::parse();

    // Auth resolution — no shim, direct construction
    let config = resolve_profile(&cli.auth_args())?;
    let auth_config = resolve_auth_config(&cli.auth_args(), config.as_ref())?;

    match &cli.command {
        Command::Mutate {
            resource,
            resource_name,
            operation,
            field_set,
            dry_run,
            preview,
            partial_failure,
            yes,
        } => {
            let customer_id = auth_config.customer_id.as_deref().ok_or_else(|| {
                anyhow::anyhow!("--customer-id is required for mutate operations")
            })?;

            let field_updates = args::parse_field_sets(field_set)?;
            mutation_validate::validate_mutation_locally(
                resource,
                &field_updates,
                (*operation).into(),
            )?;

            if *preview {
                let request = mutation::build_mutation_request(
                    resource,
                    customer_id,
                    resource_name,
                    (*operation).into(),
                    &field_updates,
                    *dry_run,
                    *partial_failure,
                )?;

                println!("MutateGoogleAdsRequest:");
                println!("  customer_id: {}", request.customer_id);
                println!("  validate_only: {}", request.validate_only);
                println!("  partial_failure: {}", request.partial_failure);
                println!(
                    "  operations: {} operation(s)",
                    request.mutate_operations.len()
                );
                println!();
                println!("  Operation 1:");
                println!("    resource_type: {}", resource);
                println!("    operation_type: {:?}", operation);
                println!("    resource_name: {}", resource_name);
                if !field_updates.is_empty() {
                    println!("    field_mask:");
                    for update in &field_updates {
                        println!("      - {}", update.field_path);
                    }
                    println!("    field_values:");
                    for update in &field_updates {
                        println!("      {} = {}", update.field_path, update.value);
                    }
                }
                return Ok(());
            }

            if *dry_run {
                eprintln!("[dry-run] Validating mutation...");
                eprintln!("[dry-run] Resource: {}", resource);
                eprintln!("[dry-run] Operation: {:?}", operation);
                eprintln!("[dry-run] Resource name: {}", resource_name);
                if !field_updates.is_empty() {
                    eprintln!("[dry-run] Fields:");
                    for update in &field_updates {
                        eprintln!("[dry-run]   {} = {}", update.field_path, update.value);
                    }
                }
            }

            if !*dry_run && !*preview && !*yes {
                let confirmed = dialoguer::Confirm::new()
                    .with_prompt(format!(
                        "Apply {:?} mutation on {} ({} field(s))?",
                        operation,
                        resource,
                        field_updates.len()
                    ))
                    .default(false)
                    .interact()?;
                if !confirmed {
                    eprintln!("Mutation cancelled.");
                    return Ok(());
                }
            }

            let api_context = get_api_access(&auth_config.to_api_access_config())
                .await
                .context("Authentication required for mutate operations")?;

            let resource_display = resource.clone();
            let resource_name_display = resource_name.clone();
            let op_display = *operation;

            let response = mutation::mutate_resource(
                api_context,
                mutation::MutationParams {
                    resource_type: resource.clone(),
                    customer_id: customer_id.to_string(),
                    resource_name: resource_name.clone(),
                    operation: (*operation).into(),
                    field_updates,
                    validate_only: *dry_run,
                    partial_failure: *partial_failure,
                },
            )
            .await?;

            if *dry_run {
                eprintln!("[dry-run] Validation PASSED — mutation would succeed if applied");
            } else {
                println!("Mutation succeeded.");
                println!("  Resource: {}", resource_display);
                println!("  Operation: {:?}", op_display);
                println!("  Resource name: {}", resource_name_display);
                let num_results = response.mutate_operation_responses.len();
                if num_results > 0 {
                    println!("  Results: {} operation(s) completed", num_results);
                }
            }
        }
    }

    Ok(())
}

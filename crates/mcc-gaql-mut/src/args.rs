use anyhow::{Context, Result, bail};
use clap::{ArgAction, Parser, Subcommand};
use std::str::FromStr;
use std::sync::LazyLock;

use mcc_gaql_common::auth::SharedAuthArgs;

#[derive(Debug, Clone)]
pub struct FieldUpdate {
    pub field_path: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationOperation {
    Update,
    Create,
    Remove,
}

static VERSION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{} ({}) built {}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        env!("BUILD_TIME")
    )
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationOpCli {
    Update,
    Create,
    Remove,
}

impl FromStr for MutationOpCli {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "update" => Ok(Self::Update),
            "create" => Ok(Self::Create),
            "remove" => Ok(Self::Remove),
            _ => Err(format!(
                "Invalid operation '{}'. Valid: update, create, remove",
                s
            )),
        }
    }
}

impl From<MutationOpCli> for MutationOperation {
    fn from(op: MutationOpCli) -> Self {
        match op {
            MutationOpCli::Update => MutationOperation::Update,
            MutationOpCli::Create => MutationOperation::Create,
            MutationOpCli::Remove => MutationOperation::Remove,
        }
    }
}

#[derive(Parser, Debug)]
#[command(author, about = "Mutate Google Ads resources", version = VERSION.as_str())]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    // Top-level auth flags — declared once, shared across all subcommands
    #[arg(short = 'c', long)]
    pub customer_id: Option<String>,

    #[arg(short = 'm', long = "mcc-id")]
    pub mcc_id: Option<String>,

    #[arg(short = 'p', long)]
    pub profile: Option<String>,

    #[arg(short = 'u', long)]
    pub user_email: Option<String>,

    #[arg(long)]
    pub remote_auth: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(about = "Mutate a Google Ads resource using reflection-based field paths")]
    Mutate {
        #[arg(long, help = "Resource type name (CamelCase, e.g. Campaign, AdGroup)")]
        resource: String,

        #[arg(long, help = "Full resource name (e.g. customers/123/campaigns/456)")]
        resource_name: String,

        #[arg(
            long,
            default_value = "update",
            help = "Operation type: update, create, remove"
        )]
        operation: MutationOpCli,

        #[arg(
            long = "set",
            action = ArgAction::Append,
            help = "field_path=value. Repeat for multiple."
        )]
        field_set: Vec<String>,

        #[arg(long, help = "Validate the mutation without applying it")]
        dry_run: bool,

        #[arg(long, help = "Show the constructed request without sending (offline)")]
        preview: bool,

        #[arg(long, default_value = "true", help = "Continue on partial failures")]
        partial_failure: bool,

        #[arg(short = 'y', long, help = "Skip confirmation prompt (CI/automation)")]
        yes: bool,
    },
    // Future:
    // UpdateBidding { ... }
}

impl Cli {
    pub fn auth_args(&self) -> SharedAuthArgs {
        SharedAuthArgs {
            customer_id: self.customer_id.clone(),
            mcc_id: self.mcc_id.clone(),
            profile: self.profile.clone(),
            user_email: self.user_email.clone(),
            remote_auth: self.remote_auth,
        }
    }
}

pub fn parse_field_set(raw: &str) -> Result<FieldUpdate> {
    let (path, value) = raw.split_once('=').ok_or_else(|| {
        anyhow::anyhow!("Invalid --set format: '{}'. Expected field_path=value", raw)
    })?;
    let path = path.trim();
    let value = value.trim();
    if path.is_empty() {
        bail!("Empty field path in --set '{}'", raw);
    }
    Ok(FieldUpdate {
        field_path: path.to_string(),
        value: value.to_string(),
    })
}

pub fn parse_field_sets(field_sets: &[String]) -> Result<Vec<FieldUpdate>> {
    field_sets
        .iter()
        .map(|s| parse_field_set(s).with_context(|| format!("Parsing --set '{}'", s)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_field_set_with_equals_in_value() {
        let result = parse_field_set("url=https://example.com?a=1").unwrap();
        assert_eq!(result.field_path, "url");
        assert_eq!(result.value, "https://example.com?a=1");
    }

    #[test]
    fn test_mutation_op_cli_invalid() {
        let result = MutationOpCli::from_str("invalid_op");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Invalid operation"));
    }
}

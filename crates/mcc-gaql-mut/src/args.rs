use anyhow::{bail, Context, Result};
use clap::{ArgAction, Parser, Subcommand};
use googleads_rs::proto::google::ads::googleads::v23::services::FieldUpdate;
use googleads_rs::proto::google::ads::googleads::v23::services::MutationOperation;
use std::str::FromStr;
use std::sync::LazyLock;

use mcc_gaql_common::auth::SharedAuthArgs;

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

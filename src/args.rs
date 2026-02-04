use anyhow::Result;
use clap::Parser;
use std::io::{self, Read};
use std::str::FromStr;

/// Output format for query results
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Csv,
    Json,
}

impl FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "table" => Ok(OutputFormat::Table),
            "csv" => Ok(OutputFormat::Csv),
            "json" => Ok(OutputFormat::Json),
            _ => Err(format!(
                "Invalid format '{}'. Valid formats are: table, csv, json",
                s
            )),
        }
    }
}

/// Efficiently run Google Ads GAQL query across one or more child accounts linked to MCC.
///
/// Supports profile-based configuration and ENV VAR override.
///
#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
    /// Google Ads GAQL query to run
    pub gaql_query: Option<String>,

    /// Load named query from file
    #[clap(short = 'q', long)]
    pub stored_query: Option<String>,

    /// Use natural language prompt instead of GAQL; prompt is converted to GAQL via LLM
    #[clap(short = 'n', long)]
    pub natural_language: bool,

    /// GAQL output filename
    #[clap(short, long)]
    pub output: Option<String>,

    /// Output format: table, csv, json (defaults to table, or config profile default if set)
    #[clap(long)]
    pub format: Option<OutputFormat>,

    /// Query using default MCC and Child CustomerIDs file specified for this profile
    #[clap(short, long)]
    pub profile: Option<String>,

    /// User email for OAuth2 authentication (auto-generates token cache)
    #[clap(short = 'u', long)]
    pub user_email: Option<String>,

    /// MCC (Manager) Customer ID for login-customer-id header.
    /// Required unless specified in config profile.
    /// For solo accounts, can be omitted if --customer-id is provided.
    #[clap(short = 'm', long = "mcc-id")]
    pub mcc_id: Option<String>,

    /// Apply query to a single account.
    /// If no --mcc-id is specified, this will be used as the MCC (for solo accounts).
    /// To query across many accounts, specify a customerids_filename in config file, or query across all child accounts via --all-linked-child-accounts.
    #[clap(short, long)]
    pub customer_id: Option<String>,

    /// List all child accounts under MCC
    #[clap(short, long)]
    pub list_child_accounts: bool,

    /// Query GoogleAdsFieldService to retrieve available fields
    #[clap(short, long)]
    pub field_service: bool,

    /// Force query to run across all linked child accounts (some may not be accessible)
    #[clap(short, long)]
    pub all_linked_child_accounts: bool,

    /// Keep going on errors
    #[clap(long)]
    pub keep_going: bool,

    /// Group by columns
    #[clap(long, multiple_occurrences(true))]
    pub groupby: Vec<String>,

    /// Sort by columns
    #[clap(long, multiple_occurrences(true))]
    pub sortby: Vec<String>,

    /// Set up configuration with interactive wizard
    #[clap(long)]
    pub setup: bool,

    /// Display current configuration and exit
    #[clap(long)]
    pub show_config: bool,

    /// Refresh field metadata cache from Google Ads API
    #[clap(long)]
    pub refresh_field_cache: bool,

    /// Clear the vector cache (LanceDB embeddings) and exit
    #[clap(long)]
    pub clear_vector_cache: bool,

    /// Show available fields for a specific resource (e.g., campaign, ad_group)
    #[clap(long)]
    pub show_fields: Option<String>,

    /// Export field metadata summary to stdout
    #[clap(long)]
    pub export_field_metadata: bool,
}

pub fn parse() -> Cli {
    let mut cli = Cli::parse();

    if cli.stored_query.is_none()
        && cli.gaql_query.is_none()
        && !cli.list_child_accounts
        && !cli.setup
        && !cli.show_config
        && !cli.refresh_field_cache
        && !cli.clear_vector_cache
        && cli.show_fields.is_none()
        && !cli.export_field_metadata
    {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .expect("Failed to read from stdin");
        if !buffer.trim().is_empty() {
            cli.gaql_query = Some(buffer);
        }
    }

    cli
}

impl Cli {
    /// Validate argument combinations after parsing
    pub fn validate(&self) -> Result<()> {
        // Ambiguous which child account(s) to query
        if self.customer_id.is_some() && self.all_linked_child_accounts {
            return Err(anyhow::anyhow!(
                "Use --customer-id to query a specific account.\n\
                    Use --mcc-id with --all-linked-child-accounts to query all child accounts under mcc.\n\
                    Please don't use --customer-id and --all-linked-child-accounts together."
            ));
        }

        // Validate that stored query and natural language aren't both specified
        if self.stored_query.is_some() && self.natural_language {
            return Err(anyhow::anyhow!(
                "Cannot use both --stored-query and --natural-language.\n\
                Choose one query method."
            ));
        }

        // Validate that natural language requires a query text
        if self.natural_language && self.gaql_query.is_none() {
            return Err(anyhow::anyhow!(
                "Natural language mode requires a query string.\n\
                Usage: mcc-gaql --natural-language \"show me all campaigns\""
            ));
        }

        // Warn if both profile and config-free mode arguments are mixed
        if self.profile.is_some() && self.mcc_id.is_some() {
            log::warn!(
                "Both --profile and --mcc-id specified. CLI --mcc-id will override profile's MCC setting."
            );
        }

        Ok(())
    }
}

use anyhow::Result;
use clap::Parser;
use std::io::{self, Read};
use std::str::FromStr;
use std::sync::LazyLock;

/// Version string including git hash and build time (computed lazily at first use)
static VERSION: LazyLock<String> = LazyLock::new(|| {
    format!(
        "{} ({}) built {}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        env!("BUILD_TIME")
    )
});

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
#[clap(author, about, version = VERSION.as_str())]
pub struct Cli {
    /// Google Ads GAQL query to run
    pub gaql_query: Option<String>,

    /// Load named query from file
    #[clap(short = 'q', long)]
    pub stored_query: Option<String>,

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
    #[clap(short, long)]
    pub customer_id: Option<String>,

    /// List all child accounts under MCC
    #[clap(short, long)]
    pub list_child_accounts: bool,

    /// Query GoogleAdsFieldService to retrieve available fields
    #[clap(long)]
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

    /// Use remote OAuth flow (paste authorization code from another device)
    #[clap(long)]
    pub remote_auth: bool,

    /// Refresh field metadata cache from Google Ads API
    #[clap(long)]
    pub refresh_field_cache: bool,

    /// Show available fields for a specific resource (e.g., campaign, ad_group)
    #[clap(long)]
    pub show_fields: Option<String>,

    /// Export field metadata summary to stdout
    #[clap(long)]
    pub export_field_metadata: bool,

    /// Show resource hierarchy: all available resources with field counts, key attributes, and compatibility info
    #[clap(long)]
    pub show_resources: bool,

    /// Validate the query against Google Ads API without executing it (requires credentials)
    #[clap(long)]
    pub validate: bool,
}

pub fn parse() -> Cli {
    let mut cli = Cli::parse();

    if cli.stored_query.is_none()
        && cli.gaql_query.is_none()
        && !cli.list_child_accounts
        && !cli.setup
        && !cli.show_config
        && !cli.refresh_field_cache
        && cli.show_fields.is_none()
        && !cli.export_field_metadata
        && !cli.show_resources
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

        // --validate requires a query source
        if self.validate && self.gaql_query.is_none() && self.stored_query.is_none() {
            return Err(anyhow::anyhow!(
                "--validate requires a query. Provide a query as a positional argument, via -q/--stored-query, or pipe one via stdin."
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

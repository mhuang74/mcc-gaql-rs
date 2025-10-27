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

    /// Output format: table, csv, json
    #[clap(long, default_value = "table")]
    pub format: OutputFormat,

    /// Query using default MCC and Child CustomerIDs file specified for this profile
    #[clap(short, long)]
    pub profile: Option<String>,

    /// User email for OAuth2 authentication (auto-generates token cache)
    #[clap(short = 'u', long)]
    pub user: Option<String>,

    /// MCC (Manager) Customer ID for login-customer-id header
    #[clap(short = 'm', long)]
    pub mcc: Option<String>,

    /// Apply query to a single CustomerID. Or use with `--all-linked-child-accounts` to query all child accounts.
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
}

pub fn parse() -> Cli {
    let mut cli = Cli::parse();

    if cli.stored_query.is_none() && cli.gaql_query.is_none() && !cli.list_child_accounts {
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

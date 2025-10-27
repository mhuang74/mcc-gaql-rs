use clap::Parser;
use std::io::{self, Read};

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

    /// Query using default MCC and Child CustomerIDs file specified for this profile
    #[clap(short, long)]
    pub profile: Option<String>,

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

    /// Initialize configuration with interactive wizard
    #[clap(long)]
    pub init: bool,
}

pub fn parse() -> Cli {
    let mut cli = Cli::parse();

    if cli.stored_query.is_none() && cli.gaql_query.is_none() && !cli.list_child_accounts && !cli.init{
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

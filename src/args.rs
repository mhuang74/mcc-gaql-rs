use clap::Parser;

/// Efficiently run GAQL or Field Metadata queries against child accounts linked to MCC.
///
/// Supports profile-based configuration and ENV VAR override.
///
#[derive(Parser, Debug)]
#[clap(author, version, about)]
pub struct Cli {
    /// Profile to load from Config file
    #[clap(short, long)]
    pub profile: Option<String>,

    /// Apply query to CustomerID
    #[clap(short, long)]
    pub customer_id: Option<String>,

    /// Load named query from file
    #[clap(short='q', long)]
    pub stored_query: Option<String>,

    /// Google Ads GAQL query to run
    pub gaql_query: Option<String>,

    /// List all child accounts under MCC
    #[clap(short, long)]
    pub list_child_accounts: bool,

    /// Query GoogleAdsFieldService to retrieve available fields
    #[clap(short, long)]
    pub field_service: bool,

    /// Apply query to all current MCC Child Accounts
    #[clap(short, long)]
    pub all_current_child_accounts: bool,

    /// Keep going on errors
    #[clap(long)]
    pub keep_going: bool,

    /// Group by columns
    #[clap(long, multiple_occurrences(true))]
    pub group_by: Vec<String>,

    /// GAQL output filename
    #[clap(short, long)]
    pub output: Option<String>,

}

pub fn parse() -> Cli {
    Cli::parse()
}

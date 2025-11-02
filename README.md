# mcc-gaql-rs

[![CI](https://github.com/mhuang74/mcc-gaql-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/mhuang74/mcc-gaql-rs/actions/workflows/rust.yml)

Command line tool to execute Google Ads GAQL queries across MCC child accounts. Inspired by [gaql-cli](https://github.com/getyourguide/gaql-cli).

## Installation

### Download Pre-built Binary (macOS Apple Silicon)

Download the latest release from [GitHub Releases](https://github.com/mhuang74/mcc-gaql-rs/releases):

```bash
# Download and extract
curl -L https://github.com/mhuang74/mcc-gaql-rs/releases/latest/download/mcc-gaql-<version>-macos-aarch64.tar.gz | tar xz

# Move to PATH
mv mcc-gaql /usr/local/bin/

# Verify installation
mcc-gaql --version
```

### Build from Source

```bash
git clone https://github.com/mhuang74/mcc-gaql-rs.git
cd mcc-gaql-rs
cargo build --release
./target/release/mcc-gaql --version
```

## Getting Started

### Quick Start: Setup Wizard

The easiest way to get started is using the interactive setup wizard:

```bash
mcc-gaql --setup
```

This will guide you through:
- Creating a new configuration profile
- Setting your MCC ID and customer ID
- Configuring your user email for OAuth2
- Optional: Setting up customer ID lists and query files

Example setup session:

```bash
# Create a new profile called "local-business"
mcc-gaql --setup

# Review your configuration
mcc-gaql --show-config --profile local-business

# Use the profile
mcc-gaql --profile local-business 'SELECT
  customer.id,
  customer.descriptive_name,
	campaign.id,
  campaign.name,
  campaign.advertising_channel_type,
  campaign.status,
  campaign.primary_status
FROM
  campaign'
```

### Configuration File

Configuration is stored in:
* `$HOME/Library/Application Support/mcc-gaql/config.toml` (macOS)
*  `~/.config/mcc-gaql/config.toml` (Linux)
*  `%APPDATA%/mcc-gaql/config.toml` (Windows)

#### Example config.toml

```toml
# Default settings applied to all profiles
[default]
queries_filename = 'query_cookbook.toml'
user_email = 'your.email@gmail.com'

# Profile for MCC account
[mycompany_mcc]
mcc_id = '123-456-7890'
customer_id = '987-654-3210'
customerids_filename = 'customer_ids.txt'
user_email = 'mcc.account@company.com'

# Profile for a specific single account
[brand_account]
customer_id = '111-222-3333'
user_email = 'brand@company.com'

# Another single account with different user
[client_account]
customer_id = '444-555-6666'
user_email = 'client@example.org'
```

#### Manual Configuration

You can also edit the config file directly:

```bash
# Open in your editor
vim "$HOME/Library/Application Support/mcc-gaql/config.toml"

# Or find config location
mcc-gaql --show-config
```

## Example Use Cases

### Query Campaign Performance (Last 30 Days)

Get campaign performance metrics for the last 30 days:

```bash
# Using a profile
mcc-gaql --profile mycompany_mcc \
  "SELECT campaign.id, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros
   FROM campaign
   WHERE segments.date DURING LAST_30_DAYS
   ORDER BY metrics.impressions DESC"

# Config-less: all params via CLI
mcc-gaql \
  --mcc-id "123-456-7890" \
  --customer-id "987-654-3210" \
  --user-email "your.email@gmail.com" \
  "SELECT campaign.id, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros
   FROM campaign
   WHERE segments.date DURING LAST_30_DAYS
   ORDER BY metrics.impressions DESC"

# Export to CSV
mcc-gaql --profile mycompany_mcc \
  --output campaign_performance_30d.csv \
  --format csv \
  "SELECT campaign.id, campaign.name, metrics.impressions, metrics.clicks, metrics.cost_micros
   FROM campaign
   WHERE segments.date DURING LAST_30_DAYS"
```

### Config-less Usage Examples

Run queries without any configuration file by passing all required parameters:

```bash
# Query single account
mcc-gaql \
  --customer-id "123-456-7890" \
  --user-email "your.email@gmail.com" \
  "SELECT campaign.name, campaign.status FROM campaign"

# Query via MCC across specific customer
mcc-gaql \
  --mcc-id "111-222-3333" \
  --customer-id "123-456-7890" \
  --user-email "mcc@company.com" \
  "SELECT campaign.name FROM campaign"

# List all child accounts under MCC
mcc-gaql \
  --mcc-id "111-222-3333" \
  --user-email "mcc@company.com" \
  --list-child-accounts

# Query all linked child accounts
mcc-gaql \
  --mcc-id "111-222-3333" \
  --user-email "mcc@company.com" \
  --all-linked-child-accounts \
  --output all_campaigns.csv \
  "SELECT customer.id, campaign.name, campaign.status FROM campaign"
```

### Additional Use Cases

Query for Asset-based Ad Extensions traffic:
```bash
mcc-gaql --profile mycompany_mcc \
  "SELECT ad_group_ad.ad.id, ad_group_ad.ad.name, metrics.impressions
   FROM ad_group_ad
   WHERE ad_group_ad.ad.type = 'RESPONSIVE_DISPLAY_AD'
   AND segments.date DURING LAST_30_DAYS"
```

Look at adoption trend of Performance Max Campaigns:
```bash
mcc-gaql --profile mycompany_mcc \
  --all-linked-child-accounts \
  --output pmax_adoption.csv \
  "SELECT customer.id, campaign.id, campaign.name, campaign.advertising_channel_type
   FROM campaign
   WHERE campaign.advertising_channel_type = 'PERFORMANCE_MAX'"
```

### Advanced Examples

Using natural language (requires LLM integration):
```bash
mcc-gaql -n "campaign changes from last 14 days with current campaign status and bidding strategy" -o recent_changes.csv
```

Query with stored queries:
```bash
mcc-gaql -q recent_campaign_changes -o all_recent_changes.csv
```

Enable debug logging:
```bash
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" mcc-gaql --profile mycompany_mcc -q my_query
```

## Profile Management

### Create a New Profile

```bash
# Interactive setup
mcc-gaql --setup

# Or edit config.toml manually
vim "$HOME/Library/Application Support/mcc-gaql/config.toml"
```

### Review Profile Configuration

```bash
# Show all profiles
mcc-gaql --show-config

# Show specific profile
mcc-gaql --show-config --profile mycompany_mcc
```

### Use a Profile

```bash
# Use profile for query
mcc-gaql --profile mycompany_mcc "SELECT campaign.name FROM campaign"

# Combine profile with additional options
mcc-gaql --profile mycompany_mcc --output results.csv --format csv "SELECT ..."
```

## Command Reference

```bash
# List child accounts
mcc-gaql --profile myprofile --list-child-accounts

# Query for all available metric fields
mcc-gaql --profile myprofile --field-service "select name, category, selectable, filterable, selectable_with where category IN ('METRIC') order by name" > metric_fields.txt

# Keep processing on errors
mcc-gaql --profile myprofile --keep-going --all-linked-child-accounts "SELECT ..."

# Format output
mcc-gaql --profile myprofile --format json "SELECT ..."  # json, csv, or table

# Sort and group results
mcc-gaql --profile myprofile --sortby "metrics.impressions" --groupby "campaign.name" "SELECT ..."
```

## Alternatives

* [gaql-cli](https://github.com/getyourguide/gaql-cli)
* [Google Ads API Report Fetcher (gaarf)](https://github.com/google/ads-api-report-fetcher)

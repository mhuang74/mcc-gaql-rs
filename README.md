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

> **For developers**: See [DEVELOPER.md](DEVELOPER.md) for building from source and embedding credentials.

## Quick Start

### Setup Wizard

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
  campaign.status
FROM campaign'
```

## Basic Use Cases

### Query Campaign Performance (Last 30 Days)

Get campaign performance metrics for the last 30 days:

```bash
# Using a profile
mcc-gaql --profile mycompany_mcc \
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

### List All Child Accounts Under MCC

```bash
# Using profile
mcc-gaql --profile mycompany_mcc --list-child-accounts

# Config-less: all params via CLI
mcc-gaql \
  --mcc-id "111-222-3333" \
  --user-email "mcc@company.com" \
  --list-child-accounts
```

### Query All Linked Child Accounts

```bash
mcc-gaql --profile mycompany_mcc \
  --all-linked-child-accounts \
  --output all_campaigns.csv \
  "SELECT customer.id, campaign.name, campaign.status FROM campaign"

# Config-less version
mcc-gaql \
  --mcc-id "111-222-3333" \
  --user-email "mcc@company.com" \
  --all-linked-child-accounts \
  --output all_campaigns.csv \
  "SELECT customer.id, campaign.name, campaign.status FROM campaign"
```

### Query Single Account Without Config File

Run queries without any configuration file by passing all required parameters:

```bash
# Query single account
mcc-gaql \
  --customer-id "123-456-7890" \
  --user-email "your.email@gmail.com" \
  "SELECT customer.id, campaign.name, campaign.status FROM campaign"

# Query via MCC across specific customer
mcc-gaql \
  --mcc-id "111-222-3333" \
  --customer-id "123-456-7890" \
  --user-email "mcc@company.com" \
  "SELECT customer.id, campaign.name, campaign.status FROM campaign"
```

## Advanced Use Cases

### Query Asset performance of Responsive Search Ads

```bash
mcc-gaql --profile mycompany_mcc \
  "
  SELECT 
    customer.id, 
    customer.descriptive_name, 
    campaign.id, 
    campaign.name, 
    campaign.advertising_channel_type, 
    ad_group.id, 
    ad_group.name, 
    ad_group.type,
    ad_group_ad.ad.id,
    ad_group_ad.ad.responsive_search_ad.headlines, 
    ad_group_ad.ad.responsive_search_ad.descriptions, 
    ad_group_ad.ad.responsive_search_ad.path1, 
    ad_group_ad.ad.responsive_search_ad.path2, 
    metrics.impressions, 
    metrics.clicks, 
    metrics.ctr, 
    metrics.cost_micros, 
    metrics.average_cpc 
  FROM ad_group_ad 
  WHERE 
    ad_group_ad.ad.type IN ('RESPONSIVE_SEARCH_AD') 
    AND segments.date DURING LAST_30_DAYS 
  ORDER BY 
    campaign.name, 
    ad_group.name, 
    metrics.ctr DESC 
  "
```

### Analyze Performance of PMax Campaigns

```bash
mcc-gaql --profile mycompany_mcc \
  --all-linked-child-accounts \
  --output pmax_performance.csv \
  "
  SELECT 
    customer.id, 
    customer.descriptive_name, 
    campaign.id, 
    campaign.advertising_channel_type, 
    campaign.name, 
    metrics.impressions, 
    metrics.clicks, 
    metrics.cost_micros,
    metrics.average_cpc,
    metrics.conversions,
    metrics.cost_per_conversion,
    customer.currency_code 
  FROM campaign 
  WHERE 
    segments.date DURING LAST_30_DAYS 
    AND campaign.advertising_channel_type IN ('PERFORMANCE_MAX') 
    AND metrics.clicks > 100
  ORDER BY 
    metrics.clicks DESC 
  "
```

### Compare CPA between Campaign Types across Accounts
```bash
mcc-gaql --profile mycompany_mcc \
  --all-linked-child-accounts \
  --sortby "metrics.cost_per_conversion" \
  --groupby "campaign.advertising_channel_type" \
  --format csv \
  --output compare_cpa_between_campaign_types.csv \
  "
  SELECT 
    customer.id, 
    customer.descriptive_name, 
    campaign.id, 
    campaign.advertising_channel_type, 
    campaign.name, 
    metrics.impressions, 
    metrics.clicks, 
    metrics.cost_micros,
    metrics.average_cpc,
    metrics.conversions,
    metrics.cost_per_conversion,
    customer.currency_code 
  FROM campaign 
  WHERE 
    segments.date DURING LAST_30_DAYS 
    AND metrics.clicks > 100
  ORDER BY 
    campaign.advertising_channel_type DESC 
  "
```

### Use Stored Queries

Queries should be stored in the config directory in a TOML file and referenced in config file via `queries_filename`, or set via environment variable `MCC_GAQL_QUERIES_FILENAME`. See [Configuration](#configuration) for examples.

See [Stored Queries File](#stored-queries-file) section for an example TOML file with properly formatted query entries.

```bash
mcc-gaql -p mycompany_mcc -q keywords_with_top_traffic_last_week --format csv -o top_keywords.csv
```

### Field Service Queries

Query for all available metric fields:

```bash
mcc-gaql --profile myprofile \
  --field-service \
  "select name, category, selectable, filterable, selectable_with
                   where category IN ('METRIC')
                   order by name
  " > metric_fields.txt
```

### Error Handling and Formatting

```bash
# Keep processing on errors
mcc-gaql --profile myprofile \
  --keep-going \
  --all-linked-child-accounts \
  "SELECT campaign.name, campaign.status FROM campaign"

# Format output as JSON or table
mcc-gaql --profile myprofile --format json "SELECT ..."  # json, csv, or table

### Natural Language Queries (Experimental)

Using natural language (requires LLM integration):

```bash
mcc-gaql -n "campaign changes from last 14 days with current campaign status and bidding strategy" -o recent_changes.csv
```

## CLI Reference

### Common Options

| Option | Description |
|--------|-------------|
| `--profile <name>` | Use a specific profile from config.toml |
| `--mcc-id <id>` | MCC account ID |
| `--customer-id <id>` | Customer account ID |
| `--user-email <email>` | User email for OAuth2 |
| `--all-linked-child-accounts` | Query all child accounts under MCC |
| `--list-child-accounts` | List all child accounts |
| `--output <file>` | Output file path |
| `--format <format>` | Output format: json, csv, or table |
| `--sortby <field>` | Sort results by field |
| `--groupby <field>` | Group results by field |
| `--keep-going` | Continue processing on errors |
| `-q <query_name>` | Use stored query from queries file |
| `-n <natural_language>` | Natural language query (requires LLM) |
| `--field-service <query>` | Query Google Ads field service |
| `--setup` | Run interactive setup wizard |
| `--show-config` | Show configuration |
| `--version` | Show version |

## Configuration

### Configuration File Location

Configuration is stored in:
* `$HOME/Library/Application Support/mcc-gaql/config.toml` (macOS)
* `~/.config/mcc-gaql/config.toml` (Linux)
* `%APPDATA%/mcc-gaql/config.toml` (Windows)

### Example config.toml

```toml
# Default settings applied to all profiles
[default]
queries_filename = 'query_cookbook.toml'
user_email = 'your.email@gmail.com'
dev_token = 'YOUR_GOOGLE_ADS_DEV_TOKEN'  # Optional - can also use env var

# Profile for MCC account
[mycompany_mcc]
mcc_id = '123-456-7890'
customer_id = '987-654-3210'
customerids_filename = 'customer_ids.txt'
user_email = 'mcc.account@company.com'
dev_token = 'YOUR_DEV_TOKEN'  # Optional - overrides default

# Profile for a specific single account
[brand_account]
customer_id = '111-222-3333'
user_email = 'brand@company.com'

# Another single account with different user
[client_account]
customer_id = '444-555-6666'
user_email = 'client@example.org'
```

### Profile Inheritance

The `[default]` section provides settings that are inherited by all profiles. Individual profiles can override these settings. This is useful for:

- Setting a common `user_email` for all profiles
- Sharing a `dev_token` across profiles
- Setting default `queries_filename` for stored queries

### Profile Management

```bash
# Create a new profile (interactive)
mcc-gaql --setup

# Show all profiles
mcc-gaql --show-config

# Show specific profile
mcc-gaql --show-config --profile mycompany_mcc

# Edit config manually
vim "$HOME/Library/Application Support/mcc-gaql/config.toml"
```

### Manual Configuration

You can also edit the config file directly:

```bash
# Open in your editor
vim "$HOME/Library/Application Support/mcc-gaql/config.toml"

# Or find config location
mcc-gaql --show-config
```

## Stored Queries File

Example TOML file with formatting guide.
```toml
###
#
# GAQL Query Cookbook
#
# Michael S. Huang (mhuang74@gmail.com)
#
#
# Naming Convention = <grain>_with_<description>, e.g. accounts_with_traffic_last_week
#
# FORMAT
#
# [query_name_using_snake_case]
# description = """
# Provide a description of query
# """
# query = """
# actual GAQL query
# """
#

[accounts_with_traffic_last_week]
description = """
Accounts with Traffic Last Week
"""
query = """
SELECT 
	customer.id, 
	customer.descriptive_name, 
	metrics.impressions, 
	metrics.clicks, 
	metrics.cost_micros,
	customer.currency_code 
FROM customer 
WHERE 
	segments.date during LAST_7_DAYS
	AND metrics.impressions > 1
"""

[keywords_with_top_traffic_last_week]
description = """
Top Keywords
"""
query = """
SELECT
	customer.id,
	customer.descriptive_name,
	campaign.id,
	campaign.name,
	campaign.advertising_channel_type,
	ad_group.id,
	ad_group.name,
	ad_group.type,
	ad_group_criterion.criterion_id,
	ad_group_criterion.keyword.text,
	metrics.impressions,
	metrics.clicks,
	metrics.cost_micros,
  metrics.conversions,
  metrics.cost_per_conversion,
  metrics.conversions_value,
	customer.currency_code 
FROM keyword_view
WHERE
	segments.date DURING LAST_7_DAYS
	and metrics.clicks > 100
ORDER BY
	metrics.clicks DESC
"""
```


## Debugging

Enable debug logging to troubleshoot issues:

```bash
# Set log level via environment variable
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" mcc-gaql --profile mycompany_mcc -q my_query

# Available log levels: error, warn, info, debug, trace
MCC_GAQL_LOG_LEVEL="debug" mcc-gaql --profile myprofile "SELECT ..."
```

## Alternatives

* [gaql-cli](https://github.com/getyourguide/gaql-cli)
* [Google Ads API Report Fetcher (gaarf)](https://github.com/google/ads-api-report-fetcher)

## Contributing

See [DEVELOPER.md](DEVELOPER.md) for development setup, project structure, and contribution guidelines.

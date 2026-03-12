# mcc-gaql-rs

[![CI](https://github.com/mhuang74/mcc-gaql-rs/actions/workflows/rust.yml/badge.svg)](https://github.com/mhuang74/mcc-gaql-rs/actions/workflows/rust.yml)
[![License](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)
[![Rust](https://img.shields.io/badge/rust-1.90+-orange.svg)](https://www.rust-lang.org)
[![GitHub release](https://img.shields.io/github/v/release/mhuang74/mcc-gaql-rs)](https://github.com/mhuang74/mcc-gaql-rs/releases)

Two Rust CLI tools for working with Google Ads GAQL queries across MCC child accounts. Inspired by [gaql-cli](https://github.com/getyourguide/gaql-cli).

## About

This project provides two separate tools:

| Tool | Size | Purpose |
|------|------|---------|
| `mcc-gaql` | ~15-20 MB | Lightweight query tool for executing GAQL queries |
| `mcc-gaql-gen` | ~400 MB | GAQL generation tool with LLM/RAG for natural language queries |

**Why two tools?** The core query tool is fast, lightweight, and has minimal dependencies. The generation tool includes LLM/RAG functionality for natural language queries, which requires many heavy dependencies. Keeping them separate allows most users to install only what they need.

> **For Developers**: See [DEVELOPER.md](DEVELOPER.md) for architecture details, development setup, and contribution guidelines.

## Installation

### Download Pre-built Binaries (macOS Apple Silicon)

Download the latest release from [GitHub Releases](https://github.com/mhuang74/mcc-gaql-rs/releases):

```bash
# Download and extract
curl -L https://github.com/mhuang74/mcc-gaql-rs/releases/latest/download/mcc-gaql-<version>-macos-aarch64.tar.gz | tar xz

# Move to PATH
mv mcc-gaql mcc-gaql-gen /usr/local/bin/

# Verify installation
mcc-gaql --version
mcc-gaql-gen --version
```

### Install Only mcc-gaql (Query Tool)

If you only need the lightweight query tool:

```bash
# Download and extract
curl -L https://github.com/mhuang74/mcc-gaql-rs/releases/latest/download/mcc-gaql-<version>-macos-aarch64.tar.gz | tar xz

# Move only mcc-gaql to PATH
mv mcc-gaql /usr/local/bin/
```

### Install Only mcc-gaql-gen (Generation Tool)

If you only need the natural language query generation:

```bash
# Download and extract
curl -L https://github.com/mhuang74/mcc-gaql-rs/releases/latest/download/mcc-gaql-<version>-macos-aarch64.tar.gz | tar xz

# Move only mcc-gaql-gen to PATH
mv mcc-gaql-gen /usr/local/bin/
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
```

## Natural Language Queries (mcc-gaql-gen)

### Overview

Use `mcc-gaql-gen` to convert natural language descriptions into GAQL queries. The `mcc-gaql` cli tool executes queries, while `mcc-gaql-gen` generates them. See [LLM Configuration](#llm-configuration-for-natural-language-queries) for setup instructions.

```bash
# Generate a GAQL query from natural language
mcc-gaql-gen generate "campaign changes from last 14 days with current campaign status and bidding strategy" > recent_changes.gaql

# Execute the generated query
mcc-gaql --profile myprofile "$(cat recent_changes.gaql)" -o recent_changes.csv

# Or pipe directly
mcc-gaql-gen generate "campaign changes from last 14 days" | xargs mcc-gaql --profile myprofile -o recent_changes.csv
```

### How It Works

The natural language feature uses a **Retrieval-Augmented Generation (RAG)** approach:

1. **Field Metadata Retrieval**: The tool retrieves relevant Google Ads field definitions from your local field cache (see [Field Metadata Management](#field-metadata-management)). This ensures the LLM knows about valid fields, their types, and which resources they belong to.

2. **Example Retrieval**: If you have a [query cookbook](#stored-queries-file) configured, semantically similar example queries are retrieved to provide additional context for the LLM.

3. **LLM Generation**: The configured LLM combines the field metadata and example queries to generate a GAQL query matching your natural language request.

### Setup Requirements

Natural language queries require an LLM provider to be configured. See [LLM Configuration](#llm-configuration-for-natural-language-queries) for detailed setup instructions.

> **Warning**: This feature is **experimental**. The LLM may generate invalid GAQL queries that will result in errors when executed against the Google Ads API. Always review generated queries when possible.

### Basic Usage

```bash
# Generate a query
mcc-gaql-gen generate "show me all campaigns with status and bidding strategy"

# Generate with local metadata
mcc-gaql-gen generate "campaign performance last 30 days including impressions, clicks, and cost" --local
```

### mcc-gaql-gen Commands

#### parse-protos

Parse Google's official proto files to extract authoritative field documentation:

```bash
mcc-gaql-gen parse-protos [OPTIONS]

OPTIONS:
    --output <PATH>    Path to proto docs cache output (default: ~/.cache/mcc-gaql/proto_docs_v23.json)
    --force             Force rebuild of cache even if it exists
```

**Output:** Creates `proto_docs_v23.json` with ~182 resource messages, ~360 enums, and ~3000 fields with their documentation.

#### enrich

Enrich field metadata with documentation:

```bash
mcc-gaql-gen enrich [OPTIONS]

OPTIONS:
    --use-proto         Use proto documentation as primary source (no LLM required)
    --output <PATH>     Path to output enriched cache
```

Use `--use-proto` for fast enrichment using proto documentation only, without LLM API calls.

#### generate

Generate GAQL from natural language:

```bash
mcc-gaql-gen generate <PROMPT> [OPTIONS]

ARGUMENTS:
    <PROMPT>      Natural language query prompt

OPTIONS:
    --queries <PATH>    Path to query cookbook TOML file
    --metadata <PATH>   Path to enriched field metadata JSON
    --basic            Use basic RAG mode (query cookbook only)
```

```
mcc-gaql-gen 0.15.0
Generate GAQL from natural language using LLM/RAG

USAGE:
    mcc-gaql-gen [SUBCOMMAND]

SUBCOMMANDS:
    parse-protos    Parse proto files from googleads-rs to extract field documentation
    enrich          Enrich field metadata with LLM descriptions or proto docs
    generate        Generate GAQL from natural language prompt
    upload          Upload metadata to Cloudflare R2
    download        Download metadata from Cloudflare R2
    clear-cache     Clear local caches
    help            Print this message or the help of the given subcommand

DEPRECATED SUBCOMMANDS:
    scrape           Scraping web docs is deprecated; use parse-protos instead
```

#### parse-protos Command

Parse Google's official proto files to extract authoritative field documentation:

```bash
# Parse all proto files (recommended first step)
mcc-gaql-gen parse-protos

# Force rebuild of cache
mcc-gaql-gen parse-protos --force

# Output:
# Proto parsing complete:
#   - 182 messages with ~3000 fields
#   - 360 enums with ~1200 values
# Cache saved to: ~/.cache/mcc-gaql/proto_docs_v23.json
```

### Tips for Better Results

- **Be specific about resources**: Mention the resource type explicitly (campaign, ad_group, keyword_view, etc.)
- **List desired fields**: Name specific metrics or attributes you want included
- **Specify date ranges**: Include date ranges explicitly (e.g., "last 30 days", "this month", "during last week")
- **Include filtering criteria**: Add conditions like "where clicks > 100" or "only active campaigns"
- **Use high-quality field metadata**: Run the full metadata pipeline before generating queries

### Recommended Metadata Pipeline

For best natural language query results, prepare your metadata using this workflow:

```bash
# 1. Parse proto files (extract authoritative documentation)
mcc-gaql-gen parse-protos

# 2. Enrich field metadata cache with proto docs
mcc-gaql-gen enrich --use-proto

# 3. Generate queries with rich context
mcc-gaql-gen generate "campaign performance last 30 days including impressions, clicks, and cost"
```

This ensures the LLM has complete field documentation including:
- Field descriptions from official proto files
- Enum value meanings (e.g., `ENABLED: Campaign is serving ads`)
- Field behavior annotations (IMMUTABLE, OUTPUT_ONLY, etc.)
- Resource-level descriptions

## CLI Reference

### mcc-gaql (Query Tool)

```
mcc-gaql 0.15.0
Efficiently run Google Ads GAQL query across one or more child accounts linked to MCC.

USAGE:
    mcc-gaql [OPTIONS] [GAQL_QUERY]

ARGS:
    <GAQL_QUERY>    Google Ads GAQL query to run

OPTIONS:
    -a, --all-linked-child-accounts    Force query to run across all linked child accounts
    -c, --customer-id <CUSTOMER_ID>    Apply query to a single account
        --export-field-metadata        Export field metadata summary to stdout
    -f, --field-service                Query GoogleAdsFieldService to retrieve available fields
        --format <FORMAT>              Output format: table, csv, json
        --groupby <GROUPBY>            Group by columns
    -h, --help                         Print help information
        --keep-going                   Keep going on errors
    -l, --list-child-accounts          List all child accounts under MCC
    -m, --mcc-id <MCC_ID>              MCC (Manager) Customer ID for login-customer-id header
    -o, --output <OUTPUT>              GAQL output filename
    -p, --profile <PROFILE>            Query using profile from config
    -q, --stored-query <STORED_QUERY>  Load named query from file
        --setup                        Set up configuration with interactive wizard
        --show-config                  Display current configuration and exit
        --show-fields <SHOW_FIELDS>    Show available fields for a specific resource
        --sortby <SORTBY>              Sort by columns
    -u, --user-email <USER_EMAIL>      User email for OAuth2 authentication
    -V, --version                      Print version information
```

### mcc-gaql-gen (Generation Tool)

#### enrich Command

```bash
mcc-gaql-gen enrich [OPTIONS]

OPTIONS:
    --use-proto                Use proto documentation as primary source (no LLM calls)
```

Use `--use-proto` to populate metadata with official proto documentation without requiring LLM API calls. This is the fastest and most reliable enrichment method.

### Common Options (mcc-gaql)

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
| `-f, --field-service` | Query Google Ads field service |
| `--show-fields <resource>` | Show available fields for a resource |
| `--export-field-metadata` | Export field metadata summary to stdout |
| `--setup` | Run interactive setup wizard |
| `--show-config` | Show configuration |
| `--version` | Show version |

### mcc-gaql-gen (Generation Tool)

See [Natural Language Queries (mcc-gaql-gen)](#natural-language-queries-mcc-gaql-gen) for usage.

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

### Environment Variable Override

All values in the config file may also be overriden via environment variables with prefix `MCC_GAQL_`.

For example:
```bash
export MCC_GAQL_QUERIES_FILENAME="my_queries.toml"
export MCC_GAQL_FORMAT="csv"
export MCC_GAQL_KEEP_GOING="true"
```

### LLM Configuration for Natural Language Queries

[Natural language queries with mcc-gaql-gen](#natural-language-queries-mcc-gaql-gen) require an LLM provider to convert natural language into GAQL. Configure using the following environment variables:

| Variable | Description | Required |
|----------|-------------|----------|
| `MCC_GAQL_LLM_API_KEY` | API key for LLM provider | Yes |
| `MCC_GAQL_LLM_BASE_URL` | Base URL for LLM provider | Yes |
| `MCC_GAQL_LLM_MODEL` | Model name (e.g., `google/gemini-flash-2.0`, `gpt-4o-mini`, `hf:MiniMaxAI/MiniMax-M2.1`) | Yes |
| `MCC_GAQL_LLM_TEMPERATURE` | Temperature for LLM generation (default: 0.1) | No |

#### Multiple Models for Concurrent Processing

You can specify multiple comma-separated models in `MCC_GAQL_LLM_MODEL` to enable concurrent processing during **field metadata enrichment**. When multiple models are configured, the tool uses a model pool to parallelize LLM calls, significantly speeding up the enrichment process.

**How it works:**
- Each model gets one concurrent "slot" (rate-limiting per model)
- Batches of fields are distributed across all available models
- If a model is busy, work is automatically routed to the next available model
- The first model in the list is considered the "preferred" model for single-model operations

**Example with multiple models:**
```bash
# Use multiple models concurrently for faster metadata enrichment
export MCC_GAQL_LLM_API_KEY="your_openrouter_api_key"
export MCC_GAQL_LLM_BASE_URL="https://openrouter.ai/api/v1"
export MCC_GAQL_LLM_MODEL="google/gemini-flash-2.0,openai/gpt-4o-mini,anthropic/claude-3.5-haiku"

# Now when you refresh the field cache, enrichment happens in parallel across all 3 models
mcc-gaql --profile myprofile --refresh-field-cache
```

> **Note:** Multiple models are only used for field metadata enrichment operations. Natural language query generation always uses the first (preferred) model.

#### Provider Examples

**OpenRouter** (default):
```bash
export MCC_GAQL_LLM_API_KEY="your_openrouter_api_key"
export MCC_GAQL_LLM_BASE_URL="https://openrouter.ai/api/v1"
export MCC_GAQL_LLM_MODEL="google/gemini-flash-2.0"
```

**OpenAI**:
```bash
export MCC_GAQL_LLM_API_KEY="your_openai_api_key"
export MCC_GAQL_LLM_BASE_URL="https://api.openai.com/v1"
export MCC_GAQL_LLM_MODEL="gpt-4o-mini"
```

**Ollama** (local):
```bash
export MCC_GAQL_LLM_BASE_URL="http://localhost:11434/v1"
export MCC_GAQL_LLM_MODEL="llama3.2"
```

#### Example Commands

Using a specific model with natural language:
```bash
MCC_GAQL_LLM_MODEL="hf:MiniMaxAI/MiniMax-M2.1" mcc-gaql --profile themade -n "performance from last week, including impression, clicks, prominence metrics, revenue, conversion, and all video metrics, except for trueview metrics" --format csv
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


## Environment Variables

All configuration values can be overridden via environment variables with prefix `MCC_GAQL_`:

| Variable | Purpose |
|----------|---------|
| `MCC_GAQL_DEV_TOKEN` | Google Ads developer token |
| `MCC_GAQL_LOG_LEVEL` | Logging level (error, warn, info, debug, trace) |
| `MCC_GAQL_LLM_API_KEY` | LLM provider API key (mcc-gaql-gen) |
| `MCC_GAQL_LLM_BASE_URL` | LLM provider base URL (mcc-gaql-gen) |
| `MCC_GAQL_LLM_MODEL` | Model name (mcc-gaql-gen) |
| `MCC_GAQL_LLM_TEMPERATURE` | LLM temperature (default: 0.1) |
| `OPENROUTER_API_KEY` | Alternative to MCC_GAQL_LLM_API_KEY |
| `R2_ACCESS_KEY` | Cloudflare R2 access key (mcc-gaql-gen upload) |
| `R2_SECRET_KEY` | Cloudflare R2 secret key (mcc-gaql-gen upload) |

### Example Configuration Override

```bash
# Override storage path for query cookbook
export MCC_GAQL_QUERIES_FILENAME="my_queries.toml"

# Set default output format
export MCC_GAQL_FORMAT="csv"

# Continue processing on errors
export MCC_GAQL_KEEP_GOING="true"
```

## File Locations

Configuration and data files are stored in:

| File/Directory | Location |
|----------------|----------|
| Config file | `~/.config/mcc-gaql/config.toml` (Linux/macOS)<br>`%APPDATA%\mcc-gaql\config.toml` (Windows) |
| Proto docs cache | `~/.cache/mcc-gaql/proto_docs_v23.json` |
| Field metadata cache | `~/.config/mcc-gaql/field_metadata.json` |
| LanceDB vector store | `~/.cache/mcc-gaql/lancedb/` |
| Scraped docs cache | `~/.config/mcc-gaql/scraped_docs.json` |
| Token cache | Same directory as config, named by user email hash |


## Debugging

Enable debug logging to troubleshoot issues:

```bash
# Set log level via environment variable
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" mcc-gaql --profile mycompany_mcc -q my_query

# Available log levels: error, warn, info, debug, trace
MCC_GAQL_LOG_LEVEL="debug" mcc-gaql --profile myprofile "SELECT ..."
```

## Google Ads Field Metadata Caching

### Overview

The `mcc-gaql-gen` tool maintains a local cache of Google Ads field metadata to support [natural language queries](#natural-language-queries-mcc-gaql-gen) and field exploration. This cache provides the LLM with knowledge of valid fields, their types, and resource relationships.

### Two Approaches to Metadata Collection

The tool supports two methods for extracting field documentation:

| Approach | Status | Description |
|----------|--------|-------------|
| **Proto Parsing** (Recommended) | ✅ Stable | Parses Google's official proto files from `googleads-rs` crate - authoritative source |
| **Web Scraping** | ⚠️ Deprecated | Scrapes HTML documentation from Google's developer website - unreliable |

#### Proto Parsing (Recommended)

The **proto parsing** approach extracts field documentation directly from Google's official protocol buffer (`.proto`) files included in the `googleads-rs` dependency. This provides authoritative,高质量 documentation straight from the source.

**Advantages:**
- Authoritative source (Google's official proto definitions)
- Complete field documentation with types and behaviors
- Extracts enum value descriptions
- Fast parsing (<30 seconds for all fields)
- No network requests required
- Works offline

**Metadata Source:** ~955 proto files bundled with `googleads-rs` at:
```
$CARGO_HOME/git/checkouts/googleads-rs-*/proto/google/ads/googleads/v23/
```

**Use this approach for:**
- Production metadata preparation
- Complete field and enum documentation
- Fast, reliable cache building

#### Web Scraping (Deprecated)

The **web scraping** approach attempts to fetch documentation from Google's developer website. This is **deprecated** due to:
- HTML structure changes breaking the scraper
- Rate limiting and CAPTCHAs
- Incomplete field coverage

**Use this approach only if:**
- You need documentation that's not in proto files (rare)
- Proto parsing fails for some reason

### Field Metadata Management

#### Step 1: Parse Proto Files

First, parse all proto files to extract documentation. This is the recommended first step:

```bash
# Parse proto files from googleads-rs (takes ~30 seconds)
mcc-gaql-gen parse-protos

# Force rebuild of cache
mcc-gaql-gen parse-protos --force

# Specify custom output path
mcc-gaql-gen parse-protos --output /custom/path/proto_docs.json
```

This creates a cache at `~/.cache/mcc-gaql/proto_docs_v23.json` containing:
- Field documentation from ~182 resource proto files
- Enum documentation from ~360 enum proto files
- Field behavior annotations (IMMUTABLE, OUTPUT_ONLY, REQUIRED, OPTIONAL)
- Type information for all fields

#### Step 2: Enrich Field Metadata

Merge the proto documentation into your field metadata cache:

```bash
# Using proto documentation only (no LLM required)
mcc-gaql-gen enrich --use-proto

# This populates FieldMetadata.description for all fields with proto docs
```

#### Step 3: Download from Cloudflare R2 (Alternative)

Instead of parsing proto files locally, you can download pre-enriched metadata from the public Cloudflare R2 bucket:

```bash
mcc-gaql-gen download --api-version v19
```

#### Show Fields for a Resource (mcc-gaql)

Display available fields for a specific resource type (e.g., campaign, ad_group, customer):

```bash
# Show all fields available for the campaign resource
mcc-gaql --show-fields campaign

# Show fields for ad_group resource
mcc-gaql --show-fields ad_group

# Show fields for customer resource
mcc-gaql --show-fields customer

# Show fields for keyword_view resource
mcc-gaql --show-fields keyword_view
```

#### Export Field Metadata (mcc-gaql)

Export the complete field metadata summary to stdout (useful for documentation or analysis):

```bash
# Export all field metadata to a file
mcc-gaql --export-field-metadata > field_metadata.txt

# Export and pipe to other tools
mcc-gaql --export-field-metadata | grep "campaign"
```

#### Upload to Cloudflare R2 (mcc-gaql-gen)

Upload enriched metadata to Cloudflare R2 for public access:

```bash
R2_ACCESS_KEY="your_key" R2_SECRET_KEY="your_secret" mcc-gaql-gen upload
```

## When to Use Each Tool

| Use Case | Recommended Tool |
|----------|-----------------|
| Run GAQL queries on Google Ads data | `mcc-gaql` |
| Query multiple MCC child accounts | `mcc-gaql` |
| Export results to CSV/JSON | `mcc-gaql` |
| Parse proto files for metadata | `mcc-gaql-gen parse-protos` |
| Enrich field metadata | `mcc-gaql-gen enrich --use-proto` |
| Generate GAQL from natural language | `mcc-gaql-gen generate` |
| Download/upload enriched metadata | `mcc-gaql-gen download/upload` |

## Alternatives

* [gaql-cli](https://github.com/getyourguide/gaql-cli)
* [Google Ads API Report Fetcher (gaarf)](https://github.com/google/ads-api-report-fetcher)

## Contributing

See [DEVELOPER.md](DEVELOPER.md) for detailed development setup and contribution guidelines.

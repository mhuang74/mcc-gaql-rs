# mcc-gaql-mut Extraction Specification

**Date:** 2026-04-22
**Status:** Planned
**Depends on:** `dynamic_mutation_cli_integration.md` (Phases 1-2 completed)
**Enables:** `pmax_bidding_strategy_updates.md` (clean architecture foundation)

---

## 1. Overview

Extract all Google Ads API access code (OAuth2, gRPC client, auth resolution, query primitives, mutation primitives) from `mcc-gaql` into `mcc-gaql-common`. Create a new `mcc-gaql-mutate` binary crate for all mutation use cases. Upgrade `mcc-gaql` from clap 3.1 to clap 4.0. This eliminates the `cli_from_mutate_args()` shim, lets `mcc-gaql-gen` drop its `mcc-gaql` dependency, and provides a clean separation between read (query) and write (mutate) paths.

### Motivation

The current `mutate` subcommand design inside `mcc-gaql` has three structural problems:

1. **Auth flag duplication** — Each subcommand re-declares `--customer-id`, `--mcc-id`, `--profile`, `--user-email`, `--remote-auth`, requiring a `cli_from_mutate_args()` mapper that constructs a synthetic `Cli` with 20 query-specific fields set to defaults just to call `ResolvedConfig::from_args_and_config()`. When `update-bidding` arrives, this pattern repeats.

2. **Tight coupling** — `ResolvedConfig::from_args_and_config()` takes the full query-specific `Cli` struct, making it impossible for other binaries to reuse auth resolution without depending on the entire query CLI surface.

3. **Mixed concerns** — Read (GAQL query, validation, field metadata) and write (mutation) paths live in one binary with one `Cli` struct, one `ResolvedConfig`, and one `main.rs` dispatch. Every new domain subcommand (`update-bidding`, future ones) adds auth flags to `Command` enum + a new mapper function.

### Goals

- `mcc-gaql-mutate` binary with top-level auth flags (no per-subcommand duplication)
- Shared auth resolution in `mcc-gaql-common` (no `Cli` coupling)
- `mcc-gaql-gen` drops `mcc-gaql` dependency (build time improvement)
- Clap 4.0 across all crates
- `mcc-gaql` query flow completely unchanged in behavior

### Non-goals

- Domain-specific subcommands (`update-bidding`) — out of scope, but the architecture must accommodate them
- `mcc-gaql-gen` clap upgrade — it already uses clap 4
- `googleads-rs` changes — no modifications needed

---

## 2. Dependency Graph

### Before

```
mcc-gaql-common ──────────── ← mcc-gaql-gen
mcc-gaql ──────────────────── ← mcc-gaql-gen (for config + googleads auth)
mcc-gaql ──────────────────── ← (standalone binary)
googleads-rs ──────────────── ← mcc-gaql (direct dep)
```

Problems: `mcc-gaql-gen` depends on `mcc-gaql` (pulls in `polars`, `cacache`, `bincode`, `dialoguer`, `figment`, `itertools`, `thousands` transitively). Auth/API code lives in `mcc-gaql` and is inaccessible to other crates without the full `mcc-gaql` lib dependency.

### After

```
mcc-gaql-common                ← mcc-gaql, mcc-gaql-gen, mcc-gaql-mutate
mcc-gaql-mutate ─────────────── ← mcc-gaql-common, googleads-rs (direct)
mcc-gaql ────────────────────── ← mcc-gaql-common (no googleads-rs direct dep)
```

`mcc-gaql-gen` drops `mcc-gaql` dependency entirely. Auth resolution, API access, and query primitives live in `mcc-gaql-common`.

---

## 3. Step-by-Step Implementation

### Step 1: Add API dependencies to `mcc-gaql-common`

**File:** `crates/mcc-gaql-common/Cargo.toml`

Add API dependencies as direct dependencies (no feature flag):

```toml
[dependencies]
# ... existing deps unchanged ...
googleads-rs = { git = "https://github.com/mhuang74/googleads-rs", branch = "main" }
tonic = { version = "0.14", features = ["transport", "tls-ring", "tls-native-roots"] }
yup-oauth2 = { version = "6.7" }
tokio-stream = { version = "0.1", features = ["net"] }
flexi_logger = { version = "0.22", features = ["compress"] }
```

**File:** `crates/mcc-gaql-common/src/lib.rs`

```rust
pub mod config;
pub mod field_metadata;
pub mod http_client;
pub mod paths;
pub mod auth;
pub mod googleads_api;
pub mod query;
pub mod util;
```

---

### Step 2: Move auth/API/query code to `mcc-gaql-common`

#### 2a. `crates/mcc-gaql-common/src/auth.rs` — Shared Auth Resolution

New types and functions that replace the `Cli`-coupled auth resolution:

```rust
use anyhow::{Context, Result};
use crate::config::{MyConfig, validate_and_normalize_customer_id};
use crate::paths::config_file_path;

const CRATE_NAME: &str = "mcc-gaql";

/// Auth flags shared across all binaries and subcommands.
/// Constructed directly from each binary's CLI args — no `Cli` struct dependency.
#[derive(Debug, Clone)]
pub struct SharedAuthArgs {
    pub customer_id: Option<String>,
    pub mcc_id: Option<String>,
    pub profile: Option<String>,
    pub user_email: Option<String>,
    pub remote_auth: bool,
}

/// Resolved auth configuration — the output of auth resolution,
/// independent of any CLI structure.
#[derive(Debug, Clone)]
pub struct ResolvedAuthConfig {
    pub mcc_customer_id: String,
    pub user_email: Option<String>,
    pub customer_id: Option<String>,
    pub token_cache_filename: String,
    pub dev_token: Option<String>,
    pub remote_auth: bool,
}

impl ResolvedAuthConfig {
    /// Convert to ApiAccessConfig for get_api_access().
    pub fn to_api_access_config(&self) -> crate::googleads_api::ApiAccessConfig {
        crate::googleads_api::ApiAccessConfig {
            mcc_customer_id: self.mcc_customer_id.clone(),
            token_cache_filename: self.token_cache_filename.clone(),
            user_email: self.user_email.clone(),
            dev_token: self.dev_token.clone(),
            use_remote_auth: self.remote_auth,
        }
    }
}

/// Resolve auth configuration from CLI args and optional config file.
/// Replaces the auth-resolution portion of `ResolvedConfig::from_args_and_config()`.
///
/// Priority: CLI args > config file > fallbacks
pub fn resolve_auth_config(
    auth: &SharedAuthArgs,
    config: Option<&MyConfig>,
) -> Result<ResolvedAuthConfig> {
    // Resolve MCC customer ID
    let mcc_customer_id = if let Some(mcc_id) = &auth.mcc_id {
        log::debug!("Using MCC from --mcc-id argument: {}", mcc_id);
        validate_and_normalize_customer_id(mcc_id).context("Invalid --mcc-id argument")?
    } else if let Some(config_mcc) = config.and_then(|c| c.mcc_id.as_ref()) {
        log::debug!("Using MCC from config profile: {}", config_mcc);
        validate_and_normalize_customer_id(config_mcc).context("Invalid mcc_id in config file")?
    } else if let Some(customer_id) = &auth.customer_id {
        log::warn!(
            "No --mcc-id specified. Using --customer-id ({}) as MCC. \
             This assumes the account is not under a manager account.",
            customer_id
        );
        validate_and_normalize_customer_id(customer_id).context("Invalid --customer-id argument")?
    } else if let Some(config_customer_id) = config.and_then(|c| c.customer_id.as_ref()) {
        log::warn!(
            "No mcc_id specified. Using customer_id ({}) from config as MCC.",
            config_customer_id
        );
        validate_and_normalize_customer_id(config_customer_id)
            .context("Invalid customer_id in config file")?
    } else {
        return Err(anyhow::anyhow!(
            "MCC customer ID required. Provide one of:\n  \
             1. CLI: --mcc-id <MCC_ID>\n  \
             2. Config profile with mcc_id: --profile <PROFILE_NAME>\n  \
             3. For solo accounts: --customer-id <CUSTOMER_ID>"
        ));
    };

    // Resolve user email
    let user_email = auth
        .user_email
        .clone()
        .or_else(|| config.and_then(|c| c.user_email.clone()));

    // Resolve token cache filename
    let explicit_token_cache = config.and_then(|c| c.token_cache_filename.clone());
    let token_cache_filename = if let Some(explicit_cache) = explicit_token_cache {
        explicit_cache
    } else if let Some(email) = user_email.as_ref() {
        crate::googleads_api::generate_token_cache_filename(email)
    } else {
        return Err(anyhow::anyhow!(
            "User email or explicit token cache filename required for authentication."
        ));
    };

    // Resolve customer_id
    let customer_id = auth
        .customer_id
        .as_ref()
        .or_else(|| config.and_then(|c| c.customer_id.as_ref()))
        .map(|id| validate_and_normalize_customer_id(id).context("Invalid customer_id"))
        .transpose()?;

    // Dev token from config only
    let dev_token = config.and_then(|c| c.dev_token.clone());

    Ok(ResolvedAuthConfig {
        mcc_customer_id,
        user_email,
        customer_id,
        token_cache_filename,
        dev_token,
        remote_auth: auth.remote_auth,
    })
}

/// Load a config profile by name.
/// Moved from mcc-gaql/src/config.rs:load().
pub fn load_profile(profile: &str) -> Result<MyConfig> {
    // Implementation is identical — just relocated.
    // Uses figment with Toml + Env providers, same as current load().
    ...
}

/// List all available profiles from the config file.
/// Moved from mcc-gaql/src/config.rs:list_profiles().
pub fn list_profiles() -> Result<Vec<String>> { ... }

/// Note: Profile auto-selection logic is NOT in common — each binary
/// decides when to auto-select based on its own conditions:
/// - mcc-gaql: conditional (only for --validate / --field-service)
/// - mcc-gaql-gen: always for validation
/// - mcc-gaql-mutate: always
/// Each binary's main.rs calls load_profile()/list_profiles() directly.
```

#### 2b. `crates/mcc-gaql-common/src/googleads_api.rs` — API Access

Moved from `mcc-gaql/src/googleads.rs` lines 59-302:

| Item | Source lines |
|---|---|
| `ENDPOINT` const | 59 |
| `GOOGLE_ADS_API_SCOPE` static | 66 |
| `FILENAME_CLIENT_SECRET` const | 65 |
| `GoogleAdsAPIAccess` struct | 89-99 |
| `GoogleAdsAPIAccess::renew_token()` | 104-129 |
| `Interceptor` impl for `GoogleAdsAPIAccess` | 131-145 |
| `generate_token_cache_filename()` | 150-153 |
| `get_dev_token()` | 158-176 |
| `get_client_secret()` | 179-207 |
| `ApiAccessConfig` struct | 210-216 |
| `get_api_access()` | 219-271 |
| `verify_and_confirm_auth()` | 274-302 |

No changes to logic. Imports updated to use `crate::paths::config_file_path` instead of `mcc_gaql_common::paths::config_file_path`.

#### 2c. `crates/mcc-gaql-common/src/util.rs` — Common Logger

```rust
use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

/// Initialize logger for any binary in the workspace.
///
/// Parameters:
/// - `crate_prefix`: Environment variable prefix (always "MCC_GAQL" across all crates)
/// - `verbose`: Enable debug-level logging
pub fn init_logger(crate_prefix: &str, verbose: bool) {
    let base_level = if verbose {
        "debug".to_string()
    } else {
        env::var(format!("{}_LOG_LEVEL", crate_prefix))
            .unwrap_or_else(|_| "off".to_string())
    };

    let my_log_dir = env::var(format!("{}_LOG_DIR", crate_prefix))
        .unwrap_or_else(|_| ".".to_string());

    let log_spec = format!("{}", base_level);

    Logger::try_with_env_or_str(log_spec)
        .unwrap()
        .use_utc()
        .log_to_file(
            FileSpec::default()
                .directory(my_log_dir)
                .suppress_timestamp()
                .basename(crate_prefix.to_lowercase().replace("_", "-")),
        )
        .format_for_files(flexi_logger::detailed_format)
        .o_append(true)
        .rotate(
            Criterion::Size(1_000_000),
            Naming::Numbers,
            Cleanup::KeepLogAndCompressedFiles(10, 100),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        .start()
        .unwrap();
}
```

Replaces duplicate `init_logger()` implementations in `mcc-gaql/src/util.rs` and `mcc-gaql-gen/src/main.rs`.

#### 2d. `crates/mcc-gaql-common/src/query.rs` — Query Primitives

Moved from `mcc-gaql/src/googleads.rs`:

| Item | Source lines |
|---|---|
| `SUB_ACCOUNTS_QUERY` const | 30-44 |
| `SUB_ACCOUNT_IDS_QUERY` const | 46-57 |
| `validate_gaql_query()` | 430-460 |
| `get_child_account_ids()` | 491-545 |
| `search_stream_rows()` | **NEW** |

New `search_stream_rows()`:

```rust
/// Execute a GAQL search_stream query and return raw GoogleAdsRow results.
/// No DataFrame dependency — usable by mcc-gaql-mutate for --from-query
/// and update-bidding current-state fetch.
pub async fn search_stream_rows(
    api_context: &GoogleAdsAPIAccess,
    customer_id: &str,
    query: &str,
) -> Result<Vec<GoogleAdsRow>> {
    use googleads_rs::google::ads::googleads::v23::services::{
        SearchGoogleAdsStreamRequest,
        google_ads_service_client::GoogleAdsServiceClient,
    };
    use tonic::codegen::InterceptedService;
    use tonic::transport::Channel;

    let mut client: GoogleAdsServiceClient<InterceptedService<Channel, GoogleAdsAPIAccess>> =
        GoogleAdsServiceClient::with_interceptor(
            api_context.channel.clone(),
            api_context.clone(),
        );

    let response = client
        .search_stream(SearchGoogleAdsStreamRequest {
            customer_id: customer_id.to_string(),
            query: query.to_string(),
            summary_row_setting: 0,
        })
        .await
        .map_err(|status| {
            anyhow::anyhow!(
                "GoogleAdsClient streaming error. Account: {}, Message: '{}'",
                customer_id,
                status.message()
            )
        })?;

    let mut stream = response.into_inner();
    let mut rows = Vec::new();

    while let Some(item) = stream.next().await {
        match item {
            Ok(stream_response) => {
                rows.extend(stream_response.results);
            }
            Err(status) => {
                bail!(
                    "GoogleAdsClient streaming error. Account: {}, Message: '{}'",
                    customer_id,
                    status.message()
                );
            }
        }
    }

    Ok(rows)
}
```

---

### Step 3: Upgrade `mcc-gaql` clap 3.1 → 4.0

**File:** `crates/mcc-gaql/Cargo.toml`

```toml
clap = { version = "4", features = ["derive", "cargo"] }  # was "3.1"
```

**File:** `crates/mcc-gaql/src/args.rs`

Clap 4 migration changes:

| Clap 3 syntax | Clap 4 syntax | Occurrences |
|---|---|---|
| `#[clap(author, about, version = ...)]` | `#[command(author, about, version = ...)]` | 1 (Cli struct) |
| `#[clap(subcommand)]` | `#[command(subcommand)]` | 1 |
| `#[clap(long, help = "...")]` | `#[arg(long, help = "...")]` | ~20 field attrs |
| `multiple_occurrences(true)` | `action = clap::ArgAction::Append` | 3 (`--groupby`, `--sortby`) |

Remove in this step (mutation code moves out):
- `Command` enum (lines 74-130)
- `MutationOpCli` enum (lines 40-71)
- `cli_from_mutate_args()` (lines 285-317)
- `parse_field_set()`/`parse_field_sets()` (lines 260-283)
- `Cli.command` field (line 136)
- All mutation-related tests (lines 370-417)

Add:
- `impl Cli { pub fn auth_args(&self) -> SharedAuthArgs }` conversion method

**File:** `crates/mcc-gaql/src/config.rs`

- `ResolvedConfig::from_args_and_config()` delegates auth resolution to `mcc_gaql_common::auth::resolve_auth_config()`

**File:** `crates/mcc-gaql/src/main.rs`

- Replace `util::init_logger()` with `mcc_gaql_common::util::init_logger("MCC_GAQL", false)`
- Remove `use mcc_gaql::util;` import statement
- Profile resolution stays local in `main.rs`, calling `mcc_gaql_common::auth::load_profile()` and `mcc_gaql_common::auth::list_profiles()` directly
- Remove duplicated auth resolution logic (MCC fallback chain, user_email, token_cache_filename, dev_token)
- `ResolvedConfig` embeds `ResolvedAuthConfig` or repeats only the query-specific fields

---

### Step 4: Update `mcc-gaql` to use common auth + remove mutation code

**Remove files:**
- `crates/mcc-gaql/src/mutation_validate.rs` → moves to `mcc-gaql-mutate`

**Remove from `crates/mcc-gaql/src/googleads.rs`:**
- `GoogleAdsAPIAccess` struct and all associated code (moved to common)
- `ApiAccessConfig` struct (moved to common)
- `get_api_access()` (moved to common)
- `generate_token_cache_filename()` (moved to common)
- `validate_gaql_query()` (moved to common)
- `get_child_account_ids()` (moved to common)
- `SUB_ACCOUNTS_QUERY`, `SUB_ACCOUNT_IDS_QUERY` constants (moved to common)
- `MutationParams` struct (moves to `mcc-gaql-mutate`)
- `mutate_resource()` (moves to `mcc-gaql-mutate`)
- `build_mutation_request()` (moves to `mcc-gaql-mutate`)
- `build_mutation_request_from_builder()` (moves to `mcc-gaql-mutate`)
- `get_dev_token()`, `get_client_secret()`, `verify_and_confirm_auth()` (moved to common)

**Keep in `crates/mcc-gaql/src/googleads.rs`:**
- `gaql_query_with_client()` — DataFrame-returning query (uses `polars`)
- `gaql_query()` — DataFrame-returning query wrapper
- `fields_query()` — field service query
- `GOOGLE_ADS_METRICS_INTEGER_FIELDS` constant — DataFrame-specific
- DataFrame/Series/metric parsing helpers

**Update `crates/mcc-gaql/src/googleads.rs` imports:**
```rust
use mcc_gaql_common::googleads_api::GoogleAdsAPIAccess;
use mcc_gaql_common::query::{search_stream_rows, validate_gaql_query, get_child_account_ids, SUB_ACCOUNTS_QUERY, SUB_ACCOUNT_IDS_QUERY};
```

**Update `crates/mcc-gaql/src/main.rs`:**
- Remove `handle_mutate()` function (lines 462-606)
- Remove `Command::Mutate` dispatch (lines 72-75)
- Remove `use mcc_gaql::mutation_validate`
- Use `mcc_gaql_common::googleads_api::get_api_access()` instead of local
- Profile resolution stays local in `main.rs`, calling `mcc_gaql_common::auth::load_profile()` and `mcc_gaql_common::auth::list_profiles()` directly. Conditional auto-selection logic (only for `--validate`/`--field-service`) preserved as-is. Dedup the profile resolution currently at lines 83-98 and 206-227 into a single local helper.

**Update `crates/mcc-gaql/src/lib.rs`:**
```rust
pub mod args;
pub mod config;
pub mod field_metadata;
pub mod googleads;
pub mod setup;
// removed: pub mod mutation_validate;
// removed: util; (moved to mcc-gaql-common)
```

**Update `crates/mcc-gaql/Cargo.toml`:**
```toml
mcc-gaql-common = { workspace = true }
# Remove:
# prost-reflect = "0.16"  (no longer needed — mutation_validate moved out)
```

**Update `crates/mcc-gaql/src/config.rs`:**
- `ResolvedConfig::from_args_and_config()` uses `resolve_auth_config()` from common
- `validate_for_operation()` unchanged (query-specific validation)
- `load()` → delegates to `mcc_gaql_common::auth::load_profile()`
- `list_profiles()` → delegates to `mcc_gaql_common::auth::list_profiles()`
- `display_config()` stays in `mcc-gaql` (query-specific UI)
- Test: update `Cli` construction (remove `command: None` field)

---

### Step 5: Update `mcc-gaql-gen` to drop `mcc-gaql` dependency

**File:** `crates/mcc-gaql-gen/Cargo.toml`

```toml
# Remove:
# mcc-gaql = { workspace = true }
# Add:
mcc-gaql-common = { workspace = true }
```

**File:** `crates/mcc-gaql-gen/src/main.rs` (lines 1117-1211, `run_validation()`)

Replace:
```rust
use mcc_gaql::config as mcc_config;
use mcc_gaql::googleads::{ApiAccessConfig, generate_token_cache_filename, get_api_access, validate_gaql_query};
```

With:
```rust
use mcc_gaql_common::auth::{load_profile, list_profiles, resolve_auth_config, SharedAuthArgs};
use mcc_gaql_common::googleads_api::{ApiAccessConfig, get_api_access};
use mcc_gaql_common::query::validate_gaql_query;
use mcc_gaql_common::util::init_logger;
```

Rewrite `run_validation()` to use `resolve_auth_config()` — eliminates ~50 lines of manual MCC/email/dev_token resolution currently duplicated from `mcc-gaql`:

```rust
async fn run_validation(query: &str, profile: Option<String>) -> Result<()> {
    let auth = SharedAuthArgs {
        customer_id: None,
        mcc_id: None,
        profile,
        user_email: None,
        remote_auth: false,
    };

    let config = if let Some(profile_name) = &auth.profile {
        log::info!("Config profile: {profile_name}");
        Some(load_profile(profile_name)
            .context(format!("Loading config for profile: {profile_name}"))?)
    } else {
        let profiles = list_profiles()?;
        if let Some(profile_name) = profiles.last() {
            eprintln!("Using profile '{}'", profile_name);
            log::info!("Auto-selected profile: {profile_name}");
            Some(load_profile(profile_name)
                .context(format!("Loading config for profile: {profile_name}"))?)
        } else {
            None
        }
    };

    let auth_config = resolve_auth_config(&auth, config.as_ref())
        .map_err(|e| anyhow::anyhow!("__config_error__:{}", e))?;

    let api_context = get_api_access(&auth_config.to_api_access_config())
        .await
        .map_err(|e| anyhow::anyhow!("__config_error__:{}", e))?;

    validate_gaql_query(api_context, &auth_config.mcc_customer_id, query).await
}
```

Build time improvement: `mcc-gaql-gen` no longer transitively compiles `polars`, `cacache`, `bincode`, `dialoguer`, `figment`, `itertools`, `thousands`.

Update logger initialization in `mcc-gaql-gen/src/main.rs`:

```rust
// Replace local init_logger() call (line 313):
// init_logger(cli.verbose);

// With common logger:
init_logger("MCC_GAQL", cli.verbose);

// Remove local init_logger() function definition (lines 1687-1713)
```

---

### Step 6: Create `mcc-gaql-mutate` crate

#### Directory Structure

```
crates/mcc-gaql-mutate/
├── Cargo.toml
├── build.rs
└── src/
    ├── main.rs
    ├── args.rs
    ├── mutation.rs
    └── mutation_validate.rs
```

#### `crates/mcc-gaql-mutate/Cargo.toml`

```toml
[package]
name = "mcc-gaql-mutate"
version.workspace = true
authors.workspace = true
edition.workspace = true
description = "Mutate Google Ads resources via CLI."

[dependencies]
mcc-gaql-common = { workspace = true }
anyhow = { workspace = true }
tokio = { workspace = true }
log = { workspace = true }
chrono = { workspace = true }
clap = { version = "4", features = ["derive", "cargo"] }
dialoguer = "0.11"
googleads-rs = { git = "https://github.com/mhuang74/googleads-rs", branch = "main" }
prost-reflect = "0.16"

[build-dependencies]
chrono = { workspace = true }
```

#### `crates/mcc-gaql-mutate/build.rs`

Standard build.rs pattern for version information:
```rust
use std::env;
use std::fs;

fn main() {
    let mut git_hash = if let Ok(hash) = std::process::Command::new("git")
        .args(&["rev-parse", "--short=8", "HEAD"])
        .output()
    {
        String::from_utf8_lossy(&hash.stdout).trim().to_string()
    } else {
        "unknown".to_string()
    };

    if git_hash.is_empty() {
        git_hash = "unknown".to_string();
    }

    let build_time = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC").to_string();

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
}
```

#### `crates/mcc-gaql-mutate/src/args.rs`

```rust
use anyhow::{Context, Result, bail};
use clap::{ArgAction, Parser, Subcommand};
use googleads_rs::{FieldUpdate, MutationOp};
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

impl From<MutationOpCli> for MutationOp {
    fn from(op: MutationOpCli) -> Self {
        match op {
            MutationOpCli::Update => MutationOp::Update,
            MutationOpCli::Create => MutationOp::Create,
            MutationOpCli::Remove => MutationOp::Remove,
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

        #[arg(long, default_value = "update", help = "Operation type: update, create, remove")]
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
```

#### `crates/mcc-gaql-mutate/src/mutation.rs`

Moved from `mcc-gaql/src/googleads.rs:547-633`:

```rust
use anyhow::{Context, Result};
use googleads_rs::{DynamicMutationBuilder, FieldUpdate, MutationOp};
use googleads_rs::google::ads::googleads::v23::services::{
    MutateGoogleAdsRequest, MutateGoogleAdsResponse,
    google_ads_service_client::GoogleAdsServiceClient,
};
use mcc_gaql_common::googleads_api::GoogleAdsAPIAccess;
use tonic::codegen::InterceptedService;
use tonic::transport::Channel;

pub struct MutationParams {
    pub resource_type: String,
    pub customer_id: String,
    pub resource_name: String,
    pub operation: MutationOp,
    pub field_updates: Vec<FieldUpdate>,
    pub validate_only: bool,
    pub partial_failure: bool,
}

pub async fn mutate_resource(
    api_context: GoogleAdsAPIAccess,
    params: MutationParams,
) -> Result<MutateGoogleAdsResponse> {
    let request = build_mutation_request(
        &params.resource_type,
        &params.customer_id,
        &params.resource_name,
        params.operation,
        &params.field_updates,
        params.validate_only,
        params.partial_failure,
    )?;

    let mut client =
        GoogleAdsServiceClient::with_interceptor(api_context.channel.clone(), api_context);

    let response = client.mutate(request).await.map_err(|status| {
        let details = String::from_utf8_lossy(status.details())
            .trim()
            .replace(|c: char| !c.is_ascii(), "")
            .replace("%", " ")
            .replace("\n", " ")
            .replace("\r", " ");
        if details.is_empty() {
            anyhow::anyhow!("{}", status.message())
        } else {
            anyhow::anyhow!("{}: {}", status.message(), details)
        }
    })?;

    Ok(response.into_inner())
}

#[allow(clippy::too_many_arguments)]
pub fn build_mutation_request(
    resource_type: &str,
    customer_id: &str,
    resource_name: &str,
    operation: MutationOp,
    field_updates: &[FieldUpdate],
    validate_only: bool,
    partial_failure: bool,
) -> Result<MutateGoogleAdsRequest> {
    let mut builder = DynamicMutationBuilder::new(resource_type, customer_id);
    builder.operation_type(operation);
    builder.validate_only(validate_only);
    builder.partial_failure(partial_failure);

    for update in field_updates {
        builder.set_field(&update.field_path, &update.value);
    }

    builder
        .build(resource_name)
        .map_err(|e| anyhow::anyhow!("Failed to build mutation request: {}", e))
}
```

#### `crates/mcc-gaql-mutate/src/mutation_validate.rs`

Moved verbatim from `crates/mcc-gaql/src/mutation_validate.rs`. All 10 unit tests move with it.

#### `crates/mcc-gaql-mutate/src/main.rs`

```rust
use anyhow::{Context, Result};
use googleads_rs::MutationOp;

use mcc_gaql_common::auth::{load_profile, list_profiles, resolve_auth_config};
use mcc_gaql_common::googleads_api::get_api_access;
use mcc_gaql_common::util::init_logger;

use crate::args::{self, Command};
use crate::mutation;
use crate::mutation_validate;

fn print_startup_banner() {
    let version_info = format!(
        "v{} ({}) built {}",
        env!("CARGO_PKG_VERSION"),
        env!("GIT_HASH"),
        env!("BUILD_TIME")
    );
    log::info!("═════════════════════════════════════════════════════════════════");
    log::info!(" mcc-gaql-mutate {} ", version_info);
    log::info!("═════════════════════════════════════════════════════════════════");
}

/// Profile resolution for mcc-gaql-mutate: always auto-select if none specified.
fn resolve_profile(auth: &mcc_gaql_common::auth::SharedAuthArgs) -> Result<Option<mcc_gaql_common::config::MyConfig>> {
    if let Some(profile_name) = &auth.profile {
        log::info!("Config profile: {profile_name}");
        Some(load_profile(profile_name)
            .context(format!("Loading config for profile: {profile_name}")))
            .transpose()
    } else {
        let profiles = list_profiles()?;
        if let Some(profile_name) = profiles.last() {
            eprintln!("Using profile '{}'", profile_name);
            log::info!("Auto-selected profile: {profile_name}");
            Some(load_profile(profile_name)
                .context(format!("Loading config for profile: {profile_name}")))
                .transpose()
        } else {
            Ok(None)
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    init_logger("MCC_GAQL", false);
    print_startup_banner();

    let cli = args::Cli::parse();

    // Auth resolution — no shim, direct construction
    let config = resolve_profile(&cli.auth_args())?;
    let auth_config = resolve_auth_config(&cli.auth_args(), config.as_ref())?;

    match &cli.command {
        Command::Mutate { resource, resource_name, operation, field_set, dry_run, preview, partial_failure } => {
            let customer_id = auth_config.customer_id.as_deref().ok_or_else(|| {
                anyhow::anyhow!("--customer-id is required for mutate operations")
            })?;

            let field_updates = args::parse_field_sets(field_set)?;
            mutation_validate::validate_mutation_locally(resource, &field_updates)?;

            if *preview {
                let request = mutation::build_mutation_request(
                    resource,
                    customer_id,
                    resource_name,
                    operation.into(),
                    &field_updates,
                    *dry_run,
                    *partial_failure,
                )?;

                println!("MutateGoogleAdsRequest:");
                println!("  customer_id: {}", request.customer_id);
                println!("  validate_only: {}", request.validate_only);
                println!("  partial_failure: {}", request.partial_failure);
                println!("  operations: {} operation(s)", request.mutate_operations.len());
                println!();
                println!("  Operation 1:");
                println!("    resource_type: {}", resource);
                println!("    operation_type: {:?}", operation);
                println!("    resource_name: {}", resource_name);
                if !field_updates.is_empty() {
                    println!("    field_mask:");
                    for update in &field_updates {
                        println!("      - {}", update.field_path);
                    }
                    println!("    field_values:");
                    for update in &field_updates {
                        println!("      {} = {}", update.field_path, update.value);
                    }
                }
                return Ok(());
            }

            if *dry_run {
                eprintln!("[dry-run] Validating mutation...");
                eprintln!("[dry-run] Resource: {}", resource);
                eprintln!("[dry-run] Operation: {:?}", operation);
                eprintln!("[dry-run] Resource name: {}", resource_name);
                if !field_updates.is_empty() {
                    eprintln!("[dry-run] Fields:");
                    for update in &field_updates {
                        eprintln!("[dry-run]   {} = {}", update.field_path, update.value);
                    }
                }
            }

            let api_context = get_api_access(&auth_config.to_api_access_config())
                .await
                .context("Authentication required for mutate operations")?;

            let resource_display = resource.clone();
            let resource_name_display = resource_name.clone();
            let op_display = *operation;

            let response = mutation::mutate_resource(
                api_context,
                mutation::MutationParams {
                    resource_type: resource.clone(),
                    customer_id: customer_id.to_string(),
                    resource_name: resource_name.clone(),
                    operation: operation.into(),
                    field_updates,
                    validate_only: *dry_run,
                    partial_failure: *partial_failure,
                },
            )
            .await?;

            if *dry_run {
                eprintln!("[dry-run] Validation PASSED — mutation would succeed if applied");
            } else {
                println!("Mutation succeeded.");
                println!("  Resource: {}", resource_display);
                println!("  Operation: {:?}", op_display);
                println!("  Resource name: {}", resource_name_display);
                let num_results = response.mutate_operation_responses.len();
                if num_results > 0 {
                    println!("  Results: {} operation(s) completed", num_results);
                }
            }
        }
    }

    Ok(())
}
```

---

### Step 7: Update workspace `Cargo.toml`

**File:** `Cargo.toml`

```toml
[workspace]
members = [
    "crates/mcc-gaql",
    "crates/mcc-gaql-gen",
    "crates/mcc-gaql-common",
    "crates/mcc-gaql-mutate",
]
```

---

## 4. Execution Order

Execute in this order to keep the tree green at each step:

| Step | Description | Verification |
|---|---|---|
| 1 | Add API dependencies to `mcc-gaql-common` | `cargo check -p mcc-gaql-common` |
| 2 | Move auth/API/query code to common | `cargo check -p mcc-gaql-common` |
| 3 | Update `mcc-gaql` to use common auth (keep mutation code temporarily) | `cargo check -p mcc-gaql -p mcc-gaql-common` |
| 4 | Upgrade `mcc-gaql` clap 3→4 + remove mutation code | `cargo check -p mcc-gaql -p mcc-gaql-common` + `cargo test -p mcc-gaql -- --test-threads=1` |
| 5 | Update `mcc-gaql-gen` to drop `mcc-gaql` dep | `cargo check -p mcc-gaql-gen -p mcc-gaql-common` |
| 6 | Create `mcc-gaql-mutate` crate | `cargo check -p mcc-gaql-mutate` + `cargo test -p mcc-gaql-mutate -- --test-threads=1` |
| 7 | Update workspace Cargo.toml | `cargo check --workspace` + `cargo fmt --all -- --check` + `cargo clippy` |

Steps 3-4 are combined in practice since they're interdependent (clap upgrade + removal + rewiring).

---

## 5. Verification Commands

After all steps:

```bash
cargo check -p mcc-gaql -p mcc-gaql-common -p mcc-gaql-mutate
cargo test -p mcc-gaql -- --test-threads=1
cargo test -p mcc-gaql-common -- --test-threads=1
cargo test -p mcc-gaql-mutate -- --test-threads=1
cargo fmt --all -- --check
cargo clippy -p mcc-gaql -p mcc-gaql-common -p mcc-gaql-mutate
```

Note: Avoid `cargo check --workspace` or `cargo test --workspace` — the `mcc-gaql-gen` crate (~400MB) is very slow to compile and may time out. Only build/test the relevant crates.

---

## 6. Behavior Preservation

| Command | Before | After |
|---|---|---|
| `mcc-gaql "SELECT ..." --profile X` | Query runs, outputs DataFrame | **Unchanged** |
| `mcc-gaql --validate ...` | Validates query | **Unchanged** |
| `mcc-gaql --setup` / `--show-config` | Runs wizard / displays config | **Unchanged** |
| `mcc-gaql mutate --resource Campaign --resource-name ... --set ... --dry-run` | Dry-run mutation | → **`mcc-gaql-mutate mutate ...`** (same behavior, new binary) |
| `mcc-gaql mutate ... --preview` | Offline preview | → **`mcc-gaql-mutate mutate ... --preview`** (same output) |
| Pre-flight validation errors | Field/resource error messages | **Unchanged** (same `mutation_validate` module) |

---

## 7. Future Extensibility

### `update-bidding` subcommand

Added as `Command::UpdateBidding` variant in `mcc-gaql-mutate/src/args.rs`. Auth flags are top-level — no duplication. Current-state fetch uses `mcc_gaql_common::query::search_stream_rows()`. NL parsing, strategy validation, and confirmation UX are contained in `mcc-gaql-mutate`.

### Additional domain subcommands

Any new mutation subcommand (e.g., `pause-campaign`, `update-budget`) follows the same pattern: add a `Command` variant, use top-level auth flags, call `mutation::mutate_resource()` or `mcc_gaql_common::query::search_stream_rows()`. Zero impact on `mcc-gaql` query binary.

---

## 8. Risk & Mitigation

| Risk | Mitigation |
|---|---|
| Clap 3→4 migration introduces subtle parsing changes | Clap 4 is largely compatible; `multiple_occurrences` → `ArgAction::Append` is the main change. Test all CLI invocations. |
| `mcc-gaql-common` becomes too heavy | All three binaries need the same API dependencies (googleads-rs, tonic, etc.), so no feature flag is needed. |
| `prost-reflect` version mismatch | `mcc-gaql-common` inherits `googleads-rs`'s `prost-reflect` 0.16. `mcc-gaql-mutate` pins the same version explicitly. |
| `mcc-gaql-gen` `run_validation()` rewrite introduces regression | Existing integration tests cover validation; manual smoke test with `mcc-gaql-gen test-run --validate`. |
| `googleads-rs` git dep with `[patch]` | Same as current — no change. `mcc-gaql-common` and `mcc-gaql-mutate` both depend on it via the same patch. |
| Common logger behavior differences | Parameterized init_logger() preserves per-binary behavior (basename, env prefix, verbose flag). |

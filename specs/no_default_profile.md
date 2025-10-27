# No Default Profile

## Problem

Currently, when no `--profile` is specified, the CLI defaults to the "test" profile. This causes issues when using `--user` without a profile:

```bash
# This command tries to use "test" profile's MCC, but mhuang@themade.org's credentials
mcc-gaql -c 3902228771 -q all_campaigns --user mhuang@themade.org

# Error: Uses test profile's MCC which mhuang@themade.org doesn't have access to
ERROR: The caller does not have permission to access customer
```

The user authenticated correctly with `--user mhuang@themade.org`, but the MCC customer ID came from the "test" profile config, which that user doesn't have permission to access.

## Proposed Solution

1. **No default profile**: Remove the automatic fallback to "test" profile
2. **Config-free mode**: Allow full operation using only CLI arguments
3. **Use customer_id as MCC**: When no profile and no `--mcc` specified, use `--customer-id` as the MCC
4. **Clear error messages**: When insufficient information provided, explain what's needed

## Implementation Plan

### 1. Add `--mcc` CLI Argument

Already implemented in `src/args.rs`:

```rust
/// MCC (Manager) Customer ID for login-customer-id header
#[clap(short = 'm', long)]
pub mcc: Option<String>,
```

### 2. Update `src/main.rs` - Config Loading Logic

Change from:
```rust
let profile = &args.profile.unwrap_or_else(|| "test".to_owned());
let config = config::load(profile).context(...)?;
```

To:
```rust
// Only load config if profile is explicitly specified
let config = if let Some(profile) = &args.profile {
    log::info!("Config profile: {profile}");
    Some(config::load(profile).context(format!("Loading config for profile: {profile}"))?)
} else {
    log::info!("No profile specified, using CLI arguments only");
    None
};
```

### 3. Update User Email Resolution

Change from:
```rust
let user_email = args.user.as_deref().or(config.user.as_deref());
```

To:
```rust
// Priority: CLI arg > config file > None
let user_email = args.user.as_deref()
    .or_else(|| config.as_ref().and_then(|c| c.user.as_deref()));
```

### 4. Update MCC Customer ID Resolution

Add new resolution logic with priority:
1. CLI `--mcc` argument (highest priority)
2. CLI `--customer-id` argument (fallback when no --mcc)
3. Config file `mcc_customerid`
4. Error if none provided (lowest priority)

```rust
// Priority: CLI --mcc > CLI --customer-id > config file
let mcc_customer_id = args.mcc.as_ref()
    .or(args.customer_id.as_ref())
    .map(|s| s.as_str())
    .or_else(|| config.as_ref().map(|c| c.mcc_customerid.as_str()))
    .ok_or_else(|| anyhow::anyhow!(
        "MCC customer ID required. Either:\n  \
         1. Provide via CLI: --mcc <MCC_ID> or --customer-id <CUSTOMER_ID>\n  \
         2. Specify config profile: --profile <PROFILE_NAME>"
    ))?;
```

### 5. Update All Config References

Throughout `main.rs`, update all direct config field accesses to handle `Option<MyConfig>`:

**Line ~50: Queries filename**
```rust
// Before
let query_filename = config.queries_filename.as_ref()
    .expect("Query cookbook filename undefined");

// After
let query_filename = config.as_ref()
    .and_then(|c| c.queries_filename.as_ref())
    .ok_or_else(|| anyhow::anyhow!(
        "Query cookbook not available. Either:\n  \
         1. Provide GAQL query directly: <QUERY>\n  \
         2. Specify config profile with queries_filename: --profile <PROFILE_NAME>"
    ))?;
```

**Line ~76: Queries filename for natural language**
```rust
// Before
let query_filename = config.queries_filename.as_ref()
    .expect("Query cookbook filename undefined");

// After
let query_filename = config.as_ref()
    .and_then(|c| c.queries_filename.as_ref())
    .ok_or_else(|| anyhow::anyhow!(
        "Query cookbook required for natural language mode. \
         Specify config profile with queries_filename: --profile <PROFILE_NAME>"
    ))?;
```

**Line ~113-140: API access calls**
```rust
// Before
googleads::get_api_access(
    &config.mcc_customerid,
    user_email,
    config.token_cache_filename.as_deref(),
).await

// After
googleads::get_api_access(
    mcc_customer_id,
    user_email,
    config.as_ref().and_then(|c| c.token_cache_filename.as_deref()),
).await
```

**Line ~126-132: Token cache cleanup**
```rust
// Before
let token_cache_filename = if let Some(legacy) = &config.token_cache_filename {
    legacy.clone()
} else if let Some(email) = user_email {
    googleads::generate_token_cache_filename(email)
} else {
    "tokencache_default.json".to_string()
};

// After
let token_cache_filename = config.as_ref()
    .and_then(|c| c.token_cache_filename.as_ref().cloned())
    .or_else(|| user_email.map(googleads::generate_token_cache_filename))
    .unwrap_or_else(|| "tokencache_default.json".to_string());
```

**Line ~159-165: List child accounts - MCC reference**
```rust
// Before
log::debug!(
    "Listing ALL child accounts under MCC {}",
    &config.mcc_customerid
);
(
    config.mcc_customerid,
    googleads::SUB_ACCOUNTS_QUERY.to_owned(),
)

// After
log::debug!(
    "Listing ALL child accounts under MCC {}",
    mcc_customer_id
);
(
    mcc_customer_id.to_string(),
    googleads::SUB_ACCOUNTS_QUERY.to_owned(),
)
```

**Line ~212: Query all linked child accounts**
```rust
// Before
let customer_id = config.mcc_customerid;

// After
let customer_id = mcc_customer_id.to_string();
```

**Line ~220-229: CustomerIDs file**
```rust
// Before
if config.customerids_filename.is_some() {
    let customerids_path =
        crate::config::config_file_path(&config.customerids_filename.unwrap()).unwrap();
    log::debug!("Querying accounts listed in file: {}", customerids_path.display());
    (util::get_child_account_ids_from_file(customerids_path.as_path()).await).ok()
} else {
    log::warn!("Expecting customerids file but none found in config");
    None
}

// After
if let Some(customerids_filename) = config.as_ref().and_then(|c| c.customerids_filename.as_ref()) {
    let customerids_path =
        crate::config::config_file_path(customerids_filename).unwrap();
    log::debug!("Querying accounts listed in file: {}", customerids_path.display());
    (util::get_child_account_ids_from_file(customerids_path.as_path()).await).ok()
} else {
    log::warn!("No customerids file specified. Use --customer-id or --all-linked-child-accounts");
    None
}
```

### 6. Update Config Module (Optional)

In `src/config.rs`, the `load()` function signature can remain the same since it already returns `Result<MyConfig>`. The caller in `main.rs` will handle whether to call it or not.

## Usage Examples

### Config-Free Mode (NEW)

```bash
# Single account query - user's own account
mcc-gaql -c 3902228771 -q all_campaigns --user mhuang@themade.org

# Query with explicit MCC different from customer account
mcc-gaql -c 1234567890 --mcc 9876543210 -q all_campaigns --user mhuang@themade.org

# Query all linked accounts under MCC
mcc-gaql --mcc 9876543210 --all-linked-child-accounts -q all_campaigns --user mhuang@themade.org
```

### Profile-Based Mode (EXISTING - Still Works)

```bash
# Use test profile (must specify explicitly now)
mcc-gaql --profile test -q all_campaigns

# Override profile's MCC
mcc-gaql --profile test --mcc 9876543210 -q all_campaigns

# Override profile's user
mcc-gaql --profile test --user other@example.com -q all_campaigns
```

### Error Messages

**Missing MCC and profile:**
```bash
$ mcc-gaql -q all_campaigns --user mhuang@themade.org
Error: MCC customer ID required. Either:
  1. Provide via CLI: --mcc <MCC_ID> or --customer-id <CUSTOMER_ID>
  2. Specify config profile: --profile <PROFILE_NAME>
```

**Stored query without profile:**
```bash
$ mcc-gaql -c 3902228771 -q all_campaigns --user mhuang@themade.org
Error: Query cookbook not available. Either:
  1. Provide GAQL query directly: <QUERY>
  2. Specify config profile with queries_filename: --profile <PROFILE_NAME>
```

**Natural language mode without profile:**
```bash
$ mcc-gaql -c 3902228771 -n "show me campaigns" --user mhuang@themade.org
Error: Query cookbook required for natural language mode. Specify config profile with queries_filename: --profile <PROFILE_NAME>
```

## Testing Plan

1. **Test config-free mode:**
   ```bash
   mcc-gaql -c 3902228771 "SELECT campaign.id, campaign.name FROM campaign" --user mhuang@themade.org
   ```

2. **Test with --mcc override:**
   ```bash
   mcc-gaql -c 1111111111 --mcc 2222222222 "SELECT ..." --user mhuang@themade.org
   ```

3. **Test profile mode still works:**
   ```bash
   mcc-gaql --profile test -q all_campaigns
   ```

4. **Test error messages:**
   ```bash
   # Should error with helpful message
   mcc-gaql -q all_campaigns --user mhuang@themade.org
   ```

5. **Test stored queries require profile:**
   ```bash
   # Should error explaining need profile for query cookbook
   mcc-gaql -c 3902228771 -q all_campaigns --user mhuang@themade.org
   ```

## Migration Guide

### For Existing Users

**Before (implicit test profile):**
```bash
mcc-gaql -q all_campaigns
```

**After (explicit profile required):**
```bash
mcc-gaql --profile test -q all_campaigns
```

**Or switch to config-free mode:**
```bash
mcc-gaql -c 3902228771 "SELECT campaign.id FROM campaign" --user your@email.com
```

### For New Users

New users can start without any config file:
```bash
mcc-gaql -c YOUR_CUSTOMER_ID "YOUR_GAQL_QUERY" --user your@email.com
```

## Benefits

1. **No magic defaults**: Explicit is better than implicit
2. **Config-free usage**: Can use tool without creating config file
3. **Better error messages**: Clear guidance when information is missing
4. **Prevents auth confusion**: Each user's credentials map to correct MCC
5. **Flexible**: Still supports profiles for complex setups

## Breaking Changes

**Breaking:** Commands without `--profile` will no longer default to "test" profile.

**Migration:** Existing scripts/aliases that rely on implicit "test" profile must add `--profile test` explicitly.

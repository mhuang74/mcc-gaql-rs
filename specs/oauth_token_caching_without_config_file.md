# OAuth Token Caching Without Config File

## Overview

Implement user-based token caching so users can specify their email via `--user` argument (or config file) instead of manually managing token cache filenames. The token cache filename will be auto-generated from the user email.

## Current Behavior

- Token cache filename must be specified in config file: `token_cache_filename = "tokencache.json"`
- No association between the cached token and the authenticated user
- Users must manually manage which cache file corresponds to which Google account

## Proposed Behavior

- Add `--user` CLI argument to specify user email for OAuth2 authentication
- Add optional `user` field to config file
- Auto-generate token cache filename from user email: `tokencache_{sanitized_email}.json`
- Remove requirement for `token_cache_filename` in config (make it optional)
- Support backward compatibility for existing configs with `token_cache_filename`

## Implementation Plan

### 1. Update CLI Arguments (`src/args.rs`)

Add new `--user` argument:

```rust
/// User email for OAuth2 authentication (auto-generates token cache)
#[clap(short = 'u', long)]
pub user: Option<String>,
```

### 2. Update Config Structure (`src/config.rs`)

Modify `MyConfig` struct:

```rust
#[derive(Deserialize, Serialize, Debug)]
pub struct MyConfig {
    /// MCC Account ID is mandatory
    pub mcc_customerid: String,

    /// Optional user email for OAuth2 (preferred over token_cache_filename)
    pub user: Option<String>,

    /// Token Cache filename (legacy - use 'user' instead)
    pub token_cache_filename: Option<String>,

    /// Optional file containing child customer_ids to query
    pub customerids_filename: Option<String>,

    /// Optional TOML file with stored queries
    pub queries_filename: Option<String>,
}
```

### 3. Update Google Ads API Access (`src/googleads.rs`)

#### 3.1 Add Helper Function

Add function to generate sanitized token cache filename from email:

```rust
/// Generate token cache filename from user email
/// Sanitizes email by replacing @ with _at_ and . with _
/// Example: user@example.com -> tokencache_user_at_example_com.json
fn generate_token_cache_filename(user_email: &str) -> String {
    let sanitized = user_email
        .replace('@', "_at_")
        .replace('.', "_");
    format!("tokencache_{}.json", sanitized)
}
```

#### 3.2 Update `get_api_access()` Signature

Change from:
```rust
pub async fn get_api_access(
    mcc_customer_id: &str,
    token_cache_filename: &str,
) -> Result<GoogleAdsAPIAccess>
```

To:
```rust
pub async fn get_api_access(
    mcc_customer_id: &str,
    user_email: Option<&str>,
    legacy_token_cache_filename: Option<&str>,
) -> Result<GoogleAdsAPIAccess>
```

#### 3.3 Implement Token Cache Resolution Logic

```rust
let token_cache_filename = if let Some(legacy) = legacy_token_cache_filename {
    // Legacy path: use explicit filename
    legacy.to_string()
} else if let Some(email) = user_email {
    // New path: auto-generate from email
    generate_token_cache_filename(email)
} else {
    // Default: use generic cache name
    "tokencache_default.json".to_string()
};
```

### 4. Update Main Application Logic (`src/main.rs`)

#### 4.1 Resolve User Email with Priority

```rust
// Priority: CLI arg > config file > None
let user_email = args.user.as_deref().or(config.user.as_deref());
```

#### 4.2 Update API Access Calls

Change from:
```rust
googleads::get_api_access(&config.mcc_customerid, &config.token_cache_filename).await
```

To:
```rust
googleads::get_api_access(
    &config.mcc_customerid,
    user_email,
    config.token_cache_filename.as_deref()
).await
```

Update both calls (lines 110 and 122 in current code).

#### 4.3 Update Error Handling for Token Cache Cleanup

When clearing invalid token cache (around line 119), need to determine the cache filename:

```rust
let token_cache_filename = if let Some(legacy) = &config.token_cache_filename {
    legacy.clone()
} else if let Some(email) = user_email {
    googleads::generate_token_cache_filename(email)
} else {
    "tokencache_default.json".to_string()
};

let token_cache_path = crate::config::config_file_path(&token_cache_filename)
    .expect("token cache path");
let _ = fs::remove_file(token_cache_path);
```

### 5. Update GoogleAdsAPIAccess Struct (Optional Enhancement)

Add user email to struct for tracking:

```rust
#[derive(Clone)]
pub struct GoogleAdsAPIAccess {
    pub channel: Channel,
    pub dev_token: MetadataValue<Ascii>,
    pub login_customer: MetadataValue<Ascii>,
    pub auth_token: Option<MetadataValue<Ascii>>,
    pub token: Option<AccessToken>,
    pub authenticator: Authenticator<<DefaultHyperClient as HyperClientBuilder>::Connector>,
    pub user_email: Option<String>,  // NEW: track which user is authenticated
}
```

## Example Usage

### Command Line

```bash
# Use specific user
mcc-gaql --user user@example.com -q some_query

# Uses config file 'user' field
mcc-gaql -q some_query

# Legacy: config file still has token_cache_filename
mcc-gaql -q some_query
```

### Config File Examples

#### New approach (recommended):
```toml
[test]
mcc_customerid = "1234567890"
user = "user@example.com"
customerids_filename = "customerids_test.txt"
queries_filename = "query_cookbook.toml"
```

#### Legacy approach (still supported):
```toml
[test]
mcc_customerid = "1234567890"
token_cache_filename = "tokencache.json"
customerids_filename = "customerids_test.txt"
queries_filename = "query_cookbook.toml"
```

## Backward Compatibility

- Existing configs with `token_cache_filename` will continue to work
- If both `user` and `token_cache_filename` are present, `token_cache_filename` takes precedence (legacy wins)
- No breaking changes to existing deployments

## Token Cache File Locations

Token cache files will be stored in the standard config directory:
- macOS: `~/Library/Application Support/mcc-gaql/tokencache_{user}.json`
- Linux: `~/.config/mcc-gaql/tokencache_{user}.json`
- Windows: `C:\Users\{username}\AppData\Roaming\mcc-gaql\tokencache_{user}.json`

## Environment Variable Override

Users can still override via environment variable:
```bash
export MCC_GAQL_USER="user@example.com"
```

## Testing Considerations

1. Test with `--user` argument
2. Test with `user` in config file
3. Test legacy `token_cache_filename` still works
4. Test default behavior (no user, no token_cache_filename)
5. Test email sanitization with special characters
6. Test multiple users can have separate token caches
7. Test token cache cleanup on auth failure

## Future Enhancements

1. Add `--list-users` command to show cached authentications
2. Add `--clear-cache` command to remove specific user's token
3. Store metadata file mapping users to their cache files and last used timestamp
4. Auto-detect user from existing token cache using Google's userinfo API

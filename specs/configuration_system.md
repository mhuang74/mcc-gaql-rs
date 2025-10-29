# Configuration System Specification

**Status**: Active
**Created**: 2025-10-28
**Updated**: 2025-10-28

## Overview

The mcc-gaql configuration system supports multiple configuration sources with a clear precedence hierarchy. Configuration can be provided through TOML files, environment variables, and command-line arguments.

## Configuration Sources (Precedence Order)

Configuration is resolved in the following order (highest to lowest precedence):

1. **Command-line arguments** (highest priority)
2. **Environment variables** (with `MCC_GAQL_` prefix)
3. **TOML profile configuration**
4. **Runtime defaults** (lowest priority)

## Configuration Structures

### MyConfig (Serializable)

Represents configuration stored in TOML files.

**Location**: `src/config.rs:42-54`

```rust
#[derive(Deserialize, Serialize, Debug)]
pub struct MyConfig {
    pub mcc_customerid: String,
    pub user: Option<String>,
    pub token_cache_filename: Option<String>,
    pub customerids_filename: Option<String>,
    pub queries_filename: Option<String>,
}
```

#### Fields

##### mcc_customerid
- **Type**: `String` (required)
- **Purpose**: Google Ads MCC (Manager) account ID
- **Format**: Digits only, no dashes (e.g., `1234567890`)
- **TOML Key**: `mcc_customerid`
- **Env Var**: `MCC_GAQL_MCC_CUSTOMERID`
- **CLI Override**: `--mcc <ID>`
- **Example**: `"1234567890"`

##### user
- **Type**: `Option<String>` (optional but recommended)
- **Purpose**: User email for OAuth2 authentication
- **Format**: Valid email address
- **TOML Key**: `user`
- **Env Var**: `MCC_GAQL_USER`
- **CLI Override**: `--user <EMAIL>`
- **Example**: `"user@example.com"`
- **Notes**:
  - Required for OAuth2 authentication flow
  - Used to generate user-specific token cache filename
  - ⚠️ Currently not prompted by setup wizard (known issue)

##### token_cache_filename
- **Type**: `Option<String>` (optional)
- **Purpose**: Custom location for OAuth2 token cache
- **Format**: Filename or absolute path
- **TOML Key**: `token_cache_filename`
- **Env Var**: `MCC_GAQL_TOKEN_CACHE_FILENAME`
- **Default Behavior**: Auto-generated from user email if not specified
- **Auto-generated Format**: `tokencache_{sanitized_email}.json`
- **Example**: `"tokencache_myuser.json"`
- **Notes**:
  - If `None`, runtime generates from user email
  - See `oauth_token_caching_without_config_file.md` for details

##### customerids_filename
- **Type**: `Option<String>` (optional)
- **Purpose**: File containing list of customer account IDs
- **Format**: Filename or absolute path to text file
- **TOML Key**: `customerids_filename`
- **Env Var**: `MCC_GAQL_CUSTOMERIDS_FILENAME`
- **Default**: `"customerids.txt"`
- **File Format**: One customer ID per line (digits only)
- **Example**: `"customerids.txt"`
- **Notes**:
  - Used by `--all-linked-child-accounts` flag
  - Can be generated via `--list-child-accounts`

##### queries_filename
- **Type**: `Option<String>` (optional)
- **Purpose**: TOML file containing saved GAQL queries
- **Format**: Filename or absolute path to TOML file
- **TOML Key**: `queries_filename`
- **Env Var**: `MCC_GAQL_QUERIES_FILENAME`
- **Default**: `"query_cookbook.toml"`
- **Example**: `"query_cookbook.toml"`
- **Notes**:
  - Used by `--stored-query <NAME>` flag
  - Query cookbook format documented separately

### ResolvedConfig (Runtime)

Represents the final resolved configuration used at runtime.

**Location**: `src/config.rs:58-64`

```rust
#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub mcc_customer_id: String,
    pub user_email: Option<String>,
    pub token_cache_filename: String,
    pub queries_filename: Option<String>,
    pub customerids_filename: Option<String>,
}
```

#### Key Differences from MyConfig

1. **Not Serializable**: Missing `Serialize`/`Deserialize` derives
2. **token_cache_filename**: Changed from `Option<String>` to `String` (always resolved)
3. **Field Naming**: `mcc_customer_id` vs `mcc_customerid`, `user_email` vs `user`

#### Resolution Logic

**Location**: `src/config.rs:142-185`

```rust
pub fn resolve_config(args: &Args) -> Result<ResolvedConfig> {
    // 1. Load MyConfig from TOML (if config exists)
    let my_config = match MyConfig::load(&args.profile) {
        Ok(cfg) => Some(cfg),
        Err(_) => None,
    };

    // 2. Resolve mcc_customer_id: CLI > config > error
    let mcc_customer_id = args.mcc.clone()
        .or_else(|| my_config.as_ref().map(|c| c.mcc_customerid.clone()))
        .ok_or_else(|| anyhow!("MCC customer ID required"))?;

    // 3. Resolve user_email: CLI > config
    let user_email = args.user.clone()
        .or_else(|| my_config.as_ref().and_then(|c| c.user.clone()));

    // 4. Resolve token_cache_filename
    let token_cache_filename = if let Some(config) = &my_config {
        config.token_cache_filename.clone()
            .or_else(|| user_email.as_ref().map(|email| {
                format!("tokencache_{}.json", sanitize_email(email))
            }))
            .unwrap_or_else(|| "tokencache.json".to_string())
    } else {
        user_email.as_ref()
            .map(|email| format!("tokencache_{}.json", sanitize_email(email)))
            .unwrap_or_else(|| "tokencache.json".to_string())
    };

    // 5. Resolve queries_filename: config or default
    let queries_filename = my_config.as_ref()
        .and_then(|c| c.queries_filename.clone());

    // 6. Resolve customerids_filename: config or default
    let customerids_filename = my_config.as_ref()
        .and_then(|c| c.customerids_filename.clone());

    Ok(ResolvedConfig {
        mcc_customer_id,
        user_email,
        token_cache_filename,
        queries_filename,
        customerids_filename,
    })
}
```

## TOML Configuration Format

### File Location

**Default Path**: `~/.config/mcc-gaql/config.toml`

Platform-specific locations:
- **Linux**: `~/.config/mcc-gaql/config.toml`
- **macOS**: `~/Library/Application Support/mcc-gaql/config.toml`
- **Windows**: `C:\Users\<username>\AppData\Roaming\mcc-gaql\config.toml`

### File Structure

The config file uses TOML table syntax with named profiles:

```toml
[profile_name]
mcc_customerid = "1234567890"
user = "user@example.com"
token_cache_filename = "tokencache_user.json"
customerids_filename = "customerids.txt"
queries_filename = "query_cookbook.toml"

[another_profile]
mcc_customerid = "9876543210"
user = "another@example.com"
```

### Optional Field Behavior

Fields with `None` values are **omitted** from the TOML file (not serialized as `null`):

```toml
[minimal_profile]
mcc_customerid = "1234567890"
# user, token_cache_filename, etc. are omitted if None
```

### Loading Implementation

Uses `figment` crate for layered configuration:

**Location**: `src/config.rs:193-248`

```rust
let figment = Figment::new()
    .merge(Toml::file(config_path))  // Load TOML file
    .merge(Env::prefixed("MCC_GAQL_"))  // Overlay env vars
    .select(profile);  // Select specific profile

let config: MyConfig = figment.extract()?;
```

## Environment Variables

All config fields can be overridden with environment variables using the `MCC_GAQL_` prefix.

### Variable Naming Convention

TOML key `foo_bar` → Environment variable `MCC_GAQL_FOO_BAR`

### Available Variables

| Environment Variable | TOML Key | Type | Example |
|---------------------|----------|------|---------|
| `MCC_GAQL_MCC_CUSTOMERID` | `mcc_customerid` | String | `"1234567890"` |
| `MCC_GAQL_USER` | `user` | String | `"user@example.com"` |
| `MCC_GAQL_TOKEN_CACHE_FILENAME` | `token_cache_filename` | String | `"tokencache.json"` |
| `MCC_GAQL_CUSTOMERIDS_FILENAME` | `customerids_filename` | String | `"customerids.txt"` |
| `MCC_GAQL_QUERIES_FILENAME` | `queries_filename` | String | `"queries.toml"` |

### Usage Example

```bash
export MCC_GAQL_MCC_CUSTOMERID="1234567890"
export MCC_GAQL_USER="user@example.com"
mcc-gaql -q "SELECT campaign.id, campaign.name FROM campaign"
```

## Command-Line Overrides

CLI arguments have the highest precedence and override all other config sources.

### Available CLI Overrides

| CLI Argument | Config Field | Type | Example |
|-------------|--------------|------|---------|
| `--mcc <ID>` | `mcc_customerid` | String | `--mcc 1234567890` |
| `--user <EMAIL>` | `user` | String | `--user user@example.com` |
| `--profile <NAME>` | N/A (profile selector) | String | `--profile production` |

### Usage Example

```bash
# Override MCC ID for this run only
mcc-gaql --mcc 1234567890 --user user@example.com -q "SELECT ..."
```

## Configuration Modes

### Mode 1: Profile-Based (Recommended)

Uses TOML config file with named profiles.

```bash
# Use default profile (first in file)
mcc-gaql -q "SELECT ..."

# Use specific profile
mcc-gaql --profile production -q "SELECT ..."
```

### Mode 2: Config-Free

No config file required, all settings via CLI arguments.

```bash
mcc-gaql --mcc 1234567890 --user user@example.com -q "SELECT ..."
```

See `no_default_profile.md` for detailed config-free documentation.

### Mode 3: Hybrid

Combine profile with CLI overrides.

```bash
# Use staging profile but override MCC ID
mcc-gaql --profile staging --mcc 9999999999 -q "SELECT ..."
```

## Profile Selection Logic

**Location**: `src/config.rs:193-248`

1. If `--profile <NAME>` specified → use that profile
2. If no profile specified → use first profile in config file
3. If config file doesn't exist → require CLI arguments

### Profile Auto-Selection

The first profile in the TOML file is used as the default:

```toml
[default]  # ← Used when no --profile specified
mcc_customerid = "1111111111"

[staging]  # Only used with --profile staging
mcc_customerid = "2222222222"
```

## Token Cache Management

The token cache stores OAuth2 refresh tokens for reuse.

### Token Cache Location Resolution

**Priority**:
1. `token_cache_filename` in config (if specified)
2. Auto-generated from `user` email: `tokencache_{sanitized_email}.json`
3. Fallback: `tokencache.json`

### Email Sanitization

Converts email to safe filename:
```rust
fn sanitize_email(email: &str) -> String {
    email.replace('@', "_at_").replace('.', "_")
}
```

Example: `user@example.com` → `user_at_example_com`

### Token Cache Behavior

- **Per-User Tokens**: Each user email gets separate token cache
- **Profile Independence**: Multiple profiles can share same user's tokens
- **No Config Required**: Token cache works in config-free mode

See `oauth_token_caching_without_config_file.md` for full details.

## Configuration Validation

### Required Fields

- **mcc_customer_id**: Must be provided via CLI, config, or env var
  - Error: `"MCC customer ID required. Provide via --mcc or config file"`

### Optional but Recommended

- **user**: Required for OAuth2 authentication
  - Warning: Missing user field may prevent authentication

### Format Validation

**MCC Customer ID**:
- Must be digits only (no dashes)
- Validated in setup wizard
- Not validated at load time (accepts any string)

**User Email**:
- No validation at load time
- Should be valid email format for OAuth2

## Error Handling

### Common Configuration Errors

#### Config File Not Found
```
Error: Config file not found at /Users/username/.config/mcc-gaql/config.toml
Hint: Run 'mcc-gaql --setup' to create a config file
```

#### Profile Not Found
```
Error: Profile 'nonexistent' not found in config file
Available profiles: default, staging, production
```

#### Missing MCC Customer ID
```
Error: MCC customer ID required. Provide via --mcc or config file
```

#### Invalid TOML Syntax
```
Error: Failed to parse config file
Caused by: invalid TOML at line 5: expected '='
```

## Testing Configuration

### Verify Current Configuration

```bash
# Show current config (planned feature)
mcc-gaql --show-config

# Test authentication with config
mcc-gaql --list-child-accounts
```

### Test Different Profiles

```bash
mcc-gaql --profile production --list-child-accounts
mcc-gaql --profile staging --list-child-accounts
```

### Test Config-Free Mode

```bash
mcc-gaql --mcc 1234567890 --user user@example.com --list-child-accounts
```

## Migration Guide

### From Legacy Token Cache

Old behavior:
- Used `token_cache_filename` from config
- Required manual specification

New behavior:
- Auto-generates from user email
- Per-user token isolation

**Migration**: No action required, existing token_cache_filename continues to work.

### Adding User Field to Existing Profiles

If profile missing `user` field:

```toml
[myprofile]
mcc_customerid = "1234567890"
# Add this line:
user = "your-email@example.com"
```

Or use CLI override:
```bash
mcc-gaql --profile myprofile --user your-email@example.com
```

## Best Practices

### Profile Naming

- Use descriptive names: `production`, `staging`, `dev`
- Avoid generic names like `myprofile`, `myprofile_2` (wizard-generated)
- Consider team conventions for shared configs

### Security

- **Never commit config files with credentials** to version control
- Use environment variables for CI/CD: `MCC_GAQL_*`
- Keep token cache files private (contain OAuth2 refresh tokens)
- Add to `.gitignore`:
  ```
  config.toml
  tokencache*.json
  customerids.txt
  ```

### Organization

- **Project-local configs**: Place `config.toml` in project directory
- **Global configs**: Use `~/.config/mcc-gaql/config.toml`
- **Team configs**: Share example configs as `config.example.toml`

### Token Cache

- **Recommended**: Let runtime auto-generate token_cache_filename
- **Don't specify** `token_cache_filename` unless you need custom location
- **One cache per user**: Multiple profiles can share same user's tokens

## Future Enhancements

### Planned Features

1. **Config Validation Command**
   ```bash
   mcc-gaql --validate-config
   ```

2. **Config Display Command**
   ```bash
   mcc-gaql --show-config
   mcc-gaql --show-config --profile staging
   ```

3. **Profile Management**
   ```bash
   mcc-gaql --list-profiles
   mcc-gaql --delete-profile staging
   mcc-gaql --rename-profile old new
   ```

4. **Export/Import**
   ```bash
   mcc-gaql --export-config > config.json
   mcc-gaql --import-config config.json
   ```

### Potential Improvements

- JSON config format support
- Project-local config file discovery (`.mcc-gaql.toml`)
- Config file encryption for sensitive data
- Config schema validation
- Config migration tools for breaking changes

## Related Specifications

- `setup_wizard.md` - Interactive configuration creation
- `no_default_profile.md` - Config-free mode details
- `oauth_token_caching_without_config_file.md` - Token cache behavior

## Implementation Files

- `src/config.rs` - Config structures and loading logic
- `src/args.rs` - CLI argument definitions
- `src/setup.rs` - Setup wizard implementation
- `src/main.rs` - Config resolution and usage

## Dependencies

- `figment` - Layered configuration management
- `serde` - Serialization framework
- `toml` - TOML parsing and generation
- `dirs` - Cross-platform directory paths
- `anyhow` - Error handling

## Changelog

### 2025-10-28
- Initial specification created
- Documented all config fields and resolution logic
- Identified gaps in wizard and serialization
- Planned improvements for token cache handling

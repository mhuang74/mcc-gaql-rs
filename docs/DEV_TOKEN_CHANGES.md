# Developer Token Configuration - Implementation Changes

## Summary

Implemented flexible, required configuration for Google Ads Developer Token. **No fallback token** - users must provide their own token via one of three methods.

## Key Changes

### 1. Removed Hardcoded Fallback Token

**Before:**
```rust
const DEV_TOKEN: &str = "EBkkx-znu2cZcEY7e74smg";  // Public fallback
```

**After:**
```rust
const EMBEDDED_DEV_TOKEN: Option<&str> = option_env!("MCC_GAQL_DEV_TOKEN");
// No fallback - token is required
```

### 2. Updated Token Resolution (`src/googleads.rs`)

**New Function:**
```rust
fn get_dev_token(config_token: Option<&str>) -> Result<String>
```

**Priority Order (No Fallback):**
1. Config file parameter (from profile)
2. Runtime environment variable `MCC_GAQL_DEV_TOKEN`
3. Compile-time embedded token
4. ❌ **Error if none found** (no fallback)

**Error Message:**
```
Google Ads Developer Token required but not found. Provide via:
  1. Config file: Add 'dev_token = "YOUR_TOKEN"' to your profile
  2. Runtime env: export MCC_GAQL_DEV_TOKEN="YOUR_TOKEN"
  3. Build time: MCC_GAQL_DEV_TOKEN="YOUR_TOKEN" cargo build

  Get your developer token at:
  https://developers.google.com/google-ads/api/docs/get-started/dev-token
```

### 3. Configuration Support

**MyConfig** (`src/config.rs`):
```rust
pub struct MyConfig {
    // ... other fields ...
    pub dev_token: Option<String>,  // NEW FIELD
}
```

**ResolvedConfig** (`src/config.rs`):
```rust
pub struct ResolvedConfig {
    // ... other fields ...
    pub dev_token: Option<String>,  // NEW FIELD
}
```

### 4. Updated Function Signatures

**get_api_access** (`src/googleads.rs`):
```rust
pub async fn get_api_access(
    mcc_customer_id: &str,
    token_cache_filename: &str,
    user_email: Option<&str>,
    dev_token: Option<&str>,  // NEW PARAMETER
) -> Result<GoogleAdsAPIAccess>
```

## Configuration Methods

### Method 1: Config File (Recommended)

```toml
[myprofile]
mcc_id = "123-456-7890"
user_email = "user@example.com"
dev_token = "YOUR_DEVELOPER_TOKEN"  # Required if not set elsewhere
```

### Method 2: Runtime Environment Variable

```bash
export MCC_GAQL_DEV_TOKEN="YOUR_DEVELOPER_TOKEN"
mcc-gaql --profile myprofile "SELECT campaign.name FROM campaign"
```

### Method 3: Compile-time Embedding

```bash
MCC_GAQL_DEV_TOKEN="YOUR_TOKEN" cargo build --release
# Token is compiled into the binary
```

## Files Modified

1. **src/googleads.rs**
   - Removed `FALLBACK_DEV_TOKEN` constant
   - Updated `get_dev_token()` to return `Result<String>`
   - Added error handling for missing token
   - Updated documentation comments

2. **src/config.rs**
   - Added `dev_token: Option<String>` to `MyConfig`
   - Added `dev_token: Option<String>` to `ResolvedConfig`
   - Updated `from_args_and_config()` implementation
   - Updated all test fixtures (4 tests)

3. **src/main.rs**
   - Updated both `get_api_access()` calls to pass `dev_token`

4. **src/setup.rs**
   - Added `dev_token: None` to config creation
   - Updated test fixtures (2 tests)

5. **tests/config_tests.rs**
   - Updated all `MyConfig` initializations (6 instances)
   - Updated all `ResolvedConfig` initializations (2 instances)

6. **README.md**
   - Added "Developer Token Configuration" section
   - Marked as **Required**
   - Documented three configuration methods
   - Removed fallback token mention

7. **docs/DEV_TOKEN_CONFIGURATION.md**
   - Removed "Fallback Token" section
   - Updated priority order (no fallback)
   - Added "Developer Token required but not found" troubleshooting
   - Removed all references to public/shared token

## Testing

✅ **All Tests Pass:**
- 21 unit tests in `src/config.rs`
- 18 integration tests in `tests/config_tests.rs`
- Builds successfully with and without embedded token
- Clippy checks pass

## Backward Compatibility

⚠️ **Breaking Change for Users Without Token**

**Before:** Tool would run with fallback public token
**After:** Tool requires explicit token configuration

**Migration:**
Users must now provide a token via one of three methods. The tool will display a clear error message explaining how to provide the token.

## Security Improvements

✅ **Better Security Posture:**
1. No public/shared token in source code
2. Forces users to use their own tokens
3. Prevents accidental production use of shared token
4. Clearer token ownership and responsibility

## Error Handling

**Graceful Error Message:**
```
Error: Google Ads Developer Token required but not found. Provide via:
  1. Config file: Add 'dev_token = "YOUR_TOKEN"' to your profile
  2. Runtime env: export MCC_GAQL_DEV_TOKEN="YOUR_TOKEN"
  3. Build time: MCC_GAQL_DEV_TOKEN="YOUR_TOKEN" cargo build

  Get your developer token at:
  https://developers.google.com/google-ads/api/docs/get-started/dev-token
```

## Documentation

📚 **Comprehensive Documentation:**
- `README.md` - Quick start with dev token
- `docs/DEV_TOKEN_CONFIGURATION.md` - Complete guide with:
  - Configuration methods
  - Security considerations
  - Troubleshooting
  - Best practices
  - Examples for each method

## Example Usage

### First-time Setup

```bash
# 1. Get your token from Google Ads
# Visit: https://developers.google.com/google-ads/api/docs/get-started/dev-token

# 2. Configure it (choose one method):

# Method A: Config file
cat >> ~/.config/mcc-gaql/config.toml <<EOF
[myprofile]
mcc_id = "123-456-7890"
user_email = "user@example.com"
dev_token = "YOUR_DEV_TOKEN_HERE"
EOF

# Method B: Environment variable
export MCC_GAQL_DEV_TOKEN="YOUR_DEV_TOKEN_HERE"

# Method C: Compile-time
MCC_GAQL_DEV_TOKEN="YOUR_TOKEN" cargo build --release

# 3. Use the tool
mcc-gaql --profile myprofile "SELECT campaign.name FROM campaign"
```

## Summary

✅ **Implemented:** Flexible token configuration with 3 methods
✅ **Removed:** Hardcoded fallback token
✅ **Required:** Users must provide their own token
✅ **Documented:** Comprehensive guides and error messages
✅ **Tested:** All tests pass
✅ **Secure:** No public tokens in source code

**Result:** Professional, secure tool that requires proper authentication configuration.

# Developer Token Configuration

## Overview

The Google Ads Developer Token is **required** to access the Google Ads API. This document explains how to configure it for `mcc-gaql`.

**Important:** A developer token must be provided via one of the methods below. The tool will not run without a valid token.

## Priority Order

The tool checks for the developer token in the following priority order:

1. **Config File** (highest priority) - Per-profile configuration
2. **Runtime Environment Variable** - `MCC_GAQL_DEV_TOKEN`
3. **Compile-time Embedded** - Set during `cargo build`

If no token is found from any source, the tool will exit with an error message.

## Configuration Methods

### 1. Config File (Recommended for Production)

Add `dev_token` to your profile in `config.toml`:

```toml
[myprofile]
mcc_id = "123-456-7890"
user_email = "user@example.com"
dev_token = "YOUR_DEVELOPER_TOKEN_HERE"
```

**Advantages:**
- Different tokens for different profiles
- Persistent configuration
- Easy to manage multiple accounts

**Use when:**
- You have multiple Google Ads accounts with different tokens
- You want persistent configuration
- You're working with a team

### 2. Runtime Environment Variable

Set the environment variable before running:

```bash
export MCC_GAQL_DEV_TOKEN="YOUR_DEVELOPER_TOKEN_HERE"
mcc-gaql --profile myprofile "SELECT campaign.name FROM campaign"
```

Or inline:

```bash
MCC_GAQL_DEV_TOKEN="YOUR_TOKEN" mcc-gaql --profile myprofile "SELECT ..."
```

**Advantages:**
- Temporary override
- Doesn't modify config files
- Good for testing

**Use when:**
- Testing different tokens
- Temporarily overriding config
- CI/CD environments

### 3. Compile-time Embedding

Embed the token at build time:

```bash
MCC_GAQL_DEV_TOKEN="YOUR_TOKEN" cargo build --release
```

The token will be compiled into the binary.

**Advantages:**
- No external configuration needed
- Single standalone binary

**Use when:**
- Distributing binaries to users
- All users share the same Google Ads application
- Simplifying deployment

**Security Note:** The embedded token is visible to anyone who can inspect the binary (e.g., via `strings`). Only embed tokens for applications where you control distribution or where the token is already considered semi-public.

## Getting Your Developer Token

1. Sign up for a Google Ads API developer token:
   https://developers.google.com/google-ads/api/docs/get-started/dev-token

2. Create a Google Cloud project (if you don't have one)

3. Enable the Google Ads API for your project

4. Apply for API access through your Google Ads Manager Account

5. Once approved, your token will appear in the Google Ads UI under:
   **Tools & Settings → Setup → API Center**

## Examples

### Example 1: Per-Profile Token

Different tokens for different clients:

```toml
# config.toml
[client_a]
mcc_id = "111-222-3333"
user_email = "clienta@company.com"
dev_token = "CLIENT_A_DEV_TOKEN"

[client_b]
mcc_id = "444-555-6666"
user_email = "clientb@company.com"
dev_token = "CLIENT_B_DEV_TOKEN"
```

Usage:

```bash
mcc-gaql --profile client_a "SELECT campaign.name FROM campaign"
mcc-gaql --profile client_b "SELECT campaign.name FROM campaign"
```

### Example 2: Environment Variable Override

Testing with a different token:

```bash
# Normal usage with config token
mcc-gaql --profile myprofile "SELECT ..."

# Override with test token
MCC_GAQL_DEV_TOKEN="TEST_TOKEN" mcc-gaql --profile myprofile "SELECT ..."
```

### Example 3: Embedded Token for Distribution

Building a binary for distribution:

```bash
# Build with embedded token
MCC_GAQL_DEV_TOKEN="YOUR_PRODUCTION_TOKEN" cargo build --release

# Distribute the binary
cp target/release/mcc-gaql /usr/local/bin/

# Users can run without configuration
mcc-gaql --user-email user@example.com --customer-id 123-456-7890 "SELECT ..."
```

## Verifying Your Configuration

To see which token source is being used, enable debug logging:

```bash
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" mcc-gaql --profile myprofile "SELECT ..."
```

Look for one of these messages:

- `"Using developer token from config"` - Config file token
- `"Using developer token from runtime environment variable"` - `MCC_GAQL_DEV_TOKEN` env var
- `"Using developer token embedded at compile time"` - Compiled-in token

If no token is found, you'll see an error message explaining how to provide one.

## Security Considerations

### Config File Token
- **Risk:** Low to Medium
- **Mitigation:**
  - File permissions (`chmod 600 config.toml`)
  - Don't commit config files to version control
  - Use `.gitignore` to exclude config directory

### Environment Variable Token
- **Risk:** Low
- **Mitigation:**
  - Variables are process-scoped
  - Not persistent unless added to shell profile
  - Good for temporary/testing use

### Embedded Token
- **Risk:** Medium
- **Mitigation:**
  - Token can be extracted from binary with `strings` command
  - Only embed if you control binary distribution
  - Consider this token "semi-public" once embedded
  - Use separate tokens for embedded vs. config-based deployments

## Troubleshooting

### "Developer token is not approved"

Your token hasn't been approved by Google yet. This can take a few days. Meanwhile, you can:
- Use test mode (if available for your use case)
- Wait for approval (typically 1-2 business days)
- Contact Google Ads API support

### "Developer Token required but not found"

The tool cannot find a developer token. Provide one via:
1. Add to config file: `dev_token = "YOUR_TOKEN"`
2. Set environment variable: `export MCC_GAQL_DEV_TOKEN="YOUR_TOKEN"`
3. Embed at build time: `MCC_GAQL_DEV_TOKEN="YOUR_TOKEN" cargo build`

### "Rate limit exceeded"

You're hitting API rate limits. Solutions:
- Upgrade your API access level with Google
- Reduce query frequency
- Use `--keep-going` flag to continue on errors
- Spread queries across multiple accounts if possible

### "Invalid developer token"

Token format or value is incorrect:
- Check for extra whitespace
- Verify token from Google Ads UI
- Ensure token is for the correct Google Ads account
- Try re-applying for API access

### "Which token am I using?"

Enable debug logging to see:

```bash
MCC_GAQL_LOG_LEVEL="debug" mcc-gaql --profile myprofile "SELECT ..."
```

## Best Practices

1. **Development**: Use config file or environment variable for flexibility
2. **Testing**: Use environment variable for easy token switching
3. **Production**: Use config file with proper file permissions
4. **Distribution**: Use compile-time embedding for standalone binaries
5. **Multi-tenant**: Use per-profile tokens in config file
6. **CI/CD**: Use environment variables or secrets management
7. **Never**: Don't commit tokens to version control
8. **Rotate**: Periodically rotate developer tokens
9. **Monitor**: Set up alerts for API quota usage
10. **Document**: Keep track of which token belongs to which account

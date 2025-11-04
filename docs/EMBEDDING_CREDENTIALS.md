# Embedding OAuth2 Credentials in the Binary

## Overview

Your `clientsecret.json` file can now be safely embedded into the binary at compile time, eliminating the need to distribute it separately. This makes tool distribution much more convenient.

## Security Considerations

**This is safe for OAuth2 "Installed/Desktop" applications** because:

1. **The `client_secret` is not highly confidential** - Google's OAuth2 documentation explicitly states that native/desktop apps cannot keep this secret
2. **Real security comes from**:
   - The OAuth2 authorization flow requiring user consent
   - User-specific tokens stored in `tokencache_*.json` files (never embedded)
3. **Common practice** - Many open-source desktop tools embed OAuth2 client credentials in their binaries or source code

## How It Works

### Automatic Detection

When you build the project, the build script (`build.rs`) automatically:
1. Looks for `clientsecret.json` in the project root
2. If found, embeds it as an environment variable at compile time
3. The runtime code tries the embedded version first, then falls back to file-based loading

### Build Messages

**Without clientsecret.json:**
```
warning: mcc-gaql@0.12.2: clientsecret.json not found in project root
warning: mcc-gaql@0.12.2: Binary will require clientsecret.json in config directory at runtime
warning: mcc-gaql@0.12.2: To embed credentials: place clientsecret.json in project root before building
```

**With clientsecret.json:**
```
warning: mcc-gaql@0.12.2: Found clientsecret.json - embedding OAuth2 credentials into binary
```

## Usage Examples

### Building with Embedded Credentials

```bash
# Place your credentials file in project root
cp ~/Downloads/clientsecret.json ./clientsecret.json

# Build the project
cargo build --release

# The binary now contains the credentials
# No need to place clientsecret.json on target machines!
./target/release/mcc-gaql --version
```

### Building without Embedded Credentials

```bash
# Simply build without placing clientsecret.json in root
cargo build --release

# The binary will look for clientsecret.json at runtime in:
#   macOS: ~/Library/Application Support/mcc-gaql/clientsecret.json
#   Linux: ~/.config/mcc-gaql/clientsecret.json
#   Windows: %APPDATA%/mcc-gaql/clientsecret.json
```

### Force External Loading (Feature Flag)

If you want to ensure credentials are NEVER embedded (even if the file exists):

```bash
cargo build --release --features external_client_secret
```

## Runtime Behavior

The code follows this priority order:

1. **Embedded credentials** (if present at compile time) - Used first
2. **File-based credentials** (fallback) - Loaded from config directory if no embedded version
3. **Error** - If neither is available

You can check which method is being used by enabling debug logging:

```bash
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" ./target/release/mcc-gaql --version
```

Look for:
- `"Using embedded client secret"` - Using compiled-in credentials
- `"No embedded client secret found, loading from file"` - Using runtime file

## Setup Wizard Integration

The setup wizard (`mcc-gaql --setup`) automatically detects if credentials are embedded:

**With embedded credentials:**
```
Next steps:
  1. OAuth2 credentials are embedded in this binary (no clientsecret.json needed)
```

**Without embedded credentials:**
```
Next steps:
  1. Place your OAuth2 credentials in: "~/.config/mcc-gaql/clientsecret.json"
     (Or rebuild with credentials embedded - see README for details)
```

## Example clientsecret.json Structure

See `clientsecret.json.example` for the expected format:

```json
{
  "installed": {
    "client_id": "YOUR_CLIENT_ID.apps.googleusercontent.com",
    "project_id": "your-project-id",
    "auth_uri": "https://accounts.google.com/o/oauth2/auth",
    "token_uri": "https://oauth2.googleapis.com/token",
    "auth_provider_x509_cert_url": "https://www.googleapis.com/oauth2/v1/certs",
    "client_secret": "YOUR_CLIENT_SECRET",
    "redirect_uris": ["http://localhost"]
  }
}
```

## Distribution Strategies

### For Public Distribution
- **Embed your credentials** to create a standalone binary
- Users don't need to obtain their own OAuth2 credentials
- All users authenticate with the same application (your OAuth2 app)

### For Private/Enterprise Use
- **Don't embed credentials** - let each deployment use their own
- Build with `--features external_client_secret` to enforce this
- Each organization uses their own Google Cloud project credentials

### For Development
- **Keep credentials in file** for easier testing with different credentials
- Add `clientsecret.json` to build directory only when making release builds

## Technical Details

### Files Modified

1. **src/googleads.rs** - Added embedded credential support with fallback
2. **src/setup.rs** - Updated setup wizard to detect embedded credentials
3. **build.rs** - New build script to handle embedding
4. **Cargo.toml** - Added `external_client_secret` feature flag
5. **clientsecret.json.example** - Example credential file format

### Feature Flags

- `default` - Supports both embedded and file-based credentials
- `external_client_secret` - Forces file-based loading, disables embedding

### Security Notes

- The `.gitignore` already excludes `*.json*`, preventing accidental commits
- User tokens (`tokencache_*.json`) are NEVER embedded, only app credentials
- This is standard practice for OAuth2 desktop/installed applications

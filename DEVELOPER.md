# Developer Documentation

This document contains technical information for developers working on mcc-gaql-rs.

## Technical Overview

mcc-gaql-rs is a command-line tool built in Rust that executes Google Ads Query Language (GAQL) queries across Manager (MCC) child accounts. The tool provides:

- OAuth2 authentication for Google Ads API access
- Parallel query execution across multiple customer accounts
- Metric aggregation across customer accounts
- Multiple output formats (CSV, JSON, table)
- Profile-based configuration management
- Optional credential embedding for standalone binaries
- Natural language to GAQL query conversion (LLM integration)

## Project Structure

```
mcc-gaql-rs/
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Core library exports
│   ├── args.rs              # CLI argument parsing (clap)
│   ├── config.rs            # Configuration management (figment/toml)
│   ├── googleads.rs         # Google Ads API integration
│   ├── setup.rs             # Interactive setup wizard (dialoguer)
│   ├── util.rs              # Utility functions
│   └── prompt2gaql.rs       # LLM-based natural language query conversion
├── tests/
│   └── config_tests.rs      # Configuration tests
├── build.rs                 # Build script for credential embedding
├── Cargo.toml               # Rust package manifest
└── .github/workflows/       # CI/CD workflows
    ├── rust.yml             # CI tests
    ├── code-review.yml      # Automated code review
    └── release.yml          # Release builds with embedded credentials
```

## Key Libraries and Dependencies

### Core Dependencies

| Library | Purpose |
|---------|---------|
| [googleads-rs](https://github.com/mhuang74/googleads-rs) | Google Ads API v22 client (gRPC) |
| [yup-oauth2](https://docs.rs/yup-oauth2/) | OAuth2 authentication flow |
| [tonic](https://docs.rs/tonic/) | gRPC client framework |
| [clap](https://docs.rs/clap/) | CLI argument parsing |
| [figment](https://docs.rs/figment/) | Configuration management with TOML/env support |
| [polars](https://docs.rs/polars/) | DataFrame operations for query results |
| [tokio](https://docs.rs/tokio/) | Async runtime |
| [serde](https://docs.rs/serde/) | Serialization/deserialization |

### Additional Dependencies

| Library | Purpose |
|---------|---------|
| [dialoguer](https://docs.rs/dialoguer/) | Interactive setup wizard prompts |
| [flexi_logger](https://docs.rs/flexi_logger/) | Flexible logging configuration |
| [rig-core](https://docs.rs/rig-core/) | LLM integration for natural language queries |
| [rig-lancedb](https://docs.rs/rig-lancedb/) | LanceDB vector store for embedding cache persistence |
| [lancedb](https://docs.rs/lancedb/) | Vector database for fast embedding retrieval |
| [arrow-array](https://docs.rs/arrow-array/) | Apache Arrow array data structures (for LanceDB) |
| [arrow-schema](https://docs.rs/arrow-schema/) | Apache Arrow schema definitions (for LanceDB) |
| [cacache](https://docs.rs/cacache/) | OAuth token caching |
| [anyhow](https://docs.rs/anyhow/) | Error handling |

## Building from Source

### Prerequisites

1. **Rust toolchain** (1.70 or later)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Protocol Buffers compiler**
   ```bash
   # macOS
   brew install protobuf

   # Ubuntu/Debian
   sudo apt install protobuf-compiler

   # Arch Linux
   sudo pacman -S protobuf
   ```

3. **Google Ads Developer Token**
   - Obtain from: https://developers.google.com/google-ads/api/docs/get-started/dev-token

4. **OAuth2 Credentials** (Desktop/Installed application type)
   - Create at: https://console.cloud.google.com/apis/credentials
   - Download as `clientsecret.json`

### Developer Token Configuration

**Required:** A Google Ads Developer Token is required to use this tool.

Get your developer token at: https://developers.google.com/google-ads/api/docs/get-started/dev-token

The token can be configured via (in priority order):

1. **Config file**: Add `dev_token = "YOUR_TOKEN"` to your profile in `config.toml`
2. **Runtime environment variable**: `export MCC_GAQL_DEV_TOKEN="YOUR_TOKEN"`
3. **Compile-time embedding**: Set `MCC_GAQL_DEV_TOKEN` during build (see [Embedding Credentials](#embedding-credentials-in-binary) section)

### Basic Build

```bash
# Clone repository
git clone https://github.com/mhuang74/mcc-gaql-rs.git
cd mcc-gaql-rs

# Build in debug mode
cargo build

# Build in release mode (optimized)
cargo build --release

# Run from source
cargo run -- --version

# Run tests
cargo test

# Run with specific test
cargo test config_tests

# Check code without building
cargo check

# Run linter
cargo clippy --all-targets --all-features -- -D warnings
```

## Embedding Credentials in Binary

For easier distribution, you can embed OAuth2 credentials and Developer Token directly into the binary at compile time. This creates a standalone executable that end users can run without additional configuration.

### Environment Variables for Embedding

1. **`MCC_GAQL_EMBED_CLIENT_SECRET`** - OAuth2 client secret JSON content
2. **`MCC_GAQL_DEV_TOKEN`** - Google Ads Developer Token

### Local Development: Embedding Credentials

#### Option 1: Set environment variables from files

```bash
MCC_GAQL_EMBED_CLIENT_SECRET="$(cat clientsecret.json)" \
MCC_GAQL_DEV_TOKEN="your-dev-token-here" \
cargo build --release
```

#### Option 2: Export in shell

```bash
export MCC_GAQL_EMBED_CLIENT_SECRET="$(cat clientsecret.json)"
export MCC_GAQL_DEV_TOKEN="your-dev-token-here"
cargo build --release
```

#### Option 3: Use direnv with .env file

```bash
# Create .env file
echo "MCC_GAQL_EMBED_CLIENT_SECRET=$(cat clientsecret.json)" > .env
echo "MCC_GAQL_DEV_TOKEN=your-dev-token-here" >> .env

# If using direnv
direnv allow

cargo build --release
```

### Build Output

When embedding credentials, you'll see build warnings:

```
warning: Embedding OAuth2 credentials from MCC_GAQL_EMBED_CLIENT_SECRET environment variable
warning: Embedding Google Ads Developer Token from MCC_GAQL_DEV_TOKEN environment variable
```

If credentials are not provided:

```
warning: MCC_GAQL_EMBED_CLIENT_SECRET environment variable not set
warning: Binary will require clientsecret.json in config directory at runtime
warning: MCC_GAQL_DEV_TOKEN environment variable not set
warning: Binary will require dev_token in config file or MCC_GAQL_DEV_TOKEN env var at runtime
```

### Runtime Behavior

- **With embedded credentials**: Binary works standalone, no external files needed
- **Without embedded credentials**: Binary loads from config directory at runtime
- **Feature flag**: Build with `--features external_client_secret` to disable embedding and always load from file

```bash
# Build with feature flag to disable credential embedding
cargo build --release --features external_client_secret
```

## GitHub Actions / CI/CD

### Setting up GitHub Secrets

For automated builds with embedded credentials, configure these secrets in your GitHub repository settings:

1. Go to **Settings** → **Secrets and variables** → **Actions**
2. Add the following repository secrets:
   - `GOOGLE_ADS_CLIENT_SECRET` - JSON content of your `clientsecret.json`
   - `GOOGLE_ADS_DEV_TOKEN` - Your Google Ads Developer Token

### Release Workflow

The release workflow (`.github/workflows/release.yml`) is triggered on version tags:

```yaml
- name: Build release binary
  env:
    MCC_GAQL_EMBED_CLIENT_SECRET: ${{ secrets.GOOGLE_ADS_CLIENT_SECRET }}
    MCC_GAQL_DEV_TOKEN: ${{ secrets.GOOGLE_ADS_DEV_TOKEN }}
  run: cargo build --release --target aarch64-apple-darwin
```

### Creating a Release

```bash
# Tag a new version
git tag v0.12.3
git push origin v0.12.3

# GitHub Actions will automatically:
# 1. Build with embedded credentials
# 2. Create release archive
# 3. Publish to GitHub Releases
```

## Development Workflow

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test config_tests

# Run with output
cargo test -- --nocapture

# Run with debug logging
MCC_GAQL_LOG_LEVEL=debug cargo test
```

### Code Quality Checks

```bash
# Format code
cargo fmt

# Run clippy lints
cargo clippy --all-targets --all-features -- -D warnings

# Check for security vulnerabilities
cargo audit
```

### Local Development with Credentials

For local development, create a test configuration:

```bash
# Run setup wizard
cargo run -- --setup

# Or manually create config
mkdir -p "$HOME/Library/Application Support/mcc-gaql"
cat > "$HOME/Library/Application Support/mcc-gaql/config.toml" <<EOF
[default]
user_email = "dev@example.com"
dev_token = "YOUR_DEV_TOKEN"

[dev_profile]
customer_id = "123-456-7890"
EOF
```

### Testing with Debug Logging

```bash
# Enable debug logging for the application
MCC_GAQL_LOG_LEVEL="info,mcc_gaql=debug" cargo run -- --profile dev_profile "SELECT campaign.name FROM campaign"

# Available log levels: error, warn, info, debug, trace
MCC_GAQL_LOG_LEVEL="debug" cargo run -- --version
```

## Project Architecture

### Authentication Flow

1. Load OAuth2 credentials (embedded or from `clientsecret.json`)
2. Check for cached OAuth2 token in `tokencache_*.json`
3. If no valid token, initiate OAuth2 device flow
4. User authorizes via browser
5. Token cached for future use
6. Attach token to gRPC requests

### Query Execution Flow

1. Parse CLI arguments and load configuration
2. Resolve customer IDs (direct, MCC children, or from file)
3. Initialize Google Ads API client
4. Execute queries in parallel across customer accounts
5. Collect and aggregate results using Polars DataFrames
6. Format output (CSV, JSON, or table)
7. Write to file or stdout

### Configuration Precedence

Configuration values are resolved in this order (highest to lowest priority):

1. CLI arguments (`--customer-id`, `--user-email`, etc.)
2. Environment variables (`MCC_GAQL_DEV_TOKEN`)
3. Profile-specific config (`[myprofile]` section)
4. Default config (`[default]` section)
5. Embedded credentials (compile-time)

## Contributing

### Code Style

- Follow Rust standard formatting (`cargo fmt`)
- Keep functions focused and well-documented
- Use meaningful variable names
- Add tests for new features
- Update documentation for user-facing changes

### Pull Request Process

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Run tests and linting
   ```bash
   cargo test
   cargo clippy --all-targets --all-features -- -D warnings
   cargo fmt --check
   ```
5. Commit with clear messages
6. Push to your fork
7. Open a Pull Request

### Testing Checklist

Before submitting a PR:

- [ ] All tests pass (`cargo test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Documentation updated (if needed)
- [ ] Tested with real Google Ads account (if applicable)

## Troubleshooting

### Protocol Buffers Errors

If you see errors related to protobuf:

```bash
# Install/update protobuf compiler
brew upgrade protobuf  # macOS
```

### OAuth2 Token Issues

```bash
# Clear cached tokens
rm "$HOME/Library/Application Support/mcc-gaql/tokencache_*.json"

# Re-authenticate
cargo run -- --setup
```

### Build Errors

```bash
# Clean build artifacts
cargo clean

# Update dependencies
cargo update

# Rebuild
cargo build
```

## Additional Resources

- [Google Ads API Documentation](https://developers.google.com/google-ads/api/docs/start)
- [GAQL Reference](https://developers.google.com/google-ads/api/docs/query/overview)
- [Rust Book](https://doc.rust-lang.org/book/)
- [googleads-rs Documentation](https://github.com/mhuang74/googleads-rs)

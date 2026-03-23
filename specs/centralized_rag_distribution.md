# Specification: Centralized RAG Resource Distribution

## Problem Statement

Currently, new users must complete a lengthy setup process before using `mcc-gaql-gen generate`:

1. Fetch field metadata from Google Ads API (`mcc-gaql --refresh-field-cache`)
2. Scrape documentation (`mcc-gaql-gen scrape`) - 10-15 minutes
3. Enrich with LLM (`mcc-gaql-gen enrich`) - 20-40 minutes, costs $$$
4. Build embeddings (`mcc-gaql-gen index`) - 3-5 minutes

This 30-60 minute process requires LLM API access and incurs token costs for enrichment.

## Goals

1. Enable instant onboarding via a single `bootstrap` command (~30 seconds)
2. Distribute pre-built RAG resources (enriched metadata + embeddings + query cookbook)
3. Eliminate LLM costs for setup (enrichment already done centrally)
4. Replace current `upload`/`download` commands with `publish`/`bootstrap`
5. Include `query_cookbook.toml` in the distribution bundle

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Cloudflare R2 Bucket                          │
│  (public read, authenticated write)                              │
├─────────────────────────────────────────────────────────────────┤
│  mcc-gaql-rag-bundle-v23.tar.gz  (~500 KB compressed)            │
│  ├── manifest.json                                               │
│  ├── field_metadata_enriched.json                                │
│  ├── query_cookbook.toml                                         │
│  ├── lancedb/                                                    │
│  │   ├── field_metadata.lance/                                   │
│  │   └── query_cookbook.lance/                                   │
│  ├── field_metadata.hash                                         │
│  └── query_cookbook.hash                                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              │ HTTPS GET (public URL)
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                   User's Machine                                 │
│  $ mcc-gaql-gen bootstrap                                        │
│  ├── Downloads & extracts bundle to ~/.cache/mcc-gaql/           │
│  ├── Copies query_cookbook.toml to ~/.config/mcc-gaql/           │
│  ├── Validates manifest checksums                                │
│  └── Ready for `generate` command                                │
└─────────────────────────────────────────────────────────────────┘
```

## Bundle Contents

### Files Included

| File | Destination | Size (uncompressed) | Purpose |
|------|-------------|---------------------|---------|
| `manifest.json` | (validated, not stored) | ~2 KB | Version info, checksums |
| `field_metadata_enriched.json` | `~/.cache/mcc-gaql/` | ~3.3 MB | Enriched field metadata |
| `query_cookbook.toml` | `~/.config/mcc-gaql/` | ~20 KB | Example GAQL queries |
| `lancedb/field_metadata.lance/` | `~/.cache/mcc-gaql/lancedb/` | ~130 KB | Field embeddings |
| `lancedb/query_cookbook.lance/` | `~/.cache/mcc-gaql/lancedb/` | ~20 KB | Query embeddings |
| `field_metadata.hash` | `~/.cache/mcc-gaql/` | ~30 bytes | Cache validation |
| `query_cookbook.hash` | `~/.cache/mcc-gaql/` | ~30 bytes | Cache validation |

**Total**: ~3.5 MB uncompressed, ~450-500 KB compressed (gzip)

### Files NOT Included

| File | Size | Reason |
|------|------|--------|
| `fastembed-models/` | 127 MB | Downloaded automatically by fastembed on first `generate` |
| `field_metadata.json` | 3.3 MB | Redundant with enriched version |
| `scraped_docs.json` | 100 KB | Only needed for enrichment |
| `proto_docs_v23.json` | 1.4 MB | Only needed for enrichment |

## Manifest Format

```json
{
  "version": "1.0",
  "api_version": "v23",
  "created_at": "2026-03-23T10:00:00Z",
  "created_by": "mcc-gaql-gen publish",
  "cli_version": "0.16.2",
  "contents": {
    "field_count": 2906,
    "resource_count": 181,
    "query_cookbook_count": 52,
    "enriched_field_count": 2850
  },
  "hashes": {
    "field_metadata": "v1-dim384\n12345678901234567890",
    "query_cookbook": "v1-dim384\n98765432109876543210"
  },
  "files": [
    {
      "path": "field_metadata_enriched.json",
      "size": 3405150,
      "sha256": "abc123..."
    },
    {
      "path": "query_cookbook.toml",
      "size": 20480,
      "sha256": "def456..."
    },
    {
      "path": "lancedb/field_metadata.lance/data/...",
      "size": 126196,
      "sha256": "..."
    }
  ],
  "compatible_versions": ["0.16.0", "0.16.1", "0.16.2"]
}
```

## Commands

### Remove: `upload` and `download`

Delete the existing commands from `main.rs`:
- `Commands::Upload { file, key }`
- `Commands::Download { public_url, key, output }`

### Add: `bootstrap`

```
mcc-gaql-gen bootstrap [OPTIONS]

Download pre-built RAG resources for instant GAQL generation.

Options:
    --url <URL>         Public URL for the bundle
                        [env: MCC_GAQL_BUNDLE_URL]
                        [default: https://pub-xxx.r2.dev/mcc-gaql-rag-bundle-v23.tar.gz]

    --version <VER>     API version to download
                        [default: v23]

    --force             Overwrite existing cache even if valid

    --skip-validation   Skip SHA256 checksum validation

    --verify-only       Check current cache validity without downloading

Examples:
    mcc-gaql-gen bootstrap                    # Download and install bundle
    mcc-gaql-gen bootstrap --verify-only      # Check if cache is valid
    mcc-gaql-gen bootstrap --force            # Force re-download
```

**Behavior:**

1. Check if valid cache already exists (unless `--force`)
2. Download bundle from URL
3. Extract to temp directory
4. Validate manifest checksums
5. Copy files to appropriate locations:
   - Cache files → `~/.cache/mcc-gaql/`
   - Config files → `~/.config/mcc-gaql/`
6. Verify hash files match manifest
7. Print success message with next steps

### Add: `publish`

```
mcc-gaql-gen publish [OPTIONS]

Create and upload a RAG bundle to R2 storage.

Options:
    --key <KEY>         Object key name
                        [default: mcc-gaql-rag-bundle-v23.tar.gz]

    --dry-run           Create bundle locally without uploading
                        Outputs to: ./mcc-gaql-rag-bundle-v23.tar.gz

    --queries <FILE>    Path to query_cookbook.toml to include
                        [default: from config dir]

Requires environment variables:
    MCC_GAQL_R2_ENDPOINT_URL
    MCC_GAQL_R2_ACCESS_KEY_ID
    MCC_GAQL_R2_SECRET_ACCESS_KEY
    MCC_GAQL_R2_BUCKET (optional, default: mcc-gaql-metadata)

Examples:
    mcc-gaql-gen publish                      # Upload to R2
    mcc-gaql-gen publish --dry-run            # Create local bundle only
```

**Behavior:**

1. Validate required cache files exist and are valid
2. Load query_cookbook.toml from config directory
3. Create manifest.json with checksums
4. Create tar.gz bundle
5. Upload to R2 (unless `--dry-run`)
6. Print public URL for distribution

## Implementation Plan

### Phase 1: Create Bundle Module

**New file: `crates/mcc-gaql-gen/src/bundle.rs`**

```rust
//! RAG resource bundle creation and extraction.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use tar::{Archive, Builder};

/// Bundle manifest
#[derive(Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    pub api_version: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub created_by: String,
    pub cli_version: String,
    pub contents: ManifestContents,
    pub hashes: ManifestHashes,
    pub files: Vec<ManifestFile>,
    pub compatible_versions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestContents {
    pub field_count: usize,
    pub resource_count: usize,
    pub query_cookbook_count: usize,
    pub enriched_field_count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestHashes {
    pub field_metadata: String,
    pub query_cookbook: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ManifestFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

/// Create a bundle from current cache
pub async fn create_bundle(
    output_path: &Path,
    query_cookbook_path: &Path,
) -> Result<Manifest>;

/// Extract bundle to cache directories
pub async fn extract_bundle(
    bundle_path: &Path,
    skip_validation: bool,
) -> Result<Manifest>;

/// Download bundle from URL to temp file
pub async fn download_bundle(url: &str) -> Result<PathBuf>;

/// Compute SHA256 hash of a file
fn compute_sha256(path: &Path) -> Result<String>;

/// Validate manifest checksums against extracted files
fn validate_checksums(manifest: &Manifest, base_dir: &Path) -> Result<()>;
```

### Phase 2: Modify R2 Module

**Modify: `crates/mcc-gaql-gen/src/r2.rs`**

Keep existing `upload()` and `download()` functions but make them private.
Add new public functions:

```rust
/// Upload a bundle file to R2
pub async fn upload_bundle(local_path: &Path, object_key: &str) -> Result<String>;

/// Download a bundle from public URL (no auth required)
pub async fn download_bundle(url: &str, dest_path: &Path) -> Result<()>;
```

### Phase 3: Update Main Commands

**Modify: `crates/mcc-gaql-gen/src/main.rs`**

1. Remove `Commands::Upload` and `Commands::Download` variants
2. Remove `cmd_upload()` and `cmd_download()` functions
3. Add new command variants:

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...

    /// Download pre-built RAG resources for instant GAQL generation
    Bootstrap {
        /// Public URL for the bundle
        #[arg(long, env = "MCC_GAQL_BUNDLE_URL")]
        url: Option<String>,

        /// API version to download
        #[arg(long, default_value = "v23")]
        version: String,

        /// Overwrite existing cache even if valid
        #[arg(long)]
        force: bool,

        /// Skip SHA256 checksum validation
        #[arg(long)]
        skip_validation: bool,

        /// Check current cache validity without downloading
        #[arg(long)]
        verify_only: bool,
    },

    /// Create and upload a RAG bundle to R2 storage
    Publish {
        /// Object key name
        #[arg(long, default_value = "mcc-gaql-rag-bundle-v23.tar.gz")]
        key: String,

        /// Create bundle locally without uploading
        #[arg(long)]
        dry_run: bool,

        /// Path to query_cookbook.toml to include
        #[arg(long)]
        queries: Option<PathBuf>,
    },
}
```

4. Add command handlers:

```rust
async fn cmd_bootstrap(
    url: Option<String>,
    version: String,
    force: bool,
    skip_validation: bool,
    verify_only: bool,
) -> Result<()>;

async fn cmd_publish(
    key: String,
    dry_run: bool,
    queries: Option<PathBuf>,
) -> Result<()>;
```

### Phase 4: Add Dependencies

**Modify: `crates/mcc-gaql-gen/Cargo.toml`**

```toml
[dependencies]
# Existing...
flate2 = "1.0"           # gzip compression
tar = "0.4"              # tar archive handling
sha2 = "0.10"            # SHA256 checksums (may already exist)
```

## File Changes Summary

| File | Action | Changes |
|------|--------|---------|
| `crates/mcc-gaql-gen/src/bundle.rs` | Create | New module for bundle operations |
| `crates/mcc-gaql-gen/src/r2.rs` | Modify | Add bundle upload/download, keep core signing |
| `crates/mcc-gaql-gen/src/main.rs` | Modify | Remove upload/download, add bootstrap/publish |
| `crates/mcc-gaql-gen/src/lib.rs` | Modify | Export bundle module |
| `crates/mcc-gaql-gen/Cargo.toml` | Modify | Add flate2, tar dependencies |

## User Workflows

### New User Onboarding (After)

```bash
# One command, ~30 seconds
mcc-gaql-gen bootstrap

# Ready to use
export MCC_GAQL_LLM_API_KEY=sk-...
mcc-gaql-gen generate "show campaign performance last week"
```

### Maintainer Publishing

```bash
# After updating enrichment or query cookbook
export MCC_GAQL_R2_ENDPOINT_URL=...
export MCC_GAQL_R2_ACCESS_KEY_ID=...
export MCC_GAQL_R2_SECRET_ACCESS_KEY=...

mcc-gaql-gen publish
# Uploads to: https://pub-xxx.r2.dev/mcc-gaql-rag-bundle-v23.tar.gz
```

### Checking Cache Status

```bash
mcc-gaql-gen bootstrap --verify-only
# Output:
# Cache status:
#   Field metadata: valid (2906 fields, updated 2026-03-23)
#   Query cookbook: valid (52 queries, updated 2026-03-23)
#   LanceDB: valid
#
# Ready for 'mcc-gaql-gen generate'
```

## Error Handling

### Bootstrap Errors

| Scenario | Error Message | Recovery |
|----------|---------------|----------|
| Network failure | "Failed to download bundle: connection refused" | Retry or check URL |
| Invalid URL | "Bundle URL not configured. Set MCC_GAQL_BUNDLE_URL or use --url" | Provide URL |
| Checksum mismatch | "Checksum validation failed for field_metadata_enriched.json" | Re-download or --skip-validation |
| Incompatible version | "Bundle requires CLI version 0.17.0+, you have 0.16.2" | Upgrade CLI |
| Disk full | "Failed to extract bundle: No space left on device" | Free disk space |

### Publish Errors

| Scenario | Error Message | Recovery |
|----------|---------------|----------|
| Missing cache | "field_metadata_enriched.json not found. Run 'enrich' first." | Complete enrichment |
| Invalid cache | "Cache validation failed: hash mismatch" | Run 'index' to rebuild |
| Missing credentials | "R2 credentials not configured" | Set environment variables |
| Upload failure | "Upload failed: HTTP 403 Forbidden" | Check credentials |

## Testing Plan

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_manifest_serialization();

    #[test]
    fn test_compute_sha256();

    #[test]
    fn test_bundle_round_trip();
}
```

### Integration Tests

1. **Bundle creation**: Create bundle, verify all files included
2. **Bundle extraction**: Extract bundle, verify files in correct locations
3. **Checksum validation**: Corrupt a file, verify validation catches it
4. **Version compatibility**: Test with different CLI versions

### Manual Testing

```bash
# Test publish (dry-run)
mcc-gaql-gen publish --dry-run
ls -la mcc-gaql-rag-bundle-v23.tar.gz
tar -tzf mcc-gaql-rag-bundle-v23.tar.gz

# Test bootstrap (from local file)
rm -rf ~/.cache/mcc-gaql/lancedb ~/.cache/mcc-gaql/*.hash
mcc-gaql-gen bootstrap --url file://./mcc-gaql-rag-bundle-v23.tar.gz

# Verify generate works
mcc-gaql-gen generate "show campaigns"
```

## Security Considerations

1. **Public URL access**: Bundle is public, no secrets included
2. **Checksum validation**: SHA256 prevents tampering
3. **Version pinning**: Manifest specifies compatible CLI versions
4. **No credential storage**: R2 credentials only needed for publish

## Future Enhancements

1. **Incremental updates**: Download only changed files
2. **Multiple API versions**: Support v22, v23, v24 bundles
3. **Regional mirrors**: CDN distribution for faster downloads
4. **Signed manifests**: GPG signing for additional security
5. **Auto-update check**: Notify users when new bundle available

## Verification Checklist

- [ ] `mcc-gaql-gen bootstrap` downloads and extracts bundle
- [ ] `mcc-gaql-gen bootstrap --verify-only` checks cache status
- [ ] `mcc-gaql-gen bootstrap --force` overwrites existing cache
- [ ] `mcc-gaql-gen publish --dry-run` creates local bundle
- [ ] `mcc-gaql-gen publish` uploads to R2
- [ ] `mcc-gaql-gen generate` works after bootstrap
- [ ] Old `upload`/`download` commands are removed
- [ ] query_cookbook.toml is included in bundle
- [ ] query_cookbook.toml is extracted to config directory
- [ ] Checksums are validated during extraction
- [ ] Incompatible versions are rejected with clear error

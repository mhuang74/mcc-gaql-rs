# Implementation Notes: Centralized RAG Resource Distribution

**Date:** 2026-03-23
**Status:** Complete
**Related Spec:** `specs/centralized_rag_distribution.md`

---

## Overview

This implementation enables instant onboarding for new `mcc-gaql-gen` users via a `bootstrap` command (~30 seconds) instead of the previous 30-60 minute setup process. Pre-built RAG resources (enriched metadata + embeddings + query cookbook) are distributed via a downloadable bundle.

---

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

---

## Files Changed

| File | Action | Description |
|------|--------|-------------|
| `crates/mcc-gaql-gen/src/bundle.rs` | Created | New module for bundle creation, extraction, and validation |
| `crates/mcc-gaql-gen/src/r2.rs` | Modified | Added `upload_bundle()` and `download_bundle()` functions |
| `crates/mcc-gaql-gen/src/main.rs` | Modified | Replaced Upload/Download commands with Bootstrap/Publish |
| `crates/mcc-gaql-gen/src/lib.rs` | Modified | Exported `bundle` module |
| `crates/mcc-gaql-gen/Cargo.toml` | Modified | Added flate2, tar, tempfile dependencies |
| `Cargo.toml` | Modified | Added workspace dependencies for flate2, tar, tempfile |

---

## New Module: `bundle.rs`

### Key Types

```rust
/// Bundle manifest with metadata and checksums
pub struct Manifest {
    pub version: String,                    // "1.0"
    pub api_version: String,                // "v23"
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub cli_version: String,
    pub contents: ManifestContents,         // Field/resource/query counts
    pub hashes: ManifestHashes,             // Cache validation hashes
    pub files: Vec<ManifestFile>,           // File list with SHA256 checksums
    pub compatible_versions: Vec<String>,   // CLI version compatibility
}

/// Cache verification results
pub struct CacheVerification {
    pub field_metadata_valid: bool,
    pub query_cookbook_valid: bool,
    pub lancedb_valid: bool,
    pub field_count: usize,
    pub resource_count: usize,
    pub enriched_field_count: usize,
    pub query_count: usize,
}
```

### Public Functions

| Function | Description |
|----------|-------------|
| `create_bundle(output_path, query_cookbook_path)` | Creates a tar.gz bundle from current cache files |
| `extract_bundle(bundle_path, skip_validation)` | Extracts bundle to temp directory and validates manifest |
| `download_bundle(url)` | Downloads bundle from URL (supports http/https/file protocols) |
| `install_bundle(bundle, force)` | Copies bundle files to cache/config directories |
| `verify_cache()` | Checks if current cache is valid without installing |

---

## New Commands

### `mcc-gaql-gen bootstrap`

Download pre-built RAG resources for instant GAQL generation.

**Options:**
- `--url <URL>` - Public URL for the bundle (env: `MCC_GAQL_BUNDLE_URL`)
- `--version <VER>` - API version to download (default: v23)
- `--force` - Overwrite existing cache even if valid
- `--skip-validation` - Skip SHA256 checksum validation
- `--verify-only` - Check current cache validity without downloading

**Examples:**
```bash
mcc-gaql-gen bootstrap                    # Download and install bundle
mcc-gaql-gen bootstrap --verify-only      # Check if cache is valid
mcc-gaql-gen bootstrap --force            # Force re-download
```

**Behavior:**
1. Check if valid cache already exists (unless `--force`)
2. Download bundle from URL
3. Extract to temp directory
4. Validate manifest checksums
5. Copy files to appropriate locations
6. Verify hash files match manifest
7. Print success message with next steps

### `mcc-gaql-gen publish`

Create and upload a RAG bundle to R2 storage.

**Options:**
- `--key <KEY>` - Object key name (default: mcc-gaql-rag-bundle-v23.tar.gz)
- `--dry-run` - Create bundle locally without uploading
- `--queries <FILE>` - Path to query_cookbook.toml to include

**Environment Variables:**
- `MCC_GAQL_R2_ENDPOINT_URL`
- `MCC_GAQL_R2_ACCESS_KEY_ID`
- `MCC_GAQL_R2_SECRET_ACCESS_KEY`
- `MCC_GAQL_R2_BUCKET` (optional, default: mcc-gaql-metadata)

**Examples:**
```bash
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

---

## Removed Commands

| Old Command | Replacement | Notes |
|-------------|-------------|-------|
| `mcc-gaql-gen upload` | `mcc-gaql-gen publish` | New command handles bundle creation + upload |
| `mcc-gaql-gen download` | `mcc-gaql-gen bootstrap` | New command handles bundle download + extraction |

---

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

**Total:** ~3.5 MB uncompressed, ~450-500 KB compressed (gzip)

### Files NOT Included

| File | Size | Reason |
|------|------|--------|
| `fastembed-models/` | 127 MB | Downloaded automatically by fastembed on first `generate` |
| `field_metadata.json` | 3.3 MB | Redundant with enriched version |
| `scraped_docs.json` | 100 KB | Only needed for enrichment |
| `proto_docs_v23.json` | 1.4 MB | Only needed for enrichment |

---

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
    }
  ],
  "compatible_versions": ["0.16.0", "0.16.1", "0.16.2"]
}
```

---

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
#   Field metadata: valid
#   Query cookbook: valid
#   LanceDB: valid
#
#   Fields: 2906 (2850 enriched)
#   Resources: 181
#   Queries: 52
#
# Ready for 'mcc-gaql-gen generate'
```

---

## Error Handling

### Bootstrap Errors

| Scenario | Error Message | Recovery |
|----------|---------------|----------|
| Network failure | "Failed to download bundle: connection refused" | Retry or check URL |
| Invalid URL | "Bundle URL not configured. Set MCC_GAQL_BUNDLE_URL or use --url" | Provide URL |
| Checksum mismatch | "Checksum validation failed for field_metadata_enriched.json" | Re-download or --skip-validation |
| Incompatible version | Warning logged, continues | May work or may need CLI upgrade |
| Disk full | "Failed to extract bundle: No space left on device" | Free disk space |

### Publish Errors

| Scenario | Error Message | Recovery |
|----------|---------------|----------|
| Missing cache | "field_metadata_enriched.json not found. Run 'enrich' first." | Complete enrichment |
| Invalid cache | "Cache validation failed: hash mismatch" | Run 'index' to rebuild |
| Missing credentials | "R2 credentials not configured" | Set environment variables |
| Upload failure | "Upload failed: HTTP 403 Forbidden" | Check credentials |

---

## Dependencies Added

### Workspace Dependencies (`Cargo.toml`)

```toml
flate2 = "1.0"      # gzip compression
tar = "0.4"         # tar archive handling
tempfile = "3.8"    # Temporary file/directory management
```

### Crate Dependencies (`crates/mcc-gaql-gen/Cargo.toml`)

```toml
flate2 = { workspace = true }
tar = { workspace = true }
tempfile = { workspace = true }
```

---

## Security Considerations

1. **Public URL access**: Bundle is public, no secrets included
2. **Checksum validation**: SHA256 prevents tampering
3. **Version pinning**: Manifest specifies compatible CLI versions
4. **No credential storage**: R2 credentials only needed for publish

---

## Testing

### Unit Tests (in bundle.rs)

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_manifest_serialization();
    #[test]
    fn test_cache_verification();
}
```

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

---

## Future Enhancements

1. **Incremental updates**: Download only changed files
2. **Multiple API versions**: Support v22, v23, v24 bundles
3. **Regional mirrors**: CDN distribution for faster downloads
4. **Signed manifests**: GPG signing for additional security
5. **Auto-update check**: Notify users when new bundle available

---

## Verification Checklist

- [x] `mcc-gaql-gen bootstrap` downloads and extracts bundle
- [x] `mcc-gaql-gen bootstrap --verify-only` checks cache status
- [x] `mcc-gaql-gen bootstrap --force` overwrites existing cache
- [x] `mcc-gaql-gen publish --dry-run` creates local bundle
- [x] `mcc-gaql-gen publish` uploads to R2
- [x] `mcc-gaql-gen generate` works after bootstrap
- [x] Old `upload`/`download` commands are removed
- [x] query_cookbook.toml is included in bundle
- [x] query_cookbook.toml is extracted to config directory
- [x] Checksums are validated during extraction
- [x] Incompatible versions show warning

//! RAG resource bundle creation and extraction.

use anyhow::{Context, Result};
use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tar::{Archive, Builder};
use tokio::fs;

use crate::vector_store;
use mcc_gaql_common::config::get_queries_from_file;
use mcc_gaql_common::field_metadata::FieldMetadataCache;

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

/// Extracted bundle info
pub struct ExtractedBundle {
    pub manifest: Manifest,
    pub temp_dir: tempfile::TempDir,
}

impl ExtractedBundle {
    /// Get the full path to a file in the extracted bundle
    pub fn file_path(&self, relative_path: &str) -> PathBuf {
        self.temp_dir.path().join(relative_path)
    }
}

/// Create a bundle from current cache
pub async fn create_bundle(output_path: &Path, query_cookbook_path: &Path) -> Result<Manifest> {
    // Load enriched field metadata to get counts
    let enriched_path = mcc_gaql_common::paths::field_metadata_enriched_path()
        .context("Could not determine enriched metadata path")?;

    if !enriched_path.exists() {
        anyhow::bail!(
            "field_metadata_enriched.json not found at {:?}. Run 'enrich' first.",
            enriched_path
        );
    }

    let field_cache = FieldMetadataCache::load_from_disk(&enriched_path)
        .await
        .context("Failed to load enriched field metadata")?;

    let enriched_count = field_cache.enriched_field_count();
    let resource_count = field_cache.get_resources().len();
    let field_count = field_cache.fields.len();

    // Load query cookbook to get count
    let query_entries = if query_cookbook_path.exists() {
        get_queries_from_file(query_cookbook_path)
            .await
            .map(|m| m.len())
            .unwrap_or(0)
    } else {
        0
    };

    // Compute hashes
    let field_metadata_hash = compute_hash_file(
        &vector_store::get_hash_path("field_metadata")
            .context("Could not determine field metadata hash path")?,
    )
    .await?;

    let query_cookbook_hash = compute_hash_file(
        &vector_store::get_hash_path("query_cookbook")
            .context("Could not determine query cookbook hash path")?,
    )
    .await?;

    // Create temp directory for staging bundle contents
    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;
    let temp_path = temp_dir.path();

    // Copy files to temp directory with correct structure
    let mut files = Vec::new();

    // 1. Copy field_metadata_enriched.json
    let enriched_dest = temp_path.join("field_metadata_enriched.json");
    fs::copy(&enriched_path, &enriched_dest)
        .await
        .context("Failed to copy enriched metadata")?;
    let enriched_sha256 = compute_sha256(&enriched_dest)?;
    let enriched_size = fs::metadata(&enriched_dest).await?.len();
    files.push(ManifestFile {
        path: "field_metadata_enriched.json".to_string(),
        size: enriched_size,
        sha256: enriched_sha256,
    });

    // 2. Copy query_cookbook.toml
    let cookbook_dest = temp_path.join("query_cookbook.toml");
    fs::copy(query_cookbook_path, &cookbook_dest)
        .await
        .with_context(|| {
            format!(
                "Failed to copy query cookbook from {:?}",
                query_cookbook_path
            )
        })?;
    let cookbook_sha256 = compute_sha256(&cookbook_dest)?;
    let cookbook_size = fs::metadata(&cookbook_dest).await?.len();
    files.push(ManifestFile {
        path: "query_cookbook.toml".to_string(),
        size: cookbook_size,
        sha256: cookbook_sha256,
    });

    // 3. Copy LanceDB directory
    let lancedb_src =
        mcc_gaql_common::paths::lancedb_path().context("Could not determine LanceDB path")?;
    let lancedb_dest = temp_path.join("lancedb");
    copy_dir_all(&lancedb_src, &lancedb_dest)
        .await
        .context("Failed to copy LanceDB directory")?;

    // Collect all LanceDB files into manifest
    collect_dir_files(&lancedb_dest, temp_path, &mut files)?;

    // 4. Copy hash files
    let cache_dir =
        mcc_gaql_common::paths::cache_dir().context("Could not determine cache directory")?;

    let field_hash_src = cache_dir.join("field_metadata.hash");
    let field_hash_dest = temp_path.join("field_metadata.hash");
    if field_hash_src.exists() {
        fs::copy(&field_hash_src, &field_hash_dest).await?;
        let hash_sha256 = compute_sha256(&field_hash_dest)?;
        let hash_size = fs::metadata(&field_hash_dest).await?.len();
        files.push(ManifestFile {
            path: "field_metadata.hash".to_string(),
            size: hash_size,
            sha256: hash_sha256,
        });
    }

    let query_hash_src = cache_dir.join("query_cookbook.hash");
    let query_hash_dest = temp_path.join("query_cookbook.hash");
    if query_hash_src.exists() {
        fs::copy(&query_hash_src, &query_hash_dest).await?;
        let hash_sha256 = compute_sha256(&query_hash_dest)?;
        let hash_size = fs::metadata(&query_hash_dest).await?.len();
        files.push(ManifestFile {
            path: "query_cookbook.hash".to_string(),
            size: hash_size,
            sha256: hash_sha256,
        });
    }

    // Create manifest
    let manifest = Manifest {
        version: "1.0".to_string(),
        api_version: field_cache.api_version.clone(),
        created_at: chrono::Utc::now(),
        created_by: "mcc-gaql-gen publish".to_string(),
        cli_version: env!("CARGO_PKG_VERSION").to_string(),
        contents: ManifestContents {
            field_count,
            resource_count,
            query_cookbook_count: query_entries,
            enriched_field_count: enriched_count,
        },
        hashes: ManifestHashes {
            field_metadata: field_metadata_hash,
            query_cookbook: query_cookbook_hash,
        },
        files,
        compatible_versions: vec![
            "0.16.0".to_string(),
            "0.16.1".to_string(),
            "0.16.2".to_string(),
        ],
    };

    // Write manifest to temp directory
    let manifest_path = temp_path.join("manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    fs::write(&manifest_path, manifest_json).await?;

    // Create tar.gz archive
    let tar_gz = File::create(output_path)?;
    let enc = GzEncoder::new(&tar_gz, Compression::default());
    let mut tar = Builder::new(enc);

    tar.append_dir_all(".", temp_path)?;
    tar.finish()?;

    // Get final size
    let bundle_size = fs::metadata(output_path).await?.len();
    log::info!(
        "Created bundle: {} ({} bytes)",
        output_path.display(),
        bundle_size
    );

    Ok(manifest)
}

/// Extract bundle to temp directory
pub async fn extract_bundle(bundle_path: &Path, skip_validation: bool) -> Result<ExtractedBundle> {
    // Create temp directory for extraction
    let temp_dir = tempfile::tempdir().context("Failed to create temp directory")?;

    // Open and extract tar.gz
    let tar_gz = File::open(bundle_path)?;
    let dec = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(dec);

    archive
        .unpack(temp_dir.path())
        .context("Failed to extract bundle")?;

    log::debug!("extract_bundle: extracted to {:?}", temp_dir.path());

    // Log extracted files for debugging
    if let Ok(mut entries) = std::fs::read_dir(temp_dir.path()) {
        while let Some(Ok(entry)) = entries.next() {
            log::debug!("extract_bundle: found {:?}", entry.file_name());
        }
    }

    // Load and validate manifest
    let manifest_path = temp_dir.path().join("manifest.json");
    if !manifest_path.exists() {
        anyhow::bail!("Bundle missing manifest.json");
    }

    let manifest_content = fs::read_to_string(&manifest_path).await?;
    let manifest: Manifest =
        serde_json::from_str(&manifest_content).context("Failed to parse manifest.json")?;

    // Validate checksums if requested
    if !skip_validation {
        validate_checksums(&manifest, temp_dir.path())?;
    }

    // Check CLI version compatibility
    let current_version = env!("CARGO_PKG_VERSION");
    if !manifest
        .compatible_versions
        .iter()
        .any(|v| v == current_version)
    {
        log::warn!(
            "Bundle was created for CLI versions {:?}, you have {}. Compatibility not guaranteed.",
            manifest.compatible_versions,
            current_version
        );
    }

    Ok(ExtractedBundle {
        manifest,
        temp_dir,
    })
}

/// Download bundle from URL to temp file
pub async fn download_bundle(url: &str) -> Result<PathBuf> {
    log::info!("Downloading bundle from {}", url);

    // Handle file:// URLs for local testing
    if url.starts_with("file://") {
        let path = url.trim_start_matches("file://");
        return Ok(PathBuf::from(path));
    }

    let client = reqwest::Client::builder()
        .user_agent("mcc-gaql-gen (bundle downloader)")
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to download bundle from {}", url))?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed: HTTP {} for {}", response.status(), url);
    }

    let bytes = response
        .bytes()
        .await
        .context("Failed to read response body")?;

    // Create temp file
    let temp_file = tempfile::NamedTempFile::with_suffix(".tar.gz")?;
    let temp_path = temp_file.into_temp_path();
    let mut file = File::create(&temp_path)?;
    file.write_all(&bytes)?;

    log::info!("Downloaded {} bytes to temp file", bytes.len());
    let path = temp_path.keep().context("Failed to persist temp file")?;
    Ok(path)
}

/// Copy files from extracted bundle to cache and config directories
pub async fn install_bundle(bundle: &ExtractedBundle, force: bool) -> Result<()> {
    let cache_dir =
        mcc_gaql_common::paths::cache_dir().context("Could not determine cache directory")?;
    let config_dir =
        mcc_gaql_common::paths::config_dir().context("Could not determine config directory")?;

    log::debug!("install_bundle: cache_dir={:?}", cache_dir);
    log::debug!("install_bundle: config_dir={:?}", config_dir);
    log::debug!("install_bundle: bundle temp_dir={:?}", bundle.temp_dir.path());

    // Ensure directories exist
    fs::create_dir_all(&cache_dir)
        .await
        .with_context(|| format!("Failed to create cache directory: {:?}", cache_dir))?;
    fs::create_dir_all(&config_dir)
        .await
        .with_context(|| format!("Failed to create config directory: {:?}", config_dir))?;

    log::debug!("install_bundle: directories created/verified");

    // Check if cache already exists and is valid (unless --force)
    if !force {
        let enriched_path = cache_dir.join("field_metadata_enriched.json");
        if enriched_path.exists() {
            // Check if hash files match
            let current_field_hash =
                compute_hash_file(&cache_dir.join("field_metadata.hash")).await?;
            if current_field_hash == bundle.manifest.hashes.field_metadata {
                log::info!(
                    "Cache already up-to-date, skipping installation (use --force to overwrite)"
                );
                return Ok(());
            }
        }
    }

    // Copy field_metadata_enriched.json to cache
    let enriched_src = bundle.file_path("field_metadata_enriched.json");
    let enriched_dest = cache_dir.join("field_metadata_enriched.json");
    log::debug!("install_bundle: copying {:?} -> {:?}", enriched_src, enriched_dest);
    log::debug!("install_bundle: enriched_src exists={}", enriched_src.exists());
    fs::copy(&enriched_src, &enriched_dest)
        .await
        .with_context(|| {
            format!(
                "Failed to copy enriched metadata: {:?} -> {:?}",
                enriched_src, enriched_dest
            )
        })?;
    log::info!("Installed field_metadata_enriched.json");

    // Copy query_cookbook.toml to config
    let cookbook_src = bundle.file_path("query_cookbook.toml");
    let cookbook_dest = config_dir.join("query_cookbook.toml");
    log::debug!("install_bundle: copying {:?} -> {:?}", cookbook_src, cookbook_dest);
    log::debug!("install_bundle: cookbook_src exists={}", cookbook_src.exists());
    if cookbook_src.exists() {
        // Remove existing file/symlink at destination to avoid broken-symlink errors
        if cookbook_dest.exists() || cookbook_dest.symlink_metadata().is_ok() {
            fs::remove_file(&cookbook_dest).await.with_context(|| {
                format!("Failed to remove existing {:?}", cookbook_dest)
            })?;
            log::debug!("install_bundle: removed existing {:?}", cookbook_dest);
        }
        fs::copy(&cookbook_src, &cookbook_dest)
            .await
            .with_context(|| {
                format!(
                    "Failed to copy query cookbook: {:?} -> {:?}",
                    cookbook_src, cookbook_dest
                )
            })?;
        log::info!("Installed query_cookbook.toml");
    } else {
        log::warn!("query_cookbook.toml not found in bundle, skipping");
    }

    // Copy LanceDB directory
    let lancedb_src = bundle.file_path("lancedb");
    let lancedb_dest = cache_dir.join("lancedb");
    log::debug!("install_bundle: copying lancedb {:?} -> {:?}", lancedb_src, lancedb_dest);
    log::debug!("install_bundle: lancedb_src exists={}", lancedb_src.exists());

    // Remove existing LanceDB if it exists
    if lancedb_dest.exists() {
        fs::remove_dir_all(&lancedb_dest).await?;
    }

    copy_dir_all(&lancedb_src, &lancedb_dest)
        .await
        .with_context(|| {
            format!(
                "Failed to copy LanceDB: {:?} -> {:?}",
                lancedb_src, lancedb_dest
            )
        })?;
    log::info!("Installed lancedb/");

    // Copy hash files
    let field_hash_src = bundle.file_path("field_metadata.hash");
    log::debug!(
        "install_bundle: field_metadata.hash exists={}",
        field_hash_src.exists()
    );
    if field_hash_src.exists() {
        let dest = cache_dir.join("field_metadata.hash");
        fs::copy(&field_hash_src, &dest).await.with_context(|| {
            format!("Failed to copy field_metadata.hash: {:?} -> {:?}", field_hash_src, dest)
        })?;
        log::info!("Installed field_metadata.hash");
    }

    let query_hash_src = bundle.file_path("query_cookbook.hash");
    log::debug!(
        "install_bundle: query_cookbook.hash exists={}",
        query_hash_src.exists()
    );
    if query_hash_src.exists() {
        let dest = cache_dir.join("query_cookbook.hash");
        fs::copy(&query_hash_src, &dest).await.with_context(|| {
            format!("Failed to copy query_cookbook.hash: {:?} -> {:?}", query_hash_src, dest)
        })?;
        log::info!("Installed query_cookbook.hash");
    }

    Ok(())
}

/// Check if current cache is valid without installing
pub async fn verify_cache() -> Result<CacheVerification> {
    let cache_dir =
        mcc_gaql_common::paths::cache_dir().context("Could not determine cache directory")?;

    let field_metadata_valid = cache_dir.join("field_metadata.hash").exists()
        && cache_dir.join("field_metadata_enriched.json").exists();

    let query_cookbook_valid = cache_dir.join("query_cookbook.hash").exists();
    let lancedb_valid = cache_dir.join("lancedb").exists();

    // Get enriched cache info
    let enriched_path = cache_dir.join("field_metadata_enriched.json");
    let (field_count, resource_count, enriched_field_count) = if enriched_path.exists() {
        match FieldMetadataCache::load_from_disk(&enriched_path).await {
            Ok(cache) => {
                let enriched = cache.enriched_field_count();
                (cache.fields.len(), cache.get_resources().len(), enriched)
            }
            Err(_) => (0, 0, 0),
        }
    } else {
        (0, 0, 0)
    };

    // Get query cookbook count
    let config_dir =
        mcc_gaql_common::paths::config_dir().context("Could not determine config directory")?;
    let cookbook_path = config_dir.join("query_cookbook.toml");
    let query_count = if cookbook_path.exists() {
        match get_queries_from_file(&cookbook_path).await {
            Ok(queries) => queries.len(),
            Err(_) => 0,
        }
    } else {
        0
    };

    Ok(CacheVerification {
        field_metadata_valid,
        query_cookbook_valid,
        lancedb_valid,
        field_count,
        resource_count,
        enriched_field_count,
        query_count,
    })
}

/// Cache verification results
#[derive(Debug)]
pub struct CacheVerification {
    pub field_metadata_valid: bool,
    pub query_cookbook_valid: bool,
    pub lancedb_valid: bool,
    pub field_count: usize,
    pub resource_count: usize,
    pub enriched_field_count: usize,
    pub query_count: usize,
}

impl CacheVerification {
    /// Check if all caches are valid
    pub fn is_valid(&self) -> bool {
        self.field_metadata_valid && self.query_cookbook_valid && self.lancedb_valid
    }
}

/// Compute SHA256 hash of a file
fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("Failed to open file: {:?}", path))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let hash = hasher.finalize();
    Ok(hex::encode(hash))
}

/// Compute hash from hash file (reads the second line which contains the actual hash)
async fn compute_hash_file(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }

    let content = fs::read_to_string(path).await?;
    let lines: Vec<&str> = content.lines().collect();

    if lines.len() >= 2 {
        Ok(lines[1].to_string())
    } else {
        Ok(String::new())
    }
}

/// Validate manifest checksums against extracted files
fn validate_checksums(manifest: &Manifest, base_dir: &Path) -> Result<()> {
    log::info!("Validating checksums for {} files...", manifest.files.len());

    for file in &manifest.files {
        let file_path = base_dir.join(&file.path);

        if !file_path.exists() {
            anyhow::bail!("Missing file in bundle: {}", file.path);
        }

        let computed_hash = compute_sha256(&file_path)
            .with_context(|| format!("Failed to compute hash for {}", file.path))?;

        if computed_hash != file.sha256 {
            anyhow::bail!(
                "Checksum mismatch for {}: expected {}, got {}",
                file.path,
                file.sha256,
                computed_hash
            );
        }
    }

    log::info!("All checksums validated successfully");
    Ok(())
}

/// Copy a directory recursively
async fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).await?;

    let mut entries = fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let file_name = src_path
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("Invalid file name"))?;
        let dst_path = dst.join(file_name);

        let metadata = entry.metadata().await?;
        if metadata.is_dir() {
            Box::pin(copy_dir_all(&src_path, &dst_path)).await?;
        } else {
            fs::copy(&src_path, &dst_path).await?;
        }
    }

    Ok(())
}

/// Collect all files in a directory recursively and add to manifest
fn collect_dir_files(dir: &Path, base: &Path, files: &mut Vec<ManifestFile>) -> Result<()> {
    for entry in walkdir::WalkDir::new(dir) {
        let entry = entry?;
        if entry.file_type().is_file() {
            let full_path = entry.path();
            let relative_path = full_path.strip_prefix(base)?;
            let sha256 = compute_sha256(full_path)?;
            let size = entry.metadata()?.len();

            files.push(ManifestFile {
                path: relative_path.to_string_lossy().to_string(),
                size,
                sha256,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_serialization() {
        let manifest = Manifest {
            version: "1.0".to_string(),
            api_version: "v23".to_string(),
            created_at: chrono::Utc::now(),
            created_by: "test".to_string(),
            cli_version: "0.16.2".to_string(),
            contents: ManifestContents {
                field_count: 100,
                resource_count: 10,
                query_cookbook_count: 5,
                enriched_field_count: 95,
            },
            hashes: ManifestHashes {
                field_metadata: "test-hash-1".to_string(),
                query_cookbook: "test-hash-2".to_string(),
            },
            files: vec![ManifestFile {
                path: "test.json".to_string(),
                size: 1000,
                sha256: "abc123".to_string(),
            }],
            compatible_versions: vec!["0.16.2".to_string()],
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest.version, deserialized.version);
        assert_eq!(
            manifest.contents.field_count,
            deserialized.contents.field_count
        );
    }

    #[test]
    fn test_cache_verification() {
        let verification = CacheVerification {
            field_metadata_valid: true,
            query_cookbook_valid: true,
            lancedb_valid: true,
            field_count: 100,
            resource_count: 10,
            enriched_field_count: 95,
            query_count: 5,
        };

        assert!(verification.is_valid());

        let invalid = CacheVerification {
            field_metadata_valid: false,
            ..verification
        };

        assert!(!invalid.is_valid());
    }
}

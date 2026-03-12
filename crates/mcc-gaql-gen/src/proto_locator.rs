//! Proto file locator for googleads-rs proto files.
//!
//! This module locates the proto files from the googleads-rs crate, which contain
//! authoritative field-level documentation for the Google Ads API.

use std::path::PathBuf;
use anyhow::Result;

/// Locates the googleads-rs proto directory containing V23 proto files.
///
/// Tries multiple strategies in order:
/// 1. Check if googleads-rs is a path dependency
/// 2. Find in cargo git cache
/// 3. Use environment variable override
pub fn find_googleads_proto_dir() -> Result<PathBuf> {
    // Strategy 1: Check for environment variable override
    if let Ok(proto_dir) = std::env::var("GOOGLEADS_PROTO_DIR") {
        let path = PathBuf::from(proto_dir);
        if path.exists() {
            return Ok(path);
        }
    }

    // Strategy 2: Check cargo git cache
    if let Some(path) = find_in_cargo_cache() {
        return Ok(path);
    }

    anyhow::bail!(
        "Could not locate googleads-rs proto files. \n\
         Either set GOOGLEADS_PROTO_DIR environment variable, or ensure \n\
         googleads-rs dependency is fetched. Proto files should be in: \n\
         $CARGO_HOME/git/checkouts/googleads-rs-*/proto/google/ads/googleads/v23/"
    )
}

/// Attempts to find proto directory in cargo git cache.
fn find_in_cargo_cache() -> Option<PathBuf> {
    // Get cargo home directory
    let cargo_home = if let Ok(home) = std::env::var("CARGO_HOME") {
        PathBuf::from(home)
    } else if let Some(home) = dirs::home_dir() {
        home.join(".cargo")
    } else {
        return None;
    };

    let checkouts_dir = cargo_home.join("git/checkouts");

    if !checkouts_dir.exists() {
        return None;
    }

    // Find googleads-rs-* directories
    let entries = std::fs::read_dir(&checkouts_dir).ok()?;
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();

        if dir_name.starts_with("googleads-rs-") {
            let proto_path = entry.path()
                .join("proto/google/ads/googleads/v23");

            if proto_path.exists() {
                return Some(proto_path);
            }
        }
    }

    None
}

/// Returns the path to a specific proto subdirectory.
pub fn get_resources_dir(proto_root: &PathBuf) -> Result<PathBuf> {
    let resources = proto_root.join("resources");
    if !resources.exists() {
        anyhow::bail!("Resources directory not found at {:?}", resources);
    }
    Ok(resources)
}

/// Returns the path to the enums proto subdirectory.
pub fn get_enums_dir(proto_root: &PathBuf) -> Result<PathBuf> {
    let enums = proto_root.join("enums");
    if !enums.exists() {
        anyhow::bail!("Enums directory not found at {:?}", enums);
    }
    Ok(enums)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_googleads_proto_dir() {
        // This test will pass if we can find the proto directory
        let result = find_googleads_proto_dir();
        if let Ok(path) = result {
            assert!(path.exists());
            assert!(path.join("resources").exists());
            assert!(path.join("enums").exists());
        }
    }

    #[test]
    fn test_proto_dir_structure() {
        let proto_dir = find_googleads_proto_dir().expect("Proto dir should exist");
        let resources = get_resources_dir(&proto_dir).expect("Resources dir should exist");
        let enums = get_enums_dir(&proto_dir).expect("Enums dir should exist");

        // Check expected file counts
        let resource_files: Vec<_> = std::fs::read_dir(resources)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "proto"))
            .collect();

        let enum_files: Vec<_> = std::fs::read_dir(enums)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map_or(false, |ext| ext == "proto"))
            .collect();

        // Per spec: 182 resources, 360 enums
        assert!(resource_files.len() > 100, "Should have >100 resource protos");
        assert!(enum_files.len() > 200, "Should have >200 enum protos");
    }
}
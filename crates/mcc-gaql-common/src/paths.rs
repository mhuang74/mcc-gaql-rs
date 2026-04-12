use anyhow::{Result, anyhow};
use std::path::PathBuf;

const CRATE_NAME: &str = "mcc-gaql";

/// Get the platform-correct config directory for mcc-gaql
/// macOS: ~/Library/Application Support/mcc-gaql/
/// Linux: ~/.config/mcc-gaql/
pub fn config_dir() -> Result<PathBuf> {
    dirs::config_dir()
        .map(|mut p| {
            p.push(CRATE_NAME);
            p
        })
        .ok_or_else(|| anyhow!("Could not determine config directory"))
}

/// Get the platform-correct cache directory for mcc-gaql
/// macOS: ~/Library/Caches/mcc-gaql/
/// Linux: ~/.cache/mcc-gaql/
pub fn cache_dir() -> Result<PathBuf> {
    dirs::cache_dir()
        .map(|mut p| {
            p.push(CRATE_NAME);
            p
        })
        .ok_or_else(|| anyhow!("Could not determine cache directory"))
}

/// Get the path to the config file (config.toml)
pub fn config_file_path(filename: &str) -> Option<PathBuf> {
    dirs::config_dir().map(|mut path| {
        path.push(CRATE_NAME);
        path.push(filename);
        path
    })
}

/// Get the default field metadata cache path
pub fn field_metadata_cache_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("field_metadata.json"))
}

/// Get the enriched field metadata cache path
pub fn field_metadata_enriched_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("field_metadata_enriched.json"))
}

/// Get the LanceDB vector store path
pub fn lancedb_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("lancedb"))
}

/// Get the scraped docs cache path
pub fn scraped_docs_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("scraped_docs.json"))
}

/// Get the path to the domain knowledge file
pub fn domain_knowledge_path() -> Result<PathBuf> {
    config_file_path("domain_knowledge.md")
        .ok_or_else(|| anyhow!("Could not determine domain knowledge path"))
}

/// Get the path to the query cookbook file
pub fn query_cookbook_path() -> Result<PathBuf> {
    config_file_path("query_cookbook.toml")
        .ok_or_else(|| anyhow!("Could not determine query cookbook path"))
}

/// Get the path to proto docs cache
pub fn proto_docs_path() -> Result<PathBuf> {
    Ok(cache_dir()?.join("proto_docs_v23.json"))
}

//
// Author: Michael S. Huang (mhuang74@gmail.com)
//
// Library module exposing public APIs for testing and potential reuse

// Increase recursion limit to prevent lance crate compilation overflow
#![recursion_limit = "512"]

pub mod args;
pub mod config;
pub mod field_metadata;
pub mod googleads;
#[cfg(feature = "llm")]
pub mod lancedb_utils;
#[cfg(feature = "llm")]
pub mod metadata_enricher;
pub mod metadata_scraper;
#[cfg(feature = "llm")]
pub mod model_pool;
#[cfg(feature = "llm")]
pub mod prompt2gaql;
pub mod setup;
pub mod util;

#[cfg(feature = "llm")]
pub use model_pool::{ModelLease, ModelPool};

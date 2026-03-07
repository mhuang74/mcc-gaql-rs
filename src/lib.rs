//
// Author: Michael S. Huang (mhuang74@gmail.com)
//
// Library module exposing public APIs for testing and potential reuse

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
pub mod prompt2gaql;
pub mod setup;
pub mod util;

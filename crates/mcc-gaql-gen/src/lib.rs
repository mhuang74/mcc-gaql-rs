// Library target for integration tests and external access.
#![recursion_limit = "512"]
pub mod enricher;
pub mod model_pool;
pub mod proto_locator;
pub mod proto_parser;
pub mod proto_docs_cache;
pub mod r2;
pub mod rag;
#[deprecated(since = "0.15.0", note = "Use proto_locator, proto_parser, and proto_docs_cache instead")]
pub mod scraper;
pub mod vector_store;

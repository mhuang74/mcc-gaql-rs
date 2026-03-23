// Library target for integration tests and external access.
#![recursion_limit = "512"]
pub mod bundle;
pub mod enricher;
pub mod formatter;
pub mod model_pool;
pub mod proto_docs_cache;
pub mod proto_locator;
pub mod proto_parser;
pub mod r2;
pub mod rag;
// ScrapedDocs is still used as a data structure for field documentation,
// even though web scraping is deprecated in favor of proto-based extraction.
pub mod scraper;
pub mod vector_store;

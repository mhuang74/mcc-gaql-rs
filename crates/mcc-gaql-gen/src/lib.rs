// Library target for integration tests and external access.
#![recursion_limit = "512"]
pub mod enricher;
pub mod model_pool;
pub mod proto_locator;
pub mod proto_parser;
pub mod proto_docs_cache;
pub mod r2;
pub mod rag;
// ScrapedDocs is still used as a data structure for field documentation,
// even though web scraping is deprecated in favor of proto-based extraction.
pub mod scraper;
pub mod vector_store;

/// Google Ads API version this workspace is built against.
/// Single source of truth — bump this (and googleads-rs) together.
pub const GOOGLEADS_API_VERSION: &str = "v24";

/// Default R2 object key for published RAG bundles.
/// Contains a literal version segment, guarded by the test below.
pub const RAG_BUNDLE_KEY: &str = "mcc-gaql-rag-bundle-v24.tar.gz";

#[cfg(test)]
mod tests {
    use super::*;

    /// Guards against RAG_BUNDLE_KEY drifting from the version constant.
    #[test]
    fn rag_bundle_key_contains_api_version() {
        assert!(RAG_BUNDLE_KEY.contains(GOOGLEADS_API_VERSION));
    }
}
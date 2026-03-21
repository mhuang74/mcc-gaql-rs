# Changelog

All notable changes to this project will be documented in this file.

## [0.16.2] - 2026-03-21

### Features
- Multi-step RAG pipeline for GAQL generation (#52)
- Add `--explain-selection-process` flag for RAG transparency
- Add `--use-query-cookbook` flag for optional RAG cookbook examples
- Add keyword-based field matching to supplement vector search
- Add concurrency to key field selection and resource description
- Add `mcc-gaql-gen metadata` subcommand for RAG debugging
- Add timing instrumentation to RAG pipeline
- Add debug logging for LLM responses
- Add model parameters logging and full prompt trace dumps
- Inject today's date and temporal examples into LLM prompts
- Improve resource selection prompt formatting
- Increase key field selection ranges for LLM
- Single-resource enrichment with retry backoff
- Clean GAQL output for generate command
- Print version banner on startup (with GIT_HASH and BUILD_TIME)
- Improve date selection; print resource descriptions in Explanation

### Fixes
- Fix nested proto message parsing (repeated/multiline fields, inline messages)
- Fix duplicate fields from nested proto messages
- Fix pre-filtering in vector search for better field selection
- Fix keyword search to use full word matching, not substring
- Fix `--batch-size` CLI argument wiring to MetadataEnricher
- Fix numeric filter values in LLM field selection response
- Fix tokio::join! type annotations for CI build
- Fix rig-core dependency (0.32.0 -> 0.33.0) for compatibility
- Fix use total_concurrency for buffer_unordered limit
- Fix populate key_metrics for views using selectable_with
- Fix DURING operator support and GaqlBuilder pattern
- Fix LanceDB deprecation warning and isolate test cache
- Fix tests deleting production cache hashes
- Standardize R2 env vars with MCC_GAQL_ prefix
- Remove 15-field truncation from LLM prompt
- Revert --trace flag; use MCC_GAQL_LOG_LEVEL=trace instead

## [0.15.0] - Previous Release

*See git history for changes prior to v0.15.0*

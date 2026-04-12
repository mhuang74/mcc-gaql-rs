# Metadata Maintenance Guide

This guide is for maintainers and developers who need to build or rebuild the Google Ads field metadata cache. Most users should use `mcc-gaql-gen bootstrap` to download pre-built metadata instead.

## When to Run This Workflow

Run the metadata maintenance workflow when:
- Upgrading to a new Google Ads API version (via googleads-rs crate update)
- New resources or fields are available but not yet in the cache
- The LLM-generated field descriptions need improvement
- Publishing updated metadata for others to use via `bootstrap`

## Prerequisites

- Ability to build `mcc-gaql-gen` from source (proto files come from build dependencies)
- Valid Google Ads API credentials (for `--refresh-field-cache`)
- LLM API credentials configured (for `enrich` command)
- R2/S3 credentials (only if publishing metadata bundles)

## The 5-Step Metadata Pipeline

### Step 1: Refresh Field Cache

```bash
mcc-gaql --refresh-field-cache
```

**Purpose:** Fetches all available resources and fields from the currently supported Google Ads API version.

**Who can run:** Anyone with valid Google Ads API credentials.

**Output:** `field_metadata.json` in your config directory.

**What it does:** Queries the Google Ads API FieldService to retrieve:
- All available resource types (campaign, ad_group, etc.)
- All available fields for each resource
- Field data types and categorization (METRIC, ATTRIBUTE, SEGMENT)
- Which fields are filterable, sortable, and selectable

---

### Step 2: Parse Proto Files

```bash
mcc-gaql-gen parse-protos
```

**Purpose:** Extracts field documentation from Google's official proto files bundled with the googleads-rs crate.

**Who can run:** Only developers who can build `mcc-gaql-gen` from source. The proto files are located in the Cargo registry/git checkouts and are not available in pre-built binaries.

**Output:** `proto_docs_v23.json` in your cache directory containing:
- ~182 resource messages with ~3000 fields
- ~360 enums with ~1200 values
- Field behavior annotations (IMMUTABLE, OUTPUT_ONLY, REQUIRED, OPTIONAL)
- Official Google documentation strings

**Options:**
- `--force` - Rebuild cache even if it exists
- `--output <PATH>` - Custom output path

**Proto file location:**
```
$CARGO_HOME/git/checkouts/googleads-rs-*/proto/google/ads/googleads/v23/
```

---

### Step 3: Enrich Metadata

```bash
# Using LLM for semantic enrichment (recommended)
mcc-gaql-gen enrich

# Using proto documentation only (faster, no LLM calls)
mcc-gaql-gen enrich --use-proto
```

**Purpose:** Enriches field metadata with human-readable descriptions and semantic context.

**Who can run:** Anyone with LLM credentials configured (unless using `--use-proto`).

**Output:** `field_metadata_enriched.json` - enhanced version of field metadata.

**What it does:**
- Merges proto documentation into field metadata
- (Without `--use-proto`) Uses LLM to generate improved descriptions based on field names and proto comments
- Adds semantic context to help natural language query generation

**Requirements for LLM mode:**
- `MCC_GAQL_LLM_API_KEY`
- `MCC_GAQL_LLM_BASE_URL`
- `MCC_GAQL_LLM_MODEL`

#### Multiple Models for Concurrent Processing

You can specify multiple comma-separated models in `MCC_GAQL_LLM_MODEL` to enable concurrent processing during **field metadata enrichment**. When multiple models are configured, the tool uses a model pool to parallelize LLM calls, significantly speeding up the enrichment process.

**How it works:**
- Each model gets one concurrent "slot" (rate-limiting per model)
- Batches of fields are distributed across all available models
- If a model is busy, work is automatically routed to the next available model
- The first model in the list is considered the "preferred" model for single-model operations

**Example with multiple models:**
```bash
# Use multiple models concurrently for faster metadata enrichment
export MCC_GAQL_LLM_API_KEY="your_openrouter_api_key"
export MCC_GAQL_LLM_BASE_URL="https://openrouter.ai/api/v1"
export MCC_GAQL_LLM_MODEL="google/gemini-flash-2.0,openai/gpt-4o-mini,anthropic/claude-3.5-haiku"

# Now when you enrich metadata, enrichment happens in parallel across all 3 models
mcc-gaql-gen enrich
```

> **Note:** Multiple models are only used for field metadata enrichment operations. Natural language query generation always uses the first (preferred) model.

---

### Step 4: Index for RAG

```bash
mcc-gaql-gen index
```

**Purpose:** Populates the LanceDB vector database with enriched metadata for fast similarity search during natural language query generation.

**Who can run:** Anyone with the enriched metadata file.

**Output:** Vector embeddings stored in `~/.cache/mcc-gaql/lancedb/`.

**What it does:**
- Generates embeddings for all resources and fields using FastEmbed
- Stores vectors in LanceDB for efficient similarity search
- Enables semantic retrieval of relevant fields during `generate` command

**Time:** Takes a few minutes depending on your hardware.

---

### Step 5: Publish Bundle

```bash
# Create and upload to R2
mcc-gaql-gen publish

# Create locally without uploading (dry run)
mcc-gaql-gen publish --dry-run
```

**Purpose:** Creates a metadata bundle and uploads it to R2 storage for distribution via `mcc-gaql-gen bootstrap`.

**Who can run:**
- **Repo owner:** Can publish to the default R2 URL (hardcoded in binary)
- **Anyone:** Can publish to a custom R2/S3 bucket and configure `bootstrap` to use it

**Requirements:**
- `MCC_GAQL_R2_ACCESS_KEY_ID`
- `MCC_GAQL_R2_SECRET_ACCESS_KEY`
- `MCC_GAQL_R2_BUCKET`
- `MCC_GAQL_R2_ENDPOINT_URL`

**Custom bucket workflow:**
```bash
# Publish to your own bucket
export MCC_GAQL_R2_ENDPOINT_URL="https://your-account.r2.cloudflarestorage.com"
export MCC_GAQL_R2_BUCKET="your-bucket"
mcc-gaql-gen publish

# Users can then bootstrap from your bucket
export MCC_GAQL_R2_PUBLIC_ID="your-public-bucket-id"
mcc-gaql-gen bootstrap
```

---

## Additional Commands

### Display Metadata

```bash
# Show metadata for a specific resource
mcc-gaql-gen metadata campaign

# Show all fields (not just LLM-limited subset)
mcc-gaql-gen metadata campaign --show-all

# Compare enriched vs non-enriched
mcc-gaql-gen metadata campaign --diff
```

**Purpose:** Debug and inspect enriched field metadata.

---

### Clear Cache

```bash
mcc-gaql-gen clear-cache
```

**Purpose:** Removes the local LanceDB vector cache. Useful when:
- Rebuilding from scratch
- Debugging embedding issues
- Freeing disk space

**Note:** This only clears the vector cache. Proto docs and enriched metadata files remain.

---

## Debugging Metadata Issues

### Checking if Metadata is Stale

Compare the API version in your metadata with Google's current version:

```bash
# Check what version your metadata was built for
head ~/.cache/mcc-gaql/field_metadata_enriched.json

# Compare with the version in googleads-rs
grep "googleads-rs" Cargo.lock
```

### Verifying Proto Parsing

```bash
# Check proto docs exist and have content
ls -la ~/.cache/mcc-gaql/proto_docs_v23.json
wc -l ~/.cache/mcc-gaql/proto_docs_v23.json

# Verify parsing worked
grep -c "message" ~/.cache/mcc-gaql/proto_docs_v23.json
```

### Checking LanceDB Contents

```bash
# Check LanceDB directory exists and has content
ls -la ~/.cache/mcc-gaql/lancedb/

# The directory should contain .lance files
find ~/.cache/mcc-gaql/lancedb -name "*.lance" | wc -l
```

### Common Issues

**Issue:** `parse-protos` fails with "proto files not found"
- **Cause:** You may not have built the project from source
- **Fix:** Run `cargo build -p mcc-gaql-gen` first to fetch dependencies

**Issue:** `enrich` fails with LLM errors
- **Cause:** LLM credentials not configured
- **Fix:** Set `MCC_GAQL_LLM_API_KEY`, `MCC_GAQL_LLM_BASE_URL`, and `MCC_GAQL_LLM_MODEL`
- **Workaround:** Use `mcc-gaql-gen enrich --use-proto` for proto-only enrichment

**Issue:** `index` takes very long or runs out of memory
- **Cause:** FastEmbed model download or embedding generation
- **Fix:** Ensure sufficient disk space (~2GB) and RAM (~4GB)

**Issue:** `publish` fails with access denied
- **Cause:** Missing or incorrect R2 credentials
- **Fix:** Verify all `MCC_GAQL_R2_*` environment variables

---

## File Reference

| File | Location | Purpose |
|------|----------|---------|
| Field metadata | `~/.config/mcc-gaql/field_metadata.json` | Raw field metadata from Google Ads API |
| Enriched metadata | `~/.cache/mcc-gaql/field_metadata_enriched.json` | Metadata with descriptions (used for generation) |
| Proto docs | `~/.cache/mcc-gaql/proto_docs_v23.json` | Parsed proto documentation |
| LanceDB | `~/.cache/mcc-gaql/lancedb/` | Vector embeddings for RAG search |
| Scraped docs | `~/.config/mcc-gaql/scraped_docs.json` | Legacy: web-scraped documentation (deprecated) |

---

## Proto Parsing vs Web Scraping

The tool supports two methods for extracting field documentation:

| Approach | Status | Description |
|----------|--------|-------------|
| **Proto Parsing** (Recommended) | Stable | Parses Google's official proto files from `googleads-rs` crate - authoritative source |
| **Web Scraping** | Deprecated | Scrapes HTML documentation from Google's developer website - unreliable |

### Proto Parsing (Recommended)

**Advantages:**
- Authoritative source (Google's official proto definitions)
- Complete field documentation with types and behaviors
- Extracts enum value descriptions
- Fast parsing (<30 seconds for all fields)
- No network requests required
- Works offline

**Use for:**
- Production metadata preparation
- Complete field and enum documentation
- Fast, reliable cache building

### Web Scraping (Deprecated)

**Deprecated due to:**
- HTML structure changes breaking the scraper
- Rate limiting and CAPTCHAs
- Incomplete field coverage

**Only use if:** You need documentation that's not in proto files (rare).

---

## Quick Reference

```bash
# Full rebuild from scratch (after API upgrade)
mcc-gaql --refresh-field-cache
mcc-gaql-gen parse-protos
mcc-gaql-gen enrich
mcc-gaql-gen index

# Publish (repo owner only for default URL)
mcc-gaql-gen publish

# Or for custom bucket
export MCC_GAQL_R2_ENDPOINT_URL="..."
export MCC_GAQL_R2_BUCKET="..."
mcc-gaql-gen publish
```

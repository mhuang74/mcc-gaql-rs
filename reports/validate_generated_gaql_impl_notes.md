# Implementation Notes: `--validate` Option for `mcc-gaql-gen generate`

**Date:** 2026-03-24
**Status:** Complete
**Related Spec:** `specs/option_to_validate_gaql.md`

---

## Overview

Added a `--validate` flag to `mcc-gaql-gen generate` that checks the generated GAQL query against the live Google Ads API before returning. Validation uses `SearchGoogleAdsRequest` with `validate_only: true`, which causes the API to parse and validate the query without executing it — no data is returned and no quota is consumed.

The generated query is always printed to stdout first; validation output goes to stderr so the two streams remain separable in scripts.

Google Ads API validation requires real credentials. There is no public unauthenticated endpoint. This feature reuses `mcc-gaql` credential management (OAuth2 token cache, config profiles) rather than introducing its own.

---

## Files Changed

| File | Action | Description |
|------|--------|-------------|
| `Cargo.toml` | Modified | Added `mcc-gaql = { path = "crates/mcc-gaql" }` to `[workspace.dependencies]` |
| `crates/mcc-gaql-gen/Cargo.toml` | Modified | Added `mcc-gaql = { workspace = true }` to dependencies |
| `crates/mcc-gaql/src/googleads.rs` | Modified | Added `SearchGoogleAdsRequest` import; added `validate_gaql_query()` function |
| `crates/mcc-gaql-gen/src/main.rs` | Modified | Added `--validate`/`--profile` CLI flags; added `run_validation()` helper; updated `cmd_generate()` |

---

## API Details

The non-streaming `SearchGoogleAdsRequest` (proto field #5: `bool validate_only`) is used instead of the streaming `SearchGoogleAdsStreamRequest`. The streaming variant does not support `validate_only`.

```
googleads_rs::google::ads::googleads::v23::services::SearchGoogleAdsRequest {
    customer_id: String,   // required
    query: String,         // required
    validate_only: bool,   // field #5 — set to true
    ..Default::default()   // page_token, page_size, search_settings
}
```

On a valid query the API returns HTTP 200 with an empty result set. On an invalid query it returns a gRPC error with a `QueryError` status and a human-readable message in the details bytes.

---

## New Function: `validate_gaql_query()`

**Location:** `crates/mcc-gaql/src/googleads.rs`

```rust
pub async fn validate_gaql_query(
    api_context: GoogleAdsAPIAccess,
    customer_id: &str,
    query: &str,
) -> Result<()>
```

- Constructs a `GoogleAdsServiceClient` with the existing interceptor (auth headers, dev token, login-customer-id).
- Calls `client.search(SearchGoogleAdsRequest { validate_only: true, .. })`.
- Returns `Ok(())` on HTTP 200.
- On gRPC error, extracts `status.message()` and `status.details()` (ASCII-sanitized), formats a combined error message, and returns `Err(anyhow::Error)`.

---

## New Function: `run_validation()`

**Location:** `crates/mcc-gaql-gen/src/main.rs`

Internal helper called from `cmd_generate()` when `--validate` is set. Handles all credential resolution and delegates to `validate_gaql_query()`.

**Profile resolution logic:**
1. If `--profile` was supplied, use it directly.
2. If not, call `mcc_gaql::config::list_profiles()`:
   - 0 profiles → config error (exit 2)
   - 1 profile → use it automatically
   - 2+ profiles → config error listing available profiles (exit 2)

**Credential resolution:**
1. Load `MyConfig` via `mcc_gaql::config::load(&profile_name)`.
2. Resolve `token_cache_filename`: explicit field from config → auto-generated from `user_email` via `generate_token_cache_filename()` → error if neither available.
3. Check the token cache file exists via `mcc_gaql_common::paths::config_file_path()`. If missing, fail with a message to run `mcc-gaql --setup`.
4. Resolve `mcc_customer_id`: `mcc_id` field → fallback to `customer_id` field → error if neither.
5. Build `ApiAccessConfig { use_remote_auth: false, .. }` and call `mcc_gaql::googleads::get_api_access()`.
6. Call `validate_gaql_query(access, &mcc_customer_id, query)`.

**Error sentinel pattern:** Config/auth errors are returned as `Err` with messages prefixed `"__config_error__:"`. The caller in `cmd_generate()` inspects this prefix to decide the exit code (2 for config errors, 1 for invalid queries), avoiding the need for a custom error enum.

---

## CLI Changes

Two new optional flags added to `Commands::Generate` in `mcc-gaql-gen/src/main.rs`:

```
--validate          Validate the generated query against Google Ads API (requires credentials)
--profile <NAME>    Profile to use for validation credentials (auto-detected if only one profile exists)
```

Both flags are ignored when `--validate` is absent, so existing behaviour is unchanged.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Query generated (and validated, if `--validate`) successfully |
| 1 | Query generated but Google Ads API rejected it as invalid |
| 2 | Validation could not run due to missing/misconfigured credentials |

Exit codes 1 and 2 are set via `std::process::exit()` immediately after printing the diagnostic to stderr. The query has already been printed to stdout at that point.

---

## Usage Examples

```bash
# Generate only (unchanged behaviour)
mcc-gaql-gen generate "clicks and impressions by campaign last 30 days"

# Validate with auto-detected profile (requires exactly one profile in config.toml)
mcc-gaql-gen generate "clicks and impressions by campaign last 30 days" --validate

# Validate with explicit profile
mcc-gaql-gen generate "clicks and impressions by campaign last 30 days" --validate --profile myprofile

# Invalid query — exits 1, prints FAILED to stderr
mcc-gaql-gen generate "select nonexistent_field from campaign" --validate

# No token cache — exits 2, prints setup instructions to stderr
mcc-gaql-gen generate "..." --validate --profile myprofile
# stderr: Validation error: Token cache 'tokencache_...' not found. Run 'mcc-gaql --setup' first.
```

---

## Verification

- `cargo check --workspace` passes cleanly with no warnings related to this change.
- The `mcc-gaql` crate is used as a library dependency only; no circular dependency exists (`mcc-gaql` does not depend on `mcc-gaql-gen`).

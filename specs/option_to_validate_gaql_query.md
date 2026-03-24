# Plan: Add `--validate` Option to `mcc-gaql`

## Context

`mcc-gaql-gen generate --validate` validates LLM-generated GAQL against the Google Ads API. This plan adds the same capability directly to `mcc-gaql`, so users can validate any arbitrary GAQL query — hand-written, stored, or piped in — without having to run it.

**Key reuse**: `validate_gaql_query()` already exists in `crates/mcc-gaql/src/googleads.rs`. The entire backend is in place; only the CLI plumbing and invocation path need to be added.

---

## Scope of Changes

Only `crates/mcc-gaql/src/` files are touched. No new crate dependencies needed.

### 1. `crates/mcc-gaql/src/args.rs`

Add a new flag to `Cli`:

```rust
/// Validate the query against Google Ads API without executing it (requires credentials)
#[clap(long)]
pub validate: bool,
```

Update `parse()`: the stdin-read guard currently skips reading stdin when no query-related flags are set. `--validate` alone (with stdin input) should still trigger stdin read — no change needed there since `gaql_query` will be `None` until stdin is read.

Update `Cli::validate()`: add a check that `--validate` requires a query to be present (either `gaql_query`, `stored_query`, or stdin). If `--validate` is set without any query source, return a helpful error rather than silently doing nothing.

### 2. `crates/mcc-gaql/src/main.rs`

Add a new early-exit branch after the existing query resolution block (after `args.gaql_query` is populated from `stored_query` or stdin, but before the `PARAMETERS omit_unselected_resource_names` injection). This preserves validate-only mode as a distinct path that never runs data queries.

**Placement**: after the stored query load (line ~236) and after stdin is resolved (done in `args::parse()`), but before the `PARAMETERS` injection that mutates the query string.

**Logic**:

```
if args.validate && args.gaql_query.is_some() {
    let query = args.gaql_query.as_deref().unwrap();
    // api_context already obtained above
    match googleads::validate_gaql_query(api_context, &mcc_customer_id, query).await {
        Ok(()) => {
            eprintln!("Validation PASSED");
            // exit 0 (fall through to Ok(()))
        }
        Err(e) => {
            eprintln!("Validation FAILED: {e}");
            process::exit(1);
        }
    }
    return Ok(());
}
```

The `api_context` is already established earlier in `main()` via `get_api_access()`, so no second auth call is needed.

**Note on query mutation**: the `PARAMETERS omit_unselected_resource_names = true` injection happens just before API execution. `--validate` should validate the query as-supplied by the user, not the mutated version. Place the validate branch before that injection so the user's original query string is validated.

---

## Behaviour

| Invocation | What happens |
|---|---|
| `mcc-gaql "SELECT ..." --validate` | Validates query, prints PASSED/FAILED to stderr, exits 0/1. No data returned. |
| `echo "SELECT ..." \| mcc-gaql --validate` | Same via stdin. |
| `mcc-gaql -q myquery --validate` | Loads stored query, then validates it. |
| `mcc-gaql "SELECT ..." --validate --profile p` | Uses named profile for credentials. |
| `mcc-gaql --validate` (no query) | Error: `--validate` requires a query. |
| `mcc-gaql "SELECT ..."` (no --validate) | Unchanged existing behaviour. |

- Query is **not** printed to stdout (unlike `mcc-gaql-gen`, there is no generated artifact to surface).
- Validation result goes to **stderr** to keep it scriptable.
- Exit codes: `0` = valid, `1` = invalid query (API rejected), `2` = auth/config error (propagated naturally via `anyhow`/`process::exit` in existing auth error handling).

---

## Key Details

1. **No new dependency**: `validate_gaql_query()` is already in `googleads.rs` and already imported in `main.rs` via `use mcc_gaql::googleads`.
2. **Auth path unchanged**: `--validate` goes through the same `get_api_access()` flow as a normal query. Token cache errors and OAuth prompts behave identically.
3. **Profile auto-detection not needed**: `mcc-gaql` already handles profile resolution in `ResolvedConfig`. The existing `--profile` flag covers explicit profile selection; without it, CLI args or env vars drive config as usual.
4. **`PARAMETERS` injection skipped**: validate branch fires before the `omit_unselected_resource_names` mutation so the query validated is the one the user wrote.
5. **No output format / groupby / sortby interaction**: validate exits before any of that code runs.

---

## Verification

1. `cargo check --workspace` — no new warnings.
2. `mcc-gaql "SELECT campaign.id FROM campaign" --validate` — expect `Validation PASSED` on stderr, exit 0.
3. `mcc-gaql "SELECT nonexistent_field FROM campaign" --validate` — expect `Validation FAILED: ...` on stderr, exit 1.
4. `echo "SELECT campaign.id FROM campaign" | mcc-gaql --validate` — stdin path works.
5. `mcc-gaql -q stored_query_name --validate` — stored query path works.
6. `mcc-gaql --validate` (no query) — clear error, no panic.
7. Existing query execution (`mcc-gaql "SELECT ..."` without `--validate`) — unchanged.

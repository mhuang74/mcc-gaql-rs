# Implementation Notes: Auto-select Last Profile for Account-Agnostic Operations

**Date:** 2026-03-24
**Status:** Complete

---

## Overview

When `--validate`, `--field-service`, or field metadata operations are used without `--profile`, both `mcc-gaql` and `mcc-gaql-gen` now automatically select the **last** profile found in the config file rather than erroring or producing no config.

Auto-selection is intentionally restricted to operations that are **account-agnostic** — i.e., operations that authenticate with the API but do not query a specific customer account. Regular GAQL queries continue to require an explicit `--profile` to avoid silent misattribution of account data.

---

## Rationale

The error `Multiple profiles found: default, dod, lovesetmatch, proteiosic, themade. Specify one with --profile.` was surfacing during `--validate` runs in `mcc-gaql-gen`. Since validation uses `validate_only: true` and is not account-specific, any valid set of credentials will work. Requiring `--profile` in this context was unnecessary friction.

The same logic applies to `mcc-gaql --validate` and `mcc-gaql --field-service`, as well as field metadata operations (`--show-fields`, `--refresh-field-cache`, `--export-field-metadata`, `--show-resources`), which authenticate to the API but do not depend on which customer account is being queried.

---

## Files Changed

### `crates/mcc-gaql-gen/src/main.rs`

In the profile resolution block (around line 918), the `_ =>` match arm that previously returned an error for multiple profiles now selects the last profile:

```rust
// Before:
_ => {
    return Err(anyhow::anyhow!(
        "__config_error__:Multiple profiles found: {}. Specify one with --profile.",
        profiles.join(", ")
    ));
}

// After:
_ => {
    let profile = profiles.last().unwrap().clone();
    eprintln!("Using profile '{}'", profile);
    profile
}
```

This applies unconditionally in `mcc-gaql-gen` because its `generate --validate` is the only operation requiring a profile.

### `crates/mcc-gaql/src/main.rs`

Two code paths updated:

**1. Field metadata operations path (~line 74)**

Always auto-selects last profile when none is specified, since all operations in this block (`--show-fields`, `--refresh-field-cache`, `--export-field-metadata`, `--show-resources`) are account-agnostic:

```rust
let config = if let Some(profile) = &args.profile {
    Some(config::load(profile).context(...)?)
} else if let Ok(profiles) = config::list_profiles() {
    if let Some(profile) = profiles.last() {
        eprintln!("Using profile '{}'", profile);
        Some(config::load(profile).context(...)?)
    } else {
        None
    }
} else {
    None
};
```

**2. Main query path (~line 187)**

Auto-selects last profile only when `args.validate || args.field_service` — both account-agnostic operations. Regular GAQL queries and account listing remain unaffected:

```rust
let config = if let Some(profile) = &args.profile {
    Some(config::load(profile).context(...)?)
} else if args.validate || args.field_service {
    if let Ok(profiles) = config::list_profiles() {
        if let Some(profile) = profiles.last() {
            eprintln!("Using profile '{}'", profile);
            log::info!("Auto-selected profile: {profile}");
            Some(config::load(profile).context(...)?)
        } else {
            None
        }
    } else {
        None
    }
} else {
    log::info!("No profile specified, using CLI arguments only");
    None
};
```

---

## Profile Ordering

Profiles are returned by `config::list_profiles()` in TOML insertion order (backed by `toml::map::Map`, which preserves order). "Last" means the last profile defined in the config file.

---

## Operations That Auto-Select vs. Require Explicit Profile

| Operation | Auto-selects last profile? |
|---|---|
| `mcc-gaql-gen generate --validate` | Yes |
| `mcc-gaql --validate` | Yes |
| `mcc-gaql --field-service` | Yes |
| `mcc-gaql --show-fields` | Yes |
| `mcc-gaql --refresh-field-cache` | Yes |
| `mcc-gaql --export-field-metadata` | Yes |
| `mcc-gaql --show-resources` | Yes |
| `mcc-gaql <gaql-query>` | No — requires explicit `--profile` |
| `mcc-gaql --list-child-accounts` | No — requires explicit `--profile` |

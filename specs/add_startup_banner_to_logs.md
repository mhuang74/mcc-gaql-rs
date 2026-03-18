# Plan: Add Startup Banner with Build Info to mcc-gaql and mcc-gaql-gen

## Context

Both `mcc-gaql` (query tool) and `mcc-gaql-gen` (LLM/RAG tool) need startup banners printed to the logs displaying:
- Tool version
- Git commit hash
- Build time

Currently:
- `mcc-gaql` has `build.rs` that generates `GIT_HASH`, and `VERSION` static in `args.rs`. No banner printed to logs.
- `mcc-gaql-gen` has no `build.rs`, no git hash or build time tracking.

## Files to Modify

### mcc-gaql (query tool)

#### 1. `crates/mcc-gaql/build.rs`
- Add `BUILD_TIME` environment variable generation (ISO 8601 UTC timestamp)

#### 2. `crates/mcc-gaql/src/args.rs`
- Update `VERSION` static to include `BUILD_TIME`

#### 3. `crates/mcc-gaql/src/main.rs`
- Add `print_startup_banner()` function using `log::info!()` macro
- Call `print_startup_banner()` early in `main()` after `init_logger()`

### mcc-gaql-gen (LLM/RAG tool)

#### 1. `crates/mcc-gaql-gen/build.rs` (NEW)
- Generate `GIT_HASH` environment variable
- Generate `BUILD_TIME` environment variable
- Add rerun directives for git changes

#### 2. `crates/mcc-gaql-gen/src/main.rs`
- Add `print_startup_banner()` function using `log::info!()` macro
- Call `print_startup_banner()` early in `main()` after `init_logger()`

## Implementation Details

### build.rs (both crates)

```rust
use std::process::Command;

fn main() {
    // Generate GIT_HASH
    let git_hash = get_git_hash();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    // Generate BUILD_TIME (ISO 8601 UTC)
    let build_time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
}

fn get_git_hash() -> String {
    // Use git rev-parse --short HEAD
    // Append -dirty if has uncommitted changes
    // Fallback to "unknown"
}
```

### args.rs (`mcc-gaql/src/args.rs`)

```rust
static VERSION: LazyLock<String> =
    LazyLock::new(|| format!("{} ({}) built {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"), env!("BUILD_TIME")));
```

### main.rs (both tools)

```rust
fn print_startup_banner(tool_name: &str) {
    let version_info = format!("v{} ({}) built {}", env!("CARGO_PKG_VERSION"), env!("GIT_HASH"), env!("BUILD_TIME"));

    log::info!("═════════════════════════════════════════════════════════════════");
    log::info!("{}", format!(" {} {} ", tool_name, version_info).chars().take(55).collect::<String>());
    log::info!("═════════════════════════════════════════════════════════════════");
}

// In main():
async fn main() -> Result<()> {
    // ... initialization code
    util::init_logger();  // or init_logger(cli.verbose)
    print_startup_banner("mcc-gaql");  // or "mcc-gaql-gen"
    // ... rest of main
}
```

## Existing Resources to Reuse

- `crates/mcc-gaql/build.rs` - Pattern for git hash generation (copy to mcc-gaql-gen, add BUILD_TIME)
- `crates/mcc-gaql/src/args.rs` - VERSION static pattern
- `chrono` crate - Already in workspace dependencies for BUILD_TIME formatting
- `flexi_logger` - Already configured in both tools

## Verification

Build both crates:

```bash
cargo build -p mcc-gaql
cargo build -p mcc-gaql-gen
```

Test with verbose mode to see log output:

```bash
# mcc-gaql
cargo run -p mcc-gaql -- --verbose --help

# mcc-gaql-gen
cargo run -p mcc-gaql-gen -- --verbose generate "show campaigns"
```

Check log files (in `~/Library/Caches/mcc-gaql/` or `<log_dir>`) for banner:
- Should show version, git hash, and build time
- Banner should appear at start of log file

Verify git hash format:
- Clean repo: shows short hash e.g., `b3e6232`
- Dirty repo: shows `b3e6232-dirty`
- No git available: shows `unknown`

Verify build time format:
- ISO 8601 UTC format: `2025-03-19T14:30:00Z`

use flexi_logger::{Cleanup, Criterion, Duplicate, FileSpec, Logger, Naming};

/// Initialize logger for any binary in the workspace.
///
/// Parameters:
/// - `crate_prefix`: Environment variable prefix (always "MCC_GAQL" across all crates)
/// - `verbose`: Enable debug-level logging
pub fn init_logger(crate_prefix: &str, verbose: bool) {
    let base_level = if verbose {
        "debug".to_string()
    } else {
        std::env::var(format!("{}_LOG_LEVEL", crate_prefix)).unwrap_or_else(|_| "off".to_string())
    };

    let my_log_dir =
        std::env::var(format!("{}_LOG_DIR", crate_prefix)).unwrap_or_else(|_| ".".to_string());

    let log_spec = base_level.to_string();

    Logger::try_with_env_or_str(log_spec)
        .unwrap()
        .use_utc()
        .log_to_file(
            FileSpec::default()
                .directory(my_log_dir)
                .suppress_timestamp()
                .basename(crate_prefix.to_lowercase().replace("_", "-")),
        )
        .format_for_files(flexi_logger::detailed_format)
        .o_append(true)
        .rotate(
            Criterion::Size(1_000_000),
            Naming::Numbers,
            Cleanup::KeepLogAndCompressedFiles(10, 100),
        )
        .duplicate_to_stderr(Duplicate::Warn)
        .start()
        .unwrap();
}

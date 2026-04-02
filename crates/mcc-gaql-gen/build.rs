use std::process::Command;

fn main() {
    // Generate GIT_HASH
    let git_hash = get_git_hash();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");
    println!("cargo:rerun-if-changed=../../resources/domain_knowledge.md");

    // Generate BUILD_TIME (ISO 8601 UTC)
    let build_time = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
}

/// Get the short git hash, with optional -dirty suffix
fn get_git_hash() -> String {
    // Get the git hash
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output();

    let git_hash = match output {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        }
        _ => "unknown".to_string(),
    };

    // Check for uncommitted changes
    let dirty_output = Command::new("git").args(["status", "--porcelain"]).output();

    let is_dirty = match dirty_output {
        Ok(output) => !output.stdout.is_empty(),
        _ => false,
    };

    if is_dirty {
        format!("{}-dirty", git_hash)
    } else {
        git_hash
    }
}

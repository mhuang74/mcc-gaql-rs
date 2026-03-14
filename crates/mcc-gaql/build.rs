use std::env;
use std::process::Command;

fn main() {
    // Tell cargo to rerun if these environment variables change
    println!("cargo:rerun-if-env-changed=MCC_GAQL_EMBED_CLIENT_SECRET");
    println!("cargo:rerun-if-env-changed=MCC_GAQL_DEV_TOKEN");

    // Get the git hash for version info
    let git_hash = get_git_hash();
    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/index");

    // Handle client secret embedding
    if let Ok(client_secret_json) = env::var("MCC_GAQL_EMBED_CLIENT_SECRET") {
        println!(
            "cargo:warning=Embedding OAuth2 credentials from MCC_GAQL_EMBED_CLIENT_SECRET environment variable"
        );
        println!(
            "cargo:rustc-env=MCC_GAQL_EMBED_CLIENT_SECRET={}",
            client_secret_json
        );
    } else {
        println!("cargo:warning=MCC_GAQL_EMBED_CLIENT_SECRET environment variable not set");
        println!(
            "cargo:warning=Binary will require clientsecret.json in config directory at runtime"
        );
        println!(
            "cargo:warning=To embed credentials: set MCC_GAQL_EMBED_CLIENT_SECRET during build"
        );
        println!(
            "cargo:warning=Example: MCC_GAQL_EMBED_CLIENT_SECRET=\"$(cat clientsecret.json)\" cargo build --release"
        );
    }

    // Handle developer token embedding
    if let Ok(dev_token) = env::var("MCC_GAQL_DEV_TOKEN") {
        println!(
            "cargo:warning=Embedding Google Ads Developer Token from MCC_GAQL_DEV_TOKEN environment variable"
        );
        println!("cargo:rustc-env=MCC_GAQL_DEV_TOKEN={}", dev_token);
    } else {
        println!("cargo:warning=MCC_GAQL_DEV_TOKEN environment variable not set");
        println!(
            "cargo:warning=Binary will require dev_token in config file or MCC_GAQL_DEV_TOKEN env var at runtime"
        );
        println!("cargo:warning=To embed dev token: set MCC_GAQL_DEV_TOKEN during build");
        println!("cargo:warning=Example: MCC_GAQL_DEV_TOKEN=\"your-token\" cargo build --release");
    }
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

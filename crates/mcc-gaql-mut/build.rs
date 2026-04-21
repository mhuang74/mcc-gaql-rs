fn main() {
    let mut git_hash = if let Ok(hash) = std::process::Command::new("git")
        .args(&["rev-parse", "--short=8", "HEAD"])
        .output()
    {
        String::from_utf8_lossy(&hash.stdout).trim().to_string()
    } else {
        "unknown".to_string()
    };

    if git_hash.is_empty() {
        git_hash = "unknown".to_string();
    }

    let build_time = chrono::Utc::now()
        .format("%Y-%m-%d %H:%M:%S UTC")
        .to_string();

    println!("cargo:rustc-env=GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=BUILD_TIME={}", build_time);
}

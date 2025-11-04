use std::env;

fn main() {
    // Tell cargo to rerun if this environment variable changes
    println!("cargo:rerun-if-env-changed=MCC_GAQL_EMBED_CLIENT_SECRET");

    // Check if MCC_GAQL_EMBED_CLIENT_SECRET environment variable is set
    if let Ok(client_secret_json) = env::var("MCC_GAQL_EMBED_CLIENT_SECRET") {
        println!("cargo:warning=Embedding OAuth2 credentials from MCC_GAQL_EMBED_CLIENT_SECRET environment variable");

        // Set environment variable that will be picked up by option_env! in the code
        println!("cargo:rustc-env=MCC_GAQL_EMBED_CLIENT_SECRET={}", client_secret_json);
    } else {
        println!("cargo:warning=MCC_GAQL_EMBED_CLIENT_SECRET environment variable not set");
        println!("cargo:warning=Binary will require clientsecret.json in config directory at runtime");
        println!("cargo:warning=To embed credentials: set MCC_GAQL_EMBED_CLIENT_SECRET during build");
        println!("cargo:warning=Example: MCC_GAQL_EMBED_CLIENT_SECRET=\"$(cat clientsecret.json)\" cargo build --release");
    }
}

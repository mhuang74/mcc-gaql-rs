use std::fs;
use std::path::Path;

fn main() {
    // Check if clientsecret.json exists in the project root
    let client_secret_path = Path::new("clientsecret.json");

    if client_secret_path.exists() {
        println!("cargo:warning=Found clientsecret.json - embedding OAuth2 credentials into binary");

        // Read the file content
        match fs::read_to_string(client_secret_path) {
            Ok(content) => {
                // Set environment variable that will be picked up by option_env! in the code
                println!("cargo:rustc-env=MCC_GAQL_EMBED_CLIENT_SECRET={}", content);
                println!("cargo:rerun-if-changed=clientsecret.json");
            }
            Err(e) => {
                println!("cargo:warning=Failed to read clientsecret.json: {}", e);
                println!("cargo:warning=Binary will require clientsecret.json in config directory at runtime");
            }
        }
    } else {
        println!("cargo:warning=clientsecret.json not found in project root");
        println!("cargo:warning=Binary will require clientsecret.json in config directory at runtime");
        println!("cargo:warning=To embed credentials: place clientsecret.json in project root before building");
    }
}

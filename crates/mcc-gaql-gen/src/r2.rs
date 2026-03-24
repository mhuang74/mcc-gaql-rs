// Cloudflare R2 client for uploading and downloading field metadata.
//
// Uses the S3-compatible API via reqwest with HMAC-SHA256 request signing.
// Public objects can be downloaded without credentials; uploads require
// MCC_GAQL_R2_ACCESS_KEY_ID and MCC_GAQL_R2_SECRET_ACCESS_KEY environment variables.

use anyhow::{Context, Result};
use std::path::Path;
use tokio::fs;

/// R2 public bucket ID, required at build time
const R2_PUBLIC_ID: &str = env!("MCC_GAQL_R2_PUBLIC_ID");

/// R2 bucket configuration
struct R2Config {
    endpoint_url: String,
    bucket: String,
    access_key: String,
    secret_key: String,
}

impl R2Config {
    /// Load R2 configuration from environment variables
    fn from_env() -> Result<Self> {
        let endpoint_url = std::env::var("MCC_GAQL_R2_ENDPOINT_URL")
            .context("MCC_GAQL_R2_ENDPOINT_URL must be set")?;
        let bucket =
            std::env::var("MCC_GAQL_R2_BUCKET").unwrap_or_else(|_| "mcc-gaql-metadata".to_string());
        let access_key = std::env::var("MCC_GAQL_R2_ACCESS_KEY_ID")
            .context("MCC_GAQL_R2_ACCESS_KEY_ID must be set for upload")?;
        let secret_key = std::env::var("MCC_GAQL_R2_SECRET_ACCESS_KEY")
            .context("MCC_GAQL_R2_SECRET_ACCESS_KEY must be set for upload")?;

        Ok(Self {
            endpoint_url,
            bucket,
            access_key,
            secret_key,
        })
    }

    /// Returns the S3 endpoint URL for this R2 bucket
    fn endpoint(&self) -> String {
        self.endpoint_url.trim_end_matches('/').to_string()
    }
}

/// Download a file from R2 public URL to a local path.
///
/// This uses the public R2 URL (no authentication required for public buckets).
/// The public URL pattern is: https://pub-<hash>.r2.dev/<object_key>
pub async fn download(public_base_url: &str, object_key: &str, dest_path: &Path) -> Result<()> {
    let url = format!("{}/{}", public_base_url.trim_end_matches('/'), object_key);
    log::info!("Downloading {} to {:?}", url, dest_path);

    let client = reqwest::Client::builder()
        .user_agent("mcc-gaql-gen (metadata downloader)")
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {} failed", url))?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed: HTTP {} for {}", response.status(), url);
    }

    let bytes = response
        .bytes()
        .await
        .context("Failed to read response body")?;

    // Create parent directories if needed
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create destination directory")?;
    }

    fs::write(dest_path, &bytes)
        .await
        .with_context(|| format!("Failed to write to {:?}", dest_path))?;

    log::info!("Downloaded {} bytes to {:?}", bytes.len(), dest_path);
    Ok(())
}

/// Upload a bundle file to R2 and return the public URL.
///
/// Requires MCC_GAQL_R2_ENDPOINT_URL, MCC_GAQL_R2_ACCESS_KEY_ID, and MCC_GAQL_R2_SECRET_ACCESS_KEY environment variables.
pub async fn upload_bundle(local_path: &Path, object_key: &str) -> Result<String> {
    upload(object_key, local_path).await?;

    // Construct public URL
    let public_url = format!("https://pub-{}.r2.dev/{}", R2_PUBLIC_ID, object_key);

    Ok(public_url)
}

/// Download a bundle from a public URL (no auth required)
pub async fn download_bundle(url: &str, dest_path: &Path) -> Result<()> {
    log::info!("Downloading bundle from {} to {:?}", url, dest_path);

    // Handle file:// URLs for local testing
    if url.starts_with("file://") {
        let source_path = url.trim_start_matches("file://");
        tokio::fs::copy(source_path, dest_path)
            .await
            .with_context(|| format!("Failed to copy from {}", source_path))?;
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .user_agent("mcc-gaql-gen (bundle downloader)")
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("Failed to download bundle from {}", url))?;

    if !response.status().is_success() {
        anyhow::bail!("Download failed: HTTP {} for {}", response.status(), url);
    }

    let bytes = response
        .bytes()
        .await
        .context("Failed to read response body")?;

    // Create parent directories if needed
    if let Some(parent) = dest_path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("Failed to create destination directory")?;
    }

    fs::write(dest_path, &bytes)
        .await
        .with_context(|| format!("Failed to write to {:?}", dest_path))?;

    log::info!("Downloaded {} bytes to {:?}", bytes.len(), dest_path);
    Ok(())
}

/// Upload a local file to R2 using the S3-compatible API with AWS Signature Version 4.
///
/// Requires MCC_GAQL_R2_ENDPOINT_URL, MCC_GAQL_R2_ACCESS_KEY_ID, and MCC_GAQL_R2_SECRET_ACCESS_KEY environment variables.
pub async fn upload(object_key: &str, source_path: &Path) -> Result<()> {
    let config = R2Config::from_env()?;

    let contents = fs::read(source_path)
        .await
        .with_context(|| format!("Failed to read {:?}", source_path))?;

    let content_type = if object_key.ends_with(".json") {
        "application/json"
    } else {
        "application/octet-stream"
    };

    let url = format!("{}/{}/{}", config.endpoint(), config.bucket, object_key);

    log::info!(
        "Uploading {:?} ({} bytes) to {}",
        source_path,
        contents.len(),
        url
    );

    // Build AWS Signature Version 4 signed request
    let signed_request = sign_s3_request(
        "PUT",
        &config.endpoint(),
        &config.bucket,
        object_key,
        &contents,
        content_type,
        &config.access_key,
        &config.secret_key,
    )?;

    let client = reqwest::Client::builder()
        .user_agent("mcc-gaql-gen (metadata uploader)")
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .context("Failed to build HTTP client")?;

    let response = client
        .put(&url)
        .headers(signed_request.headers)
        .body(contents)
        .send()
        .await
        .with_context(|| format!("PUT {} failed", url))?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("Upload failed: HTTP {} - {}", status, body);
    }

    log::info!("Successfully uploaded to {}", url);
    Ok(())
}

/// Signed request components for S3 API calls
struct SignedRequest {
    headers: reqwest::header::HeaderMap,
}

/// Sign an S3 request using AWS Signature Version 4.
///
/// Reference: https://docs.aws.amazon.com/general/latest/gr/sigv4-create-canonical-request.html
#[allow(clippy::too_many_arguments)]
fn sign_s3_request(
    method: &str,
    endpoint: &str,
    bucket: &str,
    key: &str,
    body: &[u8],
    content_type: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<SignedRequest> {
    use chrono::Utc;
    use hmac::{Hmac, Mac};
    use sha2::{Digest, Sha256};
    type HmacSha256 = Hmac<Sha256>;

    let now = Utc::now();
    let date_str = now.format("%Y%m%d").to_string();
    let datetime_str = now.format("%Y%m%dT%H%M%SZ").to_string();

    // Compute payload hash
    let payload_hash = hex::encode(Sha256::digest(body));

    // Extract host from endpoint
    let host = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://");

    // Canonical URI: /{bucket}/{key}
    let canonical_uri = format!("/{}/{}", bucket, key);

    // Canonical headers (sorted alphabetically by header name)
    let canonical_headers = format!(
        "content-type:{}\nhost:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n",
        content_type, host, payload_hash, datetime_str
    );

    // Signed headers
    let signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date";

    // Canonical request
    let canonical_request = format!(
        "{}\n{}\n\n{}\n{}\n{}",
        method, canonical_uri, canonical_headers, signed_headers, payload_hash
    );

    // String to sign
    let region = "auto"; // Cloudflare R2 uses "auto"
    let scope = format!("{}/{}/s3/aws4_request", date_str, region);
    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        datetime_str,
        scope,
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );

    // Signing key
    let sign_key = |key: &[u8], data: &str| -> Vec<u8> {
        let mut mac = HmacSha256::new_from_slice(key).expect("HMAC can take key of any size");
        mac.update(data.as_bytes());
        mac.finalize().into_bytes().to_vec()
    };

    let k_date = sign_key(format!("AWS4{}", secret_key).as_bytes(), &date_str);
    let k_region = sign_key(&k_date, region);
    let k_service = sign_key(&k_region, "s3");
    let k_signing = sign_key(&k_service, "aws4_request");

    let mut mac = HmacSha256::new_from_slice(&k_signing).expect("HMAC can take key of any size");
    mac.update(string_to_sign.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    // Authorization header
    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{},SignedHeaders={},Signature={}",
        access_key, scope, signed_headers, signature
    );

    // Build headers
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        content_type.parse().context("Invalid content-type")?,
    );
    headers.insert(
        reqwest::header::HOST,
        host.parse().context("Invalid host header")?,
    );
    headers.insert(
        "x-amz-content-sha256"
            .parse::<reqwest::header::HeaderName>()
            .unwrap(),
        payload_hash.parse().context("Invalid payload hash")?,
    );
    headers.insert(
        "x-amz-date".parse::<reqwest::header::HeaderName>().unwrap(),
        datetime_str.parse().context("Invalid datetime string")?,
    );
    headers.insert(
        reqwest::header::AUTHORIZATION,
        authorization
            .parse()
            .context("Invalid authorization header")?,
    );

    Ok(SignedRequest { headers })
}

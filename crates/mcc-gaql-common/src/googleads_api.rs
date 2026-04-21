use std::time::Duration;
use anyhow::{Context, Result, bail};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tonic::{
    Status,
    codegen::InterceptedService,
    metadata::{Ascii, MetadataValue},
    service::Interceptor,
    transport::Channel,
};
use yup_oauth2::{
    AccessToken, ApplicationSecret, InstalledFlowAuthenticator, InstalledFlowReturnMethod,
    authenticator::{Authenticator, DefaultHyperClient, HyperClientBuilder},
};

use crate::paths::config_file_path;

const ENDPOINT: &str = "https://googleads.googleapis.com:443";

// Developer Token configuration with priority order:
// 1. Config: Pass via dev_token parameter (from config file)
// 2. Runtime: Check MCC_GAQL_DEV_TOKEN env var at runtime

const FILENAME_CLIENT_SECRET: &str = "clientsecret.json";
static GOOGLE_ADS_API_SCOPE: &str = "https://www.googleapis.com/auth/adwords";

// Client secret configuration with priority order:
// 1. Runtime: Check MCC_GAQL_EMBED_CLIENT_SECRET env var at runtime
// 2. File: Load from clientsecret.json in config directory

#[derive(Clone)]
pub struct GoogleAdsAPIAccess {
    pub channel: Channel,
    pub dev_token: MetadataValue<Ascii>,
    pub login_customer: MetadataValue<Ascii>,
    pub auth_token: Option<MetadataValue<Ascii>>,
    pub token: Option<AccessToken>,
    pub authenticator: Authenticator<<DefaultHyperClient as HyperClientBuilder>::Connector>,
    #[allow(dead_code)]
    pub user_email: Option<String>,
}

impl GoogleAdsAPIAccess {
    /// Renews Access Token if none exists or if almost expired
    /// returns True if token renewed
    pub async fn renew_token(&mut self) -> Result<bool> {
        let mut renewed: bool = false;
        if self.token.is_none() || self.token.as_ref().unwrap().is_expired() {
            self.token = match self
                .authenticator
                .force_refreshed_token(&[GOOGLE_ADS_API_SCOPE])
                .await
            {
                Err(e) => {
                    bail!("failed to get access token: {:?}", e);
                }
                Ok(t) => {
                    log::debug!("Obtained access token: {t:?}");

                    let bearer_token = format!("Bearer {}", t.as_str());
                    let header_value_auth_token = MetadataValue::try_from(&bearer_token)?;
                    self.auth_token = Some(header_value_auth_token);

                    renewed = true;
                    Some(t)
                }
            };
        }
        Ok(renewed)
    }
}

impl Interceptor for GoogleAdsAPIAccess {
    fn call(&mut self, mut request: tonic::Request<()>) -> Result<tonic::Request<()>, Status> {
        request
            .metadata_mut()
            .insert("authorization", self.auth_token.as_ref().unwrap().clone());
        request
            .metadata_mut()
            .insert("developer-token", self.dev_token.clone());
        request
            .metadata_mut()
            .insert("login-customer-id", self.login_customer.clone());

        Ok(request)
    }
}

/// Generate token cache filename from user email
/// Sanitizes email by replacing @ with _at_ and . with _
/// Example: user@example.com -> tokencache_user_at_example_com.json
pub fn generate_token_cache_filename(user_email: &str) -> String {
    let sanitized = user_email.replace('@', "_at_").replace('.', "_");
    format!("tokencache_{}.json", sanitized)
}

/// Get developer token with priority order:
/// 1. Provided parameter (from config file)
/// 2. Runtime environment variable MCC_GAQL_DEV_TOKEN
fn get_dev_token(config_token: Option<&str>) -> Result<String> {
    if let Some(token) = config_token {
        log::debug!("Using developer token from config");
        return Ok(token.to_string());
    }

    if let Ok(token) = std::env::var("MCC_GAQL_DEV_TOKEN") {
        log::debug!("Using developer token from runtime environment variable");
        return Ok(token);
    }

    bail!(
        "Google Ads Developer Token required but not found. Provide via:\n  \
         1. Config file: Add 'dev_token = \"YOUR_TOKEN\"' to your profile\n  \
         2. Runtime env: export MCC_GAQL_DEV_TOKEN=\"YOUR_TOKEN\"\n\n  \
         Get your developer token at:\n  \
         https://developers.google.com/google-ads/api/docs/get-started/dev-token"
    )
}

/// Get client secret from runtime env var or file
async fn get_client_secret() -> Result<ApplicationSecret> {
    let app_secret: ApplicationSecret = if let Ok(runtime_json) =
        std::env::var("MCC_GAQL_EMBED_CLIENT_SECRET")
    {
        log::debug!("Using client secret from runtime environment variable");
        yup_oauth2::parse_application_secret(&runtime_json)
            .context("Failed to parse client secret from MCC_GAQL_EMBED_CLIENT_SECRET env var")?
    } else {
        log::debug!("Loading client secret from file");
        let client_secret_path = config_file_path(FILENAME_CLIENT_SECRET)
            .context("Failed to determine client secret path")?;
        yup_oauth2::read_application_secret(client_secret_path.as_path())
            .await
            .context("clientsecret.json file not found. Provide via MCC_GAQL_EMBED_CLIENT_SECRET env var or place clientsecret.json in config directory")?
    };

    Ok(app_secret)
}

/// Configuration for Google Ads API access
pub struct ApiAccessConfig {
    pub mcc_customer_id: String,
    pub token_cache_filename: String,
    pub user_email: Option<String>,
    pub dev_token: Option<String>,
    pub use_remote_auth: bool,
}

/// Get access to Google Ads API via OAuth2 flow and return API Credentials
pub async fn get_api_access(config: &ApiAccessConfig) -> Result<GoogleAdsAPIAccess> {
    let app_secret = get_client_secret().await?;

    let token_cache_path = config_file_path(&config.token_cache_filename)
        .context("Failed to determine token cache path")?;

    let cache_existed_prior = token_cache_path.exists();

    let auth_method = if config.use_remote_auth {
        log::info!("Using remote OAuth flow (interactive)");
        InstalledFlowReturnMethod::Interactive
    } else {
        log::debug!("Using standard OAuth flow (HTTP redirect)");
        InstalledFlowReturnMethod::HTTPRedirect
    };

    let auth: Authenticator<<DefaultHyperClient as HyperClientBuilder>::Connector> =
        InstalledFlowAuthenticator::builder(app_secret, auth_method)
            .persist_tokens_to_disk(token_cache_path.as_path())
            .build()
            .await?;

    let dev_token_value = get_dev_token(config.dev_token.as_deref())?;
    let header_value_dev_token = MetadataValue::try_from(&dev_token_value)?;
    let header_value_login_customer = MetadataValue::try_from(&config.mcc_customer_id)?;

    let tls_config = tonic::transport::ClientTlsConfig::new().with_native_roots();

    let channel: Channel = Channel::from_static(ENDPOINT)
        .tls_config(tls_config)?
        .rate_limit(100, Duration::from_secs(1))
        .concurrency_limit(100)
        .connect()
        .await?;

    let mut access = GoogleAdsAPIAccess {
        channel,
        dev_token: header_value_dev_token,
        login_customer: header_value_login_customer,
        auth_token: None,
        token: None,
        authenticator: auth,
        user_email: config.user_email.clone(),
    };

    access.renew_token().await?;

    if config.use_remote_auth && !cache_existed_prior {
        verify_and_confirm_auth(&access, &token_cache_path).await?;
    }

    Ok(access)
}

/// Verify authentication and prompt user for confirmation before saving tokens
async fn verify_and_confirm_auth(
    access: &GoogleAdsAPIAccess,
    token_cache_path: &std::path::Path,
) -> Result<()> {
    let user_email = access.user_email.as_deref().unwrap_or("(unknown)");

    println!("\nAuthenticated as: {}", user_email);
    println!("Token will be saved to: {}", token_cache_path.display());

    print!("\nSave this authentication? [Y/n] ");
    tokio::io::stdout().flush().await?;

    let mut reader = BufReader::new(tokio::io::stdin());
    let mut input = String::new();
    reader.read_line(&mut input).await?;

    let confirmed = input.trim().to_lowercase();
    if confirmed == "n" || confirmed == "no" {
        if token_cache_path.exists() {
            tokio::fs::remove_file(token_cache_path)
                .await
                .context("Failed to delete token cache after user cancellation")?;
        }
        bail!("Authentication cancelled by user. No tokens were saved.");
    }

    println!("Authentication saved successfully.\n");
    Ok(())
}

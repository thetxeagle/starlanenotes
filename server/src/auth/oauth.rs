use oauth2::{
    basic::BasicClient, reqwest::async_http_client, AuthUrl, AuthorizationCode, ClientId,
    ClientSecret, CsrfToken, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse, TokenUrl,
};
use reqwest::Client as HttpClient;
use serde::Deserialize;
use std::env;

// ============================================================
// Provider Configuration
// ============================================================

#[derive(Debug, Clone)]
pub enum OAuthProvider {
    Google,
    GitHub,
}

impl OAuthProvider {
    pub fn is_configured(&self) -> bool {
        match self {
            OAuthProvider::Google => {
                env::var("GOOGLE_CLIENT_ID").is_ok() && env::var("GOOGLE_CLIENT_SECRET").is_ok()
            }
            OAuthProvider::GitHub => {
                env::var("GITHUB_CLIENT_ID").is_ok() && env::var("GITHUB_CLIENT_SECRET").is_ok()
            }
        }
    }

    fn client_id(&self) -> Option<String> {
        match self {
            OAuthProvider::Google => env::var("GOOGLE_CLIENT_ID").ok(),
            OAuthProvider::GitHub => env::var("GITHUB_CLIENT_ID").ok(),
        }
    }

    fn client_secret(&self) -> Option<String> {
        match self {
            OAuthProvider::Google => env::var("GOOGLE_CLIENT_SECRET").ok(),
            OAuthProvider::GitHub => env::var("GITHUB_CLIENT_SECRET").ok(),
        }
    }

    fn auth_url(&self) -> &str {
        match self {
            OAuthProvider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            OAuthProvider::GitHub => "https://github.com/login/oauth/authorize",
        }
    }

    fn token_url(&self) -> &str {
        match self {
            OAuthProvider::Google => "https://oauth2.googleapis.com/token",
            OAuthProvider::GitHub => "https://github.com/login/oauth/access_token",
        }
    }

    fn scopes(&self) -> Vec<&str> {
        match self {
            OAuthProvider::Google => vec!["openid", "email", "profile"],
            OAuthProvider::GitHub => vec!["user:email", "read:user"],
        }
    }

    fn userinfo_url(&self) -> &str {
        match self {
            OAuthProvider::Google => "https://www.googleapis.com/oauth2/v3/userinfo",
            OAuthProvider::GitHub => "https://api.github.com/user",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            OAuthProvider::Google => "google",
            OAuthProvider::GitHub => "github",
        }
    }
}

// ============================================================
// User Info from Providers
// ============================================================

#[derive(Debug, Clone)]
pub struct OAuthUserInfo {
    pub provider: String,
    pub provider_user_id: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    sub: String,
    email: String,
    name: Option<String>,
    picture: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubUserInfo {
    id: i64,
    login: String,
    name: Option<String>,
    avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubEmail {
    email: String,
    primary: bool,
    verified: bool,
}

// ============================================================
// OAuth Flow
// ============================================================

pub fn build_client(
    provider: &OAuthProvider,
    base_url: &str,
) -> Result<BasicClient, anyhow::Error> {
    let client_id = provider
        .client_id()
        .ok_or_else(|| anyhow::anyhow!("{} OAuth not configured", provider.name()))?;
    let client_secret = provider
        .client_secret()
        .ok_or_else(|| anyhow::anyhow!("{} OAuth not configured", provider.name()))?;

    let redirect_url = format!(
        "{}/api/v1/auth/oauth/{}/callback",
        base_url,
        provider.name()
    );

    let client = BasicClient::new(
        ClientId::new(client_id),
        Some(ClientSecret::new(client_secret)),
        AuthUrl::new(provider.auth_url().to_string())?,
        Some(TokenUrl::new(provider.token_url().to_string())?),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_url)?);

    Ok(client)
}

/// Generate the authorization URL with PKCE challenge.
/// Returns (auth_url, csrf_token, pkce_verifier).
pub fn get_auth_url(
    provider: &OAuthProvider,
    base_url: &str,
) -> Result<(String, CsrfToken, PkceCodeVerifier), anyhow::Error> {
    let client = build_client(provider, base_url)?;
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);

    for scope in provider.scopes() {
        auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
    }

    let (auth_url, csrf_token) = auth_request.url();

    Ok((auth_url.to_string(), csrf_token, pkce_verifier))
}

/// Exchange the authorization code for tokens and fetch user info.
pub async fn exchange_code(
    provider: &OAuthProvider,
    base_url: &str,
    code: &str,
    pkce_verifier: PkceCodeVerifier,
) -> Result<OAuthUserInfo, anyhow::Error> {
    let client = build_client(provider, base_url)?;

    let token_result = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .set_pkce_verifier(pkce_verifier)
        .request_async(async_http_client)
        .await
        .map_err(|e| anyhow::anyhow!("Token exchange failed: {}", e))?;

    let access_token = token_result.access_token().secret().clone();

    fetch_user_info(provider, &access_token).await
}

async fn fetch_user_info(
    provider: &OAuthProvider,
    access_token: &str,
) -> Result<OAuthUserInfo, anyhow::Error> {
    let http_client = HttpClient::new();

    match provider {
        OAuthProvider::Google => {
            let resp: GoogleUserInfo = http_client
                .get(provider.userinfo_url())
                .bearer_auth(access_token)
                .send()
                .await?
                .json()
                .await?;

            Ok(OAuthUserInfo {
                provider: "google".to_string(),
                provider_user_id: resp.sub,
                email: resp.email,
                display_name: resp.name.unwrap_or_default(),
                avatar_url: resp.picture,
            })
        }
        OAuthProvider::GitHub => {
            let user: GitHubUserInfo = http_client
                .get(provider.userinfo_url())
                .header("User-Agent", "Starlane-Notes")
                .bearer_auth(access_token)
                .send()
                .await?
                .json()
                .await?;

            let emails: Vec<GitHubEmail> = http_client
                .get("https://api.github.com/user/emails")
                .header("User-Agent", "Starlane-Notes")
                .bearer_auth(access_token)
                .send()
                .await?
                .json()
                .await?;

            let email = emails
                .into_iter()
                .find(|e| e.primary && e.verified)
                .map(|e| e.email)
                .ok_or_else(|| anyhow::anyhow!("No verified primary email on GitHub account"))?;

            Ok(OAuthUserInfo {
                provider: "github".to_string(),
                provider_user_id: user.id.to_string(),
                email,
                display_name: user.name.unwrap_or(user.login),
                avatar_url: user.avatar_url,
            })
        }
    }
}

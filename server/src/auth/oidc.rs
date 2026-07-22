use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use openidconnect::{
    core::{CoreClient, CoreProviderMetadata, CoreResponseType},
    reqwest::async_http_client,
    AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken, IssuerUrl, Nonce,
    RedirectUrl, Scope, TokenResponse,
};
use serde_json::Value;
use std::env;

use super::oauth::OAuthUserInfo;

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub enabled: bool,
    pub display_name: String,
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scopes: Vec<String>,
    pub user_claim: String,
    pub role_claim: String,
    pub role_admin_value: String,
    pub auto_provision: bool,
}

impl OidcConfig {
    pub fn from_env() -> Self {
        let scopes = env::var("OIDC_SCOPES")
            .unwrap_or_else(|_| "openid,profile,email".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();

        OidcConfig {
            enabled: env::var("OIDC_ENABLED")
                .map(|v| v == "true")
                .unwrap_or(false),
            display_name: env::var("OIDC_DISPLAY_NAME").unwrap_or_else(|_| "SSO".to_string()),
            issuer_url: env::var("OIDC_ISSUER_URL").unwrap_or_default(),
            client_id: env::var("OIDC_CLIENT_ID").unwrap_or_default(),
            client_secret: env::var("OIDC_CLIENT_SECRET").unwrap_or_default(),
            scopes,
            user_claim: env::var("OIDC_USER_CLAIM").unwrap_or_else(|_| "email".to_string()),
            role_claim: env::var("OIDC_ROLE_CLAIM").unwrap_or_default(),
            role_admin_value: env::var("OIDC_ROLE_ADMIN_VALUE")
                .unwrap_or_else(|_| "admin".to_string()),
            auto_provision: env::var("OIDC_AUTO_PROVISION")
                .map(|v| v == "true")
                .unwrap_or(true),
        }
    }
}

/// Build OIDC client using auto-discovery from the issuer URL
pub async fn build_client(
    config: &OidcConfig,
    base_url: &str,
) -> Result<CoreClient, anyhow::Error> {
    let issuer_url = IssuerUrl::new(config.issuer_url.clone())?;

    let provider_metadata =
        CoreProviderMetadata::discover_async(issuer_url, async_http_client).await?;

    let redirect_url = format!("{}/api/v1/auth/oidc/callback", base_url);

    let client = CoreClient::from_provider_metadata(
        provider_metadata,
        ClientId::new(config.client_id.clone()),
        Some(ClientSecret::new(config.client_secret.clone())),
    )
    .set_redirect_uri(RedirectUrl::new(redirect_url)?);

    Ok(client)
}

/// Generate the authorization URL for OIDC login.
/// Returns (auth_url, csrf_token, nonce).
pub async fn get_auth_url(
    config: &OidcConfig,
    base_url: &str,
) -> Result<(String, CsrfToken, Nonce), anyhow::Error> {
    let client = build_client(config, base_url).await?;

    let mut auth_request = client.authorize_url(
        AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
        CsrfToken::new_random,
        Nonce::new_random,
    );

    for scope in &config.scopes {
        if scope != "openid" {
            auth_request = auth_request.add_scope(Scope::new(scope.clone()));
        }
    }

    let (auth_url, csrf_token, nonce) = auth_request.url();

    Ok((auth_url.to_string(), csrf_token, nonce))
}

/// Exchange the authorization code for tokens and extract user info from claims.
pub async fn exchange_code(
    config: &OidcConfig,
    base_url: &str,
    code: &str,
    nonce: Nonce,
) -> Result<OidcUserInfo, anyhow::Error> {
    let client = build_client(config, base_url).await?;

    let token_response = client
        .exchange_code(AuthorizationCode::new(code.to_string()))
        .request_async(async_http_client)
        .await
        .map_err(|e| anyhow::anyhow!("OIDC token exchange failed: {}", e))?;

    let id_token = token_response
        .id_token()
        .ok_or_else(|| anyhow::anyhow!("No ID token in OIDC response"))?;

    let claims = id_token
        .claims(&client.id_token_verifier(), &nonce)
        .map_err(|e| anyhow::anyhow!("Failed to verify ID token: {}", e))?;

    // `CoreClient` intentionally deserializes only standard claims. Decode the
    // payload only after `claims` has verified the signature, issuer, audience,
    // expiry, and nonce above so configured custom claims remain trustworthy.
    let verified_claims = verified_claim_payload(&id_token.to_string())?;
    let email = extract_string_claim(&verified_claims, &config.user_claim)
        .or_else(|| {
            (config.user_claim == "email")
                .then(|| claims.email().map(|email| email.to_string()))
                .flatten()
        })
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No usable '{}' claim in verified ID token",
                config.user_claim
            )
        })?;

    let display_name = claims
        .name()
        .and_then(|n| n.get(None))
        .map(|n| n.to_string())
        .unwrap_or_else(|| email.clone());

    let avatar_url = claims
        .picture()
        .and_then(|p| p.get(None))
        .map(|p| p.to_string());

    let provider_user_id = claims.subject().to_string();

    let role_claim_values = extract_string_claim_values(&verified_claims, &config.role_claim);

    Ok(OidcUserInfo {
        provider_user_id,
        email,
        display_name,
        avatar_url,
        role_claim_values,
    })
}

#[derive(Debug, Clone)]
pub struct OidcUserInfo {
    pub provider_user_id: String,
    pub email: String,
    pub display_name: String,
    pub avatar_url: Option<String>,
    pub role_claim_values: Vec<String>,
}

impl OidcUserInfo {
    pub fn to_oauth_user_info(&self) -> OAuthUserInfo {
        OAuthUserInfo {
            provider: "oidc".to_string(),
            provider_user_id: self.provider_user_id.clone(),
            email: self.email.clone(),
            display_name: self.display_name.clone(),
            avatar_url: self.avatar_url.clone(),
        }
    }

    pub fn resolve_role(&self, admin_value: &str) -> &str {
        if self
            .role_claim_values
            .iter()
            .any(|role| role == admin_value)
        {
            "admin"
        } else {
            "member"
        }
    }
}

fn verified_claim_payload(id_token: &str) -> Result<Value, anyhow::Error> {
    let payload = id_token
        .split('.')
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Verified ID token has no payload"))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|_| anyhow::anyhow!("Verified ID token payload is not base64url"))?;

    serde_json::from_slice(&bytes)
        .map_err(|_| anyhow::anyhow!("Verified ID token payload is not JSON"))
}

fn claim_at_path<'a>(claims: &'a Value, claim_path: &str) -> Option<&'a Value> {
    if claim_path.trim().is_empty() {
        return None;
    }

    let mut current = claims;

    for part in claim_path.split('.') {
        if part.is_empty() {
            return None;
        }
        current = current.get(part)?;
    }

    Some(current)
}

fn extract_string_claim(claims: &Value, claim_path: &str) -> Option<String> {
    claim_at_path(claims, claim_path)?
        .as_str()
        .map(ToOwned::to_owned)
}

fn extract_string_claim_values(claims: &Value, claim_path: &str) -> Vec<String> {
    match claim_at_path(claims, claim_path) {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| value.as_str().map(ToOwned::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_configured_identity_and_role_claims() {
        let claims = json!({ "realm_access": { "roles": ["member", "admin"] } });
        assert_eq!(
            extract_string_claim_values(&claims, "realm_access.roles"),
            vec!["member", "admin"]
        );
        assert_eq!(
            extract_string_claim(
                &json!({ "preferred_username": "eagle@example.com" }),
                "preferred_username"
            ),
            Some("eagle@example.com".into())
        );
        assert!(extract_string_claim(&claims, "realm_access..roles").is_none());
    }

    #[test]
    fn maps_any_matching_array_role_to_admin() {
        let user = OidcUserInfo {
            provider_user_id: "provider-user".into(),
            email: "eagle@example.com".into(),
            display_name: "Eagle".into(),
            avatar_url: None,
            role_claim_values: vec!["member".into(), "starlane-admin".into()],
        };

        assert_eq!(user.resolve_role("starlane-admin"), "admin");
        assert_eq!(user.resolve_role("owner"), "member");
    }

    #[test]
    fn decodes_jwt_payload_without_requiring_padding() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"email":"eagle@example.com"}"#);
        let claims = verified_claim_payload(&format!("header.{payload}.signature")).unwrap();

        assert_eq!(
            extract_string_claim(&claims, "email"),
            Some("eagle@example.com".into())
        );
    }
}

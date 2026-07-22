use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // user_id
    pub sid: String, // server session id
    pub email: String,
    pub role: String,
    pub exp: i64,
    pub iat: i64,
    pub token_type: TokenType,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Access,
    Refresh,
}

pub struct TokenPair {
    pub session_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: i64,
}

pub fn create_token_pair(
    user_id: &str,
    email: &str,
    role: &str,
    secret: &str,
) -> Result<TokenPair, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let session_id = uuid::Uuid::new_v4().to_string();

    // Access token: 15 minutes
    let access_claims = Claims {
        sub: user_id.to_string(),
        sid: session_id.clone(),
        email: email.to_string(),
        role: role.to_string(),
        exp: (now + Duration::minutes(15)).timestamp(),
        iat: now.timestamp(),
        token_type: TokenType::Access,
    };

    let access_token = encode(
        &Header::default(),
        &access_claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    // Refresh token: 30 days
    let refresh_claims = Claims {
        sub: user_id.to_string(),
        sid: session_id.clone(),
        email: email.to_string(),
        role: role.to_string(),
        exp: (now + Duration::days(30)).timestamp(),
        iat: now.timestamp(),
        token_type: TokenType::Refresh,
    };

    let refresh_token = encode(
        &Header::default(),
        &refresh_claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(TokenPair {
        session_id,
        access_token,
        refresh_token,
        expires_in: 900, // 15 minutes in seconds
    })
}

pub fn validate_token(
    token: &str,
    secret: &str,
    expected_type: TokenType,
) -> Result<Claims, jsonwebtoken::errors::Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.required_spec_claims.insert("exp".to_string());
    validation.required_spec_claims.insert("iat".to_string());
    validation.required_spec_claims.insert("sub".to_string());
    validation.required_spec_claims.insert("sid".to_string());
    validation.required_spec_claims.insert("email".to_string());
    validation.required_spec_claims.insert("role".to_string());
    validation
        .required_spec_claims
        .insert("token_type".to_string());

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    if token_data.claims.token_type != expected_type {
        return Err(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::InvalidToken,
        ));
    }

    Ok(token_data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-with-at-least-thirty-two-characters";

    #[test]
    fn validates_expected_access_token_type() {
        let tokens = create_token_pair("user-1", "user@example.com", "admin", TEST_SECRET)
            .expect("token pair should be created");

        let claims = validate_token(&tokens.access_token, TEST_SECRET, TokenType::Access)
            .expect("access token should validate");

        assert_eq!(claims.sub, "user-1");
        assert!(!claims.sid.is_empty());
        assert_eq!(claims.email, "user@example.com");
        assert_eq!(claims.role, "admin");
        assert_eq!(claims.token_type, TokenType::Access);
        let refresh_claims = validate_token(&tokens.refresh_token, TEST_SECRET, TokenType::Refresh)
            .expect("refresh token should validate");
        assert_eq!(claims.sid, tokens.session_id);
        assert_eq!(refresh_claims.sid, tokens.session_id);
    }

    #[test]
    fn rejects_refresh_token_when_access_token_is_required() {
        let tokens = create_token_pair("user-1", "user@example.com", "admin", TEST_SECRET)
            .expect("token pair should be created");

        let result = validate_token(&tokens.refresh_token, TEST_SECRET, TokenType::Access);

        assert!(result.is_err());
    }

    #[test]
    fn rejects_token_signed_with_different_secret() {
        let tokens = create_token_pair("user-1", "user@example.com", "admin", TEST_SECRET)
            .expect("token pair should be created");

        let result = validate_token(
            &tokens.access_token,
            "different-secret-with-at-least-thirty-two-characters",
            TokenType::Access,
        );

        assert!(result.is_err());
    }

    #[test]
    fn rejects_token_missing_session_id_claim() {
        let now = Utc::now();
        let claims = serde_json::json!({
            "sub": "user-1",
            "email": "user@example.com",
            "role": "admin",
            "exp": (now + Duration::minutes(15)).timestamp(),
            "iat": now.timestamp(),
            "token_type": "access"
        });
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(TEST_SECRET.as_bytes()),
        )
        .expect("token should encode");

        let result = validate_token(&token, TEST_SECRET, TokenType::Access);

        assert!(result.is_err());
    }
}

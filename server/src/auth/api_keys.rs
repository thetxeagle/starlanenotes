use rand::{distributions::Alphanumeric, Rng};
use sha2::{Digest, Sha256};

const API_KEY_PREFIX: &str = "stln_";
const API_KEY_SECRET_LEN: usize = 40;
const API_KEY_VISIBLE_PREFIX_LEN: usize = 12;

pub fn generate_api_key() -> String {
    let secret: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(API_KEY_SECRET_LEN)
        .map(char::from)
        .collect();
    format!("{API_KEY_PREFIX}{secret}")
}

pub fn hash_api_key(api_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(api_key.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn visible_prefix(api_key: &str) -> String {
    api_key
        .chars()
        .take(API_KEY_VISIBLE_PREFIX_LEN)
        .collect::<String>()
}

pub const API_KEY_SCOPES: &[&str] = &["read", "write", "admin"];

pub fn normalize_scopes(scopes: Option<Vec<String>>) -> Result<Vec<String>, String> {
    let mut normalized = Vec::new();
    for scope in scopes.unwrap_or_else(|| vec!["read".to_string(), "write".to_string()]) {
        let scope = scope.trim().to_lowercase();
        if scope.is_empty() {
            continue;
        }
        if !API_KEY_SCOPES.contains(&scope.as_str()) {
            return Err(format!("Unknown API key scope: {}", scope));
        }
        if !normalized.contains(&scope) {
            normalized.push(scope);
        }
    }
    if normalized.is_empty() {
        return Err("At least one API key scope is required".into());
    }
    Ok(normalized)
}

pub fn scope_json(scopes: &[String]) -> Result<String, serde_json::Error> {
    serde_json::to_string(scopes)
}

pub fn parse_scope_json(scopes: Option<&str>) -> Vec<String> {
    let Some(scopes) = scopes else {
        return API_KEY_SCOPES
            .iter()
            .map(|scope| (*scope).to_string())
            .collect();
    };
    // Malformed or unknown stored policy must not manufacture administrator
    // scope. NULL remains the documented legacy user-key compatibility case.
    match serde_json::from_str::<Vec<String>>(scopes) {
        Ok(scopes)
            if scopes
                .iter()
                .all(|scope| API_KEY_SCOPES.contains(&scope.as_str())) =>
        {
            scopes
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keys_have_stable_public_shape() {
        let api_key = generate_api_key();

        assert!(api_key.starts_with(API_KEY_PREFIX));
        assert_eq!(api_key.len(), API_KEY_PREFIX.len() + API_KEY_SECRET_LEN);
        assert_eq!(visible_prefix(&api_key).len(), API_KEY_VISIBLE_PREFIX_LEN);
    }

    #[test]
    fn hashes_are_stable_without_storing_plaintext() {
        let api_key = "stln_example-key";

        assert_eq!(hash_api_key(api_key), hash_api_key(api_key));
        assert_ne!(hash_api_key(api_key), api_key);
    }

    #[test]
    fn scopes_are_normalized_and_validated() {
        let scopes = normalize_scopes(Some(vec![
            "READ".into(),
            "write".into(),
            "read".into(),
            " ".into(),
        ]))
        .expect("valid scopes");

        assert_eq!(scopes, vec!["read", "write"]);
        assert!(normalize_scopes(Some(vec!["root".into()])).is_err());
    }
    #[test]
    fn corrupt_scope_policy_fails_closed_and_valid_legacy_policy_is_preserved() {
        for raw in [
            "{",
            "null",
            "[1]",
            "\"admin\"",
            "[\"admin\",\"unknown\"]",
            "[\"ADMIN\"]",
        ] {
            assert!(parse_scope_json(Some(raw)).is_empty(), "{raw}");
        }
        assert_eq!(
            parse_scope_json(Some("[\"read\",\"write\"]")),
            vec!["read", "write"]
        );
        assert_eq!(parse_scope_json(Some("[\"admin\"]")), vec!["admin"]);
        assert!(parse_scope_json(Some("[]")).is_empty());
        assert_eq!(parse_scope_json(None), vec!["read", "write", "admin"]);
    }
}

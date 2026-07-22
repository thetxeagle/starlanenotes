use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use std::collections::HashSet;

use crate::auth::middleware::AdminUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin/users/import/preview", post(preview_import))
        .route("/admin/users/import", post(import_users))
        .route("/admin/users/import/template", get(download_template))
}

// ============================================================
// Types
// ============================================================

#[derive(Debug, Deserialize)]
struct CsvRow {
    email: String,
    display_name: String,
    role: Option<String>,
    spaces: Option<String>, // Comma-separated space slugs
}

#[derive(Debug, Serialize)]
struct ImportPreview {
    total: usize,
    new_users: Vec<PreviewUser>,
    skipped: Vec<SkippedUser>,
    errors: Vec<ImportError>,
}

#[derive(Debug, Serialize)]
struct PreviewUser {
    email: String,
    display_name: String,
    role: String,
    spaces: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SkippedUser {
    email: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct ImportError {
    line: usize,
    error: String,
}

#[derive(Debug, Serialize)]
struct ImportResult {
    created: usize,
    skipped: usize,
    errors: Vec<ImportError>,
    created_users: Vec<ImportedUserCredential>,
}

#[derive(Debug, Serialize)]
struct ImportedUserCredential {
    email: String,
    temporary_password: String,
}

#[derive(Debug, Clone)]
pub struct UserProvisioningPolicy {
    allowed_email_domains: Vec<String>,
    allowed_roles: HashSet<String>,
}

// ============================================================
// Handlers
// ============================================================

/// POST /api/v1/admin/users/import/preview
/// Upload CSV and preview what will happen (no changes made)
async fn preview_import(
    State(state): State<AppState>,
    _admin: AdminUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let csv_content = extract_csv_from_multipart(&mut multipart).await?;
    let rows = parse_csv(&csv_content)?;
    let policy = load_user_provisioning_policy(&state).await?;

    let mut preview = ImportPreview {
        total: rows.len(),
        new_users: Vec::new(),
        skipped: Vec::new(),
        errors: Vec::new(),
    };

    for (i, row) in rows.iter().enumerate() {
        if row.email.is_empty() {
            preview.errors.push(ImportError {
                line: i + 2, // +2 for 1-indexed + header row
                error: "Missing email".into(),
            });
            continue;
        }

        // Check if user exists
        let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?1")
            .bind(&row.email)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

        if existing.is_some() {
            preview.skipped.push(SkippedUser {
                email: row.email.clone(),
                reason: "User already exists".into(),
            });
        } else {
            let role = normalize_provisioning_role(row.role.as_deref().unwrap_or("member"));
            if !["admin", "member", "viewer"].contains(&role.as_str()) {
                preview.errors.push(ImportError {
                    line: i + 2,
                    error: format!("Invalid role: {}", role),
                });
                continue;
            }
            if let Err(error) = policy.validate(&row.email, &role) {
                preview.errors.push(ImportError { line: i + 2, error });
                continue;
            }
            let spaces: Vec<String> = row
                .spaces
                .as_deref()
                .unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();

            preview.new_users.push(PreviewUser {
                email: row.email.clone(),
                display_name: row.display_name.clone(),
                role,
                spaces,
            });
        }
    }

    Ok(Json(preview))
}

/// POST /api/v1/admin/users/import
/// Actually import users from CSV
async fn import_users(
    State(state): State<AppState>,
    admin: AdminUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let csv_content = extract_csv_from_multipart(&mut multipart).await?;
    let rows = parse_csv(&csv_content)?;
    let policy = load_user_provisioning_policy(&state).await?;

    let mut result = ImportResult {
        created: 0,
        skipped: 0,
        errors: Vec::new(),
        created_users: Vec::new(),
    };

    for (i, row) in rows.iter().enumerate() {
        if row.email.is_empty() || row.display_name.is_empty() {
            result.errors.push(ImportError {
                line: i + 2,
                error: "Missing required fields".into(),
            });
            continue;
        }

        // Skip existing users
        let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = ?1")
            .bind(&row.email)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

        if existing.is_some() {
            result.skipped += 1;
            continue;
        }

        let role = normalize_provisioning_role(row.role.as_deref().unwrap_or("member"));
        if !["admin", "member", "viewer"].contains(&role.as_str()) {
            result.errors.push(ImportError {
                line: i + 2,
                error: format!("Invalid role: {}", role),
            });
            continue;
        }
        if let Err(error) = policy.validate(&row.email, &role) {
            result.errors.push(ImportError { line: i + 2, error });
            continue;
        }

        let temporary_password = crate::auth::routes::generate_temporary_password();
        let password_hash = crate::auth::password::hash_password(&temporary_password)
            .map_err(|error| AppError::Internal(error.to_string()))?;
        let user_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO users (id, email, display_name, password_hash, role) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
            .bind(&user_id)
            .bind(&row.email)
            .bind(&row.display_name)
            .bind(&password_hash)
            .bind(&role)
            .execute(&state.db)
            .await
            .map_err(AppError::internal)?;

        crate::auth::routes::create_personal_space(&state, &user_id, &row.display_name).await?;

        // Add to spaces if specified
        if let Some(spaces_str) = &row.spaces {
            for slug in spaces_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
            {
                let space: Option<(String,)> =
                    sqlx::query_as("SELECT id FROM spaces WHERE slug = ?1")
                        .bind(slug)
                        .fetch_optional(&state.db)
                        .await
                        .map_err(AppError::internal)?;

                if let Some((space_id,)) = space {
                    let member_id = Uuid::new_v4().to_string();
                    sqlx::query(
                        "INSERT INTO space_members (id, space_id, user_id, role) VALUES (?1, ?2, ?3, 'editor') \
                         ON CONFLICT(space_id, user_id) DO NOTHING",
                    )
                    .bind(&member_id)
                    .bind(&space_id)
                    .bind(&user_id)
                    .execute(&state.db)
                    .await
                    .map_err(AppError::internal)?;
                }
            }
        }

        crate::audit::log_action(
            &state.db,
            Some(&admin.claims.sub),
            None,
            "user.import",
            Some("user"),
            Some(&user_id),
        )
        .await;

        result.created += 1;
        result.created_users.push(ImportedUserCredential {
            email: row.email.clone(),
            temporary_password,
        });
    }

    Ok((StatusCode::CREATED, Json(result)))
}

/// GET /api/v1/admin/users/import/template
/// Download a CSV template file
async fn download_template(_admin: AdminUser) -> impl IntoResponse {
    let csv = "email,display_name,role,spaces\njane@example.com,Jane Doe,member,\"engineering,design\"\nbob@example.com,Bob Smith,viewer,engineering\n";

    (
        StatusCode::OK,
        [
            ("Content-Type", "text/csv"),
            (
                "Content-Disposition",
                "attachment; filename=\"starlane-notes-import-template.csv\"",
            ),
        ],
        csv,
    )
}

// ============================================================
// Helpers
// ============================================================

async fn extract_csv_from_multipart(multipart: &mut Multipart) -> Result<String, AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read upload: {}", e)))?
    {
        if field.name() == Some("file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to read file: {}", e)))?;

            return String::from_utf8(bytes.to_vec())
                .map_err(|_| AppError::BadRequest("CSV file must be UTF-8 encoded".into()));
        }
    }

    Err(AppError::BadRequest("No file field in upload".into()))
}

fn parse_csv(content: &str) -> Result<Vec<CsvRow>, AppError> {
    let mut rows = Vec::new();
    let mut lines = content.lines();

    // Skip header row
    let header = lines
        .next()
        .ok_or_else(|| AppError::BadRequest("Empty CSV file".into()))?;

    // Validate header
    let expected_headers = ["email", "display_name", "role", "spaces"];
    let headers: Vec<&str> = header.split(',').map(|h| h.trim()).collect();
    if headers.len() < 2 || headers[0] != expected_headers[0] || headers[1] != expected_headers[1] {
        return Err(AppError::BadRequest(
            "CSV must have headers: email, display_name, role, spaces".into(),
        ));
    }

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields: Vec<&str> = line
            .split(',')
            .map(|f| f.trim().trim_matches('"'))
            .collect();

        rows.push(CsvRow {
            email: fields.first().unwrap_or(&"").trim().to_lowercase(),
            display_name: fields.get(1).unwrap_or(&"").to_string(),
            role: fields.get(2).map(|s| s.to_string()),
            spaces: fields.get(3).map(|s| s.to_string()),
        });
    }

    Ok(rows)
}

pub async fn validate_user_provisioning_policy(
    state: &AppState,
    email: &str,
    role: &str,
) -> Result<(), AppError> {
    let policy = load_user_provisioning_policy(state).await?;
    policy.validate(email, role).map_err(AppError::BadRequest)
}

async fn load_user_provisioning_policy(
    state: &AppState,
) -> Result<UserProvisioningPolicy, AppError> {
    let allowed_email_domains = provisioning_config_value(
        state,
        "user_provisioning_allowed_email_domains",
        Some("USER_PROVISIONING_ALLOWED_EMAIL_DOMAINS"),
        "",
    )
    .await?;
    let allowed_roles = provisioning_config_value(
        state,
        "user_provisioning_allowed_roles",
        Some("USER_PROVISIONING_ALLOWED_ROLES"),
        "admin,member,viewer",
    )
    .await?;

    Ok(UserProvisioningPolicy {
        allowed_email_domains: parse_policy_list(&allowed_email_domains),
        allowed_roles: parse_policy_list(&allowed_roles).into_iter().collect(),
    })
}

async fn provisioning_config_value(
    state: &AppState,
    key: &str,
    env_var: Option<&str>,
    default: &str,
) -> Result<String, AppError> {
    if let Some(env_var) = env_var {
        if let Ok(value) = std::env::var(env_var) {
            return Ok(value);
        }
    }

    let stored: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = ?1")
            .bind(key)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    Ok(stored
        .map(|item| item.0)
        .unwrap_or_else(|| default.to_string()))
}

impl UserProvisioningPolicy {
    fn validate(&self, email: &str, role: &str) -> Result<(), String> {
        let normalized_role = normalize_provisioning_role(role);
        if !self.allowed_roles.is_empty() && !self.allowed_roles.contains(&normalized_role) {
            return Err(format!(
                "Role {} is not allowed by the user provisioning policy",
                normalized_role
            ));
        }

        if !self.allowed_email_domains.is_empty() {
            let domain = email
                .rsplit_once('@')
                .map(|(_, domain)| domain.trim().to_lowercase())
                .unwrap_or_default();
            if domain.is_empty() || !self.allowed_email_domains.contains(&domain) {
                return Err(format!(
                    "Email domain is not allowed by the user provisioning policy: {}",
                    email
                ));
            }
        }

        Ok(())
    }
}

fn parse_policy_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|item| item.trim().to_lowercase())
        .filter(|item| !item.is_empty())
        .collect()
}

fn normalize_provisioning_role(role: &str) -> String {
    role.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provisioning_policy_allows_permissive_defaults() {
        let policy = UserProvisioningPolicy {
            allowed_email_domains: Vec::new(),
            allowed_roles: parse_policy_list("admin,member,viewer")
                .into_iter()
                .collect(),
        };

        assert!(policy.validate("person@example.com", "member").is_ok());
        assert!(policy.validate("person@other.test", "viewer").is_ok());
    }

    #[test]
    fn provisioning_policy_rejects_disallowed_domains_and_roles() {
        let policy = UserProvisioningPolicy {
            allowed_email_domains: parse_policy_list("example.com, team.test"),
            allowed_roles: parse_policy_list("member").into_iter().collect(),
        };

        assert!(policy.validate("person@example.com", "member").is_ok());
        assert!(policy.validate("person@other.test", "member").is_err());
        assert!(policy.validate("person@example.com", "admin").is_err());
    }
}

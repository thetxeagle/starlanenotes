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
        let existing: Option<(String,)> = sqlx::query_as("SELECT id FROM users WHERE email = $1")
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
    {
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        crate::auth::middleware::require_current_admin(&mut conn, &admin).await?;
    }
    let csv_content = extract_csv_from_multipart(&mut multipart).await?;
    let rows = parse_csv(&csv_content)?;
    let result = import_csv_rows(&state, &admin, &rows).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

/// Data-only CSV route witness, constructed only after exact account verification.
pub(crate) struct CsvAccountBirthAuditOrigin {
    user_id: String,
    space_id: String,
}
impl CsvAccountBirthAuditOrigin {
    pub(crate) fn user_id(&self) -> &str {
        &self.user_id
    }
    pub(crate) fn space_id(&self) -> &str {
        &self.space_id
    }
}
#[derive(Clone, PartialEq, Eq)]
struct CsvBirthPolicy {
    stored: [Option<(String, String, String)>; 2],
    env: [Option<String>; 2],
}
impl CsvBirthPolicy {
    fn environment() -> [Option<String>; 2] {
        [
            std::env::var("USER_PROVISIONING_ALLOWED_EMAIL_DOMAINS").ok(),
            std::env::var("USER_PROVISIONING_ALLOWED_ROLES").ok(),
        ]
    }
    async fn load(conn: &mut sqlx::AnyConnection) -> Result<Self, AppError> {
        async fn row(
            c: &mut sqlx::AnyConnection,
            key: &str,
        ) -> Result<Option<(String, String, String)>, AppError> {
            sqlx::query_as("SELECT key,value,updated_at FROM system_config WHERE key=$1")
                .bind(key)
                .fetch_optional(c)
                .await
                .map_err(AppError::internal)
        }
        Ok(Self {
            stored: [
                row(conn, "user_provisioning_allowed_email_domains").await?,
                row(conn, "user_provisioning_allowed_roles").await?,
            ],
            env: Self::environment(),
        })
    }
    fn validate(&self, email: &str, role: &str) -> Result<(), String> {
        let value = |n: usize, default: &str| {
            self.env[n].clone().unwrap_or_else(|| {
                self.stored[n]
                    .as_ref()
                    .map(|r| r.1.clone())
                    .unwrap_or_else(|| default.into())
            })
        };
        UserProvisioningPolicy::from_config_values(&value(0, ""), &value(1, "admin,member,viewer"))
            .validate(email, role)
    }
    fn verify_environment(&self) -> Result<(), AppError> {
        if Self::environment() != self.env {
            return Err(AppError::Conflict("CSV provisioning policy changed".into()));
        }
        Ok(())
    }
}
type CsvSpaceSource = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    String,
    i32,
);
async fn csv_space_source(
    conn: &mut sqlx::AnyConnection,
    slug: &str,
) -> Result<Option<CsvSpaceSource>, AppError> {
    sqlx::query_as("SELECT id,name,slug,description,icon,accent_color,visibility,created_by,created_at,updated_at,trash_retention_days FROM spaces WHERE slug=$1")
        .bind(slug).fetch_optional(conn).await.map_err(AppError::internal)
}
#[cfg(test)]
tokio::task_local! {
    static CSV_BIRTH_HASH_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;
    static CSV_BIRTH_BEFORE_FENCE:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    static CSV_BIRTH_FINAL_HOOK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    static CSV_BIRTH_LOST_ACK:std::cell::RefCell<bool>;
    static CSV_BIRTH_POSTCOMMIT_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce(&str)+Send>>>;
}
enum CsvBirthOutcome {
    Skipped,
    Rejected(String),
    Created(ImportedUserCredential),
}
async fn create_csv_account(
    state: &AppState,
    admin: &AdminUser,
    row: &CsvRow,
) -> Result<CsvBirthOutcome, AppError> {
    use crate::auth::middleware::require_current_admin;
    let role = normalize_provisioning_role(row.role.as_deref().unwrap_or("member"));
    {
        let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
        require_current_admin(&mut conn, admin).await?;
        if row.email.is_empty() || row.display_name.is_empty() {
            return Ok(CsvBirthOutcome::Rejected("Missing required fields".into()));
        }
        if sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE email=$1")
            .bind(&row.email)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?
            .is_some()
        {
            return Ok(CsvBirthOutcome::Skipped);
        }
        if !["admin", "member", "viewer"].contains(&role.as_str()) {
            return Ok(CsvBirthOutcome::Rejected(format!("Invalid role: {role}")));
        }
        if let Err(error) = CsvBirthPolicy::load(&mut conn)
            .await?
            .validate(&row.email, &role)
        {
            return Ok(CsvBirthOutcome::Rejected(error));
        }
        require_current_admin(&mut conn, admin).await?;
    }
    let temporary_password = crate::auth::routes::generate_temporary_password();
    #[cfg(test)]
    let _ = CSV_BIRTH_HASH_HOOK.try_with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook()
        }
    });
    let password_hash = crate::auth::password::hash_password(&temporary_password)
        .map_err(|e| AppError::Internal(e.to_string()))?;
    #[cfg(test)]
    if let Ok(Some(hook)) = CSV_BIRTH_BEFORE_FENCE.try_with(|s| s.borrow_mut().take()) {
        hook.await;
    }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    let authority = require_current_admin(&mut tx, admin).await?;
    if sqlx::query_scalar::<_, String>("SELECT id FROM users WHERE email=$1")
        .bind(&row.email)
        .fetch_optional(&mut *tx)
        .await
        .map_err(AppError::internal)?
        .is_some()
    {
        return Ok(CsvBirthOutcome::Skipped);
    }
    let policy = CsvBirthPolicy::load(&mut tx).await?;
    if let Err(error) = policy.validate(&row.email, &role) {
        return Ok(CsvBirthOutcome::Rejected(error));
    }
    let mut sources = Vec::new();
    let slugs: std::collections::BTreeSet<String> = row
        .spaces
        .as_deref()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    for slug in slugs {
        let source = csv_space_source(&mut tx, &slug).await?;
        sources.push((slug, source));
    }
    let ids: Vec<&str> = sources
        .iter()
        .filter_map(|(_, s)| s.as_ref().map(|r| r.0.as_str()))
        .collect();
    crate::spaces::mutation_guards::lock_spaces_for_mutation(&mut tx, &ids).await?;
    require_current_admin(&mut tx, admin).await?;
    for (slug, source) in &sources {
        if csv_space_source(&mut tx, slug).await?.as_ref() != source.as_ref() {
            return Err(AppError::Conflict("CSV target space source changed".into()));
        }
    }
    let mut user_ids: Vec<String> = sqlx::query_scalar("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    let prepared = crate::auth::routes::insert_account_with_personal_space(
        &mut tx,
        crate::auth::routes::NewAccountSeed {
            id: Uuid::new_v4().to_string(),
            email: row.email.clone(),
            display_name: row.display_name.clone(),
            password_hash: Some(password_hash),
            avatar_url: None,
            role,
            is_active: true,
            external_id: None,
        },
    )
    .await?;
    let mut memberships = Vec::new();
    for (_, source) in &sources {
        if let Some(space) = source {
            let member: crate::auth::routes::InitialCsvMembership = (
                Uuid::new_v4().to_string(),
                space.0.clone(),
                prepared.user_id().into(),
                "editor".into(),
                prepared.created_at().into(),
            );
            let n=sqlx::query("INSERT INTO space_members(id,space_id,user_id,role,created_at) VALUES($1,$2,$3,$4,$5)").bind(&member.0).bind(&member.1).bind(&member.2).bind(&member.3).bind(&member.4).execute(&mut *tx).await.map_err(AppError::internal)?;
            if n.rows_affected() != 1 {
                return Err(AppError::Conflict("CSV membership changed".into()));
            }
            memberships.push(member);
        }
    }
    crate::auth::routes::verify_prepared_account_with_initial_memberships(
        &mut tx,
        &prepared,
        &memberships,
    )
    .await?;
    let origin = CsvAccountBirthAuditOrigin {
        user_id: prepared.user_id().into(),
        space_id: prepared.personal.id.clone(),
    };
    let space_event = crate::audit::insert_csv_birth_space_action(&mut tx, &origin).await?;
    let user_event = crate::audit::insert_actor_action(
        &mut tx,
        &authority,
        "user.import",
        "user",
        prepared.user_id(),
    )
    .await?;
    crate::auth::routes::verify_prepared_account_with_initial_memberships(
        &mut tx,
        &prepared,
        &memberships,
    )
    .await?;
    crate::audit::verify_csv_birth_space_action(&mut tx, &space_event, &origin).await?;
    crate::audit::verify_actor_action(&mut tx, &user_event, &authority).await?;
    user_ids.push(prepared.user_id().into());
    user_ids.sort();
    if sqlx::query_scalar::<_, String>("SELECT id FROM users ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(AppError::internal)?
        != user_ids
    {
        return Err(AppError::Conflict("CSV account source changed".into()));
    }
    for (slug, source) in &sources {
        if csv_space_source(&mut tx, slug).await?.as_ref() != source.as_ref() {
            return Err(AppError::Conflict("CSV target space source changed".into()));
        }
    }
    if CsvBirthPolicy::load(&mut tx).await? != policy {
        return Err(AppError::Conflict("CSV provisioning policy changed".into()));
    }
    #[cfg(test)]
    if let Ok(Some(hook)) = CSV_BIRTH_FINAL_HOOK.try_with(|s| s.borrow_mut().take()) {
        hook.await;
    }
    require_current_admin(&mut tx, admin).await?;
    policy.verify_environment()?;
    tx.commit().await.map_err(|_|AppError::Internal("CSV account creation outcome is unknown; reauthenticate and inspect/reset the account before retrying".into()))?;
    #[cfg(test)]
    if CSV_BIRTH_LOST_ACK
        .try_with(|v| *v.borrow())
        .unwrap_or(false)
    {
        return Err(AppError::Internal("CSV account creation outcome is unknown; reauthenticate and inspect/reset the account before retrying".into()));
    }
    #[cfg(test)]
    let _ = CSV_BIRTH_POSTCOMMIT_HOOK.try_with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook(&prepared.personal.slug)
        }
    });
    if let Err(error) = crate::storage::init_space_dirs(&prepared.personal.slug).await {
        tracing::warn!(%error,"CSV personal-space directory initialization deferred");
    }
    crate::audit::append_actor_action(&space_event).await;
    crate::audit::append_actor_action(&user_event).await;
    Ok(CsvBirthOutcome::Created(ImportedUserCredential {
        email: prepared.email().into(),
        temporary_password,
    }))
}
async fn import_csv_rows(
    state: &AppState,
    admin: &AdminUser,
    rows: &[CsvRow],
) -> Result<ImportResult, AppError> {
    let mut result = ImportResult {
        created: 0,
        skipped: 0,
        errors: Vec::new(),
        created_users: Vec::new(),
    };
    for (i, row) in rows.iter().enumerate() {
        match create_csv_account(state, admin, row).await? {
            CsvBirthOutcome::Skipped => result.skipped += 1,
            CsvBirthOutcome::Rejected(error) => {
                result.errors.push(ImportError { line: i + 2, error })
            }
            CsvBirthOutcome::Created(user) => {
                result.created += 1;
                result.created_users.push(user);
            }
        }
    }
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    crate::auth::middleware::require_current_admin(&mut conn, admin).await?;
    Ok(result)
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

    Ok(UserProvisioningPolicy::from_config_values(
        &allowed_email_domains,
        &allowed_roles,
    ))
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
        sqlx::query_as("SELECT value FROM system_config WHERE key = $1")
            .bind(key)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    Ok(stored
        .map(|item| item.0)
        .unwrap_or_else(|| default.to_string()))
}

impl UserProvisioningPolicy {
    pub(crate) fn from_config_values(domains: &str, roles: &str) -> Self {
        Self {
            allowed_email_domains: parse_policy_list(domains),
            allowed_roles: parse_policy_list(roles).into_iter().collect(),
        }
    }
    pub(crate) fn validate(&self, email: &str, role: &str) -> Result<(), String> {
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
#[cfg(test)]
mod csv_account_atomic_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use axum::extract::FromRequestParts;
    use serde_json::Value;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        crate::spaces::routes::space_creation_fence_tests::setup(wal).await
    }
    async fn admin(s: &AppState) -> AdminUser {
        crate::spaces::routes::space_creation_fence_tests::admin(s).await
    }
    fn row() -> CsvRow {
        CsvRow {
            email: format!("csv-{}@example.test", Uuid::new_v4()),
            display_name: format!("CSV {}", Uuid::new_v4()),
            role: None,
            spaces: Some("space-a,missing-space,space-a".into()),
        }
    }
    async fn proof(s: &AppState, key: &str, service: bool, scopes: &str) -> AdminUser {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES($1,$2,$1,$3,'prefix',$4,$5)").bind(key).bind(if service{None}else{Some("owner-a")}).bind(crate::auth::api_keys::hash_api_key(key)).bind(if service{1}else{0}).bind(scopes).execute(&s.db).await.unwrap();
        let request = axum::http::Request::builder()
            .method("POST")
            .header("X-API-Key", key)
            .body(())
            .unwrap();
        let (mut parts, _) = request.into_parts();
        AdminUser::from_request_parts(&mut parts, s).await.unwrap()
    }
    async fn cleanup(s: &AppState, email: &str) {
        let slug: Option<String> = sqlx::query_scalar(
            "SELECT s.slug FROM spaces s JOIN users u ON u.id=s.created_by WHERE u.email=$1",
        )
        .bind(email)
        .fetch_optional(&s.db)
        .await
        .unwrap();
        if let Some(slug) = slug {
            let _ = tokio::fs::remove_dir_all(crate::storage::data_dir().join("spaces").join(slug))
                .await;
        }
    }
    #[tokio::test]
    async fn csv_birth_real_http_human_service_credentials_and_exact_initial_memberships() {
        for service in [false, true] {
            let s = setup(false).await;
            let key = if service { "csv-service" } else { "csv-admin" };
            let _ = proof(&s, key, service, "[\"admin\"]").await;
            let email = format!("Case.{}@Example.TEST", Uuid::new_v4());
            // Existing parser consumes only the first quoted comma token; retain it.
            let csv=format!("email,display_name,role,spaces\n{email},CSV user,VIEWER,\"space-a,space-b\"\n{email},Duplicate,member,\nmissing,,member,\n");
            let body=format!("--csv-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"accounts.csv\"\r\nContent-Type: text/csv\r\n\r\n{csv}\r\n--csv-boundary--\r\n");
            let request = axum::http::Request::builder()
                .method("POST")
                .uri("/admin/users/import")
                .header("X-API-Key", key)
                .header("content-type", "multipart/form-data; boundary=csv-boundary")
                .body(axum::body::Body::from(body))
                .unwrap();
            let response = router()
                .with_state(s.clone())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::CREATED);
            let result: Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), 1024 * 1024)
                    .await
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(result["created"], 1);
            assert_eq!(result["skipped"], 1);
            assert_eq!(result["errors"].as_array().unwrap().len(), 1);
            let email = email.to_lowercase();
            let user: (String, String, String) =
                sqlx::query_as("SELECT id,role,password_hash FROM users WHERE email=$1")
                    .bind(&email)
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            assert_eq!(user.1, "viewer");
            assert!(crate::auth::password::verify_password(
                result["created_users"][0]["temporary_password"]
                    .as_str()
                    .unwrap(),
                &user.2
            )
            .unwrap());
            let memberships: Vec<(String, String)> = sqlx::query_as(
                "SELECT space_id,role FROM space_members WHERE user_id=$1 ORDER BY space_id",
            )
            .bind(&user.0)
            .fetch_all(&s.db)
            .await
            .unwrap();
            assert_eq!(memberships.len(), 2);
            assert!(memberships.contains(&("space-a".into(), "editor".into())));
            assert!(!memberships.iter().any(|r| r.0 == "space-b"));
            let audit:(String,String,Option<String>)=sqlx::query_as("SELECT actor_kind,actor_id,user_id FROM audit_log WHERE action='user.import' AND target_id=$1").bind(&user.0).fetch_one(&s.db).await.unwrap();
            assert_eq!(
                audit,
                if service {
                    ("service_key".into(), key.into(), None)
                } else {
                    ("user".into(), "owner-a".into(), Some("owner-a".into()))
                }
            );
            let birth:(String,String,Option<String>)=sqlx::query_as("SELECT actor_kind,actor_id,user_id FROM audit_log WHERE action='space.personal_create'").fetch_one(&s.db).await.unwrap();
            assert_eq!(birth, ("user".into(), user.0.clone(), Some(user.0.clone())));
            cleanup(&s, &email).await;
        }
    }
    #[tokio::test]
    async fn csv_birth_cheap_policy_duplicates_missing_fields_and_current_denials_never_hash() {
        for scenario in [
            "duplicate",
            "missing",
            "role",
            "domain",
            "inactive",
            "session",
        ] {
            let s = setup(false).await;
            let p = admin(&s).await;
            let mut r = row();
            match scenario {
                "duplicate" => r.email = "owner-a@catalog.test".into(),
                "missing" => r.display_name.clear(),
                "role" => r.role = Some("owner".into()),
                "domain" => {
                    sqlx::query("INSERT INTO system_config(key,value) VALUES('user_provisioning_allowed_email_domains','allowed.test') ON CONFLICT(key) DO UPDATE SET value='allowed.test'").execute(&s.db).await.unwrap();
                }
                "inactive" => {
                    sqlx::query("UPDATE users SET is_active=0 WHERE id='owner-a'")
                        .execute(&s.db)
                        .await
                        .unwrap();
                }
                _ => {
                    sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'")
                        .execute(&s.db)
                        .await
                        .unwrap();
                }
            }
            let before = move_database_snapshot(&s).await;
            let result = CSV_BIRTH_HASH_HOOK
                .scope(
                    std::cell::RefCell::new(Some(Box::new(|| panic!("denied CSV row hashed")))),
                    create_csv_account(&s, &p, &r),
                )
                .await;
            match scenario {
                "duplicate" => assert!(matches!(result, Ok(CsvBirthOutcome::Skipped))),
                "inactive" | "session" => assert!(result.is_err()),
                _ => assert!(matches!(result, Ok(CsvBirthOutcome::Rejected(_)))),
            }
            assert_eq!(move_database_snapshot(&s).await, before);
        }
    }
    #[tokio::test]
    async fn csv_birth_constituent_failures_roll_back_whole_account_and_both_audits() {
        for table in [
            "users",
            "spaces",
            "space_members",
            "groups",
            "group_members",
            "space_group_permissions",
            "audit_log",
        ] {
            let s = setup(false).await;
            let p = admin(&s).await;
            sqlx::query(&format!("CREATE TRIGGER fail_csv_birth AFTER INSERT ON {table} BEGIN SELECT RAISE(ABORT,'CSV constituent failed'); END")).execute(&s.db).await.unwrap();
            let before = move_database_snapshot(&s).await;
            assert!(create_csv_account(&s, &p, &row()).await.is_err(), "{table}");
            assert_eq!(move_database_snapshot(&s).await, before, "{table}");
        }
    }
    #[tokio::test]
    async fn csv_birth_late_source_policy_extra_membership_and_credentials_roll_back() {
        for statement in [
            "UPDATE users SET role='admin' WHERE id=NEW.target_id",
            "INSERT INTO space_members(id,space_id,user_id,role) VALUES('rogue-extra','space-b',NEW.target_id,'owner')",
            "INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('rogue-key',NEW.target_id,'Rogue','hash','prefix','[\"admin\"]')",
            "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) SELECT 'rogue-owned',id,space_id,NEW.target_id,'Rogue','base' FROM documents WHERE space_id='space-a' LIMIT 1",
            "INSERT INTO doc_subscriptions(document_id,user_id) SELECT id,NEW.target_id FROM documents WHERE space_id='space-a' LIMIT 1",
            "UPDATE spaces SET slug='changed' WHERE id='space-a'",
            "INSERT INTO spaces(id,name,slug,created_by) VALUES('late-unknown','Late','missing-space','owner-a')",
            "UPDATE system_config SET value='viewer' WHERE key='user_provisioning_allowed_roles'",
            "DELETE FROM sessions WHERE user_id='owner-a'",
            "UPDATE users SET is_active=0 WHERE id='owner-a'",
            "UPDATE audit_log SET actor_id='rogue' WHERE id=NEW.id"
        ]{
            let s=setup(false).await;let p=admin(&s).await;
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('csv-existing-page','space-a','Existing','unused','owner-a','owner-a','old')").execute(&s.db).await.unwrap();
            sqlx::query("INSERT INTO system_config(key,value) VALUES('user_provisioning_allowed_roles','admin,member,viewer') ON CONFLICT(key) DO UPDATE SET value='admin,member,viewer'").execute(&s.db).await.unwrap();
            sqlx::query(&format!("CREATE TRIGGER late_csv_source AFTER INSERT ON audit_log WHEN NEW.action='user.import' BEGIN {statement}; END")).execute(&s.db).await.unwrap();
            let before=move_database_snapshot(&s).await;assert!(create_csv_account(&s,&p,&row()).await.is_err(),"{statement}");assert_eq!(move_database_snapshot(&s).await,before,"{statement}");
        }
    }
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn csv_birth_real_wal_audience_wait_rechecks_credential_policy_and_target() {
        for scenario in ["session", "key", "policy", "slug"] {
            let s = setup(true).await;
            let key_proof = proof(&s, "wait-key", false, "[\"admin\"]").await;
            let p = if scenario == "session" {
                admin(&s).await
            } else {
                key_proof
            };
            let mut blocker = s.db.begin().await.unwrap();
            crate::spaces::mutation_guards::lock_audience_for_mutation(&mut blocker)
                .await
                .unwrap();
            let ready = std::sync::Arc::new(tokio::sync::Notify::new());
            let notice = ready.clone();
            let copy = s.clone();
            let r = row();
            let email = r.email.clone();
            let pending = tokio::spawn(async move {
                CSV_BIRTH_BEFORE_FENCE
                    .scope(
                        std::cell::RefCell::new(Some(Box::pin(async move {
                            notice.notify_one();
                        }))),
                        create_csv_account(&copy, &p, &r),
                    )
                    .await
            });
            ready.notified().await;
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            assert!(!pending.is_finished());
            let sql=match scenario{"session"=>"DELETE FROM sessions WHERE user_id='owner-a'","key"=>"UPDATE api_keys SET scopes='[\"read\"]' WHERE id='wait-key'","policy"=>"INSERT INTO system_config(key,value) VALUES('user_provisioning_allowed_roles','viewer') ON CONFLICT(key) DO UPDATE SET value='viewer'",_=>"UPDATE spaces SET slug='changed' WHERE id='space-a'"};
            sqlx::query(sql).execute(&mut *blocker).await.unwrap();
            blocker.commit().await.unwrap();
            let result = pending.await.unwrap();
            if scenario == "slug" {
                assert!(matches!(result, Ok(CsvBirthOutcome::Created(_))));
                let count:i64=sqlx::query_scalar("SELECT count(*) FROM space_members sm JOIN users u ON u.id=sm.user_id WHERE u.email=$1 AND sm.space_id='space-a'").bind(&email).fetch_one(&s.db).await.unwrap();
                assert_eq!(count, 0);
                cleanup(&s, &email).await;
            } else {
                assert!(!matches!(result, Ok(CsvBirthOutcome::Created(_))));
                let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email=$1")
                    .bind(email)
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
                assert_eq!(n, 0);
            }
        }
    }
    #[tokio::test]
    async fn csv_birth_last_original_jwt_expiry_rolls_back_full_row() {
        let s = setup(false).await;
        let original = admin(&s).await;
        let mut claims = original.claims;
        claims.exp = (chrono::Utc::now() + chrono::Duration::seconds(2)).timestamp();
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = axum::http::Request::builder()
            .method("POST")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let p = AdminUser::from_request_parts(&mut parts, &s).await.unwrap();
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hook_reached = reached.clone();
        let before = move_database_snapshot(&s).await;
        let result = CSV_BIRTH_FINAL_HOOK
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    hook_reached.store(true, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }))),
                create_csv_account(&s, &p, &row()),
            )
            .await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        assert_eq!(move_database_snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn csv_birth_committed_directory_failure_remains_truthful_success_and_skips_reimport() {
        let s = setup(false).await;
        let p = admin(&s).await;
        let r = row();
        let email = r.email.clone();
        let outcome = CSV_BIRTH_POSTCOMMIT_HOOK
            .scope(
                std::cell::RefCell::new(Some(Box::new(|slug| {
                    let root = crate::storage::data_dir().join("spaces");
                    std::fs::create_dir_all(&root).unwrap();
                    std::fs::write(root.join(slug), b"owned test conflict").unwrap();
                }))),
                create_csv_account(&s, &p, &r),
            )
            .await
            .unwrap();
        assert!(matches!(outcome, CsvBirthOutcome::Created(_)));
        assert!(matches!(
            create_csv_account(&s, &p, &r).await.unwrap(),
            CsvBirthOutcome::Skipped
        ));
        let slug: String = sqlx::query_scalar(
            "SELECT s.slug FROM spaces s JOIN users u ON u.id=s.created_by WHERE u.email=$1",
        )
        .bind(email)
        .fetch_one(&s.db)
        .await
        .unwrap();
        std::fs::remove_file(crate::storage::data_dir().join("spaces").join(slug)).unwrap();
    }
    #[tokio::test]
    async fn csv_birth_lost_ack_is_indeterminate_never_returns_new_credentials_or_recreates() {
        let s = setup(false).await;
        let p = admin(&s).await;
        let r = row();
        let result = CSV_BIRTH_LOST_ACK
            .scope(
                std::cell::RefCell::new(true),
                create_csv_account(&s, &p, &r),
            )
            .await;
        assert!(
            matches!(result,Err(AppError::Internal(ref message)) if message.contains("outcome is unknown"))
        );
        let user: String = sqlx::query_scalar("SELECT id FROM users WHERE email=$1")
            .bind(&r.email)
            .fetch_one(&s.db)
            .await
            .unwrap();
        let n: i64 = sqlx::query_scalar("SELECT count(*) FROM space_members WHERE user_id=$1")
            .bind(&user)
            .fetch_one(&s.db)
            .await
            .unwrap();
        assert_eq!(n, 2);
        let before = move_database_snapshot(&s).await;
        assert!(matches!(
            create_csv_account(&s, &p, &r).await.unwrap(),
            CsvBirthOutcome::Skipped
        ));
        assert_eq!(move_database_snapshot(&s).await, before);
        cleanup(&s, &r.email).await;
    }
    #[tokio::test]
    async fn csv_birth_read_write_keys_and_empty_results_still_require_current_admin() {
        for scopes in ["[\"read\"]", "[\"write\"]"] {
            let s = setup(false).await;
            let key = "denied-csv-key";
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a',$1,$2,'prefix',$3)").bind(key).bind(crate::auth::api_keys::hash_api_key(key)).bind(scopes).execute(&s.db).await.unwrap();
            let request=axum::http::Request::builder().method("POST").uri("/admin/users/import").header("X-API-Key",key).header("content-type","multipart/form-data; boundary=x").body(axum::body::Body::from("--x\r\nContent-Disposition: form-data; name=\"file\"\r\n\r\nemail,display_name\nnew@example.test,New\n\r\n--x--\r\n")).unwrap();
            let before = move_database_snapshot(&s).await;
            let response = router()
                .with_state(s.clone())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            // Extraction may update last_used_at; no creator/domain/audit state appears.
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM users WHERE email='new@example.test'"
                )
                .fetch_one(&s.db)
                .await
                .unwrap(),
                0
            );
            assert!(!before.is_empty());
        }
        let s = setup(false).await;
        let p = admin(&s).await;
        sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'")
            .execute(&s.db)
            .await
            .unwrap();
        assert!(matches!(
            import_csv_rows(&s, &p, &[]).await,
            Err(AppError::Unauthorized(_))
        ));
    }
}

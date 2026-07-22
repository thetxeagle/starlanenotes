use axum::{
    body::Body,
    extract::{Multipart, Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/files/{*storage_path}", get(serve_attachment_file))
        .route(
            "/spaces/{space_id}/documents/{doc_id}/attachments",
            get(list_attachments).post(upload_attachment),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/attachments/{attachment_id}",
            axum::routing::delete(delete_attachment),
        )
}

async fn serve_attachment_file(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(storage_path): Path<String>,
) -> Result<Response, AppError> {
    if !storage::validate_path(&storage_path) || !storage_path.starts_with("spaces/") {
        return Err(AppError::BadRequest("Invalid file path".into()));
    }

    let attachment: Option<(String, String, String)> = sqlx::query_as(
        "SELECT document_id, space_id, mime_type FROM attachments WHERE storage_path = ?1 LIMIT 1",
    )
    .bind(&storage_path)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let Some((document_id, space_id, mime_type)) = attachment else {
        return Err(AppError::NotFound("File not found".into()));
    };

    require_attachment_access(
        &state,
        &space_id,
        &document_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;

    let full_path = storage::data_dir().join(&storage_path);
    let bytes = tokio::fs::read(full_path)
        .await
        .map_err(|_| AppError::NotFound("File not found".into()))?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime_type)
        .body(Body::from(bytes))
        .map_err(AppError::internal)
}

#[derive(Serialize)]
pub struct AttachmentResponse {
    pub id: String,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub url: String,
    pub created_at: String,
}

async fn list_attachments(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_attachment_access(
        &state,
        &space_id,
        &doc_id,
        &auth.claims.sub,
        &["owner", "editor", "viewer"],
    )
    .await?;
    let attachments: Vec<(String, String, String, String, i64, String, String)> = sqlx::query_as(
        "SELECT id, filename, original_name, mime_type, size_bytes, storage_path, created_at \
         FROM attachments WHERE document_id = ?1 AND space_id = ?2 ORDER BY created_at",
    )
    .bind(&doc_id)
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let result: Vec<AttachmentResponse> = attachments
        .into_iter()
        .map(
            |(id, filename, original, mime, size, path, created)| AttachmentResponse {
                url: format!("/api/v1/files/{}", path),
                id,
                filename,
                original_name: original,
                mime_type: mime,
                size_bytes: size,
                created_at: created,
            },
        )
        .collect();

    Ok(Json(result))
}

async fn upload_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    require_attachment_access(
        &state,
        &space_id,
        &doc_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await?;
    let max_size = max_upload_size_bytes(&state).await?;

    // Get space slug
    let slug: (String,) = sqlx::query_as("SELECT slug FROM spaces WHERE id = ?1")
        .bind(&space_id)
        .fetch_one(&state.db)
        .await
        .map_err(AppError::internal)?;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Upload error: {}", e)))?
    {
        let original_name =
            validate_attachment_upload_field(field.name(), field.file_name())?.to_string();
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        let bytes = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("Failed to read file: {}", e)))?;

        if bytes.len() > max_size {
            return Err(AppError::BadRequest(format!(
                "File too large. Maximum size is {} MB",
                max_size / 1024 / 1024
            )));
        }

        let attachment_id = Uuid::new_v4().to_string();
        let ext = safe_attachment_extension(&original_name);
        let filename = format!("{}.{}", attachment_id, ext);

        // Write to disk
        let dir = storage::attachments_dir(&slug.0).join(&doc_id);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to create dir: {}", e)))?;

        let file_path = dir.join(&filename);
        tokio::fs::write(&file_path, &bytes)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to write file: {}", e)))?;

        let storage_path = format!("spaces/{}/attachments/{}/{}", slug.0, doc_id, filename);

        sqlx::query(
            "INSERT INTO attachments (id, document_id, space_id, filename, original_name, mime_type, size_bytes, storage_path, created_by) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(&attachment_id)
        .bind(&doc_id)
        .bind(&space_id)
        .bind(&filename)
        .bind(&original_name)
        .bind(&content_type)
        .bind(bytes.len() as i64)
        .bind(&storage_path)
        .bind(&auth.claims.sub)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

        return Ok((
            StatusCode::CREATED,
            Json(AttachmentResponse {
                id: attachment_id,
                filename,
                original_name,
                mime_type: content_type,
                size_bytes: bytes.len() as i64,
                url: format!("/api/v1/files/{}", storage_path),
                created_at: chrono::Utc::now().to_rfc3339(),
            }),
        ));
    }

    Err(AppError::BadRequest("No file in upload".into()))
}

async fn max_upload_size_bytes(state: &AppState) -> Result<usize, AppError> {
    if let Ok(value) = std::env::var("MAX_UPLOAD_SIZE_MB") {
        if let Some(size) = parse_upload_size_mb(&value) {
            return Ok(size);
        }
    }

    let stored: Option<(String,)> =
        sqlx::query_as("SELECT value FROM system_config WHERE key = ?1")
            .bind("max_upload_size_mb")
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    Ok(stored
        .and_then(|(value,)| parse_upload_size_mb(&value))
        .unwrap_or(100 * 1024 * 1024))
}

fn parse_upload_size_mb(value: &str) -> Option<usize> {
    let mb = value.trim().parse::<usize>().ok()?;
    if mb == 0 {
        return None;
    }
    mb.checked_mul(1024)?.checked_mul(1024)
}

fn safe_attachment_extension(original_name: &str) -> &str {
    let Some((_, extension)) = original_name.rsplit_once('.') else {
        return "bin";
    };
    if extension.is_empty()
        || extension.len() > 16
        || !extension.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return "bin";
    }
    extension
}

fn validate_attachment_upload_field<'a>(
    field_name: Option<&str>,
    file_name: Option<&'a str>,
) -> Result<&'a str, AppError> {
    if field_name != Some("file") {
        return Err(AppError::BadRequest(
            "Upload must include a file field".into(),
        ));
    }
    let file_name = file_name
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| AppError::BadRequest("Uploaded file must have a filename".into()))?;
    if file_name.len() > 255 {
        return Err(AppError::BadRequest(
            "Uploaded filename must be 255 bytes or fewer".into(),
        ));
    }
    Ok(file_name)
}

async fn delete_attachment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, attachment_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    require_attachment_access(
        &state,
        &space_id,
        &doc_id,
        &auth.claims.sub,
        &["owner", "editor"],
    )
    .await?;

    let path: Option<(String,)> = sqlx::query_as(
        "SELECT storage_path FROM attachments WHERE id = ?1 AND document_id = ?2 AND space_id = ?3",
    )
    .bind(&attachment_id)
    .bind(&doc_id)
    .bind(&space_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;

    let (storage_path,) =
        path.ok_or_else(|| AppError::NotFound("Attachment not found in this document".into()))?;
    let full_path = storage::data_dir().join(&storage_path);
    tokio::fs::remove_file(full_path).await.ok();

    sqlx::query("DELETE FROM attachments WHERE id = ?1 AND document_id = ?2 AND space_id = ?3")
        .bind(&attachment_id)
        .bind(&doc_id)
        .bind(&space_id)
        .execute(&state.db)
        .await
        .map_err(AppError::internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn require_attachment_access(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<(), AppError> {
    super::routes::require_space_role(state, space_id, user_id, allowed_roles).await?;
    let document: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM documents WHERE id = ?1 AND space_id = ?2 LIMIT 1")
            .bind(document_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;
    if document.is_none() {
        return Err(AppError::NotFound(
            "Document not found in this space".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        parse_upload_size_mb, safe_attachment_extension, validate_attachment_upload_field,
    };

    #[test]
    fn parses_upload_size_megabytes() {
        assert_eq!(parse_upload_size_mb("25"), Some(25 * 1024 * 1024));
        assert_eq!(parse_upload_size_mb(" 100 "), Some(100 * 1024 * 1024));
    }

    #[test]
    fn rejects_invalid_upload_size_values() {
        assert_eq!(parse_upload_size_mb("0"), None);
        assert_eq!(parse_upload_size_mb(""), None);
        assert_eq!(parse_upload_size_mb("abc"), None);
    }

    #[test]
    fn attachment_storage_extensions_are_safe_and_bounded() {
        assert_eq!(safe_attachment_extension("notes.pdf"), "pdf");
        assert_eq!(safe_attachment_extension("no-extension"), "bin");
        assert_eq!(safe_attachment_extension("archive.tar.gz"), "gz");
        assert_eq!(safe_attachment_extension("unsafe.dir/name"), "bin");
        assert_eq!(safe_attachment_extension("unsafe.🙃"), "bin");
        assert_eq!(
            safe_attachment_extension(&format!("file.{}", "a".repeat(17))),
            "bin"
        );
    }

    #[test]
    fn attachment_upload_requires_the_documented_file_part_and_filename() {
        assert_eq!(
            validate_attachment_upload_field(Some("file"), Some("notes.pdf")).unwrap(),
            "notes.pdf"
        );
        assert!(validate_attachment_upload_field(Some("metadata"), Some("notes.pdf")).is_err());
        assert!(validate_attachment_upload_field(Some("file"), None).is_err());
        assert!(validate_attachment_upload_field(Some("file"), Some("  ")).is_err());
        assert!(validate_attachment_upload_field(Some("file"), Some(&"a".repeat(256))).is_err());
    }
}

#[cfg(target_os="linux")]
mod restoration_sqlite_source_vfs;
#[cfg(target_os = "linux")]
pub(crate) mod file_read_custody;
#[cfg(target_os = "linux")]
pub(crate) mod private_space_layout;
#[cfg(test)]
pub(crate) mod fixture_root;
#[cfg(target_os = "linux")]
pub(crate) mod data_root_designation;
#[cfg(target_os = "linux")]
pub(crate) mod recovery_records;
mod health_capacity;
pub(crate) mod health;
#[cfg(target_os = "linux")]
pub(crate) mod attachment_upload;
#[cfg(target_os = "linux")]
pub(crate) mod attachment_capacity;
pub(crate) mod backup_restore;
pub(crate) mod backup_transfer;
pub(crate) mod backup_inventory;
pub(crate) mod backup_inspect;
pub(crate) mod asset_blob;
pub mod canonical_rewrite;
pub mod journal;
mod journal_fs;
pub mod publication;
pub mod startup;
pub mod relocation;
pub(crate) mod restoration;
#[cfg(target_os = "linux")]
pub(crate) mod restoration_plan;
use serde::Serialize;
use sqlx::{AnyPool, Row};
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use tokio::fs;

/// Get the data directory from environment or default
pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string()))
}

/// Initialize the top-level data directory so volume/bind-mount wiring is visible on startup.
pub async fn init_data_dir() -> Result<(), std::io::Error> {
    let base = data_dir();
    #[cfg(target_os = "linux")]
    match data_root_designation::initialize_compatible(&base)? {
        data_root_designation::StartupLayout::Designated => {}
        data_root_designation::StartupLayout::LegacyMarkerless => {
            fs::create_dir_all(base.join("spaces")).await?;
            fs::create_dir_all(base.join("uploads")).await?;
            fs::create_dir_all(base.join("search_index")).await?;
            eprintln!(
                "Warning: using existing markerless legacy data root {}; designation-dependent custody, recovery, and deletion operations remain unavailable",
                base.display()
            );
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        fs::create_dir_all(base.join("spaces")).await?;
        fs::create_dir_all(base.join("uploads")).await?;
        fs::create_dir_all(base.join("search_index")).await?;
    }
    #[cfg(not(target_os = "linux"))]
    fs::write(
        base.join(".starlane-notes-data-dir"),
        "Starlane Notes data directory\n",
    ).await?;
    Ok(())
}

/// Get the path to a space's directory on disk
pub fn space_dir(space_slug: &str) -> PathBuf {
    data_dir().join("spaces").join(space_slug)
}

/// Get the path to a space's docs directory
pub fn docs_dir(space_slug: &str) -> PathBuf {
    space_dir(space_slug).join("docs")
}

/// Get the path to a space's attachments directory
pub fn attachments_dir(space_slug: &str) -> PathBuf {
    space_dir(space_slug).join("attachments")
}

/// Return the local lifecycle mirror directory for a space. These mirrors are
/// human-readable exports; SQLite remains the source of truth for state.
pub fn lifecycle_dir(space_slug: &str, state: &str) -> PathBuf {
    space_dir(space_slug).join(state)
}

/// Initialize the directory structure for a new space
pub async fn init_space_dirs(space_slug: &str) -> Result<(), std::io::Error> {
    #[cfg(target_os = "linux")]
    if private_space_layout::is_designated(&data_dir())? {
        return private_space_layout::initialize(&data_dir(), space_slug);
    }
    {
    let base = space_dir(space_slug);
    fs::create_dir_all(base.join("docs")).await?;
    fs::create_dir_all(base.join("attachments")).await?;
    fs::create_dir_all(base.join("excalidraw")).await?;
    fs::create_dir_all(base.join("drafts")).await?;
    fs::create_dir_all(base.join("archived")).await?;
    fs::create_dir_all(base.join("trashed")).await?;
    fs::create_dir_all(base.join(".starlane/templates")).await?;
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct StagedFileCopy {
    pub source: String,
    pub destination: String,
}

/// Stage a regular owned file at a new safe relative path without removing the source.
/// Callers update database ownership before finalizing the source deletion.
pub async fn stage_owned_file_copy(
    source: &str,
    destination: &str,
    required: bool,
) -> Result<Option<StagedFileCopy>, std::io::Error> {
    stage_owned_file_copy_in(&data_dir(), source, destination, required).await
}

async fn stage_owned_file_copy_in(
    root: &Path,
    source: &str,
    destination: &str,
    required: bool,
) -> Result<Option<StagedFileCopy>, std::io::Error> {
    let source_path = Path::new(source);
    let destination_path = Path::new(destination);
    if !is_safe_relative_path(source_path) || !is_safe_relative_path(destination_path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Storage paths must be safe relative paths",
        ));
    }
    if source == destination {
        return Ok(None);
    }
    let source_full = root.join(source_path);
    let destination_full = root.join(destination_path);
    let metadata = match fs::symlink_metadata(&source_full).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => return Ok(None),
        Err(error) => return Err(error),
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Storage source must be a regular file",
        ));
    }
    if fs::symlink_metadata(&destination_full).await.is_ok() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("Storage destination already exists: {destination}"),
        ));
    }
    if let Some(parent) = destination_full.parent() {
        fs::create_dir_all(parent).await?;
    }
    fs::copy(&source_full, &destination_full).await?;
    if let Ok(file) = fs::OpenOptions::new()
        .write(true)
        .open(&destination_full)
        .await
    {
        file.sync_all().await?;
    }
    Ok(Some(StagedFileCopy {
        source: source.to_string(),
        destination: destination.to_string(),
    }))
}

pub async fn rollback_staged_file_copies(copies: &[StagedFileCopy]) {
    for copy in copies.iter().rev() {
        if let Err(error) = fs::remove_file(data_dir().join(&copy.destination)).await {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = copy.destination, %error, "Could not roll back staged storage file");
            }
        }
    }
}

pub async fn finalize_staged_file_copies(copies: &[StagedFileCopy]) {
    for copy in copies {
        if let Err(error) = fs::remove_file(data_dir().join(&copy.source)).await {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = copy.source, %error, "Moved storage record kept its old file copy");
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct StorageIntegrityIssue {
    pub kind: &'static str,
    pub record_type: &'static str,
    pub record_id: Option<String>,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Default, Serialize)]
pub struct StorageIntegrityReport {
    pub healthy: bool,
    pub documents: usize,
    pub drafts: usize,
    pub attachments: usize,
    pub issues: Vec<StorageIntegrityIssue>,
}

/// Compare database-owned storage records with their files without changing either side.
pub async fn audit_integrity(db: &AnyPool) -> Result<StorageIntegrityReport, sqlx::Error> {
    audit_integrity_in(&data_dir(), db).await
}

async fn audit_integrity_in(
    root: &Path,
    db: &AnyPool,
) -> Result<StorageIntegrityReport, sqlx::Error> {
    let mut report = StorageIntegrityReport::default();
    let mut owned_documents = HashSet::new();
    let mut owned_drafts = HashSet::new();
    let mut owned_attachments = HashSet::new();

    let documents = sqlx::query(
        "SELECT d.id, d.content_path, s.slug AS space_slug \
         FROM documents d JOIN spaces s ON s.id = d.space_id",
    )
    .fetch_all(db)
    .await?;
    report.documents = documents.len();
    for row in documents {
        let id: String = row.try_get("id")?;
        let path: String = row.try_get("content_path")?;
        let slug: String = row.try_get("space_slug")?;
        inspect_owned_path(
            root,
            &path,
            &format!("spaces/{slug}/docs/"),
            "document",
            true,
            &id,
            &mut owned_documents,
            &mut report.issues,
        )
        .await;
    }

    let drafts = sqlx::query(
        "SELECT d.id, d.title, s.slug AS space_slug \
         FROM document_drafts d JOIN spaces s ON s.id = d.space_id",
    )
    .fetch_all(db)
    .await?;
    report.drafts = drafts.len();
    for row in drafts {
        let id: String = row.try_get("id")?;
        let title: String = row.try_get("title")?;
        let slug: String = row.try_get("space_slug")?;
        let path = format!("spaces/{slug}/drafts/{}", document_filename(&title, &id));
        inspect_owned_path(
            root,
            &path,
            &format!("spaces/{slug}/drafts/"),
            "draft",
            false,
            &id,
            &mut owned_drafts,
            &mut report.issues,
        )
        .await;
    }

    let attachments = sqlx::query(
        "SELECT a.id, a.storage_path, s.slug AS space_slug \
         FROM attachments a JOIN spaces s ON s.id = a.space_id",
    )
    .fetch_all(db)
    .await?;
    report.attachments = attachments.len();
    for row in attachments {
        let id: String = row.try_get("id")?;
        let path: String = row.try_get("storage_path")?;
        let slug: String = row.try_get("space_slug")?;
        inspect_owned_path(
            root,
            &path,
            &format!("spaces/{slug}/attachments/"),
            "attachment",
            true,
            &id,
            &mut owned_attachments,
            &mut report.issues,
        )
        .await;
    }

    inspect_orphans(
        root,
        "docs",
        "document",
        &owned_documents,
        &mut report.issues,
    )
    .await;
    inspect_orphans(root, "drafts", "draft", &owned_drafts, &mut report.issues).await;
    inspect_orphans(
        root,
        "attachments",
        "attachment",
        &owned_attachments,
        &mut report.issues,
    )
    .await;
    report.healthy = report.issues.is_empty();
    Ok(report)
}

async fn inspect_owned_path(
    root: &Path,
    relative: &str,
    expected_prefix: &str,
    record_type: &'static str,
    required_file: bool,
    record_id: &str,
    owned: &mut HashSet<String>,
    issues: &mut Vec<StorageIntegrityIssue>,
) {
    let normalized = relative.replace('\\', "/");
    if !is_safe_relative_path(Path::new(&normalized)) {
        issues.push(StorageIntegrityIssue {
            kind: "unsafe_path",
            record_type,
            record_id: Some(record_id.to_string()),
            path: normalized,
            detail: "Database path is not a safe relative storage path".into(),
        });
        return;
    }
    owned.insert(normalized.clone());
    if !normalized.starts_with(expected_prefix) {
        issues.push(StorageIntegrityIssue {
            kind: "misplaced_file",
            record_type,
            record_id: Some(record_id.to_string()),
            path: normalized.clone(),
            detail: format!("Expected path beneath {expected_prefix}"),
        });
    }
    match fs::symlink_metadata(root.join(&normalized)).await {
        Ok(metadata) if metadata.file_type().is_symlink() => issues.push(StorageIntegrityIssue {
            kind: "unsafe_symlink",
            record_type,
            record_id: Some(record_id.to_string()),
            path: normalized,
            detail: "Owned storage path is a symbolic link".into(),
        }),
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => issues.push(StorageIntegrityIssue {
            kind: "invalid_file_type",
            record_type,
            record_id: Some(record_id.to_string()),
            path: normalized,
            detail: "Owned storage path is not a regular file".into(),
        }),
        // SQL is authoritative for private drafts. A synthesized optional mirror
        // path is not evidence that a filesystem write was ever acknowledged.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required_file => {},
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            issues.push(StorageIntegrityIssue {
                kind: "missing_file",
                record_type,
                record_id: Some(record_id.to_string()),
                path: normalized,
                detail: "Database record points to a missing file".into(),
            })
        }
        Err(error) => issues.push(StorageIntegrityIssue {
            kind: "unreadable_file",
            record_type,
            record_id: Some(record_id.to_string()),
            path: normalized,
            detail: error.to_string(),
        }),
    }
}

async fn inspect_orphans(
    root: &Path,
    leaf_dir: &str,
    record_type: &'static str,
    owned: &HashSet<String>,
    issues: &mut Vec<StorageIntegrityIssue>,
) {
    for path in files_in_space_leaf_dirs(root, leaf_dir).await {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        if !owned.contains(&relative) {
            issues.push(StorageIntegrityIssue {
                kind: "orphan_file",
                record_type,
                record_id: None,
                path: relative,
                detail: "File is not owned by a database record".into(),
            });
        }
    }
}

async fn files_in_space_leaf_dirs(root: &Path, leaf_dir: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let spaces = root.join("spaces");
    let Ok(mut space_entries) = fs::read_dir(spaces).await else {
        return files;
    };
    while let Ok(Some(space)) = space_entries.next_entry().await {
        let mut pending = vec![space.path().join(leaf_dir)];
        while let Some(directory) = pending.pop() {
            let Ok(mut entries) = fs::read_dir(directory).await else {
                continue;
            };
            while let Ok(Some(entry)) = entries.next_entry().await {
                let Ok(file_type) = entry.file_type().await else {
                    continue;
                };
                if file_type.is_dir() {
                    pending.push(entry.path());
                } else {
                    files.push(entry.path());
                }
            }
        }
    }
    files
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// Write a lifecycle mirror with one normalized frontmatter block.
pub async fn write_lifecycle_document(
    space_slug: &str,
    lifecycle: &str,
    id: &str,
    title: &str,
    frontmatter: &str,
    content: &str,
) -> Result<String, std::io::Error> {
    let filename = document_filename(title, id);
    let full_content = compose_document_markdown(frontmatter, content);
    #[cfg(target_os = "linux")]
    if private_space_layout::is_designated(&data_dir())? {
        private_space_layout::write(&data_dir(), space_slug, lifecycle, &filename, full_content.as_bytes())?;
        return Ok(format!("spaces/{space_slug}/{lifecycle}/{filename}"));
    }
    {
    let dir = lifecycle_dir(space_slug, lifecycle);
    fs::create_dir_all(&dir).await?;
    fs::write(dir.join(&filename), full_content).await?;
    }
    Ok(format!("spaces/{space_slug}/{lifecycle}/{filename}"))
}

pub async fn delete_lifecycle_document(
    space_slug: &str,
    lifecycle: &str,
    id: &str,
    title: &str,
) -> Result<(), std::io::Error> {
    let path = lifecycle_dir(space_slug, lifecycle).join(document_filename(title, id));
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub async fn delete_owned_file(relative: &str) -> Result<(), std::io::Error> {
    delete_owned_file_in(&data_dir(), relative).await
}

async fn delete_owned_file_in(root: &Path, relative: &str) -> Result<(), std::io::Error> {
    let relative = Path::new(relative);
    if !is_safe_relative_path(relative) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Storage path must be a safe relative path",
        ));
    }
    match fs::remove_file(root.join(relative)).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Write a document's markdown content to disk
pub async fn write_document(
    space_slug: &str,
    doc_id: &str,
    title: &str,
    frontmatter: &str,
    content: &str,
) -> Result<String, std::io::Error> {
    let filename = document_filename(title, doc_id);
    let full_content = compose_document_markdown(frontmatter, content);
    #[cfg(target_os = "linux")]
    if private_space_layout::is_designated(&data_dir())? {
        private_space_layout::write(&data_dir(), space_slug, "docs", &filename, full_content.as_bytes())?;
        return Ok(format!("spaces/{}/docs/{}", space_slug, filename));
    }
    {
    let dir = docs_dir(space_slug);
    fs::create_dir_all(&dir).await?;
    fs::write(dir.join(&filename), full_content).await?;
    }
    // Return relative path from data dir
    Ok(format!("spaces/{}/docs/{}", space_slug, filename))
}

pub(crate) fn compose_document_markdown(frontmatter: &str, content: &str) -> String {
    if frontmatter.is_empty() {
        return content.to_string();
    }

    use crate::template_guidance::{css_source_span, CssSourceSpan};
    let normalized = match css_source_span(content) {
        CssSourceSpan::Absent => content.replace("\r\n", "\n"),
        CssSourceSpan::Exact(span) => format!("{}{}{}", content[..span.start].replace("\r\n", "\n"),
            &content[span.clone()], content[span.end..].replace("\r\n", "\n")),
        CssSourceSpan::Ambiguous => return format!("---\n{}\n---\n\n{}", frontmatter.trim(), content),
    };
    if let Some(rest) = normalized.strip_prefix("---\n") {
        if let Some((portable_metadata, body)) = rest.split_once("\n---\n") {
            let separator = 4 + portable_metadata.len();
            if matches!(css_source_span(&normalized), CssSourceSpan::Exact(span) if separator < span.end && separator + 5 > span.start) {
                return format!("---\n{}\n---\n\n{}", frontmatter.trim(), normalized);
            }
            return format!(
                "---\n{}\n{}\n---\n\n{}",
                frontmatter.trim(),
                portable_metadata.trim(),
                body.trim_start()
            );
        }
    }
    format!("---\n{}\n---\n\n{}", frontmatter.trim(), normalized)
}

/// Build a readable, collision-safe Markdown filename from the page title and stable document ID.
pub fn document_filename(title: &str, doc_id: &str) -> String {
    fn safe_component(value: &str, fallback: &str, limit: usize) -> String {
        let normalized = value
            .trim()
            .chars()
            .map(|character| {
                if character.is_alphanumeric()
                    || matches!(character, ' ' | '.' | '-' | '_' | '(' | ')')
                {
                    character
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .trim_matches(|character| matches!(character, ' ' | '.' | '-'))
            .chars()
            .take(limit)
            .collect::<String>();
        if normalized.is_empty() {
            fallback.to_string()
        } else {
            normalized
        }
    }

    let readable_title = safe_component(title, "Untitled", 96);
    let stable_id = safe_component(doc_id, "document", 96).replace(' ', "-");
    format!("{readable_title}-{stable_id}.md")
}

/// Upgrade legacy ID-only server Markdown paths to readable title-prefixed filenames.
///
/// The database remains the source of truth for each content path. Files that are missing or whose
/// destination already exists are left untouched and reported through the server log.
pub async fn migrate_document_filenames(db: &AnyPool) -> Result<usize, sqlx::Error> {
    migrate_document_filenames_in(&data_dir(), db).await
}

async fn migrate_document_filenames_in(
    data_root: &Path,
    db: &AnyPool,
) -> Result<usize, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT d.id, d.title, d.content_path, s.slug AS space_slug \
         FROM documents d JOIN spaces s ON s.id = d.space_id",
    )
    .fetch_all(db)
    .await?;
    let mut migrated = 0;

    for row in rows {
        let document_id: String = row.try_get("id")?;
        let title: String = row.try_get("title")?;
        let current_path: String = row.try_get("content_path")?;
        let space_slug: String = row.try_get("space_slug")?;
        // Immutable publication revisions are never readable-filename migration inputs.
        // Only safe actual-space UUID revision paths belong to this format.
        if publication::is_publication_revision_path(&current_path, &space_slug) {
            continue;
        }
        let next_path = format!(
            "spaces/{space_slug}/docs/{}",
            document_filename(&title, &document_id)
        );
        if current_path == next_path {
            continue;
        }

        let current_full_path = data_root.join(&current_path);
        let next_full_path = data_root.join(&next_path);
        if !current_full_path.exists() {
            tracing::warn!(
                document_id,
                path = current_path,
                "Cannot rename missing legacy document file"
            );
            continue;
        }
        if next_full_path.exists() {
            tracing::warn!(
                document_id,
                path = next_path,
                "Cannot rename document because the readable destination already exists"
            );
            continue;
        }
        if let Some(parent) = next_full_path.parent() {
            if let Err(error) = fs::create_dir_all(parent).await {
                tracing::warn!(document_id, %error, "Cannot create readable document directory");
                continue;
            }
        }
        if let Err(error) = fs::rename(&current_full_path, &next_full_path).await {
            tracing::warn!(document_id, %error, "Cannot rename legacy document file");
            continue;
        }

        let updated = sqlx::query(
            "UPDATE documents SET content_path = $1 WHERE id = $2 AND content_path = $3",
        )
        .bind(&next_path)
        .bind(&document_id)
        .bind(&current_path)
        .execute(db)
        .await?;
        if updated.rows_affected() == 1 {
            migrated += 1;
        } else {
            let _ = fs::rename(&next_full_path, &current_full_path).await;
        }
    }

    Ok(migrated)
}

/// Read a document's content from disk
pub async fn read_document(content_path: &str) -> Result<String, std::io::Error> {
    let path = data_dir().join(content_path);
    fs::read_to_string(path).await
}

/// Delete a document file from disk
pub async fn delete_document(content_path: &str) -> Result<(), std::io::Error> {
    let path = data_dir().join(content_path);
    if path.exists() {
        fs::remove_file(path).await?;
    }
    Ok(())
}

/// Delete an entire space directory from disk
pub async fn delete_space_dir(space_slug: &str) -> Result<(), std::io::Error> {
    let path = space_dir(space_slug);
    if path.exists() {
        fs::remove_dir_all(path).await?;
    }
    Ok(())
}

/// Sanitize a slug to prevent path traversal
pub fn sanitize_slug(input: &str) -> String {
    input
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
        .to_lowercase()
}

/// Validate that a path doesn't escape the data directory
pub fn validate_path(path: &str) -> bool {
    let path = Path::new(path);

    // Reject absolute paths
    if path.is_absolute() {
        return false;
    }

    // Reject path traversal
    for component in path.components() {
        if let std::path::Component::ParentDir = component {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{
        audit_integrity_in, compose_document_markdown, delete_owned_file_in, document_filename,
        is_safe_relative_path, migrate_document_filenames_in, stage_owned_file_copy_in,
    };
    use sqlx::any::AnyPoolOptions;
    use std::path::Path;

    #[test]
    fn publication_composition_preserves_css_raw_spans_and_normalizes_only_outside() {
        for css in [String::new(), "\r\n.notice { color: red; }\r\n".into(), "\0.notice { color: red; }".into(),
            "@import 'https://invalid.example/original';".into(), "x".repeat(20_100)] {
            let block = format!("<!-- starlane:custom-css -->{css}<!-- /starlane:custom-css -->");
            let source = format!("---\r\nsynopsis: original\r\n---\r\n\r\nBefore\r\n{block}\r\nAfter\r\n");
            assert_eq!(compose_document_markdown("title: owned", &source),
                format!("---\ntitle: owned\nsynopsis: original\n---\n\nBefore\n{block}\nAfter\n"));
        }
    }
    #[test]
    fn ambiguous_css_or_header_separator_inside_css_is_not_reconstructed() {
        for source in [" \r\n<!-- starlane:custom-css\r\noriginal\0 \r\n",
            "<!-- /starlane:custom-css -->\r\n<!-- starlane:custom-css -->",
            "<!-- starlane:custom-css --><!-- /starlane:custom-css --><!-- starlane:custom-css --><!-- /starlane:custom-css -->"] {
            assert_eq!(compose_document_markdown("title: owned", source), format!("---\ntitle: owned\n---\n\n{source}"));
        }
        let source = "---\r\n<!-- starlane:custom-css -->\r\n---\r\nraw\r\n<!-- /starlane:custom-css -->";
        assert_eq!(compose_document_markdown("title: owned", source),
            "---\ntitle: owned\n---\n\n---\n<!-- starlane:custom-css -->\r\n---\r\nraw\r\n<!-- /starlane:custom-css -->");
    }

    #[test]
    fn document_filenames_are_readable_stable_and_path_safe() {
        assert_eq!(
            document_filename("Example Page", "doc-2ddce166"),
            "Example Page-doc-2ddce166.md"
        );
        assert_eq!(
            document_filename("../ Quarterly / Plan ", "doc/../../1"),
            "Quarterly - Plan-doc-..-..-1.md"
        );
        assert_eq!(document_filename("///", ""), "Untitled-document.md");
    }

    #[test]
    fn storage_and_portable_frontmatter_are_written_as_one_markdown_header() {
        let markdown = compose_document_markdown(
            "title: \"Example Page\"\nlocked: false",
            "---\nsynopsis: \"Launch notes\"\npage_width: full\n---\n\nBody",
        );
        assert_eq!(markdown.matches("---\n").count(), 2);
        assert!(markdown.starts_with("---\ntitle: \"Example Page\"\nlocked: false\nsynopsis:"));
        assert!(markdown.ends_with("\n\nBody"));
    }

    #[test]
    fn integrity_paths_must_remain_relative_and_beneath_the_data_root() {
        assert!(is_safe_relative_path(Path::new("spaces/team/docs/page.md")));
        assert!(!is_safe_relative_path(Path::new("../outside.md")));
        assert!(!is_safe_relative_path(Path::new("/tmp/outside.md")));
        assert!(!is_safe_relative_path(Path::new("")));
    }

    #[tokio::test]
    async fn staged_storage_copy_keeps_source_until_database_ownership_can_commit() {
        let root =
            std::env::temp_dir().join(format!("starlane-storage-stage-{}", uuid::Uuid::new_v4()));
        let source = "spaces/source/docs/Page-doc-1.md";
        let destination = "spaces/target/docs/Page-doc-1.md";
        tokio::fs::create_dir_all(root.join("spaces/source/docs"))
            .await
            .unwrap();
        tokio::fs::write(root.join(source), "Body").await.unwrap();

        let staged = stage_owned_file_copy_in(&root, source, destination, true)
            .await
            .unwrap()
            .expect("copy should be staged");
        assert_eq!(
            tokio::fs::read_to_string(root.join(source)).await.unwrap(),
            "Body"
        );
        assert_eq!(
            tokio::fs::read_to_string(root.join(&staged.destination))
                .await
                .unwrap(),
            "Body"
        );
        assert!(stage_owned_file_copy_in(&root, source, destination, true)
            .await
            .is_err());

        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn owned_file_deletion_rejects_paths_outside_the_data_root() {
        let root =
            std::env::temp_dir().join(format!("starlane-storage-delete-{}", uuid::Uuid::new_v4()));
        let owned = "spaces/team/attachments/doc-1/asset.png";
        tokio::fs::create_dir_all(root.join("spaces/team/attachments/doc-1"))
            .await
            .unwrap();
        tokio::fs::write(root.join(owned), "image").await.unwrap();

        delete_owned_file_in(&root, owned).await.unwrap();
        assert!(!root.join(owned).exists());
        assert!(delete_owned_file_in(&root, "../../outside.png")
            .await
            .is_err());

        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn integrity_audit_reports_missing_misplaced_and_orphan_files_without_mutation() {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite fixture");
        for statement in [
            "CREATE TABLE spaces (id TEXT PRIMARY KEY, slug TEXT NOT NULL)",
            "CREATE TABLE documents (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, content_path TEXT NOT NULL)",
            "CREATE TABLE document_drafts (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, title TEXT NOT NULL)",
            "CREATE TABLE attachments (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, storage_path TEXT NOT NULL)",
            "INSERT INTO spaces (id, slug) VALUES ('space-1', 'team')",
            "INSERT INTO documents (id, space_id, content_path) VALUES ('doc-1', 'space-1', 'spaces/team/docs/Page-doc-1.md')",
            "INSERT INTO document_drafts (id, space_id, title) VALUES ('draft-1', 'space-1', 'Draft')",
            "INSERT INTO attachments (id, space_id, storage_path) VALUES ('asset-1', 'space-1', 'spaces/other/attachments/doc-1/asset.png')",
        ] {
            sqlx::query(statement).execute(&db).await.unwrap();
        }

        let root = std::env::temp_dir().join(format!(
            "starlane-storage-integrity-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::create_dir_all(root.join("spaces/team/docs"))
            .await
            .unwrap();
        tokio::fs::create_dir_all(root.join("spaces/team/drafts"))
            .await
            .unwrap();
        tokio::fs::write(root.join("spaces/team/docs/Page-doc-1.md"), "Body")
            .await
            .unwrap();
        let orphan = root.join("spaces/team/drafts/Old-draft.md");
        tokio::fs::write(&orphan, "Old body").await.unwrap();

        let report = audit_integrity_in(&root, &db).await.unwrap();
        assert!(!report.healthy);
        assert_eq!(
            (report.documents, report.drafts, report.attachments),
            (1, 1, 1)
        );
        assert!(!report.issues.iter().any(|issue| issue.kind == "missing_file"
            && issue.record_type == "draft"));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.kind == "misplaced_file"
                && issue.record_id.as_deref() == Some("asset-1")));
        assert!(report
            .issues
            .iter()
            .any(|issue| issue.kind == "orphan_file" && issue.path.ends_with("Old-draft.md")));
        assert_eq!(tokio::fs::read_to_string(orphan).await.unwrap(), "Old body");

        tokio::fs::remove_dir_all(root).await.unwrap();
    }

    #[tokio::test]
    async fn legacy_id_only_markdown_files_are_renamed_and_relinked() {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite fixture");
        sqlx::query("CREATE TABLE spaces (id TEXT PRIMARY KEY, slug TEXT NOT NULL)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content_path TEXT NOT NULL
            )",
        )
        .execute(&db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO spaces (id, slug) VALUES ('space-1', 'team')")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO documents (id, space_id, title, content_path) \
             VALUES ('doc-1', 'space-1', 'Example Page', 'spaces/team/docs/doc-1.md')",
        )
        .execute(&db)
        .await
        .unwrap();

        let root = std::env::temp_dir().join(format!(
            "starlane-document-filename-migration-{}",
            uuid::Uuid::new_v4()
        ));
        let legacy_path = root.join("spaces/team/docs/doc-1.md");
        tokio::fs::create_dir_all(legacy_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&legacy_path, "Body").await.unwrap();

        assert_eq!(migrate_document_filenames_in(&root, &db).await.unwrap(), 1);
        let migrated_path: (String,) =
            sqlx::query_as("SELECT content_path FROM documents WHERE id = 'doc-1'")
                .fetch_one(&db)
                .await
                .unwrap();
        assert_eq!(migrated_path.0, "spaces/team/docs/Example Page-doc-1.md");
        assert!(!legacy_path.exists());
        assert_eq!(
            tokio::fs::read_to_string(root.join(&migrated_path.0))
                .await
                .unwrap(),
            "Body"
        );

        tokio::fs::remove_dir_all(root).await.unwrap();
    }
    #[tokio::test]
    async fn sql_draft_absence_is_optional_but_canonical_and_existing_mirror_findings_remain() {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        for statement in [
            "CREATE TABLE spaces (id TEXT PRIMARY KEY, slug TEXT NOT NULL)",
            "CREATE TABLE documents (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, content_path TEXT NOT NULL)",
            "CREATE TABLE document_drafts (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, title TEXT NOT NULL, content TEXT NOT NULL)",
            "CREATE TABLE attachments (id TEXT PRIMARY KEY, space_id TEXT NOT NULL, storage_path TEXT NOT NULL)",
            "INSERT INTO spaces VALUES ('space','team')",
            "INSERT INTO document_drafts VALUES ('draft','space','Private','EXACT AUTHORITATIVE SQL BODY')",
        ] {sqlx::query(statement).execute(&db).await.unwrap();}
        let root=std::env::temp_dir().join(format!("optional-draft-audit-{}",uuid::Uuid::new_v4()));
        tokio::fs::create_dir_all(root.join("spaces/team/drafts")).await.unwrap();
        let report=audit_integrity_in(&root,&db).await.unwrap();assert!(report.healthy);assert_eq!(report.drafts,1);
        assert!(!root.join("spaces/team/drafts/Private-draft.md").exists());
        let before:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id='draft'").fetch_one(&db).await.unwrap();
        sqlx::query("INSERT INTO documents VALUES ('page','space','spaces/team/docs/Required-page.md')").execute(&db).await.unwrap();
        sqlx::query("INSERT INTO attachments VALUES ('asset','space','spaces/team/attachments/required.bin')").execute(&db).await.unwrap();
        let report=audit_integrity_in(&root,&db).await.unwrap();
        for kind in ["document","attachment"] {assert!(report.issues.iter().any(|i|i.kind=="missing_file"&&i.record_type==kind));}
        let path=root.join("spaces/team/drafts/Private-draft.md");tokio::fs::create_dir(&path).await.unwrap();
        let report=audit_integrity_in(&root,&db).await.unwrap();assert!(report.issues.iter().any(|i|i.kind=="invalid_file_type"&&i.record_type=="draft"));
        tokio::fs::remove_dir(&path).await.unwrap();
        #[cfg(unix)] {
            let foreign=root.join("foreign.txt");tokio::fs::write(&foreign,b"RETAIN FOREIGN").await.unwrap();std::os::unix::fs::symlink(&foreign,&path).unwrap();
            let report=audit_integrity_in(&root,&db).await.unwrap();assert!(report.issues.iter().any(|i|i.kind=="unsafe_symlink"&&i.record_type=="draft"));assert_eq!(tokio::fs::read(&foreign).await.unwrap(),b"RETAIN FOREIGN");
        }
        assert_eq!(sqlx::query_scalar::<_,String>("SELECT content FROM document_drafts WHERE id='draft'").fetch_one(&db).await.unwrap(),before);
        tokio::fs::remove_dir_all(root).await.unwrap();
    }

}

#[cfg(all(test, target_os = "linux"))]
mod restart_fixtures;

#[cfg(target_os = "linux")]
pub(crate) mod attachment_versions;

pub(crate) mod attachment_retirement;

#[cfg(target_os="linux")]
pub(crate) mod native_family_fence;

pub(crate) mod attachment_erasure_plan;

#[cfg(target_os="linux")]
#[path="attachment_erasure_source.rs"]
pub(crate) mod attachment_erasure_source;

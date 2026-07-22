use std::path::{Path, PathBuf};
use tokio::fs;

/// Get the data directory from environment or default
pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var("DATA_DIR").unwrap_or_else(|_| "data".to_string()))
}

/// Initialize the top-level data directory so volume/bind-mount wiring is visible on startup.
pub async fn init_data_dir() -> Result<(), std::io::Error> {
    let base = data_dir();
    fs::create_dir_all(base.join("spaces")).await?;
    fs::create_dir_all(base.join("uploads")).await?;
    fs::create_dir_all(base.join("search_index")).await?;
    fs::write(
        base.join(".starlane-notes-data-dir"),
        "Starlane Notes data directory\n",
    )
    .await?;
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

/// Initialize the directory structure for a new space
pub async fn init_space_dirs(space_slug: &str) -> Result<(), std::io::Error> {
    let base = space_dir(space_slug);
    fs::create_dir_all(base.join("docs")).await?;
    fs::create_dir_all(base.join("attachments")).await?;
    fs::create_dir_all(base.join("excalidraw")).await?;
    fs::create_dir_all(base.join(".starlane/templates")).await?;
    Ok(())
}

/// Write a document's markdown content to disk
pub async fn write_document(
    space_slug: &str,
    doc_id: &str,
    frontmatter: &str,
    content: &str,
) -> Result<String, std::io::Error> {
    let dir = docs_dir(space_slug);
    fs::create_dir_all(&dir).await?;

    let filename = format!("{}.md", doc_id);
    let path = dir.join(&filename);

    let full_content = if frontmatter.is_empty() {
        content.to_string()
    } else {
        format!("---\n{}\n---\n\n{}", frontmatter, content)
    };

    fs::write(&path, full_content).await?;

    // Return relative path from data dir
    Ok(format!("spaces/{}/docs/{}", space_slug, filename))
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

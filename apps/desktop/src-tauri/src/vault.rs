use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
#[cfg(any(target_os = "ios", target_os = "macos"))]
use std::ffi::{c_char, c_void, CStr, CString};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
#[cfg(any(
    target_os = "macos",
    target_os = "windows",
    target_os = "linux",
    target_os = "freebsd"
))]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tauri_plugin_store::StoreExt;
use zip::write::SimpleFileOptions;

const SETTINGS_FILE: &str = "settings.json";
const VAULT_PATH_KEY: &str = "vault_path";
#[cfg(target_os = "ios")]
const VAULT_DOCUMENTS_RELATIVE_PATH_KEY: &str = "vault_documents_relative_path";
#[cfg(any(target_os = "ios", test))]
const IOS_VAULT_LOCATION_FALLBACK_FILE: &str = ".starlane-vault-location.json";
const ARCHIVE_SNAPSHOT_FILE: &str = "vault.json";
const ARCHIVE_ASSETS_DIR: &str = "assets";
const MIRROR_MANIFEST_FILE: &str = ".starlane-mirror-manifest.json";
const MIRROR_SPACES_DIR: &str = "spaces";
const MIRROR_PERSONAL_SPACES_DIR: &str = "personalspaces";
const MIRROR_DOCUMENTS_DIR: &str = "documents";
const MIRROR_ATTACHMENTS_DIR: &str = "attachments";
const MIRROR_TEMPLATES_DIR: &str = "templates";
const MIRROR_DRAFTS_DIR: &str = "drafts";
const MIRROR_ARCHIVED_DIR: &str = "archived";
const MIRROR_TRASHED_DIR: &str = "trashed";
const VAULT_INTERNAL_DIR: &str = ".starlane";
#[cfg(any(target_os = "ios", target_os = "android"))]
const VAULT_MARKER_FILE: &str = "README.txt";
const LOCAL_VAULT_SCHEMA_VERSION: u64 = 1;
const MAX_BROWSER_ASSET_BYTES: usize = 100 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultLocation {
    path: String,
    using_default: bool,
    #[serde(default)]
    configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultFolderSummary {
    name: String,
    path: String,
    current: bool,
}

#[cfg(any(target_os = "ios", test))]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IosVaultLocationFallback {
    #[serde(default)]
    vault_path: Option<String>,
    #[serde(default)]
    vault_documents_relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ICloudProviderSyncManifestWriteResult {
    path: String,
    relative_path: String,
    bytes_written: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultAsset {
    id: String,
    name: String,
    mime_type: Option<String>,
    size: u64,
    url: String,
    // Browser assets have no native path; raw archive JSON remains authoritative.
    #[serde(default)]
    vault_path: String,
    source: String,
    #[serde(default)]
    space_id: Option<String>,
    #[serde(default)]
    document_id: Option<String>,
    created_at: String,
    /// Exact readonly server provenance; never an upload or native install grant.
    #[serde(default, skip_serializing_if="Option::is_none")]
    server_version_provenance: Option<Value>,
    /// Local original creator history; serving still requires native held proof.
    #[serde(default, skip_serializing_if="Option::is_none")]
    native_local_version_provenance: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct VaultSnapshot {
    schema_version: u64,
    #[serde(default)]
    spaces: Vec<Value>,
    #[serde(default)]
    folders: Vec<Value>,
    #[serde(default)]
    documents: Vec<Value>,
    #[serde(default)]
    drafts: Vec<Value>,
    #[serde(default)]
    assets: Vec<VaultAsset>,
    #[serde(default)]
    templates: Vec<Value>,
}

#[path = "vault/mirror_admission.rs"]
mod mirror_admission;

#[derive(Debug, Clone, Serialize)]
pub struct MirrorReconciliationResult {
    snapshot: Value,
    files_scanned: usize,
    documents_updated: usize,
    templates_updated: usize,
    documents_imported: usize,
    templates_imported: usize,
}

fn sanitize_provider(provider: &str) -> String {
    provider
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-' || *ch == '_')
        .collect::<String>()
}

fn snapshot_file_name(provider: &str) -> String {
    let sanitized = sanitize_provider(provider);
    let provider_name = if sanitized.is_empty() {
        "none".to_string()
    } else {
        sanitized
    };
    format!("starlane-vault-{provider_name}.json")
}

fn legacy_native_database_file_name(provider: &str) -> String {
    let sanitized = sanitize_provider(provider);
    let provider_name = if sanitized.is_empty() {
        "none".to_string()
    } else {
        sanitized
    };
    format!("starlane-vault-{provider_name}.sqlite3")
}

fn native_database_file_name(provider: &str) -> String {
    let sanitized = sanitize_provider(provider);
    if sanitized.is_empty() || sanitized == "none" {
        "starlane-notes.sqlite3".to_string()
    } else {
        format!("starlane-vault-{sanitized}.sqlite3")
    }
}

fn sanitize_file_name(name: &str) -> String {
    let sanitized = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();

    if sanitized.trim_matches('-').is_empty() {
        "asset".to_string()
    } else {
        sanitized
    }
}

fn sanitize_path_segment(name: &str, fallback: &str) -> String {
    let sanitized = name
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == ' ' || ch == '.' || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches(|ch| ch == ' ' || ch == '.' || ch == '-')
        .chars()
        .take(80)
        .collect::<String>();

    if sanitized.is_empty() {
        sanitize_file_name(fallback)
    } else {
        sanitized
    }
}

fn sanitize_space_dir_name(name: &str, fallback: &str) -> String {
    sanitize_path_segment(name, fallback).replace(' ', "_")
}

fn value_array<'a>(value: &'a Value, key: &str) -> Vec<&'a Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().collect())
        .unwrap_or_default()
}

fn value_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn yaml_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn parse_mirror_value(value: &str) -> String {
    let trimmed = value.trim();
    serde_json::from_str::<String>(trimmed).unwrap_or_else(|_| trimmed.to_string())
}

fn parse_markdown_mirror_file(contents: &str) -> Option<(HashMap<String, String>, String)> {
    let rest = contents.strip_prefix("---\n")?;
    let (frontmatter, body) = rest.split_once("\n---")?;
    let body = body
        .strip_prefix("\n\n")
        .or_else(|| body.strip_prefix('\n'))
        .unwrap_or(body);
    let mut metadata = HashMap::new();

    for line in frontmatter.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        metadata.insert(key.to_string(), parse_mirror_value(value));
    }

    // The generator appends exactly one framing newline. Removing every
    // newline here silently changed otherwise untouched saved bodies.
    Some((metadata, body.strip_suffix('\n').unwrap_or(body).to_string()))
}

fn parse_markdown_import_file(contents: &str) -> (HashMap<String, String>, String) {
    parse_markdown_mirror_file(contents)
        .unwrap_or_else(|| (HashMap::new(), contents.trim_end_matches('\n').to_string()))
}

pub(crate) fn infer_mime_type(path: &Path) -> Option<String> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "md" => "text/markdown",
        "txt" => "text/plain",
        "json" => "application/json",
        _ => return None,
    };
    Some(mime.to_string())
}

fn now_nanos() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .map_err(|e| e.to_string())
}

fn provider_sync_manifest_relative_path(mutation_id: &str) -> PathBuf {
    let file_stem = sanitize_path_segment(mutation_id, "provider-sync-manifest");
    PathBuf::from(".starlane")
        .join("provider-sync")
        .join(format!("{file_stem}.json"))
}

fn write_provider_sync_manifest_file(
    root: &Path,
    mutation_id: &str,
    manifest: &Value,
) -> Result<ICloudProviderSyncManifestWriteResult, String> {
    let relative_path = provider_sync_manifest_relative_path(mutation_id);
    let path = root.join(&relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|e| e.to_string())?;
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    Ok(ICloudProviderSyncManifestWriteResult {
        path: path.to_string_lossy().into_owned(),
        relative_path: relative_path.to_string_lossy().replace('\\', "/"),
        bytes_written: bytes.len() as u64,
    })
}

fn asset_url(path: &Path) -> String {
    format!(
        "asset://localhost/{}",
        urlencoding::encode(&path.to_string_lossy())
    )
}

fn asset_path_from_url(url: &str) -> Option<PathBuf> {
    let encoded = url.strip_prefix("asset://localhost/")?;
    let decoded = urlencoding::decode(encoded).ok()?;
    Some(PathBuf::from(decoded.into_owned()))
}

fn archive_asset_name(asset: &VaultAsset) -> String {
    format!("{}-{}", asset.id, sanitize_file_name(&asset.name))
}

pub(crate) fn archive_asset_path(asset: &VaultAsset) -> String {
    format!("{ARCHIVE_ASSETS_DIR}/{}", archive_asset_name(asset))
}

fn write_asset_bytes(
    app: &AppHandle,
    provider: &str,
    space_id: Option<String>,
    document_id: Option<String>,
    name: String,
    mime_type: Option<String>,
    bytes: &[u8],
) -> Result<VaultAsset, String> {
    if bytes.is_empty() {
        return Err("Selected file is empty".to_string());
    }
    if bytes.len() > MAX_BROWSER_ASSET_BYTES {
        return Err("Selected file exceeds the 100 MB vault asset limit".to_string());
    }

    let safe_name = sanitize_file_name(&name);
    let timestamp = now_nanos()?;
    let id = format!("asset-{timestamp}");
    let assets_dir = match resolve_attachment_assets_dir(
        app,
        provider,
        space_id.as_deref(),
        document_id.as_deref(),
    )? {
        Some(path) => path,
        None => resolve_assets_dir(app)?,
    };
    let destination = assets_dir.join(format!("{id}-{safe_name}"));

    fs::write(&destination, bytes).map_err(|e| e.to_string())?;
    let metadata = fs::metadata(&destination).map_err(|e| e.to_string())?;

    Ok(VaultAsset {
        id,
        name,
        mime_type: mime_type.or_else(|| infer_mime_type(&destination)),
        size: metadata.len(),
        url: asset_url(&destination),
        vault_path: destination.to_string_lossy().into_owned(),
        source: "tauri".to_string(),
        space_id,
        document_id,
        created_at: timestamp.to_string(),
        server_version_provenance: None,
        native_local_version_provenance: None,
    })
}

pub(crate) fn delete_vault_asset_file(vault_dir: &Path, candidate: &Path) -> Result<(), String> {
    // Immutable owned-download bytes belong to their retained receipt lifecycle.
    // Ordinary asset deletion must never erase either install-proof group member.
    let owned = vault_dir.join(".starlane").join("attachment-downloads");
    if candidate.starts_with(vault_dir.join("assets").join("local-imports")) || candidate.starts_with(vault_dir.join(".starlane").join("local-import-producers")) {
        return Err("Original imported bytes require their owned retention lifecycle".into());
    }
    let local_owned = vault_dir.join("assets").join("local-covers");
    let local_creator = vault_dir.join(".starlane").join("local-cover-producers");
    let import_creator = vault_dir.join(".starlane").join("archive-imports");
    if candidate.starts_with(&import_creator) {return Err("Imported creator evidence requires its owned retention lifecycle".into());}
    if candidate.starts_with(&local_owned) || candidate.starts_with(&local_creator) {
        return Err("Immutable local cover requires its owned retention lifecycle".into());
    }
    if candidate.starts_with(&owned) {
        return Err("Immutable downloaded attachment requires its owned retention lifecycle".into());
    }

    if !candidate.exists() {
        let mut existing_parent = candidate
            .parent()
            .ok_or_else(|| "Native asset path has no parent directory".to_string())?;
        while !existing_parent.exists() {
            existing_parent = existing_parent
                .parent()
                .ok_or_else(|| "Native asset path has no existing parent directory".to_string())?;
        }
        let canonical_vault = fs::canonicalize(vault_dir).map_err(|e| e.to_string())?;
        let canonical_parent = fs::canonicalize(existing_parent).map_err(|e| e.to_string())?;
        if !canonical_parent.starts_with(&canonical_vault) {
            return Err("Native asset path is outside the active vault".to_string());
        }
        return Ok(());
    }
    let asset_path = validate_replay_asset_path(vault_dir, candidate)?;
    if !asset_path.is_file() {
        return Err("Native asset path is not a file".to_string());
    }
    let canonical_owned = fs::canonicalize(&owned).ok();
    if canonical_owned.as_ref().is_some_and(|root| asset_path.starts_with(root)) {
        return Err("Immutable downloaded attachment requires its owned retention lifecycle".into());
    }
    if [local_owned, local_creator, import_creator].iter().filter_map(|root| fs::canonicalize(root).ok()).any(|root| asset_path.starts_with(root)) {
        return Err("Immutable local cover requires its owned retention lifecycle".into());
    }
    // The generic legacy path owns no shared/imported file group. This is a
    // refusal only; it cannot authorize any member of a sealed creator family.
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if fs::symlink_metadata(&asset_path).map_err(|e|e.to_string())?.nlink()!=1 {
            return Err("Shared or imported native asset requires its owned retention lifecycle".into());
        }
    }
    fs::remove_file(asset_path).map_err(|e| e.to_string())
}

fn replace_snapshot_file(temp_path: &Path, snapshot_path: &Path) -> Result<(), String> {
    #[cfg(not(target_os = "windows"))]
    {
        return fs::rename(temp_path, snapshot_path).map_err(|e| e.to_string());
    }

    #[cfg(target_os = "windows")]
    {
        let backup_path = snapshot_path.with_extension("json.backup");
        if backup_path.exists() {
            fs::remove_file(&backup_path).map_err(|e| e.to_string())?;
        }
        if snapshot_path.exists() {
            fs::rename(snapshot_path, &backup_path).map_err(|e| e.to_string())?;
        }
        if let Err(error) = fs::rename(temp_path, snapshot_path) {
            if backup_path.exists() {
                let _ = fs::rename(&backup_path, snapshot_path);
            }
            return Err(error.to_string());
        }
        if backup_path.exists() {
            fs::remove_file(backup_path).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

fn parse_data_url(data_url: &str) -> Result<(Option<String>, Vec<u8>), String> {
    let Some(rest) = data_url.strip_prefix("data:") else {
        return Err("Expected a data URL".to_string());
    };
    let Some((header, payload)) = rest.split_once(',') else {
        return Err("Invalid data URL".to_string());
    };

    let mut mime_type = None;
    let mut is_base64 = false;
    for (index, part) in header.split(';').enumerate() {
        if index == 0 && !part.is_empty() {
            mime_type = Some(part.to_string());
        }
        if part.eq_ignore_ascii_case("base64") {
            is_base64 = true;
        }
    }

    if is_base64 {
        let bytes = general_purpose::STANDARD
            .decode(payload)
            .map_err(|e| e.to_string())?;
        Ok((mime_type, bytes))
    } else {
        let decoded = urlencoding::decode(payload).map_err(|e| e.to_string())?;
        Ok((mime_type, decoded.as_bytes().to_vec()))
    }
}

fn validate_replay_asset_path(vault_dir: &Path, candidate: &Path) -> Result<PathBuf, String> {
    let canonical_candidate = fs::canonicalize(candidate)
        .map_err(|_| "Native replay asset file was not found".to_string())?;
    if !canonical_candidate.is_file() {
        return Err("Native replay asset file was not found".to_string());
    }

    let canonical_vault_dir = fs::canonicalize(vault_dir)
        .map_err(|_| "Failed to resolve vault directory for native replay asset".to_string())?;
    if !canonical_candidate.starts_with(&canonical_vault_dir) {
        return Err("Native replay asset file must live under the active vault".to_string());
    }

    Ok(canonical_candidate)
}

#[tauri::command]
pub fn pack_native_replay_asset_payload(
    app: AppHandle,
    _provider: String,
    vault_path: Option<String>,
    url: Option<String>,
    mime_type: Option<String>,
) -> Result<String, String> {
    let candidate = vault_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            url.as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(asset_path_from_url)
        })
        .ok_or_else(|| "Native replay asset path is unavailable".to_string())?;

    let (vault_dir, _) = resolve_vault_dir(&app)?;
    ensure_vault_dir(&vault_dir)?;
    let asset_path = validate_replay_asset_path(&vault_dir, &candidate)?;
    let bytes = fs::read(&asset_path).map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err("Native replay asset file is empty".to_string());
    }
    if bytes.len() > MAX_BROWSER_ASSET_BYTES {
        return Err("Native replay asset file exceeds the 100 MB vault asset limit".to_string());
    }

    let asset_mime_type = mime_type
        .filter(|value| !value.trim().is_empty())
        .or_else(|| infer_mime_type(&asset_path))
        .unwrap_or_else(|| "application/octet-stream".to_string());
    let encoded = general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{asset_mime_type};base64,{encoded}"))
}

fn assert_archive_path(path: &str) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("Archive path is required".to_string());
    }
    Ok(())
}

fn add_file_to_archive<W: Write + Seek>(
    archive: &mut zip::ZipWriter<W>,
    archive_path: &str,
    source_path: &Path,
    options: SimpleFileOptions,
) -> Result<(), String> {
    let mut source = fs::File::open(source_path).map_err(|e| e.to_string())?;
    archive
        .start_file(archive_path, options)
        .map_err(|e| e.to_string())?;
    std::io::copy(&mut source, archive).map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_archive_snapshot(contents: &str) -> Result<VaultSnapshot, String> {
    let value: Value = serde_json::from_str(contents).map_err(|e| e.to_string())?;
    if value.get("schema_version").and_then(Value::as_u64) != Some(LOCAL_VAULT_SCHEMA_VERSION) {
        return Err("Unsupported vault schema version".to_string());
    }
    if !value.get("spaces").is_some_and(Value::is_array)
        || !value.get("folders").is_some_and(Value::is_array)
        || !value.get("documents").is_some_and(Value::is_array)
    {
        return Err("Vault file is missing required collections".to_string());
    }

    if let Some(assets) = value.get("assets").and_then(Value::as_array) {
        for asset in assets {
            if asset.get("source").and_then(Value::as_str) == Some("tauri")
                && asset.get("vault_path").and_then(Value::as_str).is_none_or(|path| path.trim().is_empty())
            {
                return Err("Native archive asset path must be a nonempty string".into());
            }
        }
    }

    for document in value["documents"].as_array().unwrap() {
        let mut assigned = 0;
        for field in ["maintenance_user_id", "maintenance_group_id"] {
            if let Some(target) = document.get(field) {
                if target.is_null() {
                    continue;
                }
                if target.as_str().is_none_or(|id| id.trim().is_empty()) {
                    return Err("Maintenance identity must be a nonempty string or null".into());
                }
                assigned += 1;
            }
        }
        if assigned > 1 {
            return Err("A page cannot have both a maintenance person and team".into());
        }
    }

    serde_json::from_value(value).map_err(|e| e.to_string())
}

fn default_vault_dir<R:tauri::Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    #[cfg(any(target_os = "ios", target_os = "android"))]
    {
        return app
            .path()
            .document_dir()
            .map(|path| path.join("Starlane Notes Vault"))
            .map_err(|e| e.to_string());
    }

    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    app.path()
        .app_data_dir()
        .map(|path| path.join("vault"))
        .map_err(|e| e.to_string())
}

fn configured_vault_dir(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    let configured = store
        .get(VAULT_PATH_KEY)
        .and_then(|value| value.as_str().map(PathBuf::from));

    #[cfg(target_os = "ios")]
    if let Some(relative_path) = store
        .get(VAULT_DOCUMENTS_RELATIVE_PATH_KEY)
        .and_then(|value| value.as_str().map(PathBuf::from))
    {
        let documents_dir = app.path().document_dir().map_err(|e| e.to_string())?;
        if is_safe_documents_relative_path(&relative_path) {
            return Ok(Some(documents_dir.join(relative_path)));
        }
        store.delete(VAULT_DOCUMENTS_RELATIVE_PATH_KEY);
        store.save().map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "ios")]
    if let Some(path) = configured {
        let documents_dir = app.path().document_dir().map_err(|e| e.to_string())?;
        let migrated = remap_stale_ios_sandbox_vault_path(&path, &documents_dir);
        if let Some(relative_path) = documents_relative_path(&migrated, &documents_dir) {
            store.set(
                VAULT_DOCUMENTS_RELATIVE_PATH_KEY,
                serde_json::json!(relative_path.to_string_lossy()),
            );
            store.delete(VAULT_PATH_KEY);
            store.save().map_err(|e| e.to_string())?;
            write_ios_vault_location_fallback(&documents_dir, &migrated)?;
            return Ok(Some(documents_dir.join(relative_path)));
        }
        if migrated != path {
            store.set(
                VAULT_PATH_KEY,
                serde_json::json!(migrated.to_string_lossy()),
            );
            store.save().map_err(|e| e.to_string())?;
            write_ios_vault_location_fallback(&documents_dir, &migrated)?;
        }
        return Ok(Some(migrated));
    }

    #[cfg(target_os = "ios")]
    {
        let documents_dir = app.path().document_dir().map_err(|e| e.to_string())?;
        if let Some(path) = read_ios_vault_location_fallback(&documents_dir)? {
            persist_ios_vault_location_to_store(&store, &path, &documents_dir)?;
            return Ok(Some(path));
        }
    }

    Ok(configured)
}

#[cfg(any(target_os = "ios", test))]
fn ios_vault_location_fallback_path(documents_dir: &Path) -> PathBuf {
    documents_dir.join(IOS_VAULT_LOCATION_FALLBACK_FILE)
}

#[cfg(any(target_os = "ios", test))]
fn fallback_payload_for_ios_vault_path(
    vault_path: &Path,
    documents_dir: &Path,
) -> IosVaultLocationFallback {
    let relative = documents_relative_path(vault_path, documents_dir)
        .map(|path| path.to_string_lossy().into_owned());
    IosVaultLocationFallback {
        vault_path: relative
            .is_none()
            .then(|| vault_path.to_string_lossy().into_owned()),
        vault_documents_relative_path: relative,
    }
}

#[cfg(any(target_os = "ios", test))]
fn resolve_ios_vault_location_fallback(
    fallback: IosVaultLocationFallback,
    documents_dir: &Path,
) -> Option<PathBuf> {
    if let Some(relative_path) = fallback
        .vault_documents_relative_path
        .as_deref()
        .map(PathBuf::from)
        .filter(|path| is_safe_documents_relative_path(path))
    {
        return Some(documents_dir.join(relative_path));
    }

    fallback.vault_path.as_deref().map(PathBuf::from)
}

#[cfg(any(target_os = "ios", test))]
fn write_ios_vault_location_fallback(
    documents_dir: &Path,
    vault_path: &Path,
) -> Result<(), String> {
    fs::create_dir_all(documents_dir).map_err(|e| e.to_string())?;
    let fallback = fallback_payload_for_ios_vault_path(vault_path, documents_dir);
    let contents = serde_json::to_string_pretty(&fallback).map_err(|e| e.to_string())?;
    fs::write(ios_vault_location_fallback_path(documents_dir), contents).map_err(|e| e.to_string())
}

#[cfg(any(target_os = "ios", test))]
fn read_ios_vault_location_fallback(documents_dir: &Path) -> Result<Option<PathBuf>, String> {
    let path = ios_vault_location_fallback_path(documents_dir);
    if !path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let fallback: IosVaultLocationFallback =
        serde_json::from_str(&contents).map_err(|e| e.to_string())?;
    Ok(resolve_ios_vault_location_fallback(fallback, documents_dir))
}

#[cfg(target_os = "ios")]
fn persist_ios_vault_location_to_store(
    store: &tauri_plugin_store::Store<tauri::Wry>,
    vault_path: &Path,
    documents_dir: &Path,
) -> Result<(), String> {
    if let Some(relative_path) = documents_relative_path(vault_path, documents_dir) {
        store.set(
            VAULT_DOCUMENTS_RELATIVE_PATH_KEY,
            serde_json::json!(relative_path.to_string_lossy()),
        );
        store.delete(VAULT_PATH_KEY);
    } else {
        store.set(
            VAULT_PATH_KEY,
            serde_json::json!(vault_path.to_string_lossy()),
        );
        store.delete(VAULT_DOCUMENTS_RELATIVE_PATH_KEY);
    }
    store.save().map_err(|e| e.to_string())
}

#[cfg(any(target_os = "ios", test))]
fn is_safe_documents_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.is_relative()
        && path.components().all(|component| {
            matches!(
                component,
                std::path::Component::Normal(_) | std::path::Component::CurDir
            )
        })
}

#[cfg(any(target_os = "ios", test))]
fn documents_relative_path(path: &Path, documents_dir: &Path) -> Option<PathBuf> {
    let relative_path = path.strip_prefix(documents_dir).ok()?.to_path_buf();
    is_safe_documents_relative_path(&relative_path).then_some(relative_path)
}

#[cfg(any(target_os = "ios", test))]
fn remap_stale_ios_sandbox_vault_path(configured: &Path, documents_dir: &Path) -> PathBuf {
    if configured.starts_with(documents_dir) {
        return configured.to_path_buf();
    }

    let mut found_documents_dir = false;
    let mut found_application_container = false;
    let mut relative_path = PathBuf::new();
    for component in configured.components() {
        match component {
            std::path::Component::Normal(name) if name == "Application" => {
                found_application_container = true;
            }
            std::path::Component::Normal(name) if !found_documents_dir && name == "Documents" => {
                found_documents_dir = true;
            }
            std::path::Component::Normal(name) if found_documents_dir => relative_path.push(name),
            _ if found_documents_dir => return configured.to_path_buf(),
            _ => {}
        }
    }

    if found_application_container && found_documents_dir && !relative_path.as_os_str().is_empty() {
        return documents_dir.join(relative_path);
    }

    configured.to_path_buf()
}

fn configured_or_current_vault_name(app: &AppHandle) -> Result<String, String> {
    let (path, _) = resolve_vault_dir(app)?;
    Ok(path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| sanitize_path_segment(name, "Starlane Notes Vault"))
        .unwrap_or_else(|| "Starlane Notes Vault".to_string()))
}

fn resolve_vault_dir(app: &AppHandle) -> Result<(PathBuf, bool), String> {
    if let Some(path) = configured_vault_dir(app)? {
        return Ok((path, false));
    }
    Ok((default_vault_dir(app)?, true))
}

fn resolve_vault_location(app: &AppHandle) -> Result<(PathBuf, bool, bool), String> {
    if let Some(path) = configured_vault_dir(app)? {
        return Ok((path, false, true));
    }
    Ok((default_vault_dir(app)?, true, false))
}

fn vault_parent_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let (path, _) = resolve_vault_dir(app)?;
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "Vault folder has no parent directory".to_string())
}

fn ensure_vault_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    ensure_mobile_vault_marker(path)
}

fn copy_dir_contents_missing(source: &Path, destination: &Path) -> Result<(), String> {
    if source == destination || !source.exists() {
        return Ok(());
    }

    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(source).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_contents_missing(&source_path, &destination_path)?;
        } else if !destination_path.exists() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(&source_path, &destination_path).map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

#[cfg(any(target_os = "ios", target_os = "macos"))]
fn apple_icloud_documents_dir() -> Result<PathBuf, String> {
    type Id = *mut c_void;
    type Sel = *mut c_void;

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Id;
        fn sel_registerName(name: *const c_char) -> Sel;
        fn objc_msgSend();
    }

    #[link(name = "Foundation", kind = "framework")]
    extern "C" {}

    unsafe fn class(name: &str) -> Result<Id, String> {
        let name = CString::new(name).map_err(|e| e.to_string())?;
        let class = objc_getClass(name.as_ptr());
        if class.is_null() {
            Err("Required Apple Foundation class is unavailable".to_string())
        } else {
            Ok(class)
        }
    }

    unsafe fn selector(name: &str) -> Result<Sel, String> {
        let name = CString::new(name).map_err(|e| e.to_string())?;
        let selector = sel_registerName(name.as_ptr());
        if selector.is_null() {
            Err("Required Apple Foundation selector is unavailable".to_string())
        } else {
            Ok(selector)
        }
    }

    unsafe fn ns_string(value: &str) -> Result<Id, String> {
        let ns_string_class = class("NSString")?;
        let selector = selector("stringWithUTF8String:")?;
        let value = CString::new(value).map_err(|e| e.to_string())?;
        let msg: unsafe extern "C" fn(Id, Sel, *const c_char) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let result = msg(ns_string_class, selector, value.as_ptr());
        if result.is_null() {
            Err("Failed to create NSString".to_string())
        } else {
            Ok(result)
        }
    }

    unsafe fn append_path_component(url: Id, component: &str) -> Result<Id, String> {
        let selector = selector("URLByAppendingPathComponent:")?;
        let component = ns_string(component)?;
        let msg: unsafe extern "C" fn(Id, Sel, Id) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let result = msg(url, selector, component);
        if result.is_null() {
            Err("Failed to append iCloud path component".to_string())
        } else {
            Ok(result)
        }
    }

    unsafe {
        let file_manager_class = class("NSFileManager")?;
        let default_manager_selector = selector("defaultManager")?;
        let default_manager_msg: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let file_manager = default_manager_msg(file_manager_class, default_manager_selector);
        if file_manager.is_null() {
            return Err("Apple file manager is unavailable".to_string());
        }

        let ubiquity_selector = selector("URLForUbiquityContainerIdentifier:")?;
        let ubiquity_msg: unsafe extern "C" fn(Id, Sel, Id) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let container_url = ubiquity_msg(file_manager, ubiquity_selector, std::ptr::null_mut());
        if container_url.is_null() {
            return Err(
                "iCloud Drive is unavailable for this app. Enable iCloud Drive on the device and ensure the app identifier has the iCloud Documents entitlement."
                    .to_string(),
            );
        }

        let documents_url = append_path_component(container_url, "Documents")?;
        let starlane_url = append_path_component(documents_url, "Starlane Notes")?;
        let path_selector = selector("path")?;
        let path_msg: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let path_string = path_msg(starlane_url, path_selector);
        if path_string.is_null() {
            return Err("Unable to resolve iCloud vault path".to_string());
        }

        let utf8_selector = selector("UTF8String")?;
        let utf8_msg: unsafe extern "C" fn(Id, Sel) -> *const c_char =
            std::mem::transmute(objc_msgSend as *const ());
        let path = utf8_msg(path_string, utf8_selector);
        if path.is_null() {
            return Err("Unable to read iCloud vault path".to_string());
        }

        Ok(PathBuf::from(
            CStr::from_ptr(path).to_string_lossy().into_owned(),
        ))
    }
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
fn apple_icloud_documents_dir() -> Result<PathBuf, String> {
    Err("iCloud Drive vaults are only available on Apple platforms".to_string())
}

#[cfg(any(target_os = "ios", target_os = "android"))]
fn ensure_mobile_vault_marker(path: &Path) -> Result<(), String> {
    let marker_path = path.join(VAULT_MARKER_FILE);
    if marker_path.exists() {
        return Ok(());
    }

    fs::write(
        marker_path,
        "This folder stores the local Starlane Notes vault for this device.\n",
    )
    .map_err(|e| e.to_string())
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
fn ensure_mobile_vault_marker(_path: &Path) -> Result<(), String> {
    Ok(())
}

fn relative_path(vault_dir: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(vault_dir)
        .ok()
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
}

// Cleanup authority is only the complete validated generated Markdown footprint.
// This does not make new mirror writes or arbitrary filesystem writers atomic.
mod mirror_cleanup {
    use super::*;
    fn error(detail: &str) -> String {
        format!("Vault mirror operation stopped ({detail}); review the manifest and remaining files")
    }
    fn segments(relative: &str) -> Result<Vec<&str>, String> {
        if relative.contains('\\') || relative.contains('\0') { return Err(error("unsafe path separator")); }
        let parts=relative.split('/').collect::<Vec<_>>();
        if parts.iter().any(|p| p.is_empty() || *p=="." || *p==".." || p.contains(':')
            || !p.bytes().all(|b| b.is_ascii_alphanumeric() || b==b' ' || b==b'.' || b==b'-' || b==b'_')) {
            return Err(error("non-generated path component"));
        }
        let kind=|offset:usize| -> bool {
            let Some(bucket)=parts.get(offset) else { return false; };
            let remaining=parts.len()-offset-1;
            matches!(*bucket,MIRROR_DOCUMENTS_DIR) && remaining>=1
                || matches!(*bucket,MIRROR_DRAFTS_DIR|MIRROR_TEMPLATES_DIR|MIRROR_ARCHIVED_DIR|MIRROR_TRASHED_DIR) && remaining==1
        };
        // Valid UNION: an ordinary space may literally be personalspaces;
        // a personal owner's name may itself equal documents or attachments.
        let ordinary=parts.len()>=4 && parts[0]==MIRROR_SPACES_DIR && kind(2);
        let personal=parts.len()>=5 && parts[0]==MIRROR_SPACES_DIR && parts[1]==MIRROR_PERSONAL_SPACES_DIR && kind(3);
        let filename=parts.last().copied().unwrap_or("");
        // A literal ordinary personalspaces/documents/folder can alias a
        // personal owner's canonical attachments. Without a proved layout,
        // ambiguity is retained rather than borrowing the ordinary parse.
        let protected_personal_bucket=parts.get(1)==Some(&MIRROR_PERSONAL_SPACES_DIR)
            && parts.get(3)==Some(&MIRROR_ATTACHMENTS_DIR);
        if protected_personal_bucket || parts.iter().any(|p| *p==VAULT_INTERNAL_DIR)
            || !(ordinary||personal) || !filename.ends_with(".md") || !filename.contains('-') || filename.len()<=4 {
            return Err(error("path is not a generated Markdown mirror"));
        }
        Ok(parts)
    }
    #[cfg(unix)]
    mod rooted {
        use super::*;
        use std::os::fd::{AsRawFd,FromRawFd};
        use std::os::unix::{ffi::OsStrExt,fs::MetadataExt};
        pub(super) fn id(file:&fs::File)->Result<(u64,u64),String> {
            let m=file.metadata().map_err(|e|e.to_string())?; Ok((m.dev(),m.ino()))
        }
        pub(super) fn regular(file:&fs::File)->Result<(),String> {
            let metadata=file.metadata().map_err(|e|e.to_string())?;
            if !metadata.is_file() || metadata.nlink()!=1 {return Err(error("nonregular or hardlinked mirror file"));}
            Ok(())
        }
        fn c(leaf:&std::ffi::OsStr)->Result<std::ffi::CString,String> {
            std::ffi::CString::new(leaf.as_bytes()).map_err(|_|error("invalid native path"))
        }
        pub(super) fn open(parent:&fs::File,leaf:&std::ffi::OsStr,directory:bool)->Result<Option<fs::File>,String> {
            let leaf=c(leaf)?;
            let flags=libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_CLOEXEC|if directory {libc::O_DIRECTORY} else {libc::O_NONBLOCK};
            let fd=unsafe {libc::openat(parent.as_raw_fd(),leaf.as_ptr(),flags)};
            if fd<0 {
                let e=std::io::Error::last_os_error();
                if e.kind()==std::io::ErrorKind::NotFound {return Ok(None);}
                return Err(error(&format!("no-follow path check: {e}")));
            }
            Ok(Some(unsafe {fs::File::from_raw_fd(fd)}))
        }
        pub(super) fn root(path:&Path)->Result<Option<fs::File>,String> {
            if !path.is_absolute() {return Err(error("vault root is not absolute"));}
            // Existing system ancestors may be aliases (Apple /var -> /private/var).
            // Resolve only the parent; the selected vault leaf must remain no-follow.
            // Every reproof repeats this resolution from the ORIGINAL selection path.
            for component in path.components() {
                if !matches!(component,std::path::Component::RootDir|std::path::Component::Normal(_)) {return Err(error("unsafe vault root component"));}
            }
            let leaf=path.file_name().ok_or_else(||error("vault root has no final leaf"))?;
            let parent=path.parent().ok_or_else(||error("vault root has no parent"))?;
            let physical_parent=match fs::canonicalize(parent) {
                Ok(parent)=>parent,
                Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),
                Err(e)=>return Err(error(&format!("vault parent resolution: {e}"))),
            };
            let mut held=fs::File::open("/").map_err(|e|e.to_string())?;
            for component in physical_parent.components() {
                match component {
                    std::path::Component::RootDir=>continue,
                    std::path::Component::Normal(leaf)=>match open(&held,leaf,true)? {Some(next)=>held=next,None=>return Ok(None)},
                    _=>return Err(error("unsafe vault root component")),
                }
            }
            open(&held,leaf,true)
        }
        pub(super) fn unlink(parent:&fs::File,leaf:&std::ffi::OsStr,directory:bool)->Result<(),String> {
            let leaf=c(leaf)?;
            let result=unsafe {libc::unlinkat(parent.as_raw_fd(),leaf.as_ptr(),if directory {libc::AT_REMOVEDIR} else {0})};
            if result==0 {return Ok(());}
            let e=std::io::Error::last_os_error();
            if e.kind()==std::io::ErrorKind::NotFound || (directory && matches!(e.raw_os_error(),Some(libc::ENOTEMPTY|libc::EEXIST))) {return Ok(());}
            Err(error(&format!("cleanup failed: {e}")))
        }
    }
    #[cfg(not(unix))]
    fn inspect(path:&Path,directory:bool)->Result<bool,String> {
        let metadata=match fs::symlink_metadata(path) {Ok(m)=>m,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(false),Err(e)=>return Err(error(&e.to_string()))};
        #[cfg(windows)]
        let reparse={use std::os::windows::fs::MetadataExt;metadata.file_attributes()&0x400!=0};
        #[cfg(not(windows))]
        let reparse=false;
        if metadata.file_type().is_symlink() || reparse || (directory && !metadata.is_dir()) || (!directory && !metadata.is_file()) {return Err(error("symlink or unexpected file type"));}
        Ok(true)
    }
    pub(super) struct Plan {
        root_path:PathBuf,
        pub paths:HashSet<String>,
        #[cfg(unix)]
        root:Option<fs::File>,
    }
    impl Plan {
        pub fn prepare(root_path:&Path)->Result<Self,String> {
            #[cfg(unix)]
            let root=rooted::root(root_path)?;
            let plan=Self {root_path:root_path.into(),paths:HashSet::new(),#[cfg(unix)]root};
            let contents=plan.read_manifest()?;
            let paths=match contents {
                None=>HashSet::new(),
                Some(bytes)=>serde_json::from_slice::<Vec<String>>(&bytes).map_err(|_|error("malformed JSON or non-string path"))?.into_iter().collect(),
            };
            let plan=Self {paths,..plan};
            // ALL entries checked before the first remove/write/mkdir, regardless
            // of HashSet iteration order. Missing valid leaves remain compatible.
            for relative in &plan.paths {plan.check(relative)?;}
            plan.verify_root()?;
            Ok(plan)
        }
        pub fn verify_root(&self)->Result<(),String> {
            #[cfg(unix)]
            {
                match (&self.root,rooted::root(&self.root_path)?) {
                    (Some(held),Some(current)) if rooted::id(held)?==rooted::id(&current)?=>{},
                    (None,None)=>{},
                    _=>return Err(error("vault root changed")),
                }
            }
            #[cfg(not(unix))]
            {
                let mut current=PathBuf::new();
                for component in self.root_path.components() {
                    if matches!(component,std::path::Component::ParentDir|std::path::Component::CurDir) {return Err(error("unsafe vault root component"));}
                    current.push(component.as_os_str());if !inspect(&current,true)? {break;}
                }
            }
            Ok(())
        }
        #[cfg(unix)]
        fn parent(&self,parts:&[&str])->Result<Option<fs::File>,String> {
            self.verify_root()?;
            let Some(root)=&self.root else {return Ok(None);};
            let mut held=root.try_clone().map_err(|e|e.to_string())?;
            for part in parts {match rooted::open(&held,std::ffi::OsStr::new(part),true)? {Some(next)=>held=next,None=>return Ok(None)}}
            Ok(Some(held))
        }
        #[cfg(not(unix))]
        fn parent_path(&self,parts:&[&str])->Result<Option<PathBuf>,String> {
            self.verify_root()?;let mut path=self.root_path.clone();
            if !inspect(&path,true)? {return Ok(None);}
            for part in parts {path.push(part);if !inspect(&path,true)? {return Ok(None);}}
            Ok(Some(path))
        }
        fn read_manifest(&self)->Result<Option<Vec<u8>>,String> {
            self.verify_root()?;
            #[cfg(unix)]
            {
                let Some(root)=&self.root else {return Ok(None);};
                let Some(mut file)=rooted::open(root,std::ffi::OsStr::new(MIRROR_MANIFEST_FILE),false)? else {return Ok(None);};
                rooted::regular(&file)?;
                let mut bytes=Vec::new();file.read_to_end(&mut bytes).map_err(|e|e.to_string())?;self.verify_root()?;Ok(Some(bytes))
            }
            #[cfg(not(unix))]
            {
                let path=self.root_path.join(MIRROR_MANIFEST_FILE);
                if !inspect(&path,false)? {return Ok(None);}
                fs::read(path).map(Some).map_err(|e|error(&e.to_string()))
            }
        }
        pub(super) fn check(&self,relative:&str)->Result<(),String> {
            let parts=segments(relative)?;
            #[cfg(unix)]
            if let Some(parent)=self.parent(&parts[..parts.len()-1])? {
                if let Some(file)=rooted::open(&parent,std::ffi::OsStr::new(parts[parts.len()-1]),false)? {
                    rooted::regular(&file)?;
                }
            }
            #[cfg(not(unix))]
            if let Some(parent)=self.parent_path(&parts[..parts.len()-1])? {inspect(&parent.join(parts[parts.len()-1]),false)?;}
            Ok(())
        }
        pub(super) fn check_directory(&self,path:&Path)->Result<(),String> {
            let relative=path.strip_prefix(&self.root_path).map_err(|_|error("directory outside selected vault"))?;
            let parts=relative.iter().map(|part|part.to_str().ok_or_else(||error("non-UTF8 generated directory"))).collect::<Result<Vec<_>,_>>()?;
            if parts.is_empty() || parts[0]!=MIRROR_SPACES_DIR || parts.iter().any(|p|p.is_empty() || *p=="." || *p==".." || !p.bytes().all(|b|b.is_ascii_alphanumeric() || matches!(b,b' '|b'.'|b'-'|b'_'))) {
                return Err(error("unsafe generated directory"));
            }
            #[cfg(unix)]
            self.parent(&parts)?;
            #[cfg(not(unix))]
            self.parent_path(&parts)?;
            Ok(())
        }
        pub fn remove(&self,relative:&str)->Result<(),String> {
            if !self.paths.contains(relative) {return Err(error("cleanup path not in preflight manifest"));}
            self.check(relative)?;let parts=segments(relative)?;
            #[cfg(unix)]
            if let Some(parent)=self.parent(&parts[..parts.len()-1])? {
                if let Some(file)=rooted::open(&parent,std::ffi::OsStr::new(parts[parts.len()-1]),false)? {
                    rooted::regular(&file)?;
                    self.verify_root()?;
                    rooted::unlink(&parent,std::ffi::OsStr::new(parts[parts.len()-1]),false)?;
                }
            }
            #[cfg(not(unix))]
            if let Some(parent)=self.parent_path(&parts[..parts.len()-1])? {
                let path=parent.join(parts[parts.len()-1]);if inspect(&path,false)? {fs::remove_file(path).map_err(|e|error(&e.to_string()))?;}
            }
            Ok(())
        }
        pub fn remove_manifest(&self)->Result<(),String> {
            self.verify_root()?;
            #[cfg(unix)]
            if let Some(root)=&self.root {
                if let Some(file)=rooted::open(root,std::ffi::OsStr::new(MIRROR_MANIFEST_FILE),false)? {
                    rooted::regular(&file)?;
                    self.verify_root()?;rooted::unlink(root,std::ffi::OsStr::new(MIRROR_MANIFEST_FILE),false)?;
                }
            }
            #[cfg(not(unix))]
            {
                let path=self.root_path.join(MIRROR_MANIFEST_FILE);
                if inspect(&path,false)? {fs::remove_file(path).map_err(|e|error(&e.to_string()))?;}
            }
            Ok(())
        }
        pub fn cleanup_parents(&self)->Result<(),String> {
            let mut parents=self.paths.iter().map(|p| {
                let mut parts=segments(p)?;parts.pop();Ok(parts.join("/"))
            }).collect::<Result<Vec<_>,String>>()?;
            parents.sort_by_key(|p|std::cmp::Reverse(p.split('/').count()));parents.dedup();
            for relative in parents {
                let parts=relative.split('/').collect::<Vec<_>>();
                // Exactly one immediate generated-MD parent per manifest entry;
                // never its space/root/internal/asset/attachment siblings.
                #[cfg(unix)]
                if let Some(parent)=self.parent(&parts[..parts.len()-1])? {
                    if rooted::open(&parent,std::ffi::OsStr::new(parts[parts.len()-1]),true)?.is_some() {
                        self.verify_root()?;rooted::unlink(&parent,std::ffi::OsStr::new(parts[parts.len()-1]),true)?;
                    }
                }
                #[cfg(not(unix))]
                if let Some(parent)=self.parent_path(&parts[..parts.len()-1])? {
                    let target=parent.join(parts[parts.len()-1]);if inspect(&target,true)? {
                        match fs::remove_dir(target) {Ok(())=>{},Err(e) if matches!(e.kind(),std::io::ErrorKind::NotFound|std::io::ErrorKind::DirectoryNotEmpty)=>{},Err(e)=>return Err(error(&e.to_string()))}
                    }
                }
            }
            Ok(())
        }
    }
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::atomic::{AtomicU64,Ordering};
    static SEQUENCE:AtomicU64=AtomicU64::new(1);
    fn root()->PathBuf {
        let root=std::env::temp_dir().join(format!("starlane-mirror-cleanup-{}-{}",std::process::id(),SEQUENCE.fetch_add(1,Ordering::Relaxed)));
        fs::create_dir(&root).unwrap();root
    }
    fn put(root:&Path,path:&str,bytes:&[u8]) {
        let path=root.join(path);fs::create_dir_all(path.parent().unwrap()).unwrap();fs::write(path,bytes).unwrap();
    }
    fn manifest(root:&Path,paths:&[&str]) {
        fs::write(root.join(MIRROR_MANIFEST_FILE),serde_json::to_vec(paths).unwrap()).unwrap();
    }
    fn tree(root:&Path)->BTreeMap<String,Vec<u8>> {
        fn walk(root:&Path,path:&Path,out:&mut BTreeMap<String,Vec<u8>>) {
            for entry in fs::read_dir(path).unwrap() {
                let entry=entry.unwrap();let path=entry.path();let ty=entry.file_type().unwrap();
                let key=path.strip_prefix(root).unwrap().to_string_lossy().to_string();
                if ty.is_symlink() {out.insert(key,fs::read_link(path).unwrap().to_string_lossy().as_bytes().to_vec());}
                else if ty.is_dir() {out.insert(format!("{key}/"),Vec::new());walk(root,&path,out);}
                else if ty.is_file() {out.insert(key,fs::read(path).unwrap());}
                else {out.insert(key,b"special-file".to_vec());}
            }
        }
        let mut out=BTreeMap::new();walk(root,root,&mut out);out
    }
    #[test]
    fn whole_manifest_protected_or_non_generated_entry_preserves_all_files_before_reset_and_write() {
        for invalid in [".starlane/starlane-notes.sqlite3",".starlane/import_receipts/history-id.md",".starlane/versions/original-id.md","assets/asset-id.bin",
            "spaces/Alpha/attachments/page/file-id.md","spaces/personalspaces/documents/attachments/file-id.md",
            "spaces/Alpha/documents/../attachments/file-id.md","spaces/Alpha/documents\\folder/file-id.md","/spaces/Alpha/documents/file-id.md",
            "spaces/Alpha/documents/C:drive/file-id.md","spaces/Alpha/drafts/folder/file-id.md","spaces/Alpha/documents/plain.txt"] {
            let root=root();put(&root,"spaces/Alpha/documents/safe-id.md",b"editable safe mirror");
            put(&root,".starlane/starlane-notes.sqlite3",b"canonical DB");put(&root,"assets/asset-id.bin",b"asset");
            put(&root,"spaces/personalspaces/documents/attachments/file-id.md",b"canonical personal attachment");
            manifest(&root,&["spaces/Alpha/documents/safe-id.md",invalid]);let before=tree(&root);
            assert!(reset_filesystem_mirror_at(&root).is_err(),"{invalid}");assert_eq!(tree(&root),before);
            let snapshot=json!({"spaces":[{"id":"s","name":"New space"}],"documents":[{"id":"new","space_id":"s","title":"New","content":"new bytes"}]});
            assert!(write_filesystem_mirror_at(&root,&snapshot).is_err(),"{invalid}");assert_eq!(tree(&root),before);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn malformed_nonstring_and_nonarray_manifest_retained_before_any_mirror_mutation() {
        for bytes in [b"{".as_slice(),b"[null]".as_slice(),b"[12]".as_slice(),b"{}".as_slice()] {
            let root=root();put(&root,"spaces/Alpha/documents/safe-id.md",b"old");put(&root,MIRROR_MANIFEST_FILE,bytes);let before=tree(&root);
            assert!(reset_filesystem_mirror_at(&root).is_err());assert_eq!(tree(&root),before);
            assert!(write_filesystem_mirror_at(&root,&json!({})).is_err());assert_eq!(tree(&root),before);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn valid_ordinary_personal_nested_and_literal_personalspaces_cleanup_preserves_other_assets() {
        let root=root();let paths=["spaces/Alpha/documents/One/Two/A..B-id.md","spaces/Alpha/drafts/Draft-id.md","spaces/Alpha/templates/Template-id.md",
            "spaces/Alpha/archived/Archive-id.md","spaces/Alpha/trashed/Trash-id.md","spaces/personalspaces/documents/documents/Folder/Page-id.md",
            "spaces/personalspaces/attachments/documents/Page-id.md","spaces/personalspaces/documents/Normal-id.md","spaces/Alpha/documents/missing-id.md"];
        for path in &paths[..paths.len()-1] {put(&root,path,b"old mirror");}
        put(&root,"spaces/Alpha/attachments/doc/asset-id.bin",b"ordinary attachment");
        put(&root,"spaces/personalspaces/documents/attachments/file-id.md",b"personal attachment");
        put(&root,".starlane/starlane-notes.sqlite3",b"DB");put(&root,"assets/native-id.bin",b"asset");
        manifest(&root,&paths);reset_filesystem_mirror_at(&root).unwrap();
        for path in paths {assert!(!root.join(path).exists());}
        assert_eq!(fs::read(root.join("spaces/personalspaces/documents/attachments/file-id.md")).unwrap(),b"personal attachment");
        assert_eq!(fs::read(root.join("spaces/Alpha/attachments/doc/asset-id.bin")).unwrap(),b"ordinary attachment");
        assert_eq!(fs::read(root.join(".starlane/starlane-notes.sqlite3")).unwrap(),b"DB");
        assert!(!root.join(MIRROR_MANIFEST_FILE).exists());fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn valid_generation_then_stale_cleanup_uses_actual_layout_and_manifest() {
        let root=root();let snapshot=json!({"spaces":[{"id":"normal","name":"Alpha"},{"id":"local-space","owner_username":"documents"}],
            "folders":[{"id":"folder","space_id":"normal","name":"A..B"}],
            "documents":[{"id":"old","space_id":"normal","title":"Old","content":"body","folder_id":"folder"},{"id":"personal","space_id":"local-space","title":"Personal","content":"body"}],
            "drafts":[{"id":"draft","space_id":"normal","title":"Draft","content":"body"}],"templates":[{"id":"template","space_id":"normal","name":"Template","content":"body"}]});
        write_filesystem_mirror_at(&root,&snapshot).unwrap();let generated=read_mirror_manifest(&root).unwrap();assert_eq!(generated.len(),4);
        put(&root,"assets/native-id.bin",b"keep");write_filesystem_mirror_at(&root,&json!({"spaces":[],"documents":[]})).unwrap();
        for path in generated {assert!(!root.join(path).exists());}assert_eq!(read_mirror_manifest(&root).unwrap().len(),0);
        assert_eq!(fs::read(root.join("assets/native-id.bin")).unwrap(),b"keep");fs::remove_dir_all(root).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn manifest_parent_leaf_and_dangling_symlinks_fail_without_outside_or_partial_mutation() {
        use std::os::unix::fs::symlink;
        for mode in ["manifest","parent","leaf","dangling","inside-parent"] {
            let root=root();let outside=root.with_extension("outside");fs::create_dir(&outside).unwrap();put(&outside,"external-id.md",b"outside");
            put(&root,"spaces/Alpha/documents/safe-id.md",b"safe");let bad="spaces/Alpha/documents/Folder/external-id.md";
            manifest(&root,&["spaces/Alpha/documents/safe-id.md",bad]);
            match mode {
                "manifest"=>{fs::rename(root.join(MIRROR_MANIFEST_FILE),outside.join("manifest.json")).unwrap();symlink(outside.join("manifest.json"),root.join(MIRROR_MANIFEST_FILE)).unwrap();}
                "parent"=>symlink(&outside,root.join("spaces/Alpha/documents/Folder")).unwrap(),
                "inside-parent"=>{put(&root,"assets/external-id.md",b"inside protected");symlink(root.join("assets"),root.join("spaces/Alpha/documents/Folder")).unwrap();}
                _=>{fs::create_dir(root.join("spaces/Alpha/documents/Folder")).unwrap();let target=if mode=="dangling" {outside.join("absent")} else {outside.join("external-id.md")};symlink(target,root.join(bad)).unwrap();}
            }
            let before=tree(&root);let external=tree(&outside);
            assert!(reset_filesystem_mirror_at(&root).is_err(),"{mode}");assert_eq!(tree(&root),before);assert_eq!(tree(&outside),external);
            assert!(write_filesystem_mirror_at(&root,&json!({})).is_err(),"{mode}");assert_eq!(tree(&root),before);assert_eq!(tree(&outside),external);
            fs::remove_dir_all(root).unwrap();fs::remove_dir_all(outside).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn hardlinked_manifest_or_prior_generated_leaf_cannot_overwrite_canonical_asset() {
        for mode in ["manifest","leaf"] {
            let root=root();let path="spaces/Alpha/documents/Safe-safe.md";
            put(&root,path,b"old mirror");manifest(&root,&[path]);
            if mode=="manifest" {
                fs::create_dir(root.join("assets")).unwrap();
                fs::hard_link(root.join(MIRROR_MANIFEST_FILE),root.join("assets/canonical-json-id.bin")).unwrap();
            } else {
                fs::create_dir(root.join("assets")).unwrap();
                fs::hard_link(root.join(path),root.join("assets/canonical-id.bin")).unwrap();
            }
            let before=tree(&root);
            let snapshot=json!({"spaces":[{"id":"s","name":"Alpha"}],"documents":[{"id":"safe","space_id":"s","title":"Safe","content":"would overwrite alias"}]});
            assert!(write_filesystem_mirror_at(&root,&snapshot).is_err(),"{mode}");assert_eq!(tree(&root),before);
            assert!(reset_filesystem_mirror_at(&root).is_err(),"{mode}");assert_eq!(tree(&root),before);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[cfg(unix)]
    #[test]
    fn ancestor_alias_supported_but_final_root_symlink_and_late_alias_retarget_rejected() {
        use std::os::unix::fs::symlink;
        let container=root();let first=container.join("first");let second=container.join("second");
        fs::create_dir_all(first.join("vault")).unwrap();fs::create_dir_all(second.join("vault")).unwrap();
        let alias=container.join("alias");symlink(&first,&alias).unwrap();let selected=alias.join("vault");
        let relative="spaces/Alpha/documents/Old-id.md";
        put(&selected,relative,b"original");manifest(&selected,&[relative]);
        let plan=mirror_cleanup::Plan::prepare(&selected).unwrap();
        plan.verify_root().unwrap();plan.remove(relative).unwrap();
        assert!(!first.join("vault").join(relative).exists());
        put(&selected,relative,b"original");
        put(&second.join("vault"),relative,b"replacement");
        let final_link=container.join("root-link");symlink(first.join("vault"),&final_link).unwrap();
        assert!(mirror_cleanup::Plan::prepare(&final_link).is_err());
        fs::remove_file(&alias).unwrap();symlink(&second,&alias).unwrap();
        assert!(plan.verify_root().is_err());assert!(plan.remove(relative).is_err());
        assert!(plan.cleanup_parents().is_err());assert!(plan.remove_manifest().is_err());
        assert_eq!(fs::read(first.join("vault").join(relative)).unwrap(),b"original");
        assert_eq!(fs::read(second.join("vault").join(relative)).unwrap(),b"replacement");
        fs::remove_dir_all(container).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn special_manifest_or_leaf_rejected_without_blocking_and_held_root_replacement_rejected() {
        use std::os::unix::ffi::OsStrExt;
        for leaf in [MIRROR_MANIFEST_FILE,"spaces/Alpha/documents/Fifo-id.md"] {
            let root=root();let target=root.join(leaf);fs::create_dir_all(target.parent().unwrap()).unwrap();
            let path=std::ffi::CString::new(target.as_os_str().as_bytes()).unwrap();assert_eq!(unsafe {libc::mkfifo(path.as_ptr(),0o600)},0);
            if leaf!=MIRROR_MANIFEST_FILE {manifest(&root,&[leaf]);}
            assert!(mirror_cleanup::Plan::prepare(&root).is_err());assert!(fs::symlink_metadata(target).is_ok());fs::remove_dir_all(root).unwrap();
        }
        let root=root();let path="spaces/Alpha/documents/Old-id.md";put(&root,path,b"original");manifest(&root,&[path]);
        let plan=mirror_cleanup::Plan::prepare(&root).unwrap();let original=root.with_extension("original");fs::rename(&root,&original).unwrap();
        fs::create_dir(&root).unwrap();put(&root,path,b"replacement");assert!(plan.remove(path).is_err());assert!(plan.cleanup_parents().is_err());
        assert_eq!(fs::read(original.join(path)).unwrap(),b"original");assert_eq!(fs::read(root.join(path)).unwrap(),b"replacement");
        fs::remove_dir_all(root).unwrap();fs::remove_dir_all(original).unwrap();
    }
}

}

fn read_mirror_manifest(vault_dir: &Path) -> Result<HashSet<String>, String> {
    Ok(mirror_cleanup::Plan::prepare(vault_dir)?.paths)
}

pub(crate) fn preflight_vault_snapshot(app:&AppHandle,snapshot:&Value)->Result<(),String> {
    let (root,_) = resolve_vault_dir(app)?;
    mirror_admission::Plan::prepare(&root,snapshot).map(|_|())
}

pub(crate) fn preflight_vault_mirror(app: &AppHandle) -> Result<(), String> {
    let (vault_dir, _) = resolve_vault_dir(app)?;
    mirror_cleanup::Plan::prepare(&vault_dir).map(|_| ())
}

fn write_mirror_manifest(vault_dir: &Path, paths: &HashSet<String>) -> Result<(), String> {
    let mut sorted = paths.iter().cloned().collect::<Vec<_>>();
    sorted.sort();
    let contents = serde_json::to_string_pretty(&sorted).map_err(|e| e.to_string())?;
    fs::write(vault_dir.join(MIRROR_MANIFEST_FILE), contents).map_err(|e| e.to_string())
}

fn space_dir_name(space: &Value) -> String {
    let id = value_str(space, "id").unwrap_or("space");
    let name = value_str(space, "name")
        .or_else(|| value_str(space, "slug"))
        .unwrap_or(id);
    sanitize_space_dir_name(name, id)
}

fn personal_space_owner_name(space: &Value) -> String {
    value_str(space, "owner_username")
        .or_else(|| value_str(space, "username"))
        .or_else(|| value_str(space, "created_by"))
        .or_else(|| value_str(space, "owner_display_name"))
        .unwrap_or("local-user")
        .to_string()
}

fn is_personal_space(space: &Value) -> bool {
    value_str(space, "id") == Some("local-space")
        || value_str(space, "slug") == Some("personal")
        || value_str(space, "name") == Some("Personal Space")
}

fn space_dir_segments(space: &Value) -> Vec<String> {
    if is_personal_space(space) {
        return vec![
            MIRROR_PERSONAL_SPACES_DIR.to_string(),
            sanitize_space_dir_name(&personal_space_owner_name(space), "local-user"),
        ];
    }

    vec![space_dir_name(space)]
}

fn space_dir_path(vault_dir: &Path, space: &Value) -> PathBuf {
    let mut path = vault_dir.join(MIRROR_SPACES_DIR);
    for segment in space_dir_segments(space) {
        path = path.join(segment);
    }
    path
}

fn markdown_file_name(item: &Value, fallback: &str) -> String {
    let id = value_str(item, "id").unwrap_or(fallback);
    let title = value_str(item, "title")
        .or_else(|| value_str(item, "name"))
        .unwrap_or(id);
    let base = sanitize_path_segment(title, id);
    let suffix = sanitize_file_name(id);
    format!("{base}-{suffix}.md")
}

fn write_markdown_mirror_file(
    path: &Path,
    item: &Value,
    title_key: &str,
    body_key: &str,
) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let mut output = String::new();
    output.push_str("---\n");
    for key in [
        "id",
        title_key,
        "space_id",
        "folder_id",
        "parent_id",
        "created_by",
        "last_updated_by",
        "created_at",
        "updated_at",
        "owner_id",
        "previous_folder_id",
        "previous_parent_id",
        "starlane_lifecycle",
    ] {
        if let Some(value) = value_str(item, key) {
            let _ = writeln!(output, "{key}: {}", yaml_string(value));
        } else if key == "parent_id" {
            // Explicit empty parent in newly generated mirrors can clear a
            // relationship; absent legacy metadata must retain its existing edge.
            let _ = writeln!(output, "parent_id: {}", yaml_string(""));
        }
    }
    output.push_str("---\n\n");
    output.push_str(item.get(body_key).and_then(Value::as_str).unwrap_or(""));
    output.push('\n');

    fs::write(path, output).map_err(|e| e.to_string())
}

fn ensure_space_dirs(space_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(space_dir.join(MIRROR_DOCUMENTS_DIR)).map_err(|e| e.to_string())?;
    fs::create_dir_all(space_dir.join(MIRROR_ATTACHMENTS_DIR)).map_err(|e| e.to_string())?;
    fs::create_dir_all(space_dir.join(MIRROR_TEMPLATES_DIR)).map_err(|e| e.to_string())?;
    fs::create_dir_all(space_dir.join(MIRROR_DRAFTS_DIR)).map_err(|e| e.to_string())?;
    fs::create_dir_all(space_dir.join(MIRROR_ARCHIVED_DIR)).map_err(|e| e.to_string())?;
    fs::create_dir_all(space_dir.join(MIRROR_TRASHED_DIR)).map_err(|e| e.to_string())
}

fn reset_filesystem_mirror(app: &AppHandle) -> Result<(), String> {
    let (vault_dir, _) = resolve_vault_dir(app)?;
    reset_filesystem_mirror_at(&vault_dir)
}

fn reset_filesystem_mirror_at(vault_dir: &Path) -> Result<(), String> {
    let cleanup = mirror_cleanup::Plan::prepare(vault_dir)?;
    for relative in &cleanup.paths { cleanup.remove(relative)?; }
    cleanup.cleanup_parents()?;
    cleanup.remove_manifest()
}

fn write_filesystem_mirror(app: &AppHandle, snapshot: &Value) -> Result<(), String> {
    let (vault_dir, _) = resolve_vault_dir(app)?;
    write_filesystem_mirror_at(&vault_dir, snapshot)
}

fn write_filesystem_mirror_at(vault_dir: &Path, snapshot: &Value) -> Result<(), String> {
    let plan = mirror_admission::Plan::prepare(vault_dir, snapshot)?;
    ensure_vault_dir(vault_dir)?;
    for space in &plan.space_dirs { ensure_space_dirs(space)?; }
    for entry in &plan.entries {
        write_markdown_mirror_file(&entry.path, entry.item, entry.title_key, "content")?;
    }
    for stale_path in plan.cleanup.paths.difference(&plan.generated_paths) {
        plan.cleanup.remove(stale_path)?;
    }
    plan.cleanup.cleanup_parents()?;
    write_mirror_manifest(vault_dir, &plan.generated_paths)
}

fn set_object_string(item: &mut Value, key: &str, value: String) {
    if let Some(object) = item.as_object_mut() {
        object.insert(key.to_string(), Value::String(value));
    }
}

fn reconcile_snapshot_collection(
    snapshot: &mut Value,
    collection_key: &str,
    id: &str,
    title_key: &str,
    body_key: &str,
    metadata: &HashMap<String, String>,
    body: &str,
) -> bool {
    let Some(items) = snapshot
        .get_mut(collection_key)
        .and_then(Value::as_array_mut)
    else {
        return false;
    };

    let Some(item) = items
        .iter_mut()
        .find(|item| value_str(item, "id") == Some(id))
    else {
        return false;
    };

    let mut changed = false;
    if let Some(title) = metadata.get(title_key) {
        if value_str(item, title_key) != Some(title.as_str()) {
            set_object_string(item, title_key, title.clone());
            changed = true;
        }
    }
    let original_body = item.get(body_key).and_then(Value::as_str);
    // Before parent_id was emitted for every generated mirror, the writer
    // trimmed body whitespace. An unchanged old mirror cannot distinguish that
    // lossy projection from an intentional trim: retain the canonical original.
    // New-format mirrors compare exact bodies, and genuinely different legacy
    // body edits still apply. The manifest has paths only, not source hashes.
    let ambiguous_legacy_trim = !metadata.contains_key("parent_id")
        && original_body.is_some_and(|original| original.trim() == body);
    if original_body != Some(body) && !ambiguous_legacy_trim {
        set_object_string(item, body_key, body.to_string());
        changed = true;
    }
    if collection_key == "documents" {
        if let Some(parent) = metadata.get("parent_id") {
            let next = if parent.is_empty() { None } else { Some(parent.as_str()) };
            if value_str(item,"parent_id") != next {
                if let Some(object)=item.as_object_mut() {
                    object.insert("parent_id".into(),next.map(|id|Value::String(id.into())).unwrap_or(Value::Null));
                    changed = true;
                }
            }
        }
    }
    for key in ["folder_id", "updated_at"] {
        if let Some(value) = metadata.get(key) {
            if value_str(item, key) != Some(value.as_str()) {
                set_object_string(item, key, value.clone());
                changed = true;
            }
        }
    }

    changed
}

fn snapshot_array_mut<'a>(
    snapshot: &'a mut Value,
    key: &str,
) -> Result<&'a mut Vec<Value>, String> {
    snapshot
        .get_mut(key)
        .and_then(Value::as_array_mut)
        .ok_or_else(|| format!("Vault snapshot is missing {key} collection"))
}

fn mirror_import_id(prefix: &str, seed: u128, index: usize) -> String {
    format!("{prefix}-{seed}-{index}")
}

fn mirror_import_timestamp(seed: u128) -> String {
    let seconds = seed / 1_000_000_000;
    format!("imported-{seconds}")
}

fn markdown_title_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(|name| sanitize_path_segment(name, "Imported Note"))
        .unwrap_or_else(|| "Imported Note".to_string())
}

fn snapshot_has_id(snapshot: &Value, collection_key: &str, id: &str) -> bool {
    value_array(snapshot, collection_key)
        .into_iter()
        .any(|item| value_str(item, "id") == Some(id))
}

fn find_mirror_space(snapshot: &Value, vault_dir: &Path, path: &Path) -> Option<(String, PathBuf)> {
    value_array(snapshot, "spaces")
        .into_iter()
        .filter_map(|space| {
            let id = value_str(space, "id")?.to_string();
            let dir = space_dir_path(vault_dir, space);
            if path.starts_with(&dir) {
                Some((id, dir))
            } else {
                None
            }
        })
        .max_by_key(|(_, dir)| dir.components().count())
}

fn find_or_create_import_folder(
    snapshot: &mut Value,
    space_id: &str,
    parent_id: Option<String>,
    name: &str,
    seed: u128,
    index: &mut usize,
) -> Result<String, String> {
    if let Some(existing_id) = value_array(snapshot, "folders")
        .into_iter()
        .find(|folder| {
            value_str(folder, "space_id") == Some(space_id)
                && value_str(folder, "parent_id") == parent_id.as_deref()
                && sanitize_path_segment(value_str(folder, "name").unwrap_or("Folder"), "Folder")
                    == name
        })
        .and_then(|folder| value_str(folder, "id").map(str::to_string))
    {
        return Ok(existing_id);
    }

    *index += 1;
    let id = mirror_import_id("folder", seed, *index);
    let timestamp = mirror_import_timestamp(seed);
    let mut folder = serde_json::Map::new();
    folder.insert("id".to_string(), Value::String(id.clone()));
    folder.insert("space_id".to_string(), Value::String(space_id.to_string()));
    folder.insert("name".to_string(), Value::String(name.to_string()));
    if let Some(parent_id) = parent_id {
        folder.insert("parent_id".to_string(), Value::String(parent_id));
    }
    folder.insert("created_at".to_string(), Value::String(timestamp.clone()));
    folder.insert("updated_at".to_string(), Value::String(timestamp));
    snapshot_array_mut(snapshot, "folders")?.push(Value::Object(folder));
    Ok(id)
}

fn folder_id_for_import_path(
    snapshot: &mut Value,
    space_id: &str,
    relative_document_path: &Path,
    seed: u128,
    index: &mut usize,
) -> Result<Option<String>, String> {
    let Some(parent) = relative_document_path.parent() else {
        return Ok(None);
    };
    let mut parent_id = None;
    for component in parent.components() {
        let name = component.as_os_str().to_string_lossy();
        if name.is_empty() {
            continue;
        }
        parent_id = Some(find_or_create_import_folder(
            snapshot, space_id, parent_id, &name, seed, index,
        )?);
    }
    Ok(parent_id)
}

fn import_untracked_markdown_files(
    vault_dir: &Path,
    snapshot: &mut Value,
    manifest: &HashSet<String>,
) -> Result<(usize, usize, usize), String> {
    let mut markdown_files = Vec::new();
    mirror_admission::collect_markdown_files(&vault_dir.join(MIRROR_SPACES_DIR), &mut markdown_files)?;
    markdown_files.sort();

    let seed = now_nanos()?;
    let mut import_index = 0;
    let mut files_scanned = 0;
    let mut documents_imported = 0;
    let mut templates_imported = 0;

    for path in markdown_files {
        let Some(relative) = relative_path(vault_dir, &path) else {
            continue;
        };
        if manifest.contains(&relative) {
            continue;
        }
        let Some((path_space_id, space_dir)) = find_mirror_space(snapshot, vault_dir, &path) else {
            continue;
        };
        let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let (metadata, body) = parse_markdown_import_file(&contents);
        files_scanned += 1;
        let space_id = metadata
            .get("space_id")
            .filter(|id| !id.trim().is_empty())
            .cloned()
            .unwrap_or(path_space_id);

        if let Ok(document_relative) = path.strip_prefix(space_dir.join(MIRROR_DOCUMENTS_DIR)) {
            import_index += 1;
            let mut id = metadata
                .get("id")
                .filter(|id| !id.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| mirror_import_id("doc", seed, import_index));
            if snapshot_has_id(snapshot, "documents", &id) {
                import_index += 1;
                id = mirror_import_id("doc", seed, import_index);
            }
            let title = metadata
                .get("title")
                .filter(|title| !title.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| markdown_title_from_path(&path));
            let folder_id = folder_id_for_import_path(
                snapshot,
                &space_id,
                document_relative,
                seed,
                &mut import_index,
            )?;
            let timestamp = mirror_import_timestamp(seed);
            let mut document = serde_json::Map::new();
            document.insert("id".to_string(), Value::String(id));
            document.insert("space_id".to_string(), Value::String(space_id));
            document.insert("title".to_string(), Value::String(title));
            document.insert("content".to_string(), Value::String(body));
            if let Some(parent) = metadata.get("parent_id").filter(|id|!id.is_empty()) {
                document.insert("parent_id".to_string(),Value::String(parent.clone()));
            }
            if let Some(folder_id) = folder_id {
                document.insert("folder_id".to_string(), Value::String(folder_id));
            }
            document.insert(
                "created_at".to_string(),
                Value::String(
                    metadata
                        .get("created_at")
                        .cloned()
                        .unwrap_or_else(|| timestamp.clone()),
                ),
            );
            document.insert(
                "updated_at".to_string(),
                Value::String(metadata.get("updated_at").cloned().unwrap_or(timestamp)),
            );
            snapshot_array_mut(snapshot, "documents")?.push(Value::Object(document));
            documents_imported += 1;
        } else if path
            .strip_prefix(space_dir.join(MIRROR_TEMPLATES_DIR))
            .is_ok()
        {
            import_index += 1;
            let mut id = metadata
                .get("id")
                .filter(|id| !id.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| mirror_import_id("template", seed, import_index));
            if snapshot_has_id(snapshot, "templates", &id) {
                import_index += 1;
                id = mirror_import_id("template", seed, import_index);
            }
            let name = metadata
                .get("name")
                .or_else(|| metadata.get("title"))
                .filter(|name| !name.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| markdown_title_from_path(&path));
            let timestamp = mirror_import_timestamp(seed);
            let mut template = serde_json::Map::new();
            template.insert("id".to_string(), Value::String(id));
            template.insert("space_id".to_string(), Value::String(space_id));
            template.insert("name".to_string(), Value::String(name));
            template.insert("content".to_string(), Value::String(body));
            template.insert("tags".to_string(), Value::Array(Vec::new()));
            template.insert(
                "created_at".to_string(),
                Value::String(
                    metadata
                        .get("created_at")
                        .cloned()
                        .unwrap_or_else(|| timestamp.clone()),
                ),
            );
            template.insert(
                "updated_at".to_string(),
                Value::String(metadata.get("updated_at").cloned().unwrap_or(timestamp)),
            );
            snapshot_array_mut(snapshot, "templates")?.push(Value::Object(template));
            templates_imported += 1;
        }
    }

    Ok((files_scanned, documents_imported, templates_imported))
}

fn reconcile_filesystem_mirror_snapshot(
    vault_dir: &Path,
    mut snapshot: Value,
) -> Result<MirrorReconciliationResult, String> {
    let manifest = read_mirror_manifest(vault_dir)?;
    let mut files_scanned = 0;
    let mut documents_updated = 0;
    let mut templates_updated = 0;

    for relative in &manifest {
        if !relative.ends_with(".md") || relative.contains("..") {
            continue;
        }
        let relative_path = Path::new(relative);
        if relative_path.is_absolute() {
            continue;
        }
        let path = vault_dir.join(relative_path);
        if !path.is_file() {
            continue;
        }
        let contents = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let Some((metadata, body)) = parse_markdown_mirror_file(&contents) else {
            continue;
        };
        files_scanned += 1;
        let Some(id) = metadata.get("id") else {
            continue;
        };

        if relative.contains(&format!("/{MIRROR_DOCUMENTS_DIR}/"))
            && reconcile_snapshot_collection(
                &mut snapshot,
                "documents",
                id,
                "title",
                "content",
                &metadata,
                &body,
            )
        {
            documents_updated += 1;
        } else if relative.contains(&format!("/{MIRROR_TEMPLATES_DIR}/"))
            && reconcile_snapshot_collection(
                &mut snapshot,
                "templates",
                id,
                "name",
                "content",
                &metadata,
                &body,
            )
        {
            templates_updated += 1;
        }
    }

    let (imported_scanned, documents_imported, templates_imported) =
        import_untracked_markdown_files(vault_dir, &mut snapshot, &manifest)?;
    files_scanned += imported_scanned;
    // Validate after the complete candidate import, allowing child-before-parent
    // filenames without ever inferring a page parent from a physical directory.
    mirror_admission::validate_page_hierarchy(&snapshot)?;

    Ok(MirrorReconciliationResult {
        snapshot,
        files_scanned,
        documents_updated,
        templates_updated,
        documents_imported,
        templates_imported,
    })
}

// This desktop request freshness boundary binds a persisted selection, not a
// physical-root lease or protected native authority. Mobile migrations remain
// on their existing path until separately adopted.
#[cfg(not(any(target_os="ios",target_os="android")))]
#[derive(Clone)]
pub(crate) struct SnapshotRequestBinding {
    settings_path:PathBuf,
    raw_vault_path:Option<Value>,
    root:PathBuf,
    provider:String,
}
#[cfg(not(any(target_os="ios",target_os="android")))]
impl SnapshotRequestBinding {
    fn read_selection(settings_path:&Path)->Result<Option<Value>,String> {
        let bytes=match fs::read(settings_path) {
            Ok(bytes)=>bytes,
            Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(None),
            Err(error)=>return Err(format!("Cannot read current vault selection: {error}")),
        };
        let settings:Value=serde_json::from_slice(&bytes).map_err(|_|"Current vault settings are malformed".to_string())?;
        let object=settings.as_object().ok_or_else(||"Current vault settings must be an object".to_string())?;
        // Never retain or emit unrelated preferences or credential settings.
        Ok(object.get(VAULT_PATH_KEY).cloned())
    }
    pub(crate) fn capture_at(settings_path:PathBuf,default_root:PathBuf,provider:&str)->Result<Self,String> {
        let raw_vault_path=Self::read_selection(&settings_path)?;
        // Preserve legacy nonstring/null fallback. A present empty string is a
        // configured path, matching configured_vault_dir's actual semantics.
        let root=raw_vault_path.as_ref().and_then(Value::as_str).map(PathBuf::from).unwrap_or(default_root);
        // Fix original relative selection semantics to the original working
        // directory rather than resolving it again after the mutex wait.
        let root=if root.is_absolute() {root} else {std::env::current_dir().map_err(|e|e.to_string())?.join(root)};
        Ok(Self {settings_path,raw_vault_path,root,provider:provider.into()})
    }
    pub(crate) fn verify_selection(&self)->Result<(),String> {
        if Self::read_selection(&self.settings_path)?!=self.raw_vault_path {
            return Err("Vault selection changed during this snapshot request".into());
        }
        Ok(())
    }
    // Selection/path proof only: never mkdir, migrate or open the destination.
    pub(crate) fn legacy_snapshot(&self)->Result<Option<Value>,String> {
        self.verify_selection()?;
        let bytes=match fs::read(self.root.join(snapshot_file_name(&self.provider))) {
            Ok(bytes)=>bytes,
            Err(error) if error.kind()==std::io::ErrorKind::NotFound=>return Ok(None),
            Err(error)=>return Err(error.to_string()),
        };
        let snapshot=serde_json::from_slice(&bytes).map_err(|_|"Original vault snapshot is malformed".to_string())?;
        self.verify_selection()?;Ok(Some(snapshot))
    }
    pub(crate) fn archive_assets_path(&self)->Result<PathBuf,String> {
        self.verify_selection()?;
        Ok(self.root.join("assets"))
    }
    pub(crate) fn expected_database_path(&self)->Result<PathBuf,String> {
        self.verify_selection()?;
        Ok(self.root.join(VAULT_INTERNAL_DIR).join(native_database_file_name(&self.provider)))
    }
    pub(crate) fn database_path(&self)->Result<PathBuf,String> {
        self.verify_selection()?;
        resolve_native_database_path_at(&self.root,&self.provider)
    }
    pub(crate) fn preflight_snapshot(&self,snapshot:&Value)->Result<(),String> {
        self.verify_selection()?;
        mirror_admission::Plan::prepare(&self.root,snapshot)?;
        self.verify_selection()
    }
    pub(crate) fn write_mirror(&self,snapshot:&Value)->Result<(),String> {
        // This is postcommit: caller preserves committed/mirror_pending truth.
        // Even a switch after this check cannot retarget the fixed root below.
        self.verify_selection()?;
        write_vault_snapshot_at(&self.root,&self.provider,snapshot)
    }
}
#[cfg(not(any(target_os="ios",target_os="android")))]
pub(crate) fn capture_snapshot_request<R:tauri::Runtime>(app:&AppHandle<R>,provider:&str)->Result<SnapshotRequestBinding,String> {
    let settings_path=tauri_plugin_store::resolve_store_path(app,SETTINGS_FILE).map_err(|e|e.to_string())?;
    SnapshotRequestBinding::capture_at(settings_path,default_vault_dir(app)?,provider)
}

fn resolve_snapshot_path(app: &AppHandle, provider: &str) -> Result<PathBuf, String> {
    let (vault_dir, _) = resolve_vault_dir(app)?;
    ensure_vault_dir(&vault_dir)?;
    Ok(vault_dir.join(snapshot_file_name(provider)))
}

pub(crate) fn resolve_native_database_path(
    app: &AppHandle,
    provider: &str,
) -> Result<PathBuf, String> {
    let (vault_dir, _) = resolve_vault_dir(app)?;
    resolve_native_database_path_at(&vault_dir,provider)
}

fn resolve_native_database_path_at(vault_dir:&Path,provider:&str)->Result<PathBuf,String> {
    ensure_vault_dir(vault_dir)?;
    let internal_dir = vault_dir.join(VAULT_INTERNAL_DIR);
    ensure_vault_dir(&internal_dir)?;
    let database_path = internal_dir.join(native_database_file_name(provider));
    let legacy_path = vault_dir.join(legacy_native_database_file_name(provider));

    if !database_path.exists() && legacy_path.exists() {
        fs::rename(&legacy_path, &database_path).map_err(|e| e.to_string())?;
    }

    Ok(database_path)
}

fn resolve_assets_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let (vault_dir, _) = resolve_vault_dir(app)?;
    let assets_dir = vault_dir.join("assets");
    ensure_vault_dir(&assets_dir)?;
    Ok(assets_dir)
}

fn resolve_attachment_assets_dir(
    app: &AppHandle,
    provider: &str,
    space_id: Option<&str>,
    document_id: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    let Some(space_id) = space_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let (vault_dir, _) = resolve_vault_dir(app)?;
    let snapshot_path = vault_dir.join(snapshot_file_name(provider));
    let space_dir_name = fs::read_to_string(snapshot_path)
        .ok()
        .and_then(|contents| serde_json::from_str::<Value>(&contents).ok())
        .and_then(|snapshot| {
            value_array(&snapshot, "spaces")
                .into_iter()
                .find(|space| value_str(space, "id") == Some(space_id))
                .map(|space| space_dir_path(&vault_dir, space))
        })
        .unwrap_or_else(|| {
            vault_dir
                .join(MIRROR_SPACES_DIR)
                .join(sanitize_space_dir_name(space_id, "space"))
        });

    let space_dir = space_dir_name;
    ensure_space_dirs(&space_dir)?;

    let mut attachments_dir = space_dir.join(MIRROR_ATTACHMENTS_DIR);
    if let Some(document_id) = document_id.map(str::trim).filter(|value| !value.is_empty()) {
        attachments_dir = attachments_dir.join(sanitize_path_segment(document_id, "document"));
    }
    ensure_vault_dir(&attachments_dir)?;
    Ok(Some(attachments_dir))
}

#[tauri::command]
pub fn get_vault_location(app: AppHandle) -> Result<VaultLocation, String> {
    let (path, using_default, configured) = resolve_vault_location(&app)?;
    ensure_vault_dir(&path)?;
    Ok(VaultLocation {
        path: path.to_string_lossy().into_owned(),
        using_default,
        configured,
    })
}

#[tauri::command]
pub fn get_vault_location_status(app: AppHandle) -> Result<VaultLocation, String> {
    let (path, using_default, configured) = resolve_vault_location(&app)?;
    Ok(VaultLocation {
        path: path.to_string_lossy().into_owned(),
        using_default,
        configured,
    })
}

#[tauri::command]
pub fn list_vault_folders(app: AppHandle) -> Result<Vec<VaultFolderSummary>, String> {
    let (current_path, _, configured) = resolve_vault_location(&app)?;
    let parent = vault_parent_dir(&app)?;
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;

    let mut folders = Vec::new();
    for entry in fs::read_dir(&parent).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }

        folders.push(VaultFolderSummary {
            name: name.to_string(),
            path: path.to_string_lossy().into_owned(),
            current: configured && path == current_path,
        });
    }

    if configured
        && current_path.is_dir()
        && !folders
            .iter()
            .any(|folder| folder.path == current_path.to_string_lossy())
    {
        folders.push(VaultFolderSummary {
            name: current_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Starlane Notes Vault")
                .to_string(),
            path: current_path.to_string_lossy().into_owned(),
            current: true,
        });
    }

    folders.sort_by(|a, b| {
        b.current
            .cmp(&a.current)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(folders)
}

#[tauri::command]
pub fn create_vault_folder(app: AppHandle, name: String) -> Result<VaultLocation, String> {
    let folder_name = sanitize_path_segment(&name, "Starlane Notes Vault");
    let parent = vault_parent_dir(&app)?;
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let path = parent.join(folder_name);
    set_vault_path(app, path.to_string_lossy().into_owned())
}

fn renamed_vault_folder_path(current_path: &Path, name: &str) -> Result<PathBuf, String> {
    let folder_name = sanitize_path_segment(name, "Starlane Notes Vault");
    let parent = current_path
        .parent()
        .ok_or_else(|| "Active vault folder cannot be renamed safely".to_string())?;
    Ok(parent.join(folder_name))
}

#[tauri::command]
pub fn rename_vault_folder(app: AppHandle, name: String) -> Result<VaultLocation, String> {
    let (current_path, using_default) = resolve_vault_dir(&app)?;
    if using_default {
        return Err(
            "Default vault folder cannot be renamed. Choose or create a custom vault first."
                .to_string(),
        );
    }
    ensure_vault_dir(&current_path)?;

    let target_path = renamed_vault_folder_path(&current_path, &name)?;
    if target_path == current_path {
        return set_vault_path(app, current_path.to_string_lossy().into_owned());
    }
    if target_path.exists() {
        return Err("A vault folder with that name already exists.".to_string());
    }

    fs::rename(&current_path, &target_path).map_err(|e| e.to_string())?;
    set_vault_path(app, target_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn rename_vault_folder_at(app: AppHandle, path: String, name: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Vault path is required".to_string());
    }
    let source_path = PathBuf::from(trimmed);
    if !source_path.is_dir() {
        return Err("Vault folder was not found.".to_string());
    }
    let parent = vault_parent_dir(&app)?;
    if source_path.parent() != Some(parent.as_path()) {
        return Err("Only app-visible vault folders can be renamed here.".to_string());
    }

    let target_path = renamed_vault_folder_path(&source_path, &name)?;
    if target_path == source_path {
        return Ok(());
    }
    if target_path.exists() {
        return Err("A vault folder with that name already exists.".to_string());
    }

    let (current_path, _) = resolve_vault_dir(&app)?;
    fs::rename(&source_path, &target_path).map_err(|e| e.to_string())?;
    if current_path == source_path {
        set_vault_path(app, target_path.to_string_lossy().into_owned())?;
    }
    Ok(())
}

#[tauri::command]
pub fn delete_vault_folder_at(app: AppHandle, path: String) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Vault path is required".to_string());
    }
    let source_path = PathBuf::from(trimmed);
    if !source_path.is_dir() {
        return Err("Vault folder was not found.".to_string());
    }

    let parent = vault_parent_dir(&app)?;
    if source_path.parent() != Some(parent.as_path()) {
        return Err("Only app-visible vault folders can be deleted here.".to_string());
    }

    let (current_path, _, configured) = resolve_vault_location(&app)?;
    if configured && current_path == source_path {
        return Err("Switch to another vault before deleting the active vault.".to_string());
    }

    let trash_dir = parent.join(".Trash");
    fs::create_dir_all(&trash_dir).map_err(|e| e.to_string())?;
    let source_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| sanitize_path_segment(name, "Starlane Notes Vault"))
        .unwrap_or_else(|| "Starlane Notes Vault".to_string());
    let deleted_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let mut target_path = trash_dir.join(format!("{}-{}", source_name, deleted_at));
    let mut suffix = 2;
    while target_path.exists() {
        target_path = trash_dir.join(format!("{}-{}-{}", source_name, deleted_at, suffix));
        suffix += 1;
    }

    fs::rename(&source_path, &target_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn move_vault_to_icloud(
    app: AppHandle,
    folder_name: Option<String>,
) -> Result<VaultLocation, String> {
    let (current_path, _) = resolve_vault_dir(&app)?;
    ensure_vault_dir(&current_path)?;

    let target_name = folder_name
        .as_deref()
        .map(|name| sanitize_path_segment(name, "Starlane Notes Vault"))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| {
            configured_or_current_vault_name(&app)
                .unwrap_or_else(|_| "Starlane Notes Vault".to_string())
        });
    let icloud_root = apple_icloud_documents_dir()?;
    let target_path = icloud_root.join(target_name);
    ensure_vault_dir(&target_path)?;
    copy_dir_contents_missing(&current_path, &target_path)?;
    set_vault_path(app, target_path.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn write_icloud_provider_sync_manifest(
    mutation_id: String,
    manifest: Value,
) -> Result<ICloudProviderSyncManifestWriteResult, String> {
    let icloud_root = apple_icloud_documents_dir()?;
    write_provider_sync_manifest_file(&icloud_root, &mutation_id, &manifest)
}

#[tauri::command]
pub fn set_vault_path(app: AppHandle, path: String) -> Result<VaultLocation, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Vault path is required".to_string());
    }

    let vault_path = PathBuf::from(trimmed);
    ensure_vault_dir(&vault_path)?;

    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    #[cfg(target_os = "ios")]
    if let Ok(documents_dir) = app.path().document_dir() {
        if let Some(relative_path) = documents_relative_path(&vault_path, &documents_dir) {
            store.set(
                VAULT_DOCUMENTS_RELATIVE_PATH_KEY,
                serde_json::json!(relative_path.to_string_lossy()),
            );
            store.delete(VAULT_PATH_KEY);
            store.save().map_err(|e| e.to_string())?;
            write_ios_vault_location_fallback(&documents_dir, &vault_path)?;
            return Ok(VaultLocation {
                path: vault_path.to_string_lossy().into_owned(),
                using_default: false,
                configured: true,
            });
        }
    }
    store.set(
        VAULT_PATH_KEY,
        serde_json::json!(vault_path.to_string_lossy()),
    );
    #[cfg(target_os = "ios")]
    store.delete(VAULT_DOCUMENTS_RELATIVE_PATH_KEY);
    store.save().map_err(|e| e.to_string())?;
    #[cfg(target_os = "ios")]
    if let Ok(documents_dir) = app.path().document_dir() {
        write_ios_vault_location_fallback(&documents_dir, &vault_path)?;
    }

    Ok(VaultLocation {
        path: vault_path.to_string_lossy().into_owned(),
        using_default: false,
        configured: true,
    })
}

#[tauri::command]
pub fn clear_vault_path(app: AppHandle) -> Result<VaultLocation, String> {
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    store.delete(VAULT_PATH_KEY);
    #[cfg(target_os = "ios")]
    store.delete(VAULT_DOCUMENTS_RELATIVE_PATH_KEY);
    store.save().map_err(|e| e.to_string())?;
    #[cfg(target_os = "ios")]
    if let Ok(documents_dir) = app.path().document_dir() {
        let _ = fs::remove_file(ios_vault_location_fallback_path(&documents_dir));
    }
    get_vault_location(app)
}

#[tauri::command]
pub fn clear_vault_path_without_initializing(app: AppHandle) -> Result<VaultLocation, String> {
    let store = app.store(SETTINGS_FILE).map_err(|e| e.to_string())?;
    store.delete(VAULT_PATH_KEY);
    #[cfg(target_os = "ios")]
    store.delete(VAULT_DOCUMENTS_RELATIVE_PATH_KEY);
    store.save().map_err(|e| e.to_string())?;
    #[cfg(target_os = "ios")]
    if let Ok(documents_dir) = app.path().document_dir() {
        let _ = fs::remove_file(ios_vault_location_fallback_path(&documents_dir));
    }

    let (path, using_default) = resolve_vault_dir(&app)?;
    Ok(VaultLocation {
        path: path.to_string_lossy().into_owned(),
        using_default,
        configured: false,
    })
}

#[tauri::command]
pub fn reveal_vault_folder(app: AppHandle) -> Result<(), String> {
    let (path, _) = resolve_vault_dir(&app)?;
    ensure_vault_dir(&path)?;

    #[cfg(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "freebsd"
    ))]
    {
        #[cfg(target_os = "macos")]
        let mut command = {
            let mut command = Command::new("open");
            command.arg(&path);
            command
        };

        #[cfg(target_os = "windows")]
        let mut command = {
            let mut command = Command::new("explorer");
            command.arg(&path);
            command
        };

        #[cfg(all(unix, not(target_os = "macos")))]
        let mut command = {
            let mut command = Command::new("xdg-open");
            command.arg(&path);
            command
        };

        command.spawn().map_err(|e| e.to_string())?;
        Ok(())
    }

    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "freebsd"
    )))]
    {
        Err("Reveal vault folder is unavailable on this platform".to_string())
    }
}

#[tauri::command]
pub fn read_vault_snapshot(app: AppHandle, provider: String) -> Result<Option<Value>, String> {
    let snapshot_path = resolve_snapshot_path(&app, &provider)?;
    if !snapshot_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(snapshot_path).map_err(|e| e.to_string())?;
    serde_json::from_str(&contents)
        .map(Some)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn write_vault_snapshot(
    app: AppHandle,
    provider: String,
    snapshot: Value,
) -> Result<(), String> {
    preflight_vault_snapshot(&app, &snapshot)?;
    let snapshot_path = resolve_snapshot_path(&app, &provider)?;
    let temp_path = snapshot_path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;

    fs::write(&temp_path, contents).map_err(|e| e.to_string())?;
    fs::File::open(&temp_path)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    replace_snapshot_file(&temp_path, &snapshot_path)?;
    write_filesystem_mirror(&app, &snapshot)
}

fn write_vault_snapshot_at(vault_dir:&Path,provider:&str,snapshot:&Value)->Result<(),String> {
    mirror_admission::Plan::prepare(vault_dir,snapshot)?;
    ensure_vault_dir(vault_dir)?;
    let snapshot_path=vault_dir.join(snapshot_file_name(provider));
    let temp_path = snapshot_path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(snapshot).map_err(|e| e.to_string())?;
    fs::write(&temp_path, contents).map_err(|e| e.to_string())?;
    fs::File::open(&temp_path)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    replace_snapshot_file(&temp_path, &snapshot_path)?;
    write_filesystem_mirror_at(vault_dir,snapshot)
}

#[tauri::command]
pub fn reconcile_vault_mirror(
    app: AppHandle,
    provider: String,
) -> Result<MirrorReconciliationResult, String> {
    preflight_vault_mirror(&app)?;
    let (vault_dir, _) = resolve_vault_dir(&app)?;
    let Some(snapshot) = read_vault_snapshot(app.clone(), provider.clone())? else {
        return Err("No local vault snapshot was found".to_string());
    };
    let result = reconcile_filesystem_mirror_snapshot(&vault_dir, snapshot)?;
    preflight_vault_snapshot(&app,&result.snapshot)?;

    let snapshot_path = resolve_snapshot_path(&app, &provider)?;
    let temp_path = snapshot_path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(&result.snapshot).map_err(|e| e.to_string())?;
    fs::write(&temp_path, contents).map_err(|e| e.to_string())?;
    fs::File::open(&temp_path)
        .and_then(|file| file.sync_all())
        .map_err(|e| e.to_string())?;
    replace_snapshot_file(&temp_path, &snapshot_path)?;
    write_filesystem_mirror(&app, &result.snapshot)?;

    Ok(result)
}

#[tauri::command]
pub fn reset_vault_snapshot(app: AppHandle, provider: String) -> Result<(), String> {
    preflight_vault_mirror(&app)?;
    let snapshot_path = resolve_snapshot_path(&app, &provider)?;
    if snapshot_path.exists() {
        fs::remove_file(snapshot_path).map_err(|e| e.to_string())?;
    }
    reset_filesystem_mirror(&app)?;
    Ok(())
}

// Export publication owns only its exclusive temporary file, never the selected
// target. Parent aliases are resolved and rechecked from the original selection.
mod archive_export_output {
    use super::*;
    use sha2::{Digest,Sha256};
    #[cfg(unix)]
    use std::os::unix::{fs::MetadataExt,io::{AsRawFd,FromRawFd},ffi::OsStrExt};
    fn error(reason:&str)->String {format!("Vault archive export stopped: {reason}; existing output and source files were not replaced")}
    fn normal(path:&Path)->Result<(),String> {
        if !path.is_absolute() || path.components().any(|c|!matches!(c,std::path::Component::RootDir|std::path::Component::Prefix(_)|std::path::Component::Normal(_))) {return Err(error("select an absolute normal output path"));}
        Ok(())
    }
    fn physical_directory(path:&Path)->Result<PathBuf,String> {
        normal(path)?;
        match fs::canonicalize(path) {
            Ok(value)=>Ok(value),
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{
                let parent=path.parent().ok_or_else(||error("missing output parent"))?;
                let leaf=path.file_name().ok_or_else(||error("missing output directory name"))?;
                Ok(physical_directory(parent)?.join(leaf))
            },
            Err(e)=>Err(error(&format!("parent resolution: {e}"))),
        }
    }
    #[cfg(windows)]
    mod windows_identity {
        use super::*;
        use std::os::windows::io::AsRawHandle;
        #[repr(C)] struct FileTime {low:u32,high:u32}
        #[repr(C)] struct HandleInformation {
            attributes:u32,creation:FileTime,access:FileTime,write:FileTime,
            volume:u32,size_high:u32,size_low:u32,links:u32,index_high:u32,index_low:u32,
        }
        #[repr(C)] struct FileIdInformation {volume:u64,id:[u8;16]}
        #[link(name="kernel32")]
        extern "system" {
            fn GetFileInformationByHandle(handle:*mut std::ffi::c_void,information:*mut HandleInformation)->i32;
            fn GetFileInformationByHandleEx(handle:*mut std::ffi::c_void,class:u32,information:*mut std::ffi::c_void,size:u32)->i32;
        }
        pub(super) fn proof(file:&fs::File)->Result<(u64,[u8;16],u32,u32),String> {
            let mut ordinary:HandleInformation=unsafe {std::mem::zeroed()};
            let mut extended:FileIdInformation=unsafe {std::mem::zeroed()};
            // FileIdInfo=18 supplies 128-bit identity, including ReFS. The old
            // 64-bit file index alone is not used as physical identity.
            if unsafe {GetFileInformationByHandle(file.as_raw_handle(),&mut ordinary)}==0 || unsafe {GetFileInformationByHandleEx(file.as_raw_handle(),18,&mut extended as *mut _ as *mut std::ffi::c_void,std::mem::size_of::<FileIdInformation>() as u32)}==0 {
                return Err(error(&format!("this destination/source does not provide required Windows file identity: {}",std::io::Error::last_os_error())));
            }
            Ok((extended.volume,extended.id,ordinary.links,ordinary.attributes))
        }
    }
    #[derive(Clone,Debug,PartialEq)]
    struct Identity {
        #[cfg(unix)] dev:u64,
        #[cfg(unix)] ino:u64,
        #[cfg(windows)] volume:u64,
        #[cfg(windows)] file_id:[u8;16],
        len:u64,
        modified:Option<SystemTime>,
    }
    fn identity(file:&fs::File,regular:bool)->Result<Identity,String> {
        let m=file.metadata().map_err(|e|error(&e.to_string()))?;
        if (regular && !m.is_file()) || (!regular && !m.is_dir()) {return Err(error("unexpected source/output file type"));}
        #[cfg(windows)] let (volume,file_id,_,attributes)=windows_identity::proof(file)?;
        #[cfg(windows)] if attributes&0x400!=0 {return Err(error("unexpected reparse handle"));}
        Ok(Identity {#[cfg(unix)]dev:m.dev(),#[cfg(unix)]ino:m.ino(),#[cfg(windows)]volume,#[cfg(windows)]file_id,len:m.len(),modified:m.modified().ok()})
    }
    fn single_link(file:&fs::File)->Result<(),String> {
        identity(file,true)?;
        #[cfg(unix)] if file.metadata().map_err(|e|error(&e.to_string()))?.nlink()!=1 {return Err(error("hardlinked output file"));}
        #[cfg(windows)] if windows_identity::proof(file)?.2!=1 {return Err(error("hardlinked output file"));}
        Ok(())
    }
    fn same_directory(a:&fs::File,b:&fs::File)->Result<bool,String> {
        let a=identity(a,false)?;let b=identity(b,false)?;
        #[cfg(unix)] {Ok(a.dev==b.dev && a.ino==b.ino)}
        #[cfg(windows)] {Ok(a.volume==b.volume && a.file_id==b.file_id)}
        #[cfg(not(any(unix,windows)))] {Ok(a==b)}
    }
    fn open_directory(path:&Path)->Result<fs::File,String> {
        #[cfg(unix)] {
            let name=std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_|error("invalid directory path"))?;
            let fd=unsafe {libc::open(name.as_ptr(),libc::O_RDONLY|libc::O_DIRECTORY|libc::O_NOFOLLOW|libc::O_CLOEXEC)};
            if fd<0 {return Err(error(&std::io::Error::last_os_error().to_string()));}
            Ok(unsafe {fs::File::from_raw_fd(fd)})
        }
        #[cfg(not(unix))] {
            let m=fs::symlink_metadata(path).map_err(|e|error(&e.to_string()))?;
            if m.file_type().is_symlink() || !m.is_dir() {return Err(error("unsafe directory"));}
            #[cfg(windows)] {
                use std::os::windows::fs::{MetadataExt,OpenOptionsExt};
                if m.file_attributes()&0x400!=0 {return Err(error("directory reparse point"));}
                // Open directory handle without following a final reparse leaf.
                fs::OpenOptions::new().read(true).custom_flags(0x02000000|0x00200000).open(path).map_err(|e|error(&e.to_string()))
            }
            #[cfg(not(windows))] {fs::File::open(path).map_err(|e|error(&e.to_string()))}
        }
    }
    fn open_leaf(parent:&fs::File,physical:&Path,leaf:&std::ffi::OsStr)->Result<Option<fs::File>,String> {
        #[cfg(unix)] {
            let leaf=std::ffi::CString::new(leaf.as_bytes()).map_err(|_|error("invalid leaf"))?;
            let fd=unsafe {libc::openat(parent.as_raw_fd(),leaf.as_ptr(),libc::O_RDONLY|libc::O_NOFOLLOW|libc::O_NONBLOCK|libc::O_CLOEXEC)};
            if fd<0 {let e=std::io::Error::last_os_error();if e.kind()==std::io::ErrorKind::NotFound {return Ok(None);}return Err(error(&e.to_string()));}
            Ok(Some(unsafe {fs::File::from_raw_fd(fd)}))
        }
        #[cfg(not(unix))] {
            let path=physical.join(leaf);
            let m=match fs::symlink_metadata(&path) {Ok(m)=>m,Err(e) if e.kind()==std::io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(error(&e.to_string()))};
            #[cfg(windows)] let reparse={use std::os::windows::fs::MetadataExt;m.file_attributes()&0x400!=0};
            #[cfg(not(windows))] let reparse=false;
            if m.file_type().is_symlink() || reparse || !m.is_file() {return Err(error("unsafe leaf"));}
            #[cfg(windows)] {
                use std::os::windows::fs::OpenOptionsExt;
                fs::OpenOptions::new().read(true).custom_flags(0x00200000).open(path).map(Some).map_err(|e|error(&e.to_string()))
            }
            #[cfg(not(windows))] {fs::File::open(path).map(Some).map_err(|e|error(&e.to_string()))}
        }
    }
    fn digest(file:&mut fs::File)->Result<Vec<u8>,String> {
        file.rewind().map_err(|e|error(&e.to_string()))?;let mut hash=Sha256::new();let mut buffer=[0u8;65536];
        loop {let n=file.read(&mut buffer).map_err(|e|error(&e.to_string()))?;if n==0 {break;}hash.update(&buffer[..n]);}
        file.rewind().map_err(|e|error(&e.to_string()))?;Ok(hash.finalize().to_vec())
    }
    struct Source {path:PathBuf,id:Identity,sha:Vec<u8>}
    impl Source {
        fn capture(path:&Path)->Result<Self,String> {
            let mut file=Self::open_current(path)?;
            let id=identity(&file,true)?;let sha=digest(&mut file)?;
            let value=Self {path:path.into(),id,sha};value.verify()?;Ok(value)
        }
        fn verify(&self)->Result<(),String> {
            let mut current=Self::open_current(&self.path)?;
            if identity(&current,true)?!=self.id || digest(&mut current)?!=self.sha {return Err(error("source changed"));}
            Ok(())
        }
        fn open_current(path:&Path)->Result<fs::File,String> {
            // Existing export accepts readable source symlinks. Follow ONLY input
            // paths, then bind the actual regular opened file identity and bytes.
            #[cfg(unix)] {
                let path=std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_|error("invalid source path"))?;
                let fd=unsafe {libc::open(path.as_ptr(),libc::O_RDONLY|libc::O_NONBLOCK|libc::O_CLOEXEC)};
                if fd<0 {return Err(error(&std::io::Error::last_os_error().to_string()));}
                Ok(unsafe {fs::File::from_raw_fd(fd)})
            }
            #[cfg(not(unix))] {fs::File::open(path).map_err(|e|error(&e.to_string()))}
        }
    }
    struct Root {selected:PathBuf,physical:PathBuf,file:fs::File,protected:Vec<(PathBuf,fs::File)>}
    impl Root {
        fn capture(selected:&Path)->Result<Self,String> {
            normal(selected)?;
            // Do not canonicalize final root leaf: a root symlink is not authority.
            let parent=fs::canonicalize(selected.parent().ok_or_else(||error("root parent missing"))?).map_err(|e|error(&e.to_string()))?;
            let physical=parent.join(selected.file_name().ok_or_else(||error("root leaf missing"))?);
            let file=open_directory(&physical)?;
            let mut protected=Vec::new();
            for relative in [VAULT_INTERNAL_DIR,"assets",MIRROR_SPACES_DIR] {
                let path=physical.join(relative);
                match fs::canonicalize(&path) {
                    Ok(actual)=>protected.push((path,open_directory(&actual)?)),
                    Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},
                    Err(e)=>return Err(error(&e.to_string())),
                }
            }
            Ok(Self {selected:selected.into(),physical,file,protected})
        }
        fn verify(&self)->Result<(),String> {
            let current=Self::capture(&self.selected)?;
            if current.physical!=self.physical || !same_directory(&self.file,&current.file)? || current.protected.len()!=self.protected.len() {return Err(error("selected vault root changed"));}
            for ((original_path,original),(current_path,current)) in self.protected.iter().zip(&current.protected) {
                if original_path!=current_path || !same_directory(original,current)? {return Err(error("protected vault directory changed"));}
            }
            Ok(())
        }
    }
    impl Root {
        fn reject_output_parent(&self,parent:&Path,leaf:&std::ffi::OsStr)->Result<(),String> {
            if parent.join(leaf).starts_with(&self.physical) {return Err(error("output must be outside the selected vault"));}
            // Native identity supplements strings: case aliases and direct binds
            // of the root/known internal, asset and mirror directories are unsafe.
            for ancestor in parent.ancestors() {
                match fs::symlink_metadata(ancestor) {
                    Ok(_)=>{
                        let actual=open_directory(ancestor)?;
                        if same_directory(&self.file,&actual)? {return Err(error("output directory aliases the vault root"));}
                        for (_,protected) in &self.protected {if same_directory(protected,&actual)? {return Err(error("output directory aliases protected vault storage"));}}
                    },
                    Err(e) if e.kind()==std::io::ErrorKind::NotFound=>{},
                    Err(e)=>return Err(error(&e.to_string())),
                }
            }
            Ok(())
        }
    }
    struct Output {selected:PathBuf,physical_parent:PathBuf,parent:fs::File,previous:Option<Source>,previous_file:Option<fs::File>,temp_name:String,temp:fs::File}
    impl Output {
        fn prepare(root:&Root,selected:&Path,sources:&[Source])->Result<Self,String> {
            normal(selected)?;let leaf=selected.file_name().ok_or_else(||error("output leaf missing"))?;
            let requested_parent=selected.parent().ok_or_else(||error("output parent missing"))?;
            let physical_parent=physical_directory(requested_parent)?;
            root.reject_output_parent(&physical_parent,leaf)?;
            root.verify()?;
            // Native save dialogs normally create this parent; direct callers retain
            // supported directory creation, after containment admission above.
            fs::create_dir_all(requested_parent).map_err(|e|error(&e.to_string()))?;
            if fs::canonicalize(requested_parent).map_err(|e|error(&e.to_string()))?!=physical_parent {return Err(error("output parent changed"));}
            root.reject_output_parent(&physical_parent,leaf)?;
            let parent=open_directory(&physical_parent)?;
            let mut previous_file=None;
            let previous=if let Some(mut file)=open_leaf(&parent,&physical_parent,leaf)? {
                single_link(&file)?;
                let source=Source {path:selected.into(),id:identity(&file,true)?,sha:digest(&mut file)?};
                previous_file=Some(file);Some(source)
            } else {None};
            if let Some(old)=&previous {
                if sources.iter().any(|source|source.id==old.id) {return Err(error("output aliases an input source"));}
                #[cfg(unix)] if sources.iter().any(|source|source.id.dev==old.id.dev && source.id.ino==old.id.ino) {return Err(error("output aliases an input source"));}
                #[cfg(windows)] if sources.iter().any(|source|source.id.volume==old.id.volume && source.id.file_id==old.id.file_id) {return Err(error("output aliases an input source"));}
            }
            // Creator FD establishes ownership. No first reopen establishes it.
            use std::sync::atomic::{AtomicU64,Ordering};static SERIAL:AtomicU64=AtomicU64::new(0);
            let temp_name=format!(".starlane-export-{}-{}-{}.tmp",std::process::id(),now_nanos()?,SERIAL.fetch_add(1,Ordering::Relaxed));
            #[cfg(unix)] let temp={
                let name=std::ffi::CString::new(temp_name.as_str()).map_err(|_|error("invalid temporary name"))?;
                let fd=unsafe {libc::openat(parent.as_raw_fd(),name.as_ptr(),libc::O_RDWR|libc::O_CREAT|libc::O_EXCL|libc::O_NOFOLLOW|libc::O_CLOEXEC,0o600)};
                if fd<0 {return Err(error(&std::io::Error::last_os_error().to_string()));}
                unsafe {fs::File::from_raw_fd(fd)}
            };
            #[cfg(not(unix))] let temp=fs::OpenOptions::new().read(true).write(true).create_new(true).open(physical_parent.join(&temp_name)).map_err(|e|error(&e.to_string()))?;
            Ok(Self {selected:selected.into(),physical_parent,parent,previous,previous_file,temp_name,temp})
        }
        fn publish(mut self,root:&Root,sources:&[Source])->Result<(),String> {
            self.temp.sync_all().map_err(|e|error(&e.to_string()))?;
            single_link(&self.temp)?;
            let expected_temp=identity(&self.temp,true)?;let expected_sha=digest(&mut self.temp)?;
            root.verify()?;for source in sources {source.verify()?;}
            let requested_parent=self.selected.parent().ok_or_else(||error("output parent missing"))?;
            if fs::canonicalize(requested_parent).map_err(|e|error(&e.to_string()))?!=self.physical_parent || !same_directory(&self.parent,&open_directory(&self.physical_parent)?)? {return Err(error("output parent changed"));}
            let leaf=self.selected.file_name().ok_or_else(||error("output leaf missing"))?;
            root.reject_output_parent(&self.physical_parent,leaf)?;
            match (&self.previous,open_leaf(&self.parent,&self.physical_parent,leaf)?) {
                (Some(old),Some(mut current))=>{single_link(&current)?;if identity(&current,true)?!=old.id || digest(&mut current)?!=old.sha || self.previous_file.as_ref().map(|file|identity(file,true)).transpose()?.as_ref()!=Some(&old.id) {return Err(error("selected output changed"));}},(None,None)=>{},_=>return Err(error("selected output changed")),
            }
            let mut current=open_leaf(&self.parent,&self.physical_parent,std::ffi::OsStr::new(&self.temp_name))?.ok_or_else(||error("temporary archive missing"))?;
            single_link(&current)?;
            if identity(&current,true)?!=expected_temp || digest(&mut current)?!=expected_sha {return Err(error("temporary archive changed"));}
            // Last bounded physical containment reproof follows all byte checks.
            root.verify()?;root.reject_output_parent(&self.physical_parent,leaf)?;
            if self.previous.is_none() {
                // Standard hard-link publication is atomic no-clobber. Recheck
                // BOTH creator and published leaf before creator-only cleanup.
                #[cfg(unix)] {
                    let from=std::ffi::CString::new(self.temp_name.as_str()).map_err(|_|error("invalid temporary name"))?;
                    let to=std::ffi::CString::new(leaf.as_bytes()).map_err(|_|error("invalid output leaf"))?;
                    if unsafe {libc::linkat(self.parent.as_raw_fd(),from.as_ptr(),self.parent.as_raw_fd(),to.as_ptr(),0)}!=0 {return Err(error(&std::io::Error::last_os_error().to_string()));}
                    let acknowledgement=(||->Result<(),String> {
                        let mut published=open_leaf(&self.parent,&self.physical_parent,leaf)?.ok_or_else(||"published leaf missing".to_string())?;
                        let mut creator=open_leaf(&self.parent,&self.physical_parent,std::ffi::OsStr::new(&self.temp_name))?.ok_or_else(||"creator evidence missing".to_string())?;
                        if identity(&published,true)?!=expected_temp || digest(&mut published)?!=expected_sha || identity(&creator,true)?!=expected_temp || digest(&mut creator)?!=expected_sha || identity(&self.temp,true)?!=expected_temp {return Err("published/creator evidence changed".into());}
                        Ok(())
                    })();
                    acknowledgement.map_err(|e|format!("Archive publication may have succeeded; retain evidence for review: {e}"))?;
                    if unsafe {libc::unlinkat(self.parent.as_raw_fd(),from.as_ptr(),0)}!=0 {return Err(format!("Archive published; temporary evidence retained: {}",std::io::Error::last_os_error()));}
                }
                #[cfg(not(unix))] {
                    fs::hard_link(self.physical_parent.join(&self.temp_name),self.physical_parent.join(leaf)).map_err(|e|error(&e.to_string()))?;
                    #[cfg(windows)] {
                        let acknowledgement=(||->Result<(),String> {
                            let mut published=open_leaf(&self.parent,&self.physical_parent,leaf)?.ok_or_else(||"published leaf missing".to_string())?;
                            let mut creator=open_leaf(&self.parent,&self.physical_parent,std::ffi::OsStr::new(&self.temp_name))?.ok_or_else(||"creator evidence missing".to_string())?;
                            if identity(&published,true)?!=expected_temp || digest(&mut published)?!=expected_sha || identity(&creator,true)?!=expected_temp || digest(&mut creator)?!=expected_sha || identity(&self.temp,true)?!=expected_temp {return Err("published/creator evidence changed".into());}
                            Ok(())
                        })();
                        acknowledgement.map_err(|e|format!("Archive publication may have succeeded; retain evidence for review: {e}"))?;
                        fs::remove_file(self.physical_parent.join(&self.temp_name)).map_err(|e|format!("Archive published; creator evidence retained: {e}"))?;
                    }
                    // Other nonUnix platforms retain their creator alias rather
                    // than infer physical origin from size/time alone.
                }
            } else {
                #[cfg(unix)] {
                    let from=std::ffi::CString::new(self.temp_name.as_str()).map_err(|_|error("invalid temporary name"))?;
                    let to=std::ffi::CString::new(leaf.as_bytes()).map_err(|_|error("invalid output leaf"))?;
                    if unsafe {libc::renameat(self.parent.as_raw_fd(),from.as_ptr(),self.parent.as_raw_fd(),to.as_ptr())}!=0 {return Err(error(&std::io::Error::last_os_error().to_string()));}
                }
                #[cfg(not(unix))] {
                    // Standard rename replaces existing regular external archives;
                    // never remove the target or use a guessed shared backup name.
                    fs::rename(self.physical_parent.join(&self.temp_name),self.physical_parent.join(leaf)).map_err(|e|error(&e.to_string()))?;
                }
            }
            #[cfg(unix)] self.parent.sync_all().map_err(|e|format!("Archive published, but directory durability acknowledgement failed: {e}"))?;
            Ok(())
        }
    }
    pub(super) fn export(root_path:&Path,snapshot_path:&Path,output:&Path)->Result<(),String> {
        let root=Root::capture(root_path)?;
        // Retain the snapshot read descriptor while capturing exact original JSON.
        let mut snapshot_file=Source::open_current(snapshot_path)?;
        let snapshot_id=identity(&snapshot_file,true)?;
        let mut snapshot_contents=String::new();snapshot_file.read_to_string(&mut snapshot_contents).map_err(|e|error(&e.to_string()))?;
        let mut sources=vec![Source {path:snapshot_path.into(),id:snapshot_id,sha:Sha256::digest(snapshot_contents.as_bytes()).to_vec()}];
        sources[0].verify()?;let snapshot=parse_archive_snapshot(&snapshot_contents)?;
        let mut entries=Vec::new();
        for asset in snapshot.assets.iter().filter(|asset|asset.source=="tauri") {
            if asset.vault_path.trim().is_empty() {return Err(error("referenced native asset path is empty"));}
            sources.push(Source::capture(Path::new(&asset.vault_path))?);entries.push(archive_asset_path(asset));
        }
        // Asset descriptors are bounded: capture/reproof one at a time, never one
        // held FD per asset. All alias identities are known before stage creation.
        for source in &sources {source.verify()?;}
        let output=Output::prepare(&root,output,&sources)?;
        let mut archive=zip::ZipWriter::new(output.temp.try_clone().map_err(|e|error(&e.to_string()))?);
        let options=SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        archive.start_file(ARCHIVE_SNAPSHOT_FILE,options).map_err(|e|error(&e.to_string()))?;
        archive.write_all(snapshot_contents.as_bytes()).map_err(|e|error(&e.to_string()))?;
        for (source,entry) in sources.iter().skip(1).zip(entries) {
            let mut input=Source::open_current(&source.path)?;
            if identity(&input,true)?!=source.id {return Err(error("source changed before streaming"));}
            archive.start_file(entry,options).map_err(|e|error(&e.to_string()))?;
            let mut hash=Sha256::new();let mut buffer=[0u8;65536];
            loop {let n=input.read(&mut buffer).map_err(|e|error(&e.to_string()))?;if n==0 {break;}hash.update(&buffer[..n]);archive.write_all(&buffer[..n]).map_err(|e|error(&e.to_string()))?;}
            if hash.finalize().to_vec()!=source.sha {return Err(error("source bytes changed during ZIP streaming"));}
            source.verify()?;
        }
        archive.finish().map_err(|e|error(&e.to_string()))?;
        output.publish(&root,&sources)
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        fn fixture()->PathBuf {let path=std::env::temp_dir().join(format!("starlane-export-{}-{}",std::process::id(),now_nanos().unwrap()));fs::create_dir_all(&path).unwrap();path}
        fn snapshot(root:&Path,asset:Option<&Path>)->PathBuf {
            let path=root.join("vault.json");
            let assets=asset.map(|p|vec![serde_json::json!({"id":"asset","name":"image.png","mime_type":"image/png","size":196608,"url":"original://asset","vault_path":p.to_string_lossy(),"source":"tauri","created_at":"original-time","unknown":"retained"})]).unwrap_or_default();
            fs::write(&path,serde_json::to_vec(&serde_json::json!({"schema_version":LOCAL_VAULT_SCHEMA_VERSION,"spaces":[],"folders":[],"documents":[{"id":"page","unknown":{"raw":"keep"}}],"assets":assets,"unknown_root":"original"})).unwrap()).unwrap();path
        }
        #[test]
        fn external_new_and_existing_archive_preserve_raw_snapshot_and_large_deflated_asset() {
            let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let asset=root.join("image.png");let bytes=vec![73u8;196608];fs::write(&asset,&bytes).unwrap();let snapshot=snapshot(&root,Some(&asset));
            let output=dir.join("new-parent/archive.zip");export(&root,&snapshot,&output).unwrap();fs::write(&output,b"previous archive").unwrap();export(&root,&snapshot,&output).unwrap();
            let mut zip=zip::ZipArchive::new(fs::File::open(&output).unwrap()).unwrap();let mut raw=Vec::new();zip.by_name(ARCHIVE_SNAPSHOT_FILE).unwrap().read_to_end(&mut raw).unwrap();assert_eq!(raw,fs::read(&snapshot).unwrap());
            let name={let parsed=parse_archive_snapshot(std::str::from_utf8(&raw).unwrap()).unwrap();archive_asset_path(&parsed.assets[0])};let mut entry=zip.by_name(&name).unwrap();assert_eq!(entry.compression(),zip::CompressionMethod::Deflated);let mut restored=Vec::new();entry.read_to_end(&mut restored).unwrap();assert_eq!(restored,bytes);
            #[cfg(unix)] {
                use std::os::unix::fs::symlink;
                let alias=dir.join("source-alias.png");symlink(&asset,&alias).unwrap();let mut value:Value=serde_json::from_slice(&fs::read(&snapshot).unwrap()).unwrap();value["assets"][0]["vault_path"]=Value::from(alias.to_string_lossy().into_owned());fs::write(&snapshot,serde_json::to_vec(&value).unwrap()).unwrap();export(&root,&snapshot,&dir.join("linked-source.zip")).unwrap();
            }
            fs::remove_dir_all(dir).unwrap();
        }
        #[test]
        fn inside_vault_and_source_alias_outputs_reject_without_creation_or_truncation() {
            let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let external=dir.join("external.bin");fs::write(&external,b"external source").unwrap();let snapshot=snapshot(&root,Some(&external));let original=fs::read(&snapshot).unwrap();
            for known in [".starlane/receipt.json","assets/asset.bin","spaces/mirror.md"] {let path=root.join(known);fs::create_dir_all(path.parent().unwrap()).unwrap();fs::write(path,b"protected sentinel").unwrap();}
            for output in [root.join(".starlane/versions/new.zip"),root.join("assets/new/archive.zip"),root.join("spaces/new/archive.zip"),snapshot.clone(),external.clone()] {assert!(export(&root,&snapshot,&output).is_err());}
            assert!(!root.join(".starlane/versions").exists());assert!(!root.join("assets/new").exists());assert!(!root.join("spaces/new").exists());for known in [".starlane/receipt.json","assets/asset.bin","spaces/mirror.md"] {assert_eq!(fs::read(root.join(known)).unwrap(),b"protected sentinel");}assert_eq!(fs::read(&snapshot).unwrap(),original);assert_eq!(fs::read(&external).unwrap(),b"external source");fs::remove_dir_all(dir).unwrap();
        }
        #[cfg(any(windows,target_os="macos"))]
        #[test]
        fn actual_case_insensitive_root_alias_cannot_create_inside_output() {
            let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let alias=dir.join("VAULT");
            // A case-sensitive volume has no such alias; its ordinary proof is
            // covered separately. Never claim this as a case-fold device test.
            if !alias.exists() {fs::remove_dir_all(dir).unwrap();return;}
            let snapshot=snapshot(&root,None);assert!(export(&alias,&snapshot,&root.join("new/archive.zip")).is_err());assert!(!root.join("new").exists());fs::remove_dir_all(dir).unwrap();
        }
        #[test]
        fn missing_referenced_asset_retains_previous_output_and_creates_no_stage() {
            let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let snapshot=snapshot(&root,Some(&dir.join("missing.png")));let output=dir.join("archive.zip");fs::write(&output,b"old archive").unwrap();let before=fs::read_dir(&dir).unwrap().count();
            assert!(export(&root,&snapshot,&output).is_err());assert_eq!(fs::read(&output).unwrap(),b"old archive");assert_eq!(fs::read_dir(&dir).unwrap().count(),before);fs::remove_dir_all(dir).unwrap();
        }
        #[test]
        fn source_or_existing_output_drift_before_publication_retains_both_and_partial_stage() {
            for change_source in [true,false] {
                let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let snapshot=snapshot(&root,None);let output=dir.join("archive.zip");fs::write(&output,b"old archive").unwrap();let root_proof=Root::capture(&root).unwrap();let sources=vec![Source::capture(&snapshot).unwrap()];
                let mut staged=Output::prepare(&root_proof,&output,&sources).unwrap();staged.temp.write_all(b"completed archive").unwrap();let temp=dir.join(&staged.temp_name);
                if change_source {fs::write(&snapshot,b"changed snapshot").unwrap();} else {fs::write(&output,b"newer external archive").unwrap();}
                assert!(staged.publish(&root_proof,&sources).is_err());assert_eq!(fs::read(&output).unwrap(),if change_source {b"old archive".as_slice()} else {b"newer external archive".as_slice()});assert!(temp.exists());fs::remove_dir_all(dir).unwrap();
            }
        }
        #[cfg(unix)]
        #[test]
        fn symlink_hardlink_fifo_outputs_and_final_root_symlink_preserve_sentinels() {
            use std::os::unix::fs::symlink;
            for kind in ["symlink","hardlink","fifo"] {
                let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let snapshot=snapshot(&root,None);let output=dir.join("archive.zip");let sentinel=dir.join("sentinel");fs::write(&sentinel,b"sentinel").unwrap();
                if kind=="symlink" {symlink(&sentinel,&output).unwrap();} else if kind=="hardlink" {fs::hard_link(&sentinel,&output).unwrap();} else {let name=std::ffi::CString::new(output.as_os_str().as_bytes()).unwrap();assert_eq!(unsafe {libc::mkfifo(name.as_ptr(),0o600)},0);}
                assert!(export(&root,&snapshot,&output).is_err());assert_eq!(fs::read(&sentinel).unwrap(),b"sentinel");assert!(fs::symlink_metadata(&output).is_ok());fs::remove_dir_all(dir).unwrap();
            }
            let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let snapshot=snapshot(&root,None);let linked=dir.join("linked");symlink(&root,&linked).unwrap();let output=dir.join("archive.zip");assert!(export(&linked,&snapshot,&output).is_err());assert!(!output.exists());fs::remove_dir_all(dir).unwrap();
        }
        #[cfg(unix)]
        #[test]
        fn parent_aliases_admitted_physical_vault_containment_and_late_retarget_rejected() {
            use std::os::unix::fs::symlink;
            let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let snapshot=snapshot(&root,None);let inside_alias=dir.join("inside");symlink(&root,&inside_alias).unwrap();assert!(export(&root,&snapshot,&inside_alias.join("missing/archive.zip")).is_err());assert!(!root.join("missing").exists());
            let first=dir.join("first");let second=dir.join("second");fs::create_dir(&first).unwrap();fs::create_dir(&second).unwrap();let alias=dir.join("output-alias");symlink(&first,&alias).unwrap();let output=alias.join("archive.zip");export(&root,&snapshot,&output).unwrap();let original=fs::read(first.join("archive.zip")).unwrap();
            let root_proof=Root::capture(&root).unwrap();let sources=vec![Source::capture(&snapshot).unwrap()];let mut stage=Output::prepare(&root_proof,&output,&sources).unwrap();stage.temp.write_all(b"candidate").unwrap();fs::remove_file(&alias).unwrap();symlink(&second,&alias).unwrap();assert!(stage.publish(&root_proof,&sources).is_err());assert_eq!(fs::read(first.join("archive.zip")).unwrap(),original);assert!(!second.join("archive.zip").exists());fs::remove_dir_all(dir).unwrap();
        }
        #[test]
        fn new_target_created_after_admission_is_not_clobbered() {
            let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let snapshot=snapshot(&root,None);let output=dir.join("archive.zip");let root_proof=Root::capture(&root).unwrap();let sources=vec![Source::capture(&snapshot).unwrap()];let mut stage=Output::prepare(&root_proof,&output,&sources).unwrap();stage.temp.write_all(b"complete").unwrap();fs::write(&output,b"new unrelated output").unwrap();assert!(stage.publish(&root_proof,&sources).is_err());assert_eq!(fs::read(&output).unwrap(),b"new unrelated output");fs::remove_dir_all(dir).unwrap();
        }
        #[cfg(unix)]
        #[test]
        fn replaced_temporary_leaf_is_never_published_or_unlinked() {
            let dir=fixture();let root=dir.join("vault");fs::create_dir(&root).unwrap();let snapshot=snapshot(&root,None);let output=dir.join("archive.zip");fs::write(&output,b"old").unwrap();let root_proof=Root::capture(&root).unwrap();let sources=vec![Source::capture(&snapshot).unwrap()];let mut stage=Output::prepare(&root_proof,&output,&sources).unwrap();stage.temp.write_all(b"complete").unwrap();let temp=dir.join(&stage.temp_name);fs::rename(&temp,temp.with_extension("held")).unwrap();fs::write(&temp,b"unowned replacement").unwrap();assert!(stage.publish(&root_proof,&sources).is_err());assert_eq!(fs::read(&temp).unwrap(),b"unowned replacement");assert_eq!(fs::read(&output).unwrap(),b"old");fs::remove_dir_all(dir).unwrap();
        }
    }
}

#[tauri::command]
pub fn export_vault_archive<R:tauri::Runtime>(
    app: AppHandle<R>,
    provider: String,
    archive_path: String,
) -> Result<(), String> {
    if cfg!(mobile) {
        return Err("Vault archive export is only available in the desktop app".to_string());
    }
    // The JSON mirror can be stale after an acknowledged canonical SQLite
    // commit. Keep the command registered, but grant no output authority until
    // canonical capture, genuine destination custody and recovery are complete.
    let _ = (app, provider, archive_path);
    Err("Complete native vault export is unavailable until canonical archive ownership and destination recovery are ready. No archive was created or replaced.".to_string())
}

#[cfg(test)]
fn restore_archive_assets<R: std::io::Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    assets_dir: &Path,
    snapshot: &mut VaultSnapshot,
    raw: &mut Value,
) -> Result<(), String> {
    for (index, asset) in snapshot.assets.iter_mut().enumerate() {
        if asset.source != "tauri" {
            continue;
        }
        let Ok(mut zipped_asset) = archive.by_name(&archive_asset_path(asset)) else {
            continue;
        };
        let destination = assets_dir.join(archive_asset_name(asset));
        let mut destination_file = fs::File::create(&destination).map_err(|e| e.to_string())?;
        std::io::copy(&mut zipped_asset, &mut destination_file).map_err(|e| e.to_string())?;
        let metadata = fs::metadata(&destination).map_err(|e| e.to_string())?;
        let target = raw
            .get_mut("assets")
            .and_then(Value::as_array_mut)
            .and_then(|assets| assets.get_mut(index))
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "Vault asset snapshot binding is invalid".to_string())?;
        target.insert("size".into(), Value::from(metadata.len()));
        target.insert(
            "vault_path".into(),
            Value::String(destination.to_string_lossy().into_owned()),
        );
        target.insert("url".into(), Value::String(asset_url(&destination)));
    }
    Ok(())
}

// Captured ZIP sessions never restore from the caller's mutable archive path.
// Unix descriptor backend only; unsupported platforms fail before asset writes.
// Eight active sessions, ten-minute expiry. Linux anonymous pre-install files
// close without path deletion; named/published evidence remains retained.
// This is not a valid-import filesystem/SQL journal or restart cleanup protocol.
#[cfg(unix)]
mod archive_provenance;

#[cfg(unix)]
pub(crate) mod archive_admission {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::MetadataExt;
    use std::sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    };
    use std::time::{Duration, Instant};

    const SESSION_LIMIT: usize = 8;
    const SESSION_TTL: Duration = Duration::from_secs(600);
    static NEXT: AtomicU64 = AtomicU64::new(1);
    static SESSIONS: OnceLock<Mutex<HashMap<String, Prepared>>> = OnceLock::new();

    #[derive(Clone, Debug, PartialEq, Eq, Serialize)]
    struct FileProof {
        dev: u64,
        ino: u64,
        len: u64,
        modified: i64,
        nanos: i64,
        sha: String,
    }
    fn identity(file: &fs::File) -> Result<(u64, u64), String> {
        let m = file.metadata().map_err(|e| e.to_string())?;
        Ok((m.dev(), m.ino()))
    }
    fn digest(file: &mut fs::File) -> Result<String, String> {
        file.rewind().map_err(|e| e.to_string())?;
        let mut sha = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            let n = file.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            sha.update(&buf[..n]);
        }
        file.rewind().map_err(|e| e.to_string())?;
        Ok(format!("{:x}", sha.finalize()))
    }
    fn proof(file: &mut fs::File) -> Result<FileProof, String> {
        let before = file.metadata().map_err(|e| e.to_string())?;
        if !before.is_file() {
            return Err("Archive file must be regular".into());
        }
        let sha = digest(file)?;
        let after = file.metadata().map_err(|e| e.to_string())?;
        let fields = |m: &fs::Metadata| (m.dev(), m.ino(), m.len(), m.mtime(), m.mtime_nsec());
        if fields(&before) != fields(&after) {
            return Err("Archive file changed during inspection".into());
        }
        Ok(FileProof {
            dev: after.dev(),
            ino: after.ino(),
            len: after.len(),
            modified: after.mtime(),
            nanos: after.mtime_nsec(),
            sha,
        })
    }
    fn name(value: &std::ffi::OsStr) -> Result<CString, String> {
        use std::os::unix::ffi::OsStrExt;
        CString::new(value.as_bytes()).map_err(|_| "Invalid archive path".into())
    }
    fn open_at(dir: &fs::File, leaf: &std::ffi::OsStr, flags: i32) -> Result<fs::File, String> {
        let leaf = name(leaf)?;
        let fd = unsafe {
            libc::openat(
                dir.as_raw_fd(),
                leaf.as_ptr(),
                flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
            )
        };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(unsafe { fs::File::from_raw_fd(fd) })
    }
    fn dir(path: &Path, create: bool) -> Result<fs::File, String> {
        if !path.is_absolute() {
            return Err("Archive destination must be absolute".into());
        }
        let mut held = fs::File::open("/").map_err(|e| e.to_string())?;
        for part in path.components() {
            let std::path::Component::Normal(leaf) = part else {
                if matches!(part, std::path::Component::RootDir) {
                    continue;
                }
                return Err("Unsafe archive directory component".into());
            };
            if create {
                let leaf = name(leaf)?;
                let rc = unsafe { libc::mkdirat(held.as_raw_fd(), leaf.as_ptr(), 0o700) };
                if rc < 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
                {
                    return Err(std::io::Error::last_os_error().to_string());
                }
            }
            held = open_at(&held, leaf, libc::O_RDONLY | libc::O_DIRECTORY)?;
        }
        Ok(held)
    }
    fn leaf(asset: &VaultAsset) -> Result<String, String> {
        let value = archive_asset_name(asset);
        let p = Path::new(&value);
        if p.components().count() != 1
            || !matches!(p.components().next(), Some(std::path::Component::Normal(_)))
        {
            return Err("Unsafe archive asset identity".into());
        }
        Ok(value)
    }
    struct Target {
        index: usize,
        leaf: String,
        prior: Option<FileProof>,
    }
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Phase { Prepared, Planned, Committing, Extracting, Publishing, Installing, Restored }
    struct Creator {
        file: fs::File,
        id: (u64, u64),
        initial_links: u64,
        source_leaf: Option<String>,
    }
    struct Captured {
        creator: Creator,
        expected: FileProof,
    }
    struct Evidence {
        leaf: String,
        file: fs::File,
        expected: FileProof,
        linked: bool,
    }
    struct Stage {
        op: String,
        root_path: PathBuf,
        root: fs::File,
        root_id: (u64, u64),
        // Named fallback is retained, never renamed/unlinked. This directory
        // identity binds current custody; it is NOT mkdir-origin deletion proof.
        named: Option<(PathBuf, fs::File, (u64, u64))>,
        evidence_started: bool,
    }
    fn unsupported_anonymous(error: &std::io::Error) -> bool {
        matches!(error.raw_os_error(), Some(libc::EOPNOTSUPP | libc::EINVAL | libc::EISDIR | libc::ENOSYS | libc::EPERM))
    }
    impl Stage {
        fn new(root_path: &Path, force_named: bool) -> Result<(Self, Creator), String> {
            let root_path = fs::canonicalize(root_path).map_err(|e| e.to_string())?;
            let root = dir(&root_path, false)?;
            let root_id = identity(&root)?;
            let op = format!("{}-{}-{}", std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed));
            let mut stage = Self { op, root_path, root, root_id, named: None, evidence_started: false };
            #[cfg(target_os = "linux")]
            if !force_named {
                let flags = libc::O_TMPFILE | libc::O_RDWR;
                match open_at_io(&stage.root, std::ffi::OsStr::new("."), flags) {
                    Ok(file) => {
                        let creator = stage.bind_creator(file, None)?;
                        return Ok((stage, creator));
                    },
                    Err(error) if unsupported_anonymous(&error) => {},
                    Err(error) => return Err(error.to_string()),
                }
            }
            #[cfg(not(target_os = "linux"))]
            let _ = force_named;
            let component = format!("starlane-zip-{}", stage.op);
            let c = name(std::ffi::OsStr::new(&component))?;
            if unsafe { libc::mkdirat(stage.root.as_raw_fd(), c.as_ptr(), 0o700) } != 0 {
                return Err(std::io::Error::last_os_error().to_string());
            }
            #[cfg(test)]
            BEFORE_NAMED_DIRECTORY_OPEN.with(|h| { if let Some(hook) = h.borrow_mut().take() { hook(&stage.root_path.join(&component)); } });
            let held = open_at(&stage.root, std::ffi::OsStr::new(&component), libc::O_RDONLY | libc::O_DIRECTORY)?;
            let id = identity(&held)?;
            stage.named = Some((stage.root_path.join(component), held, id));
            stage.verify_root()?;
            let archive = stage.create("archive.zip")?;
            Ok((stage, archive))
        }
        fn bind_creator(&self, file: fs::File, source_leaf: Option<String>) -> Result<Creator, String> {
            // Ownership is established from the kernel-returned creator FD,
            // never a reopened pathname. O_RDWR also makes held proofs readable.
            let metadata = file.metadata().map_err(|e| e.to_string())?;
            let initial_links = u64::from(source_leaf.is_some());
            if !metadata.is_file() || metadata.nlink() != initial_links { return Err("New archive creator links changed".into()); }
            let creator = Creator { id: identity(&file)?, initial_links, file, source_leaf };
            #[cfg(test)]
            BEFORE_CREATOR_BIND.with(|h| {
                if let Some(hook) = h.borrow_mut().take() { hook(self, &creator); }
            });
            self.verify_creator_path(&creator)?;
            Ok(creator)
        }
        fn create(&self, leaf: &str) -> Result<Creator, String> {
            self.verify_root()?;
            if let Some((_, held, _)) = &self.named {
                let file = open_at(held, std::ffi::OsStr::new(leaf), libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
                self.bind_creator(file, Some(leaf.into()))
            } else {
                #[cfg(target_os = "linux")]
                {
                    let file = open_at(&self.root, std::ffi::OsStr::new("."), libc::O_TMPFILE | libc::O_RDWR)?;
                    self.bind_creator(file, None)
                }
                #[cfg(not(target_os = "linux"))]
                { Err("Anonymous archive staging unavailable".into()) }
            }
        }
        fn verify_root(&self) -> Result<(), String> {
            if identity(&self.root)? != self.root_id || identity(&dir(&self.root_path, false)?)? != self.root_id {
                return Err("Private archive root changed".into());
            }
            if let Some((path, held, id)) = &self.named {
                let metadata = held.metadata().map_err(|e| e.to_string())?;
                if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0
                    || identity(held)? != *id || identity(&dir(path, false)?)? != *id {
                    return Err("Named archive custody changed; evidence retained".into());
                }
            }
            Ok(())
        }
        fn verify_creator_path(&self, creator: &Creator) -> Result<(), String> {
            self.verify_root()?;
            let metadata = creator.file.metadata().map_err(|e| e.to_string())?;
            if identity(&creator.file)? != creator.id || !metadata.is_file()
                || metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
                return Err("Archive creator descriptor changed".into());
            }
            if let Some(leaf) = &creator.source_leaf {
                let (_, held, _) = self.named.as_ref().ok_or("Named archive source unavailable")?;
                let current = open_at(held, std::ffi::OsStr::new(leaf), libc::O_RDONLY)?;
                if identity(&current)? != creator.id { return Err("Archive leaf differs from creator; evidence retained".into()); }
            }
            Ok(())
        }
        fn capture(&self, mut creator: Creator) -> Result<Captured, String> {
            self.verify_creator_path(&creator)?;
            let expected = proof(&mut creator.file)?;
            if (expected.dev, expected.ino) != creator.id || creator.file.metadata().map_err(|e| e.to_string())?.nlink() != creator.initial_links { return Err("Archive creator identity changed".into()); }
            Ok(Captured { creator, expected })
        }
        fn verify_capture(&self, captured: &mut Captured, linked: bool) -> Result<(), String> {
            self.verify_creator_path(&captured.creator)?;
            if proof(&mut captured.creator.file)? != captured.expected
                || captured.creator.file.metadata().map_err(|e| e.to_string())?.nlink()
                    != captured.creator.initial_links + u64::from(linked) {
                return Err("Captured archive source changed".into());
            }
            Ok(())
        }
        fn disposition(&self) -> &'static str {
            if self.evidence_started { "retained_for_review" }
            else if self.named.is_some() { "retained_named_preinstall" }
            else { "closed_anonymous_preinstall" }
        }
        fn evidence_name(&self, kind: &str) -> String { format!("starlane-zip-evidence-{}-{kind}", self.op) }
        fn publish(&mut self, captured: &mut Captured, kind: &str, force_copy: bool) -> Result<Evidence, String> {
            self.verify_capture(captured, false)?;
            let leaf = self.evidence_name(kind);
            let mut linked = false;
            #[cfg(target_os = "linux")]
            if !force_copy {
                let source = format!("/proc/self/fd/{}", captured.creator.file.as_raw_fd());
                let src = name(std::ffi::OsStr::new(&source))?;
                let dst = name(std::ffi::OsStr::new(&leaf))?;
                let rc = unsafe { libc::linkat(libc::AT_FDCWD, src.as_ptr(), self.root.as_raw_fd(), dst.as_ptr(), libc::AT_SYMLINK_FOLLOW) };
                if rc == 0 { linked = true; self.evidence_started = true; }
                else {
                    let error = std::io::Error::last_os_error();
                    if !matches!(error.raw_os_error(), Some(libc::EPERM | libc::EACCES | libc::ENOENT | libc::EOPNOTSUPP | libc::ENOSYS)) {
                        return Err(error.to_string());
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            let _ = force_copy;
            let mut file = if linked {
                let file = open_at(&self.root, std::ffi::OsStr::new(&leaf), libc::O_RDONLY)?;
                if identity(&file)? != captured.creator.id { return Err("Published archive alias changed; evidence retained".into()); }
                file
            } else {
                let mut creator = open_at(&self.root, std::ffi::OsStr::new(&leaf), libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
                self.evidence_started = true;
                let creator_id = identity(&creator)?;
                #[cfg(test)]
                BEFORE_COPY_CREATOR_BIND.with(|h| { if let Some(hook) = h.borrow_mut().take() { hook(self, &creator, &leaf); } });
                captured.creator.file.rewind().map_err(|e| e.to_string())?;
                std::io::copy(&mut captured.creator.file, &mut creator).map_err(|e| e.to_string())?;
                creator.sync_all().map_err(|e| e.to_string())?;
                if identity(&creator)? != creator_id { return Err("Evidence copy creator changed".into()); }
                creator
            };
            file.sync_all().map_err(|e| e.to_string())?;
            let expected = proof(&mut file)?;
            if expected.len != captured.expected.len || expected.sha != captured.expected.sha {
                return Err("Published archive evidence differs from captured source".into());
            }
            self.verify_capture(captured, linked)?;
            self.root.sync_all().map_err(|e| e.to_string())?;
            Ok(Evidence { leaf, file, expected, linked })
        }
        fn verify_evidence(&self, evidence: &mut Evidence) -> Result<(), String> {
            self.verify_root()?;
            let mut current = open_at(&self.root, std::ffi::OsStr::new(&evidence.leaf), libc::O_RDONLY)?;
            if proof(&mut evidence.file)? != evidence.expected || proof(&mut current)? != evidence.expected {
                return Err("Published archive evidence changed".into());
            }
            let expected_links = if evidence.linked { if self.named.is_some() { 2 } else { 1 } } else { 1 };
            for file in [&evidence.file, &current] {
                let metadata = file.metadata().map_err(|e| e.to_string())?;
                if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o077 != 0 {
                    return Err("Published archive evidence permissions changed".into());
                }
            }
            if evidence.file.metadata().map_err(|e| e.to_string())?.nlink() != expected_links {
                return Err("Published archive evidence links changed".into());
            }
            Ok(())
        }
        fn manifest(&mut self, bytes: &[u8]) -> Result<Evidence, String> {
            self.verify_root()?;
            let leaf = self.evidence_name("manifest.json");
            let mut file = open_at(&self.root, std::ffi::OsStr::new(&leaf), libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
            self.evidence_started = true;
            let id = identity(&file)?;
            file.write_all(bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            let expected = proof(&mut file)?;
            if (expected.dev, expected.ino) != id || expected.len != bytes.len() as u64 || expected.sha != format!("{:x}", Sha256::digest(bytes)) {
                return Err("Archive evidence manifest changed".into());
            }
            self.root.sync_all().map_err(|e| e.to_string())?;
            Ok(Evidence { leaf, file, expected, linked: false })
        }
    }
    fn open_at_io(dir: &fs::File, leaf: &std::ffi::OsStr, flags: i32) -> std::io::Result<fs::File> {
        let leaf = name(leaf).map_err(std::io::Error::other)?;
        let fd = unsafe { libc::openat(dir.as_raw_fd(), leaf.as_ptr(), flags | libc::O_NOFOLLOW | libc::O_CLOEXEC, 0o600) };
        if fd < 0 { return Err(std::io::Error::last_os_error()); }
        Ok(unsafe { fs::File::from_raw_fd(fd) })
    }
    struct Prepared {
        provider: String,
        destination: PathBuf,
        anchor: PathBuf,
        anchor_id: (u64, u64),
        stage: Stage,
        phase: Phase,
        archive: Captured,
        snapshot_json: String,
        snapshot: VaultSnapshot,
        provenance: Option<Value>,
        targets: Vec<Target>,
        created: Instant,
        joint: Option<JointAssets>,
    }
    #[derive(Serialize)]
    pub struct Inspection { pub handle: String, pub snapshot_json: String }
    #[derive(Serialize)]
    pub struct AssetUpdate { pub index: usize, pub id: String, pub size: u64, pub vault_path: String, pub url: String }
    #[derive(Serialize)]
    pub struct Restoration {
        pub handle: String, pub snapshot_json: String, pub asset_updates: Vec<AssetUpdate>, pub stage_disposition: &'static str,
    }
    #[derive(Debug, Serialize)]
    pub struct Cancellation { pub stage_disposition: &'static str }
    // All tests sharing the bounded process registry use this same guard.
    #[cfg(test)]
    pub(crate) static TEST_SERIAL: Mutex<()> = Mutex::new(());
    fn sessions() -> &'static Mutex<HashMap<String, Prepared>> { SESSIONS.get_or_init(|| Mutex::new(HashMap::new())) }
    fn expire(map: &mut HashMap<String, Prepared>) {
        map.retain(|_, s| {
            if s.phase != Phase::Prepared || s.created.elapsed() < SESSION_TTL { return true; }
            // No path deletion in any backend. Anonymous never-published files
            // disappear on last close; named evidence remains for future review.
            if s.stage.named.is_some() { eprintln!("Expired named archive stage retained for review"); }
            false
        });
    }
    pub fn prepare(provider: &str, source: &Path, destination: &Path) -> Result<Inspection, String> {
        prepare_staged(provider, source, destination, false, None)
    }
    fn prepare_staged(provider: &str, source: &Path, destination: &Path, force_named: bool, staging_root: Option<&Path>) -> Result<Inspection, String> {
        let mut map = sessions().lock().map_err(|_| "Archive session lock failed")?;
        expire(&mut map);
        if map.len() >= SESSION_LIMIT { return Err("Finish or cancel pending archive imports before preparing another".into()); }
        let source_parent = source.parent().ok_or("Archive has no parent")?;
        let source_dir = dir(source_parent, false)?;
        let mut source_file = open_at(&source_dir, source.file_name().ok_or("Archive filename required")?, libc::O_RDONLY)?;
        if source_file.metadata().map_err(|e|e.to_string())?.len() > 4096u64 * 100 * 1024 * 1024 + 128 * 1024 * 1024 { return Err("Archive compressed-source budget exceeded".into()); }
        let source_proof = proof(&mut source_file)?;
        let temp = std::env::temp_dir();
        let (stage, mut creator) = Stage::new(staging_root.unwrap_or(&temp), force_named)?;
        #[cfg(test)]
        AFTER_CAPTURE_CREATE.with(|h| -> Result<(), String> { if let Some(hook) = h.borrow_mut().take() { hook(&stage, &mut creator)?; } Ok(()) })?;
        let result = (|| -> Result<_, String> {
            std::io::copy(&mut source_file, &mut creator.file).map_err(|e| e.to_string())?;
            creator.file.sync_all().map_err(|e| e.to_string())?;
            let mut archive = stage.capture(creator)?;
            if archive.expected.sha != source_proof.sha || proof(&mut source_file)? != source_proof { return Err("Selected archive changed during capture".into()); }
            let mut zip = zip::ZipArchive::new(archive.creator.file.try_clone().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let mut snapshot_json = String::new();
            zip.by_name(ARCHIVE_SNAPSHOT_FILE).map_err(|_| "Vault archive is missing vault.json")?.take(16 * 1024 * 1024 + 1).read_to_string(&mut snapshot_json).map_err(|e| e.to_string())?;
            if snapshot_json.len() > 16 * 1024 * 1024 { return Err("Archive snapshot exceeds metadata budget".into()); }
            let provenance = super::archive_provenance::inspect(&mut zip, &snapshot_json)?;
            let snapshot = parse_archive_snapshot(&snapshot_json)?;
            let candidate:Value=serde_json::from_str(&snapshot_json).map_err(|e|e.to_string())?;
            super::mirror_admission::validate_page_hierarchy(&candidate)?;
            let mut anchor = destination.to_path_buf();
            while !anchor.exists() { if !anchor.pop() { return Err("Archive destination anchor unavailable".into()); } }
            let anchor_id = identity(&dir(&anchor, false)?)?;
            let destination_dir = if destination.exists() { Some(dir(destination, false)?) } else { None };
            let mut targets = Vec::new();
            let mut seen = HashSet::new();
            for (index, asset) in snapshot.assets.iter().enumerate() {
                if asset.source != "tauri" || zip.by_name(&archive_asset_path(asset)).is_err() { continue; }
                let leaf = leaf(asset)?;
                if !seen.insert(leaf.clone()) { return Err("Duplicate archive asset destination".into()); }
                let prior = if let Some(ref d) = destination_dir {
                    if fs::symlink_metadata(destination.join(&leaf)).is_ok() { Some(proof(&mut open_at(d, std::ffi::OsStr::new(&leaf), libc::O_RDONLY)?)?) } else { None }
                } else { None };
                targets.push(Target { index, leaf, prior });
            }
            stage.verify_capture(&mut archive, false)?;
            Ok((archive, snapshot_json, snapshot, provenance, anchor, anchor_id, targets))
        })();
        let (archive, snapshot_json, snapshot, provenance, anchor, anchor_id, targets) = result.map_err(|e| format!("{e}; stage disposition: {}", stage.disposition()))?;
        let handle = stage.op.clone();
        let reply = Inspection { handle: handle.clone(), snapshot_json: snapshot_json.clone() };
        map.insert(handle, Prepared { provider: provider.into(), destination: destination.into(), anchor, anchor_id, stage, phase: Phase::Prepared, archive, snapshot_json, snapshot, provenance, targets, created: Instant::now(), joint: None });
        Ok(reply)
    }
    fn verify_stage(s: &mut Prepared, linked_archive: bool) -> Result<(), String> {
        s.stage.verify_capture(&mut s.archive, linked_archive)?;
        if identity(&dir(&s.anchor, false)?)? != s.anchor_id { return Err("Archive destination changed".into()); }
        Ok(())
    }
    pub fn restore(provider: &str, handle: &str, snapshot_json: &str, destination: &Path) -> Result<Restoration, String> {
        restore_backend(provider, handle, snapshot_json, destination, false)
    }
    fn restore_backend(provider: &str, handle: &str, snapshot_json: &str, destination: &Path, force_copy: bool) -> Result<Restoration, String> {
        let mut map = sessions().lock().map_err(|_| "Archive session lock failed")?;
        expire(&mut map);
        let s = map.get_mut(handle).ok_or("Prepared archive unavailable; inspect and validate again")?;
        if s.provider != provider || s.destination != destination || s.snapshot_json != snapshot_json { return Err("Prepared archive binding mismatch".into()); }
        if s.phase != Phase::Prepared { return Err("Archive session already consumed".into()); }
        s.phase = Phase::Extracting;
        let result = restore_prepared(s, handle, destination, force_copy).map_err(|e| format!("{e}; stage disposition: {}", s.stage.disposition()));
        // Never infer snapshot persistence. Terminal descriptor ownership closes
        // without deleting named/evidence paths or accumulating retained FDs.
        map.remove(handle);
        result
    }
    fn restore_prepared(s: &mut Prepared, handle: &str, destination: &Path, force_copy: bool) -> Result<Restoration, String> {
        if s.provenance.is_some() { return Err("Native provenance archive requires genuine joint canonical import".into()); }
        verify_stage(s, false)?;
        let mut zip = zip::ZipArchive::new(s.archive.creator.file.try_clone().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let mut extracted = Vec::new();
        for target in &s.targets {
            let mut entry = zip.by_name(&archive_asset_path(&s.snapshot.assets[target.index])).map_err(|e| e.to_string())?;
            let mut creator = s.stage.create(&format!("asset-{}", target.index))?;
            std::io::copy(&mut entry, &mut creator.file).map_err(|e| e.to_string())?;
            creator.file.sync_all().map_err(|e| e.to_string())?;
            extracted.push((target.index, s.stage.capture(creator)?));
        }
        drop(zip);
        #[cfg(test)]
        BEFORE_INSTALL.with(|h| { if let Some(hook) = h.borrow_mut().take() { hook(s); } });
        verify_stage(s, false)?;
        if s.targets.is_empty() {
            return Ok(Restoration { handle: handle.into(), snapshot_json: s.snapshot_json.clone(), asset_updates: Vec::new(), stage_disposition: s.stage.disposition() });
        }
        s.phase = Phase::Publishing;
        let mut archive_evidence = s.stage.publish(&mut s.archive, "archive", force_copy)?;
        let mut asset_evidence = Vec::new();
        for (index, captured) in &mut extracted {
            #[cfg(test)]
            DURING_EVIDENCE_PUBLICATION.with(|h| -> Result<(), String> { if let Some(hook) = h.borrow_mut().take() { hook(&s.stage, &archive_evidence)?; } Ok(()) })?;
            asset_evidence.push(s.stage.publish(captured, &format!("asset-{index}"), force_copy)?);
        }
        let manifest = serde_json::json!({ "version":1, "phase":"installation_may_start", "op_id":s.stage.op,
            "provider":s.provider, "destination":s.destination.to_string_lossy(),
            "original_json_sha256":format!("{:x}",Sha256::digest(s.snapshot_json.as_bytes())),
            "archive":{ "leaf":archive_evidence.leaf, "proof":archive_evidence.expected },
            "assets":extracted.iter().zip(asset_evidence.iter()).map(|((index,_), e)| serde_json::json!({"index":index,"leaf":e.leaf,"proof":e.expected})).collect::<Vec<_>>() });
        let mut manifest_evidence = s.stage.manifest(&serde_json::to_vec(&manifest).map_err(|e| e.to_string())?)?;
        #[cfg(test)]
        BEFORE_EVIDENCE_FINAL.with(|h| { if let Some(hook) = h.borrow_mut().take() { hook(&s.stage, &archive_evidence, &asset_evidence, &manifest_evidence); } });
        // After ALL alias/copy/manifest I/O: repeat EVERY creator and evidence
        // proof. An early hard link exposes its source inode to namespace writes.
        verify_stage(s, archive_evidence.linked)?;
        s.stage.verify_evidence(&mut archive_evidence)?;
        for ((_, captured), evidence) in extracted.iter_mut().zip(asset_evidence.iter_mut()) {
            s.stage.verify_capture(captured, evidence.linked)?;
            s.stage.verify_evidence(evidence)?;
        }
        s.stage.verify_evidence(&mut manifest_evidence)?;
        s.stage.verify_root()?;
        // No unrelated wait/publication occurs between final full evidence proof
        // and Installing. Arbitrary same-user mutation after that check remains
        // the existing bounded filesystem limitation, not an atomicity claim.
        s.phase = Phase::Installing;
        let destination_dir = dir(destination, true)?;
        for target in &s.targets {
            let current = if fs::symlink_metadata(destination.join(&target.leaf)).is_ok() { Some(proof(&mut open_at(&destination_dir,std::ffi::OsStr::new(&target.leaf),libc::O_RDONLY)?)?) } else { None };
            if current != target.prior { return Err("Archive asset destination changed; import again".into()); }
        }
        let destination_id = identity(&destination_dir)?;
        let mut updates = Vec::new();
        for (index, mut source) in extracted {
            if proof(&mut source.creator.file)? != source.expected { return Err("Staged archive asset changed".into()); }
            let target = s.targets.iter().find(|t| t.index == index).unwrap();
            let flags = if target.prior.is_some() { libc::O_RDWR } else { libc::O_RDWR | libc::O_CREAT | libc::O_EXCL };
            let mut installed = open_at(&destination_dir,std::ffi::OsStr::new(&target.leaf),flags)?;
            if let Some(ref prior) = target.prior { if proof(&mut installed)? != *prior { return Err("Archive asset destination changed".into()); } }
            installed.set_len(0).map_err(|e| e.to_string())?;
            std::io::copy(&mut source.creator.file,&mut installed).map_err(|e| e.to_string())?;
            installed.sync_all().map_err(|e| e.to_string())?;
            if proof(&mut installed)?.sha != source.expected.sha { return Err("Archive asset installation verification failed".into()); }
            if identity(&dir(destination,false)?)? != destination_id || identity(&open_at(&destination_dir,std::ffi::OsStr::new(&target.leaf),libc::O_RDONLY)?)? != identity(&installed)? { return Err("Archive destination changed during installation".into()); }
            #[cfg(test)]
            AFTER_CANONICAL_INSTALL.with(|h| -> Result<(), String> { if let Some(hook) = h.borrow_mut().take() { hook(s)?; } Ok(()) })?;
            let asset = &s.snapshot.assets[index];
            let path = destination.join(&target.leaf);
            updates.push(AssetUpdate { index,id:asset.id.clone(),size:source.expected.len,vault_path:path.to_string_lossy().into_owned(),url:asset_url(&path) });
        }
        destination_dir.sync_all().map_err(|e| e.to_string())?;
        s.phase = Phase::Restored;
        Ok(Restoration { handle:handle.into(), snapshot_json:s.snapshot_json.clone(), asset_updates:updates,stage_disposition:"retained_for_review" })
    }
    #[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
    pub struct ImportVersion {
        pub index: usize,
        pub id: String,
        pub size: u64,
        pub sha256: String,
        pub vault_path: String,
        pub url: String,
    }
    struct JointAssets {
        versions: Vec<ImportVersion>,
        extracted: Vec<Captured>,
        archive_evidence: Evidence,
        asset_evidence: Vec<Evidence>,
        manifest: Evidence,
        digest: String,
    }
    fn json_digest(value: &Value) -> Result<String, String> {
        Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(value).map_err(|e| e.to_string())?)))
    }
    fn verify_joint(s: &mut Prepared, assets: &mut JointAssets) -> Result<(), String> {
        verify_stage(s, assets.archive_evidence.linked)?;
        s.stage.verify_evidence(&mut assets.archive_evidence)?;
        for (captured, evidence) in assets.extracted.iter_mut().zip(assets.asset_evidence.iter_mut()) {
            s.stage.verify_capture(captured, evidence.linked)?;
            s.stage.verify_evidence(evidence)?;
        }
        s.stage.verify_evidence(&mut assets.manifest)?;
        s.stage.verify_root()
    }
    /// Private extraction and immutable version projection; no destination writes.
    pub(crate) fn plan_joint(provider: &str, handle: &str, original: &str, destination: &Path) -> Result<Value, String> {
        let mut map = sessions().lock().map_err(|_| "Archive session lock failed")?;
        expire(&mut map);
        let s = map.get_mut(handle).ok_or("Original archive session is unavailable")?;
        if s.provider != provider || s.destination != destination || s.snapshot_json != original {
            return Err("Original archive plan binding changed".into());
        }
        if s.phase != Phase::Prepared { return Err("Original archive was already planned or consumed".into()); }
        verify_stage(s, false)?;
        s.phase = Phase::Extracting;
        let result = (|| {
            let mut zip = zip::ZipArchive::new(s.archive.creator.file.try_clone().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
            let mut extracted = Vec::new();
            let mut versions = Vec::new();
            for (index, asset) in s.snapshot.assets.iter().enumerate() {
                if asset.source != "tauri" { continue; }
                // A missing native member cannot silently keep a stale archive path.
                let mut entry = zip.by_name(&archive_asset_path(asset)).map_err(|_| "Archive native asset member is missing; retain the original archive for review")?;
                let mut creator = s.stage.create(&format!("joint-asset-{index}"))?;
                std::io::copy(&mut entry, &mut creator.file).map_err(|e| e.to_string())?;
                creator.file.sync_all().map_err(|e| e.to_string())?;
                let captured = s.stage.capture(creator)?;
                let version_leaf = format!("starlane-import-{}-{index}-{}", s.stage.op, leaf(asset)?);
                let path = destination.join(version_leaf);
                versions.push(ImportVersion { index, id: asset.id.clone(), size: captured.expected.len,
                    sha256: captured.expected.sha.clone(), vault_path: path.to_string_lossy().into_owned(), url: asset_url(&path) });
                extracted.push(captured);
            }
            drop(zip);
            let archive_evidence = s.stage.publish(&mut s.archive, "joint-archive", false)?;
            let mut asset_evidence = Vec::new();
            for (index, captured) in extracted.iter_mut().enumerate() {
                asset_evidence.push(s.stage.publish(captured, &format!("joint-evidence-{index}"), false)?);
            }
            let envelope = serde_json::json!({"schema_version":1,"operation_id":handle,"provider":provider,
                "original_snapshot_json":original,"archive_sha256":s.archive.expected.sha,"versions":versions,"imported_provenance":s.provenance});
            let digest = json_digest(&envelope)?;
            let manifest = s.stage.manifest(&serde_json::to_vec(&envelope).map_err(|e| e.to_string())?)?;
            let mut assets = JointAssets { versions, extracted, archive_evidence, asset_evidence, manifest, digest };
            verify_joint(s, &mut assets)?;
            let reply = serde_json::json!({"schema_version":1,"operation_id":handle,"plan_digest":assets.digest,
                "original_snapshot_json":original,"versions":assets.versions,"imported_provenance":s.provenance});
            s.joint = Some(assets);
            s.phase = Phase::Planned;
            Ok(reply)
        })();
        result.map_err(|e: String| format!("{e}; stage disposition: {}", s.stage.disposition()))
    }
    pub(crate) fn verify_joint_plan(provider: &str, handle: &str, digest: &str, destination: &Path, installed: bool) -> Result<(), String> {
        let mut map = sessions().lock().map_err(|_| "Archive session lock failed")?;
        let s = map.get_mut(handle).ok_or("Original archive plan is unavailable")?;
        if s.provider != provider || s.destination != destination || !matches!(s.phase, Phase::Planned | Phase::Committing | Phase::Restored) {
            return Err("Original archive plan binding changed".into());
        }
        let mut assets = s.joint.take().ok_or("Archive plan evidence is missing")?;
        let result = (|| {
            if assets.digest != digest { return Err("Archive plan digest changed".into()); }
            verify_joint(s, &mut assets)?;
            if installed {
                if s.phase != Phase::Restored { return Err("Archive versions were not fully installed".into()); }
                let held = dir(destination, false)?;
                for version in &assets.versions {
                    let leaf = Path::new(&version.vault_path).file_name().ok_or("Version leaf is missing")?;
                    let value = proof(&mut open_at(&held, leaf, libc::O_RDONLY)?)?;
                    if value.sha != version.sha256 || value.len != version.size { return Err("Installed archive version changed".into()); }
                }
            }
            Ok(())
        })();
        s.joint = Some(assets);
        result
    }
    #[cfg(target_os="linux")]
    pub(crate) fn prepare_joint_owner(provider:&str,handle:&str,digest:&str,destination:&Path,selection:SnapshotRequestBinding,binding:Value,fingerprint:&str,revision:&str,candidate:&Value)->Result<crate::local_database::attachment_download_owner::native_archive_import::Installation,String>{
        super::mirror_admission::validate_page_hierarchy(candidate)?;
        let mut map=sessions().lock().map_err(|_|"Archive session lock failed")?;let s=map.get_mut(handle).ok_or("Original archive stage unavailable")?;
        if s.provider!=provider||s.destination!=destination||s.phase!=Phase::Committing||s.provenance.is_none(){return Err("Original provenance import owner plan unavailable".into())}
        let mut assets=s.joint.take().ok_or("Original archive joint evidence missing")?;
        let result=(||{if assets.digest!=digest{return Err("Original joint plan changed".into())}verify_joint(s,&mut assets)?;
            let mut owner=crate::local_database::attachment_download_owner::native_archive_import::Installation::begin(selection,binding,handle,fingerprint,revision)?;
            for (version,captured) in assets.versions.iter().zip(assets.extracted.iter_mut()){
                let asset=candidate["assets"].get(version.index).ok_or("Original imported candidate asset missing")?.clone();
                if asset["vault_path"]!=version.vault_path||asset["url"]!=version.url{return Err("Original fresh imported asset path differs".into())}
                owner.add(&serde_json::to_value(version).map_err(|e|e.to_string())?,asset,&mut captured.creator.file)?;
            }
            verify_joint(s,&mut assets)?;Ok(owner)
        })();s.joint=Some(assets);result
    }
    /// After the original DB intent commits: install only fresh exclusive leaves.
    pub(crate) fn install_joint(provider: &str, handle: &str, digest: &str, destination: &Path) -> Result<(), String> {
        install_joint_with(provider,handle,digest,destination,|_|Ok(false))
    }
    pub(crate) fn install_joint_with(provider: &str, handle: &str, digest: &str, destination: &Path,mut publisher:impl FnMut(usize)->Result<bool,String>) -> Result<(), String> {
        let mut map = sessions().lock().map_err(|_| "Archive session lock failed")?;
        let s = map.get_mut(handle).ok_or("Original archive plan is unavailable")?;
        if s.provider != provider || s.destination != destination || s.phase != Phase::Committing { return Err("Original archive plan binding changed".into()); }
        let mut assets = s.joint.take().ok_or("Archive plan evidence is missing")?;
        let result = (|| {
            if assets.digest != digest { return Err("Archive plan digest changed".into()); }
            verify_joint(s, &mut assets)?;
            s.phase = Phase::Installing;
            let held = dir(destination, true)?;
            let held_id = identity(&held)?;
            for (version, captured) in assets.versions.iter().zip(assets.extracted.iter_mut()) {
                let version_leaf = Path::new(&version.vault_path).file_name().ok_or("Version leaf is missing")?;
                if publisher(version.index)? {
                    let mut installed=open_at(&held,version_leaf,libc::O_RDONLY)?;let bytes=proof(&mut installed)?;
                    if bytes.sha!=version.sha256||bytes.len!=version.size{return Err("Owned imported version bytes differ".into())}
                    #[cfg(test)]
                    AFTER_CANONICAL_INSTALL.with(|h| -> Result<(), String> { if let Some(hook) = h.borrow_mut().take() { hook(s)?; } Ok(()) })?;
                    continue;
                }
                let mut output = open_at(&held, version_leaf, libc::O_RDWR | libc::O_CREAT | libc::O_EXCL)?;
                captured.creator.file.rewind().map_err(|e| e.to_string())?;
                std::io::copy(&mut captured.creator.file, &mut output).map_err(|e| e.to_string())?;
                output.sync_all().map_err(|e| e.to_string())?;
                let bytes = proof(&mut output)?;
                if bytes.sha != version.sha256 || bytes.len != version.size
                    || identity(&open_at(&held, version_leaf, libc::O_RDONLY)?)? != identity(&output)? {
                    return Err("Exclusive imported asset verification failed".into());
                }
                #[cfg(test)]
                AFTER_CANONICAL_INSTALL.with(|h| -> Result<(), String> { if let Some(hook) = h.borrow_mut().take() { hook(s)?; } Ok(()) })?;
            }
            held.sync_all().map_err(|e| e.to_string())?;
            if identity(&dir(destination, false)?)? != held_id { return Err("Original archive asset directory changed".into()); }
            s.phase = Phase::Restored;
            Ok(())
        })();
        s.joint = Some(assets);
        result
    }

    // Reserve the existing stage across SQL awaits. No blocking session mutex is
    // held by the asynchronous caller, and cancel cannot release its creators.
    pub fn reserve_joint(provider: &str, handle: &str, digest: &str, destination: &Path) -> Result<(), String> {
        let mut map = sessions().lock().map_err(|_| "Archive session lock failed")?;
        let s = map.get_mut(handle).ok_or("Original archive stage unavailable")?;
        if s.provider != provider || s.destination != destination || s.phase != Phase::Planned || s.joint.as_ref().map(|a| a.digest.as_str()) != Some(digest) {
            return Err("Archive import is already dispatched or its original plan changed".into());
        }
        s.phase = Phase::Committing;
        Ok(())
    }
    pub fn joint_available(handle: &str) -> bool {
        sessions().lock().map(|map|map.get(handle).is_some_and(|s|s.joint.is_some())).unwrap_or(false)
    }
    pub fn release_joint(handle: &str) {
        // Terminal failure/success retains published stage evidence; releasing
        // descriptors does not unlink any canonical or private named files.
        if let Ok(mut map) = sessions().lock() { map.remove(handle); }
    }

    pub fn cancel(provider: &str, handle: &str) -> Result<Cancellation, String> {
        let mut map = sessions().lock().map_err(|_| "Archive session lock failed")?;
        if let Some(s) = map.get(handle) { if s.provider != provider { return Err("Prepared archive binding mismatch".into()); }
            if matches!(s.phase, Phase::Committing | Phase::Installing) { return Err("Archive import is dispatched; inspect its original database evidence".into()); }
        }
        let disposition = map.get(handle).map(|s| s.stage.disposition()).unwrap_or("unavailable");
        map.remove(handle);
        Ok(Cancellation { stage_disposition: disposition })
    }
    #[cfg(test)]
    thread_local! {
        static BEFORE_NAMED_DIRECTORY_OPEN: std::cell::RefCell<Option<Box<dyn FnOnce(&Path)>>> = std::cell::RefCell::new(None);
        static BEFORE_COPY_CREATOR_BIND: std::cell::RefCell<Option<Box<dyn FnOnce(&Stage,&fs::File,&str)>>> = std::cell::RefCell::new(None);
        static BEFORE_CREATOR_BIND: std::cell::RefCell<Option<Box<dyn FnOnce(&Stage, &Creator)>>> = std::cell::RefCell::new(None);
        static AFTER_CAPTURE_CREATE: std::cell::RefCell<Option<Box<dyn FnOnce(&Stage, &mut Creator) -> Result<(),String>>>> = std::cell::RefCell::new(None);
        static BEFORE_INSTALL: std::cell::RefCell<Option<Box<dyn FnOnce(&Prepared)>>> = std::cell::RefCell::new(None);
        static DURING_EVIDENCE_PUBLICATION: std::cell::RefCell<Option<Box<dyn FnOnce(&Stage,&Evidence) -> Result<(),String>>>> = std::cell::RefCell::new(None);
        static BEFORE_EVIDENCE_FINAL: std::cell::RefCell<Option<Box<dyn FnOnce(&Stage,&Evidence,&[Evidence],&Evidence)>>> = std::cell::RefCell::new(None);
        static AFTER_CANONICAL_INSTALL: std::cell::RefCell<Option<Box<dyn FnOnce(&Prepared) -> Result<(),String>>>> = std::cell::RefCell::new(None);
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        use zip::write::SimpleFileOptions;
        fn make_zip(path: &Path, json: &str, bytes: &[u8]) {
            let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("vault.json", options).unwrap();
            zip.write_all(json.as_bytes()).unwrap();
            zip.start_file("assets/present-file.bin", options).unwrap();
            zip.write_all(bytes).unwrap();
            zip.finish().unwrap().sync_all().unwrap();
        }
        fn root() -> PathBuf {
            let p = fs::canonicalize(std::env::temp_dir())
                .unwrap()
                .join(format!(
                    "starlane-zip-admission-test-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir(&p).unwrap();
            p
        }
        fn json() -> String {
            let asset = |id: &str, source: &str| serde_json::json!({"id":id,"name":"file.bin","mime_type":null,"size":99,"url":"old-url","vault_path":"old-path","source":source,"created_at":"raw","future":{"keep":id}});
            serde_json::json!({"schema_version":1,"spaces":[],"folders":[],"documents":[],"assets":[asset("browser","browser"),asset("missing","tauri"),asset("present","tauri")],"property_schemas":[{"id":"raw:p","name":" Raw ","property_type":"select","options":{"options":[" one "]}}],"future":{"unknown":[1,{"raw":true}]}}).to_string()
        }
        #[test]
        fn joint_versions_keep_old_assets_and_original_json_with_zero_and_mixed_assets() {
            let _test_guard=TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
            for zero in [false,true] {
                let root=root();let source=root.join("joint.zip");let dest=root.join("assets");fs::create_dir(&dest).unwrap();fs::write(dest.join("present-file.bin"),b"old asset bytes").unwrap();
                let mut raw:Value=serde_json::from_str(&json()).unwrap();
                raw["assets"].as_array_mut().unwrap().remove(1);
                if zero {raw["assets"]=serde_json::json!([]);}
                let original=raw.to_string();make_zip(&source,&original,b"captured version");
                let prepared=prepare("none",&source,&dest).unwrap();
                let planned=plan_joint("none",&prepared.handle,&original,&dest).unwrap();
                assert_eq!(planned["original_snapshot_json"],original);assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(),b"old asset bytes");
                let hash=planned["plan_digest"].as_str().unwrap();reserve_joint("none",&prepared.handle,hash,&dest).unwrap();
                assert!(cancel("none",&prepared.handle).is_err());
                install_joint("none",&prepared.handle,hash,&dest).unwrap();verify_joint_plan("none",&prepared.handle,hash,&dest,true).unwrap();
                assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(),b"old asset bytes");
                assert_eq!(planned["versions"].as_array().unwrap().len(),if zero {0}else{1});
                if !zero {let version=&planned["versions"][0];assert_eq!(version["index"],1);assert_eq!(version["id"],"present");let path=PathBuf::from(version["vault_path"].as_str().unwrap());assert_eq!(fs::read(&path).unwrap(),b"captured version");assert_ne!(path,dest.join("present-file.bin"));}
                assert!(install_joint("none",&prepared.handle,hash,&dest).is_err());release_joint(&prepared.handle);fs::remove_dir_all(root).unwrap();
            }
        }
        #[test]
        fn browser_missing_native_path_is_raw_exact_and_tauri_invalid_paths_hold_before_install() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let root = root();
            let source = root.join("mixed.zip");
            let dest = root.join("assets");
            fs::create_dir(&dest).unwrap();
            fs::write(dest.join("present-file.bin"), b"original destination bytes").unwrap();
            let mut raw: Value = serde_json::from_str(&json()).unwrap();
            raw["assets"] = serde_json::json!([raw["assets"][0].clone(), raw["assets"][2].clone()]);
            raw["assets"][0].as_object_mut().unwrap().remove("vault_path");
            let original = raw.to_string();
            make_zip(&source, &original, b"new bytes");
            let prepared = prepare("none", &source, &dest).unwrap();
            assert_eq!(prepared.snapshot_json, original);
            let planned = plan_joint("none", &prepared.handle, &original, &dest).unwrap();
            let versions = planned["versions"].as_array().unwrap();
            assert_eq!(versions.len(), 1);
            assert_eq!(versions[0]["index"], 1);
            assert_eq!(versions[0]["id"], "present");
            assert!(serde_json::from_str::<Value>(&prepared.snapshot_json).unwrap()["assets"][0].get("vault_path").is_none());
            cancel("none", &prepared.handle).unwrap();
            for invalid in [None, Some(Value::Null), Some(Value::String(String::new())), Some(serde_json::json!(42))] {
                let mut rejected = raw.clone();
                let asset = rejected["assets"][1].as_object_mut().unwrap();
                if let Some(value) = invalid { asset.insert("vault_path".into(), value); }
                else { asset.remove("vault_path"); }
                let original = rejected.to_string();
                make_zip(&source, &original, b"new bytes");
                let source_before = fs::read(&source).unwrap();
                let sessions_before = sessions().lock().unwrap().len();
                assert!(prepare("none", &source, &dest).err().unwrap().contains("Native archive asset path"));
                assert_eq!(sessions().lock().unwrap().len(), sessions_before);
                assert_eq!(fs::read(&source).unwrap(), source_before);
                assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"original destination bytes");
                assert_eq!(fs::read_dir(&dest).unwrap().count(), 1);
            }
            assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"original destination bytes");
            fs::remove_dir_all(root).unwrap();
        }
        #[test]
        fn joint_missing_zip_member_holds_before_install_and_retains_original_bytes() {
            let _test_guard=TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());let root=root();let source=root.join("joint.zip");let dest=root.join("assets");fs::create_dir(&dest).unwrap();fs::write(dest.join("present-file.bin"),b"old bytes").unwrap();let original=json();make_zip(&source,&original,b"new bytes");
            let prepared=prepare("none",&source,&dest).unwrap();assert!(plan_joint("none",&prepared.handle,&original,&dest).is_err());assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(),b"old bytes");assert_eq!(fs::read_dir(&dest).unwrap().count(),1);cancel("none",&prepared.handle).unwrap();fs::remove_dir_all(root).unwrap();
        }
        #[test]
        fn captured_zip_restores_exact_original_metadata_and_bytes_after_original_replacement() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let root = root();
            let source = root.join("source.zip");
            let dest = root.join("assets");
            fs::create_dir(&dest).unwrap();
            fs::write(dest.join("present-file.bin"), b"old").unwrap();
            let original = json();
            make_zip(&source, &original, b"captured");
            let p = prepare("none", &source, &dest).unwrap();
            assert_eq!(p.snapshot_json, original);
            assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"old");
            make_zip(
                &source,
                &json().replace(" Raw ", "replaced"),
                b"replacement",
            );
            let r = restore("none", &p.handle, &p.snapshot_json, &dest).unwrap();
            assert_eq!(r.snapshot_json, original);
            assert_eq!(r.asset_updates.len(), 1);
            assert_eq!(r.asset_updates[0].index, 2);
            assert_eq!(r.asset_updates[0].id, "present");
            assert_eq!(
                fs::read(dest.join("present-file.bin")).unwrap(),
                b"captured"
            );
            assert!(!dest.join("missing-file.bin").exists());
            assert!(restore("none", &p.handle, &p.snapshot_json, &dest).is_err());
            fs::remove_dir_all(root).unwrap();
        }
        #[test]
        fn deflated_export_format_streams_captured_large_entry_exactly() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let root = root();
            let source = root.join("deflated.zip");
            let dest = root.join("assets");
            let original = json();
            let bytes = b"deflated captured bytes\n".repeat(8192);
            let mut writer = zip::ZipWriter::new(fs::File::create(&source).unwrap());
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            writer.start_file(ARCHIVE_SNAPSHOT_FILE, options).unwrap();
            writer.write_all(original.as_bytes()).unwrap();
            writer
                .start_file("assets/present-file.bin", options)
                .unwrap();
            writer.write_all(&bytes).unwrap();
            writer.finish().unwrap().sync_all().unwrap();
            assert!(fs::metadata(&source).unwrap().len() < bytes.len() as u64);
            let p = prepare("none", &source, &dest).unwrap();
            assert!(!dest.exists());
            let r = restore("none", &p.handle, &p.snapshot_json, &dest).unwrap();
            assert_eq!(r.snapshot_json, original);
            assert_eq!(r.asset_updates.len(), 1);
            assert_eq!(r.asset_updates[0].index, 2);
            assert_eq!(r.asset_updates[0].size, bytes.len() as u64);
            assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), bytes);
            fs::remove_dir_all(root).unwrap();
        }
        #[test]
        fn wrong_provider_json_destination_cancel_and_expiry_never_install() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let root = root();
            let source = root.join("source.zip");
            let dest = root.join("assets");
            fs::create_dir(&dest).unwrap();
            fs::write(dest.join("present-file.bin"), b"old").unwrap();
            make_zip(&source, &json(), b"new");
            for mode in ["provider", "json", "destination", "cancel", "expired"] {
                let p = prepare("none", &source, &dest).unwrap();
                if mode == "cancel" {
                    cancel("none", &p.handle).unwrap();
                }
                if mode == "expired" {
                    sessions()
                        .lock()
                        .unwrap()
                        .get_mut(&p.handle)
                        .unwrap()
                        .created = Instant::now() - Duration::from_secs(601);
                }
                let provider = if mode == "provider" { "other" } else { "none" };
                let text = if mode == "json" {
                    p.snapshot_json.clone() + " "
                } else {
                    p.snapshot_json.clone()
                };
                let destination = if mode == "destination" {
                    root.join("other")
                } else {
                    dest.clone()
                };
                assert!(
                    restore(provider, &p.handle, &text, &destination).is_err(),
                    "{mode}"
                );
                assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"old");
                assert!(!root.join("other").exists());
                cancel("none", &p.handle).unwrap();
            }
            fs::remove_dir_all(root).unwrap();
        }
        #[test]
        fn source_stage_tamper_after_private_extraction_rejects_before_canonical_install() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let root = root();
            let source = root.join("source.zip");
            let dest = root.join("assets");
            fs::create_dir(&dest).unwrap();
            fs::write(dest.join("present-file.bin"), b"old").unwrap();
            make_zip(&source, &json(), b"new");
            let p = prepare("none", &source, &dest).unwrap();
            BEFORE_INSTALL.with(|h| {
                *h.borrow_mut() = Some(Box::new(|s| {
                    let mut file = s.archive.creator.file.try_clone().unwrap();
                    file.rewind().unwrap();
                    file.write_all(b"changed").unwrap();
                    file.sync_all().unwrap();
                }))
            });
            assert!(restore("none", &p.handle, &p.snapshot_json, &dest).is_err());
            assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"old");
            fs::remove_dir_all(root).unwrap();
        }
        #[test]
        fn changed_existing_asset_and_symlink_destination_fail_before_overwrite() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            use std::os::unix::fs::symlink;
            let root = root();
            let source = root.join("source.zip");
            let dest = root.join("assets");
            fs::create_dir(&dest).unwrap();
            let target = dest.join("present-file.bin");
            fs::write(&target, b"old").unwrap();
            make_zip(&source, &json(), b"new");
            let p = prepare("none", &source, &dest).unwrap();
            fs::write(&target, b"changed after prepare").unwrap();
            assert!(restore("none", &p.handle, &p.snapshot_json, &dest).is_err());
            assert_eq!(fs::read(&target).unwrap(), b"changed after prepare");
            fs::remove_file(&target).unwrap();
            let outside = root.join("outside");
            fs::write(&outside, b"outside").unwrap();
            symlink(&outside, &target).unwrap();
            assert!(prepare("none", &source, &dest).is_err());
            assert_eq!(fs::read(&outside).unwrap(), b"outside");
            fs::remove_dir_all(root).unwrap();
        }
        fn fixture() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
            let base = root();
            let staging = base.join("staging");
            fs::create_dir(&staging).unwrap();
            let source = base.join("source.zip");
            let dest = base.join("assets");
            make_zip(&source, &json(), b"captured");
            (base, staging, source, dest)
        }
        fn inspect_at(staging: &Path, source: &Path, dest: &Path, named: bool) -> Inspection {
            prepare_staged("none", source, dest, named, Some(staging)).unwrap()
        }
        #[cfg(target_os = "linux")]
        #[test]
        fn anonymous_cancel_expiry_and_capture_errors_close_without_any_path_deletion() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let (base, staging, source, dest) = fixture();
            fs::create_dir(&dest).unwrap();
            fs::write(dest.join("present-file.bin"), b"original canonical").unwrap();
            let original_zip = fs::read(&source).unwrap();
            for expired in [false, true] {
                let p = inspect_at(&staging, &source, &dest, false);
                {
                    let mut map = sessions().lock().unwrap();
                    let s = map.get_mut(&p.handle).unwrap();
                    assert!(s.stage.named.is_none(), "actual O_TMPFILE support is a required fixture gate");
                    assert_eq!(s.archive.creator.file.metadata().unwrap().nlink(), 0);
                    if expired { s.created = Instant::now() - Duration::from_secs(601); expire(&mut map); }
                }
                if !expired {
                    assert!(cancel("other", &p.handle).is_err());
                    assert_eq!(cancel("none", &p.handle).unwrap().stage_disposition, "closed_anonymous_preinstall");
                }
                assert!(!sessions().lock().unwrap().contains_key(&p.handle));
                assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
                assert_eq!(fs::read(&source).unwrap(), original_zip);
                assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"original canonical");
            }
            AFTER_CAPTURE_CREATE.with(|h| *h.borrow_mut() = Some(Box::new(|stage, creator| {
                assert!(stage.named.is_none());
                creator.file.write_all(b"partial private capture").unwrap();
                Err("injected private capture failure".into())
            })));
            assert!(prepare_staged("none", &source, &dest, false, Some(&staging)).is_err());
            make_zip(&source, "malformed JSON", b"not installed");
            assert!(prepare_staged("none", &source, &dest, false, Some(&staging)).is_err());
            assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
            assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"original canonical");
            fs::remove_dir_all(base).unwrap();
        }
        #[test]
        fn named_fallback_binds_archive_and_extracted_creator_fds_not_replacement_names() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let (base, staging, source, dest) = fixture();
            let zip_before = fs::read(&source).unwrap();
            for archive in [true, false] {
                let replaced_path = std::sync::Arc::new(Mutex::new(None));
                let moved = base.join(if archive { "owned-archive" } else { "owned-extracted" });
                let callback_path = replaced_path.clone();
                let moved_path = moved.clone();
                let hook = Box::new(move |stage: &Stage, creator: &Creator| {
                    let named = stage.named.as_ref().unwrap();
                    let leaf = creator.source_leaf.as_ref().unwrap();
                    assert_eq!(identity(&creator.file).unwrap(), creator.id);
                    let path = named.0.join(leaf);
                    fs::rename(&path, &moved_path).unwrap();
                    fs::write(&path, b"unowned replacement").unwrap();
                    *callback_path.lock().unwrap() = Some(path);
                });
                if archive {
                    BEFORE_CREATOR_BIND.with(|h| *h.borrow_mut() = Some(hook));
                    assert!(prepare_staged("none", &source, &dest, true, Some(&staging)).is_err());
                } else {
                    let p = inspect_at(&staging, &source, &dest, true);
                    BEFORE_CREATOR_BIND.with(|h| *h.borrow_mut() = Some(hook));
                    assert!(restore("none", &p.handle, &p.snapshot_json, &dest).is_err());
                    assert!(!sessions().lock().unwrap().contains_key(&p.handle));
                }
                assert_eq!(fs::read(replaced_path.lock().unwrap().as_ref().unwrap()).unwrap(), b"unowned replacement");
                assert!(moved.exists());
                assert!(!dest.exists());
                assert_eq!(fs::read(&source).unwrap(), zip_before);
            }
            fs::remove_dir_all(base).unwrap();
        }
        #[test]
        fn named_directory_origin_replacement_is_retained_without_cleanup_or_owner_claim() {
            use std::os::unix::fs::PermissionsExt;
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let (base, staging, source, dest) = fixture();
            let moved = base.join("original-created-directory");
            let moved_hook = moved.clone();
            BEFORE_NAMED_DIRECTORY_OPEN.with(|h| *h.borrow_mut() = Some(Box::new(move |path| {
                fs::rename(path, &moved_hook).unwrap();
                fs::create_dir(path).unwrap();
                fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
                fs::write(path.join("unowned-marker"), b"do not delete").unwrap();
            })));
            let p = inspect_at(&staging, &source, &dest, true);
            let stage_path = sessions().lock().unwrap().get(&p.handle).unwrap().stage.named.as_ref().unwrap().0.clone();
            assert_eq!(cancel("none", &p.handle).unwrap().stage_disposition, "retained_named_preinstall");
            assert_eq!(fs::read(stage_path.join("unowned-marker")).unwrap(), b"do not delete");
            assert!(stage_path.join("archive.zip").exists());
            assert!(moved.exists());
            assert!(!dest.exists());
            fs::remove_dir_all(base).unwrap();
        }
        #[test]
        fn streamed_copy_fallback_preserves_raw_metadata_and_rejects_creator_name_substitution() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let (base, staging, source, dest) = fixture();
            let p = inspect_at(&staging, &source, &dest, true);
            let r = restore_backend("none", &p.handle, &p.snapshot_json, &dest, true).unwrap();
            assert_eq!(r.snapshot_json, json());
            assert_eq!(r.stage_disposition, "retained_for_review");
            assert_eq!(r.asset_updates[0].index, 2);
            assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"captured");
            assert!(!sessions().lock().unwrap().contains_key(&p.handle));
            assert_eq!(cancel("none", &p.handle).unwrap().stage_disposition, "unavailable");
            let second = inspect_at(&staging, &source, &dest, true);
            let unowned = std::sync::Arc::new(Mutex::new(None));
            let capture = unowned.clone();
            let moved = base.join("held-copy-origin");
            BEFORE_COPY_CREATOR_BIND.with(|h| *h.borrow_mut() = Some(Box::new(move |stage, creator, leaf| {
                assert_eq!(creator.metadata().unwrap().nlink(), 1);
                let path = stage.root_path.join(leaf);
                fs::rename(&path, &moved).unwrap();
                fs::write(&path, b"unowned copied-name replacement").unwrap();
                *capture.lock().unwrap() = Some(path);
            })));
            assert!(restore_backend("none", &second.handle, &second.snapshot_json, &dest, true).is_err());
            assert_eq!(fs::read(unowned.lock().unwrap().as_ref().unwrap()).unwrap(), b"unowned copied-name replacement");
            assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"captured");
            fs::remove_dir_all(base).unwrap();
        }
        #[cfg(target_os = "linux")]
        #[test]
        fn partial_publication_collision_and_later_alias_write_never_reach_installation() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let (base, staging, source, dest) = fixture();
            fs::create_dir(&dest).unwrap();
            fs::write(dest.join("present-file.bin"), b"old canonical").unwrap();
            for mode in ["collision", "partial", "early-alias-tamper"] {
                let p = inspect_at(&staging, &source, &dest, false);
                let (archive, asset, manifest) = {
                    let map = sessions().lock().unwrap(); let stage = &map.get(&p.handle).unwrap().stage;
                    assert!(stage.named.is_none());
                    (staging.join(stage.evidence_name("archive")),staging.join(stage.evidence_name("asset-2")),staging.join(stage.evidence_name("manifest.json")))
                };
                if mode == "collision" { fs::write(&asset, b"existing unowned collision").unwrap(); }
                if mode == "partial" {
                    DURING_EVIDENCE_PUBLICATION.with(|h| *h.borrow_mut() = Some(Box::new(|_,_| Err("injected later publication failure".into()))));
                }
                if mode == "early-alias-tamper" {
                    DURING_EVIDENCE_PUBLICATION.with(|h| *h.borrow_mut() = Some(Box::new(|stage,first| {
                        fs::write(stage.root_path.join(&first.leaf), b"early alias changed during later publication").unwrap(); Ok(())
                    })));
                }
                assert!(restore_backend("none", &p.handle, &p.snapshot_json, &dest, false).is_err(), "{mode}");
                assert!(!sessions().lock().unwrap().contains_key(&p.handle));
                assert!(archive.exists(), "published evidence retained: {mode}");
                if mode == "collision" { assert_eq!(fs::read(&asset).unwrap(), b"existing unowned collision"); }
                if mode == "partial" { assert!(!asset.exists()); }
                if mode == "early-alias-tamper" { assert!(manifest.exists(), "final full pass must reject after manifest publication"); }
                assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(), b"old canonical");
            }
            fs::remove_dir_all(base).unwrap();
        }
        #[cfg(target_os = "linux")]
        #[test]
        fn final_all_evidence_and_root_proof_rejects_tamper_before_canonical_creation() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            for mode in ["archive", "asset", "manifest", "same-bytes-inode", "root", "extra-link"] {
                let (base, staging, source, dest) = fixture();
                let p = inspect_at(&staging, &source, &dest, false);
                let mode_owned = mode.to_string(); let moved_root = base.join("held-root"); let moved_leaf = base.join("original-leaf");
                let mode_root = staging.clone();
                BEFORE_EVIDENCE_FINAL.with(|h| *h.borrow_mut() = Some(Box::new(move |stage, archive, assets, manifest| {
                    let chosen = if mode_owned == "archive" { &archive.leaf } else if mode_owned == "manifest" { &manifest.leaf } else { &assets[0].leaf };
                    let path = stage.root_path.join(chosen);
                    match mode_owned.as_str() {
                        "root" => { fs::rename(&mode_root, &moved_root).unwrap(); fs::create_dir(&mode_root).unwrap(); fs::write(mode_root.join("unowned"), b"replacement root").unwrap(); },
                        "same-bytes-inode" => { let bytes=fs::read(&path).unwrap();fs::rename(&path,&moved_leaf).unwrap();fs::write(&path,bytes).unwrap(); },
                        "extra-link" => fs::hard_link(&path,&moved_leaf).unwrap(),
                        _ => fs::write(&path,b"tampered after all publication").unwrap(),
                    }
                })));
                assert!(restore_backend("none", &p.handle, &p.snapshot_json, &dest, false).is_err(), "{mode}");
                assert!(!dest.exists(), "canonical creation forbidden: {mode}");
                assert!(!sessions().lock().unwrap().contains_key(&p.handle));
                if mode=="root" { assert_eq!(fs::read(staging.join("unowned")).unwrap(),b"replacement root"); }
                fs::remove_dir_all(base).unwrap();
            }
        }
        #[cfg(target_os = "linux")]
        #[test]
        fn deflated_large_anonymous_restore_and_lost_ack_retain_full_evidence_without_fd_registry() {
            let _test_guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
            let (base, staging, source, dest) = fixture();
            let bytes = b"deflated captured bytes\n".repeat(8192);
            let original=json();
            let mut zip=zip::ZipWriter::new(fs::File::create(&source).unwrap());
            let options=SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("vault.json",options).unwrap();zip.write_all(original.as_bytes()).unwrap();
            zip.start_file("assets/present-file.bin",options).unwrap();zip.write_all(&bytes).unwrap();zip.finish().unwrap().sync_all().unwrap();
            assert_eq!(bytes.len(),196608);
            for lost in [false,true] {
                let p=inspect_at(&staging,&source,&dest,false);
                let archive_path={let map=sessions().lock().unwrap();let stage=&map.get(&p.handle).unwrap().stage;assert!(stage.named.is_none());staging.join(stage.evidence_name("archive"))};
                if lost { AFTER_CANONICAL_INSTALL.with(|h| *h.borrow_mut()=Some(Box::new(|_|Err("injected lost install acknowledgement".into())))); }
                let result=restore("none",&p.handle,&p.snapshot_json,&dest);
                if lost { assert!(result.err().unwrap().contains("retained_for_review")); }
                else { let r=result.unwrap();assert_eq!(r.snapshot_json,original);assert_eq!(r.asset_updates[0].index,2);assert_eq!(r.asset_updates[0].size,196608);assert_eq!(r.stage_disposition,"retained_for_review"); }
                assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(),bytes);
                assert!(archive_path.exists());
                assert!(!sessions().lock().unwrap().contains_key(&p.handle));
                assert_eq!(cancel("none",&p.handle).unwrap().stage_disposition,"unavailable");
                assert!(archive_path.exists());
            }
            fs::remove_dir_all(base).unwrap();
        }
        #[cfg(target_os = "linux")]
        #[test]
        fn active_restore_serializes_cancel_and_retains_installed_evidence() {
            let _test_guard=TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
            let (base,staging,source,dest)=fixture();let p=inspect_at(&staging,&source,&dest,false);
            let handle=p.handle.clone();let worker=std::sync::Arc::new(Mutex::new(None));let capture=worker.clone();
            BEFORE_INSTALL.with(|h| *h.borrow_mut()=Some(Box::new(move |_| {
                let (start_tx,start_rx)=std::sync::mpsc::channel();let(done_tx,done_rx)=std::sync::mpsc::channel();
                let thread=std::thread::spawn(move ||{start_tx.send(()).unwrap();done_tx.send(cancel("none",&handle).unwrap().stage_disposition).unwrap();});
                start_rx.recv().unwrap();assert!(done_rx.recv_timeout(Duration::from_millis(20)).is_err());*capture.lock().unwrap()=Some((thread,done_rx));
            })));
            assert_eq!(restore("none",&p.handle,&p.snapshot_json,&dest).unwrap().stage_disposition,"retained_for_review");
            let(thread,rx)=worker.lock().unwrap().take().unwrap();thread.join().unwrap();assert_eq!(rx.recv().unwrap(),"unavailable");
            assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(),b"captured");
            assert!(fs::read_dir(&staging).unwrap().count()>=3);
            fs::remove_dir_all(base).unwrap();
        }
        #[cfg(target_os = "linux")]
        #[test]
        #[ignore = "controlled subprocess entrypoint invoked by anonymous_crash_phases_preserve_only_published_evidence"]
        fn anonymous_crash_child() {
            let base=PathBuf::from(std::env::var("STARLANE_ZIP_STAGE_CRASH_FIXTURE").unwrap());
            let phase=std::env::var("STARLANE_ZIP_STAGE_CRASH_PHASE").unwrap();
            let p=inspect_at(&base.join("staging"),&base.join("source.zip"),&base.join("assets"),false);
            {let map=sessions().lock().unwrap();let s=map.get(&p.handle).unwrap();assert!(s.stage.named.is_none());assert_eq!(s.archive.creator.file.metadata().unwrap().nlink(),0);}
            fs::write(base.join("handle"),&p.handle).unwrap();
            if phase=="prepublication" { std::process::exit(77); }
            if phase=="published" { BEFORE_EVIDENCE_FINAL.with(|h| *h.borrow_mut()=Some(Box::new(|_,_,_,_|std::process::exit(78)))); }
            if phase=="installed" { AFTER_CANONICAL_INSTALL.with(|h| *h.borrow_mut()=Some(Box::new(|_|std::process::exit(79)))); }
            let _=restore("none",&p.handle,&p.snapshot_json,&base.join("assets"));panic!("fault phase did not execute");
        }
        #[cfg(target_os = "linux")]
        #[test]
        fn anonymous_crash_phases_preserve_only_published_evidence() {
            let _test_guard=TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
            for (phase,code) in [("prepublication",77),("published",78),("installed",79)] {
                let(base,staging,_source,dest)=fixture();
                let test_module = module_path!().split_once("::").expect("test crate prefix").1;
                let child_test = format!("{test_module}::anonymous_crash_child");
                let status=std::process::Command::new(std::env::current_exe().unwrap())
                    .args(["--exact",child_test.as_str(),"--ignored","--nocapture"])
                    .env("STARLANE_ZIP_STAGE_CRASH_FIXTURE",&base).env("STARLANE_ZIP_STAGE_CRASH_PHASE",phase).status().unwrap();
                assert_eq!(status.code(),Some(code));
                let handle=fs::read_to_string(base.join("handle")).unwrap();
                if phase=="prepublication" { assert_eq!(fs::read_dir(&staging).unwrap().count(),0);assert!(!dest.exists()); }
                else {
                    let archive=staging.join(format!("starlane-zip-evidence-{handle}-archive"));
                    let manifest=staging.join(format!("starlane-zip-evidence-{handle}-manifest.json"));
                    assert!(archive.exists());assert!(manifest.exists());
                    let value:Value=serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
                    assert_eq!(value["phase"],"installation_may_start");
                    if phase=="installed" { assert_eq!(fs::read(dest.join("present-file.bin")).unwrap(),b"captured"); }
                    else { assert!(!dest.exists()); }
                }
                fs::remove_dir_all(base).unwrap();
            }
        }
    }
}

#[tauri::command]
pub fn prepare_vault_archive_import(
    app: AppHandle,
    provider: String,
    archive_path: String,
) -> Result<Value, String> {
    if cfg!(mobile) {
        return Err("Vault archive import is only available in the desktop app".into());
    }
    assert_archive_path(&archive_path)?;
    #[cfg(unix)]
    {
        let source = PathBuf::from(archive_path.trim());
        let source = if source.is_absolute() {
            source
        } else {
            std::env::current_dir()
                .map_err(|e| e.to_string())?
                .join(source)
        };
        let (vault, _) = resolve_vault_dir(&app)?;
        return serde_json::to_value(archive_admission::prepare(
            &provider,
            &source,
            &vault.join("assets"),
        )?)
        .map_err(|e| e.to_string());
    }
    #[cfg(not(unix))]
    {
        let _ = (app, provider);
        Err("Safe staged archive import is not supported on this platform".into())
    }
}
// Legacy archive restoration no longer has an IPC handler: assets and SQLite
// must be committed through the original joint import request above.
#[tauri::command]
pub fn cancel_vault_archive_import(
    _app: AppHandle,
    provider: String,
    handle: String,
) -> Result<Value, String> {
    #[cfg(unix)]
    {
        serde_json::to_value(archive_admission::cancel(&provider, &handle)?).map_err(|e| e.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = (provider, handle);
        Ok(serde_json::json!({"stage_disposition":"unavailable"}))
    }
}

#[tauri::command]
pub fn import_vault_asset_from_path(
    app: AppHandle,
    provider: String,
    source_path: String,
    space_id: Option<String>,
    document_id: Option<String>,
) -> Result<VaultAsset, String> {
    let source = PathBuf::from(source_path.trim());
    if !source.is_file() {
        return Err("Dropped file was not found".to_string());
    }

    let name = source
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "asset".to_string());
    let bytes = fs::read(&source).map_err(|e| e.to_string())?;
    write_asset_bytes(
        &app,
        &provider,
        space_id,
        document_id,
        name,
        infer_mime_type(&source),
        &bytes,
    )
}

#[tauri::command]
pub fn import_vault_asset_from_data_url(
    app: AppHandle,
    provider: String,
    name: String,
    mime_type: Option<String>,
    data_url: String,
    space_id: Option<String>,
    document_id: Option<String>,
) -> Result<VaultAsset, String> {
    let (data_url_mime_type, bytes) = parse_data_url(&data_url)?;
    write_asset_bytes(
        &app,
        &provider,
        space_id,
        document_id,
        name,
        mime_type.or(data_url_mime_type),
        &bytes,
    )
}

#[tauri::command]
pub fn delete_vault_asset(app: AppHandle, vault_path: String) -> Result<(), String> {
    let (vault_dir, _) = resolve_vault_dir(&app)?;
    let candidate = PathBuf::from(vault_path.trim());
    delete_vault_asset_file(&vault_dir, &candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn vault_asset_owner_metadata_round_trips_and_legacy_assets_stay_readable() {
        let owned: VaultAsset = serde_json::from_value(json!({
            "id": "asset-1",
            "name": "diagram.png",
            "size": 12,
            "url": "asset://diagram.png",
            "vault_path": "/tmp/diagram.png",
            "source": "tauri",
            "space_id": "space-1",
            "document_id": "document-1",
            "created_at": "2026-07-13T12:00:00Z"
        }))
        .expect("owned asset should deserialize");
        let serialized = serde_json::to_value(owned).expect("owned asset should serialize");
        assert_eq!(serialized["space_id"], "space-1");
        assert_eq!(serialized["document_id"], "document-1");

        let legacy: VaultAsset = serde_json::from_value(json!({
            "id": "asset-legacy",
            "name": "legacy.png",
            "size": 12,
            "url": "asset://legacy.png",
            "vault_path": "/tmp/legacy.png",
            "source": "tauri",
            "created_at": "2026-07-13T12:00:00Z"
        }))
        .expect("legacy asset should deserialize");
        assert!(legacy.space_id.is_none());
        assert!(legacy.document_id.is_none());
    }

    #[test]
    fn personal_space_uses_owner_subfolder() {
        let space = json!({
            "id": "local-space",
            "name": "Personal Space",
            "owner_username": "local-user",
            "owner_display_name": "Local User",
            "created_by": "Local User"
        });

        assert_eq!(
            space_dir_segments(&space),
            vec!["personalspaces".to_string(), "local-user".to_string()]
        );
    }

    #[test]
    fn personal_space_uses_display_owner_when_username_is_missing() {
        let space = json!({
            "id": "local-space",
            "name": "Personal Space",
            "owner_display_name": "Local User"
        });

        assert_eq!(
            space_dir_segments(&space),
            vec!["personalspaces".to_string(), "Local_User".to_string()]
        );
    }

    #[test]
    fn named_spaces_use_underscore_separated_folder_names() {
        let space = json!({
            "id": "space-1",
            "name": "Product Roadmap 2026"
        });

        assert_eq!(
            space_dir_segments(&space),
            vec!["Product_Roadmap_2026".to_string()]
        );
    }

    #[test]
    fn local_only_native_database_uses_neutral_internal_file_name() {
        assert_eq!(native_database_file_name("none"), "starlane-notes.sqlite3");
        assert_eq!(native_database_file_name(""), "starlane-notes.sqlite3");
        assert_eq!(
            legacy_native_database_file_name("none"),
            "starlane-vault-none.sqlite3"
        );
    }

    #[test]
    fn sync_provider_native_database_names_keep_provider_context() {
        assert_eq!(
            native_database_file_name("icloud-drive"),
            "starlane-vault-icloud-drive.sqlite3"
        );
        assert_eq!(
            native_database_file_name("../google drive"),
            "starlane-vault-googledrive.sqlite3"
        );
    }

    #[test]
    fn renamed_vault_folder_path_keeps_same_parent_and_sanitizes_name() {
        let current = PathBuf::from("/tmp/Starlane Notes Vault");
        let target = renamed_vault_folder_path(&current, "../New: Vault?").expect("target path");

        assert_eq!(target, PathBuf::from("/tmp/New- Vault"));
    }

    #[test]
    fn remaps_stale_ios_sandbox_vault_paths_to_current_documents_directory() {
        let stale = Path::new(
            "/private/var/mobile/Containers/Data/Application/OLD-APP-ID/Documents/The Eagle",
        );
        let current_documents =
            Path::new("/private/var/mobile/Containers/Data/Application/CURRENT-APP-ID/Documents");

        assert_eq!(
            remap_stale_ios_sandbox_vault_path(stale, current_documents),
            current_documents.join("The Eagle"),
        );
        assert_eq!(
            remap_stale_ios_sandbox_vault_path(
                Path::new(
                    "/private/var/mobile/Library/Mobile Documents/com~apple~CloudDocs/The Eagle"
                ),
                current_documents,
            ),
            PathBuf::from(
                "/private/var/mobile/Library/Mobile Documents/com~apple~CloudDocs/The Eagle"
            ),
        );
    }

    #[test]
    fn documents_relative_vault_paths_stay_inside_the_current_app_documents_directory() {
        let documents =
            Path::new("/private/var/mobile/Containers/Data/Application/CURRENT-APP-ID/Documents");
        let vault = documents.join("The Eagle");

        assert_eq!(
            documents_relative_path(&vault, documents),
            Some(PathBuf::from("The Eagle"))
        );
        assert!(is_safe_documents_relative_path(Path::new("The Eagle")));
        assert!(!is_safe_documents_relative_path(Path::new("../The Eagle")));
        assert_eq!(
            remap_stale_ios_sandbox_vault_path(
                Path::new(
                    "/private/var/mobile/Containers/Data/Application/OLD-APP-ID/Documents/The Eagle",
                ),
                documents,
            ),
            vault,
        );
    }

    #[test]
    fn ios_vault_location_fallback_round_trips_documents_relative_and_external_paths() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let documents_dir =
            std::env::temp_dir().join(format!("starlane-ios-vault-fallback-{unique}"));
        fs::create_dir_all(&documents_dir).expect("documents dir");

        let documents_vault = documents_dir.join("The Eagle");
        write_ios_vault_location_fallback(&documents_dir, &documents_vault)
            .expect("write relative fallback");
        let fallback_path = ios_vault_location_fallback_path(&documents_dir);
        let fallback_contents = fs::read_to_string(&fallback_path).expect("fallback contents");
        assert!(fallback_contents.contains("\"vault_documents_relative_path\": \"The Eagle\""));
        assert_eq!(
            read_ios_vault_location_fallback(&documents_dir).expect("read relative fallback"),
            Some(documents_vault)
        );

        let external_vault = PathBuf::from(
            "/private/var/mobile/Library/Mobile Documents/com~apple~CloudDocs/Starlane Notes",
        );
        write_ios_vault_location_fallback(&documents_dir, &external_vault)
            .expect("write external fallback");
        assert_eq!(
            read_ios_vault_location_fallback(&documents_dir).expect("read external fallback"),
            Some(external_vault)
        );

        fs::write(
            &fallback_path,
            r#"{"vault_documents_relative_path":"../Nope"}"#,
        )
        .expect("write unsafe fallback");
        assert_eq!(
            read_ios_vault_location_fallback(&documents_dir).expect("read unsafe fallback"),
            None
        );

        fs::remove_dir_all(documents_dir).expect("cleanup");
    }

    #[test]
    fn parses_generated_markdown_frontmatter() {
        let contents =
            "---\nid: \"doc-1\"\ntitle: \"Runbook\"\nspace_id: \"space-1\"\n---\n\n# Body\n";
        let (metadata, body) = parse_markdown_mirror_file(contents).expect("frontmatter");

        assert_eq!(metadata.get("id").map(String::as_str), Some("doc-1"));
        assert_eq!(metadata.get("title").map(String::as_str), Some("Runbook"));
        assert_eq!(body, "# Body");
    }

    #[test]
    fn decodes_asset_urls_to_native_paths() {
        let path = "/Users/example/Starlane/attachments/doc-1/asset-1.png";
        let encoded = format!("asset://localhost/{}", urlencoding::encode(path));

        assert_eq!(asset_path_from_url(&encoded), Some(PathBuf::from(path)));
        assert_eq!(asset_path_from_url("https://example.test/asset.png"), None);
    }

    #[test]
    fn validates_replay_asset_paths_under_active_vault() {
        let unique = now_nanos().expect("timestamp");
        let base_dir = std::env::temp_dir().join(format!("starlane-replay-asset-test-{unique}"));
        let vault_dir = base_dir.join("vault");
        let outside_dir = base_dir.join("outside");
        fs::create_dir_all(&vault_dir).expect("vault dir");
        fs::create_dir_all(&outside_dir).expect("outside dir");
        let inside_asset = vault_dir.join("asset.txt");
        let outside_asset = outside_dir.join("asset.txt");
        fs::write(&inside_asset, "inside").expect("inside asset");
        fs::write(&outside_asset, "outside").expect("outside asset");

        assert_eq!(
            validate_replay_asset_path(&vault_dir, &inside_asset).expect("inside path"),
            fs::canonicalize(&inside_asset).expect("canonical inside")
        );
        assert!(validate_replay_asset_path(&vault_dir, &outside_asset).is_err());

        fs::remove_dir_all(base_dir).ok();
    }

    #[test]
    fn deletes_only_assets_inside_the_active_vault() {
        let unique = now_nanos().expect("timestamp");
        let base_dir = std::env::temp_dir().join(format!("starlane-delete-asset-test-{unique}"));
        let vault_dir = base_dir.join("vault");
        let outside_dir = base_dir.join("outside");
        fs::create_dir_all(vault_dir.join("spaces/team/attachments/doc-1")).expect("asset dir");
        fs::create_dir_all(&outside_dir).expect("outside dir");
        let inside = vault_dir.join("spaces/team/attachments/doc-1/asset.png");
        let outside = outside_dir.join("asset.png");
        fs::write(&inside, "inside").expect("inside asset");
        fs::write(&outside, "outside").expect("outside asset");

        delete_vault_asset_file(&vault_dir, &inside).expect("delete owned asset");
        assert!(!inside.exists());
        assert!(delete_vault_asset_file(&vault_dir, &outside).is_err());
        assert!(outside.exists());

        fs::remove_dir_all(base_dir).ok();
    }

    #[test]
    fn replaces_an_existing_snapshot_without_losing_the_previous_file_on_failure() {
        let unique = now_nanos().expect("timestamp");
        let root = std::env::temp_dir().join(format!("starlane-snapshot-replace-{unique}"));
        fs::create_dir_all(&root).expect("snapshot dir");
        let snapshot = root.join("snapshot.json");
        let temp = root.join("snapshot.json.tmp");
        fs::write(&snapshot, "old").expect("old snapshot");
        fs::write(&temp, "new").expect("new snapshot");

        replace_snapshot_file(&temp, &snapshot).expect("replace snapshot");
        assert_eq!(fs::read_to_string(&snapshot).unwrap(), "new");
        assert!(!temp.exists());

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn builds_provider_sync_manifest_relative_paths() {
        assert_eq!(
            provider_sync_manifest_relative_path("../google mutation")
                .to_string_lossy()
                .replace('\\', "/"),
            ".starlane/provider-sync/google mutation.json"
        );
        assert_eq!(
            provider_sync_manifest_relative_path("///")
                .to_string_lossy()
                .replace('\\', "/"),
            ".starlane/provider-sync/provider-sync-manifest.json"
        );
    }

    #[test]
    fn writes_provider_sync_manifest_file() {
        let unique = now_nanos().expect("timestamp");
        let root = std::env::temp_dir().join(format!("starlane-provider-sync-test-{unique}"));
        fs::create_dir_all(&root).expect("root dir");

        let result = write_provider_sync_manifest_file(
            &root,
            "mutation-1",
            &json!({
                "schema_version": 1,
                "provider": "icloud-drive",
                "direction": "push"
            }),
        )
        .expect("manifest write");

        assert_eq!(
            result.relative_path,
            ".starlane/provider-sync/mutation-1.json"
        );
        assert!(result.bytes_written > 0);
        let written = fs::read_to_string(root.join(result.relative_path)).expect("read manifest");
        assert!(written.contains("\"provider\": \"icloud-drive\""));

        fs::remove_dir_all(root).ok();
    }

    #[test]
    fn reconciles_manifest_markdown_documents_and_templates() {
        let unique = now_nanos().expect("timestamp");
        let vault_dir = std::env::temp_dir().join(format!("starlane-mirror-test-{unique}"));
        fs::create_dir_all(&vault_dir).expect("vault dir");

        let doc_relative = "spaces/Team/documents/Runbook-doc-1.md";
        let template_relative = "spaces/Team/templates/Brief-template-1.md";
        fs::create_dir_all(vault_dir.join("spaces/Team/documents")).expect("doc dir");
        fs::create_dir_all(vault_dir.join("spaces/Team/templates")).expect("template dir");
        fs::write(
            vault_dir.join(doc_relative),
            "---\nid: \"doc-1\"\ntitle: \"Updated Runbook\"\nspace_id: \"space-1\"\n---\n\nUpdated body\n",
        )
        .expect("doc write");
        fs::write(
            vault_dir.join(template_relative),
            "---\nid: \"template-1\"\nname: \"Updated Brief\"\nspace_id: \"space-1\"\n---\n\nUpdated template\n",
        )
        .expect("template write");
        write_mirror_manifest(
            &vault_dir,
            &HashSet::from([doc_relative.to_string(), template_relative.to_string()]),
        )
        .expect("manifest");

        let snapshot = json!({
            "schema_version": 1,
            "spaces": [{"id": "space-1", "name": "Team"}],
            "folders": [],
            "documents": [{
                "id": "doc-1",
                "space_id": "space-1",
                "title": "Runbook",
                "content": "Old body",
                "updated_at": "2026-07-05T00:00:00Z"
            }],
            "assets": [],
            "templates": [{
                "id": "template-1",
                "space_id": "space-1",
                "name": "Brief",
                "content": "Old template",
                "updated_at": "2026-07-05T00:00:00Z"
            }]
        });

        let result = reconcile_filesystem_mirror_snapshot(&vault_dir, snapshot).expect("reconcile");

        assert_eq!(result.files_scanned, 2);
        assert_eq!(result.documents_updated, 1);
        assert_eq!(result.templates_updated, 1);
        assert_eq!(
            result.snapshot["documents"][0]["title"].as_str(),
            Some("Updated Runbook")
        );
        assert_eq!(
            result.snapshot["documents"][0]["content"].as_str(),
            Some("Updated body")
        );
        assert_eq!(
            result.snapshot["templates"][0]["name"].as_str(),
            Some("Updated Brief")
        );
        assert_eq!(
            result.snapshot["templates"][0]["content"].as_str(),
            Some("Updated template")
        );

        fs::remove_dir_all(vault_dir).expect("cleanup");
    }

    #[test]
    fn imports_untracked_markdown_documents_and_templates() {
        let unique = now_nanos().expect("timestamp");
        let vault_dir = std::env::temp_dir().join(format!("starlane-mirror-import-test-{unique}"));
        fs::create_dir_all(&vault_dir).expect("vault dir");

        let doc_relative = "spaces/Team/documents/Research/Inbox/New Idea.md";
        let template_relative = "spaces/Team/templates/Meeting.md";
        fs::create_dir_all(vault_dir.join("spaces/Team/documents/Research/Inbox"))
            .expect("doc dir");
        fs::create_dir_all(vault_dir.join("spaces/Team/templates")).expect("template dir");
        fs::write(
            vault_dir.join(doc_relative),
            "# New Idea\n\nPlain imported body\n",
        )
        .expect("doc write");
        fs::write(
            vault_dir.join(template_relative),
            "---\nname: \"Meeting Template\"\n---\n\nAgenda\n",
        )
        .expect("template write");
        write_mirror_manifest(&vault_dir, &HashSet::new()).expect("manifest");

        let snapshot = json!({
            "schema_version": 1,
            "spaces": [{"id": "space-1", "name": "Team"}],
            "folders": [],
            "documents": [],
            "assets": [],
            "templates": []
        });

        let result = reconcile_filesystem_mirror_snapshot(&vault_dir, snapshot).expect("reconcile");

        assert_eq!(result.files_scanned, 2);
        assert_eq!(result.documents_imported, 1);
        assert_eq!(result.templates_imported, 1);
        assert_eq!(
            result.snapshot["documents"][0]["title"].as_str(),
            Some("New Idea")
        );
        assert_eq!(
            result.snapshot["documents"][0]["content"].as_str(),
            Some("# New Idea\n\nPlain imported body")
        );
        assert_eq!(result.snapshot["folders"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            result.snapshot["templates"][0]["name"].as_str(),
            Some("Meeting Template")
        );
        assert_eq!(
            result.snapshot["templates"][0]["content"].as_str(),
            Some("Agenda")
        );

        fs::remove_dir_all(vault_dir).expect("cleanup");
    }
}

#[cfg(test)]
mod archive_preservation_tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn archive(contents: &str, entries: &[(&str, &[u8])]) -> zip::ZipArchive<Cursor<Vec<u8>>> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        writer.start_file(ARCHIVE_SNAPSHOT_FILE, options).unwrap();
        writer.write_all(contents.as_bytes()).unwrap();
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        zip::ZipArchive::new(writer.finish().unwrap()).unwrap()
    }
    fn import(archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>, dir: &Path) -> Result<Value, String> {
        let mut contents = String::new();
        archive
            .by_name(ARCHIVE_SNAPSHOT_FILE)
            .map_err(|e| e.to_string())?
            .read_to_string(&mut contents)
            .map_err(|e| e.to_string())?;
        let mut typed = parse_archive_snapshot(&contents)?;
        let mut raw: Value = serde_json::from_str(&contents).map_err(|e| e.to_string())?;
        restore_archive_assets(archive, dir, &mut typed, &mut raw)?;
        Ok(raw)
    }
    fn rich_snapshot() -> Value {
        serde_json::json!({"schema_version":1,"spaces":[],"folders":[],
            "documents":[{"id":"stable-page","readable_identifier":"ENG-0042","maintainer_user_id":"person","future":{"nested":[1,{"opaque":true}]}}],
            "assets":[],"property_schemas":[{"id":"p","options":["one"]}],
            "property_values":[{"id":"v","document_id":"stable-page","property_id":"p","value":"one"}],
            "tags":[{"id":"tag"}],"doc_tags":[{"document_id":"stable-page","tag_id":"tag"}],
            "workflow_templates":[{"id":"wt","stages":[{"id":"review"}]}],
            "document_workflows":[{"id":"dw","current_stage_id":"review","status":"active"}],
            "document_workflow_events":[{"id":"event","created_by":"historical-author","note":"history"}],
            "sync_state":{"queued_operations":[{"id":"queued","payload":{"future":[null,42]}}]},
            "future_root":{"private_extension":{"preserve":true}}})
    }
    #[test]
    fn zip_roundtrip_retains_identifier_display_null_source_and_bound_queue() {
        let mut expected = rich_snapshot();
        expected["documents"] = serde_json::json!([
            {"id":"page-identifier","space_id":"space-1","title":"Runbook","content":"Exact body","human_identifier":"ENG-0042"},
            {"id":"page-cleared","space_id":"space-1","title":"Cleared","human_identifier":null},
            {"id":"page-legacy","space_id":"space-1","title":"Legacy"}
        ]);
        expected["sync_state"]["queued_operations"] = serde_json::json!([{"id":"identifier-op","entity_type":"document","entity_id":"page-identifier","action":"update_identifier","replay_provider":"google-drive","server_context":{"server_base":"https://notes.example.test/api/v1","account_id":"account-a","session_scope":"session-a"},"payload":{"id":"page-identifier","space_id":"space-1","human_identifier":"ENG-0042","source":{"page_updated_at":"2026-10-08T00:00:00Z","published_at":null,"human_identifier":null}}}]);
        let mut zipped = archive(&expected.to_string(), &[]);
        let actual = import(&mut zipped, Path::new("/unused-no-assets")).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual["documents"][0]["human_identifier"], "ENG-0042");
        assert!(actual["documents"][1]["human_identifier"].is_null());
        assert!(actual["documents"][2].get("human_identifier").is_none());
    }

    #[test]
    fn zip_roundtrip_retains_maintenance_catalog_identity_source_and_queue() {
        let mut expected = rich_snapshot();
        expected["documents"] = serde_json::json!([
            {"id":"page-identifier","space_id":"space-1","title":"Runbook","content":"Exact body","maintenance_user_id":null,"maintenance_group_id":"team-a"},
            {"id":"page-cleared","space_id":"space-1","title":"Cleared","maintenance_user_id":null,"maintenance_group_id":null},
            {"id":"page-legacy","space_id":"space-1","title":"Legacy"}
        ]);
        expected["sync_state"]["queued_operations"] = serde_json::json!([{"id":"identifier-op","entity_type":"document","entity_id":"page-identifier","action":"update_maintenance","replay_provider":"google-drive","server_context":{"server_base":"https://notes.example.test/api/v1","account_id":"account-a","session_scope":"session-a"},"payload":{"id":"page-identifier","space_id":"space-1","maintenance":{"kind":"group","group_id":"team-a"},"source":{"page_updated_at":"2026-10-08T00:00:00Z","published_at":null,"maintenance_user_id":null,"maintenance_group_id":null}}}]);
        expected["maintenance_target_catalogs"] = serde_json::json!([{"server_base":"https://notes.example.test/api/v1","account_id":"account-a","space_id":"space-1","targets":{"users":[],"groups":[{"id":"team-a","name":"Team","managed_space_id":null}],"groups_available":true}}]);
        let mut zipped = archive(&expected.to_string(), &[]);
        let actual = import(&mut zipped, Path::new("/unused-no-assets")).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual["documents"][0]["maintenance_group_id"], "team-a");
        assert!(actual["documents"][1]["maintenance_group_id"].is_null());
        assert!(actual["documents"][2].get("maintenance_group_id").is_none());
    }

    #[test]
    fn zip_roundtrip_retains_all_collections_and_unknown_nested_fields() {
        let expected = rich_snapshot();
        let mut zipped = archive(&expected.to_string(), &[]);
        assert_eq!(
            import(&mut zipped, Path::new("/unused-no-assets")).unwrap(),
            expected
        );
    }
    #[test]
    fn zip_import_rewrites_only_extracted_assets_by_original_index() {
        let mut expected = rich_snapshot();
        let asset = |id: &str, source: &str| serde_json::json!({"id":id,"name":"file.bin","mime_type":null,"size":99,"url":"old-url","vault_path":"old-path","source":source,"created_at":"old","future":{"keep":id}});
        expected["assets"] = serde_json::json!([
            asset("browser", "browser"),
            asset("missing", "tauri"),
            asset("present", "tauri")
        ]);
        let dir = std::env::temp_dir().join(format!("starlane-archive-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut zipped = archive(
            &expected.to_string(),
            &[("assets/present-file.bin", b"restored")],
        );
        let got = import(&mut zipped, &dir).unwrap();
        let path = dir.join("present-file.bin");
        expected["assets"][2]["size"] = Value::from(8);
        expected["assets"][2]["vault_path"] = Value::String(path.to_string_lossy().into_owned());
        expected["assets"][2]["url"] = Value::String(asset_url(&path));
        assert_eq!(got, expected);
        assert_eq!(fs::read(&path).unwrap(), b"restored");
        assert!(!dir.join("missing-file.bin").exists());
        fs::remove_file(path).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn zip_import_keeps_old_valid_snapshots_and_original_validation() {
        let old = serde_json::json!({"schema_version":1,"spaces":[],"folders":[],"documents":[]});
        assert_eq!(
            import(&mut archive(&old.to_string(), &[]), Path::new("/unused")).unwrap(),
            old
        );
        for invalid in [
            "{",
            r#"{"schema_version":2,"spaces":[],"folders":[],"documents":[]}"#,
            r#"{"schema_version":1,"spaces":[],"folders":[],"documents":[],"assets":[{"id":"invalid"}]}"#,
            r#"{"schema_version":1,"spaces":[],"folders":[],"documents":[],"documents":null}"#,
        ] {
            assert!(import(&mut archive(invalid, &[]), Path::new("/unused")).is_err());
        }
        // Existing validator parses JSON Value first: valid last duplicate wins.
        let duplicates = r#"{"schema_version":1,"spaces":[],"folders":[],"documents":null,"documents":[],"future":1}"#;
        let got = import(&mut archive(duplicates, &[]), Path::new("/unused")).unwrap();
        assert_eq!(got["documents"], serde_json::json!([]));
        assert_eq!(got["future"], Value::from(1));
    }
    #[test]
    fn zip_import_rejects_missing_snapshot_entry() {
        let writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let mut zipped = zip::ZipArchive::new(writer.finish().unwrap()).unwrap();
        assert!(import(&mut zipped, Path::new("/unused")).is_err());
    }
    #[test]
    fn zip_import_rejects_invalid_maintenance_before_extraction() {
        for pair in [
            serde_json::json!({"maintenance_user_id":"person","maintenance_group_id":"team"}),
            serde_json::json!({"maintenance_user_id":42}),
        ] {
            let snapshot =
                serde_json::json!({"schema_version":1,"spaces":[],"folders":[],"documents":[pair]});
            assert!(import(
                &mut archive(&snapshot.to_string(), &[]),
                Path::new("/unused")
            )
            .is_err());
        }
    }

}

#[cfg(all(test,not(any(target_os="ios",target_os="android"))))]
mod snapshot_selection_tests {
    use super::*;
    fn fixture()->PathBuf {let path=std::env::temp_dir().join(format!("starlane-selection-vault-{}-{}",std::process::id(),now_nanos().unwrap()));fs::create_dir_all(&path).unwrap();path}
    #[test]
    fn missing_settings_and_omitted_key_share_default_without_creating_at_capture() {
        let dir=fixture();let settings=dir.join("settings.json");let root=dir.join("fresh-vault");
        let request=SnapshotRequestBinding::capture_at(settings.clone(),root.clone(),"none").unwrap();assert!(!settings.exists());assert!(!root.exists());
        fs::write(&settings,br#"{"theme":"dark","token":"SECRET"}"#).unwrap();request.verify_selection().unwrap();assert!(!root.exists());
        assert_eq!(request.database_path().unwrap(),root.join(".starlane/starlane-notes.sqlite3"));assert!(!root.join("starlane-vault-none.json").exists());fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn malformed_settings_reject_without_secret_payload_or_initialization() {
        for bytes in [br#"{"vault_path":"oops","token":"SECRET""#.as_slice(),b"",b"null",b"[]",b"7"] {
            let dir=fixture();let settings=dir.join("settings.json");let root=dir.join("fresh-vault");fs::write(&settings,bytes).unwrap();
            let error=SnapshotRequestBinding::capture_at(settings.clone(),root.clone(),"none").err().unwrap();assert!(!error.contains("SECRET"));assert!(!root.exists());assert_eq!(fs::read(settings).unwrap(),bytes);fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn raw_null_nonstring_fallback_is_preserved_but_representation_change_rejected() {
        for value in [Value::Null,Value::Bool(false),Value::from(7),serde_json::json!(["raw"]),serde_json::json!({"raw":"unchanged"})] {
            let dir=fixture();let settings=dir.join("settings.json");let root=dir.join("fallback");fs::write(&settings,serde_json::to_vec(&serde_json::json!({"vault_path":value,"theme":"dark"})).unwrap()).unwrap();
            let request=SnapshotRequestBinding::capture_at(settings.clone(),root.clone(),"none").unwrap();request.verify_selection().unwrap();assert_eq!(request.root,root);assert!(!root.exists());
            fs::write(&settings,b"{}").unwrap();assert!(request.database_path().is_err());assert!(!root.exists());fs::remove_dir_all(dir).unwrap();
        }
    }
    #[test]
    fn fixed_root_helpers_preserve_legacy_database_migration_and_never_retarget_mirror() {
        let dir=fixture();let root=dir.join("original");let other=dir.join("other");fs::create_dir(&root).unwrap();let settings=dir.join("settings.json");fs::write(&settings,serde_json::to_vec(&serde_json::json!({"vault_path":root})).unwrap()).unwrap();
        let legacy=root.join(legacy_native_database_file_name("none"));fs::write(&legacy,b"old DB bytes").unwrap();let request=SnapshotRequestBinding::capture_at(settings.clone(),other.clone(),"none").unwrap();let path=request.database_path().unwrap();assert_eq!(fs::read(path).unwrap(),b"old DB bytes");assert!(!legacy.exists());
        fs::write(&settings,serde_json::to_vec(&serde_json::json!({"vault_path":other})).unwrap()).unwrap();assert!(request.write_mirror(&serde_json::json!({"schema_version":1,"spaces":[],"folders":[],"documents":[]})).is_err());assert!(!other.exists());assert!(!root.join(snapshot_file_name("none")).exists());fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn immutable_download_provenance_codec_keeps_exact_raw_record() {
        let raw = "{\"version_id\":\"v1\", \"future\": [1,2]}";
        let value=serde_json::json!({"id":"native-download-owned","name":"same-name.bin","mime_type":"application/octet-stream","size":0,"url":"asset://localhost/owned","vault_path":"/owned","source":"tauri","space_id":"local-space","document_id":"local-page","created_at":"original-time","server_version_provenance":{"format":1,"raw_version_record":raw,"future_field":{"kept":true}}});
        let asset:VaultAsset=serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(asset).unwrap(),value);
    }
    #[test]
    fn immutable_download_namespace_refuses_ordinary_delete_including_missing_and_alias() {
        let root=fixture();let owned=root.join(".starlane/attachment-downloads/operation");fs::create_dir_all(&owned).unwrap();let payload=owned.join("asset.bin");fs::write(&payload,b"owned immutable bytes").unwrap();
        assert!(delete_vault_asset_file(&root,&payload).unwrap_err().contains("retention lifecycle"));
        assert!(delete_vault_asset_file(&root,&owned.join("absent.bin")).is_err());
        #[cfg(unix)] {let alias=root.join("alias.bin");std::os::unix::fs::symlink(&payload,&alias).unwrap();assert!(delete_vault_asset_file(&root,&alias).is_err());}
        assert_eq!(fs::read(payload).unwrap(),b"owned immutable bytes");fs::remove_dir_all(root).unwrap();
    }

}

#[cfg(all(test,target_os="linux"))]
mod complete_archive_refusal_tests;

/// Checked portable historical identities, never native ownership or absence.
pub(crate) fn retained_original_disposition_ids(value:&serde_json::Value)->Result<std::collections::BTreeSet<String>,String>{archive_provenance::retained_original_disposition_ids(value)}

pub(crate) fn retained_original_local_import_ids(value:&Value)->Result<std::collections::BTreeSet<String>,String>{archive_provenance::retained_original_local_import_ids(value)}
pub(crate) fn retained_history_requires_format4(value:&serde_json::Value)->Result<bool,String>{archive_provenance::retained_history_requires_format4(value)}

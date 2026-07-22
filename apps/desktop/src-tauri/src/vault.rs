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
    vault_path: String,
    source: String,
    #[serde(default)]
    space_id: Option<String>,
    #[serde(default)]
    document_id: Option<String>,
    created_at: String,
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
    assets: Vec<VaultAsset>,
    #[serde(default)]
    templates: Vec<Value>,
}

#[derive(Debug, Clone)]
struct MirrorFolder {
    id: String,
    name: String,
    parent_id: Option<String>,
}

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

    Some((metadata, body.trim_end_matches('\n').to_string()))
}

fn parse_markdown_import_file(contents: &str) -> (HashMap<String, String>, String) {
    parse_markdown_mirror_file(contents)
        .unwrap_or_else(|| (HashMap::new(), contents.trim_end_matches('\n').to_string()))
}

fn infer_mime_type(path: &Path) -> Option<String> {
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

fn archive_asset_path(asset: &VaultAsset) -> String {
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
    })
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

    serde_json::from_value(value).map_err(|e| e.to_string())
}

fn default_vault_dir(app: &AppHandle) -> Result<PathBuf, String> {
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

fn read_mirror_manifest(vault_dir: &Path) -> HashSet<String> {
    let manifest_path = vault_dir.join(MIRROR_MANIFEST_FILE);
    let Ok(contents) = fs::read_to_string(manifest_path) else {
        return HashSet::new();
    };
    serde_json::from_str::<Vec<String>>(&contents)
        .unwrap_or_default()
        .into_iter()
        .collect()
}

fn write_mirror_manifest(vault_dir: &Path, paths: &HashSet<String>) -> Result<(), String> {
    let mut sorted = paths.iter().cloned().collect::<Vec<_>>();
    sorted.sort();
    let contents = serde_json::to_string_pretty(&sorted).map_err(|e| e.to_string())?;
    fs::write(vault_dir.join(MIRROR_MANIFEST_FILE), contents).map_err(|e| e.to_string())
}

fn remove_manifest_file(vault_dir: &Path, relative: &str) -> Result<(), String> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() || relative.contains("..") {
        return Ok(());
    }

    let path = vault_dir.join(relative_path);
    if path.is_file() {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn cleanup_empty_parent_dirs(vault_dir: &Path, previous_paths: &HashSet<String>) {
    let mut dirs = previous_paths
        .iter()
        .filter_map(|relative| {
            let relative_path = Path::new(relative);
            if relative_path.is_absolute() || relative.contains("..") {
                return None;
            }
            Some(vault_dir.join(relative_path).parent()?.to_path_buf())
        })
        .collect::<Vec<_>>();
    dirs.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    dirs.dedup();

    for dir in dirs {
        if dir.starts_with(vault_dir) && dir != vault_dir {
            let _ = fs::remove_dir(&dir);
        }
    }
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

fn folder_segments(
    folder_id: &str,
    folders: &HashMap<String, MirrorFolder>,
    memo: &mut HashMap<String, Vec<String>>,
    visiting: &mut HashSet<String>,
) -> Vec<String> {
    if let Some(cached) = memo.get(folder_id) {
        return cached.clone();
    }

    if !visiting.insert(folder_id.to_string()) {
        return Vec::new();
    }

    let Some(folder) = folders.get(folder_id) else {
        visiting.remove(folder_id);
        return Vec::new();
    };

    let mut segments = folder
        .parent_id
        .as_deref()
        .map(|parent_id| folder_segments(parent_id, folders, memo, visiting))
        .unwrap_or_default();
    segments.push(sanitize_path_segment(&folder.name, &folder.id));
    visiting.remove(folder_id);
    memo.insert(folder_id.to_string(), segments.clone());
    segments
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
        "created_by",
        "last_updated_by",
        "created_at",
        "updated_at",
    ] {
        if let Some(value) = value_str(item, key) {
            let _ = writeln!(output, "{key}: {}", yaml_string(value));
        }
    }
    output.push_str("---\n\n");
    output.push_str(value_str(item, body_key).unwrap_or(""));
    output.push('\n');

    fs::write(path, output).map_err(|e| e.to_string())
}

fn ensure_space_dirs(space_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(space_dir.join(MIRROR_DOCUMENTS_DIR)).map_err(|e| e.to_string())?;
    fs::create_dir_all(space_dir.join(MIRROR_ATTACHMENTS_DIR)).map_err(|e| e.to_string())?;
    fs::create_dir_all(space_dir.join(MIRROR_TEMPLATES_DIR)).map_err(|e| e.to_string())
}

fn reset_filesystem_mirror(app: &AppHandle) -> Result<(), String> {
    let (vault_dir, _) = resolve_vault_dir(app)?;
    let previous_paths = read_mirror_manifest(&vault_dir);
    for relative in &previous_paths {
        remove_manifest_file(&vault_dir, relative)?;
    }
    cleanup_empty_parent_dirs(&vault_dir, &previous_paths);
    let manifest_path = vault_dir.join(MIRROR_MANIFEST_FILE);
    if manifest_path.exists() {
        fs::remove_file(manifest_path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn write_filesystem_mirror(app: &AppHandle, snapshot: &Value) -> Result<(), String> {
    let (vault_dir, _) = resolve_vault_dir(app)?;
    ensure_vault_dir(&vault_dir)?;

    let previous_paths = read_mirror_manifest(&vault_dir);
    let mut generated_paths = HashSet::new();
    let mut space_dirs = HashMap::<String, PathBuf>::new();

    for space in value_array(snapshot, "spaces") {
        let id = value_str(space, "id").unwrap_or("space").to_string();
        let space_dir = space_dir_path(&vault_dir, space);
        ensure_space_dirs(&space_dir)?;
        space_dirs.insert(id, space_dir);
    }

    let mut folders = HashMap::<String, MirrorFolder>::new();
    for folder in value_array(snapshot, "folders") {
        let Some(id) = value_str(folder, "id") else {
            continue;
        };
        folders.insert(
            id.to_string(),
            MirrorFolder {
                id: id.to_string(),
                name: value_str(folder, "name").unwrap_or(id).to_string(),
                parent_id: value_str(folder, "parent_id").map(str::to_string),
            },
        );
    }

    let mut folder_memo = HashMap::<String, Vec<String>>::new();
    for document in value_array(snapshot, "documents") {
        let space_id = value_str(document, "space_id").unwrap_or("personal");
        let space_dir = space_dirs
            .entry(space_id.to_string())
            .or_insert_with(|| {
                vault_dir
                    .join(MIRROR_SPACES_DIR)
                    .join(sanitize_space_dir_name(space_id, "space"))
            })
            .clone();
        ensure_space_dirs(&space_dir)?;

        let mut doc_dir = space_dir.join(MIRROR_DOCUMENTS_DIR);
        if let Some(folder_id) = value_str(document, "folder_id") {
            let segments =
                folder_segments(folder_id, &folders, &mut folder_memo, &mut HashSet::new());
            for segment in segments {
                doc_dir = doc_dir.join(segment);
            }
        }

        let file_path = doc_dir.join(markdown_file_name(document, "document"));
        write_markdown_mirror_file(&file_path, document, "title", "content")?;
        if let Some(relative) = relative_path(&vault_dir, &file_path) {
            generated_paths.insert(relative);
        }
    }

    for template in value_array(snapshot, "templates") {
        let space_id = value_str(template, "space_id").unwrap_or("personal");
        let space_dir = space_dirs
            .entry(space_id.to_string())
            .or_insert_with(|| {
                vault_dir
                    .join(MIRROR_SPACES_DIR)
                    .join(sanitize_space_dir_name(space_id, "space"))
            })
            .clone();
        ensure_space_dirs(&space_dir)?;

        let file_path = space_dir
            .join(MIRROR_TEMPLATES_DIR)
            .join(markdown_file_name(template, "template"));
        write_markdown_mirror_file(&file_path, template, "name", "content")?;
        if let Some(relative) = relative_path(&vault_dir, &file_path) {
            generated_paths.insert(relative);
        }
    }

    for stale_path in previous_paths.difference(&generated_paths) {
        remove_manifest_file(&vault_dir, stale_path)?;
    }
    cleanup_empty_parent_dirs(&vault_dir, &previous_paths);
    write_mirror_manifest(&vault_dir, &generated_paths)
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
    if value_str(item, body_key) != Some(body) {
        set_object_string(item, body_key, body.to_string());
        changed = true;
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

fn collect_markdown_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            collect_markdown_files(&path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            files.push(path);
        }
    }
    Ok(())
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
    collect_markdown_files(&vault_dir.join(MIRROR_SPACES_DIR), &mut markdown_files)?;
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
    let manifest = read_mirror_manifest(vault_dir);
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

    Ok(MirrorReconciliationResult {
        snapshot,
        files_scanned,
        documents_updated,
        templates_updated,
        documents_imported,
        templates_imported,
    })
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
    ensure_vault_dir(&vault_dir)?;
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
    let snapshot_path = resolve_snapshot_path(&app, &provider)?;
    let temp_path = snapshot_path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?;

    fs::write(&temp_path, contents).map_err(|e| e.to_string())?;
    fs::rename(&temp_path, &snapshot_path).map_err(|e| e.to_string())?;
    write_filesystem_mirror(&app, &snapshot)
}

#[tauri::command]
pub fn reconcile_vault_mirror(
    app: AppHandle,
    provider: String,
) -> Result<MirrorReconciliationResult, String> {
    let (vault_dir, _) = resolve_vault_dir(&app)?;
    let Some(snapshot) = read_vault_snapshot(app.clone(), provider.clone())? else {
        return Err("No local vault snapshot was found".to_string());
    };
    let result = reconcile_filesystem_mirror_snapshot(&vault_dir, snapshot)?;

    let snapshot_path = resolve_snapshot_path(&app, &provider)?;
    let temp_path = snapshot_path.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(&result.snapshot).map_err(|e| e.to_string())?;
    fs::write(&temp_path, contents).map_err(|e| e.to_string())?;
    fs::rename(&temp_path, &snapshot_path).map_err(|e| e.to_string())?;
    write_filesystem_mirror(&app, &result.snapshot)?;

    Ok(result)
}

#[tauri::command]
pub fn reset_vault_snapshot(app: AppHandle, provider: String) -> Result<(), String> {
    let snapshot_path = resolve_snapshot_path(&app, &provider)?;
    if snapshot_path.exists() {
        fs::remove_file(snapshot_path).map_err(|e| e.to_string())?;
    }
    reset_filesystem_mirror(&app)?;
    Ok(())
}

#[tauri::command]
pub fn export_vault_archive(
    app: AppHandle,
    provider: String,
    archive_path: String,
) -> Result<(), String> {
    if cfg!(mobile) {
        return Err("Vault archive export is only available in the desktop app".to_string());
    }
    assert_archive_path(&archive_path)?;

    let snapshot_path = resolve_snapshot_path(&app, &provider)?;
    if !snapshot_path.exists() {
        return Err("No local vault snapshot was found".to_string());
    }

    let snapshot_contents = fs::read_to_string(&snapshot_path).map_err(|e| e.to_string())?;
    let snapshot = parse_archive_snapshot(&snapshot_contents)?;
    let output_path = PathBuf::from(archive_path.trim());
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    let archive_file = fs::File::create(&output_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipWriter::new(archive_file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    archive
        .start_file(ARCHIVE_SNAPSHOT_FILE, options)
        .map_err(|e| e.to_string())?;
    archive
        .write_all(snapshot_contents.as_bytes())
        .map_err(|e| e.to_string())?;

    for asset in snapshot
        .assets
        .iter()
        .filter(|asset| asset.source == "tauri")
    {
        let source_path = match &asset.vault_path {
            path if !path.trim().is_empty() => PathBuf::from(path),
            _ => continue,
        };
        if source_path.is_file() {
            add_file_to_archive(
                &mut archive,
                &archive_asset_path(asset),
                &source_path,
                options,
            )?;
        }
    }

    archive.finish().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn import_vault_archive(
    app: AppHandle,
    _provider: String,
    archive_path: String,
) -> Result<Value, String> {
    if cfg!(mobile) {
        return Err("Vault archive import is only available in the desktop app".to_string());
    }
    assert_archive_path(&archive_path)?;

    let archive_file =
        fs::File::open(PathBuf::from(archive_path.trim())).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(archive_file).map_err(|e| e.to_string())?;

    let mut snapshot_contents = String::new();
    archive
        .by_name(ARCHIVE_SNAPSHOT_FILE)
        .map_err(|_| "Vault archive is missing vault.json".to_string())?
        .read_to_string(&mut snapshot_contents)
        .map_err(|e| e.to_string())?;

    let mut snapshot = parse_archive_snapshot(&snapshot_contents)?;
    let assets_dir = resolve_assets_dir(&app)?;

    for asset in snapshot
        .assets
        .iter_mut()
        .filter(|asset| asset.source == "tauri")
    {
        let asset_archive_path = archive_asset_path(asset);
        let Ok(mut zipped_asset) = archive.by_name(&asset_archive_path) else {
            continue;
        };

        let destination = assets_dir.join(archive_asset_name(asset));
        let mut destination_file = fs::File::create(&destination).map_err(|e| e.to_string())?;
        std::io::copy(&mut zipped_asset, &mut destination_file).map_err(|e| e.to_string())?;
        let metadata = fs::metadata(&destination).map_err(|e| e.to_string())?;

        asset.size = metadata.len();
        asset.vault_path = destination.to_string_lossy().into_owned();
        asset.url = asset_url(&destination);
    }

    serde_json::to_value(snapshot).map_err(|e| e.to_string())
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
        let path = "/Users/eagle/Starlane/attachments/doc-1/asset-1.png";
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

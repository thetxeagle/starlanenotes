//! Offline, quiescent-snapshot inspection only. Never installs blobs or grants access.
use super::{asset_association_current, asset_receipt_current, AssetReplayReceipt};
use sha2::{Digest, Sha256};
use std::{
    fs::File,
    io::Read,
    path::{Component, Path},
};

#[derive(Debug)]
pub(crate) struct ClassifiedAssetReceipt {
    pub receipt: AssetReplayReceipt,
    pub raw_receipt: String,
    pub current_attachment: bool,
    /// Published receipts require independent canonical-rewrite classification.
    pub requires_canonical_evidence: bool,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictIncoming {
    provider: String,
    operation_id: String,
    entity_type: String,
    entity_id: String,
    action: String,
    payload_fingerprint: String,
}
fn digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn decode(raw: &str) -> Result<AssetReplayReceipt, String> {
    let r: AssetReplayReceipt =
        serde_json::from_str(raw).map_err(|_| "Malformed asset receipt".to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|_| "Malformed asset receipt".to_string())?;
    let i: StrictIncoming = serde_json::from_value(value["incoming"].clone())
        .map_err(|_| "Malformed asset incoming identity".to_string())?;
    if i.provider != r.incoming.provider
        || i.operation_id != r.incoming.operation_id
        || i.entity_type != r.incoming.entity_type
        || i.entity_id != r.incoming.entity_id
        || i.action != r.incoming.action
        || i.payload_fingerprint != r.incoming.payload_fingerprint
        || [
            &i.provider,
            &i.operation_id,
            &i.entity_id,
            &r.actor_id,
            &r.space_id,
            &r.document_id,
            &r.filename,
            &r.created_at,
        ]
        .iter()
        .any(|s| s.trim().is_empty())
        || i.entity_type != "asset"
        || i.action != "create"
        || !digest(&i.payload_fingerprint)
        || !digest(&r.binary_sha256)
        || r.byte_length <= 0
        || r.byte_length > 100 * 1024 * 1024
        || !matches!(r.source_kind.as_str(), "unchanged" | "draft" | "published")
        || (r.source_kind == "unchanged"
            && (r.source_id.is_some() || r.source_revision.is_some() || r.body_sha256.is_some()))
        || (r.source_kind != "unchanged"
            && (r.source_id.as_ref().is_none_or(|s| s.trim().is_empty())
                || r.source_revision
                    .as_ref()
                    .is_none_or(|s| s.trim().is_empty())
                || r.body_sha256.as_ref().is_none_or(|s| !digest(s))))
        || (r.source_kind == "published" && r.source_id.as_ref() != Some(&r.document_id))
    {
        return Err("Invalid asset receipt envelope".into());
    }
    let parts: Vec<_> = r.storage_path.split('/').collect();
    if parts.len() != 5
        || parts[0] != "spaces"
        || parts[2] != "attachments"
        || parts[3] != r.document_id
        || parts[4] != r.filename
        || parts.iter().any(|p| {
            p.is_empty() || *p == "." || *p == ".." || p.contains('\\') || p.contains('\0')
        })
    {
        return Err("Invalid asset receipt path".into());
    }
    Ok(r)
}

/// Reads all receipts including SQL-only historical rows. Caller must supply a
/// stopped-writer snapshot connection; no environment DATA_DIR or access mutation.
pub(crate) async fn classify_at(
    root: &Path,
    conn: &mut sqlx::AnyConnection,
) -> Result<Vec<ClassifiedAssetReceipt>, String> {
    let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT provider,incoming_id,receipt_json FROM asset_replay_receipts ORDER BY provider,incoming_id").fetch_all(&mut *conn).await.map_err(|e|e.to_string())?;
    let mut result = Vec::new();
    for (provider, id, raw) in rows {
        let r = decode(&raw)?;
        if r.incoming.provider != provider
            || r.incoming.operation_id != id
            || !asset_receipt_current(conn, &r).await?
        {
            return Err("Asset receipt SQL/envelope differs".into());
        }
        let current = asset_association_current(conn, &r).await?;
        if current {
            let slug:Option<String>=sqlx::query_scalar("SELECT s.slug FROM spaces s JOIN documents d ON d.space_id=s.id WHERE s.id=$1 AND d.id=$2").bind(&r.space_id).bind(&r.document_id).fetch_optional(&mut *conn).await.map_err(|e|e.to_string())?;
            if slug
                .as_ref()
                .is_none_or(|s| r.storage_path.split('/').nth(1) != Some(s.as_str()))
            {
                return Err("Asset owning space/path differs".into());
            }
            verify_bytes(root, &r)
                .map_err(|_| "Current asset retained bytes differ".to_string())?;
        }
        result.push(ClassifiedAssetReceipt {
            requires_canonical_evidence: r.source_kind == "published",
            receipt: r,
            raw_receipt: raw,
            current_attachment: current,
        });
    }
    Ok(result)
}

#[cfg(target_os = "linux")]
fn verify_bytes(root: &Path, r: &AssetReplayReceipt) -> std::io::Result<()> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::MetadataExt,
        },
    };
    fn open(dir: &File, name: &std::ffi::OsStr, directory: bool) -> std::io::Result<File> {
        use std::os::unix::ffi::OsStrExt;
        let n = CString::new(name.as_bytes()).map_err(|_| std::io::ErrorKind::InvalidInput)?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if directory { libc::O_DIRECTORY } else { 0 };
        // SAFETY: held directory and NUL-terminated component; returned FD owned once.
        let fd = unsafe { libc::openat(dir.as_raw_fd(), n.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_fd(fd) })
    }
    fn rooted(root: &Path, relative: &str) -> std::io::Result<(File, Vec<(u64, u64)>)> {
        if !root.is_absolute() {
            return Err(std::io::ErrorKind::InvalidInput.into());
        }
        let mut f = File::open("/")?;
        let mut ancestry = Vec::new();
        let m = f.metadata()?;
        ancestry.push((m.dev(), m.ino()));
        for c in root.components() {
            match c {
                Component::RootDir => {}
                Component::Normal(n) => {
                    f = open(&f, n, true)?;
                    let m = f.metadata()?;
                    ancestry.push((m.dev(), m.ino()));
                }
                _ => return Err(std::io::ErrorKind::InvalidInput.into()),
            }
        }
        let parts: Vec<_> = relative.split('/').collect();
        for (i, p) in parts.iter().enumerate() {
            f = open(&f, std::ffi::OsStr::new(p), i + 1 < parts.len())?;
            if i + 1 < parts.len() {
                let m = f.metadata()?;
                ancestry.push((m.dev(), m.ino()));
            }
        }
        Ok((f, ancestry))
    }
    let (mut file, ancestry) = rooted(root, &r.storage_path)?;
    let before = file.metadata()?;
    if !before.is_file() || before.len() != r.byte_length as u64 {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut h = Sha256::new();
    let mut n = 0u64;
    let mut chunk = [0u8; 65536];
    while n <= r.byte_length as u64 {
        let count = file.read(&mut chunk)?;
        if count == 0 {
            break;
        }
        n += count as u64;
        h.update(&chunk[..count]);
    }
    let (fresh_file, fresh_ancestry) = rooted(root, &r.storage_path)?;
    let fresh = fresh_file.metadata()?;
    let after = file.metadata()?;
    if ancestry != fresh_ancestry
        || n != r.byte_length as u64
        || hex::encode(h.finalize()) != r.binary_sha256
        || (
            before.dev(),
            before.ino(),
            before.len(),
            before.mtime(),
            before.mtime_nsec(),
            before.ctime(),
            before.ctime_nsec(),
        ) != (
            after.dev(),
            after.ino(),
            after.len(),
            after.mtime(),
            after.mtime_nsec(),
            after.ctime(),
            after.ctime_nsec(),
        )
        || (fresh.dev(), fresh.ino()) != (after.dev(), after.ino())
    {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    Ok(())
}
#[cfg(not(target_os = "linux"))]
fn verify_bytes(_root: &Path, _r: &AssetReplayReceipt) -> std::io::Result<()> {
    Err(std::io::ErrorKind::Unsupported.into())
}

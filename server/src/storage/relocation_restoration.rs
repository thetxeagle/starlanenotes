//! Read-only relocation evidence inspection for application-owned restoration.
//! Observed inode IDs are observations, never grants or automatically eligible bindings.
use super::*;
use sqlx::{AnyConnection, Row};
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Observation {
    pub device: u64,
    pub inode: u64,
    pub links: u64,
    pub length: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Association {
    Current { document_id: String },
    Historical,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CommitState {
    PreparedUncommitted,
    Committed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PathRequirement {
    pub subject_id: String,
    pub role: String,
    pub relative: String,
    pub original_identity: Option<(u64, u64)>,
    pub expected_length: u64,
    pub expected_sha256: String,
    pub required: bool,
    pub observed: Option<Observation>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Inspection {
    pub state: CommitState,
    pub manifest: PreparedRelocation,
    pub manifest_bytes: Vec<u8>,
    pub logical_receipt: Option<serde_json::Value>,
    pub receipt_bytes: Option<Vec<u8>>,
    pub escrow_rows: Vec<serde_json::Value>,
    pub associations: Vec<(String, String, Association)>,
    pub paths: Vec<PathRequirement>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictManifest {
    version: u32,
    op_id: String,
    source_fingerprint: String,
    request_fingerprint: String,
    files: Vec<StrictFile>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictFile {
    subject_id: String,
    kind: String,
    source: String,
    destination: String,
    sha256: String,
    length: u64,
    source_device: u64,
    source_inode: u64,
    destination_device: u64,
    destination_inode: u64,
}
fn manifest(bytes: &[u8]) -> Result<PreparedRelocation, AppError> {
    let strict: StrictManifest = serde_json::from_slice(bytes)
        .map_err(|_| conflict("invalid strict restoration manifest"))?;
    let m = PreparedRelocation {
        version: strict.version,
        op_id: strict.op_id,
        source_fingerprint: strict.source_fingerprint,
        request_fingerprint: strict.request_fingerprint,
        files: strict
            .files
            .into_iter()
            .map(|f| InstalledFile {
                subject_id: f.subject_id,
                kind: f.kind,
                source: f.source,
                destination: f.destination,
                sha256: f.sha256,
                length: f.length,
                source_device: f.source_device,
                source_inode: f.source_inode,
                destination_device: f.destination_device,
                destination_inode: f.destination_inode,
            })
            .collect(),
    };
    let mut sources = std::collections::BTreeSet::new();
    let mut destinations = std::collections::BTreeSet::new();
    let mut subjects = std::collections::BTreeSet::new();
    if !valid_manifest(&m)
        || m.files.iter().any(|f| {
            !sources.insert(f.source.clone())
                || !destinations.insert(f.destination.clone())
                || !subjects.insert((f.kind.clone(), f.subject_id.clone()))
        })
        || sources.iter().any(|p| destinations.contains(p))
    {
        return Err(conflict("ambiguous restoration manifest"));
    }
    Ok(m)
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictReceipt {
    op_id: String,
    preview_id: String,
    actor_id: String,
    source_space_id: String,
    destination_space_id: String,
    request_fingerprint: String,
    source_fingerprint: String,
    manifest_json: String,
    expected_state_json: String,
    created_at: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictSeal {
    version: u32,
    receipt: StrictReceipt,
}
fn sealed(raw: &[u8]) -> Result<serde_json::Value, AppError> {
    let seal: StrictSeal =
        serde_json::from_slice(raw).map_err(|_| conflict("invalid strict logical seal"))?;
    if seal.version != 1 {
        return Err(conflict("invalid seal version"));
    }
    serde_json::to_value(seal.receipt).map_err(AppError::internal)
}
fn optional_receipt(root: &Path, op: &str) -> Result<Option<Vec<u8>>, AppError> {
    match backend::read_receipt(root, op) {
        Ok(raw) => Ok(Some(raw)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(conflict(e)),
    }
}
async fn escrow(conn: &mut AnyConnection, op: &str) -> Result<Vec<serde_json::Value>, AppError> {
    let rows:Vec<(String,String,String,String,String,String,String)>=sqlx::query_as("SELECT op_id,document_id,family,source_id,disposition,original_json,expected_json FROM move_metadata_escrow WHERE op_id=$1 ORDER BY document_id,family,source_id").bind(op).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    Ok(rows.into_iter().map(|r|serde_json::json!({"op_id":r.0,"document_id":r.1,"family":r.2,"source_id":r.3,"disposition":r.4,"original_json":r.5,"expected_json":r.6})).collect())
}
async fn association(
    conn: &mut AnyConnection,
    f: &InstalledFile,
    destination_space: &str,
) -> Result<Association, AppError> {
    let document_id = match f.kind.as_str() {
        "canonical" => {
            let row:Option<(String,String,String,Option<String>,Option<String>)>=sqlx::query_as("SELECT d.id,d.space_id,d.content_path,d.deleted_at,d.archived_at FROM documents d WHERE d.id=$1").bind(&f.subject_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
            match row {
                Some((id, space, path, None, None))
                    if space == destination_space && path == f.destination =>
                {
                    Some(id)
                }
                _ => None,
            }
        }
        "attachment" => {
            let row: Option<(String, String, String, i64)> = sqlx::query_as(
                "SELECT document_id,space_id,storage_path,size_bytes FROM attachments WHERE id=$1",
            )
            .bind(&f.subject_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
            match row {
                Some((id, space, path, size))
                    if space == destination_space
                        && path == f.destination
                        && size >= 0
                        && size as u64 == f.length =>
                {
                    Some(id)
                }
                _ => None,
            }
        }
        "drawing" => {
            let row: Option<(String, String, String)> = sqlx::query_as(
                "SELECT document_id,space_id,storage_path FROM excalidraw_files WHERE id=$1",
            )
            .bind(&f.subject_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
            match row {
                Some((id, space, path)) if space == destination_space && path == f.destination => {
                    Some(id)
                }
                _ => None,
            }
        }
        _ => return Err(conflict("unknown file subject family")),
    };
    let Some(id) = document_id else {
        return Ok(Association::Historical);
    };
    let owner: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT space_id,deleted_at,archived_at FROM documents WHERE id=$1")
            .bind(&id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    let slug: Option<String> = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
        .bind(destination_space)
        .fetch_optional(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if !matches!(owner,Some((space,None,None)) if space==destination_space)
        || !slug.is_some_and(|slug| {
            !slug.is_empty()
                && !slug.contains('/')
                && f.destination.starts_with(&format!("spaces/{slug}/"))
        })
    {
        return Ok(Association::Historical);
    }
    Ok(Association::Current { document_id: id })
}
/// The caller supplies a quiescent verified snapshot connection. Receipt absence
/// distinguishes rollback/preparation; SQL errors never imply receipt absence.
#[cfg(target_os = "linux")]
pub(crate) async fn inspect(
    root: &Path,
    conn: &mut AnyConnection,
    op: &str,
) -> Result<Inspection, AppError> {
    let original = backend::read_manifest(root, op).map_err(conflict)?;
    let m = manifest(&original)?;
    if m.op_id != op {
        return Err(conflict("operation directory/manifest changed"));
    }
    let logical = logical_receipt(conn, op).await?;
    let escrow_rows = escrow(conn, op).await?;
    let receipt_bytes = optional_receipt(root, op)?;
    let state = if let Some(receipt) = &logical {
        let sql_manifest = manifest(
            receipt["manifest_json"]
                .as_str()
                .ok_or_else(|| conflict("invalid SQL manifest"))?
                .as_bytes(),
        )?;
        if receipt_bytes.as_deref().map(sealed).transpose()?.as_ref() != Some(receipt)
            || sql_manifest != m
            || verify_sealed_receipt(root, receipt)? != m
        {
            return Err(conflict("SQL/sealed/filesystem relocation mismatch"));
        }
        verify_escrow(conn, receipt).await?;
        CommitState::Committed
    } else {
        if !escrow_rows.is_empty() {
            return Err(conflict("escrow without committed operation"));
        }
        if let Some(raw) = &receipt_bytes {
            let receipt = sealed(raw)?;
            if manifest(receipt["manifest_json"].as_str().unwrap().as_bytes())? != m
                || receipt_manifest(&receipt)? != m
            {
                return Err(conflict("uncommitted seal/manifest mismatch"));
            }
        }
        CommitState::PreparedUncommitted
    };
    let mut paths = Vec::new();
    let mut associations = Vec::new();
    for (index, f) in m.files.iter().enumerate() {
        for (role, relative, identity, required) in [
            (
                "destination",
                f.destination.clone(),
                Some((f.destination_device, f.destination_inode)),
                state == CommitState::Committed,
            ),
            (
                "source",
                f.source.clone(),
                Some((f.source_device, f.source_inode)),
                false,
            ),
            ("payload", format!("{NS}/{op}/payload-{index}"), None, true),
        ] {
            let observed = backend::inspect_path(root, &relative).map_err(conflict)?;
            if required
                && !observed
                    .as_ref()
                    .is_some_and(|o| o.length == f.length && o.sha256 == f.sha256)
            {
                return Err(conflict(format!(
                    "required {role} bytes missing or changed"
                )));
            }
            paths.push(PathRequirement {
                subject_id: f.subject_id.clone(),
                role: role.into(),
                relative,
                original_identity: identity,
                expected_length: f.length,
                expected_sha256: f.sha256.clone(),
                required,
                observed,
            });
        }
        let association = match &logical {
            Some(receipt) => {
                association(
                    conn,
                    f,
                    receipt["destination_space_id"]
                        .as_str()
                        .ok_or_else(|| conflict("invalid destination space"))?,
                )
                .await?
            }
            None => Association::Historical,
        };
        associations.push((f.kind.clone(), f.subject_id.clone(), association));
    }
    if backend::read_manifest(root, op).map_err(conflict)? != original
        || optional_receipt(root, op)? != receipt_bytes
        || logical_receipt(conn, op).await? != logical
        || escrow(conn, op).await? != escrow_rows
    {
        return Err(conflict("relocation evidence changed during inspection"));
    }
    // Reobserve each held/path observation after SQL waits; never grant on a stale read.
    for p in &paths {
        if backend::inspect_path(root, &p.relative).map_err(conflict)? != p.observed {
            return Err(conflict("relocation path changed during inspection"));
        }
    }
    let mut current = Vec::new();
    for f in &m.files {
        let a = match &logical {
            Some(r) => association(conn, f, r["destination_space_id"].as_str().unwrap()).await?,
            None => Association::Historical,
        };
        current.push((f.kind.clone(), f.subject_id.clone(), a));
    }
    if current != associations
        || logical_receipt(conn, op).await? != logical
        || escrow(conn, op).await? != escrow_rows
    {
        return Err(conflict(
            "association or SQL evidence changed during inspection",
        ));
    }
    for p in &paths {
        if backend::inspect_path(root, &p.relative).map_err(conflict)? != p.observed {
            return Err(conflict("relocation path changed after final SQL waits"));
        }
    }
    if backend::read_manifest(root, op).map_err(conflict)? != original
        || optional_receipt(root, op)? != receipt_bytes
    {
        return Err(conflict(
            "immutable envelopes changed after final SQL waits",
        ));
    }
    Ok(Inspection {
        state,
        manifest: m,
        manifest_bytes: original,
        logical_receipt: logical,
        receipt_bytes,
        escrow_rows,
        associations,
        paths,
    })
}
#[cfg(not(target_os = "linux"))]
pub(crate) async fn inspect(
    _: &Path,
    _: &mut AnyConnection,
    _: &str,
) -> Result<Inspection, AppError> {
    Err(conflict(
        "Relocation restoration inspection requires Linux descriptor backend",
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    struct Fixture {
        root: std::path::PathBuf,
        pool: sqlx::AnyPool,
        manifest: PreparedRelocation,
        receipt: serde_json::Value,
        escrow: serde_json::Value,
    }
    impl Fixture {
        async fn new(kind: &str) -> Self {
            sqlx::any::install_default_drivers();
            let root = std::env::temp_dir()
                .join(format!("relocation-inspection-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(root.join("spaces/source/docs")).unwrap();
            std::fs::write(
                root.join("spaces/source/docs/page.md"),
                b"original committed bytes",
            )
            .unwrap();
            let pool = sqlx::any::AnyPoolOptions::new()
                .max_connections(1)
                .connect("sqlite::memory:")
                .await
                .unwrap();
            let mut conn = pool.acquire().await.unwrap();
            sqlx::raw_sql("CREATE TABLE spaces(id TEXT PRIMARY KEY,slug TEXT NOT NULL);CREATE TABLE documents(id TEXT PRIMARY KEY,space_id TEXT,content_path TEXT,deleted_at TEXT,archived_at TEXT);CREATE TABLE attachments(id TEXT PRIMARY KEY,document_id TEXT,space_id TEXT,storage_path TEXT,size_bytes INTEGER);CREATE TABLE excalidraw_files(id TEXT PRIMARY KEY,document_id TEXT,space_id TEXT,storage_path TEXT);CREATE TABLE document_access_policies(document_id TEXT PRIMARY KEY,mode TEXT);").execute(&mut *conn).await.unwrap();
            sqlx::raw_sql(include_str!("../../migrations/0039_move_relocation.sql"))
                .execute(&mut *conn)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO spaces VALUES ('source','source'),('destination','destination')",
            )
            .execute(&mut *conn)
            .await
            .unwrap();
            let subject = if kind == "canonical" { "page" } else { "asset" };
            let folder=match kind {"canonical"=>"docs/revisions","attachment"=>"attachments/page","drawing"=>"excalidraw/page",_=>unreachable!()};
            let destination = format!("spaces/destination/{folder}/{}.bin", uuid::Uuid::new_v4());
            let manifest = prepare(
                &root,
                &uuid::Uuid::new_v4().to_string(),
                &"a".repeat(64),
                &"b".repeat(64),
                &[FilePlan {
                    subject_id: subject.into(),
                    kind: kind.into(),
                    source: "spaces/source/docs/page.md".into(),
                    destination: destination.clone(),
                    sha256: None,
                    maximum: MAX_FILE,
                }],
            )
            .unwrap();
            sqlx::query("INSERT INTO documents VALUES ('page','destination',?,NULL,NULL)")
                .bind(if kind == "canonical" {
                    &destination
                } else {
                    "spaces/destination/docs/owner.md"
                })
                .execute(&mut *conn)
                .await
                .unwrap();
            sqlx::query("INSERT INTO document_access_policies VALUES ('page','restricted')")
                .execute(&mut *conn)
                .await
                .unwrap();
            if kind == "attachment" {
                sqlx::query("INSERT INTO attachments VALUES ('asset','page','destination',?,?)")
                    .bind(&destination)
                    .bind(manifest.files[0].length as i64)
                    .execute(&mut *conn)
                    .await
                    .unwrap();
            }
            if kind == "drawing" {
                sqlx::query("INSERT INTO excalidraw_files VALUES ('asset','page','destination',?)")
                    .bind(&destination)
                    .execute(&mut *conn)
                    .await
                    .unwrap();
            }
            let escrow = serde_json::json!({"op_id":manifest.op_id,"document_id":"page","family":"page_viewer_grant","source_id":"viewer","disposition":"preserve","original_json":"{\"grant\":\"private\"}","expected_json":"{}"});
            let expected = escrow_binding(&[escrow.clone()]).unwrap();
            let receipt = serde_json::json!({"op_id":manifest.op_id,"preview_id":uuid::Uuid::new_v4().to_string(),"actor_id":"actor","source_space_id":"source","destination_space_id":"destination","request_fingerprint":manifest.request_fingerprint,"source_fingerprint":manifest.source_fingerprint,"manifest_json":serde_json::to_string(&manifest).unwrap(),"expected_state_json":expected.to_string(),"created_at":"2026-10-09T20:00:00Z"});
            drop(conn);
            Self {
                root,
                pool,
                manifest,
                receipt,
                escrow,
            }
        }
        async fn record(&self, commit: bool) {
            let mut tx = self.pool.begin().await.unwrap();
            seal_receipt(&self.root, &self.receipt).unwrap();
            sqlx::query("INSERT INTO move_commit_receipts(op_id,preview_id,actor_id,source_space_id,destination_space_id,request_fingerprint,source_fingerprint,manifest_json,expected_state_json,created_at) VALUES (?,?,?,?,?,?,?,?,?,?)").bind(self.receipt["op_id"].as_str().unwrap()).bind(self.receipt["preview_id"].as_str().unwrap()).bind(self.receipt["actor_id"].as_str().unwrap()).bind(self.receipt["source_space_id"].as_str().unwrap()).bind(self.receipt["destination_space_id"].as_str().unwrap()).bind(self.receipt["request_fingerprint"].as_str().unwrap()).bind(self.receipt["source_fingerprint"].as_str().unwrap()).bind(self.receipt["manifest_json"].as_str().unwrap()).bind(self.receipt["expected_state_json"].as_str().unwrap()).bind(self.receipt["created_at"].as_str().unwrap()).execute(&mut *tx).await.unwrap();
            sqlx::query("INSERT INTO move_metadata_escrow VALUES (?,?,?,?,?,?,?)")
                .bind(self.escrow["op_id"].as_str().unwrap())
                .bind(self.escrow["document_id"].as_str().unwrap())
                .bind(self.escrow["family"].as_str().unwrap())
                .bind(self.escrow["source_id"].as_str().unwrap())
                .bind(self.escrow["disposition"].as_str().unwrap())
                .bind(self.escrow["original_json"].as_str().unwrap())
                .bind(self.escrow["expected_json"].as_str().unwrap())
                .execute(&mut *tx)
                .await
                .unwrap();
            if commit {
                tx.commit().await.unwrap()
            } else {
                tx.rollback().await.unwrap()
            }
        }
        async fn inspect(&self) -> Result<Inspection, AppError> {
            let mut conn = self.pool.acquire().await.unwrap();
            inspect(&self.root, &mut conn, &self.manifest.op_id).await
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
    #[tokio::test]
    async fn real_committed_producers_preserve_private_current_canonical_attachment_and_drawing() {
        for kind in ["canonical", "attachment", "drawing"] {
            let f = Fixture::new(kind).await;
            f.record(true).await;
            let before = std::fs::read(
                f.root
                    .join(NS)
                    .join(&f.manifest.op_id)
                    .join("manifest.json"),
            )
            .unwrap();
            let inspected = f.inspect().await.unwrap();
            assert_eq!(inspected.state, CommitState::Committed);
            assert_eq!(inspected.logical_receipt, Some(f.receipt.clone()));
            assert_eq!(inspected.escrow_rows, vec![f.escrow.clone()]);
            assert_eq!(inspected.manifest_bytes, before);
            assert_eq!(
                inspected.associations[0].2,
                Association::Current {
                    document_id: "page".into()
                }
            );
            assert!(
                inspected
                    .paths
                    .iter()
                    .find(|p| p.role == "destination")
                    .unwrap()
                    .required
            );
            let mode: String = sqlx::query_scalar(
                "SELECT mode FROM document_access_policies WHERE document_id='page'",
            )
            .fetch_one(&f.pool)
            .await
            .unwrap();
            assert_eq!(mode, "restricted");
            assert_eq!(
                std::fs::read(
                    f.root
                        .join(NS)
                        .join(&f.manifest.op_id)
                        .join("manifest.json")
                )
                .unwrap(),
                before
            );
        }
    }
    #[tokio::test]
    async fn real_sql_rollback_retains_seal_and_prepared_bytes_without_commit_or_current_authority()
    {
        let f = Fixture::new("canonical").await;
        f.record(false).await;
        let inspected = f.inspect().await.unwrap();
        assert_eq!(inspected.state, CommitState::PreparedUncommitted);
        assert!(inspected.logical_receipt.is_none());
        assert!(inspected.receipt_bytes.is_some());
        assert!(inspected.escrow_rows.is_empty());
        assert_eq!(inspected.associations[0].2, Association::Historical);
        assert!(
            !inspected
                .paths
                .iter()
                .find(|p| p.role == "destination")
                .unwrap()
                .required
        );
    }
    #[tokio::test]
    async fn superseded_deleted_and_changed_source_are_history_without_repointing() {
        let f = Fixture::new("canonical").await;
        f.record(true).await;
        sqlx::query("UPDATE documents SET content_path='spaces/destination/docs/new.md',deleted_at='deleted' WHERE id='page'").execute(&f.pool).await.unwrap();
        std::fs::write(
            f.root.join(&f.manifest.files[0].source),
            b"legitimate new source",
        )
        .unwrap();
        let inspected = f.inspect().await.unwrap();
        assert_eq!(inspected.associations[0].2, Association::Historical);
        let source = inspected.paths.iter().find(|p| p.role == "source").unwrap();
        assert!(!source.required);
        assert_ne!(
            source.observed.as_ref().unwrap().sha256,
            source.expected_sha256
        );
        let path: String = sqlx::query_scalar("SELECT content_path FROM documents WHERE id='page'")
            .fetch_one(&f.pool)
            .await
            .unwrap();
        assert_eq!(path, "spaces/destination/docs/new.md");
        std::fs::remove_file(f.root.join(&f.manifest.files[0].source)).unwrap();
        assert!(f
            .inspect()
            .await
            .unwrap()
            .paths
            .iter()
            .find(|p| p.role == "source")
            .unwrap()
            .observed
            .is_none());
    }
    #[tokio::test]
    async fn missing_historical_destination_and_changed_payload_block_even_verified_receipts() {
        let f = Fixture::new("canonical").await;
        f.record(true).await;
        sqlx::query("UPDATE move_commit_receipts SET verified_at='verified'")
            .execute(&f.pool)
            .await
            .unwrap();
        let dest = f.root.join(&f.manifest.files[0].destination);
        std::fs::remove_file(&dest).unwrap();
        assert!(f.inspect().await.is_err());
        std::fs::write(&dest, b"original committed bytes").unwrap();
        std::fs::write(
            f.root.join(NS).join(&f.manifest.op_id).join("payload-0"),
            b"changed",
        )
        .unwrap();
        assert!(f.inspect().await.is_err());
    }
    #[tokio::test]
    async fn escrow_and_full_sql_tuple_changes_reject_sealed_commit() {
        let f = Fixture::new("canonical").await;
        f.record(true).await;
        sqlx::query("UPDATE move_metadata_escrow SET original_json='changed'")
            .execute(&f.pool)
            .await
            .unwrap();
        assert!(f.inspect().await.is_err());
        sqlx::query("UPDATE move_metadata_escrow SET original_json=?")
            .bind(f.escrow["original_json"].as_str().unwrap())
            .execute(&f.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE move_commit_receipts SET actor_id='different'")
            .execute(&f.pool)
            .await
            .unwrap();
        assert!(f.inspect().await.is_err());
    }
    #[tokio::test]
    async fn strict_manifest_duplicate_unknown_and_ambiguous_subjects_reject() {
        let f = Fixture::new("canonical").await;
        let raw = std::fs::read(
            f.root
                .join(NS)
                .join(&f.manifest.op_id)
                .join("manifest.json"),
        )
        .unwrap();
        let text = String::from_utf8(raw.clone()).unwrap();
        let duplicate = text.replacen("\"version\":1", "\"version\":1,\"version\":1", 1);
        assert!(manifest(duplicate.as_bytes()).is_err());
        let mut json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        json["files"][0]["unknown"] = true.into();
        assert!(manifest(&serde_json::to_vec(&json).unwrap()).is_err());
        let mut duplicated = f.manifest.clone();
        duplicated.files.push(duplicated.files[0].clone());
        assert!(manifest(&serde_json::to_vec(&duplicated).unwrap()).is_err());
    }
    #[tokio::test]
    async fn equal_byte_replacement_is_observation_not_original_identity_authority() {
        use std::os::unix::fs::MetadataExt;
        let f = Fixture::new("canonical").await;
        f.record(true).await;
        let destination = f.root.join(&f.manifest.files[0].destination);
        std::fs::rename(&destination, f.root.join("old-destination")).unwrap();
        std::fs::write(&destination, b"original committed bytes").unwrap();
        let inspected = f.inspect().await.unwrap();
        let requirement = inspected
            .paths
            .iter()
            .find(|p| p.role == "destination")
            .unwrap();
        assert_eq!(
            requirement.original_identity,
            Some((
                f.manifest.files[0].destination_device,
                f.manifest.files[0].destination_inode
            ))
        );
        assert_ne!(
            requirement.observed.as_ref().unwrap().inode,
            f.manifest.files[0].destination_inode
        );
        assert_eq!(
            requirement.observed.as_ref().unwrap().inode,
            destination.metadata().unwrap().ino()
        );
    }
}

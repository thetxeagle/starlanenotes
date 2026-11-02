use super::attachment_upload::HeldVersionPath;
#[cfg(test)]
type VersionReadRace = (String, &'static str, Box<dyn FnOnce() + Send>);
#[cfg(test)]
static VERSION_READ_RACE: std::sync::Mutex<Option<VersionReadRace>> = std::sync::Mutex::new(None);
/// Guarded process-isolated test seam: mutate a real leaf after owner verification.
#[cfg(test)]
pub(crate) fn set_version_read_race(
    asset: String,
    phase: &'static str,
    action: Box<dyn FnOnce() + Send>,
) {
    let mut hook = VERSION_READ_RACE.lock().unwrap();
    assert!(hook.is_none());
    *hook = Some((asset, phase, action));
}
#[cfg(test)]
fn version_read_race(asset: &str, phase: &str) {
    let action = {
        let mut hook = VERSION_READ_RACE.lock().unwrap();
        if hook
            .as_ref()
            .is_some_and(|(a, p, _)| a == asset && *p == phase)
        {
            hook.take().map(|(_, _, action)| action)
        } else {
            None
        }
    };
    if let Some(action) = action {
        action();
    }
}
/// No installation grant. Constructed only from original owner custody and real move evidence.
pub(crate) struct CurrentVersionBinding {
    pub(crate) space_id: String,
    pub(crate) document_id: String,
    pub(crate) asset_id: String,
    pub(crate) storage_path: String,
    pub(crate) byte_length: i64,
    pub(crate) binary_sha256: String,
    root: std::path::PathBuf,
    held: Vec<HeldVersionPath>,
    proofs: Vec<crate::spaces::published_source::SourceProof>,
    requested_paths: std::collections::BTreeSet<String>,
}
impl CurrentVersionBinding {
    pub(crate) fn matches_requested_path(&self, path: &str) -> bool {
        self.requested_paths.contains(path)
    }
    pub(crate) fn fence_files(&self) -> Result<(), String> {
        for f in &self.held {
            f.fence(&self.root).map_err(|e| format!("{e:?}"))?;
        }
        super::restoration::fence(&self.root).map_err(|e| format!("{e:?}"))
    }
    pub(crate) fn append_sql_proofs(
        &mut self,
        into: &mut Vec<crate::spaces::published_source::SourceProof>,
    ) {
        into.append(&mut self.proofs);
    }
}
async fn current_sql_proof(
    conn: &mut AnyConnection,
    query: &str,
    args: &[&str],
) -> Result<crate::spaces::published_source::SourceProof, String> {
    Ok(crate::spaces::published_source::SourceProof {
        query: query.into(),
        parameters: args.iter().map(|v| v.to_string()).collect(),
        rows: crate::spaces::document_deletion::rows(conn, query, args)
            .await
            .map_err(|e| format!("{e:?}"))?,
    })
}
/// Unregistered assets retain ordinary download policy; absence itself is fenced.
pub(crate) async fn capture_current_asset(
    root: &FsPath,
    conn: &mut AnyConnection,
    asset: &str,
) -> Result<
    (
        Option<CurrentVersionBinding>,
        crate::spaces::published_source::SourceProof,
    ),
    String,
> {
    const EVIDENCE: &str = "SELECT (SELECT COUNT(*) FROM attachment_version_records WHERE version_id=$1 OR logical_attachment_id=$1)+(SELECT COUNT(*) FROM attachment_families WHERE id=$1 OR current_version_id=$1)+(SELECT COUNT(*) FROM attachment_version_changes WHERE expected_version_id=$1 OR new_version_id=$1) +(SELECT COUNT(*) FROM attachment_retirements WHERE attachment_id=$1) AS registry_evidence_count";
    let proof = current_sql_proof(conn, EVIDENCE, &[asset]).await?;
    if super::attachment_retirement::read(conn,asset).await?.is_some() { return Err("Attachment is retained after deletion".into()); }
    let binding = match version(conn, asset).await? {
        Some(v) => Some(capture_current_version(root, conn, &v).await?),
        None => {
            let evidence: i64 = sqlx::query_scalar(EVIDENCE)
                .bind(asset)
                .fetch_one(&mut *conn)
                .await
                .map_err(|e| e.to_string())?;
            if evidence != 0 {
                return Err("Partial immutable registry cannot become legacy download".into());
            }
            None
        }
    };
    Ok((binding, proof))
}
async fn capture_current_version(
    root: &FsPath,
    conn: &mut AnyConnection,
    v: &VersionRecord,
) -> Result<CurrentVersionBinding, String> {
    validated_version(conn, v).await?;
    let receipt = upload::receipt(conn, &v.upload_operation_id)
        .await?
        .ok_or("Original owner receipt absent")?;
    let row:Option<(String,String,String,String,String,i64,String,String,String,String)>=sqlx::query_as("SELECT a.document_id,a.space_id,a.filename,a.original_name,a.mime_type,a.size_bytes,a.storage_path,a.created_by,a.created_at,s.slug FROM attachments a JOIN documents d ON d.id=a.document_id AND d.space_id=a.space_id JOIN spaces s ON s.id=d.space_id WHERE a.id=$1 AND d.deleted_at IS NULL AND d.archived_at IS NULL")
        .bind(&v.version_id).fetch_optional(&mut*conn).await.map_err(|e|e.to_string())?;
    let a = row.ok_or("Current version association absent")?;
    if a.0 != v.original_document_id
        || a.2 != v.original_filename
        || a.3 != v.original_name
        || a.4 != v.mime_type
        || a.5 != v.byte_length
        || a.7 != v.uploader_id
        || a.8 != v.original_created_at
        || !a
            .6
            .starts_with(&format!("spaces/{}/attachments/{}/", a.9, a.0))
    {
        return Err("Current version original metadata differs".into());
    }
    let mut proofs = Vec::new();
    for (q, arg) in [
        (
            "SELECT * FROM attachment_retirements WHERE attachment_id=$1",
            v.version_id.as_str(),
        ),
        (
            "SELECT * FROM attachments WHERE id=$1",
            v.version_id.as_str(),
        ),
        (
            "SELECT * FROM attachment_version_records WHERE version_id=$1",
            v.version_id.as_str(),
        ),
        (
            "SELECT * FROM attachment_upload_receipts WHERE operation_id=$1",
            v.upload_operation_id.as_str(),
        ),
    ] {
        proofs.push(current_sql_proof(conn, q, &[arg]).await?);
    }
    if v.version_number > 1 {
        proofs.push(
            current_sql_proof(
                conn,
                "SELECT * FROM attachment_version_changes WHERE operation_id=$1",
                &[&v.upload_operation_id],
            )
            .await?,
        );
    }
    let rt = root.to_owned();
    let r = receipt.clone();
    let (mut held, mut identity) = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let base = format!(".attachment-upload-journal/{}", r.intent.attempt_id);
        let mut held = Vec::new();
        for name in [
            "request.json",
            "seal.json",
            "intent.json",
            "install-proof.json",
            "payload.part",
        ] {
            held.push(
                HeldVersionPath::open(&rt, &format!("{base}/{name}"))
                    .map_err(|e| format!("{e:?}"))?,
            );
        }
        let intent = held[2].bounded_bytes(16384).map_err(|e| format!("{e:?}"))?;
        let proof = held[3].bounded_bytes(16384).map_err(|e| format!("{e:?}"))?;
        let original = upload::validate_history(&intent, &proof, &r.intent.attempt_id)
            .map_err(|e| format!("{e:?}"))?;
        if original.intent != r.intent {
            return Err("Original physical intent differs".into());
        }
        held.push(
            HeldVersionPath::open(&rt, &r.intent.storage_path).map_err(|e| format!("{e:?}"))?,
        );
        upload::verify(&rt, &r, false).map_err(|e| format!("{e:?}"))?;
        #[cfg(test)]
        version_read_race(&r.intent.asset_id, "upload");
        for f in &held {
            f.fence(&rt).map_err(|e| format!("{e:?}"))?;
        }
        let identity = held.last().ok_or("Original source held path absent")?.captured_identity();
        Ok((held, identity))
    })
    .await
    .map_err(|e| format!("{e:?}"))??;
    let mut path = v.original_storage_path.clone();
    let mut space = v.original_space_id.clone();
    let mut visited = std::collections::BTreeSet::new();
    let mut envelope_budget = 0usize;
    while path != a.6 || space != a.1 {
        if visited.len() >= 16 || !visited.insert(path.clone()) {
            return Err("Current version relocation chain bound/cycle".into());
        }
        let alias:Option<(String,String,String,String)>=sqlx::query_as("SELECT asset_id,asset_kind,op_id,created_at FROM move_asset_path_aliases WHERE old_path=$1")
            .bind(&path).fetch_optional(&mut*conn).await.map_err(|e|e.to_string())?;
        let (asset, kind, op, created) = alias.ok_or("Current version relocation alias absent")?;
        if asset != v.version_id || kind != "attachment" {
            return Err("Current version alias owner differs".into());
        }
        let logical = super::relocation::logical_receipt(conn, &op)
            .await
            .map_err(|e| format!("{e:?}"))?
            .ok_or("Relocation receipt absent")?;
        let raw_manifest = logical["manifest_json"]
            .as_str()
            .ok_or("Relocation manifest absent")?;
        let raw_state = logical["expected_state_json"]
            .as_str()
            .ok_or("Relocation state absent")?;
        envelope_budget = envelope_budget
            .checked_add(raw_manifest.len() + raw_state.len())
            .ok_or("Relocation bound exceeded")?;
        if envelope_budget > 8_388_608 {
            return Err("Current relocation evidence budget exceeded".into());
        }
        if logical["source_space_id"] != space || logical["created_at"] != created {
            return Err("Relocation original alias/space differs".into());
        }
        let manifest: super::relocation::PreparedRelocation =
            serde_json::from_str(raw_manifest).map_err(|_| "Relocation manifest corrupt")?;
        let matching: Vec<_> = manifest
            .files
            .iter()
            .filter(|f| f.kind == "attachment" && f.subject_id == v.version_id && f.source == path)
            .collect();
        if matching.len() != 1 {
            return Err("Relocation asset binding ambiguous".into());
        }
        let f = (*matching[0]).clone();
        if !super::relocation::valid_manifest(&manifest)
            || f.sha256 != v.binary_sha256
            || f.length != v.byte_length as u64
        {
            return Err("Relocation original identity/digest differs".into());
        }
        let destination = logical["destination_space_id"]
            .as_str()
            .ok_or("Relocation destination absent")?
            .to_owned();
        let state: serde_json::Value =
            serde_json::from_str(raw_state).map_err(|_| "Relocation state corrupt")?;
        if !state["pages"].as_array().is_some_and(|pages| {
            pages
                .iter()
                .any(|p| p["id"] == v.original_document_id && p["space_id"] == destination)
        }) {
            return Err("Relocation did not bind original page".into());
        }
        super::relocation::verify_escrow(conn, &logical)
            .await
            .map_err(|e| format!("{e:?}"))?;
        for (q,args) in [("SELECT * FROM move_asset_path_aliases WHERE old_path=$1",vec![path.as_str()]),("SELECT * FROM move_commit_receipts WHERE op_id=$1",vec![op.as_str()]),("SELECT * FROM move_metadata_escrow WHERE op_id=$1 ORDER BY document_id,family,source_id",vec![op.as_str()])] {proofs.push(current_sql_proof(conn,q,&args).await?);}
        let rt = root.to_owned();
        let target = f.destination.clone();
        let expected = manifest.clone();
        let selected_attachment = f.clone();
        let prior_identity = identity;
        let lg = logical.clone();
        let operation = op.clone();
        #[cfg(test)]
        let asset_for_race = v.version_id.clone();
        let files = tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut files = Vec::new();
            for p in [
                format!(".move-relocation-journal/{operation}/manifest.json"),
                format!(".move-relocation-journal/{operation}/receipt.json"),
                target,
                selected_attachment.source.clone(),
            ] {
                files.push(HeldVersionPath::open(&rt, &p).map_err(|e| format!("{e:?}"))?);
            }
            let raw = files[0]
                .bounded_bytes(4_194_304)
                .map_err(|e| format!("{e:?}"))?;
            if raw.as_slice()
                != lg["manifest_json"]
                    .as_str()
                    .ok_or("Relocation raw manifest absent")?
                    .as_bytes()
            {
                return Err("Relocation original raw manifest differs".into());
            }
            if super::relocation::verify_sealed_receipt(&rt, &lg).map_err(|e| format!("{e:?}"))?
                != expected
            {
                return Err("Relocation sealed manifest differs".into());
            }
            super::relocation::verify_attachment_pair(&rt, &expected, &selected_attachment)
                .map_err(|e| format!("{e:?}"))?;
            if files[3].captured_identity() != prior_identity {
                return Err("Relocation source differs from prior held version".into());
            }
            #[cfg(test)]
            version_read_race(&asset_for_race, "relocation");
            for f in &files {
                f.fence(&rt).map_err(|e| format!("{e:?}"))?;
            }
            Ok(files)
        })
        .await
        .map_err(|e| format!("{e:?}"))??;
        identity = files[2].captured_identity();
        held.extend(files);
        path = f.destination;
        space = destination;
    }
    visited.insert(path);
    let result = CurrentVersionBinding {
        space_id: a.1,
        document_id: a.0,
        asset_id: v.version_id.clone(),
        storage_path: a.6,
        byte_length: v.byte_length,
        binary_sha256: v.binary_sha256.clone(),
        root: root.to_owned(),
        held,
        proofs,
        requested_paths: visited,
    };
    crate::spaces::published_source::verify_source(conn, &result.proofs)
        .await
        .map_err(|e| format!("{e:?}"))?;
    validated_version(conn, v).await?;
    result.fence_files()?;
    Ok(result)
}

/// Explicit authenticated history contract, including retained versions. No ordinary-body grant.
pub(crate) async fn capture_historical_version(root:&FsPath,conn:&mut AnyConnection,logical:&str,id:&str)->Result<(VersionRecord,CurrentVersionBinding,crate::spaces::published_source::SourceProof),String>{
    let h=history_page(conn,logical,0,1).await?.head;
    let v=version(conn,id).await?.ok_or("Version unavailable")?;
    if h.state!="active"||h.revision<=0||v.logical_attachment_id!=h.id||v.original_document_id!=h.document_id||v.version_number<1||v.version_number>h.revision{return Err("Version family differs".into())}
    let proof=current_sql_proof(conn,"SELECT * FROM attachment_families WHERE id=$1",&[logical]).await?;
    super::attachment_retirement::read(conn,id).await?;
    let binding=capture_current_version(root,conn,&v).await?;
    Ok((v,binding,proof))
}

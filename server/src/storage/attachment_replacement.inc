/// Original immutable family change, separate from the immutable upload receipt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplacementChange {
    pub expected: FamilyHead,
    pub version: VersionRecord,
    pub origin: String,
}
pub(crate) struct PreparedReplacement {
    change: ReplacementChange,
    receipt: Receipt,
    old: VersionRecord,
    prior_binding: CurrentVersionBinding,
}
pub(crate) async fn require_replacement_target(
    root: &FsPath,
    conn: &mut AnyConnection,
    family_id: &str,
    expected_version: &str,
    expected_revision: i64,
    op: &str,
    actor: &str,
    origin: &str,
    space: &str,
    doc: &str,
) -> Result<(), String> {
    if let Some(c) = replacement_change(conn, op).await? {
        if c.expected.id != family_id
            || c.expected.current_version_id != expected_version
            || c.expected.revision != expected_revision
            || c.version.uploader_id != actor
            || c.origin != origin
            || c.version.original_space_id != space
            || c.expected.document_id != doc
        {
            return Err("Replacement operation target differs".into());
        }
    } else {
        if upload::receipt(conn, op).await?.is_some() {
            return Err("Existing upload has no replacement authority".into());
        }
        if super::attachment_retirement::read(conn,expected_version).await?.is_some() { return Err("Replacement head is retained after deletion".into()); }
    let h = family(conn, family_id)
            .await?
            .ok_or("Replacement family absent")?;
        if h.current_version_id != expected_version
            || h.revision != expected_revision
            || h.state != "active"
            || h.document_id != doc
        {
            return Err("Expected current family target differs".into());
        }
        let old=version(conn,expected_version).await?.ok_or("Current version absent")?;
        let binding=capture_current_version(root,conn,&old).await?;
        if binding.space_id!=space||binding.document_id!=doc{return Err("Current relocated replacement target differs".into())}
        binding.fence_files()?;
    }
    Ok(())
}
impl PreparedReplacement {
    pub(crate) fn result(&self) -> &ReplacementChange {
        &self.change
    }
}
fn replacement_version(r: &Receipt, raw: &str, family: &str, ordinal: i64) -> VersionRecord {
    let mut v = VersionRecord::from_receipt(r, raw);
    v.logical_attachment_id = family.to_owned();
    v.version_number = ordinal;
    v
}
async fn replacement_change(
    conn: &mut AnyConnection,
    op: &str,
) -> Result<Option<ReplacementChange>, String> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT substr(receipt_json,1,32769) FROM attachment_version_changes WHERE operation_id=$1",
    )
    .bind(op)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    let Some(raw) = raw else { return Ok(None) };
    if raw.len() > 32768 {
        return Err("Replacement receipt exceeds envelope bound".into());
    }
    let c: ReplacementChange =
        serde_json::from_str(&raw).map_err(|_| "Replacement receipt shape differs")?;
    let h = &c.expected;
    let v = &c.version;
    if h.revision <= 0
        || h.state != "active"
        || v.version_number
            != h.revision
                .checked_add(1)
                .ok_or("Family revision exhausted")?
        || v.logical_attachment_id != h.id
        || v.original_document_id != h.document_id
        || v.upload_operation_id != op
        || v.version_id == h.current_version_id
    {
        return Err("Replacement receipt identity differs".into());
    }
    let exact:Option<i64>=sqlx::query_scalar("SELECT 1 FROM attachment_version_changes WHERE operation_id=$1 AND family_id=$2 AND expected_version_id=$3 AND expected_revision=$4 AND new_version_id=$5 AND applied_revision=$6 AND actor_id=$7 AND origin=$8 AND space_id=$9 AND document_id=$10 AND upload_receipt_sha256=$11 AND changed_at=$12 AND receipt_json=$13")
        .bind(op).bind(&h.id).bind(&h.current_version_id).bind(h.revision).bind(&v.version_id).bind(v.version_number).bind(&v.uploader_id).bind(&c.origin).bind(&v.original_space_id).bind(&v.original_document_id).bind(&v.raw_receipt_sha256).bind(&v.original_created_at).bind(&raw)
        .fetch_optional(&mut*conn).await.map_err(|e|e.to_string())?;
    if exact.is_none() {
        return Err("Replacement scalar envelope differs".into());
    }
    let r = upload::receipt(conn, op)
        .await?
        .ok_or("Replacement original upload receipt absent")?;
    let original = receipt_raw(conn, &r).await?;
    if replacement_version(&r, &original, &h.id, v.version_number) != *v
        || r.intent.request.origin != c.origin
    {
        return Err("Replacement original upload binding differs".into());
    }
    Ok(Some(c))
}
async fn validated_version(conn: &mut AnyConnection, v: &VersionRecord) -> Result<(), String> {
    let r = upload::receipt(conn, &v.upload_operation_id)
        .await?
        .ok_or("Original version receipt absent")?;
    let raw = receipt_raw(conn, &r).await?;
    let expected = if v.version_number == 1 {
        VersionRecord::from_receipt(&r, &raw)
    } else {
        replacement_change(conn, &v.upload_operation_id)
            .await?
            .ok_or("Original family-change receipt absent")?
            .version
    };
    if expected != *v {
        return Err("Original version metadata differs".into());
    }
    Ok(())
}
/// Retry decisions are receipt-bound. Current family may have advanced since this operation.
pub(crate) async fn resolve_replacement(
    root: &FsPath,
    conn: &mut AnyConnection,
    r: &Receipt,
    family_id: &str,
    expected_version: &str,
    expected_revision: i64,
) -> Result<Option<ReplacementChange>, String> {
    let Some(c) = replacement_change(conn, &r.intent.request.operation_id).await? else {
        return Ok(None);
    };
    if c.expected.id != family_id
        || c.expected.current_version_id != expected_version
        || c.expected.revision != expected_revision
        || c.origin != r.intent.request.origin
        || replacement_version(
            r,
            &receipt_raw(conn, r).await?,
            family_id,
            c.version.version_number,
        ) != c.version
    {
        return Err("Replacement retry binding differs".into());
    }
    let current = family(conn, family_id)
        .await?
        .ok_or("Replacement family absent")?;
    if current.state != "active"
        || current.space_id != c.expected.space_id
        || current.document_id != c.expected.document_id
        || current.created_at != c.expected.created_at
        || current.revision < c.version.version_number
        || version(conn, &c.version.version_id).await?.as_ref() != Some(&c.version)
        || !upload::association(conn, r).await?
    {
        return Err("Replacement retry association differs".into());
    }
    let history = history_inventory(conn, family_id).await?;
    if history.head != current {
        return Err("Replacement retry current history changed".into());
    }
    replacement_prior_binding(root,conn, &c).await?.fence_files()?;
    Ok(Some(c))
}
/// Caller holds the ordinary current page/space mutation transaction.
pub(crate) async fn prepare_replacement(
    root: &FsPath,
    conn: &mut AnyConnection,
    r: &Receipt,
    family_id: &str,
    expected_version: &str,
    expected_revision: i64,
) -> Result<PreparedReplacement, String> {
    if replacement_change(conn, &r.intent.request.operation_id)
        .await?
        .is_some()
    {
        return Err("Replacement already has an immutable decision".into());
    }
    if super::attachment_retirement::read(conn,expected_version).await?.is_some() { return Err("Replacement head is retained after deletion".into()); }
    let h = family(conn, family_id)
        .await?
        .ok_or("Replacement family absent")?;
    if h.current_version_id != expected_version
        || h.revision != expected_revision
        || h.state != "active"
        || h.document_id != r.intent.request.document_id
    {
        return Err("Expected current family head differs".into());
    }
    let old = version(conn, expected_version)
        .await?
        .ok_or("Current immutable version absent")?;
    validated_version(conn, &old).await?;
    if old.logical_attachment_id != h.id || old.version_number != h.revision {
        return Err("Current version ownership differs".into());
    }
    if version(conn, &r.intent.asset_id).await?.is_some() {
        return Err("New immutable version already has an owner".into());
    }
    let old_receipt = upload::receipt(conn, &old.upload_operation_id)
        .await?
        .ok_or("Current upload receipt absent")?;
    let prior_binding=capture_current_version(root,conn,&old).await?;
    if prior_binding.space_id!=r.intent.request.space_id||prior_binding.document_id!=r.intent.request.document_id{return Err("Current relocated version differs from incoming target".into())}
    prior_binding.fence_files()?;
    if upload::receipt(conn,&old.upload_operation_id).await?.as_ref()!=Some(&old_receipt){return Err("Prior owner receipt changed".into())}
    if family(conn, family_id).await?.as_ref() != Some(&h)
        || version(conn, expected_version).await?.as_ref() != Some(&old)
    {
        return Err("Family ownership changed during preparation".into());
    }
    // Reuse full receipt/association/physical-owner validation, without publishing a singleton.
    let singleton = prepare_singleton(root, conn, r).await?;
    let ordinal = h
        .revision
        .checked_add(1)
        .ok_or("Family revision exhausted")?;
    let mut v = singleton.version;
    v.logical_attachment_id = h.id.clone();
    v.version_number = ordinal;
    Ok(PreparedReplacement {
        change: ReplacementChange {
            expected: h,
            version: v,
            origin: r.intent.request.origin.clone(),
        },
        receipt: r.clone(),
        old,
        prior_binding,
    })
}
pub(crate) async fn publish_replacement(
    conn: &mut AnyConnection,
    p: &PreparedReplacement,
) -> Result<(), String> {
    let c = &p.change;
    let h = &c.expected;
    let v = &c.version;
    if version(conn, &p.old.version_id).await?.as_ref() != Some(&p.old) {
        return Err("Prior immutable version changed".into());
    }
    validated_version(conn, &p.old).await?;
    p.prior_binding.fence_files()?;
    if replacement_version(
        &p.receipt,
        &receipt_raw(conn, &p.receipt).await?,
        &h.id,
        v.version_number,
    ) != *v
        || !upload::association(conn, &p.receipt).await?
    {
        return Err("Replacement source changed before publication".into());
    }
    if sqlx::query("UPDATE attachment_families SET current_version_id=$8,revision=$9 WHERE id=$1 AND space_id=$2 AND document_id=$3 AND current_version_id=$4 AND revision=$5 AND state=$6 AND created_at=$7")
        .bind(&h.id).bind(&h.space_id).bind(&h.document_id).bind(&h.current_version_id).bind(h.revision).bind(&h.state).bind(&h.created_at).bind(&v.version_id).bind(v.version_number)
        .execute(&mut*conn).await.map_err(|e|e.to_string())?.rows_affected()!=1{return Err("Expected current family CAS failed".into());}
    sqlx::query("INSERT INTO attachment_version_records(version_id,logical_attachment_id,version_number,original_space_id,original_document_id,uploader_id,original_name,mime_type,byte_length,binary_sha256,original_created_at,original_storage_path,original_filename,upload_operation_id,upload_attempt_id,raw_receipt_sha256,intent_sha256,install_proof_sha256,provenance) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19)")
        .bind(&v.version_id).bind(&v.logical_attachment_id).bind(v.version_number).bind(&v.original_space_id).bind(&v.original_document_id).bind(&v.uploader_id).bind(&v.original_name).bind(&v.mime_type).bind(v.byte_length).bind(&v.binary_sha256).bind(&v.original_created_at).bind(&v.original_storage_path).bind(&v.original_filename).bind(&v.upload_operation_id).bind(&v.upload_attempt_id).bind(&v.raw_receipt_sha256).bind(&v.intent_sha256).bind(&v.install_proof_sha256).bind(&v.provenance)
        .execute(&mut*conn).await.map_err(|e|e.to_string())?;
    let raw = serde_json::to_string(c).map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO attachment_version_changes(operation_id,family_id,expected_version_id,expected_revision,new_version_id,applied_revision,actor_id,origin,space_id,document_id,upload_receipt_sha256,changed_at,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
        .bind(&v.upload_operation_id).bind(&h.id).bind(&h.current_version_id).bind(h.revision).bind(&v.version_id).bind(v.version_number).bind(&v.uploader_id).bind(&c.origin).bind(&v.original_space_id).bind(&v.original_document_id).bind(&v.raw_receipt_sha256).bind(&v.original_created_at).bind(raw)
        .execute(&mut*conn).await.map_err(|e|e.to_string())?;
    if resolve_replacement(&p.prior_binding.root,conn, &p.receipt, &h.id, &h.current_version_id, h.revision)
        .await?
        .as_ref()
        != Some(c)
    {
        return Err("Replacement publication evidence differs".into());
    }
    Ok(())
}
/// Capture exact prior owner evidence before the route's final policy await and sync fence.
pub(crate) async fn replacement_prior_binding(
    root: &FsPath,
    conn: &mut AnyConnection,
    c: &ReplacementChange,
) -> Result<CurrentVersionBinding, String> {
    let old = version(conn, &c.expected.current_version_id)
        .await?
        .ok_or("Prior immutable version absent")?;
    validated_version(conn, &old).await?;
    if old.logical_attachment_id != c.expected.id
        || old.version_number != c.expected.revision
        || old.original_document_id != c.expected.document_id
    {
        return Err("Prior immutable version binding differs".into());
    }
    capture_current_version(root,conn,&old).await
}

/// Completed operation retry under its current relocated association. It cannot mint an upload.
pub(crate) struct CompletedReplacementRetry {
    change:ReplacementChange,
    receipt:Receipt,
    current:CurrentVersionBinding,
    predecessor:CurrentVersionBinding,
    proofs:Vec<crate::spaces::published_source::SourceProof>,
}
pub(crate) async fn capture_completed_replacement_retry(root:&FsPath,conn:&mut AnyConnection,family_id:&str,expected_version:&str,expected_revision:i64,op:&str,actor:&str,origin:&str,space:&str,doc:&str)->Result<Option<CompletedReplacementRetry>,String>{
    let Some(change)=replacement_change(conn,op).await? else{return Ok(None)};
    if change.expected.id!=family_id||change.expected.current_version_id!=expected_version||change.expected.revision!=expected_revision||change.version.uploader_id!=actor||change.origin!=origin||change.expected.document_id!=doc{return Err("Replacement operation target differs".into())}
    let receipt=upload::receipt(conn,op).await?.ok_or("Original replacement upload receipt absent")?;
    let (version,mut current,family_proof)=capture_historical_version(root,conn,family_id,&change.version.version_id).await?;
    if version!=change.version||current.space_id!=space||current.document_id!=doc{return Err("Replacement retry current page association differs".into())}
    let mut predecessor=replacement_prior_binding(root,conn,&change).await?;
    if predecessor.space_id!=space||predecessor.document_id!=doc{return Err("Replacement predecessor current page differs".into())}
    let mut proofs=vec![family_proof,current_sql_proof(conn,"SELECT * FROM attachment_version_changes WHERE operation_id=$1",&[op]).await?];
    current.append_sql_proofs(&mut proofs);predecessor.append_sql_proofs(&mut proofs);
    current.fence_files()?;predecessor.fence_files()?;
    Ok(Some(CompletedReplacementRetry{change,receipt,current,predecessor,proofs}))
}
impl CompletedReplacementRetry {
    pub(crate) fn validate_body(&self,name:&str,mime:&str,length:u64,sha256:&str)->Result<(),String>{
        let i=&self.receipt.intent;
        if name!=i.request.original_name||mime!=i.request.mime_type||i.byte_length<0||length!=i.byte_length as u64||sha256!=i.binary_sha256{return Err("Replacement retry original bytes or metadata differ".into())}
        Ok(())
    }
    pub(crate) async fn verify_sql(&self,conn:&mut AnyConnection)->Result<(),String>{
        crate::spaces::published_source::verify_source(conn,&self.proofs).await.map_err(|e|format!("{e:?}"))?;
        if replacement_change(conn,&self.receipt.intent.request.operation_id).await?.as_ref()!=Some(&self.change)||upload::receipt(conn,&self.receipt.intent.request.operation_id).await?.as_ref()!=Some(&self.receipt){return Err("Replacement retry original decision changed".into())}
        Ok(())
    }
    pub(crate) fn draft_scope(&self)->Option<&super::attachment_upload::DraftUploadScope>{self.receipt.intent.request.draft_scope.as_ref()}
    pub(crate) fn finish(self)->Result<ReplacementChange,String>{self.current.fence_files()?;self.predecessor.fence_files()?;Ok(self.change)}
}

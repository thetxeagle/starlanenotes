// Private publication of one owner-certified marked destination. Included in linux.
// Only fresh application startup may register the resulting committed history.
pub(crate) struct SealedPublication {
    destination: PlannedDestination,
    prior: Option<ValidatedHistory>,
    admission_source: Option<admission::ClassifiedAdmissionSource>,
    completed_source:Option<crate::storage::attachment_upload::admission::committed_erasure::accounting::capture::ClassifiedCompletedBundle>,
    composite_source:Option<ClassifiedCompositeBundle>,
    historical_source:Option<ClassifiedHistoricalBundle>,
    historical_original:(Vec<HistorySqlRow>,Vec<(i64,String)>),
    historical_witness:Option<HistoricalPublicationWitness>,
    row: HistorySqlRow,
    previous_pointer: Vec<(i64,String)>,
    application_snapshot: Vec<(String,Vec<Vec<String>>)>,
    completion_relative: String,
    envelope: Vec<u8>,
    raw_completion: Vec<u8>,
    finalization_relative: String,
    raw_finalization: Vec<u8>,
    sql_spool: std::sync::Arc<std::sync::Mutex<PublicationSqlSpool>>,
}
pub(crate) struct CommittedPublication { sealed: SealedPublication }
pub(crate) struct BootReadyDestination { destination: PlannedDestination, generation_id: String, retained_sql_artifacts: std::path::PathBuf, historical_source:Option<ClassifiedHistoricalBundle>, historical_witness:Option<HistoricalPublicationWitness>, historical_original:(Vec<HistorySqlRow>,Vec<(i64,String)>) }
impl BootReadyDestination {
    pub(crate) fn path(&self)->&Path { self.destination.path() }
    pub(crate) fn fence(&self)->io::Result<()> { publication_retired_fence(&self.destination) }
    pub(crate) fn generation_id(&self)->&str { &self.generation_id }
    pub(crate) fn retained_sql_artifacts(&self)->&Path { &self.retained_sql_artifacts }
}
fn publication_json(value:&impl Serialize)->io::Result<Vec<u8>> {
    let raw=serde_json::to_vec(value).map_err(|_|fail("Cannot serialize publication record"))?;
    if raw.is_empty()||raw.len() as u64>MAX_LEDGER {return Err(fail("Publication record exceeds bound"));}
    Ok(raw)
}
fn publication_quote_identifier(s:&str)->String {format!("\"{}\"",s.replace('"',"\"\""))}
// Typed SQLite cells: full TEXT/BLOB bytes, INTEGER decimal, REAL IEEE bits, NULL.
// quote(TEXT) is deliberately never used: SQLite quote truncates at embedded NUL.
// Preserve schema, every non-custody row, and multiplicity; order is diagnostic only.
async fn publication_application_snapshot(conn:&mut sqlx::AnyConnection)->io::Result<Vec<(String,Vec<Vec<String>>)>> {
    let mut out=Vec::new();
    let schema:Vec<(String,String,String,String)>=sqlx::query_as("SELECT type,name,tbl_name,coalesce(sql,'') FROM sqlite_master ORDER BY type,name,tbl_name,sql")
        .fetch_all(&mut *conn).await.map_err(|_|fail("Cannot inspect publication schema"))?;
    out.push(("sqlite_master".into(),schema.iter().map(|(a,b,c,d)|vec![a.clone(),b.clone(),c.clone(),d.clone()]).collect()));
    for (_,name,_,_) in schema.iter().filter(|(kind,_,_,_)|kind=="table") {
        if matches!(name.as_str(),"storage_restore_generations"|"storage_restore_current") {continue;}
        let columns=sqlx::query(&format!("PRAGMA table_info({})",publication_quote_identifier(name)))
            .fetch_all(&mut *conn).await.map_err(|_|fail("Cannot inspect publication columns"))?;
        let names:Vec<String>=columns.iter().map(|r|r.try_get::<String,_>("name"))
            .collect::<Result<_,_>>().map_err(|_|fail("Invalid publication column"))?;
        if names.is_empty(){return Err(fail("Empty publication table"));}
        let expressions:Vec<String>=names.iter().map(|n|{let n=publication_quote_identifier(n);
            format!("typeof({n}), CASE WHEN typeof({n}) IN ('text','blob') THEN hex(CAST({n} AS BLOB)) ELSE '' END, CASE WHEN typeof({n})='integer' THEN {n} ELSE 0 END, CASE WHEN typeof({n})='real' THEN {n} ELSE 0.0 END")}).collect();
        let q=format!("SELECT {} FROM {}",expressions.join(","),publication_quote_identifier(name));
        let rows=sqlx::query(&q).fetch_all(&mut *conn).await.map_err(|e|io::Error::new(io::ErrorKind::InvalidData,format!("Cannot inspect publication table {name}: {e}")))?;
        let mut values=Vec::new();
        for row in rows {
            let mut cells=Vec::new();
            for i in 0..names.len() {
                let kind:String=row.try_get(i*4).map_err(|_|fail("Invalid SQL cell type"))?;
                let encoded=match kind.as_str() {
                    "null"=>String::new(),
                    "text"|"blob"=>row.try_get::<String,_>(i*4+1).map_err(|_|fail("Cannot read full SQL bytes"))?,
                    "integer"=>row.try_get::<i64,_>(i*4+2).map_err(|_|fail("Cannot read SQL integer"))?.to_string(),
                    "real"=>format!("{:016x}",row.try_get::<f64,_>(i*4+3).map_err(|_|fail("Cannot read SQL REAL"))?.to_bits()),
                    _=>return Err(fail("Unexpected SQLite storage class")),
                };
                cells.push(format!("{kind}:{encoded}"));
            }
            values.push(cells);
        }
        values.sort();out.push((name.clone(),values));
    }
    let version:i64=sqlx::query_scalar("PRAGMA user_version").fetch_one(&mut *conn).await.map_err(|_|fail("Cannot inspect user version"))?;
    out.push(("user_version".into(),vec![vec![version.to_string()]]));
    Ok(out)
}
fn publication_derived(d:&PlannedDestination,prior:Option<&ValidatedHistory>,gid:&str)
    ->io::Result<(Ledger,Vec<DeniedReservation>,Vec<HistoryProof>)> {
    use crate::storage::restoration_plan::OperationClass;
    d.fence()?;
    for unresolved in d.plan.unresolved() {
        let allowed=match unresolved.reason.as_str() {
            "pending_without_original_install_proof_requires_explicit_protocol"=>unresolved.operation_id.as_ref()
                .is_some_and(|op|unresolved.namespace==".storage-journal"&&d.minted.contains_key(op)),
            "orphan_stage_preserved_without_authority"|"prior_scope_preserved_without_fresh_classification"=>true,
            _=>false,
        };
        if !allowed{return Err(fail("Publication retains unresolved owner evidence"));}
    }
    let mut scopes=Vec::new();let mut denied=std::collections::BTreeMap::new();let mut proofs=Vec::new();
    if let Some(prior)=prior {if let Some(e)=&prior.generations[&prior.current_id].envelope {
        for deny in &e.denied {denied.insert((deny.namespace.clone(),deny.op_id.clone()),deny.clone());}
    }}
    for planned in d.plan.scopes() {
        let key=(planned.namespace.clone(),planned.operation_id.clone());
        if matches!(planned.class,OperationClass::PreparedUncommitted|OperationClass::InertReservation|OperationClass::CommittedErased) {
            denied.insert(key,DeniedReservation{namespace:planned.namespace.clone(),op_id:planned.operation_id.clone()});continue;
        }
        if denied.contains_key(&key) {return Err(fail("Publication would grant denied operation"));}
        if let Some(mint)=d.minted.get(&planned.operation_id).filter(|_|planned.namespace==".storage-journal") {
            if planned.class!=OperationClass::CommittedPending||!planned.bindings.is_empty(){return Err(fail("Mint scope differs"));}
            let stable=format!(".storage-journal/{}/install-proof.json",planned.operation_id);
            let a=d.leaves.get(&stable).ok_or_else(||fail("Stable minted proof missing"))?;
            let b=d.leaves.get(&mint.proof_relative).ok_or_else(||fail("Temporary minted proof missing"))?;
            if identity(&a.held)?!=identity(&b.held)?||a.stamp.links!=2||a.bytes.sha256!=digest(&mint.proof_bytes)
                ||d.leaves.get(&mint.intent_relative).is_none_or(|f|f.stamp.links!=1||f.bytes.sha256!=digest(&mint.intent_bytes)) {
                return Err(fail("Mint provenance differs"));
            }
            scopes.push(mint.scope.clone());
            proofs.push(HistoryProof{namespace:key.0,op_id:key.1,
                manifest:HistoryArtifact{relative:format!(".storage-journal/{}/manifest.json",planned.operation_id),sha256:digest(planned.raw_manifest.as_ref().ok_or_else(||fail("Mint original manifest missing"))?)},
                evidence:HistoryArtifact{relative:stable,sha256:digest(&mint.proof_bytes)},
                mint_intent:Some(HistoryArtifact{relative:mint.intent_relative.clone(),sha256:digest(&mint.intent_bytes)})});continue;
        }
        let raw=planned.evidence.as_ref().ok_or_else(||fail("Classified publication proof missing"))?;
        let manifest=planned.raw_manifest.as_ref().ok_or_else(||fail("Classified publication manifest missing"))?;
        let base=format!("{}/{}/",planned.namespace,planned.operation_id);
        let manifest_name=if planned.namespace==".attachment-upload-journal" {"intent.json"} else {"manifest.json"};
        let path=format!("{base}{}",if planned.namespace==".move-relocation-journal"{"manifest.json"}else{"install-proof.json"});
        let held=d.leaves.get(&path).ok_or_else(||fail("Held owner proof missing"))?;
        if held.bytes.sha256!=digest(raw)||d.leaves.get(&format!("{base}{manifest_name}")).is_none_or(|f|f.bytes.sha256!=digest(manifest)) {
            return Err(fail("Owner proof bytes differ"));
        }
        let mut bindings=Vec::new();
        for b in &planned.bindings {
            let adopted=d.leaves.get(&b.relative).ok_or_else(||fail("Certified owner leaf missing"))?;
            if adopted.bytes.size_bytes!=b.length||adopted.bytes.sha256!=b.sha256{return Err(fail("Certified owner bytes differ"));}
            bindings.push(Binding{relative:b.relative.clone(),original_device:b.original_device,original_inode:b.original_inode,
                adopted_device:adopted.stamp.device,adopted_inode:adopted.stamp.inode,length:b.length,sha256:b.sha256.clone()});
        }
        scopes.push(Scope{namespace:key.0.clone(),op_id:key.1.clone(),evidence_sha256:digest(raw),bindings});
        let prior_mint=prior.and_then(|p|p.generations[&p.current_id].envelope.as_ref())
            .and_then(|e|e.proofs.iter().find(|p|p.namespace==key.0&&p.op_id==key.1)).and_then(|p|p.mint_intent.clone());
        proofs.push(HistoryProof{namespace:key.0,op_id:key.1,manifest:HistoryArtifact{relative:format!("{base}{manifest_name}"),sha256:digest(manifest)},
            evidence:HistoryArtifact{relative:path,sha256:digest(raw)},mint_intent:prior_mint});
    }
    if let Some(prior)=prior {for old in &prior.ledger().scopes {
        if let Some(new)=scopes.iter().find(|s|s.namespace==old.namespace&&s.op_id==old.op_id) {
            if old.evidence_sha256!=new.evidence_sha256{return Err(fail("Publication reinterprets prior evidence"));}
            for b in &new.bindings {let old=old.bindings.iter().find(|v|v.relative==b.relative).ok_or_else(||fail("Publication adds predecessor binding"))?;
                if (b.original_device,b.original_inode,b.length,&b.sha256)!=(old.original_device,old.original_inode,old.length,&old.sha256) {return Err(fail("Publication reinterprets original binding"));}
            }
        }
        if !scopes.iter().any(|s|s.namespace==old.namespace&&s.op_id==old.op_id) {
            denied.insert((old.namespace.clone(),old.op_id.clone()),DeniedReservation{namespace:old.namespace.clone(),op_id:old.op_id.clone()});
        }
    }}
    let runtime=Ledger{ historical_preservation:None, composite:None,completed_absence:None,admission:None, version:1,generation_id:gid.into(),backup_id:d.plan.backup_id().into(),manifest_sha256:d.plan.manifest_sha256().into(),
        database_sha256:d.plan.database_sha256().into(),root_device:d.root_stamp.device,root_inode:d.root_stamp.inode,scopes};
    validate(&runtime)?;
    Ok((runtime,denied.into_values().collect(),proofs))
}
/// No caller-authored ledger, row, deny list or proof references enter this transition.
pub(crate) async fn seal_publication(bundle:&crate::storage::backup_inventory::ValidatedBundle,d:PlannedDestination)->io::Result<SealedPublication> {
    seal_publication_admission(bundle,d,None).await
}
pub(crate) async fn seal_publication_admission(bundle:&crate::storage::backup_inventory::ValidatedBundle,mut d:PlannedDestination,source:Option<admission::ClassifiedAdmissionSource>)->io::Result<SealedPublication> {
    let certified=source.map(|s|admission_certify_destination(bundle,&d,s)).transpose()?;
    pending_pair(bundle,&d)?;
    let mut source=bundle.db.acquire().await.map_err(|_|fail("Cannot acquire held source DB"))?;
    use sqlx::Acquire;
    let mut read=source.begin().await.map_err(|_|fail("Cannot begin source read"))?;
    let prior=inspect_history(bundle,&mut read).await?;
    let historical=historical_certify_destination(bundle,&d,&mut read).await?;
    if historical.is_some()&&certified.is_some(){return Err(fail("Historical publication cannot adopt generic admission owner"));}
    let composite=if historical.is_none(){composite_certify_destination(bundle,&d,&mut read).await?}else{None};
    let completed=if historical.is_none()&&composite.is_none(){completed_certify_destination(bundle,&d,&mut read).await?}else{None};
    let (old_rows,pointer)=history_rows(&mut read).await?;
    let snapshot=publication_application_snapshot(&mut read).await?;
    if let Some(c)=&certified {admission_source_sql(&mut read,&c.source).await?;}
    read.rollback().await.map_err(|_|fail("Cannot close source read"))?;
    pending_pair(bundle,&d)?;
    if prior.as_ref().map(|p|p.rows())!=if old_rows.is_empty(){None}else{Some(old_rows.as_slice())} {
        return Err(fail("Source history snapshot differs"));
    }
    let gid=uuid::Uuid::new_v4().to_string();
    let (mut runtime,denied,proofs)=publication_derived(&d,prior.as_ref(),&gid)?;
    if let Some(c)=&certified {runtime.version=2;runtime.admission=Some(c.roles.clone());validate(&runtime)?;c.source.fence()?;}
    if let Some(c)=&completed{runtime.version=3;runtime.completed_absence=Some(c.roles.clone());validate(&runtime)?;c.source.fence().map_err(|_|fail("Completed source changed before seal"))?;}
    if let Some(c)=&composite{runtime.version=4;runtime.composite=Some(c.roles.clone());validate(&runtime)?;c.source.fence()?;}
    if let Some(c)=&historical{runtime.version=5;runtime.historical_preservation=Some(c.roles.clone());validate(&runtime)?;c.source.fence()?;}
    let sql_spool=std::sync::Arc::new(std::sync::Mutex::new(PublicationSqlSpool::create(&d,&gid)?));
    let archives:Vec<HistoryLink>=prior.as_ref().map(|p|p.raw_generations().map(|(id,raw)|HistoryLink{generation_id:id.into(),ledger_sha256:digest(raw)}).collect()).unwrap_or_default();
    let parent=prior.as_ref().map(|p|HistoryLink{generation_id:p.current_id.clone(),ledger_sha256:digest(p.raw_ledger())});
    let base=format!("{NAMESPACE}/generations/{gid}");
    let marker_relative=format!("{base}/retired-marker");
    let runtime_hash=digest(&publication_json(&runtime)?);
    let intent=HistoryIntent{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),
        database_sha256:runtime.database_sha256.clone(),source_root:d.plan.source_root().clone(),
        target_root:crate::storage::backup_inventory::Identity{device:runtime.root_device,inode:runtime.root_inode},
        retired_marker:HistoryArtifact{relative:marker_relative,sha256:digest(&d.copied.marker_bytes)},runtime_sha256:runtime_hash.clone(),
        parent:parent.clone(),archives:archives.clone(),denied:denied.clone(),proofs:proofs.clone()};
    let raw_intent=publication_json(&intent)?;
    let created_at=chrono::Utc::now().to_rfc3339();
    let completion=HistoryCompletion{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),
        intent_sha256:digest(&raw_intent),runtime_sha256:runtime_hash,root_device:runtime.root_device.to_string(),root_inode:runtime.root_inode.to_string(),state:"committed".into(),created_at:created_at.clone()};
    let raw_completion=publication_json(&completion)?;
    let finalization_relative=format!("{base}/finalization.json");
    let finalization=HistoryFinalization{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),
        target_root:intent.target_root.clone(),original_marker:crate::storage::backup_inventory::Identity{device:d.marker_stamp.device,inode:d.marker_stamp.inode},
        retired_marker:intent.retired_marker.clone(),intent_sha256:digest(&raw_intent),completion_sha256:digest(&raw_completion)};
    let raw_finalization=publication_json(&finalization)?;
    let envelope=publication_json(&HistoryEnvelope{version:if historical.is_some(){4}else{3},runtime:runtime.clone(),intent_sha256:digest(&raw_intent),completion_sha256:digest(&raw_completion),
        finalization:Some(HistoryArtifact{relative:finalization_relative.clone(),sha256:digest(&raw_finalization)}),parent,archives,denied,proofs})?;
    history_parse(&envelope)?;
    // All described byte roles exist durably before current changes. Incomplete marker stays pinned.
    pending_new_directory(&mut d,&base)?;
    pending_new_file(&mut d,&format!("{base}/intent.json"),&raw_intent)?;
    publication_checkpoint("intent");
    let completion_relative=format!("{base}/completion.json");
    // Completion bytes are predetermined, but its durable role is absent until SQL is committed and closed.
    let staged=format!("{base}/envelope.pending");
    pending_new_file(&mut d,&staged,&envelope)?;
    if let Some(previous)=&prior {
        let archive=format!("{NAMESPACE}/archive/{}",previous.current_id);
        pending_new_directory(&mut d,&archive)?;
        publication_move(&mut d,&format!("{NAMESPACE}/current.json"),&format!("{archive}/ledger.json"))?;
        publication_checkpoint("archive");
    }
    publication_move(&mut d,&staged,&format!("{NAMESPACE}/current.json"))?;
    publication_checkpoint("current");
    let row=HistorySqlRow{generation_id:gid,backup_id:runtime.backup_id,manifest_sha256:runtime.manifest_sha256,database_sha256:runtime.database_sha256,
        ledger_sha256:digest(&envelope),root_device:runtime.root_device.to_string(),root_inode:runtime.root_inode.to_string(),state:"committed".into(),created_at};
    pending_pair(bundle,&d)?;
    Ok(SealedPublication{destination:d,admission_source:certified.map(|c|c.source),completed_source:completed.map(|c|c.source),composite_source:composite.map(|c|c.source),historical_source:historical.map(|c|c.source),historical_original:(old_rows,pointer.clone()),historical_witness:None,prior,row,previous_pointer:pointer,application_snapshot:snapshot,completion_relative,envelope,raw_completion,finalization_relative,raw_finalization,sql_spool})
}
// No-clobber renames of exactly held certified leaves; only own rename ctime and
// touched parents are recertified. Source names cannot silently disappear/reappear.
fn publication_move(d:&mut PlannedDestination,source:&str,target:&str)->io::Result<()> {
    d.fence()?;
    if d.leaves.contains_key(target){return Err(fail("Publication role exists"));}
    let before=d.leaves.get(source).ok_or_else(||fail("Publication source missing"))?;
    let old=before.stamp.clone();let expected=before.bytes.clone();
    let (a,an)=planned_parent(d,source)?;let(b,bn)=planned_parent(d,target)?;
    if exists(&b,&bn)?{return Err(fail("Publication target appeared"));}
    let an=CString::new(an).map_err(|_|fail("Invalid publication source"))?;
    let bn=CString::new(bn).map_err(|_|fail("Invalid publication target"))?;
    if unsafe{libc::syscall(libc::SYS_renameat2,a.as_raw_fd(),an.as_ptr(),b.as_raw_fd(),bn.as_ptr(),libc::RENAME_NOREPLACE)}!=0{return Err(io::Error::last_os_error());}
    a.sync_all()?;b.sync_all()?;
    let moved=open_at(&b,bn.to_str().map_err(|_|fail("Invalid publication name"))?,libc::O_RDONLY)?;
    let now=physical_stamp(&moved)?;
    if (now.device,now.inode,now.length,now.links,now.mode,now.mtime,now.mtime_ns)
        !=(old.device,old.inode,old.length,old.links,old.mode,old.mtime,old.mtime_ns)||planned_hash(&moved)?!=expected {
        return Err(fail("Publication moved leaf differs"));
    }
    let mut leaf=d.leaves.remove(source).ok_or_else(||fail("Publication source reservation missing"))?;
    leaf.stamp=now;d.leaves.insert(target.into(),leaf);
    pending_directory_delta(d,source.rsplit_once('/').map_or("",|p|p.0))?;
    if source.rsplit_once('/').map(|p|p.0)!=target.rsplit_once('/').map(|p|p.0) {
        pending_directory_delta(d,target.rsplit_once('/').map_or("",|p|p.0))?;
    }
    d.fence()
}

fn publication_database_identity(d:&PlannedDestination)->io::Result<()> {
    let pinned=d.leaves.get("starlane-notes.db").ok_or_else(||fail("Publication database missing"))?;
    let current=d.copied.root.leaf("starlane-notes.db")?;
    let now=physical_stamp(&current)?;
    if (now.device,now.inode,now.mode,now.links)!=(pinned.stamp.device,pinned.stamp.inode,pinned.stamp.mode,1)
        ||identity(&pinned.held)?!=identity(&current)? {return Err(fail("Publication database exchanged or aliased"));}
    Ok(())
}
// While our connection is open ONLY the pinned database and its DELETE journal
// are mutable. Every other file/directory and the immutable marker stays pinned.
fn publication_sql_window_fence(d:&PlannedDestination,sidecars:bool)->io::Result<()> {
    d.group_custody_fence()?;
    d.copied.guard()?;
    let now=physical_stamp(d.copied.root.fd())?;
    if (now.device,now.inode,now.mode,now.links)!=(d.root_stamp.device,d.root_stamp.inode,d.root_stamp.mode,d.root_stamp.links)
        ||physical_stamp(&d.copied.marker)?!=d.marker_stamp {return Err(fail("Publication root/marker changed"));}
    publication_database_identity(d)?;
    for (path,(held,expected)) in &d.directories {
        if physical_stamp(held)?!=*expected||physical_stamp(&destination_directory(d.copied.root.fd(),path)?)?!=*expected {
            return Err(fail("Publication unrelated directory changed"));
        }
    }
    for (path,leaf) in &d.leaves {
        if path=="starlane-notes.db" {continue;}
        let current=d.copied.root.leaf(path)?;
        if physical_stamp(&leaf.held)?!=leaf.stamp||physical_stamp(&current)?!=leaf.stamp||planned_hash(&current)?!=leaf.bytes {
            return Err(fail("Publication unrelated leaf changed"));
        }
    }
    fn walk(fd:&File,prefix:&str,files:&mut std::collections::BTreeSet<String>,dirs:&mut std::collections::BTreeSet<String>)->io::Result<()> {
        for e in std::fs::read_dir(format!("/proc/self/fd/{}",fd.as_raw_fd()))? {
            let name=e?.file_name().into_string().map_err(|_|fail("Invalid publication entry"))?;
            if !valid_relative(&name)||name.contains(['/', '\\']){return Err(fail("Unsafe publication entry"));}
            let path=if prefix.is_empty(){name.clone()}else{format!("{prefix}/{name}")};
            let f=open_at(fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;
            if f.metadata()?.is_dir(){dirs.insert(path.clone());walk(&f,&path,files,dirs)?;}
            else if f.metadata()?.is_file(){files.insert(path);}else{return Err(fail("Nonregular publication entry"));}
        }Ok(())
    }
    let mut files=std::collections::BTreeSet::new();let mut dirs=std::collections::BTreeSet::new();
    walk(d.copied.root.fd(),"",&mut files,&mut dirs)?;
    let mut expected:std::collections::BTreeSet<String>=d.leaves.keys().cloned().collect();expected.insert(DESTINATION_MARKER.into());
    if sidecars&&files.contains("starlane-notes.db-journal") {
        let journal=d.copied.root.leaf("starlane-notes.db-journal")?;
        let m=journal.metadata()?;
        if m.nlink()!=1||m.mode()&0o777!=0o600{return Err(fail("SQL journal not private"));}
        expected.insert("starlane-notes.db-journal".into());
    }
    if files!=expected||dirs!=d.directories.keys().cloned().collect(){return Err(fail("Publication SQL-window topology differs"));}
    d.group_custody_fence()?;
    Ok(())
}
pub(crate) async fn commit_publication(mut sealed:SealedPublication)->io::Result<CommittedPublication> {
    if let Some(s)=&sealed.admission_source {s.fence()?;}
    if let Some(s)=&sealed.completed_source{s.fence().map_err(|_|fail("Completed source changed before publication"))?;}
    if let Some(s)=&sealed.composite_source{s.fence()?;}
    if let Some(s)=&sealed.historical_source{s.fence()?;}
    use sqlx::Connection;
    sealed.destination.fence()?;
    let d=&sealed.destination;
    // Held directory anchors the SQLite path; no mutable process DATA_DIR is used.
    sealed.sql_spool.lock().map_err(|_|fail("SQL spool poisoned"))?.fence()?;
    let vfs=PublicationVfs::new(d,sealed.sql_spool.clone())?;
    let uri=format!("sqlite://{}?mode=rw&vfs={}",vfs.state.main_name,vfs.name());
    let mut conn=sqlx::AnyConnection::connect(&uri).await.map_err(|_|fail("Cannot open private publication DB"))?;
    let result=async {
        vfs.check()?;
        publication_database_identity(d)?;
        publication_sql_window_fence(d,false)?;
        // A captured WAL-mode header must transition through SQLite's owned
        // rollback journal before any schema read can try to open WAL sidecars.
        // Original source/bundle bytes remain immutable; only this fenced DB role
        // may change, and full typed application/history audits still precede rows.
        vfs.permit_own_journal()?;
        let locking:String=sqlx::query_scalar("PRAGMA locking_mode=EXCLUSIVE").fetch_one(&mut conn).await.map_err(|_|fail("Cannot choose exclusive transition"))?;
        if locking.to_lowercase()!="exclusive"{return Err(fail("SQLite did not choose exclusive transition"));}
        vfs.permit_empty_wal_transition()?;
        publication_checkpoint("before-wal-transition");
        let mode:String=sqlx::query_scalar("PRAGMA journal_mode=DELETE").fetch_one(&mut conn).await.map_err(|_|fail("Cannot choose bounded SQLite journal"))?;
        if mode.to_lowercase()!="delete"{return Err(fail("SQLite did not select DELETE journal"));}
        vfs.end_empty_wal_transition()?;
        publication_checkpoint("after-wal-transition");
        let locking:String=sqlx::query_scalar("PRAGMA locking_mode=NORMAL").fetch_one(&mut conn).await.map_err(|_|fail("Cannot release transition exclusivity"))?;
        if locking.to_lowercase()!="normal"{return Err(fail("SQLite did not release transition exclusivity"));}
        sqlx::query("PRAGMA synchronous=FULL").execute(&mut conn).await.map_err(|_|fail("Cannot choose durable SQLite writes"))?;
        if publication_application_snapshot(&mut conn).await?!=sealed.application_snapshot{return Err(fail("Copied application SQL differs"));}
        let expected=sealed.prior.as_ref().map(|p|p.rows().to_vec()).unwrap_or_default();
        if history_rows(&mut conn).await?!=(expected.clone(),sealed.previous_pointer.clone()) {return Err(fail("Copied history SQL differs"));}
        vfs.check()?;
        sqlx::query("BEGIN IMMEDIATE").execute(&mut conn).await.map_err(|_|fail("Cannot acquire publication SQL transaction"))?;
        // Recheck inside the exclusive writer transaction before authoring any row.
        if history_rows(&mut conn).await?!=(expected.clone(),sealed.previous_pointer.clone())
            ||publication_application_snapshot(&mut conn).await?!=sealed.application_snapshot {return Err(fail("Publication SQL changed before transaction"));}
        publication_sql_window_fence(d,true)?;
        vfs.check()?;
        let r=&sealed.row;
        sqlx::query("INSERT INTO storage_restore_generations(generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at) VALUES(?,?,?,?,?,?,?,?,?)")
            .bind(&r.generation_id).bind(&r.backup_id).bind(&r.manifest_sha256).bind(&r.database_sha256).bind(&r.ledger_sha256)
            .bind(&r.root_device).bind(&r.root_inode).bind(&r.state).bind(&r.created_at).execute(&mut conn).await.map_err(|_|fail("Cannot append committed generation"))?;
        if sealed.previous_pointer.is_empty() {
            sqlx::query("INSERT INTO storage_restore_current(singleton,generation_id) VALUES(1,?)").bind(&r.generation_id)
                .execute(&mut conn).await.map_err(|_|fail("Cannot install initial generation pointer"))?;
        } else {
            let n=sqlx::query("UPDATE storage_restore_current SET generation_id=? WHERE singleton=1 AND generation_id=?")
                .bind(&r.generation_id).bind(&sealed.previous_pointer[0].1).execute(&mut conn).await.map_err(|_|fail("Cannot CAS generation pointer"))?.rows_affected();
            if n!=1{return Err(fail("Generation pointer CAS refused"));}
        }
        #[cfg(test)]
        if std::env::var("STARLANE_PUBLICATION_SQL_FAULT").ok().as_deref()==Some("nul-text") {
            sqlx::query("UPDATE documents SET title=? WHERE id='d'").bind("a\0right").execute(&mut conn).await.map_err(|_|fail("Cannot inject SQL cell fault"))?;
        }
        #[cfg(test)]
        if std::env::var("STARLANE_PUBLICATION_SQL_FAULT").ok().as_deref()==Some("blob") {
            sqlx::query("UPDATE documents SET title=? WHERE id='d'").bind(vec![0u8,1,0,2]).execute(&mut conn).await.map_err(|_|fail("Cannot inject SQL cell fault"))?;
        }
        let mut after=expected;after.push(r.clone());after.sort_by(|a,b|a.generation_id.cmp(&b.generation_id));
        if history_rows(&mut conn).await?!=(after.clone(),vec![(1,r.generation_id.clone())])
            ||publication_application_snapshot(&mut conn).await?!=sealed.application_snapshot {return Err(fail("Publication changed protected SQL"));}
        publication_sql_window_fence(d,true)?;
        vfs.check()?;
        publication_checkpoint("before-sql-commit");
        sqlx::query("COMMIT").execute(&mut conn).await.map_err(|_|fail("Cannot commit publication SQL"))?;
        vfs.check()?;
        publication_checkpoint("after-sql-commit");
        if history_rows(&mut conn).await?!=(after,vec![(1,r.generation_id.clone())]) {return Err(fail("Committed publication SQL differs"));}
        publication_sql_window_fence(d,false)?;
        Ok(())
    }.await;
    // Connection close is mandatory even on any refusal; OS/SQLite rolls back
    // incomplete transaction. Retained marker keeps both old/new bytes inert.
    let closed=conn.close().await.map_err(|_|fail("Cannot close publication SQLite handle"));
    result?;closed?;
    drop(vfs);
    sealed.sql_spool.lock().map_err(|_|fail("SQL spool poisoned"))?.fence()?;
    publication_sql_window_fence(&sealed.destination,false)?;
    // A new immutable read-only connection bypasses the writer pager cache.
    // Its own pre-pager VFS binds the newly opened actual FD; exact captured
    // bytes/stamps must remain stable throughout the lossless disk SQL audit.
    #[cfg(test)] publication_disk_fault(&sealed.destination,false)?;
    let disk_vfs=PublicationVfs::read_after_sql(&sealed.destination,sealed.sql_spool.clone())?;
    let disk_uri=format!("sqlite://{}?mode=ro&immutable=true&vfs={}",disk_vfs.state.main_name,disk_vfs.name());
    let mut disk=sqlx::AnyConnection::connect(&disk_uri).await.map_err(|_|fail("Cannot open post-SQL private DB audit"))?;
    let disk_result=async {
        disk_vfs.check()?;
        let mut expected=sealed.prior.as_ref().map(|p|p.rows().to_vec()).unwrap_or_default();
        expected.push(sealed.row.clone());expected.sort_by(|a,b|a.generation_id.cmp(&b.generation_id));
        if history_rows(&mut disk).await?!=(expected,vec![(1,sealed.row.generation_id.clone())])
            ||publication_application_snapshot(&mut disk).await?!=sealed.application_snapshot {
            return Err(fail("Closed publication disk changed protected SQL"));
        }
        if let Some(source)=&sealed.historical_source{
            source.fence()?;
            // Exact actual old closure + one new full row was checked above
            // through this original disk VFS connection, never by caller data.
            let post=engine_transfer::historical_application_projection_in(&mut disk,None).await.map_err(|_|fail("Historical actual postpublication full projection unavailable"))?;
            let(rows,pointer)=history_rows(&mut disk).await?;
            let mut expected=sealed.historical_original.0.clone();expected.push(sealed.row.clone());expected.sort_by(|a,b|a.generation_id.cmp(&b.generation_id));
            if rows!=expected||pointer!=vec![(1,sealed.row.generation_id.clone())]{return Err(fail("Historical witnessed append/current CAS differs"));}
            sealed.historical_witness=Some(HistoricalPublicationWitness{original_rows:sealed.historical_original.0.clone(),original_pointer:sealed.historical_original.1.clone(),committed_row:sealed.row.clone(),committed_pointer:pointer,postpublication_projection:post,root:admission::Identity{device:sealed.destination.root_stamp.device,inode:sealed.destination.root_stamp.inode},main:disk_vfs.state.initial_database_stamp.clone()});
            source.fence()?;
        }
        #[cfg(test)] publication_disk_fault(&sealed.destination,true)?;
        disk_vfs.check()?;disk_vfs.state.initial_database()?;Ok(())
    }.await;
    let disk_closed=disk.close().await.map_err(|_|fail("Cannot close post-SQL disk audit"));
    disk_result?;disk_closed?;
    disk_vfs.state.ancestry()?;disk_vfs.state.initial_database()?;
    let certified_db_stamp=disk_vfs.state.initial_database_stamp.clone();
    let certified_db_bytes=disk_vfs.state.initial_database_bytes.clone();
    drop(disk_vfs);
    publication_sql_window_fence(&sealed.destination,false)?;
    let d=&mut sealed.destination;
    let db=d.leaves.get_mut("starlane-notes.db").ok_or_else(||fail("Publication DB role missing"))?;
    db.held.sync_all()?;
    if physical_stamp(&db.held)?!=certified_db_stamp||planned_hash(&db.held)?!=certified_db_bytes {
        return Err(fail("Post-audit DB stamp/bytes changed before certification"));
    }
    db.bytes=certified_db_bytes;db.stamp=certified_db_stamp;
    // Only our SQLite DELETE journal modified this root role; all other roles
    // were compared against their original stamps throughout the write window.
    pending_directory_delta(d,"")?;
    d.fence()?;
    publication_checkpoint("before-completion");
    pending_new_file(d,&sealed.completion_relative,&sealed.raw_completion)?;
    publication_checkpoint("completion");
    d.fence()?;
    Ok(CommittedPublication{sealed})
}
pub(crate) fn retire_publication(committed:CommittedPublication)->io::Result<BootReadyDestination> {
    if let Some(s)=&committed.sealed.admission_source {s.fence()?;}
    if let Some(s)=&committed.sealed.completed_source{s.fence().map_err(|_|fail("Completed source changed before finalization"))?;}
    if let Some(s)=&committed.sealed.composite_source{s.fence()?;}
    if let Some(s)=&committed.sealed.historical_source{s.fence()?;committed.sealed.historical_witness.as_ref().ok_or_else(||fail("Historical committed publication witness absent"))?.check(&committed.sealed.destination,&committed.sealed.historical_original,&committed.sealed.row)?;}
    let mut sealed=committed.sealed;
    let d=&mut sealed.destination;
    d.fence()?;
    sealed.sql_spool.lock().map_err(|_|fail("SQL spool poisoned"))?.fence()?;
    if digest(&read_ledger(&d.copied.root.leaf(&format!("{NAMESPACE}/current.json"))?)?)!=sealed.row.ledger_sha256
        ||read_ledger(&d.copied.root.leaf(&format!("{NAMESPACE}/current.json"))?)?!=sealed.envelope
        ||!d.leaves.contains_key(&sealed.completion_relative) {return Err(fail("Committed seal differs"));}
    publication_checkpoint("before-marker-retirement");
    let target=format!("{NAMESPACE}/generations/{}/retired-marker",sealed.row.generation_id);
    let (parent,name)=planned_parent(d,&target)?;
    if exists(&parent,&name)?{return Err(fail("Retired marker role already exists"));}
    let name=CString::new(name).map_err(|_|fail("Invalid retired marker path"))?;
    let old=CString::new(DESTINATION_MARKER).unwrap();
    #[cfg(test)] publication_marker_fault(d)?;
    let before=physical_stamp(&d.copied.marker)?;
    if unsafe{libc::syscall(libc::SYS_renameat2,d.copied.root.fd().as_raw_fd(),old.as_ptr(),parent.as_raw_fd(),name.as_ptr(),libc::RENAME_NOREPLACE)}!=0{return Err(io::Error::last_os_error());}
    parent.sync_all()?;d.copied.root.fd().sync_all()?;
    publication_checkpoint("after-marker-retirement");
    let marker=open_at(&parent,name.to_str().map_err(|_|fail("Invalid retired marker name"))?,libc::O_RDONLY)?;
    let after=physical_stamp(&marker)?;
    if (before.device,before.inode,before.length,before.links,before.mode,before.mtime,before.mtime_ns)
        !=(after.device,after.inode,after.length,after.links,after.mode,after.mtime,after.mtime_ns)
        ||read_ledger(&marker)?!=d.copied.marker_bytes {return Err(fail("Retired marker changed"));}
    d.leaves.insert(target.clone(),PlannedLeaf{held:marker,stamp:after,
        bytes:crate::storage::backup_inventory::Bytes{size_bytes:d.copied.marker_bytes.len() as u64,sha256:digest(&d.copied.marker_bytes)}});
    pending_directory_delta(d,"")?;
    pending_directory_delta(d,target.rsplit_once('/').ok_or_else(||fail("Invalid retirement role"))?.0)?;
    publication_retired_fence(d)?;
    publication_finalization_file(d,&sealed.finalization_relative,&sealed.raw_finalization)?;
    publication_checkpoint("after-finalization");
    // Structural bootstrap proves closed roles before ordinary initialization;
    // fresh startup still independently requires exact actual SQL commitment.
    bootstrap_gate(d.path())?;
    let retained_sql_artifacts=sealed.sql_spool.lock().map_err(|_|fail("SQL spool poisoned"))?.path();
    Ok(BootReadyDestination{destination:sealed.destination,generation_id:sealed.row.generation_id,retained_sql_artifacts,historical_source:sealed.historical_source,historical_witness:sealed.historical_witness,historical_original:sealed.historical_original})
}

fn publication_retired_fence(d:&PlannedDestination)->io::Result<()> {
    d.group_custody_fence()?;
    d.copied.root.check()?;d.copied.parent.check()?;
    if physical_stamp(d.copied.root.fd())?!=d.root_stamp{return Err(fail("Retired root changed"));}
    for (path,(held,stamp)) in &d.directories {
        if physical_stamp(held)?!=*stamp||physical_stamp(&destination_directory(d.copied.root.fd(),path)?)?!=*stamp {
            return Err(fail("Retired directory changed"));
        }
    }
    for (path,leaf) in &d.leaves {
        let current=d.copied.root.leaf(path)?;
        if physical_stamp(&leaf.held)?!=leaf.stamp||physical_stamp(&current)?!=leaf.stamp||planned_hash(&current)?!=leaf.bytes {
            return Err(fail("Retired leaf changed"));
        }
    }
    fn walk(fd:&File,prefix:&str,files:&mut std::collections::BTreeSet<String>,dirs:&mut std::collections::BTreeSet<String>)->io::Result<()> {
        for entry in std::fs::read_dir(format!("/proc/self/fd/{}",fd.as_raw_fd()))? {
            let name=entry?.file_name().into_string().map_err(|_|fail("Invalid retired entry"))?;
            if !valid_relative(&name)||name.contains(['/', '\\']){return Err(fail("Unsafe retired entry"));}
            let path=if prefix.is_empty(){name.clone()}else{format!("{prefix}/{name}")};
            let f=open_at(fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;
            if f.metadata()?.is_dir(){dirs.insert(path.clone());walk(&f,&path,files,dirs)?;}
            else if f.metadata()?.is_file(){files.insert(path);}else{return Err(fail("Invalid retired leaf"));}
        }Ok(())
    }
    let mut files=std::collections::BTreeSet::new();let mut dirs=std::collections::BTreeSet::new();
    walk(d.copied.root.fd(),"",&mut files,&mut dirs)?;
    if files!=d.leaves.keys().cloned().collect()||dirs!=d.directories.keys().cloned().collect(){return Err(fail("Retired topology changed"));}
    d.group_custody_fence()?;
    Ok(())
}

fn publication_checkpoint(_phase:&str) {
    #[cfg(test)]
    if std::env::var("STARLANE_PUBLICATION_KILL_AT").ok().as_deref()==Some(_phase) {
        // Actual publisher reached this durable boundary; the fixture process
        // is killed without unwinding or running destructor cleanup.
        unsafe { libc::kill(libc::getpid(),libc::SIGKILL); }
    }
}

#[cfg(test)]
fn publication_disk_fault(d:&PlannedDestination,during_read:bool)->io::Result<()> {
    let Ok(mode)=std::env::var("STARLANE_PUBLICATION_DISK_FAULT") else{return Ok(())};
    if mode=="nul-after-close"&&!during_read {
        let done=std::process::Command::new("python3").args(["-c","import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute('UPDATE documents SET title=? WHERE id=?',('a\\x00right'.replace('\\x00',chr(0)),'d')); c.commit(); c.close()",d.path().join("starlane-notes.db").to_str().ok_or_else(||fail("Invalid disk fault path"))?]).status()?;
        if !done.success(){return Err(fail("Cannot inject real disk SQL fault"));}
    } else if mode=="write-revert-during-read"&&during_read {
        use std::os::unix::fs::FileExt;
        let file=std::fs::OpenOptions::new().write(true).read(true).open(d.path().join("starlane-notes.db"))?;
        let mut byte=[0u8;1];let pos=file.metadata()?.len()-1;file.read_exact_at(&mut byte,pos)?;
        file.write_all_at(&[byte[0]^1],pos)?;file.sync_all()?;file.write_all_at(&byte,pos)?;file.sync_all()?;
    }Ok(())
}

#[cfg(test)]
fn publication_marker_fault(d:&PlannedDestination)->io::Result<()> {
    let Ok(foreign)=std::env::var("STARLANE_PUBLICATION_MARKER_EXCHANGE") else{return Ok(())};
    let original=d.path().join(DESTINATION_MARKER);
    let held=d.path().parent().ok_or_else(||fail("Marker fault parent missing"))?.join("test-original-marker.retained");
    std::fs::rename(&original,held)?;
    std::fs::copy(foreign,&original)?;
    Ok(())
}

fn publication_finalization_file(d:&mut PlannedDestination,relative:&str,raw:&[u8])->io::Result<()> {
    publication_retired_fence(d)?;
    #[cfg(test)]
    if std::env::var("STARLANE_PUBLICATION_FINALIZATION_IO_FAILURE").is_ok() {
        return Err(io::Error::from_raw_os_error(libc::ENOSPC));
    }
    let (parent,name)=planned_parent(d,relative)?;
    let mut file=open_at(&parent,&name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;
    {use std::io::Write;file.write_all(raw)?;}file.sync_all()?;parent.sync_all()?;
    let leaf=PlannedLeaf{stamp:physical_stamp(&file)?,bytes:crate::storage::backup_inventory::Bytes{size_bytes:raw.len()as u64,sha256:digest(raw)},held:file};
    if d.leaves.insert(relative.into(),leaf).is_some(){return Err(fail("Finalization role already certified"));}
    pending_directory_delta(d,relative.rsplit_once('/').ok_or_else(||fail("Invalid finalization role"))?.0)?;
    publication_retired_fence(d)
}

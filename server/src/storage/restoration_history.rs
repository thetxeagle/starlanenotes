// Read-only history closure. Inclusion is inside restoration::linux; no writes or activation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct HistoryLink {
    generation_id: String,
    ledger_sha256: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HistoryArtifact {
    relative: String,
    sha256: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct DeniedReservation {
    namespace: String,
    op_id: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HistoryProof {
    namespace: String,
    op_id: String,
    manifest: HistoryArtifact,
    evidence: HistoryArtifact,
    // Exact mint-intent bytes retained, not a standalone installation grant.
    mint_intent: Option<HistoryArtifact>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HistoryEnvelope {
    version: u32,
    runtime: Ledger,
    intent_sha256: String,
    completion_sha256: String,
    #[serde(default, deserialize_with="history_finalization_reference", skip_serializing_if="Option::is_none")]
    finalization: Option<HistoryArtifact>,
    parent: Option<HistoryLink>,
    archives: Vec<HistoryLink>,
    denied: Vec<DeniedReservation>,
    proofs: Vec<HistoryProof>,
}
fn history_finalization_reference<'de,D>(deserializer:D)->Result<Option<HistoryArtifact>,D::Error>
where D:serde::Deserializer<'de> {
    // Absent v2 field defaults to None; explicit null is not a legacy field
    // and must not silently bypass the versioned closed contract.
    <HistoryArtifact as serde::Deserialize>::deserialize(deserializer).map(Some)
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HistoryIntent {
    version: u32,
    generation_id: String,
    backup_id: String,
    manifest_sha256: String,
    database_sha256: String,
    source_root: crate::storage::backup_inventory::Identity,
    target_root: crate::storage::backup_inventory::Identity,
    retired_marker: HistoryArtifact,
    runtime_sha256: String,
    parent: Option<HistoryLink>,
    archives: Vec<HistoryLink>,
    denied: Vec<DeniedReservation>,
    proofs: Vec<HistoryProof>,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HistoryCompletion {
    version: u32,
    generation_id: String,
    backup_id: String,
    manifest_sha256: String,
    database_sha256: String,
    intent_sha256: String,
    runtime_sha256: String,
    root_device: String,
    root_inode: String,
    state: String,
    created_at: String,
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct HistoryFinalization {
    version: u32,
    generation_id: String,
    backup_id: String,
    target_root: crate::storage::backup_inventory::Identity,
    original_marker: crate::storage::backup_inventory::Identity,
    retired_marker: HistoryArtifact,
    intent_sha256: String,
    completion_sha256: String,
}
#[derive(Debug, Clone, serde::Serialize, PartialEq, Eq)]
pub(crate) struct HistorySqlRow {
    generation_id: String,
    backup_id: String,
    manifest_sha256: String,
    database_sha256: String,
    ledger_sha256: String,
    root_device: String,
    root_inode: String,
    state: String,
    created_at: String,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoryInstallProof { op_id:String, temp:String, dev:u64, ino:u64 }
#[derive(Debug)]
struct HistoryGeneration {
    raw: Vec<u8>,
    ledger: Ledger,
    envelope: Option<HistoryEnvelope>,
    source_root: Option<crate::storage::backup_inventory::Identity>,
}
/// Validated captured history only. Fresh owner classification remains necessary.
#[derive(Debug)]
pub(crate) struct ValidatedHistory {
    current_id: String,
    generations: std::collections::BTreeMap<String, HistoryGeneration>,
    rows: Vec<HistorySqlRow>,
    captured_files: std::collections::BTreeMap<String, crate::storage::backup_inventory::Bytes>,
    captured_ids: std::collections::BTreeMap<String, crate::storage::backup_inventory::SourceFile>,
}
impl ValidatedHistory {
    pub(crate) fn current_id(&self)->&str {&self.current_id}
    /// Private publisher shares the linux module; these are borrowed validated
    /// references, not caller-authored constructors or deserialized grants.
    fn current_denied(&self)->&[DeniedReservation] {self.generations[&self.current_id].envelope.as_ref().map(|e|e.denied.as_slice()).unwrap_or_default()}
    fn current_proofs(&self)->&[HistoryProof] {self.generations[&self.current_id].envelope.as_ref().map(|e|e.proofs.as_slice()).unwrap_or_default()}
    pub(crate) fn ledger(&self) -> &Ledger { &self.generations[&self.current_id].ledger }
    pub(crate) fn raw_ledger(&self) -> &[u8] { &self.generations[&self.current_id].raw }
    pub(crate) fn generation_count(&self) -> usize { self.generations.len() }
    pub(crate) fn rows(&self) -> &[HistorySqlRow] { &self.rows }
    pub(crate) fn raw_generations(&self) -> impl Iterator<Item=(&str,&[u8])> {
        self.generations.iter().map(|(id,g)|(id.as_str(),g.raw.as_slice()))
    }
    pub(crate) fn resolve_original(&self, namespace:&str, op:&str, evidence:&[u8], relative:&str,
        original:Identity, length:u64, sha256:&str) -> io::Result<Identity> {
        if !valid_relative(relative) || relative.contains('\\') || !valid_hash(sha256) {
            return Err(fail("Invalid history ownership requirement"));
        }
        let current=&self.generations[&self.current_id];
        if current.envelope.as_ref().is_some_and(|e|e.denied.iter().any(|d|d.namespace==namespace&&d.op_id==op)) {
            return Err(fail("History operation is denied"));
        }
        let bytes=self.captured_files.get(relative).ok_or_else(||fail("History required bytes missing"))?;
        let id=self.captured_ids.get(relative).ok_or_else(||fail("History required identity missing"))?;
        if bytes.size_bytes!=length || bytes.sha256!=sha256 { return Err(fail("History required bytes differ")); }
        let observed=(id.device,id.inode);
        if let Some(scope)=current.ledger.scopes.iter().find(|s|s.namespace==namespace&&s.op_id==op) {
            let b=scope.bindings.iter().find(|b|b.relative==relative).ok_or_else(||fail("History required binding missing"))?;
            if scope.evidence_sha256!=digest(evidence){return Err(fail("Prior lineage original evidence differs"));}
            if (b.original_device,b.original_inode)!=original
                || (b.adopted_device,b.adopted_inode)!=observed || b.length!=length || b.sha256!=sha256 {
                return Err(fail("History scoped binding differs"));
            }
        } else if original!=observed { return Err(fail("History native ownership differs")); }
        Ok(observed)
    }
}
fn history_namespace(s:&str)->bool {
    [".storage-journal",".publication-journal",".canonical-rewrite-journal",".move-relocation-journal",".attachment-upload-journal"].contains(&s)
}
fn history_link(link:&HistoryLink)->io::Result<()> {
    if !valid_id(&link.generation_id)||!valid_hash(&link.ledger_sha256) { return Err(fail("Invalid history link")); }
    Ok(())
}
fn history_parse(raw:&[u8])->io::Result<HistoryGeneration> {
    if raw.len() as u64>MAX_LEDGER { return Err(fail("History record too large")); }
    // Struct deserialization rejects duplicate fields and unknown keys. No Value intermediary.
    if let Ok(ledger)=serde_json::from_slice::<Ledger>(raw) {
        validate(&ledger)?; if ledger.version!=1 {return Err(fail("Admission owner requires finalized history envelope"));} return Ok(HistoryGeneration{raw:raw.to_vec(),ledger,envelope:None,source_root:None});
    }
    let envelope:HistoryEnvelope=serde_json::from_slice(raw).map_err(|_|fail("Malformed history envelope"))?;
    if !matches!(envelope.version,2|3|4) || !valid_hash(&envelope.intent_sha256)||!valid_hash(&envelope.completion_sha256) {
        return Err(fail("Invalid history envelope header"));
    }
    validate(&envelope.runtime)?;
    if (envelope.version==4)!=(envelope.runtime.version==5){return Err(fail("Historical runtime requires distinct finalized history4"));}
    if matches!(envelope.runtime.version,2|3) && envelope.version!=3 {return Err(fail("Admission owner requires finalized history envelope"));}
    match (envelope.version,&envelope.finalization) {
        (2,None)=>{},
        (3|4,Some(f)) if f.relative==format!("{NAMESPACE}/generations/{}/finalization.json",envelope.runtime.generation_id)&&valid_hash(&f.sha256)=>{},
        _=>return Err(fail("Invalid history finalization reference")),
    }
    let mut ids=std::collections::BTreeSet::new();
    for link in &envelope.archives {
        history_link(link)?;
        if link.generation_id==envelope.runtime.generation_id || !ids.insert(&link.generation_id) {
            return Err(fail("Duplicate history archive"));
        }
    }
    if let Some(parent)=&envelope.parent {
        history_link(parent)?;
        if !envelope.archives.contains(parent) { return Err(fail("History parent absent from archives")); }
    } else if !envelope.archives.is_empty() { return Err(fail("History archives without parent")); }
    let mut denied=std::collections::BTreeSet::new();
    for d in &envelope.denied {
        if !history_namespace(&d.namespace)||!valid_id(&d.op_id)||!denied.insert((&d.namespace,&d.op_id))
            ||envelope.runtime.scopes.iter().any(|s|s.namespace==d.namespace&&s.op_id==d.op_id) {
            return Err(fail("Invalid history denied reservation"));
        }
    }
    let mut proofs=std::collections::BTreeSet::new();
    for p in &envelope.proofs {
        if !proofs.insert((&p.namespace,&p.op_id)) { return Err(fail("Duplicate history proof")); }
        let scope=envelope.runtime.scopes.iter().find(|s|s.namespace==p.namespace&&s.op_id==p.op_id)
            .ok_or_else(||fail("History proof lacks scope"))?;
        let base=format!("{}/{}/",p.namespace,p.op_id);
        let evidence=if p.namespace==".move-relocation-journal" {"manifest.json"} else {"install-proof.json"};
        let manifest_name=if p.namespace==".attachment-upload-journal" {"intent.json"} else {"manifest.json"};
        if p.manifest.relative!=format!("{base}{manifest_name}")||p.evidence.relative!=format!("{base}{evidence}")
            || !valid_hash(&p.manifest.sha256)||p.evidence.sha256!=scope.evidence_sha256 {
            return Err(fail("History immutable proof reference differs"));
        }
        if let Some(m)=&p.mint_intent {
            let name=m.relative.strip_prefix(".restoration-custody/preproof-").and_then(|s|s.strip_suffix(".json"));
            if p.namespace!=".storage-journal"||!name.is_some_and(valid_id)||!valid_hash(&m.sha256) {
                return Err(fail("Invalid history mint intent reference"));
            }
        }
    }
    if proofs.len()!=envelope.runtime.scopes.len() {return Err(fail("History scope lacks immutable proof reference"));}
    Ok(HistoryGeneration{raw:raw.to_vec(),ledger:envelope.runtime.clone(),envelope:Some(envelope),source_root:None})
}
async fn history_rows(conn:&mut sqlx::AnyConnection)->io::Result<(Vec<HistorySqlRow>,Vec<(i64,String)>)> {
    let raw=sqlx::query("SELECT generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at FROM storage_restore_generations ORDER BY generation_id")
        .fetch_all(&mut *conn).await.map_err(|_|fail("Cannot inspect history rows"))?;
    let mut rows=Vec::new();
    for r in raw {
        let get=|k|r.try_get::<String,_>(k).map_err(|_|fail("Malformed history SQL field"));
        rows.push(HistorySqlRow{generation_id:get("generation_id")?,backup_id:get("backup_id")?,manifest_sha256:get("manifest_sha256")?,database_sha256:get("database_sha256")?,ledger_sha256:get("ledger_sha256")?,root_device:get("root_device")?,root_inode:get("root_inode")?,state:get("state")?,created_at:get("created_at")?});
    }
    let ptr=sqlx::query_as("SELECT singleton,generation_id FROM storage_restore_current ORDER BY singleton")
        .fetch_all(&mut *conn).await.map_err(|_|fail("Cannot inspect history pointer"))?;
    Ok((rows,ptr))
}

/// All generations validated together. Read-only plan provenance, never a grant constructor.
#[derive(Debug)]
pub(crate) struct HistorySnapshot {
    current_id: String,
    raw_generations: std::collections::BTreeMap<String, Vec<u8>>,
    rows: Vec<HistorySqlRow>,
    denied: Vec<DeniedReservation>,
}
impl ValidatedHistory {
    pub(crate) fn snapshot(&self) -> HistorySnapshot {
        HistorySnapshot {current_id:self.current_id.clone(),raw_generations:self.raw_generations().map(|(id,raw)|(id.into(),raw.to_vec())).collect(),rows:self.rows.clone(),denied:self.generations[&self.current_id].envelope.as_ref().map(|e|e.denied.clone()).unwrap_or_default()}
    }
}
impl HistorySnapshot {
    pub(crate) fn current_id(&self)->&str {&self.current_id}
    pub(crate) fn raw_generations(&self)->impl Iterator<Item=(&str,&[u8])> {self.raw_generations.iter().map(|(id,raw)|(id.as_str(),raw.as_slice()))}
    pub(crate) fn rows(&self)->&[HistorySqlRow] {&self.rows}
}

// Honest source adapters share the same structural validator. Structural admission
// has NO SQL authority; live activation alone validates real SQL and registers.
struct HistoryHeldRecord {held:File,stamp:PhysicalStamp,raw:Vec<u8>}
struct HistorySource<'a> {
    captured_ids:Option<&'a std::collections::BTreeMap<String,Identity>>,
    root_check:fn(&Root)->io::Result<()>,
    root:&'a Root,
    bundle:Option<&'a crate::storage::backup_inventory::ValidatedBundle>,
    files:std::collections::BTreeSet<String>,
    directories:std::collections::BTreeMap<String,(File,PhysicalStamp)>,
    records:std::collections::BTreeMap<String,HistoryHeldRecord>,
}
fn history_topology(root:&Root)->io::Result<(std::collections::BTreeSet<String>,std::collections::BTreeMap<String,(File,PhysicalStamp)>)> {
    fn walk(fd:File,path:&str,files:&mut std::collections::BTreeSet<String>,dirs:&mut std::collections::BTreeMap<String,(File,PhysicalStamp)>)->io::Result<()> {
        if fd.metadata()?.mode()&0o777!=0o700{return Err(fail("History directory is not private"));}
        let before=physical_stamp(&fd)?;
        for e in std::fs::read_dir(format!("/proc/self/fd/{}",fd.as_raw_fd()))? {
            let name=e?.file_name().into_string().map_err(|_|fail("Unsafe history entry"))?;
            if !valid_relative(&name)||name.contains(['/', '\\']){return Err(fail("Unsafe history entry"));}
            let relative=format!("{path}/{name}");
            let held=open_at(&fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;
            if held.metadata()?.is_dir(){walk(held,&relative,files,dirs)?;}else if held.metadata()?.is_file(){files.insert(relative);}else{return Err(fail("Nonregular history entry"));}
        }
        if physical_stamp(&fd)?!=before{return Err(fail("History directory changed"));}
        dirs.insert(path.into(),(fd,before));Ok(())
    }
    let mut files=std::collections::BTreeSet::new();let mut dirs=std::collections::BTreeMap::new();
    walk(open_at(root.fd(),NAMESPACE,libc::O_RDONLY|libc::O_DIRECTORY)?,NAMESPACE,&mut files,&mut dirs)?;
    Ok((files,dirs))
}
impl<'a> HistorySource<'a> {
    fn new(root:&'a Root,bundle:Option<&'a crate::storage::backup_inventory::ValidatedBundle>)->io::Result<Self> {
        Self::new_with_check(root,bundle,Root::check)
    }
    fn new_with_check(root:&'a Root,bundle:Option<&'a crate::storage::backup_inventory::ValidatedBundle>,root_check:fn(&Root)->io::Result<()>)->io::Result<Self> {
        root_check(root)?;let(files,directories)=history_topology(root)?;
        if let Some(b)=bundle {
            let captured_files=b.manifest.files.0.keys().filter(|p|p.starts_with(".restoration-custody/")).cloned().collect::<std::collections::BTreeSet<_>>();
            let captured_dirs=b.manifest.source_custody.directories.0.keys().filter(|p|*p==NAMESPACE||p.starts_with(".restoration-custody/")).cloned().collect::<std::collections::BTreeSet<_>>();
            if captured_files!=files||captured_dirs!=directories.keys().cloned().collect(){return Err(fail("History captured topology differs"));}
        }
        Ok(Self{captured_ids:None,root_check,root,bundle,files,directories,records:Default::default()})
    }
    fn read(&mut self,relative:&str)->io::Result<Vec<u8>> {
        if let Some(record)=self.records.get(relative){return Ok(record.raw.clone());}
        let held=self.root.leaf(relative)?;let before=physical_stamp(&held)?;
        if !held.metadata()?.is_file()||before.mode&0o777!=0o600||before.links==0||(relative.starts_with(".restoration-custody/")&&before.links!=1){return Err(fail("History record is not private regular data"));}
        let raw=read_ledger(&held)?;
        if physical_stamp(&held)?!=before{return Err(fail("History bytes changed during inspection"));}
        if let Some(b)=self.bundle {
            let expected=b.manifest.files.0.get(relative).ok_or_else(||fail("History inventory entry missing"))?;
            if expected.size_bytes!=raw.len()as u64||expected.sha256!=digest(&raw){return Err(fail("History bytes differ from inventory"));}
            if relative.starts_with(".restoration-custody/")&&b.manifest.source_custody.files.0.get(relative).is_none_or(|f|f.links!=1){return Err(fail("History custody record is aliased"));}
        }
        self.records.insert(relative.into(),HistoryHeldRecord{held,stamp:before,raw:raw.clone()});Ok(raw)
    }
    fn fence(&self)->io::Result<()> {
        (self.root_check)(self.root)?;
        let(files,dirs)=history_topology(self.root)?;
        if files!=self.files||dirs.keys().collect::<Vec<_>>()!=self.directories.keys().collect::<Vec<_>>(){return Err(fail("History topology changed"));}
        for(path,(held,stamp))in &self.directories {
            if physical_stamp(held)?!=*stamp||physical_stamp(&dirs[path].0)?!=*stamp{return Err(fail("History directory changed"));}
        }
        for(path,record)in &self.records {
            let fresh=self.root.leaf(path)?;
            if physical_stamp(&record.held)?!=record.stamp||physical_stamp(&fresh)?!=record.stamp||read_ledger(&fresh)?!=record.raw||physical_stamp(&fresh)?!=record.stamp{return Err(fail("History control evidence changed"));}
        }
        if let Some(b)=self.bundle{b.fence()?;}(self.root_check)(self.root)
    }
}
struct ClosedHistory<'a> {current_id:String,generations:std::collections::BTreeMap<String,HistoryGeneration>,source:HistorySource<'a>}
fn history_closure<'a>(mut source:HistorySource<'a>,expected_root:Identity,sql:Option<(&[HistorySqlRow],&Vec<(i64,String)>)>)->io::Result<ClosedHistory<'a>> {
    let source_files=source.files.clone();let source_dirs:std::collections::BTreeSet<String>=source.directories.keys().cloned().collect();
    let mut generations=std::collections::BTreeMap::new();
    let mut required_files=std::collections::BTreeSet::new();
    let mut current_marker_binding:Option<(String,Identity)>=None;
    let mut required_dirs=std::collections::BTreeSet::from([NAMESPACE.to_string()]);
    let mut read=|relative:&str|->io::Result<Vec<u8>> {
        let raw=source.read(relative)?;
        if relative.starts_with(".restoration-custody/") {
            required_files.insert(relative.to_string());
            let mut path=Path::new(relative).parent();
            while let Some(p)=path {let s=p.to_string_lossy(); if s==NAMESPACE||s.starts_with(".restoration-custody/"){required_dirs.insert(s.into_owned());} path=p.parent();}
        }
        Ok(raw)
    };
    let current_raw=read(&format!("{NAMESPACE}/current.json"))?;
    let current_id=history_parse(&current_raw)?.ledger.generation_id;
    let mut ids=std::collections::BTreeSet::from([current_id.clone()]);
    for path in &source_files {
        if let Some(id)=path.strip_prefix(".restoration-custody/archive/").and_then(|p|p.strip_suffix("/ledger.json")) {
            if !valid_id(id)||!ids.insert(id.to_string()){return Err(fail("Invalid history archive role"));}
        }
    }
    if let Some((rows,pointer))=sql {
        if history_parse(&current_raw)?.envelope.is_none() && rows.len()!=1 {return Err(fail("Prior archived lineage requires resolution"));}
        if pointer!=&vec![(1,current_id.clone())] || rows.iter().map(|r|r.generation_id.clone()).collect::<std::collections::BTreeSet<_>>()!=ids || rows.len()!=ids.len() {return Err(fail("Invalid history current pointer or SQL closure"));}
    }
    for id in &ids {

        let path=if *id==current_id {format!("{NAMESPACE}/current.json")}else{format!("{NAMESPACE}/archive/{id}/ledger.json")};
        let raw=read(&path)?;
        let mut generation=history_parse(&raw)?;
        let l=&generation.ledger;
        if l.generation_id!=*id {return Err(fail("History record role differs"));}
        let row=sql.and_then(|(rows,_)|rows.iter().find(|r|r.generation_id==*id));
        if let Some(row)=row {
            if row.state!="committed"||!valid_hash(&row.ledger_sha256)||row.created_at.is_empty()
                ||row.backup_id!=l.backup_id||row.manifest_sha256!=l.manifest_sha256||row.database_sha256!=l.database_sha256
                ||row.ledger_sha256!=digest(&raw)||row.root_device!=l.root_device.to_string()||row.root_inode!=l.root_inode.to_string() {
                return Err(fail("History SQL ledger tuple differs"));
            }
        }
        if let Some(e)=&generation.envelope {
            let base=format!("{NAMESPACE}/generations/{}/",l.generation_id);
            let raw_intent=read(&format!("{base}intent.json"))?;
            let raw_complete=read(&format!("{base}completion.json"))?;
            if digest(&raw_intent)!=e.intent_sha256||digest(&raw_complete)!=e.completion_sha256 {return Err(fail("History seal digest differs"));}
            let intent:HistoryIntent=serde_json::from_slice(&raw_intent).map_err(|_|fail("Malformed history intent"))?;
            let done:HistoryCompletion=serde_json::from_slice(&raw_complete).map_err(|_|fail("Malformed history completion"))?;
            let runtime_hash=digest(&serde_json::to_vec(l).map_err(|_|fail("Cannot hash history runtime"))?);
            if intent.version!=1||intent.generation_id!=l.generation_id||intent.backup_id!=l.backup_id||intent.manifest_sha256!=l.manifest_sha256
                ||intent.database_sha256!=l.database_sha256||intent.target_root.device!=l.root_device||intent.target_root.inode!=l.root_inode
                ||intent.runtime_sha256!=runtime_hash||intent.parent!=e.parent||intent.archives!=e.archives||intent.denied!=e.denied||intent.proofs!=e.proofs
                ||done.version!=1||done.generation_id!=l.generation_id||done.backup_id!=l.backup_id||done.manifest_sha256!=l.manifest_sha256
                ||done.database_sha256!=l.database_sha256||done.intent_sha256!=e.intent_sha256||done.runtime_sha256!=runtime_hash
                ||done.root_device!=l.root_device.to_string()||done.root_inode!=l.root_inode.to_string()||done.state!="committed"||done.created_at.is_empty()||row.is_some_and(|r|r.created_at!=done.created_at) {
                return Err(fail("History sealed fields differ"));
            }
            let marker_path=format!("{base}retired-marker");
            let marker=read(&marker_path)?;
            if intent.retired_marker.relative!=marker_path||!valid_hash(&intent.retired_marker.sha256)
                ||digest(&marker)!=intent.retired_marker.sha256
                ||marker!=format!("starlane private copy {}\n",l.backup_id).as_bytes() {
                return Err(fail("History retired marker differs"));
            }
            if let Some(reference)=&e.finalization {
                let raw_final=read(&reference.relative)?;
                if digest(&raw_final)!=reference.sha256 {return Err(fail("History finalization digest differs"));}
                let finalization:HistoryFinalization=serde_json::from_slice(&raw_final).map_err(|_|fail("Malformed history finalization"))?;
                if finalization.version!=1||finalization.generation_id!=l.generation_id||finalization.backup_id!=l.backup_id
                    ||finalization.target_root.device!=l.root_device||finalization.target_root.inode!=l.root_inode
                    ||finalization.original_marker.inode==0||finalization.retired_marker!=intent.retired_marker
                    ||finalization.intent_sha256!=e.intent_sha256||finalization.completion_sha256!=e.completion_sha256 {
                    return Err(fail("History finalization fields differ"));
                }
                if *id==current_id {
                    current_marker_binding=Some((marker_path.clone(),(finalization.original_marker.device,finalization.original_marker.inode)));
                }
            }
            generation.source_root=Some(intent.source_root);
            for p in &e.proofs {
                let evidence=read(&p.evidence.relative)?;
                if digest(&read(&p.manifest.relative)?)!=p.manifest.sha256||digest(&evidence)!=p.evidence.sha256 {
                    return Err(fail("History original proof bytes differ"));
                }
                if p.namespace==".attachment-upload-journal" {
                    let owner=crate::storage::attachment_upload::validate_history(&read(&p.manifest.relative)?,&evidence,&p.op_id)?;
                    let scope=l.scopes.iter().find(|s|s.namespace==p.namespace&&s.op_id==p.op_id).unwrap();
                    let anchor=format!(".attachment-upload-journal/{}/payload.part",p.op_id);
                    if !scope.bindings.iter().any(|b|b.relative==anchor)
                        ||scope.bindings.iter().any(|b|!owner.allowed_paths.contains(&b.relative)
                            ||(b.original_device,b.original_inode)!=owner.original_identity
                            ||b.length!=owner.intent.byte_length as u64||b.sha256!=owner.intent.binary_sha256) {
                        return Err(fail("History upload scoped original proof differs"));
                    }
                } else if p.namespace!=".move-relocation-journal" {
                    let proof:HistoryInstallProof=serde_json::from_slice(&evidence).map_err(|_|fail("Malformed history install proof"))?;
                    let scope=l.scopes.iter().find(|s|s.namespace==p.namespace&&s.op_id==p.op_id).unwrap();
                    if proof.op_id!=p.op_id||proof.ino==0||!proof.temp.strip_prefix("anchor-").is_some_and(valid_id)
                        ||scope.bindings.iter().any(|b|(b.original_device,b.original_inode)!=(proof.dev,proof.ino)) {
                        return Err(fail("History scoped original install proof differs"));
                    }
                }
                if let Some(m)=&p.mint_intent {if digest(&read(&m.relative)?)!=m.sha256 {return Err(fail("History mint provenance bytes differ"));}}
            }
        }
        if generations.insert(id.clone(),generation).is_some(){return Err(fail("Duplicate history generation"));}
    }
    let current=generations.get(&current_id).ok_or_else(||fail("Dangling history current pointer"))?;
    if (current.ledger.root_device,current.ledger.root_inode)!=expected_root {
        return Err(fail("History current captured root differs"));
    }
    let mut visited=std::collections::BTreeSet::new();
    let mut id=current_id.clone();
    loop {
        if !visited.insert(id.clone()) {return Err(fail("History parent cycle"));}
        let gen=&generations[&id];
        if let Some(e)=&gen.envelope {
            for a in &e.archives {
                let archived=generations.get(&a.generation_id).ok_or_else(||fail("History archive absent from SQL"))?;
                if digest(&archived.raw)!=a.ledger_sha256 {return Err(fail("History archive hash differs"));}
            }
            if let Some(p)=&e.parent {
                let previous=&generations[&p.generation_id].ledger;
                if gen.source_root.as_ref().map(|r|(r.device,r.inode))!=Some((previous.root_device,previous.root_inode)) {
                    return Err(fail("History parent source root differs"));
                }
                let prior=&generations[&p.generation_id];
                let is_denied=|ns:&str,op:&str|e.denied.iter().any(|d|d.namespace==ns&&d.op_id==op);
                for old in &prior.ledger.scopes {
                    match gen.ledger.scopes.iter().find(|s|s.namespace==old.namespace&&s.op_id==old.op_id) {
                        Some(new) => {
                            if new.evidence_sha256!=old.evidence_sha256 {
                                return Err(fail("History original evidence was reinterpreted"));
                            }
                            for b in &new.bindings {
                                let before=old.bindings.iter().find(|v|v.relative==b.relative)
                                    .ok_or_else(||fail("History predecessor binding missing"))?;
                                if (b.original_device,b.original_inode,b.length,&b.sha256)!=(before.original_device,before.original_inode,before.length,&before.sha256) {
                                    return Err(fail("History original binding was rewritten"));
                                }
                            }
                        }
                        None if is_denied(&old.namespace,&old.op_id)=>{},
                        None=>return Err(fail("History prior scope reservation was forgotten")),
                    }
                }
                if let Some(old)=&prior.envelope {
                    for d in &old.denied {
                        if !is_denied(&d.namespace,&d.op_id) {
                            return Err(fail("History denied reservation was forgotten"));
                        }
                    }
                }
                id=p.generation_id.clone();continue;
            }
        }
        break;
    }
    if visited.len()!=generations.len(){return Err(fail("Disconnected history generation"));}
    // Every envelope declares exactly its transitive predecessor closure, not arbitrary descendants.
    for (id,gen) in &generations {
        if let Some(e)=&gen.envelope {
            let mut ancestors=std::collections::BTreeSet::new();
            let mut parent=e.parent.as_ref();
            while let Some(p)=parent {
                if !ancestors.insert(p.generation_id.clone()){return Err(fail("History parent cycle"));}
                parent=generations[&p.generation_id].envelope.as_ref().and_then(|a|a.parent.as_ref());
            }
            if ancestors!=e.archives.iter().map(|a|a.generation_id.clone()).collect(){return Err(fail("History archive closure differs"));}
            if ancestors.contains(id){return Err(fail("History self ancestor"));}
        }
    }
    drop(read);
    if let Some((path,expected))=current_marker_binding {
        // Live root uses its held observed marker. A transported bundle uses
        // original captured source custody, never copied bundle inode numbers.
        // Historical generations preserve raw evidence without obsolete IDs.
        let observed=if let Some(ids)=source.captured_ids {
            *ids.get(&path).ok_or_else(||fail("Current marker captured PG custody missing"))?
        } else if let Some(b)=source.bundle {
            let captured=b.manifest.source_custody.files.0.get(&path).ok_or_else(||fail("Current marker captured custody missing"))?;
            (captured.device,captured.inode)
        } else {
            let record=source.records.get(&path).ok_or_else(||fail("Current marker held record missing"))?;
            (record.stamp.device,record.stamp.inode)
        };
        if observed!=expected {return Err(fail("Current finalized marker identity differs"));}
    }
    if source_files!=required_files||source_dirs!=required_dirs {return Err(fail("Unknown or incomplete history closure"));}
    source.fence()?;
    Ok(ClosedHistory{current_id,generations,source})
}

pub(crate) async fn inspect_history(bundle:&crate::storage::backup_inventory::ValidatedBundle,conn:&mut sqlx::AnyConnection)->io::Result<Option<ValidatedHistory>> {
    bundle.fence()?;
    let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table'").fetch_all(&mut *conn).await.map_err(|_|fail("Cannot inspect history schema"))?;
    let g=tables.iter().any(|s|s=="storage_restore_generations");let c=tables.iter().any(|s|s=="storage_restore_current");
    let custody=bundle.manifest.source_custody.directories.0.contains_key(NAMESPACE);
    if g!=c||(!g&&custody){return Err(fail("Incomplete history schema"));}
    if !g{bundle.fence()?;return Ok(None);}
    let(rows,pointer)=history_rows(conn).await?;
    if rows.is_empty()&&pointer.is_empty()&&!custody{bundle.fence()?;return Ok(None);}
    if !custody||pointer.len()!=1||pointer[0].0!=1{return Err(fail("Captured lineage pointer or custody missing"));}
    let root=Root::open(&bundle.data_path())?;
    let closure=history_closure(HistorySource::new(&root,Some(bundle))?,(bundle.manifest.source_custody.root.device,bundle.manifest.source_custody.root.inode),Some((&rows,&pointer)))?;
    if history_rows(conn).await?!=(rows.clone(),pointer){return Err(fail("History SQL changed during inspection"));}
    closure.source.fence()?;
    Ok(Some(ValidatedHistory{current_id:closure.current_id,generations:closure.generations,rows,captured_files:bundle.manifest.files.0.clone(),captured_ids:bundle.manifest.source_custody.files.0.clone()}))
}
/// Structural bootstrap check only. This cannot grant SQL commitment or register.
fn history_bootstrap(root:&Root)->io::Result<()> {
    if !exists(root.fd(),NAMESPACE)?{return root.check();}
    let closure=history_closure(HistorySource::new(root,None)?,identity(root.fd())?,None)?;
    closure.source.fence()
}

#[cfg(test)]
include!("restoration_history_fixture.rs");

#[cfg(test)]
mod admission_history_boundary_tests {
    use super::*;
    #[test] fn standalone_and_unfinalized_admission_owner_refuse() {
        let runtime=super::super::admission::tests::ledger(Some(super::super::admission::tests::fixture()),2);
        let raw=serde_json::to_vec(&runtime).unwrap();assert!(history_parse(&raw).is_err());
        let raw=serde_json::json!({"version":2,"runtime":runtime,"intent_sha256":"a".repeat(64),"completion_sha256":"b".repeat(64),"parent":null,"archives":[],"denied":[],"proofs":[]});
        assert!(history_parse(&serde_json::to_vec(&raw).unwrap()).is_err());
    }
    #[test] fn standalone_legacy_owner_stays_accepted() {
        let runtime=super::super::admission::tests::ledger(None,1);
        assert!(history_parse(&serde_json::to_vec(&runtime).unwrap()).is_ok());
    }
}

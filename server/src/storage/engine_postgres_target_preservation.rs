// A target-native preservation binding is minted only by actual restore
// materialization. It confers no native writer, serving, cleanup or absence role.
const PG_TARGET_NATIVE_PURPOSE: &str = "restored_pg_committed_direct_native_preservation_v1";
const PG_TARGET_NATIVE_PURPOSE_V2: &str = "restored_pg_committed_direct_replay_native_preservation_v2";
const PG_TARGET_NATIVE_LIMIT: u64 = 8 * 1024 * 1024;

#[derive(Clone,serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct PgTargetNativeStamp {
    device:u64, inode:u64, uid:u32, gid:u32, mode:u32, links:u64, length:u64,
    mtime:i64, mtime_ns:i64, ctime:i64, ctime_ns:i64,
}
impl PgTargetNativeStamp {
    fn actual(file:&File)->io::Result<Self> {
        let s=physical_stamp(file)?;let m=file.metadata()?;
        Ok(Self{device:s.device,inode:s.inode,uid:m.uid(),gid:m.gid(),mode:s.mode,
            links:s.links,length:s.length,mtime:s.mtime,mtime_ns:s.mtime_ns,ctime:s.ctime,ctime_ns:s.ctime_ns})
    }
    fn check(&self,file:&File,directory:bool)->anyhow::Result<()> {
        let m=file.metadata()?;
        ensure!(*self==Self::actual(file)?&&m.uid()==unsafe{libc::geteuid()}
            &&if directory{m.is_dir()&&m.mode()&0o7777==0o700}else{m.is_file()&&m.mode()&0o7777==0o600&&m.nlink()==1},
            "Original target-native identity/stamp/private role differs");Ok(())
    }
}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgTargetNativeFile {stamp:PgTargetNativeStamp,bytes:crate::storage::backup_inventory::Bytes}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgTargetNativeInventory {
    version:u32,purpose:String,restore_id:String,backup_id:String,generation_id:String,
    source_manifest_sha256:String,target:EngineTargetIdentity,root_device:u64,root_inode:u64,
    operations:Vec<String>,directories:PgUniqueMap<PgTargetNativeStamp>,files:PgUniqueMap<PgTargetNativeFile>,
}
fn pg_target_native_record(restore:&str)->String {
    format!(".pg-restoration-custody/{restore}/target-native-preservation.json")
}
impl PgTargetNativeInventory {
    fn validate(&self)->anyhow::Result<()> {
        ensure!((self.version==1&&self.purpose==PG_TARGET_NATIVE_PURPOSE)||(self.version==2&&self.purpose==PG_TARGET_NATIVE_PURPOSE_V2),
            "Unsupported target-native preservation purpose");
        for id in [&self.restore_id,&self.backup_id,&self.generation_id]{pg_snapshot_uuid(id)?;}
        ensure!(pg_snapshot_valid_hash(&self.source_manifest_sha256)&&!self.operations.is_empty()
            &&self.operations.len()<=512&&self.operations.windows(2).all(|p|p[0]<p[1]),
            "Target-native operation/digest bound differs");
        for id in &self.operations{pg_snapshot_uuid(id)?;}
        let namespace=".attachment-upload-admission";
        let expected=std::iter::once(namespace.to_owned()).chain(self.operations.iter().map(|id|format!("{namespace}/{id}"))).collect::<std::collections::BTreeSet<_>>();
        ensure!(self.directories.0.keys().cloned().collect::<std::collections::BTreeSet<_>>()==expected,
            "Target-native complete directory roles differ");
        let mut total=0u64;
        for id in &self.operations {
            let prefix=format!("{namespace}/{id}/");let names=self.files.0.keys().filter_map(|p|p.strip_prefix(&prefix)).collect::<Vec<_>>();
            ensure!(!names.is_empty()&&names.len()<=32&&names.contains(&"claim.json")
                &&names.iter().all(|n|if self.version==1{["claim.json","stage-origin.json"].contains(n)}else{["claim.json","stage-origin.json","replay-blob-origin.json","replay-rewrite-plan.json","replay-rewrite-origin.json"].contains(n)}),
                "Target-native complete private record roles differ");
        }
        ensure!(self.files.0.len()<=512*32,"Target-native file count bound exceeded");
        for(path,file)in &self.files.0 {
            ensure!(self.operations.iter().any(|id|path.strip_prefix(&format!("{namespace}/{id}/")).is_some_and(|p|!p.contains('/')))
                &&file.bytes.size_bytes<=8*1024*1024&&file.bytes.size_bytes==file.stamp.length
                &&pg_snapshot_valid_hash(&file.bytes.sha256),"Target-native file role/size differs");
            total=total.checked_add(file.bytes.size_bytes).context("Target-native private record total overflow")?;
            ensure!(total<=16*1024*1024,"Target-native private record total exceeds16MiB");
        }
        ensure!(serde_json::to_vec(self)?.len()as u64<=PG_TARGET_NATIVE_LIMIT,
            "Target-native canonical inventory exceeds8MiB");Ok(())
    }
}

fn pg_target_native_names(directory:&File,limit:usize)->anyhow::Result<std::collections::BTreeSet<String>> {
    let mut names=std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(format!("/proc/self/fd/{}",directory.as_raw_fd()))? {
        ensure!(names.len()<limit,"Target-native directory entry bound exceeded");
        let name=entry?.file_name().into_string().map_err(|_|fail("Target-native nonUTF8 entry"))?;
        ensure!(names.insert(name),"Target-native duplicate native entry");
    }Ok(names)
}
struct PgTargetNativeHeld {
    inventory:PgTargetNativeInventory,root:File,
    directories:BTreeMap<String,File>,files:BTreeMap<String,File>,record:File,record_stamp:PhysicalStamp,
    raw:Vec<u8>,
}
impl PgTargetNativeHeld {
    fn fence(&self,root:&Root)->anyhow::Result<()> {
        self.inventory.validate()?;
        let expected=(self.inventory.root_device,self.inventory.root_inode);
        ensure!(identity(root.fd())?==expected&&identity(&self.root)?==expected,
            "Original target-native root differs");
        let namespace=root.directory(".attachment-upload-admission")?;
        ensure!(pg_target_native_names(&namespace,512)?==self.inventory.operations.iter().cloned().collect(),
            "Original target-native complete namespace differs");
        for(path,stamp)in &self.inventory.directories.0 {
            let named=root.directory(path)?;stamp.check(&self.directories[path],true)?;stamp.check(&named,true)?;
            if path!=".attachment-upload-admission" {
                let prefix=format!("{path}/");let expected=self.inventory.files.0.keys().filter_map(|p|p.strip_prefix(&prefix).map(str::to_owned)).collect();
                ensure!(pg_target_native_names(&named,32)?==expected,"Original target-native capsule topology differs");
            }
        }
        let mut has_replay=false;
        for id in &self.inventory.operations {
            let prefix=format!(".attachment-upload-admission/{id}/");
            let claim_path=format!("{prefix}claim.json");
            let raw=read_ledger(&self.files[&claim_path])?;
            let claim:serde_json::Value=serde_json::from_slice(&raw)?;
            let producer:crate::storage::attachment_upload::admission::Producer=serde_json::from_value(claim["producer"].clone())?;
            let replay=matches!(producer,crate::storage::attachment_upload::admission::Producer::Replay{..});has_replay|=replay;
            ensure!(!replay||self.inventory.version==2,"Replay cannot use Direct-only native publication");
            if replay{ensure!(self.files.contains_key(&format!("{prefix}replay-blob-origin.json")),"Replay native publication lacks original blob role");}
            for(path,file)in self.files.iter().filter(|(path,_)|path.starts_with(&prefix)){
                let name=path.strip_prefix(&prefix).context("Native record prefix differs")?;
                let allowed=if replay{["claim.json","replay-blob-origin.json","replay-rewrite-plan.json","replay-rewrite-origin.json"].contains(&name)}else{["claim.json","stage-origin.json"].contains(&name)};
                ensure!(allowed,"Native record role differs from original producer");
                crate::storage::attachment_upload::admission::validate_preserved_capsule_record(name,&read_ledger(file)?,&raw)?;
            }
        }
        ensure!(has_replay==(self.inventory.version==2),"Native publication version differs from actual producer class");
        for(path,binding)in &self.inventory.files.0 {
            let named=root.leaf(path)?;binding.stamp.check(&self.files[path],false)?;binding.stamp.check(&named,false)?;
            ensure!(planned_hash(&self.files[path])?==binding.bytes&&planned_hash(&named)?==binding.bytes,
                "Original target-native private record payload differs");
            binding.stamp.check(&self.files[path],false)?;binding.stamp.check(&named,false)?;
        }
        for(path,stamp)in &self.inventory.directories.0{stamp.check(&self.directories[path],true)?;stamp.check(&root.directory(path)?,true)?;}
        let name=pg_target_native_record(&self.inventory.restore_id);let record=root.leaf(&name)?;
        ensure!(physical_stamp(&self.record)?==self.record_stamp&&physical_stamp(&record)?==self.record_stamp
            &&read_ledger(&self.record)?==self.raw&&read_ledger(&record)?==self.raw,
            "Original target-native publication record changed");
        Ok(())
    }
}
// This type originates only from actual held new-destination roles, never names.
struct TargetCommittedPreservationPublication {held:PgTargetNativeHeld,key:Vec<u8>}
impl TargetCommittedPreservationPublication {
    fn mint(imported:&mut ImportedPgRestore,generation:&str)->anyhow::Result<Option<Self>> {
        let p=&mut imported.prepared;p.fence()?;
        let m=&p.snapshot.manifest;
        if m.admission_owner_capture.is_some()||m.prior_admission_owner_capture.is_some()
            ||m.completed_absence_capture.is_some()||m.prior_completed_absence_capture.is_some()
            ||m.mixed_completed_absence_capture.is_some()||m.composite_admission_capture.is_some(){return Ok(None)}
        let Some(table)=p.snapshot.tables.get("attachment_capacity_reservations")else{return Ok(None)};
        if table.rows.is_empty(){return Ok(None)}
        let column=|name:&str|table.columns.iter().position(|c|c.name==name).context("Target-native captured reservation column absent");
        let id_column=column("operation_id")?;let state_column=column("state")?;
        ensure!(table.rows.iter().all(|r|r.len()==table.columns.len()),"Target-native captured row width differs");
        let states=table.rows.iter().map(|r|text(&r[state_column])).collect::<anyhow::Result<Vec<_>>>()?;
        if states.iter().any(|s|*s!="committed"){return Ok(None)}
        let ids=table.rows.iter().map(|r|text(&r[id_column]).map(str::to_owned)).collect::<anyhow::Result<std::collections::BTreeSet<_>>>()?;
        ensure!(ids.len()==table.rows.len()&&ids.len()<=512,"Target-native complete operation set differs");
        let captured=OriginalRestoredAdmissionPreservation::original_ids(m);
        ensure!(ids==captured,"Target-native original namespace/current reservations differ");
        let mut total=0usize;let mut claims=BTreeMap::new();let mut replays=BTreeMap::new();
        for row in &table.rows {
            let id=text(&row[id_column])?;pg_snapshot_uuid(id)?;
            let path=format!(".attachment-upload-admission/{id}/claim.json");
            let claim=p.snapshot.held.roles.get(&format!("files/{path}")).context("Target-native original claim role absent")?;
            let raw=read_ledger(&claim.held)?;let value:serde_json::Value=serde_json::from_slice(&raw)?;
            let producer:crate::storage::attachment_upload::admission::Producer=serde_json::from_value(value.get("producer").context("Target-native original producer absent")?.clone())?;
            ensure!(text(&row[column("admission_claim_sha256")?])?==digest(&raw)
                &&text(&row[column("request_sha256")?])?==digest(&serde_json::to_vec(&producer)?),
                "Target-native original claim/reservation association differs");
            let integer=|name:&str|->anyhow::Result<i64>{match row[column(name)?]{Cell::Integer(v)=>Ok(v),_=>bail!("Target-native captured reservation integer type differs")}};
            let optional_text=|name:&str|->anyhow::Result<Option<String>>{match &row[column(name)?]{Cell::Null=>Ok(None),Cell::Text(v)=>Ok(Some(std::str::from_utf8(v)?.into())),_=>bail!("Target-native captured reservation optional text differs")}};
            let committed_bytes=match row[column("committed_bytes")?]{Cell::Null=>None,Cell::Integer(v)=>Some(v),_=>bail!("Target-native captured committed byte type differs")};
            let reservation=crate::storage::restoration::admission::CapturedReservation{operation_id:id.into(),request_sha256:text(&row[column("request_sha256")?])?.into(),admission_claim_sha256:text(&row[column("admission_claim_sha256")?])?.into(),maximum_bytes:integer("maximum_bytes")?,state:text(&row[state_column])?.into(),committed_asset_id:optional_text("committed_asset_id")?,committed_bytes,disposition_json:optional_text("disposition_json")?,disposition_sha256:optional_text("disposition_sha256")?};
            reservation.matches_claim(&raw)?;
            if let Some(replay)=preserved_replay_receipt(&producer,&reservation,&p.snapshot.tables)?{
                preserved_page_incarnation(&p.snapshot.tables,&imported.expected_target_tables,&replay.space_id,&replay.document_id)?;
                replays.insert(id.to_owned(),replay);
            }
            claims.insert(id.to_owned(),raw.clone());
            total=total.checked_add(raw.len()).context("Target-native claim bytes overflow")?;
            ensure!(total<=16*1024*1024,"Target-native claim bytes exceed bound");
        }
        let version=if replays.is_empty(){1}else{2};
        if version==2{
            preserved_tables_match(&p.snapshot.tables,&imported.expected_target_tables)?;
            let origin=original_copied_claim_manifest(&p.snapshot.held.manifest,&claims,&|path|{
                let held=p.snapshot.held.roles.get(&format!("files/{path}")).context("Authenticated original copied source role absent")?;
                ensure!(planned_hash(&held.held)?==held.bytes,"Authenticated original copied source role changed");
                Ok(read_ledger(&held.held)?)
            })?;
            for(id,replay)in &replays{
                let held=p.snapshot.held.roles.get(&format!("files/.attachment-upload-admission/{id}/replay-blob-origin.json")).context("Original Replay blob role absent")?;
                let parent=replay.storage_path.rsplit_once('/').context("Original Replay path invalid")?.0;
                preserved_replay_blob(&p.destination.root,&read_ledger(&held.held)?,&claims[id],replay,&origin.directory(parent)?,&origin.payload(&replay.storage_path)?)?;
            }
        }
        // Bounded class preflight: no new inventory phase output without its
        // actual configured application key. Never serialize or log this key.
        let key=pg_target_final_key()?;
        let d=&mut p.destination;d.fence()?;
        let namespace=".attachment-upload-admission";
        let directory_names=std::iter::once(namespace.to_owned()).chain(ids.iter().map(|id|format!("{namespace}/{id}"))).collect::<std::collections::BTreeSet<_>>();
        let actual_directories=d.directories.keys().filter(|s|s.as_str()==namespace||s.starts_with(&format!("{namespace}/"))).cloned().collect::<std::collections::BTreeSet<_>>();
        ensure!(actual_directories==directory_names,"Target-native actual materialized directory set differs");
        let mut directories=BTreeMap::new();let mut directory_stamps=BTreeMap::new();
        for path in directory_names {
            let(file,stamp)=d.directories.get(&path).context("Actual materialized target-native directory absent")?;
            ensure!(physical_stamp(file)?==*stamp,"Actual materialized target-native directory changed");
            let wire=PgTargetNativeStamp::actual(file)?;wire.check(file,true)?;
            directories.insert(path.clone(),file.try_clone()?);directory_stamps.insert(path,wire);
        }
        let mut files=BTreeMap::new();let mut bindings=BTreeMap::new();let mut private_bytes=0u64;
        for(path,role)in d.roles.iter().filter(|(path,_)|path.starts_with(&format!("{namespace}/"))) {
            ensure!(files.len()<512*32&&role.bytes.size_bytes<=8*1024*1024,"Target-native materialized file bound exceeded");
            private_bytes=private_bytes.checked_add(role.bytes.size_bytes).context("Target-native materialized byte count overflow")?;
            ensure!(private_bytes<=16*1024*1024,"Target-native materialized private bytes exceed16MiB");
            ensure!(physical_stamp(&role.held)?==role.stamp&&planned_hash(&role.held)?==role.bytes,
                "Actual materialized target-native leaf changed");
            let wire=PgTargetNativeStamp::actual(&role.held)?;wire.check(&role.held,false)?;
            let captured=p.snapshot.held.roles.get(&format!("files/{path}")).context("Actual native leaf lacks authenticated original")?;
            ensure!(role.bytes==captured.bytes,"Actual target-native leaf differs from original snapshot");
            let relative=path.strip_prefix(&format!("{namespace}/")).context("Target-native capsule path differs")?;
            let(id,name)=relative.split_once('/').context("Target-native private record path differs")?;
            let is_replay=replays.contains_key(id);
            let allowed=if is_replay{["claim.json","replay-blob-origin.json","replay-rewrite-plan.json","replay-rewrite-origin.json"].contains(&name)}else{["claim.json","stage-origin.json"].contains(&name)};
            ensure!(allowed,"Actual materialized role differs from original producer");
            let claim=claims.get(id).context("Target-native private record has no original reservation")?;
            crate::storage::attachment_upload::admission::validate_preserved_capsule_record(name,&read_ledger(&role.held)?,claim)?;
            files.insert(path.clone(),role.held.try_clone()?);bindings.insert(path.clone(),PgTargetNativeFile{stamp:wire,bytes:role.bytes.clone()});
        }
        let inventory=PgTargetNativeInventory{version,purpose:if version==1{PG_TARGET_NATIVE_PURPOSE}else{PG_TARGET_NATIVE_PURPOSE_V2}.into(),restore_id:p.restore_id.clone(),
            backup_id:m.backup_id.clone(),generation_id:generation.into(),source_manifest_sha256:digest(&p.snapshot.held.manifest),
            target:imported.target.clone(),root_device:d.root_identity.0,root_inode:d.root_identity.1,operations:ids.into_iter().collect(),
            directories:PgUniqueMap(directory_stamps),files:PgUniqueMap(bindings)};
        inventory.validate()?;let raw=serde_json::to_vec(&inventory)?;
        ensure!(raw.len()as u64<=PG_TARGET_NATIVE_LIMIT,"Target-native publication exceeds canonical bound");
        let record_path=pg_target_native_record(&p.restore_id);
        pg_token_write(d,&record_path,&raw,".starlane-restore-incomplete")?;
        let record=d.roles.get(&record_path).context("Actual target-native publication not retained")?;
        let owner=Self{held:PgTargetNativeHeld{inventory,root:d.root.fd().try_clone()?,directories,files,
            record:record.held.try_clone()?,record_stamp:record.stamp.clone(),raw},key};
        owner.fence(d)?;p.snapshot.held.fence()?;d.fence()?;Ok(Some(owner))
    }
    fn fence(&self,destination:&OwnedPgBackup)->anyhow::Result<()> {
        destination.fence()?;
        let marker=if destination.roles.contains_key(".starlane-restore-incomplete"){".starlane-restore-incomplete".to_owned()}
            else{format!("{}/generations/{}/retired-marker",NAMESPACE,self.held.inventory.generation_id)};
        pg_token_guard(destination,&marker)?;self.held.fence(&destination.root)?;
        pg_token_guard(destination,&marker)?;destination.fence()?;Ok(())
    }
}

// Recovery consumes this actual current target's authenticated publication only.
// Parsed inventory bytes, or an old predecessor record, cannot construct it.
struct TargetCommittedPreservationOwner {proof:Arc<RestoredPgAdmission>,held:PgTargetNativeHeld}
impl TargetCommittedPreservationOwner {
    fn recover(proof:&Arc<RestoredPgAdmission>)->anyhow::Result<Self> {
        proof.physical_fence()?;let path=pg_target_native_record(&proof.attestation.restore_id);
        let record=proof.records.get(&path).context("Legacy target has no original native preservation publication; no automatic upgrade")?;
        ensure!(proof.attestation.retained_roles.0.get(&path)==Some(&record.bytes)
            &&record.bytes.size_bytes<=PG_TARGET_NATIVE_LIMIT,"Target-native record lacks actual current attestation role");
        let raw=read_ledger(&record.held)?;let inventory:PgTargetNativeInventory=serde_json::from_slice(&raw)?;
        ensure!(serde_json::to_vec(&inventory)?==raw,"Target-native inventory is not canonical");inventory.validate()?;
        let e=&proof.attestation;
        ensure!(inventory.restore_id==e.restore_id&&inventory.backup_id==e.backup_id
            &&inventory.generation_id==e.custody_generation_id&&inventory.source_manifest_sha256==e.snapshot_sha256
            &&inventory.target==e.target&&(inventory.root_device,inventory.root_inode)==(e.root_device,e.root_inode),
            "Target-native publication belongs to another original target/generation");
        let mut directories=BTreeMap::new();let mut files=BTreeMap::new();
        for(path,stamp)in &inventory.directories.0{let f=proof.root.directory(path)?;stamp.check(&f,true)?;directories.insert(path.clone(),f);}
        for(path,binding)in &inventory.files.0{let f=proof.root.leaf(path)?;binding.stamp.check(&f,false)?;
            ensure!(planned_hash(&f)?==binding.bytes,"Original target-native preserved bytes differ");binding.stamp.check(&f,false)?;files.insert(path.clone(),f);}
        let held=PgTargetNativeHeld{inventory,root:proof.root.fd().try_clone()?,directories,files,
            record:record.held.try_clone()?,record_stamp:record.stamp.clone(),raw};
        let owner=Self{proof:proof.clone(),held};owner.fence()?;Ok(owner)
    }
    fn fence(&self)->anyhow::Result<()>{self.proof.physical_fence()?;self.held.fence(&self.proof.root)?;self.proof.physical_fence()?;Ok(())}
}

// Final acknowledgment is separate from its own retained_roles to avoid a hash
// cycle. Only the actual retired publisher may create it; readers never repair it.
const PG_TARGET_FINAL_DOMAIN:&[u8]=b"StarlaneNotes/pg-target-native-preservation/finalized/v1\0";
const PG_TARGET_FINAL_LIMIT:usize=16384;
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgTargetFinalFact {
 version:u32,phase:String,purpose:String,restore_id:String,backup_id:String,generation_id:String,
 target:EngineTargetIdentity,root_device:u64,root_inode:u64,
 attestation_sha256:String,inventory_sha256:String,inventory_stamp:PgTargetNativeStamp,
 completion_sha256:String,finalization_sha256:String,
}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgTargetFinalAck{fact:PgTargetFinalFact,mac:String}
fn pg_target_final_path(id:&str)->String{format!(".pg-restoration-custody/{id}/target-native-preservation.complete.json")}
fn pg_target_final_required(e:&PgTargetAttestation)->bool{e.retained_roles.0.contains_key(&pg_target_native_record(&e.restore_id))}
fn pg_target_final_key()->anyhow::Result<Vec<u8>>{
 let key=std::env::var("JWT_SECRET").context("Target-native final acknowledgment application key unavailable")?;
 ensure!(key.len()>=32,"Target-native final acknowledgment key unavailable");Ok(key.into_bytes())
}
fn pg_target_final_fact(e:&PgTargetAttestation,attestation:&[u8],inventory:&[u8],stamp:PgTargetNativeStamp)->anyhow::Result<PgTargetFinalFact>{
 let path=pg_target_native_record(&e.restore_id);
 let role=e.retained_roles.0.get(&path).context("Current target-native inventory role absent")?;
 ensure!(role.sha256==digest(inventory)&&role.size_bytes==inventory.len()as u64&&inventory.len()as u64<=PG_TARGET_NATIVE_LIMIT,
  "Target-native final inventory not bound to exact attestation");
 ensure!(!e.retained_roles.0.contains_key(&pg_target_final_path(&e.restore_id)),"Current final acknowledgment cannot retain its own hash");
 let typed:PgTargetNativeInventory=serde_json::from_slice(inventory)?;typed.validate()?;
 ensure!(serde_json::to_vec(&typed)?==inventory,"Final native inventory is not canonical");
 Ok(PgTargetFinalFact{version:typed.version,phase:"FINALIZED".into(),purpose:typed.purpose,restore_id:e.restore_id.clone(),backup_id:e.backup_id.clone(),generation_id:e.custody_generation_id.clone(),target:e.target.clone(),root_device:e.root_device,root_inode:e.root_inode,attestation_sha256:digest(attestation),inventory_sha256:digest(inventory),inventory_stamp:stamp,completion_sha256:e.completion_sha256.clone(),finalization_sha256:e.finalization_sha256.clone()})
}
fn pg_target_final_mac(fact:&PgTargetFinalFact,key:&[u8])->anyhow::Result<String>{
 use hmac::Mac;ensure!(key.len()>=32,"Target-native final key unavailable");let raw=serde_json::to_vec(fact)?;
 ensure!(raw.len()<=PG_TARGET_FINAL_LIMIT,"Target-native final fact exceeds bound");
 let mut mac=hmac::Hmac::<sha2::Sha256>::new_from_slice(key).map_err(|_|fail("Target-native final key invalid"))?;
 mac.update(PG_TARGET_FINAL_DOMAIN);mac.update(&raw);Ok(hex::encode(mac.finalize().into_bytes()))
}
fn pg_target_final_verify(raw:&[u8],e:&PgTargetAttestation,attestation:&[u8],inventory:&[u8],original_inventory:Option<(u64,u64)>,key:&[u8])->anyhow::Result<PgTargetFinalAck>{
 use hmac::Mac;ensure!(raw.len()<=PG_TARGET_FINAL_LIMIT&&key.len()>=32,"Target-native final acknowledgment unavailable");
 let ack:PgTargetFinalAck=serde_json::from_slice(raw)?;
 ensure!(serde_json::to_vec(&ack)?==raw&&pg_snapshot_valid_hash(&ack.mac),"Target-native final acknowledgment is not canonical/authenticated");
 let expected=pg_target_final_fact(e,attestation,inventory,ack.fact.inventory_stamp.clone())?;
 ensure!(serde_json::to_vec(&ack.fact)?==serde_json::to_vec(&expected)?,"Target-native final phase/purpose/target/attestation differs");
 if let Some(id)=original_inventory{ensure!((ack.fact.inventory_stamp.device,ack.fact.inventory_stamp.inode)==id,"Target-native final original inventory identity differs");}
 let mut mac=hmac::Hmac::<sha2::Sha256>::new_from_slice(key).map_err(|_|fail("Target-native final key invalid"))?;
 mac.update(PG_TARGET_FINAL_DOMAIN);mac.update(&serde_json::to_vec(&ack.fact)?);
 mac.verify_slice(&hex::decode(&ack.mac)?).map_err(|_|fail("Target-native final acknowledgment authentication unavailable"))?;Ok(ack)
}
fn pg_target_final_current(root:&Root,e:&PgTargetAttestation,attestation:&[u8])->anyhow::Result<Option<EngineHeldRecord>>{
 if !pg_target_final_required(e){return Ok(None)}
 let inventory=root.leaf(&pg_target_native_record(&e.restore_id))?;let original=PgTargetNativeStamp::actual(&inventory)?;
 original.check(&inventory,false)?;ensure!(original.length<=PG_TARGET_NATIVE_LIMIT,"Target-native inventory exceeds bound");let raw_inventory=read_ledger(&inventory)?;
 let path=pg_target_final_path(&e.restore_id);let held=root.leaf(&path)?;let stamp=physical_stamp(&held)?;
 ensure!(held.metadata()?.uid()==unsafe{libc::geteuid()}&&stamp.mode&0o7777==0o600&&stamp.links==1&&stamp.length<=PG_TARGET_FINAL_LIMIT as u64,"Target-native final acknowledgment private role/size differs");
 let raw=read_ledger(&held)?;let ack=pg_target_final_verify(&raw,e,attestation,&raw_inventory,Some((original.device,original.inode)),&pg_target_final_key()?)?;
 ensure!(ack.fact.inventory_stamp==original,"Target-native final inventory stamp changed");
 original.check(&inventory,false)?;
 ensure!(physical_stamp(&held)?==stamp&&physical_stamp(&root.leaf(&path)?)?==stamp&&read_ledger(&root.leaf(&path)?)?==raw,"Target-native final acknowledgment changed during read");
 Ok(Some(EngineHeldRecord{held,stamp,bytes:crate::storage::backup_inventory::Bytes{size_bytes:raw.len()as u64,sha256:digest(&raw)}}))
}
// Historical receipts authenticate exact original bytes only, never copied native
// identities as current target ownership. Current admission checks stamps above.
fn pg_target_final_archive(e:&PgTargetAttestation,raw:&[u8],records:&BTreeMap<String,EngineHeldRecord>)->anyhow::Result<()> {
 if !pg_target_final_required(e){return Ok(())}
 let inventory=read_ledger(&records.get(&pg_target_native_record(&e.restore_id)).context("Historical target-native inventory absent")?.held)?;
 let record=records.get(&pg_target_final_path(&e.restore_id)).context("Historical target-native final acknowledgment absent")?;
 ensure!(record.bytes.size_bytes<=PG_TARGET_FINAL_LIMIT as u64,"Historical final acknowledgment exceeds bound");let ack=read_ledger(&record.held)?;
 pg_target_final_verify(&ack,e,raw,&inventory,None,&pg_target_final_key()?)?;Ok(())
}
// This is the only signing call site. It consumes the actual private retired
// publication; no public path/bool/JSON signer exists.
async fn publish_pg_target_final(retired:&mut RetiredPgCustody,url:&str)->anyhow::Result<()> {
 let Some(owner)=retired.committed.native_preservation.as_ref()else{return Ok(())};
 let raw=&retired.committed.attestation;let e:PgTargetAttestation=serde_json::from_slice(raw)?;let key=&owner.key;
 let imported=&mut retired.committed.sealed.archived.materialized.owner.imported;
 owner.fence(&imported.prepared.destination)?;imported.prepared.snapshot.held.fence()?;
 let mut conn=sqlx::PgConnection::connect(url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&e.target.schema_name)?)).execute(&mut conn).await?;
 let mut tx=conn.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;
 // A snapshot alone cannot exclude a writer between final observation and ACK.
 for n in imported.expected_target_tables.keys(){sqlx::query(&format!("LOCK TABLE {} IN SHARE MODE",quoted(n)?)).execute(&mut *tx).await?;}
 for n in ["pg_restoration_imports","pg_restoration_generations","pg_restoration_current"]{sqlx::query(&format!("LOCK TABLE {} IN SHARE MODE",quoted(n)?)).execute(&mut *tx).await?;}
 pg_attestation_sql(&mut tx,&e,raw).await?;
 let rows=pg_tables(&mut tx,&shape()?).await?;
 for(n,t)in &imported.expected_target_tables{if !["storage_restore_generations","storage_restore_current"].contains(&n.as_str()){ensure!(rows.get(n)==Some(t),"Application SQL changed before final target acknowledgment");}}
 imported.prepared.snapshot.held.fence()?;owner.fence(&imported.prepared.destination)?;
 let d=&mut imported.prepared.destination;
 ensure!(read_ledger(&d.root.leaf(".pg-restoration-custody/current.json")?)?==*raw,"Final actual raw attestation changed");
 let finalization=format!("{NAMESPACE}/generations/{}/finalization.json",e.custody_generation_id);
 ensure!(digest(&read_ledger(&d.root.leaf(&finalization)?)?)==e.finalization_sha256,"Actual durable finalization changed");
 let fact=pg_target_final_fact(&e,raw,&owner.held.raw,PgTargetNativeStamp::actual(&owner.held.record)?)?;
 let mac=pg_target_final_mac(&fact,&key)?;let ack=serde_json::to_vec(&PgTargetFinalAck{fact,mac})?;
 ensure!(ack.len()<=PG_TARGET_FINAL_LIMIT,"Final target acknowledgment exceeds bound");
 let marker=format!("{NAMESPACE}/generations/{}/retired-marker",e.custody_generation_id);
 pg_token_write(d,&pg_target_final_path(&e.restore_id),&ack,&marker)?;
 pg_target_final_current(&d.root,&e,raw)?.context("Actual final acknowledgment readback absent")?;
 pg_attestation_sql(&mut tx,&e,raw).await?;owner.fence(d)?;imported.prepared.snapshot.held.fence()?;
 tx.rollback().await?;conn.close().await?;Ok(())
}

#[cfg(test)]mod pg_target_final_ack_controls{
 use super::*;
 const KEY:&[u8]=b"fixture-final-target-application-key-32bytes";
 fn stamp()->PgTargetNativeStamp{PgTargetNativeStamp{device:11,inode:12,uid:1000,gid:1000,mode:libc::S_IFREG|0o600,links:1,length:18,mtime:10,mtime_ns:11,ctime:12,ctime_ns:13}}
 // Pure wire/MAC fixture, never an actual retired publication/target owner.
 fn inputs()->(PgTargetAttestation,Vec<u8>,Vec<u8>){inputs_for(1)}
 fn inputs_for(version:u32)->(PgTargetAttestation,Vec<u8>,Vec<u8>){
  let id="11111111-1111-4111-8111-111111111111";
  let namespace=".attachment-upload-admission";let capsule=format!("{namespace}/{id}");
  let mut directory=stamp();directory.mode=libc::S_IFDIR|0o700;
  let directories=[(namespace.into(),directory.clone()),(capsule.clone(),directory)].into_iter().collect();
  let mut file=stamp();file.length=5;
  let mut files=BTreeMap::new();files.insert(format!("{capsule}/claim.json"),PgTargetNativeFile{stamp:file.clone(),bytes:crate::storage::backup_inventory::Bytes{size_bytes:5,sha256:digest(b"claim")}});
  if version==2{files.insert(format!("{capsule}/replay-blob-origin.json"),PgTargetNativeFile{stamp:file,bytes:crate::storage::backup_inventory::Bytes{size_bytes:5,sha256:digest(b"typed")}});}
  let typed=PgTargetNativeInventory{version,purpose:if version==1{PG_TARGET_NATIVE_PURPOSE}else{PG_TARGET_NATIVE_PURPOSE_V2}.into(),restore_id:id.into(),backup_id:"22222222-2222-4222-8222-222222222222".into(),generation_id:"33333333-3333-4333-8333-333333333333".into(),source_manifest_sha256:"a".repeat(64),target:EngineTargetIdentity{cluster_system_identifier:"1".into(),database_oid:2,schema_oid:4,schema_name:"fixture_target".into()},root_device:7,root_inode:8,operations:vec![id.into()],directories:PgUniqueMap(directories),files:PgUniqueMap(files)};
  typed.validate().unwrap();let inventory=serde_json::to_vec(&typed).unwrap();
  let mut roles=BTreeMap::new();roles.insert(pg_target_native_record(id),crate::storage::backup_inventory::Bytes{size_bytes:inventory.len()as u64,sha256:digest(&inventory)});
  let e=PgTargetAttestation{version:1,restore_id:id.into(),backup_id:"22222222-2222-4222-8222-222222222222".into(),snapshot_sha256:"a".repeat(64),source_target:EngineTargetIdentity{cluster_system_identifier:"1".into(),database_oid:2,schema_oid:3,schema_name:"fixture_source".into()},target:EngineTargetIdentity{cluster_system_identifier:"1".into(),database_oid:2,schema_oid:4,schema_name:"fixture_target".into()},source_root_device:5,source_root_inode:6,root_device:7,root_inode:8,custody_generation_id:"33333333-3333-4333-8333-333333333333".into(),custody_ledger_sha256:"b".repeat(64),custody_sql_sha256:"c".repeat(64),import_commitment_sha256:"d".repeat(64),seal_intent_sha256:"e".repeat(64),completion_sha256:"f".repeat(64),finalization_sha256:"0".repeat(64),marker_device:9,marker_inode:10,control_relation_ids:BTreeMap::new(),catalogue_sha256:"1".repeat(64),independent_migrations_sha256:"2".repeat(64),retained_roles:crate::storage::backup_inventory::UniqueMap(roles),parent_attestation:None};
  let raw=serde_json::to_vec(&e).unwrap();(e,raw,inventory)
 }
 fn bound_stamp(inventory:&[u8])->PgTargetNativeStamp{let mut s=stamp();s.length=inventory.len()as u64;s}
 fn signed(e:&PgTargetAttestation,raw:&[u8],inventory:&[u8])->Vec<u8>{let fact=pg_target_final_fact(e,raw,inventory,bound_stamp(inventory)).unwrap();let mac=pg_target_final_mac(&fact,KEY).unwrap();serde_json::to_vec(&PgTargetFinalAck{fact,mac}).unwrap()}
 #[test]fn final_ack_requires_exact_phase_purpose_and_all_bound_values()->anyhow::Result<()>{
  let(e,raw,inventory)=inputs();let ack=signed(&e,&raw,&inventory);pg_target_final_verify(&ack,&e,&raw,&inventory,Some((11,12)),KEY)?;
  let mut prepared=pg_target_final_fact(&e,&raw,&inventory,bound_stamp(&inventory))?;prepared.phase="PREPARED".into();let mac=pg_target_final_mac(&prepared,KEY)?;let signed_prepared=serde_json::to_vec(&PgTargetFinalAck{fact:prepared,mac})?;ensure!(pg_target_final_verify(&signed_prepared,&e,&raw,&inventory,None,KEY).is_err(),"Authenticated prepared phase became FINALIZED");
  for field in ["version","phase","purpose","restore_id","backup_id","generation_id","target","root_device","root_inode","attestation_sha256","inventory_sha256","inventory_stamp","completion_sha256","finalization_sha256"]{
   let mut changed:serde_json::Value=serde_json::from_slice(&ack)?;changed["fact"][field]=match field{"version"|"root_device"|"root_inode"=>99.into(),"target"=>serde_json::json!({"cluster_system_identifier":"foreign","database_oid":99,"schema_oid":99,"schema_name":"foreign"}),"inventory_stamp"=>serde_json::json!({"device":11,"inode":99,"uid":1000,"gid":1000,"mode":33152,"links":1,"length":18,"mtime":10,"mtime_ns":11,"ctime":12,"ctime_ns":13}),_=>"foreign-or-PREPARED".into()};
   let typed:PgTargetFinalAck=serde_json::from_value(changed)?;let canonical=serde_json::to_vec(&typed)?;
   ensure!(serde_json::to_vec(&serde_json::from_slice::<PgTargetFinalAck>(&canonical)?)?==canonical,"Changed fixture not canonical");
   ensure!(pg_target_final_verify(&canonical,&e,&raw,&inventory,Some((11,12)),KEY).is_err(),"Changed final fact field accepted:{field}");
  }Ok(())
 }
 #[test]fn prepared_unsigned_foreign_key_identity_and_noncanonical_refuse()->anyhow::Result<()>{
  let(e,raw,inventory)=inputs();let ack=signed(&e,&raw,&inventory);
  ensure!(pg_target_final_verify(&inventory,&e,&raw,&inventory,None,KEY).is_err(),"Prepared inventory became final ACK");
  ensure!(pg_target_final_verify(&raw,&e,&raw,&inventory,None,KEY).is_err(),"Attestation became final ACK");
  ensure!(pg_target_final_verify(&ack,&e,&raw,&inventory,Some((11,99)),KEY).is_err(),"Foreign original inventory inode accepted");
  ensure!(pg_target_final_verify(&ack,&e,&raw,&inventory,None,b"rotated-original-application-key-32bytes").is_err(),"Rotated key accepted");
  ensure!(pg_target_final_verify(&ack,&e,&raw,&inventory,None,b"short").is_err(),"Short key accepted");
  let mut spaced=ack.clone();spaced.push(b' ');ensure!(pg_target_final_verify(&spaced,&e,&raw,&inventory,None,KEY).is_err(),"Noncanonical receipt accepted");
  let mut extended:serde_json::Value=serde_json::from_slice(&ack)?;extended["caller_completed"]=true.into();ensure!(pg_target_final_verify(&serde_json::to_vec(&extended)?,&e,&raw,&inventory,None,KEY).is_err(),"Unknown authority field accepted");
  ensure!(pg_target_final_verify(&vec![b' ';PG_TARGET_FINAL_LIMIT+1],&e,&raw,&inventory,None,KEY).is_err(),"Oversized receipt accepted");Ok(())
 }
 #[test]fn explicit_replay_v2_inventory_and_final_purpose_cannot_be_downgraded()->anyhow::Result<()>{
  let(e,raw,inventory)=inputs_for(2);let ack=signed(&e,&raw,&inventory);
  let parsed=pg_target_final_verify(&ack,&e,&raw,&inventory,Some((11,12)),KEY)?;
  ensure!(parsed.fact.version==2&&parsed.fact.purpose==PG_TARGET_NATIVE_PURPOSE_V2,"Actual v2 final purpose lost");
  let mut wrong:PgTargetNativeInventory=serde_json::from_slice(&inventory)?;wrong.version=1;wrong.purpose=PG_TARGET_NATIVE_PURPOSE.into();
  ensure!(wrong.validate().is_err(),"Replay record roles became Direct-only v1");
  let mut wrong:PgTargetNativeInventory=serde_json::from_slice(&inventory)?;wrong.purpose=PG_TARGET_NATIVE_PURPOSE.into();ensure!(wrong.validate().is_err(),"Cross-purpose v2 inventory accepted");
  let mut wrong_ack:PgTargetFinalAck=serde_json::from_slice(&ack)?;wrong_ack.fact.version=1;wrong_ack.fact.purpose=PG_TARGET_NATIVE_PURPOSE.into();wrong_ack.mac=pg_target_final_mac(&wrong_ack.fact,KEY)?;
  ensure!(pg_target_final_verify(&serde_json::to_vec(&wrong_ack)?,&e,&raw,&inventory,None,KEY).is_err(),"Even authenticated Direct-purpose final fact cannot bind v2 inventory");
  Ok(())
 }
 #[test]fn current_inventory_role_is_mandatory_and_receipt_cannot_hash_itself()->anyhow::Result<()>{
  let(mut e,raw,inventory)=inputs();ensure!(pg_target_final_required(&e),"Current inventory not recognized");
  e.retained_roles.0.insert(pg_target_final_path(&e.restore_id),crate::storage::backup_inventory::Bytes{size_bytes:1,sha256:"f".repeat(64)});ensure!(pg_target_final_fact(&e,&raw,&inventory,bound_stamp(&inventory)).is_err(),"Circular own receipt role accepted");
  e.retained_roles.0.clear();ensure!(!pg_target_final_required(&e),"Legacy noinventory forced new role");ensure!(pg_target_final_fact(&e,&raw,&inventory,bound_stamp(&inventory)).is_err(),"Legacy target upgraded by fact factory");Ok(())
 }
}

//! Original Direct native custody substrate. Deliberately no production constructor or
//! route: SQL/current-human/all-space reference authority must be composed separately.
//! Native journals are evidence of an authorized decision, never bearer cleanup grants.
use super::*;
use super::super::Receipt;

#[derive(Debug,Clone,Serialize,Deserialize,PartialEq,Eq)]
#[serde(deny_unknown_fields)]
struct Intent {
    version:u8,
    deletion_operation:String,
    deletion_plan_sha256:String,
    actor_id:String,
    receipt_json:String,
    receipt_sha256:String,
    owned:Disposal,
}
struct OwnedErasure { lease:WriterLease, intent:Intent, raw:Vec<u8> }
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
enum Step { Canonical, Payload, Complete }

// Private nonserializable absence observation. It grants neither SQL accounting
// subtraction nor unlink authority; a caller must bind current immutable SQL and
// references in its own held transaction before using the observation.
struct ExistingCompletedNative {owner:OwnedErasure,census:census::FamilyCensus,material:String}
impl ExistingCompletedNative {
    fn inspect(root:&Path,receipt:&Receipt,operation:&str,plan_sha:&str,actor:&str,paths:std::collections::BTreeMap<String,(String,i64)>,finalized:std::collections::BTreeSet<String>)->io::Result<Self>{
        let owner=OwnedErasure::open_existing(root,receipt,operation,plan_sha,actor)?;
        let census=census::FamilyCensus::capture_proved(&owner,paths,finalized)?;
        let material=owner.completed_path_material(&census)?;
        Ok(Self{owner,census,material})
    }
    fn fence(&self)->io::Result<()>{if self.owner.completed_path_material(&self.census)?!=self.material{return Err(fail("Held completed absence changed"));}Ok(())}
}

impl OwnedErasure {
    // Kept PRIVATE until a transaction-owned all-space/current-human coordinator
    // can mint a nonserializable per-step authorization. Tests prove custody only.
    fn open(root:&Path,receipt:&Receipt,operation:&str,plan_sha:&str,actor:&str)->io::Result<Self> {Self::open_inner(root,receipt,operation,plan_sha,actor,true)}
    fn open_existing(root:&Path,receipt:&Receipt,operation:&str,plan_sha:&str,actor:&str)->io::Result<Self> {Self::open_inner(root,receipt,operation,plan_sha,actor,false)}
    fn open_inner(root:&Path,receipt:&Receipt,operation:&str,plan_sha:&str,actor:&str,allow_create:bool)->io::Result<Self> {
        if !uuid(operation) || plan_sha.len()!=64 || !plan_sha.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))
            || actor.is_empty() {return Err(fail("Committed erasure decision malformed"));}
        let lease=WriterLease::open_idle(root,&receipt.intent.attempt_id)?;
        Self::open_with_lease(lease,receipt,operation,plan_sha,actor,allow_create)
    }
    fn open_with_lease(lease:WriterLease,receipt:&Receipt,operation:&str,plan_sha:&str,actor:&str,allow_create:bool)->io::Result<Self>{
        lease.verify()?;
        if !uuid(operation)||plan_sha.len()!=64||!plan_sha.bytes().all(|b|b.is_ascii_digit()||(b'a'..=b'f').contains(&b))||actor.is_empty()||lease.attempt_id()!=receipt.intent.attempt_id{return Err(fail("Original lease decision malformed"));}
        let root=&lease.0.root_path;
        if lease.producer()!= &(Producer::Direct {request:receipt.intent.request.clone()}) {
            return Err(fail("Committed erasure requires original Direct producer"));
        }
        let receipt_raw=json(receipt)?;
        let raw=if let Some(raw)=optional_bytes(&lease.0.directory,"committed-erasure-intent.json")? {raw} else {
            if !allow_create{return Err(fail("Existing completed native intent unavailable"));}
            // No missing-original adoption on first entry: full original proof and
            // installed two-link group must be intact before the durable decision.
            super::super::verify(root,receipt,false)?;
            let owned=lease.plan_direct()?;
            if owned.installed_path.as_deref()!=Some(receipt.intent.storage_path.as_str())
                || owned.payload_sha256.as_deref()!=Some(receipt.intent.binary_sha256.as_str())
                || owned.payload_length!=receipt.intent.byte_length as u64
                || owned.envelopes.get("intent.json").map(|s|digest(s.as_bytes())).as_deref()!=Some(receipt.intent_sha256.as_str())
                || owned.envelopes.get("install-proof.json").map(|s|digest(s.as_bytes())).as_deref()!=Some(receipt.install_proof_sha256.as_str()) {
                return Err(fail("Committed erasure original receipt differs"));
            }
            let intent=Intent {version:1,deletion_operation:operation.into(),deletion_plan_sha256:plan_sha.into(),actor_id:actor.into(),
                receipt_json:String::from_utf8(receipt_raw.clone()).map_err(io::Error::other)?,receipt_sha256:digest(&receipt_raw),owned};
            let raw=json(&intent)?;
            stage::write_envelope(&lease.0.directory,"committed-erasure-intent.json",&raw)?;
            disposal_checkpoint("committed-intent-durable",root);
            raw
        };
        let intent:Intent=decode(&raw)?;
        if intent.version!=1 || intent.deletion_operation!=operation || intent.deletion_plan_sha256!=plan_sha
            || intent.actor_id!=actor || intent.receipt_json.as_bytes()!=receipt_raw || intent.receipt_sha256!=digest(&receipt_raw)
            || json(&intent)?!=raw {return Err(fail("Committed erasure original decision changed"));}
        let held=Self {lease,intent,raw};held.verify()?;Ok(held)
    }
    fn verify(&self)->io::Result<()> {
        self.lease.verify()?;
        let p=&self.intent.owned;
        let receipt:Receipt=decode(self.intent.receipt_json.as_bytes())?;
        super::super::validate(&receipt.intent)?;
        if self.intent.receipt_sha256!=digest(self.intent.receipt_json.as_bytes())
            || p.installed_path.as_deref()!=Some(receipt.intent.storage_path.as_str())
            || p.attempt_id!=receipt.intent.attempt_id
            || p.payload_length!=receipt.intent.byte_length as u64
            || p.payload_sha256.as_deref()!=Some(receipt.intent.binary_sha256.as_str())
            || self.lease.producer()!= &(Producer::Direct {request:receipt.intent.request.clone()})
            || p.envelopes.get("intent.json").map(|s|digest(s.as_bytes())).as_deref()!=Some(receipt.intent_sha256.as_str())
            || p.envelopes.get("install-proof.json").map(|s|digest(s.as_bytes())).as_deref()!=Some(receipt.install_proof_sha256.as_str()) {
            return Err(fail("Erasure decision lost original receipt binding"));
        }
        if p.version!=1 || p.attempt_id!=self.lease.attempt_id() || p.claim_json.as_bytes()!=self.lease.original_claim()
            || p.request_sha256!=self.lease.producer_sha256() || p.admission_claim_sha256!=self.lease.claim_sha256()
            || p.maximum_bytes!=self.lease.maximum_bytes() as i64
            || self.lease.stage_origin()?.as_deref()!=p.stage_origin_json.as_ref().map(|s|s.as_bytes())
            || optional_bytes(&self.lease.0.directory,"committed-erasure-intent.json")?.as_deref()!=Some(self.raw.as_slice()) {
            return Err(fail("Committed erasure native intent changed"));
        }
        let (ns,dir)=self.lease.stage_directory()?.ok_or_else(||fail("Original erasure stage absent"))?;
        let origin=self.lease.verify_origin(&ns,&dir,p.stage_origin_json.as_ref().ok_or_else(||fail("Erasure origin absent"))?.as_bytes())?;
        if Some(origin.payload)!=p.payload {return Err(fail("Erasure original inode differs"));}
        for name in ["reservation.json","request.json","seal.json","intent.json","install-proof.json"] {
            if optional_bytes(&dir,name)?.as_deref()!=p.envelopes.get(name).map(|s|s.as_bytes()) {return Err(fail("Erasure immutable evidence changed"));}
        }
        if let Some(raw)=optional_bytes(&self.lease.0.directory,"committed-erasure-tombstone.json")? {
            if raw!=self.raw {return Err(fail("Erasure tombstone changed"));}
        }
        Ok(())
    }
    // Returns held handles for BOTH remaining names and verifies the complete link
    // group. Unknown extra links/copies cannot be assimilated into this original owner.
    fn paths(&self)->io::Result<(File,Option<File>,File,String,Option<File>)> {
        self.verify()?;let p=&self.intent.owned;
        let (_,dir)=self.lease.stage_directory()?.ok_or_else(||fail("Erasure stage absent"))?;
        let payload=optional_file(&dir,"payload.part")?;
        let (target_dir,name)=super::super::target(&self.lease.0.root,p.installed_path.as_deref().ok_or_else(||fail("Erasure canonical path absent"))?,false)?;
        let target=optional_file(&target_dir,&name)?;
        let count=u64::from(payload.is_some())+u64::from(target.is_some());
        for f in [payload.as_ref(),target.as_ref()].into_iter().flatten() {
            let m=f.metadata()?;
            if Some(identity(f)?)!=p.payload || !m.is_file() || m.mode()&0o077!=0 || m.nlink()!=count
                || m.len()!=p.payload_length || Some(super::super::hash(f,m.len())?)!=p.payload_sha256 {
                return Err(fail("Erasure original link group changed"));
            }
        }
        // A final tombstone never permits resurrection to become owned again.
        if optional_bytes(&self.lease.0.directory,"committed-erasure-tombstone.json")?.is_some() && count!=0 {
            return Err(fail("Erased native name resurrected"));
        }
        Ok((dir,payload,target_dir,name,target))
    }
    fn apply_one_censused(&self,expected:Step,census:&census::FamilyCensus,final_authority:impl FnOnce()->io::Result<()>)->io::Result<()> {
        if !stage::same(census.guard.root(),&self.lease.0.root)? {return Err(fail("Census belongs to another original root"));}
        census.fence()?;
        self.apply_one_checked(expected,||{census.fence()?;final_authority()})
    }
    fn completed_path_material(&self,census:&census::FamilyCensus)->io::Result<String>{
        self.verify()?;census.fence()?;if self.next()?!=Step::Complete||optional_bytes(&self.lease.0.directory,"committed-erasure-tombstone.json")?.as_deref()!=Some(self.raw.as_slice()){return Err(fail("Complete original owned path set is not proved"));}
        let p=&self.intent.owned;let identity=p.payload.as_ref().ok_or_else(||fail("Original inode missing"))?;
        let value=serde_json::json!({"version":1,"original_claim_sha256":self.lease.claim_sha256(),"original_receipt_sha256":self.intent.receipt_sha256,"native_intent_sha256":digest(&self.raw),"family_observation_sha256":census.observation_sha256()?,"original_stage_origin_sha256":digest(p.stage_origin_json.as_ref().ok_or_else(||fail("Original stage origin absent"))?.as_bytes()),"paths":[{"relative":p.installed_path.as_ref().ok_or_else(||fail("Original canonical path absent"))?,"device":identity.device,"inode":identity.inode,"byte_length":p.payload_length,"sha256":p.payload_sha256},{"relative":format!("{}/{}/payload.part",stage::NAMESPACE,self.lease.attempt_id()),"device":identity.device,"inode":identity.inode,"byte_length":p.payload_length,"sha256":p.payload_sha256}]});
        let raw=serde_json::to_string(&value).map_err(io::Error::other)?;if raw.len()>65536{return Err(fail("Owned path material exceeds bound"));}Ok(raw)
    }
    fn next(&self)->io::Result<Step> {
        let(_,payload,_,_,target)=self.paths()?;
        Ok(if target.is_some(){Step::Canonical}else if payload.is_some(){Step::Payload}else{Step::Complete})
    }
    // PRIVATE: a future async coordinator must freshly authorize exactly ONE step
    // before entering this synchronous no-await mutation; never authorize two unlinks.
    #[cfg(test)]
    fn apply_one(&self,expected:Step)->io::Result<()> {self.apply_one_checked(expected,||Ok(()))}
    fn apply_one_checked(&self,expected:Step,final_authority:impl FnOnce()->io::Result<()>)->io::Result<()> {
        // Hash/full native evidence first. A future transaction-held coordinator
        // rechecks the original credential expiry after this work and immediately
        // before mutation; the callback cannot await or create a durable grant.
        let(dir,payload,target_dir,name,target)=self.paths()?;
        let actual=if target.is_some(){Step::Canonical}else if payload.is_some(){Step::Payload}else{Step::Complete};
        if actual!=expected {return Err(fail("Erasure step changed; reauthorize"));}
        let stamp=|f:&File|->io::Result<_>{let m=f.metadata()?;Ok((m.dev(),m.ino(),m.len(),m.nlink(),m.mode(),m.mtime(),m.mtime_nsec(),m.ctime(),m.ctime_nsec()))};
        let original_stamps=[payload.as_ref(),target.as_ref()].into_iter().map(|f|f.map(&stamp).transpose()).collect::<io::Result<Vec<_>>>()?;
        final_authority()?;
        for (f,before) in [payload.as_ref(),target.as_ref()].into_iter().zip(original_stamps) {
            if f.map(&stamp).transpose()?!=before {return Err(fail("Erasure held inode changed during final authority proof"));}
        }
        match expected {
            Step::Canonical=>{unlink_owned(&target_dir,&name,target.as_ref().unwrap())?;disposal_checkpoint("committed-canonical-unlinked",&self.lease.0.root_path);},
            Step::Payload=>{unlink_owned(&dir,"payload.part",payload.as_ref().unwrap())?;disposal_checkpoint("committed-payload-unlinked",&self.lease.0.root_path);},
            Step::Complete=> {
                if optional_bytes(&self.lease.0.directory,"committed-erasure-tombstone.json")?.is_none() {
                    stage::write_envelope(&self.lease.0.directory,"committed-erasure-tombstone.json",&self.raw)?;
                    disposal_checkpoint("committed-tombstone-durable",&self.lease.0.root_path);
                }
            }
        }
        self.verify()?;Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::OpenOptionsExt;
    fn fixture()->(PathBuf,Receipt,String) {
        let root=std::env::temp_dir().join(format!("committed-erasure-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
        let req=UploadRequest {draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"direct".into(),actor_id:"original-human".into(),space_id:"space".into(),document_id:"page".into(),original_name:"owned.bin".into(),mime_type:"application/octet-stream".into()};
        let attempt=uuid::Uuid::new_v4().to_string();let lease=WriterLease::begin(&root,&attempt,Producer::Direct{request:req.clone()},64).unwrap();
        let mut ingest=super::super::super::Ingest::reserve_with_lease(&root,req,64,lease.clone()).unwrap();ingest.append(b"original committed bytes").unwrap();
        let receipt=ingest.finish().unwrap().prepare("space","bin").unwrap().install().unwrap();drop(lease);
        (root,receipt,uuid::Uuid::new_v4().to_string())
    }
    fn open(root:&Path,r:&Receipt,op:&str)->io::Result<OwnedErasure> {OwnedErasure::open(root,r,op,&"a".repeat(64),"original-human")}
    #[test]
    fn census_original_paths_require_fresh_complete_membership_each_step(){
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();
        loop {let step=owner.next().unwrap();let held=census::FamilyCensus::capture(&owner).unwrap();owner.apply_one_censused(step,&held,||Ok(())).unwrap();drop(held);if step==Step::Complete{break}}
        assert!(!root.join(&r.intent.storage_path).exists());assert!(!root.join(stage::NAMESPACE).join(&r.intent.attempt_id).join("payload.part").exists());drop(owner);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn census_refuses_selected_replay_claim_without_payload_and_foreign_copy(){
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();
        let id=uuid::Uuid::new_v4().to_string();let replay=WriterLease::begin(&root,&id,Producer::Replay{draft_scope:None,provider:"fixture".into(),incoming_id:"incoming".into(),actor_id:"original-human".into(),space_id:"space".into(),document_id:"page".into(),asset_id:r.intent.asset_id.clone(),payload_fingerprint:"a".repeat(64),binary_sha256:r.intent.binary_sha256.clone(),byte_length:r.intent.byte_length as u64},64).unwrap();drop(replay);
        let error=match census::FamilyCensus::capture(&owner){Ok(_)=>panic!("Selected empty Replay claim allowed"),Err(e)=>e};assert!(error.to_string().contains("without payload"));assert!(root.join(&r.intent.storage_path).exists());drop(owner);std::fs::remove_dir_all(root).unwrap();
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();let extra=root.join("spaces/space/attachments/page/foreign-copy.bin");std::fs::copy(root.join(&r.intent.storage_path),&extra).unwrap();assert!(census::FamilyCensus::capture(&owner).is_err());assert!(extra.exists());drop(owner);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore="only spawned by generated independent census controls"]
    fn required_native_census_child(){
        let root=PathBuf::from(std::env::var("STARLANE_ADMISSION_TEST_CHILD_ROOT").unwrap());
        match std::env::var("STARLANE_DISPOSAL_TEST_ADAPTER").unwrap().as_str(){
            "census-create"=>{let req=UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"direct".into(),actor_id:"original-human".into(),space_id:"space".into(),document_id:"page".into(),original_name:"new.bin".into(),mime_type:"application/octet-stream".into()};assert!(WriterLease::begin(&root,&uuid::Uuid::new_v4().to_string(),Producer::Direct{request:req},64).is_err());},
            "census-replace"=>{std::fs::rename(root.join(NS),root.join("foreign-replaced-admission")).unwrap();std::fs::create_dir(root.join(NS)).unwrap();},
            "census-hold"=>{let _held=crate::storage::native_family_fence::Creation::exclusive(&root).unwrap();std::fs::write(root.join("census-child-ready"),b"ready").unwrap();loop{std::thread::sleep(std::time::Duration::from_millis(100));}},
            _=>panic!("Unknown private census selector"),
        }
    }
    fn census_child(root:&Path,selector:&str)->std::process::Child{
        std::process::Command::new(std::env::current_exe().unwrap()).args(["--ignored","--exact","storage::attachment_upload::admission::committed_erasure::tests::required_native_census_child","--nocapture","--test-threads=1"]).env("STARLANE_ADMISSION_TEST_CHILD_ROOT",root).env("STARLANE_DISPOSAL_TEST_ADAPTER",selector).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap()
    }
    #[test]
    fn census_independent_process_creation_replacement_and_sigkill(){
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();let held=census::FamilyCensus::capture(&owner).unwrap();
        assert!(census_child(&root,"census-create").wait().unwrap().success());held.fence().unwrap();
        assert!(census_child(&root,"census-replace").wait().unwrap().success());assert!(held.fence().is_err());assert!(owner.apply_one_censused(Step::Canonical,&held,||Ok(())).is_err());assert!(root.join(&r.intent.storage_path).exists());drop(held);drop(owner);std::fs::remove_dir_all(root).unwrap();
        let(root,r,_op)=fixture();let mut child=census_child(&root,"census-hold");let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
        while !root.join("census-child-ready").exists(){assert!(child.try_wait().unwrap().is_none(),"Independent census child exited early");if std::time::Instant::now()>deadline{let _=child.kill();panic!("Independent census checkpoint timed out");}std::thread::sleep(std::time::Duration::from_millis(10));}
        let req=UploadRequest{draft_scope:None,operation_id:uuid::Uuid::new_v4().to_string(),origin:"direct".into(),actor_id:"original-human".into(),space_id:"space".into(),document_id:"page".into(),original_name:"new.bin".into(),mime_type:"application/octet-stream".into()};
        let id=uuid::Uuid::new_v4().to_string();assert!(WriterLease::begin(&root,&id,Producer::Direct{request:req.clone()},64).is_err());child.kill().unwrap();assert!(!child.wait().unwrap().success());
        let lease=WriterLease::begin(&root,&id,Producer::Direct{request:req},64).unwrap();assert!(root.join(&r.intent.storage_path).exists());drop(lease);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn existing_completed_observation_never_creates_intent_and_holds_exclusion(){
        let(root,r,op)=fixture();let paths=std::collections::BTreeMap::from([(r.intent.storage_path.clone(),(r.intent.asset_id.clone(),r.intent.byte_length))]);
        let observe=||ExistingCompletedNative::inspect(&root,&r,&op,&"a".repeat(64),"original-human",paths.clone(),std::collections::BTreeSet::new());
        assert!(observe().is_err());assert!(!root.join(NS).join(&r.intent.attempt_id).join("committed-erasure-intent.json").exists(),"Observation created an intent");
        let owner=open(&root,&r,&op).unwrap();owner.apply_one(Step::Canonical).unwrap();owner.apply_one(Step::Payload).unwrap();drop(owner);
        assert!(observe().is_err(),"Missing completion tombstone accepted");
        let owner=open(&root,&r,&op).unwrap();owner.apply_one(Step::Complete).unwrap();drop(owner);
        let held=observe().unwrap();held.fence().unwrap();
        let fresh=uuid::Uuid::new_v4().to_string();assert!(WriterLease::begin(&root,&fresh,Producer::Direct{request:r.intent.request.clone()},64).is_err(),"Creation crossed completed observation fence");
        assert!(OwnedErasure::open_existing(&root,&r,&uuid::Uuid::new_v4().to_string(),&"a".repeat(64),"original-human").is_err());
        std::fs::write(root.join(&r.intent.storage_path),b"foreign resurrection").unwrap();assert!(held.fence().is_err());assert!(root.join(&r.intent.storage_path).exists(),"Observation removed foreign bytes");
        drop(held);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn census_attribution_refuses_unknown_different_bytes_and_selected_alias(){
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();let paths=std::collections::BTreeMap::from([(r.intent.storage_path.clone(),(r.intent.asset_id.clone(),r.intent.byte_length))]);
        let extra=root.join("spaces/space/attachments/page/unknown.bin");std::fs::write(&extra,b"different foreign bytes").unwrap();assert!(census::FamilyCensus::capture_attributed(&owner,paths.clone()).is_err());assert!(extra.exists());std::fs::remove_file(&extra).unwrap();
        let extra_path="spaces/space/attachments/page/selected-alias.bin";std::fs::write(root.join(extra_path),vec![b'X';r.intent.byte_length as usize]).unwrap();let mut paths=paths;paths.insert(extra_path.into(),(r.intent.asset_id.clone(),r.intent.byte_length));assert!(census::FamilyCensus::capture_attributed(&owner,paths).is_err());assert!(root.join(extra_path).exists());drop(owner);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn erasure_review_prevents_original_receipt_reinstall_and_serving(){
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();assert!(crate::storage::attachment_upload::verify(&root,&r,false).is_err());
        owner.apply_one(Step::Canonical).unwrap();assert!(crate::storage::attachment_upload::install(&root,&r).is_err());assert!(!root.join(&r.intent.storage_path).exists());assert!(root.join(stage::NAMESPACE).join(&r.intent.attempt_id).join("payload.part").exists());
        owner.apply_one(Step::Payload).unwrap();owner.apply_one(Step::Complete).unwrap();assert!(crate::storage::attachment_upload::verify(&root,&r,true).is_err());drop(owner);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn census_refuses_encoded_pending_source_and_metadata(){
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();let journal=root.join(".storage-journal").join(uuid::Uuid::new_v4().to_string());std::fs::create_dir_all(&journal).unwrap();std::fs::write(journal.join("pending.md"),format!("[old](starlane-attachment%3A%2F%2F{})",r.intent.asset_id)).unwrap();assert!(census::FamilyCensus::capture(&owner).is_err());assert!(root.join(&r.intent.storage_path).exists());drop(owner);std::fs::remove_dir_all(root).unwrap();
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();let journal=root.join(".publication-journal").join(uuid::Uuid::new_v4().to_string());std::fs::create_dir_all(&journal).unwrap();let encoded=r.intent.asset_id.replace('-',"\\u002d");std::fs::write(journal.join("manifest.json"),format!("{{\"asset_id\":\"{encoded}\"}}")).unwrap();assert!(census::FamilyCensus::capture(&owner).is_err());assert!(root.join(&r.intent.storage_path).exists());drop(owner);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn census_creation_excludes_new_claim_and_replaced_namespace(){
        let(root,r,op)=fixture();let owner=open(&root,&r,&op).unwrap();let held=census::FamilyCensus::capture(&owner).unwrap();let id=uuid::Uuid::new_v4().to_string();assert!(WriterLease::begin(&root,&id,owner.lease.producer().clone(),64).is_err());assert!(!root.join(NS).join(&id).exists());
        std::fs::rename(root.join(NS),root.join("foreign-replaced-admission")).unwrap();std::fs::create_dir(root.join(NS)).unwrap();assert!(held.fence().is_err());assert!(owner.apply_one_censused(Step::Canonical,&held,||Ok(())).is_err());assert!(root.join(&r.intent.storage_path).exists());drop(held);drop(owner);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn original_direct_steps_restart_and_preserve_raw_evidence() {
        let(root,r,op)=fixture();let stage_dir=root.join(stage::NAMESPACE).join(&r.intent.attempt_id);
        let originals:Vec<_>=["reservation.json","request.json","seal.json","intent.json","install-proof.json"].into_iter().map(|n|(n,std::fs::read(stage_dir.join(n)).unwrap())).collect();
        let e=open(&root,&r,&op).unwrap();assert_eq!(e.next().unwrap(),Step::Canonical);e.apply_one(Step::Canonical).unwrap();drop(e);
        assert!(!root.join(&r.intent.storage_path).exists());assert!(stage_dir.join("payload.part").exists());
        let e=open(&root,&r,&op).unwrap();assert_eq!(e.next().unwrap(),Step::Payload);e.apply_one(Step::Payload).unwrap();drop(e);
        let e=open(&root,&r,&op).unwrap();assert_eq!(e.next().unwrap(),Step::Complete);e.apply_one(Step::Complete).unwrap();drop(e);
        let e=open(&root,&r,&op).unwrap();e.apply_one(Step::Complete).unwrap();
        for(n,raw)in originals{assert_eq!(std::fs::read(stage_dir.join(n)).unwrap(),raw)}
        assert!(!stage_dir.join("payload.part").exists());drop(e);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn foreign_inode_extra_link_wrong_decision_and_restored_root_refuse() {
        let(root,r,op)=fixture();let canonical=root.join(&r.intent.storage_path);let alias=root.join("unknown-alias");
        std::fs::hard_link(&canonical,&alias).unwrap();assert!(open(&root,&r,&op).is_err());assert!(canonical.exists());std::fs::remove_file(&alias).unwrap();
        let e=open(&root,&r,&op).unwrap();drop(e);assert!(OwnedErasure::open(&root,&r,&op,&"b".repeat(64),"original-human").is_err());
        assert!(OwnedErasure::open(&root,&r,&op,&"a".repeat(64),"another-human").is_err());
        let retained=canonical.with_extension("retained");std::fs::rename(&canonical,&retained).unwrap();std::fs::copy(&retained,&canonical).unwrap();
        let e=open(&root,&r,&op).unwrap();assert!(e.next().is_err());assert!(e.apply_one(Step::Canonical).is_err());assert!(canonical.exists()&&retained.exists());drop(e);
        // Original root inode itself is part of custody; rename the original and
        // substitute a new directory rather than treating equal relative names as proof.
        let old=root.with_extension("original");std::fs::rename(&root,&old).unwrap();std::fs::create_dir(&root).unwrap();assert!(open(&root,&r,&op).is_err());
        std::fs::remove_dir_all(root).unwrap();std::fs::remove_dir_all(old).unwrap();
    }

    #[test]
    fn last_moment_extra_alias_refuses_before_owned_unlink() {
        let(root,r,op)=fixture();let e=open(&root,&r,&op).unwrap();let canonical=root.join(&r.intent.storage_path);let alias=root.join("late-foreign-alias");
        assert!(e.apply_one_checked(Step::Canonical,||std::fs::hard_link(&canonical,&alias)).is_err());
        assert!(canonical.exists()&&alias.exists());assert!(root.join(stage::NAMESPACE).join(&r.intent.attempt_id).join("payload.part").exists());
        drop(e);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fresh_final_step_refusal_preserves_remaining_original_bytes() {
        let(root,r,op)=fixture();let e=open(&root,&r,&op).unwrap();
        assert!(e.apply_one_checked(Step::Canonical,||Err(fail("Current authority refused"))).is_err());
        assert!(root.join(&r.intent.storage_path).exists());
        e.apply_one_checked(Step::Canonical,||Ok(())).unwrap();
        assert!(e.apply_one_checked(Step::Payload,||Err(fail("New reference/current authority refused"))).is_err());
        assert!(root.join(stage::NAMESPACE).join(&r.intent.attempt_id).join("payload.part").exists());
        assert!(!root.join(NS).join(&r.intent.attempt_id).join("committed-erasure-tombstone.json").exists());
        drop(e);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn journal_cannot_rebind_original_inode_to_an_unreviewed_path() {
        let(root,r,op)=fixture();let e=open(&root,&r,&op).unwrap();let mut changed=e.intent.clone();drop(e);
        let old=root.join(&r.intent.storage_path);let borrowed=old.with_file_name("unreviewed.bin");std::fs::rename(&old,&borrowed).unwrap();
        changed.owned.installed_path=Some(borrowed.strip_prefix(&root).unwrap().to_str().unwrap().into());
        let journal=root.join(NS).join(&r.intent.attempt_id).join("committed-erasure-intent.json");
        std::fs::write(journal,json(&changed).unwrap()).unwrap();
        assert!(open(&root,&r,&op).is_err());assert!(borrowed.exists());
        assert!(root.join(stage::NAMESPACE).join(&r.intent.attempt_id).join("payload.part").exists());std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    #[ignore="only spawned by generated owned native crash controls"]
    fn required_native_erasure_crash_child() {
        let root=PathBuf::from(std::env::var("STARLANE_DISPOSAL_TEST_ROOT").unwrap());
        let raw=std::fs::read(root.join("native-test-decision.json")).unwrap();
        let(r,op):(Receipt,String)=serde_json::from_slice(&raw).unwrap();
        let e=open(&root,&r,&op).unwrap();
        for step in [Step::Canonical,Step::Payload,Step::Complete] {e.apply_one(step).unwrap();}
        panic!("Expected owned SIGKILL checkpoint");
    }
    #[test]
    fn original_native_sigkill_windows_keep_original_retry() {
        for point in ["committed-intent-durable","committed-canonical-unlinked","committed-payload-unlinked","committed-tombstone-durable"] {
            let(root,r,op)=fixture();
            let file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(root.join("native-test-decision.json")).unwrap();
            serde_json::to_writer(file, &(r.clone(),op.clone())).unwrap();
            let mut child=std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--ignored","--exact","storage::attachment_upload::admission::committed_erasure::tests::required_native_erasure_crash_child","--nocapture","--test-threads=1"])
                .env("STARLANE_DISPOSAL_TEST_ROOT",&root).env("STARLANE_DISPOSAL_TEST_CHECKPOINT",point)
                .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().unwrap();
            let deadline=std::time::Instant::now()+std::time::Duration::from_secs(15);
            while !root.join("checkpoint-ready").exists() {
                if let Some(status)=child.try_wait().unwrap(){panic!("Owned crash child exited early at {point}: {status}");}
                if std::time::Instant::now()>deadline{let _=child.kill();panic!("Owned crash checkpoint timeout at {point}");}
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert!(WriterLease::open_idle(&root,&r.intent.attempt_id).is_err());
            child.kill().unwrap();child.wait().unwrap();
            let e=open(&root,&r,&op).unwrap();
            loop {let step=e.next().unwrap();e.apply_one(step).unwrap();if step==Step::Complete{break;}}
            assert!(!root.join(&r.intent.storage_path).exists());
            assert!(!root.join(stage::NAMESPACE).join(&r.intent.attempt_id).join("payload.part").exists());
            assert!(root.join(stage::NAMESPACE).join(&r.intent.attempt_id).join("install-proof.json").exists());
            drop(e);std::fs::remove_dir_all(root).unwrap();
            println!("ORIGINAL_NATIVE_COMMITTED_ERASURE_SIGKILL_CUSTODY_ONLY_PASS {point}");
        }
    }
}

#[path="attachment_erasure_coordinator.rs"]
mod coordinator;
pub(crate) use coordinator::{cleanup, CleanupOutcome,cleanup_pending,PendingRequest,PendingOutcome,discover_pending,PendingDiscovery};

#[path="attachment_erasure_census.rs"]
mod census;

#[path="attachment_completed_accounting.rs"]
pub(crate) mod accounting;

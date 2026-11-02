// Candidate-only exact engine retirement and held read-only admission.
pub(super) fn engine_root_fence(root:&Root)->io::Result<()> {
 let now=Root::open(&root.absolute)?;
 if now.chain.len()!=root.chain.len(){return Err(fail("Engine root ancestry changed"));}
 for(a,b)in now.chain.iter().zip(&root.chain){if identity(a)?!=identity(b)?{return Err(fail("Engine root ancestry changed"));}}
 for marker in [".starlane-restore-incomplete",".starlane-backup-incomplete"]{if exists(root.fd(),marker)?{return Err(fail("Engine restoration incomplete"));}}
 Ok(())
}
fn engine_retired_fence(d:&PlannedDestination)->io::Result<()> {
 engine_root_fence(&d.copied.root)?;d.copied.parent.check()?;
 if physical_stamp(d.copied.root.fd())?!=d.root_stamp{return Err(fail("Engine retired root changed"));}
 for(path,(held,stamp))in &d.directories{if physical_stamp(held)?!=*stamp||physical_stamp(&destination_directory(d.copied.root.fd(),path)?)?!=*stamp{return Err(fail("Engine directory changed"));}}
 for(path,leaf)in &d.leaves{let now=d.copied.root.leaf(path)?;if physical_stamp(&leaf.held)?!=leaf.stamp||physical_stamp(&now)?!=leaf.stamp||planned_hash(&now)?!=leaf.bytes{return Err(fail("Engine retired leaf changed"));}}
 let (files,dirs)=engine_walk(d.copied.root.fd(),"")?;
 if files!=d.leaves.keys().cloned().collect()||dirs.keys().cloned().collect::<std::collections::BTreeSet<_>>()!=d.directories.keys().cloned().collect(){return Err(fail("Engine retired topology changed"));}
 Ok(())
}
fn engine_walk(fd:&File,prefix:&str)->io::Result<(std::collections::BTreeSet<String>,BTreeMap<String,(File,PhysicalStamp)>)> {
 fn recurse(fd:&File,prefix:&str,files:&mut std::collections::BTreeSet<String>,dirs:&mut BTreeMap<String,(File,PhysicalStamp)>)->io::Result<()> {
  for e in std::fs::read_dir(format!("/proc/self/fd/{}",fd.as_raw_fd()))? {
   let name=e?.file_name().into_string().map_err(|_|fail("NonUTF8 engine role"))?;
   if !valid_relative(&name)||name.contains(['/','\\']){return Err(fail("Unsafe engine role"));}
   let p=if prefix.is_empty(){name.clone()}else{format!("{prefix}/{name}")};
   let f=open_at(fd,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;
   if f.metadata()?.is_dir(){let stamp=physical_stamp(&f)?;recurse(&f,&p,files,dirs)?;dirs.insert(p,(f,stamp));}
   else if f.metadata()?.is_file(){files.insert(p);}else{return Err(fail("Nonregular engine role"));}
  }Ok(())
 }
 let mut files=Default::default();let mut dirs=Default::default();recurse(fd,prefix,&mut files,&mut dirs)?;Ok((files,dirs))
}
fn engine_retired_file(d:&mut PlannedDestination,path:&str,raw:&[u8])->io::Result<()> {
 engine_retired_fence(d)?;
 let(parent,name)=planned_parent(d,path)?;
 let mut f=open_at(&parent,&name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;
 {use std::io::Write;f.write_all(raw)?;}f.sync_all()?;parent.sync_all()?;
 d.leaves.insert(path.into(),PlannedLeaf{stamp:physical_stamp(&f)?,bytes:crate::storage::backup_inventory::Bytes{size_bytes:raw.len()as u64,sha256:digest(raw)},held:f});
 pending_directory_delta(d,path.rsplit_once('/').ok_or_else(||fail("Unsafe engine finalization path"))?.0)?;
 engine_retired_fence(d)
}
pub(crate) struct FinalizedEngineTransfer<'a>{committed:CommittedEngineImport<'a>}
impl FinalizedEngineTransfer<'_>{pub(crate) fn path(&self)->&Path{self.committed.path()} pub(crate) fn fence(&self)->io::Result<()>{self.committed.sealed.imported.prepared.original.bundle.fence()?;engine_retired_fence(&self.committed.sealed.imported.prepared.destination)}}
pub(crate) fn retire_engine_commit<'a>(mut committed:CommittedEngineImport<'a>)->io::Result<FinalizedEngineTransfer<'a>> {
 committed.fence()?;
 let dir=committed.sealed.imported.prepared.engine_directory.clone();
 let d=&mut committed.sealed.imported.prepared.destination;
 let target=format!("{dir}/retired-marker");let(parent,name)=planned_parent(d,&target)?;
 if exists(&parent,&name)?{return Err(fail("Engine retired role occupied"));}
 let before=physical_stamp(&d.copied.marker)?;
 let name=CString::new(name).map_err(|_|fail("Invalid engine retirement role"))?;let old=CString::new(DESTINATION_MARKER).unwrap();
 engine_checkpoint("engine-before-retirement")?;
 if unsafe{libc::syscall(libc::SYS_renameat2,d.copied.root.fd().as_raw_fd(),old.as_ptr(),parent.as_raw_fd(),name.as_ptr(),libc::RENAME_NOREPLACE)}!=0{return Err(io::Error::last_os_error());}
 parent.sync_all()?;d.copied.root.fd().sync_all()?;engine_checkpoint("engine-marker-retired")?;
 let marker=open_at(&parent,name.to_str().map_err(|_|fail("Invalid retired role"))?,libc::O_RDONLY)?;
 let after=physical_stamp(&marker)?;
 if (before.device,before.inode,before.length,before.links,before.mode,before.mtime,before.mtime_ns)!=(after.device,after.inode,after.length,after.links,after.mode,after.mtime,after.mtime_ns)||read_ledger(&marker)?!=d.copied.marker_bytes{return Err(fail("Original engine marker not retired"));}
 d.leaves.insert(target.clone(),PlannedLeaf{held:marker,stamp:after,bytes:crate::storage::backup_inventory::Bytes{size_bytes:d.copied.marker_bytes.len()as u64,sha256:digest(&d.copied.marker_bytes)}});
 pending_directory_delta(d,"")?;pending_directory_delta(d,&dir)?;
 engine_retired_fence(d)?;
 engine_retired_file(d,&format!("{dir}/finalization.json"),&committed.raw_finalization)?;
 engine_checkpoint("engine-finalization")?;
 let proof=FinalizedEngineTransfer{committed};proof.fence()?;Ok(proof)
}
struct EngineHeldRecord{held:File,stamp:PhysicalStamp,bytes:crate::storage::backup_inventory::Bytes}
pub(crate) struct EngineAdmission {
 root:Root,target:EngineTargetIdentity,raw_envelope:Vec<u8>,envelope:EngineCommitEnvelope,
 records:BTreeMap<String,EngineHeldRecord>,engine_files:std::collections::BTreeSet<String>,
 engine_directories:BTreeMap<String,(File,PhysicalStamp)>,
 history_files:std::collections::BTreeSet<String>,history_directories:BTreeMap<String,(File,PhysicalStamp)>,
}
impl EngineAdmission {
 pub(super) fn path(&self)->&Path{&self.root.absolute}
 pub(crate) fn physical_fence(&self)->io::Result<()> {
  engine_root_fence(&self.root)?;
  let engine=destination_directory(self.root.fd(),".engine-transfer-custody")?;
  let(files,dirs)=engine_walk(&engine,".engine-transfer-custody")?;
  if files!=self.engine_files||dirs.keys().collect::<Vec<_>>()!=self.engine_directories.keys().filter(|p|p.as_str()!=".engine-transfer-custody").collect::<Vec<_>>(){return Err(fail("Held engine topology changed"));}
  let(history_files,history_dirs)=history_topology(&self.root)?;
  if history_files!=self.history_files||history_dirs.keys().collect::<Vec<_>>()!=self.history_directories.keys().collect::<Vec<_>>(){return Err(fail("Held historical topology changed"));}
  for(path,(held,stamp))in self.engine_directories.iter().chain(&self.history_directories){if physical_stamp(held)?!=*stamp||physical_stamp(&destination_directory(self.root.fd(),path)?)?!=*stamp{return Err(fail("Held proof directory changed"));}}
  for(path,r)in &self.records{let now=self.root.leaf(path)?;if physical_stamp(&r.held)?!=r.stamp||physical_stamp(&now)?!=r.stamp||planned_hash(&now)?!=r.bytes||physical_stamp(&now)?!=r.stamp{return Err(fail("Held engine/history evidence changed"));}}
  engine_root_fence(&self.root)
 }
 pub(crate) async fn verify_connection(&self,conn:&mut sqlx::PgConnection)->anyhow::Result<()> {
  self.physical_fence()?;
  engine_reader_sql(conn,&self.envelope,&self.raw_envelope,&self.records).await?;
  self.physical_fence()?;Ok(())
 }
}
pub(crate) async fn admit_engine(root_path:&Path,conn:&mut sqlx::PgConnection)->anyhow::Result<EngineAdmission> {
 let root=Root::open(root_path)?;engine_root_fence(&root)?;
 let raw_envelope=read_ledger(&root.leaf(".engine-transfer-custody/current.json")?)?;
 let envelope:EngineCommitEnvelope=serde_json::from_slice(&raw_envelope)?;
 ensure!(envelope.version==1&&uuid::Uuid::parse_str(&envelope.transfer_id)?.to_string()==envelope.transfer_id,"Invalid engine envelope version/UUID");
 ensure!(envelope.target.schema_name==format!("transfer_{}",uuid::Uuid::parse_str(&envelope.transfer_id)?.simple()),"Engine schema reservation differs");
 ensure!(identity(root.fd())?==(envelope.root_device,envelope.root_inode),"Engine target root replaced");
 ensure!(engine_target_identity(conn,&envelope.target.schema_name).await?==envelope.target,"Wrong native PG target");
 let dir=format!(".engine-transfer-custody/{}",envelope.transfer_id);
 let mut records=BTreeMap::new();
 fn retain(root:&Root,records:&mut BTreeMap<String,EngineHeldRecord>,path:&str,expected_sha:Option<&str>)->anyhow::Result<Vec<u8>> {
  let held=root.leaf(path)?;let stamp=physical_stamp(&held)?;
  ensure!(stamp.mode&0o777==0o600&&stamp.links==1,"Engine record not private exclusive data");
  let raw=read_ledger(&held)?;
  ensure!(physical_stamp(&held)?==stamp,"Engine record changed during read");
  if let Some(expected)=expected_sha{ensure!(digest(&raw)==expected,"Engine record digest differs: {path}");}
  records.insert(path.into(),EngineHeldRecord{held,stamp,bytes:crate::storage::backup_inventory::Bytes{size_bytes:raw.len()as u64,sha256:digest(&raw)}});
  Ok(raw)
 }
 retain(&root,&mut records,".engine-transfer-custody/current.json",Some(&digest(&raw_envelope)))?;
 let raw_intent=retain(&root,&mut records,&format!("{dir}/intent.json"),Some(&envelope.seal_intent_sha256))?;
 let intent:EngineSealIntent=serde_json::from_slice(&raw_intent)?;
 ensure!(intent.transfer_id==envelope.transfer_id&&intent.target==envelope.target&&intent.root_device==envelope.root_device&&intent.root_inode==envelope.root_inode&&!intent.serving_allowed,"Engine seal binding differs");
 ensure!(intent.custody_generation_id==envelope.custody_generation_id&&intent.custody_ledger_sha256==envelope.custody_ledger_sha256&&intent.import_commitment_sha256==envelope.import_commitment_sha256&&intent.marker_device==envelope.marker_device&&intent.marker_inode==envelope.marker_inode&&intent.marker_sha256==envelope.retired_marker_sha256,"Engine sealed authority association differs");
 ensure!(intent.roles.keys().map(String::as_str).collect::<std::collections::BTreeSet<_>>()==["source-intent.json","original.sqlite","original-manifest.json","import-commitment.json"].into_iter().collect(),"Engine evidence role set differs");
 for(name,role)in &intent.roles {
  ensure!(valid_hash(&role.sha256)&&role.length<=i64::MAX as u64,"Unbounded engine role");
  let path=format!("{dir}/{name}");
  if name=="original.sqlite" {
   let held=root.leaf(&path)?;let stamp=physical_stamp(&held)?;let bytes=planned_hash(&held)?;
   ensure!(stamp.mode&0o777==0o600&&stamp.links==1&&physical_stamp(&held)?==stamp&&bytes.size_bytes==role.length&&bytes.sha256==role.sha256,"Original SQLite evidence changed");
   records.insert(path,EngineHeldRecord{held,stamp,bytes});
  } else {let raw=retain(&root,&mut records,&path,Some(&role.sha256))?;ensure!(raw.len()as u64==role.length,"Engine role length differs");}
 }
 let completion=retain(&root,&mut records,&format!("{dir}/completion.json"),Some(&envelope.completion_sha256))?;
 let completion:serde_json::Value=serde_json::from_slice(&completion)?;
 ensure!(completion==serde_json::json!({"version":1,"transfer_id":envelope.transfer_id,"seal_intent_sha256":envelope.seal_intent_sha256,"import_commitment_sha256":envelope.import_commitment_sha256,"target":envelope.target,"root_device":envelope.root_device,"root_inode":envelope.root_inode,"phase":"actual-engine-sql-committed-closed-readback"}),"Completion protocol differs");
 let final_raw=retain(&root,&mut records,&format!("{dir}/finalization.json"),Some(&envelope.finalization_sha256))?;
 let final_record:serde_json::Value=serde_json::from_slice(&final_raw)?;
 ensure!(final_record==serde_json::json!({"version":1,"transfer_id":envelope.transfer_id,"completion_sha256":envelope.completion_sha256,"retired_marker":format!("{dir}/retired-marker"),"retired_marker_sha256":envelope.retired_marker_sha256,"marker_device":envelope.marker_device,"marker_inode":envelope.marker_inode,"root_device":envelope.root_device,"root_inode":envelope.root_inode,"phase":"exact-marker-retired-and-all-postchecks"}),"Engine finalization differs");
 let retired=format!("{dir}/retired-marker");
 retain(&root,&mut records,&retired,Some(&envelope.retired_marker_sha256))?;
 ensure!(identity(&records[&retired].held)?==(envelope.marker_device,envelope.marker_inode),"Original physical engine marker absent");
 let engine_fd=destination_directory(root.fd(),".engine-transfer-custody")?;
 let(engine_files,mut engine_directories)=engine_walk(&engine_fd,".engine-transfer-custody")?;
 let expected=["source-intent.json","original.sqlite","original-manifest.json","import-commitment.json","intent.json","completion.json","finalization.json","retired-marker"].into_iter().map(|n|format!("{dir}/{n}")).chain([".engine-transfer-custody/current.json".into()]).collect();
 ensure!(engine_files==expected&&engine_directories.keys().cloned().collect::<Vec<_>>()==vec![dir.clone()],"Unknown/missing engine closure role");
 engine_directories.insert(".engine-transfer-custody".into(),(engine_fd.try_clone()?,physical_stamp(&engine_fd)?));
 for(held,stamp)in engine_directories.values(){ensure!(held.metadata()?.is_dir()&&stamp.mode&0o777==0o700,"Engine directory not private");}
 engine_reader_sql(conn,&envelope,&raw_envelope,&records).await?;
 // Exact finalization/current SQL/native target established before explicit historical policy.
 let raw=sqlx::query("SELECT generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at FROM storage_restore_generations ORDER BY generation_id").fetch_all(&mut *conn).await?;
 let mut rows=vec![];for r in raw{let get=|n|r.try_get::<String,_>(n);rows.push(HistorySqlRow{generation_id:get("generation_id")?,backup_id:get("backup_id")?,manifest_sha256:get("manifest_sha256")?,database_sha256:get("database_sha256")?,ledger_sha256:get("ledger_sha256")?,root_device:get("root_device")?,root_inode:get("root_inode")?,state:get("state")?,created_at:get("created_at")?});}
 let pointers:Vec<(i64,String)>=sqlx::query_as("SELECT singleton,generation_id FROM storage_restore_current ORDER BY singleton").fetch_all(&mut *conn).await?;
 let source=HistorySource::new_with_check(&root,None,engine_root_fence)?;
 let closed=history_closure(source,(envelope.root_device,envelope.root_inode),Some((&rows,&pointers)))?;
 ensure!(closed.current_id==envelope.custody_generation_id&&digest(&closed.generations[&closed.current_id].raw)==envelope.custody_ledger_sha256,"Engine historical association differs");
 for(path,r)in &closed.source.records{records.insert(path.clone(),EngineHeldRecord{held:r.held.try_clone()?,stamp:r.stamp.clone(),bytes:crate::storage::backup_inventory::Bytes{size_bytes:r.raw.len()as u64,sha256:digest(&r.raw)}});}
 let history_files=closed.source.files.clone();
 let history_directories=closed.source.directories.iter().map(|(p,(f,s))|Ok((p.clone(),(f.try_clone()?,s.clone())))).collect::<io::Result<_>>()?;
 closed.source.fence()?;drop(closed);
 let proof=EngineAdmission{root,target:envelope.target.clone(),raw_envelope,envelope,records,engine_files,engine_directories,history_files,history_directories};
 proof.verify_connection(conn).await?;Ok(proof)
}

async fn engine_reader_sql(conn:&mut sqlx::PgConnection,e:&EngineCommitEnvelope,raw:&[u8],records:&BTreeMap<String,EngineHeldRecord>)->anyhow::Result<()> {
 ensure!(engine_target_identity(conn,&e.target.schema_name).await?==e.target,"Engine native connection association differs");
 engine_control_schema(conn,&e.target.schema_name).await?;
 ensure!(engine_control_identity(conn,&e.target.schema_name).await?==(e.control_relation_ids.clone(),e.control_catalog_sha256.clone()),"Engine committed control identity/catalog replaced");
 ensure!(e.custody_shapes.keys().map(String::as_str).collect::<std::collections::BTreeSet<_>>()==["storage_restore_generations","storage_restore_current"].into_iter().collect()&&e.custody_shapes.values().all(|t|t.rows.is_empty()),"Invalid immutable custody SQL shape");
 let custody=pg_tables(conn,&e.custody_shapes).await?;
 ensure!(engine_typed_hash(&custody)?==e.custody_sql_sha256,"Immutable custody SQL rows/current pointer changed");
 let rows:Vec<(String,String,String,String)>=sqlx::query_as("SELECT transfer_id,envelope_json,envelope_sha256,state FROM engine_transfer_generations ORDER BY transfer_id").fetch_all(&mut *conn).await?;
 ensure!(rows==vec![(e.transfer_id.clone(),std::str::from_utf8(raw)?.into(),digest(raw),"committed".into())],"Engine SQL generation changed");
 let pointer:Vec<(i64,String)>=sqlx::query_as("SELECT singleton,transfer_id FROM engine_transfer_current ORDER BY singleton").fetch_all(&mut *conn).await?;
 ensure!(pointer==vec![(1,e.transfer_id.clone())],"Engine SQL current changed");
 let imported:Vec<(String,String,String)>=sqlx::query_as("SELECT transfer_id,commitment_json,commitment_sha256 FROM engine_transfer_import_commitments ORDER BY transfer_id").fetch_all(&mut *conn).await?;
 ensure!(imported.len()==1&&imported[0].0==e.transfer_id&&digest(imported[0].1.as_bytes())==e.import_commitment_sha256&&imported[0].2==e.import_commitment_sha256,"Immutable engine import changed");
 let dir=format!(".engine-transfer-custody/{}",e.transfer_id);
 let raw_import=read_ledger(&records[&format!("{dir}/import-commitment.json")].held)?;
 ensure!(raw_import==imported[0].1.as_bytes(),"Retained actual import SQL bytes differ");
 let v:serde_json::Value=serde_json::from_slice(&raw_import)?;
 ensure!(v["version"]==1&&v["phase"]=="post-custody-import-committed-only"&&v["serving_allowed"]==false&&v["target"]==serde_json::to_value(&e.target)?,"Actual import protocol differs");
 ensure!(v["pre_import_intent_sha256"].as_str()==Some(records[&format!("{dir}/source-intent.json")].bytes.sha256.as_str())&&v["source_manifest_sha256"].as_str()==Some(records[&format!("{dir}/original-manifest.json")].bytes.sha256.as_str())&&v["source_sqlite_database_sha256"].as_str()==Some(records[&format!("{dir}/original.sqlite")].bytes.sha256.as_str()),"Immutable original evidence detached from SQL import");
 ensure!(v["custody_generation_id"].as_str()==Some(e.custody_generation_id.as_str())&&v["custody_ledger_sha256"].as_str()==Some(e.custody_ledger_sha256.as_str())&&v["root_device"].as_u64()==Some(e.root_device)&&v["root_inode"].as_u64()==Some(e.root_inode),"Imported root/history association differs");
 let migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *conn).await?;
 authenticated_pg_profile(&migrations)?;ensure!(v["postgres_migrations_sha256"].as_str()==Some(digest(&serde_json::to_vec(&migrations)?).as_str()),"Independent engine PG migration ledger changed");
 ensure!(v["target_catalog_sha256"].as_str()==Some(digest(&serde_json::to_vec(&engine_without_control_catalog(catalog(conn,&e.target.schema_name).await?)?)?).as_str()),"Engine original catalog changed");
 Ok(())
}


async fn engine_control_schema(conn:&mut sqlx::PgConnection,schema:&str)->anyhow::Result<()> {
 let columns:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(c.relname,a.attname,a.attnum,format_type(a.atttypid,a.atttypmod),a.attnotnull,coalesce(pg_get_expr(d.adbin,d.adrelid),''),a.attidentity::text,a.attgenerated::text,(a.attcollation=0 OR a.attcollation='pg_catalog.default'::regcollation))::text FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace JOIN pg_catalog.pg_attribute a ON a.attrelid=c.oid LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid=c.oid AND d.adnum=a.attnum WHERE n.nspname=$1 AND c.relname IN ('engine_transfer_current','engine_transfer_generations') AND a.attnum>0 AND NOT a.attisdropped ORDER BY c.relname,a.attnum").bind(schema).fetch_all(&mut *conn).await?;
 let expected=serde_json::json!([
 ["engine_transfer_current","singleton",1,"bigint",true,"","","",true],
 ["engine_transfer_current","transfer_id",2,"text",true,"","","",true],
 ["engine_transfer_generations","transfer_id",1,"text",true,"","","",true],
 ["engine_transfer_generations","envelope_json",2,"text",true,"","","",true],
 ["engine_transfer_generations","envelope_sha256",3,"text",true,"","","",true],
 ["engine_transfer_generations","state",4,"text",true,"","","",true]]);
 let observed=columns.iter().map(|s|serde_json::from_str::<serde_json::Value>(s)).collect::<Result<Vec<_>,_>>()?;
 ensure!(serde_json::to_value(observed)?==expected,"Engine control columns/defaults/collations differ");
 let defs:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(c.relname,k.conname,k.contype::text,pg_get_constraintdef(k.oid,false),k.condeferrable,k.condeferred,k.convalidated)::text FROM pg_catalog.pg_constraint k JOIN pg_catalog.pg_class c ON c.oid=k.conrelid JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname IN ('engine_transfer_current','engine_transfer_generations') ORDER BY c.relname,k.conname").bind(schema).fetch_all(&mut *conn).await?;
 let expected=serde_json::json!([
 ["engine_transfer_current","engine_transfer_current_pkey","p","PRIMARY KEY (singleton)",false,false,true],
 ["engine_transfer_current","engine_transfer_current_singleton_check","c","CHECK ((singleton = 1))",false,false,true],
 ["engine_transfer_current","engine_transfer_current_transfer_id_fkey","f","FOREIGN KEY (transfer_id) REFERENCES engine_transfer_generations(transfer_id)",false,false,true],
 ["engine_transfer_generations","engine_transfer_generations_pkey","p","PRIMARY KEY (transfer_id)",false,false,true],
 ["engine_transfer_generations","engine_transfer_generations_state_check","c","CHECK ((state = ANY (ARRAY['committed'::text, 'needs_review'::text])))",false,false,true],
 ["engine_transfer_generations","engine_transfer_generations_transfer_id_fkey","f","FOREIGN KEY (transfer_id) REFERENCES engine_transfer_import_commitments(transfer_id)",false,false,true]]);
 let observed=defs.iter().map(|s|serde_json::from_str::<serde_json::Value>(s)).collect::<Result<Vec<_>,_>>()?;
 ensure!(serde_json::to_value(observed)?==expected,"Engine control primary/check/foreign definitions differ");
 let relations:Vec<(String,String,String,bool,bool,i64,i64,i64)>=sqlx::query_as("SELECT c.relname::text,c.relkind::text,c.relpersistence::text,c.relrowsecurity,c.relforcerowsecurity,(SELECT count(*) FROM pg_catalog.pg_trigger t WHERE t.tgrelid=c.oid AND NOT t.tgisinternal),(SELECT count(*) FROM pg_catalog.pg_policy p WHERE p.polrelid=c.oid),(SELECT count(*) FROM pg_catalog.pg_inherits i WHERE i.inhrelid=c.oid OR i.inhparent=c.oid) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname IN ('engine_transfer_current','engine_transfer_generations') ORDER BY c.relname").bind(schema).fetch_all(&mut *conn).await?;
 ensure!(relations==vec![("engine_transfer_current".into(),"r".into(),"p".into(),false,false,0,0,0),("engine_transfer_generations".into(),"r".into(),"p".into(),false,false,0,0,0)],"Engine control relation/trigger/policy differs");
 let indices:Vec<(String,String,bool,bool,bool,bool,String,String,bool,bool)>=sqlx::query_as("SELECT c.relname::text,ic.relname::text,i.indisprimary,i.indisunique,i.indisvalid,i.indisready,a.amname::text,i.indkey::text,i.indexprs IS NULL,i.indpred IS NULL FROM pg_catalog.pg_index i JOIN pg_catalog.pg_class c ON c.oid=i.indrelid JOIN pg_catalog.pg_class ic ON ic.oid=i.indexrelid JOIN pg_catalog.pg_am a ON a.oid=ic.relam JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname IN ('engine_transfer_current','engine_transfer_generations') ORDER BY c.relname,ic.relname").bind(schema).fetch_all(&mut *conn).await?;
 ensure!(indices==vec![("engine_transfer_current".into(),"engine_transfer_current_pkey".into(),true,true,true,true,"btree".into(),"1".into(),true,true),("engine_transfer_generations".into(),"engine_transfer_generations_pkey".into(),true,true,true,true,"btree".into(),"1".into(),true,true)],"Engine control indexes differ");
 Ok(())
}

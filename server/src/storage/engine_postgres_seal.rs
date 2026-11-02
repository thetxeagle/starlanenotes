include!("engine_postgres_target_preservation.rs");
// Private PG-target custody publication. No caller-authored grants and no activation.
struct SealedPgCustody {archived:ArchivedPgRestore,row:HistorySqlRow,envelope:Vec<u8>,completion:Vec<u8>,finalization:Vec<u8>,base:String}
struct CommittedPgCustody {sealed:SealedPgCustody,attestation:Vec<u8>,native_preservation:Option<TargetCommittedPreservationPublication>}
struct RetiredPgCustody {committed:CommittedPgCustody}
#[derive(serde::Serialize,serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PgTargetAttestation {
 version:u32,restore_id:String,backup_id:String,snapshot_sha256:String,source_target:EngineTargetIdentity,target:EngineTargetIdentity,
 source_root_device:u64,source_root_inode:u64,root_device:u64,root_inode:u64,
 custody_generation_id:String,custody_ledger_sha256:String,custody_sql_sha256:String,import_commitment_sha256:String,
 seal_intent_sha256:String,completion_sha256:String,finalization_sha256:String,marker_device:u64,marker_inode:u64,
 control_relation_ids:BTreeMap<String,i64>,catalogue_sha256:String,independent_migrations_sha256:String,
 retained_roles:crate::storage::backup_inventory::UniqueMap<crate::storage::backup_inventory::Bytes>,
 #[serde(default,skip_serializing_if="Option::is_none")] parent_attestation:Option<HistoryArtifact>,
}
fn pg_token_guard(d:&OwnedPgBackup,marker:&str)->anyhow::Result<()> {
 d.parent.check()?;let fresh=Root::open(&d.root.absolute)?;ensure!(fresh.chain.len()==d.root.chain.len(),"PG custody ancestry changed");
 for(a,b)in fresh.chain.iter().zip(&d.root.chain){ensure!(identity(a)?==identity(b)?,"PG custody ancestry changed");}
 ensure!(identity(&open_at(d.parent.fd(),&d.name,libc::O_RDONLY|libc::O_DIRECTORY)?)?==d.root_identity,"PG custody root replaced");
 let held=d.roles.get(marker).context("Held original PG marker missing")?;
 ensure!(physical_stamp(&held.held)?==held.stamp&&physical_stamp(&d.root.leaf(marker)?)?==held.stamp&&planned_hash(&held.held)?==held.bytes,"Held original PG marker differs");
 if marker!=".starlane-restore-incomplete" {ensure!(!exists(d.root.fd(),".starlane-restore-incomplete")?&&!exists(d.root.fd(),".starlane-backup-incomplete")?,"PG restoration marker reappeared");}
 Ok(())
}
fn pg_token_directory(d:&mut OwnedPgBackup,path:&str,marker:&str)->anyhow::Result<()> {
 ensure!(valid_relative(path),"Invalid new PG custody directory");
 let mut parent=String::new();
 for name in path.split('/') {
  let child=if parent.is_empty(){name.into()}else{format!("{parent}/{name}")};
  if !d.directories.contains_key(&child) {
   d.fence()?;pg_token_guard(d,marker)?;
   let held=if parent.is_empty(){d.root.fd().try_clone()?}else{d.directories[&parent].0.try_clone()?};
   ensure!(identity(&destination_directory(d.root.fd(),&parent)?)?==identity(&held)?,"New custody directory parent replaced");
   let created=pg_capture_new_dir(&held,name)?;
   ensure!(identity(&destination_directory(d.root.fd(),&child)?)?==identity(&created)?,"New custody directory replaced");
   pg_token_guard(d,marker)?;d.directories.insert(child.clone(),(created.try_clone()?,physical_stamp(&created)?));pg_materialize_touch_directory(d,&parent)?;d.fence()?;
  }
  parent=child;
 }
 Ok(())
}
fn pg_token_write(d:&mut OwnedPgBackup,path:&str,raw:&[u8],marker:&str)->anyhow::Result<()> {
 d.fence()?;pg_token_guard(d,marker)?;let(parent,name,parent_path)=pg_materialize_parent(d,path)?;
 let mut held=open_at(&parent,&name,libc::O_RDWR|libc::O_CREAT|libc::O_EXCL)?;use std::io::Write;held.write_all(raw)?;held.sync_all()?;parent.sync_all()?;
 ensure!(identity(&pg_materialize_parent(d,path)?.0)?==identity(&parent)?&&identity(&d.root.leaf(path)?)?==identity(&held)?,"New custody file parent or leaf raced");
 pg_token_guard(d,marker)?;d.roles.insert(path.into(),PgCaptureHeld{stamp:physical_stamp(&held)?,bytes:planned_hash(&held)?,held});pg_materialize_touch_directory(d,&parent_path)?;d.fence()?;Ok(())
}
fn pg_token_move(d:&mut OwnedPgBackup,source:&str,target:&str,marker:&str)->anyhow::Result<()> {
 d.fence()?;pg_token_guard(d,marker)?;ensure!(!d.roles.contains_key(target),"PG custody destination already held");
 let old=&d.roles[source];let stamp=old.stamp.clone();let held=old.held.try_clone()?;let bytes=old.bytes.clone();
 ensure!(stamp.links==1,"PG custody source is aliased");let(a,an,ap)=pg_materialize_parent(d,source)?;let(b,bn,bp)=pg_materialize_parent(d,target)?;
 let an=CString::new(an)?;let bn=CString::new(bn)?;
 if source==marker{engine_checkpoint("pg-restore-custody-marker-checked-before-rename")?;}
 if unsafe{libc::renameat2(a.as_raw_fd(),an.as_ptr(),b.as_raw_fd(),bn.as_ptr(),libc::RENAME_NOREPLACE)}!=0{return Err(io::Error::last_os_error().into());}
 a.sync_all()?;b.sync_all()?;held.sync_all()?;let moved=d.root.leaf(target)?;let after=physical_stamp(&moved)?;
 ensure!(identity(&moved)?==identity(&held)?&&physical_stamp(&held)?==after&&(after.length,after.links,after.mode,after.mtime,after.mtime_ns)==(stamp.length,stamp.links,stamp.mode,stamp.mtime,stamp.mtime_ns)&&planned_hash(&moved)?==bytes,"PG custody exact moved leaf differs");
 ensure!(identity(&pg_materialize_parent(d,source)?.0)?==identity(&a)?&&identity(&pg_materialize_parent(d,target)?.0)?==identity(&b)?&&!exists(&a,an.to_str()?)?,"PG custody move ancestry raced");
 // Exact marker move is verified first; its new role then becomes the guard.
 d.roles.remove(source);d.roles.insert(target.into(),PgCaptureHeld{held:moved,stamp:after,bytes});pg_materialize_touch_directory(d,&ap)?;if ap!=bp{pg_materialize_touch_directory(d,&bp)?;}
 pg_token_guard(d,if source==marker{target}else{marker})?;d.fence()?;Ok(())
}
fn seal_postgres_custody(mut archived:ArchivedPgRestore)->anyhow::Result<SealedPgCustody>{
 archived.materialized.owner.imported.prepared.fence()?;
 let prepared=&mut archived.materialized.owner.imported.prepared;
 let prior=&prepared.snapshot.history;let old:HistoryEnvelope=serde_json::from_slice(&prior.raw_generations[&prior.current_id])?;
 ensure!(old.version==3,"PG predecessor must be validated finalized v3");
 let gid=uuid::Uuid::new_v4().to_string();let base=format!("{NAMESPACE}/generations/{gid}");
 let scopes=archived.materialized.owner.scopes.iter().map(|s|Scope{namespace:s.namespace.clone(),op_id:s.operation.clone(),evidence_sha256:digest(&s.evidence),bindings:s.bindings.clone()}).collect();
 let admission=pg_certified_admission_roles(&prepared.snapshot,&prepared.destination,archived.materialized.owner.admission_replay.as_ref())?;
 let completed_absence=pg_certified_completed_roles(&prepared.snapshot,&prepared.destination,archived.materialized.owner.completed.as_ref())?;
 let composite=pg_certified_composite_roles(&prepared.snapshot,&prepared.destination,archived.materialized.owner.composite.as_ref(),completed_absence.as_ref())?;
 let runtime=Ledger{ historical_preservation:None,composite:composite.clone(),completed_absence:if composite.is_some(){None}else{completed_absence.clone()},version:if composite.is_some(){4}else if completed_absence.is_some(){3}else if admission.is_some(){2}else{1},admission,generation_id:gid.clone(),backup_id:prepared.snapshot.manifest.backup_id.clone(),manifest_sha256:digest(&prepared.snapshot.held.manifest),database_sha256:old.runtime.database_sha256.clone(),root_device:prepared.destination.root_identity.0,root_inode:prepared.destination.root_identity.1,scopes};validate(&runtime)?;
 let mut proofs=vec![];
 for s in &archived.materialized.owner.scopes {
  let dir=format!("{}/{}",s.namespace,s.operation);let manifest=format!("{dir}/{}",if s.namespace==".attachment-upload-journal"{"intent.json"}else{"manifest.json"});
  let evidence=if s.namespace==".move-relocation-journal"{manifest.clone()}else{format!("{dir}/install-proof.json")};
  ensure!(prepared.destination.roles[&evidence].bytes.sha256==digest(&s.evidence),"Fresh owner proof role differs");
  let mint=old.proofs.iter().find(|p|p.namespace==s.namespace&&p.op_id==s.operation).and_then(|p|p.mint_intent.clone());
  proofs.push(HistoryProof{namespace:s.namespace.clone(),op_id:s.operation.clone(),manifest:HistoryArtifact{sha256:prepared.destination.roles[&manifest].bytes.sha256.clone(),relative:manifest},evidence:HistoryArtifact{relative:evidence,sha256:digest(&s.evidence)},mint_intent:mint});
 }
 let denied=archived.materialized.owner.denied.iter().map(|(n,o)|DeniedReservation{namespace:n.clone(),op_id:o.clone()}).collect::<Vec<_>>();
 let archives=prior.raw_generations.iter().map(|(id,raw)|HistoryLink{generation_id:id.clone(),ledger_sha256:digest(raw)}).collect::<Vec<_>>();
 let parent=Some(HistoryLink{generation_id:prior.current_id.clone(),ledger_sha256:digest(&prior.raw_generations[&prior.current_id])});
 let d=&mut prepared.destination;let marker=d.roles[".starlane-restore-incomplete"].stamp.clone();
 ensure!(planned_hash(&d.roles[".starlane-restore-incomplete"].held)?.sha256==digest(format!("starlane private copy {}\n",runtime.backup_id).as_bytes()),"New PG marker not genuine v3 grammar");
 let intent=HistoryIntent{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),source_root:crate::storage::backup_inventory::Identity{device:prepared.snapshot.manifest.source_root.device,inode:prepared.snapshot.manifest.source_root.inode},target_root:crate::storage::backup_inventory::Identity{device:runtime.root_device,inode:runtime.root_inode},retired_marker:HistoryArtifact{relative:format!("{base}/retired-marker"),sha256:d.roles[".starlane-restore-incomplete"].bytes.sha256.clone()},runtime_sha256:digest(&serde_json::to_vec(&runtime)?),parent:parent.clone(),archives:archives.clone(),denied:denied.clone(),proofs:proofs.clone()};
 let raw_intent=serde_json::to_vec(&intent)?;let created_at=chrono::Utc::now().to_rfc3339();
 let completion=serde_json::to_vec(&HistoryCompletion{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),intent_sha256:digest(&raw_intent),runtime_sha256:intent.runtime_sha256.clone(),root_device:runtime.root_device.to_string(),root_inode:runtime.root_inode.to_string(),state:"committed".into(),created_at:created_at.clone()})?;
 let finalization=serde_json::to_vec(&HistoryFinalization{version:1,generation_id:gid.clone(),backup_id:runtime.backup_id.clone(),target_root:intent.target_root.clone(),original_marker:crate::storage::backup_inventory::Identity{device:marker.device,inode:marker.inode},retired_marker:intent.retired_marker.clone(),intent_sha256:digest(&raw_intent),completion_sha256:digest(&completion)})?;
 let envelope=serde_json::to_vec(&HistoryEnvelope{version:3,runtime:runtime.clone(),intent_sha256:digest(&raw_intent),completion_sha256:digest(&completion),finalization:Some(HistoryArtifact{relative:format!("{base}/finalization.json"),sha256:digest(&finalization)}),parent,archives,denied,proofs})?;history_parse(&envelope)?;
 let row=HistorySqlRow{generation_id:gid,backup_id:runtime.backup_id,manifest_sha256:runtime.manifest_sha256,database_sha256:runtime.database_sha256,ledger_sha256:digest(&envelope),root_device:runtime.root_device.to_string(),root_inode:runtime.root_inode.to_string(),state:"committed".into(),created_at};
 pg_token_directory(d,&base,".starlane-restore-incomplete")?;pg_token_write(d,&format!("{base}/intent.json"),&raw_intent,".starlane-restore-incomplete")?;
 let archive=format!("{NAMESPACE}/archive/{}",prior.current_id);pg_token_directory(d,&archive,".starlane-restore-incomplete")?;
 pg_token_move(d,&format!("{NAMESPACE}/current.json"),&format!("{archive}/ledger.json"),".starlane-restore-incomplete")?;
 pg_token_write(d,&format!("{NAMESPACE}/current.json"),&envelope,".starlane-restore-incomplete")?;
 if let Some(PgCapturedSourceAuthority::PgRestore{generation_id,attestation_sha256})=&prepared.snapshot.manifest.source_authority {
  let path=format!(".pg-restoration-custody/{generation_id}/source-attestation.json");
  ensure!(d.roles[".pg-restoration-custody/current.json"].bytes.sha256==*attestation_sha256,"Prior current PgRestore attestation changed before archival");
  pg_token_move(d,".pg-restoration-custody/current.json",&path,".starlane-restore-incomplete")?;
 }
 engine_checkpoint("pg-restore-custody-sealed")?;
 Ok(SealedPgCustody{archived,row,envelope,completion,finalization,base})
}
async fn pg_custody_rows(conn:&mut sqlx::PgConnection)->anyhow::Result<(Vec<HistorySqlRow>,Vec<(i64,String)>)>{
 let raw=sqlx::query("SELECT generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at FROM storage_restore_generations ORDER BY generation_id").fetch_all(&mut *conn).await?;
 let mut rows=vec![];for r in raw{rows.push(HistorySqlRow{generation_id:r.try_get("generation_id")?,backup_id:r.try_get("backup_id")?,manifest_sha256:r.try_get("manifest_sha256")?,database_sha256:r.try_get("database_sha256")?,ledger_sha256:r.try_get("ledger_sha256")?,root_device:r.try_get("root_device")?,root_inode:r.try_get("root_inode")?,state:r.try_get("state")?,created_at:r.try_get("created_at")?});}
 Ok((rows,sqlx::query_as("SELECT singleton,generation_id FROM storage_restore_current ORDER BY singleton").fetch_all(conn).await?))
}
async fn pg_restore_control_ids(conn:&mut sqlx::PgConnection,schema:&str)->anyhow::Result<BTreeMap<String,i64>>{
 let names=["pg_restoration_imports","pg_restoration_generations","pg_restoration_current","storage_restore_generations","storage_restore_current"];
 let rows:Vec<(String,i64,String,String,bool,bool,i64,i64,i64)>=sqlx::query_as("SELECT c.relname::text,c.oid::bigint,c.relkind::text,c.relpersistence::text,c.relrowsecurity,c.relforcerowsecurity,(SELECT count(*) FROM pg_catalog.pg_trigger t WHERE t.tgrelid=c.oid AND NOT t.tgisinternal),(SELECT count(*) FROM pg_catalog.pg_policy p WHERE p.polrelid=c.oid),(SELECT count(*) FROM pg_catalog.pg_inherits i WHERE i.inhrelid=c.oid OR i.inhparent=c.oid) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname=ANY($2) ORDER BY c.relname").bind(schema).bind(names.as_slice()).fetch_all(conn).await?;
 ensure!(rows.len()==names.len(),"PG restoration control relation missing");let mut ids=BTreeMap::new();for(n,id,kind,persistence,rls,forced,triggers,policies,inherits)in rows{ensure!(kind=="r"&&persistence=="p"&&!rls&&!forced&&triggers==0&&policies==0&&inherits==0,"PG control relation carries foreign behavior");ids.insert(n,id);}Ok(ids)
}
async fn commit_postgres_custody(mut sealed:SealedPgCustody,url:&str)->anyhow::Result<CommittedPgCustody>{
 let imported=&mut sealed.archived.materialized.owner.imported;imported.prepared.fence()?;
 let mut conn=sqlx::PgConnection::connect(url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&imported.target.schema_name)?)).execute(&mut conn).await?;
 ensure!(engine_target_identity(&mut conn,&imported.target.schema_name).await?==imported.target,"PG custody wrong target");
 let import_record:serde_json::Value=serde_json::from_slice(&imported.raw_commitment)?;
 let before_catalog=catalog(&mut conn,&imported.target.schema_name).await?;
 ensure!(import_record["target_catalogue_sha256"].as_str()==Some(digest(&serde_json::to_vec(&before_catalog)?).as_str()),"Actual imported PG schema changed before custody");
 let before_migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut conn).await?;
 ensure!(import_record["independent_migrations_sha256"].as_str()==Some(digest(&serde_json::to_vec(&before_migrations)?).as_str()),"Actual imported independent PG migrations changed before custody");

 let mut tx=conn.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await?;sqlx::query("SET LOCAL synchronous_commit=on").execute(&mut *tx).await?;
 for n in imported.expected_target_tables.keys(){sqlx::query(&format!("LOCK TABLE {} IN SHARE MODE",quoted(n)?)).execute(&mut *tx).await?;}
 sqlx::query("LOCK TABLE pg_restoration_imports IN SHARE MODE").execute(&mut *tx).await?;
 ensure!(pg_tables(&mut tx,&shape()?).await?==imported.expected_target_tables,"Pre-custody SQL differs from genuine import");
 let imports:Vec<(String,String,String)>=sqlx::query_as("SELECT restore_id,commitment_json,commitment_sha256 FROM pg_restoration_imports ORDER BY restore_id").fetch_all(&mut *tx).await?;ensure!(imports==vec![(imported.prepared.restore_id.clone(),std::str::from_utf8(&imported.raw_commitment)?.into(),digest(&imported.raw_commitment))],"Actual PG import commitment changed");
 let old=pg_custody_rows(&mut tx).await?;ensure!(old.0==imported.prepared.snapshot.history.rows&&old.1==vec![(1,imported.prepared.snapshot.history.current_id.clone())],"Original SQL historical rows changed");
 sqlx::query("INSERT INTO storage_restore_generations (generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(&sealed.row.generation_id).bind(&sealed.row.backup_id).bind(&sealed.row.manifest_sha256).bind(&sealed.row.database_sha256).bind(&sealed.row.ledger_sha256).bind(&sealed.row.root_device).bind(&sealed.row.root_inode).bind(&sealed.row.state).bind(&sealed.row.created_at).execute(&mut *tx).await?;
 ensure!(sqlx::query("UPDATE storage_restore_current SET generation_id=$1 WHERE singleton=1 AND generation_id=$2").bind(&sealed.row.generation_id).bind(&old.1[0].1).execute(&mut *tx).await?.rows_affected()==1,"Actual prior custody pointer changed");
 sqlx::query("CREATE TABLE pg_restoration_generations (restore_id TEXT PRIMARY KEY NOT NULL REFERENCES pg_restoration_imports(restore_id),envelope_json TEXT NOT NULL,envelope_sha256 TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN ('committed','needs_review')))").execute(&mut *tx).await?;
 sqlx::query("CREATE TABLE pg_restoration_current (singleton BIGINT PRIMARY KEY CHECK(singleton=1),restore_id TEXT NOT NULL REFERENCES pg_restoration_generations(restore_id))").execute(&mut *tx).await?;
 let after_catalog=catalog(&mut tx,&imported.target.schema_name).await?;
 let preserved_catalog=after_catalog.iter().filter(|raw|{let v:serde_json::Value=serde_json::from_str(raw).unwrap();!matches!(v.get(1).and_then(|x|x.as_str()),Some("pg_restoration_generations"|"pg_restoration_current"))}).cloned().collect::<Vec<_>>();
 ensure!(preserved_catalog==before_catalog,"Controlled PG creation changed original application schema");
 let rows=pg_tables(&mut tx,&shape()?).await?;
 for(n,t)in &imported.expected_target_tables {if !["storage_restore_generations","storage_restore_current"].contains(&n.as_str()){ensure!(rows[n]==*t,"Custody changed application table {n}");}}
 let mut expected=old.0.clone();expected.push(sealed.row.clone());expected.sort_by(|a,b|a.generation_id.cmp(&b.generation_id));ensure!(pg_custody_rows(&mut tx).await?==(expected,vec![(1,sealed.row.generation_id.clone())]),"Controlled custody transition differs");
 let custody=rows.into_iter().filter(|(n,_)|["storage_restore_generations","storage_restore_current"].contains(&n.as_str())).collect::<BTreeMap<_,_>>();
 let marker=imported.prepared.destination.roles[".starlane-restore-incomplete"].stamp.clone();
 let migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *tx).await?;
 ensure!(migrations==before_migrations,"Controlled PG custody changed independent migrations");
 let native_preservation=TargetCommittedPreservationPublication::mint(imported,&sealed.row.generation_id)?;
 let retained_roles=imported.prepared.destination.roles.iter().filter(|(p,_)|(p.starts_with(".pg-restoration-custody/")&&p.as_str()!=".pg-restoration-custody/current.json")||p.starts_with(".engine-transfer-custody/")||p.as_str()==".starlane-engine-transfer-request").map(|(p,r)|(p.clone(),r.bytes.clone())).collect();
 let parent_attestation=match &imported.prepared.snapshot.manifest.source_authority{Some(PgCapturedSourceAuthority::PgRestore{generation_id,attestation_sha256})=>Some(HistoryArtifact{relative:format!(".pg-restoration-custody/{generation_id}/source-attestation.json"),sha256:attestation_sha256.clone()}),None=>None};
 let version=if parent_attestation.is_some(){2}else{1};
 let description=PgTargetAttestation{parent_attestation,retained_roles:crate::storage::backup_inventory::UniqueMap(retained_roles),version,restore_id:imported.prepared.restore_id.clone(),backup_id:sealed.row.backup_id.clone(),snapshot_sha256:sealed.row.manifest_sha256.clone(),source_target:imported.prepared.snapshot.manifest.source_target.clone(),target:imported.target.clone(),source_root_device:imported.prepared.snapshot.manifest.source_root.device,source_root_inode:imported.prepared.snapshot.manifest.source_root.inode,root_device:marker.device,root_inode:imported.prepared.destination.root_identity.1,custody_generation_id:sealed.row.generation_id.clone(),custody_ledger_sha256:sealed.row.ledger_sha256.clone(),custody_sql_sha256:engine_typed_hash(&custody)?,import_commitment_sha256:digest(&imported.raw_commitment),seal_intent_sha256:imported.prepared.destination.roles[&format!("{}/intent.json",sealed.base)].bytes.sha256.clone(),completion_sha256:digest(&sealed.completion),finalization_sha256:digest(&sealed.finalization),marker_device:marker.device,marker_inode:marker.inode,control_relation_ids:pg_restore_control_ids(&mut tx,&imported.target.schema_name).await?,catalogue_sha256:digest(&serde_json::to_vec(&catalog(&mut tx,&imported.target.schema_name).await?)?),independent_migrations_sha256:digest(&serde_json::to_vec(&migrations)?)};
 let attestation=serde_json::to_vec(&description)?;
 pg_token_write(&mut imported.prepared.destination,".pg-restoration-custody/current.json",&attestation,".starlane-restore-incomplete")?;
 sqlx::query("INSERT INTO pg_restoration_generations (restore_id,envelope_json,envelope_sha256,state) VALUES ($1,$2,$3,'committed')").bind(&description.restore_id).bind(std::str::from_utf8(&attestation)?).bind(digest(&attestation)).execute(&mut *tx).await?;
 sqlx::query("INSERT INTO pg_restoration_current (singleton,restore_id) VALUES (1,$1)").bind(&description.restore_id).execute(&mut *tx).await?;
 imported.prepared.fence()?;tx.commit().await?;conn.close().await?;engine_checkpoint("pg-restore-custody-sql-committed")?;
 let mut read=sqlx::PgConnection::connect(url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&description.target.schema_name)?)).execute(&mut read).await?;pg_attestation_sql(&mut read,&description,&attestation).await?;read.close().await?;imported.prepared.fence()?;
 pg_token_write(&mut imported.prepared.destination,&format!("{}/completion.json",sealed.base),&sealed.completion,".starlane-restore-incomplete")?;engine_checkpoint("pg-restore-custody-completion")?;
 if let Some(owner)=&native_preservation{owner.fence(&imported.prepared.destination)?;}
 Ok(CommittedPgCustody{sealed,attestation,native_preservation})
}
async fn pg_attestation_sql(conn:&mut sqlx::PgConnection,e:&PgTargetAttestation,raw:&[u8])->anyhow::Result<()> {
 ensure!([1,2].contains(&e.version)&&(e.version==2)==e.parent_attestation.is_some()&&e.target.schema_name==format!("restore_{}",uuid::Uuid::parse_str(&e.restore_id)?.simple())&&engine_target_identity(conn,&e.target.schema_name).await?==e.target,"PG attestation native identity differs");
 ensure!(pg_restore_control_ids(conn,&e.target.schema_name).await?==e.control_relation_ids&&digest(&serde_json::to_vec(&catalog(conn,&e.target.schema_name).await?)?)==e.catalogue_sha256,"PG attestation control/catalogue identity differs");
 let migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *conn).await?;ensure!(digest(&serde_json::to_vec(&migrations)?)==e.independent_migrations_sha256,"PG attestation independent migrations differ");
 let imports:Vec<(String,String,String)>=sqlx::query_as("SELECT restore_id,commitment_json,commitment_sha256 FROM pg_restoration_imports ORDER BY restore_id").fetch_all(&mut *conn).await?;
 ensure!(imports.len()==1&&imports[0].0==e.restore_id&&imports[0].2==e.import_commitment_sha256&&digest(imports[0].1.as_bytes())==e.import_commitment_sha256,"PG import evidence changed");
 let generations:Vec<(String,String,String,String)>=sqlx::query_as("SELECT restore_id,envelope_json,envelope_sha256,state FROM pg_restoration_generations ORDER BY restore_id").fetch_all(&mut *conn).await?;ensure!(generations==vec![(e.restore_id.clone(),std::str::from_utf8(raw)?.into(),digest(raw),"committed".into())],"PG generation SQL commitment differs");
 let current:Vec<(i64,String)>=sqlx::query_as("SELECT singleton,restore_id FROM pg_restoration_current ORDER BY singleton").fetch_all(&mut *conn).await?;ensure!(current==vec![(1,e.restore_id.clone())],"PG current pointer differs");
 let custody_shape=shape()?.into_iter().filter(|(n,_)|["storage_restore_generations","storage_restore_current"].contains(&n.as_str())).collect();let tables=pg_tables(conn,&custody_shape).await?;let custody=tables.into_iter().filter(|(n,_)|["storage_restore_generations","storage_restore_current"].contains(&n.as_str())).collect::<BTreeMap<_,_>>();ensure!(engine_typed_hash(&custody)?==e.custody_sql_sha256,"PG immutable custody SQL differs");
 let(rows,pointer)=pg_custody_rows(conn).await?;ensure!(pointer==vec![(1,e.custody_generation_id.clone())]&&rows.iter().any(|r|r.generation_id==e.custody_generation_id&&r.ledger_sha256==e.custody_ledger_sha256&&r.root_device==e.root_device.to_string()&&r.root_inode==e.root_inode.to_string()),"PG custody current association differs");Ok(())
}
fn retire_postgres_custody(mut committed:CommittedPgCustody)->anyhow::Result<RetiredPgCustody>{
 if let Some(owner)=&committed.native_preservation{owner.fence(&committed.sealed.archived.materialized.owner.imported.prepared.destination)?;}
 let target=format!("{}/retired-marker",committed.sealed.base);let prepared=&mut committed.sealed.archived.materialized.owner.imported.prepared;prepared.fence()?;
 engine_checkpoint("pg-restore-custody-before-marker-retirement")?;
 pg_token_move(&mut prepared.destination,".starlane-restore-incomplete",&target,".starlane-restore-incomplete")?;
 prepared.snapshot.held.fence()?;prepared.destination.fence()?;engine_checkpoint("pg-restore-custody-marker-retired-before-finalization")?;
 if let Some(owner)=&committed.native_preservation{owner.fence(&prepared.destination)?;}
 Ok(RetiredPgCustody{committed})
}
async fn inspect_postgres_restore_custody_raw(root_path:&Path,url:&str)->anyhow::Result<()> {
 let root=Root::open(root_path)?;engine_root_fence(&root)?;
 let raw=read_ledger(&root.leaf(".pg-restoration-custody/current.json")?)?;let e:PgTargetAttestation=serde_json::from_slice(&raw)?;
 ensure!(identity(root.fd())?==(e.root_device,e.root_inode),"PG attestation wrong held root");
 let mut conn=sqlx::PgConnection::connect(url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&e.target.schema_name)?)).execute(&mut conn).await?;
 let mut tx=conn.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *tx).await?;pg_attestation_sql(&mut tx,&e,&raw).await?;let(rows,pointer)=pg_custody_rows(&mut tx).await?;
 ensure!(digest(&read_ledger(&root.leaf(".restoration-custody/current.json")?)?)==e.custody_ledger_sha256,"PG current raw custody differs");
 let source=HistorySource::new_with_check(&root,None,engine_root_fence)?;
 let closed=history_closure(source,identity(root.fd())?,Some((&rows,&pointer)))?;closed.source.fence()?;
 ensure!(closed.current_id==e.custody_generation_id,"PG closed history current differs");
 let finalization=read_ledger(&root.leaf(&format!("{NAMESPACE}/generations/{}/finalization.json",e.custody_generation_id))?)?;ensure!(digest(&finalization)==e.finalization_sha256,"PG finalization differs");
 tx.rollback().await?;conn.close().await?;engine_root_fence(&root)?;Ok(())
}

async fn inspect_postgres_restore_custody(root_path:&Path,url:&str)->anyhow::Result<()> {
 inspect_postgres_restore_custody_raw(root_path,url).await?;
 let root=Root::open(root_path)?;let raw=read_ledger(&root.leaf(".pg-restoration-custody/current.json")?)?;
 let e:PgTargetAttestation=serde_json::from_slice(&raw)?;pg_target_final_current(&root,&e,&raw)?;engine_root_fence(&root)?;Ok(())
}
// Only this nominal actual retired owner may run the preparation-only final
// inspection before its new acknowledgment exists. No public skip flag exists.
async fn inspect_actual_retired_pg_before_ack(retired:&RetiredPgCustody,url:&str)->anyhow::Result<()> {
 let p=&retired.committed.sealed.archived.materialized.owner.imported.prepared;
 p.snapshot.held.fence()?;p.destination.fence()?;
 if let Some(owner)=&retired.committed.native_preservation{owner.fence(&p.destination)?;}
 inspect_postgres_restore_custody_raw(&p.destination.root.absolute,url).await?;
 p.snapshot.held.fence()?;p.destination.fence()?;Ok(())
}

// Last durable publication is possible only from the exact privately retired token.
struct FinalizedPgCustody {retired:RetiredPgCustody}
async fn finalize_postgres_custody(mut retired:RetiredPgCustody,url:&str)->anyhow::Result<FinalizedPgCustody>{
 let native=&retired.committed.native_preservation;
 let sealed=&mut retired.committed.sealed;
 let imported=&mut sealed.archived.materialized.owner.imported;
 let marker=format!("{}/retired-marker",sealed.base);
 if let Some(owner)=native{owner.fence(&imported.prepared.destination)?;}
 imported.prepared.snapshot.held.fence()?;imported.prepared.destination.fence()?;
 pg_token_guard(&imported.prepared.destination,&marker)?;
 let e:PgTargetAttestation=serde_json::from_slice(&retired.committed.attestation)?;
 let f:HistoryFinalization=serde_json::from_slice(&sealed.finalization)?;
 ensure!(f.original_marker.device==e.marker_device&&f.original_marker.inode==e.marker_inode&&digest(&sealed.finalization)==e.finalization_sha256,"Predetermined PG finalization differs");
 let held=&imported.prepared.destination.roles[&marker];
 ensure!((held.stamp.device,held.stamp.inode)==(e.marker_device,e.marker_inode),"PG original marker retirement not proven");
 ensure!(digest(&sealed.envelope)==e.custody_ledger_sha256&&digest(&sealed.completion)==e.completion_sha256,"PG publication descriptions differ");
 let mut conn=sqlx::PgConnection::connect(url).await?;
 sqlx::query(&format!("SET search_path TO {}",quoted(&e.target.schema_name)?)).execute(&mut conn).await?;
 let mut tx=conn.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *tx).await?;
 pg_attestation_sql(&mut tx,&e,&retired.committed.attestation).await?;
 imported.prepared.snapshot.held.fence()?;imported.prepared.destination.fence()?;pg_token_guard(&imported.prepared.destination,&marker)?;
 engine_checkpoint("pg-restore-custody-before-finalization")?;
 pg_token_write(&mut imported.prepared.destination,&format!("{}/finalization.json",sealed.base),&sealed.finalization,&marker)?;
 imported.prepared.snapshot.held.fence()?;imported.prepared.destination.fence()?;pg_token_guard(&imported.prepared.destination,&marker)?;
 tx.rollback().await?;conn.close().await?;
 engine_checkpoint("pg-restore-custody-finalization-durable")?;
 // Fresh actual native SQL and the complete historical descriptor closure qualify
 // the last write. This token still does not activate a serving process.
 if let Some(owner)=native{owner.fence(&imported.prepared.destination)?;}
 inspect_actual_retired_pg_before_ack(&retired,url).await?;
 publish_pg_target_final(&mut retired,url).await?;
 Ok(FinalizedPgCustody{retired})
}

async fn pg_custody_rows_any(conn:&mut sqlx::AnyConnection)->anyhow::Result<(Vec<HistorySqlRow>,Vec<(i64,String)>)>{
 let raw=sqlx::query("SELECT generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at FROM storage_restore_generations ORDER BY generation_id").fetch_all(&mut *conn).await?;
 let mut rows=vec![];for r in raw{rows.push(HistorySqlRow{generation_id:r.try_get("generation_id")?,backup_id:r.try_get("backup_id")?,manifest_sha256:r.try_get("manifest_sha256")?,database_sha256:r.try_get("database_sha256")?,ledger_sha256:r.try_get("ledger_sha256")?,root_device:r.try_get("root_device")?,root_inode:r.try_get("root_inode")?,state:r.try_get("state")?,created_at:r.try_get("created_at")?});}
 Ok((rows,sqlx::query_as("SELECT singleton,generation_id FROM storage_restore_current ORDER BY singleton").fetch_all(conn).await?))
}
async fn pg_restore_control_ids_any(conn:&mut sqlx::AnyConnection,schema:&str)->anyhow::Result<BTreeMap<String,i64>>{
 let names=["pg_restoration_imports","pg_restoration_generations","pg_restoration_current","storage_restore_generations","storage_restore_current"];
 let rows:Vec<(String,i64,String,String,bool,bool,i64,i64,i64)>=sqlx::query_as("SELECT c.relname::text,c.oid::bigint,c.relkind::text,c.relpersistence::text,c.relrowsecurity,c.relforcerowsecurity,(SELECT count(*) FROM pg_catalog.pg_trigger t WHERE t.tgrelid=c.oid AND NOT t.tgisinternal),(SELECT count(*) FROM pg_catalog.pg_policy p WHERE p.polrelid=c.oid),(SELECT count(*) FROM pg_catalog.pg_inherits i WHERE i.inhrelid=c.oid OR i.inhparent=c.oid) FROM pg_catalog.pg_class c JOIN pg_catalog.pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname=$1 AND c.relname IN ('pg_restoration_imports','pg_restoration_generations','pg_restoration_current','storage_restore_generations','storage_restore_current') ORDER BY c.relname").bind(schema).fetch_all(conn).await?;
 ensure!(rows.len()==names.len(),"PG restoration control relation missing");let mut ids=BTreeMap::new();for(n,id,kind,persistence,rls,forced,triggers,policies,inherits)in rows{ensure!(kind=="r"&&persistence=="p"&&!rls&&!forced&&triggers==0&&policies==0&&inherits==0,"PG control relation carries foreign behavior");ids.insert(n,id);}Ok(ids)
}

async fn pg_attestation_sql_any(conn:&mut sqlx::AnyConnection,e:&PgTargetAttestation,raw:&[u8])->anyhow::Result<()> {
 ensure!([1,2].contains(&e.version)&&(e.version==2)==e.parent_attestation.is_some()&&e.target.schema_name==format!("restore_{}",uuid::Uuid::parse_str(&e.restore_id)?.simple())&&engine_target_identity_any(conn,&e.target.schema_name).await?==e.target,"PG attestation native identity differs");
 ensure!(pg_restore_control_ids_any(conn,&e.target.schema_name).await?==e.control_relation_ids&&digest(&serde_json::to_vec(&catalog_any(conn,&e.target.schema_name).await?)?)==e.catalogue_sha256,"PG attestation control/catalogue identity differs");
 let migrations:Vec<String>=sqlx::query_scalar("SELECT jsonb_build_array(version,description,(extract(epoch from installed_on)::numeric)::text,success,encode(checksum,'hex'),execution_time)::text FROM _sqlx_migrations ORDER BY version").fetch_all(&mut *conn).await?;ensure!(digest(&serde_json::to_vec(&migrations)?)==e.independent_migrations_sha256,"PG attestation independent migrations differ");
 let imports:Vec<(String,String,String)>=sqlx::query_as("SELECT restore_id,commitment_json,commitment_sha256 FROM pg_restoration_imports ORDER BY restore_id").fetch_all(&mut *conn).await?;
 ensure!(imports.len()==1&&imports[0].0==e.restore_id&&imports[0].2==e.import_commitment_sha256&&digest(imports[0].1.as_bytes())==e.import_commitment_sha256,"PG import evidence changed");
 let generations:Vec<(String,String,String,String)>=sqlx::query_as("SELECT restore_id,envelope_json,envelope_sha256,state FROM pg_restoration_generations ORDER BY restore_id").fetch_all(&mut *conn).await?;ensure!(generations==vec![(e.restore_id.clone(),std::str::from_utf8(raw)?.into(),digest(raw),"committed".into())],"PG generation SQL commitment differs");
 let current:Vec<(i64,String)>=sqlx::query_as("SELECT singleton,restore_id FROM pg_restoration_current ORDER BY singleton").fetch_all(&mut *conn).await?;ensure!(current==vec![(1,e.restore_id.clone())],"PG current pointer differs");
 let custody_shape=shape()?.into_iter().filter(|(n,_)|["storage_restore_generations","storage_restore_current"].contains(&n.as_str())).collect();let tables=pg_tables_any(conn,&custody_shape).await?;let custody=tables.into_iter().filter(|(n,_)|["storage_restore_generations","storage_restore_current"].contains(&n.as_str())).collect::<BTreeMap<_,_>>();ensure!(engine_typed_hash(&custody)?==e.custody_sql_sha256,"PG immutable custody SQL differs");
 let(rows,pointer)=pg_custody_rows_any(conn).await?;ensure!(pointer==vec![(1,e.custody_generation_id.clone())]&&rows.iter().any(|r|r.generation_id==e.custody_generation_id&&r.ledger_sha256==e.custody_ledger_sha256&&r.root_device==e.root_device.to_string()&&r.root_inode==e.root_inode.to_string()),"PG custody current association differs");Ok(())
}

// Opaque, descriptor-retained new-target authority. Original transfer evidence
// remains historical data; no EngineCommitEnvelope is fabricated or reused.
pub(crate) struct RestoredPgAdmission {
 root:Root,attestation:PgTargetAttestation,raw_attestation:Vec<u8>,
 records:BTreeMap<String,EngineHeldRecord>,
 application_projections:std::collections::BTreeSet<String>,
 topologies:BTreeMap<String,(std::collections::BTreeSet<String>,BTreeMap<String,(File,PhysicalStamp)>)>,
}
impl RestoredPgAdmission {
 pub(super) fn census_control_record(&self,path:&str,file:&File)->io::Result<bool>{self.physical_fence()?;let result=if let Some(record)=self.records.get(path){physical_stamp(file)?==record.stamp&&physical_stamp(&record.held)?==record.stamp&&planned_hash(file)?==record.bytes}else{false};self.physical_fence()?;Ok(result)}

 pub(super) fn census_application_projection(&self,path:&str,file:&File)->io::Result<bool>{self.physical_fence()?;let owned=self.application_projections.contains(path)&&self.census_control_record(path,file)?;self.physical_fence()?;Ok(owned)}
 pub(super) fn census_control_entries(&self)->io::Result<std::collections::BTreeSet<String>>{self.physical_fence()?;let entries=self.records.keys().chain(self.topologies.keys()).filter_map(|p|p.split('/').next().map(str::to_owned)).collect();self.physical_fence()?;Ok(entries)}
 pub(super) fn path(&self)->&Path{&self.root.absolute}
 pub(crate) fn physical_fence(&self)->io::Result<()> {
  engine_root_fence(&self.root)?;
  for(prefix,(files,dirs))in &self.topologies {
   let held=destination_directory(self.root.fd(),prefix)?;let(current_files,mut current_dirs)=engine_walk(&held,prefix)?;
   current_dirs.insert(prefix.clone(),(held.try_clone()?,physical_stamp(&held)?));
   if &current_files!=files||current_dirs.keys().collect::<Vec<_>>()!=dirs.keys().collect::<Vec<_>>(){return Err(fail("Held PGRestore topology changed"));}
   for(p,(f,stamp))in dirs{if physical_stamp(f)?!=*stamp||physical_stamp(&current_dirs[p].0)?!=*stamp{return Err(fail("Held PGRestore directory replaced or changed"));}}
  }
  for(p,r)in &self.records{let held=self.root.leaf(p)?;if physical_stamp(&r.held)?!=r.stamp||physical_stamp(&held)?!=r.stamp||planned_hash(&held)?!=r.bytes||physical_stamp(&held)?!=r.stamp{return Err(fail("Held PGRestore immutable evidence changed"));}}
  engine_root_fence(&self.root)
 }
 pub(super) fn matches_root(&self,root:&Root)->io::Result<()> {
  self.physical_fence()?;if root.absolute!=self.root.absolute||identity(root.fd())?!=identity(self.root.fd())?{return Err(fail("PGRestore authority/root differs"));}engine_root_fence(root)
 }
 pub(crate) async fn verify_connection(&self,conn:&mut sqlx::PgConnection)->anyhow::Result<()> {self.physical_fence()?;pg_attestation_sql(conn,&self.attestation,&self.raw_attestation).await?;self.physical_fence()?;Ok(())}
 pub(crate) async fn verify_any_connection(&self,conn:&mut sqlx::AnyConnection)->anyhow::Result<()> {self.physical_fence()?;pg_attestation_sql_any(conn,&self.attestation,&self.raw_attestation).await?;self.physical_fence()?;Ok(())}
}
async fn admit_postgres_restore(root_path:&Path,conn:&mut sqlx::PgConnection)->anyhow::Result<RestoredPgAdmission> {
 let root=Root::open(root_path)?;engine_root_fence(&root)?;
 let raw=read_ledger(&root.leaf(".pg-restoration-custody/current.json")?)?;let e:PgTargetAttestation=serde_json::from_slice(&raw)?;
 ensure!(identity(root.fd())?==(e.root_device,e.root_inode),"PGRestore root identity differs");
 pg_attestation_sql(conn,&e,&raw).await?;
 let(rows,pointer)=pg_custody_rows(conn).await?;
 let source=HistorySource::new_with_check(&root,None,engine_root_fence)?;
 let closed=history_closure(source,identity(root.fd())?,Some((&rows,&pointer)))?;
 ensure!(closed.current_id==e.custody_generation_id&&digest(&closed.generations[&closed.current_id].raw)==e.custody_ledger_sha256,"PGRestore current history differs");
 let base=format!("{NAMESPACE}/generations/{}",e.custody_generation_id);
 for(name,sha)in [("intent.json",&e.seal_intent_sha256),("completion.json",&e.completion_sha256),("finalization.json",&e.finalization_sha256)]{ensure!(digest(&closed.source.records[&format!("{base}/{name}")].raw)==*sha,"PGRestore finalized role differs");}
 let marker=&closed.source.records[&format!("{base}/retired-marker")];ensure!(identity(&marker.held)?==(e.marker_device,e.marker_inode),"PGRestore original marker identity differs");
 let mut records=BTreeMap::new();
 for(p,r)in &closed.source.records{records.insert(p.clone(),EngineHeldRecord{held:r.held.try_clone()?,stamp:r.stamp.clone(),bytes:crate::storage::backup_inventory::Bytes{size_bytes:r.raw.len()as u64,sha256:digest(&r.raw)}});}
 closed.source.fence()?;drop(closed);
 let archive=format!(".pg-restoration-custody/{}",e.restore_id);
 ensure!(!e.retained_roles.0.is_empty(),"PGRestore retained archive missing");
 for(p,expected)in &e.retained_roles.0 {
  ensure!(valid_relative(p)&&valid_hash(&expected.sha256)&&expected.size_bytes<=i64::MAX as u64&&((p.starts_with(".pg-restoration-custody/")&&p!=".pg-restoration-custody/current.json")||p.starts_with(".engine-transfer-custody/")||p==".starlane-engine-transfer-request"),"PGRestore invalid immutable archive role");
  let held=root.leaf(p)?;let stamp=physical_stamp(&held)?;
  ensure!(stamp.mode&0o777==0o600&&stamp.links==1&&planned_hash(&held)?==*expected&&physical_stamp(&held)?==stamp,"PGRestore archived raw bytes differ");records.insert(p.clone(),EngineHeldRecord{held,stamp,bytes:expected.clone()});
 }
 let current=root.leaf(".pg-restoration-custody/current.json")?;let stamp=physical_stamp(&current)?;ensure!(stamp.mode&0o777==0o600&&stamp.links==1&&read_ledger(&current)?==raw&&physical_stamp(&current)?==stamp,"PGRestore current replaced during admission");records.insert(".pg-restoration-custody/current.json".into(),EngineHeldRecord{held:current,stamp,bytes:crate::storage::backup_inventory::Bytes{size_bytes:raw.len()as u64,sha256:digest(&raw)}});
 if let Some(ack)=pg_target_final_current(&root,&e,&raw)?{records.insert(pg_target_final_path(&e.restore_id),ack);}
 let snapshot_raw=read_ledger(&records[&format!("{archive}/manifest.json")].held)?;ensure!(digest(&snapshot_raw)==e.snapshot_sha256,"PGRestore retained source snapshot differs");let snapshot:PgSnapshotManifest=serde_json::from_slice(&snapshot_raw)?;
 ensure!(snapshot.backup_id==e.backup_id&&snapshot.source_target==e.source_target&&(snapshot.source_root.device,snapshot.source_root.inode)==(e.source_root_device,e.source_root_inode),"PGRestore captured source association differs");
 for(n,sha)in [("application-rows.json",&snapshot.application_rows_sha256),("postgres-catalog.json",&snapshot.catalog_sha256),("postgres-migrations.json",&snapshot.migrations_sha256),("engine-control-rows.json",&snapshot.engine_control_rows_sha256)]{ensure!(records[&format!("{archive}/{n}")].bytes.sha256==*sha,"PGRestore captured metadata differs");}
 for(p,r)in &records{if p.starts_with(".engine-transfer-custody/"){let entry=snapshot.files.0.get(p).context("Historical engine role absent from genuine source inventory")?;match entry{PgSnapshotEntry::File{length,sha256,..}=>ensure!(r.bytes.size_bytes==*length&&r.bytes.sha256==*sha256,"Historical engine role differs from source inventory"),_=>bail!("Historical engine file has directory role")}}}
 // Every predecessor is the actual immutable source authority, not a
 // synthesized engine-transfer envelope. Its captured SQL and files stay raw.
 let application_projections=pg_restore_archived_authority_chain(&records,&e)?;
 let mut topologies=BTreeMap::new();
 for prefix in [NAMESPACE,".engine-transfer-custody",".pg-restoration-custody"]{
  let held=destination_directory(root.fd(),prefix)?;let(files,mut dirs)=engine_walk(&held,prefix)?;dirs.insert(prefix.into(),(held.try_clone()?,physical_stamp(&held)?));
  if prefix!=".restoration-custody"{ensure!(files==records.keys().filter(|p|p.starts_with(&format!("{prefix}/"))).cloned().collect(),"PGRestore unknown/missing retained archive role");}
  topologies.insert(prefix.into(),(files,dirs));
 }
 let proof=RestoredPgAdmission{root,attestation:e,raw_attestation:raw,records,application_projections,topologies};proof.verify_connection(conn).await?;Ok(proof)
}

// Candidate-only deliberate serving acquisition; ordinary namespace startup
// remains refused. Every actual connection and checkout proves the new target.
pub(crate) struct RestoredPgServing {pool:AnyPool,proof:Arc<RestoredPgAdmission>}
impl RestoredPgServing {
 pub(crate) fn pool(&self)->&AnyPool{&self.pool}
 pub(crate) async fn fence(&self)->anyhow::Result<()> {let mut conn=self.pool.acquire().await?;self.proof.verify_any_connection(&mut conn).await}
}
pub(crate) async fn open_restored_pg_serving(root_path:&Path,url:&str)->anyhow::Result<RestoredPgServing>{
 ensure!(url.starts_with("postgres://")||url.starts_with("postgresql://"),"Restored PG serving requires PostgreSQL");sqlx::any::install_default_drivers();
 let root=Root::open(root_path)?;engine_root_fence(&root)?;let e:PgTargetAttestation=serde_json::from_slice(&read_ledger(&root.leaf(".pg-restoration-custody/current.json")?)?)?;
 let schema=quoted(&e.target.schema_name)?;let mut initial=sqlx::PgConnection::connect(url).await?;sqlx::query(&format!("SET search_path TO {schema}")).execute(&mut initial).await?;
 let proof=Arc::new(admit_postgres_restore(root_path,&mut initial).await?);initial.close().await?;
 let after=proof.clone();let before=proof.clone();
 let pool=sqlx::any::AnyPoolOptions::new().max_connections(3)
 .after_connect(move|conn,_|{let proof=after.clone();let schema=schema.clone();Box::pin(async move{sqlx::query(&format!("SET search_path TO {schema}")).execute(&mut *conn).await?;proof.verify_any_connection(conn).await.map_err(|e|sqlx::Error::Protocol(format!("PGRestore connection refused: {e}")))})})
 .before_acquire(move|conn,_|{let proof=before.clone();Box::pin(async move{proof.verify_any_connection(conn).await.map_err(|e|sqlx::Error::Protocol(format!("PGRestore checkout refused: {e}")))?;Ok(true)})}).connect(url).await?;
 let serving=RestoredPgServing{pool,proof};serving.fence().await?;activate_pg_restore_proof(serving.proof.clone())?;serving.fence().await?;Ok(serving)
}


fn pg_restore_archived_authority_chain(records:&BTreeMap<String,EngineHeldRecord>,current:&PgTargetAttestation)->anyhow::Result<std::collections::BTreeSet<String>> {
 let mut projections=std::collections::BTreeSet::new();let mut raw=serde_json::to_vec(current)?;let mut visited=std::collections::BTreeSet::new();
 for _ in 0..128 {
  let e:PgTargetAttestation=serde_json::from_slice(&raw)?;pg_snapshot_uuid(&e.restore_id)?;
  pg_target_final_archive(&e,&raw,records)?;
  ensure!(visited.insert(e.restore_id.clone())&&[1,2].contains(&e.version)&&(e.version==2)==e.parent_attestation.is_some(),"PgRestore predecessor cycle/version differs");
  let archive=format!(".pg-restoration-custody/{}",e.restore_id);
  let manifest_raw=read_ledger(&records.get(&format!("{archive}/manifest.json")).context("PgRestore predecessor source manifest absent")?.held)?;
  ensure!(digest(&manifest_raw)==e.snapshot_sha256,"PgRestore predecessor source manifest changed");let m:PgSnapshotManifest=serde_json::from_slice(&manifest_raw)?;
  ensure!(m.backup_id==e.backup_id&&m.source_target==e.source_target&&(m.source_root.device,m.source_root.inode)==(e.source_root_device,e.source_root_inode),"PgRestore predecessor captured source association differs");
  let projection=format!("{archive}/application-rows.json");let record=records.get(&projection).context("PgRestore authenticated application projection absent")?;ensure!(record.bytes.sha256==m.application_rows_sha256&&planned_hash(&record.held)?==record.bytes,"PgRestore application projection role/hash differs");projections.insert(projection);
  let control_raw=read_ledger(&records.get(&format!("{archive}/engine-control-rows.json")).context("PgRestore predecessor control cells absent")?.held)?;
  ensure!(digest(&control_raw)==m.engine_control_rows_sha256,"PgRestore predecessor control cells changed");let controls:PgSnapshotControls=serde_json::from_slice(&control_raw)?;
  match &m.source_authority {
   None=>{
    ensure!([1,3].contains(&m.format_version)&&e.version==1&&e.parent_attestation.is_none(),"Original transfer authority mixed with restored source");
    let oldraw=read_ledger(&records[".engine-transfer-custody/current.json"].held)?;let old:EngineCommitEnvelope=serde_json::from_slice(&oldraw)?;
    ensure!(digest(&oldraw)==m.engine_envelope_sha256&&old.target==e.source_target&&old.transfer_id==m.engine_generation&&controls.generations==vec![(old.transfer_id.clone(),std::str::from_utf8(&oldraw)?.into(),digest(&oldraw),"committed".into())]&&controls.current==vec![(1,old.transfer_id.clone())],"PgRestore historical original engine SQL association differs");return Ok(projections);
   },
   Some(PgCapturedSourceAuthority::PgRestore{generation_id,attestation_sha256})=>{
    ensure!([2,4].contains(&m.format_version)&&e.version==2,"Restored-source authority/version differs");let link=e.parent_attestation.as_ref().context("Restored-source predecessor link absent")?;
    ensure!(link.relative==format!(".pg-restoration-custody/{generation_id}/source-attestation.json")&&link.sha256==*attestation_sha256,"Restored-source predecessor link differs");
    let previous=read_ledger(&records.get(&link.relative).context("Restored-source original attestation absent")?.held)?;ensure!(digest(&previous)==link.sha256,"Restored-source raw predecessor attestation differs");let old:PgTargetAttestation=serde_json::from_slice(&previous)?;
    ensure!(old.restore_id==*generation_id&&old.target==m.source_target&&(old.root_device,old.root_inode)==(m.source_root.device,m.source_root.inode)&&controls.generations==vec![(generation_id.clone(),std::str::from_utf8(&previous)?.into(),digest(&previous),"committed".into())]&&controls.current==vec![(1,generation_id.clone())]&&controls.imports.len()==1&&controls.imports[0].0==*generation_id&&controls.imports[0].2==old.import_commitment_sha256&&digest(controls.imports[0].1.as_bytes())==old.import_commitment_sha256,"Restored-source historical SQL/target association differs");
    for(p,entry)in &m.files.0{if p.starts_with(".pg-restoration-custody/"){if let PgSnapshotEntry::File{length,sha256,..}=entry{let actual=if p==".pg-restoration-custody/current.json"{records.get(&link.relative)}else{records.get(p)}.context("Restored-source immutable archive member absent")?;ensure!(actual.bytes.size_bytes==*length&&actual.bytes.sha256==*sha256,"Restored-source captured archive bytes differ");}}}
    raw=previous;
   }
  }
 }
 bail!("PgRestore predecessor history exceeds bound")
}

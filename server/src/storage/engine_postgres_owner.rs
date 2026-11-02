fn pg_owner_operation_bytes<T:serde::Serialize>(value:&T)->anyhow::Result<(i64,String)>{let value=serde_json::to_value(value)?;let length=value.get("length").and_then(serde_json::Value::as_i64).context("Owner operation length missing")?;let sha=value.get("sha256").and_then(serde_json::Value::as_str).context("Owner operation digest missing")?;Ok((length,sha.into()))}
// Private owner-derived PG restoration plan. No caller-authored scope constructor.
struct PgOwnerScope {namespace:String,operation:String,evidence:Vec<u8>,bindings:Vec<Binding>}
struct OwnerClassifiedPgRestore {imported:ImportedPgRestore,scopes:Vec<PgOwnerScope>,admission_replay:Option<PgValidatedReplayCapture>,completed:Option<PgValidatedCompletedCapture>,composite:Option<PgValidatedCompositeCapture>,denied:std::collections::BTreeSet<(String,String)>}
fn pg_owner_binding(snapshot:&InspectedPgSnapshot,prior:&HistoryEnvelope,ns:&str,op:&str,evidence:&[u8],path:&str,original:Identity,length:u64,sha:&str)->anyhow::Result<Binding> {
 ensure!(!prior.denied.iter().any(|d|d.namespace==ns&&d.op_id==op),"Previously denied operation cannot become PG restoration authority");
 let Some(PgSnapshotEntry::File{device,inode,length:captured_len,sha256,..})=snapshot.manifest.files.0.get(path)else{bail!("Owner-required PG captured file absent")};
 ensure!(*captured_len==length&&sha256==sha,"Owner-required PG captured bytes differ");
 let captured=(*device,*inode);
 if let Some(scope)=prior.runtime.scopes.iter().find(|s|s.namespace==ns&&s.op_id==op) {
  ensure!(scope.evidence_sha256==digest(evidence),"Prior PG scope evidence differs");
  let binding=scope.bindings.iter().find(|b|b.relative==path).context("Prior PG scope has no required path binding")?;
  ensure!((binding.original_device,binding.original_inode)==original&&(binding.adopted_device,binding.adopted_inode)==captured&&binding.length==length&&binding.sha256==sha,"Prior PG scoped physical authority differs");
 } else {ensure!(original==captured,"New native PG operation original proof identity differs from capture");}
 // IDs remain captured IDs until certified destination materialization.
 Ok(Binding{relative:path.into(),original_device:original.0,original_inode:original.1,adopted_device:captured.0,adopted_inode:captured.1,length,sha256:sha.into()})
}
fn pg_owner_install(snapshot:&InspectedPgSnapshot,prior:&HistoryEnvelope,ns:&str,op:&str,install:crate::storage::journal_fs::InstallInspection,length:i64,sha:&str)->anyhow::Result<PgOwnerScope>{
 ensure!(length>=0,"Negative PG owner length");let proof=install.proof.context("Committed PG operation missing original install proof")?;
 ensure!(!install.required_group.is_empty(),"PG owner has no required installation group");
 let bindings=install.required_group.iter().map(|p|pg_owner_binding(snapshot,prior,ns,op,&proof.raw,p,proof.original,length as u64,sha)).collect::<anyhow::Result<_>>()?;
 Ok(PgOwnerScope{namespace:ns.into(),operation:op.into(),evidence:proof.raw,bindings})
}
async fn classify_postgres_restore(imported:ImportedPgRestore,url:&str)->anyhow::Result<OwnerClassifiedPgRestore> {
 imported.prepared.fence()?;let snapshot=&imported.prepared.snapshot;
 let raw=pg_snapshot_read(&snapshot.held.root,"files/.restoration-custody/current.json",16*1024*1024,&mut BTreeMap::new())?;let prior:HistoryEnvelope=serde_json::from_slice(&raw)?;
 ensure!(prior.version==3,"Only exactly inspected finalized custody supported by this PG owner slice");
 let mut denied=prior.denied.iter().map(|d|(d.namespace.clone(),d.op_id.clone())).collect::<std::collections::BTreeSet<_>>();let mut scopes=vec![];let mut registry_relocations=vec![];
 let mut conn=sqlx::AnyConnection::connect(url).await?;sqlx::query(&format!("SET search_path TO {}",quoted(&imported.target.schema_name)?)).execute(&mut conn).await?;
 ensure!(engine_target_identity_any(&mut conn,&imported.target.schema_name).await?==imported.target,"PG owner inspection target differs");
 let mut tx=conn.begin().await?;sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY").execute(&mut *tx).await?;
 ensure!(engine_typed_hash(&pg_tables_any(&mut tx,&shape()?).await?)?==engine_typed_hash(&imported.expected_target_tables)?,"PG owner inspection SQL differs from imported snapshot");
 let target_profile=pg_profile_any(&mut tx).await?;checked_profile_shape(target_profile,&imported.expected_target_tables)?;
 let completed=pg_validate_completed_capture(snapshot,&mut tx).await?;
 let composite=pg_validate_composite_capture(snapshot,&mut tx).await?;
 let registry=if !target_profile.registry_tables().is_empty() {Some(match &completed{Some(c)=>crate::storage::attachment_versions::inspect_completed_registry_snapshot(&mut tx,&c.registry_absences).await,None=>crate::storage::attachment_versions::inspect_registry_snapshot(&mut tx).await}.map_err(|e|anyhow!(e))?)}else{None};let source=snapshot.held.root.absolute.join("files");
 for(ns,table)in [(".storage-journal","storage_journal_receipts"),(".publication-journal","publication_receipts"),(".canonical-rewrite-journal","canonical_revision_receipts"),(".move-relocation-journal","move_commit_receipts")] {
  let prefix=format!("{ns}/");let mut ops=snapshot.manifest.files.0.iter().filter_map(|(p,v)|if matches!(v,PgSnapshotEntry::Directory{..}){p.strip_prefix(&prefix).filter(|s|!s.contains('/')).map(str::to_owned)}else{None}).collect::<std::collections::BTreeSet<_>>();
  let committed=sqlx::query_scalar::<_,String>(&format!("SELECT op_id FROM {} ORDER BY op_id",quoted(table)?)).fetch_all(&mut *tx).await?.into_iter().collect::<std::collections::BTreeSet<_>>();ops.extend(committed.iter().cloned());
  for op in ops {pg_snapshot_uuid(&op)?;if !committed.contains(&op){denied.insert((ns.into(),op));continue;}
   ensure!(!denied.contains(&(ns.into(),op.clone())),"Committed PG operation conflicts with prior denial");
   let owner=match ns {
    ".storage-journal"=>{let i=crate::storage::journal::inspect_restoration(&source,&mut tx,&op).await.map_err(|e|anyhow!("{e:?}"))?;ensure!(i.class==crate::storage::journal::StorageOperationClass::CommittedComplete,"Pending first save must finish source recovery before PG capture");pg_owner_install(snapshot,&prior,ns,&op,i.install,i.manifest.length,&i.manifest.sha256)?},
    ".publication-journal"=>{let i=crate::storage::publication::inspect_restoration(&source,&mut tx,&op).await.map_err(|e|anyhow!("{e:?}"))?;ensure!(i.class!=crate::storage::publication::RestorationOperationClass::PreparedUncommitted,"Missing committed PG publication");pg_owner_install(snapshot,&prior,ns,&op,i.install,pg_owner_operation_bytes(&i.manifest)?.0,&pg_owner_operation_bytes(&i.manifest)?.1)?},
    ".canonical-rewrite-journal"=>{let i=crate::storage::canonical_rewrite::inspect_restoration(&source,&mut tx,&op).await.map_err(|e|anyhow!("{e:?}"))?;ensure!(i.class!=crate::storage::canonical_rewrite::RestorationOperationClass::PreparedUncommitted,"Missing committed PG rewrite");pg_owner_install(snapshot,&prior,ns,&op,i.install,pg_owner_operation_bytes(&i.manifest)?.0,&pg_owner_operation_bytes(&i.manifest)?.1)?},
    _=>{let i=crate::storage::relocation::restoration_inspection::inspect(&source,&mut tx,&op).await.map_err(|e|anyhow!("{e:?}"))?;ensure!(i.state==crate::storage::relocation::restoration_inspection::CommitState::Committed,"Missing committed PG relocation");registry_relocations.push((i.manifest.clone(),i.manifest_bytes.clone()));let mut bindings=vec![];
     for p in i.paths {if let Some(original)=p.original_identity {let binding=pg_owner_binding(snapshot,&prior,ns,&op,&i.manifest_bytes,&p.relative,original,p.expected_length,&p.expected_sha256);if p.required{bindings.push(binding?);}else if p.observed.as_ref().is_some_and(|o|o.length==p.expected_length&&o.sha256==p.expected_sha256){if let Ok(b)=binding{bindings.push(b);}}}else{ensure!(p.role=="payload"&&p.required&&p.observed.as_ref().is_some_and(|o|o.length==p.expected_length&&o.sha256==p.expected_sha256),"Unproven PG relocation path"); /* Payload copies have no original identity in the immutable relocation manifest. Preserve authenticated bytes without granting a runtime inode binding. */}}
     PgOwnerScope{namespace:ns.into(),operation:op.clone(),evidence:i.manifest_bytes,bindings}},
   };scopes.push(owner);
  }
 }
 let ns=".attachment-upload-journal";let prefix=format!("{ns}/");let receipts:Vec<(String,String)>=sqlx::query_as("SELECT attempt_id,operation_id FROM attachment_upload_receipts ORDER BY attempt_id").fetch_all(&mut *tx).await?;
 let receipts=receipts.into_iter().collect::<BTreeMap<_,_>>();let mut attempts=snapshot.manifest.files.0.iter().filter_map(|(p,v)|if matches!(v,PgSnapshotEntry::Directory{..}){p.strip_prefix(&prefix).filter(|s|!s.contains('/')).map(str::to_owned)}else{None}).collect::<std::collections::BTreeSet<_>>();attempts.extend(receipts.keys().cloned());
 for attempt in attempts {pg_snapshot_uuid(&attempt)?;if completed.as_ref().is_some_and(|c|c.attempts.contains(&attempt)){denied.insert((ns.into(),attempt));continue;}let Some(op)=receipts.get(&attempt)else{denied.insert((ns.into(),attempt));continue;};let owner=crate::storage::attachment_upload::inspect_restoration(&source,&mut tx,op).await.map_err(|e|anyhow!("{e:?}"))?;
  ensure!(owner.receipt.intent.attempt_id==attempt&&owner.class!=crate::storage::attachment_upload::UploadClass::CommittedPending,"Pending upload source recovery required before PG capture");
  let bindings=owner.bindings.iter().map(|b|pg_owner_binding(snapshot,&prior,ns,&attempt,&owner.original_proof,&b.relative,owner.original_identity,b.length,&b.sha256)).collect::<anyhow::Result<Vec<_>>>()?;ensure!(bindings.len()==owner.required_group.len()&&!bindings.is_empty(),"PG upload required owner group differs");scopes.push(PgOwnerScope{namespace:ns.into(),operation:attempt,evidence:owner.original_proof,bindings});
 }
 if let Some(registry)=registry {
  let certified=scopes.iter().map(|s|Scope{namespace:s.namespace.clone(),op_id:s.operation.clone(),evidence_sha256:digest(&s.evidence),bindings:s.bindings.clone()}).collect::<Vec<_>>();
  for version in registry.versions(){if registry.completed_absences().contains(&version.version_id){continue;}registry_certified_current(version,&registry.current_associations()[&version.version_id].association.4,&certified,&registry_relocations)?;}
 }
 // Reservations omitted by fresh classification stay explicitly denied.
 for old in &prior.runtime.scopes {if !scopes.iter().any(|s|s.namespace==old.namespace&&s.operation==old.op_id){denied.insert((old.namespace.clone(),old.op_id.clone()));}}
 for s in &scopes {ensure!(!denied.contains(&(s.namespace.clone(),s.operation.clone())),"PG owner plan grants denied scope");}
 let admission_replay=pg_validate_replay_capture(snapshot,&mut tx).await?;
 ensure!(engine_typed_hash(&pg_tables_any(&mut tx,&shape()?).await?)?==engine_typed_hash(&imported.expected_target_tables)?,"PG owner SQL changed during classification");tx.rollback().await?;conn.close().await?;imported.prepared.fence()?;Ok(OwnerClassifiedPgRestore{imported,scopes,admission_replay,completed,composite,denied})
}

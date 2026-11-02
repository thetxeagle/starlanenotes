// Owner-classified target topology only; no finalization or serving authority.
struct MaterializedPgRestore {owner:OwnerClassifiedPgRestore,displaced:BTreeMap<String,String>}
fn pg_materialize_marked_ancestry(d:&OwnedPgBackup)->anyhow::Result<()> {
 d.parent.check()?;let fresh=Root::open(&d.root.absolute)?;
 ensure!(fresh.chain.len()==d.root.chain.len(),"Marked PG materialization ancestry changed");
 for(a,b)in fresh.chain.iter().zip(&d.root.chain){ensure!(identity(a)?==identity(b)?,"Marked PG materialization ancestry changed");}
 ensure!(identity(&open_at(d.parent.fd(),&d.name,libc::O_RDONLY|libc::O_DIRECTORY)?)?==d.root_identity,"Marked PG materialization root replaced");
 let expected=&d.roles[".starlane-restore-incomplete"];
 ensure!(physical_stamp(&expected.held)?==expected.stamp&&physical_stamp(&d.root.leaf(".starlane-restore-incomplete")?)?==expected.stamp&&planned_hash(&expected.held)?==expected.bytes,"Exact incomplete PG marker changed");
 Ok(())
}
fn pg_materialize_touch_directory(d:&mut OwnedPgBackup,path:&str)->anyhow::Result<()> {
 let held=if path.is_empty(){d.root.fd()}else{&d.directories.get(path).context("Unowned materialization directory")?.0};
 ensure!(identity(&destination_directory(d.root.fd(),path)?)?==identity(held)?,"Materialization ancestry replaced");
 let stamp=physical_stamp(held)?;
 if path.is_empty(){ensure!(identity(held)?==d.root_identity,"Materialization root replaced");d.root_stamp=stamp;}else{d.directories.get_mut(path).unwrap().1=stamp;}
 Ok(())
}
fn pg_materialize_parent(d:&OwnedPgBackup,path:&str)->anyhow::Result<(File,String,String)> {
 ensure!(valid_relative(path),"Invalid classified materialization path");
 let(parent,name)=path.rsplit_once('/').unwrap_or(("",path));
 let held=if parent.is_empty(){d.root.fd()}else{&d.directories.get(parent).context("Unowned materialization parent")?.0};
 ensure!(identity(&destination_directory(d.root.fd(),parent)?)?==identity(held)?,"Materialization parent replaced");
 Ok((held.try_clone()?,name.into(),parent.into()))
}
fn materialize_postgres_restore(mut owner:OwnerClassifiedPgRestore)->anyhow::Result<MaterializedPgRestore>{
 owner.imported.prepared.fence()?;
 let mut groups=BTreeMap::<Identity,std::collections::BTreeSet<String>>::new();
 let mut approved=BTreeMap::<String,Identity>::new();
 let mut group_bytes=BTreeMap::new();
 for scope in &owner.scopes {ensure!(!owner.denied.contains(&(scope.namespace.clone(),scope.operation.clone())),"Denied PG scope cannot materialize");
  let mut originals=BTreeMap::<Identity,Identity>::new();let mut reverse=BTreeMap::<Identity,Identity>::new();
  for binding in &scope.bindings {
   let original=(binding.original_device,binding.original_inode);let captured=(binding.adopted_device,binding.adopted_inode);
   ensure!(!originals.insert(original,captured).is_some_and(|old|old!=captured)&&!reverse.insert(captured,original).is_some_and(|old|old!=original),"Within-operation physical custody group conflict");
   ensure!(![".restoration-custody/",".engine-transfer-custody/",".pg-restoration-custody/"].iter().any(|prefix|binding.relative.starts_with(prefix))&&!["install-proof.json","manifest.json","sealed-receipt.json","seal.json","intent.json"].iter().any(|name|binding.relative.rsplit('/').next()==Some(*name))&&binding.relative!="starlane-notes.db","Protected raw evidence cannot be materialized");
   let Some(PgSnapshotEntry::File{device,inode,length,sha256,..})=owner.imported.prepared.snapshot.manifest.files.0.get(&binding.relative)else{bail!("Certified captured file absent")};
   ensure!((*device,*inode)==captured&&*length==binding.length&&*sha256==binding.sha256,"Certified capture group differs");
   ensure!(!approved.insert(binding.relative.clone(),captured).is_some_and(|old|old!=captured),"Path has incompatible captured groups");
   let role=owner.imported.prepared.destination.roles.get(&binding.relative).context("Certified target absent")?;
   ensure!(role.stamp.links==1&&role.bytes.size_bytes==binding.length&&role.bytes.sha256==binding.sha256,"Certified initial target not independent exact bytes");
   ensure!(!group_bytes.insert(captured,role.bytes.clone()).is_some_and(|old|old!=role.bytes),"Captured physical group has incompatible byte contracts");
   groups.entry(captured).or_default().insert(binding.relative.clone());
  }
 }
 let mut displaced=BTreeMap::new();
 for members in groups.values().filter(|g|g.len()>1) {
  let paths=members.iter().cloned().collect::<Vec<_>>();let anchor=&paths[0];
  for path in &paths[1..] {
   owner.imported.prepared.fence()?;
   let d=&mut owner.imported.prepared.destination;
   let marker_before=d.roles[".starlane-restore-incomplete"].stamp.clone();
   let anchor_fd=d.roles[anchor].held.try_clone()?;let anchor_before=d.roles[anchor].stamp.clone();
   let old=d.roles[path].held.try_clone()?;let old_before=d.roles[path].stamp.clone();
   let (parent,name,parent_path)=pg_materialize_parent(d,path)?;
   let temporary=format!(".pg-certified-{}.pending",uuid::Uuid::new_v4());
   let ct=CString::new(temporary.clone())?;let cn=CString::new(name.clone())?;let ca=CString::new(format!("/proc/self/fd/{}",anchor_fd.as_raw_fd()))?;
   // Exclusive new link through a held source descriptor, never a reopened anchor name.
   if unsafe{libc::linkat(libc::AT_FDCWD,ca.as_ptr(),parent.as_raw_fd(),ct.as_ptr(),libc::AT_SYMLINK_FOLLOW)}!=0{return Err(io::Error::last_os_error().into());}
   parent.sync_all()?;
   let linked=open_at(&parent,&temporary,libc::O_RDONLY|libc::O_NONBLOCK)?;let linked_stamp=physical_stamp(&linked)?;
   ensure!(identity(&linked)?==identity(&anchor_fd)?&&linked_stamp.links==anchor_before.links+1&&linked_stamp.mode==anchor_before.mode&&linked_stamp.mtime==anchor_before.mtime&&linked_stamp.mtime_ns==anchor_before.mtime_ns&&planned_hash(&linked)?==d.roles[anchor].bytes,"Certified new link differs");
   engine_checkpoint("pg-restore-linked-before-exchange")?;
   pg_materialize_marked_ancestry(d)?;
   ensure!(physical_stamp(&d.roles[".starlane-restore-incomplete"].held)?==marker_before&&physical_stamp(&d.root.leaf(".starlane-restore-incomplete")?)?==marker_before,"Materialization marker changed");
   let(fresh_parent,_,_)=pg_materialize_parent(d,path)?;
   ensure!(identity(&fresh_parent)?==identity(&parent)?&&physical_stamp(&anchor_fd)?==linked_stamp&&physical_stamp(&d.root.leaf(path)?)?==old_before&&physical_stamp(&old)?==old_before&&identity(&d.root.leaf(anchor)?)?==identity(&anchor_fd)?,"Certified exchange raced");
   if unsafe{libc::renameat2(parent.as_raw_fd(),ct.as_ptr(),parent.as_raw_fd(),cn.as_ptr(),libc::RENAME_EXCHANGE)}!=0{return Err(io::Error::last_os_error().into());}
   parent.sync_all()?;anchor_fd.sync_all()?;
   let installed=open_at(&parent,&name,libc::O_RDONLY|libc::O_NONBLOCK)?;let retained=open_at(&parent,&temporary,libc::O_RDONLY|libc::O_NONBLOCK)?;
   let installed_stamp=physical_stamp(&installed)?;let retained_stamp=physical_stamp(&retained)?;
   engine_checkpoint("pg-restore-exchanged-before-certification")?;
   pg_materialize_marked_ancestry(d)?;
   ensure!(identity(&pg_materialize_parent(d,path)?.0)?==identity(&parent)?&&identity(&installed)?==identity(&anchor_fd)?&&identity(&retained)?==identity(&old)?&&physical_stamp(&d.root.leaf(path)?)?==installed_stamp&&physical_stamp(&open_at(&parent,&temporary,libc::O_RDONLY|libc::O_NONBLOCK)?)?==retained_stamp&&planned_hash(&installed)?==d.roles[anchor].bytes&&planned_hash(&retained)?==d.roles[path].bytes&&physical_stamp(&d.root.leaf(".starlane-restore-incomplete")?)?==marker_before,"Certified exchanged roles raced");
   // Retain the displaced exact independent copy. No unlink cleanup, no adoption of unknown leaf.
   let retained_path=if parent_path.is_empty(){temporary.clone()}else{format!("{parent_path}/{temporary}")};
   let bytes=d.roles[path].bytes.clone();d.roles.insert(retained_path.clone(),PgCaptureHeld{held:retained,stamp:retained_stamp,bytes});displaced.insert(path.clone(),retained_path);
   for member in &paths {if identity(&d.roles[member].held)?==identity(&anchor_fd)?||member==path {
    let held=d.root.leaf(member)?;ensure!(physical_stamp(&held)?==installed_stamp&&planned_hash(&held)?==d.roles[anchor].bytes,"Certified group member changed");
    d.roles.get_mut(member).unwrap().held=held;d.roles.get_mut(member).unwrap().stamp=installed_stamp.clone();
   }}
   pg_materialize_touch_directory(d,&parent_path)?;
   d.fence()?;
  }
 }
 // Adopted IDs only after exact final closed topology fence; original scoped IDs unchanged.
 owner.imported.prepared.fence()?;
 for scope in &mut owner.scopes {for binding in &mut scope.bindings {let id=identity(&owner.imported.prepared.destination.roles[&binding.relative].held)?;binding.adopted_device=id.0;binding.adopted_inode=id.1;}}
 engine_checkpoint("pg-restore-materialized")?;
 Ok(MaterializedPgRestore{owner,displaced})
}

// Close transient displaced roles into an explicit immutable private archive.
struct ArchivedPgRestore {materialized:MaterializedPgRestore}
fn archive_postgres_displaced(mut materialized:MaterializedPgRestore)->anyhow::Result<ArchivedPgRestore>{
 materialized.owner.imported.prepared.fence()?;
 let d=&mut materialized.owner.imported.prepared.destination;
 let archive=materialized.owner.imported.prepared.archive.clone();
 let held_archive=d.directories[&archive].0.try_clone()?;
 ensure!(identity(&destination_directory(d.root.fd(),&archive)?)?==identity(&held_archive)?,"PG displaced archive replaced");
 let directory=pg_capture_new_dir(&held_archive,"displaced-independent-copies")?;let destination=format!("{archive}/displaced-independent-copies");
 d.directories.insert(destination.clone(),(directory.try_clone()?,physical_stamp(&directory)?));
 pg_materialize_touch_directory(d,&archive)?;d.fence()?;
 engine_checkpoint("pg-restore-displaced-archive-created")?;
 for original in materialized.displaced.keys().cloned().collect::<Vec<_>>() {
  d.fence()?;
  let transient=materialized.displaced[&original].clone();let role=&d.roles[&transient];
  let held=role.held.try_clone()?;let before=role.stamp.clone();let bytes=role.bytes.clone();
  ensure!(before.links==1,"Displaced independent copy acquired an alias");
  let(parent,name,parent_path)=pg_materialize_parent(d,&transient)?;
  let target_name=uuid::Uuid::new_v4().to_string();let target=format!("{destination}/{target_name}");
  engine_checkpoint("pg-restore-displaced-before-archive-move")?;
  pg_materialize_marked_ancestry(d)?;
  ensure!(identity(&pg_materialize_parent(d,&transient)?.0)?==identity(&parent)?&&identity(&destination_directory(d.root.fd(),&destination)?)?==identity(&directory)?&&physical_stamp(&d.root.leaf(&transient)?)?==before&&physical_stamp(&held)?==before&&planned_hash(&held)?==bytes,"Displaced archive move raced");
  let cn=CString::new(name)?;let ct=CString::new(target_name)?;
  if unsafe{libc::renameat2(parent.as_raw_fd(),cn.as_ptr(),directory.as_raw_fd(),ct.as_ptr(),libc::RENAME_NOREPLACE)}!=0{return Err(io::Error::last_os_error().into());}
  parent.sync_all()?;directory.sync_all()?;held.sync_all()?;
  let archived=d.root.leaf(&target)?;let after=physical_stamp(&archived)?;
  ensure!(identity(&archived)?==identity(&held)?&&physical_stamp(&held)?==after&&after.links==1&&after.length==before.length&&after.mode==before.mode&&after.mtime==before.mtime&&after.mtime_ns==before.mtime_ns&&planned_hash(&archived)?==bytes,"Archived displaced physical role differs");
  pg_materialize_marked_ancestry(d)?;
  ensure!(identity(&pg_materialize_parent(d,&transient)?.0)?==identity(&parent)?&&identity(&destination_directory(d.root.fd(),&destination)?)?==identity(&directory)?&&!exists(&parent,std::str::from_utf8(cn.as_bytes())?)?,"Displaced archive ancestry raced");
  d.roles.remove(&transient);d.roles.insert(target.clone(),PgCaptureHeld{held:archived,stamp:after,bytes});
  pg_materialize_touch_directory(d,&parent_path)?;pg_materialize_touch_directory(d,&destination)?;
  materialized.displaced.insert(original,target);d.fence()?;
  engine_checkpoint("pg-restore-displaced-after-archive-move")?;
 }
 ensure!(!d.roles.keys().any(|path|path.rsplit('/').next().is_some_and(|name|name.starts_with(".pg-certified-"))),"Unclosed transient link role");
 materialized.owner.imported.prepared.fence()?;
 Ok(ArchivedPgRestore{materialized})
}

#[cfg(test)]
mod history_fixture {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn selected() -> Option<std::path::PathBuf> { std::env::var_os("STARLANE_HISTORY_FIXTURE_ROOT").map(std::path::PathBuf::from) }
    fn private_write(path:&Path,bytes:&[u8]) {
        use std::io::Write;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f=std::fs::OpenOptions::new().write(true).create_new(true).open(path).unwrap();
        std::fs::set_permissions(path,std::fs::Permissions::from_mode(0o600)).unwrap();f.write_all(bytes).unwrap();f.sync_all().unwrap();
    }
    fn private_tree(path:&Path) {
        std::fs::set_permissions(path,std::fs::Permissions::from_mode(if path.is_dir(){0o700}else{0o600})).unwrap();
        if path.is_dir(){for e in std::fs::read_dir(path).unwrap(){private_tree(&e.unwrap().path());}}
    }
    #[tokio::test]
    async fn authentic_producer_and_v1_prepare() {
        let Some(base)=selected()else{return};let source=base.join("source1");assert_eq!(source,crate::storage::data_dir());
        sqlx::any::install_default_drivers();std::fs::create_dir_all(&source).unwrap();private_tree(&source);
        let pool=AnyPool::connect(&format!("sqlite:{}?mode=rwc",source.join("starlane-notes.db").display())).await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO users(id,email,display_name) VALUES('u','u@test','User')").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO spaces(id,name,slug,created_by) VALUES('s','Space','s','u')").execute(&pool).await.unwrap();
        let mut tx=pool.begin().await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('d','s','Doc','temporary','u','u','initial')").execute(&mut*tx).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('draft','d','s','u','Doc','Body','initial')").execute(&mut*tx).await.unwrap();
        let m=crate::storage::journal::stage_document(&mut tx,"s","d","draft","initial","s","Doc","","Body").await.unwrap();
        sqlx::query("UPDATE documents SET content_path=$1 WHERE id='d'").bind(&m.path).execute(&mut*tx).await.unwrap();tx.commit().await.unwrap();
        assert!(crate::storage::journal::publish_committed(&pool,&m).await);
        let mut conn=pool.acquire().await.unwrap();let i=crate::storage::journal::inspect_restoration(&source,&mut conn,&m.op_id).await.unwrap();drop(conn);
        let proof=i.install.proof.unwrap();let md=std::fs::metadata(&source).unwrap();
        let mut bindings:Vec<Binding>=i.install.required_group.iter().map(|relative|{let f=File::open(source.join(relative)).unwrap();let id=identity(&f).unwrap();Binding{relative:relative.clone(),original_device:proof.original.0,original_inode:proof.original.1,adopted_device:id.0,adopted_inode:id.1,length:m.length as u64,sha256:m.sha256.clone()}}).collect();
        std::fs::write(base.join("binding-requirements.json"),serde_json::to_vec(&bindings).unwrap()).unwrap();
        if let Ok(mode)=std::fs::read_to_string(base.join("scope-mode")) {
            if mode=="empty"{bindings.clear();}else if mode=="removed"{bindings.pop();}else if mode!="native"&&mode!="pending"{panic!("Unknown explicit fixture scope mode");}
        }
        private_tree(&source);private_write(&source.join(DESTINATION_MARKER),b"history-fixture-marked\n");
        let ledger=Ledger{ historical_preservation:None, composite:None,completed_absence:None,admission:None, version:1,generation_id:uuid::Uuid::new_v4().to_string(),backup_id:uuid::Uuid::new_v4().to_string(),manifest_sha256:digest(b"fixture input"),database_sha256:digest(&std::fs::read(source.join("starlane-notes.db")).unwrap()),root_device:md.dev(),root_inode:md.ino(),scopes:vec![Scope{namespace:".storage-journal".into(),op_id:m.op_id,evidence_sha256:digest(&proof.raw),bindings}]};
        prepare_in(&source,&pool,&ledger).await.unwrap();std::fs::remove_file(source.join(DESTINATION_MARKER)).unwrap();activate_in(&source,&pool).await.unwrap();
        if std::fs::read_to_string(base.join("scope-mode")).ok().as_deref() .is_some_and(|m|m=="native"||m=="pending") {
            let mut tx=pool.begin().await.unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at) VALUES('d2','s','Doc2','temporary','u','u','initial-2')").execute(&mut*tx).await.unwrap();
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('draft2','d2','s','u','Doc2','Other body','initial-2')").execute(&mut*tx).await.unwrap();
            let native=crate::storage::journal::stage_document(&mut tx,"s","d2","draft2","initial-2","s","Doc2","","Other body").await.unwrap();
            sqlx::query("UPDATE documents SET content_path=$1 WHERE id='d2'").bind(&native.path).execute(&mut*tx).await.unwrap();tx.commit().await.unwrap();
            if std::fs::read_to_string(base.join("scope-mode")).ok().as_deref()!=Some("pending") {assert!(crate::storage::journal::publish_committed(&pool,&native).await);}
        }
        // Genuine post-restoration SQL mutation must not equal original input DB hash.
        sqlx::query("UPDATE users SET display_name='Changed after restore' WHERE id='u'").execute(&pool).await.unwrap();pool.close().await;
    }
    #[tokio::test]
    async fn fixture_envelope_advance() {
        let Some(base)=selected()else{return};let step:usize=std::fs::read_to_string(base.join("step")).unwrap().trim().parse().unwrap();
        let bundle=crate::storage::backup_inventory::verify(&base.join(format!("bundle{}",step-1))).await.unwrap();
        let mut conn=bundle.db.acquire().await.unwrap();let history=inspect_history(&bundle,&mut conn).await.unwrap().unwrap();drop(conn);
        let old_id=history.current_id.clone();let old_hash=digest(history.raw_ledger());
        let plan=crate::storage::restoration_plan::plan(&bundle).await.unwrap();
        let deny_pending=base.join("deny-pending").exists();
        if !deny_pending{assert!(plan.unresolved().is_empty());}
        let retained=plan.validated_history().unwrap();assert_eq!(retained.current_id(),history.current_id());assert_eq!(retained.rows(),history.rows());
        assert_eq!(retained.raw_generations().collect::<Vec<_>>(),history.raw_generations().collect::<Vec<_>>());
        let destination=reserve_and_copy(&bundle,&base.join(format!("source{step}"))).unwrap();
        let planned=materialize_groups(destination,&bundle,plan).unwrap();planned.fence().unwrap();
        let target=planned.path();let md=std::fs::metadata(target).unwrap();
        let scopes=planned.plan().scopes().iter().filter(|s|!deny_pending||s.class!=crate::storage::restoration_plan::OperationClass::CommittedPending).map(|s|Scope{namespace:s.namespace.clone(),op_id:s.operation_id.clone(),evidence_sha256:s.evidence_sha256.clone().unwrap(),bindings:s.bindings.iter().map(|b|{let id=planned.observed(&b.relative).unwrap();Binding{relative:b.relative.clone(),original_device:b.original_device,original_inode:b.original_inode,adopted_device:id.0,adopted_inode:id.1,length:b.length,sha256:b.sha256.clone()}}).collect()}).collect();
        let runtime=Ledger{ historical_preservation:None, composite:None,completed_absence:None,admission:None, version:1,generation_id:uuid::Uuid::new_v4().to_string(),backup_id:bundle.manifest.backup_id.clone(),manifest_sha256:digest(&bundle.raw_manifest),database_sha256:bundle.manifest.files.0["starlane-notes.db"].sha256.clone(),root_device:md.dev(),root_inode:md.ino(),scopes};
        let parent=Some(HistoryLink{generation_id:old_id.clone(),ledger_sha256:old_hash});
        let archives=history.rows.iter().map(|r|HistoryLink{generation_id:r.generation_id.clone(),ledger_sha256:r.ledger_sha256.clone()}).collect::<Vec<_>>();
        let proofs=runtime.scopes.iter().map(|s|{let manifest=format!("{}/{}/manifest.json",s.namespace,s.op_id);let evidence=format!("{}/{}/install-proof.json",s.namespace,s.op_id);HistoryProof{namespace:s.namespace.clone(),op_id:s.op_id.clone(),manifest:HistoryArtifact{relative:manifest.clone(),sha256:bundle.manifest.files.0[&manifest].sha256.clone()},evidence:HistoryArtifact{relative:evidence,sha256:s.evidence_sha256.clone()},mint_intent:None}}).collect::<Vec<_>>();
        let mut denied=history.generations[&history.current_id].envelope.as_ref().map(|e|e.denied.clone()).unwrap_or_default();
        if deny_pending {
            let pending=planned.plan().scopes().iter().find(|s|s.class==crate::storage::restoration_plan::OperationClass::CommittedPending).unwrap();
            assert!(pending.bindings.is_empty());assert_eq!(pending.evidence.as_deref(),pending.raw_manifest.as_deref());
            denied.push(DeniedReservation{namespace:pending.namespace.clone(),op_id:pending.operation_id.clone()});
        }
        let runtime_sha=digest(&serde_json::to_vec(&runtime).unwrap());
        let intent=HistoryIntent{version:1,generation_id:runtime.generation_id.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),source_root:bundle.manifest.source_custody.root.clone(),target_root:crate::storage::backup_inventory::Identity{device:runtime.root_device,inode:runtime.root_inode},retired_marker:HistoryArtifact{relative:format!("{NAMESPACE}/generations/{}/retired-marker",runtime.generation_id),sha256:digest(&std::fs::read(target.join(DESTINATION_MARKER)).unwrap())},runtime_sha256:runtime_sha.clone(),parent:parent.clone(),archives:archives.clone(),denied:denied.clone(),proofs:proofs.clone()};
        let raw_intent=serde_json::to_vec(&intent).unwrap();let intent_sha=digest(&raw_intent);let created="2026-10-10 00:00:00";
        let completion=HistoryCompletion{version:1,generation_id:runtime.generation_id.clone(),backup_id:runtime.backup_id.clone(),manifest_sha256:runtime.manifest_sha256.clone(),database_sha256:runtime.database_sha256.clone(),intent_sha256:intent_sha.clone(),runtime_sha256:runtime_sha,root_device:runtime.root_device.to_string(),root_inode:runtime.root_inode.to_string(),state:"committed".into(),created_at:created.into()};
        let raw_completion=serde_json::to_vec(&completion).unwrap();
        let envelope=HistoryEnvelope{version:2,finalization:None,runtime:runtime.clone(),intent_sha256:intent_sha,completion_sha256:digest(&raw_completion),parent,archives,denied,proofs};let raw=serde_json::to_vec(&envelope).unwrap();
        // Test-only schema fixture construction, NOT the unimplemented private publisher.
        let archive=target.join(format!("{NAMESPACE}/archive/{old_id}/ledger.json"));std::fs::create_dir_all(archive.parent().unwrap()).unwrap();std::fs::rename(target.join(format!("{NAMESPACE}/current.json")),archive).unwrap();
        let dir=target.join(format!("{NAMESPACE}/generations/{}",runtime.generation_id));private_write(&dir.join("intent.json"),&raw_intent);private_write(&dir.join("completion.json"),&raw_completion);private_write(&target.join(format!("{NAMESPACE}/current.json")),&raw);private_tree(target);
        let pool=AnyPool::connect(&format!("sqlite:{}?mode=rw",target.join("starlane-notes.db").display())).await.unwrap();
        sqlx::query("INSERT INTO storage_restore_generations(generation_id,backup_id,manifest_sha256,database_sha256,ledger_sha256,root_device,root_inode,state,created_at) VALUES(?,?,?,?,?,?,?,'committed',?)").bind(&runtime.generation_id).bind(&runtime.backup_id).bind(&runtime.manifest_sha256).bind(&runtime.database_sha256).bind(digest(&raw)).bind(runtime.root_device.to_string()).bind(runtime.root_inode.to_string()).bind(created).execute(&pool).await.unwrap();
        sqlx::query("UPDATE storage_restore_current SET generation_id=? WHERE singleton=1").bind(&runtime.generation_id).execute(&pool).await.unwrap();pool.close().await;std::fs::rename(target.join(DESTINATION_MARKER),dir.join("retired-marker")).unwrap();
    }
    #[tokio::test]
    async fn inspect_captured_history() {
        let Some(base)=selected()else{return};let bundle=crate::storage::backup_inventory::verify(&base.join("inspect-bundle")).await.unwrap();
        let case=std::fs::read_to_string(base.join("case")).unwrap();let mut conn=bundle.db.acquire().await.unwrap();let result=inspect_history(&bundle,&mut conn).await;
        if case.starts_with("reject") {
            let error=result.err().unwrap_or_else(||panic!("history accepted {case}"));
            if case.contains("prior binding expansion") {assert_eq!(error.to_string(),"History predecessor binding missing");}
            return;
        }
        let h=result.unwrap().unwrap();let expected:usize=case.trim().parse().unwrap();assert_eq!(h.generation_count(),expected);
        for s in &h.ledger().scopes {let proof=std::fs::read(bundle.data_path().join(format!("{}/{}/install-proof.json",s.namespace,s.op_id))).unwrap();for b in &s.bindings{h.resolve_original(&s.namespace,&s.op_id,&proof,&b.relative,(b.original_device,b.original_inode),b.length,&b.sha256).unwrap();}}
        if let Some(e)=&h.generations[&h.current_id].envelope {
            for d in &e.denied {
                assert!(h.resolve_original(&d.namespace,&d.op_id,b"not authority","missing",(0,0),0,&digest(b"not authority")).is_err());
            }
        }
        assert_ne!(h.ledger().database_sha256,bundle.manifest.files.0["starlane-notes.db"].sha256);
    }
    #[tokio::test]
    async fn restart_admission() {
        let Some(base)=selected()else{return};
        let source=PathBuf::from(std::fs::read_to_string(base.join("restart-source")).unwrap());
        let mode=std::fs::read_to_string(base.join("restart-mode")).unwrap();
        let bootstrap=bootstrap_gate(&source);
        if mode=="reject-bootstrap"{assert!(bootstrap.is_err());return;}
        bootstrap.unwrap();
        sqlx::any::install_default_drivers();
        let pool=AnyPool::connect(&format!("sqlite:{}?mode=rw",source.join("starlane-notes.db").display())).await.unwrap();
        let activation=activate_in(&source,&pool).await;
        if mode=="reject-activation"{assert!(activation.is_err());pool.close().await;return;}
        activation.unwrap();fence(&source).unwrap();
        let root=Root::open(&source).unwrap();let raw=std::fs::read(source.join(format!("{NAMESPACE}/current.json"))).unwrap();let gen=history_parse(&raw).unwrap();
        for scope in &gen.ledger.scopes {
            assert!(!fresh_install_allowed(&source,&scope.namespace,&scope.op_id).unwrap());
            let proof=std::fs::read(source.join(format!("{}/{}/{}",scope.namespace,scope.op_id,if scope.namespace==".move-relocation-journal"{"manifest.json"}else{"install-proof.json"}))).unwrap();
            for binding in &scope.bindings {
                let held=root.leaf(&binding.relative).unwrap();
                assert!(owns(&source,&scope.namespace,&scope.op_id,&proof,&binding.relative,(binding.original_device,binding.original_inode),&held).unwrap());
            }
        }
        if let Some(e)=gen.envelope {
            let held=root.leaf("starlane-notes.db").unwrap();let actual=identity(&held).unwrap();
            for d in &e.denied {
                assert!(!fresh_install_allowed(&source,&d.namespace,&d.op_id).unwrap());
                assert!(!owns(&source,&d.namespace,&d.op_id,b"native numeric IDs","starlane-notes.db",actual,&held).unwrap());
            }
        }
        if mode=="fence-control-write-restore" {
            use std::io::Write;
            let path=source.join(format!("{NAMESPACE}/current.json"));let mut f=std::fs::OpenOptions::new().write(true).truncate(true).open(&path).unwrap();f.write_all(b"changed").unwrap();f.sync_all().unwrap();
            let mut f=std::fs::OpenOptions::new().write(true).truncate(true).open(&path).unwrap();f.write_all(&raw).unwrap();f.sync_all().unwrap();assert!(fence(&source).is_err());
        } else if mode=="native-db-change" {
            sqlx::query("UPDATE users SET display_name='Native DB edit after v2 activation'").execute(&pool).await.unwrap();fence(&source).unwrap();
        } else {assert_eq!(mode,"accept");}
        pool.close().await;
    }
    #[tokio::test]
    async fn genuine_mint_marker_loss() {
        let Some(base)=selected()else{return};
        let bundle=crate::storage::backup_inventory::verify(&base.join("mint-input")).await.unwrap();
        let plan=crate::storage::restoration_plan::plan(&bundle).await.unwrap();
        let op=plan.scopes().iter().find(|s|s.class==crate::storage::restoration_plan::OperationClass::CommittedPending).unwrap().operation_id.clone();
        let copied=reserve_and_copy(&bundle,&base.join("mint-target")).unwrap();let planned=materialize_groups(copied,&bundle,plan).unwrap();
        let minted=mint_pending_preproof(&bundle,planned,&op).await.unwrap();minted.fence().unwrap();
        std::fs::remove_file(minted.path().join(DESTINATION_MARKER)).unwrap();
        assert!(bootstrap_gate(minted.path()).is_err());
        let pool=AnyPool::connect(&format!("sqlite:{}?mode=rw",minted.path().join("starlane-notes.db").display())).await.unwrap();
        assert!(activate_in(minted.path(),&pool).await.is_err());assert!(fence(minted.path()).is_err());pool.close().await;
    }

    #[tokio::test]
    async fn genuine_denied_pending_mint_refuses_without_mutation() {
        let Some(base)=selected()else{return};
        let bundle=crate::storage::backup_inventory::verify(&base.join("denied-input")).await.unwrap();
        let plan=crate::storage::restoration_plan::plan(&bundle).await.unwrap();
        let op=plan.scopes().iter().find(|s|s.class==crate::storage::restoration_plan::OperationClass::CommittedPending).unwrap().operation_id.clone();
        let mut conn=bundle.db.acquire().await.unwrap();
        let history=inspect_history(&bundle,&mut conn).await.unwrap().unwrap();assert!(history.current_denied().iter().any(|d|d.namespace==".storage-journal"&&d.op_id==op));
        let inspection=crate::storage::journal::inspect_restoration(&bundle.data_path(),&mut conn,&op).await.unwrap();
        assert_eq!(inspection.class,crate::storage::journal::StorageOperationClass::CommittedPending);assert!(inspection.install.proof.is_none());assert!(!inspection.install.installed_present);drop(conn);
        let copied=reserve_and_copy(&bundle,&base.join("denied-target")).unwrap();
        let planned=materialize_groups(copied,&bundle,plan).unwrap();planned.fence().unwrap();
        let target=planned.path().to_path_buf();let held_root=planned.copied.root.fd().try_clone().unwrap();let root_stamp=planned.root_stamp.clone();
        let before=planned.leaves.iter().map(|(p,l)|(p.clone(),(l.stamp.clone(),l.bytes.clone()))).collect::<std::collections::BTreeMap<_,_>>();
        let directories=planned.directories.iter().map(|(p,(_,s))|(p.clone(),s.clone())).collect::<std::collections::BTreeMap<_,_>>();
        let marker=planned.marker_stamp.clone();let markerbytes=planned.copied.marker_bytes.clone();
        let result=mint_pending_preproof(&bundle,planned,&op).await;
        let error=result.err().unwrap_or_else(||panic!("Denied pending operation minted"));assert_eq!(error.to_string(),"Prior denied operation cannot mint pending preproof");
        assert_eq!(physical_stamp(&held_root).unwrap(),root_stamp);
        let fresh_marker=open_at(&held_root,DESTINATION_MARKER,libc::O_RDONLY).unwrap();assert_eq!(physical_stamp(&fresh_marker).unwrap(),marker);assert_eq!(std::fs::read(target.join(DESTINATION_MARKER)).unwrap(),markerbytes);
        fn walk(path:&Path,relative:&str,files:&mut std::collections::BTreeSet<String>,dirs:&mut std::collections::BTreeSet<String>) {
            for e in std::fs::read_dir(path).unwrap(){let e=e.unwrap();let name=e.file_name().into_string().unwrap();if relative.is_empty()&&name==DESTINATION_MARKER{continue;}let p=if relative.is_empty(){name}else{format!("{relative}/{name}")};if e.file_type().unwrap().is_dir(){dirs.insert(p.clone());walk(&e.path(),&p,files,dirs);}else{assert!(e.file_type().unwrap().is_file());files.insert(p);}}
        }
        let mut files=Default::default();let mut dirs=Default::default();walk(&target,"",&mut files,&mut dirs);assert_eq!(files,before.keys().cloned().collect());assert_eq!(dirs,directories.keys().cloned().collect());
        for(path,(stamp,bytes))in before {let file=File::open(target.join(path)).unwrap();assert_eq!(physical_stamp(&file).unwrap(),stamp);assert_eq!(planned_hash(&file).unwrap(),bytes);}
        for(path,stamp)in directories {assert_eq!(physical_stamp(&destination_directory(&held_root,&path).unwrap()).unwrap(),stamp);}
        assert!(bootstrap_gate(&target).is_err());bundle.fence().unwrap();
    }

}

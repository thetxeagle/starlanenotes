
    #[tokio::test]
    async fn publication_real_completed_pending_preserves_proofs_and_sql() {
        if isolated_fixture("publication_real_completed_pending_preserves_proofs_and_sql") {return;}
        let f=Fixture::new(true,false).await;
        let scalar=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",f.source.join("starlane-notes.db").display())).await.unwrap();
        sqlx::query("CREATE TABLE publisher_cells(v)").execute(&scalar).await.unwrap();
        for _ in 0..2 {sqlx::query("INSERT INTO publisher_cells VALUES(?)").bind(None::<String>).execute(&scalar).await.unwrap();sqlx::query("INSERT INTO publisher_cells VALUES(?)").bind("").execute(&scalar).await.unwrap();}
        sqlx::query("INSERT INTO publisher_cells VALUES(?)").bind("a\0left").execute(&scalar).await.unwrap();
        sqlx::query("INSERT INTO publisher_cells VALUES(?)").bind(vec![0u8,255,0,1]).execute(&scalar).await.unwrap();
        for n in [i64::MIN,i64::MAX] {sqlx::query("INSERT INTO publisher_cells VALUES(?)").bind(n).execute(&scalar).await.unwrap();}
        for f64value in [f64::from_bits(1),1.2345678901234567,-1.2345678901234567] {sqlx::query("INSERT INTO publisher_cells VALUES(?)").bind(f64value).execute(&scalar).await.unwrap();}
        let original_real:Vec<f64>=sqlx::query_scalar("SELECT CASE WHEN typeof(v)='real' THEN v ELSE 0.0 END FROM publisher_cells ORDER BY rowid").fetch_all(&scalar).await.unwrap();
        let original_cells:Vec<(String,String)>=sqlx::query_as("SELECT typeof(v),CASE WHEN v IS NULL THEN 'NULL' ELSE hex(CAST(v AS BLOB)) END FROM publisher_cells ORDER BY rowid").fetch_all(&scalar).await.unwrap();
        scalar.close().await;
        let old_proof=std::fs::read(f.proof()).unwrap();
        let m=add_pending_operation(&f,"publisher-pending").await;
        let (b,d,target)=mint_prepared(&f).await;
        let d=super::super::restoration::mint_pending_preproof(&b,d,&m.op_id).await.unwrap();
        let seal=super::super::restoration::seal_publication(&b,d).await.unwrap();
        let committed=super::super::restoration::commit_publication(seal).await.unwrap();
        let ready=super::super::restoration::retire_publication(committed).unwrap();
        assert_eq!(ready.path(),target);
        assert!(!target.join(".starlane-restore-incomplete").exists());
        assert!(ready.retained_sql_artifacts().join("provenance.json").exists());
        assert_eq!(std::fs::read(target.join(format!(".storage-journal/{}/install-proof.json",f.manifest.op_id))).unwrap(),old_proof);
        let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=ro",target.join("starlane-notes.db").display())).await.unwrap();
        let n:i64=sqlx::query_scalar("SELECT count(*) FROM storage_restore_generations WHERE state='committed'").fetch_one(&db).await.unwrap();assert_eq!(n,1);
        let title:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE document_id='d'").fetch_one(&db).await.unwrap();assert_eq!(title,"Private body");
        let restored_real:Vec<f64>=sqlx::query_scalar("SELECT CASE WHEN typeof(v)='real' THEN v ELSE 0.0 END FROM publisher_cells ORDER BY rowid").fetch_all(&db).await.unwrap();
        let restored_cells:Vec<(String,String)>=sqlx::query_as("SELECT typeof(v),CASE WHEN v IS NULL THEN 'NULL' ELSE hex(CAST(v AS BLOB)) END FROM publisher_cells ORDER BY rowid").fetch_all(&db).await.unwrap();
        assert_eq!(original_cells,restored_cells);assert_eq!(original_real.iter().map(|f|f.to_bits()).collect::<Vec<_>>(),restored_real.iter().map(|f|f.to_bits()).collect::<Vec<_>>());
        db.close().await;
        let probe=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_fresh_startup_recovery","--nocapture","--test-threads=1"])
            .env("STARLANE_PUBLICATION_STARTUP_ROOT",&target).env("DATA_DIR",&target).output().unwrap();
        assert!(probe.status.success(),"{} {}",String::from_utf8_lossy(&probe.stdout),String::from_utf8_lossy(&probe.stderr));
        assert_eq!(std::fs::read(target.join(format!(".storage-journal/{}/install-proof.json",f.manifest.op_id))).unwrap(),old_proof);
        b.fence().unwrap();b.db.close().await;
    }
    #[tokio::test]
    async fn publication_controlled_sql_refuses_nul_tail_and_blob_mutations() {
        if isolated_fixture("publication_controlled_sql_refuses_nul_tail_and_blob_mutations") {return;}
        for kind in ["nul-text","blob"] {
            let f=Fixture::new(true,false).await;
            let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",f.source.join("starlane-notes.db").display())).await.unwrap();
            sqlx::query("UPDATE documents SET title=? WHERE id='d'").bind("a\0left").execute(&db).await.unwrap();db.close().await;
            let (b,d,target)=mint_prepared(&f).await;
            let sealed=super::super::restoration::seal_publication(&b,d).await.unwrap();
            std::env::set_var("STARLANE_PUBLICATION_SQL_FAULT",kind);
            let err=super::super::restoration::commit_publication(sealed).await.err().expect("Noncustody exact bytes mutation must refuse");
            std::env::remove_var("STARLANE_PUBLICATION_SQL_FAULT");
            assert!(err.to_string().contains("protected SQL"),"{err}");
            assert!(target.join(".starlane-restore-incomplete").exists());
            let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=ro",target.join("starlane-notes.db").display())).await.unwrap();
            let title:String=sqlx::query_scalar("SELECT title FROM documents WHERE id='d'").fetch_one(&db).await.unwrap();assert_eq!(title,"a\0left");
            let n:i64=sqlx::query_scalar("SELECT count(*) FROM storage_restore_generations").fetch_one(&db).await.unwrap();assert_eq!(n,0);
            db.close().await;b.fence().unwrap();b.db.close().await;
        }
    }

    #[tokio::test]
    async fn publication_fresh_startup_recovery() {
        let Ok(root)=std::env::var("STARLANE_PUBLICATION_STARTUP_ROOT") else{return;};
        let root=std::path::PathBuf::from(root);
        sqlx::any::install_default_drivers();
        super::super::restoration::bootstrap_gate(&root).unwrap();
        let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",root.join("starlane-notes.db").display())).await.unwrap();
        super::super::restoration::activate_in(&root,&db).await.unwrap();
        super::super::journal::recover(&db).await.unwrap();
        let pending:i64=sqlx::query_scalar("SELECT count(*) FROM storage_journal_receipts WHERE state='pending'").fetch_one(&db).await.unwrap();assert_eq!(pending,0);
        super::super::restoration::fence(&root).unwrap();db.close().await;
    }

    #[tokio::test]
    async fn publication_foreign_equal_byte_hotjournal_and_symlink_never_mutate_foreign() {
        if isolated_fixture("publication_foreign_equal_byte_hotjournal_and_symlink_never_mutate_foreign") {return;}
        use sha2::{Digest,Sha256};
        use std::os::unix::fs::PermissionsExt;
        for mode in ["equal-bytes","hot-journal","symlink"] {
            let f=Fixture::new(true,false).await;
            let (b,d,target)=mint_prepared(&f).await;
            let sealed=super::super::restoration::seal_publication(&b,d).await.unwrap();
            let foreign_root=f.source.parent().unwrap().join(format!("foreign-{mode}"));
            std::fs::create_dir(&foreign_root).unwrap();std::fs::set_permissions(&foreign_root,std::fs::Permissions::from_mode(0o700)).unwrap();
            let foreign=foreign_root.join("starlane-notes.db");std::fs::copy(target.join("starlane-notes.db"),&foreign).unwrap();
            if mode=="hot-journal" {
                let code="import sqlite3,sys,os\nc=sqlite3.connect(sys.argv[1]);c.execute('PRAGMA journal_mode=DELETE');c.execute('PRAGMA synchronous=FULL');c.execute('PRAGMA cache_size=1');c.execute('BEGIN IMMEDIATE');c.execute(\"UPDATE document_drafts SET content=? WHERE id='draft'\",('Foreign private uncommitted '*8000,));os._exit(0)";
                let done=std::process::Command::new("python3").args(["-c",code,foreign.to_str().unwrap()]).status().unwrap();assert!(done.success());
                let journal=std::fs::read(foreign_root.join("starlane-notes.db-journal")).unwrap();assert!(journal.len()>512);assert_ne!(&journal[..8],&[0u8;8]);
            }
            let snapshot=||std::fs::read_dir(&foreign_root).unwrap().map(|e|{let p=e.unwrap().path();(p.file_name().unwrap().to_string_lossy().to_string(),hex::encode(Sha256::digest(std::fs::read(&p).unwrap())))}).collect::<std::collections::BTreeMap<_,_>>();
            let before=snapshot();
            let foreign_sql=||{let out=std::process::Command::new("python3").args(["-c","import sqlite3,sys,json; c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro&immutable=1',uri=True); print(json.dumps(c.execute('SELECT id,hex(content) FROM document_drafts ORDER BY id').fetchall()))",foreign.to_str().unwrap()]).output().unwrap();assert!(out.status.success());out.stdout};
            let before_sql=foreign_sql();
            std::env::set_var("STARLANE_PUBLICATION_FOREIGN_DB",&foreign);
            if mode=="symlink"{std::env::set_var("STARLANE_PUBLICATION_FOREIGN_SYMLINK","1");}
            let failed=super::super::restoration::commit_publication(sealed).await.err().expect("Foreign actual opened FD must refuse");
            std::env::remove_var("STARLANE_PUBLICATION_FOREIGN_DB");std::env::remove_var("STARLANE_PUBLICATION_FOREIGN_SYMLINK");
            assert!(failed.to_string().contains("open private publication DB"),"{failed}");
            assert_eq!(snapshot(),before,"foreign bytes and all original sidecars unchanged for {mode}");assert_eq!(foreign_sql(),before_sql,"foreign logical SQL unchanged");
            assert!(target.join(".starlane-restore-incomplete").exists());
            assert!(target.join("test-foreign-open-entry.retained").exists()||target.join("test-original-db.retained").exists());
            b.fence().unwrap();b.db.close().await;
        }
    }

    #[tokio::test]
    async fn publication_process_kills_preserve_incomplete_refusal_and_completed_startup() {
        if isolated_fixture("publication_process_kills_preserve_incomplete_refusal_and_completed_startup") {return;}
        use std::os::unix::process::ExitStatusExt;
        for phase in ["intent","archive","current","before-sql-commit","after-sql-commit","before-completion","completion","before-marker-retirement","after-marker-retirement","after-finalization"] {
            let record=std::env::temp_dir().join(format!("publisher-kill-{}.json",uuid::Uuid::new_v4()));
            let child=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_kill_driver","--nocapture","--test-threads=1"])
                .env("STARLANE_PUBLICATION_KILL_RECORD",&record).env("STARLANE_PUBLICATION_KILL_AT",phase).output().unwrap();
            assert_eq!(child.status.signal(),Some(9),"phase {phase}: {} {}",String::from_utf8_lossy(&child.stdout),String::from_utf8_lossy(&child.stderr));
            let paths:serde_json::Value=serde_json::from_slice(&std::fs::read(&record).unwrap()).unwrap();
            let target=PathBuf::from(paths["target"].as_str().unwrap());
            let marker=target.join(".starlane-restore-incomplete");
            if phase=="after-finalization" {
                assert!(!marker.exists());
                let probe=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_fresh_startup_recovery","--nocapture","--test-threads=1"])
                    .env("STARLANE_PUBLICATION_STARTUP_ROOT",&target).env("DATA_DIR",&target).output().unwrap();
                assert!(probe.status.success(),"{phase}: {} {}",String::from_utf8_lossy(&probe.stdout),String::from_utf8_lossy(&probe.stderr));
            } else {
                assert!(marker.exists()||phase=="after-marker-retirement","phase {phase} must retain marker until rename");
                let probe=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_fresh_refusal_probe","--nocapture","--test-threads=1"])
                    .env("STARLANE_PUBLICATION_REFUSAL_ROOT",&target).output().unwrap();
                assert!(probe.status.success(),"{phase}: {} {}",String::from_utf8_lossy(&probe.stdout),String::from_utf8_lossy(&probe.stderr));
                if ["after-sql-commit","before-completion"].contains(&phase) {
                    let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=ro&immutable=true",target.join("starlane-notes.db").display())).await.unwrap();
                    let n:i64=sqlx::query_scalar("SELECT count(*) FROM storage_restore_generations WHERE state='committed'").fetch_one(&db).await.unwrap();assert_eq!(n,1);db.close().await;
                    // Retain marker evidence outside DATA rather than unlinking it. Exact
                    // missing completion must still independently reject fresh bootstrap.
                    std::fs::rename(&marker,target.parent().unwrap().join(format!("marker-{phase}.retained"))).unwrap();
                    let before=std::fs::read(target.join("starlane-notes.db")).unwrap();
                    let probe=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_fresh_refusal_probe","--nocapture","--test-threads=1"])
                        .env("STARLANE_PUBLICATION_REFUSAL_ROOT",&target).output().unwrap();
                    assert!(probe.status.success(),"missing completion {phase}: {} {}",String::from_utf8_lossy(&probe.stdout),String::from_utf8_lossy(&probe.stderr));
                    assert_eq!(before,std::fs::read(target.join("starlane-notes.db")).unwrap());
                }
            }
            let b=super::super::backup_inventory::verify(std::path::Path::new(paths["bundle"].as_str().unwrap())).await.unwrap();b.fence().unwrap();b.db.close().await;
        }
    }
    #[tokio::test]
    async fn publication_kill_driver() {
        let Ok(record)=std::env::var("STARLANE_PUBLICATION_KILL_RECORD") else{return;};
        let f=Fixture::new(true,false).await;
        let m=add_pending_operation(&f,"kill-pending").await;
        let (b,d,target)=mint_prepared(&f).await;
        let d=super::super::restoration::mint_pending_preproof(&b,d,&m.op_id).await.unwrap();
        std::fs::write(&record,serde_json::to_vec(&serde_json::json!({"source":f.source,"bundle":f.bundle,"target":target})).unwrap()).unwrap();
        let sealed=super::super::restoration::seal_publication(&b,d).await.unwrap();
        let committed=super::super::restoration::commit_publication(sealed).await.unwrap();
        let first=super::super::restoration::retire_publication(committed).unwrap();
        if std::env::var("STARLANE_PUBLICATION_KILL_AT").ok().as_deref()==Some("archive") {
            let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",first.path().join("starlane-notes.db").display())).await.unwrap();
            std::env::set_var("DATA_DIR",first.path());super::super::restoration::activate_in(first.path(),&db).await.unwrap();super::super::journal::recover(&db).await.unwrap();db.close().await;
            b.fence().unwrap();b.db.close().await;
            let prior=Fixture{source:first.path().to_path_buf(),bundle:f.source.parent().unwrap().join("archive-kill-bundle"),manifest:f.manifest.clone()};
            let m=add_pending_operation(&prior,"archive-kill-pending").await;prior.capture();
            let b=super::super::backup_inventory::verify(&prior.bundle).await.unwrap();let p=plan(&b).await.unwrap();
            let target=f.source.parent().unwrap().join("archive-kill-target");let copied=super::super::restoration::reserve_and_copy(&b,&target).unwrap();
            let d=super::super::restoration::materialize_groups(copied,&b,p).unwrap();let d=super::super::restoration::mint_pending_preproof(&b,d,&m.op_id).await.unwrap();
            std::fs::write(&record,serde_json::to_vec(&serde_json::json!({"source":prior.source,"bundle":prior.bundle,"target":target})).unwrap()).unwrap();
            super::super::restoration::seal_publication(&b,d).await.unwrap();
        }
        panic!("Expected actual publisher process kill checkpoint");
    }
    #[tokio::test]
    async fn publication_fresh_refusal_probe() {
        let Ok(root)=std::env::var("STARLANE_PUBLICATION_REFUSAL_ROOT") else{return;};
        assert!(super::super::restoration::bootstrap_gate(std::path::Path::new(&root)).is_err(),"Incomplete closure must reject before opening application SQL");
    }

    #[tokio::test]
    async fn publication_repeated_real_backup_preserves_archives_and_new_native_scope() {
        if isolated_fixture("publication_repeated_real_backup_preserves_archives_and_new_native_scope") {return;}
        let f=Fixture::new(true,false).await;
        let original_proof=std::fs::read(f.proof()).unwrap();
        let (b,d,target)=mint_prepared(&f).await;
        let sealed=super::super::restoration::seal_publication(&b,d).await.unwrap();
        let committed=super::super::restoration::commit_publication(sealed).await.unwrap();
        let first=super::super::restoration::retire_publication(committed).unwrap();
        b.fence().unwrap();b.db.close().await;
        let mut previous=first.path().to_path_buf();
        let mut previous_rows:Vec<(String,String,String)>=Vec::new();
        let mut raw_generations=std::collections::BTreeMap::<String,Vec<u8>>::new();
        for round in [2,3] {
            std::env::set_var("DATA_DIR",&previous);
            let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",previous.join("starlane-notes.db").display())).await.unwrap();
            super::super::restoration::activate_in(&previous,&db).await.unwrap();
            super::super::journal::recover(&db).await.unwrap();
            let rows:Vec<(String,String,String)>=sqlx::query_as("SELECT generation_id,created_at,ledger_sha256 FROM storage_restore_generations ORDER BY generation_id").fetch_all(&db).await.unwrap();
            for old in &previous_rows {assert!(rows.contains(old),"Prior SQL identity/created_at remains unchanged");}
            previous_rows=rows;
            let id:String=sqlx::query_scalar("SELECT generation_id FROM storage_restore_current WHERE singleton=1").fetch_one(&db).await.unwrap();
            raw_generations.insert(id,std::fs::read(previous.join(".restoration-custody/current.json")).unwrap());
            db.close().await;
            let next=Fixture{source:previous.clone(),bundle:f.source.parent().unwrap().join(format!("bundle-{round}")),manifest:f.manifest.clone()};
            let m=add_pending_operation(&next,&format!("native-after-restore-{round}")).await;
            next.capture();
            let b=super::super::backup_inventory::verify(&next.bundle).await.unwrap();
            let p=plan(&b).await.unwrap();
            let target=f.source.parent().unwrap().join(format!("publication-round-{round}"));
            let copied=super::super::restoration::reserve_and_copy(&b,&target).unwrap();
            let d=super::super::restoration::materialize_groups(copied,&b,p).unwrap();
            let d=super::super::restoration::mint_pending_preproof(&b,d,&m.op_id).await.unwrap();
            let seal=super::super::restoration::seal_publication(&b,d).await.unwrap();
            let committed=super::super::restoration::commit_publication(seal).await.unwrap();
            let ready=super::super::restoration::retire_publication(committed).unwrap();
            for (id,raw) in &raw_generations {assert_eq!(std::fs::read(target.join(format!(".restoration-custody/archive/{id}/ledger.json"))).unwrap(),*raw);}
            assert_eq!(std::fs::read(target.join(format!(".storage-journal/{}/install-proof.json",f.manifest.op_id))).unwrap(),original_proof);
            let probe=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_fresh_startup_recovery","--nocapture","--test-threads=1"])
                .env("STARLANE_PUBLICATION_STARTUP_ROOT",&target).env("DATA_DIR",&target).output().unwrap();
            assert!(probe.status.success(),"round {round}: {} {}",String::from_utf8_lossy(&probe.stdout),String::from_utf8_lossy(&probe.stderr));
            b.fence().unwrap();b.db.close().await;previous=ready.path().to_path_buf();
            // Each source lives in the same retained fixture base until the final
            // round; intermediate Fixture Drop would delete subsequent evidence.
            std::mem::forget(next);
        }
    }

    #[tokio::test]
    async fn publication_parent_and_delete_exchange_retain_foreign_originals() {
        if isolated_fixture("publication_parent_and_delete_exchange_retain_foreign_originals") {return;}
        use sha2::{Digest,Sha256};
        for mode in ["parent","journal-delete"] {
            let f=Fixture::new(true,false).await;
            let (b,d,target)=mint_prepared(&f).await;
            let sealed=super::super::restoration::seal_publication(&b,d).await.unwrap();
            let foreign_root=f.source.parent().unwrap().join(format!("foreign-{mode}"));std::fs::create_dir(&foreign_root).unwrap();
            let foreign=foreign_root.join("starlane-notes.db");std::fs::copy(target.join("starlane-notes.db"),&foreign).unwrap();
            if mode=="journal-delete" {
                let code="import sqlite3,sys,os\nc=sqlite3.connect(sys.argv[1]);c.execute('PRAGMA journal_mode=DELETE');c.execute('PRAGMA synchronous=FULL');c.execute('PRAGMA cache_size=1');c.execute('BEGIN IMMEDIATE');c.execute(\"UPDATE document_drafts SET content=? WHERE id='draft'\",('Foreign uncommitted '*8000,));os._exit(0)";
                assert!(std::process::Command::new("python3").args(["-c",code,foreign.to_str().unwrap()]).status().unwrap().success());
                assert!(foreign_root.join("starlane-notes.db-journal").exists());
            }
            let snapshot=||std::fs::read_dir(&foreign_root).unwrap().map(|e|{let p=e.unwrap().path();(p.file_name().unwrap().to_string_lossy().to_string(),hex::encode(Sha256::digest(std::fs::read(p).unwrap())))}).collect::<std::collections::BTreeMap<_,_>>();
            let before=snapshot();
            if mode=="parent" {std::env::set_var("STARLANE_PUBLICATION_FOREIGN_PARENT",&foreign_root);} else {std::env::set_var("STARLANE_PUBLICATION_FOREIGN_JOURNAL",foreign_root.join("starlane-notes.db-journal"));}
            let failed=super::super::restoration::commit_publication(sealed).await.err().expect("Persistent role exchange must refuse");
            std::env::remove_var("STARLANE_PUBLICATION_FOREIGN_PARENT");std::env::remove_var("STARLANE_PUBLICATION_FOREIGN_JOURNAL");
            assert_eq!(snapshot(),before,"Foreign original DB and sidecars unchanged: {failed}");
            if mode=="parent" {assert!(target.parent().unwrap().join("test-owned-parent.retained/.starlane-restore-incomplete").exists());}
            else {
                assert!(target.join(".starlane-restore-incomplete").exists());assert!(target.join("test-original-journal.retained").exists());
                let foreign_metadata=std::fs::metadata(foreign_root.join("starlane-notes.db-journal")).unwrap();
                let foreign_id=(foreign_metadata.dev(),foreign_metadata.ino());
                let mut found=false;
                use std::os::unix::fs::MetadataExt;
                for e in std::fs::read_dir(target.parent().unwrap()).unwrap() {let p=e.unwrap().path();if !p.file_name().unwrap().to_string_lossy().starts_with(".starlane-restoration-sql-"){continue;}for e in std::fs::read_dir(p).unwrap(){let p=e.unwrap().path();if !p.file_name().unwrap().to_string_lossy().starts_with("journal-"){continue;}let m=std::fs::metadata(&p).unwrap();if (m.dev(),m.ino())==foreign_id {found=true;}}}
                assert!(found,"Wrong moved foreign leaf must be retained, never unlinked/certified");
                let current:serde_json::Value=serde_json::from_slice(&std::fs::read(target.join(".restoration-custody/current.json")).unwrap()).unwrap();
                let gid=current["runtime"]["generation_id"].as_str().unwrap();assert!(!target.join(format!(".restoration-custody/generations/{gid}/completion.json")).exists());
            }
            b.fence().unwrap();b.db.close().await;
        }
    }

    #[tokio::test]
    async fn publication_closed_disk_sql_and_exact_byte_reversion_refuse() {
        if isolated_fixture("publication_closed_disk_sql_and_exact_byte_reversion_refuse") {return;}
        for mode in ["nul-after-close","write-revert-during-read"] {
            let f=Fixture::new(true,false).await;
            let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",f.source.join("starlane-notes.db").display())).await.unwrap();
            sqlx::query("UPDATE documents SET title=? WHERE id='d'").bind("a\0left").execute(&db).await.unwrap();db.close().await;
            let (b,d,target)=mint_prepared(&f).await;
            let seal=super::super::restoration::seal_publication(&b,d).await.unwrap();
            std::env::set_var("STARLANE_PUBLICATION_DISK_FAULT",mode);
            let err=super::super::restoration::commit_publication(seal).await.err().expect("Closed disk tamper cannot certify committed destination");
            std::env::remove_var("STARLANE_PUBLICATION_DISK_FAULT");
            assert!(err.to_string().contains("protected SQL")||err.to_string().contains("stamp/bytes"),"{mode}: {err}");
            assert!(target.join(".starlane-restore-incomplete").exists());
            let current:serde_json::Value=serde_json::from_slice(&std::fs::read(target.join(".restoration-custody/current.json")).unwrap()).unwrap();
            assert!(!target.join(format!(".restoration-custody/generations/{}/completion.json",current["runtime"]["generation_id"].as_str().unwrap())).exists());
            b.fence().unwrap();b.db.close().await;
        }
    }

    // Real marker exchange after the final pre-rename fence must keep v3 inert,
    // even though SQL/completion are durable and the marker name disappears.
    #[tokio::test]
    async fn publication_v3_marker_exchange_refuses_fresh_admission() {
        if isolated_fixture("publication_v3_marker_exchange_refuses_fresh_admission") {return;}
        use std::os::unix::fs::{MetadataExt,PermissionsExt};
        let f=Fixture::new(true,false).await;let (b,d,target)=mint_prepared(&f).await;
        let foreign=f.source.parent().unwrap().join("foreign-equal-byte-marker");std::fs::copy(target.join(".starlane-restore-incomplete"),&foreign).unwrap();std::fs::set_permissions(&foreign,std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(std::fs::metadata(&foreign).unwrap().nlink(),1);let foreign_before=std::fs::read(&foreign).unwrap();
        let sealed=super::super::restoration::seal_publication(&b,d).await.unwrap();let committed=super::super::restoration::commit_publication(sealed).await.unwrap();
        std::env::set_var("STARLANE_PUBLICATION_MARKER_EXCHANGE",&foreign);
        let err=super::super::restoration::retire_publication(committed).err().expect("Foreign physical marker must refuse retirement");std::env::remove_var("STARLANE_PUBLICATION_MARKER_EXCHANGE");assert!(err.to_string().contains("Retired marker changed"),"{err}");
        assert!(!target.join(".starlane-restore-incomplete").exists());assert!(target.parent().unwrap().join("test-original-marker.retained").exists());assert_eq!(std::fs::read(&foreign).unwrap(),foreign_before);
        let probe=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_fresh_refusal_probe","--nocapture","--test-threads=1"])
            .env("STARLANE_PUBLICATION_REFUSAL_ROOT",&target).output().unwrap();
        assert!(probe.status.success(),"V3 finalization must refuse incomplete admission: {} {}",String::from_utf8_lossy(&probe.stdout),String::from_utf8_lossy(&probe.stderr));
        std::fs::write(target.parent().unwrap().join("v3-marker-finalization-observed.json"),serde_json::to_vec_pretty(&serde_json::json!({"retirement_refused":true,"marker_absent":true,"fresh_ordinary_startup_admitted":false,"foreign_original_unchanged":true})).unwrap()).unwrap();
        b.fence().unwrap();b.db.close().await;
        // Keep genuine race files/SQL/logical evidence for independent review.
        std::mem::forget(f);
    }

    #[tokio::test]
    async fn publication_v3_missing_finalization_io_refuses_fresh_startup() {
        if isolated_fixture("publication_v3_missing_finalization_io_refuses_fresh_startup") {return;}
        let f=Fixture::new(true,false).await;let (b,d,target)=mint_prepared(&f).await;
        let seal=super::super::restoration::seal_publication(&b,d).await.unwrap();let committed=super::super::restoration::commit_publication(seal).await.unwrap();
        std::env::set_var("STARLANE_PUBLICATION_FINALIZATION_IO_FAILURE","1");
        let err=super::super::restoration::retire_publication(committed).err().expect("Finalization write error must refuse BootReady");std::env::remove_var("STARLANE_PUBLICATION_FINALIZATION_IO_FAILURE");assert_eq!(err.raw_os_error(),Some(libc::ENOSPC));
        assert!(!target.join(".starlane-restore-incomplete").exists());
        let current:serde_json::Value=serde_json::from_slice(&std::fs::read(target.join(".restoration-custody/current.json")).unwrap()).unwrap();assert_eq!(current["version"],3);
        let completion=target.join(format!(".restoration-custody/generations/{}/completion.json",current["runtime"]["generation_id"].as_str().unwrap()));assert!(completion.exists());
        assert!(!target.join(current["finalization"]["relative"].as_str().unwrap()).exists());
        let before=std::fs::read(target.join("starlane-notes.db")).unwrap();
        let probe=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_fresh_refusal_probe","--nocapture","--test-threads=1"]).env("STARLANE_PUBLICATION_REFUSAL_ROOT",&target).output().unwrap();
        assert!(probe.status.success(),"{} {}",String::from_utf8_lossy(&probe.stdout),String::from_utf8_lossy(&probe.stderr));assert_eq!(before,std::fs::read(target.join("starlane-notes.db")).unwrap());b.fence().unwrap();b.db.close().await;
    }
    #[tokio::test]
    async fn publication_v3_original_marker_identity_survives_transport_but_replacement_refuses() {
        if isolated_fixture("publication_v3_original_marker_identity_survives_transport_but_replacement_refuses") {return;}
        use std::os::unix::fs::MetadataExt;
        let f=Fixture::new(true,false).await;let (b,d,target)=mint_prepared(&f).await;
        let seal=super::super::restoration::seal_publication(&b,d).await.unwrap();let committed=super::super::restoration::commit_publication(seal).await.unwrap();super::super::restoration::retire_publication(committed).unwrap();
        b.fence().unwrap();b.db.close().await;
        let current:serde_json::Value=serde_json::from_slice(&std::fs::read(target.join(".restoration-custody/current.json")).unwrap()).unwrap();let gid=current["runtime"]["generation_id"].as_str().unwrap();
        let marker=target.join(format!(".restoration-custody/generations/{gid}/retired-marker"));let old=std::fs::metadata(&marker).unwrap();
        let transport=Fixture{source:target.clone(),bundle:f.source.parent().unwrap().join("v3-transport-bundle"),manifest:f.manifest.clone()};transport.capture();let held=super::super::backup_inventory::verify(&transport.bundle).await.unwrap();
        assert_ne!(std::fs::metadata(held.data_path().join(marker.strip_prefix(&target).unwrap())).unwrap().ino(),old.ino());plan(&held).await.unwrap();held.fence().unwrap();held.db.close().await;
        let independent=f.source.parent().unwrap().join("independent-marker-copy");std::fs::copy(&marker,&independent).unwrap();std::fs::rename(&marker,f.source.parent().unwrap().join("original-finalized-marker.retained")).unwrap();std::fs::rename(&independent,&marker).unwrap();assert_ne!(std::fs::metadata(&marker).unwrap().ino(),old.ino());
        let probe=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","storage::restoration_plan::tests::publication_fresh_refusal_probe","--nocapture","--test-threads=1"]).env("STARLANE_PUBLICATION_REFUSAL_ROOT",&target).output().unwrap();assert!(probe.status.success(),"{} {}",String::from_utf8_lossy(&probe.stdout),String::from_utf8_lossy(&probe.stderr));std::mem::forget(transport);
    }

    #[tokio::test]
    async fn publication_wal_foreign_sidecars_and_exclusive_open_races_refuse() {
        if isolated_fixture("publication_wal_foreign_sidecars_and_exclusive_open_races_refuse") {return;}
        for mode in ["occupied-wal","occupied-shm","occupied-journal","open-empty","open-bytes","open-symlink","retire-empty","retire-bytes","retire-symlink"] {
            let f=Fixture::new(true,false).await;
            let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=rw",f.source.join("starlane-notes.db").display())).await.unwrap();
            let journal:String=sqlx::query_scalar("PRAGMA journal_mode=WAL").fetch_one(&db).await.unwrap();assert_eq!(journal,"wal");db.close().await;
            let (b,d,target)=mint_prepared(&f).await;assert_eq!(&std::fs::read(target.join("starlane-notes.db")).unwrap()[18..20],&[2,2]);
            let sealed=super::super::restoration::seal_publication(&b,d).await.unwrap();
            let foreign=target.parent().unwrap().join(format!("foreign-{mode}.retained"));let bytes=if mode.ends_with("empty"){Vec::new()}else{b"foreign WAL original bytes must remain exact".to_vec()};
            std::fs::write(&foreign,&bytes).unwrap();use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&foreign,std::fs::Permissions::from_mode(0o600)).unwrap();
            if let Some(role)=mode.strip_prefix("occupied-"){std::fs::hard_link(&foreign,target.join(format!("starlane-notes.db-{role}"))).unwrap();}
            else {let phase=if mode.starts_with("open-"){"open"}else{"retire"};std::env::set_var("STARLANE_PUBLICATION_WAL_FAULT_PHASE",phase);std::env::set_var("STARLANE_PUBLICATION_FOREIGN_WAL",&foreign);if mode.ends_with("symlink"){std::env::set_var("STARLANE_PUBLICATION_FOREIGN_WAL_SYMLINK","1");}}
            assert!(super::super::restoration::commit_publication(sealed).await.is_err(),"mode {mode}");
            for key in ["STARLANE_PUBLICATION_WAL_FAULT_PHASE","STARLANE_PUBLICATION_FOREIGN_WAL","STARLANE_PUBLICATION_FOREIGN_WAL_SYMLINK"]{std::env::remove_var(key);}
            assert_eq!(std::fs::read(&foreign).unwrap(),bytes,"foreign {mode}");assert!(target.join(".starlane-restore-incomplete").is_file());
            if mode.starts_with("retire-"){assert_eq!(std::fs::read(target.join("test-owned-wal.retained")).unwrap(),Vec::<u8>::new());}
            let db=sqlx::any::AnyPoolOptions::new().max_connections(1).connect(&format!("sqlite://{}?mode=ro&immutable=true",target.join("starlane-notes.db").display())).await.unwrap();
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM storage_restore_generations").fetch_one(&db).await.unwrap();assert_eq!(count,0);db.close().await;
            assert!(super::super::restoration::bootstrap_gate(&target).is_err());b.fence().unwrap();b.db.close().await;
        }
    }

    #[tokio::test]
    async fn publication_retired_group_write_attempt_cannot_regain_boot_ready_authority() {
        if isolated_fixture("publication_retired_group_write_attempt_cannot_regain_boot_ready_authority") { return; }
        use std::os::unix::fs::OpenOptionsExt;
        for mode in ["cancelled-open", "alias-return"] {
            let fixture = Fixture::new(true, false).await;
            let (bundle, destination, target) = mint_prepared(&fixture).await;
            let relative = destination.plan().groups().iter()
                .find(|group| group.binding_members.len() > 1).unwrap().binding_members[0].clone();
            let sealed = super::super::restoration::seal_publication(&bundle, destination).await.unwrap();
            let committed = super::super::restoration::commit_publication(sealed).await.unwrap();
            let ready = super::super::restoration::retire_publication(committed).unwrap();
            ready.fence().unwrap();
            let selected = target.join(relative);
            let original = std::fs::read(&selected).unwrap();
            if mode == "cancelled-open" {
                let _cancelled = std::fs::OpenOptions::new().write(true)
                    .custom_flags(libc::O_NONBLOCK).open(&selected);
            } else {
                let alias = target.parent().unwrap().join("retired-external-group-alias");
                std::fs::hard_link(&selected, &alias).unwrap();
                std::fs::remove_file(alias).unwrap();
            }
            assert_eq!(std::fs::read(&selected).unwrap(), original);
            let error = ready.fence().expect_err("Retired group custody must remain invalid despite exact bytes returning");
            assert!(error.to_string().contains("File read custody"), "expected retained kernel custody refusal: {error}");
            assert!(ready.fence().is_err());
            assert!(std::fs::read_dir(&target).unwrap().next().is_some());
            bundle.db.close().await;
        }
    }

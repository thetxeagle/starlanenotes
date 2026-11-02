// Linux native physical proof cases included inside publication::tests.
use super::*;
    #[tokio::test]
    async fn historical_anchor_accepts_retired_leaf_and_refuses_tampering() {
        for mutation in ["retire","move","anchor","proof","payload"] {
            let (s,a,r,i)=state().await;let mut tx=begin(&s).await;
            let m=prepare_in(&r,&mut tx,&a,i,"","PRIVATE NEXT").await.unwrap();
            tx.rollback().await.unwrap();let dir=r.join(NS).join(m.op_id());
            match mutation {
                "retire"=>fs::remove_file(r.join(m.path())).unwrap(),
                "move"=>fs::rename(r.join(m.path()),r.join("retired.md")).unwrap(),
                "anchor"=>{let bytes=fs::read(dir.join("install-anchor")).unwrap();fs::remove_file(dir.join("install-anchor")).unwrap();fs::write(dir.join("install-anchor"),bytes).unwrap();},
                "proof"=>{let mut proof:serde_json::Value=serde_json::from_slice(&fs::read(dir.join("install-proof.json")).unwrap()).unwrap();proof["ino"]=serde_json::json!(0);fs::write(dir.join("install-proof.json"),serde_json::to_vec(&proof).unwrap()).unwrap();},
                "payload"=>fs::write(dir.join("payload"),"CORRUPT").unwrap(),_=>unreachable!()
            }
            let result=journal_fs::verify_historical_anchor(&r,NS,&journal_fs::FileDescriptor::from(&m.file));
            assert_eq!(result.is_ok(),matches!(mutation,"retire"|"move"),"{mutation}: {result:?}");
            fs::remove_dir_all(r).unwrap();
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn historical_restored_missing_proof_refuses_without_repair() {
        use std::os::unix::fs::{MetadataExt,PermissionsExt};
        let (s,a,r,i)=state().await;
        let mut tx=begin(&s).await;
        let m=prepare_in(&r,&mut tx,&a,i.clone(),"","PRIVATE NEXT").await.unwrap();
        tx.rollback().await.unwrap();
        let dir=r.join(NS).join(m.op_id());
        let raw=journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap();
        // Model a classified restore carrying only the immutable stage. Existing
        // evidence is preserved; the classifier owns an empty physical group.
        fs::remove_file(r.join(m.path())).unwrap();
        for entry in fs::read_dir(&dir).unwrap() {
            let entry=entry.unwrap(); let name=entry.file_name();
            if name.to_string_lossy().starts_with("anchor-") || name=="install-anchor" || name=="install-proof.json" {
                fs::remove_file(entry.path()).unwrap();
            }
        }
        fs::set_permissions(&r,fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(r.join(".starlane-restore-incomplete"),"fixture restore").unwrap();
        let md=fs::metadata(&r).unwrap();
        let ledger=crate::storage::restoration::Ledger {
            admission:None,completed_absence:None,composite:None,historical_preservation:None,
            version:1,generation_id:Uuid::new_v4().to_string(),backup_id:Uuid::new_v4().to_string(),
            manifest_sha256:digest(b"backup manifest"),database_sha256:digest(b"backup database"),
            root_device:md.dev(),root_inode:md.ino(),scopes:vec![crate::storage::restoration::Scope {
                namespace:NS.into(),op_id:m.op_id().into(),evidence_sha256:digest(&raw),bindings:vec![],
            }],
        };
        crate::storage::restoration::prepare_in(&r,&s.db,&ledger).await.unwrap();
        fs::remove_file(r.join(".starlane-restore-incomplete")).unwrap();
        crate::storage::restoration::activate_in(&r,&s.db).await.unwrap();
        let mut tx=begin(&s).await;
        assert!(prepare_in(&r,&mut tx,&a,i,"","PRIVATE NEXT").await.is_err());
        tx.rollback().await.unwrap();
        assert!(journal_fs::verify_historical_anchor(&r,NS,&journal_fs::FileDescriptor::from(&m.file)).is_err());
        assert!(!dir.join("install-proof.json").exists());
        assert!(!r.join(m.path()).exists());
        assert_eq!(journal_fs::manifest_bytes(&r,NS,m.op_id()).unwrap(),raw);
        assert_eq!(fs::read_to_string(r.join(m.source_path)).unwrap(),"PUBLIC OLD");
        fs::remove_dir_all(r).unwrap();
    }

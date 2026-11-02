use super::*;
use sqlx::sqlite::SqliteJournalMode;
use std::fs;
use crate::local_database::{migrate_database,write_snapshot_in_tx,read_snapshot_from_pool};
use std::path::{Path,PathBuf};
use std::time::{SystemTime,UNIX_EPOCH};

fn fixture(label: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "starlane-native-download-{label}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    root
}
fn import_request(root: &Path, bytes: &[u8]) -> custody::Request {
    use sha2::{Digest, Sha256};
    let selection = vault::SnapshotRequestBinding::capture_at(
        root.with_extension("settings.json"),
        root.into(),
        "none",
    )
    .unwrap();
    let destination = custody::Destination::capture(&selection).unwrap();
    custody::Request{destination,association:custody::Association{space_id:"local-space".into(),document_id:"prior-page".into()},operation_id:"7d0e0c9f-66b8-4cb0-8cc9-1b1495e18575".into(),origin:json!({"version":1,"serverBase":"https://fixture.invalid","accountId":"native-fixture-account","sessionId":"original-session","cacheScope":"session-native-fixture","storageMode":"server","provider":"none"}),server_version_raw:serde_json::to_string(&json!({"version_id":"original-immutable-version","logical_attachment_id":"original-family","version_number":2,"original_space_id":"original-space","original_document_id":"original-page","uploader_id":"original-uploader","original_name":"same-name.bin","mime_type":"application/octet-stream","byte_length":bytes.len(),"binary_sha256":format!("{:x}",Sha256::digest(bytes)),"original_created_at":"2026-10-10T12:00:00.000Z"})).unwrap()}
}
pub(crate) async fn initialized(root: &Path) {
    fs::create_dir(root.join(".starlane")).unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(root.join(".starlane/starlane-notes.sqlite3"))
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Delete),
        )
        .await
        .unwrap();
    migrate_database(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    write_snapshot_in_tx(&mut tx,"none",&json!({"spaces":[{"id":"local-space","name":"Actual local page space"}],"assets":[],"documents":[{"id":"prior-page","space_id":"local-space","title":"Preserved prior page"}]})).await.unwrap();
    tx.commit().await.unwrap();
    pool.close().await;
}
fn capture(root: &Path, request: custody::Request) -> custody::Captured {
    let selection = vault::SnapshotRequestBinding::capture_at(
        root.with_extension("settings.json"),
        root.into(),
        "none",
    )
    .unwrap();
    let current = request.origin.clone();
    custody::Captured::capture(selection, request, &current).unwrap()
}

pub(crate) fn local()->Value{json!({"version":1,"serverBase":"local-vault","accountId":"local-owner","sessionId":"original-local-session","cacheScope":"local-session","storageMode":"local","provider":"none"})}
async fn pool(root:&Path)->Pool<Sqlite>{SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(root.join(".starlane/starlane-notes.sqlite3"))).await.unwrap()}
pub(crate) async fn archive_owner(root:&Path)->Owner{
    let db=pool(root).await;
    let revision:String=sqlx::query_scalar("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_one(&db).await.unwrap();db.close().await;
    let selection=vault::SnapshotRequestBinding::capture_at(root.with_extension("settings.json"),root.into(),"none").unwrap();
    Owner::capture(selection,"none",&revision,local()).unwrap()
}
pub(crate) async fn installed(root:&Path)->Value{
    initialized(root).await;let bytes=b"exact retired original";let request=import_request(root,bytes);let owner=capture(root,request.clone());
    owner.begin(&request.origin).await.unwrap();owner.append(&request.origin,0,bytes.to_vec()).await.unwrap();owner.finish(&request.origin).await.unwrap()
}
#[tokio::test]
async fn canonical_capture_never_reads_stale_json_mirror(){
    let root=fixture("archive-mirror");initialized(&root).await;
    let mirror=root.join("starlane-vault-none.json");fs::write(&mirror,b"{\"documents\":[],\"assets\":[]}").unwrap();let before=fs::read(&mirror).unwrap();
    let owner=archive_owner(&root).await;
    owner.with_capture(local(),|c|{assert_eq!(c.snapshot["documents"][0]["id"],"prior-page");assert!(!c.canonical_revision.is_empty());assert_eq!(c.snapshot["mirror_pending"],true);Ok(())}).await.unwrap();
    assert_eq!(fs::read(&mirror).unwrap(),before);fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn actual_separate_process_writer_denied_until_capture_fence_finishes(){
    let root=fixture("archive-writer");initialized(&root).await;let owner=archive_owner(&root).await;let path=root.join(".starlane/starlane-notes.sqlite3");
    owner.with_capture(local(),move |_|{
        let result=std::process::Command::new("python3").args(["-c","import sqlite3,sys; c=sqlite3.connect(sys.argv[1],timeout=0.02);\ntry: c.execute(\"UPDATE local_vault_metadata SET value='foreign' WHERE key='canonical_revision'\"); c.commit(); print('CHANGED')\nexcept sqlite3.OperationalError: print('LOCKED')\nfinally: c.close()"] ).arg(path).output().map_err(err)?;
        assert!(result.status.success());assert_eq!(String::from_utf8(result.stdout).unwrap().trim(),"LOCKED");Ok(())
    }).await.unwrap();let db=pool(&root).await;sqlx::query("UPDATE local_vault_metadata SET value='after' WHERE key='canonical_revision'").execute(&db).await.unwrap();db.close().await;fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn stale_revision_and_original_context_refuse(){
    let root=fixture("archive-context");initialized(&root).await;let mut owner=archive_owner(&root).await;owner.revision="stale".into();assert!(owner.with_capture(local(),|_|Ok(())).await.unwrap_err().contains("revision changed"));
    let owner=archive_owner(&root).await;let mut changed=local();changed["sessionId"]=json!("changed");assert!(owner.with_capture(changed,|_|Ok(())).await.unwrap_err().contains("context changed"));fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn malformed_normalized_and_bridge_records_refuse_without_silent_drop(){
    for table in ["local_documents","local_vault_records"]{
        let root=fixture("archive-invalid-row");initialized(&root).await;let db=pool(&root).await;
        if table=="local_documents"{sqlx::query("UPDATE local_documents SET payload='broken'").execute(&db).await.unwrap();}else{sqlx::query("INSERT INTO local_vault_records(collection,id,payload,row_order) VALUES('drafts','invalid','broken',1)").execute(&db).await.unwrap();}
        db.close().await;let owner=archive_owner(&root).await;assert!(owner.with_capture(local(),|_|Ok(())).await.unwrap_err().contains("Malformed canonical archive payload"));fs::remove_dir_all(root).unwrap();
    }
}
#[tokio::test]
async fn missing_database_refuses_without_creating_root_or_fallback(){
    let root=fixture("archive-missing");let missing=root.join("not-created");let selection=vault::SnapshotRequestBinding::capture_at(root.with_extension("settings.json"),missing.clone(),"none").unwrap();assert!(Owner::capture(selection,"none","revision",local()).is_err());assert!(!missing.exists());fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn exact_native_receipt_intent_binary_and_raw_version_are_owned(){
    let root=fixture("archive-native");let ack=installed(&root).await;let original=ack["original_ack"]["asset"].clone();let expected=original.clone();let owner=archive_owner(&root).await;
    owner.with_capture(local(),move |c|{assert_eq!(c.binaries.len(),1);assert_eq!(c.binaries[0].asset,expected);assert_eq!(c.snapshot["assets"][0],expected);assert_eq!(c.raw_receipts.len(),1);let receipt:Value=serde_json::from_str(&c.raw_receipts[0]).unwrap();assert_eq!(receipt["asset"],expected);let mut bytes=Vec::new();c.binaries[0].copy_exact(&mut bytes)?;assert_eq!(bytes,b"exact retired original");Ok(())}).await.unwrap();fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn corrupted_native_bytes_and_sql_asset_binding_refuse(){
    for binary in [true,false]{let root=fixture("archive-corrupt");let ack=installed(&root).await;
        if binary{let path=ack["original_ack"]["asset"]["vault_path"].as_str().unwrap();fs::write(path,b"changed original bytes").unwrap();}
        else{let db=pool(&root).await;let mut asset=ack["original_ack"]["asset"].clone();asset["name"]=json!("newer same name");sqlx::query("UPDATE local_assets SET payload=?1").bind(serde_json::to_string(&asset).unwrap()).execute(&db).await.unwrap();db.close().await;}
        let owner=archive_owner(&root).await;assert!(owner.with_capture(local(),|_|Ok(())).await.is_err());fs::remove_dir_all(root).unwrap();
    }
}
#[tokio::test]
async fn invalid_receipt_refuses_before_consumer_and_remote_assets_never_omit(){
    for bad_receipt in [true,false]{let root=fixture("archive-refusal");initialized(&root).await;let db=pool(&root).await;
        if bad_receipt{sqlx::query("CREATE TABLE native_attachment_download_receipts(operation_id TEXT PRIMARY KEY,request_sha256 TEXT,receipt TEXT)").execute(&db).await.unwrap();sqlx::query("INSERT INTO native_attachment_download_receipts VALUES('op','sha','broken')").execute(&db).await.unwrap();}
        else{let snapshot=json!({"spaces":[],"documents":[],"assets":[{"id":"remote","source":"server","size":3}]});let mut tx=db.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();}
        db.close().await;let owner=archive_owner(&root).await;assert!(owner.with_capture(local(),|_| -> Result<(),String> {panic!("Refused capture reached consumer")}).await.is_err());fs::remove_dir_all(root).unwrap();
    }
}
#[tokio::test]
async fn original_selection_change_at_consumer_fence_refuses(){
    let root=fixture("archive-selection");initialized(&root).await;let owner=archive_owner(&root).await;let settings=root.with_extension("settings.json");let saved=settings.clone();
    assert!(owner.with_capture(local(),move |_|{fs::write(&settings,b"{\"vault_path\":\"/foreign\"}").map_err(err)?;Ok(())}).await.unwrap_err().contains("selection changed"));fs::remove_file(saved).unwrap();fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
#[ignore="actual 100MiB bounded native archive capture gate"]
async fn actual_100mib_legacy_owned_binary_is_copied_in_64k_chunks(){
    let root=fixture("archive-100mib");initialized(&root).await;fs::create_dir(root.join("assets")).unwrap();let path=root.join("assets/large.bin");let mut file=File::create(&path).unwrap();for _ in 0..1600{file.write_all(&[7;65536]).unwrap();}file.sync_all().unwrap();drop(file);
    let db=pool(&root).await;let original=read_snapshot_from_pool(&db,"none").await.unwrap().unwrap();let mut snapshot=original;snapshot["assets"]=json!([{"id":"legacy-owned","source":"tauri","size":104857600,"name":"same-name.bin","vault_path":path,"url":"asset://original","created_at":"original","future":{"retain":true}}]);let mut tx=db.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();db.close().await;
    struct Check{length:usize,max:usize}impl Write for Check{fn write(&mut self,b:&[u8])->std::io::Result<usize>{assert!(b.iter().all(|v|*v==7));self.length+=b.len();self.max=self.max.max(b.len());Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
    let owner=archive_owner(&root).await;owner.with_capture(local(),|c|{assert_eq!(c.binaries.len(),1);assert_eq!(c.binaries[0].asset["future"]["retain"],true);let mut check=Check{length:0,max:0};c.binaries[0].copy_exact(&mut check)?;assert_eq!(check.length,104857600);assert_eq!(check.max,65536);println!("ACTUAL_NATIVE_ARCHIVE_100MIB_ALL_BYTES_AND_64K_WRITES_PASS");Ok(())}).await.unwrap();fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn same_name_newer_binary_keeps_both_exact_original_versions(){
    let root=fixture("archive-two-versions");installed(&root).await;let newer=b"NEWER SAME NAME";
    let mut request=import_request(&root,newer);request.operation_id="7d0e0c9f-66b8-4cb0-8cc9-1b1495e18576".into();
    let mut version:Value=serde_json::from_str(&request.server_version_raw).unwrap();version["version_id"]=json!("newer-version");version["version_number"]=json!(3);version["unknown_original_field"]=json!({"retain":"exact"});request.server_version_raw=serde_json::to_string_pretty(&version).unwrap();let exact=request.server_version_raw.clone();
    let native=capture(&root,request.clone());native.begin(&request.origin).await.unwrap();native.append(&request.origin,0,newer.to_vec()).await.unwrap();native.finish(&request.origin).await.unwrap();
    let owner=archive_owner(&root).await;owner.with_capture(local(),move |c|{assert_eq!(c.binaries.len(),2);let mut bodies=Vec::new();for b in &c.binaries{assert_eq!(b.asset["name"],"same-name.bin");let mut body=Vec::new();b.copy_exact(&mut body)?;bodies.push(body);}
        assert!(bodies.contains(&b"exact retired original".to_vec()));assert!(bodies.contains(&newer.to_vec()));assert!(c.binaries.iter().any(|b|b.asset["server_version_provenance"]["raw_version_record"]==exact));c.fence()?;Ok(())}).await.unwrap();fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn rewritten_semantically_equal_intent_and_install_proof_are_not_original_bytes(){
    let root=fixture("archive-intent-drift");let ack=installed(&root).await;let parent=PathBuf::from(ack["original_ack"]["asset"]["vault_path"].as_str().unwrap()).parent().unwrap().to_path_buf();
    let owner=archive_owner(&root).await;let result=owner.with_capture(local(),move |c|{let path=parent.join("intent.json");let mut raw=fs::read(&path).map_err(err)?;raw.push(b' ');fs::write(&path,&raw).map_err(err)?;let proof_path=parent.join("install.json");let mut proof:Value=serde_json::from_slice(&fs::read(&proof_path).map_err(err)?).map_err(err)?;proof["intent_sha256"]=json!(format!("{:x}",Sha256::digest(&raw)));fs::write(proof_path,serde_json::to_vec(&proof).map_err(err)?).map_err(err)?;c.fence()}).await;
    assert!(result.unwrap_err().contains("private intent bytes changed"));fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn native_historical_receipt_does_not_recreate_removed_assets(){
    let root=fixture("archive-historical");installed(&root).await;let db=pool(&root).await;let mut snapshot=read_snapshot_from_pool(&db,"none").await.unwrap().unwrap();snapshot["assets"]=json!([]);let mut tx=db.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();db.close().await;
    archive_owner(&root).await.with_capture(local(),|c|{assert!(c.binaries.is_empty());assert_eq!(c.raw_receipts.len(),1);assert_eq!(c.snapshot["assets"],json!([]));Ok(())}).await.unwrap();fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
#[ignore="actual 100MiB receipt-owned native canonical archive capture"]
async fn actual_100mib_receipt_owned_binary_manifest_and_stream_match_original(){
    let root=fixture("archive-receipt-100mib");initialized(&root).await;
    let chunk=[7u8;65536];let mut hash=Sha256::new();for _ in 0..1600{hash.update(chunk);}let expected_hash=format!("{:x}",hash.finalize());
    let mut request=import_request(&root,&[]);let mut version:Value=serde_json::from_str(&request.server_version_raw).unwrap();version["byte_length"]=json!(104857600);version["binary_sha256"]=json!(expected_hash);version["retired_at"]=json!("original-retirement-evidence");version["future_evidence"]=json!({"preserve":"original"});request.server_version_raw=serde_json::to_string_pretty(&version).unwrap();let exact_raw=request.server_version_raw.clone();
    let download=capture(&root,request.clone());download.begin(&request.origin).await.unwrap();for index in 0..1600{download.append(&request.origin,index,chunk.to_vec()).await.unwrap();}download.finish(&request.origin).await.unwrap();
    struct Check{length:u64,max:usize}impl Write for Check{fn write(&mut self,b:&[u8])->std::io::Result<usize>{assert!(b.iter().all(|v|*v==7));self.length+=b.len() as u64;self.max=self.max.max(b.len());Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}
    let owner=archive_owner(&root).await;owner.with_capture(local(),move |c|{assert_eq!(c.binaries.len(),1);let binary=&c.binaries[0];assert_eq!(binary.binary_sha256,expected_hash);assert_eq!(binary.asset["server_version_provenance"]["raw_version_record"],exact_raw);assert!(binary.native_intent_sha256.is_some());assert_eq!(c.raw_receipts.len(),1);let mut check=Check{length:0,max:0};binary.copy_exact(&mut check)?;assert_eq!(check.length,104857600);assert_eq!(check.max,65536);c.fence()?;println!("ACTUAL_NATIVE_ARCHIVE_RECEIPT_100MIB_HASH_RAW_VERSION_ALL_BYTES_64K_PASS");Ok(())}).await.unwrap();fs::remove_dir_all(root).unwrap();
}


#[tokio::test]
async fn actual_wal_canonical_capture_preserves_reopen_and_blocks_separate_writer(){
    let root=fixture("archive-wal");initialized(&root).await;let db=pool(&root).await;sqlx::query("PRAGMA journal_mode=WAL").execute(&db).await.unwrap();sqlx::query("UPDATE local_documents SET title='Canonical WAL page'").execute(&db).await.unwrap();db.close().await;
    let owner=archive_owner(&root).await;let path=root.join(".starlane/starlane-notes.sqlite3");owner.with_capture(local(),move |c|{assert_eq!(c.snapshot["documents"][0]["id"],"prior-page");let result=std::process::Command::new("python3").args(["-c","import sqlite3,sys; c=sqlite3.connect(sys.argv[1],timeout=0.02);\ntry: c.execute(\"UPDATE local_vault_metadata SET value='foreign' WHERE key='canonical_revision'\"); c.commit(); print('CHANGED')\nexcept sqlite3.OperationalError: print('LOCKED')\nfinally: c.close()"]).arg(path).output().map_err(err)?;assert!(result.status.success());assert_eq!(String::from_utf8(result.stdout).unwrap().trim(),"LOCKED");c.fence()?;Ok(())}).await.unwrap();
    let db=pool(&root).await;let title:String=sqlx::query_scalar("SELECT title FROM local_documents WHERE id='prior-page'").fetch_one(&db).await.unwrap();assert_eq!(title,"Canonical WAL page");db.close().await;fs::remove_dir_all(root).unwrap();
}
#[tokio::test]
async fn original_physical_root_exchange_refuses_same_content_replacement(){
    let root=fixture("archive-root-exchange");initialized(&root).await;let owner=archive_owner(&root).await;let held=root.with_extension("original-held");fs::rename(&root,&held).unwrap();fs::create_dir(&root).unwrap();fs::create_dir(root.join(".starlane")).unwrap();fs::copy(held.join(".starlane/starlane-notes.sqlite3"),root.join(".starlane/starlane-notes.sqlite3")).unwrap();let before=fs::read(root.join(".starlane/starlane-notes.sqlite3")).unwrap();assert!(owner.with_capture(local(),|_|Ok(())).await.is_err());assert_eq!(fs::read(root.join(".starlane/starlane-notes.sqlite3")).unwrap(),before);fs::remove_dir_all(root).unwrap();fs::remove_dir_all(held).unwrap();
}

#[tokio::test]
async fn incomplete_or_foreign_local_origin_never_mints_capture_authority(){
    let root=fixture("archive-origin-validation");initialized(&root).await;let good=archive_owner(&root).await;
    for context in [json!({"provider":"none","storageMode":"local"}),json!({"version":1,"provider":"none","storageMode":"local","serverBase":"local","accountId":"local","sessionId":"","cacheScope":"local"})]{
        assert!(Owner::capture(good.selection.clone(),"none",&good.revision,context).is_err());
    }
    fs::remove_dir_all(root).unwrap();
}

#[tokio::test]
async fn canonical_whole_archive_zip_exact_snapshot_receipts_and_binaries(){
 use std::io::Read;
 let root=fixture("whole-output");installed(&root).await;
 let db=pool(&root).await;let expected_receipt:String=sqlx::query_scalar("SELECT receipt FROM native_attachment_download_receipts").fetch_one(&db).await.unwrap();db.close().await;
 let output=root.with_extension("archive.zip");let owner=archive_owner(&root).await;
 let destination=output.clone();
 owner.with_capture(local(),move|capture|{
  let binding=capture.output_binding();let selected=super::super::native_archive_output::test_selection(destination.clone(),binding.clone())?;
  super::super::native_archive_output::compose(capture,selected,||Ok(binding.clone()))?;
  let mut zip=zip::ZipArchive::new(fs::File::open(&destination).map_err(err)?).map_err(err)?;
  let mut snapshot=String::new();zip.by_name("vault.json").map_err(err)?.read_to_string(&mut snapshot).map_err(err)?;
  assert_eq!(serde_json::from_str::<Value>(&snapshot).unwrap(),capture.snapshot);
  let mut raw=String::new();zip.by_name("starlane-native/raw-receipts.json").map_err(err)?.read_to_string(&mut raw).map_err(err)?;
  let receipts:Vec<String>=serde_json::from_str(&raw).unwrap();assert_eq!(receipts,capture.raw_receipts);assert_eq!(receipts,vec![expected_receipt]);
  for binary in &capture.binaries{let asset:vault::VaultAsset=serde_json::from_value(binary.asset.clone()).unwrap();let mut bytes=Vec::new();zip.by_name(&vault::archive_asset_path(&asset)).map_err(err)?.read_to_end(&mut bytes).map_err(err)?;assert_eq!(bytes,b"exact retired original");assert_eq!(format!("{:x}",Sha256::digest(&bytes)),binary.binary_sha256);}
  drop(zip);
  let imported=destination.with_extension("fresh-import");fs::create_dir(&imported).map_err(err)?;
  let admission=vault::archive_admission::prepare("none",&destination,&imported.join("assets"))?;
  let plan=vault::archive_admission::plan_joint("none",&admission.handle,&snapshot,&imported.join("assets"))?;
  assert_eq!(plan["imported_provenance"]["raw_receipts_json"],raw);
  assert_eq!(plan["imported_provenance"]["authority"],"portable_provenance_only");
  vault::archive_admission::cancel("none",&admission.handle)?;fs::remove_dir_all(imported).map_err(err)?;
  Ok(())
 }).await.unwrap();fs::remove_dir_all(&root).unwrap();fs::remove_file(output).unwrap();
}

#[tokio::test]
#[ignore="actual complete ZIP100MiB disk publication and bounded byte readback"]
async fn actual_whole_archive_zip_100mib_exact_disk_readback(){
 use std::io::Read;
 let root=fixture("whole-zip-100mib");initialized(&root).await;fs::create_dir(root.join("assets")).unwrap();let path=root.join("assets/large.bin");let mut file=File::create(&path).unwrap();for _ in 0..1600{file.write_all(&[7;65536]).unwrap();}file.sync_all().unwrap();drop(file);
 let db=pool(&root).await;let mut snapshot=read_snapshot_from_pool(&db,"none").await.unwrap().unwrap();snapshot["assets"]=json!([{"id":"legacy-owned","source":"tauri","size":104857600,"name":"same-name.bin","vault_path":path,"url":"asset://original","created_at":"original","mime_type":"application/octet-stream","future":{"retain":true}}]);let mut tx=db.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();db.close().await;
 let output=root.with_extension("archive.zip");let destination=output.clone();let owner=archive_owner(&root).await;
 owner.with_capture(local(),move|c|{let binding=c.output_binding();let selected=super::super::native_archive_output::test_selection(destination.clone(),binding.clone())?;super::super::native_archive_output::compose(c,selected,||Ok(binding.clone()))?;
 let mut zip=zip::ZipArchive::new(File::open(&destination).map_err(err)?).map_err(err)?;let asset:vault::VaultAsset=serde_json::from_value(c.binaries[0].asset.clone()).map_err(err)?;let mut entry=zip.by_name(&vault::archive_asset_path(&asset)).map_err(err)?;assert_eq!(entry.size(),104857600);let mut b=[0u8;65536];let mut count=0u64;let mut hash=Sha256::new();loop{let n=entry.read(&mut b).map_err(err)?;if n==0{break}assert!(b[..n].iter().all(|v|*v==7));count+=n as u64;hash.update(&b[..n]);}assert_eq!(count,104857600);assert_eq!(format!("{:x}",hash.finalize()),c.binaries[0].binary_sha256);println!("ACTUAL_WHOLE_ARCHIVE_ZIP_100MIB_ALL_BYTES_HASH_READBACK_PASS");Ok(())}).await.unwrap();fs::remove_dir_all(root).unwrap();fs::remove_file(output).unwrap();
}

#[tokio::test]
async fn interrupted_whole_archive_review_preserves_exact_sql_receipts(){
 use std::io::Read;
 let root=fixture("archive-review-raw-receipts");installed(&root).await;
 let db=pool(&root).await;let raw:String=sqlx::query_scalar("SELECT receipt FROM native_attachment_download_receipts").fetch_one(&db).await.unwrap();db.close().await;
 let output=root.with_extension("review-archive.zip");let owner=archive_owner(&root).await;let destination=output.clone();
 let saved_binding=std::sync::Arc::new(std::sync::Mutex::new(Value::Null));let copy=saved_binding.clone();
 assert!(owner.with_capture(local(),move|capture|{let binding=capture.output_binding();*copy.lock().unwrap()=binding.clone();let selected=super::super::native_archive_output::test_selection(destination,binding.clone())?;super::super::native_archive_output::test_compose_interrupted(capture,selected,||Ok(binding.clone()))}).await.unwrap_err().contains("controlled interruption"));
 let binding=saved_binding.lock().unwrap().clone();let parent=output.parent().unwrap();let pending=super::super::native_archive_output::pending(parent,&binding).unwrap();let report=pending["operations"].as_array().unwrap().iter().find(|row|row["destination"]==output.to_string_lossy().as_ref()).unwrap().clone();let op=report["operation"].as_str().unwrap();
 let stage=parent.join(format!(".starlane-archive-{op}/archive.zip"));let mut zip=zip::ZipArchive::new(File::open(&stage).unwrap()).unwrap();let mut bytes=String::new();zip.by_name("starlane-native/raw-receipts.json").unwrap().read_to_string(&mut bytes).unwrap();assert_eq!(serde_json::from_str::<Vec<String>>(&bytes).unwrap(),vec![raw.clone()]);drop(zip);
 super::super::native_archive_output::acknowledge_review(parent,op,&binding,&report).unwrap();assert!(!output.exists());assert!(stage.exists());let db=pool(&root).await;let after:String=sqlx::query_scalar("SELECT receipt FROM native_attachment_download_receipts").fetch_one(&db).await.unwrap();assert_eq!(after,raw);db.close().await;fs::remove_dir_all(root).unwrap();
}

pub(crate) async fn export_original_fixture(root:&Path,destination:PathBuf){archive_owner(root).await.with_capture(local(),move|c|{let binding=c.output_binding();let selected=super::super::native_archive_output::test_selection(destination,binding.clone())?;super::super::native_archive_output::compose(c,selected,||Ok(binding.clone()))?;Ok(())}).await.unwrap();}

pub(crate) async fn installed_import_owner_bytes(root:&Path,length:usize){
 initialized(root).await;let mut request=import_request(root,&[]);let mut hash=Sha256::new();let chunk=[7;65536];let mut remaining=length;while remaining>0{let n=remaining.min(chunk.len());hash.update(&chunk[..n]);remaining-=n;}
 let mut version:Value=serde_json::from_str(&request.server_version_raw).unwrap();version["byte_length"]=json!(length);version["binary_sha256"]=json!(format!("{:x}",hash.finalize()));version["retired_at"]=json!("original-retirement");version["future"]=json!({"preserve":"raw original version"});request.server_version_raw=serde_json::to_string_pretty(&version).unwrap();let owner=capture(root,request.clone());owner.begin(&request.origin).await.unwrap();let mut remaining=length;let mut index=0;while remaining>0{let n=remaining.min(chunk.len());owner.append(&request.origin,index,chunk[..n].to_vec()).await.unwrap();remaining-=n;index+=1;}owner.finish(&request.origin).await.unwrap();
 let db=pool(root).await;let mut snapshot=read_snapshot_from_pool(&db,"none").await.unwrap().unwrap();let url=snapshot["assets"][0]["url"].as_str().unwrap();snapshot["documents"][0]["content"]=json!(format!("🌙 ![exact]({url})"));let mut tx=db.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();db.close().await;
}

async fn genuine_import_reexport_roundtrip(length:usize,lost_ack:bool){
 use crate::local_database::joint_archive::tests::{genuine_import_owner_fixture,import_existing_owner_archive};
 use super::super::{native_archive_output,native_archive_import};use std::io::Read;
 let(root,selection,request,current,first)=genuine_import_owner_fixture(length).await;let vault=root.join("new-vault");let sql=SqlitePoolOptions::new().connect_with(SqliteConnectOptions::new().filename(vault.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();let original_receipt:String=sqlx::query_scalar("SELECT result_json FROM native_archive_import_receipts WHERE operation_id=?1").bind(&request.operation).fetch_one(&sql).await.unwrap();sql.close().await;
 let output=root.join("reexport.zip");let archive=output.clone();let raw_receipt=original_receipt.clone();let owner=Owner::capture(selection,"none",first["canonical_revision"].as_str().unwrap(),current.clone()).unwrap();
 owner.with_capture(current.clone(),move|capture|{assert_eq!(capture.raw_imports.len(),1);assert_eq!(capture.raw_imports[0]["receipt_json"],raw_receipt);assert!(capture.binaries[0].native_intent_sha256.is_none());assert!(capture.binaries[0].imported_provenance().is_some());let binding=capture.output_binding();let selected=native_archive_output::test_selection(archive.clone(),binding.clone())?;native_archive_output::compose(capture,selected,||Ok(binding.clone()))?;let mut zip=zip::ZipArchive::new(File::open(archive).map_err(err)?).map_err(err)?;let mut manifest=String::new();zip.by_name("starlane-native/manifest.json").map_err(err)?.read_to_string(&mut manifest).map_err(err)?;let manifest:Value=serde_json::from_str(&manifest).map_err(err)?;assert_eq!(manifest["format"],2);assert_eq!(manifest["binaries"][0]["kind"],"joint_import");assert!(manifest["binaries"][0]["native_intent_sha256"].is_null());Ok(())}).await.unwrap();
 let(selection,second_request,second)=import_existing_owner_archive(&output,root.join("third-vault"),lost_ack).await;
 assert_eq!(first["candidate"]["assets"][0]["server_version_provenance"],second["candidate"]["assets"][0]["server_version_provenance"]);assert_ne!(first["candidate"]["assets"][0]["vault_path"],second["candidate"]["assets"][0]["vault_path"]);let first_creator:Value=serde_json::from_str(first["native_installation"]["manifest_json"].as_str().unwrap()).unwrap();let second_creator:Value=serde_json::from_str(second["native_installation"]["manifest_json"].as_str().unwrap()).unwrap();assert_ne!(first_creator["binaries"][0]["identity"],second_creator["binaries"][0]["identity"]);let imports:Value=serde_json::from_str(second["imported_provenance"]["joint_imports_json"].as_str().unwrap()).unwrap();assert_eq!(imports[0]["receipt_json"],original_receipt);
 let reader=native_archive_import::Owner::capture(selection,"none",second_request,current.clone()).unwrap();reader.with_read(current,move|reader|{struct Check{count:usize,max:usize}impl Write for Check{fn write(&mut self,b:&[u8])->std::io::Result<usize>{assert!(b.iter().all(|v|*v==7));self.count+=b.len();self.max=self.max.max(b.len());Ok(b.len())}fn flush(&mut self)->std::io::Result<()>{Ok(())}}let mut check=Check{count:0,max:0};assert_eq!(reader.copy_exact(&mut check)?,length as u64);assert_eq!(check.count,length);assert!(check.max<=65536);Ok(())}).await.unwrap();fs::remove_dir_all(root).unwrap();
}
#[tokio::test]async fn imported_canonical_reexport_preserves_typed_receipt_chain_and_fresh_destination_ownership(){let _guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());genuine_import_reexport_roundtrip(196608,false).await;}
#[tokio::test]async fn imported_reexport_lost_joint_ack_resolves_original_receipt_and_reads_new_owner(){let _guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());genuine_import_reexport_roundtrip(196608,true).await;}
#[tokio::test]#[ignore="actual source receipt/import/canonical reexport/reimport reader100MiB"]async fn actual_imported_canonical_reexport_100mib(){let _guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());genuine_import_reexport_roundtrip(104857600,false).await;println!("ACTUAL_GENUINE_IMPORTED_CANONICAL_REEXPORT_REIMPORT_100MIB_TYPED_RAW_CHAIN_ALL_BYTES_64K_PASS");}
#[tokio::test]async fn retained_owned_capture_preserves_history_without_serving_or_inode_resurrection(){
 use crate::local_database::joint_archive::tests::genuine_import_owner_fixture;
 let _guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());
 for change in ["reference","inode","historical"]{
  let(root,selection,request,current,result)=genuine_import_owner_fixture(7).await;let path=root.join("new-vault/.starlane/starlane-notes.sqlite3");let db=SqlitePoolOptions::new().connect_with(SqliteConnectOptions::new().filename(&path)).await.unwrap();let mut revision=result["canonical_revision"].as_str().unwrap().to_string();
  if change=="inode"{let original=PathBuf::from(result["candidate"]["assets"][0]["vault_path"].as_str().unwrap());let retained=original.with_extension("old");fs::rename(&original,&retained).unwrap();fs::copy(retained,original).unwrap();}else{let mut snapshot=read_snapshot_from_pool(&db,"none").await.unwrap().unwrap();if change=="reference"{snapshot["documents"][0]["content"]=json!("removed current reference");}else{snapshot["assets"]=json!([]);snapshot["documents"][0]["content"]=json!("removed asset");}let mut tx=db.begin().await.unwrap();revision=write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();}db.close().await;
  let interactive=super::super::native_archive_import::Owner::capture(selection.clone(),"none",request,current.clone()).unwrap();assert!(interactive.with_read(current.clone(),|_|Ok(())).await.is_err());
  let empty=change=="historical";let owner=Owner::capture(selection,"none",&revision,current.clone()).unwrap();let result=owner.with_capture(current,move|capture|{assert_eq!(capture.binaries.len(),if empty{0}else{1});assert_eq!(capture.raw_imports.len(),1);if empty{assert_eq!(capture.snapshot["assets"],json!([]));}Ok(())}).await;
  if change=="inode"{assert!(result.is_err());}else{result.unwrap();}fs::remove_dir_all(root).unwrap();
 }
}

#[tokio::test]async fn actual_imported_format2_missing_sidecar_rewritten_raw_receipt_and_mislabeled_download_refuse(){
 use crate::local_database::joint_archive::tests::genuine_import_owner_fixture;use super::super::native_archive_output;use std::io::Read;
 let _guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());let(root,selection,_,current,result)=genuine_import_owner_fixture(196608).await;let output=root.join("portable.zip");let archive=output.clone();Owner::capture(selection,"none",result["canonical_revision"].as_str().unwrap(),current.clone()).unwrap().with_capture(current,move|capture|{let binding=capture.output_binding();let selected=native_archive_output::test_selection(archive,binding.clone())?;native_archive_output::compose(capture,selected,||Ok(binding.clone()))?;Ok(())}).await.unwrap();
 let mut source=zip::ZipArchive::new(File::open(&output).unwrap()).unwrap();let mut members=Vec::new();for i in 0..source.len(){let mut member=source.by_index(i).unwrap();let mut bytes=Vec::new();member.read_to_end(&mut bytes).unwrap();members.push((member.name().to_string(),bytes));}drop(source);
 for fault in ["missing","rewritten","kind","authority"]{let mut members=members.clone();let index=members.iter().position(|(n,_)|n=="starlane-native/manifest.json").unwrap();let mut manifest:Value=serde_json::from_slice(&members[index].1).unwrap();match fault{
  "missing"=>members.retain(|(n,_)|n!="starlane-native/joint-imports.json"),
  "rewritten"=>{let j=members.iter().position(|(n,_)|n=="starlane-native/joint-imports.json").unwrap();let mut joints:Value=serde_json::from_slice(&members[j].1).unwrap();let raw=joints[0]["receipt_json"].as_str().unwrap();joints[0]["receipt_json"]=json!(format!("{raw} "));members[j].1=serde_json::to_vec(&joints).unwrap();manifest["joint_imports_sha256"]=json!(format!("{:x}",Sha256::digest(&members[j].1)));members[index].1=serde_json::to_vec(&manifest).unwrap();},
  "kind"=>{manifest["binaries"][0]["kind"]=json!("download");manifest["binaries"][0]["native_intent_sha256"]=json!("a".repeat(64));members[index].1=serde_json::to_vec(&manifest).unwrap();},
  _=>{manifest["binaries"][0]["serve_authority"]=json!(true);members[index].1=serde_json::to_vec(&manifest).unwrap();}
 };let altered=root.join(format!("bad-{fault}.zip"));let mut zip=zip::ZipWriter::new(File::create(&altered).unwrap());for(name,bytes)in members{zip.start_file(name,zip::write::SimpleFileOptions::default()).unwrap();zip.write_all(&bytes).unwrap();}zip.finish().unwrap().sync_all().unwrap();let destination=root.join(format!("refused-{fault}/assets"));assert!(vault::archive_admission::prepare("none",&altered,&destination).is_err(),"{fault} must refuse portable evidence admission");assert!(!destination.parent().unwrap().exists());}
 fs::remove_dir_all(root).unwrap();
}

#[tokio::test]async fn imported_reexport_same_filename_never_retargets_newer_local_binary(){
 use crate::local_database::joint_archive::tests::{genuine_import_owner_fixture,import_existing_owner_archive};use super::super::{native_archive_output,native_archive_import};use std::io::Read;
 let _guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());let(root,selection,_,current,_)=genuine_import_owner_fixture(23).await;let vault=root.join("new-vault");let newer=vault.join("assets/newer.bin");fs::write(&newer,b"NEWER SAME NAME").unwrap();let sql=SqlitePoolOptions::new().connect_with(SqliteConnectOptions::new().filename(vault.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();let mut snapshot=read_snapshot_from_pool(&sql,"none").await.unwrap().unwrap();snapshot["assets"].as_array_mut().unwrap().push(json!({"id":"newer-file","source":"tauri","name":"same-name.bin","mime_type":"application/octet-stream","size":15,"url":"asset://newer-original","vault_path":newer,"created_at":"later","future":{"keep":"newer"}}));let mut tx=sql.begin().await.unwrap();let revision=write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();sql.close().await;let output=root.join("same-name.zip");let archive=output.clone();Owner::capture(selection,"none",&revision,current.clone()).unwrap().with_capture(current.clone(),move|capture|{assert_eq!(capture.binaries.len(),2);let binding=capture.output_binding();let selected=native_archive_output::test_selection(archive,binding.clone())?;native_archive_output::compose(capture,selected,||Ok(binding.clone()))?;Ok(())}).await.unwrap();
 let mut zip=zip::ZipArchive::new(File::open(&output).unwrap()).unwrap();let mut old=vec![];let original:vault::VaultAsset=serde_json::from_value(snapshot["assets"][0].clone()).unwrap();zip.by_name(&vault::archive_asset_path(&original)).unwrap().read_to_end(&mut old).unwrap();assert_eq!(old,vec![7;23]);let mut new=vec![];zip.by_name("assets/newer-file-same-name.bin").unwrap().read_to_end(&mut new).unwrap();assert_eq!(new,b"NEWER SAME NAME");drop(zip);
 let(selection,request,second)=import_existing_owner_archive(&output,root.join("third-vault"),false).await;assert_ne!(second["candidate"]["assets"][0]["vault_path"],second["candidate"]["assets"][1]["vault_path"]);native_archive_import::Owner::capture(selection,"none",request,current.clone()).unwrap().with_read(current,|reader|{let mut bytes=vec![];reader.copy_exact(&mut bytes)?;assert_eq!(bytes,vec![7;23]);Ok(())}).await.unwrap();fs::remove_dir_all(root).unwrap();
}

#[tokio::test]async fn actual_orphan_import_receipt_refuses_before_capture_consumer(){
 use crate::local_database::joint_archive::tests::genuine_import_owner_fixture;
 let _guard=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());let(root,selection,_,current,_)=genuine_import_owner_fixture(7).await;
 let db=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(root.join("new-vault/.starlane/starlane-notes.sqlite3")).foreign_keys(false)).await.unwrap();
 let names:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='trigger' AND tbl_name='native_archive_import_intents'").fetch_all(&db).await.unwrap();for name in names{sqlx::query(&format!("DROP TRIGGER \"{}\"",name.replace('"',"\"\""))).execute(&db).await.unwrap();}
 sqlx::query("DELETE FROM native_archive_import_intents").execute(&db).await.unwrap();let revision:String=sqlx::query_scalar("SELECT value FROM local_vault_metadata WHERE key='canonical_revision'").fetch_one(&db).await.unwrap();db.close().await;
 let failure=Owner::capture(selection,"none",&revision,current.clone()).unwrap().with_capture(current,|_|->Result<(),String>{panic!("Orphan imported receipt reached archive consumer")}).await.unwrap_err();assert!(failure.contains("orphan or conflicting"),"{failure}");fs::remove_dir_all(root).unwrap();
}

async fn generic_canonical_import_fixture(label:&str)->(PathBuf,PathBuf,Value){
 use std::os::unix::fs::PermissionsExt;
 let base=fixture(label);let source=base.join("source");fs::create_dir(&source).unwrap();fs::set_permissions(&source,fs::Permissions::from_mode(0o700)).unwrap();initialized(&source).await;fs::create_dir(source.join("assets")).unwrap();fs::set_permissions(source.join("assets"),fs::Permissions::from_mode(0o700)).unwrap();
 let mut assets=vec![];for(id,body,associated)in[("generic-page",vec![1u8,2,3],true),("generic-root",vec![4u8,5,6],false)]{let file=source.join("assets").join(format!("{id}.bin"));fs::write(&file,&body).unwrap();fs::set_permissions(&file,fs::Permissions::from_mode(0o600)).unwrap();let mut asset=json!({"id":id,"name":"same-name.bin","mime_type":"application/octet-stream","size":body.len(),"url":format!("asset://localhost/{}",file.display()),"vault_path":file,"source":"tauri","created_at":"2026-10-15T00:00:00Z"});if associated{asset["space_id"]=json!("local-space");asset["document_id"]=json!("prior-page");}assets.push(asset);}
 let pool=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(source.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();let mut snapshot=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();snapshot["assets"]=json!(assets);let mut tx=pool.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();pool.close().await;
 let archive=base.join("source.zip");let path=archive.clone();archive_owner(&source).await.with_capture(local(),move|capture|{assert_eq!(capture.binaries.len(),2);let binding=capture.output_binding();let selected=super::super::native_archive_output::test_selection(path,binding.clone())?;super::super::native_archive_output::compose(capture,selected,||Ok(binding.clone()))}).await.unwrap();let destination=base.join("destination");let(_,result)=crate::local_database::joint_archive::tests::import_existing_canonical_archive(&archive,destination.clone(),false).await;fs::remove_dir_all(source).unwrap();(base,destination,result)
}
#[tokio::test]
async fn generic_page_and_root_assets_reexport_from_new_creator_without_source_or_serving_grant(){
 let _serial=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());let(base,destination,result)=generic_canonical_import_fixture("generic-root-roundtrip").await;
 assert!(!base.join("source").exists());let path=base.join("again.zip");let archive=path.clone();archive_owner(&destination).await.with_capture(local(),move|capture|{assert_eq!(capture.binaries.len(),2);assert_eq!(capture.raw_imports.len(),1);for binary in &capture.binaries{assert!(binary.imported_provenance().is_some());let mut bytes=vec![];binary.copy_exact(&mut bytes)?;assert_eq!(bytes,if binary.asset["id"]=="generic-page"{vec![1,2,3]}else{vec![4,5,6]});}let binding=capture.output_binding();let selected=super::super::native_archive_output::test_selection(path,binding.clone())?;super::super::native_archive_output::compose(capture,selected,||Ok(binding.clone()))}).await.unwrap();
 let(_,third)=crate::local_database::joint_archive::tests::import_existing_canonical_archive(&archive,base.join("third"),true).await;assert_eq!(third["candidate"]["assets"].as_array().unwrap().len(),2);
 let selection=vault::SnapshotRequestBinding::capture_at(destination.with_extension("settings.json"),destination.clone(),"none").unwrap();for(index,asset)in result["candidate"]["assets"].as_array().unwrap().iter().enumerate(){let request=super::super::native_archive_import::Request{operation:result["operation_id"].as_str().unwrap().into(),request_fingerprint:result["request_fingerprint"].as_str().unwrap().into(),asset_id:asset["id"].as_str().unwrap().into(),index,space_id:"local-space".into(),document_id:"prior-page".into()};let owner=super::super::native_archive_import::Owner::capture(selection.clone(),"none",request,local()).unwrap();assert!(owner.with_read(local(),|_|Ok(())).await.is_err(),"canonical archive purpose must not manufacture an interactive page grant");}fs::remove_dir_all(base).unwrap();
}
#[tokio::test]
async fn generic_import_foreign_inode_and_removed_canonical_row_refuse_resurrection(){
 use std::os::unix::fs::PermissionsExt;
 let _serial=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());let(base,destination,result)=generic_canonical_import_fixture("generic-root-negative").await;let asset=&result["candidate"]["assets"][1];let named=PathBuf::from(asset["vault_path"].as_str().unwrap());let original=base.join("held-original");fs::rename(&named,&original).unwrap();let bytes=fs::read(&original).unwrap();fs::write(&named,&bytes).unwrap();fs::set_permissions(&named,fs::Permissions::from_mode(0o600)).unwrap();assert!(archive_owner(&destination).await.with_capture(local(),|_|Ok(())).await.is_err());assert_eq!(fs::read(&original).unwrap(),bytes);assert_eq!(fs::read(&named).unwrap(),bytes);fs::remove_file(&named).unwrap();fs::rename(&original,&named).unwrap();
 let pool=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(destination.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();let mut snapshot=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();snapshot["assets"].as_array_mut().unwrap().retain(|a|a["id"]!="generic-root");let mut tx=pool.begin().await.unwrap();write_snapshot_in_tx(&mut tx,"none",&snapshot).await.unwrap();tx.commit().await.unwrap();pool.close().await;archive_owner(&destination).await.with_capture(local(),|capture|{assert_eq!(capture.binaries.len(),1);assert_eq!(capture.binaries[0].asset["id"],"generic-page");assert_eq!(capture.raw_imports.len(),1);Ok(())}).await.unwrap();assert_eq!(fs::read(named).unwrap(),bytes);fs::remove_dir_all(base).unwrap();
}
#[tokio::test]
async fn generic_canonical_membership_rejects_duplicate_and_corrupt_actual_receipt_observations(){
 let _serial=vault::archive_admission::TEST_SERIAL.lock().unwrap_or_else(|e|e.into_inner());let(base,destination,_)=generic_canonical_import_fixture("generic-root-receipt").await;let pool=SqlitePoolOptions::new().max_connections(1).connect_with(SqliteConnectOptions::new().filename(destination.join(".starlane/starlane-notes.sqlite3"))).await.unwrap();let snapshot=read_snapshot_from_pool(&pool,"none").await.unwrap().unwrap();let instance:String=sqlx::query_scalar("SELECT instance_id FROM native_archive_instance WHERE singleton=1").fetch_one(&pool).await.unwrap();let rows:Vec<(String,String,String,String,String,String,String)>=sqlx::query_as("SELECT i.operation_id,i.request_fingerprint,i.provider,i.instance_id,i.request_json,r.result_json,r.result_digest FROM native_archive_import_intents i JOIN native_archive_import_receipts r ON r.operation_id=i.operation_id AND r.request_fingerprint=i.request_fingerprint").fetch_all(&pool).await.unwrap();pool.close().await;let selection=vault::SnapshotRequestBinding::capture_at(destination.with_extension("settings.json"),destination.clone(),"none").unwrap();let asset=&snapshot["assets"][0];let capture=|rows:&Vec<(String,String,String,String,String,String,String)>|super::super::native_archive_import::canonical_archive::try_capture(selection.clone(),"none",local(),&snapshot,&instance,rows,asset);assert!(capture(&rows).unwrap().is_some());let mut duplicate=rows.clone();duplicate.push(rows[0].clone());assert!(capture(&duplicate).is_err());let mut corrupt=rows.clone();corrupt[0].6="0".repeat(64);assert!(capture(&corrupt).is_err());let mut removed=snapshot.clone();removed["assets"]=json!([]);assert!(super::super::native_archive_import::canonical_archive::try_capture(selection,"none",local(),&removed,&instance,&rows,asset).is_err());fs::remove_dir_all(base).unwrap();
}

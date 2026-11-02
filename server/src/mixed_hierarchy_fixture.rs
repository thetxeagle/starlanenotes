//! Opt-in real migrated SQLite/PostgreSQL mixed hierarchy routes, synthetic owned data only.
use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
use serde_json::{json, Value};
use sqlx::{AnyPool, Connection};
use tower::ServiceExt;

const SPACE: &str = "mixed-source";
const TARGET: &str = "mixed-target";
const JWT: &str = "fictional-mixed-fixture-secret";

fn token(actor: &str) -> String {
    crate::auth::jwt::create_token_pair_for_session(actor, &format!("{actor}@deep.invalid"),
        "member", JWT, Some(&format!("deep-{actor}")), 30).unwrap().access_token
}
async fn request(s: &crate::AppState, actor: &str, method: &str, url: &str, body: Value) -> (StatusCode, Value) {
    let response = crate::api_routes().with_state(s.clone()).oneshot(Request::builder()
        .method(method).uri(url).header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", token(actor)))
        .body(Body::from(body.to_string())).unwrap()).await.unwrap();
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 16 * 1024 * 1024).await.unwrap();
    let value = if bytes.is_empty() { Value::Null } else {
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({"unparsed_error":String::from_utf8_lossy(&bytes)}))
    };
    (status, value)
}
async fn call(s: &crate::AppState, actor: &str, method: &str, url: &str, body: Value, expected: StatusCode) -> Value {
    let (status, result) = request(s, actor, method, url, body).await;
    assert_eq!(status, expected, "{actor} {method} {url}: {result}"); result
}
async fn setup(postgres: bool) -> (crate::AppState, String) {
    let owned = std::path::PathBuf::from(std::env::var("STARLANE_CORE_WORKSPACE").expect("required owned workspace"));
    let root = crate::storage::data_dir();
    assert!(owned.is_absolute() && root.starts_with(&owned) && root != owned);
    assert!(!root.exists(), "fixture requires a fresh DATA_DIR");
    std::fs::create_dir_all(&root).unwrap();
    sqlx::any::install_default_drivers();
    let schema = format!("mixed_hierarchy_{}", uuid::Uuid::new_v4().simple());
    let db: AnyPool = if postgres {
        let url = std::env::var("STARLANE_CORE_PG_URL").expect("required owned PG URL");
        assert!(url.starts_with("postgres://"));
        let mut control = sqlx::AnyConnection::connect(&url).await.unwrap();
        sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut control).await.unwrap();
        control.close().await.unwrap();
        let selected = schema.clone();
        sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(move |c,_| {
            let name=selected.clone(); Box::pin(async move {sqlx::query(&format!("SET search_path TO {name}")).execute(c).await?;Ok(())})
        }).connect(&url).await.unwrap()
    } else {
        sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(|c,_| Box::pin(async move {
            sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;
            sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?; Ok(())
        })).connect(&format!("sqlite://{}?mode=rwc",root.join("mixed.sqlite").display())).await.unwrap()
    };
    crate::db::migrate_selected(&db, if postgres {crate::db::DatabaseEngine::Postgres} else {crate::db::DatabaseEngine::Sqlite}).await.unwrap();
    for actor in ["owner","editor","viewer"] {
        sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES($1,$2,$1,'member','member')").bind(actor).bind(format!("{actor}@deep.invalid")).execute(&db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES($1,$2,'fictional','2999-01-01T00:00:00Z','browser')").bind(format!("deep-{actor}")).bind(actor).execute(&db).await.unwrap();
    }
    for space in [SPACE,TARGET] {
        sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner')").bind(space).execute(&db).await.unwrap();
        for (actor,role) in [("owner","owner"),("editor","editor"),("viewer","viewer")] {
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,$2,$3,$4)").bind(format!("deep-{space}-{actor}")).bind(space).bind(actor).bind(role).execute(&db).await.unwrap();
        }
    }
    let s=crate::AppState{db,jwt_secret:JWT.into(),base_url:"http://deep.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search-index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
    (s,schema)
}
const LEAF: &str = "mixed-page-32";
const FOLDER: &str = "mixed-folder-32";
const PAYLOAD: &[u8] = b"\0owned mixed immutable asset\xff\r\n";
const TABLES: &[&str] = &[
 "users","sessions","spaces","space_members","folders","documents","document_drafts","document_draft_access",
 "properties_schema","property_values","doc_versions","document_publication_snapshots","draft_publication_receipts","snapshot_recovery_receipts",
 "attachments","attachment_upload_receipts","attachment_families","attachment_version_records","attachment_version_changes","attachment_retirements",
 "storage_journal_receipts","draft_birth_receipts","draft_transfer_receipts","draft_save_replay_receipts","publication_receipts","canonical_revision_receipts",
 "move_commit_receipts","move_metadata_escrow","move_asset_path_aliases","audit_log"
];
async fn domain(s:&crate::AppState)->Value {
 let mut c=s.db.acquire().await.unwrap();let mut out=serde_json::Map::new();
 for table in TABLES {
  let mut rows=crate::spaces::document_deletion::rows(&mut c,&format!("SELECT * FROM {table}"),&[]).await.unwrap();
  // Real bearer extraction intentionally updates last_seen_at and rolling expiry
  // even when a later route denies. Preserve every other credential field.
  if *table=="sessions" {for row in &mut rows {let row=row.as_object_mut().unwrap();row.remove("last_seen_at");row.remove("expires_at");}}
  rows.sort_by_cached_key(Value::to_string);out.insert((*table).into(),json!(rows));
 }
 Value::Object(out)
}
fn equal(label:&str,before:&Value,after:&Value){
 if before!=after {
  let changed=before.as_object().unwrap().keys().filter(|k|before[*k]!=after[*k]).collect::<Vec<_>>();
  let root=crate::storage::data_dir();std::fs::write(root.join("mixed-failed-before.json"),serde_json::to_vec_pretty(before).unwrap()).unwrap();std::fs::write(root.join("mixed-failed-after.json"),serde_json::to_vec_pretty(after).unwrap()).unwrap();
  panic!("{label}: canonical mismatch in {changed:?}; exact owned evidence retained");
 }
}
fn files()->std::collections::BTreeMap<String,String>{
 use sha2::{Digest,Sha256};
 fn walk(root:&std::path::Path,p:&std::path::Path,out:&mut std::collections::BTreeMap<String,String>){
  let m=std::fs::symlink_metadata(p).unwrap();assert!(!m.file_type().is_symlink(),"owned positive fixture has no symlinks");
  if m.is_dir(){for e in std::fs::read_dir(p).unwrap(){walk(root,&e.unwrap().path(),out);}}
  else{assert!(m.is_file());out.insert(p.strip_prefix(root).unwrap().to_string_lossy().into_owned(),hex::encode(Sha256::digest(std::fs::read(p).unwrap())));}
 }
 let root=crate::storage::data_dir();let mut out=std::collections::BTreeMap::new();
 for dir in ["spaces",".storage-journal",".publication-journal",".canonical-rewrite-journal",".move-relocation-journal",".attachment-upload-journal"] {let p=root.join(dir);if p.exists(){walk(&root,&p,&mut out);}}
 out
}
fn page(space:&str,id:&str)->String{format!("/spaces/{space}/documents/{id}")}
async fn create_folder(s:&crate::AppState,space:&str,id:&str,parent:Option<&str>){call(s,"owner","POST",&format!("/spaces/{space}/folders"),json!({"source_id":id,"name":id,"parent_id":parent}),StatusCode::CREATED).await;}
fn body(id:&str)->String{format!("Original mixed {id} Ω\r\n[[mixed-page-32]]\r\n<!-- starlane:custom-css -->\r\n.notice {{ color: inherit; }}\r\n<!-- /starlane:custom-css -->\r\n")}
async fn create_page(s:&crate::AppState,id:&str,parent:Option<&str>)->String {
 let p=call(s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"source_id":id,"title":id,"parent_id":parent,"folder_id":FOLDER,"content":body(id)}),StatusCode::CREATED).await;
 assert_eq!(p["id"],id);p["draft_id"].as_str().unwrap().into()
}
async fn raw(s:&crate::AppState,actor:&str,url:&str)->(StatusCode,Vec<u8>){
 let r=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().uri(url.trim_start_matches("/api/v1")).header("Authorization",format!("Bearer {}",token(actor))).body(Body::empty()).unwrap()).await.unwrap();
 (r.status(),to_bytes(r.into_body(),16*1024*1024).await.unwrap().to_vec())
}
async fn upload(s:&crate::AppState)->Value{
 let mut b=b"--mixed\r\nContent-Disposition: form-data; name=\"file\"; filename=\"owned-mixed.bin\"\r\nContent-Type: application/octet-stream\r\n\r\n".to_vec();b.extend_from_slice(PAYLOAD);b.extend_from_slice(b"\r\n--mixed--\r\n");
 let r=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().method("POST").uri(format!("{}/attachments",page(SPACE,LEAF))).header("Authorization",format!("Bearer {}",token("owner"))).header("x-starlane-upload-operation",uuid::Uuid::new_v4().to_string()).header("content-type","multipart/form-data; boundary=mixed").body(Body::from(b)).unwrap()).await.unwrap();
 let code=r.status();let bytes=to_bytes(r.into_body(),100000).await.unwrap();assert_eq!(code,StatusCode::CREATED,"upload: {}",String::from_utf8_lossy(&bytes));serde_json::from_slice(&bytes).unwrap()
}
async fn save(s:&crate::AppState,space:&str,draft:&str,content:&str){
 let base=format!("{}/drafts/{draft}",page(space,LEAF));let source=call(s,"owner","GET",&format!("{base}/save-source"),Value::Null,StatusCode::OK).await;
 call(s,"owner","PUT",&base,json!({"source_digest":source["draft_source_digest"],"content":content}),StatusCode::OK).await;
}
async fn birth(s:&crate::AppState)->String{
 let url=format!("{}/drafts",page(SPACE,LEAF));let request=json!({"operation_id":uuid::Uuid::new_v4().to_string()});
 let draft=call(s,"owner","POST",&url,request.clone(),StatusCode::CREATED).await;
 let before=domain(s).await;let retry=call(s,"owner","POST",&url,request,StatusCode::CREATED).await;assert_eq!(retry,draft);equal("exact published-source draft birth retry",&before,&domain(s).await);
 draft["id"].as_str().unwrap().to_owned()
}
async fn publish(s:&crate::AppState,draft:&str){
 let base=format!("{}/drafts/{draft}",page(SPACE,LEAF));let source=call(s,"owner","GET",&format!("{base}/publish-source"),Value::Null,StatusCode::OK).await;
 call(s,"owner","POST",&format!("{base}/publish"),json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"draft_source_digest":source["draft_source_digest"],"page_source_digest":source["page_source_digest"]}),StatusCode::OK).await;
}
async fn denied(s:&crate::AppState,actor:&str,method:&str,url:&str,body:Value){
 let before=domain(s).await;let original_files=files();let(code,v)=request(s,actor,method,url,body).await;assert!(code.is_client_error(),"expected refusal: {code} {v}");
 equal("refused request",&before,&domain(s).await);assert_eq!(original_files,files(),"refused request changed retained body/asset/journal bytes");
}
async fn lifecycle(s:&crate::AppState,space:&str,target:&str,method:&str,suffix:&str,action:&str){
 let before=domain(s).await;let original_files=files();
 call(s,"owner",method,&format!("{}{suffix}",page(space,target)),Value::Null,StatusCode::NO_CONTENT).await;
 let after=domain(s).await;let mut expected=before.clone();
 let row=after["documents"].as_array().unwrap().iter().find(|r|r["id"]==target).unwrap();
 let old=expected["documents"].as_array_mut().unwrap().iter_mut().find(|r|r["id"]==target).unwrap();
 for field in ["updated_at","archived_at","deleted_at","deleted_by"]{old[field]=row[field].clone();}
 expected["documents"].as_array_mut().unwrap().sort_by_cached_key(Value::to_string);
 let audits=after["audit_log"].as_array().unwrap();let prior=before["audit_log"].as_array().unwrap();assert!(prior.iter().all(|r|audits.contains(r)));
 let added=audits.iter().filter(|r|!prior.contains(r)).collect::<Vec<_>>();assert_eq!(added.len(),1);assert_eq!(added[0]["action"],action);assert_eq!(added[0]["target_id"],target);
 expected["audit_log"]=after["audit_log"].clone();equal("target-only lifecycle",&expected,&after);assert_eq!(original_files,files());
 match action {"document.archive"=>{assert!(row["archived_at"].is_string());assert!(row["deleted_at"].is_null());},"document.unarchive"=>assert!(row["archived_at"].is_null()),"document.delete"=>{assert!(row["deleted_at"].is_string());assert!(row["archived_at"].is_null());},"document.restore"=>{assert!(row["deleted_at"].is_null());assert!(row["archived_at"].is_null());},_=>unreachable!()}
}
async fn catalog(s:&crate::AppState,space:&str)->Vec<String>{
 call(s,"owner","GET",&format!("/spaces/{space}/documents?include_nested=true"),Value::Null,StatusCode::OK).await.as_array().unwrap().iter().filter_map(|r|r["id"].as_str().filter(|id|id.starts_with("mixed-page-")||id.starts_with("mixed-sibling-")).map(str::to_owned)).collect()
}
async fn lifecycle_cycle(s:&crate::AppState,space:&str){
 let target="mixed-page-16";assert_eq!(catalog(s,space).await.len(),64);
 lifecycle(s,space,target,"POST","/archive","document.archive").await;
 // Current contract: archived metadata can remain inspectable, never assert404 here.
 call(s,"owner","GET",&page(space,target),Value::Null,StatusCode::OK).await;assert_eq!(catalog(s,space).await.len(),47);
 lifecycle(s,space,target,"DELETE","/archive","document.unarchive").await;assert_eq!(catalog(s,space).await.len(),64);
 lifecycle(s,space,target,"POST","/archive","document.archive").await;
 lifecycle(s,space,target,"DELETE","","document.delete").await;assert_eq!(catalog(s,space).await.len(),47);
 lifecycle(s,space,target,"POST","/trash","document.restore").await;assert_eq!(catalog(s,space).await.len(),64);
}
async fn preview(s:&crate::AppState)->Value{
 call(s,"owner","POST",&format!("/spaces/{SPACE}/moves/preview"),json!({"root":{"kind":"folder","id":"mixed-folder-1"},"destination":{"space_id":TARGET,"folder_id":null,"parent_id":"mixed-target-folder-4"}}),StatusCode::OK).await
}
fn intent(p:&Value)->Value{json!({"fingerprint":p["fingerprint"],"decisions":[{"family":"property","source_id":"mixed-property-source","choice":{"kind":"map","destination_id":"mixed-property-target"}}],"public_links":"preserve","confirm_audience_change":true,"confirm_draft_space_change":true})}
fn compare_moved(before:&Value,after:&Value){
 for table in ["folders","documents","document_drafts","attachments","property_values"]{
  let mut expected=before[table].clone();
  for row in expected.as_array_mut().unwrap(){
   let id=row["id"].as_str().unwrap().to_owned();let actual=after[table].as_array().unwrap().iter().find(|r|r["id"]==id).unwrap();
   if table=="property_values"{if row["property_id"]=="mixed-property-source"{row["property_id"]=json!("mixed-property-target");}}
   else if row["space_id"]==SPACE {
    row["space_id"]=json!(TARGET);
    if table=="folders"&&id=="mixed-folder-1"{row["parent_id"]=json!("mixed-target-folder-4");}
    if table=="documents"||table=="attachments"{
     let key=if table=="documents"{"content_path"}else{"storage_path"};
     let old_path=row[key].as_str().unwrap();let new_path=actual[key].as_str().unwrap();assert!(new_path.starts_with(&format!("spaces/{TARGET}/")));
     assert_eq!(std::fs::read(crate::storage::data_dir().join(old_path)).unwrap(),std::fs::read(crate::storage::data_dir().join(new_path)).unwrap());row[key]=actual[key].clone();
    }
   }
   assert_eq!(&*row,actual,"only explicit reviewed scope/path/property mapping changes allowed table={table} id={id}");
  }
  assert_eq!(expected.as_array().unwrap().len(),after[table].as_array().unwrap().len());
 }
 for table in ["properties_schema","doc_versions","document_publication_snapshots","draft_publication_receipts","snapshot_recovery_receipts","storage_journal_receipts","draft_birth_receipts","draft_transfer_receipts","draft_save_replay_receipts","publication_receipts","canonical_revision_receipts","attachment_upload_receipts","attachment_families","attachment_version_records","attachment_version_changes","attachment_retirements","document_draft_access"]{assert_eq!(before[table],after[table],"immutable/current retained evidence changed unexpectedly table={table}");}
}
async fn backlinks(s:&crate::AppState,space:&str)->std::collections::BTreeSet<String>{call(s,"owner","GET",&format!("{}/backlinks",page(space,LEAF)),Value::Null,StatusCode::OK).await.as_array().unwrap().iter().map(|r|r["document_id"].as_str().unwrap().to_owned()).collect()}
async fn reopen(postgres:bool,schema:&str)->crate::AppState{
 let db=if postgres {let url=std::env::var("STARLANE_CORE_PG_URL").unwrap();let schema=schema.to_owned();sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(move|c,_|{let schema=schema.clone();Box::pin(async move{sqlx::query(&format!("SET search_path TO {schema}")).execute(c).await?;Ok(())})}).connect(&url).await.unwrap()}
 else {sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(|c,_|Box::pin(async move{sqlx::query("PRAGMA foreign_keys=ON").execute(&mut*c).await?;sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;Ok(())})).connect(&format!("sqlite://{}",crate::storage::data_dir().join("mixed.sqlite").display())).await.unwrap()};
 crate::AppState{db,jwt_secret:JWT.into(),base_url:"http://deep.invalid".into(),search_index:crate::search::SearchIndex::new(crate::storage::data_dir().join("search-index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()}
}
async fn matrix(postgres:bool){
 let(s,schema)=setup(postgres).await;
 for depth in 1..=32{let id=format!("mixed-folder-{depth}");let parent=(depth>1).then(||format!("mixed-folder-{}",depth-1));create_folder(&s,SPACE,&id,parent.as_deref()).await;}
 for depth in 1..=4{let id=format!("mixed-target-folder-{depth}");let parent=(depth>1).then(||format!("mixed-target-folder-{}",depth-1));create_folder(&s,TARGET,&id,parent.as_deref()).await;}
 let mut leaf_draft=String::new();for depth in 1..=32 {let id=format!("mixed-page-{depth}");let parent=(depth>1).then(||format!("mixed-page-{}",depth-1));leaf_draft=create_page(&s,&id,parent.as_deref()).await;}
 for n in 0..32 {create_page(&s,&format!("mixed-sibling-{n}"),Some("mixed-page-1")).await;}
 println!("MIXED_ACTUAL_PRODUCERS folders32 pages32 siblings32 postgres={postgres}");
 for (space,id) in [(SPACE,"mixed-property-source"),(TARGET,"mixed-property-target")] {let p=call(&s,"owner","POST",&format!("/spaces/{space}/properties"),json!({"source_id":id,"name":"Mixed retained text","property_type":"text"}),StatusCode::CREATED).await;assert_eq!(p["id"],id);}
 call(&s,"owner","PUT",&format!("{}/properties",page(SPACE,LEAF)),json!({"values":[{"source_id":"mixed-value-original","property_id":"mixed-property-source","value":"Preserved opaque Ω"}]}),StatusCode::NO_CONTENT).await;
 let asset=upload(&s).await;let asset_url=asset["url"].as_str().unwrap().to_owned();assert_eq!(raw(&s,"owner",&asset_url).await,(StatusCode::OK,PAYLOAD.to_vec()));
 let published1=format!("FIRST mixed publication Ω\r\n[[mixed-page-1]]\r\n![owned]({asset_url})\r\n");let published2=format!("SECOND mixed publication Ω\r\n[[mixed-page-1]]\r\n![owned]({asset_url})\r\n");
 save(&s,SPACE,&leaf_draft,&published1).await;publish(&s,&leaf_draft).await;
 // Publication consumes the chosen draft. New private work uses the real birth route.
 leaf_draft=birth(&s).await;save(&s,SPACE,&leaf_draft,&published2).await;publish(&s,&leaf_draft).await;leaf_draft=birth(&s).await;
 let private=format!("PRIVATE retained unpublished Ω\r\n[[mixed-page-1]]\r\n![owned]({asset_url})\r\n");save(&s,SPACE,&leaf_draft,&private).await;
 let history=call(&s,"owner","GET",&format!("{}/publication-snapshots",page(SPACE,LEAF)),Value::Null,StatusCode::OK).await;assert_eq!(history["snapshots"].as_array().unwrap().len(),2);
 let links=backlinks(&s,SPACE).await;assert_eq!(links.len(),63);assert!(links.contains("mixed-sibling-31"));
 denied(&s,"editor","GET",&page(SPACE,LEAF),Value::Null).await;
 denied(&s,"owner","PUT",&format!("/spaces/{SPACE}/folders/mixed-folder-1"),json!({"parent_id":FOLDER})).await;
 denied(&s,"owner","POST",&format!("{}/move",page(SPACE,"mixed-page-1")),json!({"target_space_id":SPACE,"folder_id":FOLDER,"parent_id":LEAF})).await;
 denied(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"title":"Missing parent","folder_id":FOLDER,"parent_id":"missing"})).await;
 call(&s,"owner","POST",&format!("/spaces/{TARGET}/documents"),json!({"source_id":"mixed-foreign","title":"Foreign fixture","content":"Owned foreign"}),StatusCode::CREATED).await;
 denied(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"title":"Foreign parent","folder_id":FOLDER,"parent_id":"mixed-foreign"})).await;
 sqlx::query("UPDATE space_members SET role='viewer' WHERE space_id=$1 AND user_id='owner'").bind(SPACE).execute(&s.db).await.unwrap();denied(&s,"owner","POST",&format!("{}/archive",page(SPACE,LEAF)),Value::Null).await;sqlx::query("UPDATE space_members SET role='owner' WHERE space_id=$1 AND user_id='owner'").bind(SPACE).execute(&s.db).await.unwrap();
 // Fault setup, not an alternate mutation authority: strong creation ancestors respect current locks.
 sqlx::query("UPDATE documents SET locked=1,locked_by='editor' WHERE id='mixed-page-1'").execute(&s.db).await.unwrap();denied(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"title":"Locked parent","folder_id":FOLDER,"parent_id":LEAF})).await;sqlx::query("UPDATE documents SET locked=0,locked_by=NULL WHERE id='mixed-page-1'").execute(&s.db).await.unwrap();
 lifecycle_cycle(&s,SPACE).await;
 // Real lifecycle final-write rollback; children, properties, drafts, snapshots and bytes remain exact.
 if postgres {sqlx::query("CREATE FUNCTION mixed_late() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE space_members SET role='viewer' WHERE user_id='owner'; RETURN NEW; END $$").execute(&s.db).await.unwrap();sqlx::query("CREATE TRIGGER mixed_late AFTER UPDATE OF archived_at ON documents FOR EACH ROW EXECUTE FUNCTION mixed_late()").execute(&s.db).await.unwrap();}
 else{sqlx::query("CREATE TRIGGER mixed_late AFTER UPDATE OF archived_at ON documents BEGIN UPDATE space_members SET role='viewer' WHERE user_id='owner'; END").execute(&s.db).await.unwrap();}
 denied(&s,"owner","POST",&format!("{}/archive",page(SPACE,LEAF)),Value::Null).await;sqlx::query(if postgres{"DROP TRIGGER mixed_late ON documents"}else{"DROP TRIGGER mixed_late"}).execute(&s.db).await.unwrap();
 denied(&s,"owner","GET","/files/spaces/%2e%2e/foreign.bin",Value::Null).await;
 println!("MIXED_LIFECYCLE_TARGET_ONLY archive/unarchive/archive/Trash/restore +private/revoked/locks/cycles/finalwrite/path PASS postgres={postgres}");
 let stale=preview(&s).await;assert_eq!(stale["blockers"],json!([]),"{stale}");
 let private_final=format!("{private}AFTER preview exact source change\r\n");save(&s,SPACE,&leaf_draft,&private_final).await;
 denied(&s,"owner","POST",&format!("/moves/previews/{}/commit",stale["preview_id"].as_str().unwrap()),intent(&stale)).await;
 let p=preview(&s).await;assert_eq!(p["blockers"],json!([]),"{p}");assert_eq!(p["commit_available"],true,"{p}");
 let before=domain(&s).await;let original_files=files();let commit=intent(&p);let commit_url=format!("/moves/previews/{}/commit",p["preview_id"].as_str().unwrap());
 let result=call(&s,"owner","POST",&commit_url,commit.clone(),StatusCode::OK).await;assert_eq!(result["committed"],true);let moved=domain(&s).await;compare_moved(&before,&moved);
 let moved_files=files();for(path,hash)in original_files{assert_eq!(moved_files.get(&path),Some(&hash),"move discarded original evidence {path}");}
 call(&s,"owner","POST",&commit_url,commit.clone(),StatusCode::OK).await;equal("exact reviewed retry",&moved,&domain(&s).await);
 assert_eq!(backlinks(&s,TARGET).await,links);assert_eq!(raw(&s,"owner",&asset_url).await,(StatusCode::OK,PAYLOAD.to_vec()));
 let props=call(&s,"owner","GET",&format!("{}/properties",page(TARGET,LEAF)),Value::Null,StatusCode::OK).await;assert!(props.as_array().unwrap().iter().any(|p|p["property_id"]=="mixed-property-target"&&p["value"]=="Preserved opaque Ω"));
 let moved_history=call(&s,"owner","GET",&format!("{}/publication-snapshots",page(TARGET,LEAF)),Value::Null,StatusCode::OK).await;assert_eq!(moved_history["snapshots"],history["snapshots"]);
 for row in history["snapshots"].as_array().unwrap(){let id=row["id"].as_str().unwrap();let(code,bytes)=raw(&s,"owner",&format!("{}/publication-snapshots/{id}/raw",page(TARGET,LEAF))).await;assert_eq!(code,StatusCode::OK);let expected:Vec<u8>=sqlx::query_scalar("SELECT raw_markdown FROM document_publication_snapshots WHERE id=$1").bind(id).fetch_one(&s.db).await.unwrap();assert_eq!(bytes,expected);}
 denied(&s,"viewer","GET",&page(TARGET,LEAF),Value::Null).await;denied(&s,"viewer","GET",asset_url.trim_start_matches("/api/v1"),Value::Null).await;
 sqlx::query("UPDATE users SET is_active=0 WHERE id='owner'").execute(&s.db).await.unwrap();denied(&s,"owner","POST",&commit_url,commit).await;denied(&s,"owner","GET",asset_url.trim_start_matches("/api/v1"),Value::Null).await;sqlx::query("UPDATE users SET is_active=1 WHERE id='owner'").execute(&s.db).await.unwrap();
 lifecycle_cycle(&s,TARGET).await;
 let expected=domain(&s).await;let expected_files=files();s.db.close().await;drop(s);
 let s=reopen(postgres,&schema).await;equal("pool/index reopen",&expected,&domain(&s).await);assert_eq!(files(),expected_files);assert_eq!(catalog(&s,TARGET).await.len(),64);
 let draft=call(&s,"owner","GET",&format!("{}/drafts/{leaf_draft}",page(TARGET,LEAF)),Value::Null,StatusCode::OK).await;assert_eq!(draft["content"],private_final);assert_eq!(backlinks(&s,TARGET).await,links);assert_eq!(raw(&s,"owner",&asset_url).await,(StatusCode::OK,PAYLOAD.to_vec()));
 println!("MIXED_REVIEWED_MOVE identity/edges/body/drafts/property_mapping/links/history/binary_asset/source_evidence/retry/reopen PASS postgres={postgres}");
 // Synthetic topology-only broad resource input; no fabricated history, receipts or custody.
 create_folder(&s,TARGET,"mixed-budget-root",None).await;
 let mut tx=s.db.begin().await.unwrap();for chunk in 0..100{let values=(0..100).map(|n|format!("(${},$1,$2,${})",n+3,n+3)).collect::<Vec<_>>().join(",");let sql=format!("INSERT INTO folders(id,space_id,parent_id,name) VALUES {values}");let mut q=sqlx::query(&sql).bind(TARGET).bind("mixed-budget-root");for n in 0..100{q=q.bind(format!("mixed-budget-{}",chunk*100+n));}q.execute(&mut *tx).await.unwrap();}tx.commit().await.unwrap();
 let before_budget=domain(&s).await;let retained_budget_files=files();
 let(code,error)=request(&s,"owner","PUT",&format!("/spaces/{TARGET}/folders/mixed-budget-root"),json!({"name":"must refuse10001"})).await;
 assert_eq!(code,StatusCode::BAD_REQUEST,"{error}");assert!(error.to_string().contains("10000-node traversal budget"),"{error}");equal("10001 folder operation budget",&before_budget,&domain(&s).await);assert_eq!(files(),retained_budget_files);
 let(code,error)=request(&s,"owner","POST",&format!("/spaces/{TARGET}/moves/preview"),json!({"root":{"kind":"folder","id":"mixed-budget-root"},"destination":{"space_id":SPACE,"parent_id":null,"folder_id":null}})).await;
 assert_eq!(code,StatusCode::NOT_FOUND,"{error}");equal("reviewed1000 inventory admission",&before_budget,&domain(&s).await);assert_eq!(files(),retained_budget_files);
 let record=json!({"schema":schema,"postgres":postgres,"folders":32,"page_chain":32,"siblings":32,"private_draft_id":leaf_draft,"snapshots":2,"asset_id":asset["id"],"asset_bytes":PAYLOAD.len(),"lifecycle":"target_only_archive_unarchive_archive_trash_restore_before_and_after_move","move":"genuine_reviewed_cross_space_folder_root_with_explicit_property_map","reload":"close_and_reopen_sql_pool_and_search_index_not_full_restore","resource":"synthetic10001_folder_resource_refusal_and_existing1000_review_inventory_refusal"});
 std::fs::write(crate::storage::data_dir().join("mixed-runtime-result.json"),serde_json::to_vec_pretty(&record).unwrap()).unwrap();
 println!("MIXED_HIERARCHY_MATRIX_PASS postgres={postgres} folders32 pages32 siblings32 snapshots2 actual_attachment/private_drafts/properties/wiki_links reviewed_move/retry target_only_lifecycle/no_cascade source_preservation late_rollback path/resource_refusal schema={schema}");s.db.close().await;
}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE and fresh DATA_DIR; run alone"]
async fn required_sqlite_mixed_hierarchy_runtime_matrix(){matrix(false).await}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE, STARLANE_CORE_PG_URL and fresh DATA_DIR; run alone"]
async fn required_postgres_mixed_hierarchy_runtime_matrix(){matrix(true).await}

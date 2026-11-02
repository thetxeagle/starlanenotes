//! Opt-in actual SQLite/PostgreSQL lifecycle routes; synthetic owned data only.
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{json,Value};
use sqlx::{AnyPool,Connection};
use tower::ServiceExt;
const SPACE:&str="pg-life-space";
const PAGE:&str="pg-life-page";
const RELATED:&str="pg-life-related";
const JWT:&str="owned-postgres-lifecycle-fixture-secret";
const FIRST:&str="FIRST original Ω café\n";
const SECOND:&str="SECOND saved Ω\n<!-- starlane:custom-css -->\n.notice {font-weight:700}\n<!-- /starlane:custom-css -->\n";
const REFERENCED_BYTES:&[u8]=b"REFERENCED shared draft attachment \xCE\xA9";
const ORPHAN_BYTES:&[u8]=b"PRIVATE unreferenced draft attachment";
const SECOND_TITLE:&str="Shared draft metadata Ω";
const SECOND_ICON:&str="🛰️";
const SECOND_COVER:&str="https://example.invalid/shared-draft-cover.png";
fn page()->String{format!("/spaces/{SPACE}/documents/{PAGE}")}
fn token(actor:&str)->String{crate::auth::jwt::create_token_pair_for_session(actor,&format!("{actor}@life.invalid"),"member",JWT,Some(&format!("life-{actor}")),30).unwrap().access_token}
async fn request(s:&crate::AppState,actor:&str,method:&str,url:&str,body:Value)->(StatusCode,Vec<u8>){
 let r=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().method(method).uri(url).header("Content-Type","application/json").header("Authorization",format!("Bearer {}",token(actor))).body(Body::from(body.to_string())).unwrap()).await.unwrap();
 (r.status(),to_bytes(r.into_body(),8*1024*1024).await.unwrap().to_vec())
}
async fn call(s:&crate::AppState,actor:&str,method:&str,url:&str,body:Value,status:StatusCode)->Value{
 let(code,bytes)=request(s,actor,method,url,body).await;
 assert_eq!(code,status,"{actor} {method} {url}: {}",String::from_utf8_lossy(&bytes));
 if bytes.is_empty(){Value::Null}else{serde_json::from_slice(&bytes).expect("JSON lifecycle response")}
}
async fn upload(s:&crate::AppState,actor:&str,draft:&str,body:&[u8])->Value{
 let boundary="owned-lifecycle-boundary";let operation=uuid::Uuid::new_v4().to_string();let mut encoded=format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"shared.txt\"\r\nContent-Type: text/plain\r\n\r\n").into_bytes();encoded.extend_from_slice(body);encoded.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
 let r=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().method("POST").uri(format!("{}/attachments",page())).header("Authorization",format!("Bearer {}",token(actor))).header("Content-Type",format!("multipart/form-data; boundary={boundary}")).header("x-starlane-upload-operation",operation).header("x-starlane-upload-length",body.len().to_string()).header("x-starlane-upload-draft",draft).body(Body::from(encoded)).unwrap()).await.unwrap();
 let status=r.status();let bytes=to_bytes(r.into_body(),8*1024*1024).await.unwrap();assert_eq!(status,StatusCode::CREATED,"{actor} attachment upload: {}",String::from_utf8_lossy(&bytes));serde_json::from_slice(&bytes).unwrap()
}
fn asset_id(asset:&Value)->&str{asset["id"].as_str().expect("Actual upload asset id")}
fn asset_path(asset:&Value)->&str{asset["url"].as_str().expect("Actual upload URL").strip_prefix("/api/v1").expect("Application-relative upload URL")}
async fn assert_asset(s:&crate::AppState,actor:&str,asset:&Value,status:StatusCode,expected:Option<&[u8]>){let(code,bytes)=request(s,actor,"GET",asset_path(asset),Value::Null).await;assert_eq!(code,status,"{actor} attachment bytes: {}",String::from_utf8_lossy(&bytes));if let Some(expected)=expected{assert_eq!(bytes,expected)}}
async fn protected(s:&crate::AppState)->Value{
 let mut c=s.db.acquire().await.unwrap();let mut out=serde_json::Map::new();
 for(table,query)in [("page","SELECT * FROM documents WHERE id=$1"),("snapshots","SELECT * FROM document_publication_snapshots WHERE document_id=$1"),("publication_receipts","SELECT * FROM draft_publication_receipts WHERE document_id=$1")]{out.insert(table.into(),json!(crate::spaces::document_deletion::rows(&mut c,query,&[PAGE]).await.unwrap()));}
 Value::Object(out)
}
async fn publish(s:&crate::AppState,draft:&str)->Value{
 let base=format!("{}/drafts/{draft}",page());
 let source=call(s,"owner","GET",&format!("{base}/publish-source"),Value::Null,StatusCode::OK).await;
 let body=json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"draft_source_digest":source["draft_source_digest"],"page_source_digest":source["page_source_digest"]});
 let result=call(s,"owner","POST",&format!("{base}/publish"),body.clone(),StatusCode::OK).await;
 let before=protected(s).await;
 call(s,"owner","POST",&format!("{base}/publish"),body,StatusCode::OK).await;
 assert_eq!(protected(s).await,before,"historical publish retry changes current source");result
}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE, STARLANE_CORE_PG_URL and DATA_DIR"]
async fn required_postgres_lifecycle_publish_history_recovery(){
 lifecycle_publish_history_recovery(true).await;
}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE and fresh DATA_DIR"]
async fn required_sqlite_lifecycle_publish_history_recovery(){
 lifecycle_publish_history_recovery(false).await;
}
async fn lifecycle_publish_history_recovery(postgres:bool){
 let owned=std::path::PathBuf::from(std::env::var("STARLANE_CORE_WORKSPACE").expect("required existing opt-in workspace"));
 let root=crate::storage::data_dir();assert!(owned.is_absolute()&&root.starts_with(&owned)&&root!=owned);
 assert!(!root.exists(),"Lifecycle fixture requires a fresh DATA_DIR");std::fs::create_dir_all(&root).unwrap();sqlx::any::install_default_drivers();
 let schema=format!("lifecycle_{}",uuid::Uuid::new_v4().simple());
 let db:AnyPool=if postgres {
  let url=std::env::var("STARLANE_CORE_PG_URL").expect("required owned PG URL");assert!(url.starts_with("postgres://"));
  let mut control=sqlx::AnyConnection::connect(&url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut control).await.unwrap();control.close().await.unwrap();
  let selected=schema.clone();sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(move|c,_|{let name=selected.clone();Box::pin(async move{sqlx::query(&format!("SET search_path TO {name}")).execute(c).await?;Ok(())})}).connect(&url).await.unwrap()
 }else{
  sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(|c,_|Box::pin(async move{sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;Ok(())})).connect(&format!("sqlite://{}?mode=rwc",root.join("lifecycle.sqlite").display())).await.unwrap()
 };
 crate::db::migrate_selected(&db,if postgres{crate::db::DatabaseEngine::Postgres}else{crate::db::DatabaseEngine::Sqlite}).await.unwrap();
 for actor in ["owner","editor","viewer"]{sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES($1,$2,$1,'member','member')").bind(actor).bind(format!("{actor}@life.invalid")).execute(&db).await.unwrap();sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES($1,$2,'not-a-token','2999-01-01T00:00:00Z','browser')").bind(format!("life-{actor}")).bind(actor).execute(&db).await.unwrap();}
 sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,'Owned lifecycle','pg-life','private','owner')").bind(SPACE).execute(&db).await.unwrap();
 for(actor,role)in[("owner","owner"),("editor","editor"),("viewer","viewer")]{sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,$2,$3,$4)").bind(format!("life-member-{actor}")).bind(SPACE).bind(actor).bind(role).execute(&db).await.unwrap();}
 let s=crate::AppState{db,jwt_secret:JWT.into(),base_url:"http://owned.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search-index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
 let created=call(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"source_id":PAGE,"title":"Owned lifecycle","content":FIRST,"property_values":[]}),StatusCode::CREATED).await;
 let first_draft=created["draft_id"].as_str().unwrap();publish(&s,first_draft).await;
 let related=call(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"source_id":RELATED,"title":"Related knowledge","content":"Stable related page","property_values":[]}),StatusCode::CREATED).await;let related_draft=related["draft_id"].as_str().unwrap();
 let related_base=format!("/spaces/{SPACE}/documents/{RELATED}/drafts/{related_draft}");let related_source=call(&s,"owner","GET",&format!("{related_base}/publish-source"),Value::Null,StatusCode::OK).await;call(&s,"owner","POST",&format!("{related_base}/publish"),json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"draft_source_digest":related_source["draft_source_digest"],"page_source_digest":related_source["page_source_digest"]}),StatusCode::OK).await;
 let d=call(&s,"owner","POST",&format!("{}/drafts",page()),json!({}),StatusCode::CREATED).await;let id=d["id"].as_str().unwrap();let draft=format!("{}/drafts/{id}",page());
 call(&s,"owner","PUT",&format!("{draft}/access/editor"),json!({"role":"editor"}),StatusCode::NO_CONTENT).await;
 let referenced=upload(&s,"editor",id,REFERENCED_BYTES).await;let orphan=upload(&s,"editor",id,ORPHAN_BYTES).await;let second=format!("{SECOND}\n[[Related knowledge|{RELATED}]]\n![Exact shared attachment](starlane-attachment://{})\n",asset_id(&referenced));
 assert_asset(&s,"viewer",&referenced,StatusCode::NOT_FOUND,None).await;assert_asset(&s,"viewer",&orphan,StatusCode::NOT_FOUND,None).await;
 let src=call(&s,"editor","GET",&format!("{draft}/save-source"),Value::Null,StatusCode::OK).await;
 call(&s,"editor","PUT",&draft,json!({"source_digest":src["draft_source_digest"],"title":SECOND_TITLE,"icon":SECOND_ICON,"cover_image":SECOND_COVER,"content":second}),StatusCode::OK).await;
 let winner=call(&s,"owner","GET",&draft,Value::Null,StatusCode::OK).await;
 assert_eq!(winner["title"],SECOND_TITLE);assert_eq!(winner["icon"],SECOND_ICON);assert_eq!(winner["cover_image"],SECOND_COVER);
 let loser=json!({"source_digest":src["draft_source_digest"],"title":"Held competing proposal","icon":"⚠️","cover_image":"https://example.invalid/held-proposal.png","content":"STALE LOSER Ω retained"});
 let conflict=call(&s,"owner","PUT",&draft,loser.clone(),StatusCode::CONFLICT).await;
 assert!(conflict["message"].as_str().unwrap().starts_with("source_conflict:"));
 assert_eq!(call(&s,"owner","GET",&draft,Value::Null,StatusCode::OK).await,winner);
 let copy=call(&s,"owner","POST",&format!("{}/drafts",page()),json!({"force_new":true,"operation_id":uuid::Uuid::new_v4().to_string()}),StatusCode::CREATED).await;
 let copy_id=copy["id"].as_str().unwrap();let copy_path=format!("{}/drafts/{copy_id}",page());
 let retained=call(&s,"owner","PUT",&copy_path,json!({"source_digest":copy["source_digest"],"title":loser["title"],"icon":loser["icon"],"cover_image":loser["cover_image"],"content":loser["content"]}),StatusCode::OK).await;
 for field in ["title","icon","cover_image","content"]{assert_eq!(retained[field],loser[field],"Competing proposal field {field} was not preserved in the deliberate private copy");}
 assert_eq!(retained["owner_id"],"owner");assert_eq!(call(&s,"owner","GET",&draft,Value::Null,StatusCode::OK).await,winner);
 let before=protected(&s).await;let(code,_)=request(&s,"editor","GET",&format!("{draft}/publish-source"),Value::Null).await;assert!(matches!(code,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND));assert_eq!(protected(&s).await,before);
 let revocation_source=call(&s,"editor","GET",&format!("{draft}/save-source"),Value::Null,StatusCode::OK).await;
 let retained_after_revocation=json!({"title":"Retained after revocation","icon":"🛡️","cover_image":"https://example.invalid/revoked-proposal.png","content":"LOCAL WORK SURVIVED REVOCATION Ω"});
 call(&s,"owner","DELETE",&format!("{draft}/access/editor"),Value::Null,StatusCode::NO_CONTENT).await;
 let(code,_)=request(&s,"editor","GET",&draft,Value::Null).await;assert!(matches!(code,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND));
 let(code,_)=request(&s,"editor","PUT",&draft,json!({"source_digest":revocation_source["draft_source_digest"],"title":retained_after_revocation["title"],"icon":retained_after_revocation["icon"],"cover_image":retained_after_revocation["cover_image"],"content":retained_after_revocation["content"]})).await;assert!(matches!(code,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND));
 assert_eq!(call(&s,"owner","GET",&draft,Value::Null,StatusCode::OK).await,winner,"Revoked write changed the protected shared draft");
 publish(&s,id).await;
 assert_asset(&s,"viewer",&referenced,StatusCode::OK,Some(REFERENCED_BYTES)).await;assert_asset(&s,"viewer",&orphan,StatusCode::NOT_FOUND,None).await;
 let backlinks=call(&s,"editor","GET",&format!("/spaces/{SPACE}/documents/{RELATED}/backlinks"),Value::Null,StatusCode::OK).await;assert!(backlinks.as_array().unwrap().iter().any(|row|row["document_id"]==PAGE),"Stable related-page reference is absent after publication");
 let list=format!("{}/publication-snapshots",page());let history=call(&s,"editor","GET",&list,Value::Null,StatusCode::OK).await;let rows=history["snapshots"].as_array().expect("history array");assert_eq!(rows.len(),2);

 // Select by exact portable content instead of assuming timestamp ordering.
 let mut selected=None;for row in rows{let path=format!("{list}/{}",row["id"].as_str().unwrap());let envelope=call(&s,"editor","GET",&path,Value::Null,StatusCode::OK).await;let detail=envelope["snapshot"].clone();if detail["content"]["portable_markdown"]==second{selected=Some((path,detail));}}
 let(path,detail)=selected.expect("actual publication captures exact portable body");let sid=detail["id"].as_str().unwrap();
 assert_eq!(detail["content"]["title"],SECOND_TITLE);assert_eq!(detail["content"]["icon"],SECOND_ICON);assert_eq!(detail["content"]["cover_reference"],SECOND_COVER);
 assert_eq!(detail["comparisons"]["attachments"].as_array().unwrap().iter().map(|row|row["id"].as_str().unwrap()).collect::<Vec<_>>(),vec![asset_id(&referenced)]);
 let(code,raw)=request(&s,"editor","GET",&format!("{path}/raw"),Value::Null).await;assert_eq!(code,StatusCode::OK);let stored:Vec<u8>=sqlx::query_scalar("SELECT raw_markdown FROM document_publication_snapshots WHERE id=$1").bind(sid).fetch_one(&s.db).await.unwrap();assert_eq!(raw,stored);assert_eq!(raw,crate::storage::compose_document_markdown(&crate::spaces::documents::document_storage_frontmatter(SECOND_TITLE,Some(SECOND_ICON),false,false),&second).into_bytes());
 let before=protected(&s).await;let(code,_)=request(&s,"viewer","GET",&path,Value::Null).await;assert!(matches!(code,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND));assert_eq!(protected(&s).await,before);
 let source=call(&s,"editor","GET",&format!("{path}/recovery-source"),Value::Null,StatusCode::OK).await;assert_eq!(source["recoverable"],true);
 let recovery_operation=uuid::Uuid::new_v4().to_string();let intent=json!({"schema_version":1,"operation_id":recovery_operation,"snapshot_source_digest":source["snapshot_source_digest"],"page_source_digest":source["page_source_digest"]});
 let(code,_)=request(&s,"viewer","POST",&format!("{path}/recover"),intent.clone()).await;assert!(matches!(code,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND));assert_eq!(protected(&s).await,before);
 let recovery=call(&s,"editor","POST",&format!("{path}/recover"),intent.clone(),StatusCode::CREATED).await;assert_eq!(protected(&s).await,before);let recovered=recovery["draft"]["id"].as_str().unwrap();assert_eq!(recovery["draft"]["content"],second);assert_eq!(recovery["draft"]["title"],SECOND_TITLE);assert_eq!(recovery["draft"]["icon"],SECOND_ICON);assert_eq!(recovery["draft"]["cover_image"],SECOND_COVER);assert_eq!(recovery["draft"]["owner_id"],"editor");
 let inherited:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_audience_bindings WHERE asset_id=$1 AND binding_kind='draft' AND target_id=$2 AND source_operation_id=$3").bind(asset_id(&referenced)).bind(recovered).bind(&recovery_operation).fetch_one(&s.db).await.unwrap();assert_eq!(inherited,1,"Recovery did not inherit the exact published attachment reference");assert_asset(&s,"editor",&referenced,StatusCode::OK,Some(REFERENCED_BYTES)).await;
 let repeated=call(&s,"editor","POST",&format!("{path}/recover"),intent.clone(),StatusCode::OK).await;assert_eq!(repeated["draft"]["id"],recovered);assert_eq!(protected(&s).await,before);
 let count:i64=sqlx::query_scalar("SELECT count(*) FROM snapshot_recovery_receipts").fetch_one(&s.db).await.unwrap();assert_eq!(count,1);
 let target=format!("{}/drafts/{recovered}",page());call(&s,"editor","PUT",&target,json!({"source_digest":recovery["draft"]["source_digest"],"content":"LATER_RECOVERED_EDIT"}),StatusCode::OK).await;
 call(&s,"editor","POST",&format!("{path}/recover"),intent,StatusCode::CONFLICT).await;assert_eq!(call(&s,"editor","GET",&target,Value::Null,StatusCode::OK).await["content"],"LATER_RECOVERED_EDIT");assert_eq!(protected(&s).await,before);
 let recovery_source=call(&s,"editor","GET",&format!("{target}/save-source"),Value::Null,StatusCode::OK).await;
 let recovered_after_revocation=call(&s,"editor","PUT",&target,json!({"source_digest":recovery_source["draft_source_digest"],"title":retained_after_revocation["title"],"icon":retained_after_revocation["icon"],"cover_image":retained_after_revocation["cover_image"],"content":retained_after_revocation["content"]}),StatusCode::OK).await;
 for field in ["title","icon","cover_image","content"]{assert_eq!(recovered_after_revocation[field],retained_after_revocation[field],"Retained local field {field} was not recovered after access revocation");}
 let reloaded=crate::AppState{db:s.db.clone(),jwt_secret:JWT.into(),base_url:"http://owned.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("reloaded-search-index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
 let published=call(&reloaded,"editor","GET",&format!("{}/content",page()),Value::Null,StatusCode::OK).await;assert_eq!(published["content"].as_str().unwrap().as_bytes(),raw.as_slice());assert_eq!(published["published_metadata"],json!({"title":SECOND_TITLE,"icon":SECOND_ICON,"cover_image":SECOND_COVER}));
 let reloaded_history=call(&reloaded,"editor","GET",&list,Value::Null,StatusCode::OK).await;let reloaded_rows=reloaded_history["snapshots"].as_array().expect("reloaded history array");assert_eq!(reloaded_rows.len(),rows.len());assert!(reloaded_rows.iter().any(|row|row["id"]==sid));
 let reloaded_detail=call(&reloaded,"editor","GET",&path,Value::Null,StatusCode::OK).await;assert_eq!(reloaded_detail["snapshot"],detail);let(code,reloaded_raw)=request(&reloaded,"editor","GET",&format!("{path}/raw"),Value::Null).await;assert_eq!(code,StatusCode::OK);assert_eq!(reloaded_raw,raw);
 let reloaded_recovery=call(&reloaded,"editor","GET",&target,Value::Null,StatusCode::OK).await;for field in ["title","icon","cover_image","content"]{assert_eq!(reloaded_recovery[field],retained_after_revocation[field],"Reloaded recovery field {field} diverged");}assert_asset(&reloaded,"editor",&referenced,StatusCode::OK,Some(REFERENCED_BYTES)).await;
 let engine=if postgres{"postgres"}else{"sqlite"};std::fs::write(owned.join(format!("{engine}-lifecycle-result.json")),json!({"engine":engine,"schema":if postgres{Value::String(schema)}else{Value::Null},"publish_retries":2,"publication_snapshots":rows.len(),"recovery_receipts":count,"stale_save":409,"edited_recovery_retry":409,"source_invariant":true,"metadata_invariant":true,"stable_reference_invariant":true,"referenced_attachment_invariant":true,"private_orphan_invariant":true,"revoked_access_invariant":true,"recoverable_local_work_invariant":true,"reloaded_application_state_invariant":true}).to_string()).unwrap();s.db.close().await;
}


fn css_runtime_cases() -> Vec<(&'static str,String)> {
 let values=vec![("empty",String::new()),("crlf","\r\n.notice {font-weight:700}\r\n".into()),
  ("nul","\n/* \0 retained */ .notice {font-weight:700}\n".into()),
  ("blocked","\n@import 'https://invalid.example/original'; .notice {background:url(https://invalid.example/x)}\n".into()),
  ("oversize",format!("\n/*{}*/\n.notice {{font-weight:700}}\n","x".repeat(20_100))),
  ("guidance-in-css","\r\n<!-- starlane-template:v1:guidance:eyJpZCI6ImNzcyIsInRleHQiOiJPcmlnaW5hbCJ9 -->\r\n.notice {font-weight:700}\r\n".into())];
 values.into_iter().map(|(name,css)|(name,format!("CSS runtime original Ω\n<!-- starlane:custom-css -->{css}<!-- /starlane:custom-css -->\n"))).collect()
}
async fn css_runtime_matrix(postgres:bool){
 let base=crate::storage::data_dir();
 for (name,raw) in css_runtime_cases(){
  let selected=base.join(name);assert!(!selected.exists(),"Each runtime fixture needs a fresh owned root");
  std::env::set_var("DATA_DIR",&selected);
  css_runtime_lifecycle(&raw,postgres).await;
  if postgres && raw.contains('\0') {
   println!("CSS_RUNTIME_NUL_REFUSAL_PASS engine=postgres unchanged_page_draft_publication_history exact_original_proposal_retained");
  } else {
   println!("CSS_RUNTIME_CASE_PASS engine={} case={name} save_reload_publish_history_raw_new_private_recovery_exact",if postgres{"postgres"}else{"sqlite"});
  }
 }
 std::env::set_var("DATA_DIR",base);
}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE/STARLANE_CORE_PG_URL and fresh DATA_DIR; run alone"]
async fn required_postgres_css_runtime_lifecycle_matrix(){css_runtime_matrix(true).await}
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE and fresh DATA_DIR; run alone"]
async fn required_sqlite_css_runtime_lifecycle_matrix(){css_runtime_matrix(false).await}
async fn css_runtime_lifecycle(second: &str, postgres: bool){
 let owned=std::path::PathBuf::from(std::env::var("STARLANE_CORE_WORKSPACE").expect("required existing opt-in workspace"));
 let root=crate::storage::data_dir();assert!(owned.is_absolute()&&root.starts_with(&owned)&&root!=owned);
 std::fs::create_dir_all(&root).unwrap();sqlx::any::install_default_drivers();
 let schema=format!("lifecycle_css_{}",uuid::Uuid::new_v4().simple());
 let db:AnyPool=if postgres {
  let url=std::env::var("STARLANE_CORE_PG_URL").expect("owned PG URL required");assert!(url.starts_with("postgres://"));
  let mut control=sqlx::AnyConnection::connect(&url).await.unwrap();sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut control).await.unwrap();control.close().await.unwrap();
  let selected=schema.clone();sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(move|c,_|{let name=selected.clone();Box::pin(async move{sqlx::query(&format!("SET search_path TO {name}")).execute(c).await?;Ok(())})}).connect(&url).await.unwrap()
 } else {
  sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(|c,_|Box::pin(async move{sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;Ok(())})).connect(&format!("sqlite://{}?mode=rwc",root.join("css-lifecycle.sqlite").display())).await.unwrap()
 };
 crate::db::migrate_selected(&db,if postgres{crate::db::DatabaseEngine::Postgres}else{crate::db::DatabaseEngine::Sqlite}).await.unwrap();
 for actor in ["owner","editor","viewer"]{sqlx::query("INSERT INTO users(id,email,display_name,role,account_kind) VALUES($1,$2,$1,'member','member')").bind(actor).bind(format!("{actor}@life.invalid")).execute(&db).await.unwrap();sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,session_kind) VALUES($1,$2,'not-a-token','2999-01-01T00:00:00Z','browser')").bind(format!("life-{actor}")).bind(actor).execute(&db).await.unwrap();}
 sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,'Owned lifecycle','pg-life','private','owner')").bind(SPACE).execute(&db).await.unwrap();
 for(actor,role)in[("owner","owner"),("editor","editor"),("viewer","viewer")]{sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,$2,$3,$4)").bind(format!("life-member-{actor}")).bind(SPACE).bind(actor).bind(role).execute(&db).await.unwrap();}
 let s=crate::AppState{db,jwt_secret:JWT.into(),base_url:"http://owned.invalid".into(),search_index:crate::search::SearchIndex::new(root.join("search-index")).unwrap(),oauth_cache:crate::auth::routes::OAuthStateCache::new(),ws_rooms:crate::sync::WsRooms::default()};
 let created=call(&s,"owner","POST",&format!("/spaces/{SPACE}/documents"),json!({"source_id":PAGE,"title":"Owned lifecycle","content":FIRST,"property_values":[]}),StatusCode::CREATED).await;
 let first_draft=created["draft_id"].as_str().unwrap();publish(&s,first_draft).await;
 let d=call(&s,"owner","POST",&format!("{}/drafts",page()),json!({}),StatusCode::CREATED).await;let id=d["id"].as_str().unwrap();let draft=format!("{}/drafts/{id}",page());
 call(&s,"owner","PUT",&format!("{draft}/access/editor"),json!({"role":"editor"}),StatusCode::NO_CONTENT).await;
 let src=call(&s,"editor","GET",&format!("{draft}/save-source"),Value::Null,StatusCode::OK).await;
 if postgres && second.contains('\0') {
  let proposal=json!({"source_digest":src["draft_source_digest"],"content":second});
  let before=protected(&s).await;
  let original=call(&s,"owner","GET",&draft,Value::Null,StatusCode::OK).await;
  let receipts:i64=sqlx::query_scalar("SELECT COUNT(*) FROM snapshot_recovery_receipts").fetch_one(&s.db).await.unwrap();
  let (status,_)=request(&s,"editor","PUT",&draft,proposal.clone()).await;
  assert_eq!(status,StatusCode::INTERNAL_SERVER_ERROR,"PostgreSQL TEXT must truthfully refuse original NUL without encoding or stripping");
  assert_eq!(proposal["content"],second,"Original proposal carrier must remain exact");
  assert_eq!(call(&s,"owner","GET",&draft,Value::Null,StatusCode::OK).await,original);
  assert_eq!(protected(&s).await,before,"Rejected NUL save changed page/publication/history");
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM snapshot_recovery_receipts").fetch_one(&s.db).await.unwrap(),receipts);
  s.db.close().await;
  return;
 }
 call(&s,"editor","PUT",&draft,json!({"source_digest":src["draft_source_digest"],"content":second}),StatusCode::OK).await;
 let winner=call(&s,"owner","GET",&draft,Value::Null,StatusCode::OK).await;
 call(&s,"owner","PUT",&draft,json!({"source_digest":src["draft_source_digest"],"content":"STALE_LOSER"}),StatusCode::CONFLICT).await;
 assert_eq!(call(&s,"owner","GET",&draft,Value::Null,StatusCode::OK).await,winner);
 let before=protected(&s).await;let(code,_)=request(&s,"editor","GET",&format!("{draft}/publish-source"),Value::Null).await;assert!(matches!(code,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND));assert_eq!(protected(&s).await,before);
 publish(&s,id).await;
 let list=format!("{}/publication-snapshots",page());let history=call(&s,"editor","GET",&list,Value::Null,StatusCode::OK).await;let rows=history["snapshots"].as_array().expect("history array");assert_eq!(rows.len(),2);

 // Select by exact portable content instead of assuming timestamp ordering.
 let mut selected=None;for row in rows{let path=format!("{list}/{}",row["id"].as_str().unwrap());let envelope=call(&s,"editor","GET",&path,Value::Null,StatusCode::OK).await;let detail=envelope["snapshot"].clone();if detail["content"]["portable_markdown"]==second{selected=Some((path,detail));}}
 let(path,detail)=selected.expect("actual publication captures exact portable body");let sid=detail["id"].as_str().unwrap();
 let(code,raw)=request(&s,"editor","GET",&format!("{path}/raw"),Value::Null).await;assert_eq!(code,StatusCode::OK);let stored:Vec<u8>=sqlx::query_scalar("SELECT raw_markdown FROM document_publication_snapshots WHERE id=$1").bind(sid).fetch_one(&s.db).await.unwrap();assert_eq!(raw,stored);
 let block_start=second.find("<!-- starlane:custom-css").expect("CSS fixture marker");
 let block_end=second.rfind("<!-- /starlane:custom-css -->").expect("CSS fixture closing marker")+"<!-- /starlane:custom-css -->".len();
 let original_span=&second.as_bytes()[block_start..block_end];
 assert!(raw.windows(original_span.len()).any(|span|span==original_span),"Raw published/history snapshot changed original CSS bytes");
assert_eq!(raw,crate::storage::compose_document_markdown(&crate::spaces::documents::document_storage_frontmatter("Owned lifecycle",None,false,false),second).into_bytes());
 let before=protected(&s).await;let(code,_)=request(&s,"viewer","GET",&path,Value::Null).await;assert!(matches!(code,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND));assert_eq!(protected(&s).await,before);
 let source=call(&s,"editor","GET",&format!("{path}/recovery-source"),Value::Null,StatusCode::OK).await;assert_eq!(source["recoverable"],true);
 let intent=json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"snapshot_source_digest":source["snapshot_source_digest"],"page_source_digest":source["page_source_digest"]});
 let(code,_)=request(&s,"viewer","POST",&format!("{path}/recover"),intent.clone()).await;assert!(matches!(code,StatusCode::FORBIDDEN|StatusCode::NOT_FOUND));assert_eq!(protected(&s).await,before);
 let recovery=call(&s,"editor","POST",&format!("{path}/recover"),intent.clone(),StatusCode::CREATED).await;assert_eq!(protected(&s).await,before);let recovered=recovery["draft"]["id"].as_str().unwrap();assert_eq!(recovery["draft"]["content"],second);assert_eq!(recovery["draft"]["owner_id"],"editor");
 let repeated=call(&s,"editor","POST",&format!("{path}/recover"),intent.clone(),StatusCode::OK).await;assert_eq!(repeated["draft"]["id"],recovered);assert_eq!(protected(&s).await,before);
 let count:i64=sqlx::query_scalar("SELECT count(*) FROM snapshot_recovery_receipts").fetch_one(&s.db).await.unwrap();assert_eq!(count,1);
 let target=format!("{}/drafts/{recovered}",page());call(&s,"editor","PUT",&target,json!({"source_digest":recovery["draft"]["source_digest"],"content":"LATER_RECOVERED_EDIT"}),StatusCode::OK).await;
 call(&s,"editor","POST",&format!("{path}/recover"),intent,StatusCode::CONFLICT).await;assert_eq!(call(&s,"editor","GET",&target,Value::Null,StatusCode::OK).await["content"],"LATER_RECOVERED_EDIT");assert_eq!(protected(&s).await,before);
 std::fs::write(owned.join("postgres-lifecycle-result.json"),json!({"schema":schema,"publish_retries":2,"publication_snapshots":rows.len(),"recovery_receipts":count,"stale_save":409,"edited_recovery_retry":409,"source_invariant":true}).to_string()).unwrap();s.db.close().await;
}

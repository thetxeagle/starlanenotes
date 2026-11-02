//! Real authenticated router/multipart journeys: no synthetic audience origins or bindings.
use crate::{AppState, storage};
use axum::{body::{Body,to_bytes}, http::{Request,StatusCode}};
use serde_json::{json,Value};
use tower::ServiceExt;
use crate::spaces::drafts::draft_reader_current_tests as drafts;

const PAGE:&str="/spaces/space-1/documents/doc-1";
struct Fixture {
    state:AppState, slug:String, path:std::path::PathBuf, draft:String,
    _root:crate::storage::fixture_root::FixtureRoot,
}
impl Fixture {
    async fn new()->Self {
        let root=crate::storage::fixture_root::FixtureRoot::private();
        let(state,slug,path)=drafts::fixture().await;
        sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&state.db).await.unwrap();
        let draft=uuid::Uuid::new_v4().to_string();
        clone_draft(&state,&draft).await;
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'editor-1','editor','owner-1')").bind(&draft).execute(&state.db).await.unwrap();
        // Fourth real signed fixture account: shared draft viewer distinct from
        // viewer-1, who retains ordinary page View but no private participation.
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('draft-viewer','draft-viewer@example.test','Draft viewer','member')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('publication-draft-viewer','draft-viewer','hash','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('draft-viewer-membership','space-1','draft-viewer','viewer')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'draft-viewer','viewer','owner-1')").bind(&draft).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('page-editor','page-editor@example.test','Page editor','member')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('publication-page-editor','page-editor','hash','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('page-editor-membership','space-1','page-editor','editor')").execute(&state.db).await.unwrap();
        Self{state,slug,path,draft,_root:root}
    }
    async fn close(self){drafts::cleanup(self.state,&self.slug,&self.path).await;}
}
async fn clone_draft(state:&AppState,id:&str){
    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,icon,cover_image,base_updated_at,created_at,updated_at) SELECT $1,document_id,space_id,owner_id,title,content,icon,cover_image,base_updated_at,created_at,updated_at FROM document_drafts WHERE id='draft-1'")
        .bind(id).execute(&state.db).await.unwrap();
}
async fn request(state:&AppState,user:&str,method:&str,path:&str,body:Option<Value>)->(StatusCode,Vec<u8>){
    let mut builder=Request::builder().method(method).uri(path).header("authorization",format!("Bearer {}",drafts::token(state,user)));
    let body=if let Some(body)=body {builder=builder.header("content-type","application/json");Body::from(body.to_string())}else{Body::empty()};
    let response=crate::api_routes().with_state(state.clone()).oneshot(builder.body(body).unwrap()).await.unwrap();
    let status=response.status();let bytes=to_bytes(response.into_body(),2_000_000).await.unwrap().to_vec();(status,bytes)
}
async fn get(state:&AppState,user:&str,path:&str)->(StatusCode,Value){
    let(status,bytes)=request(state,user,"GET",path,None).await;
    (status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
async fn upload(state:&AppState,user:&str,draft:Option<&str>,operation:&str,body:&[u8],replacement:Option<(&str,&str,i64)>)->(StatusCode,Value){
    let path=replacement.map(|(family,_,_)|format!("{PAGE}/attachment-families/{family}/versions")).unwrap_or_else(||format!("{PAGE}/attachments"));
    let mut builder=Request::builder().method("POST").uri(path)
        .header("authorization",format!("Bearer {}",drafts::token(state,user)))
        .header("content-type","multipart/form-data; boundary=owned-audience-boundary")
        .header("x-starlane-upload-operation",operation).header("x-starlane-upload-length",body.len().to_string());
    if let Some(id)=draft {builder=builder.header("x-starlane-upload-draft",id);}
    if let Some((_,version,revision))=replacement {builder=builder.header("x-starlane-attachment-version",version).header("x-starlane-attachment-revision",revision.to_string());}
    let mut encoded=b"--owned-audience-boundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"private.txt\"\r\nContent-Type: text/plain\r\n\r\n".to_vec();
    encoded.extend_from_slice(body);encoded.extend_from_slice(b"\r\n--owned-audience-boundary--\r\n");
    let response=crate::api_routes().with_state(state.clone()).oneshot(builder.body(Body::from(encoded)).unwrap()).await.unwrap();
    let status=response.status();let bytes=to_bytes(response.into_body(),2_000_000).await.unwrap();
    let selected:Value=serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if replacement.is_some() && status==StatusCode::CREATED {
        // Replacement returns ReplacementChange, not AttachmentResponse. Obtain
        // the actual authorized attachment projection through its real GET API.
        let version=selected["version"]["version_id"].as_str().expect("Actual replacement ACK exact version");
        let(list_status,list)=get(state,user,&format!("{PAGE}/attachments")).await;
        assert_eq!(list_status,StatusCode::OK,"{list}");
        let attachment=list.as_array().unwrap().iter().find(|item|item["id"]==version).expect("Committed replacement visible in authorized actual attachment list").clone();
        return(status,attachment)
    }
    (status,selected)
}
fn id(value:&Value)->&str {value["id"].as_str().unwrap_or_else(||panic!("Expected real upload/copy ACK id; got {value}"))}
fn metadata(asset:&Value)->String {format!("{PAGE}/attachment-versions/{}",id(asset))}
fn bytes_path(asset:&Value)->&str {asset["url"].as_str().unwrap().strip_prefix("/api/v1").unwrap()}
async fn expect_bytes(state:&AppState,user:&str,asset:&Value,expected:&[u8]){
    let(status,body)=request(state,user,"GET",bytes_path(asset),None).await;
    assert_eq!(status,StatusCode::OK,"Authorized exact private bytes user={user}: {}",String::from_utf8_lossy(&body));assert_eq!(body,expected);
}
async fn save_pin(f:&Fixture,asset:&Value)->Value {save_draft_pin(&f.state,&f.draft,asset).await}
async fn save_draft_pin(state:&AppState,selected:&str,asset:&Value)->Value {
    let path=format!("{PAGE}/drafts/{selected}");
    let(status,draft)=get(state,"owner-1",&path).await;assert_eq!(status,StatusCode::OK);
    let(status,bytes)=request(state,"owner-1","PUT",&path,Some(json!({"source_digest":draft["source_digest"],"content":format!("[Exact private asset](starlane-attachment://{})",id(asset))}))).await;
    assert_eq!(status,StatusCode::OK,"Actual draft CAS save: {}",String::from_utf8_lossy(&bytes));
    serde_json::from_slice(&bytes).unwrap()
}

async fn reuse(state:&AppState,draft:&str,operation:&str,asset:&Value)->(StatusCode,Value){
    let(status,bytes)=request(state,"owner-1","POST",&format!("{PAGE}/drafts/{draft}/attachment-bindings"),Some(json!({"schema_version":1,"operation_id":operation,"version_id":id(asset)}))).await;
    (status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}
async fn publish_pin(state:&AppState,draft:&str,asset:&Value){
    save_draft_pin(state,draft,asset).await;
    let(status,review)=get(state,"owner-1",&format!("{PAGE}/drafts/{draft}/publish-source")).await;assert_eq!(status,StatusCode::OK,"{review}");
    let(status,bytes)=request(state,"owner-1","POST",&format!("{PAGE}/drafts/{draft}/publish"),Some(json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"draft_source_digest":review["draft_source_digest"],"page_source_digest":review["page_source_digest"]}))).await;
    assert_eq!(status,StatusCode::OK,"Real owner publication of exact selected pin: {}",String::from_utf8_lossy(&bytes));
}

#[tokio::test]
async fn genuine_multipart_private_audience_and_original_scope_retry_are_exact(){
    let f=Fixture::new().await;
    let before=get(&f.state,"viewer-1",PAGE).await;assert_eq!(before.0,StatusCode::OK);
    let canonical_path:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&f.state.db).await.unwrap();
    let canonical=std::fs::read(storage::data_dir().join(&canonical_path)).unwrap();
    let operation=uuid::Uuid::new_v4().to_string();let payload=b"PRIVATE DRAFT ORIGINAL BINARY";
    let(status,asset)=upload(&f.state,"owner-1",Some(&f.draft),&operation,payload,None).await;assert_eq!(status,StatusCode::CREATED,"{asset}");
    assert_eq!(asset["audience"]["schema_version"],2);assert_eq!(asset["audience"]["draft_id"],f.draft);
    for user in ["owner-1","editor-1","draft-viewer"] {
        assert_eq!(get(&f.state,user,&metadata(&asset)).await.0,StatusCode::OK);expect_bytes(&f.state,user,&asset,payload).await;
    }
    assert_eq!(get(&f.state,"viewer-1",&format!("{PAGE}/drafts/{}",f.draft)).await.0,StatusCode::NOT_FOUND);
    assert_eq!(get(&f.state,"viewer-1",&metadata(&asset)).await.0,StatusCode::NOT_FOUND);
    assert_eq!(request(&f.state,"viewer-1","GET",bytes_path(&asset),None).await.0,StatusCode::NOT_FOUND);
    let(status,list)=get(&f.state,"viewer-1",&format!("{PAGE}/attachments")).await;assert_eq!(status,StatusCode::OK);assert_eq!(list,json!([]));
    assert_eq!(get(&f.state,"viewer-1","/spaces/space-1/attachments").await.0,StatusCode::NOT_FOUND,"Space library retains its existing editor restriction");
    assert_eq!(get(&f.state,"page-editor",PAGE).await.0,StatusCode::OK);
    assert_eq!(get(&f.state,"page-editor",&format!("{PAGE}/drafts/{}",f.draft)).await.0,StatusCode::NOT_FOUND);
    let(status,list)=get(&f.state,"page-editor","/spaces/space-1/attachments").await;assert_eq!(status,StatusCode::OK);assert_eq!(list,json!([]),"Page-only editor library cannot disclose another private draft upload");
    let saved=save_pin(&f,&asset).await;assert!(saved["content"].as_str().unwrap().contains(id(&asset)));
    assert_eq!(get(&f.state,"viewer-1",PAGE).await,before,"Uploading and saving a private pin cannot change published projection");
    assert_eq!(std::fs::read(storage::data_dir().join(canonical_path)).unwrap(),canonical);
    let(status,retry)=upload(&f.state,"owner-1",Some(&f.draft),&operation,payload,None).await;assert_eq!(status,StatusCode::CREATED,"{retry}");assert_eq!(retry,asset);
    let other=uuid::Uuid::new_v4().to_string();clone_draft(&f.state,&other).await;
    let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM attachment_upload_receipts").fetch_one(&f.state.db).await.unwrap();
    let(status,_)=upload(&f.state,"owner-1",Some(&other),&operation,payload,None).await;assert_eq!(status,StatusCode::CONFLICT);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM attachment_upload_receipts").fetch_one(&f.state.db).await.unwrap(),count);
    let(status,_)=upload(&f.state,"draft-viewer",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"viewer cannot upload",None).await;assert!(!status.is_success());
    let(status,legacy)=upload(&f.state,"owner-1",None,&uuid::Uuid::new_v4().to_string(),b"EXPLICIT LEGACY PAGE ASSET",None).await;assert_eq!(status,StatusCode::CREATED,"{legacy}");
    assert!(legacy.get("audience").is_none());expect_bytes(&f.state,"viewer-1",&legacy,b"EXPLICIT LEGACY PAGE ASSET").await;
    f.close().await;
}

#[tokio::test]
async fn genuine_replacements_hide_private_first_and_head_from_history_totals_and_cursors(){
    let f=Fixture::new().await;
    let(status,first)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"PRIVATE FIRST",None).await;assert_eq!(status,StatusCode::CREATED,"{first}");
    let visible_draft=uuid::Uuid::new_v4().to_string();clone_draft(&f.state,&visible_draft).await;
    sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'viewer-1','viewer','owner-1')").bind(&visible_draft).execute(&f.state.db).await.unwrap();
    let family=first["logical_attachment_id"].as_str().unwrap();
    let reuse_operation=uuid::Uuid::new_v4().to_string();
    let(status,binding)=reuse(&f.state,&visible_draft,&reuse_operation,&first).await;assert_eq!(status,StatusCode::OK,"{binding}");
    assert_eq!(binding["version_id"],id(&first));assert_eq!(binding["draft_id"],visible_draft);
    let(status,retry)=reuse(&f.state,&visible_draft,&reuse_operation,&first).await;assert_eq!(status,StatusCode::OK,"{retry}");assert_eq!(retry,binding,"Exact reviewed reuse retry preserves its original binding receipt");
    let wrong_target=uuid::Uuid::new_v4().to_string();clone_draft(&f.state,&wrong_target).await;
    assert_eq!(reuse(&f.state,&wrong_target,&reuse_operation,&first).await.0,StatusCode::CONFLICT,"Original reuse operation cannot authorize another target draft");
    let(status,second)=upload(&f.state,"owner-1",Some(&visible_draft),&uuid::Uuid::new_v4().to_string(),b"VISIBLE SECOND",Some((family,id(&first),1))).await;assert_eq!(status,StatusCode::CREATED,"{second}");
    assert_eq!(reuse(&f.state,&visible_draft,&reuse_operation,&second).await.0,StatusCode::CONFLICT,"Original reuse cannot change the reviewed exact source version");
    let(status,before_publication)=get(&f.state,"viewer-1",&format!("{PAGE}/attachment-families/{family}/versions")).await;assert_eq!(status,StatusCode::OK,"{before_publication}");assert_eq!(before_publication["total"],2,"Both source and replacement are legitimately shared while B exists");
    publish_pin(&f.state,&visible_draft,&second).await;
    assert_eq!(get(&f.state,"owner-1",&format!("{PAGE}/drafts/{visible_draft}")).await.0,StatusCode::NOT_FOUND);
    let(status,visible_head)=get(&f.state,"viewer-1",&format!("{PAGE}/attachment-families/{family}/versions")).await;
    assert_eq!(status,StatusCode::OK,"{visible_head}");assert_eq!(visible_head["schema_version"],2);assert_eq!(visible_head["total"],1);
    assert_eq!(visible_head["current_head"],json!({"version_id":id(&second),"revision":2}),"Visible count is not the physical family revision");
    let(status,third)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"PRIVATE HEAD",Some((family,id(&second),2))).await;assert_eq!(status,StatusCode::CREATED,"{third}");
    let path=format!("{PAGE}/attachment-families/{family}/versions?limit=1");
    let(status,history)=get(&f.state,"viewer-1",&path).await;assert_eq!(status,StatusCode::OK,"{history}");
    assert_eq!(history["schema_version"],2);assert!(history["current_head"].is_null(),"Private head exposes neither ID nor revision");
    assert_eq!(history["total"],1);assert_eq!(history["versions"].as_array().unwrap().len(),1);
    assert_eq!(history["versions"][0]["version_id"],id(&second));assert_eq!(history["versions"][0]["version_number"],2);assert!(history["next_after"].is_null());
    // The stable logical family ID legitimately equals the first asset UUID.
    // Deny private version records/retirement projections, not that family ID.
    for private in [&first,&third] {
        assert!(history["versions"].as_array().unwrap().iter().all(|v|v["version_id"]!=id(private)));
        assert!(history["retained_versions"].as_object().unwrap().get(id(private)).is_none());
    }
    assert!(!history.to_string().contains(id(&third)),"Private head identity must not occur in any projection");
    for private in [&first,&third] {assert_eq!(get(&f.state,"viewer-1",&metadata(private)).await.0,StatusCode::NOT_FOUND);assert_eq!(request(&f.state,"viewer-1","GET",bytes_path(private),None).await.0,StatusCode::NOT_FOUND);}
    expect_bytes(&f.state,"viewer-1",&second,b"VISIBLE SECOND").await;
    let(status,owner)=get(&f.state,"owner-1",&format!("{PAGE}/attachment-families/{family}/versions")).await;assert_eq!(status,StatusCode::OK,"{owner}");assert_eq!(owner["total"],3);
    let(status,tail)=get(&f.state,"viewer-1",&format!("{PAGE}/attachment-families/{family}/versions?after=2&limit=1")).await;assert_eq!(status,StatusCode::OK);assert_eq!(tail["total"],1);assert_eq!(tail["versions"],json!([]));assert!(tail["next_after"].is_null());
    // A separate family covers the opposite boundary: visible v1, private v2.
    let another_draft=uuid::Uuid::new_v4().to_string();clone_draft(&f.state,&another_draft).await;
    sqlx::query("UPDATE document_drafts SET base_updated_at=(SELECT updated_at FROM documents WHERE id='doc-1') WHERE id=$1").bind(&another_draft).execute(&f.state.db).await.unwrap();
    let(status,visible_first)=upload(&f.state,"owner-1",Some(&another_draft),&uuid::Uuid::new_v4().to_string(),b"VISIBLE FIRST",None).await;assert_eq!(status,StatusCode::CREATED,"{visible_first}");
    let second_family=visible_first["logical_attachment_id"].as_str().unwrap();
    let(status,binding)=reuse(&f.state,&f.draft,&uuid::Uuid::new_v4().to_string(),&visible_first).await;assert_eq!(status,StatusCode::OK,"{binding}");
    let(status,hidden_head)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"HIDDEN SECOND",Some((second_family,id(&visible_first),1))).await;assert_eq!(status,StatusCode::CREATED,"{hidden_head}");
    publish_pin(&f.state,&another_draft,&visible_first).await;
    let(status,opposite)=get(&f.state,"viewer-1",&format!("{PAGE}/attachment-families/{second_family}/versions")).await;assert_eq!(status,StatusCode::OK,"{opposite}");
    assert_eq!(opposite["total"],1);assert!(opposite["current_head"].is_null());assert_eq!(opposite["versions"][0]["version_id"],id(&visible_first));assert!(!opposite.to_string().contains(id(&hidden_head)));
    f.close().await;
}

#[tokio::test]
async fn actual_metadata_and_download_recheck_revocation_after_capture(){
    let f=Fixture::new().await;
    let(status,asset)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"PRIVATE REVOKED SOURCE",None).await;assert_eq!(status,StatusCode::CREATED,"{asset}");
    let db=f.state.db.clone();let draft=f.draft.clone();
    let(status,_)=storage::attachment_versions::HISTORY_BEFORE_FINAL.scope(std::cell::RefCell::new(Some(Box::pin(async move {
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id=$1 AND user_id='editor-1'").bind(draft).execute(&db).await.unwrap();
    }))),get(&f.state,"editor-1",&metadata(&asset))).await;
    assert_eq!(status,StatusCode::NOT_FOUND,"Late metadata revocation must not return captured metadata");
    sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'editor-1','editor','owner-1')").bind(&f.draft).execute(&f.state.db).await.unwrap();
    let db=f.state.db.clone();let draft=f.draft.clone();
    let family=asset["logical_attachment_id"].as_str().unwrap();let path=format!("{PAGE}/attachment-families/{family}/versions/{}/file",id(&asset));
    let(status,body)=storage::attachment_versions::HISTORY_DOWNLOAD_BEFORE_FINAL.scope(std::cell::RefCell::new(Some(Box::pin(async move {
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id=$1 AND user_id='editor-1'").bind(draft).execute(&db).await.unwrap();
    }))),request(&f.state,"editor-1","GET",&path,None)).await;
    assert_eq!(status,StatusCode::NOT_FOUND,"Late download revocation must discard captured bytes");assert!(!body.windows(b"PRIVATE REVOKED SOURCE".len()).any(|w|w==b"PRIVATE REVOKED SOURCE"));
    expect_bytes(&f.state,"owner-1",&asset,b"PRIVATE REVOKED SOURCE").await;
    f.close().await;
}

#[tokio::test]
async fn actual_owner_publication_promotes_only_referenced_exact_pin_and_retains_private_orphan(){
    let f=Fixture::new().await;
    let(status,published)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"EXACT PUBLISHED PIN",None).await;assert_eq!(status,StatusCode::CREATED,"{published}");
    let(status,orphan)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"PRIVATE UNREFERENCED UPLOAD",None).await;assert_eq!(status,StatusCode::CREATED,"{orphan}");
    save_pin(&f,&published).await;
    let(status,review)=get(&f.state,"owner-1",&format!("{PAGE}/drafts/{}/publish-source",f.draft)).await;assert_eq!(status,StatusCode::OK,"{review}");
    let operation=uuid::Uuid::new_v4().to_string();
    let(status,bytes)=request(&f.state,"owner-1","POST",&format!("{PAGE}/drafts/{}/publish",f.draft),Some(json!({"schema_version":1,"operation_id":operation,"draft_source_digest":review["draft_source_digest"],"page_source_digest":review["page_source_digest"]}))).await;
    assert_eq!(status,StatusCode::OK,"Actual owner publication: {}",String::from_utf8_lossy(&bytes));
    assert_eq!(get(&f.state,"owner-1",&format!("{PAGE}/drafts/{}",f.draft)).await.0,StatusCode::NOT_FOUND);
    expect_bytes(&f.state,"viewer-1",&published,b"EXACT PUBLISHED PIN").await;
    assert_eq!(get(&f.state,"viewer-1",&metadata(&published)).await.0,StatusCode::OK);
    assert_eq!(get(&f.state,"viewer-1",&metadata(&orphan)).await.0,StatusCode::NOT_FOUND);
    assert_eq!(request(&f.state,"viewer-1","GET",bytes_path(&orphan),None).await.0,StatusCode::NOT_FOUND);
    let mut conn=f.state.db.acquire().await.unwrap();
    let original=storage::attachment_upload::receipt(&mut conn,&sqlx::query_scalar::<_,String>("SELECT operation_id FROM attachment_upload_receipts WHERE asset_id=$1").bind(id(&orphan)).fetch_one(&f.state.db).await.unwrap()).await.unwrap().unwrap();
    assert!(storage::attachment_upload::verify(&storage::data_dir(),&original,false).unwrap(),"Unreferenced private original receipt and bytes remain retained after draft consumption");
    let grants:Vec<String>=sqlx::query_scalar("SELECT asset_id FROM attachment_audience_bindings WHERE binding_kind='publication' ORDER BY asset_id").fetch_all(&mut *conn).await.unwrap();assert_eq!(grants,vec![id(&published).to_owned()]);
    drop(conn);f.close().await;
}

/// Uses the documented explicitly owned PostgreSQL gate, with a unique schema.
/// Auth identities are fictional signed fixtures; every asset/audience receipt
/// below is produced by the real multipart and publication routes.
#[tokio::test]
#[ignore="requires owned STARLANE_CORE_WORKSPACE, STARLANE_CORE_PG_URL and DATA_DIR; run alone"]
async fn required_postgres_actual_multipart_audience_publication_and_private_history(){
    use sqlx::Connection;
    sqlx::any::install_default_drivers();
    let owned=std::path::PathBuf::from(std::env::var("STARLANE_CORE_WORKSPACE").expect("owned workspace required"));
    let root=storage::data_dir();assert!(owned.is_absolute()&&root.starts_with(&owned)&&root!=owned);
    let url=std::env::var("STARLANE_CORE_PG_URL").expect("owned PostgreSQL URL required");assert!(url.starts_with("postgres://"));
    let schema=format!("asset_audience_{}",uuid::Uuid::new_v4().simple());
    let mut control=sqlx::AnyConnection::connect(&url).await.unwrap();
    sqlx::query(&format!("CREATE SCHEMA {schema}")).execute(&mut control).await.unwrap();
    let selected=schema.clone();
    let pool=sqlx::any::AnyPoolOptions::new().max_connections(4).after_connect(move|conn,_|{let schema=selected.clone();Box::pin(async move{sqlx::query(&format!("SET search_path TO {schema}")).execute(conn).await?;Ok(())})}).connect(&url).await.unwrap();
    crate::db::migrate_selected(&pool,crate::db::DatabaseEngine::Postgres).await.unwrap();
    assert_eq!(sqlx::query_scalar::<_,String>("SELECT current_schema()::text").fetch_one(&pool).await.unwrap(),schema);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT max(version) FROM _sqlx_migrations").fetch_one(&pool).await.unwrap(),9);
    let mut state=crate::spaces::properties::tests::property_test_state().await;state.db.close().await;state.db=pool;
    let slug=format!("pg-audience-{}",uuid::Uuid::new_v4().simple());
    for(user,role)in[("owner-1","owner"),("editor-1","editor"),("viewer-1","viewer")]{
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(user).bind(format!("{user}@audience.test.invalid")).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'fixture','2999-01-01T00:00:00Z')").bind(format!("publication-{user}")).bind(user).execute(&state.db).await.unwrap();
        if user=="owner-1"{sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-1','PG audience',$1,'private','owner-1')").bind(&slug).execute(&state.db).await.unwrap();}
        sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-1',$2,$3)").bind(format!("membership-{user}")).bind(user).bind(role).execute(&state.db).await.unwrap();
    }
    let path=storage::write_document(&slug,"doc-1","Published title","","PG ORIGINAL PUBLISHED BODY").await.unwrap();
    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,created_at,updated_at,published_at,public) VALUES('doc-1','space-1','Published title',$1,'owner-1','owner-1','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',0)").bind(&path).execute(&state.db).await.unwrap();
    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES('draft-1','doc-1','space-1','owner-1','Draft title','PG private body','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
    let draft=uuid::Uuid::new_v4().to_string();clone_draft(&state,&draft).await;
    sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'editor-1','editor','owner-1')").bind(&draft).execute(&state.db).await.unwrap();
    let canonical=std::fs::read(root.join(&path)).unwrap();
    let(status,asset)=upload(&state,"owner-1",Some(&draft),&uuid::Uuid::new_v4().to_string(),b"PG EXACT PUBLISHED PIN",None).await;assert_eq!(status,StatusCode::CREATED,"{asset}");
    let(status,orphan)=upload(&state,"owner-1",Some(&draft),&uuid::Uuid::new_v4().to_string(),b"PG PRIVATE ORPHAN",None).await;assert_eq!(status,StatusCode::CREATED,"{orphan}");
    assert_eq!(get(&state,"viewer-1",PAGE).await.0,StatusCode::OK);
    assert_eq!(get(&state,"viewer-1",&metadata(&asset)).await.0,StatusCode::NOT_FOUND);
    expect_bytes(&state,"editor-1",&asset,b"PG EXACT PUBLISHED PIN").await;
    assert_eq!(std::fs::read(root.join(&path)).unwrap(),canonical,"Private multipart must not publish the page");
    publish_pin(&state,&draft,&asset).await;
    expect_bytes(&state,"viewer-1",&asset,b"PG EXACT PUBLISHED PIN").await;
    assert_eq!(get(&state,"viewer-1",&metadata(&orphan)).await.0,StatusCode::NOT_FOUND);
    assert_eq!(request(&state,"viewer-1","GET",bytes_path(&orphan),None).await.0,StatusCode::NOT_FOUND);
    let fresh=uuid::Uuid::new_v4().to_string();clone_draft(&state,&fresh).await;
    let family=asset["logical_attachment_id"].as_str().unwrap();
    let(status,private)=upload(&state,"owner-1",Some(&fresh),&uuid::Uuid::new_v4().to_string(),b"PG PRIVATE REPLACEMENT",Some((family,id(&asset),1))).await;assert_eq!(status,StatusCode::CREATED,"{private}");
    let(status,history)=get(&state,"viewer-1",&format!("{PAGE}/attachment-families/{family}/versions?limit=1")).await;assert_eq!(status,StatusCode::OK,"{history}");
    assert_eq!(history["schema_version"],2);assert_eq!(history["total"],1);assert!(history["current_head"].is_null());assert!(history["next_after"].is_null());
    assert_eq!(history["versions"][0]["version_id"],id(&asset));assert!(!history.to_string().contains(id(&private)));
    assert_eq!(get(&state,"viewer-1",&metadata(&private)).await.0,StatusCode::NOT_FOUND);
    expect_bytes(&state,"owner-1",&private,b"PG PRIVATE REPLACEMENT").await;
    let(_,owner_history)=get(&state,"owner-1",&format!("{PAGE}/attachment-families/{family}/versions")).await;assert_eq!(owner_history["total"],2);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_audience_origins").fetch_one(&state.db).await.unwrap(),3);
    state.db.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE")).execute(&mut control).await.unwrap();
    tokio::fs::remove_dir_all(storage::space_dir(&slug)).await.unwrap();
}

#[tokio::test]
async fn actual_registered_copy_is_independent_private_and_original_retry_survives_source_revoke(){
    let f=Fixture::new().await;
    let payload=b"EXACT CROSS PAGE COPY ORIGINAL";
    let(status,source)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),payload,None).await;assert_eq!(status,StatusCode::CREATED,"{source}");
    let(status,review)=get(&f.state,"editor-1",&metadata(&source)).await;assert_eq!(status,StatusCode::OK,"{review}");
    let original=review["version"].clone();assert_eq!(original["version_id"],id(&source));
    // A distinct real canonical page and editor-owned private draft; no grant is
    // inserted into either source or target asset audience tables by the test.
    let target_path=storage::write_document(&f.slug,"copy-target","Copy target","","COPY TARGET PUBLISHED").await.unwrap();
    sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('copy-target','space-1','Copy target',$1,'owner-1','owner-1','published')").bind(&target_path).execute(&f.state.db).await.unwrap();
    let target=uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES($1,'copy-target','space-1','editor-1','Copy target','UNCHANGED TARGET BODY',(SELECT updated_at FROM documents WHERE id='copy-target'),'2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')").bind(&target).execute(&f.state.db).await.unwrap();
    let operation=uuid::Uuid::new_v4().to_string();
    let body=json!({"schema_version":1,"operation_id":operation,"source_document_id":"doc-1","version_id":id(&source),"expected_source_version":original});
    let path=format!("/spaces/space-1/documents/copy-target/drafts/{target}/attachments/copy");
    let(status,raw)=request(&f.state,"editor-1","POST",&path,Some(body.clone())).await;assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&raw));
    let copied:Value=serde_json::from_slice(&raw).unwrap();assert_ne!(id(&copied),id(&source));assert!(uuid::Uuid::parse_str(id(&copied)).is_ok());assert_eq!(copied["document_id"],"copy-target");assert_eq!(copied["audience"]["draft_id"],target);
    expect_bytes(&f.state,"editor-1",&copied,payload).await;
    assert_eq!(request(&f.state,"viewer-1","GET",bytes_path(&copied),None).await.0,StatusCode::NOT_FOUND);
    let association:(String,String)=sqlx::query_as("SELECT document_id,storage_path FROM attachments WHERE id=$1").bind(id(&source)).fetch_one(&f.state.db).await.unwrap();assert_eq!(association.0,"doc-1");
    assert_eq!(get(&f.state,"owner-1",&metadata(&source)).await.1,review,"Copy must not rewrite original version or association");
    let copied_meta=format!("/spaces/space-1/documents/copy-target/attachment-versions/{}",id(&copied));
    let(status,copy_review)=get(&f.state,"editor-1",&copied_meta).await;assert_eq!(status,StatusCode::OK,"{copy_review}");assert_eq!(copy_review["version"]["version_number"],1);
    let unchanged:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id=$1").bind(&target).fetch_one(&f.state.db).await.unwrap();assert_eq!(unchanged,"UNCHANGED TARGET BODY");
    let mut same_page_request=body.clone();same_page_request["operation_id"]=json!(uuid::Uuid::new_v4().to_string());
    let(status,raw)=request(&f.state,"editor-1","POST",&format!("{PAGE}/drafts/{}/attachments/copy",f.draft),Some(same_page_request)).await;assert_eq!(status,StatusCode::CREATED,"Registered same-page copy: {}",String::from_utf8_lossy(&raw));
    let same_page:Value=serde_json::from_slice(&raw).unwrap();assert_ne!(id(&same_page),id(&source));assert_ne!(id(&same_page),id(&copied));assert_eq!(same_page["document_id"],"doc-1");assert_eq!(same_page["audience"]["draft_id"],f.draft);
    expect_bytes(&f.state,"editor-1",&same_page,payload).await;
    let(status,same_meta)=get(&f.state,"editor-1",&metadata(&same_page)).await;assert_eq!(status,StatusCode::OK);assert_eq!(same_meta["version"]["version_number"],1);
    assert_eq!(get(&f.state,"viewer-1",&metadata(&same_page)).await.0,StatusCode::NOT_FOUND);
    assert_eq!(get(&f.state,"owner-1",&metadata(&source)).await.1,review,"Same-page copy retains original immutable version and association");
    let before:i64=sqlx::query_scalar("SELECT count(*) FROM attachments").fetch_one(&f.state.db).await.unwrap();
    sqlx::query("DELETE FROM document_draft_access WHERE draft_id=$1 AND user_id='editor-1'").bind(&f.draft).execute(&f.state.db).await.unwrap();
    assert_eq!(get(&f.state,"editor-1",&metadata(&source)).await.0,StatusCode::NOT_FOUND);
    let(status,retry)=request(&f.state,"editor-1","POST",&path,Some(body.clone())).await;assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&retry));assert_eq!(serde_json::from_slice::<Value>(&retry).unwrap(),copied,"Original completed copy uses its own exact receipt and current target authority");
    let mut changed=body.clone();changed["expected_source_version"]["binary_sha256"]=json!("f".repeat(64));assert_eq!(request(&f.state,"editor-1","POST",&path,Some(changed)).await.0,StatusCode::CONFLICT);
    let mut new_operation=body;new_operation["operation_id"]=json!(uuid::Uuid::new_v4().to_string());assert_eq!(request(&f.state,"editor-1","POST",&path,Some(new_operation)).await.0,StatusCode::NOT_FOUND);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachments").fetch_one(&f.state.db).await.unwrap(),before);
    expect_bytes(&f.state,"editor-1",&copied,payload).await;
    f.close().await;
}

#[tokio::test]
async fn actual_legacy_copy_review_preserves_opaque_source_without_inventing_version(){
    let f=Fixture::new().await;
    // Explicit old page-owned producer fixture: an unregistered asset with an
    // opaque historical ID. The proof under test comes from the real GET route.
    let relative=format!("spaces/{}/attachments/doc-1/old-opaque.asset",f.slug);
    let full=storage::data_dir().join(&relative);std::fs::create_dir_all(full.parent().unwrap()).unwrap();std::fs::write(&full,b"ACTUAL LEGACY OPAQUE COPY").unwrap();
    sqlx::query("INSERT INTO attachments(id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES('old-opaque.asset','doc-1','space-1','old-opaque.asset','legacy.bin','application/octet-stream',25,$1,'owner-1')").bind(&relative).execute(&f.state.db).await.unwrap();
    let mut conn=f.state.db.acquire().await.unwrap();crate::spaces::asset_audiences::admit_legacy(&mut conn,"old-opaque.asset","space-1","doc-1").await.unwrap();drop(conn);
    assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM attachment_version_records WHERE version_id='old-opaque.asset'").fetch_one(&f.state.db).await.unwrap(),0);
    let(status,proof)=get(&f.state,"owner-1",&format!("{PAGE}/attachment-copy-source/old-opaque.asset")).await;assert_eq!(status,StatusCode::OK,"{proof}");assert_eq!(proof["kind"],"legacy_snapshot_v1");assert_eq!(proof["asset_id"],"old-opaque.asset");
    let body=json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"source_document_id":"doc-1","version_id":"old-opaque.asset","expected_legacy_source":proof});
    let(status,raw)=request(&f.state,"owner-1","POST",&format!("{PAGE}/drafts/{}/attachments/copy",f.draft),Some(body.clone())).await;assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&raw));
    let copied:Value=serde_json::from_slice(&raw).unwrap();assert_ne!(id(&copied),"old-opaque.asset");assert!(uuid::Uuid::parse_str(id(&copied)).is_ok());expect_bytes(&f.state,"owner-1",&copied,b"ACTUAL LEGACY OPAQUE COPY").await;
    assert_eq!(get(&f.state,"viewer-1",&metadata(&copied)).await.0,StatusCode::NOT_FOUND);
    assert_eq!(std::fs::read(&full).unwrap(),b"ACTUAL LEGACY OPAQUE COPY");
    let mut changed=body;changed["expected_legacy_source"]["original_name"]=json!("changed.bin");assert_eq!(request(&f.state,"owner-1","POST",&format!("{PAGE}/drafts/{}/attachments/copy",f.draft),Some(changed)).await.0,StatusCode::CONFLICT);
    f.close().await;
}

async fn birth_from_published(state:&AppState,user:&str)->Value{
    let(status,raw)=request(state,user,"POST",&format!("{PAGE}/drafts"),Some(json!({"force_new":true,"operation_id":uuid::Uuid::new_v4().to_string()}))).await;
    assert_eq!(status,StatusCode::CREATED,"Real published draft birth: {}",String::from_utf8_lossy(&raw));serde_json::from_slice(&raw).unwrap()
}
async fn publish_selected(state:&AppState,user:&str,selected:&str)->(StatusCode,Value){
    let path=format!("{PAGE}/drafts/{selected}");
    let(status,review)=get(state,user,&format!("{path}/publish-source")).await;
    if status!=StatusCode::OK{return(status,review)}
    let(status,raw)=request(state,user,"POST",&format!("{path}/publish"),Some(json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"draft_source_digest":review["draft_source_digest"],"page_source_digest":review["page_source_digest"]}))).await;
    (status,serde_json::from_slice(&raw).unwrap_or(Value::Null))
}

#[tokio::test]
async fn actual_published_birth_and_older_snapshot_recovery_inherit_only_exact_authorized_pins(){
    let f=Fixture::new().await;
    let hidden_draft=uuid::Uuid::new_v4().to_string();clone_draft(&f.state,&hidden_draft).await;
    let(status,hidden)=upload(&f.state,"owner-1",Some(&hidden_draft),&uuid::Uuid::new_v4().to_string(),b"UNRELATED PRIVATE PIN",None).await;assert_eq!(status,StatusCode::CREATED,"{hidden}");
    let(status,pin)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"EXACT OLD SNAPSHOT PIN",None).await;assert_eq!(status,StatusCode::CREATED,"{pin}");
    publish_pin(&f.state,&f.draft,&pin).await;
    let snapshot:String=sqlx::query_scalar("SELECT id FROM document_publication_snapshots WHERE document_id='doc-1' AND kind='publication'").fetch_one(&f.state.db).await.unwrap();
    let born=birth_from_published(&f.state,"editor-1").await;let born_id=born["id"].as_str().unwrap();
    assert!(born["content"].as_str().unwrap().contains(id(&pin)));
    expect_bytes(&f.state,"editor-1",&pin,b"EXACT OLD SNAPSHOT PIN").await;
    let born_bindings:Vec<String>=sqlx::query_scalar("SELECT asset_id FROM attachment_audience_bindings WHERE binding_kind='draft' AND target_id=$1 ORDER BY asset_id").bind(born_id).fetch_all(&f.state.db).await.unwrap();assert_eq!(born_bindings,vec![id(&pin).to_string()]);
    let later=birth_from_published(&f.state,"owner-1").await;let later_id=later["id"].as_str().unwrap();
    let(status,raw)=request(&f.state,"owner-1","PUT",&format!("{PAGE}/drafts/{later_id}"),Some(json!({"source_digest":later["source_digest"],"content":"LATER PUBLICATION WITHOUT OLD PIN"}))).await;assert_eq!(status,StatusCode::OK,"{}",String::from_utf8_lossy(&raw));
    let(status,result)=publish_selected(&f.state,"owner-1",later_id).await;assert_eq!(status,StatusCode::OK,"{result}");
    let published_before=get(&f.state,"viewer-1",PAGE).await;assert_eq!(published_before.0,StatusCode::OK);
    let published_path:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&f.state.db).await.unwrap();
    let published_bytes=std::fs::read(storage::data_dir().join(&published_path)).unwrap();
    assert!(String::from_utf8_lossy(&published_bytes).contains("LATER PUBLICATION WITHOUT OLD PIN"));
    let prefix=format!("{PAGE}/publication-snapshots/{snapshot}");
    let(status,review)=get(&f.state,"editor-1",&format!("{prefix}/recovery-source")).await;assert_eq!(status,StatusCode::OK,"{review}");
    let(status,raw)=request(&f.state,"editor-1","POST",&format!("{prefix}/recover"),Some(json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"snapshot_source_digest":review["snapshot_source_digest"],"page_source_digest":review["page_source_digest"]}))).await;assert_eq!(status,StatusCode::CREATED,"{}",String::from_utf8_lossy(&raw));
    let recovered:Value=serde_json::from_slice(&raw).unwrap();let recovered_id=recovered["draft"]["id"].as_str().unwrap();assert_ne!(recovered_id,born_id);assert!(recovered["draft"]["content"].as_str().unwrap().contains(id(&pin)));
    let recovered_bindings:Vec<String>=sqlx::query_scalar("SELECT asset_id FROM attachment_audience_bindings WHERE binding_kind='draft' AND target_id=$1 ORDER BY asset_id").bind(recovered_id).fetch_all(&f.state.db).await.unwrap();assert_eq!(recovered_bindings,vec![id(&pin).to_string()]);
    expect_bytes(&f.state,"editor-1",&pin,b"EXACT OLD SNAPSHOT PIN").await;
    assert_eq!(get(&f.state,"editor-1",&metadata(&hidden)).await.0,StatusCode::NOT_FOUND);
    let current=birth_from_published(&f.state,"editor-1").await;let guessed_id=current["id"].as_str().unwrap();
    let(ready,review)=get(&f.state,"editor-1",&format!("{PAGE}/drafts/{guessed_id}/publish-source")).await;assert_eq!(ready,StatusCode::OK,"Positive current authorized publish-source before introducing guessed pin: {review}");
    let(status,_)=request(&f.state,"editor-1","POST",&format!("{PAGE}/drafts/{guessed_id}/attachment-bindings"),Some(json!({"schema_version":1,"operation_id":uuid::Uuid::new_v4().to_string(),"version_id":id(&hidden)}))).await;assert_eq!(status,StatusCode::NOT_FOUND,"Guessed pin cannot acquire a private binding");
    let(status,raw)=request(&f.state,"editor-1","PUT",&format!("{PAGE}/drafts/{guessed_id}"),Some(json!({"source_digest":current["source_digest"],"content":format!("[Guessed](starlane-attachment://{})",id(&hidden))}))).await;
    if status==StatusCode::OK{let(status,result)=publish_selected(&f.state,"editor-1",guessed_id).await;assert_eq!(status,StatusCode::NOT_FOUND,"Current otherwise authorized draft must refuse unavailable guessed private pin: {result}");}
    else{assert!(!status.is_success(),"{}",String::from_utf8_lossy(&raw));}
    assert_eq!(get(&f.state,"viewer-1",&metadata(&hidden)).await.0,StatusCode::NOT_FOUND);
    assert_eq!(get(&f.state,"viewer-1",PAGE).await,published_before,"Recovery and refused guessed-pin publication preserve the exact canonical metadata projection");
    assert_eq!(std::fs::read(storage::data_dir().join(&published_path)).unwrap(),published_bytes,"Exact original canonical Markdown bytes remain unchanged");
    f.close().await;
}

#[tokio::test]
async fn actual_published_pin_private_binding_allows_current_edit_without_canonical_view(){
    let f=Fixture::new().await;
    let(status,pin)=upload(&f.state,"owner-1",Some(&f.draft),&uuid::Uuid::new_v4().to_string(),b"EDIT ONLY PRIVATE PREVIEW",None).await;assert_eq!(status,StatusCode::CREATED,"{pin}");
    publish_pin(&f.state,&f.draft,&pin).await;
    let born=birth_from_published(&f.state,"editor-1").await;let selected=born["id"].as_str().unwrap();
    sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('asset-edit-only','space-1','Edit only','[\"edit_content\"]','owner-1')").execute(&f.state.db).await.unwrap();
    sqlx::query("UPDATE space_members SET role='asset-edit-only' WHERE user_id='editor-1'").execute(&f.state.db).await.unwrap();
    assert_eq!(get(&f.state,"editor-1",PAGE).await.0,StatusCode::NOT_FOUND);
    let(status,raw)=request(&f.state,"editor-1","PUT",&format!("{PAGE}/drafts/{selected}"),Some(json!({"source_digest":born["source_digest"],"content":born["content"]}))).await;assert_eq!(status,StatusCode::OK,"Current draft Edit remains: {}",String::from_utf8_lossy(&raw));
    assert_eq!(get(&f.state,"editor-1",&metadata(&pin)).await.0,StatusCode::OK,"Denied publication View cannot hide another valid private binding");
    expect_bytes(&f.state,"editor-1",&pin,b"EDIT ONLY PRIVATE PREVIEW").await;
    f.close().await;
}

#[tokio::test]
async fn actual_legacy_published_birth_preserves_unknown_paths_and_opaque_references_without_grants(){
    let f=Fixture::new().await;
    let original="Ordinary existing page text  \r\n\r\n![Old unresolved image](/api/v1/files/old-unknown/missing.png)\r\n\r\n[Old opaque asset](starlane-attachment://old-opaque-id)\r\n";
    let path=storage::write_document(&f.slug,"doc-1","Published title","",original).await.unwrap();
    let canonical:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&f.state.db).await.unwrap();
    assert_eq!(path,canonical,"Use the existing standard file-backed published fixture");
    assert_eq!(storage::read_document(&canonical).await.unwrap(),original);
    for table in ["attachments","attachment_audience_origins","attachment_audience_bindings"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&f.state.db).await.unwrap(),0);}
    let born=birth_from_published(&f.state,"editor-1").await;
    assert_eq!(born["content"],original,"Draft birth preserves every original unresolved reference and CRLF byte");
    let selected=born["id"].as_str().unwrap();
    let saved:String=sqlx::query_scalar("SELECT content FROM document_drafts WHERE id=$1").bind(selected).fetch_one(&f.state.db).await.unwrap();assert_eq!(saved,original);
    assert_eq!(std::fs::read(storage::data_dir().join(canonical)).unwrap(),original.as_bytes(),"Private birth cannot rewrite the original published file");
    for table in ["attachments","attachment_audience_origins","attachment_audience_bindings"]{assert_eq!(sqlx::query_scalar::<_,i64>(&format!("SELECT count(*) FROM {table}")).fetch_one(&f.state.db).await.unwrap(),0,"Unknown reference did not fabricate {table}");}
    f.close().await;
}

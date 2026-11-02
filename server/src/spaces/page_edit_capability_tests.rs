use crate::{AppState,spaces::drafts::draft_reader_current_tests as fixture};
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::{Value,json};
use tower::ServiceExt;
async fn get(s:&AppState,user:&str,key:Option<&str>)->(StatusCode,Value){
 let mut request=Request::builder().uri("/spaces/space-1/documents/doc-1");
 request=if let Some(key)=key{request.header("X-API-Key",key)}else{request.header("Authorization",format!("Bearer {}",fixture::token(s,user)))};
 let response=crate::api_routes().with_state(s.clone()).oneshot(request.body(Body::empty()).unwrap()).await.unwrap();let status=response.status();
 if status==StatusCode::OK{assert_eq!(response.headers().get("Cache-Control").unwrap(),"no-store")}
 let body=to_bytes(response.into_body(),2_000_000).await.unwrap();(status,serde_json::from_slice(&body).unwrap())
}
async fn setup()->(AppState,String,std::path::PathBuf){let(s,slug,path)=fixture::fixture().await;sqlx::query("PRAGMA journal_mode=WAL").execute(&s.db).await.unwrap();sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();(s,slug,path)}
#[tokio::test]
async fn signed_get_projects_actual_lock_owner_exception_without_creating_authority(){
 let(s,slug,path)=setup().await;
 for(user,owner,locked,expected)in [("editor-1",Some("editor-1"),1,true),("owner-1",Some("owner-1"),1,true),("editor-1",Some("owner-1"),1,false),("editor-1",None,1,false),("viewer-1",Some("viewer-1"),1,false),("editor-1",None,0,true)]{
 sqlx::query("UPDATE documents SET locked=$1,locked_by=$2 WHERE id='doc-1'").bind(locked).bind(owner).execute(&s.db).await.unwrap();let before=fixture::snapshot(&s,true).await;
 let(status,dto)=get(&s,user,None).await;assert_eq!(status,StatusCode::OK,"{dto}");assert_eq!(dto["page_edit_allowed"],expected,"{user}/{owner:?}/{locked}");assert_eq!(dto["locked"],locked==1);assert_eq!(dto["locked_by"],json!(owner));assert_eq!(fixture::snapshot(&s,true).await,before);}
 fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn custom_current_edit_role_may_edit_owned_lock_but_view_only_role_cannot(){
 let(s,slug,path)=setup().await;
 sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('cap-custom','space-1','Capability custom','[\"edit_content\"]','owner-1')").execute(&s.db).await.unwrap();
 sqlx::query("UPDATE space_members SET role='cap-custom' WHERE space_id='space-1' AND user_id='editor-1'").execute(&s.db).await.unwrap();sqlx::query("UPDATE documents SET locked=1,locked_by='editor-1' WHERE id='doc-1'").execute(&s.db).await.unwrap();
 // Current metadata GET keeps its existing View prerequisite; this field
 // does not expand a write-only role into a page reader.
 let(status,dto)=get(&s,"editor-1",None).await;assert_ne!(status,StatusCode::OK,"{dto}");
 sqlx::query("UPDATE space_roles SET permissions='[\"view\",\"edit_content\"]' WHERE id='cap-custom'").execute(&s.db).await.unwrap();
 let(status,dto)=get(&s,"editor-1",None).await;assert_eq!(status,StatusCode::OK,"{dto}");assert_eq!(dto["page_edit_allowed"],true);
 sqlx::query("UPDATE space_roles SET permissions='[\"view\"]' WHERE id='cap-custom'").execute(&s.db).await.unwrap();let(status,dto)=get(&s,"editor-1",None).await;assert_eq!(status,StatusCode::OK,"{dto}");assert_eq!(dto["page_edit_allowed"],false);fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn direct_viewer_never_gets_edit_capability_or_owner_attribution(){
 let(s,slug,path)=setup().await;sqlx::query("UPDATE users SET account_kind='guest' WHERE id='viewer-1'").execute(&s.db).await.unwrap();sqlx::query("UPDATE documents SET locked=1,locked_by='viewer-1' WHERE id='doc-1'").execute(&s.db).await.unwrap();
 sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES('cap-view','doc-1','space-1','viewer-1','viewer','page','original','owner-1','original','original')").execute(&s.db).await.unwrap();
 let(status,dto)=get(&s,"viewer-1",None).await;assert_eq!(status,StatusCode::OK,"{dto}");assert_eq!(dto["page_access_source"],"direct_page_grant");assert_eq!(dto["page_edit_allowed"],false);assert_eq!(dto["locked_by"],Value::Null);fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn read_key_does_not_inherit_human_edit_role_or_matching_lock_owner(){
 let(s,slug,path)=setup().await;sqlx::query("UPDATE documents SET locked=1,locked_by='editor-1' WHERE id='doc-1'").execute(&s.db).await.unwrap();
 sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('cap-read','editor-1','Read capability',$1,'cap','[\"read\"]')").bind(crate::auth::api_keys::hash_api_key("cap-read-secret")).execute(&s.db).await.unwrap();
 let(status,dto)=get(&s,"editor-1",Some("cap-read-secret")).await;assert_eq!(status,StatusCode::OK,"{dto}");assert_eq!(dto["page_edit_allowed"],false);fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn signed_get_rejects_same_stamp_late_role_lock_policy_or_identity_drift(){
 for statement in ["UPDATE space_members SET role='viewer' WHERE space_id='space-1' AND user_id='editor-1'","UPDATE documents SET locked=1,locked_by='owner-1' WHERE id='doc-1'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','editor-1','owner-1')","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('doc-1','space-1','restricted','cap-late','owner-1')","DELETE FROM sessions WHERE id='publication-editor-1'"]{
 let(s,slug,path)=setup().await;let before_stamp:String=sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();
 let(status,dto)=super::IDENTIFIER_GET_AFTER_CAPABILITY.scope((s.db.clone(),statement),get(&s,"editor-1",None)).await;assert_ne!(status,StatusCode::OK,"{statement}: {dto}");assert!(dto.get("page_edit_allowed").is_none(),"no stale capability DTO");
 let after_stamp:String=sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();assert_eq!(before_stamp,after_stamp);fixture::cleanup(s,&slug,&path).await;}
}

#[tokio::test]
async fn signed_get_reproves_current_edit_after_inventory_await_boundary(){
 for statement in ["UPDATE space_members SET role='viewer' WHERE space_id='space-1' AND user_id='editor-1'","UPDATE documents SET locked=1,locked_by='owner-1' WHERE id='doc-1'","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('doc-1','space-1','restricted','cap-final','owner-1')","DELETE FROM sessions WHERE id='publication-editor-1'"]{
 let(s,slug,path)=setup().await;
 let(status,dto)=super::PAGE_EDIT_GET_AFTER_INVENTORY.scope((s.db.clone(),statement),get(&s,"editor-1",None)).await;
 assert_ne!(status,StatusCode::OK,"{statement}: {dto}");assert!(dto.get("page_edit_allowed").is_none());fixture::cleanup(s,&slug,&path).await;}
}

use super::*;
use crate::spaces::drafts::draft_reader_current_tests as fixture;
use axum::{body::{Body,to_bytes},http::{Request,StatusCode}};
use serde_json::Value;
use tower::ServiceExt;
async fn get(s:&AppState,user:&str)->(StatusCode,Value){
 let response=super::router().with_state(s.clone()).oneshot(Request::builder().uri("/spaces/space-1/documents/doc-1/drafts/draft-1/save-source").header("Authorization",format!("Bearer {}",fixture::token(s,user))).body(Body::empty()).unwrap()).await.unwrap();
 let status=response.status();if status==StatusCode::OK {assert_eq!(response.headers()["Cache-Control"],"no-store");}
 (status,serde_json::from_slice(&to_bytes(response.into_body(),2_000_000).await.unwrap()).unwrap())
}
#[tokio::test]
async fn write_only_selected_editor_reviews_exact_source_without_view_expansion(){
 let(s,slug,path)=fixture::fixture().await;
 sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('save-custom','space-1','Write only','[\"edit_content\"]','owner-1')").execute(&s.db).await.unwrap();
 sqlx::query("UPDATE space_members SET role='save-custom' WHERE space_id='space-1' AND user_id='editor-1'").execute(&s.db).await.unwrap();
 sqlx::query("UPDATE documents SET locked=1,locked_by='editor-1' WHERE id='doc-1'").execute(&s.db).await.unwrap();
 sqlx::query("UPDATE documents SET title='CANONICAL PRIVATE LABEL',published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();
 let before=fixture::snapshot(&s,true).await;let(status,body)=get(&s,"editor-1").await;assert_eq!(status,StatusCode::OK,"{body}");
 let mut conn=s.db.acquire().await.unwrap();let row=draft_save::selected(&mut conn,"space-1","doc-1","draft-1").await.unwrap();assert_eq!(body["draft_source_digest"],draft_save::digest(&row).unwrap());assert_eq!(body["draft"],json!({"title":row["title"],"content":row["content"],"icon":row["icon"],"cover_image":row["cover_image"]}));drop(conn);
 assert_eq!(body["selected_draft"]["source_digest"],body["draft_source_digest"]);
 assert_eq!(body["selected_draft"]["document_title"],row["title"]);
 assert_eq!(body["selected_draft"]["access_role"],"editor");assert_eq!(body["selected_draft"]["can_publish"],false);
 assert!(!body.to_string().contains("CANONICAL PRIVATE LABEL"));
 assert_eq!(body.as_object().unwrap().len(),12);assert_eq!(fixture::snapshot(&s,true).await,before);
 let r=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().uri("/spaces/space-1/documents/doc-1/drafts/draft-1").header("Authorization",format!("Bearer {}",fixture::token(&s,"editor-1"))).body(Body::empty()).unwrap()).await.unwrap();assert_ne!(r.status(),StatusCode::OK);
 sqlx::query("UPDATE documents SET locked_by='owner-1' WHERE id='doc-1'").execute(&s.db).await.unwrap();assert_ne!(get(&s,"editor-1").await.0,StatusCode::OK);
 fixture::cleanup(s,&slug,&path).await;
}
#[tokio::test]
async fn selected_viewer_cannot_review_and_same_stamp_late_edit_source_is_held(){
 for statement in ["UPDATE document_drafts SET content='LATE CONTENT' WHERE id='draft-1'","UPDATE document_draft_access SET role='viewer' WHERE draft_id='draft-1' AND user_id='editor-1'","UPDATE space_members SET role='viewer' WHERE space_id='space-1' AND user_id='editor-1'","UPDATE documents SET locked=1,locked_by='owner-1' WHERE id='doc-1'","UPDATE documents SET locked=1,locked_by='editor-1' WHERE id='doc-1'","DELETE FROM sessions WHERE id='publication-editor-1'"]{
 let(s,slug,path)=fixture::fixture().await;sqlx::query("PRAGMA journal_mode=WAL").execute(&s.db).await.unwrap();assert_ne!(get(&s,"viewer-1").await.0,StatusCode::OK);
 let observer=s.clone();let(status,body)=draft_save::AFTER_READ_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query(statement).execute(&observer.db).await.unwrap();}))),get(&s,"editor-1")).await;assert_ne!(status,StatusCode::OK,"{statement}: {body}");fixture::cleanup(s,&slug,&path).await;}
}
#[tokio::test]
async fn current_owner_projection_is_exact_but_read_key_has_no_write_review(){
 let(s,slug,path)=fixture::fixture().await;
 let(status,body)=get(&s,"owner-1").await;assert_eq!(status,StatusCode::OK,"{body}");assert_eq!(body["selected_draft"]["access_role"],"owner");assert_eq!(body["selected_draft"]["can_publish"],true);assert_eq!(body["selected_draft"]["owner_id"],"owner-1");
 sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('save-read','owner-1','Read only',$1,'save','[\"read\"]')").bind(crate::auth::api_keys::hash_api_key("save-source-read-secret")).execute(&s.db).await.unwrap();
 let response=super::router().with_state(s.clone()).oneshot(Request::builder().uri("/spaces/space-1/documents/doc-1/drafts/draft-1/save-source").header("X-API-Key","save-source-read-secret").body(Body::empty()).unwrap()).await.unwrap();assert_ne!(response.status(),StatusCode::OK);
 sqlx::query("UPDATE documents SET archived_at='archived' WHERE id='doc-1'").execute(&s.db).await.unwrap();assert_ne!(get(&s,"owner-1").await.0,StatusCode::OK);fixture::cleanup(s,&slug,&path).await;
}

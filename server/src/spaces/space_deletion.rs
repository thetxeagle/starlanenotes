//! Actual owner space deletion. Keep legacy SQL cascades; retained directory
//! bytes are never unlinked by a stale slug. No synthetic deletion receipt.
use super::{access, mutation_guards, routes};
use crate::{auth::{middleware::AuthUser, routes::AppError}, AppState};
use serde_json::{json, Value};
use sqlx::AnyConnection;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
type Domain = BTreeMap<String, Vec<Value>>;
fn changed() -> AppError { AppError::Conflict("Space deletion source changed; no deletion was confirmed".into()) }
const TABLES: &[&str] = &[
    "api_keys",
    "asset_delete_replay_receipts",
    "asset_replay_receipts",
    "attachments",
    "audience_mutation_fence",
    "audit_log",
    "bookmark_cache",
    "canonical_revision_receipts",
    "comments",
    "doc_subscriptions",
    "doc_tags",
    "doc_versions",
    "document_access_allowlist",
    "document_access_policies",
    "document_viewer_grants",
    "document_create_replay_receipts",
    "document_delete_replay_receipts",
    "document_draft_access",
    "document_drafts",
    "document_identifier_receipts",
    "document_maintenance_receipts",
    "document_metadata_replay_receipts",
    "document_workflow_events",
    "document_workflows",
    "documents",
    "draft_birth_receipts",
    "draft_transfer_receipts",
    "draft_save_replay_receipts",
    "excalidraw_files",
    "folders",
    "global_roles",
    "group_global_roles",
    "group_members",
    "groups",
    "move_asset_path_aliases",
    "move_commit_receipts",
    "move_metadata_escrow",
    "move_previews",
    "native_auth_handoffs",
    "notification_prefs",
    "notifications",
    "oauth_accounts",
    "passkey_challenges",
    "passkeys",
    "password_reset_tokens",
    "properties_schema",
    "property_values",
    "publication_receipts",
    "reactions",
    "sessions",
    "space_group_permissions",
    "space_members",
    "space_roles",
    "space_user_blocks",
    "spaces",
    "starred_documents",
    "storage_journal_receipts",
    "sync_replay_checkpoints",
    "sync_replay_manifests",
    "sync_replay_operations",
    "system_config",
    "tags",
    "template_drafts",
    "template_replay_receipts",
    "template_revisions",
    "templates",
    "users",
    "webhook_deliveries",
    "webhooks",
    "workflow_templates",
];
// Exact FK edges from migrations0001–0046; RESTRICT grants are explicitly removed.
// These are expected SQL effects, never extra cleanup statements.
const EDGES: &[(&str,&str,&str,&str,&str)] = &[
    ("document_viewer_grants","document_id,space_id","documents","id,space_id","RESTRICT"),
    ("document_viewer_grants","space_id","spaces","id","RESTRICT"),
    ("document_viewer_grants","user_id","users","id","RESTRICT"),
    ("document_viewer_grants","created_by","users","id","RESTRICT"),
    ("space_members","space_id","spaces","id","CASCADE"),
    ("folders","space_id","spaces","id","CASCADE"),
    ("folders","parent_id","folders","id","CASCADE"),
    ("documents","space_id","spaces","id","CASCADE"),
    ("documents","folder_id","folders","id","SET NULL"),
    ("doc_versions","document_id","documents","id","CASCADE"),
    ("tags","space_id","spaces","id","CASCADE"),
    ("doc_tags","document_id","documents","id","CASCADE"),
    ("doc_tags","tag_id","tags","id","CASCADE"),
    ("properties_schema","space_id","spaces","id","CASCADE"),
    ("property_values","document_id","documents","id","CASCADE"),
    ("property_values","property_id","properties_schema","id","CASCADE"),
    ("attachments","document_id","documents","id","CASCADE"),
    ("attachments","space_id","spaces","id","CASCADE"),
    ("excalidraw_files","document_id","documents","id","CASCADE"),
    ("excalidraw_files","space_id","spaces","id","CASCADE"),
    ("comments","document_id","documents","id","CASCADE"),
    ("comments","parent_id","comments","id","CASCADE"),
    ("reactions","document_id","documents","id","CASCADE"),
    ("doc_subscriptions","document_id","documents","id","CASCADE"),
    ("webhooks","space_id","spaces","id","CASCADE"),
    ("webhook_deliveries","webhook_id","webhooks","id","CASCADE"),
    ("templates","space_id","spaces","id","CASCADE"),
    ("group_members","group_id","groups","id","CASCADE"),
    ("space_group_permissions","space_id","spaces","id","CASCADE"),
    ("space_group_permissions","group_id","groups","id","CASCADE"),
    ("workflow_templates","space_id","spaces","id","CASCADE"),
    ("document_workflows","document_id","documents","id","CASCADE"),
    ("document_workflows","workflow_template_id","workflow_templates","id","CASCADE"),
    ("document_workflow_events","document_workflow_id","document_workflows","id","CASCADE"),
    ("document_workflow_events","document_id","documents","id","CASCADE"),
    ("starred_documents","document_id","documents","id","CASCADE"),
    ("documents","parent_id","documents","id","SET NULL"),
    ("groups","managed_space_id","spaces","id","CASCADE"),
    ("space_roles","space_id","spaces","id","CASCADE"),
    ("space_user_blocks","space_id","spaces","id","CASCADE"),
    ("document_drafts","document_id","documents","id","CASCADE"),
    ("document_drafts","space_id","spaces","id","CASCADE"),
    ("document_draft_access","draft_id","document_drafts","id","CASCADE"),
    ("group_global_roles","group_id","groups","id","CASCADE"),
    ("template_drafts","template_id","templates","id","CASCADE"),
    ("template_drafts","space_id","spaces","id","CASCADE"),
    ("template_revisions","template_id","templates","id","CASCADE"),
    ("template_revisions","space_id","spaces","id","CASCADE"),
    ("document_access_policies","document_id","documents","id","CASCADE"),
    ("document_access_policies","space_id","spaces","id","CASCADE"),
    ("document_access_allowlist","document_id,space_id","document_access_policies","document_id,space_id","CASCADE"),
    ("document_access_allowlist","group_id","groups","id","CASCADE"),
    ("documents","maintenance_group_id","groups","id","SET NULL"),
];

async fn capture(conn: &mut AnyConnection) -> Result<Domain, AppError> {
    let mut data = Domain::new();
    for table in TABLES {
        data.insert((*table).into(), super::document_deletion::rows(conn,
            &format!("SELECT * FROM {table}"), &[]).await.map_err(AppError::internal)?);
    }
    Ok(data)
}
async fn verify(conn: &mut AnyConnection, expected: &Domain) -> Result<(), AppError> {
    if capture(conn).await? != *expected { return Err(changed()); } Ok(())
}
fn references(row: &Value, child: &str, parent: &Value, columns: &str) -> bool {
    child.split(',').zip(columns.split(',')).all(|(a,b)| !row[a].is_null() && row[a] == parent[b])
}
fn expected_delete(original: &Domain, space: &str) -> Result<Domain, AppError> {
    let mut expected = original.clone(); let mut removed = Domain::new();
    let targets: Vec<Value> = original["spaces"].iter().filter(|r|r["id"]==space).cloned().collect();
    if targets.len()!=1 {return Err(AppError::NotFound("Space not found".into()));}
    removed.insert("spaces".into(), targets);
    expected.get_mut("spaces").unwrap().retain(|r|r["id"]!=space);
    // Exact explicit deletion, not an implicit FK cascade or receipt erasure.
    expected.get_mut("document_viewer_grants").unwrap().retain(|r|r["space_id"]!=space);
    loop {
        let mut progress = false;
        for &(table, child, parent, columns, action) in EDGES {
            if action!="CASCADE" {continue;}
            let parents = removed.get(parent).cloned().unwrap_or_default();
            let rows = expected.get_mut(table).unwrap(); let mut lost=Vec::new();
            rows.retain(|row| {
                let discard=parents.iter().any(|p|references(row,child,p,columns));
                if discard {lost.push(row.clone());} !discard
            });
            if !lost.is_empty() {progress=true; removed.entry(table.into()).or_default().extend(lost);}
        }
        if !progress {break;}
    }
    for &(table,child,parent,columns,action) in EDGES {
        if action!="SET NULL" {continue;}
        for row in expected.get_mut(table).unwrap() {
            if removed.get(parent).is_some_and(|parents|parents.iter().any(|p|references(row,child,p,columns))) {
                for column in child.split(',') {row[column]=Value::Null;}
            }
        }
    }
    for rows in expected.values_mut() {rows.sort_by_cached_key(Value::to_string);}
    Ok(expected)
}
fn affected_spaces(original:&Domain, expected:&Domain, target:&str)->Vec<String> {
    let mut spaces=BTreeSet::from([target.to_owned()]);
    let mut global=false;
    for (table,rows) in original {
        for row in rows.iter().filter(|row|!expected[table].contains(row)) {
            if let Some(s)=row["space_id"].as_str() {spaces.insert(s.into());}
            if table=="group_global_roles" {global=true;}
            if let Some(document)=row["document_id"].as_str() {
                if let Some(p)=original["documents"].iter().find(|p|p["id"]==document) {
                    if let Some(s)=p["space_id"].as_str() {spaces.insert(s.into());}
                }
            }
        }
    }
    if global {for row in &original["spaces"] {if let Some(s)=row["id"].as_str(){spaces.insert(s.into());}}}
    spaces.into_iter().collect()
}
async fn require_owner(conn:&mut AnyConnection,auth:&AuthUser,space:&str)->Result<(),AppError> {
    access::require_current_human_preference_write(conn,auth).await?;
    routes::require_space_role_on_connection(conn,space,&auth.claims.sub,&["owner"]).await?;
    Ok(())
}
pub(crate) async fn delete(state:&AppState,auth:&AuthUser,space:&str)->Result<(),AppError> {
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)] before_lock().await;
    mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    require_owner(&mut tx,auth,space).await?;
    let original=capture(&mut tx).await?;
    let mut expected=expected_delete(&original,space)?;
    let spaces=affected_spaces(&original,&expected,space);
    mutation_guards::lock_spaces_for_mutation(&mut tx,&spaces.iter().map(String::as_str).collect::<Vec<_>>()).await?;
    // Related SET NULL/ACL/group cascades can change other page policies. Lock
    // those exact existing rows in global ID order after all space guards.
    let mut pages:Vec<(String,String)>=original["documents"].iter()
        .filter(|p|!expected["documents"].contains(p))
        .map(|p|Ok((p["id"].as_str().ok_or_else(changed)?.into(),p["space_id"].as_str().ok_or_else(changed)?.into())))
        .collect::<Result<_,AppError>>()?;
    for row in &original["document_access_allowlist"] {
        if !expected["document_access_allowlist"].contains(row) {
            if let Some(p)=original["documents"].iter().find(|p|p["id"]==row["document_id"]) {
                pages.push((p["id"].as_str().ok_or_else(changed)?.into(),p["space_id"].as_str().ok_or_else(changed)?.into()));
            }
        }
    }
    pages.sort(); pages.dedup();
    for (id,scope) in pages {
        if sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2").bind(&id).bind(&scope)
            .execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1 {return Err(changed());}
    }
    verify(&mut tx,&original).await?; require_owner(&mut tx,auth,space).await?;
    let grant_count=original["document_viewer_grants"].iter().filter(|r|r["space_id"]==space).count() as u64;
    if sqlx::query("DELETE FROM document_viewer_grants WHERE space_id=$1").bind(space).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=grant_count {return Err(changed());}
    if sqlx::query("DELETE FROM spaces WHERE id=$1").bind(space).execute(&mut *tx)
        .await.map_err(AppError::internal)?.rows_affected()!=1 {return Err(changed());}
    let id=Uuid::new_v4().to_string();
    let created:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,'space.delete','space',$3) RETURNING created_at")
        .bind(&id).bind(&auth.claims.sub).bind(space).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit=json!({"id":id,"user_id":auth.claims.sub,"ip_address":null,"actor_kind":"user","actor_id":auth.claims.sub,
        "action":"space.delete","target_type":"space","target_id":space,"created_at":created,"before_value":null,"after_value":null});
    expected.get_mut("audit_log").unwrap().push(audit.clone());
    expected.get_mut("audit_log").unwrap().sort_by_cached_key(Value::to_string);
    #[cfg(test)] after_write().await;
    // The original owner proof is consumed intentionally: its target and direct
    // membership can legitimately disappear. Every original actor/policy row is
    // compared against only the exact expected FK outcome; no fake post-state
    // principal or missing-space role resolver substitutes new authority.
    verify(&mut tx,&expected).await?;
    #[cfg(test)] before_identity().await;
    access::require_current_human_preference_write(&mut tx,auth).await?;
    tx.commit().await.map_err(|_|AppError::Conflict("Space deletion outcome is unknown; retain evidence and inspect current state before another deletion".into()))?;
    crate::audit::append_committed_move_action(&audit).await;
    crate::webhooks::dispatch::dispatch_event(&state.db,"space.deleted",
        &json!({"space_id":space,"deleted_by":auth.claims.sub}),None).await;
    Ok(())
}
#[cfg(test)] type Hook=std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>;
#[cfg(test)] tokio::task_local! {pub(crate) static BEFORE_LOCK:std::cell::RefCell<Option<Hook>>;pub(crate) static AFTER_WRITE:std::cell::RefCell<Option<Hook>>;pub(crate) static BEFORE_IDENTITY:std::cell::RefCell<Option<Hook>>;}
#[cfg(test)] async fn before_lock(){if let Ok(Some(h))=BEFORE_LOCK.try_with(|v|v.borrow_mut().take()){h.await}}
#[cfg(test)] async fn after_write(){if let Ok(Some(h))=AFTER_WRITE.try_with(|v|v.borrow_mut().take()){h.await}}
#[cfg(test)] async fn before_identity(){if let Ok(Some(h))=BEFORE_IDENTITY.try_with(|v|v.borrow_mut().take()){h.await}}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::{Body,to_bytes},http::{Request,StatusCode},extract::FromRequestParts};
    use tower::ServiceExt;
    async fn fixture(wal:bool)->AppState {
        let mut s=crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("PRAGMA foreign_keys=ON").execute(&s.db).await.unwrap();
        if wal {
            let path=std::env::temp_dir().join(format!("space-delete-{}.sqlite",Uuid::new_v4()));
            // Use the established migrated disk fixture, not an unproved
            // VACUUM export followed by mode=rw with no file witness.
            s.db.close().await;
            s.db=sqlx::any::AnyPoolOptions::new().max_connections(3).after_connect(|c,_|Box::pin(async move {
                sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;
                sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *c).await?;
                sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;Ok(())
            })).connect(&format!("sqlite://{}?mode=rwc",path.display())).await.unwrap();
            sqlx::migrate!("./migrations").run(&s.db).await.unwrap();
            for id in ["owner-a","viewer-a","editor-a"] {
                sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(id).bind(format!("{id}@example.test")).execute(&s.db).await.unwrap();
                sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'fixture','2999-01-01T00:00:00Z')").bind(format!("metadata-{id}")).bind(id).execute(&s.db).await.unwrap();
            }
            for space in ["space-a","space-b"] {
                sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner-a')").bind(space).execute(&s.db).await.unwrap();
            }
            for (id,role) in [("owner-a","owner"),("viewer-a","viewer"),("editor-a","editor")] {
                sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)").bind(format!("member-{id}")).bind(id).bind(role).execute(&s.db).await.unwrap();
            }
            for (id,space) in [("target-a","space-a"),("target-b","space-b")] {
                sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,$2,$1,'','owner-a','owner-a','published')").bind(id).bind(space).execute(&s.db).await.unwrap();
            }
            assert!(std::fs::metadata(&path).unwrap().is_file());
            assert!(std::fs::metadata(&path).unwrap().len()>0);
            let actual:(i64,String,String)=sqlx::query_as("PRAGMA database_list").fetch_one(&s.db).await.unwrap();
            assert_eq!(std::fs::canonicalize(actual.2).unwrap(),std::fs::canonicalize(&path).unwrap());
            let mode:String=sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&s.db).await.unwrap();assert_eq!(mode.to_lowercase(),"wal");
            let fk:i64=sqlx::query_scalar("PRAGMA foreign_keys").fetch_one(&s.db).await.unwrap();assert_eq!(fk,1);

        }
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,archived_at,deleted_at) VALUES('archived','space-a','Private archive','','editor-a','editor-a','old',NULL),('trashed','space-a','Private trash','','viewer-a','viewer-a',NULL,'old')").execute(&s.db).await.unwrap();
        for (id,page,space,owner) in [("draft-a","target-a","space-a","owner-a"),("draft-archive","archived","space-a","editor-a"),("draft-trash","trashed","space-a","viewer-a"),("foreign","target-b","space-b","owner-a")] {
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES($1,$2,$3,$4,$1,'KEEP BYTES','old','old','old')").bind(id).bind(page).bind(space).bind(owner).execute(&s.db).await.unwrap();
        }
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft-a','editor-a','editor','owner-a'),('foreign','viewer-a','viewer','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO draft_birth_receipts(actor_id,operation_id,space_id,document_id,request_fingerprint,draft_id,result_digest,result_json) VALUES('owner-a','historical-birth','space-a','target-a','opaque','draft-a','opaque','{}')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO draft_save_replay_receipts(provider,incoming_id,actor_id,entity_type,entity_id,action,payload_fingerprint,space_id,document_id,draft_id,source_digest,result_digest,created_at,receipt_json) VALUES('none','historical-save','owner-a','document','target-a','update_content','opaque','space-a','target-a','draft-a','opaque','opaque','old','{}')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO publication_receipts(op_id,space_id,document_id,draft_id,actor_id,source_path,source_revision,content_path,revision,sha256,byte_length,payload_fingerprint,manifest_json) VALUES('historical-publication','space-a','target-a','draft-a','owner-a','old','old','immutable','new','opaque',1,'opaque','{}')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO canonical_revision_receipts(op_id,space_id,document_id,actor_id,reason,provider,incoming_id,entity_type,entity_id,action,payload_fingerprint,source_path,source_revision,published_stamp,content_path,revision,sha256,byte_length,manifest_json) VALUES('historical-canonical','space-a','target-a','owner-a','replay','none','incoming','document','target-a','update_content','opaque','old','old','old','immutable','new','opaque',1,'{}')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('purge-service',NULL,'Service fixture',$1,'fixture',1,'[\"admin\"]')").bind(crate::auth::api_keys::hash_api_key("purge-service")).execute(&s.db).await.unwrap();
        for scope in ["read","write","admin"] {sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'owner-a','Purge fixture',$2,'fixture',$3)").bind(format!("purge-{scope}")).bind(crate::auth::api_keys::hash_api_key(&format!("purge-{scope}"))).bind(format!("[\"{scope}\"]")).execute(&s.db).await.unwrap();}
        s
    }
    fn claims(user:&str,exp:i64)->crate::auth::jwt::Claims {crate::auth::jwt::Claims{sub:user.into(),sid:format!("metadata-{user}"),email:format!("{user}@example.test"),role:"member".into(),exp,iat:0,token_type:crate::auth::jwt::TokenType::Access}}
    fn signed(s:&AppState,user:&str)->String {jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims(user,i64::MAX),&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap()}
    async fn auth(s:&AppState,user:&str)->AuthUser {
        let (mut parts,_)=Request::builder().method("DELETE").header("Authorization",format!("Bearer {}",signed(s,user))).body(Body::empty()).unwrap().into_parts();
        AuthUser::from_request_parts(&mut parts,s).await.unwrap()
    }
    async fn snapshot(s:&AppState)->Vec<(String,Option<String>)> {crate::spaces::documents::tests::move_database_snapshot(s).await}
    async fn http(s:&AppState,credential:&str,key:bool)->(StatusCode,Value) {
        // Restore ONLY real extractor bookkeeping. Retain every identity/security
        // column and all domain rows in the full state comparison.
        let sessions:Vec<(String,Option<String>,String)>=sqlx::query_as("SELECT id,last_seen_at,expires_at FROM sessions").fetch_all(&s.db).await.unwrap();
        let keys:Vec<(String,Option<String>)>=sqlx::query_as("SELECT id,last_used_at FROM api_keys").fetch_all(&s.db).await.unwrap();
        let mut r=Request::builder().method("DELETE").uri("/spaces/space-a");r=if key {r.header("X-API-Key",credential)}else{r.header("Authorization",format!("Bearer {credential}"))};
        let response=crate::api_routes().with_state(s.clone()).oneshot(r.body(Body::empty()).unwrap()).await.unwrap();
        for (id,last,expires) in sessions {sqlx::query("UPDATE sessions SET last_seen_at=$1,expires_at=$2 WHERE id=$3").bind(last).bind(expires).bind(id).execute(&s.db).await.unwrap();}
        for (id,last) in keys {sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id=$2").bind(last).bind(id).execute(&s.db).await.unwrap();}
        let status=response.status();let b=to_bytes(response.into_body(),usize::MAX).await.unwrap();(status,if b.is_empty(){Value::Null}else{serde_json::from_slice(&b).unwrap()})
    }

    #[tokio::test]
    async fn actual_http_owner_cascade_foreign_setnull_and_all_receipts_retain_bytes() {
        let s=fixture(false).await;let slug=format!("space-delete-{}",Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'").bind(&slug).execute(&s.db).await.unwrap();
        let path=crate::storage::write_document(&slug,"target-a","Original","","RETAIN ALL BYTES").await.unwrap();
        crate::storage::write_lifecycle_document(&slug,"drafts","draft-a","Private","","RETAIN PRIVATE").await.unwrap();
        sqlx::query("INSERT INTO groups(id,name,managed_space_id) VALUES('managed-delete','Deletion managed fixture','space-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('managed-member','managed-delete','viewer-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('foreign-grant','space-b','managed-delete','viewer')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE documents SET maintenance_group_id='managed-delete',parent_id='target-a' WHERE id='target-b'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('target-b','space-b','restricted','original','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,group_id,created_by) VALUES('foreign-group-policy','target-b','space-b','managed-delete','owner-a')").execute(&s.db).await.unwrap();
        let before=capture(&mut s.db.acquire().await.unwrap()).await.unwrap();
        let files=crate::spaces::documents::tests::move_file_snapshot(&slug);
        assert_eq!(http(&s,&signed(&s,"owner-a"),false).await,(StatusCode::NO_CONTENT,Value::Null));
        let after=capture(&mut s.db.acquire().await.unwrap()).await.unwrap();
        for table in TABLES.iter().filter(|t|t.contains("receipt")||**t=="move_metadata_escrow"||**t=="move_asset_path_aliases") {assert_eq!(after[*table],before[*table],"{table}");}
        assert!(after["spaces"].iter().any(|v|v["id"]=="space-b"));
        let foreign=after["documents"].iter().find(|v|v["id"]=="target-b").unwrap();
        assert!(foreign["parent_id"].is_null());assert!(foreign["maintenance_group_id"].is_null());
        assert!(after["document_access_policies"].iter().any(|v|v["document_id"]=="target-b"));
        assert!(!after["document_access_allowlist"].iter().any(|v|v["id"]=="foreign-group-policy"));
        assert!(!after["groups"].iter().any(|v|v["id"]=="managed-delete"));
        assert!(after["document_drafts"].iter().any(|v|v["id"]=="foreign"));
        let audit:Vec<_>=after["audit_log"].iter().filter(|v|v["action"]=="space.delete").collect();
        assert_eq!(audit.len(),1);assert_eq!(audit[0]["actor_id"],"owner-a");
        assert_eq!(crate::spaces::documents::tests::move_file_snapshot(&slug),files);
        assert_eq!(crate::storage::read_document(&path).await.unwrap(),"RETAIN ALL BYTES");
        assert_eq!(http(&s,&signed(&s,"owner-a"),false).await.0,StatusCode::FORBIDDEN);
    }
    #[tokio::test]
    async fn actual_http_owner_only_write_admin_read_service_and_foreign_controls() {
        for scope in ["write","admin"] {
            let s=fixture(false).await;
            assert_eq!(http(&s,&format!("purge-{scope}"),true).await.0,StatusCode::NO_CONTENT);
        }
        for credential in ["purge-read","purge-service"] {
            let s=fixture(false).await;let before=snapshot(&s).await;
            assert!(!http(&s,credential,true).await.0.is_success());assert_eq!(snapshot(&s).await,before);
        }
        for actor in ["editor-a","viewer-a"] {
            let s=fixture(false).await;let before=snapshot(&s).await;
            assert!(!http(&s,&signed(&s,actor),false).await.0.is_success());assert_eq!(snapshot(&s).await,before);
        }
        let s=fixture(false).await;let a=auth(&s,"owner-a").await;
        let before=snapshot(&s).await;assert!(delete(&s,&a,"missing-space").await.is_err());assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn current_owner_block_activity_and_role_loss_deny_without_changes() {
        for statement in ["INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')", "UPDATE users SET is_active=0 WHERE id='owner-a'", "UPDATE space_members SET role='editor' WHERE space_id='space-a' AND user_id='owner-a'"] {
            let s=fixture(false).await;let a=auth(&s,"owner-a").await;
            sqlx::query(statement).execute(&s.db).await.unwrap();let before=snapshot(&s).await;
            assert!(delete(&s,&a,"space-a").await.is_err());assert_eq!(snapshot(&s).await,before);
        }
    }
    #[tokio::test]
    async fn audit_failure_and_source_receipt_foreign_trigger_changes_roll_back() {
        for statement in [
            "CREATE TRIGGER fail_space_delete BEFORE INSERT ON audit_log WHEN NEW.action='space.delete' BEGIN SELECT RAISE(ABORT,'fixture audit failure'); END",
            "CREATE TRIGGER fail_space_delete AFTER DELETE ON spaces WHEN OLD.id='space-a' BEGIN UPDATE users SET is_active=0 WHERE id='owner-a'; END",
            "CREATE TRIGGER fail_space_delete AFTER DELETE ON spaces WHEN OLD.id='space-a' BEGIN UPDATE documents SET title='UNEXPECTED FOREIGN' WHERE id='target-b'; END",
            "CREATE TRIGGER fail_space_delete AFTER DELETE ON spaces WHEN OLD.id='space-a' BEGIN DELETE FROM draft_birth_receipts WHERE operation_id='historical-birth'; END",
            "CREATE TRIGGER fail_space_delete AFTER INSERT ON audit_log WHEN NEW.action='space.delete' BEGIN UPDATE audit_log SET actor_id='viewer-a' WHERE id=NEW.id; END",
        ] {
            let s=fixture(false).await;let a=auth(&s,"owner-a").await;
            sqlx::query(statement).execute(&s.db).await.unwrap();let before=snapshot(&s).await;
            assert!(delete(&s,&a,"space-a").await.is_err());assert_eq!(snapshot(&s).await,before);
        }
    }
    #[tokio::test]
    async fn real_wal_guard_wait_rechecks_owner_or_block_before_cascade() {
        for statement in ["UPDATE space_members SET role='editor' WHERE space_id='space-a' AND user_id='owner-a'", "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')"] {
            let s=fixture(true).await;let a=auth(&s,"owner-a").await;
            let mut held=s.db.begin().await.unwrap();mutation_guards::lock_audience_for_mutation(&mut held).await.unwrap();
            mutation_guards::lock_spaces_for_mutation(&mut held,&["space-a"]).await.unwrap();
            let (entered,ready)=tokio::sync::oneshot::channel();let state=s.clone();
            let task=tokio::spawn(BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move{entered.send(()).unwrap();}))),async move{delete(&state,&a,"space-a").await}));
            ready.await.unwrap();tokio::time::sleep(std::time::Duration::from_millis(40)).await;assert!(!task.is_finished(),"actual adopted writer guard must hold deletion");
            sqlx::query(statement).execute(&mut *held).await.unwrap();held.commit().await.unwrap();
            let expected=snapshot(&s).await;assert!(task.await.unwrap().is_err());assert_eq!(snapshot(&s).await,expected);
        }
    }
    #[tokio::test]
    async fn final_signed_jwt_expiry_after_audit_rolls_back_entire_cascade() {
        let s=fixture(false).await;let expiration=chrono::Utc::now().timestamp()+2;
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims("owner-a",expiration),&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
        let (mut parts,_)=Request::builder().method("DELETE").header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap().into_parts();
        let a=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();let before=snapshot(&s).await;
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let mark=reached.clone();
        let result=BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            mark.store(true,std::sync::atomic::Ordering::SeqCst);
            while chrono::Utc::now().timestamp()<=expiration {tokio::time::sleep(std::time::Duration::from_millis(50)).await;}
        }))),delete(&s,&a,"space-a")).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert!(matches!(result,Err(AppError::Unauthorized(_))));assert_eq!(snapshot(&s).await,before);
    }
}

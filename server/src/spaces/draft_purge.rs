//! Explicit manager-only space purge. SQL is authoritative; retain mirrors and
//! immutable receipt evidence. This is not routine page/draft visibility.
use super::{access, mutation_guards};
use crate::{auth::{middleware::AuthUser, routes::AppError}, AppState};
use serde_json::{json, Value};
use sqlx::AnyConnection;
use uuid::Uuid;

#[derive(Clone)]
struct Proof {
    query: String,
    rows: Vec<Value>,
}
fn changed() -> AppError {
    AppError::Conflict("Draft purge source or management authority changed; no purge was confirmed".into())
}
async fn rows(conn: &mut AnyConnection, query: &str) -> Result<Vec<Value>, AppError> {
    super::document_deletion::rows(conn, query, &[]).await.map_err(AppError::internal)
}
async fn capture(conn: &mut AnyConnection) -> Result<Vec<Proof>, AppError> {
    let mut proof = Vec::new();
    // Preserve foreign rows as well as the complete scoped set. No projection
    // of these raw witnesses is returned to the manager or an audit callback.
    for table in [
        "users", "spaces", "space_members", "space_group_permissions",
        "space_user_blocks", "space_roles", "groups", "group_members",
        "group_global_roles", "global_roles", "documents", "document_access_policies",
        "document_access_allowlist", "document_drafts", "document_draft_access",
        "draft_birth_receipts", "draft_transfer_receipts", "draft_save_replay_receipts", "publication_receipts",
        "canonical_revision_receipts", "audit_log",
    ] {
        let query = format!("SELECT * FROM {table}");
        proof.push(Proof { rows: rows(conn, &query).await?, query });
    }
    Ok(proof)
}
async fn verify(conn: &mut AnyConnection, proof: &[Proof]) -> Result<(), AppError> {
    for witness in proof {
        if rows(conn, &witness.query).await? != witness.rows {
            return Err(changed());
        }
    }
    Ok(())
}

pub(crate) async fn purge(state: &AppState, auth: &AuthUser, space: &str) -> Result<u64, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)] before_lock().await;
    mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    mutation_guards::lock_spaces_for_mutation(&mut tx, &[space]).await?;
    access::require_space_membership_management(&mut tx, auth, space).await?;
    let mut expected = capture(&mut tx).await?;
    let mut selected: Vec<(String, String)> = expected.iter()
        .find(|p| p.query == "SELECT * FROM document_drafts").unwrap().rows.iter()
        .filter(|r| r["space_id"] == space)
        .map(|r| Ok((
            r["document_id"].as_str().ok_or_else(changed)?.to_owned(),
            r["id"].as_str().ok_or_else(changed)?.to_owned(),
        )))
        .collect::<Result<_, AppError>>()?;
    selected.sort();
    let mut pages: Vec<&str> = selected.iter().map(|r| r.0.as_str()).collect();
    pages.sort_unstable();
    pages.dedup();
    for page in pages {
        let count = sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
            .bind(page).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
        if count.rows_affected() != 1 { return Err(changed()); }
    }
    // Draft/share locks follow the sorted scoped page locks; no private labels
    // or participation filters narrow this explicit whole-space management.
    let mut ids: Vec<String> = selected.iter().map(|r| r.1.clone()).collect();
    ids.sort();
    for id in &ids {
        let count = sqlx::query("UPDATE document_drafts SET id=id WHERE id=$1 AND space_id=$2")
            .bind(id).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
        if count.rows_affected() != 1 { return Err(changed()); }
        sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id=$1")
            .bind(id).execute(&mut *tx).await.map_err(AppError::internal)?;
    }
    verify(&mut tx, &expected).await?;
    access::require_space_membership_management(&mut tx, auth, space).await?;
    if ids.is_empty() {
        access::require_current_human_preference_write(&mut tx, auth).await?;
        tx.rollback().await.map_err(AppError::internal)?;
        return Ok(0);
    }
    let count = sqlx::query("DELETE FROM document_drafts WHERE space_id=$1")
        .bind(space).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected();
    if count != ids.len() as u64 { return Err(changed()); }
    for witness in &mut expected {
        if witness.query == "SELECT * FROM document_drafts" {
            witness.rows.retain(|row| row["space_id"] != space);
        }
        if witness.query == "SELECT * FROM document_draft_access" {
            witness.rows.retain(|row| row["draft_id"].as_str()
                .is_none_or(|id| !ids.iter().any(|value| value == id)));
        }
    }
    let id = Uuid::new_v4().to_string();
    let created: String = sqlx::query_scalar(
        "INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,'document.drafts_purged','space',$3) RETURNING created_at",
    ).bind(&id).bind(&auth.claims.sub).bind(space)
        .fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit = json!({
        "id": id, "user_id": auth.claims.sub, "ip_address": null,
        "action": "document.drafts_purged", "target_type": "space", "target_id": space,
        "created_at": created, "actor_kind": "user", "actor_id": auth.claims.sub,
        "before_value": null, "after_value": null,
    });
    let audits = &mut expected.iter_mut().find(|p| p.query == "SELECT * FROM audit_log").unwrap().rows;
    audits.push(audit.clone());
    audits.sort_by_cached_key(Value::to_string);
    #[cfg(test)] after_write().await;
    // Inventory/source SQL first, then actual current manager and identity LAST.
    verify(&mut tx, &expected).await?;
    access::require_space_membership_management(&mut tx, auth, space).await?;
    #[cfg(test)] before_identity().await;
    access::require_current_human_preference_write(&mut tx, auth).await?;
    tx.commit().await.map_err(|_| AppError::Conflict(
        "Draft purge outcome is unknown; retain evidence and inspect current state before another purge".into(),
    ))?;
    crate::audit::append_committed_move_action(&audit).await;
    Ok(count)
}

#[cfg(test)] type Hook=std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>;
#[cfg(test)] tokio::task_local! {pub(crate) static BEFORE_LOCK:std::cell::RefCell<Option<Hook>>;pub(crate) static AFTER_WRITE:std::cell::RefCell<Option<Hook>>;pub(crate) static BEFORE_IDENTITY:std::cell::RefCell<Option<Hook>>;}
#[cfg(test)] async fn before_lock(){let h=BEFORE_LOCK.try_with(|v|v.borrow_mut().take()).ok().flatten();if let Some(h)=h{h.await}}
#[cfg(test)] async fn after_write(){let h=AFTER_WRITE.try_with(|v|v.borrow_mut().take()).ok().flatten();if let Some(h)=h{h.await}}
#[cfg(test)] async fn before_identity(){let h=BEFORE_IDENTITY.try_with(|v|v.borrow_mut().take()).ok().flatten();if let Some(h)=h{h.await}}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::{Body,to_bytes},http::{Request,StatusCode},extract::FromRequestParts};
    use tower::ServiceExt;
    async fn fixture(wal:bool)->AppState {
        let mut s=crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("PRAGMA foreign_keys=ON").execute(&s.db).await.unwrap();
        if wal {
            let path=std::env::temp_dir().join(format!("draft-purge-{}.sqlite",Uuid::new_v4()));
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
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'; INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,archived_at,deleted_at) VALUES('archived','space-a','Private archive','','editor-a','editor-a','old',NULL),('trashed','space-a','Private trash','','viewer-a','viewer-a',NULL,'old')").execute(&s.db).await.unwrap();
        for (id,page,space,owner) in [("draft-a","target-a","space-a","owner-a"),("draft-archive","archived","space-a","editor-a"),("draft-trash","trashed","space-a","viewer-a"),("foreign","target-b","space-b","owner-a")] {
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES($1,$2,$3,$4,$1,'KEEP BYTES','old','old','old')").bind(id).bind(page).bind(space).bind(owner).execute(&s.db).await.unwrap();
        }
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft-a','editor-a','editor','owner-a'),('foreign','viewer-a','viewer','owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO draft_birth_receipts(actor_id,operation_id,space_id,document_id,request_fingerprint,draft_id,result_digest,result_json) VALUES('owner-a','historical-birth','space-a','target-a','opaque','draft-a','opaque','{}'); INSERT INTO draft_transfer_receipts(actor_id,operation_id,request_fingerprint,space_id,document_id,draft_id,draft_created_at,source_digest,receipt_json,created_at) VALUES('owner-a','historical-transfer','opaque','space-a','target-a','draft-a','old','opaque','{}','old'); INSERT INTO draft_save_replay_receipts(provider,incoming_id,actor_id,entity_type,entity_id,action,payload_fingerprint,space_id,document_id,draft_id,source_digest,result_digest,created_at,receipt_json) VALUES('none','historical-save','owner-a','document','target-a','update_content','opaque','space-a','target-a','draft-a','opaque','opaque','old','{}')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO publication_receipts(op_id,space_id,document_id,draft_id,actor_id,source_path,source_revision,content_path,revision,sha256,byte_length,payload_fingerprint,manifest_json) VALUES('historical-publication','space-a','target-a','draft-a','owner-a','old','old','immutable','new','opaque',1,'opaque','{}'); INSERT INTO canonical_revision_receipts(op_id,space_id,document_id,actor_id,reason,provider,incoming_id,entity_type,entity_id,action,payload_fingerprint,source_path,source_revision,published_stamp,content_path,revision,sha256,byte_length,manifest_json) VALUES('historical-canonical','space-a','target-a','owner-a','replay','none','incoming','document','target-a','update_content','opaque','old','old','old','immutable','new','opaque',1,'{}')").execute(&s.db).await.unwrap();
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
        let mut r=Request::builder().method("DELETE").uri("/spaces/space-a/drafts");r=if key {r.header("X-API-Key",credential)}else{r.header("Authorization",format!("Bearer {credential}"))};
        let response=crate::api_routes().with_state(s.clone()).oneshot(r.body(Body::empty()).unwrap()).await.unwrap();
        for (id,last,expires) in sessions {sqlx::query("UPDATE sessions SET last_seen_at=$1,expires_at=$2 WHERE id=$3").bind(last).bind(expires).bind(id).execute(&s.db).await.unwrap();}
        for (id,last) in keys {sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id=$2").bind(last).bind(id).execute(&s.db).await.unwrap();}
        let status=response.status();let b=to_bytes(response.into_body(),usize::MAX).await.unwrap();(status,serde_json::from_slice(&b).unwrap())
    }
    #[tokio::test]
    async fn actual_http_manager_whole_space_and_empty_purge_retain_foreign_receipts_and_files() {
        let s=fixture(false).await;let slug=format!("purge-{}",Uuid::new_v4());
        sqlx::query("UPDATE spaces SET slug=$1 WHERE id='space-a'").bind(&slug).execute(&s.db).await.unwrap();
        let canonical=crate::storage::write_document(&slug,"target-a","Canonical","","PUBLISHED RETAIN").await.unwrap();
        crate::storage::write_lifecycle_document(&slug,"drafts","draft-a","Private","","PRIVATE RETAIN").await.unwrap();
        let files=crate::spaces::documents::tests::move_file_snapshot(&slug);
        let mut receipts=Vec::new();
        for table in ["draft_birth_receipts","draft_transfer_receipts","draft_save_replay_receipts","publication_receipts","canonical_revision_receipts"] {
            let captured=rows(&mut s.db.acquire().await.unwrap(),&format!("SELECT * FROM {table}")).await.unwrap();
            assert!(!captured.is_empty(),"retention evidence must be populated: {table}");
            receipts.push((table,captured));
        }
        let (status,value)=http(&s,&signed(&s,"owner-a"),false).await;assert_eq!(status,StatusCode::OK);assert_eq!(value,json!({"deleted":3}));
        assert_eq!(rows(&mut s.db.acquire().await.unwrap(),"SELECT id FROM document_drafts").await.unwrap(),vec![json!({"id":"foreign"})]);
        assert_eq!(rows(&mut s.db.acquire().await.unwrap(),"SELECT draft_id FROM document_draft_access").await.unwrap(),vec![json!({"draft_id":"foreign"})]);
        for (table,original) in receipts {
            assert_eq!(rows(&mut s.db.acquire().await.unwrap(),&format!("SELECT * FROM {table}")).await.unwrap(),original, "{table}");
        }
        assert_eq!(crate::spaces::documents::tests::move_file_snapshot(&slug),files);assert_eq!(crate::storage::read_document(&canonical).await.unwrap(),"PUBLISHED RETAIN");
        let before=snapshot(&s).await;assert_eq!(http(&s,&signed(&s,"owner-a"),false).await,(StatusCode::OK,json!({"deleted":0})));assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn actual_http_method_ceiling_and_custom_manage_without_page_view() {
        for scope in ["write","admin"] {
            let s=fixture(false).await;
            sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('purge-only','space-a','Purge only','[\"manage_space\"]','owner-a')").execute(&s.db).await.unwrap();
            sqlx::query("UPDATE space_members SET role='purge-only' WHERE space_id='space-a' AND user_id='owner-a'").execute(&s.db).await.unwrap();
            let stored:String=sqlx::query_scalar("SELECT role FROM space_members WHERE space_id='space-a' AND user_id='owner-a'").fetch_one(&s.db).await.unwrap();
            assert_eq!(stored,"purge-only");
            let original=auth(&s,"owner-a").await;
            let permission=access::require_space_membership_management(&mut s.db.acquire().await.unwrap(),&original,"space-a").await.unwrap();
            assert!(permission.contains(&access::Permission::ManageSpace));
            assert!(!permission.contains(&access::Permission::View));
            assert_eq!(http(&s,&format!("purge-{scope}"),true).await,(StatusCode::OK,json!({"deleted":3})));
        }
        let s=fixture(false).await;let before=snapshot(&s).await;assert!(!http(&s,"purge-read",true).await.0.is_success());assert_eq!(snapshot(&s).await,before);
        assert!(!http(&s,"purge-service",true).await.0.is_success());assert_eq!(snapshot(&s).await,before);
        assert!(!http(&s,&signed(&s,"editor-a"),false).await.0.is_success());assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn blocked_inactive_and_foreign_space_deny_without_domain_changes() {
        for mutation in ["INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','owner-a','owner-a')","UPDATE users SET is_active=0 WHERE id='owner-a'","DELETE FROM space_members WHERE space_id='space-a' AND user_id='owner-a'"] {
            let s=fixture(false).await;let a=auth(&s,"owner-a").await;sqlx::query(mutation).execute(&s.db).await.unwrap();let before=snapshot(&s).await;
            assert!(purge(&s,&a,"space-a").await.is_err());assert_eq!(snapshot(&s).await,before);
        }
        let s=fixture(false).await;let a=auth(&s,"owner-a").await;let before=snapshot(&s).await;assert!(purge(&s,&a,"space-b").await.is_err());assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn audit_failure_and_trigger_unexpected_scoped_foreign_sources_roll_back() {
        for statement in ["CREATE TRIGGER reject_purge BEFORE INSERT ON audit_log WHEN NEW.action='document.drafts_purged' BEGIN SELECT RAISE(ABORT,'audit blocked'); END", "CREATE TRIGGER corrupt_purge AFTER DELETE ON document_drafts WHEN OLD.id='draft-a' BEGIN UPDATE document_drafts SET title='FOREIGN CORRUPTION' WHERE id='foreign'; END",
            "CREATE TRIGGER corrupt_purge AFTER INSERT ON audit_log WHEN NEW.action='document.drafts_purged' BEGIN DELETE FROM publication_receipts WHERE op_id='historical-publication'; END",
            "CREATE TRIGGER corrupt_purge AFTER INSERT ON audit_log WHEN NEW.action='document.drafts_purged' BEGIN UPDATE canonical_revision_receipts SET revision='TAMPERED' WHERE op_id='historical-canonical'; END", "CREATE TRIGGER corrupt_purge AFTER INSERT ON audit_log WHEN NEW.action='document.drafts_purged' BEGIN UPDATE documents SET title='CANONICAL CORRUPTION' WHERE id='target-a'; END", "CREATE TRIGGER phantom_purge AFTER INSERT ON audit_log WHEN NEW.action='document.drafts_purged' BEGIN INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES('phantom','target-a','space-a','owner-a','PHANTOM','NEW','old','old','old'); END"] {
            let s=fixture(false).await;let a=auth(&s,"owner-a").await;sqlx::query(statement).execute(&s.db).await.unwrap();let before=snapshot(&s).await;
            assert!(purge(&s,&a,"space-a").await.is_err());assert_eq!(snapshot(&s).await,before);
        }
    }
    #[tokio::test]
    async fn real_wal_guard_wait_rechecks_current_manager_after_external_demotion() {
        let s=fixture(true).await;let a=auth(&s,"owner-a").await;
        let mut held=s.db.begin().await.unwrap();mutation_guards::lock_audience_for_mutation(&mut held).await.unwrap();
        let (started,ready)=tokio::sync::oneshot::channel();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let r=reached.clone();let state=s.clone();
        let task=tokio::spawn(async move {BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move {r.store(true,std::sync::atomic::Ordering::SeqCst);let _=started.send(());}))),purge(&state,&a,"space-a")).await});
        ready.await.unwrap();assert!(reached.load(std::sync::atomic::Ordering::SeqCst));tokio::task::yield_now().await;assert!(!task.is_finished());
        sqlx::query("UPDATE space_members SET role='viewer' WHERE space_id='space-a' AND user_id='owner-a'").execute(&mut *held).await.unwrap();held.commit().await.unwrap();let before=snapshot(&s).await;
        assert!(task.await.unwrap().is_err());assert_eq!(snapshot(&s).await,before);
    }
    #[tokio::test]
    async fn final_minted_jwt_expiry_after_inventory_rolls_back_complete_set() {
        let s=fixture(false).await;let a=auth(&s,"owner-a").await;let original=AuthUser{claims:claims("owner-a",chrono::Utc::now().timestamp()+2),api_key_scopes:a.api_key_scopes};
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&original.claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();let (mut parts,_)=Request::builder().method("DELETE").header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap().into_parts();let original=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();let before=snapshot(&s).await;
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let r=reached.clone();let result=BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move{r.store(true,std::sync::atomic::Ordering::SeqCst);tokio::time::sleep(std::time::Duration::from_secs(3)).await;}))),purge(&s,&original,"space-a")).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert!(result.is_err());assert_eq!(snapshot(&s).await,before);
    }
}

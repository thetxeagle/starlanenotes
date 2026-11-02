//! Original-source draft CAS. No mirror/canonical filesystem writes or new access model.
use super::access::{self, DraftOperation, DraftParticipation, PageOperation, Purpose};
use crate::{AppState, auth::{middleware::AuthUser, routes::AppError}};
use crate::storage::canonical_rewrite::IncomingIdentity;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Column, Row};
use uuid::Uuid;

#[derive(Clone, Default)]
pub(crate) struct Patch {
    pub title: Option<String>, pub icon: Option<String>,
    pub cover_image: Option<String>, pub content: Option<String>,
}
pub(crate) struct Saved {
    pub row: Value, pub owner_name: String, pub document_title: String,
    pub participation: DraftParticipation,
}
#[derive(Clone)]
struct SourceProof { query: String, parameters: Vec<String>, rows: Vec<Value> }
fn conflict() -> AppError {
    AppError::Conflict("source_conflict: Draft or access source changed; retain your edit and review the original draft before saving again".into())
}
pub(crate) fn required(value: Option<&Value>) -> Result<String, AppError> {
    match value {
        Some(Value::String(s)) if s.len()==64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) => Ok(s.clone()),
        _ => Err(AppError::Conflict("source_required: Save requires the original draft source_digest; retain your edit and explicitly review the draft before retrying".into())),
    }
}
/// Canonical sorted-key JSON of EVERY raw selected draft column. No display joins,
/// normalization, clock-only assumption or presentation default enters this digest.
pub(crate) fn digest(row: &Value) -> Result<String, AppError> {
    fn ordered(v:&Value)->Value {match v {
        Value::Object(m)=>{let mut keys:Vec<_>=m.keys().collect(); keys.sort(); let mut out=serde_json::Map::new(); for k in keys {out.insert(k.clone(),ordered(&m[k]));} Value::Object(out)},
        Value::Array(a)=>Value::Array(a.iter().map(ordered).collect()),v=>v.clone()
    }}
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&ordered(row)).map_err(AppError::internal)?)))
}
/// d.* plus display-only aliases, read in ONE statement, produces the same raw
/// source as SELECT *; future draft columns are included without a handwritten list.
pub(crate) fn joined_source(row:&sqlx::any::AnyRow)->Result<Value,AppError> {
    let mut out=serde_json::Map::new();
    for c in row.columns() {
        let k=c.name(); if matches!(k,"owner_name"|"document_title") {continue;}
        let v=if let Ok(v)=row.try_get::<Option<String>,_>(k) {v.map(Value::String).unwrap_or(Value::Null)}
        else if let Ok(v)=row.try_get::<Option<i64>,_>(k) {v.map(|v|json!(v)).unwrap_or(Value::Null)}
        else {return Err(conflict());};
        out.insert(k.into(),v);
    }
    Ok(Value::Object(out))
}
pub(crate) fn string(row:&Value,key:&str)->Result<String,AppError> {row.get(key).and_then(Value::as_str).map(str::to_owned).ok_or_else(conflict)}
pub(crate) fn optional(row:&Value,key:&str)->Result<Option<String>,AppError> {match row.get(key) {Some(Value::Null)=>Ok(None),Some(Value::String(s))=>Ok(Some(s.clone())),_=>Err(conflict())}}
async fn rows(conn:&mut AnyConnection,q:&str,args:&[&str])->Result<Vec<Value>,AppError> {
    super::document_deletion::rows(conn,q,args).await.map_err(AppError::internal)
}
pub(crate) async fn selected(conn:&mut AnyConnection,space:&str,doc:&str,draft:&str)->Result<Value,AppError> {
    let r=rows(conn,"SELECT * FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",&[draft,doc,space]).await?;
    if r.len()!=1 {return Err(AppError::NotFound("Draft unavailable".into()));} Ok(r[0].clone())
}
async fn verify_source(conn:&mut AnyConnection,proofs:&[SourceProof])->Result<(),AppError> {
    for p in proofs {let a:Vec<&str>=p.parameters.iter().map(String::as_str).collect();if rows(conn,&p.query,&a).await?!=p.rows {return Err(conflict());}}
    Ok(())
}
async fn capture_source(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    ancestors: &[String],
) -> Result<Vec<SourceProof>, AppError> {
    let mut specs = vec![
        (
            "SELECT * FROM documents WHERE space_id=$1".into(),
            vec![space.into()],
        ),
        ("SELECT id,space_id,parent_id FROM documents".into(), vec![]),
        (
            "SELECT id,role,is_active,account_kind FROM users WHERE id=$1".into(),
            vec![auth.claims.sub.clone()],
        ),
    ];
    for table in [
        "spaces",
        "space_members",
        "space_group_permissions",
        "space_user_blocks",
        "space_roles",
    ] {
        specs.push((
            format!(
                "SELECT * FROM {table} WHERE {}=$1",
                if table == "spaces" { "id" } else { "space_id" }
            ),
            vec![space.into()],
        ));
    }
    for table in [
        "groups",
        "group_members",
        "group_global_roles",
        "global_roles",
    ] {
        specs.push((format!("SELECT * FROM {table}"), vec![]));
    }
    for table in [
        "attachments",
        "comments",
        "doc_subscriptions",
        "doc_tags",
        "doc_versions",
        "document_access_allowlist",
        "document_access_policies",
        "document_drafts",
        "document_workflow_events",
        "document_workflows",
        "excalidraw_files",
        "property_values",
        "reactions",
        "starred_documents",
    ] {
        specs.push((
            format!("SELECT * FROM {table} WHERE document_id=$1"),
            vec![id.into()],
        ));
    }
    specs.push(("SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1".into(),vec![id.into()]));
    for parent in ancestors {
        for table in [
            "documents",
            "document_drafts",
            "document_access_policies",
            "document_access_allowlist",
        ] {
            specs.push((
                format!(
                    "SELECT * FROM {table} WHERE {}=$1",
                    if table == "documents" {
                        "id"
                    } else {
                        "document_id"
                    }
                ),
                vec![parent.clone()],
            ));
        }
        specs.push(("SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1".into(),vec![parent.clone()]));
    }
    let mut proof = Vec::new();
    for (query, parameters) in specs {
        let args: Vec<&str> = parameters.iter().map(String::as_str).collect();
        let current = rows(conn, &query, &args).await?;
        proof.push(SourceProof {
            query,
            parameters,
            rows: current,
        });
    }
    Ok(proof)
}


/// Method-aware response proof; captures one exact selected source and never
/// swaps a newer row into a confirmed save/create response.
pub(crate) async fn inspect(state:&AppState,auth:&AuthUser,space:&str,doc:&str,draft:&str,mutation:bool,original:Option<&Value>)->Result<Saved,AppError> {
    Ok(inspect_with_page(state,auth,space,doc,draft,mutation,original).await?.0)
}
/// Current human Edit-only review including the exact internally protected page
/// observation. Consumers project only lock/stamp; no View or policy expansion.
pub(crate) async fn inspect_edit_source(state:&AppState,auth:&AuthUser,space:&str,doc:&str,draft:&str)->Result<(Saved,Value),AppError> {
    inspect_with_page(state,auth,space,doc,draft,true,None).await
}
async fn inspect_with_page(state:&AppState,auth:&AuthUser,space:&str,doc:&str,draft:&str,mutation:bool,original:Option<&Value>)->Result<(Saved,Value),AppError> {
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    let principal=access::principal_from_auth(auth)?;
    let operation=if mutation {DraftOperation::Edit}else{DraftOperation::View};
    let authority=access::resolve_draft_access(&mut conn,principal,space,doc,draft,operation,Purpose::Routine).await?;
    let row=selected(&mut conn,space,doc,draft).await?;
    if original.is_some_and(|r|r!=&row) {return Err(conflict());}
    let mut proof=capture_source(&mut conn,auth,space,doc,&authority.page.ancestor_ids).await?;
    let owner=string(&row,"owner_id")?;
    let owner_rows=rows(&mut conn,"SELECT id,display_name FROM users WHERE id=$1",&[&owner]).await?;
    if owner_rows.len()!=1 {return Err(conflict());}
    let owner_name=string(&owner_rows[0],"display_name")?;
    proof.push(SourceProof{query:"SELECT id,display_name FROM users WHERE id=$1".into(),parameters:vec![owner],rows:owner_rows});
    let page=proof[0].rows.iter().find(|r|r["id"]==doc).ok_or_else(conflict)?;
    // An unpublished canonical title is not the selected private draft's label.
    let document_title=if page["published_at"].is_null() {string(&row,"title")?}else{string(page,"title")?};
    #[cfg(test)] before_read_proof().await;
    verify_source(&mut conn,&proof).await?;
    // Inventory reads may wait after earlier policy rows were compared. Prove
    // current typed participation only after the complete inventory boundary.
    #[cfg(test)] after_read_inventory().await;
    let current=access::resolve_draft_access(&mut conn,principal,space,doc,draft,operation,Purpose::Routine).await?;
    if current.participation!=authority.participation {return Err(conflict());}
    if selected(&mut conn,space,doc,draft).await?!=row {return Err(conflict());}
    // The lock observation belongs to this exact protected page, even when a
    // late change would still leave the same actor authorized to Edit.
    if rows(&mut conn,"SELECT * FROM documents WHERE id=$1 AND space_id=$2",&[doc,space]).await? != vec![page.clone()] {return Err(conflict());}
    if mutation {access::require_current_human_preference_write(&mut conn,auth).await?;}else{access::require_current_human_read(&mut conn,auth).await?;}
    Ok((Saved{row,owner_name,document_title,participation:current.participation},page.clone()))
}
fn validate(patch:&Patch)->Result<(),AppError> {
    if patch.title.as_ref().is_some_and(|s|s.trim().is_empty() || s.trim().len()>500) {return Err(AppError::BadRequest("Draft title must be between 1 and 500 characters".into()));}
    if patch.content.as_ref().is_some_and(|s|s.len()>5_242_880) {return Err(AppError::BadRequest("Draft content must be at most 5 MB".into()));}
    Ok(())
}
fn expected_row(before:&Value,patch:&Patch,stamp:&str)->Value {
    let mut out=before.clone();
    if let Some(s)=&patch.title {out["title"]=json!(s.trim());}
    for (key,s) in [("icon",&patch.icon),("cover_image",&patch.cover_image)] {
        if let Some(s)=s {out[key]=if s.trim().is_empty() {Value::Null}else{json!(s)};}
    }
    if let Some(s)=&patch.content {out["content"]=json!(s);}
    out["updated_at"]=json!(stamp);out
}
fn receipt_value(incoming:&IncomingIdentity,actor:&str,space:&str,doc:&str,draft:&str,source:&str,result:&str,result_row:&Value,stamp:&str)->Value {
    json!({"incoming":incoming,"actor_id":actor,"space_id":space,"document_id":doc,"draft_id":draft,"source_digest":source,"result_digest":result,"result_source":result_row,"created_at":stamp})
}
fn expected_receipt(incoming:&IncomingIdentity,receipt:&Value)->Result<Value,AppError> {
    Ok(json!({"provider":incoming.provider,"incoming_id":incoming.operation_id,"actor_id":string(receipt,"actor_id")?,"entity_type":incoming.entity_type,"entity_id":incoming.entity_id,"action":incoming.action,"payload_fingerprint":incoming.payload_fingerprint,"space_id":string(receipt,"space_id")?,"document_id":string(receipt,"document_id")?,"draft_id":string(receipt,"draft_id")?,"source_digest":string(receipt,"source_digest")?,"result_digest":string(receipt,"result_digest")?,"created_at":string(receipt,"created_at")?,"receipt_json":serde_json::to_string(receipt).map_err(AppError::internal)?}))
}
pub(crate) async fn has_historical(state:&AppState,auth:&AuthUser,provider:&str,id:&str)->Result<bool,AppError> {
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_preference_write(&mut conn,auth).await?;
    let found:Option<String>=sqlx::query_scalar("SELECT incoming_id FROM draft_save_replay_receipts WHERE provider=$1 AND incoming_id=$2").bind(provider).bind(id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    access::require_current_human_preference_write(&mut conn,auth).await?;
    Ok(found.is_some())
}
/// ACK-only exact incoming evidence. Never query/reapply the current draft: later
/// saves, publication, deletion and reused IDs must survive a historical retry.
pub(crate) async fn historical(state:&AppState,auth:&AuthUser,incoming:&IncomingIdentity,doc:&str,draft:&str,source:&str)->Result<bool,AppError> {
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_preference_write(&mut conn,auth).await?;
    let captured=rows(&mut conn,"SELECT * FROM draft_save_replay_receipts WHERE provider=$1 AND incoming_id=$2",&[&incoming.provider,&incoming.operation_id]).await?;
    if captured.is_empty() {access::require_current_human_preference_write(&mut conn,auth).await?;return Ok(false);}
    if captured.len()!=1 {return Err(conflict());}
    let receipt:Value=serde_json::from_str(&string(&captured[0],"receipt_json")?).map_err(|_|conflict())?;
    let result=receipt.get("result_source").ok_or_else(conflict)?;
    if digest(result)?!=string(&receipt,"result_digest")? || result["id"]!=draft || result["document_id"]!=doc || result["space_id"]!=receipt["space_id"] || result["updated_at"]!=receipt["created_at"] {return Err(conflict());}
    if receipt["incoming"]!=json!(incoming) || receipt["actor_id"]!=auth.claims.sub || receipt["document_id"]!=doc || receipt["draft_id"]!=draft || receipt["source_digest"]!=source || captured!=vec![expected_receipt(incoming,&receipt)?] {return Err(conflict());}
    if rows(&mut conn,"SELECT * FROM draft_save_replay_receipts WHERE provider=$1 AND incoming_id=$2",&[&incoming.provider,&incoming.operation_id]).await?!=captured {return Err(conflict());}
    access::require_current_human_preference_write(&mut conn,auth).await?;
    Ok(true)
}
/// The ONLY save writer. REST has no synthesized incoming receipt; replay passes
/// its original exact provider/op/payload identity. Unknown commit stays reviewable.
pub(crate) async fn save(state:&AppState,auth:&AuthUser,space:&str,doc:&str,draft:&str,source:&str,patch:Patch,stamp:&str,incoming:Option<&IncomingIdentity>)->Result<Saved,AppError> {
    required(Some(&json!(source)))?;validate(&patch)?;
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)] before_lock().await;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx,&[space]).await?;
    let n=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if n.rows_affected()!=1 {return Err(AppError::NotFound("Document unavailable".into()));}
    sqlx::query("UPDATE document_drafts SET id=id WHERE document_id=$1 AND space_id=$2").bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id IN(SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2)").bind(doc).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    let principal=access::principal_from_auth(auth)?;
    access::resolve_page_access(&mut tx,principal,space,doc,PageOperation::Edit,Purpose::Routine).await?;
    let access=access::resolve_draft_access(&mut tx,principal,space,doc,draft,DraftOperation::Edit,Purpose::Routine).await?;
    let before=selected(&mut tx,space,doc,draft).await?;
    if digest(&before)?!=source {return Err(conflict());}
    let mut expected=capture_source(&mut tx,auth,space,doc,&access.page.ancestor_ids).await?;
    let owner=string(&before,"owner_id")?;
    expected.push(SourceProof{query:"SELECT id,display_name FROM users WHERE id=$1".into(),parameters:vec![owner.clone()],rows:rows(&mut tx,"SELECT id,display_name FROM users WHERE id=$1",&[&owner]).await?});
    let next=expected_row(&before,&patch,stamp);
    let owner_name:String=sqlx::query_scalar("SELECT display_name FROM users WHERE id=$1").bind(string(&before,"owner_id")?).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let page=expected[0].rows.iter().find(|r|r["id"]==doc).ok_or_else(conflict)?;
    let document_title=if page["published_at"].is_null() {string(&next,"title")?}else{string(page,"title")?};
    // Conditional full known row, including nullable presentation and placement.
    // SELECT * digest and final outcome also bind any future additional raw columns.
    let n=sqlx::query("UPDATE document_drafts SET title=$1,icon=$2,cover_image=$3,content=$4,updated_at=$5 WHERE id=$6 AND document_id=$7 AND space_id=$8 AND owner_id=$9 AND title=$10 AND content=$11 AND base_updated_at=$12 AND created_at=$13 AND updated_at=$14 AND ((icon=$15) OR (icon IS NULL AND $15 IS NULL)) AND ((cover_image=$16) OR (cover_image IS NULL AND $16 IS NULL)) AND ((previous_folder_id=$17) OR (previous_folder_id IS NULL AND $17 IS NULL)) AND ((previous_parent_id=$18) OR (previous_parent_id IS NULL AND $18 IS NULL))")
        .bind(string(&next,"title")?).bind(optional(&next,"icon")?).bind(optional(&next,"cover_image")?).bind(string(&next,"content")?).bind(stamp).bind(draft).bind(doc).bind(space).bind(string(&before,"owner_id")?).bind(string(&before,"title")?).bind(string(&before,"content")?).bind(string(&before,"base_updated_at")?).bind(string(&before,"created_at")?).bind(string(&before,"updated_at")?).bind(optional(&before,"icon")?).bind(optional(&before,"cover_image")?).bind(optional(&before,"previous_folder_id")?).bind(optional(&before,"previous_parent_id")?).execute(&mut *tx).await.map_err(AppError::internal)?;
    if n.rows_affected()!=1 {return Err(conflict());}
    let proof=expected.iter_mut().find(|p|p.query=="SELECT * FROM document_drafts WHERE document_id=$1" && p.parameters==vec![doc.to_owned()]).ok_or_else(conflict)?;
    let row=proof.rows.iter_mut().find(|r|r["id"]==draft).ok_or_else(conflict)?;*row=next.clone();proof.rows.sort_by_cached_key(Value::to_string);
    let mut audits=Vec::new();
    for action in ["document.draft_update","document.draft_autosave"] {
        let id=Uuid::new_v4().to_string();
        let created:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,$3,'document_draft',$4) RETURNING created_at").bind(&id).bind(&auth.claims.sub).bind(action).bind(draft).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
        audits.push(json!({"id":id,"user_id":auth.claims.sub,"actor_kind":"user","actor_id":auth.claims.sub,"action":action,"target_type":"document_draft","target_id":draft,"created_at":created,"ip_address":null,"before_value":null,"after_value":null}));
    }
    let receipt=if let Some(incoming)=incoming {
        let r=receipt_value(incoming,&auth.claims.sub,space,doc,draft,source,&digest(&next)?,&next,stamp);
        sqlx::query("INSERT INTO draft_save_replay_receipts(provider,incoming_id,actor_id,entity_type,entity_id,action,payload_fingerprint,space_id,document_id,draft_id,source_digest,result_digest,created_at,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)")
            .bind(&incoming.provider).bind(&incoming.operation_id).bind(&auth.claims.sub).bind(&incoming.entity_type).bind(&incoming.entity_id).bind(&incoming.action).bind(&incoming.payload_fingerprint).bind(space).bind(doc).bind(draft).bind(source).bind(digest(&next)?).bind(stamp).bind(serde_json::to_string(&r).map_err(AppError::internal)?).execute(&mut *tx).await.map_err(AppError::internal)?;
        Some((incoming,r))
    }else{None};
    #[cfg(test)] after_writes().await;
    for audit in &audits {if rows(&mut tx,"SELECT * FROM audit_log WHERE id=$1",&[audit["id"].as_str().ok_or_else(conflict)?]).await?!=vec![audit.clone()] {return Err(conflict());}}
    if let Some((incoming,r))=&receipt {if rows(&mut tx,"SELECT * FROM draft_save_replay_receipts WHERE provider=$1 AND incoming_id=$2",&[&incoming.provider,&incoming.operation_id]).await?!=vec![expected_receipt(incoming,r)?] {return Err(conflict());}}
    verify_source(&mut tx,&expected).await?;
    access::resolve_page_access(&mut tx,principal,space,doc,PageOperation::Edit,Purpose::Routine).await?;
    let final_access=access::resolve_draft_access(&mut tx,principal,space,doc,draft,DraftOperation::Edit,Purpose::Routine).await?;
    if final_access.participation!=access.participation {return Err(conflict());}
    if selected(&mut tx,space,doc,draft).await?!=next {return Err(conflict());}
    #[cfg(test)] before_identity().await;
    access::require_current_human_preference_write(&mut tx,auth).await?;
    tx.commit().await.map_err(|_|AppError::Conflict("Draft save outcome is unknown; retain the original edit and operation for review".into()))?;
    crate::sync::drafts::notify_commit(state, &crate::sync::drafts::DraftKey { space_id: space.into(), document_id: doc.into(), draft_id: draft.into() }).await;
    for audit in &audits {crate::audit::append_committed_move_action(audit).await;}
    // Mutable mirrors are deliberately deferred; SQL source is authoritative.
    Ok(Saved{row:next,owner_name,document_title,participation:final_access.participation})
}
#[cfg(test)] tokio::task_local! {
    pub(crate) static AFTER_READ_INVENTORY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static BEFORE_READ_PROOF:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static BEFORE_LOCK:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static AFTER_WRITES:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
}
#[cfg(test)] async fn after_writes(){if let Ok(Some(h))=AFTER_WRITES.try_with(|s|s.borrow_mut().take()){h.await}}
#[cfg(test)] async fn before_identity(){if let Ok(Some(h))=BEFORE_IDENTITY.try_with(|s|s.borrow_mut().take()){h.await}}

#[cfg(test)] async fn before_lock(){if let Ok(Some(h))=BEFORE_LOCK.try_with(|s|s.borrow_mut().take()){h.await}}

#[cfg(test)] async fn before_read_proof(){if let Ok(Some(h))=BEFORE_READ_PROOF.try_with(|s|s.borrow_mut().take()){h.await}}

#[cfg(test)] async fn after_read_inventory(){if let Ok(Some(h))=AFTER_READ_INVENTORY.try_with(|s|s.borrow_mut().take()){h.await}}

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{AnyConnection, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

use crate::{auth::{middleware::AuthUser, routes::AppError}, AppState};
use super::access::{DraftParticipation, PageOperation, Purpose};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space_id}/documents/{document_id}/drafts/{draft_id}/discussion",
            get(list_active).post(create_comment),
        )
        .route(
            "/spaces/{space_id}/documents/{document_id}/drafts/{draft_id}/discussion/{comment_id}",
            axum::routing::put(update_comment).delete(delete_comment),
        )
        .route(
            "/spaces/{space_id}/documents/{document_id}/drafts/{draft_id}/discussion/{comment_id}/resolve",
            axum::routing::post(resolve_comment),
        )
        .route(
            "/spaces/{space_id}/documents/{document_id}/draft-discussions",
            get(list_closed_custody),
        )
        .route(
            "/spaces/{space_id}/documents/{document_id}/draft-discussions/{discussion_id}",
            get(list_closed),
        )
}

#[derive(Deserialize)]
pub struct SourceRequest { pub source_digest: String }

#[derive(Deserialize)]
pub struct CreateRequest {
    pub source_digest: String,
    pub content: String,
    pub parent_id: Option<String>,
    pub selection_start: Option<i64>,
    pub selection_end: Option<i64>,
    pub selection_text: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateRequest { pub source_digest: String, pub content: String }

#[derive(Serialize)]
pub struct DiscussionResponse {
    pub discussion_id: Option<String>,
    pub draft_id: String,
    pub source_digest: String,
    pub state: String,
    pub mention_candidates: Vec<MentionCandidate>,
    pub comments: Vec<CommentResponse>,
}

#[derive(Clone, Serialize)]
pub struct MentionCandidate {
    pub user_id: String,
    pub display_name: String,
    pub email: String,
}

#[derive(Serialize)]
pub struct ClosedDiscussionSummary {
    pub id: String,
    pub draft_id: String,
    pub publication_operation_id: String,
    pub closed_at: String,
    pub comment_count: i64,
    pub unresolved_count: i64,
}

#[derive(Serialize)]
pub struct ClosedDiscussionListResponse {
    pub schema_version: i64,
    pub space_id: String,
    pub document_id: String,
    pub discussions: Vec<ClosedDiscussionSummary>,
}

#[derive(Clone, Serialize)]
pub struct CommentResponse {
    pub id: String,
    pub draft_id: String,
    pub source_digest: String,
    pub parent_id: Option<String>,
    pub user_id: String,
    pub user_name: String,
    pub content: String,
    pub selection_start: Option<i64>,
    pub selection_end: Option<i64>,
    pub selection_text: Option<String>,
    pub anchor_outdated: bool,
    pub resolved: bool,
    pub resolved_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub replies: Vec<CommentResponse>,
}

type StoredComment = (String,Option<String>,String,String,String,String,Option<i64>,Option<i64>,Option<String>,i64,Option<String>,String,String);

fn valid_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_body(content: &str) -> Result<(), AppError> {
    if content.is_empty() { return Err(AppError::BadRequest("Comment body is required".into())); }
    if content.len() > 10_000 { return Err(AppError::BadRequest("Comment body must be at most 10000 bytes".into())); }
    Ok(())
}

fn utf16_slice(source: &str, start: i64, end: i64) -> Option<String> {
    let start=usize::try_from(start).ok()?; let end=usize::try_from(end).ok()?;
    if start>=end { return None; }
    let units=source.encode_utf16().collect::<Vec<_>>();
    if end>units.len() { return None; }
    String::from_utf16(&units[start..end]).ok()
}

fn validate_anchor(req: &CreateRequest, content: &str) -> Result<(), AppError> {
    match (req.selection_start,req.selection_end,req.selection_text.as_deref()) {
        (None,None,None)=>Ok(()),
        (Some(start),Some(end),Some(text)) if utf16_slice(content,start,end).as_deref()==Some(text)=>Ok(()),
        _=>Err(AppError::BadRequest("Draft comment anchor must match exact UTF-16 scalar boundaries in the selected source".into())),
    }
}

fn participation_label(value: DraftParticipation) -> Result<&'static str, AppError> {
    match value {
        DraftParticipation::Owner=>Ok("owner"),
        DraftParticipation::SharedViewer=>Ok("viewer"),
        DraftParticipation::SharedEditor=>Ok("editor"),
        DraftParticipation::RecoveryAuditRequired=>Err(AppError::Forbidden("Draft discussion is unavailable during recovery audit".into())),
    }
}

struct ActiveSource { row: Value, digest: String, content: String, participation: &'static str }

async fn active_source(
    conn:&mut AnyConnection, auth:&AuthUser, space:&str, document:&str, draft:&str, supplied:&str,
) -> Result<ActiveSource,AppError> {
    if !valid_digest(supplied) { return Err(AppError::Conflict("source_required: Draft discussion requires the exact current draft source digest".into())); }
    let authority=super::access::resolve_draft_comment_access(conn,super::access::principal_from_auth(auth)?,space,document,draft,Purpose::Routine).await?;
    let row=super::draft_save::selected(conn,space,document,draft).await?;
    let digest=super::draft_save::digest(&row)?;
    if digest!=supplied { return Err(AppError::Conflict("source_conflict: Draft discussion source changed; refresh before continuing".into())); }
    let content=super::draft_save::string(&row,"content")?;
    Ok(ActiveSource{row,digest,content,participation:participation_label(authority.participation)?})
}

async fn lock_draft(conn:&mut AnyConnection,space:&str,document:&str,draft:&str)->Result<(),AppError>{
    let result=sqlx::query("UPDATE document_drafts SET id=id WHERE id=$1 AND document_id=$2 AND space_id=$3")
        .bind(draft).bind(document).bind(space).execute(conn).await.map_err(AppError::internal)?;
    if result.rows_affected()!=1 { return Err(AppError::NotFound("Draft unavailable".into())); }
    Ok(())
}

async fn ensure_context(
    conn:&mut AnyConnection, auth:&AuthUser, space:&str, document:&str, draft:&str, source:&ActiveSource,
) -> Result<String,AppError> {
    let existing:Option<(String,String,String,String,String,String)>=sqlx::query_as(
        "SELECT id,space_id,document_id,draft_created_at,draft_owner_id,state FROM draft_discussions WHERE draft_id=$1"
    ).bind(draft).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let owner=super::draft_save::string(&source.row,"owner_id")?;
    let created=super::draft_save::string(&source.row,"created_at")?;
    let now=chrono::Utc::now().to_rfc3339();
    let id=if let Some((id,s,d,c,o,state))=existing {
        if s!=space||d!=document||c!=created||o!=owner||state!="active" { return Err(AppError::Conflict("Draft discussion custody changed; retain the request for review".into())); }
        sqlx::query("UPDATE draft_discussions SET current_source_digest=$1,updated_at=$2 WHERE id=$3 AND state='active'")
            .bind(&source.digest).bind(&now).bind(&id).execute(&mut *conn).await.map_err(AppError::internal)?;
        id
    } else {
        let id=Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO draft_discussions(id,space_id,document_id,draft_id,draft_created_at,draft_owner_id,current_source_digest,state,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,'active',$8,$8)")
            .bind(&id).bind(space).bind(document).bind(draft).bind(&created).bind(&owner).bind(&source.digest).bind(&now)
            .execute(&mut *conn).await.map_err(AppError::internal)?;
        id
    };
    sqlx::query("INSERT INTO draft_discussion_participants(discussion_id,user_id,participation,first_seen_at,last_seen_at) VALUES($1,$2,$3,$4,$4) ON CONFLICT(discussion_id,user_id) DO UPDATE SET participation=excluded.participation,last_seen_at=excluded.last_seen_at")
        .bind(&id).bind(&auth.claims.sub).bind(source.participation).bind(&now).execute(&mut *conn).await.map_err(AppError::internal)?;
    Ok(id)
}

async fn context_id(conn:&mut AnyConnection,space:&str,document:&str,draft:&str)->Result<Option<String>,AppError>{
    sqlx::query_scalar("SELECT id FROM draft_discussions WHERE space_id=$1 AND document_id=$2 AND draft_id=$3 AND state='active'")
        .bind(space).bind(document).bind(draft).fetch_optional(conn).await.map_err(AppError::internal)
}

async fn rows(conn:&mut AnyConnection,discussion:&str)->Result<Vec<StoredComment>,AppError>{
    sqlx::query_as("SELECT c.id,c.parent_id,c.user_id,u.display_name,c.content,c.source_digest,c.selection_start,c.selection_end,c.selection_text,c.resolved,c.resolved_by,c.created_at,c.updated_at FROM draft_discussion_comments c JOIN users u ON u.id=c.user_id WHERE c.discussion_id=$1 ORDER BY c.created_at,c.id")
        .bind(discussion).fetch_all(conn).await.map_err(AppError::internal)
}

fn response(discussion:Option<String>,draft:&str,digest:&str,state:&str,mention_candidates:Vec<MentionCandidate>,stored:Vec<StoredComment>,content:Option<&str>)->DiscussionResponse{
    let make=|r:&StoredComment|CommentResponse{id:r.0.clone(),draft_id:draft.into(),source_digest:r.5.clone(),parent_id:r.1.clone(),user_id:r.2.clone(),user_name:r.3.clone(),content:r.4.clone(),selection_start:r.6,selection_end:r.7,selection_text:r.8.clone(),anchor_outdated:r.6.is_some()&&(r.5!=digest||content.and_then(|body|utf16_slice(body,r.6.unwrap(),r.7.unwrap())).as_deref()!=r.8.as_deref()),resolved:r.9!=0,resolved_by:r.10.clone(),created_at:r.11.clone(),updated_at:r.12.clone(),replies:vec![]};
    let mut top=Vec::new();
    for row in stored.iter().filter(|r|r.1.is_none()) { let mut item=make(row); item.replies=stored.iter().filter(|r|r.1.as_deref()==Some(row.0.as_str())).map(&make).collect(); top.push(item); }
    DiscussionResponse{discussion_id:discussion,draft_id:draft.into(),source_digest:digest.into(),state:state.into(),mention_candidates,comments:top}
}

async fn mention_candidates(conn:&mut AnyConnection,space:&str,document:&str,draft:&str)->Result<Vec<MentionCandidate>,AppError>{
    let users:Vec<(String,String,String)>=sqlx::query_as(
        "SELECT u.id,u.display_name,u.email FROM users u JOIN document_drafts d ON d.id=$1 AND d.document_id=$2 AND d.space_id=$3 WHERE u.is_active=1 AND u.account_kind='member' AND (u.id=d.owner_id OR EXISTS(SELECT 1 FROM document_draft_access a WHERE a.draft_id=d.id AND a.user_id=u.id AND a.role IN ('viewer','editor'))) ORDER BY u.display_name,u.id LIMIT 100"
    ).bind(draft).bind(document).bind(space).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut eligible=Vec::new();
    for (user_id,display_name,email) in users {
        if super::access::resolve_draft_comment_access(
            conn,
            super::access::Principal::User{user_id:&user_id,session_id:None,token_expires_at:None},
            space,document,draft,Purpose::Routine,
        ).await.is_ok(){
            eligible.push(MentionCandidate{user_id,display_name,email});
        }
    }
    Ok(eligible)
}

async fn list_active(State(state):State<AppState>,auth:AuthUser,Path((space,document,draft)):Path<(String,String,String)>,Query(req):Query<SourceRequest>)->Result<impl IntoResponse,AppError>{
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    let source=active_source(&mut conn,&auth,&space,&document,&draft,&req.source_digest).await?;
    let id=context_id(&mut conn,&space,&document,&draft).await?;
    let stored=if let Some(id)=&id {rows(&mut conn,id).await?}else{vec![]};
    let candidates=mention_candidates(&mut conn,&space,&document,&draft).await?;
    active_source(&mut conn,&auth,&space,&document,&draft,&req.source_digest).await?;
    super::access::require_current_human_read(&mut conn,&auth).await?;
    Ok(Json(response(id,&draft,&source.digest,"active",candidates,stored,Some(&source.content))))
}

async fn begin_mutation<'a>(state:&'a AppState,auth:&AuthUser,space:&str,document:&str,draft:&str,digest:&str)->Result<(Transaction<'a,sqlx::Any>,ActiveSource,String),AppError>{
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx,&[space]).await?;
    lock_draft(&mut tx,space,document,draft).await?;
    let source=active_source(&mut tx,auth,space,document,draft,digest).await?;
    let context=ensure_context(&mut tx,auth,space,document,draft,&source).await?;
    Ok((tx,source,context))
}

async fn finish(mut tx:Transaction<'_,sqlx::Any>,auth:&AuthUser,space:&str,document:&str,draft:&str,digest:&str)->Result<(),AppError>{
    active_source(&mut tx,auth,space,document,draft,digest).await?;
    super::access::require_current_human_preference_write(&mut tx,auth).await?;
    tx.commit().await.map_err(AppError::internal)
}

async fn deliver_comment_notifications(
    state:&AppState, actor:&str, discussion:&str, content:&str, reply:bool,
    subscribers:Vec<String>, mentioned:BTreeSet<String>,
){
    for recipient in &mentioned {
        if recipient==actor { continue; }
        crate::notifications::notify(
            &state.db,recipient,"mention","You were mentioned in a shared draft",
            Some(content),Some(actor),Some("draft_discussion"),Some(discussion),
        ).await;
    }
    let notification_type=if reply{"comment_reply"}else{"comment"};
    let title=if reply{"New reply in a shared draft"}else{"New comment in a shared draft"};
    for recipient in subscribers {
        if recipient==actor||mentioned.contains(&recipient){continue;}
        crate::notifications::notify(
            &state.db,&recipient,notification_type,title,Some(content),Some(actor),
            Some("draft_discussion"),Some(discussion),
        ).await;
    }
}

async fn deliver_edited_mentions(state:&AppState,actor:&str,discussion:&str,content:&str,mentioned:BTreeSet<String>){
    for recipient in mentioned {
        if recipient==actor{continue;}
        crate::notifications::notify(
            &state.db,&recipient,"mention","You were mentioned in a shared draft",
            Some(content),Some(actor),Some("draft_discussion"),Some(discussion),
        ).await;
    }
}

async fn create_comment(State(state):State<AppState>,auth:AuthUser,Path((space,document,draft)):Path<(String,String,String)>,Json(req):Json<CreateRequest>)->Result<impl IntoResponse,AppError>{
    validate_body(&req.content)?;
    let mentioned=super::documents::portable_mention_user_ids(&req.content);
    let (mut tx,source,context)=begin_mutation(&state,&auth,&space,&document,&draft,&req.source_digest).await?;
    validate_anchor(&req,&source.content)?;
    if let Some(parent)=&req.parent_id {
        let found:Option<String>=sqlx::query_scalar("SELECT id FROM draft_discussion_comments WHERE id=$1 AND discussion_id=$2 AND parent_id IS NULL").bind(parent).bind(&context).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
        if found.is_none(){return Err(AppError::NotFound("Draft comment unavailable".into()));}
    }
    let id=Uuid::new_v4().to_string(); let now=chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO draft_discussion_comments(id,discussion_id,parent_id,user_id,content,source_digest,selection_start,selection_end,selection_text,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10)")
        .bind(&id).bind(&context).bind(&req.parent_id).bind(&auth.claims.sub).bind(&req.content).bind(&source.digest).bind(req.selection_start).bind(req.selection_end).bind(&req.selection_text).bind(&now).execute(&mut *tx).await.map_err(AppError::internal)?;
    sqlx::query("INSERT INTO draft_discussion_subscriptions(discussion_id,user_id,created_at) VALUES($1,$2,$3) ON CONFLICT(discussion_id,user_id) DO NOTHING")
        .bind(&context).bind(&auth.claims.sub).bind(&now).execute(&mut *tx).await.map_err(AppError::internal)?;
    let subscribers:Vec<String>=sqlx::query_scalar("SELECT user_id FROM draft_discussion_subscriptions WHERE discussion_id=$1 ORDER BY user_id")
        .bind(&context).fetch_all(&mut *tx).await.map_err(AppError::internal)?;
    finish(tx,&auth,&space,&document,&draft,&req.source_digest).await?;
    deliver_comment_notifications(&state,&auth.claims.sub,&context,&req.content,req.parent_id.is_some(),subscribers,mentioned).await;
    Ok((StatusCode::CREATED,Json(serde_json::json!({"id":id,"discussion_id":context}))))
}

async fn update_comment(State(state):State<AppState>,auth:AuthUser,Path((space,document,draft,id)):Path<(String,String,String,String)>,Json(req):Json<UpdateRequest>)->Result<impl IntoResponse,AppError>{
    validate_body(&req.content)?; let(mut tx,_,context)=begin_mutation(&state,&auth,&space,&document,&draft,&req.source_digest).await?;
    let previous:Option<String>=sqlx::query_scalar("SELECT content FROM draft_discussion_comments WHERE id=$1 AND discussion_id=$2 AND user_id=$3")
        .bind(&id).bind(&context).bind(&auth.claims.sub).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    let Some(previous)=previous else{return Err(AppError::Forbidden("Only the draft comment author can modify this comment".into()));};
    let mentioned=super::documents::newly_added_portable_mention_user_ids(&previous,&req.content);
    let changed=sqlx::query("UPDATE draft_discussion_comments SET content=$1,updated_at=$2 WHERE id=$3 AND discussion_id=$4 AND user_id=$5")
        .bind(&req.content).bind(chrono::Utc::now().to_rfc3339()).bind(&id).bind(&context).bind(&auth.claims.sub).execute(&mut *tx).await.map_err(AppError::internal)?;
    if changed.rows_affected()!=1{return Err(AppError::Forbidden("Only the draft comment author can modify this comment".into()));}
    finish(tx,&auth,&space,&document,&draft,&req.source_digest).await?;
    deliver_edited_mentions(&state,&auth.claims.sub,&context,&req.content,mentioned).await;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_comment(State(state):State<AppState>,auth:AuthUser,Path((space,document,draft,id)):Path<(String,String,String,String)>,Query(req):Query<SourceRequest>)->Result<impl IntoResponse,AppError>{
    let(mut tx,_,context)=begin_mutation(&state,&auth,&space,&document,&draft,&req.source_digest).await?;
    let author:Option<String>=sqlx::query_scalar("SELECT user_id FROM draft_discussion_comments WHERE id=$1 AND discussion_id=$2").bind(&id).bind(&context).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if author.as_deref()!=Some(auth.claims.sub.as_str()){return Err(AppError::Forbidden("Only the draft comment author can delete this comment".into()));}
    let replies:i64=sqlx::query_scalar("SELECT COUNT(*) FROM draft_discussion_comments WHERE discussion_id=$1 AND parent_id=$2").bind(&context).bind(&id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    if replies!=0{return Err(AppError::Conflict("Draft comments with replies must be retained".into()));}
    sqlx::query("DELETE FROM draft_discussion_comments WHERE id=$1 AND discussion_id=$2 AND user_id=$3").bind(&id).bind(&context).bind(&auth.claims.sub).execute(&mut *tx).await.map_err(AppError::internal)?;
    finish(tx,&auth,&space,&document,&draft,&req.source_digest).await?; Ok(StatusCode::NO_CONTENT)
}

async fn resolve_comment(State(state):State<AppState>,auth:AuthUser,Path((space,document,draft,id)):Path<(String,String,String,String)>,Query(req):Query<SourceRequest>)->Result<impl IntoResponse,AppError>{
    let(mut tx,_,context)=begin_mutation(&state,&auth,&space,&document,&draft,&req.source_digest).await?; let now=chrono::Utc::now().to_rfc3339();
    let changed=sqlx::query("UPDATE draft_discussion_comments SET resolved=1,resolved_by=$1,resolved_at=$2,updated_at=$2 WHERE id=$3 AND discussion_id=$4")
        .bind(&auth.claims.sub).bind(&now).bind(&id).bind(&context).execute(&mut *tx).await.map_err(AppError::internal)?;
    if changed.rows_affected()!=1{return Err(AppError::NotFound("Draft comment unavailable".into()));}
    finish(tx,&auth,&space,&document,&draft,&req.source_digest).await?; Ok(StatusCode::NO_CONTENT)
}

async fn list_closed_custody(State(state):State<AppState>,auth:AuthUser,Path((space,document)):Path<(String,String)>)->Result<impl IntoResponse,AppError>{
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    super::access::resolve_page_access(&mut conn,super::access::principal_from_auth(&auth)?,&space,&document,PageOperation::Comment,Purpose::Routine).await?;
    super::access::require_current_human_read(&mut conn,&auth).await?;
    let rows:Vec<(String,String,String,String,i64,i64)>=sqlx::query_as(
        "SELECT d.id,d.draft_id,d.publication_operation_id,d.closed_at,\
         (SELECT COUNT(*) FROM draft_discussion_comments c WHERE c.discussion_id=d.id),\
         (SELECT COUNT(*) FROM draft_discussion_comments c WHERE c.discussion_id=d.id AND c.parent_id IS NULL AND c.resolved=0)\
         FROM draft_discussions d WHERE d.space_id=$1 AND d.document_id=$2 AND d.draft_owner_id=$3 AND d.state='closed'\
         ORDER BY d.closed_at DESC,d.id LIMIT 100"
    ).bind(&space).bind(&document).bind(&auth.claims.sub).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    super::access::resolve_page_access(&mut conn,super::access::principal_from_auth(&auth)?,&space,&document,PageOperation::Comment,Purpose::Routine).await?;
    super::access::require_current_human_read(&mut conn,&auth).await?;
    Ok(Json(ClosedDiscussionListResponse{schema_version:1,space_id:space,document_id:document,discussions:rows.into_iter().map(|row|ClosedDiscussionSummary{id:row.0,draft_id:row.1,publication_operation_id:row.2,closed_at:row.3,comment_count:row.4,unresolved_count:row.5}).collect()}))
}

async fn list_closed(State(state):State<AppState>,auth:AuthUser,Path((space,document,discussion)):Path<(String,String,String)>)->Result<impl IntoResponse,AppError>{
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    let row:Option<(String,String,String,String,String)>=sqlx::query_as("SELECT draft_id,draft_owner_id,current_source_digest,state,space_id FROM draft_discussions WHERE id=$1 AND document_id=$2")
        .bind(&discussion).bind(&document).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let Some((draft,owner,digest,state,stored_space))=row else{return Err(AppError::NotFound("Draft discussion unavailable".into()));};
    if stored_space!=space||state!="closed"{return Err(AppError::NotFound("Draft discussion unavailable".into()));}
    super::access::resolve_page_access(&mut conn,super::access::principal_from_auth(&auth)?,&space,&document,PageOperation::Comment,Purpose::Routine).await?;
    if owner!=auth.claims.sub{return Err(AppError::Forbidden("Closed draft discussion is available only to its original owner".into()));}
    let stored=rows(&mut conn,&discussion).await?;
    super::access::resolve_page_access(&mut conn,super::access::principal_from_auth(&auth)?,&space,&document,PageOperation::Comment,Purpose::Routine).await?;
    super::access::require_current_human_read(&mut conn,&auth).await?;
    Ok(Json(response(Some(discussion),&draft,&digest,"closed",vec![],stored,None)))
}

/// Close private discussion custody inside the owner publication transaction.
/// The selected draft is still present and authority has already been proved.
pub(crate) async fn close_for_publication(conn:&mut AnyConnection,space:&str,document:&str,draft:&str,operation:&str,source_digest:&str,closed_at:&str)->Result<(),AppError>{
    let contexts:Vec<String>=sqlx::query_scalar("SELECT id FROM draft_discussions WHERE space_id=$1 AND document_id=$2 AND draft_id=$3 AND state='active'")
        .bind(space).bind(document).bind(draft).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    if contexts.len()>1{return Err(AppError::Conflict("Draft discussion custody is ambiguous".into()));}
    if let Some(id)=contexts.first(){
        let changed=sqlx::query("UPDATE draft_discussions SET state='closed',publication_operation_id=$1,closed_at=$2,current_source_digest=$3,updated_at=$2 WHERE id=$4 AND state='active'")
            .bind(operation).bind(closed_at).bind(source_digest).bind(id).execute(&mut *conn).await.map_err(AppError::internal)?;
        if changed.rows_affected()!=1{return Err(AppError::Conflict("Draft discussion changed during publication".into()));}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, extract::{Path, Query, State}, response::IntoResponse, Json};

    fn auth(user: &str) -> AuthUser {
        crate::spaces::properties::tests::test_auth(user)
    }

    async fn state() -> AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        add_draft(&state).await;
        state
    }

    async fn add_draft(state: &AppState) {
        sqlx::query(
            "INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES('draft-a','target-a','space-a','owner-a','Private','Alpha 😀 Beta','published','2026-10-21T00:00:00Z','2026-10-21T00:00:00Z')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft-a','editor-a','editor','owner-a'),('draft-a','viewer-a','viewer','owner-a')")
            .execute(&state.db)
            .await
            .unwrap();
    }

    async fn postgres_state() -> AppState {
        sqlx::any::install_default_drivers();
        let url = std::env::var("STARLANE_TEST_POSTGRES_URL")
            .expect("STARLANE_TEST_POSTGRES_URL must name a disposable empty database");
        let db = sqlx::any::AnyPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .unwrap();
        crate::db::migrate_selected(&db, crate::db::DatabaseEngine::Postgres)
            .await
            .unwrap();
        for id in ["owner-a", "viewer-a", "editor-a"] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')")
                .bind(id)
                .bind(format!("{id}@example.test"))
                .execute(&db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'discussion-test','2999-01-01T00:00:00Z')")
                .bind(format!("metadata-{id}"))
                .bind(id)
                .execute(&db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-a','space-a','space-a','private','owner-a')")
            .execute(&db)
            .await
            .unwrap();
        for (user, role) in [
            ("owner-a", "owner"),
            ("viewer-a", "viewer"),
            ("editor-a", "editor"),
        ] {
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-a',$2,$3)")
                .bind(format!("member-{user}"))
                .bind(user)
                .bind(role)
                .execute(&db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','target-a','','owner-a','owner-a','published')")
            .execute(&db)
            .await
            .unwrap();
        let state = AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".into(),
            base_url: "http://localhost:3000".into(),
            search_index: crate::search::SearchIndex::new(std::env::temp_dir().join(format!(
                "starlane-draft-discussion-postgres-{}",
                Uuid::new_v4()
            )))
            .unwrap(),
            oauth_cache: crate::auth::routes::OAuthStateCache::new(),
            ws_rooms: crate::sync::WsRooms::default(),
        };
        add_draft(&state).await;
        state
    }

    async fn digest(state: &AppState) -> String {
        let mut conn = state.db.acquire().await.unwrap();
        let row = super::super::draft_save::selected(
            &mut conn,
            "space-a",
            "target-a",
            "draft-a",
        )
        .await
        .unwrap();
        super::super::draft_save::digest(&row).unwrap()
    }

    fn active_path() -> Path<(String, String, String)> {
        Path(("space-a".into(), "target-a".into(), "draft-a".into()))
    }

    async fn json_body(response: impl IntoResponse) -> Value {
        let response = response.into_response();
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
    }

    #[test]
    fn source_and_anchor_contract_rejects_ambiguous_input() {
        assert!(valid_digest(&"a".repeat(64)));
        assert!(!valid_digest(&"A".repeat(64)));
        assert!(!valid_digest("short"));
        assert_eq!(utf16_slice("Alpha 😀 Beta", 6, 8).as_deref(), Some("😀"));
        assert!(utf16_slice("Alpha 😀 Beta", 6, 7).is_none());
        assert!(validate_anchor(
            &CreateRequest {
                source_digest: "a".repeat(64),
                content: "note".into(),
                parent_id: None,
                selection_start: Some(6),
                selection_end: Some(8),
                selection_text: Some("😀".into()),
            },
            "Alpha 😀 Beta",
        )
        .is_ok());
    }

    async fn exercise_selected_draft_discussion(state: AppState) {
        for (id,user,kind) in [
            ("owner-mention","owner-a","mention"),
            ("editor-reply","editor-a","comment_reply"),
        ] {
            sqlx::query("INSERT INTO notification_prefs(id,user_id,notification_type,channel) VALUES($1,$2,$3,'in_app')")
                .bind(id).bind(user).bind(kind).execute(&state.db).await.unwrap();
        }
        let source = digest(&state).await;
        let initial = list_active(
            State(state.clone()),
            auth("editor-a"),
            active_path(),
            Query(SourceRequest { source_digest: source.clone() }),
        ).await.unwrap();
        let initial = json_body(initial).await;
        let candidates = initial["mention_candidates"].as_array().unwrap();
        assert!(candidates.iter().any(|candidate| candidate["user_id"] == "owner-a"));
        assert!(candidates.iter().any(|candidate| candidate["user_id"] == "editor-a"));
        let created = create_comment(
            State(state.clone()),
            auth("editor-a"),
            active_path(),
            Json(CreateRequest {
                source_digest: source.clone(),
                content: "Check the emoji".into(),
                parent_id: None,
                selection_start: Some(6),
                selection_end: Some(8),
                selection_text: Some("😀".into()),
            }),
        )
        .await
        .unwrap();
        let created = json_body(created).await;
        let discussion = created["discussion_id"].as_str().unwrap().to_owned();
        let comment = created["id"].as_str().unwrap().to_owned();

        update_comment(
            State(state.clone()),
            auth("editor-a"),
            Path(("space-a".into(), "target-a".into(), "draft-a".into(), comment.clone())),
            Json(UpdateRequest { source_digest: source.clone(), content: "Check this emoji @[Owner](owner-a)".into() }),
        ).await.unwrap();
        let owner_notification:(String,String,String)=sqlx::query_as("SELECT type,target_type,target_id FROM notifications WHERE user_id='owner-a' ORDER BY created_at DESC,id DESC LIMIT 1")
            .fetch_one(&state.db).await.unwrap();
        assert_eq!(owner_notification,("mention".into(),"draft_discussion".into(),discussion.clone()));
        let reply = create_comment(
            State(state.clone()),
            auth("owner-a"),
            active_path(),
            Json(CreateRequest {
                source_digest: source.clone(),
                content: "Confirmed".into(),
                parent_id: Some(comment.clone()),
                selection_start: None,
                selection_end: None,
                selection_text: None,
            }),
        ).await.unwrap();
        let reply = json_body(reply).await["id"].as_str().unwrap().to_owned();
        let editor_notification:(String,String,String)=sqlx::query_as("SELECT type,target_type,target_id FROM notifications WHERE user_id='editor-a' ORDER BY created_at DESC,id DESC LIMIT 1")
            .fetch_one(&state.db).await.unwrap();
        assert_eq!(editor_notification,("comment_reply".into(),"draft_discussion".into(),discussion.clone()));
        resolve_comment(
            State(state.clone()), auth("owner-a"),
            Path(("space-a".into(), "target-a".into(), "draft-a".into(), comment.clone())),
            Query(SourceRequest { source_digest: source.clone() }),
        ).await.unwrap();
        assert!(matches!(
            delete_comment(
                State(state.clone()), auth("editor-a"),
                Path(("space-a".into(), "target-a".into(), "draft-a".into(), comment.clone())),
                Query(SourceRequest { source_digest: source.clone() }),
            ).await,
            Err(AppError::Conflict(_))
        ));
        delete_comment(
            State(state.clone()), auth("owner-a"),
            Path(("space-a".into(), "target-a".into(), "draft-a".into(), reply)),
            Query(SourceRequest { source_digest: source.clone() }),
        ).await.unwrap();

        let row: (String, String) = sqlx::query_as(
            "SELECT p.participation,c.source_digest FROM draft_discussion_participants p JOIN draft_discussion_comments c ON c.discussion_id=p.discussion_id WHERE p.discussion_id=$1 AND p.user_id='editor-a'",
        )
        .bind(&discussion)
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(row, ("editor".into(), source.clone()));

        sqlx::query("UPDATE document_drafts SET content='Alpha 😀 changed',updated_at='2026-10-21T00:01:00Z' WHERE id='draft-a'")
            .execute(&state.db)
            .await
            .unwrap();
        let changed_source = digest(&state).await;
        assert!(matches!(
            list_active(
                State(state.clone()),
                auth("editor-a"),
                active_path(),
                Query(SourceRequest { source_digest: source }),
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        let listed = list_active(
            State(state.clone()),
            auth("editor-a"),
            active_path(),
            Query(SourceRequest {
                source_digest: changed_source.clone(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(json_body(listed).await["comments"][0]["anchor_outdated"], true);

        sqlx::query("DELETE FROM document_draft_access WHERE draft_id='draft-a' AND user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(list_active(
            State(state.clone()),
            auth("editor-a"),
            active_path(),
            Query(SourceRequest {
                source_digest: changed_source.clone(),
            }),
        )
        .await
        .is_err());

        let mut tx = state.db.begin().await.unwrap();
        close_for_publication(
            &mut tx,
            "space-a",
            "target-a",
            "draft-a",
            "publish-a",
            &changed_source,
            "2026-10-21T00:02:00Z",
        )
        .await
        .unwrap();
        sqlx::query("DELETE FROM document_drafts WHERE id='draft-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let custody = list_closed_custody(
            State(state.clone()),
            auth("owner-a"),
            Path(("space-a".into(), "target-a".into())),
        )
        .await
        .unwrap();
        let custody = json_body(custody).await;
        assert_eq!(custody["schema_version"], 1);
        assert_eq!(custody["discussions"][0]["id"], discussion);
        assert_eq!(custody["discussions"][0]["publication_operation_id"], "publish-a");
        assert_eq!(custody["discussions"][0]["comment_count"], 1);
        assert_eq!(custody["discussions"][0]["unresolved_count"], 0);
        let editor_custody = list_closed_custody(
            State(state.clone()),
            auth("editor-a"),
            Path(("space-a".into(), "target-a".into())),
        )
        .await
        .unwrap();
        assert_eq!(json_body(editor_custody).await["discussions"], serde_json::json!([]));

        let closed = list_closed(
            State(state.clone()),
            auth("owner-a"),
            Path(("space-a".into(), "target-a".into(), discussion.clone())),
        )
        .await
        .unwrap();
        let closed = json_body(closed).await;
        assert_eq!(closed["state"], "closed");
        assert_eq!(closed["comments"][0]["content"], "Check this emoji @[Owner](owner-a)");
        assert_eq!(closed["comments"][0]["resolved"], true);
        assert_eq!(closed["comments"][0]["anchor_outdated"], true);
        assert!(list_closed(
            State(state),
            auth("editor-a"),
            Path(("space-a".into(), "target-a".into(), discussion)),
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn selected_draft_discussion_tracks_source_revocation_and_closed_custody() {
        exercise_selected_draft_discussion(state().await).await;
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL in STARLANE_TEST_POSTGRES_URL"]
    async fn postgres_selected_draft_discussion_tracks_source_revocation_and_closed_custody() {
        let state = postgres_state().await;
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM _sqlx_migrations")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            11
        );
        exercise_selected_draft_discussion(state).await;
    }
}

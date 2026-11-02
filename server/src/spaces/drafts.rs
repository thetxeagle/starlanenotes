use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::{Row, AnyConnection};
use serde_json::{json,Value};
use super::access::{self,PageOperation,DraftOperation,DraftParticipation,Purpose};
use uuid::Uuid;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::search::index::DocEntry;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space_id}/drafts",
            get(list_drafts).delete(purge_space_drafts),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/drafts",
            axum::routing::post(create_or_resume_draft),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/draft-transfers",
            axum::routing::post(create_draft_transfer),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}",
            get(get_draft).put(update_draft).delete(delete_draft),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish-source",
            get(review_publication),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/publish",
            axum::routing::post(publish_draft),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access",
            get(list_draft_access),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access-candidates",
            get(list_draft_access_candidates),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/drafts/{draft_id}/access/{user_id}",
            axum::routing::put(grant_draft_access).delete(revoke_draft_access),
        )
}

#[derive(Deserialize, Default)]
pub struct ListDraftsQuery {
    pub document_id: Option<String>,
}

pub(crate) type CreateDraftRequest = super::draft_birth::Request;

#[derive(Deserialize)]
pub struct UpdateDraftRequest {
    pub source_digest: Option<Value>,
    pub title: Option<String>,
    pub icon: Option<String>,
    pub cover_image: Option<String>,
    pub content: Option<String>,
}

#[derive(Deserialize)]
pub struct GrantDraftAccessRequest {
    pub role: String,
}

#[derive(Serialize)]
pub struct DraftSummaryResponse {
    pub id: String,
    pub document_id: String,
    pub space_id: String,
    pub owner_id: String,
    pub owner_name: String,
    pub document_title: String,
    pub title: String,
    pub access_role: String,
    pub can_publish: bool,
    pub created_at: String,
    pub updated_at: String,
    pub previous_folder_id: Option<String>,
    pub previous_parent_id: Option<String>,
}

#[derive(Serialize)]
pub struct DraftResponse {
    #[serde(skip_serializing_if="Option::is_none")]
    pub birth_operation_id: Option<String>,
    pub source_digest: String,
    pub id: String,
    pub document_id: String,
    pub space_id: String,
    pub owner_id: String,
    pub owner_name: String,
    pub document_title: String,
    pub title: String,
    pub icon: Option<String>,
    pub cover_image: Option<String>,
    pub content: String,
    pub base_updated_at: String,
    pub access_role: String,
    pub can_publish: bool,
    pub created_at: String,
    pub updated_at: String,
    pub previous_folder_id: Option<String>,
    pub previous_parent_id: Option<String>,
}

#[derive(Serialize)]
pub struct DraftAccessResponse {
    pub user_id: String,
    pub display_name: String,
    pub email: String,
    pub role: String,
}

#[derive(Serialize)]
pub struct DraftAccessCandidateResponse {
    pub user_id: String,
    pub display_name: String,
    pub email: String,
}

fn validate_draft_content(content: &str) -> Result<(), AppError> {
    if content.len() > 5_242_880 {
        return Err(AppError::BadRequest(
            "Draft content must be at most 5 MB".into(),
        ));
    }
    Ok(())
}

fn validate_draft_title(title: &str) -> Result<(), AppError> {
    let title = title.trim();
    if title.is_empty() || title.len() > 500 {
        return Err(AppError::BadRequest(
            "Draft title must be between 1 and 500 characters".into(),
        ));
    }
    Ok(())
}

async fn purge_space_drafts(
    State(state): State<AppState>, auth: AuthUser, Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let deleted = super::draft_purge::purge(&state, &auth, &space_id).await?;
    Ok(Json(serde_json::json!({ "deleted": deleted })))
}

async fn mirror_draft(state: &AppState, draft: &DraftResponse) -> bool {
    let slug: Option<String> = sqlx::query_scalar("SELECT slug FROM spaces WHERE id = $1")
        .bind(&draft.space_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten();
    let Some(slug) = slug else { return false };
    let frontmatter = format!(
        "starlane_lifecycle: draft\ndraft_id: {}\ndocument_id: {}\nowner_id: {}\nowner: {}\ncreated_at: {}\nupdated_at: {}\nprevious_folder_id: {}\nprevious_parent_id: {}",
        draft.id,
        draft.document_id,
        draft.owner_id,
        draft.owner_name,
        draft.created_at,
        draft.updated_at,
        draft.previous_folder_id.as_deref().unwrap_or(""),
        draft.previous_parent_id.as_deref().unwrap_or(""),
    );
    if let Err(error) = storage::write_lifecycle_document(
        &slug,
        "drafts",
        &draft.id,
        &draft.title,
        &frontmatter,
        &draft.content,
    )
    .await
    {
        tracing::warn!(draft_id = %draft.id, %error, "Failed to mirror draft to local storage");
        return false;
    }
    true
}

pub(crate) async fn user_can_access_document_draft(
    state: &AppState,
    space_id: &str,
    document_id: &str,
    user_id: &str,
) -> Result<bool, AppError> {
    let allowed: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM document_drafts d \
         LEFT JOIN document_draft_access a ON a.draft_id = d.id AND a.user_id = $1 \
         WHERE d.space_id = $2 AND d.document_id = $3 \
           AND (d.owner_id = $1 OR a.user_id = $1) LIMIT 1",
    )
    .bind(user_id)
    .bind(space_id)
    .bind(document_id)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::internal)?;
    Ok(allowed.is_some())
}

/// Data-only reader inventory. No authorization, mutation or response labels.
/// Shared by actual draft and version metadata readers on their caller connection.
pub(crate) struct DraftReaderInventory(Vec<(String, Vec<String>, Vec<Value>)>);
pub(crate) async fn capture_draft_reader_inventory(conn:&mut AnyConnection,space:&str,actor:&str)->Result<DraftReaderInventory,AppError> {
    let mut specs:Vec<(String,Vec<String>)>=Vec::new();
    for table in ["spaces","documents","space_members","space_group_permissions","space_user_blocks","space_roles","document_access_policies","document_access_allowlist","document_drafts"] {
        specs.push((format!("SELECT * FROM {table} WHERE {}=$1",if table=="spaces" {"id"}else{"space_id"}),vec![space.into()]));
    }
    specs.push(("SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.space_id=$1".into(),vec![space.into()]));
    specs.push(("SELECT v.* FROM doc_versions v JOIN documents d ON d.id=v.document_id WHERE d.space_id=$1".into(),vec![space.into()]));
    specs.push(("SELECT id,display_name,email,role,is_active FROM users WHERE id=$2 OR id IN(SELECT owner_id FROM document_drafts WHERE space_id=$1) OR id IN(SELECT v.created_by FROM doc_versions v JOIN documents d ON d.id=v.document_id WHERE d.space_id=$1)".into(),vec![space.into(),actor.into()]));
    specs.push(("SELECT id,user_id,scopes,is_service_account,expires_at FROM api_keys WHERE user_id=$1".into(),vec![actor.into()]));
    specs.push(("SELECT id,user_id FROM sessions WHERE user_id=$1".into(),vec![actor.into()]));
    for table in ["groups","group_members","group_global_roles","global_roles"] {specs.push((format!("SELECT * FROM {table}"),vec![]));}
    let mut out=Vec::new();
    for (q,args) in specs {let refs:Vec<_>=args.iter().map(String::as_str).collect();let rows=super::document_deletion::rows(conn,&q,&refs).await.map_err(AppError::internal)?;out.push((q,args,rows));}
    Ok(DraftReaderInventory(out))
}
pub(crate) async fn verify_draft_reader_inventory(conn:&mut AnyConnection,proof:&DraftReaderInventory)->Result<(),AppError> {
    for (q,args,expected) in &proof.0 {let refs:Vec<_>=args.iter().map(String::as_str).collect();if super::document_deletion::rows(conn,q,&refs).await.map_err(AppError::internal)?!=*expected {return Err(AppError::Conflict("Draft or page access source changed; refresh before continuing".into()));}}
    Ok(())
}
async fn draft_reader_space_view(conn:&mut AnyConnection,auth:&AuthUser,space:&str)->Result<(),AppError> {
    let granted=access::resolve_space_access(conn,access::principal_from_auth(auth)?,space,Purpose::Routine).await?;
    if !granted.allows(access::Permission::View) {return Err(AppError::NotFound("Drafts unavailable".into()));} Ok(())
}
fn draft_reader_denial(error:&AppError)->bool {matches!(error,AppError::NotFound(_)|AppError::Forbidden(_))}
const DRAFT_SUMMARY_QUERY:&str="SELECT d.*,u.display_name AS owner_name,doc.title AS document_title,doc.published_at AS page_published_at,CASE WHEN d.owner_id=$1 THEN 'owner' ELSE a.role END AS access_role FROM document_drafts d JOIN documents doc ON doc.id=d.document_id AND doc.space_id=d.space_id JOIN users u ON u.id=d.owner_id LEFT JOIN document_draft_access a ON a.draft_id=d.id AND a.user_id=$1 WHERE d.space_id=$2 AND (d.owner_id=$1 OR a.user_id=$1) AND ($3 IS NULL OR d.document_id=$3) ORDER BY d.updated_at DESC";
fn draft_reader_raw_row(row:&sqlx::any::AnyRow)->Result<Value,AppError>{
    use sqlx::Column;let mut fields=serde_json::Map::new();
    for col in row.columns(){let name=col.name();let v=if let Ok(v)=row.try_get::<Option<String>,_>(name){v.map(Value::String).unwrap_or(Value::Null)}else if let Ok(v)=row.try_get::<Option<i64>,_>(name){v.map(|v|json!(v)).unwrap_or(Value::Null)}else{return Err(AppError::Conflict("Draft source unavailable".into()));};fields.insert(name.into(),v);}Ok(Value::Object(fields))
}
async fn list_drafts(
    State(state): State<AppState>,auth: AuthUser,Path(space_id):Path<String>,Query(query):Query<ListDraftsQuery>,
)->Result<impl IntoResponse,AppError> {
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_read(&mut conn,&auth).await?;
    let principal=access::principal_from_auth(&auth)?;
    draft_reader_space_view(&mut conn,&auth,&space_id).await?;
    let proof=capture_draft_reader_inventory(&mut conn,&space_id,&auth.claims.sub).await?;
    let rows=sqlx::query(DRAFT_SUMMARY_QUERY).bind(&auth.claims.sub).bind(&space_id).bind(query.document_id.as_deref()).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let captured=rows.iter().map(draft_reader_raw_row).collect::<Result<Vec<_>,_>>()?;
    let mut result=Vec::new();let mut selected=Vec::new();
    for row in rows {
        let id:String=row.try_get("id").map_err(AppError::internal)?;
        let doc:String=row.try_get("document_id").map_err(AppError::internal)?;
        let allowed=match access::resolve_draft_access(&mut conn,principal,&space_id,&doc,&id,DraftOperation::View,Purpose::Routine).await {Ok(v)=>v,Err(e) if draft_reader_denial(&e)=>continue,Err(e)=>return Err(e)};
        let owner_id:String=row.try_get("owner_id").map_err(AppError::internal)?;
        let title:String=row.try_get("title").map_err(AppError::internal)?;
        let published:Option<String>=row.try_get("page_published_at").map_err(AppError::internal)?;
        result.push(DraftSummaryResponse {
            id:id.clone(),document_id:doc.clone(),space_id:space_id.clone(),owner_id:owner_id.clone(),owner_name:row.try_get("owner_name").map_err(AppError::internal)?,
            document_title:if published.is_none(){title.clone()}else{row.try_get("document_title").map_err(AppError::internal)?},title,
            access_role:row.try_get("access_role").map_err(AppError::internal)?,can_publish:owner_id==auth.claims.sub,
            created_at:row.try_get("created_at").map_err(AppError::internal)?,updated_at:row.try_get("updated_at").map_err(AppError::internal)?,
            previous_folder_id:row.try_get("previous_folder_id").map_err(AppError::internal)?,previous_parent_id:row.try_get("previous_parent_id").map_err(AppError::internal)?,
        });
        selected.push((doc,id,allowed.participation));
    }
    #[cfg(test)] draft_reader_before_inventory().await;
    verify_draft_reader_inventory(&mut conn,&proof).await?;
    #[cfg(test)] draft_reader_after_inventory().await;
    let fresh=sqlx::query(DRAFT_SUMMARY_QUERY).bind(&auth.claims.sub).bind(&space_id).bind(query.document_id.as_deref()).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    if fresh.iter().map(draft_reader_raw_row).collect::<Result<Vec<_>,_>>()?!=captured {return Err(AppError::Conflict("Draft metadata changed; refresh before continuing".into()));}
    // All inventory SQL precedes the final exact typed audience proofs.
    draft_reader_space_view(&mut conn,&auth,&space_id).await?;
    for (doc,id,participation) in &selected {
        let final_access=access::resolve_draft_access(&mut conn,principal,&space_id,doc,id,DraftOperation::View,Purpose::Routine).await?;
        if &final_access.participation!=participation {return Err(AppError::Conflict("Draft participation changed; refresh before continuing".into()));}
    }
    #[cfg(test)] draft_reader_before_identity().await;
    access::require_current_human_read(&mut conn,&auth).await?;
    // Automated expiry purge and mutable mirrors are deferred, never GET effects.
    Ok(Json(result))
}
#[cfg(test)] tokio::task_local! {
    pub(crate) static DRAFT_READER_BEFORE_INVENTORY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static DRAFT_READER_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static DRAFT_READER_AFTER_INVENTORY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
}
#[cfg(test)] async fn draft_reader_before_inventory(){if let Ok(Some(h))=DRAFT_READER_BEFORE_INVENTORY.try_with(|s|s.borrow_mut().take()){h.await;}}
#[cfg(test)] async fn draft_reader_after_inventory(){if let Ok(Some(h))=DRAFT_READER_AFTER_INVENTORY.try_with(|s|s.borrow_mut().take()){h.await;}}

async fn create_or_resume_draft(
    State(state): State<AppState>, auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
    Json(req): Json<CreateDraftRequest>,
) -> Result<impl IntoResponse, AppError> {
    let outcome=super::draft_birth::create(&state,&auth,&space_id,&document_id,req).await?;
    let status=if outcome.created {StatusCode::CREATED}else{StatusCode::OK};
    let mut response=response_from_saved(outcome.saved,&auth.claims.sub)?;
    response.birth_operation_id=outcome.operation_id;
    Ok((status,Json(response)))
}

async fn create_draft_transfer(
    State(state): State<AppState>, auth: AuthUser,
    Path((space_id, document_id)): Path<(String, String)>,
    Json(req): Json<super::draft_transfer::Request>,
) -> Result<impl IntoResponse, AppError> {
    Ok((StatusCode::CREATED, Json(super::draft_transfer::create(
        &state, &auth, &space_id, &document_id, req,
    ).await?)))
}

async fn fetch_draft(state:&AppState,space_id:&str,document_id:&str,draft_id:&str,auth:&AuthUser,mutation:bool,original:Option<&Value>)->Result<DraftResponse,AppError> {
    response_from_saved(super::draft_save::inspect(state,auth,space_id,document_id,draft_id,mutation,original).await?,&auth.claims.sub)
}

async fn get_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id, draft_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(
        fetch_draft(&state, &space_id, &document_id, &draft_id, &auth,false,None).await?,
    ))
}

async fn update_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id, draft_id)): Path<(String, String, String)>,
    Json(req): Json<UpdateDraftRequest>,
) -> Result<impl IntoResponse, AppError> {
    tracing::info!(space_id,document_id,draft_id,user_id=%auth.claims.sub,"Draft update attempt");
    let result=async {
        let source=super::draft_save::required(req.source_digest.as_ref())?;
        let saved=super::draft_save::save(&state,&auth,&space_id,&document_id,&draft_id,&source,
            super::draft_save::Patch{title:req.title,icon:req.icon,cover_image:req.cover_image,content:req.content},
            &chrono::Utc::now().to_rfc3339(),None).await?;
        response_from_saved(saved,&auth.claims.sub)
    }.await;
    match result {
        Ok(draft)=>{tracing::info!(space_id,document_id,draft_id,"Draft update persisted; mirror deferred");Ok(Json(draft))},
        Err(error)=>{
            // save() has returned: its transaction is fully dropped/rolled back.
            // This intentional advisory failure record is not a successful save.
            tracing::warn!(space_id,document_id,draft_id,?error,"Draft update failed");
            crate::audit::log_action(&state.db,Some(&auth.claims.sub),None,"document.draft_update_failed",Some("document_draft"),Some(&draft_id)).await;
            Err(error)
        }
    }
}
pub(crate) fn response_from_saved(saved:super::draft_save::Saved,user:&str)->Result<DraftResponse,AppError> {
    use super::draft_save::{string,optional,digest};
    let row=&saved.row;
    Ok(DraftResponse{
        birth_operation_id:None,        source_digest:digest(row)?,id:string(row,"id")?,document_id:string(row,"document_id")?,space_id:string(row,"space_id")?,
        owner_id:string(row,"owner_id")?,owner_name:saved.owner_name,document_title:saved.document_title,
        title:string(row,"title")?,icon:optional(row,"icon")?,cover_image:optional(row,"cover_image")?,content:string(row,"content")?,
        base_updated_at:string(row,"base_updated_at")?,access_role:match saved.participation {DraftParticipation::Owner=>"owner",DraftParticipation::SharedEditor=>"editor",DraftParticipation::SharedViewer=>"viewer",DraftParticipation::RecoveryAuditRequired=>"recovery"}.into(),
        can_publish:row["owner_id"]==user,created_at:string(row,"created_at")?,updated_at:string(row,"updated_at")?,
        previous_folder_id:optional(row,"previous_folder_id")?,previous_parent_id:optional(row,"previous_parent_id")?,
    })
}

async fn delete_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id, draft_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::draft_owner::mutate(
        &state, &auth, &space_id, &document_id, &draft_id,
        super::draft_owner::Change::Delete,
    ).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn review_publication(
    State(state): State<AppState>, auth: AuthUser,
    Path((space_id, document_id, draft_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    use super::draft_publication as core;
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    let principal=access::principal_from_auth(&auth)?;
    let authority=access::resolve_draft_access(&mut conn,principal,&space_id,&document_id,&draft_id,DraftOperation::Edit,Purpose::Routine).await?;
    if authority.participation!=DraftParticipation::Owner {return Err(AppError::Forbidden("Only the draft owner can review publication".into()));}
    let proof=publication_source(&mut conn,&auth,&space_id,&document_id,&authority.page.ancestor_ids).await?;
    let page=proof[0].rows.iter().find(|r|r["id"]==document_id).ok_or_else(core::conflict)?.clone();
    let draft=super::draft_save::selected(&mut conn,&space_id,&document_id,&draft_id).await?;
    let bytes=core::page_bytes(&page).await?;
    let response=json!({"schema_version":1,"space_id":space_id,"document_id":document_id,"draft_id":draft_id,
        "draft_source_digest":super::draft_save::digest(&draft)?,"page_source_digest":core::page_digest(&page,bytes.as_deref())?,
        "page_published":!page["published_at"].is_null(),"page_updated_at":page["updated_at"],
        "draft":core::projection(&draft,&publication_string(&draft,"content")?)});
    publication_verify_source(&mut conn,&proof).await?;
    let current=access::resolve_draft_access(&mut conn,principal,&space_id,&document_id,&draft_id,DraftOperation::Edit,Purpose::Routine).await?;
    if current.participation!=DraftParticipation::Owner {return Err(core::conflict());}
    publication_verify_source(&mut conn,&proof).await?;
    if super::draft_save::selected(&mut conn,&space_id,&document_id,&draft_id).await?!=draft {return Err(core::conflict());}
    access::require_current_human_preference_write(&mut conn,&auth).await?;
    core::verify_page_bytes(&page,&bytes)?;
    Ok(Json(response))
}

async fn publish_draft(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id, draft_id)): Path<(String, String, String)>,
    request: Option<Json<super::draft_publication::Request>>,
) -> Result<impl IntoResponse, AppError> {
    use super::draft_publication as core;
    let Json(request)=request.ok_or_else(core::required)?;
    core::validate(&request)?;
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx,&[&space_id]).await?;
    if let Some(response)=core::retry(&mut tx,&auth,&space_id,&document_id,&draft_id,&request).await? {
        // retry already proves legacy base fingerprints independently of the
        // newer immutable capture anchor. Do not invent typed history for those
        // original receipts; missing modern anchors were refused by retry above.
        let captures=super::publication_snapshots::read_validated(&mut tx,&document_id).await?;
        if captures.iter().any(|row|row.kind=="publication"&&row.operation==request.operation_id) {
            super::asset_audiences::verify_publication(&mut tx,&document_id,&request.operation_id).await?;
        }
        tx.commit().await.map_err(AppError::internal)?;
        return Ok(Json(response));
    }
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&document_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    if locked.rows_affected()!=1 {return Err(AppError::NotFound("Document unavailable".into()));}
    sqlx::query("UPDATE document_drafts SET id=id WHERE document_id=$1 AND space_id=$2").bind(&document_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id IN(SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2)").bind(&document_id).bind(&space_id).execute(&mut *tx).await.map_err(AppError::internal)?;
    let principal=access::principal_from_auth(&auth)?;
    let authority=access::resolve_draft_access(&mut tx,principal,&space_id,&document_id,&draft_id,DraftOperation::Edit,Purpose::Routine).await?;
    if authority.participation!=DraftParticipation::Owner {return Err(AppError::Forbidden("Only the draft owner can publish this draft".into()));}
    let original=publication_source(&mut tx,&auth,&space_id,&document_id,&authority.page.ancestor_ids).await?;
    let page=original[0].rows.iter().find(|r|r["id"]==document_id).ok_or_else(publication_changed)?.clone();
    let selected=publication_rows(&mut tx,"SELECT * FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",&[&draft_id,&document_id,&space_id]).await?;
    if selected.len()!=1 {return Err(publication_changed());}
    let draft=&selected[0];
    let original_bytes=core::page_bytes(&page).await?;
    if super::draft_save::digest(draft)?!=request.draft_source_digest || core::page_digest(&page,original_bytes.as_deref())?!=request.page_source_digest {return Err(core::conflict());}
    let source_path=publication_string(&page,"content_path")?;
    let source_revision=publication_string(&page,"updated_at")?;
    if !page["published_at"].is_null() && source_revision!=publication_string(draft,"base_updated_at")? {
        return Err(AppError::Conflict("source_conflict: The published page changed after this draft started. Start a new draft before publishing.".into()));
    }
    let draft_title=publication_string(draft,"title")?;
    let draft_icon=publication_optional(draft,"icon")?;
    let draft_cover=publication_optional(draft,"cover_image")?;
    let published_content=crate::template_guidance::strip_template_guidance(&publication_string(draft,"content")?);

    use super::publication_snapshots as history;
    let mut audience_proofs=super::asset_audiences::audience_proofs(&mut tx,&document_id).await?;
    let audience_pins=super::asset_audiences::prepare_publication(&mut tx,&auth,&space_id,&document_id,&draft_id,&published_content,draft_cover.as_deref()).await?;
    let comparison = history::publication_comparisons(&mut tx, &space_id, &document_id,&audience_pins).await?;
    let result_raw = storage::compose_document_markdown(&super::documents::document_storage_frontmatter(&draft_title, draft_icon.as_deref(), page["locked"].as_i64()==Some(1), page["public"].as_i64()==Some(1)), &published_content).into_bytes();
    let result_content = history::content(draft_title.clone(), draft_icon.clone(), draft_cover.clone(), Some(published_content.clone()), history::Extraction::ExactPublishedDraft, &result_raw)?;
    let mut capture = history::Capture { result: history::payload(history::Binding::CaptureV2, &document_id, &space_id, result_content, comparison.clone()), result_raw, baseline: None, baseline_raw: None, baseline_published_at: None };
    if let Some(raw) = &original_bytes {
        if history::needs_baseline(&mut tx, &document_id).await? {
            let original_title = publication_string(&page, "title")?;
            let original_icon = publication_optional(&page, "icon")?;
            let prefix = super::documents::document_storage_frontmatter(&original_title, original_icon.as_deref(), page["locked"].as_i64()==Some(1), page["public"].as_i64()==Some(1));
            let (portable, extraction) = history::baseline_portable(raw, &prefix);
            let body = history::content(original_title, original_icon, publication_optional(&page, "cover_image")?, portable, extraction, raw)?;
            let mut baseline = history::payload(history::Binding::CaptureV2, &document_id, &space_id, body, comparison.clone());
            baseline.baseline_source_page = Some(page.clone());
            capture.baseline = Some(baseline); capture.baseline_raw = Some(raw.clone()); capture.baseline_published_at = Some(publication_string(&page, "published_at")?);
        }
    }
    let base_fingerprint = core::storage_fingerprint(&auth, &space_id, &document_id, &draft_id, &request, draft)?;
    let mut capture_fingerprint = history::fingerprint_v2(&base_fingerprint, &capture.digest()?)?;
    if let Some(original_manifest) = storage::publication::retained_publication(&request.operation_id)? {
        let manifest = json!(original_manifest);
        if manifest["payload_fingerprint"] == base_fingerprint {
            // Pre-upgrade stages retain their original physical identity. Content
            // remains anchored; comparison observations belong to eventual commit.
            capture.result.binding = history::Binding::LegacyContentV1;
            capture.result.comparison_policy = "comparison_only_observed_at_commit_original_prepare_unbound".into();
            if let Some(baseline) = &mut capture.baseline {
                baseline.binding = history::Binding::LegacyContentV1;
                baseline.comparison_policy = capture.result.comparison_policy.clone();
            }
            capture_fingerprint = base_fingerprint;
        } else if manifest["payload_fingerprint"] != capture_fingerprint { return Err(core::conflict()); }
    }
    let now=chrono::Utc::now().to_rfc3339();
    let frontmatter=super::documents::document_storage_frontmatter(&draft_title,draft_icon.as_deref(),page["locked"].as_i64()==Some(1),page["public"].as_i64()==Some(1));
    #[cfg(test)] publication_route_crash_point("before_prepare",&request.operation_id);
    let prepared=storage::publication::prepare_publication(&mut tx,&auth,storage::publication::PublicationIntent {
        op_id:request.operation_id.clone(),space_id:space_id.clone(),document_id:document_id.clone(),draft_id:draft_id.clone(),source_path:source_path.clone(),source_revision:source_revision.clone(),revision:now.clone(),payload_fingerprint:capture_fingerprint,
    },&frontmatter,&published_content).await?;
    #[cfg(test)] publication_route_crash_point("after_prepare",&request.operation_id);
    let now=prepared.revision().to_owned();
    let mut expected=original.clone();
    let target=expected[0].rows.iter_mut().find(|r|r["id"]==document_id).ok_or_else(publication_changed)?;
    target["title"]=json!(draft_title);target["icon"]=json!(draft_icon);target["cover_image"]=json!(draft_cover);target["last_updated_by"]=json!(auth.claims.sub);target["updated_at"]=json!(now);target["published_at"]=json!(now);target["content_path"]=json!(prepared.path());
    expected[0].rows.sort_by_cached_key(Value::to_string);
    let n=sqlx::query("UPDATE documents SET title=$1,icon=$2,cover_image=$3,last_updated_by=$4,updated_at=$5,published_at=$5,content_path=$6 WHERE id=$7 AND space_id=$8 AND content_path=$9 AND updated_at=$10 AND archived_at IS NULL AND deleted_at IS NULL").bind(&draft_title).bind(&draft_icon).bind(&draft_cover).bind(&auth.claims.sub).bind(&now).bind(prepared.path()).bind(&document_id).bind(&space_id).bind(&source_path).bind(&source_revision).execute(&mut *tx).await.map_err(AppError::internal)?;
    if n.rows_affected()!=1 {return Err(publication_changed());}
    #[cfg(test)] publication_route_crash_point("after_page_sql",&request.operation_id);
    let audit_id=Uuid::new_v4().to_string();
    let audit_time:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,'document.draft_publish','document',$3) RETURNING created_at").bind(&audit_id).bind(&auth.claims.sub).bind(&document_id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    #[cfg(test)] publication_route_crash_point("after_audit",&request.operation_id);
    let audit=json!({"id":audit_id,"user_id":auth.claims.sub,"ip_address":null,"action":"document.draft_publish","target_type":"document","target_id":document_id,"created_at":audit_time,"actor_kind":"user","actor_id":auth.claims.sub});
    storage::publication::record_publication(&mut tx,&auth,&prepared).await?;
    #[cfg(test)] publication_route_crash_point("after_storage_receipt",&request.operation_id);
    let result_page=expected[0].rows.iter().find(|r|r["id"]==document_id).ok_or_else(core::conflict)?.clone();
    let recorded=core::record(&mut tx,&auth,&space_id,&document_id,&draft_id,&request,draft,&result_page,&prepared,&published_content).await?;
    #[cfg(test)] publication_route_crash_point("after_route_receipt",&request.operation_id);
    let captured = history::record(&mut tx, &capture, &request.operation_id, &now, &auth.claims.sub).await?;
    let added_audience_rows=super::asset_audiences::record_publication(&mut tx,&document_id,&request.operation_id,&audience_pins).await?;
    let expected_bindings=audience_proofs.iter_mut().find(|p|p.query.starts_with("SELECT * FROM attachment_audience_bindings ")).ok_or_else(core::conflict)?;
    expected_bindings.rows.extend(added_audience_rows);expected_bindings.rows.sort_by_cached_key(Value::to_string);
    for proof in &mut expected {
        if proof.query == "SELECT * FROM document_publication_snapshots WHERE document_id=$1" {
            proof.rows.extend(publication_rows(&mut tx, "SELECT * FROM document_publication_snapshots WHERE document_id=$1 AND capture_operation_id=$2", &[&document_id, &request.operation_id]).await?);
            proof.rows.sort_by_cached_key(Value::to_string);
        }
    }
    history::verify_record(&mut tx, &captured).await?;
    history::verify_comparisons(&mut tx, &space_id, &document_id, &comparison).await?;
    #[cfg(test)] publication_after_receipt().await;
    // All own receipt/audit writes precede exact latest selected-draft proof.
    publication_verify_source(&mut tx,&expected).await?;
    access::resolve_draft_access(&mut tx,principal,&space_id,&document_id,&draft_id,DraftOperation::Edit,Purpose::Routine).await?;
    if publication_rows(&mut tx,"SELECT * FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3",&[&draft_id,&document_id,&space_id]).await?!=selected {return Err(publication_changed());}
    if super::asset_audiences::prepare_publication(&mut tx,&auth,&space_id,&document_id,&draft_id,&published_content,draft_cover.as_deref()).await?!=audience_pins {return Err(publication_changed())}
    super::draft_discussions::close_for_publication(&mut tx,&space_id,&document_id,&draft_id,&request.operation_id,&request.draft_source_digest,&now).await?;
    let consumed=sqlx::query("DELETE FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3 AND owner_id=$4 AND updated_at=$5 AND content=$6").bind(&draft_id).bind(&document_id).bind(&space_id).bind(&auth.claims.sub).bind(publication_string(draft,"updated_at")?).bind(publication_string(draft,"content")?).execute(&mut *tx).await.map_err(AppError::internal)?;
    if consumed.rows_affected()!=1 {return Err(publication_changed());}
    #[cfg(test)] publication_route_crash_point("after_consume",&request.operation_id);
    for proof in &mut expected {
        if proof.query=="SELECT * FROM document_drafts WHERE document_id=$1" {proof.rows.retain(|r|r["id"]!=draft_id);}
        if proof.query.starts_with("SELECT a.* FROM document_draft_access a JOIN document_drafts d") && proof.parameters==vec![document_id.clone()] {proof.rows.retain(|r|r["draft_id"]!=draft_id);}
    }
    let stored=publication_rows(&mut tx,"SELECT * FROM audit_log WHERE id=$1",&[&audit_id]).await?;
    let audit_row=json!({"id":audit_id,"user_id":auth.claims.sub,"ip_address":null,"action":"document.draft_publish","target_type":"document","target_id":document_id,"created_at":audit_time,"actor_kind":"user","actor_id":auth.claims.sub,"before_value":null,"after_value":null});
    if stored!=vec![audit_row] {return Err(publication_changed());}
    publication_verify_source(&mut tx,&expected).await?;
    storage::publication::verify_recorded_publication(&mut tx,&prepared).await?;
    access::resolve_page_access(&mut tx,principal,&space_id,&document_id,PageOperation::Edit,Purpose::Routine).await?;
    publication_verify_source(&mut tx,&expected).await?;
    core::verify_record(&mut tx,&recorded.row).await?;
    #[cfg(test)] publication_before_identity().await;
    access::require_current_human_preference_write(&mut tx,&auth).await?;
    history::verify_record(&mut tx, &captured).await?;
    history::verify_comparisons(&mut tx, &space_id, &document_id, &comparison).await?;
    access::require_current_human_preference_write(&mut tx,&auth).await?;
    core::verify_page_bytes(&page,&original_bytes)?;
    super::published_source::verify_source(&mut tx,&audience_proofs).await?;
    storage::publication::verify_prepared_publication(&prepared)?;
    #[cfg(test)] publication_route_crash_point("before_commit",&request.operation_id);
    tx.commit().await.map_err(|_|AppError::Conflict(format!("Publication commit outcome requires review; retained operation {}",prepared.op_id())))?;
    #[cfg(test)] publication_route_crash_point("after_commit",&request.operation_id);
    crate::audit::append_committed_move_action(&audit).await;
    // Old canonical and draft mirror files are intentionally retained.
    let derived=index_committed_publication(&state,&auth,&space_id,&document_id,&now,prepared.path(),&draft_title,&published_content,&prepared).await;
    if let Err(error)=&derived {tracing::warn!(draft_id,?error,"Published draft derived refresh deferred");}
    // Mention delivery is deferred until its recipient gate proves current page
    // visibility; the legacy space-only helper can disclose restricted titles.
    if derived.is_ok() {
    crate::webhooks::dispatch::dispatch_public_page_published(
        &state.db,
        &space_id,
        &document_id,
    ).await;
    }
    #[cfg(test)] publication_route_crash_point("before_ack",&request.operation_id);
    Ok(Json(recorded.response))
}

async fn list_draft_access(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id, draft_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(super::draft_owner::inspect(
        &state, &auth, &space_id, &document_id, &draft_id, false,
    ).await?))
}

async fn list_draft_access_candidates(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id, draft_id)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(super::draft_owner::inspect(
        &state, &auth, &space_id, &document_id, &draft_id, true,
    ).await?))
}

async fn grant_draft_access(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id, draft_id, target_user_id)): Path<(String, String, String, String)>,
    Json(req): Json<GrantDraftAccessRequest>,
) -> Result<impl IntoResponse, AppError> {
    super::draft_owner::mutate(
        &state, &auth, &space_id, &document_id, &draft_id,
        super::draft_owner::Change::Grant { recipient: &target_user_id, role: &req.role },
    ).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn revoke_draft_access(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, document_id, draft_id, target_user_id)): Path<(String, String, String, String)>,
) -> Result<impl IntoResponse, AppError> {
    super::draft_owner::mutate(
        &state, &auth, &space_id, &document_id, &draft_id,
        super::draft_owner::Change::Revoke { recipient: &target_user_id },
    ).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
fn copy_publication_auth(auth:&AuthUser)->AuthUser {AuthUser{claims:auth.claims.clone(),api_key_scopes:auth.api_key_scopes.clone()}}
#[cfg(test)]
async fn publish_with_proof(state:State<AppState>,auth:AuthUser,route:Path<(String,String,String)>) -> Result<impl IntoResponse,AppError> {
    let mut conn=state.0.db.acquire().await.map_err(AppError::internal)?;
    let (space,document,draft)=&route.0;
    let selected=super::draft_save::selected(&mut conn,space,document,draft).await?;
    let page=publication_rows(&mut conn,"SELECT * FROM documents WHERE id=$1 AND space_id=$2",&[document,space]).await?.into_iter().next().ok_or_else(publication_changed)?;
    let bytes=super::draft_publication::page_bytes(&page).await?;
    let request=super::draft_publication::Request{schema_version:1,operation_id:Uuid::new_v4().to_string(),draft_source_digest:super::draft_save::digest(&selected)?,page_source_digest:super::draft_publication::page_digest(&page,bytes.as_deref())?};
    drop(conn);
    publish_draft(state,auth,route,Some(Json(request))).await
}

#[cfg(test)]
mod tests {
    use super::{
        get_draft, publish_with_proof, update_draft, validate_draft_content, validate_draft_title,
        UpdateDraftRequest,
    };
    use crate::auth::jwt::{Claims, TokenType};
    use crate::auth::middleware::AuthUser;
    use crate::auth::routes::AppError;
    use crate::AppState;
    use axum::extract::{Path, State};
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use axum::Json;
    use sqlx::any::AnyPoolOptions;

    async fn draft_handler_test_state() -> (AppState, String, std::path::PathBuf) {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        for statement in [
            "CREATE TABLE users (account_kind TEXT NOT NULL DEFAULT 'member',id TEXT PRIMARY KEY, role TEXT NOT NULL, display_name TEXT NOT NULL)",
            "CREATE TABLE spaces (id TEXT PRIMARY KEY, name TEXT NOT NULL DEFAULT 'Test Space', slug TEXT NOT NULL, visibility TEXT NOT NULL, trash_retention_days INTEGER NOT NULL DEFAULT 30)",
            "CREATE TABLE space_members (space_id TEXT NOT NULL, user_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE groups (id TEXT PRIMARY KEY)",
            "CREATE TABLE group_members (group_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_group_permissions (space_id TEXT NOT NULL, group_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE space_user_blocks (space_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_roles (id TEXT NOT NULL, space_id TEXT NOT NULL, permissions TEXT NOT NULL)",
            "CREATE TABLE global_roles (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', permissions TEXT NOT NULL DEFAULT '[]', built_in INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE group_global_roles (group_id TEXT NOT NULL, role_id TEXT NOT NULL, PRIMARY KEY(group_id, role_id))",
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY, space_id TEXT NOT NULL, title TEXT NOT NULL, icon TEXT,
                cover_image TEXT, content_path TEXT NOT NULL DEFAULT '', locked INTEGER NOT NULL DEFAULT 0,
                locked_by TEXT, public INTEGER NOT NULL DEFAULT 0, public_slug TEXT,
                created_by TEXT NOT NULL, last_updated_by TEXT NOT NULL, created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL, published_at TEXT
            )",
            "CREATE TABLE document_drafts (
                id TEXT PRIMARY KEY, document_id TEXT NOT NULL, space_id TEXT NOT NULL,
                owner_id TEXT NOT NULL, title TEXT NOT NULL, icon TEXT, cover_image TEXT,
                content TEXT NOT NULL, base_updated_at TEXT NOT NULL, created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL, previous_folder_id TEXT, previous_parent_id TEXT
            )",
            "CREATE TABLE document_draft_access (draft_id TEXT NOT NULL, user_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE audit_log (id TEXT PRIMARY KEY, user_id TEXT, ip_address TEXT, action TEXT, target_type TEXT, target_id TEXT)",
        ] {
            sqlx::query(statement)
                .execute(&db)
                .await
                .expect("create draft handler fixture table");
        }

        let slug = format!("draft-handler-test-{}", uuid::Uuid::new_v4());
        let index_path = std::env::temp_dir().join(format!(
            "starlane-notes-draft-test-{}",
            uuid::Uuid::new_v4()
        ));
        let state = AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".to_string(),
            base_url: "http://localhost:3000".to_string(),
            search_index: crate::search::SearchIndex::new(index_path.clone())
                .expect("search index"),
            oauth_cache: crate::auth::routes::OAuthStateCache::new(),
            ws_rooms: crate::sync::WsRooms::default(),
        };
        sqlx::query("INSERT INTO spaces (id, slug, visibility) VALUES ('space-1', $1, 'private')")
            .bind(&slug)
            .execute(&state.db)
            .await
            .expect("insert draft handler space");
        for (user_id, space_role) in [
            ("owner-1", "owner"),
            ("editor-1", "editor"),
            ("viewer-1", "viewer"),
        ] {
            sqlx::query("INSERT INTO users (id, role, display_name) VALUES ($1, 'member', $1)")
                .bind(user_id)
                .execute(&state.db)
                .await
                .expect("insert draft handler user");
            sqlx::query(
                "INSERT INTO space_members (space_id, user_id, role) VALUES ('space-1', $1, $2)",
            )
            .bind(user_id)
            .bind(space_role)
            .execute(&state.db)
            .await
            .expect("insert draft handler membership");
        }
        sqlx::query(
            "INSERT INTO documents (
                id, space_id, title, content_path, created_by, last_updated_by,
                created_at, updated_at, published_at
             ) VALUES (
                'doc-1', 'space-1', 'Published title', '', 'owner-1', 'owner-1',
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', NULL
             )",
        )
        .execute(&state.db)
        .await
        .expect("insert draft handler document");
        sqlx::query(
            "INSERT INTO document_drafts (
                id, document_id, space_id, owner_id, title, content, base_updated_at,
                created_at, updated_at
             ) VALUES (
                'draft-1', 'doc-1', 'space-1', 'owner-1', 'Draft title', 'Original body',
                '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'
             )",
        )
        .execute(&state.db)
        .await
        .expect("insert draft handler draft");
        for (user_id, role) in [("editor-1", "editor"), ("viewer-1", "viewer")] {
            sqlx::query(
                "INSERT INTO document_draft_access (draft_id, user_id, role) VALUES ('draft-1', $1, $2)",
            )
            .bind(user_id)
            .bind(role)
            .execute(&state.db)
            .await
            .expect("insert draft handler access");
        }
        (state, slug, index_path)
    }

    fn test_auth(user_id: &str) -> AuthUser {
        AuthUser {
            claims: Claims {
                sub: user_id.to_string(),
                sid: "session-1".to_string(),
                email: format!("{user_id}@example.test"),
                role: "member".to_string(),
                exp: i64::MAX,
                iat: 0,
                token_type: TokenType::Access,
            },
            api_key_scopes: None,
        }
    }

    async fn cleanup_draft_handler_test(state: AppState, slug: &str, index_path: &std::path::Path) {
        let _ = tokio::fs::remove_dir_all(crate::storage::space_dir(slug)).await;
        let _ = tokio::fs::remove_dir_all(index_path).await;
        drop(state);
    }

    #[test]
    fn draft_input_limits_match_document_limits() {
        assert!(validate_draft_title("Launch plan").is_ok());
        assert!(validate_draft_title("").is_err());
        assert!(validate_draft_title(&"x".repeat(501)).is_err());
        assert!(validate_draft_content(&"x".repeat(5_242_880)).is_ok());
        assert!(validate_draft_content(&"x".repeat(5_242_881)).is_err());
    }

    #[tokio::test]
    async fn draft_handlers_allow_owner_and_shared_editor_writes_but_keep_viewers_read_only() {
        let (state, slug, db_path) = super::publication_consumer_tests::legacy_state().await;
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft-1','viewer-1','viewer','owner-1')").execute(&state.db).await.unwrap();

        for user_id in ["owner-1", "editor-1", "viewer-1"] {
            let response = get_draft(
                State(state.clone()),
                super::publication_consumer_tests::legacy_auth(&state,user_id).await,
                Path(("space-1".into(), "doc-1".into(), "draft-1".into())),
            )
            .await
            .expect("authorized draft read")
            .into_response();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{user_id} should read draft"
            );
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("read draft response");
            let draft: serde_json::Value = serde_json::from_slice(&body).expect("decode draft");
            assert_eq!(
                draft["access_role"].as_str(),
                Some(user_id.strip_suffix("-1").unwrap())
            );
            assert_eq!(draft["can_publish"].as_bool(), Some(user_id == "owner-1"));
        }

        for user_id in ["owner-1", "editor-1"] {
            update_draft(
                State(state.clone()),
                super::publication_consumer_tests::legacy_auth(&state,user_id).await,
                Path(("space-1".into(), "doc-1".into(), "draft-1".into())),
                Json(UpdateDraftRequest {
                    source_digest: Some(serde_json::json!(super::super::draft_save::digest(&super::super::draft_save::selected(&mut state.db.acquire().await.unwrap(),"space-1","doc-1","draft-1").await.unwrap()).unwrap())),
                    title: None,
                    icon: None,
                    cover_image: None,
                    content: Some(format!("Saved by {user_id}")),
                }),
            )
            .await
            .expect("owner and shared editor can update draft");
        }

        let viewer_write = update_draft(
            State(state.clone()),
            super::publication_consumer_tests::legacy_auth(&state,"viewer-1").await,
            Path(("space-1".into(), "doc-1".into(), "draft-1".into())),
            Json(UpdateDraftRequest {
                    source_digest: Some(serde_json::json!(super::super::draft_save::digest(&super::super::draft_save::selected(&mut state.db.acquire().await.unwrap(),"space-1","doc-1","draft-1").await.unwrap()).unwrap())),
                title: None,
                icon: None,
                cover_image: None,
                content: Some("Viewer must not write".into()),
            }),
        )
        .await;
        assert!(matches!(viewer_write, Err(AppError::NotFound(_))));
        let content: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE id = 'draft-1'")
                .fetch_one(&state.db)
                .await
                .expect("read final draft content");
        assert_eq!(content, "Saved by editor-1");

        state.db.close().await;let _=tokio::fs::remove_dir_all(crate::storage::space_dir(&slug)).await;let _=std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn draft_publish_requires_owner_and_rejects_locked_or_stale_pages_without_mutation() {
        let (state, slug, index_path) = super::publication_consumer_tests::legacy_state().await;
        let route = || Path(("space-1".into(), "doc-1".into(), "draft-1".into()));

        let editor_publish =
            publish_with_proof(State(state.clone()), super::publication_consumer_tests::legacy_auth(&state,"editor-1").await, route()).await;
        assert!(matches!(editor_publish, Err(AppError::Forbidden(_))));
        let unchanged_content: String =
            sqlx::query_scalar("SELECT content FROM document_drafts WHERE id = 'draft-1'")
                .fetch_one(&state.db)
                .await
                .expect("read draft after editor publish attempt");
        assert_eq!(unchanged_content, "Original body");

        sqlx::query(
            "UPDATE documents SET locked = 1, locked_by = 'editor-1' WHERE id = 'doc-1'",
        )
        .execute(&state.db)
        .await
        .expect("lock document");
        let locked_publish =
            publish_with_proof(State(state.clone()), super::publication_consumer_tests::legacy_auth(&state,"owner-1").await, route()).await;
        assert!(matches!(locked_publish, Err(AppError::Conflict(_))));
        let locked_state: (Option<String>, String) = sqlx::query_as(
            "SELECT published_at, (SELECT id FROM document_drafts WHERE id = 'draft-1') \
             FROM documents WHERE id = 'doc-1'",
        )
        .fetch_one(&state.db)
        .await
        .expect("read state after locked publish");
        assert!(locked_state.0.is_none());
        assert_eq!(locked_state.1, "draft-1");

        sqlx::query(
            "UPDATE documents SET locked = 0, locked_by = NULL, \
             updated_at = '2026-02-01T00:00:00Z', published_at = '2026-01-15T00:00:00Z' \
             WHERE id = 'doc-1'",
        )
        .execute(&state.db)
        .await
        .expect("set published page revision");
        let stale_publish =
            publish_with_proof(State(state.clone()), super::publication_consumer_tests::legacy_auth(&state,"owner-1").await, route()).await;
        assert!(matches!(stale_publish, Err(AppError::Conflict(_))));
        let stale_state: (String, Option<String>, String) = sqlx::query_as(
            "SELECT updated_at, published_at, (SELECT id FROM document_drafts WHERE id = 'draft-1') \
             FROM documents WHERE id = 'doc-1'",
        )
        .fetch_one(&state.db)
        .await
        .expect("read state after stale publish");
        assert_eq!(stale_state.0, "2026-02-01T00:00:00Z");
        assert_eq!(stale_state.1.as_deref(), Some("2026-01-15T00:00:00Z"));
        assert_eq!(stale_state.2, "draft-1");

        cleanup_draft_handler_test(state, &slug, &index_path).await;
    }

    #[tokio::test]
    async fn draft_owner_can_publish_matching_revision() {
        let (state, slug, index_path) = super::publication_consumer_tests::legacy_state().await;
        let current_revision = "2026-01-01T00:00:00Z";
        sqlx::query(
            "UPDATE documents SET published_at = '2026-01-02T00:00:00Z' WHERE id = 'doc-1'",
        )
        .execute(&state.db)
        .await
        .expect("mark page published");
        sqlx::query(
            "UPDATE document_drafts SET base_updated_at = $1, content = 'Published body' \
             WHERE id = 'draft-1'",
        )
        .bind(current_revision)
        .execute(&state.db)
        .await
        .expect("align draft base revision");

        let response = publish_with_proof(
            State(state.clone()),
            super::publication_consumer_tests::legacy_auth(&state,"owner-1").await,
            Path(("space-1".into(), "doc-1".into(), "draft-1".into())),
        )
        .await
        .expect("owner publishes matching draft")
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        let published_at: Option<String> =
            sqlx::query_scalar("SELECT published_at FROM documents WHERE id = 'doc-1'")
                .fetch_one(&state.db)
                .await
                .expect("read published timestamp");
        assert!(published_at.is_some());
        let remaining_drafts: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM document_drafts WHERE id = 'draft-1'")
                .fetch_one(&state.db)
                .await
                .expect("count remaining drafts");
        assert_eq!(remaining_drafts, 0);
        let content_path: String =
            sqlx::query_scalar("SELECT content_path FROM documents WHERE id = 'doc-1'")
                .fetch_one(&state.db)
                .await
                .expect("read published content path");
        let markdown = crate::storage::read_document(&content_path)
            .await
            .expect("read published page");
        assert_eq!(markdown,super::publication_consumer_tests::expected_markdown("Published body"));

        cleanup_draft_handler_test(state, &slug, &index_path).await;
    }
}

#[derive(Clone)]
struct PublicationProof {
    query: String,
    parameters: Vec<String>,
    rows: Vec<Value>,
}
async fn publication_rows(conn: &mut AnyConnection, q: &str, args: &[&str]) -> Result<Vec<Value>, AppError> {
    super::document_deletion::rows(conn, q, args)
        .await
        .map_err(AppError::internal)
}
async fn publication_source(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    ancestors: &[String],
) -> Result<Vec<PublicationProof>, AppError> {
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
        "properties_schema",
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
        "document_publication_snapshots",
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
        let current = publication_rows(conn, &query, &args).await?;
        proof.push(PublicationProof {
            query,
            parameters,
            rows: current,
        });
    }
    Ok(proof)
}

fn publication_changed()->AppError {AppError::Conflict("Publication source changed; retain the draft and retry".into())}
fn publication_string(row:&Value,key:&str)->Result<String,AppError> {row.get(key).and_then(Value::as_str).map(str::to_string).ok_or_else(publication_changed)}
fn publication_optional(row:&Value,key:&str)->Result<Option<String>,AppError> {match row.get(key) {Some(Value::Null)=>Ok(None),Some(Value::String(s))=>Ok(Some(s.clone())),_=>Err(publication_changed())}}
async fn publication_verify_source(conn:&mut AnyConnection,expected:&[PublicationProof])->Result<(),AppError> {
    for proof in expected {let args:Vec<&str>=proof.parameters.iter().map(String::as_str).collect();if publication_rows(conn,&proof.query,&args).await?!=proof.rows {return Err(publication_changed());}}
    Ok(())
}
async fn index_committed_publication(state:&AppState,auth:&AuthUser,space:&str,id:&str,stamp:&str,path:&str,title:&str,body:&str,prepared:&storage::publication::PreparedPublication)->Result<(),AppError> {
    let mut tx=state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx,&[space]).await?;
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(id).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if locked.rows_affected()!=1 {return Err(publication_changed());}
    access::resolve_page_access(&mut tx,access::principal_from_auth(auth)?,space,id,PageOperation::Edit,Purpose::Routine).await?;
    let current:Option<(String,String,String,String)>=sqlx::query_as("SELECT content_path,updated_at,published_at,title FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(id).bind(space).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if current!=Some((path.into(),stamp.into(),stamp.into(),title.into())) {return Err(publication_changed());}
    access::require_current_human_preference_write(&mut tx,auth).await?;
    storage::publication::verify_prepared_publication(prepared)?;
    state.search_index.upsert(DocEntry {doc_id:id.into(),space_id:space.into(),title:title.into(),body:body.into(),tags:String::new()}).map_err(AppError::internal)?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(())
}
#[cfg(test)]
tokio::task_local! {static PUBLICATION_AFTER_RECEIPT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;static PUBLICATION_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;}
#[cfg(test)] async fn publication_after_receipt(){if let Ok(Some(h))=PUBLICATION_AFTER_RECEIPT.try_with(|s|s.borrow_mut().take()){h.await}}
#[cfg(test)] async fn publication_before_identity(){if let Ok(Some(h))=PUBLICATION_BEFORE_IDENTITY.try_with(|s|s.borrow_mut().take()){h.await}}

#[cfg(test)]
mod publication_consumer_tests {
    use super::*;
    use axum::{body::{Body,to_bytes},extract::FromRequestParts,http::Request};
    use tower::ServiceExt;
    pub(super) async fn legacy_state()->(AppState,String,std::path::PathBuf) {
        let mut state=crate::spaces::properties::tests::property_test_state().await;
        let path=std::env::temp_dir().join(format!("publication-http-{}.sqlite",Uuid::new_v4()));
        state.db=sqlx::any::AnyPoolOptions::new().max_connections(2).after_connect(|conn,_|Box::pin(async move {
            sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *conn).await?;
            sqlx::query("PRAGMA busy_timeout=5000").execute(&mut *conn).await?;Ok(())
        })).connect(&format!("sqlite://{}?mode=rwc",path.display())).await.unwrap();
        sqlx::query("PRAGMA journal_mode=WAL").execute(&state.db).await.unwrap();
        sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
        for (id,role) in [("owner-1","owner"),("editor-1","editor"),("viewer-1","viewer")] {
            sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(id).bind(format!("{id}@example.test")).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at,device_name) VALUES($1,$2,'hash','2999-01-01T00:00:00Z','Browser')").bind(format!("publication-{id}")).bind(id).execute(&state.db).await.unwrap();
            if id=="owner-1" {sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES('space-1','Published space',$1,'private','owner-1')").bind(format!("publication-{}",Uuid::new_v4())).execute(&state.db).await.unwrap();}
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($1,'space-1',$2,$3)").bind(format!("membership-{id}")).bind(id).bind(role).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('session-1','owner-1','hash','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        let slug:String=sqlx::query_scalar("SELECT slug FROM spaces WHERE id='space-1'").fetch_one(&state.db).await.unwrap();
        let old=storage::write_document(&slug,"doc-1","Published title","","PUBLIC OLD UNIQUE").await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,created_at,updated_at,published_at,public,public_slug) VALUES('doc-1','space-1','Published title',$1,'owner-1','owner-1','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z',NULL,1,$2)").bind(old).bind(format!("link-{slug}")).execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES('draft-1','doc-1','space-1','owner-1','Draft title','Original body','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft-1','editor-1','editor','owner-1')").execute(&state.db).await.unwrap();
        (state,slug,path)
    }
    pub(super) fn expected_markdown(body:&str)->String {
        // Exact plain-body branch of storage::compose_document_markdown (private
        // storage API); retain full frontmatter/newlines, never contains-only.
        let frontmatter=super::super::documents::document_storage_frontmatter("Draft title",None,false,true);
        format!("---\n{}\n---\n\n{}",frontmatter.trim(),body)
    }
    fn token(state:&AppState,user:&str,exp:i64)->String {
        let mut claims=crate::spaces::properties::tests::test_auth(user).claims;claims.sid=format!("publication-{user}");claims.exp=exp;
        jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap()
    }
    pub(super) async fn legacy_auth(state:&AppState,user:&str)->AuthUser {auth(state,user,i64::MAX).await}
    async fn auth(state:&AppState,user:&str,exp:i64)->AuthUser {
        let (mut parts,_)=Request::builder().method("POST").header("Authorization",format!("Bearer {}",token(state,user,exp))).body(Body::empty()).unwrap().into_parts();
        AuthUser::from_request_parts(&mut parts,state).await.unwrap()
    }
    fn route()->Path<(String,String,String)> {Path(("space-1".into(),"doc-1".into(),"draft-1".into()))}
    async fn published(state:&AppState) {sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&state.db).await.unwrap();sqlx::query("UPDATE document_drafts SET content='PUBLIC NEXT UNIQUE' WHERE id='draft-1'").execute(&state.db).await.unwrap();}
    async fn cleanup(state:AppState,slug:&str,path:&std::path::Path) {state.db.close().await;let _=tokio::fs::remove_dir_all(storage::space_dir(slug)).await;let _=std::fs::remove_file(path);}
    async fn confirmed_request(s:&AppState,a:&AuthUser)->super::super::draft_publication::Request {
        let response=review_publication(State(s.clone()),copy_publication_auth(&a),route()).await.unwrap().into_response();
        let proof:Value=serde_json::from_slice(&to_bytes(response.into_body(),2_000_000).await.unwrap()).unwrap();
        super::super::draft_publication::Request{schema_version:1,operation_id:Uuid::new_v4().to_string(),draft_source_digest:proof["draft_source_digest"].as_str().unwrap().into(),page_source_digest:proof["page_source_digest"].as_str().unwrap().into()}
    }
    async fn exact_publish(s:&AppState,a:&AuthUser,r:super::super::draft_publication::Request)->Result<Value,AppError> {
        let response=publish_draft(State(s.clone()),copy_publication_auth(&a),route(),Some(Json(r))).await?.into_response();
        Ok(serde_json::from_slice(&to_bytes(response.into_body(),2_000_000).await.unwrap()).unwrap())
    }
    async fn receipt_count(s:&AppState)->i64 {sqlx::query_scalar("SELECT count(*) FROM draft_publication_receipts").fetch_one(&s.db).await.unwrap()}

    #[tokio::test]
    async fn owner_confirmation_rejects_intervening_draft_page_and_published_body_sources() {
        for mutation in ["draft","page","bytes"] {
            let (s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",i64::MAX).await;
            let request=confirmed_request(&s,&a).await;
            match mutation {
                "draft"=>{sqlx::query("UPDATE document_drafts SET title='Collaborator title',content='COLLABORATOR BODY',icon='remote-icon',cover_image='remote-cover' WHERE id='draft-1'").execute(&s.db).await.unwrap();},
                "page"=>{sqlx::query("UPDATE documents SET icon='other-page-icon' WHERE id='doc-1'").execute(&s.db).await.unwrap();},
                _=>{let old:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();std::fs::write(storage::data_dir().join(old),b"EXTERNALLY CHANGED PUBLISHED BYTES").unwrap();}
            }
            let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
            let error=exact_publish(&s,&a,request).await.unwrap_err();assert!(matches!(error,AppError::Conflict(ref message) if message.starts_with("source_conflict:")));
            assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);assert_eq!(receipt_count(&s).await,0);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn missing_http_publication_body_and_nonowner_review_cannot_create_receipt() {
        let(s,slug,path)=legacy_state().await;published(&s).await;
        for user in ["editor-1","viewer-1"] {let a=auth(&s,user,i64::MAX).await;assert!(review_publication(State(s.clone()),a,route()).await.is_err());}
        let response=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().method("POST").uri("/spaces/space-1/documents/doc-1/drafts/draft-1/publish").header("Authorization",format!("Bearer {}",token(&s,"owner-1",i64::MAX))).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::CONFLICT);assert_eq!(receipt_count(&s).await,0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM document_drafts WHERE id='draft-1'").fetch_one(&s.db).await.unwrap(),1);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn consumed_publication_retry_returns_exact_historical_result_without_adopting_later_page_or_reused_draft() {
        let(s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",i64::MAX).await;let request=confirmed_request(&s,&a).await;
        let applied=exact_publish(&s,&a,request.clone()).await.unwrap();assert_eq!(applied["disposition"],"applied");
        let mut historical=applied.clone();historical["disposition"]=json!("historical");
        let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
        assert_eq!(exact_publish(&s,&a,request.clone()).await.unwrap(),historical);assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);
        sqlx::query("UPDATE documents SET title='Later page',icon='later-icon' WHERE id='doc-1'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES('draft-1','doc-1','space-1','owner-1','New draft','REUSED ID BODY','later','later','later')").execute(&s.db).await.unwrap();
        sqlx::query("CREATE TRIGGER reject_reused_draft_touch AFTER UPDATE ON document_drafts BEGIN SELECT RAISE(ABORT,'historical retry touched reused draft'); END").execute(&s.db).await.unwrap();
        let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
        assert_eq!(exact_publish(&s,&a,request.clone()).await.unwrap(),historical);assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);
        let mut changed=request;changed.draft_source_digest="0".repeat(64);assert!(exact_publish(&s,&a,changed).await.is_err());assert_eq!(receipt_count(&s).await,1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log WHERE action='document.draft_publish'").fetch_one(&s.db).await.unwrap(),1);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn historical_publication_requires_same_actor_current_page_and_identity_and_exact_receipts() {
        for mutation in ["actor","account","session","blocked","deleted","app-receipt","storage-receipt","install"] {
            let(s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",i64::MAX).await;let request=confirmed_request(&s,&a).await;
            exact_publish(&s,&a,request.clone()).await.unwrap();
            let retry_actor=if mutation=="actor" {auth(&s,"editor-1",i64::MAX).await}else{copy_publication_auth(&a)};
            match mutation {
                "account"=>{sqlx::query("UPDATE users SET is_active=0 WHERE id='owner-1'").execute(&s.db).await.unwrap();},
                "session"=>{sqlx::query("DELETE FROM sessions WHERE user_id='owner-1'").execute(&s.db).await.unwrap();},
                "blocked"=>{sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','owner-1','editor-1')").execute(&s.db).await.unwrap();},
                "deleted"=>{sqlx::query("UPDATE documents SET deleted_at='later' WHERE id='doc-1'").execute(&s.db).await.unwrap();},
                "app-receipt"=>{sqlx::query("UPDATE draft_publication_receipts SET response_json='{}'").execute(&s.db).await.unwrap();},
                "storage-receipt"=>{sqlx::query("UPDATE publication_receipts SET payload_fingerprint='bad'").execute(&s.db).await.unwrap();},
                "install"=>{let canonical:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();std::fs::write(storage::data_dir().join(canonical),b"CORRUPT").unwrap();},
                _=>{}
            }
            let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
            assert!(exact_publish(&s,&retry_actor,request).await.is_err(),"{mutation}");assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before,"{mutation}");assert_eq!(receipt_count(&s).await,1);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn coherent_route_receipt_request_rebinding_cannot_change_immutable_confirmation() {
        let(s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",i64::MAX).await;let mut request=confirmed_request(&s,&a).await;
        let mut response=exact_publish(&s,&a,request.clone()).await.unwrap();
        request.page_source_digest="0".repeat(64);response["page_source_digest"]=json!(request.page_source_digest);
        let fingerprint=super::super::draft_save::digest(&json!({"actor_id":a.claims.sub,"space_id":"space-1","document_id":"doc-1","draft_id":"draft-1","request":request})).unwrap();
        sqlx::query("UPDATE draft_publication_receipts SET request_json=$1,request_fingerprint=$2,response_json=$3")
            .bind(serde_json::to_string(&request).unwrap()).bind(fingerprint).bind(response.to_string()).execute(&s.db).await.unwrap();
        let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
        assert!(exact_publish(&s,&a,request).await.is_err());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);assert_eq!(receipt_count(&s).await,1);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn unpublished_confirmation_ignores_mirror_bytes_but_binds_exact_private_row() {
        let(s,slug,path)=legacy_state().await;let a=auth(&s,"owner-1",i64::MAX).await;
        let request=confirmed_request(&s,&a).await;
        let mirror:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();
        std::fs::write(storage::data_dir().join(mirror),b"UNPUBLISHED MIRROR IS NOT AUTHORITY").unwrap();
        assert_eq!(confirmed_request(&s,&a).await.page_source_digest,request.page_source_digest);
        let applied=exact_publish(&s,&a,request).await.unwrap();assert_eq!(applied["disposition"],"applied");assert_ne!(applied["publication"]["content"],"UNPUBLISHED MIRROR IS NOT AUTHORITY");cleanup(s,&slug,&path).await;
    }

    #[tokio::test]
    async fn original_operation_concurrent_retry_publishes_and_audits_once() {
        let(s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",i64::MAX).await;let request=confirmed_request(&s,&a).await;
        let(left,right)=tokio::join!(exact_publish(&s,&a,request.clone()),exact_publish(&s,&a,request));
        let mut outcomes=vec![left.unwrap()["disposition"].as_str().unwrap().to_string(),right.unwrap()["disposition"].as_str().unwrap().to_string()];outcomes.sort();assert_eq!(outcomes,vec!["applied","historical"]);
        assert_eq!(receipt_count(&s).await,1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM publication_receipts").fetch_one(&s.db).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log WHERE action='document.draft_publish'").fetch_one(&s.db).await.unwrap(),1);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_signed_publication_raw_schema_same_stamp_and_invalid_options_drift_roll_back() {
        for mutation in ["UPDATE properties_schema SET name='Changed schema' WHERE id='schema-proof'", "UPDATE properties_schema SET options='{new invalid raw JSON' WHERE id='schema-proof'"] {
            let(s,slug,path)=legacy_state().await;published(&s).await;
            sqlx::query("INSERT INTO properties_schema(id,space_id,name,property_type,options,sort_order,created_at) VALUES('schema-proof','space-1','Original schema','select','{original invalid raw JSON',0,'same stamp')").execute(&s.db).await.unwrap();
            let a=auth(&s,"owner-1",i64::MAX).await;let request=confirmed_request(&s,&a).await;
            sqlx::query(&format!("CREATE TRIGGER publication_schema_drift AFTER INSERT ON audit_log WHEN NEW.action='document.draft_publish' BEGIN {mutation}; END")).execute(&s.db).await.unwrap();
            let before=super::draft_reader_current_tests::snapshot(&s,true).await;
            let old:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();
            let old_bytes=storage::read_document(&old).await.unwrap();
            let response=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().method("POST").uri("/spaces/space-1/documents/doc-1/drafts/draft-1/publish").header("Authorization",format!("Bearer {}",token(&s,"owner-1",i64::MAX))).header("Content-Type","application/json").body(Body::from(serde_json::to_string(&request).unwrap())).unwrap()).await.unwrap();
            assert_eq!(response.status(),StatusCode::CONFLICT,"{mutation}");
            assert_eq!(super::draft_reader_current_tests::snapshot(&s,true).await,before,"schema drift rolls back all publication SQL; only extractor session renewal is normalized");
            assert_eq!(storage::read_document(&old).await.unwrap(),old_bytes);assert_eq!(receipt_count(&s).await,0);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM publication_receipts").fetch_one(&s.db).await.unwrap(),0);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM document_drafts WHERE id='draft-1'").fetch_one(&s.db).await.unwrap(),1);
            let raw:Option<String>=sqlx::query_scalar("SELECT options FROM properties_schema WHERE id='schema-proof'").fetch_one(&s.db).await.unwrap();assert_eq!(raw.as_deref(),Some("{original invalid raw JSON"));
            cleanup(s,&slug,&path).await;
        }
    }

    #[tokio::test]
    async fn route_receipt_trigger_drift_rolls_back_then_original_operation_resumes_exactly_once() {
        let(s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",i64::MAX).await;let request=confirmed_request(&s,&a).await;
        sqlx::query("CREATE TRIGGER corrupt_route_receipt AFTER INSERT ON draft_publication_receipts BEGIN UPDATE draft_publication_receipts SET response_json='{}' WHERE operation_id=NEW.operation_id; END").execute(&s.db).await.unwrap();
        let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
        assert!(exact_publish(&s,&a,request.clone()).await.is_err());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);assert_eq!(receipt_count(&s).await,0);
        sqlx::query("DROP TRIGGER corrupt_route_receipt").execute(&s.db).await.unwrap();
        let applied=exact_publish(&s,&a,request.clone()).await.unwrap();
        assert_eq!(applied["disposition"],"applied");assert_eq!(applied["operation_id"],request.operation_id);
        assert_eq!(receipt_count(&s).await,1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM publication_receipts").fetch_one(&s.db).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log WHERE action='document.draft_publish'").fetch_one(&s.db).await.unwrap(),1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM document_drafts WHERE id='draft-1'").fetch_one(&s.db).await.unwrap(),0);
        let historical=exact_publish(&s,&a,request).await.unwrap();let mut expected=applied;expected["disposition"]=json!("historical");assert_eq!(historical,expected);
        assert_eq!(receipt_count(&s).await,1);cleanup(s,&slug,&path).await;
    }

    #[tokio::test]
    async fn actual_signed_router_owner_publishes_immutable_and_retains_old_bytes() {
        let (s,slug,path)=legacy_state().await;published(&s).await;
        let old:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();
        let a=auth(&s,"owner-1",i64::MAX).await;
        let proof=review_publication(State(s.clone()),a,route()).await.unwrap().into_response();
        let proof:Value=serde_json::from_slice(&to_bytes(proof.into_body(),2_000_000).await.unwrap()).unwrap();
        let request=json!({"schema_version":1,"operation_id":Uuid::new_v4().to_string(),"draft_source_digest":proof["draft_source_digest"],"page_source_digest":proof["page_source_digest"]});
        let response=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().method("POST").uri("/spaces/space-1/documents/doc-1/drafts/draft-1/publish").header("Authorization",format!("Bearer {}",token(&s,"owner-1",i64::MAX))).header("Content-Type","application/json").body(Body::from(request.to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(),StatusCode::OK);
        let next:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();assert_ne!(next,old);assert!(next.contains("/docs/revisions/"));assert_eq!(storage::read_document(&old).await.unwrap(),"PUBLIC OLD UNIQUE");assert_eq!(storage::read_document(&next).await.unwrap(),expected_markdown("PUBLIC NEXT UNIQUE"));
        let receipt:i64=sqlx::query_scalar("SELECT COUNT(*) FROM publication_receipts WHERE document_id='doc-1'").fetch_one(&s.db).await.unwrap();assert_eq!(receipt,1);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_shared_editor_owner_denial_and_read_scope_keep_entire_database() {
        let (s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"editor-1",i64::MAX).await;let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;assert!(publish_with_proof(State(s.clone()),a,route()).await.is_err());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('publication-read','owner-1','Read',$1,'test','[\"read\"]')").bind(crate::auth::api_keys::hash_api_key("publication-read-raw")).execute(&s.db).await.unwrap();
        let (mut parts,_)=Request::builder().method("GET").header("X-API-Key","publication-read-raw").body(Body::empty()).unwrap().into_parts();let a=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;assert!(publish_with_proof(State(s.clone()),a,route()).await.is_err());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn audit_receipt_and_latest_draft_trigger_drift_roll_back_all_sql_and_old_body() {
        for mutation in ["SELECT RAISE(ABORT,'audit failure');","UPDATE document_drafts SET content='NEWER SAVED',title='Newer title' WHERE id='draft-1';","UPDATE documents SET maintenance_user_id='viewer-1' WHERE id='doc-1';","DELETE FROM sessions WHERE user_id='owner-1';"] {
            let (s,slug,path)=legacy_state().await;published(&s).await;sqlx::query(&format!("CREATE TRIGGER publication_drift AFTER INSERT ON audit_log WHEN NEW.action='document.draft_publish' BEGIN {mutation} END")).execute(&s.db).await.unwrap();
            let a=auth(&s,"owner-1",i64::MAX).await;let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;assert!(publish_with_proof(State(s.clone()),a,route()).await.is_err(),"{mutation}");assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before,"{mutation}");let old:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();assert_eq!(storage::read_document(&old).await.unwrap(),"PUBLIC OLD UNIQUE");cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn receipt_failure_or_triggered_latest_save_cannot_consume_selected_draft() {
        for mutation in ["SELECT RAISE(ABORT,'receipt failure');","UPDATE document_drafts SET title='Latest saved title',content='LATEST SAVED BODY' WHERE id='draft-1';"] {
            let (s,slug,path)=legacy_state().await;published(&s).await;sqlx::query(&format!("CREATE TRIGGER publication_receipt_drift AFTER INSERT ON publication_receipts BEGIN {mutation} END")).execute(&s.db).await.unwrap();
            let a=auth(&s,"owner-1",i64::MAX).await;let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;assert!(publish_with_proof(State(s.clone()),a,route()).await.is_err());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn last_signed_jwt_expiry_after_receipt_proofs_rolls_back_audit_and_consumption() {
        let (s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",chrono::Utc::now().timestamp()+2).await;let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
        let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let r=reached.clone();
        let result=PUBLICATION_BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {r.store(true,std::sync::atomic::Ordering::SeqCst);tokio::time::sleep(std::time::Duration::from_secs(3)).await;}))),publish_with_proof(State(s.clone()),a,route())).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert!(result.is_err());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn final_installed_file_corruption_rejects_without_publishing_new_path() {
        let (s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",i64::MAX).await;let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;let revisions=storage::space_dir(&slug).join("docs/revisions");
        let result=PUBLICATION_BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {for f in std::fs::read_dir(revisions).unwrap(){std::fs::write(f.unwrap().path(),b"CORRUPT").unwrap();}}))),publish_with_proof(State(s.clone()),a,route())).await;assert!(result.is_err());assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_wal_audience_wait_revalidates_session_account_and_page_block() {
        for change in ["DELETE FROM sessions WHERE user_id='owner-1'","UPDATE users SET is_active=0 WHERE id='owner-1'","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','owner-1','editor-1')"] {
            let (s,slug,path)=legacy_state().await;published(&s).await;let a=auth(&s,"owner-1",i64::MAX).await;
            let mut blocker=s.db.begin().await.unwrap();super::super::mutation_guards::lock_audience_for_mutation(&mut blocker).await.unwrap();
            let actor=s.clone();let (started,ready)=tokio::sync::oneshot::channel();let task=tokio::spawn(async move {let _=started.send(());publish_with_proof(State(actor),a,route()).await.map(IntoResponse::into_response)});ready.await.unwrap();tokio::task::yield_now().await;assert!(!task.is_finished());
            sqlx::query(change).execute(&mut *blocker).await.unwrap();blocker.commit().await.unwrap();let before=crate::spaces::documents::tests::move_database_snapshot(&s).await;
            assert!(task.await.unwrap().is_err(),"{change}");assert_eq!(crate::spaces::documents::tests::move_database_snapshot(&s).await,before,"{change}");let old:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();assert_eq!(storage::read_document(&old).await.unwrap(),"PUBLIC OLD UNIQUE");cleanup(s,&slug,&path).await;
        }
    }
    #[test]
    fn actual_anonymous_wal_child_has_explicit_deployment_configuration() {
        let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","spaces::drafts::publication_consumer_tests::real_wal_reader_observes_old_public_body_before_commit_then_new_after_commit","--ignored","--nocapture"]).env("ALLOW_PUBLIC_PAGES","true").output().unwrap();
        let stdout=String::from_utf8_lossy(&output.stdout);let stderr=String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(),"{stdout} {stderr}");
        assert!(stdout.contains("running 1 test") && stdout.contains("1 passed; 0 failed") && stdout.contains("spaces::drafts::publication_consumer_tests::real_wal_reader_observes_old_public_body_before_commit_then_new_after_commit"),"Expected actual WAL child execution: {stdout} {stderr}");
        println!("{stdout}");if !stderr.is_empty(){eprintln!("{stderr}");}
    }
    #[tokio::test]
    #[ignore="parent invokes with isolated deployment configuration"]
    async fn real_wal_reader_observes_old_public_body_before_commit_then_new_after_commit() {
        assert!(super::super::documents::public_pages_enabled());
        let (s,slug,path)=legacy_state().await;published(&s).await;let link:String=sqlx::query_scalar("SELECT public_slug FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();let a=auth(&s,"owner-1",i64::MAX).await;let reader=s.clone();let oldlink=link.clone();let staged_dir=storage::space_dir(&slug).join("docs/revisions");let staged_slug=slug.clone();
        let result=PUBLICATION_AFTER_RECEIPT.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            let response=crate::api_routes().with_state(reader.clone()).oneshot(Request::builder().uri(format!("/p/{oldlink}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let value:Value=serde_json::from_slice(&to_bytes(response.into_body(),2_000_000).await.unwrap()).unwrap();assert_eq!(value["content"],"PUBLIC OLD UNIQUE");assert_eq!(value["title"],"Published title");assert_eq!(value["updated_at"],"2026-01-01T00:00:00Z");
            let revision=std::fs::read_dir(staged_dir).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();let uri=format!("/files/spaces/{staged_slug}/docs/revisions/{revision}");
            let anonymous=crate::api_routes().with_state(reader.clone()).oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(anonymous.status(),StatusCode::UNAUTHORIZED);


        }))),publish_with_proof(State(s.clone()),a,route())).await;assert!(result.is_ok());
        let response=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().uri(format!("/p/{link}")).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);let value:Value=serde_json::from_slice(&to_bytes(response.into_body(),2_000_000).await.unwrap()).unwrap();assert_eq!(value["content"],expected_markdown("PUBLIC NEXT UNIQUE"));assert_eq!(value["title"],"Draft title");
        let installed:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();let response=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().uri(format!("/files/{installed}")).header("Authorization",format!("Bearer {}",token(&s,"owner-1",i64::MAX))).body(Body::empty()).unwrap()).await.unwrap();assert_ne!(response.status(),StatusCode::OK);cleanup(s,&slug,&path).await;
    }
}

#[cfg(test)]
mod publication_custom_permission_tests {
    use super::*;
    use axum::{body::Body,extract::FromRequestParts,http::Request};
    #[tokio::test]
    async fn original_write_only_key_with_actual_custom_editcontent_publishes_without_view() {
        let (s,slug,path)=super::publication_consumer_tests::legacy_state().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('publication-edit','space-1','Edit only','[\"edit_content\"]','owner-1')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='publication-edit' WHERE user_id='owner-1'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('publication-write','owner-1','Write',$1,'test','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key("publication-write-raw")).execute(&s.db).await.unwrap();
        let (mut parts,_)=Request::builder().method("POST").header("X-API-Key","publication-write-raw").body(Body::empty()).unwrap().into_parts();let a=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();
        let proof=review_publication(State(s.clone()),copy_publication_auth(&a),Path(("space-1".into(),"doc-1".into(),"draft-1".into()))).await.unwrap().into_response();
        let proof:Value=serde_json::from_slice(&axum::body::to_bytes(proof.into_body(),2_000_000).await.unwrap()).unwrap();assert!(proof.get("published_content").is_none());
        assert!(publish_with_proof(State(s.clone()),a,Path(("space-1".into(),"doc-1".into(),"draft-1".into()))).await.is_ok());
        s.db.close().await;let _=tokio::fs::remove_dir_all(storage::space_dir(&slug)).await;let _=std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod draft_save_cas_tests {
    // Guarded Linux child seam; a default suite sees idle only.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn simultaneous_same_original_save_fresh_process_child() {
        let Ok(root) = std::env::var("STARLANE_SAVE_ACCEPTANCE_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        assert!(root.is_absolute());
        assert_eq!(storage::data_dir(), root);
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::symlink_metadata(&root).unwrap();
        assert!(meta.is_dir() && !meta.file_type().is_symlink());
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
        let phase = std::env::var("STARLANE_SAVE_ACCEPTANCE_PHASE").unwrap();
        if phase == "writer" {
            let (s, slug, path) = fixture().await;
            // Both identities go through the actual JWT extractor/current session
            // path already used by the real HTTP save regression fixture.
            let a = auth(&s, "owner-1").await;
            let b = auth(&s, "editor-1").await;
            let original = source(&s).await;
            let old_path: String =
                sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'")
                    .fetch_one(&s.db)
                    .await
                    .unwrap();
            let old = std::fs::read(root.join(&old_path)).unwrap();
            let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(2));
            let sa = s.clone();
            let sb = s.clone();
            let oa = original.clone();
            let ob = original.clone();
            let ba = barrier.clone();
            let bb = barrier.clone();
            let ta = tokio::spawn(async move {
                core::BEFORE_LOCK
                    .scope(
                        std::cell::RefCell::new(Some(Box::pin(async move {
                            ba.wait().await;
                        }))),
                        save(&sa, &a, &oa, "CONCURRENT OWNER", "same-stamp"),
                    )
                    .await
            });
            let tb = tokio::spawn(async move {
                core::BEFORE_LOCK
                    .scope(
                        std::cell::RefCell::new(Some(Box::pin(async move {
                            bb.wait().await;
                        }))),
                        save(&sb, &b, &ob, "CONCURRENT EDITOR", "same-stamp"),
                    )
                    .await
            });
            let (ra, rb) = tokio::join!(ta, tb);
            let ra = ra.unwrap();
            let rb = rb.unwrap();
            assert_eq!(usize::from(ra.is_ok()) + usize::from(rb.is_ok()), 1);
            let (winner, loser) = if ra.is_ok() {
                ("CONCURRENT OWNER", rb)
            } else {
                ("CONCURRENT EDITOR", ra)
            };
            assert!(
                matches!(loser, Err(AppError::Conflict(_))),
                "second same-original writer must conflict"
            );
            let current = core::selected(
                &mut s.db.acquire().await.unwrap(),
                "space-1",
                "doc-1",
                "draft-1",
            )
            .await
            .unwrap();
            assert_eq!(current["content"], winner);
            assert_eq!(std::fs::read(root.join(&old_path)).unwrap(), old);
            let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM audit_log WHERE action IN('document.draft_update','document.draft_autosave')").fetch_one(&s.db).await.unwrap();
            assert_eq!(count, 2);
            let all = snapshot(&s).await;
            std::fs::write(root.join("race-result.json"),serde_json::to_vec(&json!({"database":path,"slug":slug,"original_digest":original,"winner":winner,"full_rows":all,"canonical_path":old_path,"canonical_bytes":old})).unwrap()).unwrap();
            s.db.close().await;
            std::process::exit(77);
        }
        assert_eq!(phase, "recovery");
        let result: Value =
            serde_json::from_slice(&std::fs::read(root.join("race-result.json")).unwrap()).unwrap();
        let mut s = crate::spaces::properties::tests::property_test_state().await;
        s.db = sqlx::any::AnyPoolOptions::new()
            .max_connections(2)
            .after_connect(|c, _| {
                Box::pin(async move {
                    sqlx::query("PRAGMA foreign_keys=ON")
                        .execute(&mut *c)
                        .await?;
                    sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;
                    Ok(())
                })
            })
            .connect(&format!(
                "sqlite://{}",
                result["database"].as_str().unwrap()
            ))
            .await
            .unwrap();
        let actual = snapshot(&s).await;
        assert_eq!(serde_json::to_value(&actual).unwrap(), result["full_rows"]);
        let original = result["original_digest"].as_str().unwrap();
        let a = auth(&s, "owner-1").await;
        let before_stale = snapshot(&s).await;
        assert!(matches!(
            save(&s, &a, original, "STALE AFTER RESTART", "same-stamp").await,
            Err(AppError::Conflict(_))
        ));
        assert_eq!(snapshot(&s).await, before_stale);
        let current = core::selected(
            &mut s.db.acquire().await.unwrap(),
            "space-1",
            "doc-1",
            "draft-1",
        )
        .await
        .unwrap();
        assert_eq!(current["content"], result["winner"]);
        assert_ne!(core::digest(&current).unwrap(), original);
        let old: Vec<u8> = serde_json::from_value(result["canonical_bytes"].clone()).unwrap();
        assert_eq!(
            std::fs::read(root.join(result["canonical_path"].as_str().unwrap())).unwrap(),
            old
        );
        println!("genuine simultaneous current-JWT saves: exactly one committed, stale peer conflict, whole SQL state and old canonical survive fresh process");
        s.db.close().await;
    }
    use super::*;
    use super::super::draft_save as core;
    use axum::{body::{Body,to_bytes},extract::FromRequestParts,http::Request};
    use tower::ServiceExt;
    async fn fixture()->(AppState,String,std::path::PathBuf) {super::publication_consumer_tests::legacy_state().await}
    async fn auth(s:&AppState,user:&str)->AuthUser {super::publication_consumer_tests::legacy_auth(s,user).await}
    async fn source(s:&AppState)->String {let mut c=s.db.acquire().await.unwrap();core::digest(&core::selected(&mut c,"space-1","doc-1","draft-1").await.unwrap()).unwrap()}
    async fn snapshot(s:&AppState)->Vec<(String,Vec<Value>)> {
        let mut c=s.db.acquire().await.unwrap();
        // SQLite emits TEXT for every raw row, including declared DATETIME.
        // No table or security column is excluded, including migrations.
        let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").fetch_all(&mut *c).await.unwrap();
        let mut out=Vec::new();
        for t in tables {
            let quoted=t.replace('"',r#""""#);
            let columns=sqlx::query(&format!(r#"PRAGMA table_info("{quoted}")"#)).fetch_all(&mut *c).await.unwrap();
            let fields=columns.iter().map(|column| {
                let name:String=column.get("name");let identifier=name.replace('"',r#""""#);let key=name.replace(char::from(39),"''");
                // Blob bytes remain exact hex, not a lossy driver conversion.
                format!(r#"'{key}',CASE WHEN typeof("{identifier}")='blob' THEN json_object('__sqlite_blob_hex',hex("{identifier}")) ELSE "{identifier}" END"#)
            }).collect::<Vec<_>>().join(",");
            let raw:Vec<String>=sqlx::query_scalar(&format!(r#"SELECT json_object({fields}) FROM "{quoted}" ORDER BY rowid"#)).fetch_all(&mut *c).await.unwrap();
            out.push((t,raw.into_iter().map(|row|serde_json::from_str(&row).unwrap()).collect()));
        }
        out
    }
    async fn cleanup(s:AppState,slug:&str,path:&std::path::Path) {s.db.close().await;let _=tokio::fs::remove_dir_all(storage::space_dir(slug)).await;let _=std::fs::remove_file(path);}
    async fn save(s:&AppState,a:&AuthUser,source:&str,body:&str,stamp:&str)->Result<core::Saved,AppError> {
        core::save(s,a,"space-1","doc-1","draft-1",source,core::Patch{content:Some(body.into()),..Default::default()},stamp,None).await
    }
    fn bearer(s:&AppState,a:&AuthUser)->String {jsonwebtoken::encode(&jsonwebtoken::Header::default(),&a.claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap()}
    #[tokio::test]
    async fn actual_http_fetch_and_save_return_exact_full_raw_original_and_result_sources() {
        let (s,slug,path)=fixture().await;let a=auth(&s,"editor-1").await;let url="/spaces/space-1/documents/doc-1/drafts/draft-1";
        let response=router().with_state(s.clone()).oneshot(Request::builder().uri(url).header("Authorization",format!("Bearer {}",bearer(&s,&a))).body(Body::empty()).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
        let v:Value=serde_json::from_slice(&to_bytes(response.into_body(),usize::MAX).await.unwrap()).unwrap();assert_eq!(v["source_digest"],source(&s).await);
        let resumed=router().with_state(s.clone()).oneshot(Request::builder().method("POST").uri("/spaces/space-1/documents/doc-1/drafts").header("Authorization",format!("Bearer {}",bearer(&s,&a))).header("Content-Type","application/json").body(Body::from(json!({"source_draft_id":"draft-1","source_digest":v["source_digest"],"operation_id":Uuid::new_v4().to_string()}).to_string())).unwrap()).await.unwrap();
        assert_eq!(resumed.status(),StatusCode::CREATED);
        let created:Value=serde_json::from_slice(&to_bytes(resumed.into_body(),usize::MAX).await.unwrap()).unwrap();
        let selected=core::selected(&mut s.db.acquire().await.unwrap(),"space-1","doc-1",created["id"].as_str().unwrap()).await.unwrap();assert_eq!(created["source_digest"],core::digest(&selected).unwrap());
        let response=router().with_state(s.clone()).oneshot(Request::builder().method("PUT").uri(url).header("Authorization",format!("Bearer {}",bearer(&s,&a))).header("Content-Type","application/json").body(Body::from(json!({"source_digest":v["source_digest"],"content":"HTTP accepted","title":" New title ","icon":"","cover_image":"   "}).to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::OK);
        let v:Value=serde_json::from_slice(&to_bytes(response.into_body(),usize::MAX).await.unwrap()).unwrap();assert_eq!(v["source_digest"],source(&s).await);assert_eq!(v["content"],"HTTP accepted");assert_eq!(v["title"],"New title");assert!(v["icon"].is_null()&&v["cover_image"].is_null());assert_eq!(v["access_role"],"editor");assert_eq!(v["can_publish"],false);
        let rows=super::super::document_deletion::rows(&mut s.db.acquire().await.unwrap(),"SELECT * FROM audit_log",&[]).await.unwrap();let saved:Vec<_>=rows.into_iter().filter(|r|r["action"]=="document.draft_update"||r["action"]=="document.draft_autosave").collect();assert_eq!(saved.len(),2);for r in saved {assert_eq!(r["actor_id"],"editor-1");assert_eq!(r["actor_kind"],"user");}
        cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn simultaneous_signed_owner_and_shared_editor_put_keep_one_exact_winner() {
        let (state, slug, path) = fixture().await;
        let owner = auth(&state, "owner-1").await;
        let editor = auth(&state, "editor-1").await;
        let original_source = source(&state).await;
        let journal: String = sqlx::query_scalar("PRAGMA journal_mode")
            .fetch_one(&state.db).await.unwrap();
        assert_eq!(journal.to_lowercase(), "wal");
        assert!(path.is_file(), "concurrency fixture must use a real SQLite file");
        let before = snapshot(&state).await;
        let page = before.iter().find(|(name, _)| name == "documents").unwrap().1[0].clone();
        let canonical_path = page["content_path"].as_str().unwrap();
        let published_bytes = storage::read_document(canonical_path).await.unwrap();
        let intents = [
            json!({"source_digest":original_source,"title":" Owner winner candidate ","content":"OWNER RETAINED BODY","icon":"owner-icon","cover_image":"https://fictional.invalid/owner.png"}),
            json!({"source_digest":original_source,"title":" Editor winner candidate ","content":"EDITOR RETAINED BODY","icon":"editor-icon","cover_image":"https://fictional.invalid/editor.png"}),
        ];
        let retained_intents = intents.clone();
        // Three-party gate proves both independently authenticated HTTP requests
        // have entered their actual SQL save transactions before either can write.
        let barrier = std::sync::Arc::new(tokio::sync::Barrier::new(3));
        let mut tasks = Vec::new();
        let mut arrivals = Vec::new();
        for (index, actor) in [owner, editor].into_iter().enumerate() {
            let request = Request::builder().method("PUT")
                .uri("/spaces/space-1/documents/doc-1/drafts/draft-1")
                .header("Authorization", format!("Bearer {}", bearer(&state, &actor)))
                .header("Content-Type", "application/json")
                .body(Body::from(intents[index].to_string())).unwrap();
            let app = router().with_state(state.clone());
            let gate = barrier.clone();
            let (reached, arrived) = tokio::sync::oneshot::channel();
            arrivals.push(arrived);
            tasks.push(tokio::spawn(async move {
                core::BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move {
                    reached.send(()).unwrap();
                    gate.wait().await;
                }))), async move {
                    let response = app.oneshot(request).await.unwrap();
                    let status = response.status();
                    let body: Value = serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
                    (index, status, body)
                }).await
            }));
        }
        for arrival in arrivals {
            tokio::time::timeout(std::time::Duration::from_secs(10), arrival).await
                .expect("both HTTP saves must reach the transaction gate").unwrap();
        }
        assert!(tasks.iter().all(|task| !task.is_finished()));
        barrier.wait().await;
        let mut outcomes = Vec::new();
        for task in tasks {
            outcomes.push(tokio::time::timeout(std::time::Duration::from_secs(10), task).await
                .expect("competing WAL saves must terminate").unwrap());
        }
        assert_eq!(outcomes.iter().filter(|(_, status, _)| *status == StatusCode::OK).count(), 1, "{outcomes:?}");
        assert_eq!(outcomes.iter().filter(|(_, status, _)| *status == StatusCode::CONFLICT).count(), 1, "{outcomes:?}");
        let (winner_index, _, winner) = outcomes.iter().find(|(_, status, _)| *status == StatusCode::OK).unwrap();
        let (loser_index, _, loser) = outcomes.iter().find(|(_, status, _)| *status == StatusCode::CONFLICT).unwrap();
        assert!(loser["message"].as_str().unwrap().starts_with("source_conflict:"));
        let accepted = &intents[*winner_index];
        for field in ["content", "icon", "cover_image"] { assert_eq!(winner[field], accepted[field]); }
        assert_eq!(winner["title"], accepted["title"].as_str().unwrap().trim());
        assert_eq!(winner["source_digest"], source(&state).await);
        assert_eq!(intents, retained_intents, "losing submitted bytes remain available to the caller");
        let final_draft = core::selected(&mut state.db.acquire().await.unwrap(), "space-1", "doc-1", "draft-1").await.unwrap();
        for field in ["title", "content", "icon", "cover_image"] { assert_eq!(final_draft[field], winner[field]); }
        assert_ne!(final_draft["content"], intents[*loser_index]["content"]);
        assert_eq!(storage::read_document(canonical_path).await.unwrap(), published_bytes);
        let mut after = snapshot(&state).await;
        let original_audits = &before.iter().find(|(name, _)| name == "audit_log").unwrap().1;
        let audits = &mut after.iter_mut().find(|(name, _)| name == "audit_log").unwrap().1;
        let added: Vec<_> = audits.iter().filter(|row| !original_audits.contains(row)).cloned().collect();
        let winner_actor = if *winner_index == 0 { "owner-1" } else { "editor-1" };
        let loser_actor = if *loser_index == 0 { "owner-1" } else { "editor-1" };
        assert_eq!(added.len(), 3);
        for action in ["document.draft_update", "document.draft_autosave"] {
            let matching: Vec<_> = added.iter().filter(|row| row["action"] == action).collect();
            assert_eq!(matching.len(), 1);
            assert_eq!(matching[0]["actor_id"], winner_actor);
            assert_eq!(matching[0]["actor_kind"], "user");
            assert_eq!(matching[0]["target_id"], "draft-1");
        }
        let failed: Vec<_> = added.iter().filter(|row| row["action"] == "document.draft_update_failed").collect();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0]["user_id"], loser_actor);
        *audits = original_audits.clone();
        let drafts = &mut after.iter_mut().find(|(name, _)| name == "document_drafts").unwrap().1;
        let mut expected_draft = before.iter().find(|(name, _)| name == "document_drafts").unwrap().1[0].clone();
        for field in ["title", "content", "icon", "cover_image", "updated_at"] { expected_draft[field] = final_draft[field].clone(); }
        assert_eq!(*drafts, vec![expected_draft]);
        *drafts = before.iter().find(|(name, _)| name == "document_drafts").unwrap().1.clone();
        // Signed HTTP extraction legitimately renews rolling session timestamps.
        // Every domain row, immutable receipt, publication and version is unchanged
        // except the exact one draft update and its three explicitly proved audits.
        after.retain(|(name, _)| name != "sessions");
        let mut expected = before; expected.retain(|(name, _)| name != "sessions");
        assert_eq!(after, expected);
        cleanup(state, &slug, &path).await;
    }
    #[tokio::test]
    async fn two_clients_original_source_cas_and_same_stamp_drift_preserve_whole_db() {
        let(s,slug,path)=fixture().await;let a=auth(&s,"owner-1").await;let b=auth(&s,"editor-1").await;let original=source(&s).await;
        save(&s,&b,&original,"NEWER B","same-stamp").await.unwrap();let committed=snapshot(&s).await;
        assert!(matches!(save(&s,&a,&original,"OLDER A","later").await,Err(AppError::Conflict(_))));assert_eq!(snapshot(&s).await,committed);
        for (key,value) in [("content","other body"),("title","other title"),("base_updated_at","other base"),("previous_folder_id","prior-placement")] {
            let stale=source(&s).await;sqlx::query(&format!("UPDATE document_drafts SET {key}=$1 WHERE id='draft-1'")).bind(value).execute(&s.db).await.unwrap();let before=snapshot(&s).await;
            assert!(save(&s,&a,&stale,"STALE","same-stamp").await.is_err());assert_eq!(snapshot(&s).await,before);
        }
        cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn missing_or_invalid_source_returns_actionable_conflict_with_only_failure_audit() {
        let(s,slug,path)=fixture().await;
        for source_digest in [None,Some(Value::Null),Some(json!(false)),Some(json!("A".repeat(64))),Some(json!("a".repeat(63)))] {
            let a=auth(&s,"owner-1").await;let before=snapshot(&s).await;
            let error=update_draft(State(s.clone()),a,Path(("space-1".into(),"doc-1".into(),"draft-1".into())),Json(UpdateDraftRequest{source_digest,title:None,icon:None,cover_image:None,content:Some("retained editor".into())})).await.err().unwrap();assert!(matches!(error,AppError::Conflict(ref m) if m.starts_with("source_required:")));
            let mut after=snapshot(&s).await;let old=before.iter().find(|r|r.0=="audit_log").unwrap().1.clone();let audits=&mut after.iter_mut().find(|r|r.0=="audit_log").unwrap().1;
            let added:Vec<_>=audits.iter().filter(|r|!old.contains(r)).cloned().collect();assert_eq!(added.len(),1);assert_eq!(added[0]["action"],"document.draft_update_failed");assert_eq!(added[0]["user_id"],"owner-1");*audits=old;assert_eq!(after,before);
        }
        cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn selected_private_editor_lock_and_current_session_are_real_authority() {
        let(s,slug,path)=fixture().await;let owner=auth(&s,"owner-1").await;let editor=auth(&s,"editor-1").await;let viewer=auth(&s,"viewer-1").await;let original=source(&s).await;let before=snapshot(&s).await;
        assert!(save(&s,&viewer,&original,"VIEWER","v").await.is_err());assert_eq!(snapshot(&s).await,before);
        save(&s,&editor,&original,"SHARED EDITOR","e").await.unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-1' WHERE id='doc-1'").execute(&s.db).await.unwrap();let original=source(&s).await;let before=snapshot(&s).await;assert!(save(&s,&editor,&original,"LOCKED","l").await.is_err());assert_eq!(snapshot(&s).await,before);
        sqlx::query("UPDATE documents SET locked=0,locked_by=NULL WHERE id='doc-1'").execute(&s.db).await.unwrap();sqlx::query("DELETE FROM sessions WHERE user_id='owner-1'").execute(&s.db).await.unwrap();let before=snapshot(&s).await;assert!(save(&s,&owner,&original,"EXPIRED","x").await.is_err());assert_eq!(snapshot(&s).await,before);
        cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn audit_receipt_draft_and_policy_trigger_drift_roll_back_every_owned_write() {
        for (table,trigger) in [
            ("audit_log","UPDATE audit_log SET after_value='tamper' WHERE id=NEW.id"),
            ("audit_log","UPDATE document_drafts SET base_updated_at='tamper' WHERE id='draft-1'"),
            ("audit_log","DELETE FROM document_draft_access WHERE draft_id='draft-1'"),
            ("draft_save_replay_receipts","UPDATE draft_save_replay_receipts SET receipt_json='{}' WHERE incoming_id=NEW.incoming_id"),
            ("draft_save_replay_receipts","UPDATE users SET is_active=0 WHERE id='editor-1'"),
            ("draft_save_replay_receipts","UPDATE documents SET locked=1,locked_by='owner-1' WHERE id='doc-1'")
        ] {
            let(s,slug,path)=fixture().await;let a=auth(&s,"editor-1").await;let original=source(&s).await;
            sqlx::query(&format!("CREATE TRIGGER drift AFTER INSERT ON {table} BEGIN {trigger}; END")).execute(&s.db).await.unwrap();let before=snapshot(&s).await;
            let incoming=storage::canonical_rewrite::IncomingIdentity{provider:"provider-A".into(),operation_id:"save-op".into(),entity_type:"document".into(),entity_id:"doc-1".into(),action:"update_content".into(),payload_fingerprint:"b".repeat(64)};
            assert!(core::save(&s,&a,"space-1","doc-1","draft-1",&original,core::Patch{content:Some("INTENDED".into()),..Default::default()},"stamp",Some(&incoming)).await.is_err(),"{trigger}");assert_eq!(snapshot(&s).await,before,"{trigger}");cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn real_wal_wait_reproof_rejects_revoked_original_session_without_save() {
        let(s,slug,path)=fixture().await;let a=auth(&s,"editor-1").await;let original=source(&s).await;
        let mut blocking=s.db.begin().await.unwrap();super::super::mutation_guards::lock_audience_for_mutation(&mut blocking).await.unwrap();
        let (begun,entered)=tokio::sync::oneshot::channel();let worker=s.clone();
        let task=tokio::spawn(async move {core::BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move {begun.send(()).unwrap();}))),save(&worker,&a,&original,"SHOULD RETAIN","wal")).await});
        entered.await.unwrap();sqlx::query("DELETE FROM sessions WHERE user_id='editor-1'").execute(&mut *blocking).await.unwrap();blocking.commit().await.unwrap();let before=snapshot(&s).await;
        assert!(task.await.unwrap().is_err());assert_eq!(snapshot(&s).await,before);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn last_actual_jwt_expiry_after_source_and_audit_proofs_rolls_back() {
        let(s,slug,path)=fixture().await;let mut claims=auth(&s,"owner-1").await.claims;claims.exp=chrono::Utc::now().timestamp()+2;
        let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
        let(mut parts,_)=Request::builder().method("PUT").header("Authorization",format!("Bearer {token}")).body(Body::empty()).unwrap().into_parts();
        let a=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();let original=source(&s).await;let before=snapshot(&s).await;
        let result=core::BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async {tokio::time::sleep(std::time::Duration::from_secs(3)).await;}))),save(&s,&a,&original,"RETENTAINED","expiry")).await;
        assert!(result.is_err());assert_eq!(snapshot(&s).await,before);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_selected_get_wal_content_and_presentation_drift_cannot_mix_source() {
        for sql in ["UPDATE document_drafts SET content='NEW SAME STAMP' WHERE id='draft-1'", "UPDATE document_drafts SET title='NEW SAME STAMP',icon='new' WHERE id='draft-1'", "UPDATE document_draft_access SET role='viewer' WHERE draft_id='draft-1' AND user_id='editor-1'"] {
            let(s,slug,path)=fixture().await;let a=auth(&s,"editor-1").await;let observer=s.clone();
            let result=core::BEFORE_READ_PROOF.scope(std::cell::RefCell::new(Some(Box::pin(async move {sqlx::query(sql).execute(&observer.db).await.unwrap();}))),get_draft(State(s.clone()),a,Path(("space-1".into(),"doc-1".into(),"draft-1".into())))).await;
            assert!(matches!(result,Err(AppError::Conflict(_))),"{sql}");
            let a=auth(&s,"editor-1").await;let clean=get_draft(State(s.clone()),a,Path(("space-1".into(),"doc-1".into(),"draft-1".into()))).await.unwrap().into_response();
            let v:Value=serde_json::from_slice(&to_bytes(clean.into_body(),usize::MAX).await.unwrap()).unwrap();assert_eq!(v["source_digest"],source(&s).await);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn actual_get_late_wal_policy_after_inventory_rejects_with_live_read_identity() {
        let(s,slug,path)=fixture().await;let a=auth(&s,"editor-1").await;
        let before=source(&s).await;let observer=s.clone();let current=auth(&s,"editor-1").await;
        let result=core::AFTER_READ_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            // A distinct pooled connection commits to the actual migrated disk-WAL
            // after the saved inventory has been checked. Credentials stay valid.
            sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','editor-1','owner-1')").execute(&observer.db).await.unwrap();
            let mut conn=observer.db.acquire().await.unwrap();
            access::require_current_human_read(&mut conn,&current).await.unwrap();
        }))),get_draft(State(s.clone()),a,Path(("space-1".into(),"doc-1".into(),"draft-1".into())))).await;
        assert!(result.is_err(),"Late page policy must not survive the final typed proof");
        assert_eq!(source(&s).await,before);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT COUNT(*) FROM audit_log WHERE action='document.draft_update' OR action='document.draft_autosave'").fetch_one(&s.db).await.unwrap(),0);
        sqlx::query("DELETE FROM space_user_blocks WHERE space_id='space-1' AND user_id='editor-1'").execute(&s.db).await.unwrap();
        let clean=get_draft(State(s.clone()),auth(&s,"editor-1").await,Path(("space-1".into(),"doc-1".into(),"draft-1".into()))).await.unwrap().into_response();
        let v:Value=serde_json::from_slice(&to_bytes(clean.into_body(),usize::MAX).await.unwrap()).unwrap();assert_eq!(v["source_digest"],before);
        cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_write_only_edit_key_save_succeeds_read_and_service_keys_fail_unchanged() {
        let(s,slug,path)=fixture().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('save-edit-only','space-1','Edit only','[\"edit_content\"]','owner-1')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='save-edit-only' WHERE user_id='editor-1'").execute(&s.db).await.unwrap();
        for(scopes,service)in[("[\"write\"]",0),("[\"read\"]",0),("[\"admin\"]",1)] {
            let raw=format!("draft-key-{}",Uuid::new_v4());let key_id=Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,'editor-1','Draft actual key',$2,'test',$3,$4)").bind(&key_id).bind(crate::auth::api_keys::hash_api_key(&raw)).bind(scopes).bind(service).execute(&s.db).await.unwrap();
            let(mut parts,_)=Request::builder().method(if scopes=="[\"read\"]" {"GET"}else{"PUT"}).header("X-API-Key",raw).body(Body::empty()).unwrap().into_parts();
            let a=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();let original=source(&s).await;let before=snapshot(&s).await;
            let result=save(&s,&a,&original,"KEY ACCEPTED","key").await;
            if scopes=="[\"write\"]" {assert!(result.is_ok());}else{assert!(result.is_err());assert_eq!(snapshot(&s).await,before);}
        }
        cleanup(s,&slug,&path).await;
    }

}

#[cfg(test)] async fn draft_reader_before_identity(){if let Ok(Some(h))=DRAFT_READER_BEFORE_IDENTITY.try_with(|s|s.borrow_mut().take()){h.await;}}

#[cfg(test)]
pub(crate) mod draft_reader_current_tests {
    use super::*;
    use axum::{body::{Body,to_bytes},http::Request};
    use tower::ServiceExt;
    pub(crate) async fn fixture()->(AppState,String,std::path::PathBuf){super::publication_consumer_tests::legacy_state().await}
    pub(crate) fn token(s:&AppState,user:&str)->String {
        let mut claims=crate::spaces::properties::tests::test_auth(user).claims;claims.sid=format!("publication-{user}");claims.exp=i64::MAX;
        jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap()
    }
    pub(crate) async fn request(s:&AppState,user:&str,path:&str)->(StatusCode,Value){
        let r=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().uri(path).header("Authorization",format!("Bearer {}",token(s,user))).body(Body::empty()).unwrap()).await.unwrap();
        let status=r.status();let bytes=to_bytes(r.into_body(),2_000_000).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }
    pub(crate) async fn snapshot(s:&AppState,bookkeeping:bool)->Vec<(String,Vec<Value>)> {
        let mut c=s.db.acquire().await.unwrap();
        // SQLite emits TEXT for every raw row, including declared DATETIME.
        // No table or security column is excluded, including migrations.
        let tables:Vec<String>=sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name").fetch_all(&mut *c).await.unwrap();
        let mut out=Vec::new();
        for t in tables {
            let quoted=t.replace('"',r#""""#);
            let columns=sqlx::query(&format!(r#"PRAGMA table_info("{quoted}")"#)).fetch_all(&mut *c).await.unwrap();
            let fields=columns.iter().map(|column| {
                let name:String=column.get("name");let identifier=name.replace('"',r#""""#);let key=name.replace(char::from(39),"''");
                // Blob bytes remain exact hex, not a lossy driver conversion.
                format!(r#"'{key}',CASE WHEN typeof("{identifier}")='blob' THEN json_object('__sqlite_blob_hex',hex("{identifier}")) ELSE "{identifier}" END"#)
            }).collect::<Vec<_>>().join(",");
            let raw:Vec<String>=sqlx::query_scalar(&format!(r#"SELECT json_object({fields}) FROM "{quoted}" ORDER BY rowid"#)).fetch_all(&mut *c).await.unwrap();
            let mut rows:Vec<Value>=raw.into_iter().map(|row|serde_json::from_str(&row).unwrap()).collect();
            if bookkeeping {for row in &mut rows {if t=="api_keys" {row.as_object_mut().unwrap().remove("last_used_at");}if t=="sessions" {row.as_object_mut().unwrap().remove("last_seen_at");row.as_object_mut().unwrap().remove("expires_at");}}rows.sort_by_cached_key(Value::to_string);}
            out.push((t,rows));
        }
        out
    }
    fn files(root:&std::path::Path)->Vec<(String,Vec<u8>)>{
        fn walk(root:&std::path::Path,p:&std::path::Path,out:&mut Vec<(String,Vec<u8>)>){if !p.exists(){return;}for e in std::fs::read_dir(p).unwrap(){let p=e.unwrap().path();if p.is_dir(){walk(root,&p,out);}else{out.push((p.strip_prefix(root).unwrap().to_string_lossy().into_owned(),std::fs::read(&p).unwrap()));}}}
        let mut out=Vec::new();walk(root,root,&mut out);out.sort_by(|a,b|a.0.cmp(&b.0));out
    }
    pub(crate) async fn cleanup(s:AppState,slug:&str,path:&std::path::Path){s.db.close().await;let _=tokio::fs::remove_dir_all(storage::space_dir(slug)).await;let _=std::fs::remove_file(path);}
    pub(crate) async fn auth(s:&AppState,user:&str)->AuthUser{super::publication_consumer_tests::legacy_auth(s,user).await}
    async fn expiring_auth(s:&AppState,user:&str)->AuthUser {
        use axum::extract::FromRequestParts;
        let mut claims=crate::spaces::properties::tests::test_auth(user).claims;claims.sid=format!("publication-{user}");claims.exp=chrono::Utc::now().timestamp()+3;
        let signed=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
        let (mut parts,_)=Request::builder().uri("/spaces/space-1/drafts").header("Authorization",format!("Bearer {signed}")).body(Body::empty()).unwrap().into_parts();
        AuthUser::from_request_parts(&mut parts,s).await.unwrap()
    }
    async fn list(s:AppState,a:AuthUser)->Result<axum::response::Response,AppError>{list_drafts(State(s),a,Path("space-1".into()),Query(ListDraftsQuery::default())).await.map(IntoResponse::into_response)}
    #[tokio::test]
    async fn actual_http_private_summary_uses_exact_draft_title_without_global_purge_or_mirror_writes(){
        let (s,slug,path)=fixture().await;
        sqlx::query("UPDATE documents SET title='SECRET CANONICAL' WHERE id='doc-1'").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE document_drafts SET updated_at='2000-01-01T00:00:00Z' WHERE id='draft-1'").execute(&s.db).await.unwrap();
        let foreign_slug=format!("foreign-reader-{}",Uuid::new_v4());sqlx::query("INSERT INTO spaces(id,name,slug,created_by,visibility) VALUES('foreign','Foreign',$1,'owner-1','private')").bind(&foreign_slug).execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('foreign-page','foreign','Hidden victim','unused','owner-1','owner-1')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('victim','foreign-page','foreign','owner-1','Hidden victim','RETAIN FOREIGN BYTES','old','2000-01-01T00:00:00Z')").execute(&s.db).await.unwrap();
        let marker=storage::space_dir(&slug).join("reader-must-retain.txt");tokio::fs::write(&marker,b"RETAIN ORIGINAL BYTES").await.unwrap();
        storage::write_lifecycle_document(&foreign_slug,"drafts","victim","Hidden victim","","RETAIN FOREIGN BYTES").await.unwrap();
        let before_files=files(&storage::space_dir(&slug));let before_foreign=files(&storage::space_dir(&foreign_slug));let before=snapshot(&s,true).await;let (status,v)=request(&s,"editor-1","/spaces/space-1/drafts").await;
        assert_eq!(status,StatusCode::OK,"{v}");assert_eq!(v.as_array().unwrap().len(),1);assert_eq!(v[0]["title"],"Draft title");assert_eq!(v[0]["document_title"],"Draft title");assert!(!v.to_string().contains("SECRET CANONICAL"));assert!(!v.to_string().contains("Hidden victim"));
        assert_eq!(snapshot(&s,true).await,before);assert_eq!(tokio::fs::read(&marker).await.unwrap(),b"RETAIN ORIGINAL BYTES");assert_eq!(files(&storage::space_dir(&slug)),before_files);assert_eq!(files(&storage::space_dir(&foreign_slug)),before_foreign);
        assert!(!storage::space_dir(&slug).join("drafts").exists(),"GET must not create lifecycle mirrors");let _=tokio::fs::remove_dir_all(storage::space_dir(&foreign_slug)).await;cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_http_custom_view_read_key_and_original_method_ceilings(){
        let (s,slug,path)=fixture().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom-reader','space-1','Custom reader','[\"view\"]','owner-1')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom-reader' WHERE user_id='editor-1'").execute(&s.db).await.unwrap();
        for (id,scopes,service) in [("reader-key",r#"["read"]"#,0_i64),("writer-key",r#"["write"]"#,0),("service-key",r#"["read","admin"]"#,1)] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,'editor-1',$1,$2,'fixture',$3,$4)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(scopes).bind(service).execute(&s.db).await.unwrap();
            let r=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().uri("/spaces/space-1/drafts").header("X-API-Key",id).body(Body::empty()).unwrap()).await.unwrap();
            if id=="reader-key" {assert_eq!(r.status(),StatusCode::OK);}else{assert_ne!(r.status(),StatusCode::OK);}
        }
        assert_eq!(request(&s,"editor-1","/spaces/space-1/drafts").await.0,StatusCode::OK);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_http_restricted_ancestor_and_lifecycle_omit_private_rows_without_label_leaks(){
        let (s,slug,path)=fixture().await;
        assert_eq!(request(&s,"viewer-1","/spaces/space-1/drafts").await.1,json!([]));
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('parent','space-1','SECRET PARENT','unused','owner-1','owner-1','2026-01-01T00:00:00Z')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='parent' WHERE id='doc-1'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('parent','space-1','restricted','r1','owner-1')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('allow-owner','parent','space-1','owner-1','owner-1')").execute(&s.db).await.unwrap();
        let (status,v)=request(&s,"editor-1","/spaces/space-1/drafts").await;assert_eq!(status,StatusCode::OK);assert_eq!(v,json!([]));assert!(!v.to_string().contains("SECRET"));
        assert_eq!(request(&s,"owner-1","/spaces/space-1/drafts").await.1.as_array().unwrap().len(),1);
        sqlx::query("UPDATE document_access_policies SET mode='inherited' WHERE document_id='parent'").execute(&s.db).await.unwrap();
        for column in ["archived_at","deleted_at"] {sqlx::query(&format!("UPDATE documents SET {column}='2026-01-01T00:00:00Z' WHERE id='doc-1'")).execute(&s.db).await.unwrap();assert_eq!(request(&s,"editor-1","/spaces/space-1/drafts").await.1,json!([]));sqlx::query(&format!("UPDATE documents SET {column}=NULL WHERE id='doc-1'")).execute(&s.db).await.unwrap();}
        sqlx::query("UPDATE documents SET title='Actual published title',published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();assert_eq!(request(&s,"editor-1","/spaces/space-1/drafts").await.1[0]["document_title"],"Actual published title");cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn disk_wal_final_inventory_rejects_selected_tuple_owner_label_and_new_row_drift(){
        for sql in ["UPDATE document_drafts SET title='NEWER SAME STAMP' WHERE id='draft-1'","UPDATE users SET display_name='NEWER OWNER LABEL' WHERE id='owner-1'","INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('new-row','doc-1','space-1','editor-1','New row','body','base')"] {
            let (s,slug,path)=fixture().await;let a=auth(&s,"editor-1").await;let db=s.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
            let r=DRAFT_READER_BEFORE_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query(sql).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);}))),list(s.clone(),a)).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert!(matches!(r,Err(AppError::Conflict(_))));assert_eq!(request(&s,"editor-1","/spaces/space-1/drafts").await.0,StatusCode::OK);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn disk_wal_late_policy_and_selected_share_loss_fail_with_alternate_page_view(){
        for sql in ["INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','editor-1','owner-1')","DELETE FROM document_draft_access WHERE draft_id='draft-1' AND user_id='editor-1'"] {
            let (s,slug,path)=fixture().await;sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('alternate','doc-1','space-1','editor-1','Alternate','body','base')").execute(&s.db).await.unwrap();let a=auth(&s,"editor-1").await;let check=auth(&s,"editor-1").await;let db=s.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
            let r=DRAFT_READER_AFTER_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query(sql).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);}))),list(s.clone(),a)).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert!(r.is_err());let mut c=s.db.acquire().await.unwrap();assert!(access::require_current_human_read(&mut c,&check).await.is_ok());
            if sql.starts_with("DELETE") {assert!(access::resolve_page_access(&mut c,access::principal_from_auth(&check).unwrap(),"space-1","doc-1",PageOperation::View,Purpose::Routine).await.is_ok(),"alternate draft retains actual page View");}drop(c);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn empty_list_rechecks_revoked_identity_and_final_jwt_expiry(){
        let (s,slug,path)=fixture().await;let a=expiring_auth(&s,"viewer-1").await;let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
        // Reach LAST identity with an already-expired original token; DB session stays long lived.
        let r=DRAFT_READER_BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move{tokio::time::sleep(std::time::Duration::from_secs(4)).await;flag.store(true,std::sync::atomic::Ordering::SeqCst);}))),list(s.clone(),a)).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert!(matches!(r,Err(AppError::Unauthorized(_))));
        let a=auth(&s,"viewer-1").await;let db=s.db.clone();let r=DRAFT_READER_AFTER_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query("DELETE FROM sessions WHERE id='publication-viewer-1'").execute(&db).await.unwrap();}))),list(s.clone(),a)).await;assert!(matches!(r,Err(AppError::Unauthorized(_))));cleanup(s,&slug,&path).await;
    }
}

#[cfg(test)]
mod draft_birth_tests {
    use super::*;
    use super::super::draft_birth as birth;
    use super::draft_reader_current_tests::{fixture,snapshot,cleanup,auth};
    use axum::{body::{Body,to_bytes},extract::FromRequestParts,http::Request};
    use tower::ServiceExt;
    fn request()->birth::Request {birth::Request{force_new:Some(true),operation_id:Some(Uuid::new_v4().to_string()),source_draft_id:Some("draft-1".into()),source_digest:None}}
    async fn original(s:&AppState)->Value {super::super::draft_save::selected(&mut s.db.acquire().await.unwrap(),"space-1","doc-1","draft-1").await.unwrap()}
    async fn fork_request(s:&AppState)->birth::Request {let mut r=request();r.source_digest=Some(json!(super::super::draft_save::digest(&original(s).await).unwrap()));r}
    async fn create(s:&AppState,original:&AuthUser,r:birth::Request)->Result<birth::Outcome,AppError> {birth::create(s,original,"space-1","doc-1",r).await}
    async fn http(s:&AppState,user:&str,r:&birth::Request)->(StatusCode,Value) {
        let bookkeeping:Vec<(String,Option<String>,String)>=sqlx::query_as("SELECT id,last_seen_at,expires_at FROM sessions WHERE user_id=$1").bind(user).fetch_all(&s.db).await.unwrap();
        let a=auth(s,user).await;let token=jsonwebtoken::encode(&jsonwebtoken::Header::default(),&a.claims,&jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes())).unwrap();
        let response=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().method("POST").uri("/spaces/space-1/documents/doc-1/drafts").header("Authorization",format!("Bearer {token}")).header("Content-Type","application/json").body(Body::from(serde_json::to_vec(r).unwrap())).unwrap()).await.unwrap();
        for (id,last,expires) in bookkeeping {sqlx::query("UPDATE sessions SET last_seen_at=$1,expires_at=$2 WHERE id=$3").bind(last).bind(expires).bind(id).execute(&s.db).await.unwrap();}
        let status=response.status();let bytes=to_bytes(response.into_body(),usize::MAX).await.unwrap();(status,serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn actual_http_private_birth_resume_force_and_exact_receipt() {
        let(s,slug,path)=fixture().await;let r=fork_request(&s).await;
        let(status,a)=http(&s,"editor-1",&r).await;assert_eq!(status,StatusCode::CREATED);assert_eq!(a["content"],"Original body");assert_eq!(a["title"],"Draft title");assert_eq!(a["birth_operation_id"],r.operation_id.clone().unwrap());assert_eq!(a["owner_id"],"editor-1");
        let result=super::super::draft_save::selected(&mut s.db.acquire().await.unwrap(),"space-1","doc-1",a["id"].as_str().unwrap()).await.unwrap();assert_eq!(a["source_digest"],super::super::draft_save::digest(&result).unwrap());
        let before=snapshot(&s,false).await;let(status,b)=http(&s,"editor-1",&r).await;assert_eq!(status,StatusCode::CREATED);assert_eq!(a,b);assert_eq!(snapshot(&s,false).await,before);
        let(status,resumed)=http(&s,"editor-1",&birth::Request::default()).await;assert_eq!(status,StatusCode::OK);assert_eq!(resumed["id"],a["id"]);assert_eq!(snapshot(&s,false).await,before);
        let mut second=r.clone();second.operation_id=Some(Uuid::new_v4().to_string());let(status,b)=http(&s,"editor-1",&second).await;assert_eq!(status,StatusCode::CREATED);assert_ne!(a["id"],b["id"]);
        assert!(!storage::space_dir(&slug).join("drafts").exists());cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn private_missing_viewer_stale_and_foreign_sources_preserve_all_rows_and_shell() {
        let(s,slug,path)=fixture().await;let shell:String=sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&s.db).await.unwrap();let bytes=storage::read_document(&shell).await.unwrap();
        for (user,r) in [("editor-1",birth::Request::default()),("viewer-1",fork_request(&s).await),("editor-1",birth::Request{source_digest:Some(json!("0".repeat(64))),..fork_request(&s).await}),("editor-1",birth::Request{source_draft_id:Some("missing-private".into()),..fork_request(&s).await}),("editor-1",birth::Request{operation_id:None,..fork_request(&s).await})] {
            let original_auth=auth(&s,user).await;let before=snapshot(&s,false).await;assert!(create(&s,&original_auth,r).await.is_err());assert_eq!(snapshot(&s,false).await,before);assert_eq!(storage::read_document(&shell).await.unwrap(),bytes);
        }
        let a=auth(&s,"editor-1").await;sqlx::query("DELETE FROM document_draft_access WHERE user_id='editor-1'").execute(&s.db).await.unwrap();let before=snapshot(&s,false).await;assert!(create(&s,&a,fork_request(&s).await).await.is_err());assert_eq!(snapshot(&s,false).await,before);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn unknown_ack_exact_retry_and_later_save_delete_recreated_result_never_rebirth() {
        let(s,slug,path)=fixture().await;let r=fork_request(&s).await;let a=auth(&s,"editor-1").await;
        let result=birth::UNKNOWN_ACK.scope(true,birth::create(&s,&a,"space-1","doc-1",r.clone())).await;assert!(matches!(result,Err(AppError::Conflict(_))));
        let id:String=sqlx::query_scalar("SELECT draft_id FROM draft_birth_receipts WHERE operation_id=$1").bind(r.operation_id.as_ref().unwrap()).fetch_one(&s.db).await.unwrap();let before=snapshot(&s,false).await;let retried=create(&s,&a,r.clone()).await.unwrap();assert_eq!(retried.saved.row["id"],id);assert_eq!(snapshot(&s,false).await,before);
        sqlx::query("UPDATE document_drafts SET content='NEWER BODY' WHERE id=$1").bind(&id).execute(&s.db).await.unwrap();let before=snapshot(&s,false).await;assert!(create(&s,&a,r.clone()).await.is_err());assert_eq!(snapshot(&s,false).await,before);
        sqlx::query("DELETE FROM document_drafts WHERE id=$1").bind(&id).execute(&s.db).await.unwrap();let before=snapshot(&s,false).await;assert!(create(&s,&a,r.clone()).await.is_err());assert_eq!(snapshot(&s,false).await,before);
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES($1,'doc-1','space-1','editor-1','Recreated','RECREATED','base')").bind(&id).execute(&s.db).await.unwrap();let before=snapshot(&s,false).await;assert!(create(&s,&a,r.clone()).await.is_err());assert_eq!(snapshot(&s,false).await,before);
        let mut different=r.clone();different.force_new=Some(false);assert!(create(&s,&a,different).await.is_err());cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn late_insert_trigger_result_source_policy_audit_and_receipt_corruption_rolls_back() {
        for statement in ["UPDATE document_drafts SET content='trigger-result' WHERE id=NEW.id;","UPDATE document_drafts SET content='trigger-source' WHERE id='draft-1';","INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','editor-1','owner-1');"] {
            let(s,slug,path)=fixture().await;let r=fork_request(&s).await;let a=auth(&s,"editor-1").await;sqlx::query(&format!("CREATE TRIGGER birth_corrupt AFTER INSERT ON document_drafts BEGIN {statement} END")).execute(&s.db).await.unwrap();let before=snapshot(&s,false).await;assert!(create(&s,&a,r).await.is_err());assert_eq!(snapshot(&s,false).await,before);cleanup(s,&slug,&path).await;
        }
        for (table,statement) in [("audit_log","UPDATE audit_log SET before_value='bad' WHERE id=NEW.id;"),("draft_birth_receipts","UPDATE draft_birth_receipts SET result_digest='bad' WHERE operation_id=NEW.operation_id;")] {
            let(s,slug,path)=fixture().await;let r=fork_request(&s).await;let a=auth(&s,"editor-1").await;sqlx::query(&format!("CREATE TRIGGER birth_corrupt AFTER INSERT ON {table} BEGIN {statement} END")).execute(&s.db).await.unwrap();let before=snapshot(&s,false).await;assert!(create(&s,&a,r).await.is_err());assert_eq!(snapshot(&s,false).await,before);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn real_wal_policy_and_selected_source_drift_before_guard_wait_deny_live_actor() {
        for statement in ["INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','editor-1','owner-1')","UPDATE document_draft_access SET role='viewer' WHERE user_id='editor-1'","UPDATE document_drafts SET content='WAL NEW SOURCE' WHERE id='draft-1'","UPDATE documents SET archived_at='2026-01-01' WHERE id='doc-1'"] {
            let(s,slug,path)=fixture().await;let r=fork_request(&s).await;let original_auth=auth(&s,"editor-1").await;let observer=s.clone();let current=AuthUser{claims:original_auth.claims.clone(),api_key_scopes:original_auth.api_key_scopes.clone()};
            let result=birth::BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move {sqlx::query(statement).execute(&observer.db).await.unwrap();let mut c=observer.db.acquire().await.unwrap();access::require_current_human_preference_write(&mut c,&current).await.unwrap();}))),birth::create(&s,&original_auth,"space-1","doc-1",r)).await;
            assert!(result.is_err());assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM draft_birth_receipts").fetch_one(&s.db).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM document_drafts").fetch_one(&s.db).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log").fetch_one(&s.db).await.unwrap(),0);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn published_default_birth_and_custom_write_key_do_not_require_view() {
        let(s,slug,path)=fixture().await;sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('birth-edit','space-1','Edit only','[\"edit_content\"]','owner-1')").execute(&s.db).await.unwrap();sqlx::query("UPDATE space_members SET role='birth-edit' WHERE user_id='editor-1'").execute(&s.db).await.unwrap();
        for (scopes,service) in [("[\"write\"]",0),("[\"read\"]",0),("[\"admin\"]",1)] {
            let key=Uuid::new_v4().to_string();sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,'editor-1','Birth key',$2,'test',$3,$4)").bind(&key).bind(crate::auth::api_keys::hash_api_key(&key)).bind(scopes).bind(service).execute(&s.db).await.unwrap();
            let(mut parts,_)=Request::builder().method(if scopes=="[\"read\"]"{"GET"}else{"POST"}).header("X-API-Key",&key).body(Body::empty()).unwrap().into_parts();let a=AuthUser::from_request_parts(&mut parts,&s).await.unwrap();let before=snapshot(&s,false).await;
            let result=birth::create(&s,&a,"space-1","doc-1",birth::Request{operation_id:Some(Uuid::new_v4().to_string()),..Default::default()}).await;
            if scopes=="[\"write\"]" {let out=result.unwrap();assert!(out.created);assert!(out.saved.row["content"].as_str().unwrap().contains("PUBLIC OLD UNIQUE"));}else{assert!(result.is_err());assert_eq!(snapshot(&s,false).await,before);}
        }
        cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_wal_guard_wait_rechecks_block_and_preserves_only_external_change() {
        let(s,slug,path)=fixture().await;let r=fork_request(&s).await;let a=auth(&s,"editor-1").await;
        let mut held=s.db.begin().await.unwrap();super::super::mutation_guards::lock_audience_for_mutation(&mut held).await.unwrap();
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','editor-1','owner-1')").execute(&mut *held).await.unwrap();
        let (entered,wait)=tokio::sync::oneshot::channel();let owned=s.clone();
        let task=tokio::spawn(async move {birth::BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move {entered.send(()).unwrap();}))),birth::create(&owned,&a,"space-1","doc-1",r)).await});
        wait.await.unwrap();tokio::time::sleep(std::time::Duration::from_millis(50)).await;assert!(!task.is_finished(),"Actual competing SQL guard must hold the request");held.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());let current=auth(&s,"editor-1").await;let before=snapshot(&s,false).await;let mut c=s.db.acquire().await.unwrap();access::require_current_human_preference_write(&mut c,&current).await.unwrap();drop(c);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM draft_birth_receipts").fetch_one(&s.db).await.unwrap(),0);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM document_drafts").fetch_one(&s.db).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log").fetch_one(&s.db).await.unwrap(),0);assert_eq!(snapshot(&s,false).await,before);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn concurrent_default_requests_serialize_one_birth_and_one_resume() {
        let(s,slug,path)=fixture().await;sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();
        let r=birth::Request{operation_id:Some(Uuid::new_v4().to_string()),..Default::default()};let second=birth::Request{operation_id:Some(Uuid::new_v4().to_string()),..Default::default()};let a=auth(&s,"editor-1").await;let b=AuthUser{claims:a.claims.clone(),api_key_scopes:a.api_key_scopes.clone()};let s1=s.clone();let s2=s.clone();
        let(left,right)=tokio::join!(birth::create(&s1,&a,"space-1","doc-1",r),birth::create(&s2,&b,"space-1","doc-1",second));let left=left.unwrap();let right=right.unwrap();assert_ne!(left.created,right.created);assert_eq!(left.saved.row,right.saved.row);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM draft_birth_receipts").fetch_one(&s.db).await.unwrap(),1);assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log WHERE action='document.draft_create'").fetch_one(&s.db).await.unwrap(),1);cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn current_lock_restriction_inactive_account_and_lifecycle_deny_before_birth() {
        for statement in ["UPDATE documents SET locked=1,locked_by='owner-1' WHERE id='doc-1'","UPDATE documents SET archived_at='2026-01-01' WHERE id='doc-1'","UPDATE documents SET deleted_at='2026-01-01' WHERE id='doc-1'","UPDATE users SET is_active=0 WHERE id='editor-1'","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('doc-1','space-1','restricted','r','owner-1')"] {
            let(s,slug,path)=fixture().await;let a=auth(&s,"editor-1").await;let r=fork_request(&s).await;sqlx::query(statement).execute(&s.db).await.unwrap();let before=snapshot(&s,false).await;assert!(birth::create(&s,&a,"space-1","doc-1",r).await.is_err());assert_eq!(snapshot(&s,false).await,before);cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn minted_jwt_expiry_after_final_sql_rolls_back_every_birth_row() {
        let(s,slug,path)=fixture().await;let mut a=auth(&s,"editor-1").await;a.claims.exp=chrono::Utc::now().timestamp()+2;let expires=a.claims.exp;let r=fork_request(&s).await;let before=snapshot(&s,false).await;
        let result=birth::BEFORE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {while chrono::Utc::now().timestamp()<=expires {tokio::time::sleep(std::time::Duration::from_millis(50)).await;}}))),birth::create(&s,&a,"space-1","doc-1",r)).await;
        assert!(result.is_err());assert_eq!(snapshot(&s,false).await,before);cleanup(s,&slug,&path).await;
    }
}

#[cfg(test)]
mod draft_transfer_tests {
    use super::*;
    use super::super::{draft_save, draft_transfer as transfer};
    use super::draft_reader_current_tests::{auth, cleanup, fixture, snapshot};

    async fn request(state: &AppState) -> transfer::Request {
        let source = draft_save::selected(
            &mut state.db.acquire().await.unwrap(), "space-1", "doc-1", "draft-1",
        ).await.unwrap();
        let reviewed = transfer::ReviewedDraft {
            title: "Reviewed local title".into(), icon: Some("🛰️".into()),
            cover_image: None, content: "Exact reviewed local body\r\nopaque asset-local".into(),
        };
        let snapshot_digest = draft_save::digest(&json!({
            "title": reviewed.title, "icon": reviewed.icon,
            "cover_image": reviewed.cover_image, "content": reviewed.content,
        })).unwrap();
        transfer::Request {
            schema_version: 1,
            operation_id: Uuid::new_v4().to_string(),
            local_source: transfer::LocalSource {
                provider: "native-local".into(), root_identity: "root-receipt-a".into(),
                space_id: "local-space".into(), document_id: "local-page".into(),
                draft_id: "local-draft".into(), draft_created_at: "2026-10-19T00:00:00Z".into(),
                snapshot_digest,
            },
            server_source: transfer::ServerSource::Draft {
                draft_id: "draft-1".into(),
                draft_created_at: draft_save::string(&source, "created_at").unwrap(),
                source_digest: draft_save::digest(&source).unwrap(),
            },
            reviewed_draft: reviewed,
            selected_operation_fingerprints: vec!["a".repeat(64), "b".repeat(64)],
        }
    }

    #[tokio::test]
    async fn reviewed_private_transfer_creates_one_owned_target_and_exact_retry_receipt() {
        let (state, slug, path) = fixture().await;
        let actor = auth(&state, "editor-1").await;
        let request = request(&state).await;
        let receipt = transfer::create(&state, &actor, "space-1", "doc-1", request.clone()).await.unwrap();
        let row = draft_save::selected(&mut state.db.acquire().await.unwrap(), "space-1", "doc-1", &receipt.target_draft_id).await.unwrap();
        assert_eq!(row["owner_id"], "editor-1");
        assert_eq!(row["content"], "Exact reviewed local body\r\nopaque asset-local");
        assert_eq!(receipt.target_source_digest, draft_save::digest(&row).unwrap());
        assert_eq!(receipt.local_source, request.local_source);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM document_draft_access WHERE draft_id=$1").bind(&receipt.target_draft_id).fetch_one(&state.db).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_, i64>("SELECT count(*) FROM draft_birth_receipts").fetch_one(&state.db).await.unwrap(), 0);
        let before = snapshot(&state, false).await;
        let retry = transfer::create(&state, &actor, "space-1", "doc-1", request.clone()).await.unwrap();
        assert_eq!(retry, receipt);
        assert_eq!(snapshot(&state, false).await, before);
        let mut changed = request;
        changed.reviewed_draft.content.push_str(" changed");
        changed.local_source.snapshot_digest = draft_save::digest(&json!({
            "title": changed.reviewed_draft.title, "icon": changed.reviewed_draft.icon,
            "cover_image": changed.reviewed_draft.cover_image, "content": changed.reviewed_draft.content,
        })).unwrap();
        assert!(matches!(transfer::create(&state, &actor, "space-1", "doc-1", changed).await, Err(AppError::Conflict(_))));
        assert_eq!(snapshot(&state, false).await, before);
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn transfer_rejects_unreviewed_digest_stale_server_source_and_page_viewer() {
        for case in ["local", "server", "viewer"] {
            let (state, slug, path) = fixture().await;
            let mut request = request(&state).await;
            let actor = auth(&state, if case == "viewer" { "viewer-1" } else { "editor-1" }).await;
            if case == "local" { request.local_source.snapshot_digest = "0".repeat(64); }
            if case == "server" {
                request.server_source = transfer::ServerSource::Draft {
                    draft_id: "draft-1".into(), draft_created_at: "stale".into(), source_digest: "0".repeat(64),
                };
            }
            let before = snapshot(&state, false).await;
            assert!(transfer::create(&state, &actor, "space-1", "doc-1", request).await.is_err());
            assert_eq!(snapshot(&state, false).await, before);
            cleanup(state, &slug, &path).await;
        }
    }
}

#[cfg(test)]
mod draft_owner_tests {
    use super::*;
    use super::super::draft_owner as core;
    use super::draft_reader_current_tests::{fixture, snapshot, cleanup, auth};
    use axum::{body::{Body, to_bytes}, extract::FromRequestParts, http::Request};
    use tower::ServiceExt;

    const DRAFT: &str = "/spaces/space-1/documents/doc-1/drafts/draft-1";

    async fn http(
        state: &AppState, original: &AuthUser, method: &str, path: &str, body: Value,
    ) -> (StatusCode, Value) {
        // Only restore actual extractor bookkeeping to its captured prior values;
        // every table and security/source column remains in full DB assertions.
        let bookkeeping: Vec<(String, Option<String>, String)> = sqlx::query_as(
            "SELECT id,last_seen_at,expires_at FROM sessions WHERE user_id=$1",
        ).bind(&original.claims.sub).fetch_all(&state.db).await.unwrap();
        let token = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &original.claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let response = crate::api_routes().with_state(state.clone()).oneshot(
            Request::builder().method(method).uri(path)
                .header("Authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(if body.is_null() { Body::empty() } else { Body::from(body.to_string()) }).unwrap(),
        ).await.unwrap();
        let status = response.status();
        let raw = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        for (id, last, expires) in bookkeeping {
            sqlx::query("UPDATE sessions SET last_seen_at=$1,expires_at=$2 WHERE id=$3")
                .bind(last).bind(expires).bind(id).execute(&state.db).await.unwrap();
        }
        (status, serde_json::from_slice(&raw).unwrap_or(Value::Null))
    }
    async fn mutate(state: &AppState, original: &AuthUser, change: core::Change<'_>) -> Result<(), AppError> {
        core::mutate(state, original, "space-1", "doc-1", "draft-1", change).await
    }
    async fn share(state: &AppState, user: &str) -> Vec<Value> {
        super::super::document_deletion::rows(&mut state.db.acquire().await.unwrap(),
            "SELECT * FROM document_draft_access WHERE draft_id=$1 AND user_id=$2", &["draft-1", user]).await.unwrap()
    }
    async fn view_only(state: &AppState) {
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('owner-view','space-1','View only','[\"view\"]','owner-1')")
            .execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='owner-view' WHERE user_id='owner-1'")
            .execute(&state.db).await.unwrap();
    }

    #[tokio::test]
    async fn actual_http_owner_view_only_locked_management_and_ordered_read_only_catalogs() {
        let (state, slug, path) = fixture().await;
        view_only(&state).await;
        sqlx::query("UPDATE documents SET locked=1,locked_by='editor-1' WHERE id='doc-1'")
            .execute(&state.db).await.unwrap();
        let owner = auth(&state, "owner-1").await;
        let before = snapshot(&state, false).await;
        let (status, candidates) = http(&state, &owner, "GET", &format!("{DRAFT}/access-candidates"), Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert!(candidates.as_array().unwrap().iter().any(|row| row["user_id"] == "viewer-1"));
        assert_eq!(snapshot(&state, false).await, before);
        let (status, access) = http(&state, &owner, "GET", &format!("{DRAFT}/access"), Value::Null).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(access, json!([{"user_id":"editor-1","display_name":"editor-1","email":"editor-1@example.test","role":"editor"}]));
        assert_eq!(snapshot(&state, false).await, before);
        let (status, _) = http(&state, &owner, "PUT", &format!("{DRAFT}/access/viewer-1"), json!({"role":"viewer"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let first = share(&state, "viewer-1").await;
        let (status, _) = http(&state, &owner, "PUT", &format!("{DRAFT}/access/viewer-1"), json!({"role":"editor"})).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let second = share(&state, "viewer-1").await;
        assert_eq!(first[0]["created_at"], second[0]["created_at"]);
        assert_eq!(second[0]["role"], "editor");
        let (status, _) = http(&state, &owner, "DELETE", &format!("{DRAFT}/access/viewer-1"), Value::Null).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        let before = snapshot(&state, false).await;
        let (status, _) = http(&state, &owner, "DELETE", &format!("{DRAFT}/access/viewer-1"), Value::Null).await;
        assert_eq!(status, StatusCode::NO_CONTENT);
        assert_eq!(snapshot(&state, false).await, before);
        for body in [json!({"role":"owner"}), json!({"role":"bad"})] {
            let before = snapshot(&state, false).await;
            assert_eq!(http(&state, &owner, "PUT", &format!("{DRAFT}/access/viewer-1"), body).await.0, StatusCode::BAD_REQUEST);
            assert_eq!(snapshot(&state, false).await, before);
        }
        let before = snapshot(&state, false).await;
        assert_eq!(http(&state, &owner, "PUT", &format!("{DRAFT}/access/owner-1"), json!({"role":"viewer"})).await.0, StatusCode::BAD_REQUEST);
        assert_eq!(snapshot(&state, false).await, before);
        assert!(!storage::space_dir(&slug).join("drafts").exists());
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn owner_delete_last_private_draft_retains_mirrors_assets_and_immutable_receipts() {
        let (state, slug, path) = fixture().await;
        let owner = auth(&state, "owner-1").await;
        let row = super::super::draft_save::selected(&mut state.db.acquire().await.unwrap(), "space-1", "doc-1", "draft-1").await.unwrap();
        let operation = Uuid::new_v4().to_string();
        let born = super::super::draft_birth::create(&state, &owner, "space-1", "doc-1", super::super::draft_birth::Request {
            force_new: Some(true), operation_id: Some(operation), source_draft_id: Some("draft-1".into()),
            source_digest: Some(json!(super::super::draft_save::digest(&row).unwrap())),
        }).await.unwrap();
        let born_id = born.saved.row["id"].as_str().unwrap();
        core::mutate(&state, &owner, "space-1", "doc-1", born_id, core::Change::Delete).await.unwrap();
        let receipts = super::super::document_deletion::rows(&mut state.db.acquire().await.unwrap(), "SELECT * FROM draft_birth_receipts", &[]).await.unwrap();
        let mirror = storage::space_dir(&slug).join("drafts").join("retained.md");
        std::fs::create_dir_all(mirror.parent().unwrap()).unwrap();
        std::fs::write(&mirror, b"RETAIN PRIVATE MIRROR").unwrap();
        let asset = storage::space_dir(&slug).join("attachments").join("retained.bin");
        std::fs::create_dir_all(asset.parent().unwrap()).unwrap();
        std::fs::write(&asset, b"RETAIN ASSET").unwrap();
        let canonical: String = sqlx::query_scalar("SELECT content_path FROM documents WHERE id='doc-1'").fetch_one(&state.db).await.unwrap();
        let old = storage::read_document(&canonical).await.unwrap();
        assert_eq!(http(&state, &owner, "DELETE", DRAFT, Value::Null).await.0, StatusCode::NO_CONTENT);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM document_drafts WHERE document_id='doc-1'").fetch_one(&state.db).await.unwrap(), 0);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM document_draft_access").fetch_one(&state.db).await.unwrap(), 0);
        assert_eq!(super::super::document_deletion::rows(&mut state.db.acquire().await.unwrap(), "SELECT * FROM draft_birth_receipts", &[]).await.unwrap(), receipts);
        assert_eq!(std::fs::read(mirror).unwrap(), b"RETAIN PRIVATE MIRROR");
        assert_eq!(std::fs::read(asset).unwrap(), b"RETAIN ASSET");
        assert_eq!(storage::read_document(&canonical).await.unwrap(), old);
        assert!(access::resolve_page_access(&mut state.db.acquire().await.unwrap(), access::principal_from_auth(&owner).unwrap(), "space-1", "doc-1", PageOperation::View, Purpose::Routine).await.is_err());
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn write_only_view_key_management_read_key_catalog_and_service_denial() {
        let (state, slug, path) = fixture().await;
        view_only(&state).await;
        for (scopes, service) in [("[\"write\"]",0), ("[\"read\"]",0), ("[\"admin\"]",1)] {
            let raw = Uuid::new_v4().to_string();
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,'owner-1','Owner key',$2,'test',$3,$4)")
                .bind(&raw).bind(crate::auth::api_keys::hash_api_key(&raw)).bind(scopes).bind(service).execute(&state.db).await.unwrap();
            let (mut parts, _) = Request::builder().method(if scopes == "[\"read\"]" { "GET" } else { "PUT" })
                .header("X-API-Key", &raw).body(Body::empty()).unwrap().into_parts();
            let original = AuthUser::from_request_parts(&mut parts, &state).await.unwrap();
            let before = snapshot(&state, false).await;
            let result = mutate(&state, &original, core::Change::Grant { recipient: "viewer-1", role: "viewer" }).await;
            if scopes == "[\"write\"]" {
                assert!(result.is_ok());
                assert!(core::inspect(&state, &original, "space-1", "doc-1", "draft-1", false).await.is_err());
            } else {
                assert!(result.is_err());
                assert_eq!(snapshot(&state, false).await, before);
                if scopes == "[\"read\"]" { assert!(core::inspect(&state, &original, "space-1", "doc-1", "draft-1", false).await.is_ok()); }
            }
        }
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn lifecycle_owner_view_and_nonowner_no_recovery_ownership_or_foreign_delete() {
        for state_sql in ["UPDATE documents SET archived_at='2026-01-01' WHERE id='doc-1'", "UPDATE documents SET deleted_at='2026-01-01' WHERE id='doc-1'"] {
            let (state, slug, path) = fixture().await;
            let owner = auth(&state, "owner-1").await;
            let editor = auth(&state, "editor-1").await;
            sqlx::query(state_sql).execute(&state.db).await.unwrap();
            let before = snapshot(&state, false).await;
            assert!(mutate(&state, &editor, core::Change::Delete).await.is_err());
            assert!(core::inspect(&state, &editor, "space-1", "doc-1", "draft-1", false).await.is_err());
            assert!(core::mutate(&state, &owner, "foreign", "doc-1", "draft-1", core::Change::Delete).await.is_err());
            assert!(core::mutate(&state, &owner, "space-1", "foreign", "draft-1", core::Change::Delete).await.is_err());
            assert_eq!(snapshot(&state, false).await, before);
            assert!(core::inspect(&state, &owner, "space-1", "doc-1", "draft-1", false).await.is_ok());
            mutate(&state, &owner, core::Change::Delete).await.unwrap();
            cleanup(state, &slug, &path).await;
        }
        let (state, slug, path) = fixture().await;
        sqlx::query("UPDATE users SET role='admin' WHERE id='editor-1'").execute(&state.db).await.unwrap();
        let admin = auth(&state, "editor-1").await;
        let before = snapshot(&state, false).await;
        assert!(mutate(&state, &admin, core::Change::Grant { recipient:"viewer-1", role:"viewer" }).await.is_err());
        assert!(mutate(&state, &admin, core::Change::Delete).await.is_err());
        assert_eq!(snapshot(&state, false).await, before);
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn prospective_share_requires_current_target_view_blocks_and_private_ancestors() {
        let (state, slug, path) = fixture().await;
        let owner = auth(&state, "owner-1").await;
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES('hidden','hidden@example.test','HIDDEN PERSON','member')").execute(&state.db).await.unwrap();
        let projected = core::inspect(&state, &owner, "space-1", "doc-1", "draft-1", true).await.unwrap();
        assert!(!serde_json::to_string(&projected).unwrap().contains("HIDDEN"));
        let before = snapshot(&state, false).await;
        assert!(mutate(&state, &owner, core::Change::Grant { recipient:"hidden", role:"viewer" }).await.is_err());
        assert_eq!(snapshot(&state, false).await, before);
        // A new selected draft share can establish participation on this private
        // target, but cannot open a different private ancestor.
        assert!(share(&state, "viewer-1").await.is_empty());
        for statement in [
            "UPDATE users SET is_active=0 WHERE id='viewer-1'",
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','viewer-1','owner-1')",
        ] {
            sqlx::query(statement).execute(&state.db).await.unwrap();
            let before = snapshot(&state, false).await;
            assert!(mutate(&state, &owner, core::Change::Grant { recipient:"viewer-1", role:"viewer" }).await.is_err());
            assert_eq!(snapshot(&state, false).await, before);
            let candidates = core::inspect(&state, &owner, "space-1", "doc-1", "draft-1", true).await.unwrap();
            assert!(!candidates.iter().any(|row| row["user_id"] == "viewer-1"));
            sqlx::query("UPDATE users SET is_active=1 WHERE id='viewer-1'").execute(&state.db).await.unwrap();
            sqlx::query("DELETE FROM space_user_blocks WHERE space_id='space-1' AND user_id='viewer-1'").execute(&state.db).await.unwrap();
        }
        mutate(&state, &owner, core::Change::Grant { recipient:"viewer-1", role:"viewer" }).await.unwrap();
        mutate(&state, &owner, core::Change::Revoke { recipient:"viewer-1" }).await.unwrap();
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) SELECT 'private-parent',space_id,'Parent',content_path,created_by,last_updated_by FROM documents WHERE id='doc-1'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('parent-draft','private-parent','space-1','owner-1','Parent','PRIVATE PARENT','base')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET parent_id='private-parent' WHERE id='doc-1'").execute(&state.db).await.unwrap();
        let before = snapshot(&state, false).await;
        assert!(mutate(&state, &owner, core::Change::Grant { recipient:"viewer-1", role:"viewer" }).await.is_err());
        assert_eq!(snapshot(&state, false).await, before);
        let candidates = core::inspect(&state, &owner, "space-1", "doc-1", "draft-1", true).await.unwrap();
        assert!(!candidates.iter().any(|row| row["user_id"] == "viewer-1"));
        sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('doc-1','space-1','restricted','r','owner-1')").execute(&state.db).await.unwrap();
        let before = snapshot(&state, false).await;
        assert!(mutate(&state, &owner, core::Change::Delete).await.is_err());
        assert_eq!(snapshot(&state, false).await, before);
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn catalogs_late_wal_recipient_owner_and_source_loss_do_not_return_stale_labels() {
        for statement in [
            "UPDATE users SET is_active=0 WHERE id='viewer-1'",
            "UPDATE users SET display_name='CHANGED LABEL' WHERE id='viewer-1'",
            "INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','owner-1','owner-1')",
            "DELETE FROM document_draft_access WHERE user_id='editor-1'",
        ] {
            let (state, slug, path) = fixture().await;
            let owner = auth(&state, "owner-1").await;
            let observer = state.clone();
            let result = core::AFTER_READ_PROJECTION.scope(std::cell::RefCell::new(Some(Box::pin(async move {
                sqlx::query(statement).execute(&observer.db).await.unwrap();
            }))), core::inspect(&state, &owner, "space-1", "doc-1", "draft-1", true)).await;
            assert!(result.is_err());
            let before = snapshot(&state, false).await;
            access::require_current_human_read(&mut state.db.acquire().await.unwrap(), &owner).await.unwrap();
            assert_eq!(snapshot(&state, false).await, before);
            cleanup(state, &slug, &path).await;
        }
        // Genuine late recipient changes leave the owner and original Read
        // credential valid. No stale person label or share role may escape.
        for (statement, candidates) in [
            ("UPDATE users SET is_active=0 WHERE id='viewer-1'", true),
            ("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','viewer-1','owner-1')", true),
            ("UPDATE users SET display_name='LATE LABEL' WHERE id='viewer-1'", true),
            ("UPDATE document_draft_access SET role='viewer' WHERE user_id='editor-1'", false),
            ("UPDATE users SET display_name='LATE LABEL' WHERE id='editor-1'", false),
        ] {
            let (state, slug, path) = fixture().await;
            let owner = auth(&state, "owner-1").await;
            let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mark = reached.clone();
            let observer = state.clone();
            let result = core::AFTER_READ_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
                sqlx::query(statement).execute(&observer.db).await.unwrap();
                mark.store(true, std::sync::atomic::Ordering::SeqCst);
            }))), core::inspect(&state, &owner, "space-1", "doc-1", "draft-1", candidates)).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
            assert!(result.is_err(), "{statement}");
            let before = snapshot(&state, false).await;
            let mut conn = state.db.acquire().await.unwrap();
            access::require_current_human_read(&mut conn, &owner).await.unwrap();
            access::resolve_owned_draft_access(&mut conn, &owner, "space-1", "doc-1", "draft-1", access::DraftOwnerCredential::Read).await.unwrap();
            drop(conn);
            assert_eq!(snapshot(&state, false).await, before);
            cleanup(state, &slug, &path).await;
        }
        let (state, slug, path) = fixture().await;
        let owner = auth(&state, "owner-1").await;
        let observer = state.clone();
        let result = core::AFTER_READ_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','owner-1','owner-1')").execute(&observer.db).await.unwrap();
        }))), core::inspect(&state, &owner, "space-1", "doc-1", "draft-1", false)).await;
        assert!(result.is_err());
        access::require_current_human_read(&mut state.db.acquire().await.unwrap(), &owner).await.unwrap();
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn late_trigger_delete_grant_revoke_source_audit_and_unexpected_share_roll_back() {
        for (table, event, statement, kind) in [
            ("document_drafts", "DELETE", "UPDATE documents SET title='CORRUPTED' WHERE id='doc-1';", "delete"),
            ("document_drafts", "DELETE", "DELETE FROM draft_birth_receipts WHERE document_id='doc-1';", "receipt"),
            ("document_draft_access", "INSERT", "UPDATE users SET is_active=0 WHERE id='viewer-1';", "grant"),
            ("document_draft_access", "DELETE", "UPDATE document_drafts SET content='CORRUPTED' WHERE id='draft-1';", "revoke"),
            ("audit_log", "INSERT", "UPDATE audit_log SET after_value='CORRUPTED' WHERE id=NEW.id;", "grant"),
            ("audit_log", "INSERT", "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('draft-1','viewer-1','viewer','owner-1');", "revoke"),
        ] {
            let (state, slug, path) = fixture().await;
            let owner = auth(&state, "owner-1").await;
            if kind == "receipt" {
                let row = super::super::draft_save::selected(&mut state.db.acquire().await.unwrap(), "space-1", "doc-1", "draft-1").await.unwrap();
                super::super::draft_birth::create(&state, &owner, "space-1", "doc-1", super::super::draft_birth::Request {
                    force_new: Some(true), operation_id: Some(Uuid::new_v4().to_string()),
                    source_draft_id: Some("draft-1".into()), source_digest: Some(json!(super::super::draft_save::digest(&row).unwrap())),
                }).await.unwrap();
            }
            sqlx::query(&format!("CREATE TRIGGER owner_corrupt AFTER {event} ON {table} BEGIN {statement} END")).execute(&state.db).await.unwrap();
            let before = snapshot(&state, false).await;
            let change = match kind {
                "delete" | "receipt" => core::Change::Delete,
                "grant" => core::Change::Grant { recipient:"viewer-1", role:"viewer" },
                _ => core::Change::Revoke { recipient:"editor-1" },
            };
            assert!(mutate(&state, &owner, change).await.is_err());
            assert_eq!(snapshot(&state, false).await, before);
            cleanup(state, &slug, &path).await;
        }
    }

    #[tokio::test]
    async fn actual_guard_wait_rechecks_owner_policy_and_only_external_change_survives() {
        let (state, slug, path) = fixture().await;
        let owner = auth(&state, "owner-1").await;
        let current = AuthUser { claims: owner.claims.clone(), api_key_scopes: owner.api_key_scopes.clone() };
        let mut held = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_audience_for_mutation(&mut held).await.unwrap();
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','owner-1','owner-1')").execute(&mut *held).await.unwrap();
        let (entered, wait) = tokio::sync::oneshot::channel();
        let owned = state.clone();
        let task = tokio::spawn(async move {
            core::BEFORE_LOCK.scope(std::cell::RefCell::new(Some(Box::pin(async move { entered.send(()).unwrap(); }))),
                core::mutate(&owned, &owner, "space-1", "doc-1", "draft-1", core::Change::Delete)).await
        });
        wait.await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(!task.is_finished());
        held.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        let before = snapshot(&state, false).await;
        access::require_current_human_preference_write(&mut state.db.acquire().await.unwrap(), &current).await.unwrap();
        assert_eq!(snapshot(&state, false).await, before);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM document_drafts").fetch_one(&state.db).await.unwrap(), 1);
        assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM audit_log").fetch_one(&state.db).await.unwrap(), 0);
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn same_session_real_http_renewal_before_inventory_keeps_catalog_available() {
        let (state, slug, path) = fixture().await;
        let owner = auth(&state, "owner-1").await;
        let token = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &owner.claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let before = snapshot(&state, false).await;
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mark = reached.clone();
        let observer = state.clone();
        let session = owner.claims.sid.clone();
        let (status, body) = DRAFT_READER_BEFORE_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            let previous: String = sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id=$1")
                .bind(&session).fetch_one(&observer.db).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let response = crate::api_routes().with_state(observer.clone()).oneshot(
                Request::builder().method("GET").uri("/auth/me")
                    .header("Authorization", format!("Bearer {token}")).body(Body::empty()).unwrap(),
            ).await.unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            let renewed: String = sqlx::query_scalar("SELECT expires_at FROM sessions WHERE id=$1")
                .bind(&session).fetch_one(&observer.db).await.unwrap();
            assert_ne!(renewed, previous, "real extractor must change rolling expiry before inventory equality");
            mark.store(true, std::sync::atomic::Ordering::SeqCst);
        }))), http(&state, &owner, "GET", "/spaces/space-1/drafts", Value::Null)).await;
        assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(status, StatusCode::OK);
        assert!(body.as_array().unwrap().iter().any(|row| row["id"] == "draft-1"));
        // HTTP helper restores only its exact captured extractor bookkeeping.
        assert_eq!(snapshot(&state, false).await, before);
        cleanup(state, &slug, &path).await;
    }

    #[tokio::test]
    async fn late_selected_session_expiry_corruption_deletion_and_reassignment_still_deny() {
        for statement in [
            "UPDATE sessions SET expires_at='2000-01-01T00:00:00Z' WHERE id='publication-owner-1'",
            "UPDATE sessions SET expires_at='malformed' WHERE id='publication-owner-1'",
            "DELETE FROM sessions WHERE id='publication-owner-1'",
            "UPDATE sessions SET user_id='viewer-1' WHERE id='publication-owner-1'",
        ] {
            let (state, slug, path) = fixture().await;
            let owner = auth(&state, "owner-1").await;
            let observer = state.clone();
            let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let mark = reached.clone();
            let expected = std::sync::Arc::new(std::sync::Mutex::new(None));
            let captured = expected.clone();
            let result = DRAFT_READER_AFTER_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
                sqlx::query(statement).execute(&observer.db).await.unwrap();
                let after_external_change = snapshot(&observer, false).await;
                *captured.lock().unwrap() = Some(after_external_change);
                mark.store(true, std::sync::atomic::Ordering::SeqCst);
            }))), list_drafts(State(state.clone()), owner, Path("space-1".into()), Query(ListDraftsQuery::default()))).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst));
            assert!(matches!(result, Err(AppError::Unauthorized(_))), "{statement}");
            assert_eq!(snapshot(&state, false).await, expected.lock().unwrap().take().unwrap());
            cleanup(state, &slug, &path).await;
        }
    }

    async fn short_signed_auth(state: &AppState, original: &AuthUser, method: &str) -> AuthUser {
        let mut claims = original.claims.clone();
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(&jsonwebtoken::Header::default(), &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes())).unwrap();
        let request = Request::builder().method(method).uri(DRAFT)
            .header("Authorization", format!("Bearer {token}")).body(()).unwrap();
        let (mut parts, _) = request.into_parts();
        AuthUser::from_request_parts(&mut parts, state).await.unwrap()
    }

    #[tokio::test]
    async fn final_original_jwt_expiry_rolls_back_write_and_suppresses_read_projection() {
        let (state, slug, path) = fixture().await;
        let original = auth(&state, "owner-1").await;
        let owner = short_signed_auth(&state, &original, "DELETE").await;
        let expiration = owner.claims.exp;
        let before = snapshot(&state, false).await;
        let result = core::BEFORE_WRITE_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            while chrono::Utc::now().timestamp() <= expiration { tokio::time::sleep(std::time::Duration::from_millis(50)).await; }
        }))), mutate(&state, &owner, core::Change::Delete)).await;
        assert!(result.is_err());
        assert_eq!(snapshot(&state, false).await, before);
        let owner = short_signed_auth(&state, &original, "GET").await;
        let expiration = owner.claims.exp;
        let before = snapshot(&state, false).await;
        let result = core::BEFORE_READ_IDENTITY.scope(std::cell::RefCell::new(Some(Box::pin(async move {
            while chrono::Utc::now().timestamp() <= expiration { tokio::time::sleep(std::time::Duration::from_millis(50)).await; }
        }))), core::inspect(&state, &owner, "space-1", "doc-1", "draft-1", false)).await;
        assert!(result.is_err());
        assert_eq!(snapshot(&state, false).await, before);
        cleanup(state, &slug, &path).await;
    }
}

// Test-only route checkpoints. Production has no environment-driven failpoint.
#[cfg(test)]
tokio::task_local! {
    static PUBLICATION_ROUTE_CRASH: Box<dyn Fn(&str,&str)+Send>;
}
#[cfg(test)]
fn publication_route_crash_point(phase:&str,operation_id:&str) {
    let _=PUBLICATION_ROUTE_CRASH.try_with(|hook|hook(phase,operation_id));
}
#[cfg(all(test,target_os="linux"))]
#[path="draft_publication_restart_tests.rs"]
mod publication_route_restart_tests;

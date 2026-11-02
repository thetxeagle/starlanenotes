//! Review-only move inventory. A preview is not relocation authority or a transactional snapshot.
mod commit;
use super::access::{self, MovePreviewCredential};
use crate::{
    auth::{middleware::AuthUser, routes::AppError},
    AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Column, Connection, Row};
use std::collections::{BTreeMap, BTreeSet};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/moves/previews/{preview_id}/commit",
            post(commit::commit_preview),
        )
        .route(
            "/moves/commits/{op_id}/preserved/{document_id}",
            get(commit::read_preserved),
        )
        .route(
            "/moves/commits/{op_id}/preserved/{document_id}/inspect",
            post(commit::inspect_preserved),
        )
        .route(
            "/moves/documents/{document_id}/preserved/inspect",
            post(commit::inspect_preserved_index),
        )
        .route("/spaces/{space_id}/moves/preview", post(create_preview))
        .route(
            "/moves/previews/{preview_id}",
            get(get_preview).delete(cancel_preview),
        )
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MovePreviewRequest {
    pub root: MoveRoot,
    pub destination: MoveDestination,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MoveRoot {
    pub kind: String,
    pub id: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MoveDestination {
    pub space_id: String,
    pub folder_id: Option<String>,
    pub parent_id: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct MovePreviewResponse {
    pub preview_id: String,
    pub expires_at: String,
    pub fingerprint: String,
    pub stale: bool,
    pub inventory: Value,
    pub access_consequences: Value,
    pub blockers: Vec<String>,
    pub commit_available: bool,
    pub commit_unavailable_reason: String,
}
struct Review {
    fingerprint: String,
    inventory: Value,
    access: Value,
    blockers: Vec<String>,
}

async fn create_preview(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(source): Path<String>,
    Json(req): Json<MovePreviewRequest>,
) -> Result<Json<MovePreviewResponse>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let prepared = review_move_current(
        &mut conn,
        &auth,
        &source,
        &req,
        MovePreviewCredential::Write,
    )
    .await?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now();
    let created = now.to_rfc3339();
    let expires = (now + chrono::Duration::minutes(10)).to_rfc3339();
    let request = serde_json::to_string(&req).map_err(AppError::internal)?;
    let summary = serde_json::to_string(
        &json!({"schema_version":1,"fingerprint_algorithm":"sha256","commit_available":true}),
    )
    .map_err(AppError::internal)?;
    let mut tx = conn.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(
        &mut tx,
        &[&source, &req.destination.space_id],
    )
    .await
    .map_err(conceal)?;
    sqlx::query("INSERT INTO move_previews(id,actor_id,source_space_id,root_kind,root_id,request_json,fingerprint,manifest_json,created_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(&id).bind(&auth.claims.sub).bind(&source).bind(&req.root.kind).bind(&req.root.id).bind(&request)
        .bind(&prepared.review.fingerprint).bind(&summary).bind(&created).bind(&expires).execute(&mut *tx).await.map_err(AppError::internal)?;
    let expected = json!({"id":id,"actor_id":auth.claims.sub,"source_space_id":source,"root_kind":req.root.kind,"root_id":req.root.id,"request_json":request,"fingerprint":prepared.review.fingerprint,"manifest_json":summary,"created_at":created,"expires_at":expires,"cancelled_at":null});
    verify_review(
        &mut tx,
        &auth,
        &source,
        &req,
        &prepared,
        MovePreviewCredential::Write,
    )
    .await?;
    if owned_preview(&mut tx, &id, &auth.claims.sub).await? != expected {
        return Err(unavailable());
    }
    #[cfg(test)]
    after_final_record(&mut tx).await;
    require_preview_identity(&mut tx, &auth, MovePreviewCredential::Write).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(Json(response(
        id,
        expires,
        prepared.review.fingerprint.clone(),
        false,
        prepared.review,
    )))
}
async fn get_preview(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<Json<MovePreviewResponse>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_preview_identity(&mut conn, &auth, MovePreviewCredential::Read).await?;
    let record = owned_preview(&mut conn, &id, &auth.claims.sub).await?;
    let source = text(&record, "source_space_id");
    let req: MovePreviewRequest =
        serde_json::from_str(text(&record, "request_json")).map_err(|_| unavailable())?;
    if text(&record, "root_kind") != req.root.kind || text(&record, "root_id") != req.root.id {
        return Err(unavailable());
    }
    let prepared =
        review_move_current(&mut conn, &auth, source, &req, MovePreviewCredential::Read).await?;
    verify_review(
        &mut conn,
        &auth,
        source,
        &req,
        &prepared,
        MovePreviewCredential::Read,
    )
    .await?;
    if owned_preview(&mut conn, &id, &auth.claims.sub).await? != record {
        return Err(unavailable());
    }
    #[cfg(test)]
    after_final_record(&mut conn).await;
    require_preview_identity(&mut conn, &auth, MovePreviewCredential::Read).await?;
    let stale = prepared.review.fingerprint != text(&record, "fingerprint");
    Ok(Json(response(
        id,
        text(&record, "expires_at").into(),
        text(&record, "fingerprint").into(),
        stale,
        prepared.review,
    )))
}
async fn cancel_preview(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_preview_identity(&mut conn, &auth, MovePreviewCredential::Write).await?;
    let mut expected = cancellable_preview(&mut conn, &id, &auth.claims.sub).await?;
    // Own-record cleanup remains possible after losing page authority or after
    // source deletion. Existing referenced spaces are guarded, never granted.
    let destination = serde_json::from_str::<MovePreviewRequest>(text(&expected, "request_json"))
        .ok()
        .map(|r| r.destination.space_id)
        .unwrap_or_default();
    let scope = rows_raw(
        &mut conn,
        "SELECT id FROM spaces WHERE id=$1 OR id=$2",
        &[text(&expected, "source_space_id"), &destination],
    )
    .await?;
    let spaces: Vec<&str> = scope.iter().map(|r| text(r, "id")).collect();
    let mut tx = conn.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &spaces).await?;
    require_preview_identity(&mut tx, &auth, MovePreviewCredential::Write).await?;
    let rows =
        sqlx::query("UPDATE move_previews SET id=id WHERE id=$1 AND actor_id=$2 AND expires_at>$3")
            .bind(&id)
            .bind(&auth.claims.sub)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if rows.rows_affected() != 1
        || cancellable_preview(&mut tx, &id, &auth.claims.sub).await? != expected
    {
        return Err(unavailable());
    }
    let now = chrono::Utc::now().to_rfc3339();
    let updated = sqlx::query(
        "UPDATE move_previews SET cancelled_at=$1 WHERE id=$2 AND actor_id=$3 AND expires_at>$1",
    )
    .bind(&now)
    .bind(&id)
    .bind(&auth.claims.sub)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if updated.rows_affected() != 1 {
        return Err(unavailable());
    }
    expected["cancelled_at"] = json!(now);
    if cancellable_preview(&mut tx, &id, &auth.claims.sub).await? != expected {
        return Err(unavailable());
    }
    #[cfg(test)]
    after_final_record(&mut tx).await;
    require_preview_identity(&mut tx, &auth, MovePreviewCredential::Write).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
fn response(
    id: String,
    expires: String,
    fingerprint: String,
    stale: bool,
    review: Review,
) -> MovePreviewResponse {
    MovePreviewResponse {
        preview_id: id,
        expires_at: expires,
        fingerprint,
        stale,
        inventory: review.inventory,
        access_consequences: review.access,
        blockers: review.blockers,
        commit_available: commit::writers_ready(),
        commit_unavailable_reason: String::new(),
    }
}
async fn owned_preview(conn: &mut AnyConnection, id: &str, actor: &str) -> Result<Value, AppError> {
    rows_raw(conn,"SELECT * FROM move_previews WHERE id=$1 AND actor_id=$2 AND cancelled_at IS NULL AND expires_at>$3",&[id,actor,&chrono::Utc::now().to_rfc3339()]).await?.into_iter().next().ok_or_else(unavailable)
}
async fn cancellable_preview(
    conn: &mut AnyConnection,
    id: &str,
    actor: &str,
) -> Result<Value, AppError> {
    rows_raw(
        conn,
        "SELECT * FROM move_previews WHERE id=$1 AND actor_id=$2 AND expires_at>$3",
        &[id, actor, &chrono::Utc::now().to_rfc3339()],
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(unavailable)
}
fn unavailable() -> AppError {
    AppError::NotFound("Move preview unavailable".into())
}
fn conceal(error: AppError) -> AppError {
    match error {
        AppError::NotFound(_) | AppError::Forbidden(_) => unavailable(),
        other => other,
    }
}
async fn require_preview_identity(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    mode: MovePreviewCredential,
) -> Result<(), AppError> {
    match mode {
        MovePreviewCredential::Read => access::require_current_human_read(conn, auth)
            .await
            .map(|_| ()),
        MovePreviewCredential::Write => access::require_current_human_preference_write(conn, auth)
            .await
            .map(|_| ()),
    }
}
#[derive(Clone)]
struct RawReviewProof {
    query: String,
    parameters: Vec<String>,
    rows: Vec<Value>,
}
struct PreparedReview {
    manifest: Value,
    review: Review,
    proofs: Vec<RawReviewProof>,
    pages: Vec<(Value, bool)>,
    metadata_spaces: BTreeSet<String>,
    public_pages_enabled: bool,
}
struct ReviewContext<'a> {
    conn: &'a mut AnyConnection,
    auth: &'a AuthUser,
    mode: MovePreviewCredential,
    proofs: Vec<RawReviewProof>,
    pages: Vec<(Value, bool)>,
    metadata_spaces: BTreeSet<String>,
    page_authority: BTreeMap<String, Value>,
    metadata_authority: BTreeMap<String, Value>,
}
impl ReviewContext<'_> {
    async fn rows(&mut self, query: &str, parameters: &[&str]) -> Result<Vec<Value>, AppError> {
        let rows = rows_raw(self.conn, query, parameters).await?;
        self.proofs.push(RawReviewProof {
            query: query.into(),
            parameters: parameters.iter().map(|p| p.to_string()).collect(),
            rows: rows.clone(),
        });
        Ok(rows)
    }
}
async fn resolve_preview_page(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    mode: MovePreviewCredential,
    page: &Value,
) -> Result<Option<access::PageAccess>, AppError> {
    let operation = match mode {
        MovePreviewCredential::Read => access::PageOperation::View,
        MovePreviewCredential::Write => access::PageOperation::InspectMovePreviewFromMutation,
    };
    match access::resolve_page_access(
        conn,
        access::principal_from_auth(auth)?,
        text(page, "space_id"),
        text(page, "id"),
        operation,
        access::Purpose::Routine,
    )
    .await
    {
        Ok(current) => Ok(Some(current)),
        Err(AppError::NotFound(_)) | Err(AppError::Forbidden(_)) => Ok(None),
        Err(error) => Err(error),
    }
}
async fn page_visible(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    mode: MovePreviewCredential,
    page: &Value,
) -> Result<bool, AppError> {
    Ok(resolve_preview_page(conn, auth, mode, page)
        .await?
        .is_some())
}
fn preview_page_operation(mode:MovePreviewCredential)->access::PageOperation {
    match mode {MovePreviewCredential::Read=>access::PageOperation::View,MovePreviewCredential::Write=>access::PageOperation::InspectMovePreviewFromMutation}
}
async fn capture_page_authority(ctx:&mut ReviewContext<'_>,id:&str)->Result<(),AppError> {
    if ctx.page_authority.contains_key(id) {return Ok(());}
            let row = ctx
                .rows("SELECT * FROM documents WHERE id=$1", &[&id])
                .await?;
            let drafts = ctx
                .rows("SELECT * FROM document_drafts WHERE document_id=$1", &[&id])
                .await?;
            let shares=ctx.rows("SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1",&[&id]).await?;
            let policy = ctx
                .rows(
                    "SELECT * FROM document_access_policies WHERE document_id=$1",
                    &[&id],
                )
                .await?;
            let allowlist = ctx
                .rows(
                    "SELECT * FROM document_access_allowlist WHERE document_id=$1",
                    &[&id],
                )
                .await?;
            ctx.page_authority
                .insert(id.to_owned(), json!({"page":row,"drafts":drafts,"shares":shares,"restriction_policy":policy,"restriction_allowlist":allowlist}));    Ok(())
}
async fn page_available(ctx: &mut ReviewContext<'_>, page: &Value) -> Result<bool, AppError> {
    let current = resolve_preview_page(ctx.conn, ctx.auth, ctx.mode, page).await?;
    let available = current.is_some();
    ctx.pages.push((page.clone(), available));
    if let Some(current) = current {
        let ids = std::iter::once(current.document_id).chain(current.ancestor_ids);
        for id in ids {
            if ctx.page_authority.contains_key(&id) {
                continue;
            }
            capture_page_authority(ctx,&id).await?;
        }
    }
    Ok(available)
}
async fn require_visible_space(ctx: &mut ReviewContext<'_>, space: &str) -> Result<(), AppError> {
    access::require_move_preview_metadata_space(ctx.conn, ctx.auth, space, ctx.mode).await?;
    ctx.metadata_spaces.insert(space.into());
    if !ctx.metadata_authority.contains_key(space) {
        let mut authority = BTreeMap::new();
        for table in [
            "spaces",
            "space_members",
            "space_group_permissions",
            "space_user_blocks",
            "space_roles",
        ] {
            let key = if table == "spaces" { "id" } else { "space_id" };
            authority.insert(
                table,
                ctx.rows(&format!("SELECT * FROM {table} WHERE {key}=$1"), &[space])
                    .await?,
            );
        }
        ctx.metadata_authority
            .insert(space.into(), json!(authority));
    }
    Ok(())
}
fn public_pages_enabled() -> bool {
    std::env::var("ALLOW_PUBLIC_PAGES")
        .ok()
        .is_none_or(|value| !value.trim().eq_ignore_ascii_case("false"))
}
async fn verify_review(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    source: &str,
    req: &MovePreviewRequest,
    prepared: &PreparedReview,
    mode: MovePreviewCredential,
) -> Result<(), AppError> {
    access::require_move_preview_space(conn, auth, source, mode)
        .await
        .map_err(conceal)?;
    access::require_move_preview_space(conn, auth, &req.destination.space_id, mode)
        .await
        .map_err(conceal)?;
    if public_pages_enabled() != prepared.public_pages_enabled {
        return Err(unavailable());
    }
    for space in &prepared.metadata_spaces {
        access::require_move_preview_metadata_space(conn, auth, space, mode)
            .await
            .map_err(conceal)?;
    }
    // Recheck actual visibility after inventory and filesystem I/O.
    // Exact selected rows are then compared after current credential proof.
    let mut by_space:BTreeMap<&str,BTreeSet<String>>=BTreeMap::new();
    for (page,_) in &prepared.pages {by_space.entry(text(page,"space_id")).or_default().insert(text(page,"id").to_owned());}
    for (space,targets) in by_space {
        let batch=access::resolve_page_batch(conn,access::principal_from_auth(auth)?,space,&targets,preview_page_operation(mode)).await?;
        for (page,available) in &prepared.pages {
            if text(page,"space_id")!=space {continue;}
            let current=match batch.require(text(page,"id")) {Ok(())=>true,Err(AppError::NotFound(_))|Err(AppError::Forbidden(_))=>false,Err(error)=>return Err(error)};
            if current!=*available {return Err(unavailable());}
        }
    }
    require_preview_identity(conn, auth, mode).await?;
    for proof in &prepared.proofs {
        let params: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
        if rows_raw(conn, &proof.query, &params).await? != proof.rows {
            return Err(unavailable());
        }
    }
    #[cfg(test)]
    after_raw_proofs().await;
    for expected in prepared.manifest["maintenance_target_sources"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let current = super::maintenance::target_source(
            conn,
            expected["user"].as_str(),
            expected["group"].as_str(),
        )
        .await?;
        if current != expected["witness"] {
            return Err(unavailable());
        }
    }
    #[cfg(test)]
    after_maintenance_targets().await;
    if let Some(expected) = prepared.manifest.get("maintenance_catalog") {
        let credential = match mode {
            MovePreviewCredential::Read => super::maintenance::TargetCredential::Read,
            MovePreviewCredential::Write => super::maintenance::TargetCredential::Write,
        };
        let current =
            super::maintenance::inspect_targets(conn, auth, &req.destination.space_id, credential)
                .await?;
        if json!({"catalog":current.catalog,"witness":current.witness}) != *expected {
            return Err(unavailable());
        }
    }
    if let Some(grants) = prepared.manifest["viewer_grants"].as_array().filter(|g| !g.is_empty()) {
        let mut ctx = ReviewContext { conn, auth, mode, proofs: Vec::new(), pages: Vec::new(), metadata_spaces: BTreeSet::new(), page_authority: BTreeMap::new(), metadata_authority: BTreeMap::new() };
        if json!(viewer_grant_projection(&mut ctx, source, req, grants).await?) != prepared.review.inventory["viewer_grants"] {
            return Err(unavailable());
        }
        for proof in &prepared.proofs {
            let params: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
            if rows_raw(ctx.conn, &proof.query, &params).await? != proof.rows { return Err(unavailable()); }
        }
    }
    require_preview_identity(conn, auth, mode).await?;
    access::principal_from_auth(auth)?;
    Ok(())
}

fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row.get(key).and_then(Value::as_str).unwrap_or("")
}
fn nonnull(row: &Value, key: &str) -> bool {
    row.get(key).is_some_and(|value| !value.is_null())
}

/// Portable raw row inventory, with stable column ordering and binary values encoded explicitly.
async fn rows_raw(
    conn: &mut AnyConnection,
    query: &str,
    parameters: &[&str],
) -> Result<Vec<Value>, AppError> {
    let mut query = sqlx::query(query);
    for value in parameters {
        query = query.bind(*value);
    }
    let mut result = Vec::new();
    for row in query.fetch_all(conn).await.map_err(AppError::internal)? {
        let mut record = BTreeMap::new();
        for column in row.columns() {
            let name = column.name();
            let value = if let Ok(value) = row.try_get::<Option<String>, _>(name) {
                value.map(Value::String).unwrap_or(Value::Null)
            } else if let Ok(value) = row.try_get::<Option<i64>, _>(name) {
                value.map(|v| json!(v)).unwrap_or(Value::Null)
            } else if let Ok(value) = row.try_get::<Option<f64>, _>(name) {
                value.map(|v| json!(v)).unwrap_or(Value::Null)
            } else if let Ok(value) = row.try_get::<Option<Vec<u8>>, _>(name) {
                value
                    .map(|v| json!({"binary_hex":hex_digest_bytes(&v)}))
                    .unwrap_or(Value::Null)
            } else {
                return Err(AppError::Internal("Cannot inventory move state".into()));
            };
            record.insert(name.to_string(), value);
        }
        result.push(serde_json::to_value(record).map_err(AppError::internal)?);
    }
    result.sort_by_cached_key(Value::to_string);
    Ok(result)
}
fn hex_digest_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn digest(value: &Value) -> String {
    hex_digest_bytes(&Sha256::digest(value.to_string().as_bytes()))
}

async fn review_move_current(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    source: &str,
    req: &MovePreviewRequest,
    mode: MovePreviewCredential,
) -> Result<PreparedReview, AppError> {
    require_preview_identity(conn, auth, mode).await?;
    let actor = auth.claims.sub.as_str();
    let mut ctx = ReviewContext {
        conn,
        auth,
        mode,
        proofs: Vec::new(),
        pages: Vec::new(),
        metadata_spaces: BTreeSet::new(),
        page_authority: BTreeMap::new(),
        metadata_authority: BTreeMap::new(),
    };
    if req.root.kind == "folder" && req.destination.folder_id.is_some() {
        return Err(AppError::BadRequest(
            "Folder previews use destination.parent_id for placement".into(),
        ));
    }
    if !matches!(req.root.kind.as_str(), "document" | "folder")
        || req.root.id.trim().is_empty()
        || req.destination.space_id.trim().is_empty()
    {
        return Err(AppError::BadRequest(
            "A document or folder root and destination space are required".into(),
        ));
    }
    access::require_move_preview_space(ctx.conn, auth, source, mode)
        .await
        .map_err(conceal)?;
    access::require_move_preview_space(ctx.conn, auth, &req.destination.space_id, mode)
        .await
        .map_err(conceal)?;
    let all_pages = ctx
        .rows("SELECT * FROM documents WHERE space_id=$1", &[source])
        .await?;
    let all_folders = ctx
        .rows("SELECT * FROM folders WHERE space_id=$1", &[source])
        .await?;
    let mut page_ids = BTreeSet::new();
    let mut folder_ids = BTreeSet::new();
    if req.root.kind == "document" {
        let root = all_pages
            .iter()
            .find(|row| text(row, "id") == req.root.id)
            .ok_or_else(unavailable)?;
        if !page_available(&mut ctx, root).await? {
            return Err(unavailable());
        }
        page_ids.insert(req.root.id.clone());
    } else {
        if !all_folders.iter().any(|row| text(row, "id") == req.root.id) {
            return Err(unavailable());
        }
        folder_ids.insert(req.root.id.clone());
    }
    if req.root.kind == "folder" {
        // Build adjacency once instead of rescanning every source row at every
        // depth. Preserve the existing 1000-item reviewed-move inventory limit.
        let mut child_folders: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut folder_pages: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut child_pages: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for row in &all_folders { child_folders.entry(text(row, "parent_id")).or_default().push(text(row, "id")); }
        for row in &all_pages {
            folder_pages.entry(text(row, "folder_id")).or_default().push(text(row, "id"));
            child_pages.entry(text(row, "parent_id")).or_default().push(text(row, "id"));
        }
        let mut queue = std::collections::VecDeque::from([(true, req.root.id.clone())]);
        while let Some((is_folder, id)) = queue.pop_front() {
            if is_folder {
                for child in child_folders.get(id.as_str()).into_iter().flatten() {
                    if folder_ids.insert((*child).to_owned()) { queue.push_back((true, (*child).to_owned())); }
                }
            }
            let children = if is_folder { folder_pages.get(id.as_str()) } else { child_pages.get(id.as_str()) };
            for child in children.into_iter().flatten() {
                if page_ids.insert((*child).to_owned()) { queue.push_back((false, (*child).to_owned())); }
            }
            if page_ids.len() + folder_ids.len() > 1000 { return Err(unavailable()); }
        }
    } else {
        let mut children:BTreeMap<&str,Vec<&str>>=BTreeMap::new();
        for page in &all_pages {children.entry(text(page,"parent_id")).or_default().push(text(page,"id"));}
        let mut pending=std::collections::VecDeque::from([req.root.id.clone()]);
        while let Some(id)=pending.pop_front() {
            for child in children.get(id.as_str()).into_iter().flatten() {
                if page_ids.insert((*child).to_owned()) {pending.push_back((*child).to_owned());}
            }
            if page_ids.len()+folder_ids.len()>1000 {return Err(unavailable());}
        }
    }
    let mut blockers = BTreeSet::new();
    if page_ids.len() + folder_ids.len() > 1000 {
        return Err(unavailable());
    }
    let pages: Vec<Value> = all_pages
        .into_iter()
        .filter(|row| page_ids.contains(text(row, "id")))
        .collect();
    let folders: Vec<Value> = all_folders
        .into_iter()
        .filter(|row| folder_ids.contains(text(row, "id")))
        .collect();
    let source_access=if page_ids.is_empty() {None} else {
        match access::resolve_page_batch(ctx.conn,access::principal_from_auth(ctx.auth)?,source,&page_ids,preview_page_operation(mode)).await {
            Ok(batch)=>Some(batch),Err(AppError::NotFound(_))|Err(AppError::Forbidden(_))=>None,Err(error)=>return Err(error),
        }
    };
    if let Some(batch)=&source_access {for id in batch.authorized_nodes() {capture_page_authority(&mut ctx,&id).await?;}}
    let mut undisclosed = false;
    for row in &pages {
        let available=match source_access.as_ref().map_or_else(||Err(unavailable()),|batch|batch.require(text(row,"id"))) {Ok(())=>true,Err(AppError::NotFound(_))|Err(AppError::Forbidden(_))=>false,Err(error)=>return Err(error)};
        ctx.pages.push((row.clone(),available));
        if !available {undisclosed=true;}
        if nonnull(row, "deleted_at") || nonnull(row, "archived_at") {
            blockers.insert("inactive_items".to_string());
        }
        if row.get("locked").and_then(Value::as_i64).unwrap_or(0) != 0
            && text(row, "locked_by") != actor
        {
            blockers.insert("locked_items".to_string());
        }
    }
    let mut placement = Vec::new();
    let mut participating_folders = folder_ids.clone();
    let mut participating_pages=access::PageTraversalBudget::default();
    for id in &page_ids {participating_pages.visit(id)?;}
    let mut destination_page_rows=Vec::new();
    for (table, id) in [
        ("folders", req.destination.folder_id.as_deref()),
        (
            if req.root.kind == "folder" {
                "folders"
            } else {
                "documents"
            },
            req.destination.parent_id.as_deref(),
        ),
    ] {
        if let Some(id) = id {
            let found = ctx
                .rows(
                    &format!("SELECT * FROM {table} WHERE id=$1 AND space_id=$2"),
                    &[id, &req.destination.space_id],
                )
                .await?;
            if found.is_empty()
                || (table == "documents" && !page_available(&mut ctx, &found[0]).await?)
            {
                return Err(unavailable());
            }
            if table == "documents"
                && found[0].get("folder_id").and_then(Value::as_str)
                    != req.destination.folder_id.as_deref()
            {
                blockers.insert("destination_hierarchy_invalid".into());
            }
            let mut current = Some(id.to_string());
            let mut seen = BTreeSet::new();
            while let Some(ancestor_id) = current {
                if !seen.insert(ancestor_id.clone()) {
                    blockers.insert("destination_hierarchy_invalid".into());
                    break;
                }
                if table == "documents" {participating_pages.visit(&ancestor_id)?;}
                if table == "folders" {
                    participating_folders.insert(ancestor_id.clone());
                    if participating_folders.len() > super::folders::MAX_FOLDER_OPERATION_NODES {
                        return Err(AppError::BadRequest("Folder operation exceeds the 10000-node traversal budget".into()));
                    }
                }
                let ancestor = ctx
                    .rows(
                        &format!("SELECT * FROM {table} WHERE id=$1"),
                        &[&ancestor_id],
                    )
                    .await?;
                if ancestor.is_empty() || text(&ancestor[0], "space_id") != req.destination.space_id
                {
                    blockers.insert("destination_hierarchy_invalid".into());
                    break;
                }
                if table == "documents"
                    && ancestor[0].get("folder_id").and_then(Value::as_str)
                        != req.destination.folder_id.as_deref()
                {
                    blockers.insert("destination_hierarchy_invalid".into());
                }
                if table == "documents" {destination_page_rows.push(ancestor[0].clone());}
                if nonnull(&ancestor[0], "deleted_at") || nonnull(&ancestor[0], "archived_at") {
                    blockers.insert("destination_inactive".into());
                }
                if ancestor[0]
                    .get("locked")
                    .and_then(Value::as_i64)
                    .unwrap_or(0)
                    != 0
                    && text(&ancestor[0], "locked_by") != actor
                {
                    blockers.insert("destination_locked".into());
                }
                current = ancestor[0]
                    .get("parent_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                placement.push(json!({"kind":table,"row":ancestor[0]}));
            }
            if (table == "folders" && folder_ids.contains(id))
                || (table == "documents" && page_ids.contains(id))
            {
                blockers.insert("destination_inside_subtree".into());
            }
        }
    }
    if !destination_page_rows.is_empty() {
        let targets=destination_page_rows.iter().map(|page|text(page,"id").to_owned()).collect();
        let batch=access::resolve_page_batch(ctx.conn,access::principal_from_auth(ctx.auth)?,&req.destination.space_id,&targets,preview_page_operation(mode)).await?;
        batch.require_all()?;
        for id in batch.authorized_nodes() {capture_page_authority(&mut ctx,&id).await?;}
        for page in &destination_page_rows {ctx.pages.push((page.clone(),true));}
    }
    if !folder_graph_is_acyclic(&pages) || !folder_graph_is_acyclic(&folders) {
        blockers.insert("destination_hierarchy_invalid".into());
    }
    placement.sort_by_cached_key(Value::to_string);
    let mut source_ancestors = Vec::new();
    let mut checked_folder_ancestors = BTreeSet::new();
    let mut checked_page_ancestors = BTreeSet::new();
    for (table, items) in [("documents", &pages), ("folders", &folders)] {
        for item in items {
            let mut current = Some(text(item, "id").to_string());
            let mut seen = BTreeSet::new();
            while let Some(id) = current {
                if !seen.insert(id.clone()) {
                    blockers.insert("source_hierarchy_invalid".into());
                    break;
                }
                if table == "documents" {
                    if !checked_page_ancestors.insert(id.clone()) {break;}
                    participating_pages.visit(&id)?;
                }
                if table == "folders" {
                    if !checked_folder_ancestors.insert(id.clone()) { break; }
                    participating_folders.insert(id.clone());
                    if participating_folders.len() > super::folders::MAX_FOLDER_OPERATION_NODES {
                        return Err(AppError::BadRequest("Folder operation exceeds the 10000-node traversal budget".into()));
                    }
                }
                let ancestors = ctx
                    .rows(&format!("SELECT * FROM {table} WHERE id=$1"), &[&id])
                    .await?;
                if ancestors.is_empty() || text(&ancestors[0], "space_id") != source {
                    blockers.insert("cross_boundary_hierarchy_requires_resolution".into());
                    source_ancestors.extend(ancestors);
                    break;
                }
                current = ancestors[0]
                    .get("parent_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                source_ancestors.push(ancestors[0].clone());
            }
        }
    }
    source_ancestors.sort_by_cached_key(Value::to_string);
    let mut viewer_grants = Vec::new();
    for id in &page_ids {
        viewer_grants.extend(ctx.rows("SELECT * FROM document_viewer_grants WHERE document_id=$1", &[id]).await?);
    }
    viewer_grants.sort_by_cached_key(Value::to_string);
    let viewer_projection = if undisclosed { Vec::new() } else {
        viewer_grant_projection(&mut ctx, source, req, &viewer_grants).await?
    };
    let mut manifest = json!({"actor_id":actor,"source_space_id":source,"request":req,"pages":pages,"folders":folders});
    manifest["viewer_grants"] = json!(viewer_grants);
    manifest["destination_placement"] = json!(placement);
    manifest["source_ancestors"] = json!(source_ancestors);
    manifest["public_pages_enabled"] = json!(public_pages_enabled());
    let mut metadata: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    for table in [
        "property_values",
        "doc_tags",
        "document_workflows",
        "document_workflow_events",
        "doc_versions",
        "document_drafts",
        "document_access_policies",
        "document_access_allowlist",
        "attachments",
        "excalidraw_files",
        "draft_birth_receipts",
        "draft_transfer_receipts",
        "draft_save_replay_receipts",
        "publication_receipts",
        "canonical_revision_receipts",
    ] {
        let mut collected = Vec::new();
        for id in &page_ids {
            collected.extend(
                ctx.rows(
                    &format!("SELECT * FROM {table} WHERE document_id=$1"),
                    &[id],
                )
                .await?,
            );
        }
        collected.sort_by_cached_key(Value::to_string);
        metadata.insert(table.to_string(), collected);
    }
    for (table,column) in [("attachment_upload_receipts","document_id"),("attachment_families","document_id"),("attachment_version_records","original_document_id"),("attachment_version_changes","document_id"),("attachment_retirements","document_id")] {
        let mut rows=Vec::new();for id in &page_ids {rows.extend(ctx.rows(&format!("SELECT * FROM {table} WHERE {column}=$1"),&[id]).await?);}
        rows.sort_by_cached_key(Value::to_string);metadata.insert(table.into(),rows);
    }
    let mut grants = Vec::new();
    for draft in &metadata["document_drafts"] {
        grants.extend(
            ctx.rows(
                "SELECT * FROM document_draft_access WHERE draft_id=$1",
                &[text(draft, "id")],
            )
            .await?,
        );
    }
    grants.sort_by_cached_key(Value::to_string);
    metadata.insert("document_draft_access".into(), grants);
    let source_defs = definition_inventory(
        &mut ctx,
        &metadata,
        &req.destination.space_id,
        &mut blockers,
    )
    .await?;
    manifest["metadata"] = json!(metadata);
    manifest["definitions"] = source_defs.1;
    // Include incoming and outgoing scope changes; never emit inaccessible endpoint identities.
    let relations=ctx.rows("SELECT v.*,s.space_id AS schema_space_id FROM property_values v JOIN properties_schema s ON s.id=v.property_id WHERE s.property_type='relation'",&[]).await?;
    let mut relevant_relations = Vec::new();
    let mut visible_relations = Vec::new();
    let mut relation_endpoints = Vec::new();
    let mut relation_definitions = Vec::new();
    for relation in relations {
        let from = text(&relation, "document_id");
        let to = text(&relation, "value");
        if !page_ids.contains(from) && !page_ids.contains(to) {
            continue;
        }
        let crossing = page_ids.contains(from) != page_ids.contains(to);
        if crossing && !to.is_empty() && source != req.destination.space_id {
            blockers.insert("external_relations_require_resolution".into());
        }
        let mut visible = true;
        for id in [from, to] {
            if id.is_empty() {
                continue;
            }
            let endpoint = ctx
                .rows("SELECT * FROM documents WHERE id=$1", &[id])
                .await?;
            let available = !endpoint.is_empty() && page_available(&mut ctx, &endpoint[0]).await?;
            if !available {
                visible = false;
            }
            let mut authority = BTreeMap::new();
            if let Some(page) = endpoint.first() {
                for table in [
                    "spaces",
                    "space_members",
                    "space_group_permissions",
                    "space_user_blocks",
                    "space_roles",
                ] {
                    let key = if table == "spaces" { "id" } else { "space_id" };
                    authority.insert(
                        table,
                        ctx.rows(
                            &format!("SELECT * FROM {table} WHERE {key}=$1"),
                            &[text(page, "space_id")],
                        )
                        .await?,
                    );
                }
            }
            relation_endpoints
                .push(json!({"endpoint":endpoint,"available":available,"authority":authority}));
        }
        if visible {
            visible_relations.push(
                json!({"assignment_id":relation["id"],"property_id":relation["property_id"],"from_document_id":from,"to_document_id":to,"crosses_subtree":crossing}),
            );
        } else {
            blockers.insert("unavailable_related_items".into());
        }
        relation_definitions.extend(
            ctx.rows(
                "SELECT * FROM properties_schema WHERE id=$1",
                &[text(&relation, "property_id")],
            )
            .await?,
        );
        relevant_relations.push(relation);
    }
    relation_endpoints.sort_by_cached_key(Value::to_string);
    relation_definitions.sort_by_cached_key(Value::to_string);
    manifest["relations"] = json!(relevant_relations);
    manifest["relation_endpoints"] = json!(relation_endpoints);
    manifest["relation_definitions"] = json!(relation_definitions);
    let mut boundary = Vec::new();
    for id in &page_ids {
        for row in ctx
            .rows(
                "SELECT id,space_id,parent_id,folder_id FROM documents WHERE parent_id=$1",
                &[id],
            )
            .await?
        {
            if !page_ids.contains(text(&row, "id")) {
                boundary.push(row);
            }
        }
    }
    for page in &pages {
        if text(page, "id") != req.root.id
            && nonnull(page, "parent_id")
            && !page_ids.contains(text(page, "parent_id"))
        {
            boundary.push(page.clone());
        }
        if req.root.kind == "folder"
            && nonnull(page, "folder_id")
            && !folder_ids.contains(text(page, "folder_id"))
        {
            boundary.push(page.clone());
        }
    }
    for id in &folder_ids {
        for row in ctx
            .rows(
                "SELECT id,space_id,parent_id FROM folders WHERE parent_id=$1",
                &[id],
            )
            .await?
        {
            if !folder_ids.contains(text(&row, "id")) {
                boundary.push(row);
            }
        }
    }
    if !boundary.is_empty() {
        blockers.insert("cross_boundary_hierarchy_requires_resolution".into());
    }
    boundary.sort_by_cached_key(Value::to_string);
    manifest["hierarchy_boundary"] = json!(boundary);
    let mut storage = Vec::new();
    for page in &pages {
        storage.push(file_fingerprint(text(page, "content_path"), true, &mut blockers).await);
    }
    for table in ["attachments", "excalidraw_files"] {
        for asset in &metadata[table] {
            storage.push(file_fingerprint(text(asset, "storage_path"), true, &mut blockers).await);
        }
    }
    // Optional draft/lifecycle mirrors are inventoried too; authoritative draft bodies remain DB-only.
    let spaces = ctx
        .rows(
            "SELECT * FROM spaces WHERE id=$1 OR id=$2",
            &[source, &req.destination.space_id],
        )
        .await?;
    let slug = spaces
        .iter()
        .find(|row| text(row, "id") == source)
        .map(|row| text(row, "slug"))
        .unwrap_or("");
    for draft in &metadata["document_drafts"] {
        let path = format!(
            "spaces/{slug}/drafts/{}",
            crate::storage::document_filename(text(draft, "title"), text(draft, "id"))
        );
        storage.push(file_fingerprint(&path, false, &mut blockers).await);
    }
    for page in &pages {
        for lifecycle in ["archived", "trashed"] {
            let path = format!(
                "spaces/{slug}/{lifecycle}/{}",
                crate::storage::document_filename(text(page, "title"), text(page, "id"))
            );
            storage.push(file_fingerprint(&path, false, &mut blockers).await);
        }
    }
    storage.sort_by_cached_key(Value::to_string);
    manifest["storage"] = json!(storage);
    manifest["spaces"] = json!(spaces);
    let audience = audience_snapshot(&mut ctx, source, &req.destination.space_id).await?;
    manifest["audience"] = audience.0;
    let access = json!({"space_audience_gained":audience.1,"space_audience_lost":audience.2,"public_links":"Existing intentional page links remain independent of space visibility; lifecycle and deployment switch still apply.","private_drafts":"Ownership and explicit draft grants must remain separate; destination page permissions may change participation.","permission_model":"existing_space_roles_and_draft_participation"});
    manifest["typed_page_authority"] = json!(ctx.page_authority);
    manifest["typed_metadata_authority"] = json!(ctx.metadata_authority);
    // An unpublished canonical title is not authorized by participation in another draft.
    // Capture the exact selected draft query as a final source proof for every subtree page.
    // Internal collision evidence includes all lifecycle states, never destination labels.
    let identifier_destination_rows = ctx.rows("SELECT id,human_identifier,human_identifier_key FROM documents WHERE space_id=$1 ORDER BY id", &[&req.destination.space_id]).await?;
    manifest["identifier_destination_rows"] = json!(identifier_destination_rows);
    let identifier_edit_access=if page_ids.is_empty(){None}else{match access::resolve_page_batch(ctx.conn,access::principal_from_auth(ctx.auth)?,source,&page_ids,access::PageOperation::Edit).await {
        Ok(batch)=>Some(batch),Err(AppError::Forbidden(_))|Err(AppError::NotFound(_))|Err(AppError::Conflict(_))=>None,Err(error)=>return Err(error),
    }};
    let mut projected_pages = Vec::new();
    for page in &pages {
        let title = if nonnull(page, "published_at") {
            page["title"].clone()
        } else {
            let selected = ctx.rows("SELECT dd.id,dd.title,dd.updated_at FROM document_drafts dd WHERE dd.document_id=$1 AND dd.space_id=$2 AND (dd.owner_id=$3 OR EXISTS(SELECT 1 FROM document_draft_access a WHERE a.draft_id=dd.id AND a.user_id=$3 AND a.role IN ('viewer','editor'))) ORDER BY CASE WHEN dd.owner_id=$3 THEN 0 ELSE 1 END,dd.updated_at DESC,dd.id ASC LIMIT 1", &[text(page, "id"), source, actor]).await?;
            let Some(draft) = selected.first() else {
                undisclosed = true;
                continue;
            };
            draft["title"].clone()
        };
        let identifier = page.get("human_identifier").cloned().unwrap_or(Value::Null);
        let normalized = super::page_metadata::normalize_identifier(identifier.as_str())?;
        if json!(normalized.0) != identifier
            || json!(normalized.1)
                != page
                    .get("human_identifier_key")
                    .cloned()
                    .unwrap_or(Value::Null)
        {
            return Err(AppError::Conflict(
                "Move identifier source needs review".into(),
            ));
        }
        let collision = normalized.1.as_ref().is_some_and(|key| {
            identifier_destination_rows.iter().any(|row| {
                !pages.iter().any(|moving| moving["id"] == row["id"])
                    && row["human_identifier_key"].as_str() == Some(key)
            }) || pages.iter().any(|other| {
                text(other, "id") != text(page, "id")
                    && other["human_identifier_key"].as_str() == Some(key)
            })
        });
        let edit_allowed=identifier_edit_access.as_ref().is_some_and(|batch|batch.require(text(page,"id")).is_ok());
        projected_pages.push(json!({"id":page["id"],"title":title,"space_id":page["space_id"],"folder_id":page["folder_id"],"parent_id":page["parent_id"],"published":nonnull(page,"published_at"),"public_link_enabled":page.get("public").and_then(Value::as_i64)==Some(1),"human_identifier":identifier,"identifier_resolution_required":collision,"identifier_edit_allowed":edit_allowed}));
    }
    let inventory = if undisclosed {
        blockers.clear();
        blockers.insert("subtree_unavailable".into());
        json!({})
    } else {
        json!({"pages":projected_pages,"folders":folders,"viewer_grants":viewer_projection,"metadata":source_defs.0,"relations":visible_relations,"attachments":metadata["attachments"].len(),"drawings":metadata["excalidraw_files"].len(),"history_versions":metadata["doc_versions"].len(),"workflow_history_events":metadata["document_workflow_events"].len(),"private_drafts_preserved":true})
    };
    let mut relation_targets = Vec::new();
    let mut target_sources = Vec::new();
    let target_rows=ctx.rows("SELECT * FROM documents WHERE space_id=$1 AND archived_at IS NULL AND deleted_at IS NULL ORDER BY id LIMIT 101",&[&req.destination.space_id]).await?;
    // Do not expose raw candidate counts or whether hidden rows filled the bound.
    let targets_complete = false;
    for target in target_rows.iter().take(100) {
        if !page_available(&mut ctx, target).await? {
            continue;
        }
        let title = if nonnull(target, "published_at") {
            target["title"].clone()
        } else {
            let selected=ctx.rows("SELECT dd.id,dd.title,dd.updated_at FROM document_drafts dd WHERE dd.document_id=$1 AND dd.space_id=$2 AND (dd.owner_id=$3 OR EXISTS(SELECT 1 FROM document_draft_access a WHERE a.draft_id=dd.id AND a.user_id=$3 AND a.role IN ('viewer','editor'))) ORDER BY CASE WHEN dd.owner_id=$3 THEN 0 ELSE 1 END,dd.updated_at DESC,dd.id ASC LIMIT 1",&[text(target,"id"),&req.destination.space_id,actor]).await?;
            let Some(draft) = selected.first() else {
                continue;
            };
            target_sources.push(json!({"target":target,"selected_draft":draft}));
            draft["title"].clone()
        };
        target_sources.push(json!({"target":target}));
        relation_targets.push(json!({"id":target["id"],"title":title}));
    }
    manifest["relation_target_choice_sources"] = json!(target_sources);
    let mut inventory = inventory;
    if !undisclosed {
        inventory["relation_targets"] = json!(relation_targets);
        inventory["relation_targets_complete"] = json!(targets_complete);
        inventory["relation_targets_unavailable_reason"] = if targets_complete {
            Value::Null
        } else {
            json!("Target choices are bounded to this current review. If the required page is not listed, preserve the relation or choose another destination.")
        };
    }
    let credential = match ctx.mode {
        MovePreviewCredential::Read => super::maintenance::TargetCredential::Read,
        MovePreviewCredential::Write => super::maintenance::TargetCredential::Write,
    };
    let targets = super::maintenance::inspect_targets(
        ctx.conn,
        ctx.auth,
        &req.destination.space_id,
        credential,
    )
    .await?;
    manifest["maintenance_catalog"] = json!({"catalog":targets.catalog,"witness":targets.witness});
    let mut target_sources = Vec::new();
    let mut preservation = BTreeMap::new();
    for page in &pages {
        let (user, group) = commit::maintenance_pair(page)?;
        let witness =
            super::maintenance::target_source(ctx.conn, user.as_deref(), group.as_deref()).await?;
        if user.is_some() && witness["users"].as_array().map(Vec::len) != Some(1)
            || group.is_some() && witness["groups"].as_array().map(Vec::len) != Some(1)
        {
            return Err(AppError::Conflict("Maintenance source needs review".into()));
        }
        target_sources
            .push(json!({"document_id":page["id"],"user":user,"group":group,"witness":witness}));
        let valid = match super::maintenance::validate_assignment(
            ctx.conn,
            ctx.auth,
            &req.destination.space_id,
            user.as_deref(),
            group.as_deref(),
            match ctx.mode {
                MovePreviewCredential::Read => super::maintenance::TargetUse::PreserveMoveRead,
                MovePreviewCredential::Write => super::maintenance::TargetUse::PreserveMove,
            },
        )
        .await
        {
            Ok(_) => true,
            Err(AppError::Forbidden(_))
            | Err(AppError::NotFound(_))
            | Err(AppError::Conflict(_)) => false,
            Err(error) => return Err(error),
        };
        preservation.insert(text(page, "id").to_string(), valid);
    }
    manifest["maintenance_target_sources"] = json!(target_sources);
    let maintenance_edit_access=if page_ids.is_empty(){None}else{match access::resolve_page_batch(ctx.conn,access::principal_from_auth(ctx.auth)?,source,&page_ids,access::PageOperation::Edit).await {
        Ok(batch)=>Some(batch),Err(AppError::Forbidden(_))|Err(AppError::NotFound(_))|Err(AppError::Conflict(_))=>None,Err(error)=>return Err(error),
    }};
    for page in inventory
        .get_mut("pages")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let raw = pages
            .iter()
            .find(|r| r["id"] == page["id"])
            .ok_or_else(unavailable)?;
        let (user, group) = commit::maintenance_pair(raw)?;
        let editable=maintenance_edit_access.as_ref().is_some_and(|batch|batch.require(text(raw,"id")).is_ok());
        page["maintenance_user_id"] = json!(user);
        page["maintenance_group_id"] = json!(group);
        page["maintenance_resolution_required"] = json!(!preservation[text(raw, "id")]);
        page["maintenance_edit_allowed"] = json!(editable);
    }
    if inventory["pages"]
        .as_array()
        .is_some_and(|pages| !pages.is_empty())
    {
        inventory["maintenance_targets"] = targets.catalog;
    }
    let fingerprint = digest(&manifest);
    #[cfg(test)]
    after_inventory().await;
    Ok(PreparedReview {
        manifest: manifest.clone(),
        review: Review {
            fingerprint,
            inventory,
            access,
            blockers: blockers.into_iter().collect(),
        },
        proofs: ctx.proofs,
        pages: ctx.pages,
        metadata_spaces: ctx.metadata_spaces,
        public_pages_enabled: manifest["public_pages_enabled"].as_bool().unwrap_or(false),
    })
}

async fn definition_inventory(
    ctx: &mut ReviewContext<'_>,
    metadata: &BTreeMap<String, Vec<Value>>,
    destination: &str,
    blockers: &mut BTreeSet<String>,
) -> Result<(Value, Value), AppError> {
    let mut visible = Vec::new();
    let mut raw = Vec::new();
    for (kind, table, assignments, key) in [
        (
            "property",
            "properties_schema",
            "property_values",
            "property_id",
        ),
        ("tag", "tags", "doc_tags", "tag_id"),
        (
            "workflow",
            "workflow_templates",
            "document_workflows",
            "workflow_template_id",
        ),
    ] {
        let create_allowed = match commit::require_create_metadata(
            ctx.conn,
            ctx.auth,
            destination,
            kind == "workflow",
        )
        .await
        {
            Ok(()) => true,
            Err(AppError::Forbidden(_)) | Err(AppError::NotFound(_)) => false,
            Err(error) => return Err(error),
        };
        let ids: BTreeSet<&str> = metadata[assignments]
            .iter()
            .map(|row| text(row, key))
            .collect();
        let targets = ctx
            .rows(
                &format!("SELECT * FROM {table} WHERE space_id=$1"),
                &[destination],
            )
            .await?;
        raw.push(json!({"kind":kind,"destination_definitions":targets}));
        for id in ids {
            let definitions = ctx
                .rows(&format!("SELECT * FROM {table} WHERE id=$1"), &[id])
                .await?;
            if definitions.is_empty() {
                blockers.insert("unavailable_metadata_definitions".into());
                continue;
            }
            let definition = &definitions[0];
            raw.push(json!({"kind":kind,"source_definition":definition}));
            match require_visible_space(ctx, text(definition, "space_id")).await {
                Ok(_) => {}
                Err(AppError::Forbidden(_)) | Err(AppError::NotFound(_)) => {
                    blockers.insert("unavailable_metadata_definitions".into());
                    continue;
                }
                Err(error) => return Err(error),
            }
            let values: Vec<&Value> = metadata[assignments]
                .iter()
                .filter(|row| text(row, key) == id)
                .collect();
            let mut user_values_valid = true;
            if kind == "property" && text(definition, "property_type") == "user" {
                for value in &values {
                    if let Some(user) = value
                        .get("value")
                        .and_then(Value::as_str)
                        .filter(|user| !user.is_empty())
                    {
                        let active: Option<i64> =
                            sqlx::query_scalar("SELECT 1 FROM users WHERE id=$1 AND is_active=1")
                                .bind(user)
                                .fetch_optional(&mut *ctx.conn)
                                .await
                                .map_err(AppError::internal)?;
                        if active.is_none() {
                            user_values_valid = false;
                        }
                    }
                }
            }
            let mut valid_workflows = BTreeSet::new();
            if kind == "workflow" {
                for target in &targets {
                    if workflow_approvers_valid(ctx, target, destination).await? {
                        valid_workflows.insert(text(target, "id").to_string());
                    }
                }
            }
            let candidates:Vec<Value>=targets.iter().filter(|target| {
                if kind=="property" {
                    let property_type=text(definition,"property_type");
                    if text(target,"property_type")!=property_type || property_type=="relation" || (property_type=="user" && !user_values_valid) {return false;}
                    let options=parse_json_field(target,"options");
                    if nonnull(target,"options") && options.is_none() {return false;}
                    if super::property_validation::validate_schema(text(target,"name"),property_type,options.as_ref()).is_err() {return false;}
                    values.iter().all(|value|super::property_validation::validate_value(property_type,options.as_ref(),value.get("value").and_then(Value::as_str)).is_ok())
                } else if kind=="workflow" {
                    valid_workflows.contains(text(target,"id")) && workflow_candidate_valid(definition,target,&values,&metadata["document_workflow_events"])
                } else {true}
            }).map(|target|json!({"id":target["id"],"name":target["name"],"choice_details":if kind=="workflow" {json!({"stages":parse_json_field(target,"stages")})} else if kind=="property" {json!({"property_type":target["property_type"],"options":parse_json_field(target,"options")})} else {json!({})},"compatibility":if kind=="property" {"type_and_stored_values"} else if kind=="workflow" {"exact_stages_and_approvers"} else {"explicit_tag_selection"}})).collect();
            let resolution_candidates: Vec<Value> = if kind == "property"
                && text(definition, "property_type") == "relation"
            {
                targets.iter().filter(|t|text(t,"property_type")=="relation" && super::property_validation::validate_schema(text(t,"name"),"relation",parse_json_field(t,"options").as_ref()).is_ok()).map(|t|json!({"id":t["id"],"name":t["name"],"compatibility":"requires_explicit_relation_targets","choice_details":{"property_type":"relation","options":parse_json_field(t,"options")}})).collect()
            } else {
                Vec::new()
            };
            // Relation endpoints are projected only by the independently authorized relation inventory.
            // Ordinary scalar assignment identities/values are needed for explicit value remapping.
            let choice_assignments: Vec<Value> = if kind == "property"
                && text(definition, "property_type") != "relation"
            {
                values.iter().map(|value| json!({"id":value["id"],"document_id":value["document_id"],"value":value["value"]})).collect()
            } else {
                Vec::new()
            };
            visible.push(json!({"create_allowed":create_allowed,"assignments":choice_assignments,"kind":kind,"source_definition":definition,"assignment_count":values.len(),"candidates":candidates,"resolution_candidates":resolution_candidates,"decision_required":true,"relation_scope_requires_resolution":kind=="property" && text(definition,"property_type")=="relation"}));
        }
    }
    Ok((json!(visible), json!(raw)))
}
/// Linear graph validation for retained folder rows; external ancestors are
/// validated separately against their actual same-space database rows.
fn folder_graph_is_acyclic(items: &[Value]) -> bool {
    let parents: BTreeMap<&str, &str> = items.iter()
        .map(|row| (text(row, "id"), text(row, "parent_id"))).collect();
    let mut complete = BTreeSet::new();
    for id in parents.keys() {
        let mut path = BTreeSet::new();
        let mut current = *id;
        while let Some(parent) = parents.get(current) {
            if complete.contains(current) { break; }
            if !path.insert(current) { return false; }
            current = parent;
        }
        complete.extend(path);
    }
    true
}
fn valid_workflow_stages(definition: &Value) -> bool {
    let Some(stages) = parse_json_field(definition, "stages") else {
        return false;
    };
    let Ok(stages) = serde_json::from_value::<Vec<super::workflows::WorkflowStage>>(stages) else {
        return false;
    };
    if !(2..=12).contains(&stages.len()) || stages.iter().any(|stage| stage.name.trim().is_empty())
    {
        return false;
    }
    let ids: BTreeSet<&str> = stages
        .iter()
        .filter_map(|stage| stage.id.as_deref())
        .filter(|id| !id.trim().is_empty())
        .collect();
    ids.len() == stages.len()
}

fn workflow_candidate_valid(
    source: &Value,
    target: &Value,
    assignments: &[&Value],
    history: &[Value],
) -> bool {
    if nonnull(source, "approvers") && parse_json_field(source, "approvers").is_none() {
        return false;
    }
    if !valid_workflow_stages(target) {
        return false;
    }
    let Some(Value::Array(stages)) = parse_json_field(target, "stages") else {
        return false;
    };
    if parse_json_field(source, "stages") != Some(Value::Array(stages.clone()))
        || parse_json_field(source, "approvers") != parse_json_field(target, "approvers")
    {
        return false;
    }
    let ids: BTreeSet<&str> = stages.iter().map(|stage| text(stage, "id")).collect();
    if ids.contains("") || ids.len() != stages.len() {
        return false;
    }
    if assignments
        .iter()
        .any(|row| !ids.contains(text(row, "current_stage_id")))
    {
        return false;
    }
    let documents: BTreeSet<&str> = assignments
        .iter()
        .map(|row| text(row, "document_id"))
        .collect();
    history
        .iter()
        .filter(|row| documents.contains(text(row, "document_id")))
        .all(|event| {
            ["from_stage_id", "to_stage_id"]
                .iter()
                .all(|key| !nonnull(event, key) || ids.contains(text(event, key)))
        })
}
async fn workflow_approvers_valid(
    ctx: &mut ReviewContext<'_>,
    target: &Value,
    destination: &str,
) -> Result<bool, AppError> {
    let approvers = match parse_json_field(target, "approvers") {
        None if !nonnull(target, "approvers") => vec![],
        None => return Ok(false),
        Some(Value::Array(values)) => values,
        _ => return Ok(false),
    };
    if !valid_workflow_stages(target) {
        return Ok(false);
    }
    let Some(Value::Array(stages)) = parse_json_field(target, "stages") else {
        return Ok(false);
    };
    let stage_ids: BTreeSet<&str> = stages.iter().map(|stage| text(stage, "id")).collect();
    for approver in approvers {
        if nonnull(&approver, "stage_id") && !stage_ids.contains(text(&approver, "stage_id")) {
            return Ok(false);
        }
        let id = text(&approver, "approver_id");
        if id.is_empty() {
            return Ok(false);
        }
        match text(&approver, "approver_type") {
            "user" => {
                let active: Option<i64> =
                    sqlx::query_scalar("SELECT 1 FROM users WHERE id=$1 AND is_active=1")
                        .bind(id)
                        .fetch_optional(&mut *ctx.conn)
                        .await
                        .map_err(AppError::internal)?;
                if active.is_none() {
                    return Ok(false);
                }
                match hypothetical_space_view(ctx.conn, destination, id).await {
                    Ok(_) => {}
                    Err(AppError::Forbidden(_)) | Err(AppError::NotFound(_)) => return Ok(false),
                    Err(error) => return Err(error),
                }
            }
            "role" => {
                if !matches!(id, "owner" | "admin")
                    && !super::routes::SPACE_ROLE_DEFINITIONS
                        .iter()
                        .any(|role| role.id == id)
                {
                    let role:Option<i64>=sqlx::query_scalar("SELECT 1 FROM space_roles WHERE id=$1 AND space_id=$2 UNION SELECT 1 FROM global_roles WHERE id=$1").bind(id).bind(destination).fetch_optional(&mut *ctx.conn).await.map_err(AppError::internal)?;
                    if role.is_none() {
                        return Ok(false);
                    }
                }
            }
            _ => return Ok(false),
        }
    }
    Ok(true)
}

fn parse_json_field(row: &Value, key: &str) -> Option<Value> {
    row.get(key)
        .filter(|v| !v.is_null())
        .and_then(|v| v.as_str())
        .and_then(|v| serde_json::from_str(v).ok())
}

async fn hypothetical_space_view(
    conn: &mut AnyConnection,
    space: &str,
    user: &str,
) -> Result<(), AppError> {
    let access = access::resolve_space_access(
        conn,
        access::Principal::User {
            user_id: user,
            session_id: None,
            token_expires_at: None,
        },
        space,
        access::Purpose::Routine,
    )
    .await?;
    if !access.allows(access::Permission::View) {
        return Err(unavailable());
    }
    Ok(())
}
/// Only an already-entitled source manager receives this grant projection.
/// Booleans describe this review; confirmation rechecks actual authority.
async fn viewer_grant_projection(
    ctx: &mut ReviewContext<'_>, source: &str, req: &MovePreviewRequest,
    grants: &[Value],
) -> Result<Vec<Value>, AppError> {
    let write = matches!(ctx.mode, MovePreviewCredential::Write);
    let mut projection = Vec::new();
    for grant in grants {
        let page = text(grant, "document_id");
        super::page_viewer_grant_management::authority(ctx.conn, ctx.auth, source, page, write, false)
            .await.map_err(conceal)?;
        let recipient = ctx.rows("SELECT id,display_name,email,role,is_active,account_kind FROM users WHERE id=$1", &[text(grant, "user_id")]).await?;
        let raw = recipient.first().ok_or_else(unavailable)?;
        let available = grant_move_eligible(
            super::page_viewer_grant_management::recipient(ctx.conn, source, text(grant, "user_id"), true).await,
        )?;
        let preserve_allowed = grant_move_eligible(require_viewer_grant_preservation(
            ctx.conn, ctx.auth, source, req, grant, write,
        ).await)?;
        // Capture every actual policy in both source and prospective parent
        // chains. Internal raw evidence remains separate from this safe projection.
        for (space, selected) in [(source, Some(page)), (req.destination.space_id.as_str(), req.destination.parent_id.as_deref().filter(|_| req.root.kind == "document"))] {
            if let Some(selected) = selected {
                let ancestors = super::page_viewer_grant_management::authority(ctx.conn, ctx.auth, space, selected, write, false).await;
                let ancestors = match ancestors {
                    Ok(ancestors) => ancestors,
                    Err(AppError::Forbidden(_)) | Err(AppError::NotFound(_)) | Err(AppError::Conflict(_)) => continue,
                    Err(error) => return Err(error),
                };
                {
                    for id in std::iter::once(selected.to_owned()).chain(ancestors) {
                        ctx.rows("SELECT * FROM documents WHERE id=$1 AND space_id=$2", &[&id, space]).await?;
                        ctx.rows("SELECT * FROM document_access_policies WHERE document_id=$1 AND space_id=$2", &[&id, space]).await?;
                        ctx.rows("SELECT * FROM document_access_allowlist WHERE document_id=$1 AND space_id=$2", &[&id, space]).await?;
                    }
                }
            }
        }
        projection.push(json!({"grant_id":grant["id"],"document_id":grant["document_id"],"revision":grant["revision"],"role":"viewer","scope":"page",
            "recipient":{"available":available,"display_name":if available {raw["display_name"].clone()} else {Value::Null}},
            "preserve_allowed":preserve_allowed,"remove_allowed":true}));
    }
    projection.sort_by_cached_key(Value::to_string);
    Ok(projection)
}
fn grant_move_eligible<T>(result: Result<T, AppError>) -> Result<bool, AppError> {
    match result {
        Ok(_) => Ok(true),
        Err(AppError::Forbidden(_)) | Err(AppError::NotFound(_)) | Err(AppError::Conflict(_)) => Ok(false),
        Err(error) => Err(error),
    }
}
async fn require_viewer_grant_preservation(
    conn: &mut AnyConnection, auth: &AuthUser, source: &str,
    req: &MovePreviewRequest, grant: &Value, write: bool,
) -> Result<(), AppError> {
    super::page_viewer_grant_management::authority(conn, auth, source, text(grant, "document_id"), write, true).await?;
    super::page_viewer_grant_management::move_space_authority(conn, auth, &req.destination.space_id, write, true).await?;
    if req.root.kind == "document" {
        if let Some(parent) = req.destination.parent_id.as_deref() {
            super::page_viewer_grant_management::authority(conn, auth, &req.destination.space_id, parent, write, true).await?;
        }
    }
    super::page_viewer_grant_management::recipient(conn, &req.destination.space_id, text(grant, "user_id"), true).await?;
    Ok(())
}

async fn audience_snapshot(
    ctx: &mut ReviewContext<'_>,
    source: &str,
    destination: &str,
) -> Result<(Value, usize, usize), AppError> {
    let users = ctx
        .rows("SELECT id,role,is_active,account_kind FROM users ORDER BY id", &[])
        .await?;
    let mut source_viewers = BTreeSet::new();
    let mut destination_viewers = BTreeSet::new();
    for user in &users {
        if user.get("is_active").and_then(Value::as_i64) != Some(1) {
            continue;
        }
        let id = text(user, "id");
        for (space, viewers) in [
            (source, &mut source_viewers),
            (destination, &mut destination_viewers),
        ] {
            match hypothetical_space_view(ctx.conn, space, id).await {
                Ok(_) => {
                    viewers.insert(id.to_string());
                }
                Err(AppError::Forbidden(_)) | Err(AppError::NotFound(_)) => {}
                Err(error) => return Err(error),
            }
        }
    }
    let mut bindings = BTreeMap::new();
    for table in [
        "space_members",
        "space_group_permissions",
        "space_user_blocks",
        "space_roles",
    ] {
        bindings.insert(
            table,
            ctx.rows(
                &format!("SELECT * FROM {table} WHERE space_id=$1 OR space_id=$2"),
                &[source, destination],
            )
            .await?,
        );
    }
    // Global/group membership and active account state can change the audience without touching pages.
    for table in ["group_members", "group_global_roles", "global_roles"] {
        bindings.insert(
            table,
            ctx.rows(&format!("SELECT * FROM {table}"), &[]).await?,
        );
    }
    let gained = destination_viewers.difference(&source_viewers).count();
    let lost = source_viewers.difference(&destination_viewers).count();
    Ok((
        json!({"users":users,"bindings":bindings,"source_viewers":source_viewers,"destination_viewers":destination_viewers}),
        gained,
        lost,
    ))
}

async fn file_fingerprint(
    relative: &str,
    required: bool,
    blockers: &mut BTreeSet<String>,
) -> Value {
    use std::path::{Component, Path};
    let path = Path::new(relative);
    if path.components().next().is_none()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        blockers.insert("storage_unavailable".into());
        return json!({"path":relative,"status":"unsafe"});
    }
    let root = match tokio::fs::canonicalize(crate::storage::data_dir()).await {
        Ok(root) => root,
        Err(_) => {
            if required {
                blockers.insert("storage_unavailable".into());
            }
            return json!({"path":relative,"status":"missing"});
        }
    };
    let mut full = root.clone();
    for component in path.components() {
        full.push(component.as_os_str());
        match tokio::fs::symlink_metadata(&full).await {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                blockers.insert("storage_unavailable".into());
                return json!({"path":relative,"status":"unsafe"});
            }
            Ok(_) => {}
            Err(_) => {
                if required {
                    blockers.insert("storage_unavailable".into());
                }
                return json!({"path":relative,"status":"missing"});
            }
        }
    }
    let canonical = match tokio::fs::canonicalize(&full).await {
        Ok(path) if path.starts_with(&root) => path,
        _ => {
            blockers.insert("storage_unavailable".into());
            return json!({"path":relative,"status":"unsafe"});
        }
    };
    match tokio::fs::read(canonical).await {
        Ok(bytes) => {
            json!({"path":relative,"sha256":hex_digest_bytes(&Sha256::digest(&bytes)),"bytes":bytes.len()})
        }
        Err(_) => {
            blockers.insert("storage_unavailable".into());
            json!({"path":relative,"status":"unreadable"})
        }
    }
}

#[cfg(test)]
thread_local! {
    static AFTER_MAINTENANCE_TARGETS:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};
    static AFTER_RAW_PROOFS:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};
    static AFTER_FINAL_RECORD:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};
    static FINAL_RECORD_SQL:std::cell::RefCell<Option<&'static str>>=const{std::cell::RefCell::new(None)};
}
#[cfg(test)]
async fn after_maintenance_targets() {
    let hook = AFTER_MAINTENANCE_TARGETS.with(|h| h.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}
#[cfg(test)]
async fn after_raw_proofs() {
    let hook = AFTER_RAW_PROOFS.with(|h| h.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}
#[cfg(test)]
async fn after_final_record(conn: &mut AnyConnection) {
    if let Some(statement) = FINAL_RECORD_SQL.with(|h| h.borrow_mut().take()) {
        sqlx::query(statement).execute(&mut *conn).await.unwrap();
    }
    let hook = AFTER_FINAL_RECORD.with(|h| h.borrow_mut().take());
    if let Some(hook) = hook {
        hook.await;
    }
}
#[cfg(test)]
thread_local! {static AFTER_INVENTORY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn after_inventory() {
    let hook = AFTER_INVENTORY.with(|h| h.borrow_mut().take());
    if let Some(h) = hook {
        h.await;
    }
}
#[cfg(test)]
async fn review_move(
    state: &AppState,
    actor: &str,
    source: &str,
    req: &MovePreviewRequest,
) -> Result<Review, AppError> {
    let auth = tests::test_auth(actor);
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let prepared =
        review_move_current(&mut conn, &auth, source, req, MovePreviewCredential::Read).await?;
    verify_review(
        &mut conn,
        &auth,
        source,
        req,
        &prepared,
        MovePreviewCredential::Read,
    )
    .await?;
    Ok(prepared.review)
}
#[cfg(test)]
mod tests {
    use super::super::documents::tests::{
        move_database_snapshot, move_file_snapshot, move_guard_fixture,
    };
    use super::*;
    pub(super) fn test_auth(actor: &str) -> AuthUser {
        let mut auth = super::super::documents::tests::test_auth(actor);
        auth.claims.sid = format!("preview-{actor}");
        auth
    }
    pub(super) async fn fixture() -> (AppState, String, String) {
        fixture_with_wal(false).await
    }
    pub(super) async fn fixture_with_wal(wal: bool) -> (AppState, String, String) {
        use sqlx::any::AnyPoolOptions;
        let (mut state, source, target) = move_guard_fixture().await;
        let url = if wal {
            format!(
                "sqlite:///tmp/move-preview-{}.sqlite?mode=rwc",
                uuid::Uuid::new_v4()
            )
        } else {
            "sqlite::memory:".into()
        };
        let db = AnyPoolOptions::new()
            .max_connections(if wal { 2 } else { 1 })
            .connect(&url)
            .await
            .unwrap();
        if wal {
            sqlx::query("PRAGMA journal_mode=WAL")
                .execute(&db)
                .await
                .unwrap();
        }
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        state.db = db;
        for id in ["mover", "admin-mover", "other-owner", "SECRET-recipient"] {
            sqlx::query("INSERT INTO users (id,email,display_name,role) VALUES ($1,$2,$1,$3)")
                .bind(id)
                .bind(format!("{id}@example.test"))
                .bind(if id == "admin-mover" {
                    "admin"
                } else {
                    "member"
                })
                .execute(&state.db)
                .await
                .unwrap();
        }
        for actor in ["mover", "admin-mover", "other-owner", "SECRET-recipient"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,$1,'2099-01-01T00:00:00Z')").bind(format!("preview-{actor}")).bind(actor).execute(&state.db).await.unwrap();
        }
        for (id, slug) in [("move-source", &source), ("move-target", &target)] {
            sqlx::query("INSERT INTO spaces (id,name,slug,visibility,created_by) VALUES ($1,$1,$2,'private','mover')").bind(id).bind(slug).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO space_members (id,space_id,user_id,role) VALUES ($1,$2,'mover','editor')").bind(format!("member-{id}")).bind(id).execute(&state.db).await.unwrap();
        }
        for (id, parent) in [
            ("source-folder", None),
            ("child-folder", Some("source-folder")),
            ("other-folder", None),
        ] {
            sqlx::query(
                "INSERT INTO folders (id,space_id,parent_id,name) VALUES ($1,'move-source',$2,$1)",
            )
            .bind(id)
            .bind(parent)
            .execute(&state.db)
            .await
            .unwrap();
        }
        sqlx::query("INSERT INTO folders (id,space_id,name) VALUES ('target-folder','move-target','Target')").execute(&state.db).await.unwrap();
        for (id, parent, folder) in [
            ("move-page", None, "source-folder"),
            ("move-child", Some("move-page"), "child-folder"),
        ] {
            let path = crate::storage::write_document(
                &source,
                id,
                id,
                "title: Preserved",
                "Preserved canonical body",
            )
            .await
            .unwrap();
            sqlx::query("INSERT INTO documents (id,space_id,folder_id,parent_id,title,content_path,created_by,last_updated_by,published_at,public,public_slug) VALUES ($1,'move-source',$2,$3,$1,$4,'mover','mover','old',1,$1)").bind(id).bind(folder).bind(parent).bind(path).execute(&state.db).await.unwrap();
        }
        for query in [
            "INSERT INTO properties_schema (id,space_id,name,property_type) VALUES ('source-property','move-source','Score','number'),('target-number','move-target','Different name','number'),('target-text','move-target','Score','text')",
            "INSERT INTO property_values (id,document_id,property_id,value) VALUES ('source-value','move-page','source-property','42')",
            "INSERT INTO tags (id,space_id,name) VALUES ('source-tag','move-source','Important'),('target-tag','move-target','Different name')",
            "INSERT INTO doc_tags VALUES ('move-page','source-tag')",
            "INSERT INTO workflow_templates (id,space_id,name,stages,approvers,created_by) VALUES ('source-workflow','move-source','Review','[{\"id\":\"review\",\"name\":\"Review\"},{\"id\":\"done\",\"name\":\"Done\"}]','[]','mover'),('target-workflow','move-target','Other review','[{\"id\":\"review\",\"name\":\"Review\"},{\"id\":\"done\",\"name\":\"Done\"}]','[]','mover')",
            "INSERT INTO document_workflows (id,document_id,workflow_template_id,current_stage_id,created_by) VALUES ('workflow','move-page','source-workflow','review','mover')",
            "INSERT INTO document_workflow_events (id,document_workflow_id,document_id,to_stage_id,action,note,created_by) VALUES ('event','workflow','move-page','review','start','History note','mover')",
            "INSERT INTO doc_versions (id,document_id,version_number,change_set,created_by) VALUES ('version','move-page',1,X'0102','mover')",
            "INSERT INTO document_drafts (id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES ('move-draft','move-page','move-source','mover','Saved draft','Private saved body','old')",
            "INSERT INTO document_draft_access (draft_id,user_id,role,granted_by) VALUES ('move-draft','other-owner','editor','mover')",
        ] {sqlx::query(query).execute(&state.db).await.unwrap();}
        let drawing = crate::storage::space_dir(&source).join("excalidraw/drawing.json");
        tokio::fs::create_dir_all(drawing.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&drawing, b"{\"drawing\":true}")
            .await
            .unwrap();
        let relative = drawing
            .strip_prefix(crate::storage::data_dir())
            .unwrap()
            .to_string_lossy()
            .into_owned();
        sqlx::query("INSERT INTO excalidraw_files (id,document_id,space_id,filename,storage_path,created_by) VALUES ('drawing','move-page','move-source','drawing.json',$1,'mover')").bind(relative).execute(&state.db).await.unwrap();
        {let mut legacy=state.db.acquire().await.unwrap();crate::spaces::asset_audiences::admit_legacy(&mut legacy,"attachment","move-source","move-page").await.unwrap();}
        let attachment = crate::storage::space_dir(&source).join("attachments/file.bin");
        let relative = attachment
            .strip_prefix(crate::storage::data_dir())
            .unwrap()
            .to_string_lossy()
            .into_owned();
        sqlx::query("INSERT INTO attachments (id,document_id,space_id,filename,original_name,mime_type,size_bytes,storage_path,created_by) VALUES ('attachment','move-page','move-source','file.bin','file.bin','application/octet-stream',16,$1,'mover')").bind(relative).execute(&state.db).await.unwrap();
        (state, source, target)
    }
    pub(super) fn request(kind: &str) -> MovePreviewRequest {
        MovePreviewRequest {
            root: MoveRoot {
                kind: kind.into(),
                id: if kind == "folder" {
                    "source-folder"
                } else {
                    "move-page"
                }
                .into(),
            },
            destination: MoveDestination {
                space_id: "move-target".into(),
                folder_id: None,
                parent_id: None,
            },
        }
    }
    async fn without_authorized_directory_catalog(state: &AppState, value: Value) -> Value {
        let mut value = value;
        let inventory = if value.get("inventory").is_some() {
            value.get_mut("inventory").unwrap()
        } else {
            &mut value
        };
        if let Some(catalog) = inventory.get_mut("maintenance_targets") {
            let users: Vec<(String, String)> =
                sqlx::query_as("SELECT id,display_name FROM users WHERE is_active=1 ORDER BY id")
                    .fetch_all(&state.db)
                    .await
                    .unwrap();
            assert_eq!(
                catalog["users"],
                json!(users
                    .into_iter()
                    .map(|(id, name)| json!({"id":id,"name":name,"active":true}))
                    .collect::<Vec<_>>())
            );
            // Only the separately authorized active directory projection is excluded;
            // hidden page/draft/group/history/privacy negatives remain unchanged.
            catalog.as_object_mut().unwrap().remove("users");
        }
        value
    }
    async fn canonical(state: &AppState) -> Vec<(String, Option<String>)> {
        move_database_snapshot(state)
            .await
            .into_iter()
            .filter(|(table, _)| table != "move_previews")
            .collect()
    }
    async fn enable_records(state: &AppState) {
        sqlx::raw_sql(include_str!("../../migrations/0028_move_previews.sql"))
            .execute(&state.db)
            .await
            .unwrap();
    }
    async fn cleanup(slug: &str) {
        match tokio::fs::remove_dir_all(crate::storage::space_dir(slug)).await {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => panic!("cleanup: {error}"),
        }
    }

    #[tokio::test]
    async fn previews_collect_page_folder_closures_and_never_relocate_canonical_data() {
        let (state, source, target) = fixture().await;
        enable_records(&state).await;
        let before = canonical(&state).await;
        let files = move_file_snapshot(&source);
        for kind in ["document", "folder"] {
            let preview = create_preview(
                State(state.clone()),
                test_auth("mover"),
                Path("move-source".into()),
                Json(request(kind)),
            )
            .await
            .unwrap()
            .0;
            assert!(preview.commit_available);
            assert!(!preview.stale);
            assert_eq!(preview.inventory["pages"].as_array().unwrap().len(), 2);
            assert_eq!(preview.inventory["drawings"], 1);
            assert_eq!(preview.inventory["workflow_history_events"], 1);
            assert_eq!(preview.inventory["history_versions"], 1);
            let properties = preview.inventory["metadata"].as_array().unwrap();
            let property = properties
                .iter()
                .find(|entry| entry["kind"] == "property")
                .unwrap();
            assert_eq!(property["candidates"].as_array().unwrap().len(), 1);
            assert_eq!(property["candidates"][0]["id"], "target-number");
            if kind == "folder" {
                assert_eq!(preview.inventory["folders"].as_array().unwrap().len(), 2);
            }
            let reread = get_preview(
                State(state.clone()),
                test_auth("mover"),
                Path(preview.preview_id),
            )
            .await
            .unwrap()
            .0;
            assert!(!reread.stale);
        }
        let mut req = request("folder");
        req.destination.space_id = "move-source".into();
        assert!(review_move(&state, "mover", "move-source", &req)
            .await
            .is_ok());
        assert_eq!(canonical(&state).await, before);
        assert_eq!(move_file_snapshot(&source), files);
        assert!(!crate::storage::space_dir(&target).exists());
        cleanup(&source).await;
    }

    #[tokio::test]
    async fn inaccessible_descendant_suppresses_ids_titles_counts_and_entire_inventory() {
        let (state, source, _) = fixture().await;
        enable_records(&state).await;
        sqlx::query("UPDATE documents SET title='SECRET private title',published_at=NULL WHERE id='move-child'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts (id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES ('SECRET-private-draft','move-child','move-source','other-owner','SECRET draft title','SECRET draft body','old','old','old')").execute(&state.db).await.unwrap();
        let preview = create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(request("folder")),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(preview.inventory, json!({}));
        assert_eq!(preview.blockers, vec!["subtree_unavailable"]);
        assert!(!serde_json::to_string(&preview).unwrap().contains("SECRET"));
        cleanup(&source).await;
    }

    #[tokio::test]
    async fn hidden_private_drafts_are_fingerprinted_but_never_exposed() {
        let (state, source, _) = fixture().await;
        enable_records(&state).await;
        sqlx::query("INSERT INTO document_drafts (id,document_id,space_id,owner_id,title,content,base_updated_at,created_at,updated_at) VALUES ('SECRET-draft','move-page','move-source','other-owner','SECRET title','SECRET content','old','old','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access (draft_id,user_id,role,granted_by) VALUES ('SECRET-draft','SECRET-recipient','viewer','mover')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE doc_versions SET change_set=$1")
            .bind(b"SECRET version body".as_slice())
            .execute(&state.db)
            .await
            .unwrap();
        let created = create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(request("document")),
        )
        .await
        .unwrap()
        .0;
        assert!(!without_authorized_directory_catalog(
            &state,
            serde_json::to_value(&created).unwrap()
        )
        .await
        .to_string()
        .contains("SECRET"));
        let saved: (String, String) =
            sqlx::query_as("SELECT manifest_json,request_json FROM move_previews WHERE id=$1")
                .bind(&created.preview_id)
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert!(
            !without_authorized_directory_catalog(&state, serde_json::from_str(&saved.0).unwrap())
                .await
                .to_string()
                .contains("SECRET")
                && !saved.1.contains("SECRET")
        );
        assert!(!saved.0.contains("binary_hex"));

        sqlx::query(
            "UPDATE document_drafts SET content='SECRET changed content' WHERE id='SECRET-draft'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let changed = get_preview(
            State(state.clone()),
            test_auth("mover"),
            Path(created.preview_id),
        )
        .await
        .unwrap()
        .0;
        assert!(changed.stale);
        assert!(!without_authorized_directory_catalog(
            &state,
            serde_json::to_value(&changed).unwrap()
        )
        .await
        .to_string()
        .contains("SECRET"));
        cleanup(&source).await;
    }

    #[tokio::test]
    async fn preview_identity_expiry_cancellation_and_revalidation_protect_saved_inventory() {
        let (state, source, _) = fixture().await;
        enable_records(&state).await;
        let created = create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(request("document")),
        )
        .await
        .unwrap()
        .0;
        assert!(get_preview(
            State(state.clone()),
            test_auth("admin-mover"),
            Path(created.preview_id.clone())
        )
        .await
        .is_err());
        assert!(cancel_preview(
            State(state.clone()),
            test_auth("admin-mover"),
            Path(created.preview_id.clone())
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO space_user_blocks (space_id,user_id,created_by) VALUES ('move-source','mover','admin-mover')").execute(&state.db).await.unwrap();
        assert!(get_preview(
            State(state.clone()),
            test_auth("mover"),
            Path(created.preview_id.clone())
        )
        .await
        .is_err());
        sqlx::query("DELETE FROM space_user_blocks")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            cancel_preview(
                State(state.clone()),
                test_auth("mover"),
                Path(created.preview_id.clone())
            )
            .await
            .unwrap(),
            StatusCode::NO_CONTENT
        );
        assert!(get_preview(
            State(state.clone()),
            test_auth("mover"),
            Path(created.preview_id)
        )
        .await
        .is_err());
        let another = create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(request("document")),
        )
        .await
        .unwrap()
        .0;
        sqlx::query("UPDATE move_previews SET expires_at='2000-01-01T00:00:00Z'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(get_preview(
            State(state.clone()),
            test_auth("mover"),
            Path(another.preview_id)
        )
        .await
        .is_err());
        cleanup(&source).await;
    }

    #[tokio::test]
    async fn fingerprints_detect_values_definitions_history_assets_grants_and_audience_drift() {
        let (state, source, _) = fixture().await;
        let req = request("document");
        let mut previous = review_move(&state, "mover", "move-source", &req)
            .await
            .unwrap()
            .fingerprint;
        for mutation in [
            "UPDATE property_values SET value='43'", "UPDATE properties_schema SET options='[]' WHERE id='source-property'",
            "UPDATE document_workflow_events SET note='Changed history'", "UPDATE document_draft_access SET role='viewer'",
            "UPDATE documents SET public=0 WHERE id='move-page'", "UPDATE documents SET locked=1,locked_by='mover' WHERE id='move-page'",
            "UPDATE space_members SET role='viewer' WHERE space_id='move-target' AND user_id='mover'",
        ] {
            if mutation.contains("space_members") { // Keep the actor authorized; change another audience member instead.
                sqlx::query("INSERT INTO space_members (id,space_id,user_id,role) VALUES ('other-membership','move-source','admin-mover','viewer')").execute(&state.db).await.unwrap();
                sqlx::query("UPDATE users SET role='member' WHERE id='admin-mover'").execute(&state.db).await.unwrap();
            } else {sqlx::query(mutation).execute(&state.db).await.unwrap();}
            let current=review_move(&state,"mover","move-source",&req).await.unwrap().fingerprint;assert_ne!(current,previous);previous=current;
        }
        let drawing = crate::storage::space_dir(&source).join("excalidraw/drawing.json");
        tokio::fs::write(drawing, b"changed drawing").await.unwrap();
        assert_ne!(
            review_move(&state, "mover", "move-source", &req)
                .await
                .unwrap()
                .fingerprint,
            previous
        );
        cleanup(&source).await;
    }

    #[tokio::test]
    async fn external_relation_and_hierarchy_conflicts_are_visible_only_when_authorized() {
        let (state, source, _) = fixture().await;
        sqlx::query("INSERT INTO properties_schema (id,space_id,name,property_type,options) VALUES ('relation','move-source','Related','relation',NULL)").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO documents (id,space_id,title,created_by,last_updated_by,created_at,updated_at,published_at,parent_id,content_path) VALUES ('SECRET-outside','move-target','SECRET outside title','mover','mover','old','old',NULL,'move-page','')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO property_values (id,document_id,property_id,value) VALUES ('outgoing','move-page','relation','SECRET-outside'),('incoming','SECRET-outside','relation','move-page')").execute(&state.db).await.unwrap();
        let review = review_move(&state, "mover", "move-source", &request("document"))
            .await
            .unwrap();
        assert!(review
            .blockers
            .contains(&"external_relations_require_resolution".into()));
        assert!(review
            .blockers
            .contains(&"cross_boundary_hierarchy_requires_resolution".into()));
        assert_eq!(review.inventory["relations"], json!([]));
        assert!(
            !without_authorized_directory_catalog(&state, review.inventory.clone())
                .await
                .to_string()
                .contains("SECRET")
        );
        cleanup(&source).await;
    }

    #[tokio::test]
    async fn preview_storage_checks_reject_escape_and_symlink_without_reading_or_writing() {
        let (state, source, _) = fixture().await;
        let before = move_file_snapshot(&source);
        sqlx::query("UPDATE attachments SET storage_path='../outside-secret'")
            .execute(&state.db)
            .await
            .unwrap();
        let review = review_move(&state, "mover", "move-source", &request("document"))
            .await
            .unwrap();
        assert!(review.blockers.contains(&"storage_unavailable".into()));
        #[cfg(unix)]
        {
            let link = crate::storage::space_dir(&source).join("attachments/escape");
            std::os::unix::fs::symlink("/etc/passwd", &link).unwrap();
            let relative = link
                .strip_prefix(crate::storage::data_dir())
                .unwrap()
                .to_string_lossy()
                .into_owned();
            sqlx::query("UPDATE attachments SET storage_path=$1")
                .bind(relative)
                .execute(&state.db)
                .await
                .unwrap();
            let review = review_move(&state, "mover", "move-source", &request("document"))
                .await
                .unwrap();
            assert!(review.blockers.contains(&"storage_unavailable".into()));
            tokio::fs::remove_file(link).await.unwrap();
        }
        assert_eq!(move_file_snapshot(&source), before);
        cleanup(&source).await;
    }
    #[tokio::test]
    async fn placement_and_external_relation_endpoint_changes_stale_the_preview() {
        let (state, source, _) = fixture().await;
        enable_records(&state).await;
        let mut req = request("document");
        req.destination.folder_id = Some("target-folder".into());
        let created = create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(req),
        )
        .await
        .unwrap()
        .0;
        sqlx::query("UPDATE folders SET name='Target changed' WHERE id='target-folder'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            get_preview(
                State(state.clone()),
                test_auth("mover"),
                Path(created.preview_id)
            )
            .await
            .unwrap()
            .0
            .stale
        );
        sqlx::query("INSERT INTO documents (id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES ('outside-page','move-source','Outside','','mover','mover','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO properties_schema (id,space_id,name,property_type) VALUES ('relation','move-source','Related','relation')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO property_values (id,document_id,property_id,value) VALUES ('outgoing','move-page','relation','outside-page')").execute(&state.db).await.unwrap();
        for mutation in [
            "UPDATE documents SET archived_at='old' WHERE id='outside-page'",
            "UPDATE documents SET space_id='move-target' WHERE id='outside-page'",
            "UPDATE documents SET parent_id='move-child' WHERE id='outside-page'",
        ] {
            let preview = create_preview(
                State(state.clone()),
                test_auth("mover"),
                Path("move-source".into()),
                Json(request("document")),
            )
            .await
            .unwrap()
            .0;
            sqlx::query(mutation).execute(&state.db).await.unwrap();
            assert!(
                get_preview(
                    State(state.clone()),
                    test_auth("mover"),
                    Path(preview.preview_id)
                )
                .await
                .unwrap()
                .0
                .stale
            );
        }
        cleanup(&source).await;
    }

    #[tokio::test]
    async fn workflow_history_stage_and_approver_bindings_control_candidates() {
        let (state, source, _) = fixture().await;
        let inventory = review_move(&state, "mover", "move-source", &request("document"))
            .await
            .unwrap()
            .inventory;
        let candidates = |inventory: &Value| {
            inventory["metadata"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["kind"] == "workflow")
                .unwrap()["candidates"]
                .as_array()
                .unwrap()
                .len()
        };
        assert_eq!(candidates(&inventory), 1);
        sqlx::query("UPDATE document_workflow_events SET from_stage_id='missing-stage'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            candidates(
                &review_move(&state, "mover", "move-source", &request("document"))
                    .await
                    .unwrap()
                    .inventory
            ),
            0
        );
        sqlx::query("UPDATE document_workflow_events SET from_stage_id=NULL")
            .execute(&state.db)
            .await
            .unwrap();
        let approver="[{\"approver_type\":\"user\",\"approver_id\":\"other-owner\",\"stage_id\":\"review\"}]";
        sqlx::query("UPDATE workflow_templates SET approvers=$1")
            .bind(approver)
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            candidates(
                &review_move(&state, "mover", "move-source", &request("document"))
                    .await
                    .unwrap()
                    .inventory
            ),
            0
        );
        cleanup(&source).await;
    }
    #[tokio::test]
    async fn malformed_workflow_and_inactive_user_candidates_are_not_advertised() {
        let (state, source, _) = fixture().await;
        let candidate_count = |inventory: &Value, kind: &str| {
            inventory["metadata"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["kind"] == kind)
                .unwrap()["candidates"]
                .as_array()
                .unwrap()
                .len()
        };
        sqlx::query("UPDATE workflow_templates SET approvers='not-json'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            candidate_count(
                &review_move(&state, "mover", "move-source", &request("document"))
                    .await
                    .unwrap()
                    .inventory,
                "workflow"
            ),
            0
        );
        sqlx::query("UPDATE workflow_templates SET approvers='[]'")
            .execute(&state.db)
            .await
            .unwrap();
        for stages in [
            "[{\"id\":\"review\",\"name\":\"Review\"}]",
            "[42,43]",
            "[{\"id\":\"review\",\"name\":\"\"},{\"id\":\"done\",\"name\":\"Done\"}]",
        ] {
            sqlx::query("UPDATE workflow_templates SET stages=$1")
                .bind(stages)
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(
                candidate_count(
                    &review_move(&state, "mover", "move-source", &request("document"))
                        .await
                        .unwrap()
                        .inventory,
                    "workflow"
                ),
                0
            );
        }
        sqlx::query("UPDATE properties_schema SET property_type='user' WHERE id IN ('source-property','target-number')").execute(&state.db).await.unwrap();
        for user in ["missing-account", "other-owner"] {
            sqlx::query("UPDATE property_values SET value=$1")
                .bind(user)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("UPDATE users SET is_active=0 WHERE id='other-owner'")
                .execute(&state.db)
                .await
                .unwrap();
            assert_eq!(
                candidate_count(
                    &review_move(&state, "mover", "move-source", &request("document"))
                        .await
                        .unwrap()
                        .inventory,
                    "property"
                ),
                0
            );
        }
        sqlx::query("UPDATE users SET is_active=1 WHERE id='other-owner'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            candidate_count(
                &review_move(&state, "mover", "move-source", &request("document"))
                    .await
                    .unwrap()
                    .inventory,
                "property"
            ),
            1
        );
        cleanup(&source).await;
    }

    #[tokio::test]
    async fn root_placement_accepts_deep_pages_and_folders_but_retains_parent_folder_checks() {
        let (state, source, _) = fixture().await;
        for (id, parent) in [
            ("third-folder", "child-folder"),
            ("fourth-folder", "third-folder"),
        ] {
            sqlx::query(
                "INSERT INTO folders (id,space_id,name,parent_id) VALUES ($1,'move-source',$1,$2)",
            )
            .bind(id)
            .bind(parent)
            .execute(&state.db)
            .await
            .unwrap();
        }
        assert!(
            !review_move(&state, "mover", "move-source", &request("folder"))
                .await.unwrap().blockers.contains(&"destination_hierarchy_invalid".into())
        );
        for (id, parent) in [("third-page", "move-child"), ("fourth-page", "third-page")] {
            sqlx::query("INSERT INTO documents (id,space_id,title,content_path,created_by,last_updated_by,published_at,parent_id) VALUES ($1,'move-source',$1,'','mover','mover','old',$2)").bind(id).bind(parent).execute(&state.db).await.unwrap();
        }
        assert!(
            !review_move(&state, "mover", "move-source", &request("document"))
                .await
                .unwrap()
                .blockers
                .contains(&"destination_hierarchy_invalid".into())
        );
        sqlx::query("DELETE FROM documents WHERE id IN ('third-page','fourth-page')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO folders (id,space_id,name) VALUES ('other-target-folder','move-target','Other target')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO documents (id,space_id,title,content_path,created_by,last_updated_by,published_at,parent_id,folder_id) VALUES ('target-root','move-target','Root','','mover','mover','old',NULL,'other-target-folder'),('target-child','move-target','Child','','mover','mover','old','target-root','target-folder')").execute(&state.db).await.unwrap();
        let mut req = request("document");
        req.root.id = "move-child".into();
        req.destination.folder_id = Some("target-folder".into());
        req.destination.parent_id = Some("target-child".into());
        assert!(review_move(&state, "mover", "move-source", &req)
            .await
            .unwrap()
            .blockers
            .contains(&"destination_hierarchy_invalid".into()));
        cleanup(&source).await;
    }
    async fn key(state: &AppState, id: &str, scope: &str) -> AuthUser {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'mover',$1,$2,$1,$3)")
            .bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
        let mut auth = test_auth("mover");
        auth.claims.sid = format!("api-key:{id}");
        auth.api_key_scopes = Some(vec![scope.into()]);
        auth
    }
    async fn read_token(state: &AppState, id: &str) -> Result<Json<MovePreviewResponse>, AppError> {
        get_preview(State(state.clone()), test_auth("mover"), Path(id.into())).await
    }
    async fn create_token(state: &AppState) -> Result<Json<MovePreviewResponse>, AppError> {
        create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(request("document")),
        )
        .await
    }
    #[tokio::test]
    async fn current_preview_http_key_method_ceiling_and_read_no_writes() {
        use tower::ServiceExt;
        let (state, source, target) = fixture().await;
        for (id, scope) in [
            ("preview-read", "read"),
            ("preview-write", "write"),
            ("preview-admin", "admin"),
        ] {
            let _ = key(&state, id, scope).await;
        }
        let before = canonical(&state).await;
        let files = move_file_snapshot(&source);
        let post = |id: &str| {
            axum::http::Request::builder()
                .method("POST")
                .uri("/spaces/move-source/moves/preview")
                .header("X-API-Key", id)
                .header("content-type", "application/json")
                .body(axum::body::Body::from(
                    serde_json::to_vec(&request("document")).unwrap(),
                ))
                .unwrap()
        };
        let denied = router()
            .with_state(state.clone())
            .oneshot(post("preview-read"))
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);
        let allowed = router()
            .with_state(state.clone())
            .oneshot(post("preview-write"))
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::OK);
        let body: Value = serde_json::from_slice(
            &axum::body::to_bytes(allowed.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        let id = body["preview_id"].as_str().unwrap();
        for (key, status) in [
            ("preview-read", StatusCode::OK),
            ("preview-write", StatusCode::FORBIDDEN),
            ("preview-admin", StatusCode::OK),
        ] {
            let request = axum::http::Request::builder()
                .uri(format!("/moves/previews/{id}"))
                .header("X-API-Key", key)
                .body(axum::body::Body::empty())
                .unwrap();
            let response = router()
                .with_state(state.clone())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(response.status(), status, "{key}");
        }
        // HTTP key bookkeeping is the only canonical change permitted by middleware.
        sqlx::query("UPDATE api_keys SET last_used_at=NULL")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(canonical(&state).await, before);
        assert_eq!(move_file_snapshot(&source), files);
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn current_preview_exact_move_union_private_ancestor_and_lock_blockers() {
        let (state, source, target) = fixture().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('source-move','move-source','Custom','[\"view\",\"move_content\"]','mover'),('target-move','move-target','Custom','[\"view\",\"move_content\"]','mover')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role=CASE space_id WHEN 'move-source' THEN 'source-move' ELSE 'target-move' END WHERE user_id='mover'").execute(&state.db).await.unwrap();
        assert!(create_token(&state).await.is_ok());
        sqlx::query(
            "UPDATE space_roles SET permissions='[\"view\",\"comment\"]' WHERE id='target-move'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert!(create_token(&state).await.is_err());
        sqlx::query("UPDATE space_roles SET permissions='[\"view\",\"move_content\"]' WHERE id='target-move'").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='move-page'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE document_drafts SET owner_id='other-owner' WHERE id='move-draft'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(create_token(&state).await.is_err());
        let mut child_request = request("document");
        child_request.root.id = "move-child".into();
        assert!(create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(child_request.clone())
        )
        .await
        .is_err());
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('move-draft','mover','viewer','other-owner')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='other-owner' WHERE id='move-child'")
            .execute(&state.db)
            .await
            .unwrap();
        let shared = create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(child_request),
        )
        .await
        .unwrap()
        .0;
        assert!(!shared.inventory["pages"].as_array().unwrap().is_empty());
        assert!(shared.blockers.iter().any(|s| s.contains("lock")));
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn current_preview_late_insert_drift_rolls_back_record_and_canonical_state() {
        for change in [
            "DELETE FROM sessions WHERE id='preview-mover'",
            "UPDATE documents SET title='TRIGGER SECRET' WHERE id='move-child'",
            "UPDATE move_previews SET actor_id='other-owner' WHERE id=NEW.id",
        ] {
            let (state, source, target) = fixture().await;
            sqlx::query(&format!(
                "CREATE TRIGGER preview_late AFTER INSERT ON move_previews BEGIN {change}; END"
            ))
            .execute(&state.db)
            .await
            .unwrap();
            let before = move_database_snapshot(&state).await;
            let files = move_file_snapshot(&source);
            assert!(create_token(&state).await.is_err(), "{change}");
            assert_eq!(move_database_snapshot(&state).await, before, "{change}");
            assert_eq!(move_file_snapshot(&source), files);
            cleanup(&source).await;
            cleanup(&target).await;
        }
    }
    #[tokio::test]
    async fn current_preview_cancel_own_record_after_policy_loss_but_not_revoked_identity() {
        let (state, source, target) = fixture().await;
        let id = create_token(&state).await.unwrap().0.preview_id;
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('move-source','mover','other-owner')").execute(&state.db).await.unwrap();
        assert_eq!(
            cancel_preview(State(state.clone()), test_auth("mover"), Path(id.clone()))
                .await
                .unwrap(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            cancel_preview(State(state.clone()), test_auth("mover"), Path(id.clone()))
                .await
                .unwrap(),
            StatusCode::NO_CONTENT
        );
        sqlx::query("DELETE FROM sessions WHERE id='preview-mover'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            cancel_preview(State(state.clone()), test_auth("mover"), Path(id))
                .await
                .is_err()
        );
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn current_preview_external_wal_source_record_and_credential_drift() {
        for statement in [
            "DELETE FROM sessions WHERE id='preview-mover'",
            "UPDATE documents SET title='WAL SECRET' WHERE id='move-child'",
            "UPDATE properties_schema SET options='[\"WAL SECRET\"]' WHERE id='source-property'",
            "UPDATE move_previews SET cancelled_at='now'",
            "UPDATE document_draft_access SET role='viewer' WHERE draft_id='move-draft'",
        ] {
            let (state, source, target) = fixture_with_wal(true).await;
            let id = create_token(&state).await.unwrap().0.preview_id;
            let db = state.db.clone();
            AFTER_INVENTORY.with(|hook| {
                *hook.borrow_mut() = Some(Box::pin(async move {
                    sqlx::query(statement).execute(&db).await.unwrap();
                }))
            });
            assert!(read_token(&state, &id).await.is_err(), "{statement}");
            let record: Option<String> =
                sqlx::query_scalar("SELECT manifest_json FROM move_previews WHERE id=$1")
                    .bind(&id)
                    .fetch_optional(&state.db)
                    .await
                    .unwrap();
            let record = record.unwrap();
            assert!(!record.contains("Private saved body"));
            assert!(!record.contains("WAL SECRET"));
            cleanup(&source).await;
            cleanup(&target).await;
        }
    }

    #[tokio::test]
    async fn current_preview_stored_key_scope_account_and_session_are_not_claims() {
        for statement in [
            "UPDATE api_keys SET scopes='[\"write\"]' WHERE id='current-read'",
            "DELETE FROM api_keys WHERE id='current-read'",
            "UPDATE users SET is_active=0 WHERE id='mover'",
            "UPDATE api_keys SET user_id='other-owner' WHERE id='current-read'",
            "UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z' WHERE id='current-read'",
        ] {
            let (state, source, target) = fixture().await;
            let id = create_token(&state).await.unwrap().0.preview_id;
            let auth = key(&state, "current-read", "read").await;
            sqlx::query(statement).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(
                get_preview(State(state.clone()), auth, Path(id))
                    .await
                    .is_err(),
                "{statement}"
            );
            assert_eq!(move_database_snapshot(&state).await, before);
            cleanup(&source).await;
            cleanup(&target).await;
        }
        let (state, source, target) = fixture().await;
        let id = create_token(&state).await.unwrap().0.preview_id;
        let mut auth = test_auth("mover");
        auth.claims.exp = 0;
        assert!(get_preview(State(state.clone()), auth, Path(id))
            .await
            .is_err());
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn current_preview_cancel_late_record_or_identity_changes_roll_back() {
        for statement in [
            "DELETE FROM sessions WHERE id='preview-mover'",
            "UPDATE move_previews SET request_json='{}' WHERE id=NEW.id",
        ] {
            let (state, source, target) = fixture().await;
            let id = create_token(&state).await.unwrap().0.preview_id;
            sqlx::query(&format!("CREATE TRIGGER cancellation_late AFTER UPDATE OF cancelled_at ON move_previews BEGIN {statement}; END")).execute(&state.db).await.unwrap();
            let before = move_database_snapshot(&state).await;
            assert!(
                cancel_preview(State(state.clone()), test_auth("mover"), Path(id))
                    .await
                    .is_err()
            );
            assert_eq!(move_database_snapshot(&state).await, before);
            cleanup(&source).await;
            cleanup(&target).await;
        }
    }

    #[tokio::test]
    async fn current_preview_external_wal_private_ancestor_share_revocation() {
        let (state, source, target) = fixture_with_wal(true).await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='move-page'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE document_drafts SET owner_id='other-owner' WHERE id='move-draft'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('move-draft','mover','viewer','other-owner')").execute(&state.db).await.unwrap();
        let mut req = request("document");
        req.root.id = "move-child".into();
        let id = create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(req),
        )
        .await
        .unwrap()
        .0
        .preview_id;
        let db = state.db.clone();
        AFTER_INVENTORY.with(|hook|*hook.borrow_mut()=Some(Box::pin(async move{sqlx::query("DELETE FROM document_draft_access WHERE draft_id='move-draft' AND user_id='mover'").execute(&db).await.unwrap();})));
        assert!(read_token(&state, &id).await.is_err());
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn current_preview_final_raw_and_record_wal_waits_recheck_key_and_session() {
        for final_record in [false, true] {
            for revoke_key in [false, true] {
                let (state, source, target) = fixture_with_wal(true).await;
                let id = create_token(&state).await.unwrap().0.preview_id;
                let auth = if revoke_key {
                    key(&state, "last-read", "read").await
                } else {
                    test_auth("mover")
                };
                let db = state.db.clone();
                let hook = Box::pin(async move {
                    let sql = if revoke_key {
                        "UPDATE api_keys SET scopes='[\"write\"]' WHERE id='last-read'"
                    } else {
                        "DELETE FROM sessions WHERE id='preview-mover'"
                    };
                    sqlx::query(sql).execute(&db).await.unwrap();
                });
                if final_record {
                    AFTER_FINAL_RECORD.with(|slot| *slot.borrow_mut() = Some(hook));
                } else {
                    AFTER_RAW_PROOFS.with(|slot| *slot.borrow_mut() = Some(hook));
                }
                assert!(
                    get_preview(State(state.clone()), auth, Path(id.clone()))
                        .await
                        .is_err(),
                    "record={final_record} key={revoke_key}"
                );
                let cancelled: Option<String> =
                    sqlx::query_scalar("SELECT cancelled_at FROM move_previews WHERE id=$1")
                        .bind(id)
                        .fetch_one(&state.db)
                        .await
                        .unwrap();
                assert_eq!(cancelled, None);
                cleanup(&source).await;
                cleanup(&target).await;
            }
        }
    }
    #[tokio::test]
    async fn current_preview_create_final_record_revocation_rolls_back_staged_record() {
        for revoke_key in [false, true] {
            let (state, source, target) = fixture().await;
            let auth = if revoke_key {
                key(&state, "last-write", "write").await
            } else {
                test_auth("mover")
            };
            let before = move_database_snapshot(&state).await;
            FINAL_RECORD_SQL.with(|slot| {
                *slot.borrow_mut() = Some(if revoke_key {
                    "UPDATE api_keys SET scopes='[\"read\"]' WHERE id='last-write'"
                } else {
                    "DELETE FROM sessions WHERE id='preview-mover'"
                })
            });
            assert!(create_preview(
                State(state.clone()),
                auth,
                Path("move-source".into()),
                Json(request("document"))
            )
            .await
            .is_err());
            assert_eq!(move_database_snapshot(&state).await, before);
            cleanup(&source).await;
            cleanup(&target).await;
        }
    }
    #[tokio::test]
    async fn current_preview_cancel_final_record_revocation_rolls_back_staged_cancellation() {
        let (state, source, target) = fixture().await;
        let id = create_token(&state).await.unwrap().0.preview_id;
        let before = move_database_snapshot(&state).await;
        FINAL_RECORD_SQL.with(|slot| {
            *slot.borrow_mut() = Some("DELETE FROM sessions WHERE id='preview-mover'")
        });
        assert!(
            cancel_preview(State(state.clone()), test_auth("mover"), Path(id))
                .await
                .is_err()
        );
        assert_eq!(move_database_snapshot(&state).await, before);
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn current_preview_restriction_recipient_and_revision_wal_drift_is_not_actor_only() {
        for revision_change in [false, true] {
            let (state, source, target) = fixture_with_wal(true).await;
            sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('move-page','move-source','restricted','original','mover')").execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('move-actor-allow','move-page','move-source','mover','mover'),('move-other-allow','move-page','move-source','other-owner','mover')").execute(&state.db).await.unwrap();
            let id = create_token(&state).await.unwrap().0.preview_id;
            let fingerprint: String =
                sqlx::query_scalar("SELECT fingerprint FROM move_previews WHERE id=$1")
                    .bind(&id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            let db = state.db.clone();
            AFTER_INVENTORY.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
                sqlx::query(if revision_change {"UPDATE document_access_policies SET revision='external-revision' WHERE document_id='move-page'"} else {"DELETE FROM document_access_allowlist WHERE id='move-other-allow'"}).execute(&db).await.unwrap();
            })));
            assert!(
                read_token(&state, &id).await.is_err(),
                "recipient-only change must not acknowledge collected inventory"
            );
            let same: String =
                sqlx::query_scalar("SELECT fingerprint FROM move_previews WHERE id=$1")
                    .bind(&id)
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(same, fingerprint);
            // Actor participation remains valid; a fresh review exposes only a changed token.
            let fresh = read_token(&state, &id).await.unwrap().0;
            assert!(fresh.stale);
            let replacement = create_token(&state).await.unwrap().0;
            assert_ne!(replacement.fingerprint, fingerprint);
            cleanup(&source).await;
            cleanup(&target).await;
        }
    }
    #[tokio::test]
    async fn reviewed_choice_projection_contains_only_authorized_scalars_and_exact_create_capability(
    ) {
        let (state, source, target) = fixture().await;
        sqlx::query("UPDATE space_members SET role='owner' WHERE space_id='move-target' AND user_id='mover'").execute(&state.db).await.unwrap();
        let before = canonical(&state).await;
        let prepared = review_move(&state, "mover", "move-source", &request("document"))
            .await
            .unwrap();
        let metadata = prepared.inventory["metadata"].as_array().unwrap();
        let property = metadata.iter().find(|m| m["kind"] == "property").unwrap();
        assert_eq!(
            property["assignments"],
            json!([{"id":"source-value","document_id":"move-page","value":"42"}])
        );
        assert_eq!(property["create_allowed"], true);
        let workflow = metadata.iter().find(|m| m["kind"] == "workflow").unwrap();
        assert_eq!(workflow["create_allowed"], true);
        assert!(workflow["candidates"][0]["choice_details"]["stages"].is_array());
        assert!(workflow["candidates"][0].get("created_by").is_none());
        assert!(!prepared
            .inventory
            .to_string()
            .contains("Private saved body"));
        assert!(
            !without_authorized_directory_catalog(&state, prepared.inventory.clone())
                .await
                .to_string()
                .contains("SECRET-recipient")
        );
        assert_eq!(canonical(&state).await, before);
        // Existing space editor can create tags/properties, never owner-only workflow definitions.
        sqlx::query("UPDATE space_members SET role='editor' WHERE space_id='move-target' AND user_id='mover'").execute(&state.db).await.unwrap();
        let current = review_move(&state, "mover", "move-source", &request("document"))
            .await
            .unwrap();
        let items = current.inventory["metadata"].as_array().unwrap();
        assert_eq!(
            items.iter().find(|m| m["kind"] == "property").unwrap()["create_allowed"],
            true
        );
        assert_eq!(
            items.iter().find(|m| m["kind"] == "workflow").unwrap()["create_allowed"],
            false
        );
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn fresh_relation_target_choices_use_exact_private_title_write_key_and_final_wal_share_proof(
    ) {
        let (state, source, target) = fixture_with_wal(true).await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('private-target','move-target','UNSHARED CANONICAL TITLE','','other-owner','other-owner'),('hidden-target','move-target','HIDDEN TARGET TITLE','','other-owner','other-owner')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('selected-target-draft','private-target','move-target','other-owner','Shared target title','PRIVATE TARGET BODY','old','2090'),('alternative-target-draft','private-target','move-target','other-owner','Other shared target','OTHER PRIVATE BODY','old','2080')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('selected-target-draft','mover','viewer','other-owner'),('alternative-target-draft','mover','viewer','other-owner')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO properties_schema(id,space_id,name,property_type) VALUES('relation-source','move-source','Related','relation'),('relation-destination','move-target','Destination related','relation')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('visible-edge','move-page','relation-source','private-target')").execute(&state.db).await.unwrap();
        let auth = key(&state, "target-choice-key", "write").await;
        let preview = create_preview(
            State(state.clone()),
            AuthUser {
                claims: auth.claims.clone(),
                api_key_scopes: auth.api_key_scopes.clone(),
            },
            Path("move-source".into()),
            Json(request("document")),
        )
        .await
        .unwrap()
        .0;
        assert_eq!(
            preview.inventory["relation_targets"],
            json!([{"id":"private-target","title":"Shared target title"}])
        );
        let raw = preview.inventory.to_string();
        for secret in [
            "UNSHARED CANONICAL TITLE",
            "HIDDEN TARGET TITLE",
            "hidden-target",
            "PRIVATE TARGET BODY",
            "selected-target-draft",
        ] {
            assert!(!raw.contains(secret), "{secret}");
        }
        let relation = preview.inventory["metadata"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["source_definition"]["id"] == "relation-source")
            .unwrap();
        assert_eq!(relation["candidates"], json!([]));
        assert_eq!(
            relation["resolution_candidates"][0]["compatibility"],
            "requires_explicit_relation_targets"
        );
        assert_eq!(relation["assignments"], json!([]));
        let db = state.db.clone();
        AFTER_INVENTORY.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move{sqlx::query("DELETE FROM document_draft_access WHERE draft_id='selected-target-draft' AND user_id='mover'").execute(&db).await.unwrap();})));
        assert!(create_preview(
            State(state.clone()),
            auth,
            Path("move-source".into()),
            Json(request("document"))
        )
        .await
        .is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM move_previews")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        let fresh = create_token(&state).await.unwrap().0;
        assert_eq!(
            fresh.inventory["relation_targets"][0]["title"],
            "Other shared target"
        );
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn source_subtree_private_titles_use_exact_readable_drafts_and_final_wal_share_proof() {
        let (state, source, target) = fixture_with_wal(true).await;
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id='move-draft'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("DELETE FROM document_drafts WHERE id='move-draft'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL,title='SECRET canonical source' WHERE id IN ('move-page','move-child')").execute(&state.db).await.unwrap();
        for (page, id, title) in [
            ("move-page", "source-root-share", "Shared root"),
            ("move-child", "source-child-share", "Shared child"),
        ] {
            sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES($1,$2,'move-source','other-owner',$3,'SECRET private body','old','2090')").bind(id).bind(page).bind(title).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'mover','editor','other-owner')").bind(id).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('alternative-child','move-child','move-source','other-owner','Alternative child','SECRET alternate body','old','2080')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('alternative-child','mover','editor','other-owner')").execute(&state.db).await.unwrap();
        let preview = create_token(&state).await.unwrap().0;
        let pages = preview.inventory["pages"].as_array().unwrap();
        assert_eq!(
            pages.iter().find(|p| p["id"] == "move-page").unwrap()["title"],
            "Shared root"
        );
        assert_eq!(
            pages.iter().find(|p| p["id"] == "move-child").unwrap()["title"],
            "Shared child"
        );
        assert!(
            !without_authorized_directory_catalog(&state, preview.inventory.clone())
                .await
                .to_string()
                .contains("SECRET")
        );
        assert!(!preview.inventory.to_string().contains("source-child-share"));
        let db = state.db.clone();
        AFTER_INVENTORY.with(|slot| *slot.borrow_mut() = Some(Box::pin(async move {
            sqlx::query("DELETE FROM document_draft_access WHERE draft_id='source-child-share' AND user_id='mover'").execute(&db).await.unwrap();
        })));
        assert!(create_token(&state).await.is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM move_previews")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        let fresh = create_token(&state).await.unwrap().0;
        assert_eq!(
            fresh.inventory["pages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == "move-child")
                .unwrap()["title"],
            "Alternative child"
        );
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn identifier_preview_binds_hidden_archive_collision_keys_after_wal_wait_without_labels()
    {
        let (state, source, target) = fixture_with_wal(true).await;
        sqlx::query("UPDATE documents SET human_identifier='Visible-1',human_identifier_key='VISIBLE-1' WHERE id='move-page'").execute(&state.db).await.unwrap();
        let preview = create_token(&state).await.unwrap().0;
        let page = preview.inventory["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "move-page")
            .unwrap();
        assert_eq!(page["human_identifier"], "Visible-1");
        assert_eq!(page["identifier_resolution_required"], false);
        let mut same_space = request("document");
        same_space.destination.space_id = "move-source".into();
        let same = create_preview(
            State(state.clone()),
            test_auth("mover"),
            Path("move-source".into()),
            Json(same_space.clone()),
        )
        .await
        .unwrap()
        .0;
        let own = same.inventory["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "move-page")
            .unwrap();
        assert_eq!(own["human_identifier"], "Visible-1");
        assert_eq!(own["identifier_resolution_required"], false);
        sqlx::query("UPDATE documents SET locked=1,locked_by='other-owner' WHERE id='move-page'")
            .execute(&state.db)
            .await
            .unwrap();
        for req in [same_space, request("document")] {
            let locked = create_preview(
                State(state.clone()),
                test_auth("mover"),
                Path("move-source".into()),
                Json(req),
            )
            .await
            .unwrap()
            .0;
            assert!(locked.blockers.iter().any(|s| s.contains("lock")));
            let own = locked.inventory["pages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == "move-page")
                .unwrap();
            assert_eq!(own["identifier_edit_allowed"], false);
        }
        sqlx::query("UPDATE documents SET locked=0,locked_by=NULL WHERE id='move-page'")
            .execute(&state.db)
            .await
            .unwrap();
        let db = state.db.clone();
        AFTER_INVENTORY.with(|slot|*slot.borrow_mut()=Some(Box::pin(async move {
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,human_identifier,human_identifier_key,deleted_at) VALUES('SECRET-holder','move-target','SECRET title','','other-owner','other-owner','visible-1','VISIBLE-1','old')").execute(&db).await.unwrap();
        })));
        assert!(create_token(&state).await.is_err());
        let fresh = create_token(&state).await.unwrap().0;
        assert_eq!(
            fresh.inventory["pages"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["id"] == "move-page")
                .unwrap()["identifier_resolution_required"],
            true
        );
        assert!(
            !without_authorized_directory_catalog(&state, fresh.inventory.clone())
                .await
                .to_string()
                .contains("SECRET")
        );
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn maintenance_preview_foreign_team_requires_resolution_and_fk_clear_is_fresh_without_timestamp_change(
    ) {
        let (state, source, target) = fixture_with_wal(true).await;
        sqlx::query("UPDATE space_members SET role='owner' WHERE user_id='mover'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name,managed_space_id) VALUES('source-maintainers','Source team','move-source'),('destination-maintainers','Destination team','move-target'),('hidden-maintainers','HIDDEN TEAM',NULL)").execute(&state.db).await.unwrap();
        sqlx::query(
            "UPDATE documents SET maintenance_group_id='source-maintainers' WHERE id='move-page'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let first = create_token(&state).await.unwrap().0;
        let page = first.inventory["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "move-page")
            .unwrap();
        assert_eq!(page["maintenance_resolution_required"], true);
        assert_eq!(page["maintenance_edit_allowed"], true);
        assert!(!first.inventory["maintenance_targets"]
            .to_string()
            .contains("HIDDEN"));
        let before: String =
            sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='move-page'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let db = state.db.clone();
        AFTER_INVENTORY.with(|slot| {
            *slot.borrow_mut() = Some(Box::pin(async move {
                sqlx::query("DELETE FROM groups WHERE id='source-maintainers'")
                    .execute(&db)
                    .await
                    .unwrap();
            }))
        });
        assert!(create_token(&state).await.is_err());
        let now: (Option<String>, String) = sqlx::query_as(
            "SELECT maintenance_group_id,updated_at FROM documents WHERE id='move-page'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(now, (None, before));
        let fresh = create_token(&state).await.unwrap().0;
        let page = fresh.inventory["pages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "move-page")
            .unwrap();
        assert_eq!(page["maintenance_group_id"], Value::Null);
        assert_eq!(page["maintenance_resolution_required"], false);
        cleanup(&source).await;
        cleanup(&target).await;
    }
    #[tokio::test]
    async fn maintenance_catalog_and_source_late_wal_drift_reject_after_raw_proofs() {
        for source_drift in [false, true] {
            let (state, source, target) = fixture_with_wal(true).await;
            sqlx::query(
                "UPDATE documents SET maintenance_user_id='other-owner' WHERE id='move-page'",
            )
            .execute(&state.db)
            .await
            .unwrap();
            let id = create_token(&state).await.unwrap().0.preview_id;
            let db = state.db.clone();
            AFTER_RAW_PROOFS.with(|slot| *slot.borrow_mut() = Some(Box::pin(async move {
                let sql = if source_drift { "UPDATE users SET display_name='Changed assigned person' WHERE id='other-owner'" } else { "INSERT INTO users(id,email,display_name,is_active) VALUES('new-directory-person','new-directory-person@test.invalid','New visible person',1)" };
                sqlx::query(sql).execute(&db).await.unwrap();
            })));
            assert!(
                read_token(&state, &id).await.is_err(),
                "source={source_drift}"
            );
            cleanup(&source).await;
            cleanup(&target).await;
        }
    }
    #[tokio::test]
    async fn maintenance_catalog_group_visibility_reproved_after_target_witness_waits() {
        let (state, source, target) = fixture_with_wal(true).await;
        sqlx::query("UPDATE space_members SET role='owner' WHERE user_id='mover'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name,managed_space_id) VALUES('destination-maintainers','Destination team','move-target')").execute(&state.db).await.unwrap();
        let first = create_token(&state).await.unwrap().0;
        assert_eq!(
            first.inventory["maintenance_targets"]["groups_available"],
            true
        );
        let db = state.db.clone();
        AFTER_MAINTENANCE_TARGETS.with(|slot| *slot.borrow_mut() = Some(Box::pin(async move {
            sqlx::query("UPDATE space_members SET role='editor' WHERE user_id='mover' AND space_id='move-target'").execute(&db).await.unwrap();
        })));
        assert!(read_token(&state, &first.preview_id).await.is_err());
        let mut conn = state.db.acquire().await.unwrap();
        access::require_move_preview_space(
            &mut conn,
            &test_auth("mover"),
            "move-target",
            MovePreviewCredential::Read,
        )
        .await
        .unwrap();
        drop(conn);
        cleanup(&source).await;
        cleanup(&target).await;
    }
}

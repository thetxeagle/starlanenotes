//! Reviewed local-draft transfer into one owner-private server draft.
//! This is a separate operation/receipt namespace from draft birth and replay.
use super::{
    access::{self, DraftOperation, DraftParticipation, PageOperation, Purpose},
    draft_save, published_source,
};
use crate::{
    auth::{middleware::AuthUser, routes::AppError},
    AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::AnyConnection;
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct LocalSource {
    pub provider: String,
    pub root_identity: String,
    pub space_id: String,
    pub document_id: String,
    pub draft_id: String,
    pub draft_created_at: String,
    pub snapshot_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum ServerSource {
    Draft {
        draft_id: String,
        draft_created_at: String,
        source_digest: String,
    },
    PublishedPage {
        page_updated_at: String,
        source_digest: String,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewedDraft {
    pub title: String,
    pub icon: Option<String>,
    pub cover_image: Option<String>,
    pub content: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub schema_version: u32,
    pub operation_id: String,
    pub local_source: LocalSource,
    pub server_source: ServerSource,
    pub reviewed_draft: ReviewedDraft,
    #[serde(default)]
    pub selected_operation_fingerprints: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub(crate) struct Receipt {
    pub schema_version: u32,
    pub operation_id: String,
    pub request_fingerprint: String,
    pub server_actor_id: String,
    pub server_space_id: String,
    pub server_document_id: String,
    pub target_draft_id: String,
    pub target_draft_created_at: String,
    pub target_source_digest: String,
    pub local_source: LocalSource,
    pub selected_operation_fingerprints: Vec<String>,
    pub created_at: String,
}

/// Reprove one immutable transfer winner before replay binds any original work to it.
pub(crate) async fn receipt_for_replay_binding(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    operation_id: &str,
) -> Result<Receipt, AppError> {
    if Uuid::parse_str(operation_id).is_err() {
        return Err(conflict());
    }
    access::require_current_human_preference_write(conn, auth).await?;
    let captured = rows(
        conn,
        "SELECT * FROM draft_transfer_receipts WHERE actor_id=$1 AND operation_id=$2",
        &[&auth.claims.sub, operation_id],
    )
    .await?;
    if captured.len() != 1 {
        return Err(conflict());
    }
    let receipt: Receipt = serde_json::from_str(&draft_save::string(&captured[0], "receipt_json")?)
        .map_err(|_| conflict())?;
    if receipt.schema_version != 1
        || receipt.server_actor_id != auth.claims.sub
        || receipt.operation_id != operation_id
        || captured != vec![expected_row(&receipt)?]
    {
        return Err(conflict());
    }
    let principal = access::principal_from_auth(auth)?;
    access::resolve_page_access(
        conn,
        principal,
        &receipt.server_space_id,
        &receipt.server_document_id,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await?;
    access::resolve_draft_access(
        conn,
        principal,
        &receipt.server_space_id,
        &receipt.server_document_id,
        &receipt.target_draft_id,
        DraftOperation::Edit,
        Purpose::Routine,
    )
    .await?;
    let target = draft_save::selected(
        conn,
        &receipt.server_space_id,
        &receipt.server_document_id,
        &receipt.target_draft_id,
    )
    .await?;
    if draft_save::string(&target, "created_at")? != receipt.target_draft_created_at
        || draft_save::digest(&target)? != receipt.target_source_digest
        || rows(
            conn,
            "SELECT * FROM draft_transfer_receipts WHERE actor_id=$1 AND operation_id=$2",
            &[&auth.claims.sub, operation_id],
        )
        .await?
            != captured
    {
        return Err(conflict());
    }
    access::require_current_human_preference_write(conn, auth).await?;
    Ok(receipt)
}

fn conflict() -> AppError {
    AppError::Conflict("transfer_conflict: Transfer source, authority, or immutable receipt changed; retain the original reviewed request".into())
}
fn required() -> AppError {
    AppError::Conflict("transfer_source_required: Transfer requires one exact reviewed local snapshot and an authorized server source".into())
}
fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn request_fingerprint(request: &Request) -> Result<String, AppError> {
    draft_save::digest(&serde_json::to_value(request).map_err(AppError::internal)?)
}
fn reviewed_digest(reviewed: &ReviewedDraft) -> Result<String, AppError> {
    draft_save::digest(&json!({
        "title": reviewed.title,
        "icon": reviewed.icon,
        "cover_image": reviewed.cover_image,
        "content": reviewed.content,
    }))
}
fn validate(request: &Request) -> Result<(), AppError> {
    if request.schema_version != 1 || Uuid::parse_str(&request.operation_id).is_err() {
        return Err(required());
    }
    let local = &request.local_source;
    if [
        local.provider.as_str(),
        local.root_identity.as_str(),
        local.space_id.as_str(),
        local.document_id.as_str(),
        local.draft_id.as_str(),
        local.draft_created_at.as_str(),
    ]
    .iter()
    .any(|v| v.is_empty() || v.len() > 1000)
        || !sha(&local.snapshot_digest)
        || reviewed_digest(&request.reviewed_draft)? != local.snapshot_digest
        || request.reviewed_draft.title.trim().is_empty()
        || request.reviewed_draft.title.len() > 500
        || request.reviewed_draft.content.len() > 5_242_880
        || request
            .selected_operation_fingerprints
            .iter()
            .any(|v| !sha(v))
        || request
            .selected_operation_fingerprints
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != request.selected_operation_fingerprints.len()
    {
        return Err(required());
    }
    match &request.server_source {
        ServerSource::Draft {
            draft_id,
            draft_created_at,
            source_digest,
        } if !draft_id.is_empty() && !draft_created_at.is_empty() && sha(source_digest) => Ok(()),
        ServerSource::PublishedPage {
            page_updated_at,
            source_digest,
        } if !page_updated_at.is_empty() && sha(source_digest) => Ok(()),
        _ => Err(required()),
    }
}
async fn rows(
    conn: &mut AnyConnection,
    query: &str,
    args: &[&str],
) -> Result<Vec<Value>, AppError> {
    super::document_deletion::rows(conn, query, args)
        .await
        .map_err(AppError::internal)
}
fn expected_row(receipt: &Receipt) -> Result<Value, AppError> {
    Ok(json!({
        "actor_id": receipt.server_actor_id,
        "operation_id": receipt.operation_id,
        "request_fingerprint": receipt.request_fingerprint,
        "space_id": receipt.server_space_id,
        "document_id": receipt.server_document_id,
        "draft_id": receipt.target_draft_id,
        "draft_created_at": receipt.target_draft_created_at,
        "source_digest": receipt.target_source_digest,
        "receipt_json": serde_json::to_string(receipt).map_err(AppError::internal)?,
        "created_at": receipt.created_at,
    }))
}
async fn historical(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    document: &str,
    request: &Request,
) -> Result<Option<Receipt>, AppError> {
    let captured = rows(
        conn,
        "SELECT * FROM draft_transfer_receipts WHERE actor_id=$1 AND operation_id=$2",
        &[&auth.claims.sub, &request.operation_id],
    )
    .await?;
    if captured.is_empty() {
        return Ok(None);
    }
    if captured.len() != 1 {
        return Err(conflict());
    }
    let receipt: Receipt = serde_json::from_str(&draft_save::string(&captured[0], "receipt_json")?)
        .map_err(|_| conflict())?;
    if receipt.schema_version != 1
        || receipt.server_actor_id != auth.claims.sub
        || receipt.server_space_id != space
        || receipt.server_document_id != document
        || receipt.operation_id != request.operation_id
        || receipt.request_fingerprint != request_fingerprint(request)?
        || captured != vec![expected_row(&receipt)?]
    {
        return Err(conflict());
    }
    let principal = access::principal_from_auth(auth)?;
    access::resolve_page_access(
        conn,
        principal,
        space,
        document,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await?;
    if rows(
        conn,
        "SELECT * FROM draft_transfer_receipts WHERE actor_id=$1 AND operation_id=$2",
        &[&auth.claims.sub, &request.operation_id],
    )
    .await?
        != captured
    {
        return Err(conflict());
    }
    access::require_current_human_preference_write(conn, auth).await?;
    Ok(Some(receipt))
}

pub(crate) async fn create(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    document: &str,
    request: Request,
) -> Result<Receipt, AppError> {
    validate(&request)?;
    let fingerprint = request_fingerprint(&request)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space]).await?;
    access::require_current_human_preference_write(&mut tx, auth).await?;
    if let Some(receipt) = historical(&mut tx, auth, space, document, &request).await? {
        tx.rollback().await.map_err(AppError::internal)?;
        return Ok(receipt);
    }
    let locked = sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(document).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::NotFound("Document unavailable".into()));
    }
    sqlx::query("UPDATE document_drafts SET id=id WHERE document_id=$1 AND space_id=$2")
        .bind(document)
        .bind(space)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id IN(SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2)").bind(document).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    let principal = access::principal_from_auth(auth)?;
    let authority = access::resolve_page_access(
        &mut tx,
        principal,
        space,
        document,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await?;
    let mut proof =
        published_source::capture_source(&mut tx, auth, space, document, &authority.ancestor_ids)
            .await?;
    let page = proof[0]
        .rows
        .iter()
        .find(|row| row["id"] == document)
        .ok_or_else(conflict)?
        .clone();
    let mut private_source = None;
    let mut published_bytes = None;
    let (base, folder, parent) = match &request.server_source {
        ServerSource::Draft {
            draft_id,
            draft_created_at,
            source_digest,
        } => {
            if !page["published_at"].is_null() {
                return Err(conflict());
            }
            let source_access = access::resolve_draft_access(
                &mut tx,
                principal,
                space,
                document,
                draft_id,
                DraftOperation::Edit,
                Purpose::Routine,
            )
            .await?;
            if !matches!(
                source_access.participation,
                DraftParticipation::Owner | DraftParticipation::SharedEditor
            ) {
                return Err(conflict());
            }
            let source = draft_save::selected(&mut tx, space, document, draft_id).await?;
            if draft_save::string(&source, "created_at")? != *draft_created_at
                || draft_save::digest(&source)? != *source_digest
            {
                return Err(conflict());
            }
            private_source = Some((
                draft_id.clone(),
                source.clone(),
                source_access.participation,
            ));
            (
                draft_save::string(&source, "base_updated_at")?,
                draft_save::optional(&source, "previous_folder_id")?,
                draft_save::optional(&source, "previous_parent_id")?,
            )
        }
        ServerSource::PublishedPage {
            page_updated_at,
            source_digest,
        } => {
            if page["published_at"].is_null()
                || draft_save::string(&page, "updated_at")? != *page_updated_at
            {
                return Err(conflict());
            }
            let path = draft_save::string(&page, "content_path")?;
            let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id=$1")
                .bind(space)
                .fetch_one(&mut *tx)
                .await
                .map_err(AppError::internal)?;
            if path.is_empty()
                || path.contains(char::from(92))
                || !std::path::Path::new(&path)
                    .components()
                    .all(|component| matches!(component, std::path::Component::Normal(_)))
                || !path.starts_with(&format!("spaces/{slug}/docs/"))
            {
                return Err(conflict());
            }
            let bytes = crate::storage::read_document(&path)
                .await
                .map_err(AppError::internal)?;
            if published_source::digest(&page, bytes.as_bytes())? != *source_digest {
                return Err(conflict());
            }
            published_bytes = Some((path, bytes));
            (
                page_updated_at.clone(),
                draft_save::optional(&page, "folder_id")?,
                draft_save::optional(&page, "parent_id")?,
            )
        }
    };
    let target = Uuid::new_v4().to_string();
    let stamp = chrono::Utc::now().to_rfc3339();
    let raw = sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,icon,cover_image,content,base_updated_at,created_at,updated_at,previous_folder_id,previous_parent_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$10,$11,$12) RETURNING *")
        .bind(&target).bind(document).bind(space).bind(&auth.claims.sub).bind(request.reviewed_draft.title.trim()).bind(&request.reviewed_draft.icon).bind(&request.reviewed_draft.cover_image).bind(&request.reviewed_draft.content).bind(&base).bind(&stamp).bind(&folder).bind(&parent)
        .fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let target_row = draft_save::joined_source(&raw)?;
    let target_digest = draft_save::digest(&target_row)?;
    let receipt = Receipt {
        schema_version: 1,
        operation_id: request.operation_id.clone(),
        request_fingerprint: fingerprint,
        server_actor_id: auth.claims.sub.clone(),
        server_space_id: space.into(),
        server_document_id: document.into(),
        target_draft_id: target.clone(),
        target_draft_created_at: stamp.clone(),
        target_source_digest: target_digest.clone(),
        local_source: request.local_source.clone(),
        selected_operation_fingerprints: request.selected_operation_fingerprints.clone(),
        created_at: stamp.clone(),
    };
    sqlx::query("INSERT INTO draft_transfer_receipts(actor_id,operation_id,request_fingerprint,space_id,document_id,draft_id,draft_created_at,source_digest,receipt_json,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
        .bind(&auth.claims.sub).bind(&request.operation_id).bind(&receipt.request_fingerprint).bind(space).bind(document).bind(&target).bind(&stamp).bind(&target_digest).bind(serde_json::to_string(&receipt).map_err(AppError::internal)?).bind(&stamp)
        .execute(&mut *tx).await.map_err(AppError::internal)?;
    let audit_id = Uuid::new_v4().to_string();
    let audit_stamp: String = sqlx::query_scalar("INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,'document.draft_transfer','document_draft',$3) RETURNING created_at")
        .bind(&audit_id).bind(&auth.claims.sub).bind(&target).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit = json!({"id":audit_id,"user_id":auth.claims.sub,"ip_address":null,"action":"document.draft_transfer","target_type":"document_draft","target_id":target,"created_at":audit_stamp,"actor_kind":"user","actor_id":auth.claims.sub,"before_value":null,"after_value":null});
    let drafts = proof
        .iter_mut()
        .find(|p| {
            p.query == "SELECT * FROM document_drafts WHERE document_id=$1"
                && p.parameters == vec![document.to_owned()]
        })
        .ok_or_else(conflict)?;
    drafts.rows.push(target_row.clone());
    drafts.rows.sort_by_cached_key(Value::to_string);
    published_source::verify_source(&mut tx, &proof).await?;
    if rows(
        &mut tx,
        "SELECT * FROM draft_transfer_receipts WHERE actor_id=$1 AND operation_id=$2",
        &[&auth.claims.sub, &request.operation_id],
    )
    .await?
        != vec![expected_row(&receipt)?]
        || rows(&mut tx, "SELECT * FROM audit_log WHERE id=$1", &[&audit_id]).await?
            != vec![audit.clone()]
    {
        return Err(conflict());
    }
    access::resolve_page_access(
        &mut tx,
        principal,
        space,
        document,
        PageOperation::Edit,
        Purpose::Routine,
    )
    .await?;
    if let Some((source_id, source_row, participation)) = private_source {
        let final_source = access::resolve_draft_access(
            &mut tx,
            principal,
            space,
            document,
            &source_id,
            DraftOperation::Edit,
            Purpose::Routine,
        )
        .await?;
        if final_source.participation != participation
            || draft_save::selected(&mut tx, space, document, &source_id).await? != source_row
        {
            return Err(conflict());
        }
    }
    let target_access = access::resolve_draft_access(
        &mut tx,
        principal,
        space,
        document,
        &target,
        DraftOperation::Edit,
        Purpose::Routine,
    )
    .await?;
    if target_access.participation != DraftParticipation::Owner
        || draft_save::selected(&mut tx, space, document, &target).await? != target_row
    {
        return Err(conflict());
    }
    access::require_current_human_preference_write(&mut tx, auth).await?;
    // Synchronous canonical-path proof after the final SQL/credential await.
    if let Some((path, bytes)) = published_bytes {
        if std::fs::read_to_string(crate::storage::data_dir().join(path))
            .map_err(AppError::internal)?
            != bytes
        {
            return Err(conflict());
        }
    }
    tx.commit().await.map_err(|_| {
        AppError::Conflict(
            "Draft transfer outcome is unknown; retain the exact operation_id and reviewed request"
                .into(),
        )
    })?;
    crate::audit::append_committed_move_action(&audit).await;
    Ok(receipt)
}

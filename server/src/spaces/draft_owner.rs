//! Consumed owner draft metadata operations. View and original method credential
//! are separate from body Edit, Publish, canonical DeleteContent and recovery.
use super::{access::{self, DraftOwnerCredential}, draft_save, published_source};
use crate::{auth::{middleware::AuthUser, routes::AppError}, AppState};
use serde_json::{json, Value};
use sqlx::AnyConnection;
use uuid::Uuid;

fn conflict() -> AppError {
    AppError::Conflict("Draft owner, recipient or access source changed; retain the request for review".into())
}
async fn rows(conn: &mut AnyConnection, query: &str, args: &[&str]) -> Result<Vec<Value>, AppError> {
    super::document_deletion::rows(conn, query, args).await.map_err(AppError::internal)
}
async fn capture(
    conn: &mut AnyConnection, auth: &AuthUser, space: &str, document: &str, ancestors: &[String],
) -> Result<Vec<published_source::SourceProof>, AppError> {
    let mut proof = published_source::capture_source(conn, auth, space, document, ancestors).await?;
    let query = "SELECT id,display_name,email,role,is_active FROM users";
    proof.push(published_source::SourceProof {
        query: query.into(), parameters: vec![], rows: rows(conn, query, &[]).await?,
    });
    // Draft cleanup must not silently cascade or trigger removal of immutable
    // birth/save/publication evidence. These rows remain exact after mutation.
    for table in ["draft_birth_receipts", "draft_transfer_receipts", "draft_save_replay_receipts",
        "publication_receipts", "canonical_revision_receipts"] {
        let query = format!("SELECT * FROM {table} WHERE document_id=$1 AND space_id=$2");
        proof.push(published_source::SourceProof {
            rows: rows(conn, &query, &[document, space]).await?,
            query, parameters: vec![document.into(), space.into()],
        });
    }
    Ok(proof)
}
fn source_denial(error: &AppError) -> bool {
    matches!(error, AppError::NotFound(_) | AppError::Forbidden(_))
}
async fn recipient_eligible(
    conn: &mut AnyConnection, space: &str, document: &str, draft: &str, recipient: &str,
) -> Result<bool, AppError> {
    match access::inspect_draft_share_recipient(conn, space, document, draft, recipient).await {
        Ok(()) => Ok(true),
        Err(error) if source_denial(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

/// Existing owner-only audience inspection, on one autocommit connection.
/// No purge, grants, files or successful mutation audit are written by GET.
pub(crate) async fn inspect(
    state: &AppState, auth: &AuthUser, space: &str, document: &str, draft: &str, candidates: bool,
) -> Result<Vec<Value>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let authority = access::resolve_owned_draft_access(
        &mut conn, auth, space, document, draft, DraftOwnerCredential::Read,
    ).await?;
    let selected = draft_save::selected(&mut conn, space, document, draft).await?;
    let proof = capture(&mut conn, auth, space, document, &authority.page.ancestor_ids).await?;
    let query = if candidates {
        "SELECT id AS user_id,display_name,email FROM users WHERE is_active=1 AND id!=$1 ORDER BY display_name,email"
    } else {
        "SELECT a.user_id,u.display_name,u.email,a.role FROM document_draft_access a JOIN users u ON u.id=a.user_id WHERE a.draft_id=$1 ORDER BY lower(u.display_name),lower(u.email)"
    };
    let parameter = if candidates { auth.claims.sub.as_str() } else { draft };
    // Preserve SQL response ordering; source snapshots use their own canonical order.
    use sqlx::Row;
    let queried = sqlx::query(query).bind(parameter).fetch_all(&mut *conn)
        .await.map_err(AppError::internal)?;
    let queried_source: Vec<Value> = queried.iter().map(draft_save::joined_source)
        .collect::<Result<_, _>>()?;
    let mut projection = Vec::new();
    for row in queried {
        let recipient: String = row.try_get("user_id").map_err(AppError::internal)?;
        if candidates && !recipient_eligible(&mut conn, space, document, draft, &recipient).await? {
            continue;
        }
        let mut entry = json!({
            "user_id": recipient,
            "display_name": row.try_get::<String,_>("display_name").map_err(AppError::internal)?,
            "email": row.try_get::<String,_>("email").map_err(AppError::internal)?,
        });
        if !candidates {
            entry["role"] = json!(row.try_get::<String,_>("role").map_err(AppError::internal)?);
        }
        projection.push(entry);
    }
    #[cfg(test)] after_read_projection().await;
    // Eligibility reproof precedes complete raw witness verification. Ineligible
    // late candidates reject the old projection rather than retain their labels.
    if candidates {
        for entry in &projection {
            if !recipient_eligible(&mut conn, space, document, draft,
                draft_save::string(entry, "user_id")?.as_str()).await?
            { return Err(conflict()); }
        }
    }
    published_source::verify_source(&mut conn, &proof).await?;
    #[cfg(test)] after_read_inventory().await;
    // The inventory check itself awaits SQL. Recipient policy and the exact
    // projected account/share tuple must still match after those waits.
    if candidates {
        for entry in &projection {
            if !recipient_eligible(&mut conn, space, document, draft,
                draft_save::string(entry, "user_id")?.as_str()).await?
            { return Err(conflict()); }
        }
    }
    let current_query = sqlx::query(query).bind(parameter).fetch_all(&mut *conn)
        .await.map_err(AppError::internal)?;
    let current_source: Vec<Value> = current_query.iter().map(draft_save::joined_source)
        .collect::<Result<_, _>>()?;
    if current_source != queried_source { return Err(conflict()); }
    access::resolve_owned_draft_access(
        &mut conn, auth, space, document, draft, DraftOwnerCredential::Read,
    ).await?;
    if draft_save::selected(&mut conn, space, document, draft).await? != selected {
        return Err(conflict());
    }
    #[cfg(test)] before_read_identity().await;
    access::require_current_human_read(&mut conn, auth).await?;
    Ok(projection)
}

pub(crate) enum Change<'a> {
    Delete,
    Grant { recipient: &'a str, role: &'a str },
    Revoke { recipient: &'a str },
}
async fn audit(
    conn: &mut AnyConnection, auth: &AuthUser, action: &str, draft: &str,
) -> Result<Value, AppError> {
    let id = Uuid::new_v4().to_string();
    let created: String = sqlx::query_scalar(
        "INSERT INTO audit_log(id,user_id,actor_kind,actor_id,action,target_type,target_id) VALUES($1,$2,'user',$2,$3,'document_draft',$4) RETURNING created_at",
    ).bind(&id).bind(&auth.claims.sub).bind(action).bind(draft)
        .fetch_one(conn).await.map_err(AppError::internal)?;
    Ok(json!({
        "id": id, "user_id": auth.claims.sub, "ip_address": null,
        "actor_kind": "user", "actor_id": auth.claims.sub, "action": action,
        "target_type": "document_draft", "target_id": draft, "created_at": created,
        "before_value": null, "after_value": null,
    }))
}
fn replace_share(
    proof: &mut [published_source::SourceProof], document: &str, draft: &str,
    recipient: Option<&str>, next: Option<Value>,
) -> Result<(), AppError> {
    let witness = proof.iter_mut().find(|p|
        p.query == "SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1"
        && p.parameters == vec![document.to_owned()]
    ).ok_or_else(conflict)?;
    witness.rows.retain(|row| !(row["draft_id"] == draft
        && recipient.is_none_or(|recipient| row["user_id"] == recipient)));
    if let Some(row) = next { witness.rows.push(row); }
    witness.rows.sort_by_cached_key(Value::to_string);
    Ok(())
}

/// Audience→space→scoped page/draft/access guards. Receipts and mirror bytes are
/// retained. Unknown commit never implies deletion/erasure or rollback.
pub(crate) async fn mutate(
    state: &AppState, auth: &AuthUser, space: &str, document: &str, draft: &str, change: Change<'_>,
) -> Result<(), AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    #[cfg(test)] before_lock().await;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space]).await?;
    let count = sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
        .bind(document).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    if count.rows_affected() != 1 { return Err(AppError::NotFound("Draft unavailable".into())); }
    sqlx::query("UPDATE document_drafts SET id=id WHERE document_id=$1 AND space_id=$2")
        .bind(document).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id IN(SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2)")
        .bind(document).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    let original = access::resolve_owned_draft_access(
        &mut tx, auth, space, document, draft, DraftOwnerCredential::Write,
    ).await?;
    if let Change::Grant { recipient, role } = &change {
        if *recipient == auth.claims.sub {
            return Err(AppError::BadRequest("The draft owner already has full access".into()));
        }
        if !matches!(*role, "viewer" | "editor") {
            return Err(AppError::BadRequest("Draft access role must be viewer or editor".into()));
        }
    }
    let selected = draft_save::selected(&mut tx, space, document, draft).await?;
    let mut expected = capture(&mut tx, auth, space, document, &original.page.ancestor_ids).await?;
    let mut deleted = false;
    let mut grant_recipient = None;
    let mut event = None;
    match change {
        Change::Delete => {
            let count = sqlx::query("DELETE FROM document_drafts WHERE id=$1 AND document_id=$2 AND space_id=$3 AND owner_id=$4")
                .bind(draft).bind(document).bind(space).bind(&auth.claims.sub)
                .execute(&mut *tx).await.map_err(AppError::internal)?;
            if count.rows_affected() != 1 { return Err(conflict()); }
            let witness = expected.iter_mut().find(|p|
                p.query == "SELECT * FROM document_drafts WHERE document_id=$1"
                && p.parameters == vec![document.to_owned()]
            ).ok_or_else(conflict)?;
            witness.rows.retain(|row| row["id"] != draft);
            replace_share(&mut expected, document, draft, None, None)?;
            deleted = true;
            event = Some(audit(&mut tx, auth, "document.draft_delete", draft).await?);
        }
        Change::Grant { recipient, role } => {
            if !recipient_eligible(&mut tx, space, document, draft, recipient).await? {
                return Err(AppError::BadRequest("User cannot access this draft's page scope".into()));
            }
            let previous = rows(&mut tx,
                "SELECT * FROM document_draft_access WHERE draft_id=$1 AND user_id=$2", &[draft, recipient]).await?;
            if previous.len() > 1 { return Err(conflict()); }
            let stamp = chrono::Utc::now().to_rfc3339();
            let raw = sqlx::query(
                "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$5) ON CONFLICT(draft_id,user_id) DO UPDATE SET role=excluded.role,granted_by=excluded.granted_by,updated_at=excluded.updated_at RETURNING *",
            ).bind(draft).bind(recipient).bind(role).bind(&auth.claims.sub).bind(&stamp)
                .fetch_one(&mut *tx).await.map_err(AppError::internal)?;
            let row = draft_save::joined_source(&raw)?;
            let created = previous.first().map(|p| draft_save::string(p, "created_at"))
                .transpose()?.unwrap_or_else(|| stamp.clone());
            if row != json!({"draft_id":draft,"user_id":recipient,"role":role,
                "granted_by":auth.claims.sub,"created_at":created,"updated_at":stamp})
            { return Err(conflict()); }
            replace_share(&mut expected, document, draft, Some(recipient), Some(row))?;
            grant_recipient = Some(recipient);
            event = Some(audit(&mut tx, auth, "document.draft_access_grant", draft).await?);
        }
        Change::Revoke { recipient } => {
            let previous = rows(&mut tx,
                "SELECT * FROM document_draft_access WHERE draft_id=$1 AND user_id=$2", &[draft, recipient]).await?;
            if previous.len() > 1 { return Err(conflict()); }
            let count = sqlx::query("DELETE FROM document_draft_access WHERE draft_id=$1 AND user_id=$2")
                .bind(draft).bind(recipient).execute(&mut *tx).await.map_err(AppError::internal)?;
            if count.rows_affected() != previous.len() as u64 { return Err(conflict()); }
            replace_share(&mut expected, document, draft, Some(recipient), None)?;
            if !previous.is_empty() {
                event = Some(audit(&mut tx, auth, "document.draft_access_revoke", draft).await?);
            }
        }
    }
    #[cfg(test)] after_writes().await;
    if let Some(audit) = &event {
        if rows(&mut tx, "SELECT * FROM audit_log WHERE id=$1",
            &[draft_save::string(audit, "id")?.as_str()]).await? != vec![audit.clone()]
        { return Err(conflict()); }
    }
    published_source::verify_source(&mut tx, &expected).await?;
    if let Some(recipient) = grant_recipient {
        if !recipient_eligible(&mut tx, space, document, draft, recipient).await? {
            return Err(conflict());
        }
    }
    if deleted {
        access::verify_owned_draft_removed(&mut tx, auth, &original).await?;
    } else {
        access::resolve_owned_draft_access(
            &mut tx, auth, space, document, draft, DraftOwnerCredential::Write,
        ).await?;
        if draft_save::selected(&mut tx, space, document, draft).await? != selected {
            return Err(conflict());
        }
    }
    #[cfg(test)] before_write_identity().await;
    access::require_current_human_preference_write(&mut tx, auth).await?;
    tx.commit().await.map_err(|_| AppError::Conflict(
        "Draft management outcome is unknown; retain the original request for review".into(),
    ))?;
    if let Some(event) = event { crate::audit::append_committed_move_action(&event).await; }
    Ok(())
}

#[cfg(test)] tokio::task_local! {
    pub(crate) static BEFORE_LOCK: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>>>>;
    pub(crate) static AFTER_WRITES: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>>>>;
    pub(crate) static AFTER_READ_PROJECTION: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>>>>;
    pub(crate) static AFTER_READ_INVENTORY: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>>>>;
    pub(crate) static BEFORE_READ_IDENTITY: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>>>>;
    pub(crate) static BEFORE_WRITE_IDENTITY: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>>>>;
}
#[cfg(test)] async fn before_lock() { if let Ok(Some(h)) = BEFORE_LOCK.try_with(|s| s.borrow_mut().take()) { h.await; } }
#[cfg(test)] async fn after_writes() { if let Ok(Some(h)) = AFTER_WRITES.try_with(|s| s.borrow_mut().take()) { h.await; } }
#[cfg(test)] async fn after_read_projection() { if let Ok(Some(h)) = AFTER_READ_PROJECTION.try_with(|s| s.borrow_mut().take()) { h.await; } }
#[cfg(test)] async fn after_read_inventory() { if let Ok(Some(h)) = AFTER_READ_INVENTORY.try_with(|s| s.borrow_mut().take()) { h.await; } }
#[cfg(test)] async fn before_read_identity() { if let Ok(Some(h)) = BEFORE_READ_IDENTITY.try_with(|s| s.borrow_mut().take()) { h.await; } }
#[cfg(test)] async fn before_write_identity() { if let Ok(Some(h)) = BEFORE_WRITE_IDENTITY.try_with(|s| s.borrow_mut().take()) { h.await; } }

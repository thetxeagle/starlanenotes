//! Actual SQL-only soft-trash and archive transitions. No harddelete, cascade, body publication or mirror write.
use super::access::{self, PageOperation, Purpose};
use crate::{
    auth::{middleware::AuthUser, routes::AppError},
    AppState,
};
use serde_json::{json, Value};
use sqlx::AnyConnection;

#[derive(Clone)]
struct Proof {
    query: String,
    parameters: Vec<String>,
    rows: Vec<Value>,
}
async fn rows(conn: &mut AnyConnection, q: &str, args: &[&str]) -> Result<Vec<Value>, AppError> {
    super::document_deletion::rows(conn, q, args)
        .await
        .map_err(AppError::internal)
}
async fn source(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    ancestors: &[String],
) -> Result<Vec<Proof>, AppError> {
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
        proof.push(Proof {
            query,
            parameters,
            rows: current,
        });
    }
    Ok(proof)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Intent {
    Trash,
    Archive,
    Unarchive,
    Restore,
}
impl Intent {
    fn operation(self) -> PageOperation {
        match self {
            Self::Trash => PageOperation::Trash,
            Self::Archive => PageOperation::Archive,
            Self::Unarchive => PageOperation::UnarchiveArchived,
            Self::Restore => PageOperation::RestoreTrashed,
        }
    }
    fn final_operation(self) -> PageOperation {
        match self {
            Self::Trash => PageOperation::DeleteTrashed,
            Self::Archive => PageOperation::Archive,
            Self::Unarchive => PageOperation::UnarchiveActive,
            Self::Restore => PageOperation::RestoreNotDeleted,
        }
    }
    fn action(self) -> &'static str {
        match self {
            Self::Trash => "document.delete",
            Self::Archive => "document.archive",
            Self::Unarchive => "document.unarchive",
            Self::Restore => "document.restore",
        }
    }
}
/// Committed data only; no reusable authorization.
pub(crate) struct CommittedTransition {
    pub(crate) webhook: Value,
    pub(crate) audit: Value,
    space: String,
    id: String,
    stamp: String,
    actor: String,
    intent: Intent,
}
fn unavailable() -> AppError {
    AppError::NotFound("Document not found".into())
}
fn changed() -> AppError {
    AppError::Conflict("Page lifecycle source changed; retry".into())
}

pub(crate) async fn trash(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    id: &str,
) -> Result<CommittedTransition, AppError> {
    transition(state, auth, space, id, Intent::Trash).await
}
pub(crate) async fn archive(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    id: &str,
) -> Result<CommittedTransition, AppError> {
    transition(state, auth, space, id, Intent::Archive).await
}
pub(crate) async fn unarchive(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    id: &str,
) -> Result<CommittedTransition, AppError> {
    transition(state, auth, space, id, Intent::Unarchive).await
}
pub(crate) async fn restore(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    id: &str,
) -> Result<CommittedTransition, AppError> {
    transition(state, auth, space, id, Intent::Restore).await
}
async fn transition(
    state: &AppState,
    auth: &AuthUser,
    space: &str,
    id: &str,
    intent: Intent,
) -> Result<CommittedTransition, AppError> {
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[space]).await?;
    #[cfg(test)]
    if intent == Intent::Trash {
        after_guard().await;
    }
    let n = sqlx::query(
        "UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND (($4=1 AND deleted_at IS NOT NULL) OR ($4=0 AND deleted_at IS NULL)) AND ($3=0 OR archived_at IS NOT NULL)",
    )
    .bind(id)
    .bind(space)
    .bind(if intent == Intent::Unarchive {1_i64} else {0_i64})
    .bind(if intent == Intent::Restore {1_i64} else {0_i64})
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if n.rows_affected() != 1 {
        return Err(if intent == Intent::Unarchive {
            AppError::NotFound("Archived document not found".into())
        } else if intent == Intent::Restore {
            AppError::NotFound("Trashed document not found".into())
        } else {
            unavailable()
        });
    }
    let principal = access::principal_from_auth(auth)?;
    let authority = access::resolve_page_access(
        &mut tx,
        principal,
        space,
        id,
        intent.operation(),
        Purpose::Routine,
    )
    .await?;
    // Hold private-participation rows before the source snapshot. Do not write their content.
    sqlx::query("UPDATE document_drafts SET id=id WHERE document_id=$1 AND space_id=$2")
        .bind(id)
        .bind(space)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id IN(SELECT id FROM document_drafts WHERE document_id=$1 AND space_id=$2)").bind(id).bind(space).execute(&mut *tx).await.map_err(AppError::internal)?;
    let current = access::resolve_page_access(
        &mut tx,
        principal,
        space,
        id,
        intent.operation(),
        Purpose::Routine,
    )
    .await?;
    if current.ancestor_ids != authority.ancestor_ids {
        return Err(changed());
    }
    let original = source(&mut tx, auth, space, id, &authority.ancestor_ids).await?;
    let mut expected = original.clone();
    let page = expected[0]
        .rows
        .iter_mut()
        .find(|row| row["id"] == id)
        .ok_or_else(unavailable)?;
    let stamp = chrono::Utc::now().to_rfc3339();
    match intent {
        Intent::Trash => {
            page["deleted_at"] = json!(stamp);
            page["deleted_by"] = json!(auth.claims.sub);
            page["archived_at"] = Value::Null;
        }
        Intent::Archive => {
            page["archived_at"] = json!(stamp);
        }
        Intent::Unarchive => {
            page["archived_at"] = Value::Null;
        }
        Intent::Restore => {
            page["deleted_at"] = Value::Null;
            page["deleted_by"] = Value::Null;
        }
    }
    page["updated_at"] = json!(stamp);
    expected[0].rows.sort_by_cached_key(Value::to_string);
    let updated = match intent {
        Intent::Trash => sqlx::query("UPDATE documents SET deleted_at=$1,deleted_by=$2,archived_at=NULL,updated_at=$1 WHERE id=$3 AND space_id=$4 AND deleted_at IS NULL").bind(&stamp).bind(&auth.claims.sub).bind(id).bind(space).execute(&mut *tx).await,
        Intent::Archive => sqlx::query("UPDATE documents SET archived_at=$1,updated_at=$1 WHERE id=$2 AND space_id=$3 AND deleted_at IS NULL").bind(&stamp).bind(id).bind(space).execute(&mut *tx).await,
        Intent::Unarchive => sqlx::query("UPDATE documents SET archived_at=NULL,updated_at=$1 WHERE id=$2 AND space_id=$3 AND deleted_at IS NULL AND archived_at IS NOT NULL").bind(&stamp).bind(id).bind(space).execute(&mut *tx).await,
        Intent::Restore => sqlx::query("UPDATE documents SET deleted_at=NULL,deleted_by=NULL,updated_at=$1 WHERE id=$2 AND space_id=$3 AND deleted_at IS NOT NULL").bind(&stamp).bind(id).bind(space).execute(&mut *tx).await
    }.map_err(AppError::internal)?;
    if updated.rows_affected() != 1 {
        return Err(unavailable());
    }
    // Fixed-action, actual authorized human attribution; no fabricated Auth/Admin witness.
    let audit_id = uuid::Uuid::new_v4().to_string();
    let audit_time:String=sqlx::query_scalar("INSERT INTO audit_log(id,user_id,action,target_type,target_id) VALUES($1,$2,$3,'document',$4) RETURNING created_at").bind(&audit_id).bind(&auth.claims.sub).bind(intent.action()).bind(id).fetch_one(&mut *tx).await.map_err(AppError::internal)?;
    let audit = json!({"id":audit_id,"user_id":auth.claims.sub,"ip_address":null,"action":intent.action(),"target_type":"document","target_id":id,"created_at":audit_time});
    #[cfg(test)]
    if intent == Intent::Trash {
        after_audit().await;
    }
    let stored = rows(&mut tx, "SELECT * FROM audit_log WHERE id=$1", &[&audit_id]).await?;
    let audit_row = json!({"id":audit_id,"user_id":auth.claims.sub,"ip_address":null,"action":intent.action(),"target_type":"document","target_id":id,"created_at":audit_time,"actor_kind":null,"actor_id":null,"before_value":null,"after_value":null});
    if stored != vec![audit_row] {
        return Err(changed());
    }
    // All canonical/audit SQL and snapshot waits precede final real target policy.
    for proof in &expected {
        let args: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
        if rows(&mut tx, &proof.query, &args).await? != proof.rows {
            return Err(changed());
        }
    }
    access::resolve_page_access(
        &mut tx,
        principal,
        space,
        id,
        intent.final_operation(),
        Purpose::Routine,
    )
    .await?;
    #[cfg(test)]
    match intent {
        Intent::Trash => before_identity().await,
        Intent::Archive => {
            if let Ok(Some(hook)) = ARCHIVE_BEFORE_IDENTITY.try_with(|s| s.borrow_mut().take()) {
                hook.await
            }
        }
        Intent::Unarchive => {
            if let Ok(Some(hook)) = UNARCHIVE_BEFORE_IDENTITY.try_with(|s| s.borrow_mut().take()) {
                hook.await
            }
        }
        Intent::Restore => {
            if let Ok(Some(hook)) = RESTORE_BEFORE_IDENTITY.try_with(|s| s.borrow_mut().take()) {
                hook.await
            }
        }
    }
    access::require_current_human_preference_write(&mut tx, auth).await?;
    let webhook = match intent {
        Intent::Trash => json!({"document_id":id,"space_id":space,"deleted_by":auth.claims.sub}),
        Intent::Archive => json!({"document_id":id,"space_id":space,"archived_by":auth.claims.sub}),
        Intent::Unarchive => {
            json!({"document_id":id,"space_id":space,"unarchived_by":auth.claims.sub})
        }
        Intent::Restore => json!({"document_id":id,"space_id":space,"restored_by":auth.claims.sub}),
    };
    #[cfg(test)]
    let forced = match intent {
        Intent::Trash => TRASH_ROLLBACK.try_with(|flag| *flag).unwrap_or(false),
        Intent::Archive => ARCHIVE_ROLLBACK.try_with(|flag| *flag).unwrap_or(false),
        Intent::Unarchive => UNARCHIVE_ROLLBACK.try_with(|flag| *flag).unwrap_or(false),
        Intent::Restore => RESTORE_ROLLBACK.try_with(|flag| *flag).unwrap_or(false),
    };
    #[cfg(test)]
    if forced {
        tx.rollback().await.map_err(AppError::internal)?;
        return Err(AppError::Internal(
            match intent {
                Intent::Trash => "Trash commit did not succeed; files retained",
                Intent::Archive => "Archive commit did not succeed; files retained",
                Intent::Unarchive => "Unarchive commit did not succeed; files retained",
                Intent::Restore => "Restore commit did not succeed; files retained",
            }
            .into(),
        ));
    }
    tx.commit().await.map_err(AppError::internal)?;
    Ok(CommittedTransition {
        webhook,
        audit,
        space: space.into(),
        id: id.into(),
        stamp,
        actor: auth.claims.sub.clone(),
        intent,
    })
}
/// Advisory index removal is bound to the exact committed lifecycle stamp, not a restored/reused ID.
pub(crate) async fn remove_index(
    state: &AppState,
    auth: &AuthUser,
    committed: &CommittedTransition,
) -> Result<(), AppError> {
    if committed.actor != auth.claims.sub {
        return Err(changed());
    }
    if matches!(committed.intent, Intent::Unarchive | Intent::Restore) {
        return Err(changed());
    }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&committed.space]).await?;
    let n = sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
        .bind(&committed.id)
        .bind(&committed.space)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if n.rows_affected() != 1 {
        return Err(unavailable());
    }
    let row:Option<(String,Option<String>,Option<String>,Option<String>)>=sqlx::query_as(
        "SELECT updated_at,deleted_at,deleted_by,archived_at FROM documents WHERE id=$1 AND space_id=$2"
    ).bind(&committed.id).bind(&committed.space).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    let exact = match (&row, committed.intent) {
        (Some((updated, deleted, deleted_by, archived)), Intent::Trash) => {
            updated == &committed.stamp
                && deleted.as_deref() == Some(committed.stamp.as_str())
                && deleted_by.as_deref() == Some(committed.actor.as_str())
                && archived.is_none()
        }
        (Some((updated, deleted, _, archived)), Intent::Archive) => {
            updated == &committed.stamp
                && deleted.is_none()
                && archived.as_deref() == Some(committed.stamp.as_str())
        }
        _ => false,
    };
    if !exact {
        return Err(changed());
    }
    access::resolve_page_access(
        &mut tx,
        access::principal_from_auth(auth)?,
        &committed.space,
        &committed.id,
        committed.intent.final_operation(),
        Purpose::Routine,
    )
    .await?;
    access::require_current_human_preference_write(&mut tx, auth).await?;
    state
        .search_index
        .delete(&committed.id)
        .map_err(AppError::internal)?;
    tx.rollback().await.map_err(AppError::internal)?;
    Ok(())
}
#[cfg(test)]
tokio::task_local! {
 pub(crate) static TRASH_AFTER_GUARD:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static TRASH_AFTER_AUDIT:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static TRASH_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static TRASH_ROLLBACK:bool;
 pub(crate) static ARCHIVE_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static ARCHIVE_ROLLBACK:bool;
 pub(crate) static UNARCHIVE_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static UNARCHIVE_ROLLBACK:bool;
 pub(crate) static RESTORE_BEFORE_IDENTITY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
 pub(crate) static RESTORE_ROLLBACK:bool;
}
#[cfg(test)]
async fn after_guard() {
    if let Ok(Some(h)) = TRASH_AFTER_GUARD.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn after_audit() {
    if let Ok(Some(h)) = TRASH_AFTER_AUDIT.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}
#[cfg(test)]
async fn before_identity() {
    if let Ok(Some(h)) = TRASH_BEFORE_IDENTITY.try_with(|s| s.borrow_mut().take()) {
        h.await
    }
}

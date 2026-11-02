use sqlx::AnyConnection;

use crate::auth::routes::AppError;

/// Serialize adopted audience-topology transactions before enumerating spaces.
/// Call FIRST, before any space/family lock, on an active caller transaction.
/// Never acquire this after a space guard and never create the singleton here.
/// Missing migration/row fails closed. This protects only explicitly adopted
/// writers; unadopted space creation, group membership and SCIM remain pending.
pub(crate) async fn lock_audience_for_mutation(conn: &mut AnyConnection) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE audience_mutation_fence SET id=id WHERE id=1")
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if result.rows_affected() != 1 {
        return Err(AppError::Conflict("Audience fence unavailable".into()));
    }
    Ok(())
}

/// Acquire ordered space write guards on the caller's transaction connection.
///
/// Call before finer document/folder/schema locks, validation reads, or canonical
/// storage work. After waiting, re-resolve subjects in their expected space and
/// revalidate permissions/lifecycle using this same connection. This helper does
/// not authorize, start/commit a transaction, or protect unadopted mutators.
/// Passing an autocommit connection releases each guard immediately and provides
/// no protection for subsequent work; callers must supply an active transaction.
///
/// Assign the identity column to itself so the guard does not depend on a mutable
/// descriptive field. Do not rely on a stronger PostgreSQL foreign-key lock:
/// its heap update can detect the unchanged key and use a no-key-update lock,
/// which permits FK key-share checks. Inserts and metadata mutators must adopt
/// this guard explicitly. SQLite serializes writers through its database write
/// lock. The statement preserves stored IDs/names/timestamps; every multi-space
/// caller uses the same order.
pub(crate) async fn lock_spaces_for_mutation(
    conn: &mut AnyConnection,
    space_ids: &[&str],
) -> Result<(), AppError> {
    let mut ordered = space_ids.to_vec();
    ordered.sort_unstable();
    ordered.dedup();
    for space_id in ordered {
        let guarded = sqlx::query("UPDATE spaces SET id = id WHERE id = $1")
            .bind(space_id)
            .execute(&mut *conn)
            .await
            .map_err(AppError::internal)?;
        if guarded.rows_affected() != 1 {
            return Err(AppError::NotFound("Space not found".into()));
        }
    }
    Ok(())
}

/// Check current account and the existing operation policy after acquiring space guards.
/// Reuse this after any later blocking lock before relying on account activity.
pub(crate) async fn require_active_space_mutation(
    conn: &mut AnyConnection,
    space_id: &str,
    user_id: &str,
    allowed_roles: &[&str],
) -> Result<String, AppError> {
    let active: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM users WHERE id = $1 AND is_active = 1")
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    if active.is_none() {
        return Err(AppError::Unauthorized("Account unavailable".into()));
    }
    super::routes::require_space_role_on_connection(conn, space_id, user_id, allowed_roles).await
}

/// Existing page/space policy for metadata writes, evaluated after the space guard.
/// The caller must keep all validation and writes on this transaction connection.
/// This does not grant access from creator attribution, add page grants, or expose drafts.
pub(crate) async fn require_active_document_mutation(
    conn: &mut AnyConnection,
    space_id: &str,
    document_id: &str,
    user_id: &str,
) -> Result<(), AppError> {
    let locked_page = sqlx::query("UPDATE documents SET id = id WHERE id = $1 AND space_id = $2 AND deleted_at IS NULL AND archived_at IS NULL")
        .bind(document_id).bind(space_id).execute(&mut *conn).await.map_err(AppError::internal)?;
    if locked_page.rows_affected() != 1 {
        return Err(AppError::NotFound("Document unavailable".into()));
    }
    let page: Option<(Option<String>, i64, Option<String>)> = sqlx::query_as(
        "SELECT published_at, locked, locked_by FROM documents WHERE id = $1 AND space_id = $2 AND deleted_at IS NULL AND archived_at IS NULL",
    ).bind(document_id).bind(space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let Some((published_at, locked, locked_by)) = page else {
        return Err(AppError::NotFound("Document unavailable".into()));
    };
    // The document lock may have waited while another transaction deactivated
    // this account. Resolve account activity and authority only after that wait.
    require_active_space_mutation(conn, space_id, user_id, &["editor"]).await?;
    if published_at.is_none() {
        let participation: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM document_drafts d WHERE d.document_id = $1 AND d.space_id = $2 AND (d.owner_id = $3 OR EXISTS (SELECT 1 FROM document_draft_access a WHERE a.draft_id = d.id AND a.user_id = $3 AND a.role = 'editor')) LIMIT 1",
        ).bind(document_id).bind(space_id).bind(user_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if participation.is_none() {
            return Err(AppError::NotFound("Document unavailable".into()));
        }
    }
    if locked != 0 && locked_by.as_deref() != Some(user_id) {
        return Err(AppError::Conflict(
            "Document is locked by another user".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        lock_spaces_for_mutation, require_active_document_mutation, require_active_space_mutation,
    };
    use crate::auth::routes::AppError;
    use sqlx::{any::AnyPoolOptions, AnyPool};
    use std::time::Duration;

    #[tokio::test]
    async fn account_activity_is_checked_after_document_lock_and_denial_rolls_back() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("CREATE TRIGGER deactivate_at_document_lock AFTER UPDATE OF id ON documents WHEN NEW.id='target-a' BEGIN UPDATE users SET is_active=0 WHERE id='editor-a'; END")
            .execute(&state.db).await.unwrap();
        let before: (String, String) =
            sqlx::query_as("SELECT title,updated_at FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        let mut tx = state.db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        let result =
            require_active_document_mutation(&mut tx, "space-a", "target-a", "editor-a").await;
        assert!(matches!(result, Err(AppError::Unauthorized(_))));
        tx.rollback().await.unwrap();
        let active: i64 = sqlx::query_scalar("SELECT is_active FROM users WHERE id='editor-a'")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(active, 1);
        let after: (String, String) =
            sqlx::query_as("SELECT title,updated_at FROM documents WHERE id='target-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn private_metadata_requires_editor_participation_even_for_admin() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES ('admin','admin@example.test','Administrator','admin')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES ('draft','target-a','space-a','owner-a','Private','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        for actor in ["admin", "editor-a"] {
            let mut tx = state.db.begin().await.unwrap();
            lock_spaces_for_mutation(&mut tx, &["space-a"])
                .await
                .unwrap();
            assert!(matches!(
                require_active_document_mutation(&mut tx, "space-a", "target-a", actor).await,
                Err(AppError::NotFound(_))
            ));
            tx.rollback().await.unwrap();
        }
        let mut tx = state.db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        sqlx::query("UPDATE document_draft_access SET role='editor' WHERE user_id='editor-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        require_active_document_mutation(&mut tx, "space-a", "target-a", "editor-a")
            .await
            .unwrap();
        sqlx::query("DELETE FROM document_draft_access WHERE user_id='editor-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(matches!(
            require_active_document_mutation(&mut tx, "space-a", "target-a", "editor-a").await,
            Err(AppError::NotFound(_))
        ));
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn current_scope_lifecycle_locks_and_owner_operation_are_enforced() {
        let state = crate::spaces::properties::tests::property_test_state().await;
        let mut tx = state.db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut tx, &["space-a"])
            .await
            .unwrap();
        assert!(matches!(
            require_active_document_mutation(&mut tx, "space-a", "target-b", "owner-a").await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            require_active_space_mutation(&mut tx, "space-a", "editor-a", &["owner"]).await,
            Err(AppError::Forbidden(_))
        ));
        for column in ["archived_at", "deleted_at"] {
            sqlx::query(&format!(
                "UPDATE documents SET {column}='old' WHERE id='target-a'"
            ))
            .execute(&mut *tx)
            .await
            .unwrap();
            assert!(matches!(
                require_active_document_mutation(&mut tx, "space-a", "target-a", "owner-a").await,
                Err(AppError::NotFound(_))
            ));
            sqlx::query(&format!(
                "UPDATE documents SET {column}=NULL WHERE id='target-a'"
            ))
            .execute(&mut *tx)
            .await
            .unwrap();
        }
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        require_active_document_mutation(&mut tx, "space-a", "target-a", "owner-a")
            .await
            .unwrap();
        assert!(matches!(
            require_active_document_mutation(&mut tx, "space-a", "target-a", "editor-a").await,
            Err(AppError::Conflict(_))
        ));
        sqlx::query("UPDATE documents SET locked_by=NULL WHERE id='target-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(matches!(
            require_active_document_mutation(&mut tx, "space-a", "target-a", "owner-a").await,
            Err(AppError::Conflict(_))
        ));
        tx.rollback().await.unwrap();
    }

    async fn fixture(url: &str, connections: u32) -> AnyPool {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(connections)
            .connect(url)
            .await
            .expect("open guard fixture");
        sqlx::query("CREATE TABLE spaces (id TEXT PRIMARY KEY NOT NULL, name TEXT NOT NULL)")
            .execute(&db)
            .await
            .expect("create spaces");
        for id in ["space-a", "space-b"] {
            sqlx::query("INSERT INTO spaces (id, name) VALUES ($1, $1)")
                .bind(id)
                .execute(&db)
                .await
                .expect("seed space");
        }
        db
    }

    #[tokio::test]
    async fn guards_are_sorted_deduplicated_and_do_not_change_space_data() {
        let db = fixture("sqlite::memory:", 1).await;
        sqlx::query("CREATE TABLE guard_order (sequence INTEGER PRIMARY KEY, space_id TEXT)")
            .execute(&db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER observe_guard AFTER UPDATE OF id ON spaces BEGIN INSERT INTO guard_order (space_id) VALUES (NEW.id); END")
            .execute(&db).await.unwrap();
        let before: Vec<(String, String)> =
            sqlx::query_as("SELECT id, name FROM spaces ORDER BY id")
                .fetch_all(&db)
                .await
                .unwrap();
        let mut tx = db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut tx, &["space-b", "space-a", "space-b"])
            .await
            .unwrap();
        let observed: Vec<String> =
            sqlx::query_scalar("SELECT space_id FROM guard_order ORDER BY sequence")
                .fetch_all(&mut *tx)
                .await
                .unwrap();
        assert_eq!(observed, vec!["space-a", "space-b"]);
        tx.commit().await.unwrap();
        let after: Vec<(String, String)> =
            sqlx::query_as("SELECT id, name FROM spaces ORDER BY id")
                .fetch_all(&db)
                .await
                .unwrap();
        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn missing_space_rejects_and_caller_rolls_back_earlier_work() {
        let db = fixture("sqlite::memory:", 1).await;
        let mut tx = db.begin().await.unwrap();
        sqlx::query("UPDATE spaces SET name = 'Tentative' WHERE id = 'space-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        let result = lock_spaces_for_mutation(&mut tx, &["space-a", "space-z-missing"]).await;
        assert!(matches!(result, Err(AppError::NotFound(message)) if message == "Space not found"));
        tx.rollback().await.unwrap();
        let name: String = sqlx::query_scalar("SELECT name FROM spaces WHERE id = 'space-a'")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(name, "space-a");
    }

    #[tokio::test]
    async fn empty_guard_set_does_not_mutate_spaces() {
        let db = fixture("sqlite::memory:", 1).await;
        let mut tx = db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut tx, &[]).await.unwrap();
        tx.commit().await.unwrap();
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM spaces")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn sqlite_competing_writer_waits_then_reads_committed_state() {
        let path = std::env::temp_dir().join(format!(
            "starlane-space-guard-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let db = fixture(&format!("sqlite://{}?mode=rwc", path.display()), 2).await;
        let mut first = db.begin().await.unwrap();
        lock_spaces_for_mutation(&mut first, &["space-a"])
            .await
            .unwrap();
        sqlx::query("UPDATE spaces SET name = 'First committed' WHERE id = 'space-a'")
            .execute(&mut *first)
            .await
            .unwrap();
        let other_db = db.clone();
        let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
        let mut waiting = tokio::spawn(async move {
            let mut tx = other_db.begin().await.unwrap();
            sqlx::query("PRAGMA busy_timeout = 2000")
                .execute(&mut *tx)
                .await
                .unwrap();
            ready_tx.send(()).unwrap();
            lock_spaces_for_mutation(&mut tx, &["space-a"])
                .await
                .unwrap();
            let name: String = sqlx::query_scalar("SELECT name FROM spaces WHERE id = 'space-a'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            name
        });
        tokio::time::timeout(Duration::from_secs(2), ready_rx)
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut waiting)
                .await
                .is_err()
        );
        first.commit().await.unwrap();
        let name = tokio::time::timeout(Duration::from_secs(3), waiting)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(name, "First committed");
        db.close().await;
        std::fs::remove_file(path).unwrap();
    }
}

/// Caller owns the ordered space-first transaction. Lock the active scoped row,
/// then retain the real principal through current typed edit/ancestor/restriction policy.
pub(crate) async fn require_current_page_edit(
    conn: &mut AnyConnection,
    principal: super::access::Principal<'_>,
    space: &str,
    document: &str,
) -> Result<super::access::PageAccess, AppError> {
    let row=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND deleted_at IS NULL AND archived_at IS NULL").bind(document).bind(space).execute(&mut *conn).await.map_err(AppError::internal)?;
    if row.rows_affected() != 1 {
        return Err(AppError::NotFound("Document unavailable".into()));
    }
    super::access::resolve_page_access(
        conn,
        principal,
        space,
        document,
        super::access::PageOperation::Edit,
        super::access::Purpose::Routine,
    )
    .await
}

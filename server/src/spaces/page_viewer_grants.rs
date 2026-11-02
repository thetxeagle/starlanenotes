//! Dormant page-only viewer proof. No insert/update/delete or HTTP API exists.
//! Binding fields are immutable in the future management contract; revision and
//! complete row equality are fresh proof, never a permission-bearing token.
use crate::auth::routes::AppError;
use serde::Serialize;
use sqlx::AnyConnection;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DirectViewerGrant {
    pub id: String,
    pub document_id: String,
    pub space_id: String,
    pub user_id: String,
    pub role: String,
    pub scope: String,
    pub revision: String,
    pub created_by: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Captures every grant column, current recipient activity and actual scoped
/// active published target. A grant cannot open a lifecycle or unpublished row.
pub(crate) async fn read_current_direct_viewer_grant(
    conn: &mut AnyConnection,
    space: &str,
    document: &str,
    actor: &str,
) -> Result<Option<DirectViewerGrant>, AppError> {
    let row: Option<(String,String,String,String,String,String,String,String,String,String,i64)> = sqlx::query_as(
        "SELECT g.id,g.document_id,g.space_id,g.user_id,g.role,g.scope,g.revision,g.created_by,g.created_at,g.updated_at,u.is_active FROM document_viewer_grants g JOIN users u ON u.id=g.user_id WHERE g.document_id=$1 AND g.user_id=$2"
    ).bind(document).bind(actor).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let Some((id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at,active)) = row else { return Ok(None) };
    if space_id != space || document_id != document || user_id != actor || role != "viewer" || scope != "page" || [id.as_str(),revision.as_str(),created_by.as_str(),created_at.as_str(),updated_at.as_str()].iter().any(|value| value.trim().is_empty()) {
        return Err(AppError::Conflict("Access policy is invalid".into()));
    }
    if active != 1 { return Ok(None) }
    let published: Option<i64> = sqlx::query_scalar("SELECT 1 FROM documents WHERE id=$1 AND space_id=$2 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL")
        .bind(document).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    if published.is_none() { return Ok(None) }
    Ok(Some(DirectViewerGrant { id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at }))
}

/// Complete grant and current recipient-kind/activity evidence for a selected
/// page. These are raw source dependencies, never cached authorization.
pub(crate) async fn capture_page_dependencies(
    conn: &mut AnyConnection, space: &str, document: &str,
) -> Result<Vec<super::published_source::SourceProof>, AppError> {
    let mut proofs = Vec::new();
    for query in [
        "SELECT * FROM document_viewer_grants WHERE document_id=$1 AND space_id=$2",
        "SELECT u.id,u.role,u.is_active,u.account_kind FROM users u WHERE EXISTS(SELECT 1 FROM document_viewer_grants g WHERE g.user_id=u.id AND g.document_id=$1 AND g.space_id=$2)",
    ] {
        let rows = super::document_deletion::rows(conn,query,&[document,space]).await.map_err(AppError::internal)?;
        proofs.push(super::published_source::SourceProof { query:query.into(), parameters:vec![document.into(),space.into()], rows });
    }
    Ok(proofs)
}
/// Audience/source inventory for an existing space; selected readers should use
/// page dependencies instead. No labels or email addresses are projected.
pub(crate) async fn capture_space_dependencies(
    conn: &mut AnyConnection, space: &str,
) -> Result<serde_json::Value, AppError> {
    let grants=super::document_deletion::rows(conn,"SELECT * FROM document_viewer_grants WHERE space_id=$1",&[space]).await.map_err(AppError::internal)?;
    let recipients=super::document_deletion::rows(conn,"SELECT u.id,u.role,u.is_active,u.account_kind FROM users u WHERE EXISTS(SELECT 1 FROM document_viewer_grants g WHERE g.user_id=u.id AND g.space_id=$1)",&[space]).await.map_err(AppError::internal)?;
    Ok(serde_json::json!({"grants":grants,"recipients":recipients}))
}

#[cfg(test)]
mod witness_tests {
    use super::*;
    use crate::spaces::{properties::tests::{property_test_state,test_auth},published_source,document_deletion,space_deletion};
    use uuid::Uuid;
    async fn fixture(wal:bool)->crate::AppState {
        let mut state=property_test_state().await;
        if wal {
            state.db.close().await;
            let file=std::env::temp_dir().join(format!("viewer-witness-{}.sqlite",Uuid::new_v4()));
            state.db=sqlx::any::AnyPoolOptions::new().max_connections(3).after_connect(|c,_|Box::pin(async move {
                sqlx::query("PRAGMA foreign_keys=ON").execute(&mut *c).await?;
                sqlx::query("PRAGMA journal_mode=WAL").execute(&mut *c).await?;
                sqlx::query("PRAGMA busy_timeout=5000").execute(c).await?;Ok(())
            })).connect(&format!("sqlite://{}?mode=rwc",file.display())).await.unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for id in ["owner-a","viewer-a"] {
                sqlx::query("INSERT INTO users(id,email,display_name,role) VALUES($1,$2,$1,'member')").bind(id).bind(format!("{id}@example.test")).execute(&state.db).await.unwrap();
                sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'fixture','2999-01-01T00:00:00Z')").bind(format!("metadata-{id}")).bind(id).execute(&state.db).await.unwrap();
            }
            for space in ["space-a","space-b"] {
                sqlx::query("INSERT INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private','owner-a')").bind(space).execute(&state.db).await.unwrap();
            }
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner-member','space-a','owner-a','owner')").execute(&state.db).await.unwrap();
            for (id,space) in [("target-a","space-a"),("target-b","space-b")] {
                sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,$2,$1,'','owner-a','owner-a','published')").bind(id).bind(space).execute(&state.db).await.unwrap();
            }
            assert!(std::fs::metadata(&file).unwrap().len()>0);
            let mode:String=sqlx::query_scalar("PRAGMA journal_mode").fetch_one(&state.db).await.unwrap();assert_eq!(mode.to_lowercase(),"wal");
        }
        sqlx::query("PRAGMA foreign_keys=ON").execute(&state.db).await.unwrap();
        for (id,page,space) in [("grant-a","target-a","space-a"),("grant-b","target-b","space-b")] {
            sqlx::query("INSERT INTO document_viewer_grants(id,document_id,space_id,user_id,role,scope,revision,created_by,created_at,updated_at) VALUES($1,$2,$3,'viewer-a','viewer','page','original','owner-a','created','updated')").bind(id).bind(page).bind(space).execute(&state.db).await.unwrap();
        }
        state
    }
    #[tokio::test]
    async fn viewer_raw_source_rejects_wal_grant_revision_and_recipient_kind_changes() {
        for mutation in ["UPDATE document_viewer_grants SET revision='new' WHERE id='grant-a'","UPDATE users SET role='guest' WHERE id='viewer-a'"] {
            let state=fixture(true).await;let mut first=state.db.acquire().await.unwrap();
            let proof=published_source::capture_source(&mut first,&test_auth("owner-a"),"space-a","target-a",&[]).await.unwrap();
            // A separate connection commits the change after original capture.
            sqlx::query(mutation).execute(&state.db).await.unwrap();
            assert!(published_source::verify_source(&mut first,&proof).await.is_err());
        }
    }
    #[tokio::test]
    async fn viewer_selected_source_keeps_unrelated_space_grants_outside_page_dependency() {
        let state=fixture(false).await;let mut conn=state.db.acquire().await.unwrap();
        let proof=capture_page_dependencies(&mut conn,"space-a","target-a").await.unwrap();
        sqlx::query("UPDATE document_viewer_grants SET revision='foreign-new' WHERE id='grant-b'").execute(&mut *conn).await.unwrap();
        published_source::verify_source(&mut conn,&proof).await.unwrap();
        assert_eq!(proof[0].rows.len(),1);assert_eq!(proof[0].rows[0]["id"],"grant-a");
    }
    #[tokio::test]
    async fn viewer_page_delete_exact_grants_and_unrelated_anchor_survive() {
        let state=fixture(false).await;let mut tx=state.db.begin().await.unwrap();
        sqlx::query("UPDATE documents SET deleted_at='trash' WHERE id='target-a'").execute(&mut *tx).await.unwrap();
        let before=document_deletion::rows(&mut tx,"SELECT * FROM document_viewer_grants WHERE id='grant-b'",&[]).await.unwrap();
        let prepared=document_deletion::prepare(&mut tx,&test_auth("owner-a"),"space-a","target-a",None).await.unwrap();
        document_deletion::delete_prepared(&mut tx,&test_auth("owner-a"),&prepared).await.unwrap();
        document_deletion::verify_authority(&mut tx,&test_auth("owner-a"),&prepared).await.unwrap();
        tx.commit().await.unwrap();let mut conn=state.db.acquire().await.unwrap();
        assert_eq!(document_deletion::rows(&mut conn,"SELECT * FROM document_viewer_grants",&[]).await.unwrap(),before);
    }
    #[tokio::test]
    async fn viewer_page_delete_foreign_grant_trigger_rolls_back_full_transaction() {
        let state=fixture(false).await;
        sqlx::query("UPDATE documents SET deleted_at='trash' WHERE id='target-a'").execute(&state.db).await.unwrap();
        sqlx::query("CREATE TRIGGER corrupt_foreign_grant AFTER DELETE ON document_viewer_grants WHEN OLD.id='grant-a' BEGIN UPDATE document_viewer_grants SET revision='tampered' WHERE id='grant-b'; END").execute(&state.db).await.unwrap();
        let mut tx=state.db.begin().await.unwrap();let before=document_deletion::rows(&mut tx,"SELECT * FROM document_viewer_grants",&[]).await.unwrap();
        let prepared=document_deletion::prepare(&mut tx,&test_auth("owner-a"),"space-a","target-a",None).await.unwrap();
        assert!(document_deletion::delete_prepared(&mut tx,&test_auth("owner-a"),&prepared).await.is_err());tx.rollback().await.unwrap();
        let mut conn=state.db.acquire().await.unwrap();assert_eq!(document_deletion::rows(&mut conn,"SELECT * FROM document_viewer_grants",&[]).await.unwrap(),before);
        assert_eq!(document_deletion::rows(&mut conn,"SELECT id FROM documents WHERE id='target-a'",&[]).await.unwrap().len(),1);
    }
    #[tokio::test]
    async fn viewer_space_delete_removes_only_exact_grants_with_actual_atomic_audit() {
        let state=fixture(false).await;let mut conn=state.db.acquire().await.unwrap();
        let before=document_deletion::rows(&mut conn,"SELECT * FROM document_viewer_grants WHERE id='grant-b'",&[]).await.unwrap();drop(conn);
        space_deletion::delete(&state,&test_auth("owner-a"),"space-a").await.unwrap();let mut conn=state.db.acquire().await.unwrap();
        assert_eq!(document_deletion::rows(&mut conn,"SELECT * FROM document_viewer_grants",&[]).await.unwrap(),before);
        assert_eq!(document_deletion::rows(&mut conn,"SELECT action,user_id,target_id FROM audit_log WHERE action='space.delete'",&[]).await.unwrap(),vec![serde_json::json!({"action":"space.delete","user_id":"owner-a","target_id":"space-a"})]);
    }
    #[tokio::test]
    async fn viewer_space_delete_foreign_grant_trigger_keeps_pages_grants_and_audit_exact() {
        let state=fixture(false).await;
        sqlx::query("CREATE TRIGGER space_grant_tamper AFTER DELETE ON document_viewer_grants WHEN OLD.id='grant-a' BEGIN UPDATE document_viewer_grants SET revision='tampered' WHERE id='grant-b'; END").execute(&state.db).await.unwrap();
        let mut conn=state.db.acquire().await.unwrap();let mut before=Vec::new();
        for table in ["spaces","documents","document_viewer_grants","audit_log"] {before.push(document_deletion::rows(&mut conn,&format!("SELECT * FROM {table}"),&[]).await.unwrap());}drop(conn);
        assert!(space_deletion::delete(&state,&test_auth("owner-a"),"space-a").await.is_err());
        let mut conn=state.db.acquire().await.unwrap();let mut after=Vec::new();for table in ["spaces","documents","document_viewer_grants","audit_log"] {after.push(document_deletion::rows(&mut conn,&format!("SELECT * FROM {table}"),&[]).await.unwrap());}assert_eq!(after,before);
    }

}

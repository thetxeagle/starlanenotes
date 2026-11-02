use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space_id}/documents/{doc_id}/versions",
            get(list_versions),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}",
            get(get_version),
        )
        .route(
            "/spaces/{space_id}/documents/{doc_id}/versions/{version_id}/restore",
            axum::routing::post(restore_version),
        )
}

#[derive(Serialize)]
pub struct VersionResponse {
    pub id: String,
    pub version_number: i64,
    pub lines_added: i64,
    pub lines_removed: i64,
    pub created_by: String,
    pub author_name: String,
    pub created_at: String,
}

async fn version_rows(conn:&mut sqlx::AnyConnection,doc:&str,version:Option<&str>)->Result<Vec<serde_json::Value>,AppError> {
    let q=sqlx::query("SELECT v.*,u.display_name AS author_name FROM doc_versions v JOIN users u ON u.id=v.created_by WHERE v.document_id=$1 AND ($2 IS NULL OR v.id=$2) ORDER BY v.version_number DESC").bind(doc).bind(version);
    use sqlx::{Row,Column};
    let mut out=Vec::new();
    for row in q.fetch_all(conn).await.map_err(AppError::internal)? {
        let mut value=serde_json::Map::new();
        for col in row.columns() {let name=col.name();let v=if let Ok(v)=row.try_get::<Option<String>,_>(name){v.map(serde_json::Value::String).unwrap_or(serde_json::Value::Null)}else if let Ok(v)=row.try_get::<Option<i64>,_>(name){v.map(|v|serde_json::json!(v)).unwrap_or(serde_json::Value::Null)}else if let Ok(v)=row.try_get::<Option<Vec<u8>>,_>(name){v.map(|v|serde_json::json!({"binary_hex":hex::encode(v)})).unwrap_or(serde_json::Value::Null)}else{return Err(AppError::Conflict("Version source unavailable".into()));};value.insert(name.into(),v);}
        out.push(serde_json::Value::Object(value));
    }
    Ok(out)
}
fn version_response(row:&serde_json::Value)->Result<VersionResponse,AppError> {
    use super::draft_save::string;
    let number=|key:&str|row[key].as_i64().ok_or_else(||AppError::Conflict("Version source unavailable".into()));
    Ok(VersionResponse{id:string(row,"id")?,version_number:number("version_number")?,lines_added:number("lines_added")?,lines_removed:number("lines_removed")?,created_by:string(row,"created_by")?,author_name:string(row,"author_name")?,created_at:string(row,"created_at")?})
}
async fn read_versions(state:&AppState,auth:&AuthUser,space:&str,doc:&str,version:Option<&str>)->Result<Vec<VersionResponse>,AppError> {
    use super::access::{self,PageOperation,Purpose};
    let mut conn=state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_read(&mut conn,auth).await?;
    let principal=access::principal_from_auth(auth)?;
    access::resolve_page_access(&mut conn,principal,space,doc,PageOperation::View,Purpose::Routine).await?;
    let proof=super::drafts::capture_draft_reader_inventory(&mut conn,space,&auth.claims.sub).await?;
    let rows=version_rows(&mut conn,doc,version).await?;
    #[cfg(test)] version_reader_before_inventory().await;
    super::drafts::verify_draft_reader_inventory(&mut conn,&proof).await?;
    #[cfg(test)] version_reader_after_inventory().await;
    if version_rows(&mut conn,doc,version).await?!=rows {return Err(AppError::Conflict("Version source changed; refresh before continuing".into()));}
    access::resolve_page_access(&mut conn,principal,space,doc,PageOperation::View,Purpose::Routine).await?;
    access::require_current_human_read(&mut conn,auth).await?;
    if version.is_some() && rows.is_empty(){return Err(AppError::BadRequest("Version not found".into()));}
    rows.iter().map(version_response).collect()
}
async fn list_versions(State(state):State<AppState>,auth:AuthUser,Path((space_id,doc_id)):Path<(String,String)>)->Result<impl IntoResponse,AppError> {
    Ok(Json(read_versions(&state,&auth,&space_id,&doc_id,None).await?))
}
async fn get_version(State(state):State<AppState>,auth:AuthUser,Path((space_id,doc_id,version_id)):Path<(String,String,String)>)->Result<impl IntoResponse,AppError> {
    let row=read_versions(&state,&auth,&space_id,&doc_id,Some(&version_id)).await?.into_iter().next().ok_or_else(||AppError::BadRequest("Version not found".into()))?;
    Ok(Json(row))
}
#[cfg(test)] tokio::task_local! {
    pub(crate) static VERSION_READER_BEFORE_INVENTORY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
    pub(crate) static VERSION_READER_AFTER_INVENTORY:std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()>+Send>>>>;
}
#[cfg(test)] async fn version_reader_before_inventory(){if let Ok(Some(h))=VERSION_READER_BEFORE_INVENTORY.try_with(|s|s.borrow_mut().take()){h.await;}}
#[cfg(test)] async fn version_reader_after_inventory(){if let Ok(Some(h))=VERSION_READER_AFTER_INVENTORY.try_with(|s|s.borrow_mut().take()){h.await;}}

async fn restore_version(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id, version_id)): Path<(String, String, String)>,
) -> Result<Json<serde_json::Value>, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;
    require_document_in_space(&state, &space_id, &doc_id).await?;

    let version_exists: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM doc_versions WHERE id = $1 AND document_id = $2")
            .bind(&version_id)
            .bind(&doc_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    if version_exists.is_none() {
        return Err(AppError::BadRequest("Version not found".into()));
    }

    Err(AppError::BadRequest(
        "Version restore is unavailable; no page content was changed.".into(),
    ))
}

async fn require_document_in_space(
    state: &AppState,
    space_id: &str,
    doc_id: &str,
) -> Result<(), AppError> {
    let exists: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM documents WHERE id = $1 AND space_id = $2")
            .bind(doc_id)
            .bind(space_id)
            .fetch_optional(&state.db)
            .await
            .map_err(AppError::internal)?;

    if exists.is_none() {
        return Err(AppError::NotFound(
            "Document not found in this space".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{get_version, list_versions, restore_version};
    use crate::auth::jwt::{Claims, TokenType};
    use crate::auth::middleware::AuthUser;
    use crate::auth::routes::AppError;
    use crate::AppState;
    use axum::extract::{Path, State};
    use sqlx::any::AnyPoolOptions;
    use uuid::Uuid;

    async fn version_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        let index_path =
            std::env::temp_dir().join(format!("starlane-notes-versions-test-{}", Uuid::new_v4()));
        AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".to_string(),
            base_url: "http://localhost:3000".to_string(),
            search_index: crate::search::SearchIndex::new(index_path).expect("search index"),
            oauth_cache: crate::auth::routes::OAuthStateCache::new(),
            ws_rooms: crate::sync::WsRooms::default(),
        }
    }

    fn test_auth(user_id: &str) -> AuthUser {
        AuthUser {
            claims: Claims {
                sub: user_id.to_string(),
                sid: format!("session-{user_id}"),
                email: format!("{user_id}@example.test"),
                role: "member".to_string(),
                exp: i64::MAX,
                iat: 0,
                token_type: TokenType::Access,
            },
            api_key_scopes: None,
        }
    }

    async fn seed_version_fixture(state: &AppState) {
        for (space_id, user_id, role) in [
            ("space-a", "viewer-a", "viewer"),
            ("space-b", "viewer-b", "viewer"),
            ("space-a", "editor-a", "editor"),
            ("space-b", "editor-b", "editor"),
        ] {
            sqlx::query(
                "INSERT OR IGNORE INTO users(id,email,role,display_name) VALUES($1,$1,'member',$1)",
            )
            .bind(user_id)
            .execute(&state.db)
            .await
            .unwrap();
            sqlx::query("INSERT OR IGNORE INTO spaces(id,name,slug,visibility,created_by) VALUES($1,$1,$1,'private',$2)")
                .bind(space_id).bind(user_id)
                .execute(&state.db)
                .await
                .unwrap();
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'hash','2999-01-01T00:00:00Z')").bind(format!("session-{user_id}")).bind(user_id).execute(&state.db).await.unwrap();
            sqlx::query("INSERT INTO space_members(id,space_id,user_id,role) VALUES($2,$1,$2,$3)")
                .bind(space_id)
                .bind(user_id)
                .bind(role)
                .execute(&state.db)
                .await
                .unwrap();
        }
        for (doc_id, space_id) in [
            ("doc-a", "space-a"),
            ("doc-b", "space-a"),
            ("doc-c", "space-b"),
        ] {
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,$2,$1,'unused','editor-a','editor-a','2026-01-01T00:00:00Z')")
                .bind(doc_id)
                .bind(space_id)
                .execute(&state.db)
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO users(id,email,display_name) VALUES('outsider','outsider@example.test','Outsider')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('session-outsider','outsider','hash','2999-01-01T00:00:00Z')").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO doc_versions (id, document_id, version_number, created_by, change_set) \
             VALUES ('version-a', 'doc-a', 1, 'editor-a', X'0102')",
        )
        .execute(&state.db)
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn version_reads_require_space_view_and_bind_document_ids() {
        let state = version_test_state().await;
        seed_version_fixture(&state).await;

        assert!(matches!(
            list_versions(
                State(state.clone()),
                test_auth("outsider"),
                Path(("space-a".into(), "doc-a".into())),
            )
            .await,
            Err(AppError::NotFound(_))
        ));

        assert!(list_versions(
            State(state.clone()),
            test_auth("viewer-a"),
            Path(("space-a".into(), "doc-a".into())),
        )
        .await
        .is_ok());

        assert!(matches!(
            list_versions(
                State(state.clone()),
                test_auth("viewer-b"),
                Path(("space-b".into(), "doc-a".into())),
            )
            .await,
            Err(AppError::NotFound(_))
        ));

        assert!(get_version(
            State(state.clone()),
            test_auth("viewer-a"),
            Path(("space-a".into(), "doc-a".into(), "version-a".into())),
        )
        .await
        .is_ok());

        assert!(matches!(
            get_version(
                State(state.clone()),
                test_auth("viewer-a"),
                Path(("space-a".into(), "doc-b".into(), "version-a".into())),
            )
            .await,
            Err(AppError::BadRequest(_))
        ));

        assert!(matches!(
            get_version(
                State(state),
                test_auth("viewer-b"),
                Path(("space-b".into(), "doc-c".into(), "version-a".into())),
            )
            .await,
            Err(AppError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn restore_is_unsupported_and_does_not_create_a_history_row() {
        let state = version_test_state().await;
        seed_version_fixture(&state).await;

        assert!(matches!(
            restore_version(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "doc-b".into(), "version-a".into())),
            )
            .await,
            Err(AppError::BadRequest(_))
        ));
        let version_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_versions")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(version_count, 1);

        assert!(matches!(
            restore_version(
                State(state.clone()),
                test_auth("editor-a"),
                Path(("space-a".into(), "doc-a".into(), "version-a".into())),
            )
            .await,
            Err(AppError::BadRequest(message))
                if message == "Version restore is unavailable; no page content was changed."
        ));
        let version_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM doc_versions")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(version_count, 1);

        assert!(matches!(
            restore_version(
                State(state),
                test_auth("editor-b"),
                Path(("space-b".into(), "doc-a".into(), "version-a".into())),
            )
            .await,
            Err(AppError::NotFound(_))
        ));
    }
}


#[cfg(test)]
mod version_reader_current_tests {
    use super::*;
    use super::super::drafts::draft_reader_current_tests as fixture;
    use axum::http::StatusCode;
    async fn setup()->(AppState,String,std::path::PathBuf){let (s,slug,path)=fixture::fixture().await;sqlx::query("INSERT INTO doc_versions(id,document_id,version_number,created_by,change_set) VALUES('version-1','doc-1',1,'owner-1',X'0102')").execute(&s.db).await.unwrap();(s,slug,path)}
    #[tokio::test]
    async fn actual_signed_version_metadata_has_current_page_and_exact_document_scope(){
        let (s,slug,path)=setup().await;let before=fixture::snapshot(&s,true).await;
        for url in ["/spaces/space-1/documents/doc-1/versions","/spaces/space-1/documents/doc-1/versions/version-1"] {
            let (status,v)=fixture::request(&s,"editor-1",url).await;assert_eq!(status,StatusCode::OK,"{v}");assert!(v.to_string().contains("owner-1"));assert!(!v.to_string().contains("change_set"));
            let (denied,v)=fixture::request(&s,"viewer-1",url).await;assert_eq!(denied,StatusCode::NOT_FOUND,"{v}");assert!(!v.to_string().contains("owner-1"));
        }
        let (wrong,v)=fixture::request(&s,"editor-1","/spaces/foreign/documents/doc-1/versions/version-1").await;assert_ne!(wrong,StatusCode::OK);assert!(!v.to_string().contains("owner-1"));
        assert_eq!(fixture::snapshot(&s,true).await,before);
        sqlx::query("UPDATE documents SET published_at=updated_at WHERE id='doc-1'").execute(&s.db).await.unwrap();
        assert_eq!(fixture::request(&s,"viewer-1","/spaces/space-1/documents/doc-1/versions").await.0,StatusCode::OK);
        assert_eq!(fixture::request(&s,"viewer-1","/spaces/space-1/documents/doc-1/versions/missing").await.0,StatusCode::BAD_REQUEST);
        fixture::cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn actual_version_read_key_and_custom_view_positive_write_service_denied(){
        use axum::{body::Body,http::Request};use tower::ServiceExt;
        let (s,slug,path)=setup().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('custom-reader','space-1','Custom reader','[\"view\"]','owner-1')").execute(&s.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom-reader' WHERE user_id='editor-1'").execute(&s.db).await.unwrap();
        for (id,scopes,service) in [("version-read",r#"["read"]"#,0_i64),("version-write",r#"["write"]"#,0),("version-service",r#"["read","admin"]"#,1)] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes,is_service_account) VALUES($1,'editor-1',$1,$2,'test',$3,$4)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(scopes).bind(service).execute(&s.db).await.unwrap();
            let r=crate::api_routes().with_state(s.clone()).oneshot(Request::builder().uri("/spaces/space-1/documents/doc-1/versions/version-1").header("X-API-Key",id).body(Body::empty()).unwrap()).await.unwrap();if id=="version-read" {assert_eq!(r.status(),StatusCode::OK);}else{assert_ne!(r.status(),StatusCode::OK);}
        }
        fixture::cleanup(s,&slug,&path).await;
    }
    #[tokio::test]
    async fn disk_wal_version_inventory_rejects_new_history_and_author_label_same_stamp(){
        for sql in ["INSERT INTO doc_versions(id,document_id,version_number,created_by,change_set) VALUES('new-version','doc-1',2,'owner-1',X'0304')","UPDATE users SET display_name='NEW AUTHOR LABEL' WHERE id='owner-1'","UPDATE doc_versions SET lines_added=17 WHERE id='version-1'"] {
            let (s,slug,path)=setup().await;let a=fixture::auth(&s,"editor-1").await;let db=s.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
            let r=VERSION_READER_BEFORE_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query(sql).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);}))),read_versions(&s,&a,"space-1","doc-1",None)).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert!(matches!(r,Err(AppError::Conflict(_))));assert_eq!(fixture::request(&s,"editor-1","/spaces/space-1/documents/doc-1/versions").await.0,StatusCode::OK);fixture::cleanup(s,&slug,&path).await;
        }
    }
    #[tokio::test]
    async fn disk_wal_version_final_policy_rejects_block_share_and_session_revocation(){
        for sql in ["INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-1','editor-1','owner-1')","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('doc-1','space-1','restricted','r1','owner-1')","DELETE FROM document_draft_access WHERE draft_id='draft-1' AND user_id='editor-1'","DELETE FROM sessions WHERE id='publication-editor-1'"] {
            let (s,slug,path)=setup().await;let a=fixture::auth(&s,"editor-1").await;let db=s.db.clone();let reached=std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));let flag=reached.clone();
            let r=VERSION_READER_AFTER_INVENTORY.scope(std::cell::RefCell::new(Some(Box::pin(async move{sqlx::query(sql).execute(&db).await.unwrap();flag.store(true,std::sync::atomic::Ordering::SeqCst);}))),read_versions(&s,&a,"space-1","doc-1",Some("version-1"))).await;
            assert!(reached.load(std::sync::atomic::Ordering::SeqCst));assert!(r.is_err());if !sql.starts_with("DELETE FROM sessions") {let mut c=s.db.acquire().await.unwrap();assert!(super::super::access::require_current_human_read(&mut c,&a).await.is_ok(),"original Read identity survives resource loss");}
            let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM doc_versions").fetch_one(&s.db).await.unwrap();assert_eq!(count,1);fixture::cleanup(s,&slug,&path).await;
        }
    }
}

//! SQL-only page references. No draft/body/publication or permission changes.
use super::{access, mutation_guards};
use crate::{auth::middleware::AuthUser, auth::routes::AppError, AppState};
use axum::{
    extract::{Path, State},
    routing::patch,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Column, Row};

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/spaces/{space}/documents/{document}/identifier",
        patch(set_identifier),
    )
}
pub(crate) fn normalize_identifier(
    value: Option<&str>,
) -> Result<(Option<String>, Option<String>), AppError> {
    let Some(value) = value.map(str::trim).filter(|v| !v.is_empty()) else {
        return Ok((None, None));
    };
    if value.len() > 64
        || !value.is_ascii()
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
    {
        return Err(AppError::BadRequest("Identifier must be 1–64 ASCII characters, start with a letter or digit, and use only letters, digits, period, underscore or hyphen".into()));
    }
    Ok((Some(value.into()), Some(value.to_ascii_uppercase())))
}
pub(crate) async fn ensure_identifier_available(
    conn: &mut AnyConnection,
    space: &str,
    key: Option<&str>,
    except: Option<&str>,
) -> Result<(), AppError> {
    let Some(key) = key else { return Ok(()) };
    let exists:Option<i64>=sqlx::query_scalar("SELECT 1 FROM documents WHERE space_id=$1 AND human_identifier_key=$2 AND ($3 IS NULL OR id<>$3)").bind(space).bind(key).bind(except).fetch_optional(conn).await.map_err(AppError::internal)?;
    if exists.is_some() {
        return Err(AppError::Conflict(
            "Identifier is already reserved in this space, including archived or trashed pages"
                .into(),
        ));
    }
    Ok(())
}
fn conflict() -> AppError {
    AppError::Conflict(
        "Identifier or page source changed; retain the request and review current metadata".into(),
    )
}
fn nullable(v: Option<&Value>) -> Result<Option<String>, AppError> {
    match v {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(AppError::BadRequest(
            "Identifier and nullable source fields must be supplied as a string or null".into(),
        )),
    }
}
fn timestamp(s: &str) -> Result<String, AppError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&chrono::Utc).to_rfc3339())
        .map_err(|_| AppError::BadRequest("Source timestamp must be RFC3339".into()))
}
fn same_stamp(a: &str, b: &str) -> bool {
    a == b || matches!((timestamp(a),timestamp(b)),(Ok(a),Ok(b)) if a==b)
}
#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct IdentifierOutcome {
    pub operation_id: String,
    pub human_identifier: Option<String>,
    pub page_updated_at: String,
    pub published_at: Option<String>,
    pub historical: bool,
}
#[derive(Clone, Deserialize, Serialize, PartialEq)]
struct Receipt {
    entity_type: String,
    action: String,
    provider: String,
    operation_id: String,
    actor_id: String,
    document_id: String,
    space_id: String,
    payload: Value,
    before: Value,
    after: Value,
    outcome: IdentifierOutcome,
}
fn fingerprint(v: &Value) -> Result<String, AppError> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(v).map_err(AppError::internal)?,
    )))
}
/// Raw source equality is data evidence, never an authority capability.
async fn rows(
    conn: &mut AnyConnection,
    query: &str,
    args: &[&str],
) -> Result<Vec<Value>, AppError> {
    let mut q = sqlx::query(query);
    for arg in args {
        q = q.bind(*arg);
    }
    let mut values = Vec::new();
    for row in q.fetch_all(conn).await.map_err(AppError::internal)? {
        let mut map = serde_json::Map::new();
        for c in row.columns() {
            let n = c.name();
            let v = if let Ok(v) = row.try_get::<Option<String>, _>(n) {
                v.map(Value::String).unwrap_or(Value::Null)
            } else if let Ok(v) = row.try_get::<Option<i64>, _>(n) {
                v.map(|v| json!(v)).unwrap_or(Value::Null)
            } else if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(n) {
                v.map(|v| json!({"binary":hex::encode(v)}))
                    .unwrap_or(Value::Null)
            } else {
                return Err(AppError::Internal(
                    "Page metadata source cannot be inspected".into(),
                ));
            };
            map.insert(n.into(), v);
        }
        values.push(Value::Object(map));
    }
    values.sort_by_cached_key(Value::to_string);
    Ok(values)
}
async fn page(conn: &mut AnyConnection, space: &str, id: &str) -> Result<Value, AppError> {
    rows(
        conn,
        "SELECT * FROM documents WHERE id=$1 AND space_id=$2",
        &[id, space],
    )
    .await?
    .into_iter()
    .next()
    .ok_or_else(|| AppError::NotFound("Page unavailable".into()))
}
async fn draft_sources(conn: &mut AnyConnection, id: &str) -> Result<Value, AppError> {
    Ok(
        json!({"drafts":rows(conn,"SELECT * FROM document_drafts WHERE document_id=$1",&[id]).await?,"shares":rows(conn,"SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1",&[id]).await?}),
    )
}
async fn edit(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
) -> Result<(), AppError> {
    access::resolve_page_access(
        conn,
        access::principal_from_auth(auth)?,
        space,
        id,
        access::PageOperation::Edit,
        access::Purpose::Routine,
    )
    .await
    .map(|_| ())
}
pub(crate) async fn edit_allowed(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
) -> Result<bool, AppError> {
    match edit(conn, auth, space, id).await {
        Ok(()) => Ok(true),
        Err(AppError::NotFound(_)) | Err(AppError::Forbidden(_)) | Err(AppError::Conflict(_)) => {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}
async fn stored(
    conn: &mut AnyConnection,
    provider: &str,
    id: &str,
) -> Result<Option<(Receipt, Value)>, AppError> {
    let raw = rows(
        conn,
        "SELECT * FROM document_identifier_receipts WHERE provider=$1 AND incoming_id=$2",
        &[provider, id],
    )
    .await?;
    let Some(row) = raw.into_iter().next() else {
        return Ok(None);
    };
    let receipt: Receipt = serde_json::from_str(row["receipt_json"].as_str().ok_or_else(conflict)?)
        .map_err(|_| conflict())?;
    Ok(Some((receipt, row)))
}
fn expected_row(r: &Receipt) -> Result<Value, AppError> {
    Ok(
        json!({"provider":r.provider,"incoming_id":r.operation_id,"actor_id":r.actor_id,"document_id":r.document_id,"space_id":r.space_id,"payload_fingerprint":fingerprint(&r.payload)?,"receipt_json":serde_json::to_string(r).map_err(AppError::internal)?}),
    )
}
/// ACK-only historical proof: no current page read or reassignment, and only the recorded outcome.
pub(crate) async fn historical(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    id: &str,
    payload: &Value,
) -> Result<Option<IdentifierOutcome>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_preference_write(&mut conn, auth).await?;
    let Some((r, row)) = stored(&mut conn, provider, id).await? else {
        return Ok(None);
    };
    if r.entity_type != "document"
        || r.action != "update_identifier"
        || r.provider != provider
        || r.operation_id != id
        || r.actor_id != auth.claims.sub
        || r.payload != *payload
        || r.document_id != payload["id"].as_str().unwrap_or("")
        || r.space_id != payload["space_id"].as_str().unwrap_or("")
        || r.outcome.operation_id != id
        || r.outcome.historical
        || r.outcome.human_identifier != r.after["human_identifier"].as_str().map(str::to_string)
        || r.outcome.page_updated_at != r.after["updated_at"].as_str().unwrap_or("")
        || r.outcome.published_at != r.after["published_at"].as_str().map(str::to_string)
        || row != expected_row(&r)?
    {
        return Err(conflict());
    }
    if rows(
        &mut conn,
        "SELECT * FROM document_identifier_receipts WHERE provider=$1 AND incoming_id=$2",
        &[provider, id],
    )
    .await?
        != vec![row]
    {
        return Err(conflict());
    }
    access::require_current_human_preference_write(&mut conn, auth).await?;
    let mut outcome = r.outcome;
    outcome.historical = true;
    Ok(Some(outcome))
}
fn validate_payload(
    payload: &Value,
) -> Result<(String, String, Option<String>, Option<String>), AppError> {
    let o = payload
        .as_object()
        .ok_or_else(|| AppError::BadRequest("Identifier request must be an object".into()))?;
    if o.len() != 4
        || !["id", "space_id", "human_identifier", "source"]
            .iter()
            .all(|k| o.contains_key(*k))
    {
        return Err(AppError::BadRequest(
            "Identifier request requires exact identity, intended value and source".into(),
        ));
    }
    let string = |k: &str| {
        o.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                AppError::BadRequest("Page identity must be an exact nonempty string".into())
            })
    };
    let source = o["source"].as_object().ok_or_else(conflict)?;
    if source.len() != 3
        || !["page_updated_at", "published_at", "human_identifier"]
            .iter()
            .all(|k| source.contains_key(*k))
    {
        return Err(conflict());
    }
    timestamp(source["page_updated_at"].as_str().ok_or_else(conflict)?)?;
    if let Some(t) = nullable(source.get("published_at"))? {
        timestamp(&t)?;
    }
    nullable(source.get("human_identifier"))?;
    let (display, key) = normalize_identifier(nullable(o.get("human_identifier"))?.as_deref())?;
    Ok((string("space_id")?, string("id")?, display, key))
}
pub(crate) async fn mutate(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    id: &str,
    payload: &Value,
) -> Result<IdentifierOutcome, AppError> {
    if provider.is_empty() || id.is_empty() || provider.len() > 256 || id.len() > 256 {
        return Err(AppError::BadRequest(
            "Provider and durable operation identity are required".into(),
        ));
    }
    let (space, doc, display, key) = validate_payload(payload)?;
    if let Some(outcome) = historical(state, auth, provider, id, payload).await? {
        return Ok(outcome);
    }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    mutation_guards::lock_audience_for_mutation(&mut tx).await?;
    mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space]).await?;
    if sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
        .bind(&doc)
        .bind(&space)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?
        .rows_affected()
        != 1
    {
        return Err(AppError::NotFound("Page unavailable".into()));
    }
    edit(&mut tx, auth, &space, &doc).await?;
    let before = page(&mut tx, &space, &doc).await?;
    let drafts = draft_sources(&mut tx, &doc).await?;
    let source = &payload["source"];
    let actual_stamp = before["updated_at"].as_str().ok_or_else(conflict)?;
    if !same_stamp(
        actual_stamp,
        source["page_updated_at"].as_str().ok_or_else(conflict)?,
    ) || before["human_identifier"] != source["human_identifier"]
        || match (
            before["published_at"].as_str(),
            source["published_at"].as_str(),
        ) {
            (None, None) => false,
            (Some(a), Some(b)) => !same_stamp(a, b),
            _ => true,
        }
    {
        return Err(conflict());
    }
    ensure_identifier_available(&mut tx, &space, key.as_deref(), Some(&doc)).await?;
    let mut after = before.clone();
    if before["human_identifier"] != json!(display) {
        let now = chrono::Utc::now().to_rfc3339();
        let result=sqlx::query("UPDATE documents SET human_identifier=$1,human_identifier_key=$2,updated_at=$3,last_updated_by=$4 WHERE id=$5 AND space_id=$6").bind(&display).bind(&key).bind(&now).bind(&auth.claims.sub).bind(&doc).bind(&space).execute(&mut *tx).await;
        match result{Ok(r)if r.rows_affected()==1=>{},Err(sqlx::Error::Database(e))if e.is_unique_violation()=>return Err(AppError::Conflict("Identifier is already reserved in this space, including archived or trashed pages".into())),Err(e)=>return Err(AppError::internal(e)),_=>return Err(conflict())}
        after["human_identifier"] = json!(display);
        after["human_identifier_key"] = json!(key);
        after["updated_at"] = json!(now);
        after["last_updated_by"] = json!(auth.claims.sub);
    }
    let outcome = IdentifierOutcome {
        operation_id: id.into(),
        human_identifier: display,
        page_updated_at: after["updated_at"].as_str().ok_or_else(conflict)?.into(),
        published_at: after["published_at"].as_str().map(str::to_string),
        historical: false,
    };
    let r = Receipt {
        entity_type: "document".into(),
        action: "update_identifier".into(),
        provider: provider.into(),
        operation_id: id.into(),
        actor_id: auth.claims.sub.clone(),
        document_id: doc.clone(),
        space_id: space.clone(),
        payload: payload.clone(),
        before,
        after: after.clone(),
        outcome: outcome.clone(),
    };
    let expected = expected_row(&r)?;
    sqlx::query("INSERT INTO document_identifier_receipts(provider,incoming_id,actor_id,document_id,space_id,payload_fingerprint,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(provider).bind(id).bind(&auth.claims.sub).bind(&doc).bind(&space).bind(fingerprint(payload)?).bind(expected["receipt_json"].as_str().unwrap()).execute(&mut *tx).await.map_err(AppError::internal)?;
    #[cfg(test)]
    after_write(&mut tx).await;
    edit(&mut tx, auth, &space, &doc).await?;
    if page(&mut tx, &space, &doc).await? != after
        || draft_sources(&mut tx, &doc).await? != drafts
        || rows(
            &mut tx,
            "SELECT * FROM document_identifier_receipts WHERE provider=$1 AND incoming_id=$2",
            &[provider, id],
        )
        .await?
            != vec![expected]
    {
        return Err(conflict());
    }
    #[cfg(test)]
    {
        let delay = AFTER_PROOF_DELAY_MS.with(|slot| slot.replace(0));
        if delay > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
        }
    }
    access::require_current_human_preference_write(&mut tx, auth).await?;
    #[cfg(test)]
    let fault = COMMIT_FAULT.with(|slot| slot.replace(0));
    #[cfg(test)]
    let committed = if fault == 1 {
        tx.rollback().await.map_err(AppError::internal)?;
        Err(sqlx::Error::Protocol("injected rollback".into()))
    } else {
        tx.commit().await.and_then(|()| {
            if fault == 2 {
                Err(sqlx::Error::Protocol(
                    "injected lost acknowledgement".into(),
                ))
            } else {
                Ok(())
            }
        })
    };
    #[cfg(not(test))]
    let committed = tx.commit().await;
    if let Err(error) = committed {
        if let Some(mut confirmed) = historical(state, auth, provider, id, payload).await? {
            confirmed.historical = true;
            return Ok(confirmed);
        }
        return Err(AppError::internal(error));
    }
    Ok(outcome)
}
async fn set_identifier(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space, doc)): Path<(String, String)>,
    Json(request): Json<Value>,
) -> Result<Json<IdentifierOutcome>, AppError> {
    let body = request.as_object().ok_or_else(conflict)?;
    if body.len() != 4 {
        return Err(AppError::BadRequest(
            "Expected provider, operation_id, human_identifier and source".into(),
        ));
    }
    let provider = body
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(conflict)?;
    let id = body
        .get("operation_id")
        .and_then(Value::as_str)
        .ok_or_else(conflict)?;
    let payload = json!({"id":doc,"space_id":space,"human_identifier":body.get("human_identifier").ok_or_else(conflict)?,"source":body.get("source").ok_or_else(conflict)?});
    Ok(Json(mutate(&state, &auth, provider, id, &payload).await?))
}
#[cfg(test)]
thread_local! {static AFTER_PROOF_DELAY_MS:std::cell::Cell<u64>=const{std::cell::Cell::new(0)};static COMMIT_FAULT:std::cell::Cell<u8>=const{std::cell::Cell::new(0)};static LATE_SQL:std::cell::RefCell<Option<&'static str>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn after_write(conn: &mut AnyConnection) {
    if let Some(sql) = LATE_SQL.with(|s| s.borrow_mut().take()) {
        sqlx::query(sql).execute(conn).await.unwrap();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::FromRequestParts;
    use tower::ServiceExt;
    const STAMP: &str = "2000-01-01T00:00:00Z";
    async fn setup(wal: bool) -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,created_at,updated_at,published_at,automerge_state) VALUES('identifier-page','space-a','Page','immutable-original.md','owner-a','owner-a',$1,$1,$1,$2),('identifier-other','space-a','Other','other.md','owner-a','owner-a',$1,$1,$1,NULL)").bind(STAMP).bind(vec![1u8,2,3]).execute(&s.db).await.unwrap();
        s
    }
    fn token(s: &AppState, user: &str) -> String {
        let mut a = crate::spaces::documents::tests::test_auth(user);
        a.claims.sid = format!("metadata-{user}");
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &a.claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap()
    }
    async fn auth(s: &AppState, user: &str) -> AuthUser {
        let request = axum::http::Request::builder()
            .method("PATCH")
            .header("Authorization", format!("Bearer {}", token(s, user)))
            .body(())
            .unwrap();
        let (mut p, _) = request.into_parts();
        AuthUser::from_request_parts(&mut p, s).await.unwrap()
    }
    async fn payload(s: &AppState, doc: &str, value: Value) -> Value {
        let row: (String, Option<String>, Option<String>) = sqlx::query_as(
            "SELECT updated_at,published_at,human_identifier FROM documents WHERE id=$1",
        )
        .bind(doc)
        .fetch_one(&s.db)
        .await
        .unwrap();
        json!({"id":doc,"space_id":"space-a","human_identifier":value,"source":{"page_updated_at":row.0,"published_at":row.1,"human_identifier":row.2}})
    }
    async fn snapshot(s: &AppState) -> Vec<(String, Option<String>)> {
        crate::spaces::documents::tests::move_database_snapshot(s).await
    }
    #[test]
    fn identifier_normalization_is_ascii_bounded_and_case_preserving() {
        assert_eq!(
            normalize_identifier(Some("  Ops-21._a  ")).unwrap(),
            (Some("Ops-21._a".into()), Some("OPS-21._A".into()))
        );
        for v in [None, Some(""), Some("  ")] {
            assert_eq!(normalize_identifier(v).unwrap(), (None, None));
        }
        for v in ["_bad", ".bad", "-bad", "ab cd", "ümlaut", "x/y"] {
            assert!(normalize_identifier(Some(v)).is_err(), "{v}");
        }
        assert!(normalize_identifier(Some(&"a".repeat(64))).is_ok());
        assert!(normalize_identifier(Some(&"a".repeat(65))).is_err());
    }
    #[tokio::test]
    async fn identifier_http_actual_actor_and_key_ceilings_preserve_nonidentifier_source() {
        let s = setup(false).await;
        let p = payload(&s, "identifier-page", json!(" Ops-7 ")).await;
        let before = page(
            &mut *s.db.acquire().await.unwrap(),
            "space-a",
            "identifier-page",
        )
        .await
        .unwrap();
        let response=router().with_state(s.clone()).oneshot(axum::http::Request::builder().method("PATCH").uri("/spaces/space-a/documents/identifier-page/identifier").header("Authorization",format!("Bearer {}",token(&s,"editor-a"))).header("Content-Type","application/json").body(axum::body::Body::from(json!({"provider":"self-hosted","operation_id":"route-id","human_identifier":p["human_identifier"],"source":p["source"]}).to_string())).unwrap()).await.unwrap();
        assert_eq!(response.status(), 200);
        let a = auth(&s, "editor-a").await;
        let current = page(
            &mut *s.db.acquire().await.unwrap(),
            "space-a",
            "identifier-page",
        )
        .await
        .unwrap();
        for k in before.as_object().unwrap().keys().filter(|k| {
            ![
                "human_identifier",
                "human_identifier_key",
                "updated_at",
                "last_updated_by",
            ]
            .contains(&k.as_str())
        }) {
            assert_eq!(current[k], before[k], "{k}");
        }
        assert_eq!(current["human_identifier"], "Ops-7");
        assert_eq!(current["human_identifier_key"], "OPS-7");
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('id-read-key','editor-a','Read',$1,'idread','[\"read\"]'),('id-write-key','editor-a','Write',$2,'idwrite','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key("id-read-key")).bind(crate::auth::api_keys::hash_api_key("id-write-key")).execute(&s.db).await.unwrap();
        for (key, status) in [("id-read-key", 403), ("id-write-key", 200)] {
            let p = payload(&s, "identifier-page", json!("Ops-8")).await;
            let response=router().with_state(s.clone()).oneshot(axum::http::Request::builder().method("PATCH").uri("/spaces/space-a/documents/identifier-page/identifier").header("X-API-Key",key).header("Content-Type","application/json").body(axum::body::Body::from(json!({"provider":"self-hosted","operation_id":key,"human_identifier":p["human_identifier"],"source":p["source"]}).to_string())).unwrap()).await.unwrap();
            assert_eq!(response.status().as_u16(), status);
        }
        assert!(!historical(&s, &a, "self-hosted", "absent", &p)
            .await
            .unwrap()
            .is_some());
    }
    #[tokio::test]
    async fn identifier_collisions_include_archive_and_trash_and_other_space_is_independent() {
        for lifecycle in ["archived_at", "deleted_at"] {
            let s = setup(false).await;
            sqlx::query(&format!("UPDATE documents SET human_identifier='Ops-7',human_identifier_key='OPS-7',{lifecycle}=$1 WHERE id='identifier-other'")).bind(STAMP).execute(&s.db).await.unwrap();
            let a = auth(&s, "editor-a").await;
            let p = payload(&s, "identifier-page", json!("ops-7")).await;
            let before = snapshot(&s).await;
            assert!(matches!(
                mutate(&s, &a, "p", "collision", &p).await,
                Err(AppError::Conflict(_))
            ));
            assert_eq!(snapshot(&s).await, before);
            sqlx::query("UPDATE documents SET space_id='space-b' WHERE id='identifier-other'")
                .execute(&s.db)
                .await
                .unwrap();
            assert!(mutate(&s, &a, "p", "available", &p).await.is_ok());
        }
    }
    #[tokio::test]
    async fn identifier_private_participation_locks_lifecycle_and_inactive_actor_are_enforced() {
        for case in [
            "private",
            "viewer-share",
            "locked",
            "archive",
            "trash",
            "inactive",
            "restricted",
        ] {
            let s = setup(false).await;
            match case {
                "private" | "viewer-share" => {
                    sqlx::query(
                        "UPDATE documents SET published_at=NULL WHERE id='identifier-page'",
                    )
                    .execute(&s.db)
                    .await
                    .unwrap();
                    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('id-draft','identifier-page','space-a','owner-a','Own','BODY',$1)").bind(STAMP).execute(&s.db).await.unwrap();
                    if case == "viewer-share" {
                        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('id-draft','editor-a','viewer','owner-a')").execute(&s.db).await.unwrap();
                    }
                }
                "locked" => {
                    sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='identifier-page'").execute(&s.db).await.unwrap();
                }
                "archive" | "trash" => {
                    sqlx::query(&format!(
                        "UPDATE documents SET {}=$1 WHERE id='identifier-page'",
                        if case == "archive" {
                            "archived_at"
                        } else {
                            "deleted_at"
                        }
                    ))
                    .bind(STAMP)
                    .execute(&s.db)
                    .await
                    .unwrap();
                }
                "restricted" => {
                    sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,created_by,revision) VALUES('identifier-page','space-a','restricted','owner-a','restriction')").execute(&s.db).await.unwrap();
                }
                _ => {}
            }
            let a = auth(&s, "editor-a").await;
            if case == "inactive" {
                sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
                    .execute(&s.db)
                    .await
                    .unwrap();
            }
            let p = payload(&s, "identifier-page", json!("No-1")).await;
            let before = snapshot(&s).await;
            assert!(mutate(&s, &a, "p", case, &p).await.is_err(), "{case}");
            assert_eq!(snapshot(&s).await, before);
            if case == "viewer-share" {
                sqlx::query(
                    "UPDATE document_draft_access SET role='editor' WHERE draft_id='id-draft'",
                )
                .execute(&s.db)
                .await
                .unwrap();
                assert!(mutate(&s, &a, "p", "shared-editor", &p).await.is_ok());
            }
        }
    }
    #[tokio::test]
    async fn identifier_source_scope_and_explicit_field_types_fail_without_writes() {
        let s = setup(false).await;
        let a = auth(&s, "editor-a").await;
        let original = payload(&s, "identifier-page", json!("Ref-1")).await;
        let mut bad = Vec::new();
        for value in [json!(4), json!({}), json!([])] {
            let mut p = original.clone();
            p["human_identifier"] = value;
            bad.push(p);
        }
        for (key, value) in [
            ("id", json!("foreign")),
            ("space_id", json!("space-b")),
            (
                "source",
                json!({"page_updated_at":STAMP,"published_at":STAMP,"human_identifier":"not-current"}),
            ),
        ] {
            let mut p = original.clone();
            p[key] = value;
            bad.push(p);
        }
        let mut p = original.clone();
        p.as_object_mut().unwrap().remove("human_identifier");
        bad.push(p);
        for (i, p) in bad.into_iter().enumerate() {
            let before = snapshot(&s).await;
            assert!(mutate(&s, &a, "p", &i.to_string(), &p).await.is_err());
            assert_eq!(snapshot(&s).await, before);
        }
        let viewer = auth(&s, "viewer-a").await;
        assert!(mutate(&s, &viewer, "p", "viewer", &original).await.is_err());
    }
    #[tokio::test]
    async fn identifier_late_write_source_credential_and_receipt_drift_roll_back() {
        for sql in["UPDATE documents SET title='tampered' WHERE id='identifier-page'","UPDATE documents SET published_at=NULL WHERE id='identifier-page'","UPDATE users SET is_active=0 WHERE id='editor-a'","DELETE FROM sessions WHERE id='metadata-editor-a'","UPDATE document_identifier_receipts SET receipt_json='{}'","INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('injected','identifier-page','space-a','editor-a','Secret','Injected','old')"]{let s=setup(false).await;let a=auth(&s,"editor-a").await;let p=payload(&s,"identifier-page",json!("New-1")).await;let before=snapshot(&s).await;LATE_SQL.with(|slot|*slot.borrow_mut()=Some(sql));assert!(mutate(&s,&a,"p","late",&p).await.is_err(),"{sql}");assert_eq!(snapshot(&s).await,before);}
    }
    #[tokio::test]
    async fn identifier_historical_receipts_ack_only_recorded_outcome_and_reject_identity_collisions(
    ) {
        let s = setup(false).await;
        let a = auth(&s, "editor-a").await;
        let p = payload(&s, "identifier-page", json!("First-1")).await;
        let first = mutate(&s, &a, "actual-provider", "same-id", &p)
            .await
            .unwrap();
        let newer = payload(&s, "identifier-page", json!("Later-2")).await;
        mutate(&s, &a, "actual-provider", "later-id", &newer)
            .await
            .unwrap();
        let owner = auth(&s, "owner-a").await;
        let before = snapshot(&s).await;
        let acknowledged = mutate(&s, &a, "actual-provider", "same-id", &p)
            .await
            .unwrap();
        assert!(acknowledged.historical);
        assert_eq!(acknowledged.human_identifier, first.human_identifier);
        assert_eq!(acknowledged.page_updated_at, first.page_updated_at);
        assert_eq!(snapshot(&s).await, before);
        assert!(historical(&s, &owner, "actual-provider", "same-id", &p)
            .await
            .is_err());
        assert!(mutate(&s, &a, "other-provider", "same-id", &p)
            .await
            .is_err());
        assert!(mutate(&s, &a, "actual-provider", "same-id", &newer)
            .await
            .is_err());
        assert_eq!(snapshot(&s).await, before);
    }
    #[tokio::test]
    async fn identifier_true_wal_wait_rechecks_actual_session_before_any_canonical_change() {
        let s = setup(true).await;
        let a = auth(&s, "editor-a").await;
        let p = payload(&s, "identifier-page", json!("Wait-1")).await;
        let mut holding = s.db.begin().await.unwrap();
        mutation_guards::lock_audience_for_mutation(&mut holding)
            .await
            .unwrap();
        let state = s.clone();
        let job = tokio::spawn(async move { mutate(&state, &a, "p", "wait", &p).await });
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert!(!job.is_finished());
        sqlx::query("DELETE FROM sessions WHERE id='metadata-editor-a'")
            .execute(&mut *holding)
            .await
            .unwrap();
        holding.commit().await.unwrap();
        let before = snapshot(&s).await;
        assert!(job.await.unwrap().is_err());
        assert_eq!(snapshot(&s).await, before);
    }
}
#[cfg(test)]
mod integration_tests {
    use super::*;
    use tower::ServiceExt;
    async fn setup() -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(false).await;
        crate::storage::init_data_dir().await.unwrap();
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id='space-a'")
            .fetch_one(&s.db)
            .await
            .unwrap();
        crate::storage::init_space_dirs(&slug).await.unwrap();
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('creation-identifier-key','editor-a','Write',$1,'cid','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key("creation-identifier-key")).execute(&s.db).await.unwrap();
        s
    }
    async fn request(s: &AppState, method: &str, path: &str, body: Value) -> (u16, Value) {
        let mut builder = axum::http::Request::builder().method(method).uri(path);
        if path.ends_with("/duplicate") {
            let mut actual = crate::spaces::documents::tests::test_auth("editor-a");
            actual.claims.sid = "metadata-editor-a".into();
            let token = jsonwebtoken::encode(
                &jsonwebtoken::Header::default(),
                &actual.claims,
                &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
            )
            .unwrap();
            builder = builder.header("Authorization", format!("Bearer {token}"));
        } else {
            builder = builder.header("X-API-Key", "creation-identifier-key");
        }
        let r = super::super::documents::router()
            .merge(super::router())
            .with_state(s.clone())
            .oneshot(
                builder
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = r.status().as_u16();
        let bytes = axum::body::to_bytes(r.into_body(), 1024 * 1024)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }
    #[tokio::test]
    async fn identifier_create_import_get_and_duplicate_are_consumed_and_collision_preflight_retains_db(
    ) {
        let _data_root=crate::storage::fixture_root::FixtureRoot::private();
        let s = setup().await;
        let (status,created)=request(&s,"POST","/spaces/space-a/documents",json!({"title":"Identifier import","content":"OWN BODY","human_identifier":" Work-1 ","source_id":"e84d384b-2f43-4c5b-8bca-dd7c036f2092","created_at":"2000-01-01T00:00:00Z","updated_at":"2001-01-01T00:00:00Z"})).await;
        assert!(matches!(status, 201 | 202), "{status}/{created}");
        assert_eq!(created["human_identifier"], "Work-1");
        let id = created["id"].as_str().unwrap();
        let (_, copy) = request(
            &s,
            "POST",
            &format!("/spaces/space-a/documents/{id}/duplicate"),
            json!({"title":"Identifier copy"}),
        )
        .await;
        assert!(copy["id"].is_string(), "{copy}");
        assert!(copy["human_identifier"].is_null());
        let pair: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT human_identifier,human_identifier_key FROM documents WHERE id=$1",
        )
        .bind(copy["id"].as_str().unwrap())
        .fetch_one(&s.db)
        .await
        .unwrap();
        assert_eq!(pair, (None, None));
        let last_used: Option<String> = sqlx::query_scalar(
            "SELECT last_used_at FROM api_keys WHERE id='creation-identifier-key'",
        )
        .fetch_one(&s.db)
        .await
        .unwrap();
        let before = crate::spaces::documents::tests::move_database_snapshot(&s).await;
        let (status, _) = request(
            &s,
            "POST",
            "/spaces/space-a/documents",
            json!({"title":"Duplicate identifier","human_identifier":"work-1"}),
        )
        .await;
        assert_eq!(status, 409);
        sqlx::query("UPDATE api_keys SET last_used_at=$1 WHERE id='creation-identifier-key'")
            .bind(last_used)
            .execute(&s.db)
            .await
            .unwrap();
        let after = crate::spaces::documents::tests::move_database_snapshot(&s).await;
        assert_eq!(after, before);
        let mut human = crate::spaces::documents::tests::test_auth("editor-a");
        human.claims.sid = "metadata-editor-a".into();
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &human.claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap();
        let r = super::super::documents::router()
            .with_state(s.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri(format!("/spaces/space-a/documents/{id}"))
                    .header("Authorization", format!("Bearer {token}"))
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let bytes = axum::body::to_bytes(r.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let dto: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(dto["human_identifier"], "Work-1");
        assert_eq!(dto["identifier_edit_allowed"], true);
    }
    #[tokio::test]
    async fn identifier_get_capability_is_current_false_for_locks_and_read_only_keys() {
        let _data_root=crate::storage::fixture_root::FixtureRoot::private();
        let s = setup().await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,human_identifier,human_identifier_key,locked,locked_by) VALUES('capability-page','space-a','Page','','owner-a','owner-a','2000-01-01T00:00:00Z','Readable-1','READABLE-1',1,'owner-a')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('capability-read-key','editor-a','Read',$1,'cidread','[\"read\"]')").bind(crate::auth::api_keys::hash_api_key("capability-read-key")).execute(&s.db).await.unwrap();
        let mut actual = crate::spaces::documents::tests::test_auth("editor-a");
        actual.claims.sid = "metadata-editor-a".into();
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &actual.claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap();
        for (key, value) in [
            ("Authorization", format!("Bearer {token}")),
            ("X-API-Key", "capability-read-key".into()),
        ] {
            let r = super::super::documents::router()
                .with_state(s.clone())
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/spaces/space-a/documents/capability-page")
                        .header(key, value)
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(r.status(), 200);
            let bytes = axum::body::to_bytes(r.into_body(), 1024 * 1024)
                .await
                .unwrap();
            let dto: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(dto["human_identifier"], "Readable-1");
            assert_eq!(dto["identifier_edit_allowed"], false);
        }
        sqlx::query("UPDATE documents SET locked=0,locked_by=NULL WHERE id='capability-page'")
            .execute(&s.db)
            .await
            .unwrap();
        let r = super::super::documents::router()
            .with_state(s.clone())
            .oneshot(
                axum::http::Request::builder()
                    .uri("/spaces/space-a/documents/capability-page")
                    .header("X-API-Key", "capability-read-key")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status(), 200);
        let bytes = axum::body::to_bytes(r.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let dto: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(dto["identifier_edit_allowed"], false);
    }
    #[tokio::test]
    async fn identifier_migration_keeps_existing_nulls_and_pair_unique_constraints() {
        let _data_root=crate::storage::fixture_root::FixtureRoot::private();
        let s = setup().await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('legacy','space-a','Legacy','','owner-a','owner-a')").execute(&s.db).await.unwrap();
        let pair: (Option<String>, Option<String>) = sqlx::query_as(
            "SELECT human_identifier,human_identifier_key FROM documents WHERE id='legacy'",
        )
        .fetch_one(&s.db)
        .await
        .unwrap();
        assert_eq!(pair, (None, None));
        assert!(
            sqlx::query("UPDATE documents SET human_identifier='Unpaired' WHERE id='legacy'")
                .execute(&s.db)
                .await
                .is_err()
        );
        assert!(sqlx::query("UPDATE documents SET human_identifier='Display',human_identifier_key='wrong' WHERE id='legacy'").execute(&s.db).await.is_err());
    }
}

#[cfg(test)]
mod commit_tests {
    use super::*;
    use axum::extract::FromRequestParts;
    async fn fixture(exp: i64) -> (AppState, AuthUser, Value) {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(false).await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,updated_at,published_at) VALUES('commit-page','space-a','Page','original.md','owner-a','owner-a','2000-01-01T00:00:00Z','2000-01-01T00:00:00Z')").execute(&s.db).await.unwrap();
        let mut a = crate::spaces::documents::tests::test_auth("editor-a");
        a.claims.sid = "metadata-editor-a".into();
        a.claims.exp = exp;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &a.claims,
            &jsonwebtoken::EncodingKey::from_secret(s.jwt_secret.as_bytes()),
        )
        .unwrap();
        let req = axum::http::Request::builder()
            .method("PATCH")
            .header("Authorization", format!("Bearer {token}"))
            .body(())
            .unwrap();
        let (mut parts, _) = req.into_parts();
        let a = AuthUser::from_request_parts(&mut parts, &s).await.unwrap();
        let p = json!({"id":"commit-page","space_id":"space-a","human_identifier":"Commit-1","source":{"human_identifier":null,"page_updated_at":"2000-01-01T00:00:00+00:00","published_at":"2000-01-01T00:00:00Z"}});
        (s, a, p)
    }
    #[tokio::test]
    async fn identifier_last_actual_jwt_expiry_after_all_source_proofs_rolls_back() {
        let (s, a, p) = fixture(chrono::Utc::now().timestamp() + 2).await;
        let before = crate::spaces::documents::tests::move_database_snapshot(&s).await;
        AFTER_PROOF_DELAY_MS.with(|slot| slot.set(2200));
        assert!(matches!(
            mutate(&s, &a, "p", "expired-last", &p).await,
            Err(AppError::Unauthorized(_))
        ));
        assert_eq!(
            crate::spaces::documents::tests::move_database_snapshot(&s).await,
            before
        );
    }
    #[tokio::test]
    async fn identifier_commit_rollback_is_not_acknowledged_but_exact_lost_ack_is_historical() {
        for fault in [1, 2] {
            let (s, a, p) = fixture(chrono::Utc::now().timestamp() + 3600).await;
            let before = crate::spaces::documents::tests::move_database_snapshot(&s).await;
            COMMIT_FAULT.with(|slot| slot.set(fault));
            let result = mutate(&s, &a, "p", "uncertain", &p).await;
            if fault == 1 {
                assert!(result.is_err());
                assert_eq!(
                    crate::spaces::documents::tests::move_database_snapshot(&s).await,
                    before
                );
            } else {
                let result = result.unwrap();
                assert!(result.historical);
                assert_eq!(result.human_identifier, Some("Commit-1".into()));
                let retry = mutate(&s, &a, "p", "uncertain", &p).await.unwrap();
                assert!(retry.historical);
                sqlx::query("DELETE FROM sessions WHERE id='metadata-editor-a'")
                    .execute(&s.db)
                    .await
                    .unwrap();
                assert!(historical(&s, &a, "p", "uncertain", &p).await.is_err());
            }
        }
    }
}

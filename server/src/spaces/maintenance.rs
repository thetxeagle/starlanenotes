//! Page responsibility is metadata: it never grants access or transfers a draft.
use super::{access, mutation_guards};
use crate::{auth::middleware::AuthUser, auth::routes::AppError, AppState};
use axum::{
    extract::{Path, State},
    routing::{get, patch},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{AnyConnection, Column, Row};
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/spaces/{space}/documents/{document}/maintenance",
            patch(set_maintenance),
        )
        .route(
            "/spaces/{space}/documents/{document}/maintenance/targets",
            get(get_targets),
        )
}
fn conflict() -> AppError {
    AppError::Conflict(
        "Maintenance assignment or page source changed; retain the request and review current metadata".into(),
    )
}
fn nullable(v: Option<&Value>) -> Result<Option<String>, AppError> {
    match v {
        Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        _ => Err(AppError::BadRequest(
            "Nullable maintenance source fields must be supplied as a string or null".into(),
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
fn fingerprint(v: &Value) -> Result<String, AppError> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(v).map_err(AppError::internal)?,
    )))
}
/// Raw source equality is data evidence, never an authority capability.
pub(crate) async fn rows(
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

#[derive(Clone, Copy)]
pub(crate) enum TargetUse {
    Select,
    PreserveMove,
    PreserveMoveRead,
}
#[derive(Clone, Copy)]
pub(crate) enum TargetCredential {
    Read,
    Write,
}
#[derive(Clone, PartialEq)]
pub(crate) struct TargetCatalogProof {
    pub catalog: Value,
    pub witness: Value,
}
fn pair(user: Option<&str>, group: Option<&str>) -> Result<(), AppError> {
    if user.is_some() && group.is_some()
        || user.is_some_and(str::is_empty)
        || group.is_some_and(str::is_empty)
    {
        return Err(AppError::BadRequest(
            "Maintenance requires one exact person or group identity, or unassigned".into(),
        ));
    }
    Ok(())
}
/// Data evidence only. Do not persist password hashes, key material or unrelated members.
pub(crate) async fn target_source(
    conn: &mut AnyConnection,
    user: Option<&str>,
    group: Option<&str>,
) -> Result<Value, AppError> {
    pair(user, group)?;
    Ok(
        json!({"users":if let Some(id)=user{rows(conn,"SELECT id,display_name,is_active FROM users WHERE id=$1",&[id]).await?}else{vec![]},"groups":if let Some(id)=group{rows(conn,"SELECT id,name,description,managed_space_id,scim_managed,external_id,created_at,updated_at FROM groups WHERE id=$1",&[id]).await?}else{vec![]},"grants":if let Some(id)=group{rows(conn,"SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE group_id=$1 ORDER BY id",&[id]).await?}else{vec![]}}),
    )
}
/// Original caller supplies PageEdit/Move authority separately; this is target eligibility only.
pub(crate) async fn validate_assignment(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    user: Option<&str>,
    group: Option<&str>,
    use_: TargetUse,
) -> Result<Value, AppError> {
    pair(user, group)?;
    if matches!(use_, TargetUse::PreserveMoveRead) {
        access::require_current_human_read(conn, auth).await?;
    } else {
        access::require_current_human_preference_write(conn, auth).await?;
    }
    let source = target_source(conn, user, group).await?;
    if user.is_some() {
        let users = source["users"].as_array().unwrap();
        if users.len() != 1
            || matches!(use_, TargetUse::Select) && users[0]["is_active"].as_i64() != Some(1)
        {
            return Err(AppError::NotFound("Maintenance target unavailable".into()));
        }
    }
    if group.is_some() {
        if !access::maintenance_groups_available(
            conn,
            auth,
            space,
            if matches!(use_, TargetUse::PreserveMoveRead) {
                access::MaintenanceTargetCredential::Read
            } else {
                access::MaintenanceTargetCredential::Write
            },
        )
        .await?
        {
            return Err(AppError::NotFound("Maintenance target unavailable".into()));
        }
        let groups = source["groups"].as_array().unwrap();
        if groups.len() != 1 {
            return Err(AppError::NotFound("Maintenance target unavailable".into()));
        }
        let managed = groups[0]["managed_space_id"].as_str();
        if managed.is_some_and(|id| id != space)
            || managed != Some(space)
                && !source["grants"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|g| g["space_id"].as_str() == Some(space))
        {
            return Err(AppError::NotFound("Maintenance target unavailable".into()));
        }
    }
    if matches!(use_, TargetUse::PreserveMoveRead) {
        access::require_current_human_read(conn, auth).await?;
    } else {
        access::require_current_human_preference_write(conn, auth).await?;
    }
    Ok(source)
}
pub(crate) async fn inspect_targets(
    conn: &mut AnyConnection,
    auth: &AuthUser,
    space: &str,
    credential: TargetCredential,
) -> Result<TargetCatalogProof, AppError> {
    let scope = match credential {
        TargetCredential::Read => access::MaintenanceTargetCredential::Read,
        TargetCredential::Write => access::MaintenanceTargetCredential::Write,
    };
    match credential {
        TargetCredential::Read => access::require_current_human_read(conn, auth).await?,
        TargetCredential::Write => {
            access::require_current_human_preference_write(conn, auth).await?
        }
    };
    let available = access::maintenance_groups_available(conn, auth, space, scope).await?;
    let users = rows(
        conn,
        "SELECT id,display_name,is_active FROM users WHERE is_active=1 ORDER BY id",
        &[],
    )
    .await?;
    let groups = if available {
        rows(conn,"SELECT g.id,g.name,g.description,g.managed_space_id,g.scim_managed,g.external_id,g.created_at,g.updated_at FROM groups g WHERE (g.managed_space_id IS NULL OR g.managed_space_id=$1) AND (g.managed_space_id=$1 OR EXISTS(SELECT 1 FROM space_group_permissions p WHERE p.group_id=g.id AND p.space_id=$1)) ORDER BY g.id",&[space]).await?
    } else {
        vec![]
    };
    let grants = if available {
        rows(conn,"SELECT p.id,p.space_id,p.group_id,p.role,p.created_at,p.updated_at FROM space_group_permissions p JOIN groups g ON g.id=p.group_id WHERE p.space_id=$1 AND (g.managed_space_id IS NULL OR g.managed_space_id=$1) ORDER BY p.id",&[space]).await?
    } else {
        vec![]
    };
    let catalog = json!({"users":users.iter().map(|u|json!({"id":u["id"],"name":u["display_name"].as_str().filter(|n|!n.trim().is_empty()).unwrap_or("Unnamed person"),"active":true})).collect::<Vec<_>>(),"groups":groups.iter().map(|g|json!({"id":g["id"],"name":g["name"],"managed_space_id":g["managed_space_id"]})).collect::<Vec<_>>(),"groups_available":available});
    if access::maintenance_groups_available(conn, auth, space, scope).await? != available {
        return Err(conflict());
    }
    match credential {
        TargetCredential::Read => access::require_current_human_read(conn, auth).await?,
        TargetCredential::Write => {
            access::require_current_human_preference_write(conn, auth).await?
        }
    };
    Ok(TargetCatalogProof {
        catalog,
        witness: json!({"users":users,"groups":groups,"grants":grants}),
    })
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
#[derive(Clone, Deserialize, Serialize, PartialEq, Debug)]
pub struct MaintenanceOutcome {
    pub operation_id: String,
    pub maintenance_user_id: Option<String>,
    pub maintenance_group_id: Option<String>,
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
    target: Value,
    outcome: MaintenanceOutcome,
}
async fn stored(
    conn: &mut AnyConnection,
    provider: &str,
    id: &str,
) -> Result<Option<(Receipt, Value)>, AppError> {
    let raw = rows(
        conn,
        "SELECT * FROM document_maintenance_receipts WHERE provider=$1 AND incoming_id=$2",
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
fn intent(v: &Value) -> Result<(Option<String>, Option<String>), AppError> {
    let o = v
        .as_object()
        .ok_or_else(|| AppError::BadRequest("Maintenance intent must be an object".into()))?;
    let kind = o.get("kind").and_then(Value::as_str).ok_or_else(conflict)?;
    let id = |key: &str| {
        o.get(key)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| {
                AppError::BadRequest("Maintenance target must be an exact nonempty string".into())
            })
    };
    match kind {
        "unassigned" if o.len() == 1 => Ok((None, None)),
        "user" if o.len() == 2 => Ok((Some(id("user_id")?), None)),
        "group" if o.len() == 2 => Ok((None, Some(id("group_id")?))),
        _ => Err(AppError::BadRequest(
            "Maintenance requires exact unassigned, user or group intent".into(),
        )),
    }
}
fn validate_payload(
    p: &Value,
) -> Result<(String, String, Option<String>, Option<String>), AppError> {
    let o = p.as_object().ok_or_else(conflict)?;
    if o.len() != 4
        || !["id", "space_id", "maintenance", "source"]
            .iter()
            .all(|k| o.contains_key(*k))
    {
        return Err(conflict());
    }
    let id = |k: &str| {
        o.get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(conflict)
    };
    let source = o["source"].as_object().ok_or_else(conflict)?;
    if source.len() != 4
        || ![
            "page_updated_at",
            "published_at",
            "maintenance_user_id",
            "maintenance_group_id",
        ]
        .iter()
        .all(|k| source.contains_key(*k))
    {
        return Err(conflict());
    }
    timestamp(source["page_updated_at"].as_str().ok_or_else(conflict)?)?;
    if let Some(t) = nullable(source.get("published_at"))? {
        timestamp(&t)?;
    }
    let user = nullable(source.get("maintenance_user_id"))?;
    let group = nullable(source.get("maintenance_group_id"))?;
    pair(user.as_deref(), group.as_deref())?;
    let (user, group) = intent(&o["maintenance"])?;
    Ok((id("space_id")?, id("id")?, user, group))
}
pub(crate) async fn historical(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    id: &str,
    payload: &Value,
) -> Result<Option<MaintenanceOutcome>, AppError> {
    validate_payload(payload)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    access::require_current_human_preference_write(&mut conn, auth).await?;
    let Some((r, row)) = stored(&mut conn, provider, id).await? else {
        return Ok(None);
    };
    if r.entity_type != "document"
        || r.action != "update_maintenance"
        || r.provider != provider
        || r.operation_id != id
        || r.actor_id != auth.claims.sub
        || r.payload != *payload
        || r.document_id != payload["id"].as_str().unwrap_or("")
        || r.space_id != payload["space_id"].as_str().unwrap_or("")
        || r.outcome.operation_id != id
        || r.outcome.historical
        || r.outcome.maintenance_user_id
            != r.after["maintenance_user_id"].as_str().map(str::to_string)
        || r.outcome.maintenance_group_id
            != r.after["maintenance_group_id"].as_str().map(str::to_string)
        || r.outcome.page_updated_at != r.after["updated_at"].as_str().unwrap_or("")
        || r.outcome.published_at != r.after["published_at"].as_str().map(str::to_string)
        || row != expected_row(&r)?
    {
        return Err(conflict());
    }
    let (_, _, user, group) = validate_payload(payload)?;
    let mut expected_after = r.before.clone();
    let changed = r.before["maintenance_user_id"] != json!(user)
        || r.before["maintenance_group_id"] != json!(group);
    if changed {
        expected_after["maintenance_user_id"] = json!(user);
        expected_after["maintenance_group_id"] = json!(group);
        expected_after["updated_at"] = json!(r.outcome.page_updated_at);
        expected_after["last_updated_by"] = json!(r.actor_id);
    }
    if expected_after != r.after
        || r.before["maintenance_user_id"] != payload["source"]["maintenance_user_id"]
        || r.before["maintenance_group_id"] != payload["source"]["maintenance_group_id"]
    {
        return Err(conflict());
    }
    if rows(
        &mut conn,
        "SELECT * FROM document_maintenance_receipts WHERE provider=$1 AND incoming_id=$2",
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
pub(crate) async fn mutate(
    state: &AppState,
    auth: &AuthUser,
    provider: &str,
    id: &str,
    payload: &Value,
) -> Result<MaintenanceOutcome, AppError> {
    if provider.is_empty() || id.is_empty() || provider.len() > 256 || id.len() > 256 {
        return Err(conflict());
    }
    let (space, doc, user, group) = validate_payload(payload)?;
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
    if !same_stamp(
        before["updated_at"].as_str().ok_or_else(conflict)?,
        source["page_updated_at"].as_str().ok_or_else(conflict)?,
    ) || before["maintenance_user_id"] != source["maintenance_user_id"]
        || before["maintenance_group_id"] != source["maintenance_group_id"]
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
    let changed = before["maintenance_user_id"] != json!(user)
        || before["maintenance_group_id"] != json!(group);
    let target = if changed {
        validate_assignment(
            &mut tx,
            auth,
            &space,
            user.as_deref(),
            group.as_deref(),
            TargetUse::Select,
        )
        .await?
    } else {
        target_source(&mut tx, user.as_deref(), group.as_deref()).await?
    };
    let mut after = before.clone();
    if changed {
        let now = chrono::Utc::now().to_rfc3339();
        if sqlx::query("UPDATE documents SET maintenance_user_id=$1,maintenance_group_id=$2,updated_at=$3,last_updated_by=$4 WHERE id=$5 AND space_id=$6").bind(&user).bind(&group).bind(&now).bind(&auth.claims.sub).bind(&doc).bind(&space).execute(&mut *tx).await.map_err(AppError::internal)?.rows_affected()!=1{return Err(conflict())}
        after["maintenance_user_id"] = json!(user);
        after["maintenance_group_id"] = json!(group);
        after["updated_at"] = json!(now);
        after["last_updated_by"] = json!(auth.claims.sub);
    }
    let outcome = MaintenanceOutcome {
        operation_id: id.into(),
        maintenance_user_id: user.clone(),
        maintenance_group_id: group.clone(),
        page_updated_at: after["updated_at"].as_str().ok_or_else(conflict)?.into(),
        published_at: after["published_at"].as_str().map(str::to_string),
        historical: false,
    };
    let r = Receipt {
        entity_type: "document".into(),
        action: "update_maintenance".into(),
        provider: provider.into(),
        operation_id: id.into(),
        actor_id: auth.claims.sub.clone(),
        document_id: doc.clone(),
        space_id: space.clone(),
        payload: payload.clone(),
        before,
        after: after.clone(),
        target: target.clone(),
        outcome: outcome.clone(),
    };
    let expected = expected_row(&r)?;
    sqlx::query("INSERT INTO document_maintenance_receipts(provider,incoming_id,actor_id,document_id,space_id,payload_fingerprint,receipt_json) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(provider).bind(id).bind(&auth.claims.sub).bind(&doc).bind(&space).bind(fingerprint(payload)?).bind(expected["receipt_json"].as_str().unwrap()).execute(&mut *tx).await.map_err(AppError::internal)?;
    #[cfg(test)]
    after_write(&mut tx).await;
    edit(&mut tx, auth, &space, &doc).await?;
    let current_target = if changed {
        validate_assignment(
            &mut tx,
            auth,
            &space,
            user.as_deref(),
            group.as_deref(),
            TargetUse::Select,
        )
        .await?
    } else {
        target_source(&mut tx, user.as_deref(), group.as_deref()).await?
    };
    if current_target != target
        || page(&mut tx, &space, &doc).await? != after
        || draft_sources(&mut tx, &doc).await? != drafts
        || rows(
            &mut tx,
            "SELECT * FROM document_maintenance_receipts WHERE provider=$1 AND incoming_id=$2",
            &[provider, id],
        )
        .await?
            != vec![expected]
    {
        return Err(conflict());
    }
    edit(&mut tx, auth, &space, &doc).await?;
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
        if let Some(outcome) = historical(state, auth, provider, id, payload).await? {
            return Ok(outcome);
        }
        return Err(AppError::internal(error));
    }
    Ok(outcome)
}
async fn set_maintenance(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space, doc)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<MaintenanceOutcome>, AppError> {
    let o = body.as_object().ok_or_else(conflict)?;
    if o.len() != 4
        || !["provider", "operation_id", "maintenance", "source"]
            .iter()
            .all(|k| o.contains_key(*k))
    {
        return Err(conflict());
    }
    let provider = o["provider"].as_str().ok_or_else(conflict)?;
    let op = o["operation_id"].as_str().ok_or_else(conflict)?;
    Ok(Json(
        mutate(
            &state,
            &auth,
            provider,
            op,
            &json!({"id":doc,"space_id":space,"maintenance":o["maintenance"],"source":o["source"]}),
        )
        .await?,
    ))
}
async fn get_targets(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space, doc)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    let principal = access::principal_from_auth(&auth)?;
    access::resolve_page_access(
        &mut conn,
        principal,
        &space,
        &doc,
        access::PageOperation::View,
        access::Purpose::Routine,
    )
    .await?;
    let before = page(&mut conn, &space, &doc).await?;
    let draft = if before["published_at"].is_null() {
        Some(super::documents::readable_discovery_draft(&mut conn, &auth, &space, &doc).await?)
    } else {
        None
    };
    let catalog = inspect_targets(&mut conn, &auth, &space, TargetCredential::Read).await?;
    #[cfg(test)]
    after_catalog(&state).await;
    if page(&mut conn, &space, &doc).await? != before
        || inspect_targets(&mut conn, &auth, &space, TargetCredential::Read).await? != catalog
    {
        return Err(conflict());
    }
    access::resolve_page_access(
        &mut conn,
        access::principal_from_auth(&auth)?,
        &space,
        &doc,
        access::PageOperation::View,
        access::Purpose::Routine,
    )
    .await?;
    if let Some(draft) = draft {
        super::documents::require_current_content_draft_source(
            &mut conn, &auth, &space, &doc, &draft,
        )
        .await?;
    }
    if access::maintenance_groups_available(
        &mut conn,
        &auth,
        &space,
        access::MaintenanceTargetCredential::Read,
    )
    .await?
        != catalog.catalog["groups_available"]
            .as_bool()
            .unwrap_or(false)
    {
        return Err(conflict());
    }
    access::require_current_human_read(&mut conn, &auth).await?;
    Ok(Json(catalog.catalog))
}
#[cfg(test)]
thread_local! {static COMMIT_FAULT:std::cell::Cell<u8>=const{std::cell::Cell::new(0)};static LATE_SQL:std::cell::RefCell<Option<&'static str>>=const{std::cell::RefCell::new(None)};static CATALOG_SQL:std::cell::RefCell<Option<&'static str>>=const{std::cell::RefCell::new(None)};}
#[cfg(test)]
async fn after_write(conn: &mut AnyConnection) {
    let statement = LATE_SQL.with(|s| s.borrow_mut().take());
    if let Some(statement) = statement {
        sqlx::query(statement).execute(conn).await.unwrap();
    }
}
#[cfg(test)]
async fn after_catalog(state: &AppState) {
    let statement = CATALOG_SQL.with(|s| s.borrow_mut().take());
    if let Some(statement) = statement {
        sqlx::query(statement).execute(&state.db).await.unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::FromRequestParts;
    const STAMP: &str = "2000-01-01T00:00:00Z";
    async fn setup(wal: bool) -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,created_at,updated_at,published_at,automerge_state) VALUES('maintenance-page','space-a','Page','immutable-original.md','owner-a','owner-a',$1,$1,$1,$2),('maintenance-other','space-a','Other','other.md','owner-a','owner-a',$1,$1,$1,NULL)").bind(STAMP).bind(vec![1u8,2,3]).execute(&s.db).await.unwrap();
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
    async fn snapshot(s: &AppState) -> Vec<(String, Option<String>)> {
        crate::spaces::documents::tests::move_database_snapshot(s).await
    }

    async fn payload(s: &AppState, assignment: Value) -> Value {
        let r:(String,Option<String>,Option<String>,Option<String>)=sqlx::query_as("SELECT updated_at,published_at,maintenance_user_id,maintenance_group_id FROM documents WHERE id='maintenance-page'").fetch_one(&s.db).await.unwrap();
        json!({"id":"maintenance-page","space_id":"space-a","maintenance":assignment,"source":{"page_updated_at":r.0,"published_at":r.1,"maintenance_user_id":r.2,"maintenance_group_id":r.3}})
    }
    #[tokio::test]
    async fn maintenance_sql_only_current_assignment_and_historical_ack() {
        let s = setup(false).await;
        let a = auth(&s, "owner-a").await;
        let input = payload(&s, json!({"kind":"user","user_id":"editor-a"})).await;
        let before: (String,Option<String>,Vec<u8>)=sqlx::query_as("SELECT content_path,published_at,automerge_state FROM documents WHERE id='maintenance-page'").fetch_one(&s.db).await.unwrap();
        let r = mutate(&s, &a, "none", "maintenance-op", &input)
            .await
            .unwrap();
        assert_eq!(r.maintenance_user_id.as_deref(), Some("editor-a"));
        let after:(String,Option<String>,Vec<u8>)=sqlx::query_as("SELECT content_path,published_at,automerge_state FROM documents WHERE id='maintenance-page'").fetch_one(&s.db).await.unwrap();
        assert_eq!(before, after);
        let clear = payload(&s, json!({"kind":"unassigned"})).await;
        mutate(&s, &a, "none", "clear-op", &clear).await.unwrap();
        let old = historical(&s, &a, "none", "maintenance-op", &input)
            .await
            .unwrap()
            .unwrap();
        assert!(old.historical);
        assert_eq!(old.maintenance_user_id.as_deref(), Some("editor-a"));
        assert!(sqlx::query_scalar::<_, Option<String>>(
            "SELECT maintenance_user_id FROM documents WHERE id='maintenance-page'"
        )
        .fetch_one(&s.db)
        .await
        .unwrap()
        .is_none());
    }
    #[tokio::test]
    async fn maintenance_invalid_targets_and_exact_source_fail_without_writes() {
        let s = setup(false).await;
        let a = auth(&s, "owner-a").await;
        for target in [
            json!({"kind":"user","user_id":"missing"}),
            json!({"kind":"group","group_id":"missing"}),
            json!({"kind":"user","user_id":"editor-a","extra":true}),
            json!({"kind":"unassigned","group_id":null}),
        ] {
            let input = payload(&s, target).await;
            let before = snapshot(&s).await;
            assert!(mutate(&s, &a, "none", "bad", &input).await.is_err());
            assert_eq!(before, snapshot(&s).await);
        }
        let mut input = payload(&s, json!({"kind":"unassigned"})).await;
        input["source"]["maintenance_user_id"] = json!("old-deleted-person");
        let before = snapshot(&s).await;
        assert!(mutate(&s, &a, "none", "stale", &input).await.is_err());
        assert_eq!(before, snapshot(&s).await);
    }
    #[tokio::test]
    async fn maintenance_late_source_actor_receipt_and_target_drift_roll_back() {
        let s = setup(false).await;
        let a = auth(&s, "owner-a").await;
        for sql in [
            "UPDATE users SET is_active=0 WHERE id='owner-a'",
            "UPDATE users SET is_active=0 WHERE id='editor-a'",
            "UPDATE documents SET title='tampered' WHERE id='maintenance-page'",
            "UPDATE document_maintenance_receipts SET actor_id='editor-a'",
        ] {
            let input = payload(&s, json!({"kind":"user","user_id":"editor-a"})).await;
            let before = snapshot(&s).await;
            LATE_SQL.with(|x| *x.borrow_mut() = Some(sql));
            assert!(mutate(&s, &a, "none", "late", &input).await.is_err());
            assert_eq!(before, snapshot(&s).await);
        }
    }
    #[tokio::test]
    async fn maintenance_group_catalog_scope_and_unchanged_inactive_person() {
        let s = setup(false).await;
        let a = auth(&s, "owner-a").await;
        sqlx::query(
            "UPDATE documents SET maintenance_user_id='editor-a' WHERE id='maintenance-page'",
        )
        .execute(&s.db)
        .await
        .unwrap();
        sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
            .execute(&s.db)
            .await
            .unwrap();
        let input = payload(&s, json!({"kind":"user","user_id":"editor-a"})).await;
        let before: String =
            sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='maintenance-page'")
                .fetch_one(&s.db)
                .await
                .unwrap();
        let r = mutate(&s, &a, "none", "noop", &input).await.unwrap();
        assert_eq!(before, r.page_updated_at);
        let cat = inspect_targets(
            &mut *s.db.acquire().await.unwrap(),
            &a,
            "space-a",
            TargetCredential::Read,
        )
        .await
        .unwrap();
        assert!(!cat.catalog["users"]
            .as_array()
            .unwrap()
            .iter()
            .any(|u| u["id"] == "editor-a"));
    }
    #[tokio::test]
    async fn maintenance_real_http_key_ceilings_and_scoped_group_selection() {
        use tower::ServiceExt;
        let s = setup(false).await;
        sqlx::query("INSERT INTO groups(id,name) VALUES('maintenance-group','Responsible team'),('hidden-group','Hidden team')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('maintenance-grant','space-a','maintenance-group','viewer'),('hidden-grant','space-b','hidden-group','viewer')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('maintenance-read','owner-a','Read',$1,'read','[\"read\"]'),('maintenance-write','owner-a','Write',$2,'write','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key("maintenance-read")).bind(crate::auth::api_keys::hash_api_key("maintenance-write")).execute(&s.db).await.unwrap();
        for (key, method, status) in [
            ("maintenance-read", "GET", 200),
            ("maintenance-write", "GET", 403),
            ("maintenance-read", "PATCH", 403),
            ("maintenance-write", "PATCH", 200),
        ] {
            let input = payload(&s, json!({"kind":"group","group_id":"maintenance-group"})).await;
            let path = if method == "GET" {
                "/spaces/space-a/documents/maintenance-page/maintenance/targets"
            } else {
                "/spaces/space-a/documents/maintenance-page/maintenance"
            };
            let response=router().with_state(s.clone()).oneshot(axum::http::Request::builder().method(method).uri(path).header("X-API-Key",key).header("Content-Type","application/json").body(axum::body::Body::from(json!({"provider":"none","operation_id":format!("{method}-{key}"),"maintenance":input["maintenance"],"source":input["source"]}).to_string())).unwrap()).await.unwrap();
            assert_eq!(response.status().as_u16(), status, "{key}/{method}");
            if method == "GET" && status == 200 {
                let body: Value = serde_json::from_slice(
                    &axum::body::to_bytes(response.into_body(), usize::MAX)
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert!(body["groups"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|g| g["id"] == "maintenance-group"));
                assert!(!body["groups"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|g| g["id"] == "hidden-group"));
            }
        }
        let editor = auth(&s, "editor-a").await;
        let cat = inspect_targets(
            &mut *s.db.acquire().await.unwrap(),
            &editor,
            "space-a",
            TargetCredential::Read,
        )
        .await
        .unwrap();
        assert_eq!(cat.catalog["groups_available"], false);
        assert_eq!(cat.catalog["groups"], json!([]));
        let input = payload(&s, json!({"kind":"group","group_id":"hidden-group"})).await;
        let owner = auth(&s, "owner-a").await;
        let before = snapshot(&s).await;
        assert!(mutate(&s, &owner, "none", "hidden-group", &input)
            .await
            .is_err());
        assert_eq!(before, snapshot(&s).await);
    }
    #[tokio::test]
    async fn maintenance_fk_clear_same_stamp_is_stale_and_has_no_access_effect() {
        let s = setup(false).await;
        let a = auth(&s, "owner-a").await;
        sqlx::query("INSERT INTO groups(id,name) VALUES('deleted-maintenance','Team')")
            .execute(&s.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET maintenance_group_id='deleted-maintenance' WHERE id='maintenance-page'").execute(&s.db).await.unwrap();
        let input = payload(&s, json!({"kind":"unassigned"})).await;
        let mut c = s.db.acquire().await.unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&mut *c)
            .await
            .unwrap();
        assert_eq!(
            sqlx::query_scalar::<_, i64>("PRAGMA foreign_keys")
                .fetch_one(&mut *c)
                .await
                .unwrap(),
            1
        );
        sqlx::query("DELETE FROM groups WHERE id='deleted-maintenance'")
            .execute(&mut *c)
            .await
            .unwrap();
        drop(c);
        let current = payload(&s, json!({"kind":"unassigned"})).await;
        assert_eq!(
            input["source"]["page_updated_at"],
            current["source"]["page_updated_at"]
        );
        assert_eq!(current["source"]["maintenance_group_id"], Value::Null);
        let before = snapshot(&s).await;
        assert!(mutate(&s, &a, "none", "stale-fk", &input).await.is_err());
        assert_eq!(before, snapshot(&s).await);
    }
    #[tokio::test]
    async fn maintenance_commit_rollback_retains_failure_but_lost_ack_is_historical() {
        let s = setup(false).await;
        let a = auth(&s, "owner-a").await;
        let input = payload(&s, json!({"kind":"user","user_id":"editor-a"})).await;
        let before = snapshot(&s).await;
        COMMIT_FAULT.with(|x| x.set(1));
        assert!(mutate(&s, &a, "none", "rollback", &input).await.is_err());
        assert_eq!(before, snapshot(&s).await);
        COMMIT_FAULT.with(|x| x.set(2));
        let r = mutate(&s, &a, "none", "lost", &input).await.unwrap();
        assert!(r.historical);
        assert_eq!(r.maintenance_user_id.as_deref(), Some("editor-a"));
        let mut wrong = input.clone();
        wrong["maintenance"] = json!({"kind":"unassigned"});
        assert!(historical(&s, &a, "none", "lost", &wrong).await.is_err());
    }
    #[tokio::test]
    async fn maintenance_true_wal_catalog_current_session_and_target_rechecks() {
        let s = setup(true).await;
        let a = auth(&s, "owner-a").await;
        CATALOG_SQL
            .with(|x| *x.borrow_mut() = Some("DELETE FROM sessions WHERE user_id='owner-a'"));
        assert!(get_targets(
            State(s.clone()),
            a,
            Path(("space-a".into(), "maintenance-page".into()))
        )
        .await
        .is_err());
        let s = setup(true).await;
        let a = auth(&s, "owner-a").await;
        CATALOG_SQL.with(|x| {
            *x.borrow_mut() =
                Some("UPDATE users SET display_name='Changed directory' WHERE id='editor-a'")
        });
        assert!(get_targets(
            State(s.clone()),
            a,
            Path(("space-a".into(), "maintenance-page".into()))
        )
        .await
        .is_err());
    }
    #[tokio::test]
    async fn maintenance_true_wal_fence_wait_rechecks_session() {
        let s = setup(true).await;
        let a = auth(&s, "owner-a").await;
        let input = payload(&s, json!({"kind":"user","user_id":"editor-a"})).await;
        let mut blocker = s.db.begin().await.unwrap();
        mutation_guards::lock_audience_for_mutation(&mut blocker)
            .await
            .unwrap();
        sqlx::query("DELETE FROM sessions WHERE user_id='owner-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let copy = s.clone();
        let task = tokio::spawn(async move { mutate(&copy, &a, "none", "wait", &input).await });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!task.is_finished());
        blocker.commit().await.unwrap();
        assert!(task.await.unwrap().is_err());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_maintenance_receipts")
                .fetch_one(&s.db)
                .await
                .unwrap(),
            0
        );
    }
    #[tokio::test]
    async fn maintenance_private_participation_locks_lifecycle_and_inactive_actor_are_enforced() {
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
                        "UPDATE documents SET published_at=NULL WHERE id='maintenance-page'",
                    )
                    .execute(&s.db)
                    .await
                    .unwrap();
                    sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('id-draft','maintenance-page','space-a','owner-a','Own','BODY',$1)").bind(STAMP).execute(&s.db).await.unwrap();
                    if case == "viewer-share" {
                        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('id-draft','editor-a','viewer','owner-a')").execute(&s.db).await.unwrap();
                    }
                }
                "locked" => {
                    sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='maintenance-page'").execute(&s.db).await.unwrap();
                }
                "archive" | "trash" => {
                    sqlx::query(&format!(
                        "UPDATE documents SET {}=$1 WHERE id='maintenance-page'",
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
                    sqlx::query("INSERT INTO document_access_policies(document_id,space_id,mode,created_by,revision) VALUES('maintenance-page','space-a','restricted','owner-a','restriction')").execute(&s.db).await.unwrap();
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
            let p = payload(&s, json!({"kind":"user","user_id":"owner-a"})).await;
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
    async fn maintenance_create_get_duplicate_and_failed_target_are_consumed() {
        let _data_root=crate::storage::fixture_root::FixtureRoot::private();
        use tower::ServiceExt;
        let s = setup(false).await;
        crate::storage::init_data_dir().await.unwrap();
        let slug: String = sqlx::query_scalar("SELECT slug FROM spaces WHERE id='space-a'")
            .fetch_one(&s.db)
            .await
            .unwrap();
        crate::storage::init_space_dirs(&slug).await.unwrap();
        let app = super::super::documents::router()
            .merge(router())
            .with_state(s.clone());
        let request = |method: &str, path: &str, body: Value| {
            axum::http::Request::builder()
                .method(method)
                .uri(path)
                .header("Authorization", format!("Bearer {}", token(&s, "editor-a")))
                .header("Content-Type", "application/json")
                .body(axum::body::Body::from(body.to_string()))
                .unwrap()
        };
        let response=app.clone().oneshot(request("POST","/spaces/space-a/documents",json!({"title":"Responsible page","content":"Private body","maintenance_user_id":"owner-a"}))).await.unwrap();
        assert_eq!(response.status(), 201);
        let created: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(created["maintenance_user_id"], "owner-a");
        let id = created["id"].as_str().unwrap();
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                &format!("/spaces/space-a/documents/{id}"),
                json!({}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let current: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(current["maintenance_user_id"], "owner-a");
        assert_eq!(current["maintenance_edit_allowed"], true);
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/spaces/space-a/documents/{id}/duplicate"),
                json!({}),
            ))
            .await
            .unwrap();
        assert!(response.status().is_success());
        let copy: Value = serde_json::from_slice(
            &axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap(),
        )
        .unwrap();
        assert_eq!(copy["maintenance_user_id"], Value::Null);
        assert_eq!(copy["maintenance_group_id"], Value::Null);
        let a = auth(&s, "editor-a").await;
        let _ = a;
        let bookkeeping: (String, String) = sqlx::query_as(
            "SELECT last_seen_at,expires_at FROM sessions WHERE id='metadata-editor-a'",
        )
        .fetch_one(&s.db)
        .await
        .unwrap();
        let before = snapshot(&s).await;
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/spaces/space-a/documents",
                json!({"title":"Invalid","maintenance_user_id":"missing"}),
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), 404);
        sqlx::query(
            "UPDATE sessions SET last_seen_at=$1,expires_at=$2 WHERE id='metadata-editor-a'",
        )
        .bind(&bookkeeping.0)
        .bind(&bookkeeping.1)
        .execute(&s.db)
        .await
        .unwrap();
        assert_eq!(before, snapshot(&s).await);
        let before = snapshot(&s).await;
        let response=app.oneshot(request("POST","/spaces/space-a/documents",json!({"title":"Invalid pair","maintenance_user_id":"owner-a","maintenance_group_id":"missing"}))).await.unwrap();
        assert_eq!(response.status(), 400);
        sqlx::query(
            "UPDATE sessions SET last_seen_at=$1,expires_at=$2 WHERE id='metadata-editor-a'",
        )
        .bind(&bookkeeping.0)
        .bind(&bookkeeping.1)
        .execute(&s.db)
        .await
        .unwrap();
        assert_eq!(before, snapshot(&s).await);
    }
}

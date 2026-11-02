use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        // Property schema (space-level)
        .route(
            "/spaces/{space_id}/properties",
            get(list_property_schema).post(create_property_schema),
        )
        .route(
            "/spaces/{space_id}/properties/reorder",
            axum::routing::put(reorder_property_schema),
        )
        .route(
            "/spaces/{space_id}/properties/{property_id}",
            axum::routing::put(update_property_schema).delete(delete_property_schema),
        )
        // Property values (document-level)
        .route(
            "/spaces/{space_id}/documents/{doc_id}/properties",
            get(list_property_values).put(set_property_values),
        )
        // Backlinks
        .route(
            "/spaces/{space_id}/documents/{doc_id}/backlinks",
            get(list_backlinks),
        )
}

#[derive(Deserialize)]
pub struct CreatePropertySchemaRequest {
    pub name: String,
    pub property_type: String,
    pub options: Option<serde_json::Value>,
    pub source_id: Option<String>,
    pub created_at: Option<String>,
}

#[derive(Deserialize)]
pub struct ReorderPropertySchemaRequest {
    pub property_ids: Vec<String>,
}

#[derive(Serialize)]
pub struct PropertySchemaResponse {
    pub id: String,
    pub name: String,
    pub property_type: String,
    pub options: Option<serde_json::Value>,
    pub sort_order: i32,
}

#[derive(Serialize)]
pub struct PropertyValueResponse {
    pub property_id: String,
    pub property_name: String,
    pub property_type: String,
    pub options: Option<serde_json::Value>,
    pub value: Option<String>,
}

#[derive(Deserialize)]
pub struct SetPropertyValuesRequest {
    pub values: Vec<PropertyValueEntry>,
}

#[derive(Deserialize)]
pub struct PropertyValueEntry {
    pub source_id: Option<String>,
    pub property_id: String,
    pub value: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Serialize)]
pub struct BacklinkResponse {
    pub document_id: String,
    pub title: String,
    pub space_id: String,
    pub space_name: String,
}

fn metadata_actor(auth: &AuthUser) -> Result<&str, AppError> {
    if matches!(
        super::access::principal_from_auth(auth)?,
        super::access::Principal::ServiceKey { .. }
    ) {
        return Err(AppError::Unauthorized(
            "Metadata principal unavailable".into(),
        ));
    }
    Ok(auth.claims.sub.as_str())
}
async fn require_metadata_space_mutation(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    owner_only: bool,
) -> Result<(), AppError> {
    let access = super::access::resolve_space_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        super::access::Purpose::Routine,
    )
    .await?;
    if !(if owner_only {
        access.allows(super::access::Permission::ManageSpace)
    } else {
        access.allows(super::access::Permission::EditContent)
    }) {
        return Err(AppError::NotFound("Metadata target unavailable".into()));
    }
    Ok(())
}
async fn require_metadata_page_mutation(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    page: &str,
) -> Result<(), AppError> {
    require_metadata_space_mutation(conn, auth, space, false).await?;
    let locked=sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(page).bind(space).execute(&mut *conn).await.map_err(AppError::internal)?;
    if locked.rows_affected() != 1 {
        return Err(AppError::NotFound("Document unavailable".into()));
    }
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        page,
        super::access::PageOperation::Edit,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}

pub(crate) async fn mutate_property_create(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    req: CreatePropertySchemaRequest,
    prop_id: String,
    created_at: String,
    sort_order: Option<i32>,
    replay: bool,
    match_time: bool,
) -> Result<(PropertySchemaResponse, bool), AppError> {
    metadata_actor(auth)?;
    super::property_validation::validate_schema(
        &req.name,
        &req.property_type,
        req.options.as_ref(),
    )?;

    let options_json = req.options.as_ref().map(|o| o.to_string());

    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    let existing: Option<(String,String,String,Option<String>,i32,String)> = sqlx::query_as("SELECT space_id,name,property_type,options,sort_order,created_at FROM properties_schema WHERE id=$1").bind(&prop_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if let Some((scope, name, kind, options, sort, created)) = existing {
        let parsed = options
            .as_deref()
            .map(serde_json::from_str::<serde_json::Value>)
            .transpose()
            .map_err(AppError::internal)?;
        let time_matches = !match_time
            || chrono::DateTime::parse_from_rfc3339(&created)
                .ok()
                .zip(chrono::DateTime::parse_from_rfc3339(&created_at).ok())
                .is_some_and(|(a, b)| a == b);
        if !replay
            || scope != space_id
            || name != req.name
            || kind != req.property_type
            || parsed != req.options
            || sort_order.is_some_and(|requested| requested != sort)
            || !time_matches
        {
            return Err(AppError::Conflict(
                "Property import identity conflicts with canonical metadata".into(),
            ));
        }
        require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
        tx.commit().await.map_err(AppError::internal)?;
        return Ok((
            PropertySchemaResponse {
                id: prop_id,
                name,
                property_type: kind,
                options: parsed,
                sort_order: sort,
            },
            true,
        ));
    }
    let max_sort: (Option<i32>,) =
        sqlx::query_as("SELECT MAX(sort_order) FROM properties_schema WHERE space_id = $1")
            .bind(&space_id)
            .fetch_one(&mut *tx)
            .await
            .map_err(AppError::internal)?;

    let sort_order = match sort_order {
        Some(value) => value,
        None => max_sort
            .0
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| AppError::Conflict("Property order limit reached".into()))?,
    };

    let inserted = sqlx::query(
        "INSERT INTO properties_schema (id, space_id, name, property_type, options, sort_order, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(&prop_id)
    .bind(&space_id)
    .bind(&req.name)
    .bind(&req.property_type)
    .bind(&options_json)
    .bind(sort_order)
    .bind(&created_at)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if inserted.rows_affected() != 1 {
        return Err(AppError::Conflict("Metadata write did not apply".into()));
    }

    require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok((
        PropertySchemaResponse {
            id: prop_id,
            name: req.name,
            property_type: req.property_type,
            options: req.options,
            sort_order,
        },
        false,
    ))
}

pub(crate) async fn mutate_property_update(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    property_id: String,
    req: CreatePropertySchemaRequest,
    sort_order: Option<i32>,
) -> Result<(), AppError> {
    metadata_actor(auth)?;
    super::property_validation::validate_schema(
        &req.name,
        &req.property_type,
        req.options.as_ref(),
    )?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    let locked =
        sqlx::query("UPDATE properties_schema SET name = name WHERE id = $1 AND space_id = $2")
            .bind(&property_id)
            .bind(&space_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if locked.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "Property not found in this space".into(),
        ));
    }
    let old_type: Option<String> = sqlx::query_scalar(
        "SELECT property_type FROM properties_schema WHERE id = $1 AND space_id = $2",
    )
    .bind(&property_id)
    .bind(&space_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    let old_type =
        old_type.ok_or_else(|| AppError::NotFound("Property not found in this space".into()))?;
    let values: Vec<Option<String>> =
        sqlx::query_scalar("SELECT value FROM property_values WHERE property_id = $1")
            .bind(&property_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if old_type != req.property_type
        && values
            .iter()
            .any(|value| value.as_deref().is_some_and(|value| !value.is_empty()))
    {
        return Err(AppError::BadRequest("Property type cannot change while values are populated; export or explicitly convert/clear those values first".into()));
    }
    for value in &values {
        super::property_validation::validate_value(&req.property_type, req.options.as_ref(), value.as_deref())
            .map_err(|_| AppError::BadRequest("Schema change would invalidate existing values; keep their options or explicitly convert/clear those values first".into()))?;
    }
    let options_json = req.options.as_ref().map(|o| o.to_string());

    // Schema locking can wait behind a nonparticipating legacy writer.
    require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    let result = sqlx::query(
        "UPDATE properties_schema SET name = $1, property_type = $2, options = $3, sort_order = COALESCE($6, sort_order) \
         WHERE id = $4 AND space_id = $5",
    )
    .bind(&req.name)
    .bind(&req.property_type)
    .bind(&options_json)
    .bind(&property_id)
    .bind(&space_id)
    .bind(sort_order)
    .execute(&mut *tx)
    .await
    .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "Property not found in this space".into(),
        ));
    }

    require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(())
}

pub(crate) async fn mutate_property_reorder(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    req: ReorderPropertySchemaRequest,
) -> Result<(), AppError> {
    metadata_actor(auth)?;
    let mut ordered = req.property_ids.clone();
    ordered.sort();
    if ordered.windows(2).any(|ids| ids[0] == ids[1]) {
        return Err(AppError::BadRequest(
            "A property may only appear once in a reorder".into(),
        ));
    }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    // Lock and resolve the whole batch before changing any sort order.
    for id in &ordered {
        let row =
            sqlx::query("UPDATE properties_schema SET name = name WHERE id = $1 AND space_id = $2")
                .bind(id)
                .bind(&space_id)
                .execute(&mut *tx)
                .await
                .map_err(AppError::internal)?;
        if row.rows_affected() != 1 {
            return Err(AppError::NotFound(
                "Property not found in this space".into(),
            ));
        }
    }
    require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    for (index, id) in req.property_ids.iter().enumerate() {
        let row = sqlx::query(
            "UPDATE properties_schema SET sort_order = $1 WHERE id = $2 AND space_id = $3",
        )
        .bind((index + 1) as i32)
        .bind(id)
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if row.rows_affected() != 1 {
            return Err(AppError::NotFound(
                "Property not found in this space".into(),
            ));
        }
    }
    require_metadata_space_mutation(&mut tx, auth, &space_id, false).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(())
}

pub(crate) async fn mutate_property_delete(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    property_id: String,
) -> Result<(), AppError> {
    metadata_actor(auth)?;
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_metadata_space_mutation(&mut tx, auth, &space_id, true).await?;
    let exists =
        sqlx::query("UPDATE properties_schema SET name = name WHERE id = $1 AND space_id = $2")
            .bind(&property_id)
            .bind(&space_id)
            .execute(&mut *tx)
            .await
            .map_err(AppError::internal)?;
    if exists.rows_affected() != 1 {
        return Err(AppError::NotFound(
            "Property not found in this space".into(),
        ));
    }
    require_metadata_space_mutation(&mut tx, auth, &space_id, true).await?;
    let foreign: Option<i64> = sqlx::query_scalar("SELECT 1 FROM property_values pv LEFT JOIN documents d ON d.id = pv.document_id WHERE pv.property_id = $1 AND (d.id IS NULL OR d.space_id <> $2) LIMIT 1")
        .bind(&property_id).bind(&space_id).fetch_optional(&mut *tx).await.map_err(AppError::internal)?;
    if foreign.is_some() {
        return Err(AppError::Conflict(
            "Property has values outside this space; review them before deleting".into(),
        ));
    }
    let result = sqlx::query("DELETE FROM properties_schema WHERE id = $1 AND space_id = $2")
        .bind(&property_id)
        .bind(&space_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(
            "Property not found in this space".into(),
        ));
    }

    require_metadata_space_mutation(&mut tx, auth, &space_id, true).await?;
    tx.commit().await.map_err(AppError::internal)?;
    Ok(())
}

pub(crate) async fn mutate_property_values(
    state: &AppState,
    auth: &AuthUser,
    space_id: String,
    doc_id: String,
    mut prepared: Vec<(PropertyValueEntry, String, String, String)>,
) -> Result<(), AppError> {
    metadata_actor(auth)?;
    let principal = super::access::principal_from_auth(auth)?;
    if prepared.len() > 100 {
        return Err(AppError::BadRequest(
            "At most 100 property values may be supplied".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    if prepared
        .iter()
        .any(|(entry, _, _, _)| !seen.insert(entry.property_id.clone()))
    {
        return Err(AppError::BadRequest(
            "A property may only appear once in a batch".into(),
        ));
    }
    let mut tx = state.db.begin().await.map_err(AppError::internal)?;
    super::mutation_guards::lock_spaces_for_mutation(&mut tx, &[&space_id]).await?;
    require_metadata_page_mutation(&mut tx, auth, &space_id, &doc_id).await?;
    // Deterministic schema-lock order prevents two overlapping batches deadlocking.
    prepared.sort_by(|a, b| a.0.property_id.cmp(&b.0.property_id));
    let assignments: Vec<(&str, Option<&str>)> = prepared
        .iter()
        .map(|(entry, _, _, _)| (entry.property_id.as_str(), entry.value.as_deref()))
        .collect();
    let relation_targets = super::property_validation::validate_assignment_batch(
        &mut tx,
        &space_id,
        principal,
        &assignments,
    )
    .await?;
    // Recheck actor policy after all schema/relation/user row-lock waits.
    require_metadata_page_mutation(&mut tx, auth, &space_id, &doc_id).await?;
    for (entry, id, created_at, updated_at) in prepared {
        let inserted = sqlx::query(
            "INSERT INTO property_values (id, document_id, property_id, value, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6) \
             ON CONFLICT(document_id, property_id) \
             DO UPDATE SET value = $4, updated_at = $6",
        )
        .bind(&id)
        .bind(&doc_id)
        .bind(&entry.property_id)
        .bind(&entry.value)
        .bind(&created_at)
        .bind(&updated_at)
        .execute(&mut *tx)
        .await
        .map_err(AppError::internal)?;
        if inserted.rows_affected() != 1 {
            return Err(AppError::Conflict("Metadata write did not apply".into()));
        }
    }

    require_metadata_page_mutation(&mut tx, auth, &space_id, &doc_id).await?;
    super::property_validation::revalidate_relation_targets(
        &mut tx,
        &space_id,
        principal,
        &relation_targets,
    )
    .await?;
    #[cfg(test)]
    if let Ok(Some(hook)) =
        PROPERTY_VALUES_AFTER_RELATIONS.try_with(|slot| slot.borrow_mut().take())
    {
        hook.await;
    }
    // All relation policy/source awaits precede this original method-scope proof.
    super::access::require_current_human_preference_write(&mut tx, auth).await?;
    tx.commit().await.map_err(AppError::internal)?;

    Ok(())
}

#[cfg(test)]
tokio::task_local! {
    static PROPERTY_VALUES_AFTER_RELATIONS: std::cell::RefCell<Option<std::pin::Pin<Box<dyn std::future::Future<Output=()> + Send>>>>;
}

async fn list_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    metadata_actor(&auth)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_metadata_space_view(&mut conn, &auth, &space_id).await?;
    let schemas:Vec<(String,String,String,Option<String>,i32)>=sqlx::query_as("SELECT id,name,property_type,options,sort_order FROM properties_schema WHERE space_id=$1 ORDER BY sort_order").bind(&space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    #[cfg(test)]
    tests::pause_metadata_read().await;
    let result: Vec<PropertySchemaResponse> = schemas
        .into_iter()
        .map(
            |(id, name, property_type, opts, sort_order)| PropertySchemaResponse {
                id,
                name,
                property_type,
                options: opts.and_then(|raw| serde_json::from_str(&raw).ok()),
                sort_order,
            },
        )
        .collect();
    require_metadata_space_view(&mut conn, &auth, &space_id).await?;
    Ok(Json(result))
}

async fn create_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<CreatePropertySchemaRequest>,
) -> Result<impl IntoResponse, AppError> {
    let id = super::routes::import_id_or_uuid(req.source_id.as_deref())?;
    let created = super::routes::import_timestamp_or_now(req.created_at.as_deref())?;
    let (result, _) = mutate_property_create(
        &state, &auth, space_id, req, id, created, None, false, false,
    )
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, property_id)): Path<(String, String)>,
    Json(req): Json<CreatePropertySchemaRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_property_update(&state, &auth, space_id, property_id, req, None).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reorder_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
    Json(req): Json<ReorderPropertySchemaRequest>,
) -> Result<impl IntoResponse, AppError> {
    mutate_property_reorder(&state, &auth, space_id, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_property_schema(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, property_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    mutate_property_delete(&state, &auth, space_id, property_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_property_values(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    metadata_actor(&auth)?;
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    require_metadata_page_view(&mut conn, &auth, &space_id, &doc_id).await?;
    let values:Vec<(String,String,String,Option<String>,Option<String>)>=sqlx::query_as("SELECT ps.id,ps.name,ps.property_type,ps.options,pv.value FROM properties_schema ps LEFT JOIN property_values pv ON ps.id=pv.property_id AND pv.document_id=$1 WHERE ps.space_id=$2 ORDER BY ps.sort_order").bind(&doc_id).bind(&space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    // Preserve raw relation values. An opaque restricted-target presentation needs
    // a consumer write audit; empty redaction could overwrite hidden data on save.
    #[cfg(test)]
    tests::pause_metadata_read().await;
    let result: Vec<PropertyValueResponse> = values
        .into_iter()
        .map(
            |(property_id, property_name, property_type, options, value)| PropertyValueResponse {
                property_id,
                property_name,
                property_type,
                options: options.and_then(|raw| serde_json::from_str(&raw).ok()),
                value,
            },
        )
        .collect();
    require_metadata_page_view(&mut conn, &auth, &space_id, &doc_id).await?;
    Ok(Json(result))
}

async fn set_property_values(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
    Json(req): Json<SetPropertyValuesRequest>,
) -> Result<impl IntoResponse, AppError> {
    if req.values.len() > 100 {
        return Err(AppError::BadRequest(
            "At most 100 property values may be supplied".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    if req
        .values
        .iter()
        .any(|entry| !seen.insert(entry.property_id.clone()))
    {
        return Err(AppError::BadRequest(
            "A property may only appear once in a batch".into(),
        ));
    }
    let mut prepared = Vec::new();
    for entry in req.values {
        let id = super::routes::import_id_or_uuid(entry.source_id.as_deref())?;
        let created = super::routes::import_timestamp_or_now(entry.created_at.as_deref())?;
        let updated =
            super::routes::import_timestamp_or_default(entry.updated_at.as_deref(), &created)?;
        prepared.push((entry, id, created, updated));
    }
    mutate_property_values(&state, &auth, space_id, doc_id, prepared).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn require_metadata_space_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
) -> Result<(), AppError> {
    let access = super::access::resolve_space_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        super::access::Purpose::Routine,
    )
    .await?;
    if !access.allows(super::access::Permission::View) {
        return Err(AppError::NotFound("Metadata target unavailable".into()));
    }
    Ok(())
}
async fn require_metadata_page_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    page: &str,
) -> Result<(), AppError> {
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        page,
        super::access::PageOperation::View,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}

async fn backlink_page_view(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    document: &str,
) -> Result<(), AppError> {
    super::access::resolve_page_access(
        conn,
        super::access::principal_from_auth(auth)?,
        space,
        document,
        super::access::PageOperation::View,
        super::access::Purpose::Routine,
    )
    .await?;
    Ok(())
}
struct BacklinkReadSource {
    document_id: String,
    published_at: Option<String>,
    content_path: String,
    updated_at: String,
    draft_id: Option<String>,
}
async fn backlink_read_source_current(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    source: &BacklinkReadSource,
) -> Result<bool, AppError> {
    match backlink_page_view(conn, auth, space, &source.document_id).await {
        Ok(()) => {}
        Err(AppError::NotFound(_)) => return Ok(false),
        Err(error) => return Err(error),
    }
    if let Some(published) = source.published_at.as_deref() {
        match super::documents::require_current_document_read_source(
            conn,
            space,
            &source.document_id,
            published,
            &source.content_path,
            &source.updated_at,
        )
        .await
        {
            Ok(()) => {}
            Err(AppError::Conflict(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
    } else {
        let current:Option<(Option<String>,)> = sqlx::query_as("SELECT published_at FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&source.document_id).bind(space).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
        if current != Some((None,)) {
            return Ok(false);
        }
        let Some(draft) = source.draft_id.as_deref() else {
            return Ok(false);
        };
        match super::documents::require_discovery_draft_view(
            conn,
            auth,
            space,
            &source.document_id,
            draft,
        )
        .await
        {
            Ok(()) => {}
            Err(AppError::NotFound(_)) => return Ok(false),
            Err(error) => return Err(error),
        }
    }
    Ok(true)
}
async fn list_backlinks(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((space_id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    backlink_page_view(&mut conn, &auth, &space_id, &doc_id).await?;
    let target:Option<(String,Option<String>,String,String)>=sqlx::query_as("SELECT title,published_at,content_path,updated_at FROM documents WHERE id=$1 AND space_id=$2 AND archived_at IS NULL AND deleted_at IS NULL").bind(&doc_id).bind(&space_id).fetch_optional(&mut *conn).await.map_err(AppError::internal)?;
    let (mut title, published_at, content_path, updated_at) =
        target.ok_or_else(|| AppError::NotFound("Target unavailable".into()))?;
    let mut target = BacklinkReadSource {
        document_id: doc_id.clone(),
        published_at,
        content_path,
        updated_at,
        draft_id: None,
    };
    if target.published_at.is_none() {
        let draft =
            super::documents::readable_discovery_draft(&mut conn, &auth, &space_id, &doc_id)
                .await?;
        title = draft.title;
        target.draft_id = Some(draft.id);
    }
    let linking_docs:Vec<(String,String,String,String,Option<String>,String)>=sqlx::query_as("SELECT d.id,d.title,s.name,d.content_path,d.published_at,d.updated_at FROM documents d JOIN spaces s ON s.id=d.space_id WHERE d.id<>$1 AND d.space_id=$2 AND d.archived_at IS NULL AND d.deleted_at IS NULL").bind(&doc_id).bind(&space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut matches = Vec::new();
    for (document_id, mut source_title, space_name, content_path, published_at, updated_at) in
        linking_docs
    {
        match backlink_page_view(&mut conn, &auth, &space_id, &document_id).await {
            Ok(()) => {}
            Err(AppError::NotFound(_)) => continue,
            Err(error) => return Err(error),
        }
        let mut source = BacklinkReadSource {
            document_id: document_id.clone(),
            published_at,
            content_path,
            updated_at,
            draft_id: None,
        };
        let content = if source.published_at.is_some() {
            storage::read_document(&source.content_path)
                .await
                .unwrap_or_default()
        } else {
            let draft = match super::documents::readable_discovery_draft(
                &mut conn,
                &auth,
                &space_id,
                &document_id,
            )
            .await
            {
                Ok(draft) => draft,
                Err(AppError::NotFound(_)) => continue,
                Err(error) => return Err(error),
            };
            source_title = draft.title;
            source.draft_id = Some(draft.id);
            draft.content
        };
        if !backlink_read_source_current(&mut conn, &auth, &space_id, &source).await? {
            continue;
        }
        if contains_backlink_reference(&content, &title, &doc_id) {
            matches.push((
                BacklinkResponse {
                    document_id,
                    title: source_title,
                    space_id: space_id.clone(),
                    space_name,
                },
                source,
            ));
        }
    }
    let mut result = Vec::new();
    for (response, source) in matches {
        if backlink_read_source_current(&mut conn, &auth, &space_id, &source).await? {
            result.push(response);
        }
    }
    if !backlink_read_source_current(&mut conn, &auth, &space_id, &target).await? {
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(Json(result))
}

#[derive(Debug, PartialEq, Eq)]
struct WikiLinkReference {
    title: String,
    document_id: Option<String>,
}

fn contains_backlink_reference(content: &str, target_title: &str, target_doc_id: &str) -> bool {
    let normalized_target = normalize_title(target_title);
    let has_wiki_reference =
        extract_wiki_links(content)
            .iter()
            .any(|link| match link.document_id.as_deref() {
                Some(document_id) => document_id == target_doc_id,
                None => normalize_title(&link.title) == normalized_target,
            });

    if has_wiki_reference {
        return true;
    }

    extract_doc_id_links(content)
        .iter()
        .any(|doc_id| doc_id == target_doc_id)
}

fn normalize_title(title: &str) -> String {
    title.trim().to_lowercase()
}

fn extract_wiki_links(content: &str) -> Vec<WikiLinkReference> {
    let mut links = Vec::new();
    let mut offset = 0;

    while let Some(start_rel) = content[offset..].find("[[") {
        let start = offset + start_rel + 2;
        let Some(end_rel) = content[start..].find("]]") else {
            break;
        };
        let end = start + end_rel;
        let raw = &content[start..end];
        let (title, document_id) = raw
            .split_once('|')
            .map_or((raw, None), |(title, document_id)| {
                (title, Some(document_id))
            });
        let title = title.trim();
        if !title.is_empty() {
            let document_id = document_id
                .map(str::trim)
                .filter(|document_id| !document_id.is_empty())
                .map(ToOwned::to_owned);
            links.push(WikiLinkReference {
                title: title.to_string(),
                document_id,
            });
        }
        offset = end + 2;
    }

    links
}

fn extract_doc_id_links(content: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut offset = 0;

    while let Some(start_rel) = content[offset..].find("/docs/") {
        let start = offset + start_rel + "/docs/".len();
        let tail = &content[start..];
        let len = tail
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .count();
        if len > 0 {
            ids.push(tail[..len].to_string());
        }
        offset = start;
    }

    ids
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{
        contains_backlink_reference, extract_doc_id_links, extract_wiki_links, list_backlinks,
        WikiLinkReference,
    };
    use crate::auth::jwt::{Claims, TokenType};
    use crate::auth::middleware::AuthUser;
    use crate::auth::routes::AppError;
    use crate::AppState;
    use axum::extract::{Path, State};
    use axum::response::IntoResponse;
    use sqlx::any::AnyPoolOptions;
    use uuid::Uuid;

    async fn backlink_test_state() -> AppState {
        sqlx::any::install_default_drivers();
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        for statement in [
            "CREATE TABLE users (account_kind TEXT NOT NULL DEFAULT 'member',id TEXT PRIMARY KEY, role TEXT NOT NULL)",
            "CREATE TABLE spaces (id TEXT PRIMARY KEY, name TEXT NOT NULL, visibility TEXT NOT NULL)",
            "CREATE TABLE space_members (space_id TEXT NOT NULL, user_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE groups (id TEXT PRIMARY KEY)",
            "CREATE TABLE group_members (group_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_group_permissions (space_id TEXT NOT NULL, group_id TEXT NOT NULL, role TEXT NOT NULL)",
            "CREATE TABLE space_user_blocks (space_id TEXT NOT NULL, user_id TEXT NOT NULL)",
            "CREATE TABLE space_roles (id TEXT NOT NULL, space_id TEXT NOT NULL, permissions TEXT NOT NULL)",
            "CREATE TABLE global_roles (id TEXT PRIMARY KEY, name TEXT NOT NULL, description TEXT NOT NULL DEFAULT '', permissions TEXT NOT NULL DEFAULT '[]', built_in INTEGER NOT NULL DEFAULT 0)",
            "CREATE TABLE group_global_roles (group_id TEXT NOT NULL, role_id TEXT NOT NULL, PRIMARY KEY(group_id, role_id))",
            "CREATE TABLE documents (
                id TEXT PRIMARY KEY,
                space_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content_path TEXT NOT NULL,
                deleted_at TEXT
            )",
        ] {
            sqlx::query(statement)
                .execute(&db)
                .await
                .expect("create backlink fixture table");
        }
        for (space_id, name) in [("space-a", "Space A"), ("space-b", "Space B")] {
            sqlx::query("INSERT INTO spaces (id, name, visibility) VALUES ($1, $2, 'private')")
                .bind(space_id)
                .bind(name)
                .execute(&db)
                .await
                .expect("seed backlink fixture space");
        }
        for (user_id, space_id, role) in [
            ("owner-a", "space-a", "owner"),
            ("viewer-a", "space-a", "viewer"),
        ] {
            sqlx::query("INSERT INTO users (id, role) VALUES ($1, 'member')")
                .bind(user_id)
                .execute(&db)
                .await
                .expect("seed backlink fixture user");
            sqlx::query("INSERT INTO space_members (space_id, user_id, role) VALUES ($1, $2, $3)")
                .bind(space_id)
                .bind(user_id)
                .bind(role)
                .execute(&db)
                .await
                .expect("seed backlink fixture membership");
        }
        for (id, space_id, title) in [
            ("target-a", "space-a", "Shared title"),
            ("target-b", "space-b", "Shared title"),
        ] {
            sqlx::query(
                "INSERT INTO documents (id, space_id, title, content_path) VALUES ($1, $2, $3, '')",
            )
            .bind(id)
            .bind(space_id)
            .bind(title)
            .execute(&db)
            .await
            .expect("seed backlink fixture document");
        }
        let index_path =
            std::env::temp_dir().join(format!("starlane-notes-backlinks-test-{}", Uuid::new_v4()));
        AppState {
            db,
            jwt_secret: "test-secret-with-enough-length-for-jwt".to_string(),
            base_url: "http://localhost:3000".to_string(),
            search_index: crate::search::SearchIndex::new(index_path).expect("search index"),
            oauth_cache: crate::auth::routes::OAuthStateCache::new(),
            ws_rooms: crate::sync::WsRooms::default(),
        }
    }

    pub(crate) fn test_auth(user_id: &str) -> AuthUser {
        AuthUser {
            claims: Claims {
                sub: user_id.to_string(),
                sid: format!("metadata-{user_id}"),
                email: format!("{user_id}@example.test"),
                role: "member".to_string(),
                exp: i64::MAX,
                iat: 0,
                token_type: TokenType::Access,
            },
            api_key_scopes: None,
        }
    }

    pub(crate) async fn property_test_state() -> AppState {
        let mut state = backlink_test_state().await;
        let db = AnyPoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&db).await.unwrap();
        state.db = db;
        for id in ["owner-a", "viewer-a", "editor-a"] {
            sqlx::query(
                "INSERT INTO users (id,email,display_name,role) VALUES ($1,$2,$1,'member')",
            )
            .bind(id)
            .bind(format!("{id}@example.test"))
            .execute(&state.db)
            .await
            .unwrap();
        }
        for user in ["owner-a", "viewer-a", "editor-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'metadata-test','2999-01-01T00:00:00Z')").bind(format!("metadata-{user}")).bind(user).execute(&state.db).await.unwrap();
        }
        for space in ["space-a", "space-b"] {
            sqlx::query("INSERT INTO spaces (id,name,slug,visibility,created_by) VALUES ($1,$1,$1,'private','owner-a')")
                .bind(space).execute(&state.db).await.unwrap();
        }
        for (user, role) in [
            ("owner-a", "owner"),
            ("viewer-a", "viewer"),
            ("editor-a", "editor"),
        ] {
            sqlx::query(
                "INSERT INTO space_members (id,space_id,user_id,role) VALUES ($1,'space-a',$2,$3)",
            )
            .bind(format!("member-{user}"))
            .bind(user)
            .bind(role)
            .execute(&state.db)
            .await
            .unwrap();
        }
        for (id, space) in [("target-a", "space-a"), ("target-b", "space-b")] {
            sqlx::query("INSERT INTO documents (id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES ($1,$2,$1,'','owner-a','owner-a','published')")
                .bind(id).bind(space).execute(&state.db).await.unwrap();
        }
        sqlx::query("INSERT INTO properties_schema(id, space_id, name, property_type, options) VALUES ('number-a','space-a','Count','number',NULL), ('select-a','space-a','Choice','select','[\"A\",\"B\"]'), ('foreign','space-b','Secret','text',NULL), ('relation-a','space-a','Related','relation',NULL)")
            .execute(&state.db).await.unwrap();
        state
    }

    fn value_entry(property_id: &str, value: &str) -> super::PropertyValueEntry {
        super::PropertyValueEntry {
            property_id: property_id.into(),
            value: Some(value.into()),
            source_id: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[tokio::test]
    async fn property_batches_reject_foreign_and_malformed_entries_without_partial_writes() {
        let state = property_test_state().await;
        for invalid in [
            value_entry("foreign", "secret"),
            value_entry("number-a", "NaN"),
            value_entry("select-a", "C"),
        ] {
            let result = super::set_property_values(
                State(state.clone()),
                test_auth("owner-a"),
                Path(("space-a".into(), "target-a".into())),
                axum::Json(super::SetPropertyValuesRequest {
                    values: vec![value_entry("select-a", "A"), invalid],
                }),
            )
            .await;
            assert!(matches!(result, Err(AppError::BadRequest(_))));
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM property_values")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
        let result = super::set_property_values(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(super::SetPropertyValuesRequest {
                values: vec![value_entry("number-a", "3"), value_entry("select-a", "B")],
            }),
        )
        .await;
        assert!(result.is_ok());
        // A collision at the second INSERT must roll back the first INSERT.
        sqlx::query("DELETE FROM property_values")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO property_values (id, document_id, property_id, value) VALUES ('aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa','target-b','foreign','kept')").execute(&state.db).await.unwrap();
        let mut collision = value_entry("select-a", "A");
        collision.source_id = Some("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into());
        let result = super::set_property_values(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(super::SetPropertyValuesRequest {
                values: vec![value_entry("number-a", "3"), collision],
            }),
        )
        .await;
        assert!(result.is_err());
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM property_values WHERE document_id = 'target-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn incompatible_schema_changes_preserve_schema_and_values() {
        let state = property_test_state().await;
        sqlx::query("INSERT INTO property_values (id,document_id,property_id,value) VALUES ('existing','target-a','select-a','A')").execute(&state.db).await.unwrap();
        for (kind, options) in [
            ("text", None),
            ("select", Some(serde_json::json!(["B"]))),
            ("invented", None),
        ] {
            let result = super::update_property_schema(
                State(state.clone()),
                test_auth("owner-a"),
                Path(("space-a".into(), "select-a".into())),
                axum::Json(super::CreatePropertySchemaRequest {
                    name: "Renamed".into(),
                    property_type: kind.into(),
                    options,
                    source_id: None,
                    created_at: None,
                }),
            )
            .await;
            assert!(matches!(result, Err(AppError::BadRequest(_))));
            let schema: (String, String) = sqlx::query_as(
                "SELECT name,property_type FROM properties_schema WHERE id='select-a'",
            )
            .fetch_one(&state.db)
            .await
            .unwrap();
            assert_eq!(schema, ("Choice".into(), "select".into()));
            let value: String =
                sqlx::query_scalar("SELECT value FROM property_values WHERE id='existing'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(value, "A");
        }
    }

    #[tokio::test]
    async fn relations_require_available_same_space_pages_or_explicit_draft_access() {
        let state = property_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        for target in ["target-b", "missing", "target-a"] {
            let mut conn = state.db.acquire().await.unwrap();
            let result = super::super::property_validation::validate_assignment(
                &mut conn,
                "space-a",
                super::super::access::principal_from_auth(&test_auth("owner-a")).unwrap(),
                "relation-a",
                Some(target),
            )
            .await;
            assert!(matches!(result, Err(AppError::BadRequest(_))));
        }
        sqlx::query("INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES ('draft','space-a','target-a','owner-a','Draft','old')").execute(&state.db).await.unwrap();
        {
            let mut conn = state.db.acquire().await.unwrap();
            assert!(super::super::property_validation::validate_assignment(
                &mut conn,
                "space-a",
                super::super::access::principal_from_auth(&test_auth("owner-a")).unwrap(),
                "relation-a",
                Some("target-a")
            )
            .await
            .is_ok());
            assert!(super::super::property_validation::validate_assignment(
                &mut conn,
                "space-a",
                super::super::access::principal_from_auth(&test_auth("viewer-a")).unwrap(),
                "relation-a",
                Some("target-a")
            )
            .await
            .is_err());
        }
        sqlx::query("UPDATE documents SET published_at='2026-10-07T00:00:00Z', archived_at='2026-10-07T01:00:00Z' WHERE id='target-a'").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert!(super::super::property_validation::validate_assignment(
            &mut conn,
            "space-a",
            super::super::access::principal_from_auth(&test_auth("owner-a")).unwrap(),
            "relation-a",
            Some("target-a")
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn reorder_requires_edit_content_and_rejects_whole_invalid_batches() {
        let state = property_test_state().await;
        sqlx::query(
            "UPDATE properties_schema SET sort_order = CASE id WHEN 'number-a' THEN 5 ELSE 9 END",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let before: Vec<(String, i64)> =
            sqlx::query_as("SELECT id,sort_order FROM properties_schema ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        for (actor, ids) in [
            ("viewer-a", vec!["select-a", "number-a"]),
            ("owner-a", vec!["number-a", "foreign"]),
            ("owner-a", vec!["number-a", "number-a"]),
        ] {
            let result = super::reorder_property_schema(
                State(state.clone()),
                test_auth(actor),
                Path("space-a".into()),
                axum::Json(super::ReorderPropertySchemaRequest {
                    property_ids: ids.into_iter().map(str::to_owned).collect(),
                }),
            )
            .await;
            assert!(result.is_err());
            let after: Vec<(String, i64)> =
                sqlx::query_as("SELECT id,sort_order FROM properties_schema ORDER BY id")
                    .fetch_all(&state.db)
                    .await
                    .unwrap();
            assert_eq!(after, before);
        }
        sqlx::query("CREATE TRIGGER fail_reorder BEFORE UPDATE OF sort_order ON properties_schema WHEN NEW.id='select-a' BEGIN SELECT RAISE(ABORT,'forced late failure'); END").execute(&state.db).await.unwrap();
        assert!(super::reorder_property_schema(
            State(state.clone()),
            test_auth("owner-a"),
            Path("space-a".into()),
            axum::Json(super::ReorderPropertySchemaRequest {
                property_ids: vec!["number-a".into(), "select-a".into()]
            })
        )
        .await
        .is_err());
        let after: Vec<(String, i64)> =
            sqlx::query_as("SELECT id,sort_order FROM properties_schema ORDER BY id")
                .fetch_all(&state.db)
                .await
                .unwrap();
        assert_eq!(after, before);
        sqlx::query("DROP TRIGGER fail_reorder")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO space_roles (id,space_id,name,permissions,created_by) VALUES ('custom-edit','space-a','Custom','[\"edit_content\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='custom-edit' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::reorder_property_schema(
            State(state.clone()),
            test_auth("editor-a"),
            Path("space-a".into()),
            axum::Json(super::ReorderPropertySchemaRequest {
                property_ids: vec!["select-a".into(), "number-a".into()]
            })
        )
        .await
        .is_ok());
    }

    #[tokio::test]
    async fn property_writes_require_active_editable_page_and_draft_participation() {
        let state = property_test_state().await;
        for change in ["UPDATE documents SET archived_at='old' WHERE id='target-a'", "UPDATE documents SET archived_at=NULL,deleted_at='old' WHERE id='target-a'", "UPDATE documents SET deleted_at=NULL,locked=1,locked_by='editor-a' WHERE id='target-a'", "UPDATE documents SET locked=0,published_at=NULL WHERE id='target-a'"] {
            for statement in change.split("; ") { sqlx::query(statement).execute(&state.db).await.unwrap(); }
            assert!(super::set_property_values(State(state.clone()),test_auth("owner-a"),Path(("space-a".into(),"target-a".into())),axum::Json(super::SetPropertyValuesRequest{values:vec![value_entry("number-a","2")]})).await.is_err());
        }
        sqlx::query("INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES ('draft','space-a','target-a','editor-a','Private','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('draft','owner-a','viewer','editor-a')").execute(&state.db).await.unwrap();
        assert!(super::set_property_values(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(super::SetPropertyValuesRequest {
                values: vec![value_entry("number-a", "2")]
            })
        )
        .await
        .is_err());
        sqlx::query("UPDATE document_draft_access SET role='editor'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::set_property_values(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(super::SetPropertyValuesRequest {
                values: vec![value_entry("number-a", "2")]
            })
        )
        .await
        .is_ok());
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES ('space-a','owner-a','editor-a')").execute(&state.db).await.unwrap();
        assert!(super::set_property_values(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(super::SetPropertyValuesRequest {
                values: vec![value_entry("number-a", "3")]
            })
        )
        .await
        .is_err());
        let value: String = sqlx::query_scalar("SELECT value FROM property_values")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(value, "2");
    }

    #[tokio::test]
    async fn property_scope_is_rechecked_after_wait_and_foreign_cascade_is_blocked() {
        let state = property_test_state().await;
        let mut blocker = state.db.begin().await.unwrap();
        super::super::mutation_guards::lock_spaces_for_mutation(
            &mut blocker,
            &["space-a", "space-b"],
        )
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET space_id='space-b' WHERE id='target-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let queued_state = state.clone();
        let mut queued = tokio::spawn(async move {
            super::set_property_values(
                State(queued_state),
                test_auth("owner-a"),
                Path(("space-a".into(), "target-a".into())),
                axum::Json(super::SetPropertyValuesRequest {
                    values: vec![value_entry("number-a", "2")],
                }),
            )
            .await
            .map(|_| ())
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(30), &mut queued)
                .await
                .is_err()
        );
        blocker.commit().await.unwrap();
        assert!(matches!(queued.await.unwrap(), Err(AppError::NotFound(_))));
        sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES ('legacy','target-b','number-a','9')").execute(&state.db).await.unwrap();
        assert!(matches!(
            super::delete_property_schema(
                State(state.clone()),
                test_auth("owner-a"),
                Path(("space-a".into(), "number-a".into()))
            )
            .await,
            Err(AppError::Conflict(_))
        ));
        let value: String =
            sqlx::query_scalar("SELECT value FROM property_values WHERE id='legacy'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(value, "9");
    }

    #[tokio::test]
    async fn actor_policy_is_rechecked_after_metadata_row_locks() {
        for action in ["update", "reorder", "values", "values_commit", "delete"] {
            let state = property_test_state().await;
            if action == "values_commit" {
                sqlx::query("CREATE TRIGGER revoke_during_value_insert AFTER INSERT ON property_values BEGIN UPDATE users SET is_active=0 WHERE id='owner-a'; END").execute(&state.db).await.unwrap();
            } else {
                sqlx::query("CREATE TRIGGER revoke_during_schema_lock AFTER UPDATE OF name ON properties_schema BEGIN UPDATE users SET is_active=0 WHERE id='owner-a'; END").execute(&state.db).await.unwrap();
            }
            let denied = match action {
                "update" => super::update_property_schema(
                    State(state.clone()),
                    test_auth("owner-a"),
                    Path(("space-a".into(), "number-a".into())),
                    axum::Json(super::CreatePropertySchemaRequest {
                        name: "New".into(),
                        property_type: "number".into(),
                        options: None,
                        source_id: None,
                        created_at: None,
                    }),
                )
                .await
                .map(|_| ()),
                "reorder" => super::reorder_property_schema(
                    State(state.clone()),
                    test_auth("owner-a"),
                    Path("space-a".into()),
                    axum::Json(super::ReorderPropertySchemaRequest {
                        property_ids: vec!["number-a".into()],
                    }),
                )
                .await
                .map(|_| ()),
                "delete" => super::delete_property_schema(
                    State(state.clone()),
                    test_auth("owner-a"),
                    Path(("space-a".into(), "number-a".into())),
                )
                .await
                .map(|_| ()),
                _ => super::set_property_values(
                    State(state.clone()),
                    test_auth("owner-a"),
                    Path(("space-a".into(), "target-a".into())),
                    axum::Json(super::SetPropertyValuesRequest {
                        values: vec![value_entry("number-a", "2")],
                    }),
                )
                .await
                .map(|_| ()),
            };
            assert!(matches!(denied, Err(AppError::Unauthorized(_))), "{action}");
            let schema: (String, i64) =
                sqlx::query_as("SELECT name,sort_order FROM properties_schema WHERE id='number-a'")
                    .fetch_one(&state.db)
                    .await
                    .unwrap();
            assert_eq!(schema, ("Count".into(), 0));
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM property_values")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 0);
            let active: i64 = sqlx::query_scalar("SELECT is_active FROM users WHERE id='owner-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(active, 1);
        }
    }
    #[tokio::test]
    async fn batch_locks_global_user_targets_in_sorted_order_before_validation() {
        let state = property_test_state().await;
        for query in [
            "INSERT INTO properties_schema(id,space_id,name,property_type) VALUES ('user-first','space-a','First','user'),('user-last','space-a','Last','user'),('user-middle','space-a','Middle','user')",
            "CREATE TABLE observed_user_locks(sequence INTEGER PRIMARY KEY, user_id TEXT)",
            "CREATE TRIGGER observe_user_lock AFTER UPDATE OF is_active ON users BEGIN INSERT INTO observed_user_locks(user_id) VALUES (NEW.id); END",
        ] { sqlx::query(query).execute(&state.db).await.unwrap(); }
        let mut tx = state.db.begin().await.unwrap();
        super::super::property_validation::validate_assignment_batch(
            &mut tx,
            "space-a",
            super::super::access::principal_from_auth(&test_auth("owner-a")).unwrap(),
            &[
                ("user-first", Some("viewer-a")),
                ("user-last", Some("editor-a")),
                ("user-middle", Some("viewer-a")),
            ],
        )
        .await
        .unwrap();
        let users: Vec<String> =
            sqlx::query_scalar("SELECT user_id FROM observed_user_locks ORDER BY sequence")
                .fetch_all(&mut *tx)
                .await
                .unwrap();
        assert_eq!(&users[..2], &["editor-a".to_owned(), "viewer-a".to_owned()]);
        assert_eq!(users.len(), 5); // Two distinct prelocks, then three already-owned checks.
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn relation_target_grant_is_rechecked_after_target_lock() {
        let state = property_test_state().await;
        for query in [
            "UPDATE documents SET published_at=NULL WHERE id='target-a'",
            "INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES ('draft','space-a','target-a','editor-a','Private','old')",
            "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES ('draft','owner-a','viewer','editor-a')",
            "CREATE TRIGGER revoke_target_grant AFTER UPDATE OF title ON documents BEGIN DELETE FROM document_draft_access WHERE draft_id='draft'; END",
        ] { sqlx::query(query).execute(&state.db).await.unwrap(); }
        let mut tx = state.db.begin().await.unwrap();
        assert!(matches!(
            super::super::property_validation::validate_assignment(
                &mut tx,
                "space-a",
                super::super::access::principal_from_auth(&test_auth("owner-a")).unwrap(),
                "relation-a",
                Some("target-a")
            )
            .await,
            Err(AppError::BadRequest(_))
        ));
        tx.rollback().await.unwrap();
        let grants: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_draft_access")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(grants, 1);
    }
    fn backlink_auth(user: &str) -> AuthUser {
        let mut auth = test_auth(user);
        auth.claims.sid = format!("backlink-session-{user}");
        auth
    }
    async fn scoped_backlink_state() -> AppState {
        let state = property_test_state().await;
        for user in ["owner-a", "viewer-a", "editor-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'test','2099-01-01T00:00:00Z')").bind(format!("backlink-session-{user}")).bind(user).execute(&state.db).await.unwrap();
        }
        state
    }
    async fn backlink_rows(
        state: &AppState,
        auth: AuthUser,
        target: &str,
    ) -> Result<serde_json::Value, AppError> {
        let response = list_backlinks(
            State(state.clone()),
            auth,
            Path(("space-a".into(), target.into())),
        )
        .await?
        .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        Ok(serde_json::from_slice(&bytes).unwrap())
    }
    #[tokio::test]
    async fn backlinks_filter_private_ancestors_lifecycle_and_canonical_private_files() {
        let state = scoped_backlink_state().await;
        let slug = format!("backlinks-{}", Uuid::new_v4());
        for (id, parent, published) in [
            ("visible-source", None, Some("published")),
            ("private-parent", None, None),
            ("hidden-child", Some("private-parent"), Some("published")),
            ("private-source", None, None),
            ("archived-source", None, Some("published")),
            ("trashed-source", None, Some("published")),
        ] {
            let path = crate::storage::write_document(
                &slug,
                id,
                id,
                "",
                "See [[target-a]] /docs/target-a",
            )
            .await
            .unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,parent_id,content_path,created_by,last_updated_by,published_at) VALUES($1,'space-a',$1,$2,$3,'owner-a','owner-a',$4)").bind(id).bind(parent).bind(path).bind(published).execute(&state.db).await.unwrap();
        }
        sqlx::query("UPDATE documents SET archived_at='2099' WHERE id='archived-source'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET deleted_at='2099' WHERE id='trashed-source'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='visible-source'")
            .execute(&state.db)
            .await
            .unwrap();
        let rows = backlink_rows(&state, backlink_auth("viewer-a"), "target-a")
            .await
            .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["document_id"], "visible-source");
        assert!(!rows.to_string().contains("private"));
        assert!(!rows.to_string().contains("hidden"));
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('unshared-source-draft','private-source','space-a','owner-a','Unshared secret','/docs/target-a','base','2099'),('readable-source-draft','private-source','space-a','owner-a','Shared source title','No backlink here','base','2000')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('readable-source-draft','viewer-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            backlink_rows(&state, backlink_auth("viewer-a"), "target-a")
                .await
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        sqlx::query(
            "UPDATE document_drafts SET content='/docs/target-a' WHERE id='readable-source-draft'",
        )
        .execute(&state.db)
        .await
        .unwrap();
        let rows = backlink_rows(&state, backlink_auth("viewer-a"), "target-a")
            .await
            .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 2);
        assert!(rows
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["title"] == "Shared source title"));
        assert!(!rows.to_string().contains("Unshared"));
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn backlinks_private_target_uses_exact_draft_title_not_unshared_canonical_title() {
        let state = scoped_backlink_state().await;
        let slug = format!("backlinks-{}", Uuid::new_v4());
        sqlx::query("UPDATE documents SET published_at=NULL,title='Canonical hidden title' WHERE id='target-a'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at) VALUES('readable-target','target-a','space-a','owner-a','Readable title','Private body','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('readable-target','viewer-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        for (id, content) in [
            ("readable-link", "[[Readable title]]"),
            ("hidden-title-link", "[[Canonical hidden title]]"),
            ("stable-link", "[[Old title|target-a]]"),
        ] {
            let path = crate::storage::write_document(&slug, id, id, "", content)
                .await
                .unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES($1,'space-a',$1,$2,'owner-a','owner-a','published')").bind(id).bind(path).execute(&state.db).await.unwrap();
        }
        let rows = backlink_rows(&state, backlink_auth("viewer-a"), "target-a")
            .await
            .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 2);
        assert!(!rows.to_string().contains("hidden-title-link"));
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn backlinks_recheck_current_session_key_block_and_source_branch_after_waits() {
        let state = scoped_backlink_state().await;
        let mut blocker = state.db.acquire().await.unwrap();
        let worker = state.clone();
        let pending = tokio::spawn(async move {
            backlink_rows(&worker, backlink_auth("viewer-a"), "target-a").await
        });
        tokio::task::yield_now().await;
        sqlx::query("DELETE FROM sessions WHERE user_id='viewer-a'")
            .execute(&mut *blocker)
            .await
            .unwrap();
        drop(blocker);
        assert!(pending.await.unwrap().is_err());
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('backlink-key','editor-a','Read','hash','key','[\"read\"]')").execute(&state.db).await.unwrap();
        let key_auth = || {
            let mut auth = backlink_auth("editor-a");
            auth.claims.sid = "api-key:backlink-key".into();
            auth.api_key_scopes = Some(vec!["read".into()]);
            auth
        };
        assert!(backlink_rows(&state, key_auth(), "target-a").await.is_ok());
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='backlink-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(backlink_rows(&state, key_auth(), "target-a").await.is_err());
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','editor-a','owner-a')").execute(&state.db).await.unwrap();
        assert!(backlink_rows(&state, backlink_auth("editor-a"), "target-a")
            .await
            .is_err());
        sqlx::query("DELETE FROM space_user_blocks")
            .execute(&state.db)
            .await
            .unwrap();
        let source: (Option<String>, String, String) = sqlx::query_as(
            "SELECT published_at,content_path,updated_at FROM documents WHERE id='target-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('own-source','target-a','space-a','editor-a','Own','base')").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        let source = super::BacklinkReadSource {
            document_id: "target-a".into(),
            published_at: source.0,
            content_path: source.1,
            updated_at: source.2,
            draft_id: None,
        };
        assert!(!super::backlink_read_source_current(
            &mut conn,
            &backlink_auth("editor-a"),
            "space-a",
            &source
        )
        .await
        .unwrap());
    }

    #[tokio::test]
    async fn backlinks_selected_draft_revocation_does_not_fall_back_to_other_participation() {
        let state = scoped_backlink_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,content,base_updated_at,updated_at) VALUES('selected','target-a','space-a','owner-a','Selected','/docs/source','base','2099'),('other-readable','target-a','space-a','owner-a','Other','Other body','base','2000')").execute(&state.db).await.unwrap();
        for draft in ["selected", "other-readable"] {
            sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES($1,'viewer-a','viewer','owner-a')").bind(draft).execute(&state.db).await.unwrap();
        }
        let mut conn = state.db.acquire().await.unwrap();
        let draft = super::super::documents::readable_discovery_draft(
            &mut conn,
            &backlink_auth("viewer-a"),
            "space-a",
            "target-a",
        )
        .await
        .unwrap();
        assert_eq!(draft.id, "selected");
        sqlx::query("DELETE FROM document_draft_access WHERE draft_id='selected'")
            .execute(&mut *conn)
            .await
            .unwrap();
        assert!(super::backlink_page_view(
            &mut conn,
            &backlink_auth("viewer-a"),
            "space-a",
            "target-a"
        )
        .await
        .is_ok());
        let source = super::BacklinkReadSource {
            document_id: "target-a".into(),
            published_at: None,
            content_path: String::new(),
            updated_at: String::new(),
            draft_id: Some(draft.id),
        };
        assert!(!super::backlink_read_source_current(
            &mut conn,
            &backlink_auth("viewer-a"),
            "space-a",
            &source
        )
        .await
        .unwrap());
    }


    thread_local! {
        static METADATA_READ_PAUSE: std::cell::RefCell<Option<(tokio::sync::oneshot::Sender<()>,tokio::sync::oneshot::Receiver<()>)>> = const { std::cell::RefCell::new(None) };
    }
    pub(crate) async fn pause_metadata_read() {
        let pause = METADATA_READ_PAUSE.with(|slot| slot.borrow_mut().take());
        if let Some((ready, resume)) = pause {
            let _ = ready.send(());
            resume.await.unwrap();
        }
    }

    #[tokio::test]
    async fn metadata_reads_observe_external_wal_credential_visibility_and_association_changes() {
        for variant in [
            "schema-session",
            "tag-session",
            "doc-tag-session",
            "values-private",
            "tag-association",
        ] {
            let mut state = property_test_state().await;
            let file = std::env::temp_dir()
                .join(format!("metadata-read-wal-{}.sqlite", uuid::Uuid::new_v4()));
            state.db = AnyPoolOptions::new()
                .max_connections(2)
                .after_connect(|conn, _| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA journal_mode=WAL")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query("PRAGMA busy_timeout=5000")
                            .execute(conn)
                            .await?;
                        Ok(())
                    })
                })
                .connect(&format!("sqlite://{}?mode=rwc", file.display()))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for sql in [
                "INSERT INTO users(id,email,display_name,role) VALUES('owner-a','owner@wal.test','Owner','member')",
                "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('metadata-owner-a','owner-a','wal','2999-01-01T00:00:00Z')",
                "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','A','wal','owner-a')",
                "INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner','space-a','owner-a','owner')",
                "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Target','','owner-a','owner-a','old')",
                "INSERT INTO tags(id,space_id,name) VALUES('tag','space-a','Tag')",
                "INSERT INTO doc_tags(document_id,tag_id) VALUES('target-a','tag')",
                "INSERT INTO properties_schema(id,space_id,name,property_type) VALUES('prop','space-a','Property','text')",
            ]{sqlx::query(sql).execute(&state.db).await.unwrap();}
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
            METADATA_READ_PAUSE.with(|slot| *slot.borrow_mut() = Some((ready_tx, resume_rx)));
            let writer = state.db.clone();
            let change = match variant {
                "values-private" => "UPDATE documents SET published_at=NULL WHERE id='target-a'",
                "tag-association" => "DELETE FROM doc_tags",
                _ => "DELETE FROM sessions WHERE id='metadata-owner-a'",
            };
            let committed = tokio::spawn(async move {
                tokio::time::timeout(std::time::Duration::from_secs(5), ready_rx)
                    .await
                    .unwrap()
                    .unwrap();
                sqlx::query(change).execute(&writer).await.unwrap();
                resume_tx.send(()).unwrap();
            });
            let response = match variant {
                "schema-session" => super::list_property_schema(
                    State(state.clone()),
                    test_auth("owner-a"),
                    Path("space-a".into()),
                )
                .await
                .map(IntoResponse::into_response),
                "values-private" => super::list_property_values(
                    State(state.clone()),
                    test_auth("owner-a"),
                    Path(("space-a".into(), "target-a".into())),
                )
                .await
                .map(IntoResponse::into_response),
                "doc-tag-session" => {
                    super::super::tags::test_list_doc_tags(&state, test_auth("owner-a")).await
                }
                _ => super::super::tags::test_list_tags(&state, test_auth("owner-a")).await,
            };
            committed.await.unwrap();
            if variant == "tag-association" {
                let body = axum::body::to_bytes(response.unwrap().into_body(), 10000)
                    .await
                    .unwrap();
                let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
                assert_eq!(json[0]["doc_count"], 0);
            } else {
                assert!(
                    response.is_err(),
                    "{variant} returned stale private metadata"
                );
            }
            state.db.close().await;
            std::fs::remove_file(file).unwrap();
        }
    }

    pub(crate) fn copy_auth(auth: &AuthUser) -> AuthUser {
        AuthUser {
            claims: auth.claims.clone(),
            api_key_scopes: auth.api_key_scopes.clone(),
        }
    }

    pub(crate) async fn metadata_key(state: &AppState, actor: &str, scopes: &str) -> AuthUser {
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,is_service_account,scopes) VALUES('metadata-key',$1,'Test','hash','test',0,$2)").bind(actor).bind(scopes).execute(&state.db).await.unwrap();
        let mut auth = test_auth(actor);
        auth.claims.sid = "api-key:metadata-key".into();
        auth.api_key_scopes = Some(vec!["admin".into()]); // Claimed scope never overrides current key row.
        auth
    }

    #[tokio::test]
    async fn property_current_key_method_ceiling_and_final_revocation_are_atomic() {
        let state = property_test_state().await;
        let auth = metadata_key(&state, "owner-a", r#"["write"]"#).await;
        assert!(super::list_property_schema(
            State(state.clone()),
            copy_auth(&auth),
            Path("space-a".into())
        )
        .await
        .is_err());
        assert!(super::list_property_values(
            State(state.clone()),
            copy_auth(&auth),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
        let set = || super::SetPropertyValuesRequest {
            values: vec![value_entry("number-a", "12")],
        };
        assert!(super::set_property_values(
            State(state.clone()),
            copy_auth(&auth),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(set())
        )
        .await
        .is_ok());
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::list_property_schema(
            State(state.clone()),
            copy_auth(&auth),
            Path("space-a".into())
        )
        .await
        .is_ok());
        assert!(super::list_property_values(
            State(state.clone()),
            copy_auth(&auth),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_ok());
        assert!(super::set_property_values(
            State(state.clone()),
            copy_auth(&auth),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(set())
        )
        .await
        .is_err());
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("CREATE TRIGGER downgrade_metadata_key AFTER UPDATE OF value ON property_values BEGIN UPDATE api_keys SET scopes='[\"read\"]'; END").execute(&state.db).await.unwrap();
        let result = super::set_property_values(
            State(state.clone()),
            copy_auth(&auth),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(super::SetPropertyValuesRequest {
                values: vec![value_entry("number-a", "19")],
            }),
        )
        .await;
        assert!(result.is_err());
        let value: String =
            sqlx::query_scalar("SELECT value FROM property_values WHERE property_id='number-a'")
                .fetch_one(&state.db)
                .await
                .unwrap();
        assert_eq!(value, "12");
        let scopes: String = sqlx::query_scalar("SELECT scopes FROM api_keys")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(scopes, r#"["write"]"#);
    }

    #[tokio::test]
    async fn private_property_write_only_shared_editor_and_current_credentials_fail_closed() {
        let state = property_test_state().await;
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('owned','target-a','space-a','editor-a','Private','base')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('owned','owner-a','editor','editor-a')").execute(&state.db).await.unwrap();
        let auth = metadata_key(&state, "owner-a", r#"["write"]"#).await;
        let set = || super::SetPropertyValuesRequest {
            values: vec![value_entry("number-a", "3")],
        };
        assert!(super::set_property_values(
            State(state.clone()),
            copy_auth(&auth),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(set())
        )
        .await
        .is_ok());
        sqlx::query("UPDATE document_draft_access SET role='viewer'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::set_property_values(
            State(state.clone()),
            copy_auth(&auth),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(set())
        )
        .await
        .is_err());
        for change in [
            "UPDATE api_keys SET scopes='broken'",
            "UPDATE api_keys SET expires_at='2000-01-01T00:00:00Z'",
            "UPDATE api_keys SET user_id='editor-a'",
        ] {
            let mut tx = state.db.begin().await.unwrap();
            sqlx::query(change).execute(&mut *tx).await.unwrap();
            assert!(
                super::require_metadata_space_mutation(&mut tx, &auth, "space-a", false)
                    .await
                    .is_err()
            );
            tx.rollback().await.unwrap();
        }
        let mut expired = copy_auth(&auth);
        expired.claims.exp = 0;
        assert!(
            super::list_property_schema(State(state.clone()), expired, Path("space-a".into()))
                .await
                .is_err()
        );
        let mut service = copy_auth(&auth);
        service.claims.sub = "service:metadata-key".into();
        service.claims.sid = "service:metadata-key".into();
        assert!(super::set_property_values(
            State(state.clone()),
            service,
            Path(("space-a".into(), "target-a".into())),
            axum::Json(set())
        )
        .await
        .is_err());
        let mut tx = state.db.begin().await.unwrap();
        sqlx::query("DELETE FROM sessions WHERE id='metadata-owner-a'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(super::require_metadata_space_mutation(
            &mut tx,
            &test_auth("owner-a"),
            "space-a",
            false
        )
        .await
        .is_err());
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn property_owner_capability_is_exact_and_raw_relation_read_is_preserved() {
        let state = property_test_state().await;
        sqlx::query("INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('manage-only','space-a','Manager','[\"manage_space\"]','owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("UPDATE space_members SET role='manage-only' WHERE user_id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::delete_property_schema(
            State(state.clone()),
            test_auth("editor-a"),
            Path(("space-a".into(), "number-a".into()))
        )
        .await
        .is_ok());
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-b'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('kept','target-a','relation-a','target-b')").execute(&state.db).await.unwrap();
        let response = super::list_property_values(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "target-a".into())),
        )
        .await
        .unwrap()
        .into_response();
        let bytes = axum::body::to_bytes(response.into_body(), 10000)
            .await
            .unwrap();
        let values: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(values
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["property_id"] == "relation-a" && v["value"] == "target-b"));
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::list_property_values(
            State(state.clone()),
            test_auth("owner-a"),
            Path(("space-a".into(), "target-a".into()))
        )
        .await
        .is_err());
    }

    #[tokio::test]
    async fn backlinks_require_an_available_target_in_the_routed_space() {
        let state = scoped_backlink_state().await;

        let same_space = list_backlinks(
            State(state.clone()),
            backlink_auth("owner-a"),
            Path(("space-a".to_string(), "target-a".to_string())),
        )
        .await
        .expect("same-space target is available");
        assert_eq!(
            same_space.into_response().status(),
            axum::http::StatusCode::OK
        );

        for target_id in ["target-b", "missing-target"] {
            let result = list_backlinks(
                State(state.clone()),
                backlink_auth("owner-a"),
                Path(("space-a".to_string(), target_id.to_string())),
            )
            .await;
            assert!(matches!(result, Err(AppError::NotFound(_))));
        }

        sqlx::query(
            "UPDATE documents SET deleted_at = '2026-10-06T00:00:00Z' WHERE id = 'target-a'",
        )
        .execute(&state.db)
        .await
        .expect("mark backlink target deleted");
        let deleted = list_backlinks(
            State(state.clone()),
            backlink_auth("owner-a"),
            Path(("space-a".to_string(), "target-a".to_string())),
        )
        .await;
        assert!(matches!(deleted, Err(AppError::NotFound(_))));

        let denied = list_backlinks(
            State(state),
            backlink_auth("viewer-a"),
            Path(("space-a".to_string(), "target-b".to_string())),
        )
        .await;
        assert!(matches!(denied, Err(AppError::NotFound(_))));
    }

    #[test]
    fn extracts_legacy_title_only_and_stable_id_links() {
        let content = "Refs: [[Roadmap]], [[Design Notes|design]], [[  Spec ]]";
        assert_eq!(
            extract_wiki_links(content),
            vec![
                WikiLinkReference {
                    title: "Roadmap".to_string(),
                    document_id: None,
                },
                WikiLinkReference {
                    title: "Design Notes".to_string(),
                    document_id: Some("design".to_string()),
                },
                WikiLinkReference {
                    title: "Spec".to_string(),
                    document_id: None,
                },
            ]
        );
    }

    #[test]
    fn matches_legacy_title_only_links_case_insensitively() {
        let content = "Refs: [[RoadMap]] and [[Other]]";
        assert!(contains_backlink_reference(
            content,
            " roadmap ",
            "doc-1234"
        ));
        assert!(!contains_backlink_reference(content, "Missing", "doc-1234"));
    }

    #[test]
    fn stable_document_ids_override_titles_for_backlink_matching() {
        let content = "Refs: [[Launch Plan|doc-a]]";
        assert!(contains_backlink_reference(content, "Launch Plan", "doc-a"));
        assert!(!contains_backlink_reference(
            content,
            "Launch Plan",
            "doc-b"
        ));
        assert!(!contains_backlink_reference(
            content,
            "Renamed target",
            "doc-b"
        ));
    }

    #[test]
    fn empty_pipe_target_keeps_legacy_title_matching() {
        assert!(contains_backlink_reference(
            "[[Launch Plan| ]]",
            "launch plan",
            "doc-a"
        ));
    }

    #[test]
    fn detects_doc_id_links() {
        let content = "Open /spaces/a/docs/doc-1234 and also /docs/other_doc";
        assert_eq!(extract_doc_id_links(content), vec!["doc-1234", "other_doc"]);
        assert!(contains_backlink_reference(content, "No Match", "doc-1234"));
    }
    async fn relation_fixture(state: &AppState) {
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('relation-target','space-a','Target','','owner-a','owner-a','published'),('private-parent','space-a','Parent','','owner-a','owner-a',NULL)").execute(&state.db).await.unwrap();
    }
    async fn set_relation(state: &AppState, auth: &AuthUser, target: &str) -> Result<(), AppError> {
        super::set_property_values(
            State(state.clone()),
            copy_auth(auth),
            Path(("space-a".into(), "target-a".into())),
            axum::Json(super::SetPropertyValuesRequest {
                values: vec![
                    value_entry("number-a", "7"),
                    value_entry("relation-a", target),
                ],
            }),
        )
        .await
        .map(|_| ())
    }
    #[tokio::test]
    async fn relation_http_write_only_key_retains_view_account_policy_without_read_ceiling() {
        use axum::{body::Body, http::Request};
        use tower::ServiceExt;
        let state = property_test_state().await;
        relation_fixture(&state).await;
        let raw = "relation-http-key";
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('relation-http','editor-a','HTTP',$1,'prefix','[\"write\"]')").bind(crate::auth::api_keys::hash_api_key(raw)).execute(&state.db).await.unwrap();
        let app = super::router().with_state(state.clone());
        let request = || {
            Request::builder()
                .method("PUT")
                .uri("/spaces/space-a/documents/target-a/properties")
                .header("X-API-Key", raw)
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"values":[{"property_id":"relation-a","value":"relation-target"}]}"#,
                ))
                .unwrap()
        };
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            axum::http::StatusCode::NO_CONTENT
        );
        sqlx::query("UPDATE documents SET published_at=NULL,locked=1,locked_by='owner-a' WHERE id='relation-target'").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES('relation-draft','space-a','relation-target','owner-a','Private','old')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('relation-draft','editor-a','viewer','owner-a')").execute(&state.db).await.unwrap();
        assert_eq!(
            app.clone().oneshot(request()).await.unwrap().status(),
            axum::http::StatusCode::NO_CONTENT
        );
        sqlx::query("UPDATE api_keys SET scopes='[\"read\"]' WHERE id='relation-http'")
            .execute(&state.db)
            .await
            .unwrap();
        assert_eq!(
            app.oneshot(request()).await.unwrap().status(),
            axum::http::StatusCode::FORBIDDEN
        );
    }
    #[tokio::test]
    async fn relation_target_private_ancestry_and_corrupt_shares_never_use_attribution_or_admin() {
        let state = property_test_state().await;
        relation_fixture(&state).await;
        sqlx::query("UPDATE documents SET parent_id='private-parent' WHERE id='relation-target'")
            .execute(&state.db)
            .await
            .unwrap();
        let auth = test_auth("owner-a");
        for admin in [false, true] {
            if admin {
                sqlx::query("UPDATE users SET role='admin' WHERE id='owner-a'")
                    .execute(&state.db)
                    .await
                    .unwrap();
            }
            assert!(matches!(
                set_relation(&state, &auth, "relation-target").await,
                Err(AppError::BadRequest(_))
            ));
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM property_values")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 0);
        }
        sqlx::query("INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES('parent-draft','space-a','private-parent','editor-a','Parent','old')").execute(&state.db).await.unwrap();
        // Corrupt legacy policy fixture only; production constraints remain enabled.
        sqlx::query("PRAGMA ignore_check_constraints=ON")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('parent-draft','owner-a','unknown','editor-a')").execute(&state.db).await.unwrap();
        sqlx::query("PRAGMA ignore_check_constraints=OFF")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(matches!(
            set_relation(&state, &auth, "relation-target").await,
            Err(AppError::BadRequest(_))
        ));
        sqlx::query("UPDATE document_draft_access SET role='viewer'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(set_relation(&state, &auth, "relation-target").await.is_ok());
    }
    #[tokio::test]
    async fn relation_target_lifecycle_foreign_cycle_and_final_share_revocation_roll_back_batch() {
        for change in ["UPDATE documents SET archived_at='now' WHERE id='relation-target'", "UPDATE documents SET deleted_at='now' WHERE id='relation-target'", "UPDATE documents SET space_id='space-b' WHERE id='relation-target'", "UPDATE documents SET parent_id='relation-target' WHERE id='relation-target'", "UPDATE documents SET parent_id='target-b' WHERE id='relation-target'", "UPDATE documents SET parent_id='private-parent' WHERE id='relation-target'; UPDATE documents SET archived_at='now' WHERE id='private-parent'"] {
            let state=property_test_state().await; relation_fixture(&state).await;
            for statement in change.split("; ") { sqlx::query(statement).execute(&state.db).await.unwrap(); }
            assert!(matches!(set_relation(&state,&test_auth("owner-a"),"relation-target").await,Err(AppError::BadRequest(_))));
            let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM property_values").fetch_one(&state.db).await.unwrap();assert_eq!(count,0);
        }
        let state = property_test_state().await;
        relation_fixture(&state).await;
        for q in ["UPDATE documents SET published_at=NULL WHERE id='relation-target'", "INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES('relation-draft','space-a','relation-target','editor-a','Private','old')", "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('relation-draft','owner-a','viewer','editor-a')", "CREATE TRIGGER relation_late_revoke AFTER INSERT ON property_values BEGIN DELETE FROM document_draft_access WHERE draft_id='relation-draft'; END"] {sqlx::query(q).execute(&state.db).await.unwrap();}
        assert!(matches!(
            set_relation(&state, &test_auth("owner-a"), "relation-target").await,
            Err(AppError::BadRequest(_))
        ));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM property_values")
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(count, 0);
        let grants: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM document_draft_access WHERE draft_id='relation-draft'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(grants, 1);
    }

    #[tokio::test]
    async fn relation_wait_observes_externally_committed_wal_target_and_session_revocation() {
        for variant in ["session", "share"] {
            let mut state = property_test_state().await;
            let file =
                std::env::temp_dir().join(format!("relation-wait-{}.sqlite", uuid::Uuid::new_v4()));
            state.db = AnyPoolOptions::new()
                .max_connections(2)
                .after_connect(|conn, _| {
                    Box::pin(async move {
                        sqlx::query("PRAGMA journal_mode=WAL")
                            .execute(&mut *conn)
                            .await?;
                        sqlx::query("PRAGMA busy_timeout=5000")
                            .execute(conn)
                            .await?;
                        Ok(())
                    })
                })
                .connect(&format!("sqlite://{}?mode=rwc", file.display()))
                .await
                .unwrap();
            sqlx::migrate!("./migrations").run(&state.db).await.unwrap();
            for q in ["INSERT INTO users(id,email,display_name,role) VALUES('owner-a','owner@relation.test','Owner','member'),('editor-a','editor@relation.test','Editor','member')", "INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES('metadata-owner-a','owner-a','test','2999-01-01T00:00:00Z')", "INSERT INTO spaces(id,name,slug,created_by) VALUES('space-a','A','relation-wal','owner-a')", "INSERT INTO space_members(id,space_id,user_id,role) VALUES('owner','space-a','owner-a','owner')", "INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at) VALUES('target-a','space-a','Source','','owner-a','owner-a','old'),('relation-target','space-a','Private','','editor-a','editor-a',NULL)", "INSERT INTO properties_schema(id,space_id,name,property_type) VALUES('number-a','space-a','Count','number'),('relation-a','space-a','Related','relation')", "INSERT INTO document_drafts(id,space_id,document_id,owner_id,title,base_updated_at) VALUES('relation-draft','space-a','relation-target','editor-a','Private','old')", "INSERT INTO document_draft_access(draft_id,user_id,role,granted_by) VALUES('relation-draft','owner-a','viewer','editor-a')"] {sqlx::query(q).execute(&state.db).await.unwrap();}
            let mut blocker = state.db.begin().await.unwrap();
            super::super::mutation_guards::lock_spaces_for_mutation(&mut blocker, &["space-a"])
                .await
                .unwrap();
            sqlx::query(if variant == "session" {
                "DELETE FROM sessions WHERE id='metadata-owner-a'"
            } else {
                "DELETE FROM document_draft_access WHERE draft_id='relation-draft'"
            })
            .execute(&mut *blocker)
            .await
            .unwrap();
            let queued_state = state.clone();
            let mut queued = tokio::spawn(async move {
                set_relation(&queued_state, &test_auth("owner-a"), "relation-target").await
            });
            assert!(
                tokio::time::timeout(std::time::Duration::from_millis(30), &mut queued)
                    .await
                    .is_err()
            );
            blocker.commit().await.unwrap();
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(5), queued)
                    .await
                    .unwrap()
                    .unwrap()
                    .is_err()
            );
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM property_values")
                .fetch_one(&state.db)
                .await
                .unwrap();
            assert_eq!(count, 0);
            state.db.close().await;
            std::fs::remove_file(file).unwrap();
        }
    }
}

#[cfg(test)]
mod property_last_write_tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use tower::ServiceExt;

    async fn setup() -> AppState {
        let state = tests::property_test_state().await;
        sqlx::query("INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by,published_at,locked,locked_by) VALUES('reference-page','space-a','Reference','','owner-a','owner-a','published',1,'owner-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO property_values(id,document_id,property_id,value) VALUES('last-number','target-a','number-a','7'),('last-relation','target-a','relation-a','target-a')").execute(&state.db).await.unwrap();
        state
    }
    fn values() -> serde_json::Value {
        serde_json::json!([{"property_id":"number-a","value":"9"},{"property_id":"relation-a","value":"reference-page"}])
    }
    fn manifest() -> serde_json::Value {
        serde_json::json!({
            "schema_version":1,"provider":"real-provider",
            "replay_contract":{"schema_version":1,"operation_order":"queued_at_ascending","idempotency_key":"operation.id","transport":"server-api","server_backends":["sqlite","postgresql"],"required_payload_status":"available","conflict_policy":"server_validates_version_and_permissions","web_canonical_store":"server-api"},
            "operation_count":1,"operations":[{
                "id":"last-relation-write","entity_type":"document_property_values","entity_id":"target-a","action":"set","queued_at":"2026-10-08T00:00:00Z","payload_status":"available",
                "payload":{"document_id":"target-a","space_id":"space-a","values":values()}
            }]
        })
    }
    async fn body(response: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    #[tokio::test]
    async fn property_rest_last_original_jwt_after_relation_proof_rolls_back_full_database() {
        let state = setup().await;
        let mut claims = tests::test_auth("editor-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let session: (String, Option<String>) = sqlx::query_as(
            "SELECT expires_at,last_seen_at FROM sessions WHERE id='metadata-editor-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert!(session.0.starts_with("2999"));
        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        let request = Request::builder()
            .method("PUT")
            .uri("/spaces/space-a/documents/target-a/properties")
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from(
                serde_json::json!({"values":values()}).to_string(),
            ))
            .unwrap();
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hook_reached = reached.clone();
        let response = PROPERTY_VALUES_AFTER_RELATIONS
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    hook_reached.store(true, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await
                }))),
                router().with_state(state.clone()).oneshot(request),
            )
            .await
            .unwrap();
        assert!(
            reached.load(std::sync::atomic::Ordering::SeqCst),
            "real source and final relation policy gate must have completed"
        );
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        // Only standard extractor bookkeeping is normalized; actual session identity,
        // refresh hash, activity, roles, keys and every domain field remain compared.
        sqlx::query(
            "UPDATE sessions SET expires_at=$1,last_seen_at=$2 WHERE id='metadata-editor-a'",
        )
        .bind(&session.0)
        .bind(&session.1)
        .execute(&state.db)
        .await
        .unwrap();
        assert_eq!(
            crate::spaces::documents::tests::move_database_snapshot(&state).await,
            before
        );
    }
    #[tokio::test]
    async fn property_queued_adapter_last_original_jwt_retains_failed_intent_and_rolls_back_values()
    {
        let state = setup().await;
        let mut claims = tests::test_auth("editor-a").claims;
        claims.exp = chrono::Utc::now().timestamp() + 2;
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &claims,
            &jsonwebtoken::EncodingKey::from_secret(state.jwt_secret.as_bytes()),
        )
        .unwrap();
        let session: (String, Option<String>) = sqlx::query_as(
            "SELECT expires_at,last_seen_at FROM sessions WHERE id='metadata-editor-a'",
        )
        .fetch_one(&state.db)
        .await
        .unwrap();
        let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        let incoming = manifest();
        let request = Request::builder()
            .method("POST")
            .uri("/sync/replay")
            .header("Authorization", format!("Bearer {token}"))
            .header("Content-Type", "application/json")
            .body(Body::from(incoming.to_string()))
            .unwrap();
        let reached = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let hook_reached = reached.clone();
        let response = PROPERTY_VALUES_AFTER_RELATIONS
            .scope(
                std::cell::RefCell::new(Some(Box::pin(async move {
                    hook_reached.store(true, std::sync::atomic::Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await
                }))),
                crate::sync::api_router()
                    .with_state(state.clone())
                    .oneshot(request),
            )
            .await
            .unwrap();
        assert!(
            reached.load(std::sync::atomic::Ordering::SeqCst),
            "queued adapter must reach the shared final relation boundary"
        );
        assert_eq!(response.status(), StatusCode::OK);
        let response = body(response).await;
        assert_eq!(response["failed_operation_count"], 1);
        assert_eq!(response["applied_operation_count"], 0);
        assert_eq!(response["results"][0]["status"], "failed");
        let ledger:(String,String,String,String,String)=sqlx::query_as("SELECT status,attempted_by,provider,payload,error FROM sync_replay_operations WHERE operation_id='last-relation-write'").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            (&ledger.0, &ledger.1, &ledger.2),
            (
                &"failed".to_string(),
                &"editor-a".to_string(),
                &"real-provider".to_string()
            )
        );
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&ledger.3).unwrap(),
            incoming["operations"][0]["payload"]
        );
        assert!(ledger.4.contains("Unauthorized"));
        sqlx::query(
            "UPDATE sessions SET expires_at=$1,last_seen_at=$2 WHERE id='metadata-editor-a'",
        )
        .bind(&session.0)
        .bind(&session.1)
        .execute(&state.db)
        .await
        .unwrap();
        let after = crate::spaces::documents::tests::move_database_snapshot(&state).await;
        // Only the precisely verified failed work and its partial manifest history are new.
        assert_eq!(
            after
                .iter()
                .filter(|(table, _)| !matches!(
                    table.as_str(),
                    "sync_replay_operations" | "sync_replay_manifests"
                ))
                .collect::<Vec<_>>(),
            before
                .iter()
                .filter(|(table, _)| !matches!(
                    table.as_str(),
                    "sync_replay_operations" | "sync_replay_manifests"
                ))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sync_replay_operations")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
        let history:(String,String,String,i64,i64,i64)=sqlx::query_as("SELECT provider,attempted_by,status,failed_operation_count,applied_operation_count,recovery_checkpoint_count FROM sync_replay_manifests").fetch_one(&state.db).await.unwrap();
        assert_eq!(
            history,
            (
                "real-provider".into(),
                "editor-a".into(),
                "partial".into(),
                1,
                0,
                0
            )
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM sync_replay_manifests")
                .fetch_one(&state.db)
                .await
                .unwrap(),
            1
        );
    }
    #[tokio::test]
    async fn property_final_write_keeps_actual_write_only_relation_contract() {
        let state = setup().await;
        for (id, scope) in [("last-write-key", "write"), ("last-read-key", "read")] {
            sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES($1,'editor-a',$1,$2,'test',$3)").bind(id).bind(crate::auth::api_keys::hash_api_key(id)).bind(format!("[\"{scope}\"]")).execute(&state.db).await.unwrap();
        }
        for (key, expected) in [
            ("last-read-key", StatusCode::FORBIDDEN),
            ("last-write-key", StatusCode::NO_CONTENT),
        ] {
            let before = crate::spaces::documents::tests::move_database_snapshot(&state).await;
            let request = Request::builder()
                .method("PUT")
                .uri("/spaces/space-a/documents/target-a/properties")
                .header("X-API-Key", key)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    serde_json::json!({"values":values()}).to_string(),
                ))
                .unwrap();
            let response = router()
                .with_state(state.clone())
                .oneshot(request)
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
            if expected == StatusCode::FORBIDDEN {
                sqlx::query("UPDATE api_keys SET last_used_at=NULL WHERE id=$1")
                    .bind(key)
                    .execute(&state.db)
                    .await
                    .unwrap();
                assert_eq!(
                    crate::spaces::documents::tests::move_database_snapshot(&state).await,
                    before
                );
            }
        }
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT value FROM property_values WHERE id='last-relation'"
            )
            .fetch_one(&state.db)
            .await
            .unwrap(),
            "reference-page"
        );
    }
}

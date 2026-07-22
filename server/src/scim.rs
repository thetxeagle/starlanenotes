use axum::{
    extract::{Path, Query, State},
    http::{
        header::{AUTHORIZATION, LOCATION},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Sha256;

use crate::AppState;

const SCIM_SERVICE_PROVIDER_CONFIG_SCHEMA: &str =
    "urn:ietf:params:scim:schemas:core:2.0:ServiceProviderConfig";
const SCIM_LIST_RESPONSE_SCHEMA: &str = "urn:ietf:params:scim:api:messages:2.0:ListResponse";
const SCIM_USER_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
const SCIM_GROUP_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";
const SCIM_RESOURCE_TYPE_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:ResourceType";
const SCIM_SCHEMA_SCHEMA: &str = "urn:ietf:params:scim:schemas:core:2.0:Schema";
const SCIM_BEARER_TOKEN_ENV: &str = "SCIM_BEARER_TOKEN";
const SCIM_MAX_PAGE_SIZE: u32 = 100;

type HmacSha256 = Hmac<Sha256>;
type ScimUserRow = (String, String, String, i32, String, String, Option<String>);
type ScimGroupRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/ServiceProviderConfig", get(service_provider_config))
        .route("/ResourceTypes", get(list_resource_types))
        .route("/ResourceTypes/{resource_type}", get(get_resource_type))
        .route("/Schemas", get(list_schemas))
        .route("/Schemas/{schema_id}", get(get_schema))
        .route("/Users", get(list_users).post(create_user))
        .route(
            "/Users/{user_id}",
            get(get_user)
                .put(replace_user)
                .patch(patch_user)
                .delete(delete_user),
        )
        .route("/Groups", get(list_groups).post(create_group))
        .route(
            "/Groups/{group_id}",
            get(get_group)
                .put(replace_group)
                .patch(patch_group)
                .delete(delete_group),
        )
}

async fn service_provider_config(headers: HeaderMap) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    Ok(Json(service_provider_config_document()))
}

async fn list_resource_types(headers: HeaderMap) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    let resources = vec![
        scim_user_resource_type_document(),
        scim_group_resource_type_document(),
    ];
    Ok(Json(scim_list_response(resources)))
}

async fn get_resource_type(
    headers: HeaderMap,
    Path(resource_type): Path<String>,
) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    if resource_type.eq_ignore_ascii_case("User") {
        Ok(Json(scim_user_resource_type_document()))
    } else if resource_type.eq_ignore_ascii_case("Group") {
        Ok(Json(scim_group_resource_type_document()))
    } else {
        Err(ScimError::NotFound)
    }
}

async fn list_schemas(headers: HeaderMap) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    let resources = vec![scim_user_schema_document(), scim_group_schema_document()];
    Ok(Json(scim_list_response(resources)))
}

async fn get_schema(
    headers: HeaderMap,
    Path(schema_id): Path<String>,
) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    if schema_id == SCIM_USER_SCHEMA {
        Ok(Json(scim_user_schema_document()))
    } else if schema_id == SCIM_GROUP_SCHEMA {
        Ok(Json(scim_group_schema_document()))
    } else {
        Err(ScimError::NotFound)
    }
}

async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScimListQuery>,
) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    let request = ScimListRequest::try_from(query)?;
    let (total_results, users) = match request.filter {
        None => {
            let (total_results,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
                .fetch_one(&state.db)
                .await
                .map_err(|_| ScimError::Internal)?;
            let users: Vec<ScimUserRow> = sqlx::query_as(
                "SELECT id, email, display_name, is_active, created_at, updated_at, external_id \
                 FROM users ORDER BY email LIMIT ?1 OFFSET ?2",
            )
            .bind(i64::from(request.count))
            .bind(i64::from(request.start_index - 1))
            .fetch_all(&state.db)
            .await
            .map_err(|_| ScimError::Internal)?;
            (total_results, users)
        }
        Some(ScimUserFilter::UserNameEq(email)) => {
            let (total_results,): (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM users WHERE LOWER(email) = LOWER(?1)")
                    .bind(&email)
                    .fetch_one(&state.db)
                    .await
                    .map_err(|_| ScimError::Internal)?;
            let users: Vec<ScimUserRow> = sqlx::query_as(
                "SELECT id, email, display_name, is_active, created_at, updated_at, external_id \
                 FROM users WHERE LOWER(email) = LOWER(?1) ORDER BY email LIMIT ?2 OFFSET ?3",
            )
            .bind(email)
            .bind(i64::from(request.count))
            .bind(i64::from(request.start_index - 1))
            .fetch_all(&state.db)
            .await
            .map_err(|_| ScimError::Internal)?;
            (total_results, users)
        }
        Some(ScimUserFilter::ExternalIdEq(external_id)) => {
            let (total_results,): (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM users WHERE external_id = ?1")
                    .bind(&external_id)
                    .fetch_one(&state.db)
                    .await
                    .map_err(|_| ScimError::Internal)?;
            let users: Vec<ScimUserRow> = sqlx::query_as(
                "SELECT id, email, display_name, is_active, created_at, updated_at, external_id \
                 FROM users WHERE external_id = ?1 ORDER BY email LIMIT ?2 OFFSET ?3",
            )
            .bind(external_id)
            .bind(i64::from(request.count))
            .bind(i64::from(request.start_index - 1))
            .fetch_all(&state.db)
            .await
            .map_err(|_| ScimError::Internal)?;
            (total_results, users)
        }
    };
    let resources = users
        .into_iter()
        .map(
            |(id, email, display_name, is_active, created_at, updated_at, external_id)| {
                scim_user_document(
                    &id,
                    &email,
                    &display_name,
                    is_active != 0,
                    &created_at,
                    &updated_at,
                    external_id.as_deref(),
                )
            },
        )
        .collect::<Vec<_>>();
    Ok(Json(json!({
        "schemas": [SCIM_LIST_RESPONSE_SCHEMA],
        "totalResults": total_results,
        "startIndex": request.start_index,
        "itemsPerPage": resources.len(),
        "Resources": resources,
    })))
}

async fn list_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScimListQuery>,
) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    let request = ScimListRequest::try_from(query)?;
    if request.filter.is_some() {
        return Err(ScimError::InvalidFilter);
    }
    let (total_results,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM groups")
        .fetch_one(&state.db)
        .await
        .map_err(|_| ScimError::Internal)?;
    let groups: Vec<ScimGroupRow> = sqlx::query_as(
        "SELECT id, name, description, created_at, updated_at, external_id FROM groups ORDER BY name LIMIT ?1 OFFSET ?2",
    )
    .bind(i64::from(request.count))
    .bind(i64::from(request.start_index - 1))
    .fetch_all(&state.db)
    .await
    .map_err(|_| ScimError::Internal)?;
    let mut resources = Vec::with_capacity(groups.len());
    for (id, name, description, created_at, updated_at, external_id) in groups {
        resources.push(
            scim_group_document(
                &state,
                &id,
                &name,
                description.as_deref(),
                &created_at,
                &updated_at,
                external_id.as_deref(),
            )
            .await?,
        );
    }
    Ok(Json(json!({
        "schemas": [SCIM_LIST_RESPONSE_SCHEMA],
        "totalResults": total_results,
        "startIndex": request.start_index,
        "itemsPerPage": resources.len(),
        "Resources": resources,
    })))
}

async fn get_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    let group: Option<ScimGroupRow> = sqlx::query_as(
        "SELECT id, name, description, created_at, updated_at, external_id FROM groups WHERE id = ?1",
    )
    .bind(&group_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ScimError::Internal)?;
    let (id, name, description, created_at, updated_at, external_id) =
        group.ok_or(ScimError::NotFound)?;
    Ok(Json(
        scim_group_document(
            &state,
            &id,
            &name,
            description.as_deref(),
            &created_at,
            &updated_at,
            external_id.as_deref(),
        )
        .await?,
    ))
}

async fn patch_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(request): Json<ScimPatchRequest>,
) -> Result<StatusCode, ScimError> {
    require_scim_bearer_token(&headers)?;
    let member_ids = parse_group_membership_replace_patch(request)?;
    let mut transaction = state.db.begin().await.map_err(|_| ScimError::Internal)?;
    let group_exists: Option<(i64,)> =
        sqlx::query_as("SELECT 1 FROM groups WHERE id = ?1 AND scim_managed = 1 LIMIT 1")
            .bind(&group_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| ScimError::Internal)?;
    if group_exists.is_none() {
        return Err(ScimError::NotFound);
    }
    for member_id in &member_ids {
        let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM users WHERE id = ?1 LIMIT 1")
            .bind(member_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| ScimError::Internal)?;
        if exists.is_none() {
            return Err(ScimError::InvalidResource);
        }
    }

    sqlx::query("DELETE FROM group_members WHERE group_id = ?1")
        .bind(&group_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ScimError::Internal)?;
    let now = chrono::Utc::now().to_rfc3339();
    for member_id in &member_ids {
        sqlx::query(
            "INSERT INTO group_members (id, group_id, user_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&group_id)
        .bind(member_id)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ScimError::Internal)?;
    }
    sqlx::query("UPDATE groups SET updated_at = ?1 WHERE id = ?2")
        .bind(&now)
        .bind(&group_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ScimError::Internal)?;
    transaction
        .commit()
        .await
        .map_err(|_| ScimError::Internal)?;

    crate::audit::log_action(
        &state.db,
        None,
        None,
        "group.scim_membership_replace",
        Some("group"),
        Some(&group_id),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn replace_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
    Json(request): Json<ScimGroupCreateRequest>,
) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    let name = request.display_name.trim().to_owned();
    let description = request
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let member_ids = normalized_scim_group_member_ids(request.members)?;
    let supplied_external_id = request
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if name.is_empty()
        || name.len() > 100
        || description
            .as_ref()
            .is_some_and(|value| value.len() > 1_000)
    {
        return Err(ScimError::InvalidResource);
    }

    let mut transaction = state.db.begin().await.map_err(|_| ScimError::Internal)?;
    let existing: Option<(Option<String>, String)> = sqlx::query_as(
        "SELECT external_id, created_at FROM groups WHERE id = ?1 AND scim_managed = 1",
    )
    .bind(&group_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ScimError::Internal)?;
    let (external_id, created_at) = existing.ok_or(ScimError::NotFound)?;
    if supplied_external_id != external_id.as_deref() {
        return Err(ScimError::InvalidResource);
    }
    validate_scim_group_members(&mut transaction, &member_ids).await?;

    let duplicate: Option<(String,)> =
        sqlx::query_as("SELECT id FROM groups WHERE name = ?1 AND id != ?2 LIMIT 1")
            .bind(&name)
            .bind(&group_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| ScimError::Internal)?;
    if duplicate.is_some() {
        return Err(ScimError::Conflict);
    }

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE groups SET name = ?1, description = ?2, updated_at = ?3 WHERE id = ?4")
        .bind(&name)
        .bind(&description)
        .bind(&now)
        .bind(&group_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| {
            if is_unique_constraint_error(&error) {
                ScimError::Conflict
            } else {
                ScimError::Internal
            }
        })?;
    replace_scim_group_members(&mut transaction, &group_id, &member_ids, &now).await?;
    transaction
        .commit()
        .await
        .map_err(|_| ScimError::Internal)?;

    crate::audit::log_action(
        &state.db,
        None,
        None,
        "group.scim_replace",
        Some("group"),
        Some(&group_id),
    )
    .await;
    Ok(Json(
        scim_group_document(
            &state,
            &group_id,
            &name,
            description.as_deref(),
            &created_at,
            &now,
            external_id.as_deref(),
        )
        .await?,
    ))
}

async fn delete_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Result<StatusCode, ScimError> {
    require_scim_bearer_token(&headers)?;
    let result = sqlx::query("DELETE FROM groups WHERE id = ?1 AND scim_managed = 1")
        .bind(&group_id)
        .execute(&state.db)
        .await
        .map_err(|_| ScimError::Internal)?;
    if result.rows_affected() == 0 {
        return Err(ScimError::NotFound);
    }
    crate::audit::log_action(
        &state.db,
        None,
        None,
        "group.scim_delete",
        Some("group"),
        Some(&group_id),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

async fn validate_scim_group_members(
    transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    member_ids: &[String],
) -> Result<(), ScimError> {
    for member_id in member_ids {
        let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM users WHERE id = ?1 LIMIT 1")
            .bind(member_id)
            .fetch_optional(&mut **transaction)
            .await
            .map_err(|_| ScimError::Internal)?;
        if exists.is_none() {
            return Err(ScimError::InvalidResource);
        }
    }
    Ok(())
}

async fn replace_scim_group_members(
    transaction: &mut sqlx::Transaction<'_, sqlx::Any>,
    group_id: &str,
    member_ids: &[String],
    now: &str,
) -> Result<(), ScimError> {
    sqlx::query("DELETE FROM group_members WHERE group_id = ?1")
        .bind(group_id)
        .execute(&mut **transaction)
        .await
        .map_err(|_| ScimError::Internal)?;
    for member_id in member_ids {
        sqlx::query(
            "INSERT INTO group_members (id, group_id, user_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(group_id)
        .bind(member_id)
        .bind(now)
        .execute(&mut **transaction)
        .await
        .map_err(|_| ScimError::Internal)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScimGroupCreateRequest {
    display_name: String,
    description: Option<String>,
    members: Option<Vec<ScimGroupMemberRequest>>,
    external_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScimGroupMemberRequest {
    value: String,
}

async fn create_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ScimGroupCreateRequest>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), ScimError> {
    require_scim_bearer_token(&headers)?;
    let name = request.display_name.trim();
    let description = request
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let member_ids = normalized_scim_group_member_ids(request.members)?;
    let external_id = request
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    if name.is_empty()
        || name.len() > 100
        || description
            .as_ref()
            .is_some_and(|value| value.len() > 1_000)
        || external_id.as_ref().is_some_and(|value| value.len() > 255)
    {
        return Err(ScimError::InvalidResource);
    }

    let mut transaction = state.db.begin().await.map_err(|_| ScimError::Internal)?;
    for member_id in &member_ids {
        let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM users WHERE id = ?1 LIMIT 1")
            .bind(member_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| ScimError::Internal)?;
        if exists.is_none() {
            return Err(ScimError::InvalidResource);
        }
    }

    let group_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let created = sqlx::query(
        "INSERT INTO groups (id, name, description, external_id, scim_managed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
    )
    .bind(&group_id)
    .bind(name)
    .bind(&description)
    .bind(&external_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await;
    if let Err(error) = created {
        return if is_unique_constraint_error(&error) {
            Err(ScimError::Conflict)
        } else {
            Err(ScimError::Internal)
        };
    }

    for member_id in &member_ids {
        sqlx::query(
            "INSERT INTO group_members (id, group_id, user_id, created_at) VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(&group_id)
        .bind(member_id)
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ScimError::Internal)?;
    }
    transaction
        .commit()
        .await
        .map_err(|_| ScimError::Internal)?;

    crate::audit::log_action(
        &state.db,
        None,
        None,
        "group.scim_create",
        Some("group"),
        Some(&group_id),
    )
    .await;

    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        LOCATION,
        HeaderValue::from_str(&format!("/scim/v2/Groups/{group_id}"))
            .expect("SCIM group UUID path"),
    );
    Ok((
        StatusCode::CREATED,
        response_headers,
        Json(
            scim_group_document(
                &state,
                &group_id,
                name,
                description.as_deref(),
                &now,
                &now,
                external_id.as_deref(),
            )
            .await?,
        ),
    ))
}

fn normalized_scim_group_member_ids(
    members: Option<Vec<ScimGroupMemberRequest>>,
) -> Result<Vec<String>, ScimError> {
    let mut member_ids = Vec::new();
    for member in members.unwrap_or_default() {
        let id = member.value.trim();
        if id.is_empty() || id.len() > 128 || member_ids.iter().any(|existing| existing == id) {
            return Err(ScimError::InvalidResource);
        }
        member_ids.push(id.to_owned());
    }
    Ok(member_ids)
}

fn parse_group_membership_replace_patch(
    request: ScimPatchRequest,
) -> Result<Vec<String>, ScimError> {
    if !request
        .schemas
        .iter()
        .any(|schema| schema == "urn:ietf:params:scim:api:messages:2.0:PatchOp")
        || request.operations.len() != 1
    {
        return Err(ScimError::InvalidResource);
    }
    let operation = request
        .operations
        .into_iter()
        .next()
        .expect("one operation checked");
    if !operation.op.eq_ignore_ascii_case("replace")
        || !operation
            .path
            .as_deref()
            .is_some_and(|path| path.eq_ignore_ascii_case("members"))
    {
        return Err(ScimError::InvalidResource);
    }
    let members = operation
        .value
        .as_array()
        .ok_or(ScimError::InvalidResource)?
        .iter()
        .map(|member| {
            member
                .get("value")
                .and_then(Value::as_str)
                .map(|value| ScimGroupMemberRequest {
                    value: value.to_owned(),
                })
                .ok_or(ScimError::InvalidResource)
        })
        .collect::<Result<Vec<_>, _>>()?;
    normalized_scim_group_member_ids(Some(members))
}

fn is_unique_constraint_error(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|database_error| database_error.is_unique_violation())
}

async fn scim_group_document(
    state: &AppState,
    id: &str,
    name: &str,
    description: Option<&str>,
    created_at: &str,
    updated_at: &str,
    external_id: Option<&str>,
) -> Result<Value, ScimError> {
    let members: Vec<(String, String)> = sqlx::query_as(
        "SELECT u.id, u.display_name FROM group_members gm JOIN users u ON u.id = gm.user_id WHERE gm.group_id = ?1 ORDER BY u.display_name",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| ScimError::Internal)?;
    Ok(json!({
        "schemas": [SCIM_GROUP_SCHEMA],
        "id": id,
        "displayName": name,
        "description": description,
        "externalId": external_id,
        "members": members.into_iter().map(|(value, display)| json!({ "value": value, "display": display })).collect::<Vec<_>>(),
        "meta": { "resourceType": "Group", "created": created_at, "lastModified": updated_at },
    }))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScimUserCreateRequest {
    user_name: String,
    display_name: Option<String>,
    active: Option<bool>,
    external_id: Option<String>,
}

type ScimUserReplaceRequest = ScimUserCreateRequest;

async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ScimUserCreateRequest>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), ScimError> {
    require_scim_bearer_token(&headers)?;
    let email = request.user_name.trim().to_ascii_lowercase();
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&email)
        .to_owned();
    let external_id = request
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);

    if !is_valid_scim_email(&email)
        || display_name.len() > 100
        || external_id.as_ref().is_some_and(|id| id.len() > 255)
    {
        return Err(ScimError::InvalidResource);
    }

    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT id FROM users WHERE LOWER(email) = LOWER(?1) OR (?2 IS NOT NULL AND external_id = ?2)",
    )
    .bind(&email)
    .bind(&external_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ScimError::Internal)?;
    if existing.is_some() {
        return Err(ScimError::Conflict);
    }

    let user_id = uuid::Uuid::new_v4().to_string();
    let active = request.active.unwrap_or(true);
    sqlx::query(
        "INSERT INTO users (id, email, display_name, role, is_active, external_id) \
         VALUES (?1, ?2, ?3, 'member', ?4, ?5)",
    )
    .bind(&user_id)
    .bind(&email)
    .bind(&display_name)
    .bind(if active { 1 } else { 0 })
    .bind(&external_id)
    .execute(&state.db)
    .await
    .map_err(|_| ScimError::Internal)?;

    crate::auth::routes::create_personal_space(&state, &user_id, &display_name)
        .await
        .map_err(|_| ScimError::Internal)?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        LOCATION,
        HeaderValue::from_str(&format!("/scim/v2/Users/{user_id}")).expect("SCIM user UUID path"),
    );
    Ok((
        StatusCode::CREATED,
        response_headers,
        Json(scim_user_document(
            &user_id,
            &email,
            &display_name,
            active,
            &now,
            &now,
            external_id.as_deref(),
        )),
    ))
}

fn is_valid_scim_email(email: &str) -> bool {
    email.len() <= 254
        && email.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty()
                && domain.contains('.')
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        })
}

fn scim_list_response(resources: Vec<Value>) -> Value {
    json!({
        "schemas": [SCIM_LIST_RESPONSE_SCHEMA],
        "totalResults": resources.len(),
        "startIndex": 1,
        "itemsPerPage": resources.len(),
        "Resources": resources,
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScimListQuery {
    filter: Option<String>,
    start_index: Option<u32>,
    count: Option<u32>,
}

struct ScimListRequest {
    filter: Option<ScimUserFilter>,
    start_index: u32,
    count: u32,
}

enum ScimUserFilter {
    UserNameEq(String),
    ExternalIdEq(String),
}

impl TryFrom<ScimListQuery> for ScimListRequest {
    type Error = ScimError;

    fn try_from(query: ScimListQuery) -> Result<Self, Self::Error> {
        let start_index = query.start_index.unwrap_or(1);
        if start_index == 0 {
            return Err(ScimError::InvalidFilter);
        }
        let count = query.count.unwrap_or(SCIM_MAX_PAGE_SIZE);
        if count > SCIM_MAX_PAGE_SIZE {
            return Err(ScimError::InvalidFilter);
        }
        let filter = query.filter.map(parse_scim_user_filter).transpose()?;
        Ok(Self {
            filter,
            start_index,
            count,
        })
    }
}

fn parse_scim_user_filter(filter: String) -> Result<ScimUserFilter, ScimError> {
    let Some((attribute, value)) = filter.trim().split_once(char::is_whitespace) else {
        return Err(ScimError::InvalidFilter);
    };
    if !attribute.eq_ignore_ascii_case("userName") && !attribute.eq_ignore_ascii_case("externalId")
    {
        return Err(ScimError::InvalidFilter);
    }
    let value = value.trim();
    let Some(value) = value.strip_prefix("eq").map(str::trim) else {
        return Err(ScimError::InvalidFilter);
    };
    let Some(value) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(ScimError::InvalidFilter);
    };
    if value.is_empty() || value.contains('"') {
        return Err(ScimError::InvalidFilter);
    }
    if attribute.eq_ignore_ascii_case("userName") {
        Ok(ScimUserFilter::UserNameEq(value.to_owned()))
    } else {
        Ok(ScimUserFilter::ExternalIdEq(value.to_owned()))
    }
}

async fn get_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    let user: Option<ScimUserRow> = sqlx::query_as(
        "SELECT id, email, display_name, is_active, created_at, updated_at, external_id FROM users WHERE id = ?1",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ScimError::Internal)?;

    let (id, email, display_name, is_active, created_at, updated_at, external_id) =
        user.ok_or(ScimError::NotFound)?;
    Ok(Json(scim_user_document(
        &id,
        &email,
        &display_name,
        is_active != 0,
        &created_at,
        &updated_at,
        external_id.as_deref(),
    )))
}

async fn replace_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(request): Json<ScimUserReplaceRequest>,
) -> Result<Json<Value>, ScimError> {
    require_scim_bearer_token(&headers)?;
    let email = request.user_name.trim().to_ascii_lowercase();
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&email)
        .to_owned();
    let supplied_external_id = request
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if !is_valid_scim_email(&email) || display_name.len() > 100 {
        return Err(ScimError::InvalidResource);
    }

    let existing: Option<(String, String)> = sqlx::query_as(
        "SELECT external_id, created_at FROM users WHERE id = ?1 AND external_id IS NOT NULL",
    )
    .bind(&user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| ScimError::Internal)?;
    let (external_id, created_at) = existing.ok_or(ScimError::NotFound)?;
    if supplied_external_id.is_some_and(|value| value != external_id) {
        return Err(ScimError::InvalidResource);
    }

    let conflict: Option<(String,)> =
        sqlx::query_as("SELECT id FROM users WHERE LOWER(email) = LOWER(?1) AND id != ?2 LIMIT 1")
            .bind(&email)
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| ScimError::Internal)?;
    if conflict.is_some() {
        return Err(ScimError::Conflict);
    }

    let active = request.active.unwrap_or(true);
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE users SET email = ?1, display_name = ?2, is_active = ?3, updated_at = ?4 WHERE id = ?5",
    )
    .bind(&email)
    .bind(&display_name)
    .bind(if active { 1 } else { 0 })
    .bind(&now)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|error| {
        if is_unique_constraint_error(&error) {
            ScimError::Conflict
        } else {
            ScimError::Internal
        }
    })?;

    crate::audit::log_action(
        &state.db,
        None,
        None,
        "user.scim_replace",
        Some("user"),
        Some(&user_id),
    )
    .await;

    Ok(Json(scim_user_document(
        &user_id,
        &email,
        &display_name,
        active,
        &created_at,
        &now,
        Some(&external_id),
    )))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScimPatchRequest {
    schemas: Vec<String>,
    operations: Vec<ScimPatchOperation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScimPatchOperation {
    op: String,
    path: Option<String>,
    value: Value,
}

async fn patch_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
    Json(request): Json<ScimPatchRequest>,
) -> Result<StatusCode, ScimError> {
    require_scim_bearer_token(&headers)?;
    let change = parse_user_replace_patch(request)?;
    let now = chrono::Utc::now().to_rfc3339();
    let audit_action = match &change {
        ScimUserPatch::Active(_) => "user.scim_lifecycle_update",
        ScimUserPatch::DisplayName(_) | ScimUserPatch::UserName(_) => "user.scim_profile_update",
    };
    let result = match change {
        ScimUserPatch::Active(active) => sqlx::query(
            "UPDATE users SET is_active = ?1, updated_at = ?2 \
             WHERE id = ?3 AND external_id IS NOT NULL",
        )
        .bind(if active { 1 } else { 0 })
        .bind(&now)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(|_| ScimError::Internal)?,
        ScimUserPatch::DisplayName(display_name) => sqlx::query(
            "UPDATE users SET display_name = ?1, updated_at = ?2 \
             WHERE id = ?3 AND external_id IS NOT NULL",
        )
        .bind(&display_name)
        .bind(&now)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(|_| ScimError::Internal)?,
        ScimUserPatch::UserName(email) => sqlx::query(
            "UPDATE users SET email = ?1, updated_at = ?2 \
             WHERE id = ?3 AND external_id IS NOT NULL",
        )
        .bind(&email)
        .bind(&now)
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(|error| {
            if is_unique_constraint_error(&error) {
                ScimError::Conflict
            } else {
                ScimError::Internal
            }
        })?,
    };
    if result.rows_affected() == 0 {
        return Err(ScimError::NotFound);
    }

    crate::audit::log_action(
        &state.db,
        None,
        None,
        audit_action,
        Some("user"),
        Some(&user_id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<StatusCode, ScimError> {
    require_scim_bearer_token(&headers)?;
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE users SET is_active = 0, updated_at = ?1 WHERE id = ?2 AND external_id IS NOT NULL",
    )
    .bind(&now)
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|_| ScimError::Internal)?;
    if result.rows_affected() == 0 {
        return Err(ScimError::NotFound);
    }
    crate::audit::log_action(
        &state.db,
        None,
        None,
        "user.scim_deprovision",
        Some("user"),
        Some(&user_id),
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

enum ScimUserPatch {
    Active(bool),
    DisplayName(String),
    UserName(String),
}

fn parse_user_replace_patch(request: ScimPatchRequest) -> Result<ScimUserPatch, ScimError> {
    if !request
        .schemas
        .iter()
        .any(|schema| schema == "urn:ietf:params:scim:api:messages:2.0:PatchOp")
        || request.operations.len() != 1
    {
        return Err(ScimError::InvalidResource);
    }
    let operation = request
        .operations
        .into_iter()
        .next()
        .expect("one operation checked");
    if !operation.op.eq_ignore_ascii_case("replace") {
        return Err(ScimError::InvalidResource);
    }
    let Some(path) = operation.path.as_deref() else {
        return Err(ScimError::InvalidResource);
    };
    if path.eq_ignore_ascii_case("active") {
        return operation
            .value
            .as_bool()
            .map(ScimUserPatch::Active)
            .ok_or(ScimError::InvalidResource);
    }
    if path.eq_ignore_ascii_case("displayName") {
        let display_name = operation
            .value
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty() && value.len() <= 100)
            .map(str::to_owned)
            .ok_or(ScimError::InvalidResource)?;
        return Ok(ScimUserPatch::DisplayName(display_name));
    }
    if path.eq_ignore_ascii_case("userName") {
        let email = operation
            .value
            .as_str()
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .filter(|value| is_valid_scim_email(value))
            .ok_or(ScimError::InvalidResource)?;
        return Ok(ScimUserPatch::UserName(email));
    }
    Err(ScimError::InvalidResource)
}

fn scim_user_document(
    id: &str,
    email: &str,
    display_name: &str,
    active: bool,
    created_at: &str,
    updated_at: &str,
    external_id: Option<&str>,
) -> Value {
    json!({
        "schemas": [SCIM_USER_SCHEMA],
        "id": id,
        "userName": email,
        "displayName": display_name,
        "active": active,
        "externalId": external_id,
        "emails": [{ "value": email, "primary": true }],
        "meta": { "resourceType": "User", "created": created_at, "lastModified": updated_at },
    })
}

fn scim_user_resource_type_document() -> Value {
    json!({
        "schemas": [SCIM_RESOURCE_TYPE_SCHEMA],
        "id": "User",
        "name": "User",
        "endpoint": "/Users",
        "description": "Starlane Notes user directory entries provisioned through the SCIM User endpoint.",
        "schema": SCIM_USER_SCHEMA,
        "schemaExtensions": [],
    })
}

fn scim_group_resource_type_document() -> Value {
    json!({
        "schemas": [SCIM_RESOURCE_TYPE_SCHEMA],
        "id": "Group",
        "name": "Group",
        "endpoint": "/Groups",
        "description": "Starlane Notes groups provisioned and lifecycle-managed through SCIM.",
        "schema": SCIM_GROUP_SCHEMA,
        "schemaExtensions": [],
    })
}

fn scim_user_schema_document() -> Value {
    json!({
        "schemas": [SCIM_SCHEMA_SCHEMA],
        "id": SCIM_USER_SCHEMA,
        "name": "User",
        "description": "Core user attributes provisioned by Starlane Notes through SCIM User creation.",
        "attributes": [
            { "name": "userName", "type": "string", "multiValued": false, "required": true, "caseExact": false, "mutability": "immutable", "returned": "always", "uniqueness": "server" },
            { "name": "externalId", "type": "string", "multiValued": false, "required": false, "caseExact": true, "mutability": "immutable", "returned": "default", "uniqueness": "server" },
            { "name": "displayName", "type": "string", "multiValued": false, "required": false, "caseExact": false, "mutability": "readWrite", "returned": "default", "uniqueness": "none" },
            { "name": "active", "type": "boolean", "multiValued": false, "required": false, "mutability": "readWrite", "returned": "default", "uniqueness": "none" },
            { "name": "emails", "type": "complex", "multiValued": true, "required": false, "mutability": "readOnly", "returned": "default", "uniqueness": "none", "subAttributes": [
                { "name": "value", "type": "string", "multiValued": false, "required": true, "caseExact": false, "mutability": "readOnly", "returned": "always", "uniqueness": "none" },
                { "name": "primary", "type": "boolean", "multiValued": false, "required": false, "mutability": "readOnly", "returned": "default", "uniqueness": "none" }
            ] }
        ],
    })
}

fn scim_group_schema_document() -> Value {
    json!({
        "schemas": [SCIM_SCHEMA_SCHEMA],
        "id": SCIM_GROUP_SCHEMA,
        "name": "Group",
        "description": "Group attributes provisioned and lifecycle-managed by Starlane Notes through SCIM.",
        "attributes": [
            { "name": "displayName", "type": "string", "multiValued": false, "required": true, "caseExact": false, "mutability": "readWrite", "returned": "always", "uniqueness": "server" },
            { "name": "description", "type": "string", "multiValued": false, "required": false, "caseExact": false, "mutability": "readWrite", "returned": "default", "uniqueness": "none" },
            { "name": "externalId", "type": "string", "multiValued": false, "required": false, "caseExact": true, "mutability": "immutable", "returned": "default", "uniqueness": "server" },
            { "name": "members", "type": "complex", "multiValued": true, "required": false, "mutability": "readWrite", "returned": "default", "uniqueness": "none", "subAttributes": [
                { "name": "value", "type": "string", "multiValued": false, "required": true, "caseExact": true, "mutability": "readWrite", "returned": "always", "uniqueness": "none" },
                { "name": "display", "type": "string", "multiValued": false, "required": false, "caseExact": false, "mutability": "readOnly", "returned": "default", "uniqueness": "none" }
            ] }
        ],
    })
}

fn require_scim_bearer_token(headers: &HeaderMap) -> Result<(), ScimError> {
    let configured_token = std::env::var(SCIM_BEARER_TOKEN_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ScimError::NotConfigured)?;
    let supplied_token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|value| !value.is_empty())
        .ok_or(ScimError::Unauthorized)?;

    if scim_tokens_match(&configured_token, supplied_token) {
        Ok(())
    } else {
        Err(ScimError::Unauthorized)
    }
}

fn scim_tokens_match(expected: &str, supplied: &str) -> bool {
    let mut expected_mac = HmacSha256::new_from_slice(expected.as_bytes()).expect("HMAC key");
    expected_mac.update(b"starlane-notes-scim-token");
    let expected_tag = expected_mac.finalize().into_bytes();

    let mut supplied_mac = HmacSha256::new_from_slice(supplied.as_bytes()).expect("HMAC key");
    supplied_mac.update(b"starlane-notes-scim-token");
    supplied_mac.verify_slice(&expected_tag).is_ok()
}

fn service_provider_config_document() -> Value {
    json!({
        "schemas": [SCIM_SERVICE_PROVIDER_CONFIG_SCHEMA],
        "patch": { "supported": true },
        "bulk": { "supported": false, "maxOperations": 0, "maxPayloadSize": 0 },
        "filter": { "supported": true, "maxResults": SCIM_MAX_PAGE_SIZE },
        "changePassword": { "supported": false },
        "sort": { "supported": false },
        "etag": { "supported": false },
        "authenticationSchemes": [{
            "type": "oauthbearertoken",
            "name": "Bearer Authentication",
            "description": "Use the configured SCIM bearer token.",
            "primary": true
        }]
    })
}

#[derive(Debug)]
enum ScimError {
    NotConfigured,
    Unauthorized,
    NotFound,
    Conflict,
    InvalidFilter,
    InvalidResource,
    Internal,
}

impl IntoResponse for ScimError {
    fn into_response(self) -> Response {
        let (status, detail) = match self {
            Self::NotConfigured => (StatusCode::NOT_FOUND, "SCIM is not configured"),
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "Invalid SCIM bearer token"),
            Self::NotFound => (StatusCode::NOT_FOUND, "SCIM resource not found"),
            Self::Conflict => (
                StatusCode::CONFLICT,
                "A SCIM user with that identity already exists",
            ),
            Self::InvalidFilter => (
                StatusCode::BAD_REQUEST,
                "Unsupported SCIM filter or pagination",
            ),
            Self::InvalidResource => (StatusCode::BAD_REQUEST, "Invalid SCIM User resource"),
            Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "SCIM request failed"),
        };
        (
            status,
            Json(json!({
                "schemas": ["urn:ietf:params:scim:api:messages:2.0:Error"],
                "status": status.as_u16().to_string(),
                "detail": detail,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_provider_config_advertises_only_available_capabilities() {
        let config = service_provider_config_document();
        assert_eq!(config["schemas"][0], SCIM_SERVICE_PROVIDER_CONFIG_SCHEMA);
        assert_eq!(config["patch"]["supported"], true);
        assert_eq!(config["filter"]["supported"], true);
        assert_eq!(config["filter"]["maxResults"], SCIM_MAX_PAGE_SIZE);
        assert_eq!(
            config["authenticationSchemes"][0]["type"],
            "oauthbearertoken"
        );
    }

    #[test]
    fn discovery_documents_advertise_the_implemented_user_and_group_contracts() {
        let resource_types = scim_list_response(vec![
            scim_user_resource_type_document(),
            scim_group_resource_type_document(),
        ]);
        assert_eq!(resource_types["totalResults"], 2);
        assert_eq!(resource_types["Resources"][0]["endpoint"], "/Users");
        assert_eq!(resource_types["Resources"][0]["schema"], SCIM_USER_SCHEMA);

        let schema = scim_user_schema_document();
        assert_eq!(schema["id"], SCIM_USER_SCHEMA);
        assert_eq!(schema["attributes"][0]["name"], "userName");
        assert_eq!(schema["attributes"][0]["mutability"], "immutable");
        assert_eq!(schema["attributes"][1]["name"], "externalId");
        assert_eq!(schema["attributes"][3]["mutability"], "readWrite");
        assert_eq!(schema["attributes"][4]["subAttributes"][0]["name"], "value");

        let group_schema = scim_group_schema_document();
        assert_eq!(group_schema["id"], SCIM_GROUP_SCHEMA);
        assert_eq!(group_schema["attributes"][2]["name"], "externalId");
        assert_eq!(group_schema["attributes"][2]["mutability"], "immutable");
        assert_eq!(group_schema["attributes"][3]["name"], "members");
        assert_eq!(group_schema["attributes"][3]["mutability"], "readWrite");
    }

    #[test]
    fn scim_token_comparison_accepts_only_the_configured_token() {
        assert!(scim_tokens_match("configured-token", "configured-token"));
        assert!(!scim_tokens_match("configured-token", "different-token"));
    }

    #[test]
    fn scim_user_document_maps_the_core_user_attributes() {
        let user = scim_user_document(
            "user-1",
            "eagle@example.com",
            "Eagle",
            false,
            "created",
            "updated",
            Some("idp-user-1"),
        );
        assert_eq!(user["userName"], "eagle@example.com");
        assert_eq!(user["active"], false);
        assert_eq!(user["externalId"], "idp-user-1");
        assert_eq!(user["emails"][0]["primary"], true);
    }

    #[test]
    fn scim_user_creation_requires_a_bounded_email_identity() {
        assert!(is_valid_scim_email("eagle@example.com"));
        assert!(!is_valid_scim_email("not-an-email"));
        assert!(!is_valid_scim_email("eagle@localhost"));
        assert!(!is_valid_scim_email(&format!(
            "{}@example.com",
            "e".repeat(250)
        )));
    }

    #[test]
    fn scim_group_creation_requires_unique_non_empty_member_ids() {
        let members = normalized_scim_group_member_ids(Some(vec![
            ScimGroupMemberRequest {
                value: "user-a".into(),
            },
            ScimGroupMemberRequest {
                value: "user-b".into(),
            },
        ]))
        .expect("unique member IDs are accepted");
        assert_eq!(members, ["user-a", "user-b"]);

        assert!(normalized_scim_group_member_ids(Some(vec![
            ScimGroupMemberRequest {
                value: "user-a".into(),
            },
            ScimGroupMemberRequest {
                value: "user-a".into(),
            },
        ]))
        .is_err());
        assert!(
            normalized_scim_group_member_ids(Some(vec![ScimGroupMemberRequest {
                value: " ".into(),
            }]))
            .is_err()
        );
    }

    #[test]
    fn scim_group_membership_patch_accepts_only_one_members_replace_operation() {
        let request = ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".into()],
            operations: vec![ScimPatchOperation {
                op: "replace".into(),
                path: Some("members".into()),
                value: json!([{"value": "user-a"}, {"value": "user-b"}]),
            }],
        };
        assert_eq!(
            parse_group_membership_replace_patch(request).expect("valid membership replacement"),
            ["user-a", "user-b"]
        );

        let invalid = ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".into()],
            operations: vec![ScimPatchOperation {
                op: "add".into(),
                path: Some("members".into()),
                value: json!([]),
            }],
        };
        assert!(parse_group_membership_replace_patch(invalid).is_err());
    }

    #[test]
    fn scim_patch_accepts_safe_single_field_replace_operations() {
        let request = ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".into()],
            operations: vec![ScimPatchOperation {
                op: "replace".into(),
                path: Some("active".into()),
                value: Value::Bool(false),
            }],
        };
        assert!(matches!(
            parse_user_replace_patch(request).unwrap(),
            ScimUserPatch::Active(false)
        ));

        let display_name = ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".into()],
            operations: vec![ScimPatchOperation {
                op: "replace".into(),
                path: Some("displayName".into()),
                value: Value::String("Eagle".into()),
            }],
        };
        assert!(matches!(
            parse_user_replace_patch(display_name).unwrap(),
            ScimUserPatch::DisplayName(value) if value == "Eagle"
        ));

        let email = ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".into()],
            operations: vec![ScimPatchOperation {
                op: "replace".into(),
                path: Some("userName".into()),
                value: Value::String("EAGLE@example.com".into()),
            }],
        };
        assert!(matches!(
            parse_user_replace_patch(email).unwrap(),
            ScimUserPatch::UserName(value) if value == "eagle@example.com"
        ));

        let invalid = ScimPatchRequest {
            schemas: vec!["urn:ietf:params:scim:api:messages:2.0:PatchOp".into()],
            operations: vec![ScimPatchOperation {
                op: "replace".into(),
                path: Some("userName".into()),
                value: Value::String("not-an-email".into()),
            }],
        };
        assert!(parse_user_replace_patch(invalid).is_err());
    }

    #[test]
    fn missing_scim_resources_return_scim_not_found_errors() {
        let response = ScimError::NotFound.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn scim_user_name_filter_accepts_the_idp_lookup_shape() {
        let filter = parse_scim_user_filter("userName eq \"eagle@example.com\"".into()).unwrap();
        assert!(
            matches!(filter, ScimUserFilter::UserNameEq(email) if email == "eagle@example.com")
        );
    }

    #[test]
    fn scim_external_id_filter_accepts_the_idp_correlation_lookup_shape() {
        let filter = parse_scim_user_filter("externalId eq \"idp-user-1\"".into()).unwrap();
        assert!(
            matches!(filter, ScimUserFilter::ExternalIdEq(external_id) if external_id == "idp-user-1")
        );
    }

    #[test]
    fn scim_user_filter_rejects_unsupported_operators_and_attributes() {
        assert!(parse_scim_user_filter("displayName eq \"Eagle\"".into()).is_err());
        assert!(parse_scim_user_filter("userName co \"@example.com\"".into()).is_err());
    }

    #[test]
    fn scim_list_request_enforces_one_based_bounded_pagination() {
        let request = ScimListRequest::try_from(ScimListQuery {
            filter: None,
            start_index: Some(4),
            count: Some(10),
        })
        .unwrap();
        assert_eq!(request.start_index, 4);
        assert_eq!(request.count, 10);
        assert!(ScimListRequest::try_from(ScimListQuery {
            filter: None,
            start_index: Some(0),
            count: None,
        })
        .is_err());
        assert!(ScimListRequest::try_from(ScimListQuery {
            filter: None,
            start_index: None,
            count: Some(SCIM_MAX_PAGE_SIZE + 1),
        })
        .is_err());
    }
}

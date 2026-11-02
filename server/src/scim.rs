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

#[cfg(test)]
tokio::task_local! {static SCIM_DIRECTORY_FINAL_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;}
fn scim_directory_final_check(credential: &CurrentScimCredential) -> Result<(), ScimError> {
    #[cfg(test)]
    let _ = SCIM_DIRECTORY_FINAL_HOOK.try_with(|hook| {
        if let Some(callback) = hook.borrow_mut().take() { callback(); }
    });
    credential.require_current()
}

async fn list_users(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScimListQuery>,
) -> Result<Json<Value>, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let mut conn = state.db.acquire().await.map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let request = ScimListRequest::try_from(query)?;
    let (total_results, users) = match request.filter {
        None => {
            let (total_results,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
                .fetch_one(&mut *conn)
                .await
                .map_err(|_| ScimError::Internal)?;
            let users: Vec<ScimUserRow> = sqlx::query_as(
                "SELECT id, email, display_name, is_active, created_at, updated_at, external_id \
                 FROM users ORDER BY email LIMIT $1 OFFSET $2",
            )
            .bind(i64::from(request.count))
            .bind(i64::from(request.start_index - 1))
            .fetch_all(&mut *conn)
            .await
            .map_err(|_| ScimError::Internal)?;
            (total_results, users)
        }
        Some(ScimUserFilter::UserNameEq(email)) => {
            let (total_results,): (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM users WHERE LOWER(email) = LOWER($1)")
                    .bind(&email)
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(|_| ScimError::Internal)?;
            let users: Vec<ScimUserRow> = sqlx::query_as(
                "SELECT id, email, display_name, is_active, created_at, updated_at, external_id \
                 FROM users WHERE LOWER(email) = LOWER($1) ORDER BY email LIMIT $2 OFFSET $3",
            )
            .bind(email)
            .bind(i64::from(request.count))
            .bind(i64::from(request.start_index - 1))
            .fetch_all(&mut *conn)
            .await
            .map_err(|_| ScimError::Internal)?;
            (total_results, users)
        }
        Some(ScimUserFilter::ExternalIdEq(external_id)) => {
            let (total_results,): (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM users WHERE external_id = $1")
                    .bind(&external_id)
                    .fetch_one(&mut *conn)
                    .await
                    .map_err(|_| ScimError::Internal)?;
            let users: Vec<ScimUserRow> = sqlx::query_as(
                "SELECT id, email, display_name, is_active, created_at, updated_at, external_id \
                 FROM users WHERE external_id = $1 ORDER BY email LIMIT $2 OFFSET $3",
            )
            .bind(external_id)
            .bind(i64::from(request.count))
            .bind(i64::from(request.start_index - 1))
            .fetch_all(&mut *conn)
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
    let response = Json(json!({
        "schemas": [SCIM_LIST_RESPONSE_SCHEMA],
        "totalResults": total_results,
        "startIndex": request.start_index,
        "itemsPerPage": resources.len(),
        "Resources": resources,
    }));
    scim_directory_final_check(&credential)?;
    Ok(response)
}

async fn list_groups(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<ScimListQuery>,
) -> Result<Json<Value>, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let mut conn = state.db.acquire().await.map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let request = ScimListRequest::try_from(query)?;
    if request.filter.is_some() {
        return Err(ScimError::InvalidFilter);
    }
    let (total_results,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM groups")
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| ScimError::Internal)?;
    let groups: Vec<ScimGroupRow> = sqlx::query_as(
        "SELECT id, name, description, created_at, updated_at, external_id FROM groups ORDER BY name LIMIT $1 OFFSET $2",
    )
    .bind(i64::from(request.count))
    .bind(i64::from(request.start_index - 1))
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| ScimError::Internal)?;
    let mut resources = Vec::with_capacity(groups.len());
    for (id, name, description, created_at, updated_at, external_id) in groups {
        resources.push(
            scim_group_document(
                &mut conn,
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
    let response = Json(json!({
        "schemas": [SCIM_LIST_RESPONSE_SCHEMA],
        "totalResults": total_results,
        "startIndex": request.start_index,
        "itemsPerPage": resources.len(),
        "Resources": resources,
    }));
    scim_directory_final_check(&credential)?;
    Ok(response)
}

async fn get_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(group_id): Path<String>,
) -> Result<Json<Value>, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let mut conn = state.db.acquire().await.map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let group: Option<ScimGroupRow> = sqlx::query_as(
        "SELECT id, name, description, created_at, updated_at, external_id FROM groups WHERE id = $1",
    )
    .bind(&group_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let (id, name, description, created_at, updated_at, external_id) =
        group.ok_or(ScimError::NotFound)?;
    let response = Json(
        scim_group_document(
            &mut conn,
            &id,
            &name,
            description.as_deref(),
            &created_at,
            &updated_at,
            external_id.as_deref(),
        )
        .await?,
    );
    scim_directory_final_check(&credential)?;
    Ok(response)
}

type ScimGroupRawRow = (
    String,
    String,
    Option<String>,
    String,
    String,
    Option<String>,
    i64,
    Option<String>,
);
type ScimGroupPolicy = (String, String, String, String, String, String, String);
type ScimGroupAllowlist = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
);
type ScimGroupPage = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
);
#[derive(Default)]
struct ScimGroupDependencies {
    maintenance: Vec<(String, String)>,
    global: Vec<String>,
    local: Vec<(String, String)>,
    policies: Vec<(String, String)>,
}
#[derive(PartialEq, Debug)]
struct ScimGroupSource {
    maintenance: Vec<serde_json::Value>,
    spaces: Vec<String>,
    group: Option<ScimGroupRawRow>,
    members: Vec<(String, String, String, String)>,
    grants: Vec<(String, String, String, String, String, String)>,
    roles: Vec<(String, String, String)>,
    global: Vec<(String, String, String, String, i64, String, String)>,
    local: Vec<(
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        String,
    )>,
    policies: Vec<ScimGroupPolicy>,
    allowlist: Vec<ScimGroupAllowlist>,
    pages: Vec<ScimGroupPage>,
}
async fn scim_group_dependencies(
    conn: &mut sqlx::AnyConnection,
    group: &str,
) -> Result<ScimGroupDependencies, ScimError> {
    Ok(ScimGroupDependencies {
        maintenance: sqlx::query_as("SELECT id,space_id FROM documents WHERE maintenance_group_id=$1 ORDER BY id").bind(group).fetch_all(&mut *conn).await.map_err(|_|ScimError::Internal)?,
        global: sqlx::query_scalar("SELECT role_id FROM group_global_roles WHERE group_id=$1 ORDER BY role_id").bind(group).fetch_all(&mut *conn).await.map_err(|_|ScimError::Internal)?,
        local: sqlx::query_as("SELECT DISTINCT p.space_id,p.role FROM space_group_permissions p JOIN space_roles r ON r.space_id=p.space_id AND r.id=p.role WHERE p.group_id=$1 ORDER BY p.space_id,p.role").bind(group).fetch_all(&mut *conn).await.map_err(|_|ScimError::Internal)?,
        policies: sqlx::query_as("SELECT DISTINCT document_id,space_id FROM document_access_allowlist WHERE group_id=$1 ORDER BY document_id,space_id").bind(group).fetch_all(&mut *conn).await.map_err(|_|ScimError::Internal)?,
    })
}
fn scim_group_drift() -> ScimError {
    ScimError::Internal
}
async fn scim_group_source(
    conn: &mut sqlx::AnyConnection,
    group: &str,
    deps: &ScimGroupDependencies,
) -> Result<ScimGroupSource, ScimError> {
    let spaces = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| ScimError::Internal)?;
    let group: Option<ScimGroupRawRow> = sqlx::query_as("SELECT id,name,description,created_at,updated_at,external_id,scim_managed,managed_space_id FROM groups WHERE id=$1").bind(group).fetch_optional(&mut *conn).await.map_err(|_|ScimError::Internal)?;
    let mut global = Vec::new();
    for id in &deps.global {
        global.push(sqlx::query_as("SELECT id,name,description,permissions,built_in,created_at,updated_at FROM global_roles WHERE id=$1").bind(id).fetch_optional(&mut *conn).await.map_err(|_|ScimError::Internal)?.ok_or_else(scim_group_drift)?);
    }
    let mut local = Vec::new();
    for (space, id) in &deps.local {
        local.push(sqlx::query_as("SELECT id,space_id,name,description,permissions,created_by,created_at,updated_at FROM space_roles WHERE space_id=$1 AND id=$2").bind(space).bind(id).fetch_optional(&mut *conn).await.map_err(|_|ScimError::Internal)?.ok_or_else(scim_group_drift)?);
    }
    let mut policies = Vec::new();
    let mut pages = Vec::new();
    let mut allowlist = Vec::new();
    for (doc, space) in &deps.policies {
        policies.push(sqlx::query_as("SELECT document_id,space_id,mode,revision,created_by,created_at,updated_at FROM document_access_policies WHERE document_id=$1 AND space_id=$2").bind(doc).bind(space).fetch_optional(&mut *conn).await.map_err(|_|ScimError::Internal)?.ok_or_else(scim_group_drift)?);
        pages.push(sqlx::query_as("SELECT id,space_id,parent_id,folder_id,published_at,archived_at,deleted_at,locked_by,created_by FROM documents WHERE id=$1 AND space_id=$2").bind(doc).bind(space).fetch_optional(&mut *conn).await.map_err(|_|ScimError::Internal)?.ok_or_else(scim_group_drift)?);
        let rows: Vec<ScimGroupAllowlist> = sqlx::query_as("SELECT id,document_id,space_id,user_id,group_id,created_by,created_at FROM document_access_allowlist WHERE document_id=$1 ORDER BY id").bind(doc).fetch_all(&mut *conn).await.map_err(|_|ScimError::Internal)?;
        if rows
            .iter()
            .any(|r| &r.2 != space || r.3.is_some() == r.4.is_some())
        {
            return Err(scim_group_drift());
        }
        allowlist.extend(rows);
    }
    let mut maintenance = Vec::new();
    for (id, space) in &deps.maintenance {
        let mut records = crate::spaces::maintenance::rows(
            conn,
            "SELECT * FROM documents WHERE id=$1 AND space_id=$2",
            &[id, space],
        )
        .await
        .map_err(|_| ScimError::Internal)?;
        if records.len() != 1 {
            return Err(scim_group_drift());
        }
        maintenance.append(&mut records);
    }
    Ok(ScimGroupSource {
        maintenance,
        spaces,
        group,
        members: Vec::new(),
        grants: Vec::new(),
        roles: Vec::new(),
        global,
        local,
        policies,
        allowlist,
        pages,
    })
}
// Keep link queries keyed to the requested ID even after deletion, so a trigger
// cannot conceal a recreated association behind an absent canonical group.
async fn full_scim_group_source(
    conn: &mut sqlx::AnyConnection,
    group: &str,
    deps: &ScimGroupDependencies,
) -> Result<ScimGroupSource, ScimError> {
    let mut source = scim_group_source(conn, group, deps).await?;
    source.members = sqlx::query_as(
        "SELECT id,group_id,user_id,created_at FROM group_members WHERE group_id=$1 ORDER BY id",
    )
    .bind(group)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| ScimError::Internal)?;
    source.grants=sqlx::query_as("SELECT id,space_id,group_id,role,created_at,updated_at FROM space_group_permissions WHERE group_id=$1 ORDER BY id").bind(group).fetch_all(&mut *conn).await.map_err(|_|ScimError::Internal)?;
    source.roles=sqlx::query_as("SELECT group_id,role_id,created_at FROM group_global_roles WHERE group_id=$1 ORDER BY role_id").bind(group).fetch_all(&mut *conn).await.map_err(|_|ScimError::Internal)?;
    let corrupt:Option<i64>=sqlx::query_scalar("SELECT 1 FROM group_members m LEFT JOIN users u ON u.id=m.user_id WHERE m.group_id=$1 AND u.id IS NULL UNION ALL SELECT 1 FROM space_group_permissions p LEFT JOIN spaces s ON s.id=p.space_id LEFT JOIN space_roles r ON r.id=p.role AND r.space_id=p.space_id WHERE p.group_id=$1 AND (s.id IS NULL OR (p.role NOT IN ('owner','space_admin','admin','editor','contributor','commenter','viewer') AND r.id IS NULL)) LIMIT 1").bind(group).fetch_optional(&mut *conn).await.map_err(|_|ScimError::Internal)?;
    if corrupt.is_some() {
        return Err(scim_group_drift());
    }
    let actual_maintenance: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,space_id FROM documents WHERE maintenance_group_id=$1 ORDER BY id",
    )
    .bind(group)
    .fetch_all(&mut *conn)
    .await
    .map_err(|_| ScimError::Internal)?;
    if actual_maintenance
        .iter()
        .any(|p| !deps.maintenance.contains(p))
    {
        return Err(scim_group_drift());
    }
    let actual_policies:Vec<(String,String)>=sqlx::query_as("SELECT DISTINCT document_id,space_id FROM document_access_allowlist WHERE group_id=$1 ORDER BY document_id,space_id").bind(group).fetch_all(&mut *conn).await.map_err(|_|ScimError::Internal)?;
    if actual_policies.iter().any(|p| !deps.policies.contains(p)) {
        return Err(scim_group_drift());
    }
    Ok(source)
}
struct ScimNormalizedGroup {
    name: String,
    description: Option<String>,
    members: Vec<String>,
    external_id: Option<String>,
}
fn normalize_scim_group(
    request: ScimGroupCreateRequest,
    create: bool,
) -> Result<ScimNormalizedGroup, ScimError> {
    let name = request.display_name.trim().to_owned();
    let description = request
        .description
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    let members = normalized_scim_group_member_ids(request.members)?;
    let external_id = request
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    if name.is_empty()
        || name.len() > 100
        || description.as_ref().is_some_and(|v| v.len() > 1000)
        || (create && external_id.as_ref().is_some_and(|v| v.len() > 255))
    {
        return Err(ScimError::InvalidResource);
    }
    Ok(ScimNormalizedGroup {
        name,
        description,
        members,
        external_id,
    })
}
enum ScimGroupChange {
    Create(ScimNormalizedGroup),
    Replace(ScimNormalizedGroup),
    Members(Vec<String>),
    Delete,
}
#[cfg(test)]
tokio::task_local! {static SCIM_GROUP_FINAL_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;}
async fn mutate_scim_group(
    state: &AppState,
    credential: &CurrentScimCredential,
    group: &str,
    change: ScimGroupChange,
) -> Result<Option<Value>, ScimError> {
    let mut tx = state.db.begin().await.map_err(|_| ScimError::Internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx)
        .await
        .map_err(|_| ScimError::Internal)?;
    let spaces: Vec<String> = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| ScimError::Internal)?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(
        &mut tx,
        &spaces.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await
    .map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let create = matches!(change, ScimGroupChange::Create(_));
    let locked = sqlx::query("UPDATE groups SET id=id WHERE id=$1 AND scim_managed=1")
        .bind(group)
        .execute(&mut *tx)
        .await
        .map_err(|_| ScimError::Internal)?;
    if (!create && locked.rows_affected() != 1) || (create && locked.rows_affected() != 0) {
        return Err(if create {
            ScimError::Conflict
        } else {
            ScimError::NotFound
        });
    }
    let deps = scim_group_dependencies(&mut tx, group).await?;
    for id in &deps.global {
        if sqlx::query("UPDATE global_roles SET id=id WHERE id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?
            .rows_affected()
            != 1
        {
            return Err(scim_group_drift());
        }
    }
    for (space, id) in &deps.local {
        if sqlx::query("UPDATE space_roles SET permissions=permissions WHERE space_id=$1 AND id=$2")
            .bind(space)
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?
            .rows_affected()
            != 1
        {
            return Err(scim_group_drift());
        }
    }
    for (select, update) in [
        (
            "SELECT id FROM group_members WHERE group_id=$1 ORDER BY id",
            "UPDATE group_members SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT id FROM space_group_permissions WHERE group_id=$1 ORDER BY id",
            "UPDATE space_group_permissions SET id=id WHERE group_id=$1 AND id=$2",
        ),
        (
            "SELECT role_id FROM group_global_roles WHERE group_id=$1 ORDER BY role_id",
            "UPDATE group_global_roles SET group_id=group_id WHERE group_id=$1 AND role_id=$2",
        ),
    ] {
        let ids: Vec<String> = sqlx::query_scalar(select)
            .bind(group)
            .fetch_all(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?;
        for id in ids {
            if sqlx::query(update)
                .bind(group)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|_| ScimError::Internal)?
                .rows_affected()
                != 1
            {
                return Err(scim_group_drift());
            }
        }
    }
    for (doc, space) in &deps.maintenance {
        if sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
            .bind(doc)
            .bind(space)
            .execute(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?
            .rows_affected()
            != 1
        {
            return Err(scim_group_drift());
        }
    }
    for (doc, space) in &deps.policies {
        if sqlx::query("UPDATE documents SET title=title WHERE id=$1 AND space_id=$2")
            .bind(doc)
            .bind(space)
            .execute(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?
            .rows_affected()
            != 1
        {
            return Err(scim_group_drift());
        }
        if sqlx::query("UPDATE document_access_policies SET revision=revision WHERE document_id=$1 AND space_id=$2").bind(doc).bind(space).execute(&mut *tx).await.map_err(|_|ScimError::Internal)?.rows_affected()!=1 {return Err(scim_group_drift());}
        let ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM document_access_allowlist WHERE document_id=$1 ORDER BY id",
        )
        .bind(doc)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| ScimError::Internal)?;
        for id in ids {
            if sqlx::query(
                "UPDATE document_access_allowlist SET id=id WHERE id=$1 AND document_id=$2",
            )
            .bind(id)
            .bind(doc)
            .execute(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?
            .rows_affected()
                != 1
            {
                return Err(scim_group_drift());
            }
        }
    }
    credential.require_current()?;
    let mut expected = full_scim_group_source(&mut tx, group, &deps).await?;
    if expected.spaces != spaces {
        return Err(ScimError::Internal);
    }
    if create
        && (expected.group.is_some()
            || !expected.members.is_empty()
            || !expected.grants.is_empty()
            || !expected.roles.is_empty()
            || !deps.policies.is_empty())
    {
        return Err(ScimError::Conflict);
    }
    if !create && expected.group.as_ref().is_none_or(|r| r.6 != 1) {
        return Err(ScimError::NotFound);
    }
    let requested = match &change {
        ScimGroupChange::Create(r) | ScimGroupChange::Replace(r) => Some(&r.members),
        ScimGroupChange::Members(ids) => Some(ids),
        ScimGroupChange::Delete => None,
    };
    let mut user_ids: Vec<String> = expected.members.iter().map(|r| r.2.clone()).collect();
    if let Some(ids) = requested {
        user_ids.extend(ids.iter().cloned());
    }
    user_ids.sort();
    user_ids.dedup();
    let mut users = Vec::new();
    for id in &user_ids {
        let row: Option<(String, String, String, i64, Option<String>)> = sqlx::query_as(
            "SELECT id,email,display_name,is_active,external_id FROM users WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| ScimError::Internal)?;
        users.push(row.ok_or(ScimError::InvalidResource)?);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let (action, members) = match change {
        ScimGroupChange::Create(r) => {
            let result=sqlx::query("INSERT INTO groups(id,name,description,external_id,scim_managed,created_at,updated_at) VALUES($1,$2,$3,$4,1,$5,$5)").bind(group).bind(&r.name).bind(&r.description).bind(&r.external_id).bind(&now).execute(&mut *tx).await.map_err(|e|if is_unique_constraint_error(&e){ScimError::Conflict}else{ScimError::Internal})?;
            if result.rows_affected() != 1 {
                return Err(ScimError::Internal);
            }
            expected.group = Some((
                group.into(),
                r.name,
                r.description,
                now.clone(),
                now.clone(),
                r.external_id,
                1,
                None,
            ));
            ("group.scim_create", Some(r.members))
        }
        ScimGroupChange::Replace(r) => {
            let row = expected.group.as_mut().ok_or(ScimError::NotFound)?;
            if r.external_id != row.5 {
                return Err(ScimError::InvalidResource);
            }
            let duplicate: Option<String> =
                sqlx::query_scalar("SELECT id FROM groups WHERE name=$1 AND id!=$2 LIMIT 1")
                    .bind(&r.name)
                    .bind(group)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(|_| ScimError::Internal)?;
            if duplicate.is_some() {
                return Err(ScimError::Conflict);
            }
            let result=sqlx::query("UPDATE groups SET name=$1,description=$2,updated_at=$3 WHERE id=$4 AND scim_managed=1").bind(&r.name).bind(&r.description).bind(&now).bind(group).execute(&mut *tx).await.map_err(|e|if is_unique_constraint_error(&e){ScimError::Conflict}else{ScimError::Internal})?;
            if result.rows_affected() != 1 {
                return Err(ScimError::NotFound);
            }
            row.1 = r.name;
            row.2 = r.description;
            row.4 = now.clone();
            ("group.scim_replace", Some(r.members))
        }
        ScimGroupChange::Members(ids) => {
            if sqlx::query("UPDATE groups SET updated_at=$1 WHERE id=$2 AND scim_managed=1")
                .bind(&now)
                .bind(group)
                .execute(&mut *tx)
                .await
                .map_err(|_| ScimError::Internal)?
                .rows_affected()
                != 1
            {
                return Err(ScimError::NotFound);
            }
            expected.group.as_mut().ok_or(ScimError::NotFound)?.4 = now.clone();
            ("group.scim_membership_replace", Some(ids))
        }
        ScimGroupChange::Delete => {
            if sqlx::query("DELETE FROM groups WHERE id=$1 AND scim_managed=1")
                .bind(group)
                .execute(&mut *tx)
                .await
                .map_err(|_| ScimError::Internal)?
                .rows_affected()
                != 1
            {
                return Err(ScimError::NotFound);
            }
            for page in &mut expected.maintenance {
                page["maintenance_group_id"] = serde_json::Value::Null;
            }
            expected.group = None;
            expected.members.clear();
            expected.grants.clear();
            expected.roles.clear();
            expected.allowlist.retain(|r| r.4.as_deref() != Some(group));
            ("group.scim_delete", None)
        }
    };
    if let Some(ids) = members {
        let deleted = sqlx::query("DELETE FROM group_members WHERE group_id=$1")
            .bind(group)
            .execute(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?;
        if deleted.rows_affected() != expected.members.len() as u64 {
            return Err(ScimError::Internal);
        }
        expected.members.clear();
        for user in ids {
            let id = uuid::Uuid::new_v4().to_string();
            if sqlx::query(
                "INSERT INTO group_members(id,group_id,user_id,created_at) VALUES($1,$2,$3,$4)",
            )
            .bind(&id)
            .bind(group)
            .bind(&user)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?
            .rows_affected()
                != 1
            {
                return Err(ScimError::Internal);
            }
            expected.members.push((id, group.into(), user, now.clone()));
        }
        expected.members.sort_by(|a, b| a.0.cmp(&b.0));
    }
    credential.require_current()?;
    let event =
        match crate::audit::insert_scim_actor_action(&mut tx, credential, action, "group", group)
            .await
        {
            Ok(event) => event,
            Err(_) => {
                credential.require_current()?;
                return Err(ScimError::Internal);
            }
        };
    // Build wire members within the transaction; no pooled post-commit read can
    // turn a committed mutation into a failed acknowledgement.
    let response = if let Some(row) = &expected.group {
        let member_rows:Vec<(String,String)>=sqlx::query_as("SELECT u.id,u.display_name FROM group_members m JOIN users u ON u.id=m.user_id WHERE m.group_id=$1 ORDER BY u.display_name").bind(group).fetch_all(&mut *tx).await.map_err(|_|ScimError::Internal)?;
        Some(
            json!({"schemas":[SCIM_GROUP_SCHEMA],"id":row.0,"displayName":row.1,"description":row.2,"externalId":row.5,"members":member_rows.into_iter().map(|(value,display)|json!({"value":value,"display":display})).collect::<Vec<_>>(),"meta":{"resourceType":"Group","created":row.3,"lastModified":row.4}}),
        )
    } else {
        None
    };
    if full_scim_group_source(&mut tx, group, &deps).await? != expected {
        return Err(ScimError::Internal);
    }
    for (id, row) in user_ids.iter().zip(users) {
        let current: Option<(String, String, String, i64, Option<String>)> = sqlx::query_as(
            "SELECT id,email,display_name,is_active,external_id FROM users WHERE id=$1",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| ScimError::Internal)?;
        if current != Some(row) {
            return Err(ScimError::Internal);
        }
    }
    if crate::audit::verify_scim_actor_action(&mut tx, &event, credential)
        .await
        .is_err()
    {
        credential.require_current()?;
        return Err(ScimError::Internal);
    }
    #[cfg(test)]
    let _ = SCIM_GROUP_FINAL_HOOK.try_with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
    credential.require_current()?;
    tx.commit().await.map_err(|_| ScimError::Internal)?;
    crate::audit::append_actor_action(&event).await;
    Ok(response)
}
async fn patch_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<ScimPatchRequest>,
) -> Result<StatusCode, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let members = parse_group_membership_replace_patch(request)?;
    mutate_scim_group(&state, &credential, &id, ScimGroupChange::Members(members)).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn replace_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<ScimGroupCreateRequest>,
) -> Result<Json<Value>, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let request = normalize_scim_group(request, false)?;
    Ok(Json(
        mutate_scim_group(&state, &credential, &id, ScimGroupChange::Replace(request))
            .await?
            .ok_or(ScimError::Internal)?,
    ))
}
async fn delete_group(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    mutate_scim_group(&state, &credential, &id, ScimGroupChange::Delete).await?;
    Ok(StatusCode::NO_CONTENT)
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
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let request = normalize_scim_group(request, true)?;
    let id = uuid::Uuid::new_v4().to_string();
    let response = mutate_scim_group(&state, &credential, &id, ScimGroupChange::Create(request))
        .await?
        .ok_or(ScimError::Internal)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        LOCATION,
        HeaderValue::from_str(&format!("/scim/v2/Groups/{id}")).expect("SCIM group UUID path"),
    );
    Ok((StatusCode::CREATED, headers, Json(response)))
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
    conn: &mut sqlx::AnyConnection,
    id: &str,
    name: &str,
    description: Option<&str>,
    created_at: &str,
    updated_at: &str,
    external_id: Option<&str>,
) -> Result<Value, ScimError> {
    let members: Vec<(String, String)> = sqlx::query_as(
        "SELECT u.id, u.display_name FROM group_members gm JOIN users u ON u.id = gm.user_id WHERE gm.group_id = $1 ORDER BY u.display_name",
    )
    .bind(id)
    .fetch_all(&mut *conn)
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

#[cfg(test)]
tokio::task_local! {static SCIM_ACCOUNT_FINAL_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;}
async fn create_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<ScimUserCreateRequest>,
) -> Result<(StatusCode, HeaderMap, Json<Value>), ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let email = request.user_name.trim().to_ascii_lowercase();
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(&email)
        .to_owned();
    let external_id = request
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    if !is_valid_scim_email(&email)
        || display_name.len() > 100
        || external_id.as_ref().is_some_and(|v| v.len() > 255)
    {
        return Err(ScimError::InvalidResource);
    }
    let mut tx = state.db.begin().await.map_err(|_| ScimError::Internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx)
        .await
        .map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id FROM users WHERE LOWER(email)=LOWER($1) OR ($2 IS NOT NULL AND external_id=$2)",
    )
    .bind(&email)
    .bind(&external_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    if existing.is_some() {
        return Err(ScimError::Conflict);
    }
    let prepared = crate::auth::routes::insert_account_with_personal_space(
        &mut tx,
        crate::auth::routes::NewAccountSeed {
            id: uuid::Uuid::new_v4().to_string(),
            email,
            display_name,
            password_hash: None,
            avatar_url: None,
            role: "member".into(),
            is_active: request.active.unwrap_or(true),
            external_id,
        },
    )
    .await
    .map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let mut events = Vec::new();
    for (action, target, id) in [
        ("user.scim_create", "user", prepared.user_id()),
        (
            "space.personal_create",
            "space",
            prepared.personal.id.as_str(),
        ),
    ] {
        let event =
            match crate::audit::insert_scim_actor_action(&mut tx, &credential, action, target, id)
                .await
            {
                Ok(event) => event,
                Err(_) => {
                    credential.require_current()?;
                    return Err(ScimError::Internal);
                }
            };
        events.push(event);
    }
    crate::auth::routes::verify_prepared_account(&mut tx, &prepared)
        .await
        .map_err(|_| ScimError::Internal)?;
    for event in &events {
        if crate::audit::verify_scim_actor_action(&mut tx, event, &credential)
            .await
            .is_err()
        {
            credential.require_current()?;
            return Err(ScimError::Internal);
        }
    }
    let response = scim_user_document(
        prepared.user_id(),
        prepared.email(),
        prepared.display_name(),
        prepared.is_active(),
        prepared.created_at(),
        prepared.updated_at(),
        prepared.external_id(),
    );
    let mut response_headers = HeaderMap::new();
    response_headers.insert(
        LOCATION,
        HeaderValue::from_str(&format!("/scim/v2/Users/{}", prepared.user_id()))
            .expect("SCIM user UUID path"),
    );
    #[cfg(test)]
    let _ = SCIM_ACCOUNT_FINAL_HOOK.try_with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
    credential.require_current()?;
    tx.commit().await.map_err(|_| ScimError::Internal)?;
    // SQL is canonical. Directory/JSONL delivery after a known commit is advisory;
    // neither can acknowledge an unknown database commit as successful.
    if let Err(error) = crate::storage::init_space_dirs(&prepared.personal.slug).await {
        tracing::warn!(error=%error,"SCIM personal-space directory initialization deferred");
    }
    for event in &events {
        crate::audit::append_actor_action(event).await;
    }
    Ok((StatusCode::CREATED, response_headers, Json(response)))
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
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let mut conn = state.db.acquire().await.map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let user: Option<ScimUserRow> = sqlx::query_as(
        "SELECT id, email, display_name, is_active, created_at, updated_at, external_id FROM users WHERE id = $1",
    )
    .bind(&user_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|_| ScimError::Internal)?;

    credential.require_current()?;
    let (id, email, display_name, is_active, created_at, updated_at, external_id) =
        user.ok_or(ScimError::NotFound)?;
    let response = Json(scim_user_document(
        &id,
        &email,
        &display_name,
        is_active != 0,
        &created_at,
        &updated_at,
        external_id.as_deref(),
    ));
    scim_directory_final_check(&credential)?;
    Ok(response)
}

// This is the configured SCIM credential, never a human or API-key administrator.
// Deliberately no Debug/Serialize: the supplied secret is not an audit identity.
pub(crate) struct CurrentScimCredential {
    supplied: String,
}
impl CurrentScimCredential {
    fn from_headers(headers: &HeaderMap) -> Result<Self, ScimError> {
        require_scim_bearer_token(headers)?;
        let supplied = headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(ScimError::Unauthorized)?
            .to_owned();
        Ok(Self { supplied })
    }
    fn require_current(&self) -> Result<(), ScimError> {
        let configured = std::env::var(SCIM_BEARER_TOKEN_ENV)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .ok_or(ScimError::NotConfigured)?;
        if scim_tokens_match(&configured, &self.supplied) {
            Ok(())
        } else {
            Err(ScimError::Unauthorized)
        }
    }
    pub(crate) fn is_current(&self) -> bool {
        self.require_current().is_ok()
    }
}
#[derive(Clone, PartialEq, Eq, sqlx::FromRow)]
struct ScimMutationUser {
    account_kind: String,
    id: String,
    email: String,
    display_name: String,
    password_hash: Option<String>,
    avatar_url: Option<String>,
    role: String,
    is_active: i64,
    totp_secret: Option<String>,
    totp_enabled: i64,
    email_otp_enabled: i64,
    recovery_codes: Option<String>,
    created_at: String,
    updated_at: String,
    external_id: Option<String>,
    username: Option<String>,
}
async fn scim_mutation_user(
    conn: &mut sqlx::AnyConnection,
    id: &str,
) -> Result<Option<ScimMutationUser>, ScimError> {
    sqlx::query_as("SELECT account_kind,id,email,display_name,password_hash,avatar_url,role,is_active,totp_secret,totp_enabled,email_otp_enabled,recovery_codes,created_at,updated_at,external_id,username FROM users WHERE id=$1").bind(id).fetch_optional(conn).await.map_err(|_|ScimError::Internal)
}
#[derive(PartialEq, Eq)]
struct ScimUserSource {
    user: Option<ScimMutationUser>,
    spaces: Vec<String>,
    provenance: Vec<Vec<Vec<Option<String>>>>,
}
async fn scim_user_source(
    conn: &mut sqlx::AnyConnection,
    id: &str,
) -> Result<ScimUserSource, ScimError> {
    use sqlx::Row;
    let user = scim_mutation_user(conn, id).await?;
    let spaces = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *conn)
        .await
        .map_err(|_| ScimError::Internal)?;
    // These associations are not modified by a SCIM profile/activity update.
    // Exact raw tuples reject trigger-induced grant, membership or private-share drift.
    let queries=[
        "SELECT CAST(id AS TEXT),CAST(space_id AS TEXT),CAST(user_id AS TEXT),CAST(role AS TEXT),CAST(created_at AS TEXT) FROM space_members WHERE user_id=$1 ORDER BY id",
        "SELECT CAST(id AS TEXT),CAST(group_id AS TEXT),CAST(user_id AS TEXT),CAST(created_at AS TEXT) FROM group_members WHERE user_id=$1 ORDER BY id",
        "SELECT CAST(space_id AS TEXT),CAST(user_id AS TEXT),CAST(created_by AS TEXT),CAST(created_at AS TEXT) FROM space_user_blocks WHERE user_id=$1 ORDER BY space_id",
        "SELECT CAST(id AS TEXT),CAST(document_id AS TEXT),CAST(space_id AS TEXT),CAST(user_id AS TEXT),CAST(group_id AS TEXT),CAST(created_by AS TEXT),CAST(created_at AS TEXT) FROM document_access_allowlist WHERE user_id=$1 ORDER BY id",
        "SELECT CAST(draft_id AS TEXT),CAST(user_id AS TEXT),CAST(role AS TEXT),CAST(granted_by AS TEXT),CAST(created_at AS TEXT),CAST(updated_at AS TEXT) FROM document_draft_access WHERE user_id=$1 ORDER BY draft_id",
    ];
    let mut provenance = Vec::new();
    for query in queries {
        let rows = sqlx::query(query)
            .bind(id)
            .fetch_all(&mut *conn)
            .await
            .map_err(|_| ScimError::Internal)?;
        let mut raw = Vec::new();
        for row in rows {
            let mut cols = Vec::new();
            for i in 0..row.len() {
                cols.push(
                    row.try_get::<Option<String>, _>(i)
                        .map_err(|_| ScimError::Internal)?,
                );
            }
            raw.push(cols);
        }
        provenance.push(raw);
    }
    Ok(ScimUserSource {
        user,
        spaces,
        provenance,
    })
}
enum ScimUserChange {
    Replace {
        email: String,
        display_name: String,
        active: bool,
        external_id: Option<String>,
    },
    Patch(ScimUserPatch),
    Deactivate,
}
#[cfg(test)]
tokio::task_local! {static SCIM_USER_FINAL_HOOK:std::cell::RefCell<Option<Box<dyn FnOnce()+Send>>>;}
async fn mutate_scim_user(
    state: &AppState,
    credential: &CurrentScimCredential,
    id: &str,
    change: ScimUserChange,
) -> Result<Value, ScimError> {
    let mut tx = state.db.begin().await.map_err(|_| ScimError::Internal)?;
    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut tx)
        .await
        .map_err(|_| ScimError::Internal)?;
    let spaces: Vec<String> = sqlx::query_scalar("SELECT id FROM spaces ORDER BY id")
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| ScimError::Internal)?;
    crate::spaces::mutation_guards::lock_spaces_for_mutation(
        &mut tx,
        &spaces.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .await
    .map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    let locked = sqlx::query("UPDATE users SET id=id WHERE id=$1 AND external_id IS NOT NULL")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|_| ScimError::Internal)?;
    credential.require_current()?;
    if locked.rows_affected() != 1 {
        return Err(ScimError::NotFound);
    }
    let mut expected = scim_user_source(&mut tx, id).await?;
    if expected.spaces != spaces {
        return Err(ScimError::Internal);
    }
    let row = expected
        .user
        .as_mut()
        .filter(|r| r.external_id.is_some())
        .ok_or(ScimError::NotFound)?;
    let now = chrono::Utc::now().to_rfc3339();
    let (action, result) = match change {
        ScimUserChange::Replace {
            email,
            display_name,
            active,
            external_id,
        } => {
            if external_id
                .as_ref()
                .is_some_and(|v| Some(v) != row.external_id.as_ref())
            {
                return Err(ScimError::InvalidResource);
            }
            let conflict: Option<String> = sqlx::query_scalar(
                "SELECT id FROM users WHERE LOWER(email)=LOWER($1) AND id!=$2 LIMIT 1",
            )
            .bind(&email)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| ScimError::Internal)?;
            if conflict.is_some() {
                return Err(ScimError::Conflict);
            }
            row.email = email;
            row.display_name = display_name;
            row.is_active = i64::from(active);
            ("user.scim_replace",sqlx::query("UPDATE users SET email=$1,display_name=$2,is_active=$3,updated_at=$4 WHERE id=$5 AND external_id=$6").bind(&row.email).bind(&row.display_name).bind(row.is_active).bind(&now).bind(id).bind(&row.external_id).execute(&mut *tx).await)
        }
        ScimUserChange::Patch(ScimUserPatch::Active(active)) => {
            row.is_active = i64::from(active);
            (
                "user.scim_lifecycle_update",
                sqlx::query(
                    "UPDATE users SET is_active=$1,updated_at=$2 WHERE id=$3 AND external_id=$4",
                )
                .bind(row.is_active)
                .bind(&now)
                .bind(id)
                .bind(&row.external_id)
                .execute(&mut *tx)
                .await,
            )
        }
        ScimUserChange::Patch(ScimUserPatch::DisplayName(name)) => {
            row.display_name = name;
            (
                "user.scim_profile_update",
                sqlx::query(
                    "UPDATE users SET display_name=$1,updated_at=$2 WHERE id=$3 AND external_id=$4",
                )
                .bind(&row.display_name)
                .bind(&now)
                .bind(id)
                .bind(&row.external_id)
                .execute(&mut *tx)
                .await,
            )
        }
        ScimUserChange::Patch(ScimUserPatch::UserName(email)) => {
            row.email = email;
            (
                "user.scim_profile_update",
                sqlx::query(
                    "UPDATE users SET email=$1,updated_at=$2 WHERE id=$3 AND external_id=$4",
                )
                .bind(&row.email)
                .bind(&now)
                .bind(id)
                .bind(&row.external_id)
                .execute(&mut *tx)
                .await,
            )
        }
        ScimUserChange::Deactivate => {
            row.is_active = 0;
            (
                "user.scim_deprovision",
                sqlx::query(
                    "UPDATE users SET is_active=0,updated_at=$1 WHERE id=$2 AND external_id=$3",
                )
                .bind(&now)
                .bind(id)
                .bind(&row.external_id)
                .execute(&mut *tx)
                .await,
            )
        }
    };
    let result = result.map_err(|e| {
        if is_unique_constraint_error(&e) {
            ScimError::Conflict
        } else {
            ScimError::Internal
        }
    })?;
    if result.rows_affected() != 1 {
        return Err(ScimError::NotFound);
    }
    row.updated_at = now;
    let response = scim_user_document(
        &row.id,
        &row.email,
        &row.display_name,
        row.is_active != 0,
        &row.created_at,
        &row.updated_at,
        row.external_id.as_deref(),
    );
    credential.require_current()?;
    let event =
        match crate::audit::insert_scim_actor_action(&mut tx, credential, action, "user", id).await
        {
            Ok(event) => event,
            Err(_) => {
                credential.require_current()?;
                return Err(ScimError::Internal);
            }
        };
    if scim_user_source(&mut tx, id).await? != expected {
        return Err(ScimError::Internal);
    }
    if crate::audit::verify_scim_actor_action(&mut tx, &event, credential)
        .await
        .is_err()
    {
        credential.require_current()?;
        return Err(ScimError::Internal);
    }
    #[cfg(test)]
    let _ = SCIM_USER_FINAL_HOOK.try_with(|slot| {
        if let Some(hook) = slot.borrow_mut().take() {
            hook();
        }
    });
    credential.require_current()?;
    tx.commit().await.map_err(|_| ScimError::Internal)?;
    crate::audit::append_actor_action(&event).await;
    Ok(response)
}
async fn replace_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<ScimUserReplaceRequest>,
) -> Result<Json<Value>, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let email = request.user_name.trim().to_ascii_lowercase();
    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .unwrap_or(&email)
        .to_owned();
    let external_id = request
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned);
    if !is_valid_scim_email(&email) || display_name.len() > 100 {
        return Err(ScimError::InvalidResource);
    }
    Ok(Json(
        mutate_scim_user(
            &state,
            &credential,
            &id,
            ScimUserChange::Replace {
                email,
                display_name,
                active: request.active.unwrap_or(true),
                external_id,
            },
        )
        .await?,
    ))
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
    Path(id): Path<String>,
    Json(request): Json<ScimPatchRequest>,
) -> Result<StatusCode, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    let change = parse_user_replace_patch(request)?;
    mutate_scim_user(&state, &credential, &id, ScimUserChange::Patch(change)).await?;
    Ok(StatusCode::NO_CONTENT)
}
async fn delete_user(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<StatusCode, ScimError> {
    let credential = CurrentScimCredential::from_headers(&headers)?;
    mutate_scim_user(&state, &credential, &id, ScimUserChange::Deactivate).await?;
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
#[cfg(test)]
mod current_user_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        sqlx::query(
            "UPDATE users SET external_id='idp-user',role='viewer',is_active=0 WHERE id='viewer-a'",
        )
        .execute(&s.db)
        .await
        .unwrap();
        s
    }
    fn headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer isolated-scim-test"),
        );
        h
    }
    fn change(mode: usize) -> ScimUserChange {
        match mode {
            0 => ScimUserChange::Replace {
                email: "updated@example.test".into(),
                display_name: "Updated".into(),
                active: true,
                external_id: Some("idp-user".into()),
            },
            1 => ScimUserChange::Patch(ScimUserPatch::DisplayName("Patched".into())),
            _ => ScimUserChange::Deactivate,
        }
    }
    async fn request(
        s: &AppState,
        method: &str,
        id: &str,
        body: Value,
        token: &str,
    ) -> axum::response::Response {
        router()
            .with_state(s.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method(method)
                    .uri(format!("/Users/{id}"))
                    .header(AUTHORIZATION, format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    async fn run_case(case: &str) {
        match case {
            "wire" => {
                let s = setup(false).await;
                let r=request(&s,"PUT","viewer-a",json!({"userName":" UPDATED@EXAMPLE.TEST ","displayName":" ","externalId":"idp-user"}),"isolated-scim-test").await;
                assert_eq!(r.status(), StatusCode::OK);
                let body: Value = serde_json::from_slice(
                    &axum::body::to_bytes(r.into_body(), 10240).await.unwrap(),
                )
                .unwrap();
                assert_eq!(body["active"], true);
                assert_eq!(body["displayName"], "updated@example.test");
                assert_eq!(body["externalId"], "idp-user");
                assert_eq!(request(&s,"PATCH","viewer-a",json!({"schemas":["urn:ietf:params:scim:api:messages:2.0:PatchOp"],"operations":[{"op":"replace","path":"active","value":false}]}),"isolated-scim-test").await.status(),StatusCode::NO_CONTENT);
                for _ in 0..2 {
                    assert_eq!(
                        request(&s, "DELETE", "viewer-a", json!({}), "isolated-scim-test")
                            .await
                            .status(),
                        StatusCode::NO_CONTENT
                    );
                }
                let row = scim_mutation_user(&mut *s.db.acquire().await.unwrap(), "viewer-a")
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(row.role, "viewer");
                assert_eq!(row.is_active, 0);
                let rows:Vec<(Option<String>,String,String)>=sqlx::query_as("SELECT user_id,actor_kind,actor_id FROM audit_log WHERE action LIKE 'user.scim_%'").fetch_all(&s.db).await.unwrap();
                assert_eq!(rows.len(), 4);
                assert!(rows
                    .iter()
                    .all(|r| r.0.is_none() && r.1 == "scim_token" && r.2 == "SCIM_BEARER_TOKEN"));
            }
            "denial" => {
                let s = setup(false).await;
                let before = move_database_snapshot(&s).await;
                for method in ["PUT", "PATCH", "DELETE"] {
                    let body = if method == "PUT" {
                        json!({"userName":"valid@example.test"})
                    } else {
                        json!({"schemas":["urn:ietf:params:scim:api:messages:2.0:PatchOp"],"operations":[{"op":"replace","path":"active","value":false}]})
                    };
                    assert_eq!(
                        request(&s, method, "viewer-a", body.clone(), "wrong")
                            .await
                            .status(),
                        StatusCode::UNAUTHORIZED
                    );
                    for id in ["missing", "owner-a"] {
                        assert_eq!(
                            request(&s, method, id, body.clone(), "isolated-scim-test")
                                .await
                                .status(),
                            StatusCode::NOT_FOUND
                        );
                    }
                }
                assert_eq!(
                    request(
                        &s,
                        "PUT",
                        "viewer-a",
                        json!({"userName":"valid@example.test","externalId":"changed"}),
                        "isolated-scim-test"
                    )
                    .await
                    .status(),
                    StatusCode::BAD_REQUEST
                );
                assert_eq!(move_database_snapshot(&s).await, before);
                std::env::remove_var(SCIM_BEARER_TOKEN_ENV);
                assert_eq!(
                    request(&s, "DELETE", "viewer-a", json!({}), "isolated-scim-test")
                        .await
                        .status(),
                    StatusCode::NOT_FOUND
                );
            }
            "rollback" => {
                for mode in 0..3 {
                    for trigger in [
                    "CREATE TRIGGER scim_drift AFTER UPDATE OF updated_at ON users WHEN NEW.id='viewer-a' BEGIN UPDATE users SET role='admin' WHERE id=NEW.id; END",
                    "CREATE TRIGGER scim_drift AFTER UPDATE OF updated_at ON users WHEN NEW.id='viewer-a' BEGIN DELETE FROM space_members WHERE user_id=NEW.id; END",
                    "CREATE TRIGGER scim_drift AFTER INSERT ON audit_log BEGIN SELECT RAISE(ABORT,'audit failure'); END",
                    "CREATE TRIGGER scim_drift AFTER INSERT ON audit_log BEGIN UPDATE audit_log SET after_value='injected' WHERE id=NEW.id; END",
                    "CREATE TRIGGER scim_drift AFTER INSERT ON audit_log BEGIN UPDATE users SET external_id='changed' WHERE id='viewer-a'; END",
                ]{let s=setup(false).await;sqlx::query(trigger).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;let p=CurrentScimCredential::from_headers(&headers()).unwrap();assert!(mutate_scim_user(&s,&p,"viewer-a",change(mode)).await.is_err(),"{mode}:{trigger}");assert_eq!(move_database_snapshot(&s).await,before,"{mode}:{trigger}");}
                }
            }
            "wait" => {
                for mode in 0..3 {
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV, "isolated-scim-test");
                    let s = setup(true).await;
                    let p = CurrentScimCredential::from_headers(&headers()).unwrap();
                    let mut blocker = s.db.begin().await.unwrap();
                    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut blocker)
                        .await
                        .unwrap();
                    let c = s.clone();
                    let mut task = tokio::spawn(async move {
                        mutate_scim_user(&c, &p, "viewer-a", change(mode)).await
                    });
                    assert!(
                        tokio::time::timeout(std::time::Duration::from_millis(75), &mut task)
                            .await
                            .is_err()
                    );
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV, "rotated");
                    blocker.commit().await.unwrap();
                    let before = move_database_snapshot(&s).await;
                    assert!(matches!(task.await.unwrap(), Err(ScimError::Unauthorized)));
                    assert_eq!(move_database_snapshot(&s).await, before);
                }
            }
            "final" => {
                for mode in 0..3 {
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV, "isolated-scim-test");
                    let s = setup(false).await;
                    let p = CurrentScimCredential::from_headers(&headers()).unwrap();
                    let before = move_database_snapshot(&s).await;
                    let hook: Box<dyn FnOnce() + Send> =
                        Box::new(|| std::env::set_var(SCIM_BEARER_TOKEN_ENV, "final-rotation"));
                    let result = SCIM_USER_FINAL_HOOK
                        .scope(
                            std::cell::RefCell::new(Some(hook)),
                            mutate_scim_user(&s, &p, "viewer-a", change(mode)),
                        )
                        .await;
                    assert!(matches!(result, Err(ScimError::Unauthorized)));
                    assert_eq!(move_database_snapshot(&s).await, before);
                }
            }
            "idp" => {
                let s = setup(false).await;
                sqlx::query(
                    "UPDATE users SET role='admin',external_id='last-admin' WHERE id='owner-a'",
                )
                .execute(&s.db)
                .await
                .unwrap();
                sqlx::query("UPDATE users SET role='member' WHERE id!='owner-a'")
                    .execute(&s.db)
                    .await
                    .unwrap();
                let p = CurrentScimCredential::from_headers(&headers()).unwrap();
                mutate_scim_user(&s, &p, "owner-a", ScimUserChange::Deactivate)
                    .await
                    .unwrap();
                let row = scim_mutation_user(&mut *s.db.acquire().await.unwrap(), "owner-a")
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(row.role, "admin");
                assert_eq!(row.is_active, 0);
                // PATCH writes only its requested field, preserving legacy roles/profile.
                sqlx::query("CREATE TRIGGER scim_no_role BEFORE UPDATE OF role ON users BEGIN SELECT RAISE(ABORT,'role rewritten'); END").execute(&s.db).await.unwrap();
                mutate_scim_user(
                    &s,
                    &p,
                    "viewer-a",
                    ScimUserChange::Patch(ScimUserPatch::Active(true)),
                )
                .await
                .unwrap();
            }
            _ => panic!("unknown child case"),
        }
    }
    // Each environment-changing case runs alone in a child process. The parent
    // test process and other suites never observe the SCIM secret/config changes.
    fn subprocess(case: &str) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "scim::current_user_tests::scim_current_user_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("STARLANE_SCIM_CHILD_CASE", case)
            .env(SCIM_BEARER_TOKEN_ENV, "isolated-scim-test")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout).to_string()
                + &String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn scim_current_user_http_wire_and_audit() {
        subprocess("wire");
    }
    #[test]
    fn scim_current_user_http_denials_no_writes() {
        subprocess("denial");
    }
    #[test]
    fn scim_current_user_late_sql_and_audit_rollback() {
        subprocess("rollback");
    }
    #[test]
    fn scim_current_user_real_wal_fence_rotation() {
        subprocess("wait");
    }
    #[test]
    fn scim_current_user_final_token_rotation_rollback() {
        subprocess("final");
    }
    #[test]
    fn scim_current_user_preserves_identity_provider_lifecycle() {
        subprocess("idp");
    }
    #[test]
    #[ignore]
    fn scim_current_user_child() {
        let case = std::env::var("STARLANE_SCIM_CHILD_CASE").expect("subprocess only");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .unwrap()
            .block_on(run_case(&case));
    }
}
#[cfg(test)]
mod current_group_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        let s = crate::spaces::routes::space_creation_fence_tests::setup(wal).await;
        for q in ["INSERT INTO groups(id,name,external_id,scim_managed) VALUES('scim-g','SCIM group','idp-group',1),('manual-g','Manual group',NULL,0)","INSERT INTO group_members(id,group_id,user_id) VALUES('initial-member','scim-g','viewer-a')","INSERT INTO group_global_roles(group_id,role_id) VALUES('scim-g','viewer')","INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('scim-grant','space-a','scim-g','viewer')","INSERT INTO documents(id,space_id,title,content_path,created_by,last_updated_by) VALUES('page-a','space-a','Page','space-a/docs/page-a.md','owner-a','owner-a')","INSERT INTO space_roles(id,space_id,name,permissions,created_by) VALUES('scim-role','space-b','Custom SCIM role','[\"view\"]','owner-a')","INSERT INTO space_group_permissions(id,space_id,group_id,role) VALUES('scim-custom-grant','space-b','scim-g','scim-role')","INSERT INTO document_access_policies(document_id,space_id,mode,revision,created_by) VALUES('page-a','space-a','restricted','scim-policy','owner-a')","INSERT INTO document_access_allowlist(id,document_id,space_id,group_id,created_by) VALUES('scim-allow','page-a','space-a','scim-g','owner-a')","INSERT INTO document_access_allowlist(id,document_id,space_id,user_id,created_by) VALUES('scim-keep','page-a','space-a','owner-a','owner-a')"]{sqlx::query(q).execute(&s.db).await.unwrap();}
        sqlx::query("UPDATE documents SET maintenance_group_id='scim-g' WHERE id='page-a'")
            .execute(&s.db)
            .await
            .unwrap();
        s
    }
    fn proof() -> CurrentScimCredential {
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer isolated-scim-group"),
        );
        CurrentScimCredential::from_headers(&h).unwrap()
    }
    fn normalized() -> ScimNormalizedGroup {
        ScimNormalizedGroup {
            name: "Changed".into(),
            description: Some("Description".into()),
            members: vec!["owner-a".into(), "editor-a".into()],
            external_id: Some("idp-group".into()),
        }
    }
    fn change(mode: usize) -> ScimGroupChange {
        match mode {
            0 => ScimGroupChange::Create(ScimNormalizedGroup {
                name: "Created".into(),
                description: None,
                members: vec!["editor-a".into()],
                external_id: None,
            }),
            1 => ScimGroupChange::Replace(normalized()),
            2 => ScimGroupChange::Members(vec!["editor-a".into()]),
            _ => ScimGroupChange::Delete,
        }
    }
    async fn request(
        s: &AppState,
        method: &str,
        id: Option<&str>,
        body: Value,
        token: &str,
    ) -> axum::response::Response {
        let uri = id
            .map(|id| format!("/Groups/{id}"))
            .unwrap_or("/Groups".into());
        router()
            .with_state(s.clone())
            .oneshot(
                axum::http::Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(AUTHORIZATION, format!("Bearer {token}"))
                    .header("Content-Type", "application/json")
                    .body(axum::body::Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }
    async fn run_case(case: &str) {
        match case {
            "wire" => {
                let s = setup(false).await;
                let r=request(&s,"POST",None,json!({"displayName":" Created ","description":" ","externalId":" new-idp ","members":[{"value":"editor-a"}]}),"isolated-scim-group").await;
                assert_eq!(r.status(), StatusCode::CREATED);
                let location = r.headers()[LOCATION].to_str().unwrap().to_string();
                let body: Value = serde_json::from_slice(
                    &axum::body::to_bytes(r.into_body(), 10240).await.unwrap(),
                )
                .unwrap();
                assert_eq!(body["displayName"], "Created");
                assert_eq!(body["description"], Value::Null);
                assert_eq!(body["externalId"], "new-idp");
                assert!(location.ends_with(body["id"].as_str().unwrap()));
                assert_eq!(body["members"][0]["value"], "editor-a");
                let r=request(&s,"PUT",Some("scim-g"),json!({"displayName":" Replaced ","externalId":"idp-group","members":[{"value":"owner-a"}]}),"isolated-scim-group").await;
                assert_eq!(r.status(), StatusCode::OK);
                assert_eq!(request(&s,"PATCH",Some("scim-g"),json!({"schemas":["urn:ietf:params:scim:api:messages:2.0:PatchOp"],"operations":[{"op":"replace","path":"members","value":[{"value":"editor-a"}]}]}),"isolated-scim-group").await.status(),StatusCode::NO_CONTENT);
                assert_eq!(
                    request(
                        &s,
                        "DELETE",
                        Some("scim-g"),
                        json!({}),
                        "isolated-scim-group"
                    )
                    .await
                    .status(),
                    StatusCode::NO_CONTENT
                );
                let rows:Vec<(Option<String>,String,String)>=sqlx::query_as("SELECT user_id,actor_kind,actor_id FROM audit_log WHERE action LIKE 'group.scim_%'").fetch_all(&s.db).await.unwrap();
                assert_eq!(rows.len(), 4);
                assert!(rows
                    .iter()
                    .all(|r| r.0.is_none() && r.1 == "scim_token" && r.2 == "SCIM_BEARER_TOKEN"));
            }
            "denial" => {
                let s = setup(false).await;
                let before = move_database_snapshot(&s).await;
                for mode in 0..4 {
                    let id = if mode == 0 { "new-group" } else { "scim-g" };
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV, "isolated-scim-group");
                    let p = proof();
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV, "rotated");
                    assert!(matches!(
                        mutate_scim_group(&s, &p, id, change(mode)).await,
                        Err(ScimError::Unauthorized)
                    ));
                }
                std::env::set_var(SCIM_BEARER_TOKEN_ENV, "isolated-scim-group");
                for mode in 1..4 {
                    for id in ["manual-g", "missing"] {
                        assert!(matches!(
                            mutate_scim_group(&s, &proof(), id, change(mode)).await,
                            Err(ScimError::NotFound)
                        ));
                    }
                }
                let mut r = normalized();
                r.external_id = None;
                assert!(matches!(
                    mutate_scim_group(&s, &proof(), "scim-g", ScimGroupChange::Replace(r)).await,
                    Err(ScimError::InvalidResource)
                ));
                assert_eq!(move_database_snapshot(&s).await, before);
            }
            "replacement" => {
                let s = setup(false).await;
                sqlx::query("UPDATE groups SET managed_space_id='space-a' WHERE id='scim-g'")
                    .execute(&s.db)
                    .await
                    .unwrap();
                sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
                    .execute(&s.db)
                    .await
                    .unwrap();
                let result = mutate_scim_group(
                    &s,
                    &proof(),
                    "scim-g",
                    ScimGroupChange::Replace(normalized()),
                )
                .await
                .unwrap()
                .unwrap();
                assert_eq!(result["members"].as_array().unwrap().len(), 2);
                let deps = scim_group_dependencies(&mut *s.db.acquire().await.unwrap(), "scim-g")
                    .await
                    .unwrap();
                let source =
                    full_scim_group_source(&mut *s.db.acquire().await.unwrap(), "scim-g", &deps)
                        .await
                        .unwrap();
                assert_eq!(source.grants.len(), 2);
                assert_eq!(source.roles.len(), 1);
                assert_eq!(source.allowlist.len(), 2);
                mutate_scim_group(&s, &proof(), "scim-g", ScimGroupChange::Members(vec![]))
                    .await
                    .unwrap();
                let count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM group_members WHERE group_id='scim-g'",
                )
                .fetch_one(&s.db)
                .await
                .unwrap();
                assert_eq!(count, 0);
            }
            "creation" => {
                for table in ["groups", "group_members", "audit_log"] {
                    let s = setup(false).await;
                    sqlx::query(&format!("CREATE TRIGGER fail_scim AFTER INSERT ON {table} BEGIN SELECT RAISE(ABORT,'constituent failure'); END")).execute(&s.db).await.unwrap();
                    let before = move_database_snapshot(&s).await;
                    assert!(mutate_scim_group(&s, &proof(), "new-scim", change(0))
                        .await
                        .is_err());
                    assert_eq!(move_database_snapshot(&s).await, before);
                }
                let s = setup(false).await;
                let before = move_database_snapshot(&s).await;
                let mut r = normalized();
                r.name = "SCIM group".into();
                assert!(matches!(
                    mutate_scim_group(&s, &proof(), "new-scim", ScimGroupChange::Create(r)).await,
                    Err(ScimError::Conflict)
                ));
                assert!(matches!(
                    mutate_scim_group(
                        &s,
                        &proof(),
                        "scim-g",
                        ScimGroupChange::Members(vec!["missing".into()])
                    )
                    .await,
                    Err(ScimError::InvalidResource)
                ));
                assert_eq!(move_database_snapshot(&s).await, before);
            }
            "cascade" => {
                let s = setup(false).await;
                let stamp: String =
                    sqlx::query_scalar("SELECT updated_at FROM documents WHERE id='page-a'")
                        .fetch_one(&s.db)
                        .await
                        .unwrap();
                mutate_scim_group(&s, &proof(), "scim-g", ScimGroupChange::Delete)
                    .await
                    .unwrap();
                let pair: (Option<String>, String) = sqlx::query_as(
                    "SELECT maintenance_group_id,updated_at FROM documents WHERE id='page-a'",
                )
                .fetch_one(&s.db)
                .await
                .unwrap();
                assert_eq!(pair, (None, stamp));
                for q in [
                    "SELECT COUNT(*) FROM groups WHERE id='scim-g'",
                    "SELECT COUNT(*) FROM group_members WHERE group_id='scim-g'",
                    "SELECT COUNT(*) FROM group_global_roles WHERE group_id='scim-g'",
                    "SELECT COUNT(*) FROM space_group_permissions WHERE group_id='scim-g'",
                    "SELECT COUNT(*) FROM document_access_allowlist WHERE group_id='scim-g'",
                ] {
                    let n: i64 = sqlx::query_scalar(q).fetch_one(&s.db).await.unwrap();
                    assert_eq!(n, 0);
                }
                let kept: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM document_access_allowlist WHERE id='scim-keep'",
                )
                .fetch_one(&s.db)
                .await
                .unwrap();
                assert_eq!(kept, 1);
                let policy: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM document_access_policies WHERE revision='scim-policy'",
                )
                .fetch_one(&s.db)
                .await
                .unwrap();
                assert_eq!(policy, 1);
                for trigger in ["CREATE TRIGGER scim_delete_drift AFTER DELETE ON groups BEGIN UPDATE space_roles SET permissions='[]' WHERE id='scim-role'; END","CREATE TRIGGER scim_delete_drift AFTER DELETE ON groups BEGIN UPDATE global_roles SET permissions='[]' WHERE id='viewer'; END","CREATE TRIGGER scim_delete_drift AFTER DELETE ON groups BEGIN UPDATE document_access_policies SET revision='changed' WHERE document_id='page-a'; END","CREATE TRIGGER scim_delete_drift AFTER DELETE ON groups BEGIN DELETE FROM document_access_allowlist WHERE id='scim-keep'; END"]{let s=setup(false).await;sqlx::query(trigger).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;assert!(mutate_scim_group(&s,&proof(),"scim-g",change(3)).await.is_err());assert_eq!(move_database_snapshot(&s).await,before);}
            }
            "rollback" => {
                for mode in 0..4 {
                    for trigger in ["CREATE TRIGGER scim_drift AFTER INSERT ON audit_log BEGIN UPDATE groups SET external_id='changed' WHERE id IN('scim-g','new-scim'); UPDATE global_roles SET permissions='[]' WHERE id='viewer'; END","CREATE TRIGGER scim_drift AFTER INSERT ON audit_log BEGIN UPDATE audit_log SET before_value='changed' WHERE id=NEW.id; END","CREATE TRIGGER scim_drift AFTER INSERT ON audit_log BEGIN UPDATE users SET display_name='changed' WHERE id IN('viewer-a','editor-a'); END","CREATE TRIGGER scim_drift AFTER INSERT ON audit_log BEGIN SELECT RAISE(ABORT,'audit failure'); END"]{let s=setup(false).await;sqlx::query(trigger).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;assert!(mutate_scim_group(&s,&proof(),if mode==0{"new-scim"}else{"scim-g"},change(mode)).await.is_err(),"{mode}:{trigger}");assert_eq!(move_database_snapshot(&s).await,before,"{mode}:{trigger}");}
                }
            }
            "wait" => {
                for mode in 0..4 {
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV, "isolated-scim-group");
                    let s = setup(true).await;
                    let p = proof();
                    let mut blocker = s.db.begin().await.unwrap();
                    crate::spaces::mutation_guards::lock_audience_for_mutation(&mut blocker)
                        .await
                        .unwrap();
                    let c = s.clone();
                    let mut task = tokio::spawn(async move {
                        mutate_scim_group(
                            &c,
                            &p,
                            if mode == 0 { "new-scim" } else { "scim-g" },
                            change(mode),
                        )
                        .await
                    });
                    assert!(
                        tokio::time::timeout(std::time::Duration::from_millis(75), &mut task)
                            .await
                            .is_err()
                    );
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV, "rotated");
                    blocker.commit().await.unwrap();
                    let before = move_database_snapshot(&s).await;
                    assert!(matches!(task.await.unwrap(), Err(ScimError::Unauthorized)));
                    assert_eq!(move_database_snapshot(&s).await, before);
                }
            }
            "final" => {
                for mode in 0..4 {
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV, "isolated-scim-group");
                    let s = setup(false).await;
                    let before = move_database_snapshot(&s).await;
                    let hook: Box<dyn FnOnce() + Send> =
                        Box::new(|| std::env::set_var(SCIM_BEARER_TOKEN_ENV, "rotated"));
                    let result = SCIM_GROUP_FINAL_HOOK
                        .scope(
                            std::cell::RefCell::new(Some(hook)),
                            mutate_scim_group(
                                &s,
                                &proof(),
                                if mode == 0 { "new-scim" } else { "scim-g" },
                                change(mode),
                            ),
                        )
                        .await;
                    assert!(matches!(result, Err(ScimError::Unauthorized)));
                    assert_eq!(move_database_snapshot(&s).await, before);
                }
            }
            _ => panic!("unknown child case"),
        }
    }
    fn subprocess(case: &str) {
        let o = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "scim::current_group_tests::scim_current_group_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("STARLANE_SCIM_GROUP_CHILD_CASE", case)
            .env(SCIM_BEARER_TOKEN_ENV, "isolated-scim-group")
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{}",
            String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr)
        );
    }
    #[test]
    fn scim_current_group_http_wire_and_audit() {
        subprocess("wire");
    }
    #[test]
    fn scim_current_group_denials_no_writes() {
        subprocess("denial");
    }
    #[test]
    fn scim_current_group_full_replace_inactive_members() {
        subprocess("replacement");
    }
    #[test]
    fn scim_current_group_creation_constituent_rollback() {
        subprocess("creation");
    }
    #[test]
    fn scim_current_group_delete_fixed_dependency_proof() {
        subprocess("cascade");
    }
    #[test]
    fn scim_current_group_late_audit_and_source_rollback() {
        subprocess("rollback");
    }
    #[test]
    fn scim_current_group_actual_wal_fence_rotation() {
        subprocess("wait");
    }
    #[test]
    fn scim_current_group_final_token_rotation() {
        subprocess("final");
    }
    #[test]
    #[ignore]
    fn scim_current_group_child() {
        let case = std::env::var("STARLANE_SCIM_GROUP_CHILD_CASE").expect("subprocess only");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .unwrap()
            .block_on(run_case(&case));
    }
}
#[cfg(test)]
mod account_atomic_tests {
    use super::*;
    use crate::spaces::documents::tests::move_database_snapshot;
    use tower::ServiceExt;
    async fn setup(wal: bool) -> AppState {
        crate::spaces::routes::space_creation_fence_tests::setup(wal).await
    }
    fn headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_static("Bearer isolated-scim-account"),
        );
        h
    }
    fn input(active: Option<bool>) -> ScimUserCreateRequest {
        ScimUserCreateRequest {
            user_name: " ATOMIC@SCIM.TEST ".into(),
            display_name: Some(" Atomic Person ".into()),
            active,
            external_id: Some(" idp-atomic ".into()),
        }
    }
    async fn create(
        s: &AppState,
        active: Option<bool>,
    ) -> Result<(StatusCode, HeaderMap, Json<Value>), ScimError> {
        create_user(State(s.clone()), headers(), Json(input(active))).await
    }
    fn disk() -> Vec<std::path::PathBuf> {
        fn walk(root: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(root)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
            {
                let path = entry.path();
                out.push(path.clone());
                if std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
                    walk(&path, out);
                }
            }
        }
        let mut rows = Vec::new();
        walk(&crate::storage::data_dir(), &mut rows);
        rows.sort();
        rows
    }
    async fn directory(s: &AppState, uri: &str) -> axum::response::Response {
        router().with_state(s.clone()).oneshot(axum::http::Request::builder()
            .uri(uri).header(AUTHORIZATION,"Bearer isolated-scim-account")
            .body(axum::body::Body::empty()).unwrap()).await.unwrap()
    }
    async fn directory_cases(case: &str) {
        let s = setup(false).await;
        sqlx::query("INSERT INTO groups(id,name,external_id) VALUES('directory-g','Directory group','directory-external')").execute(&s.db).await.unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('directory-member','directory-g','viewer-a')").execute(&s.db).await.unwrap();
        let before = move_database_snapshot(&s).await;
        let routes = ["/Users", "/Users/viewer-a", "/Groups", "/Groups/directory-g"];
        for uri in routes {
            std::env::set_var(SCIM_BEARER_TOKEN_ENV,"isolated-scim-account");
            let r = match case {
                "directory-final" => {
                    let hook: Box<dyn FnOnce()+Send> = Box::new(|| std::env::set_var(SCIM_BEARER_TOKEN_ENV,"rotated"));
                    SCIM_DIRECTORY_FINAL_HOOK.scope(std::cell::RefCell::new(Some(hook)),directory(&s,uri)).await
                },
                "directory-wait" => {
                    // The fixture has exactly one connection: real acquisition wait.
                    let held = s.db.acquire().await.unwrap();
                    let cloned=s.clone(); let target=uri.to_string();
                    let mut task=tokio::spawn(async move { directory(&cloned,&target).await });
                    assert!(tokio::time::timeout(std::time::Duration::from_millis(75),&mut task).await.is_err());
                    std::env::set_var(SCIM_BEARER_TOKEN_ENV,"rotated"); drop(held);
                    task.await.unwrap()
                },
                _ => directory(&s,uri).await,
            };
            let expected=if case=="directory-wire" {StatusCode::OK} else {StatusCode::UNAUTHORIZED};
            assert_eq!(r.status(),expected,"{uri}");
            let body:Value=serde_json::from_slice(&axum::body::to_bytes(r.into_body(),65536).await.unwrap()).unwrap();
            assert!(!body.to_string().contains("isolated-scim-account"));
            if case!="directory-wire" {assert_eq!(body["status"],"401");assert!(body.get("Resources").is_none());assert!(body.get("members").is_none());}
            else if uri=="/Groups/directory-g" {assert_eq!(body["externalId"],"directory-external");assert_eq!(body["members"][0]["value"],"viewer-a");}
            else if uri=="/Users/viewer-a" {assert_eq!(body["id"],"viewer-a");}
        }
        std::env::set_var(SCIM_BEARER_TOKEN_ENV,"isolated-scim-account");
        for uri in ["/Users?filter=userName%20eq%20%22viewer-a%40test.local%22","/Users?filter=externalId%20eq%20%22absent%22","/Users?count=0","/Groups?count=0"] {
            assert_eq!(directory(&s,uri).await.status(),StatusCode::OK,"{uri}");
        }
        assert_eq!(directory(&s,"/Users/missing").await.status(),StatusCode::NOT_FOUND);
        assert_eq!(directory(&s,"/Groups/missing").await.status(),StatusCode::NOT_FOUND);
        assert_eq!(move_database_snapshot(&s).await,before);
    }
    async fn run_case(case: &str) {
        if case.starts_with("directory-") { directory_cases(case).await; return; }
        match case{
 "wire"=>{let s=setup(false).await;let response=router().with_state(s.clone()).oneshot(axum::http::Request::builder().method("POST").uri("/Users").header(AUTHORIZATION,"Bearer isolated-scim-account").header("Content-Type","application/json").body(axum::body::Body::from(json!({"userName":" ATOMIC@SCIM.TEST ","displayName":" Atomic Person ","externalId":" idp-atomic "}).to_string())).unwrap()).await.unwrap();assert_eq!(response.status(),StatusCode::CREATED);let location=response.headers()[LOCATION].to_str().unwrap().to_string();let raw=axum::body::to_bytes(response.into_body(),16384).await.unwrap();let body:Value=serde_json::from_slice(&raw).unwrap();assert!(!String::from_utf8_lossy(&raw).contains("isolated-scim-account"));let id=body["id"].as_str().unwrap();assert_eq!(location,format!("/scim/v2/Users/{id}"));assert_eq!(body["userName"],"atomic@scim.test");assert_eq!(body["displayName"],"Atomic Person");assert_eq!(body["externalId"],"idp-atomic");assert_eq!(body["active"],true);assert!(body.get("password_hash").is_none());
 let user=scim_mutation_user(&mut *s.db.acquire().await.unwrap(),id).await.unwrap().unwrap();assert_eq!(user.role,"member");assert!(user.password_hash.is_none());assert!(user.avatar_url.is_none());assert_eq!(body["meta"]["created"],user.created_at);assert_eq!(body["meta"]["lastModified"],user.updated_at);
 let space:(String,String,String)=sqlx::query_as("SELECT id,slug,visibility FROM spaces WHERE created_by=$1").bind(id).fetch_one(&s.db).await.unwrap();assert_eq!(space.2,"internal");let topology:(i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT COUNT(*) FROM space_members WHERE space_id=$1 AND user_id=$2 AND role='owner'),(SELECT COUNT(*) FROM groups WHERE managed_space_id=$1),(SELECT COUNT(*) FROM group_members m JOIN groups g ON g.id=m.group_id WHERE g.managed_space_id=$1 AND m.user_id=$2),(SELECT COUNT(*) FROM space_group_permissions p JOIN groups g ON g.id=p.group_id WHERE g.managed_space_id=$1 AND p.space_id=$1 AND p.role='owner'),(SELECT COUNT(*) FROM group_global_roles r JOIN groups g ON g.id=r.group_id WHERE g.managed_space_id=$1)").bind(&space.0).bind(id).fetch_one(&s.db).await.unwrap();assert_eq!(topology,(1,1,1,1,0));assert!(crate::storage::space_dir(&space.1).is_dir());
 let events:Vec<(String,String,String,Option<String>)>=sqlx::query_as("SELECT action,actor_kind,actor_id,user_id FROM audit_log WHERE actor_kind='scim_token' ORDER BY action").fetch_all(&s.db).await.unwrap();assert_eq!(events.len(),2);assert_eq!(events[0].0,"space.personal_create");assert_eq!(events[1].0,"user.scim_create");assert!(events.iter().all(|r|r.1=="scim_token"&&r.2=="SCIM_BEARER_TOKEN"&&r.3.is_none()));
 },
 "inactive"=>{let s=setup(false).await;let (_,_,Json(body))=create(&s,Some(false)).await.unwrap();assert_eq!(body["active"],false);let id=body["id"].as_str().unwrap();let row=scim_mutation_user(&mut *s.db.acquire().await.unwrap(),id).await.unwrap().unwrap();assert_eq!(row.is_active,0);let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM spaces WHERE created_by=$1").bind(id).fetch_one(&s.db).await.unwrap();assert_eq!(count,1);},
 "denial"=>{let s=setup(false).await;let before=move_database_snapshot(&s).await;let mut bad=input(None);bad.user_name="invalid".into();assert!(matches!(create_user(State(s.clone()),headers(),Json(bad)).await,Err(ScimError::InvalidResource)));let mut h=headers();h.insert(AUTHORIZATION,HeaderValue::from_static("Bearer wrong"));assert!(matches!(create_user(State(s.clone()),h,Json(input(None))).await,Err(ScimError::Unauthorized)));assert_eq!(move_database_snapshot(&s).await,before);
 let _=create(&s,None).await.unwrap();let before=move_database_snapshot(&s).await;for changed_email in [false,true]{let mut req=input(None);if changed_email{req.user_name="different@scim.test".into();}assert!(matches!(create_user(State(s.clone()),headers(),Json(req)).await,Err(ScimError::Conflict)));assert_eq!(move_database_snapshot(&s).await,before);}
 },
 "constituents"=>{for table in ["users","spaces","space_members","groups","group_members","space_group_permissions","audit_log"]{let s=setup(false).await;sqlx::query(&format!("CREATE TRIGGER fail_account AFTER INSERT ON {table} BEGIN SELECT RAISE(ABORT,'constituent failure'); END")).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;let before_disk=disk();assert!(create(&s,None).await.is_err(),"{table}");assert_eq!(move_database_snapshot(&s).await,before,"{table}");assert_eq!(disk(),before_disk,"files written before commit: {table}");}},
 "late"=>{for trigger in [
 "CREATE TRIGGER account_drift AFTER INSERT ON users BEGIN UPDATE users SET role='admin',password_hash='unexpected' WHERE id=NEW.id; END",
 "CREATE TRIGGER account_drift AFTER INSERT ON spaces BEGIN UPDATE spaces SET slug='unexpected',visibility='public' WHERE id=NEW.id; END",
 "CREATE TRIGGER account_drift AFTER INSERT ON space_members BEGIN UPDATE space_members SET role='viewer' WHERE id=NEW.id; END",
 "CREATE TRIGGER account_drift AFTER INSERT ON groups BEGIN UPDATE groups SET external_id='unexpected' WHERE id=NEW.id; END",
 "CREATE TRIGGER account_drift AFTER INSERT ON group_members BEGIN INSERT INTO group_members(id,group_id,user_id,created_at) VALUES('extra-person',NEW.group_id,'editor-a',NEW.created_at); END",
 "CREATE TRIGGER account_drift AFTER INSERT ON space_group_permissions BEGIN INSERT INTO group_global_roles(group_id,role_id) VALUES(NEW.group_id,'owner'); END",
 "CREATE TRIGGER account_drift AFTER INSERT ON audit_log BEGIN UPDATE audit_log SET before_value='unexpected' WHERE id=NEW.id; END",
 "CREATE TRIGGER account_drift AFTER INSERT ON audit_log BEGIN UPDATE users SET external_id='unexpected' WHERE email='atomic@scim.test'; END",
 "CREATE TRIGGER account_drift AFTER INSERT ON audit_log BEGIN INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) SELECT 'unexpected-session',id,'unexpected','2999-01-01' FROM users WHERE email='atomic@scim.test' ON CONFLICT(id) DO NOTHING; END",
 ]{let s=setup(false).await;sqlx::query(trigger).execute(&s.db).await.unwrap();let before=move_database_snapshot(&s).await;assert!(create(&s,None).await.is_err(),"{trigger}");assert_eq!(move_database_snapshot(&s).await,before,"{trigger}");}},
 "wait"=>{let s=setup(true).await;let mut blocker=s.db.begin().await.unwrap();crate::spaces::mutation_guards::lock_audience_for_mutation(&mut blocker).await.unwrap();let c=s.clone();let mut task=tokio::spawn(async move{create(&c,None).await});assert!(tokio::time::timeout(std::time::Duration::from_millis(75),&mut task).await.is_err());std::env::set_var(SCIM_BEARER_TOKEN_ENV,"rotated");blocker.commit().await.unwrap();let before=move_database_snapshot(&s).await;assert!(matches!(task.await.unwrap(),Err(ScimError::Unauthorized)));assert_eq!(move_database_snapshot(&s).await,before);},
 "final"=>{let s=setup(false).await;let before=move_database_snapshot(&s).await;let hook:Box<dyn FnOnce()+Send>=Box::new(||std::env::set_var(SCIM_BEARER_TOKEN_ENV,"rotated"));let result=SCIM_ACCOUNT_FINAL_HOOK.scope(std::cell::RefCell::new(Some(hook)),create(&s,None)).await;assert!(matches!(result,Err(ScimError::Unauthorized)));assert_eq!(move_database_snapshot(&s).await,before);},
 "legacy"=>{let s=setup(false).await;for _ in 0..2{crate::auth::routes::create_personal_space(&s,"viewer-a","Same Name").await.unwrap();}let rows:Vec<(String,String)>=sqlx::query_as("SELECT slug,visibility FROM spaces WHERE created_by='viewer-a' ORDER BY slug").fetch_all(&s.db).await.unwrap();assert_eq!(rows,vec![("personal-viewer-a".into(),"internal".into()),("same-names-space".into(),"internal".into())]);let before=move_database_snapshot(&s).await;assert!(crate::auth::routes::create_personal_space(&s,"viewer-a","Same Name").await.is_err());assert_eq!(move_database_snapshot(&s).await,before);
 let (_,_,Json(body))=create(&s,None).await.unwrap();let id=body["id"].as_str().unwrap();let expected_slug="atomic-persons-space";let slug:String=sqlx::query_scalar("SELECT slug FROM spaces WHERE created_by=$1").bind(id).fetch_one(&s.db).await.unwrap();assert_eq!(slug,expected_slug);
 },_=>panic!("unknown child case"),}
    }
    fn subprocess(case: &str) {
        let o = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "scim::account_atomic_tests::scim_account_atomic_child",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("STARLANE_SCIM_ACCOUNT_CHILD_CASE", case)
            .env(
                "DATA_DIR",
                std::env::temp_dir().join(format!("scim-account-child-{}", uuid::Uuid::new_v4())),
            )
            .env(SCIM_BEARER_TOKEN_ENV, "isolated-scim-account")
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{}",
            String::from_utf8_lossy(&o.stdout).to_string() + &String::from_utf8_lossy(&o.stderr)
        );
    }
    #[test]
    fn scim_account_atomic_http_topology_and_configured_actor() {
        subprocess("wire");
    }
    #[test]
    fn scim_account_atomic_inactive_creation_retained() {
        subprocess("inactive");
    }
    #[test]
    fn scim_account_atomic_duplicate_identity_and_denials() {
        subprocess("denial");
    }
    #[test]
    fn scim_account_atomic_each_constituent_failure_no_orphans_or_files() {
        subprocess("constituents");
    }
    #[test]
    fn scim_account_atomic_late_source_audit_and_grant_drift() {
        subprocess("late");
    }
    #[test]
    fn scim_account_atomic_real_wal_fence_rotation() {
        subprocess("wait");
    }
    #[test]
    fn scim_account_atomic_final_token_rotation_rollback() {
        subprocess("final");
    }
    #[test]
    fn scim_account_atomic_standalone_existing_user_slug_compatibility() {
        subprocess("legacy");
    }
    #[test]
    fn scim_directory_wire_filters_and_no_writes() { subprocess("directory-wire"); }
    #[test]
    fn scim_directory_final_token_rotation_denies_all_four_routes() { subprocess("directory-final"); }
    #[test]
    fn scim_directory_real_connection_wait_rotation_denies_all_four_routes() { subprocess("directory-wait"); }
    #[test]
    #[ignore]
    fn scim_account_atomic_child() {
        let case = std::env::var("STARLANE_SCIM_ACCOUNT_CHILD_CASE").expect("subprocess only");
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .unwrap()
            .block_on(run_case(&case));
    }
}

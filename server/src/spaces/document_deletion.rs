//! Shared SQL-only harddelete domain. Callers own audience/space guards, transaction,
//! original credential LAST and commit. No receipt, provider or filesystem authority.
use super::access::{self, PageOperation, Permission, Purpose};
use crate::auth::{middleware::AuthUser, routes::AppError};
use serde_json::Value;
use sha2::{Digest, Sha256};
#[derive(Clone)]
struct DeleteProof {
    query: String,
    parameters: Vec<String>,
    rows: Vec<Value>,
}
pub(crate) async fn rows(
    conn: &mut sqlx::AnyConnection,
    sql: &str,
    args: &[&str],
) -> Result<Vec<Value>, String> {
    use sqlx::{Column, Row};
    let mut q = sqlx::query(sql);
    for arg in args {
        q = q.bind(*arg);
    }
    let mut result = Vec::new();
    for row in q.fetch_all(conn).await.map_err(|e| e.to_string())? {
        let mut fields = serde_json::Map::new();
        for col in row.columns() {
            let name = col.name();
            let v = if let Ok(v) = row.try_get::<Option<String>, _>(name) {
                v.map(Value::String).unwrap_or(Value::Null)
            } else if let Ok(v) = row.try_get::<Option<i64>, _>(name) {
                v.map(|v| serde_json::json!(v)).unwrap_or(Value::Null)
            } else if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(name) {
                v.map(|v| serde_json::json!({"binary_hex":hex::encode(v)}))
                    .unwrap_or(Value::Null)
            } else {
                return Err("Cannot capture deletion source; retain queued deletion".into());
            };
            fields.insert(name.into(), v);
        }
        result.push(Value::Object(fields));
    }
    result.sort_by_cached_key(Value::to_string);
    Ok(result)
}
const DELETE_CASCADE_TABLES: &[&str] = &[
    "attachments",
    "comments",
    "doc_subscriptions",
    "doc_tags",
    "doc_versions",
    "document_access_allowlist",
    "document_access_policies",
    "document_viewer_grants",
    "document_drafts",
    "document_workflow_events",
    "document_workflows",
    "excalidraw_files",
    "property_values",
    "reactions",
    "starred_documents",
];
async fn delete_domain(
    conn: &mut sqlx::AnyConnection,
    space: &str,
    id: &str,
) -> Result<std::collections::BTreeMap<String, Vec<Value>>, String> {
    let mut data = std::collections::BTreeMap::new();
    data.insert(
        "documents".into(),
        rows(conn, "SELECT * FROM documents WHERE space_id=$1", &[space]).await?,
    );
    // SET NULL can change a child's inherited restriction; new/foreign child
    // edges are also part of the expected outcome, never a hidden side effect.
    data.insert(
        "hierarchy".into(),
        rows(conn, "SELECT id,space_id,parent_id FROM documents", &[]).await?,
    );
    for table in DELETE_CASCADE_TABLES {
        data.insert(
            (*table).into(),
            rows(
                conn,
                &format!("SELECT * FROM {table} WHERE document_id=$1"),
                &[id],
            )
            .await?,
        );
    }
    data.insert("document_draft_access".into(),rows(conn,"SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1",&[id]).await?);
    data.insert(
        "comment_cascade_edges".into(),
        rows(
            conn,
            "SELECT c.* FROM comments c JOIN comments p ON p.id=c.parent_id WHERE p.document_id=$1",
            &[id],
        )
        .await?,
    );
    data.insert("workflow_cascade_edges".into(),rows(conn,"SELECT e.* FROM document_workflow_events e JOIN document_workflows w ON w.id=e.document_workflow_id WHERE w.document_id=$1",&[id]).await?);

    data.insert("viewer_grants_all".into(),rows(conn,"SELECT * FROM document_viewer_grants",&[]).await?);
    data.insert("viewer_grant_recipients".into(),rows(conn,"SELECT id,role,is_active,account_kind FROM users",&[]).await?);
    Ok(data)
}
async fn delete_authority_proofs(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    ancestors: &[String],
) -> Result<Vec<DeleteProof>, String> {
    let mut specifications = vec![(
        "SELECT id,role,is_active,account_kind FROM users WHERE id=$1".to_owned(),
        vec![auth.claims.sub.clone()],
    )];
    for table in [
        "spaces",
        "space_members",
        "space_group_permissions",
        "space_user_blocks",
        "space_roles",
    ] {
        specifications.push((
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
        specifications.push((format!("SELECT * FROM {table}"), vec![]));
    }
    for id in ancestors {
        for table in [
            "documents",
            "document_drafts",
            "document_access_policies",
            "document_access_allowlist",
        ] {
            specifications.push((
                format!(
                    "SELECT * FROM {table} WHERE {}=$1",
                    if table == "documents" {
                        "id"
                    } else {
                        "document_id"
                    }
                ),
                vec![id.clone()],
            ));
        }
        specifications.push(("SELECT a.* FROM document_draft_access a JOIN document_drafts d ON d.id=a.draft_id WHERE d.document_id=$1".into(),vec![id.clone()]));
    }
    let mut result = Vec::new();
    for (query, parameters) in specifications {
        let args: Vec<&str> = parameters.iter().map(String::as_str).collect();
        let rows = rows(conn, &query, &args).await?;
        result.push(DeleteProof {
            query,
            parameters,
            rows,
        });
    }
    Ok(result)
}

fn unavailable() -> AppError {
    AppError::NotFound("Trashed document not found".into())
}
fn changed(message: &str) -> AppError {
    AppError::Conflict(message.into())
}
fn capture_error(error: String) -> AppError {
    AppError::internal(error)
}
pub(crate) struct PreparedDeletion {
    actor_id: String,
    space: String,
    id: String,
    operation: PageOperation,
    domain: std::collections::BTreeMap<String, Vec<Value>>,
    proofs: Vec<DeleteProof>,
    fingerprint: String,
    paths: Vec<String>,
}
impl PreparedDeletion {
    pub(crate) fn fingerprint(&self) -> &str {
        &self.fingerprint
    }
    pub(crate) fn retention_snapshot(&self) -> serde_json::Value { serde_json::json!({"actor_id":self.actor_id,"space_id":self.space,"document_id":self.id,"source_fingerprint":self.fingerprint,"domain":self.domain,"paths":self.paths}) }
    pub(crate) fn retained_paths(&self) -> &[String] {
        &self.paths
    }
}
/// Must follow caller-owned audience FIRST and ordered space guards.
pub(crate) async fn prepare(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    id: &str,
    required: Option<PageOperation>,
) -> Result<PreparedDeletion, AppError> {
    let principal = access::principal_from_auth(auth)?;
    let n = sqlx::query("UPDATE documents SET id=id WHERE id=$1 AND space_id=$2")
        .bind(&id)
        .bind(&space)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if n.rows_affected() != 1 {
        return Err(unavailable());
    }
    let source = rows(
        conn,
        "SELECT * FROM documents WHERE id=$1 AND space_id=$2",
        &[&id, &space],
    )
    .await
    .map_err(capture_error)?;
    let page = source.first().ok_or_else(unavailable)?;
    let op = if !page["deleted_at"].is_null() {
        PageOperation::DeleteTrashed
    } else if !page["archived_at"].is_null() {
        PageOperation::DeleteArchived
    } else {
        PageOperation::DeleteActive
    };
    if required.is_some_and(|wanted| wanted != op) {
        return Err(unavailable());
    }
    let authority =
        access::resolve_page_access(conn, principal, &space, &id, op, Purpose::Routine).await?;
    let child: Option<i64> =
        sqlx::query_scalar("SELECT 1 FROM documents WHERE parent_id=$1 LIMIT 1")
            .bind(&id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(AppError::internal)?;
    if child.is_some() {
        return Err(changed(
            "Document deletion has dependent hierarchy; retain queued deletion for review",
        ));
    }
    // Corrupt foreign associations cannot be erased by a nominally scoped cascade.
    for q in [
        "SELECT 1 FROM attachments WHERE document_id=$1 AND space_id<>$2",
        "SELECT 1 FROM excalidraw_files WHERE document_id=$1 AND space_id<>$2",
        "SELECT 1 FROM document_drafts WHERE document_id=$1 AND space_id<>$2",
        "SELECT 1 FROM document_access_policies WHERE document_id=$1 AND space_id<>$2",
        "SELECT 1 FROM document_access_allowlist WHERE document_id=$1 AND space_id<>$2",
        "SELECT 1 FROM doc_tags a LEFT JOIN tags d ON d.id=a.tag_id WHERE a.document_id=$1 AND (d.id IS NULL OR d.space_id<>$2)",
        "SELECT 1 FROM property_values a LEFT JOIN properties_schema d ON d.id=a.property_id WHERE a.document_id=$1 AND (d.id IS NULL OR d.space_id<>$2)",
        "SELECT 1 FROM document_workflows a LEFT JOIN workflow_templates d ON d.id=a.workflow_template_id WHERE a.document_id=$1 AND (d.id IS NULL OR d.space_id<>$2)",
        "SELECT 1 FROM comments child JOIN comments parent ON parent.id=child.parent_id WHERE parent.document_id=$1 AND child.document_id<>$1 AND $2<>''",
        "SELECT 1 FROM document_workflow_events e LEFT JOIN document_workflows w ON w.id=e.document_workflow_id WHERE (w.document_id=$1 AND e.document_id<>$1 OR e.document_id=$1 AND (w.id IS NULL OR w.document_id<>$1)) AND $2<>''",
    ] {if !rows(conn,q,&[id,space]).await.map_err(capture_error)?.is_empty(){return Err(changed("Document deletion source needs review; no canonical state changed"))}}
    for table in DELETE_CASCADE_TABLES {
        sqlx::query(&format!(
            "UPDATE {table} SET document_id=document_id WHERE document_id=$1"
        ))
        .bind(&id)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    }
    sqlx::query("UPDATE document_draft_access SET draft_id=draft_id WHERE draft_id IN(SELECT id FROM document_drafts WHERE document_id=$1)").bind(&id).execute(&mut *conn).await.map_err(AppError::internal)?;
    access::resolve_page_access(conn, principal, &space, &id, op, Purpose::Routine).await?;
    let domain = delete_domain(conn, space, id)
        .await
        .map_err(capture_error)?;
    if domain["documents"].iter().find(|v| v["id"] == id) != Some(page) {
        return Err(changed(
            "Document deletion source changed; retain queued deletion",
        ));
    }
    let proofs = delete_authority_proofs(conn, auth, space, &authority.ancestor_ids)
        .await
        .map_err(capture_error)?;
    if domain["comment_cascade_edges"]
        .iter()
        .any(|v| v["document_id"] != id)
        || domain["workflow_cascade_edges"]
            .iter()
            .any(|v| v["document_id"] != id)
    {
        return Err(changed(
            "Document deletion source needs review; no canonical state changed",
        ));
    }
    let fingerprint = hex::encode(Sha256::digest(
        serde_json::to_vec(&domain).map_err(AppError::internal)?,
    ));
    let mut paths = vec![page["content_path"]
        .as_str()
        .ok_or_else(|| changed("Deletion source path invalid"))?
        .to_owned()];
    for table in ["attachments", "excalidraw_files"] {
        for row in &domain[table] {
            paths.push(
                row["storage_path"]
                    .as_str()
                    .ok_or_else(|| changed("Deletion asset path invalid"))?
                    .into(),
            );
        }
    }
    paths.sort();
    paths.dedup();
    Ok(PreparedDeletion {
        actor_id: auth.claims.sub.clone(),
        space: space.into(),
        id: id.into(),
        operation: op,
        domain,
        proofs,
        fingerprint,
        paths,
    })
}

pub(crate) async fn delete_prepared(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    prepared: &PreparedDeletion,
) -> Result<(), AppError> {
    if prepared.actor_id != auth.claims.sub {
        return Err(AppError::Forbidden("Deletion actor changed".into()));
    }
    let (space, id) = (&prepared.space, &prepared.id);
    access::resolve_page_access(
        conn,
        access::principal_from_auth(auth)?,
        space,
        id,
        prepared.operation,
        Purpose::Routine,
    )
    .await?;
    if delete_domain(conn, space, id)
        .await
        .map_err(capture_error)?
        != prepared.domain
    {
        return Err(changed(
            "Document deletion source changed; retain queued deletion",
        ));
    }
    // RESTRICT is deliberate: remove only the exact captured page grants in
    // this caller-owned transaction, then compare the complete expected domain.
    let grants_removed=sqlx::query("DELETE FROM document_viewer_grants WHERE document_id=$1 AND space_id=$2")
        .bind(id).bind(space).execute(&mut *conn).await.map_err(AppError::internal)?.rows_affected();
    if grants_removed != prepared.domain["document_viewer_grants"].len() as u64 { return Err(changed("Document viewer grants changed; retain deletion")); }
    let removed = sqlx::query("DELETE FROM documents WHERE id=$1 AND space_id=$2")
        .bind(id)
        .bind(space)
        .execute(&mut *conn)
        .await
        .map_err(AppError::internal)?;
    if removed.rows_affected() != 1 {
        return Err(unavailable());
    }
    let mut expected = prepared.domain.clone();
    expected
        .get_mut("documents")
        .unwrap()
        .retain(|v| v["id"] != *id);
    expected
        .get_mut("hierarchy")
        .unwrap()
        .retain(|v| v["id"] != *id);
    expected.get_mut("viewer_grants_all").unwrap().retain(|v| v["document_id"] != *id);
    for table in DELETE_CASCADE_TABLES {
        expected.insert((*table).into(), Vec::new());
    }
    for table in [
        "document_draft_access",
        "comment_cascade_edges",
        "workflow_cascade_edges",
    ] {
        expected.insert(table.into(), Vec::new());
    }
    if delete_domain(conn, space, id)
        .await
        .map_err(capture_error)?
        != expected
    {
        return Err(changed(
            "Document deletion cascade changed; retain queued deletion",
        ));
    }
    Ok(())
}
/// Consumed source authority survives only if actor/ancestors remain exact.
/// No nonexistent target lookup, fake principal or grant renewal.
pub(crate) async fn verify_authority(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    prepared: &PreparedDeletion,
) -> Result<(), AppError> {
    if prepared.actor_id != auth.claims.sub {
        return Err(AppError::Forbidden("Deletion actor changed".into()));
    }
    let current = access::resolve_space_access(
        conn,
        access::principal_from_auth(auth)?,
        &prepared.space,
        Purpose::Routine,
    )
    .await?;
    if !current.allows(Permission::DeleteContent) {
        return Err(unavailable());
    }
    for proof in &prepared.proofs {
        let args: Vec<&str> = proof.parameters.iter().map(String::as_str).collect();
        if rows(conn, &proof.query, &args)
            .await
            .map_err(capture_error)?
            != proof.rows
        {
            return Err(changed(
                "Deletion actor or ancestor policy changed; retain queued deletion",
            ));
        }
    }
    Ok(())
}

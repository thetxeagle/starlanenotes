use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::auth::middleware::AuthUser;
use crate::auth::routes::AppError;
use crate::storage;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/spaces/{space_id}/graph", get(get_space_graph))
}

#[derive(Serialize)]
pub struct GraphNode {
    pub id: String,
    pub title: String,
    pub folder_id: Option<String>,
    pub connection_count: usize,
    pub orphan: bool,
}

#[derive(Serialize)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub relation: String,
}

#[derive(Serialize)]
pub struct GraphResponse {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Clone)]
struct GraphDoc {
    id: String,
    title: String,
    folder_id: Option<String>,
    content_path: String,
    published_at: String,
    updated_at: String,
}

/// GET /api/v1/spaces/{space_id}/graph
async fn get_space_graph(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    Ok(Json(read_space_graph(&state, &auth, &space_id).await?))
}
async fn graph_page_view(
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
async fn graph_space_view(
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
        return Err(AppError::NotFound("Target unavailable".into()));
    }
    Ok(())
}
async fn graph_doc_current(
    conn: &mut sqlx::AnyConnection,
    auth: &AuthUser,
    space: &str,
    doc: &GraphDoc,
) -> Result<bool, AppError> {
    match graph_page_view(conn, auth, space, &doc.id).await {
        Ok(()) => {}
        Err(AppError::NotFound(_)) => return Ok(false),
        Err(error) => return Err(error),
    }
    match super::documents::require_current_document_read_source(
        conn,
        space,
        &doc.id,
        &doc.published_at,
        &doc.content_path,
        &doc.updated_at,
    )
    .await
    {
        Ok(()) => Ok(true),
        Err(AppError::Conflict(_)) => Ok(false),
        Err(error) => Err(error),
    }
}
pub(crate) async fn read_space_graph(
    state: &AppState,
    auth: &AuthUser,
    space_id: &str,
) -> Result<GraphResponse, AppError> {
    let mut conn = state.db.acquire().await.map_err(AppError::internal)?;
    graph_space_view(&mut conn, auth, space_id).await?;
    let rows:Vec<(String,String,Option<String>,String,String,String)>=sqlx::query_as("SELECT id,title,folder_id,content_path,published_at,updated_at FROM documents WHERE space_id=$1 AND published_at IS NOT NULL AND archived_at IS NULL AND deleted_at IS NULL ORDER BY title").bind(space_id).fetch_all(&mut *conn).await.map_err(AppError::internal)?;
    let mut loaded = Vec::new();
    for (id, title, folder_id, content_path, published_at, updated_at) in rows {
        let doc = GraphDoc {
            id,
            title,
            folder_id,
            content_path,
            published_at,
            updated_at,
        };
        if !graph_doc_current(&mut conn, auth, space_id, &doc).await? {
            continue;
        }
        let content = storage::read_document(&doc.content_path)
            .await
            .unwrap_or_default();
        if graph_doc_current(&mut conn, auth, space_id, &doc).await? {
            loaded.push((doc, content));
        }
    }
    // Resolve every result again after all file I/O. Build the induced graph only
    // from this final visible set, so hidden endpoints never affect degrees.
    let mut docs = Vec::new();
    let mut contents = HashMap::new();
    for (doc, content) in loaded {
        if graph_doc_current(&mut conn, auth, space_id, &doc).await? {
            contents.insert(doc.id.clone(), content);
            docs.push(doc);
        }
    }
    graph_space_view(&mut conn, auth, space_id).await?;
    // Revalidate the final set before projecting titles, edges, and counts.
    // Hidden titles must not participate in ambiguous-title resolution.
    let mut current_docs = Vec::new();
    for doc in docs {
        if graph_doc_current(&mut conn, auth, space_id, &doc).await? {
            current_docs.push(doc);
        }
    }
    let docs = current_docs;
    let edge_types = visible_graph_edges(&docs, &contents);
    graph_space_view(&mut conn, auth, space_id).await?;

    let mut connection_count: HashMap<String, usize> = HashMap::new();
    let mut edges: Vec<GraphEdge> = Vec::new();

    for ((source, target), relations) in edge_types {
        *connection_count.entry(source.clone()).or_insert(0) += 1;
        *connection_count.entry(target.clone()).or_insert(0) += 1;

        let mut relation_values: Vec<String> = relations.into_iter().collect();
        relation_values.sort_unstable();
        edges.push(GraphEdge {
            source,
            target,
            relation: relation_values.join("+"),
        });
    }

    let nodes: Vec<GraphNode> = docs
        .into_iter()
        .map(|doc| {
            let count = connection_count.get(&doc.id).copied().unwrap_or(0);
            GraphNode {
                id: doc.id,
                title: doc.title,
                folder_id: doc.folder_id,
                connection_count: count,
                orphan: count == 0,
            }
        })
        .collect();

    graph_space_view(&mut conn, auth, space_id).await?;
    Ok(GraphResponse { nodes, edges })
}

fn visible_graph_edges(
    docs: &[GraphDoc],
    contents: &HashMap<String, String>,
) -> HashMap<(String, String), HashSet<String>> {
    let mut exact_title_to_id: HashMap<String, String> = HashMap::new();
    let mut normalized_title_to_ids: HashMap<String, Vec<String>> = HashMap::new();
    let mut doc_ids: HashSet<String> = HashSet::new();

    for doc in docs {
        exact_title_to_id.insert(doc.title.clone(), doc.id.clone());
        normalized_title_to_ids
            .entry(normalize_title(&doc.title))
            .or_default()
            .push(doc.id.clone());
        doc_ids.insert(doc.id.clone());
    }

    let mut edge_types: HashMap<(String, String), HashSet<String>> = HashMap::new();

    for doc in docs {
        let content = contents
            .get(&doc.id)
            .map(String::as_str)
            .unwrap_or_default();

        for wiki_title in extract_wiki_links(&content) {
            if let Some(target_id) =
                resolve_title_reference(&wiki_title, &exact_title_to_id, &normalized_title_to_ids)
            {
                if target_id != doc.id {
                    let key = edge_key(&doc.id, &target_id);
                    edge_types
                        .entry(key)
                        .or_default()
                        .insert("wiki".to_string());
                }
            }
        }

        for target_id in extract_doc_id_links(&content) {
            if target_id != doc.id && doc_ids.contains(&target_id) {
                let key = edge_key(&doc.id, &target_id);
                edge_types
                    .entry(key)
                    .or_default()
                    .insert("mention".to_string());
            }
        }
    }

    edge_types
}

fn resolve_title_reference(
    title: &str,
    exact_title_to_id: &HashMap<String, String>,
    normalized_title_to_ids: &HashMap<String, Vec<String>>,
) -> Option<String> {
    if let Some(doc_id) = exact_title_to_id.get(title) {
        return Some(doc_id.clone());
    }

    let normalized = normalize_title(title);
    let matches = normalized_title_to_ids.get(&normalized)?;
    if matches.len() == 1 {
        return Some(matches[0].clone());
    }

    None
}

fn edge_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

fn normalize_title(title: &str) -> String {
    title.trim().to_lowercase()
}

fn extract_wiki_links(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut offset = 0;

    while let Some(start_rel) = content[offset..].find("[[") {
        let start = offset + start_rel + 2;
        let Some(end_rel) = content[start..].find("]]") else {
            break;
        };
        let end = start + end_rel;
        let raw = &content[start..end];
        let title = raw.split('|').next().unwrap_or("").trim();
        if !title.is_empty() {
            links.push(title.to_string());
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
mod tests {
    use super::{extract_doc_id_links, extract_wiki_links};

    fn graph_auth(user: &str) -> crate::auth::middleware::AuthUser {
        let mut auth = crate::spaces::properties::tests::test_auth(user);
        auth.claims.sid = format!("graph-session-{user}");
        auth
    }
    async fn graph_state() -> crate::AppState {
        let state = crate::spaces::properties::tests::property_test_state().await;
        for user in ["owner-a", "editor-a", "viewer-a"] {
            sqlx::query("INSERT INTO sessions(id,user_id,refresh_token_hash,expires_at) VALUES($1,$2,'test','2099-01-01T00:00:00Z')").bind(format!("graph-session-{user}")).bind(user).execute(&state.db).await.unwrap();
        }
        state
    }
    #[tokio::test]
    async fn graph_visible_induced_edges_counts_and_orphans_exclude_hidden_pages_and_ancestors() {
        let state = graph_state().await;
        let slug = format!("graph-{}", uuid::Uuid::new_v4());
        for (id, parent, published, content) in [
            (
                "visible-peer",
                None,
                Some("published"),
                "/docs/target-a /docs/hidden-child [[ visible-orphan ]]",
            ),
            ("visible-orphan", None, Some("published"), "No links"),
            ("isolated-node", None, Some("published"), "No links"),
            ("private-parent", None, None, "/docs/target-a"),
            (
                "hidden-child",
                Some("private-parent"),
                Some("published"),
                "/docs/target-a",
            ),
            ("archived-node", None, Some("published"), "/docs/target-a"),
            ("trashed-node", None, Some("published"), "/docs/target-a"),
        ] {
            let path = crate::storage::write_document(&slug, id, id, "", content)
                .await
                .unwrap();
            sqlx::query("INSERT INTO documents(id,space_id,title,parent_id,content_path,created_by,last_updated_by,published_at) VALUES($1,'space-a',$1,$2,$3,'owner-a','owner-a',$4)").bind(id).bind(parent).bind(path).bind(published).execute(&state.db).await.unwrap();
        }
        sqlx::query("UPDATE documents SET archived_at='2099' WHERE id='archived-node'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET deleted_at='2099' WHERE id='trashed-node'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET locked=1,locked_by='owner-a' WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("UPDATE documents SET title=' VISIBLE-ORPHAN ' WHERE id='hidden-child'")
            .execute(&state.db)
            .await
            .unwrap();
        let result = super::read_space_graph(&state, &graph_auth("viewer-a"), "space-a")
            .await
            .unwrap();
        assert_eq!(result.nodes.len(), 4);
        assert_eq!(result.edges.len(), 2);
        assert!(result.edges.iter().any(|edge| edge.relation == "wiki"));
        for node in result.nodes {
            if node.id == "isolated-node" {
                assert!(node.orphan);
                assert_eq!(node.connection_count, 0);
            } else {
                assert!(!node.orphan);
                assert_eq!(
                    node.connection_count,
                    if node.id == "visible-peer" { 2 } else { 1 }
                );
            }
            assert!(!node.id.contains("hidden"));
            assert!(!node.id.contains("private"));
        }
        tokio::fs::remove_dir_all(crate::storage::space_dir(&slug))
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn graph_global_group_view_current_account_and_key_proof_are_required() {
        let state = graph_state().await;
        sqlx::query("DELETE FROM space_members WHERE user_id='viewer-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO groups(id,name) VALUES('graph-group','Graph')")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO group_members(id,group_id,user_id) VALUES('graph-member','graph-group','viewer-a')").execute(&state.db).await.unwrap();
        sqlx::query("INSERT INTO global_roles(id,name,permissions) VALUES('graph-view','Opaque','[\"view\"]')").execute(&state.db).await.unwrap();
        sqlx::query(
            "INSERT INTO group_global_roles(group_id,role_id) VALUES('graph-group','graph-view')",
        )
        .execute(&state.db)
        .await
        .unwrap();
        assert_eq!(
            super::read_space_graph(&state, &graph_auth("viewer-a"), "space-a")
                .await
                .unwrap()
                .nodes
                .len(),
            1
        );
        sqlx::query("INSERT INTO space_user_blocks(space_id,user_id,created_by) VALUES('space-a','viewer-a','owner-a')").execute(&state.db).await.unwrap();
        assert!(
            super::read_space_graph(&state, &graph_auth("viewer-a"), "space-a")
                .await
                .is_err()
        );
        sqlx::query("INSERT INTO api_keys(id,user_id,name,key_hash,key_prefix,scopes) VALUES('graph-key','editor-a','Read','hash','key','[\"read\"]')").execute(&state.db).await.unwrap();
        let key_auth = || {
            let mut auth = graph_auth("editor-a");
            auth.claims.sid = "api-key:graph-key".into();
            auth.api_key_scopes = Some(vec!["read".into()]);
            auth
        };
        assert!(super::read_space_graph(&state, &key_auth(), "space-a")
            .await
            .is_ok());
        sqlx::query("UPDATE api_keys SET scopes='[\"write\"]' WHERE id='graph-key'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(super::read_space_graph(&state, &key_auth(), "space-a")
            .await
            .is_err());
        sqlx::query("UPDATE users SET is_active=0 WHERE id='editor-a'")
            .execute(&state.db)
            .await
            .unwrap();
        assert!(
            super::read_space_graph(&state, &graph_auth("editor-a"), "space-a")
                .await
                .is_err()
        );
        let mut service = graph_auth("owner-a");
        service.claims.sub = "service:graph-key".into();
        service.claims.sid = "service:graph-key".into();
        service.api_key_scopes = Some(vec!["admin".into()]);
        assert!(super::read_space_graph(&state, &service, "space-a")
            .await
            .is_err());
    }
    #[tokio::test]
    async fn graph_final_source_check_rejects_unpublication_even_with_another_owned_draft() {
        let state = graph_state().await;
        let source:(String,String,String,String,Option<String>,String)=sqlx::query_as("SELECT id,title,content_path,published_at,folder_id,updated_at FROM documents WHERE id='target-a'").fetch_one(&state.db).await.unwrap();
        let doc = super::GraphDoc {
            id: source.0,
            title: source.1,
            content_path: source.2,
            published_at: source.3,
            folder_id: source.4,
            updated_at: source.5,
        };
        sqlx::query("UPDATE documents SET published_at=NULL WHERE id='target-a'")
            .execute(&state.db)
            .await
            .unwrap();
        sqlx::query("INSERT INTO document_drafts(id,document_id,space_id,owner_id,title,base_updated_at) VALUES('graph-own','target-a','space-a','editor-a','Own','base')").execute(&state.db).await.unwrap();
        let mut conn = state.db.acquire().await.unwrap();
        assert!(
            !super::graph_doc_current(&mut conn, &graph_auth("editor-a"), "space-a", &doc)
                .await
                .unwrap()
        );
    }

    #[test]
    fn parses_wiki_links_and_aliases() {
        let input = "See [[Roadmap]], [[Design Notes|design]], and [[  Spec ]]";
        let links = extract_wiki_links(input);
        assert_eq!(links, vec!["Roadmap", "Design Notes", "Spec"]);
    }

    #[test]
    fn parses_doc_ids_from_links() {
        let input = "Open [/x](/spaces/a/docs/1111-2222) and /docs/abcd1234 plus /docs/not-an-id?";
        let ids = extract_doc_id_links(input);
        assert_eq!(ids, vec!["1111-2222", "abcd1234", "not-an-id"]);
    }
}

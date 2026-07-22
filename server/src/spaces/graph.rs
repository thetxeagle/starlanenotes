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
}

/// GET /api/v1/spaces/{space_id}/graph
async fn get_space_graph(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(space_id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    super::routes::require_space_role(&state, &space_id, &auth.claims.sub, &["owner", "editor"])
        .await?;

    let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, title, folder_id, content_path \
         FROM documents \
         WHERE space_id = ?1 \
         ORDER BY title",
    )
    .bind(&space_id)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::internal)?;

    let docs: Vec<GraphDoc> = rows
        .into_iter()
        .map(|(id, title, folder_id, content_path)| GraphDoc {
            id,
            title,
            folder_id,
            content_path,
        })
        .collect();

    let mut exact_title_to_id: HashMap<String, String> = HashMap::new();
    let mut normalized_title_to_ids: HashMap<String, Vec<String>> = HashMap::new();
    let mut doc_ids: HashSet<String> = HashSet::new();

    for doc in &docs {
        exact_title_to_id.insert(doc.title.clone(), doc.id.clone());
        normalized_title_to_ids
            .entry(normalize_title(&doc.title))
            .or_default()
            .push(doc.id.clone());
        doc_ids.insert(doc.id.clone());
    }

    let mut edge_types: HashMap<(String, String), HashSet<String>> = HashMap::new();

    for doc in &docs {
        let content = storage::read_document(&doc.content_path)
            .await
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

    Ok(Json(GraphResponse { nodes, edges }))
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
